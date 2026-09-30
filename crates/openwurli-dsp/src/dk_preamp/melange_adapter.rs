//! Melange-generated DK preamp adapter with shadow pump cancellation.
//!
//! LDR resistance is declared as `.runtime R` in the netlist (plugin-
//! driven, not a user knob). `set_runtime_R_r_ldr` marks matrices dirty;
//! the next `process_sample` does a lazy rebuild before the NR solve.
//! max_iter=200 for convergence across the full R_ldr range (1K-1M).

use crate::gen_preamp::{self, CircuitState};
use crate::preamp::PreampModel;
use std::sync::OnceLock;

static SETTLED_STATE: OnceLock<CircuitState> = OnceLock::new();

fn compute_settled_state() -> CircuitState {
    let mut s = CircuitState::default();
    for _ in 0..176_400 {
        gen_preamp::process_sample(0.0, &mut s);
    }
    s
}

fn init_state(sample_rate: f64) -> CircuitState {
    let cached = SETTLED_STATE.get_or_init(compute_settled_state);
    let mut state = cached.clone();
    if (sample_rate - gen_preamp::SAMPLE_RATE).abs() > 0.5 {
        state.set_sample_rate(sample_rate);
    }
    state
}

/// Bitwise comparison preserves NaN payloads and signed zeros as well as
/// ordinary coefficients. Used only when a matrix rebuild is already pending.
fn same_matrix_bits<const ROWS: usize, const COLS: usize>(
    a: &[[f64; COLS]; ROWS],
    b: &[[f64; COLS]; ROWS],
) -> bool {
    a.iter()
        .flatten()
        .zip(b.iter().flatten())
        .all(|(a, b)| a.to_bits() == b.to_bits())
}

pub struct DkPreamp {
    main: CircuitState,
    shadow: CircuitState,
    sample_rate: f64,
    noise_enabled: bool,
    thermal_gain: f64,
}

impl DkPreamp {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            main: init_state(sample_rate),
            shadow: init_state(sample_rate),
            sample_rate,
            noise_enabled: false,
            thermal_gain: 1.0,
        }
    }

    /// Enable/disable authentic Johnson-Nyquist thermal noise on the preamp
    /// resistors. Only the `main` state draws noise — the `shadow` state
    /// stays noiseless so pump subtraction cancels R_ldr DC drift without
    /// also cancelling the noise we just added.
    pub fn set_noise_enabled(&mut self, on: bool) {
        self.noise_enabled = on;
        self.main.set_noise_enabled(on);
    }

    /// Scale thermal noise amplitude. `1.0` = physics-honest full noise:
    /// every preamp resistor contributes its `sqrt(4·k_B·T·R·BW)` density,
    /// matching what ngspice `.NOISE` reports for the same netlist (~8 µV
    /// RMS at the preamp output). That lands near −86 dBFS at DAW default
    /// gain staging — the same place a clean DI of a real 200A sits.
    ///
    /// The plugin's `noise_gain` param wraps this via `set_noise_gain` and
    /// defaults to `1.0×` (asserted in `openwurli-plugin/src/lib.rs`). See
    /// `openwurli-plugin/src/params.rs` for the authoritative dBFS figures
    /// (that doc is the single source of truth). Raise above `1.0` to
    /// exaggerate the noise for "vintage hiss"; `30×` ≈ −56 dBFS.
    pub fn set_thermal_gain(&mut self, gain: f64) {
        self.thermal_gain = gain;
        self.main.set_thermal_gain(gain);
    }
}

impl DkPreamp {
    /// Rebuild deterministic coefficients once when the two solver instances
    /// have identical matrix inputs and identical preserved inverse state.
    /// Failed inversion leaves old S/K/S_NI untouched, so checking the latter
    /// makes that fallback identical too. Never copy dynamic circuit state.
    fn share_pending_matrices(&mut self) -> bool {
        let a = &self.main;
        let b = &self.shadow;
        let eligible = a.matrices_dirty
            && b.matrices_dirty
            && a.current_sample_rate.to_bits() == b.current_sample_rate.to_bits()
            && same_matrix_bits(&a.g_work, &b.g_work)
            && same_matrix_bits(&a.c_work, &b.c_work)
            && same_matrix_bits(&a.s, &b.s)
            && same_matrix_bits(&a.k, &b.k)
            && same_matrix_bits(&a.s_ni, &b.s_ni)
            && same_matrix_bits(&a.s_be, &b.s_be)
            && same_matrix_bits(&a.k_be, &b.k_be)
            && same_matrix_bits(&a.s_ni_be, &b.s_ni_be);
        if !eligible {
            return false;
        }
        self.main.rebuild_matrices(
            self.main.current_sample_rate * gen_preamp::OVERSAMPLING_FACTOR as f64,
        );
        self.main.matrices_dirty = false;
        // This is precisely rebuild_matrices' write set. In particular the
        // unused substep inverses, chord LU, history, voltages and RNG remain
        // private to each solver. Generated source is unchanged.
        self.shadow.a = self.main.a;
        self.shadow.a_neg = self.main.a_neg;
        self.shadow.a_be = self.main.a_be;
        self.shadow.a_neg_be = self.main.a_neg_be;
        self.shadow.a_neg_sub = self.main.a_neg_sub;
        self.shadow.s = self.main.s;
        self.shadow.k = self.main.k;
        self.shadow.s_ni = self.main.s_ni;
        self.shadow.s_be = self.main.s_be;
        self.shadow.k_be = self.main.k_be;
        self.shadow.s_ni_be = self.main.s_ni_be;
        self.shadow.chord_valid = false;
        self.shadow.matrices_dirty = false;
        true
    }

