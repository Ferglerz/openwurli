//! Post-voice-sum analog chain — tremolo-modulated preamp → power amp.
//!
//! Encapsulates oversampling policy and stage ordering so `WurliEngine` only
//! sums voices and applies speaker + user volume.

use crate::oversampler::Oversampler;
use crate::power_amp::PowerAmp;
use crate::preamp::PreampModel;
use crate::preamp_variant::{PreampKind, PreampVariant};
use crate::tables;
use crate::tremolo::Tremolo;

/// Tremolo → preamp → fixed circuit drive → power amp, with optional 2× OS.
pub struct AnalogChain {
    tremolo: Tremolo,
    preamp: PreampVariant,
    oversampler: Oversampler,
    power_amp: PowerAmp,
    sample_rate: f64,
    oversample: bool,
    up_buf: Vec<f64>,
}

impl AnalogChain {
    pub fn new(sample_rate: f64, tremolo_depth: f64) -> Self {
        let oversample = sample_rate < 88_200.0;
        let os_sr = if oversample {
            sample_rate * 2.0
        } else {
            sample_rate
        };
        Self {
            tremolo: Tremolo::new(tremolo_depth, os_sr),
            preamp: PreampVariant::new(os_sr),
            oversampler: Oversampler::new(),
            power_amp: PowerAmp::new_at_sample_rate(os_sr),
            sample_rate,
            oversample,
            up_buf: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.preamp.reset();
        self.tremolo.reset();
        self.oversampler.reset();
        self.power_amp.reset();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64, tremolo_depth: f64) {
        self.sample_rate = sample_rate;
        self.oversample = sample_rate < 88_200.0;
        let os_sr = if self.oversample {
            sample_rate * 2.0
        } else {
            sample_rate
        };
        self.preamp.set_sample_rate(os_sr);
        self.tremolo = Tremolo::new(tremolo_depth, os_sr);
        self.oversampler = Oversampler::new();
        self.power_amp = PowerAmp::new_at_sample_rate(os_sr);
    }

    pub fn preamp_kind(&self) -> PreampKind {
        self.preamp.kind()
    }

    pub fn set_preamp_kind(&mut self, kind: PreampKind) {
        self.preamp.set_kind(kind, None);
    }

    pub fn set_noise_enabled(&mut self, on: bool) {
        self.preamp.set_noise_enabled(on);
    }

    pub fn set_noise_gain(&mut self, gain: f64) {
        self.preamp.set_thermal_gain(gain);
    }

    pub fn set_rail_sag(&mut self, on: bool) {
        self.power_amp.set_rail_sag(on);
    }

    pub fn rail_sag_enabled(&self) -> bool {
        self.power_amp.rail_sag_enabled()
    }

    pub fn power_amp_diag(&self) -> (u64, u64, f64) {
        self.power_amp.diag_snapshot()
    }

    pub fn ensure_up_buf(&mut self, base_len: usize) {
        let need = base_len * 2;
        if self.up_buf.len() < need {
            self.up_buf.resize(need, 0.0);
        }
    }

    /// Process voice sum → post-power-amp output at base rate.
    ///
    /// `tremolo_depth` is called once per **base-rate** sample (before each OS
    /// pair when oversampling).
    pub fn render_to_power_amp_out(
        &mut self,
        sum: &[f64],
        out: &mut [f64],
        mut tremolo_depth: impl FnMut() -> f64,
    ) {
        let len = sum.len().min(out.len());
        if len == 0 {
            return;
        }
        self.ensure_up_buf(len);

        if self.oversample {
            self.oversampler
                .upsample_2x(&sum[..len], &mut self.up_buf[..len * 2]);

            for i in 0..len {
                let depth = tremolo_depth();
                self.tremolo.set_depth(depth);

                for j in 0..2 {
                    let idx = i * 2 + j;
                    let r_ldr = self.tremolo.process();
                    self.preamp.set_ldr_resistance(r_ldr);
                    let preamp_out = self.preamp.process_sample(self.up_buf[idx]);
                    self.up_buf[idx] = self
                        .power_amp
                        .process(preamp_out * tables::FIXED_CIRCUIT_DRIVE);
                }
            }

            self.oversampler
                .downsample_2x(&self.up_buf[..len * 2], &mut out[..len]);
        } else {
            for i in 0..len {
                let depth = tremolo_depth();
                self.tremolo.set_depth(depth);
                let r_ldr = self.tremolo.process();
                self.preamp.set_ldr_resistance(r_ldr);
                let preamp_out = self.preamp.process_sample(sum[i]);
                out[i] = self
                    .power_amp
                    .process(preamp_out * tables::FIXED_CIRCUIT_DRIVE);
            }
        }
    }
}
