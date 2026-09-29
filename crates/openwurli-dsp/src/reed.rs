//! Modal reed oscillator — 7 damped sinusoidal modes.
//!
//! Each mode uses a quadrature oscillator (sin/cos pair rotated per sample)
//! instead of computing sin(phase) per sample. This eliminates 7 transcendental
//! calls per sample per voice — the dominant CPU cost in v0.1.x.
//!
//! Per-mode frequency jitter (Ornstein-Uhlenbeck process) breaks the perfect
//! phase coherence of digital oscillators. Real reeds have nonlinear frequency-
//! amplitude coupling (backbone curve), pickup loading, and micro-turbulence
//! that cause each mode's frequency to wander slightly. Without this, the
//! static spectral interference pattern sounds "metallic" and "resonant."
//!
//! Jitter is subsampled (every 16 samples) because the OU correlation time
//! (τ=20ms, ~882 samples at 44.1kHz) is far longer than the update interval.

use std::f64::consts::{PI, TAU};

use wide::f64x2;

use crate::tables::NUM_MODES;

/// Modes processed as `f64x2` pairs. Mode 6 stays scalar so the LCG
/// jitter stream keeps its original 0..6 order.
const SIMD_PAIRS: usize = NUM_MODES / 2;

/// RMS frequency jitter as fraction of mode frequency (~0.04% = 4 cents peak).
const JITTER_SIGMA: f64 = 0.0004;

/// OU correlation time in seconds (~20ms). Controls how fast modes drift
/// relative to each other — long enough for perceptible beating, short enough
/// to evolve within a note's sustain.
const JITTER_TAU: f64 = 0.020;

/// sqrt(3): scaling for uniform[-1,1] to achieve unit variance.
/// Uniform(-√3, √3) has variance 1.0, matching the Gaussian's variance.
const SQRT_3: f64 = 1.732_050_808_0;

/// Jitter subsample interval. OU τ=20ms spans ~882 samples at 44.1kHz.
/// Updating every 16 samples (0.36ms) is 55x below correlation time.
const JITTER_SUBSAMPLE: u64 = 16;

/// Quadrature renormalization interval. Every 1024 samples, correct the
/// radius of each (s,c) pair to prevent drift from accumulated FP error.
/// Cost: 7 sqrt + 7 div per 1024 samples = 0.014 transcendentals/sample.
const RENORM_INTERVAL: u64 = 1024;

/// Per-mode oscillator state as struct-of-arrays so two modes load into one
/// `f64x2` register. Same `f64` ops as the old array-of-structs loop.
pub struct ModalReed {
    s: [f64; NUM_MODES],
    c: [f64; NUM_MODES],
    cos_inc: [f64; NUM_MODES],
    sin_inc: [f64; NUM_MODES],
    phase_inc: [f64; NUM_MODES],
    // Jitter is held for 16 samples. Cache its corrected rotation until the
    // next OU update; evaluating the same f64 expression again adds no detail.
    rotation_cos: [f64; NUM_MODES],
    rotation_sin: [f64; NUM_MODES],
    amplitude: [f64; NUM_MODES],
    decay_mult: [f64; NUM_MODES],
    envelope: [f64; NUM_MODES],
    jitter_drift: [f64; NUM_MODES],
    damper_rate: [f64; NUM_MODES],
    damper_mult: [f64; NUM_MODES],
    sample: u64,
    // Onset ramp
    onset_ramp_samples: u64,
    onset_ramp_inc: f64,
    onset_shape_exp: f64,
    // Damper state
    damper_active: bool,
    damper_ramp_samples: f64,
    damper_release_count: f64,
    damper_ramp_done: bool,
    // Jitter PRNG and coefficients
    jitter_state: u32,
    jitter_revert: f64,
    jitter_diffusion: f64,
}

#[inline]
fn load2(arr: &[f64; NUM_MODES], i: usize) -> f64x2 {
    f64x2::new([arr[i], arr[i + 1]])
}