    fn process_with_matrix_reuse<const REUSE: bool>(&mut self, input: f64) -> f64 {
        if REUSE {
            self.share_pending_matrices();
        }
        let main_out = gen_preamp::process_sample(input, &mut self.main)[0];
        let pump = gen_preamp::process_sample(0.0, &mut self.shadow)[0];
        let result = main_out - pump;
        if !result.is_finite() {
            self.reset();
            return 0.0;
        }
        result
    }
}

impl PreampModel for DkPreamp {
    fn process_sample(&mut self, input: f64) -> f64 {
        self.process_with_matrix_reuse::<{ cfg!(feature = "cpu-study-heavy-matrices") }>(input)
    }

    fn set_ldr_resistance(&mut self, r_ldr_path: f64) {
        self.main.set_runtime_R_r_ldr(r_ldr_path);
        self.shadow.set_runtime_R_r_ldr(r_ldr_path);
    }

    fn reset(&mut self) {
        self.main = init_state(self.sample_rate);
        self.shadow = init_state(self.sample_rate);
        self.main.set_noise_enabled(self.noise_enabled);
        self.main.set_thermal_gain(self.thermal_gain);
    }
}

#[cfg(test)]
mod matrix_reuse_tests {
    use super::*;

    #[test]
    fn shared_rebuild_preserves_heavy_output_state_noise_and_reset() {
        for sample_rate in [44_100.0, 88_200.0, 96_000.0] {
            for noise in [false, true] {
                let mut native = DkPreamp::new(sample_rate);
                let mut candidate = DkPreamp::new(sample_rate);
                native.set_noise_enabled(noise);
                candidate.set_noise_enabled(noise);
                for i in 0..1024 {
                    if i == 512 {
                        native.reset();
                        candidate.reset();
                    }
                    let resistance = if i < 128 {
                        13_235.294_117_647_06
                    } else {
                        19_000.0 + 500_000.0 * (0.5 + 0.5 * (i as f64 * 0.03125).sin())
                    };
                    native.set_ldr_resistance(resistance);
                    candidate.set_ldr_resistance(resistance);
                    let input = 0.05 * (i as f64 * 0.0625).sin();
                    let a = native.process_with_matrix_reuse::<false>(input);
                    let b = candidate.process_with_matrix_reuse::<true>(input);
                    assert_eq!(
                        a.to_bits(),
                        b.to_bits(),
                        "sample {i}, rate {sample_rate}, noise {noise}"
                    );
                    for (a, b) in [
                        (&native.main, &candidate.main),
                        (&native.shadow, &candidate.shadow),
                    ] {
                        for (x, y) in a.v_prev.iter().zip(b.v_prev.iter()) {
                            assert_eq!(x.to_bits(), y.to_bits());
                        }
                        for (x, y) in a
                            .i_nl_prev
                            .iter()
                            .chain(a.i_nl_prev_prev.iter())
                            .zip(b.i_nl_prev.iter().chain(b.i_nl_prev_prev.iter()))
                        {
                            assert_eq!(x.to_bits(), y.to_bits());
                        }
                        assert_eq!(a.last_nr_iterations, b.last_nr_iterations);
                        assert_eq!(a.diag_nan_reset_count, b.diag_nan_reset_count);
                        assert_eq!(a.diag_magnitude_reset_count, b.diag_magnitude_reset_count);
                    }
                }
            }
        }
    }

    #[test]
    fn shared_rebuild_preserves_failed_inverse_and_rejects_mismatched_state() {
        let mut preamp = DkPreamp::new(88_200.0);
        // Force a singular rebuild: the generated function deliberately retains
        // its previous inverse matrices. Sharing must retain precisely those.
        preamp.main.g_work = [[0.0; gen_preamp::N]; gen_preamp::N];
        preamp.main.c_work = [[0.0; gen_preamp::N]; gen_preamp::N];
        preamp.shadow.g_work = preamp.main.g_work;
        preamp.shadow.c_work = preamp.main.c_work;
        preamp.main.matrices_dirty = true;
        preamp.shadow.matrices_dirty = true;
        let mut expected = preamp.shadow.clone();
        expected.rebuild_matrices(expected.current_sample_rate);
        assert!(preamp.share_pending_matrices());
        assert!(same_matrix_bits(&expected.s, &preamp.shadow.s));
        assert!(same_matrix_bits(&expected.k, &preamp.shadow.k));
        assert!(same_matrix_bits(&expected.s_ni, &preamp.shadow.s_ni));
        assert!(same_matrix_bits(&expected.s_be, &preamp.shadow.s_be));
        assert!(same_matrix_bits(&expected.k_be, &preamp.shadow.k_be));
        assert!(same_matrix_bits(&expected.s_ni_be, &preamp.shadow.s_ni_be));
        assert!(same_matrix_bits(
            &expected.a_neg_sub,
            &preamp.shadow.a_neg_sub
        ));
        assert!(!preamp.shadow.matrices_dirty);
        assert!(!preamp.shadow.chord_valid);

        preamp.main.matrices_dirty = true;
        preamp.shadow.matrices_dirty = true;
        preamp.shadow.s[0][0] += 1.0;
        assert!(
            !preamp.share_pending_matrices(),
            "different retained inverse needs native rebuild"
        );
        preamp.shadow.s = preamp.main.s;
        preamp.shadow.g_work[0][0] = 1.0;
        assert!(
            !preamp.share_pending_matrices(),
            "different circuit configuration needs native rebuild"
        );
        assert!(preamp.shadow.matrices_dirty);
        assert!(preamp.main.matrices_dirty);
    }
}