#[inline]
fn store2(arr: &mut [f64; NUM_MODES], i: usize, v: f64x2) {
    let [a, b] = v.to_array();
    arr[i] = a;
    arr[i + 1] = b;
}

/// Scalar leftover lane (mode 6). Same add-then-rotate-then-decay order
/// as the original per-mode loop.
#[inline]
fn advance_scalar_mode(
    s: &mut f64,
    c: &mut f64,
    ci: f64,
    si: f64,
    amplitude: f64,
    decay_mult: f64,
    envelope: &mut f64,
    onset: f64,
) -> f64 {
    let contrib = amplitude * *s * onset * *envelope;
    let s_new = *s * ci + *c * si;
    let c_new = *c * ci - *s * si;
    *s = s_new;
    *c = c_new;
    *envelope *= decay_mult;
    contrib
}

/// LCG PRNG → scaled uniform noise with unit variance.
/// Free function to avoid borrow conflicts when iterating `&mut modes`.
#[inline]
fn lcg_uniform_scaled(state: &mut u32) -> f64 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    let u = (*state >> 1) as f64 / (u32::MAX as f64 / 2.0);
    (u * 2.0 - 1.0) * SQRT_3
}

impl ModalReed {
    /// Create a new modal reed oscillator.
    ///
    /// - `fundamental_hz`: fundamental frequency after detuning
    /// - `mode_ratios`: f_n / f_1 for each mode
    /// - `amplitudes`: initial amplitude for each mode (post dwell-filter, post variation)
    /// - `decay_rates_db`: decay rate in dB/s for each mode
    /// - `onset_time_s`: onset ramp duration in seconds (reed mechanical inertia)
    /// - `velocity`: 0.0 (pp) to 1.0 (ff), controls onset ramp shape
    /// - `sample_rate`: audio sample rate in Hz
    /// - `jitter_seed`: RNG seed for per-mode frequency jitter (decorrelates voices)
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fundamental_hz: f64,
        mode_ratios: &[f64; NUM_MODES],
        amplitudes: &[f64; NUM_MODES],
        decay_rates_db: &[f64; NUM_MODES],
        onset_time_s: f64,
        velocity: f64,
        sample_rate: f64,
        jitter_seed: u32,
    ) -> Self {
        // Ornstein-Uhlenbeck jitter coefficients
        let dt = 1.0 / sample_rate;
        let jitter_revert = (-dt / JITTER_TAU).exp();
        let jitter_diffusion = JITTER_SIGMA * (1.0 - jitter_revert * jitter_revert).sqrt();

        // Initialize jitter_drift from the OU stationary distribution N(0, JITTER_SIGMA).
        // Uses Box-Muller for the initial draw (one-time cost at note-on).
        let mut jitter_state = jitter_seed.max(1);
        let mut initial_drifts = [0.0f64; NUM_MODES];
        for d in &mut initial_drifts {
            jitter_state = jitter_state.wrapping_mul(1664525).wrapping_add(1013904223);
            let u1 = (jitter_state >> 1) as f64 / (u32::MAX as f64 / 2.0);
            jitter_state = jitter_state.wrapping_mul(1664525).wrapping_add(1013904223);
            let u2 = (jitter_state >> 1) as f64 / (u32::MAX as f64 / 2.0);
            let r = (-2.0 * u1.max(1e-30).ln()).sqrt();
            *d = JITTER_SIGMA * r * (TAU * u2).cos();
        }

        let mut s = [0.0f64; NUM_MODES];
        let mut c = [1.0f64; NUM_MODES];
        let mut cos_inc = [0.0f64; NUM_MODES];
        let mut sin_inc = [0.0f64; NUM_MODES];
        let mut phase_inc = [0.0f64; NUM_MODES];
        let mut decay_mult = [0.0f64; NUM_MODES];
        for i in 0..NUM_MODES {
            let freq = fundamental_hz * mode_ratios[i];
            let inc = TAU * freq / sample_rate;
            let alpha_nepers = decay_rates_db[i] / 8.686;
            s[i] = 0.0;
            c[i] = 1.0;
            cos_inc[i] = inc.cos();
            sin_inc[i] = inc.sin();
            phase_inc[i] = inc;
            decay_mult[i] = (-(alpha_nepers / sample_rate)).exp();
        }

        // Onset ramp
        let ramp_samps = (onset_time_s * sample_rate).round() as u64;
        let ramp_inc = if ramp_samps > 0 {
            PI / ramp_samps as f64
        } else {
            0.0
        };

        let onset_shape_exp = 1.0 + (1.0 - velocity);

        Self {
            s,
            c,
            cos_inc,
            sin_inc,
            phase_inc,
            // Sample zero updates jitter and fills both caches before use.
            rotation_cos: cos_inc,
            rotation_sin: sin_inc,
            amplitude: *amplitudes,
            decay_mult,
            envelope: [1.0; NUM_MODES],
            jitter_drift: initial_drifts,
            damper_rate: [0.0; NUM_MODES],
            damper_mult: [1.0; NUM_MODES],
            sample: 0,
            onset_ramp_samples: ramp_samps,
            onset_ramp_inc: ramp_inc,
            onset_shape_exp,
            damper_active: false,
            damper_ramp_samples: 0.0,
            damper_release_count: 0.0,
            damper_ramp_done: false,
            jitter_state,
            jitter_revert,
            jitter_diffusion,
        }
    }

    /// Start the damper (called on note_off).
    ///
    /// Three-phase progressive model: felt progressively contacts the reed,
    /// with higher modes damped more aggressively.
    ///
    /// - `midi_note`: for register-dependent ramp time
    /// - `sample_rate`: for time constant conversion
    pub fn start_damper(&mut self, midi_note: u8, sample_rate: f64) {
        // Top 5 keys: no damper (natural decay only)
        if midi_note >= 92 {
            return;
        }

        let base_rate = (55.0 * 2.0_f64.powf((midi_note as f64 - 60.0) / 24.0)).max(0.5);
        for m in 0..NUM_MODES {
            let factor = (base_rate * 3.0_f64.powi(m as i32)).min(2000.0);
            self.damper_rate[m] = factor / sample_rate;
            self.damper_mult[m] = (-self.damper_rate[m]).exp();
        }

        let ramp_time = if midi_note < 48 {
            0.050
        } else if midi_note < 72 {
            0.025
        } else {
            0.008
        };

        self.damper_ramp_samples = ramp_time * sample_rate;
        self.damper_active = true;
        self.damper_release_count = 0.0;
        self.damper_ramp_done = false;
    }

    /// Render samples into the output buffer (additive, does NOT clear buffer).
    pub fn render(&mut self, output: &mut [f64]) {
        let revert = self.jitter_revert;
        let diffusion = self.jitter_diffusion;

        for sample in output.iter_mut() {
            let mut sum = 0.0f64;

            // Advance damper if active
            if self.damper_active {
                self.damper_release_count += 1.0;
                let t = self.damper_release_count;
                let ramp = self.damper_ramp_samples;
                if !self.damper_ramp_done {
                    if t > ramp {
                        self.damper_ramp_done = true;
                    } else {
                        // During ramp: apply instantaneous rate (still needs exp per mode)
                        for i in 0..NUM_MODES {
                            let inst_rate = self.damper_rate[i] * t / ramp;
                            self.envelope[i] *= (-inst_rate).exp();
                        }
                    }
                }
                if self.damper_ramp_done {
                    for i in 0..NUM_MODES {
                        self.envelope[i] *= self.damper_mult[i];
                    }
                }
            }

            // Onset ramp: all modes ramp together during hammer contact.
            // Shape exponent: ff -> cosine^1 (standard), pp -> cosine^2 (softer)
            let onset = if self.sample < self.onset_ramp_samples {
                let n = self.sample as f64;
                let cosine = 0.5 * (1.0 - (n * self.onset_ramp_inc).cos());
                // Optimized powf: ff (exp~1.0) uses cosine, pp (exp~2.0) uses cosine²
                if self.onset_shape_exp <= 1.001 {
                    cosine
                } else if self.onset_shape_exp >= 1.999 {
                    cosine * cosine
                } else {
                    cosine.powf(self.onset_shape_exp)
                }
            } else {
                1.0
            };

            // Subsample jitter update: every 16 samples
            if self.sample & (JITTER_SUBSAMPLE - 1) == 0 {
                for i in 0..NUM_MODES {
                    let noise = lcg_uniform_scaled(&mut self.jitter_state);
                    self.jitter_drift[i] = revert * self.jitter_drift[i] + diffusion * noise;
                    // Preserve the original Taylor correction and operation order.
                    // Only its evaluation frequency changes: drift and phase_inc
                    // stay constant until the next jitter update.
                    let delta_phase = self.jitter_drift[i] * self.phase_inc[i];
                    self.rotation_cos[i] = self.cos_inc[i] - delta_phase * self.sin_inc[i];
                    self.rotation_sin[i] = self.sin_inc[i] + delta_phase * self.cos_inc[i];
                }
            }

            // Quadrature oscillator: 0 transcendentals per mode per sample.
            // Two modes per `f64x2`; leftover mode 6 stays scalar.
            let onset_v = f64x2::new([onset, onset]);
            for pair in 0..SIMD_PAIRS {
                let i = pair * 2;
                let s = load2(&self.s, i);
                let c = load2(&self.c, i);
                let contrib = load2(&self.amplitude, i) * s * onset_v * load2(&self.envelope, i);
                let [c0, c1] = contrib.to_array();
                sum += c0;
                sum += c1;

                let ci = load2(&self.rotation_cos, i);
                let si = load2(&self.rotation_sin, i);
                let env_next = load2(&self.envelope, i) * load2(&self.decay_mult, i);
                store2(&mut self.s, i, s * ci + c * si);
                store2(&mut self.c, i, c * ci - s * si);
                store2(&mut self.envelope, i, env_next);
            }
            sum += advance_scalar_mode(
                &mut self.s[6],
                &mut self.c[6],
                self.rotation_cos[6],
                self.rotation_sin[6],
                self.amplitude[6],
                self.decay_mult[6],
                &mut self.envelope[6],
                onset,
            );

            // Renormalize quadrature radius every 1024 samples.
            // Keep this scalar: wide::recip can be a hardware estimate.
            if self.sample & (RENORM_INTERVAL - 1) == 0 && self.sample > 0 {
                for i in 0..NUM_MODES {
                    let r_sq = self.s[i] * self.s[i] + self.c[i] * self.c[i];
                    let r_inv = 1.0 / r_sq.sqrt();
                    self.s[i] *= r_inv;
                    self.c[i] *= r_inv;
                }
            }

            *sample += sum;
            self.sample += 1;
        }
    }

    /// Check if the reed has decayed below a threshold (all modes).
    pub fn is_silent(&self, threshold_db: f64) -> bool {
        let threshold_linear = 10.0_f64.powf(threshold_db / 20.0);
        self.amplitude
            .iter()
            .zip(self.envelope.iter())
            .all(|(amp, env)| (amp * env).abs() <= threshold_linear)
    }

    /// Check if damper is active.
    pub fn is_damping(&self) -> bool {
        self.damper_active
    }

    /// Get release time in seconds (for safety timeout).
    pub fn release_seconds(&self, sample_rate: f64) -> f64 {
        if self.damper_active {
            self.damper_release_count / sample_rate
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_mode_sine() {
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [0.0f64; NUM_MODES];

        let mut reed = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, 44100.0, 42);
        let mut buf = vec![0.0f64; 44100];
        reed.render(&mut buf);

        // With 0.04% jitter, frequency should still be within ~3 Hz of 440
        let mut crossings = 0u32;
        for i in 1..buf.len() {
            if buf[i - 1] < 0.0 && buf[i] >= 0.0 {
                crossings += 1;
            }
        }
        assert!(
            (crossings as f64 - 440.0).abs() < 3.0,
            "Average frequency should be ~440 Hz, got {crossings} crossings"
        );
    }

    #[test]
    fn test_decay() {
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let mut decays = [0.0f64; NUM_MODES];
        decays[0] = 60.0;

        let sr = 44100.0;
        let mut reed = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, sr, 42);

        let half_sec = (sr * 0.5) as usize;
        let mut buf = vec![0.0f64; half_sec];
        reed.render(&mut buf);

        let peak = buf[buf.len() - 200..]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        assert!(peak < 0.1, "expected decay, got peak {peak}");
        assert!(peak > 0.01, "decayed too much, got peak {peak}");
    }

    #[test]
    fn test_onset_ramp_shapes_attack() {
        // With a 20ms ramp (bass ring-up), the onset should build up smoothly.
        let sr = 44100.0;
        let ramp = 0.020;
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [0.0f64; NUM_MODES];

        let mut reed = ModalReed::new(440.0, &ratios, &amps, &decays, ramp, 1.0, sr, 42);
        let n = (sr * 0.050) as usize;
        let mut buf = vec![0.0f64; n];
        reed.render(&mut buf);

        assert!(
            buf[0].abs() < 0.01,
            "First sample should be near zero, got {:.6}",
            buf[0]
        );

        let mid_ramp = (ramp * 0.5 * sr) as usize;
        let mid_peak = buf[mid_ramp.saturating_sub(5)..mid_ramp + 5]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        assert!(
            mid_peak < 0.8,
            "Mid-ramp peak should be < 0.8, got {mid_peak:.4}"
        );

        let late_start = (sr * 0.030) as usize;
        let late_peak = buf[late_start..late_start + 200]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        assert!(
            late_peak > 0.85,
            "Post-ramp peak should be ~1.0, got {late_peak:.4}"
        );
    }

    #[test]
    fn test_onset_ramp_ff_vs_pp() {
        let sr = 44100.0;
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [0.0f64; NUM_MODES];

        let mut reed_ff = ModalReed::new(440.0, &ratios, &amps, &decays, 0.001, 1.0, sr, 42);
        let mut reed_pp = ModalReed::new(440.0, &ratios, &amps, &decays, 0.005, 0.0, sr, 42);

        let n = (sr * 0.010) as usize;
        let mut buf_ff = vec![0.0f64; n];
        let mut buf_pp = vec![0.0f64; n];
        reed_ff.render(&mut buf_ff);
        reed_pp.render(&mut buf_pp);

        let t2ms = (sr * 0.002) as usize;
        let ff_energy: f64 = buf_ff[..t2ms].iter().map(|x| x * x).sum();
        let pp_energy: f64 = buf_pp[..t2ms].iter().map(|x| x * x).sum();
        assert!(
            ff_energy > pp_energy * 1.5,
            "ff should be louder than pp at 2ms: ff={ff_energy:.6}, pp={pp_energy:.6}"
        );
    }

    #[test]
    fn test_onset_zero_dwell_is_instant() {
        let sr = 44100.0;
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [0.0f64; NUM_MODES];

        let mut reed = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, sr, 42);
        let mut buf = vec![0.0f64; 100];
        reed.render(&mut buf);

        let early_peak = buf[..10].iter().map(|x| x.abs()).fold(0.0f64, f64::max);
        assert!(
            early_peak > 0.05,
            "Zero dwell should give immediate amplitude, got {early_peak:.6}"
        );
    }

    #[test]
    fn test_jitter_breaks_phase_coherence() {
        let sr = 44100.0;
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        amps[1] = 0.3;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [0.0f64; NUM_MODES];

        let mut reed_a = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, sr, 100);
        let mut reed_b = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, sr, 200);

        let n = (sr * 0.5) as usize;
        let mut buf_a = vec![0.0f64; n];
        let mut buf_b = vec![0.0f64; n];
        reed_a.render(&mut buf_a);
        reed_b.render(&mut buf_b);

        let late_start = (sr * 0.2) as usize;
        let mut diff_sq = 0.0;
        let mut sig_sq = 0.0;
        for i in late_start..n {
            diff_sq += (buf_a[i] - buf_b[i]).powi(2);
            sig_sq += buf_a[i].powi(2);
        }
        let rms_diff = (diff_sq / (n - late_start) as f64).sqrt();
        let rms_sig = (sig_sq / (n - late_start) as f64).sqrt();

        let relative_diff = rms_diff / rms_sig.max(1e-10);
        assert!(
            relative_diff > 0.001,
            "Jitter should cause measurable difference: relative_diff={relative_diff:.6}"
        );
        assert!(
            relative_diff < 0.5,
            "Jitter should be subtle, not overwhelming: relative_diff={relative_diff:.4}"
        );
    }

    #[test]
    fn test_jitter_deterministic_with_same_seed() {
        let sr = 44100.0;
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [0.0f64; NUM_MODES];

        let mut reed_a = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, sr, 42);
        let mut reed_b = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, sr, 42);

        let n = (sr * 0.2) as usize;
        let mut buf_a = vec![0.0f64; n];
        let mut buf_b = vec![0.0f64; n];
        reed_a.render(&mut buf_a);
        reed_b.render(&mut buf_b);

        assert_eq!(buf_a, buf_b, "Same seed should produce identical output");
    }

    fn render_checksum_c4(block_size: usize) -> u64 {
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        amps[1] = 0.3;
        amps[2] = 0.1;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [4.5, 18.0, 40.0, 80.0, 120.0, 180.0, 240.0];
        let mut reed = ModalReed::new(261.63, &ratios, &amps, &decays, 0.002, 0.8, 44_100.0, 42);
        let mut buf = vec![0.0f64; 8192];
        for block in buf.chunks_mut(block_size) {
            reed.render(block);
        }
        reed.start_damper(60, 44_100.0);
        for block in buf.chunks_mut(block_size) {
            reed.render(block);
        }
        buf.iter()
            .map(|x| x.to_bits())
            .fold(0u64, |acc, bits| acc.wrapping_mul(16_777_619) ^ bits)
    }

    #[test]
    fn test_render_c4_is_block_size_independent() {
        // Native libm results differ across architectures. Compare the same
        // calculation locally instead of imposing one machine's golden bits.
        // The external pre/post harness separately checks historical scalar output.
        let checksum = render_checksum_c4(8192);
        for block_size in [1, 15, 257, 1024] {
            assert_eq!(
                render_checksum_c4(block_size), checksum,
                "reed output changed with host block size {block_size}"
            );
        }
    }

    #[test]
    fn test_jitter_preserves_frequency() {
        let sr = 44100.0;
        let mut amps = [0.0f64; NUM_MODES];
        amps[0] = 1.0;
        let ratios = [1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3];
        let decays = [0.0f64; NUM_MODES];

        let mut reed = ModalReed::new(440.0, &ratios, &amps, &decays, 0.0, 1.0, sr, 77);
        let n = sr as usize;
        let mut buf = vec![0.0f64; n];
        reed.render(&mut buf);

        let mut crossings = 0u32;
        for i in 1..buf.len() {
            if buf[i - 1] < 0.0 && buf[i] >= 0.0 {
                crossings += 1;
            }
        }
        assert!(
            (crossings as f64 - 440.0).abs() < 3.0,
            "Average frequency should be ~440 Hz, got {crossings} crossings"
        );
    }
}
