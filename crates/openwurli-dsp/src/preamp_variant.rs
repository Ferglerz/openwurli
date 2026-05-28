//! Runtime-selectable preamp backend with click-free crossfade on swap.
//!
//! Both melange and legacy solvers are always compiled so hosts can A/B or
//! hot-swap without rebuilding. The compile-time `legacy-preamp` feature still
//! controls which type `dk_preamp::DkPreamp` aliases for backward compatibility.

use crate::dk_preamp::melange_adapter;
use crate::dk_preamp_legacy;
use crate::preamp::PreampModel;

/// Active preamp solver selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreampKind {
    Melange,
    Legacy,
}

/// Holds both preamp backends; [`Self::set_kind`] crossfades between them.
pub struct PreampVariant {
    melange: melange_adapter::DkPreamp,
    legacy: dk_preamp_legacy::DkPreamp,
    active: PreampKind,
    /// During swap: fade from `swap_from` → `active`.
    swap_from: Option<PreampKind>,
    swap_fade: u32,
    swap_fade_len: u32,
    sample_rate: f64,
}

impl PreampVariant {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            melange: melange_adapter::DkPreamp::new(sample_rate),
            legacy: dk_preamp_legacy::DkPreamp::new(sample_rate),
            active: PreampKind::Melange,
            swap_from: None,
            swap_fade: 0,
            swap_fade_len: 0,
            sample_rate,
        }
    }

    pub fn kind(&self) -> PreampKind {
        self.active
    }

    /// Select backend. If `kind` differs from the current one, starts a linear
    /// crossfade over `fade_ms` milliseconds (default 5 ms when `None`).
    pub fn set_kind(&mut self, kind: PreampKind, fade_ms: Option<f64>) {
        if kind == self.active && self.swap_from.is_none() {
            return;
        }
        let fade_ms = fade_ms.unwrap_or(5.0);
        let fade_len = ((self.sample_rate * fade_ms * 0.001).round() as u32).max(1);
        self.swap_from = Some(self.active);
        self.active = kind;
        self.swap_fade = fade_len;
        self.swap_fade_len = fade_len;
        self.with_active(|p| p.reset());
        self.with_kind(kind, |p| p.reset());
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.melange = melange_adapter::DkPreamp::new(sample_rate);
        self.legacy = dk_preamp_legacy::DkPreamp::new(sample_rate);
        self.swap_from = None;
        self.swap_fade = 0;
    }

    fn with_kind<F, R>(&mut self, kind: PreampKind, f: F) -> R
    where
        F: FnOnce(&mut dyn PreampModel) -> R,
    {
        match kind {
            PreampKind::Melange => f(&mut self.melange),
            PreampKind::Legacy => f(&mut self.legacy),
        }
    }

    fn with_active<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut dyn PreampModel) -> R,
    {
        let k = self.active;
        self.with_kind(k, f)
    }

    fn process_kind(&mut self, kind: PreampKind, input: f64) -> f64 {
        self.with_kind(kind, |p| p.process_sample(input))
    }

    fn set_ldr_kind(&mut self, kind: PreampKind, r: f64) {
        self.with_kind(kind, |p| p.set_ldr_resistance(r));
    }
}

impl PreampModel for PreampVariant {
    fn process_sample(&mut self, input: f64) -> f64 {
        if let Some(from) = self.swap_from {
            if self.swap_fade > 0 {
                let t = 1.0 - (self.swap_fade as f64 / self.swap_fade_len as f64);
                let out_from = self.process_kind(from, input);
                let out_to = self.process_kind(self.active, input);
                self.swap_fade = self.swap_fade.saturating_sub(1);
                if self.swap_fade == 0 {
                    self.swap_from = None;
                }
                return out_from * (1.0 - t) + out_to * t;
            }
            self.swap_from = None;
        }
        self.process_kind(self.active, input)
    }

    fn set_ldr_resistance(&mut self, r_ldr_path: f64) {
        self.set_ldr_kind(self.active, r_ldr_path);
        if let Some(from) = self.swap_from {
            self.set_ldr_kind(from, r_ldr_path);
        }
    }

    fn reset(&mut self) {
        self.with_kind(PreampKind::Melange, |p| p.reset());
        self.with_kind(PreampKind::Legacy, |p| p.reset());
        self.swap_from = None;
        self.swap_fade = 0;
    }

    fn set_noise_enabled(&mut self, on: bool) {
        self.melange.set_noise_enabled(on);
    }

    fn set_thermal_gain(&mut self, gain: f64) {
        self.melange.set_thermal_gain(gain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preamp_variant_swap_completes() {
        let mut pv = PreampVariant::new(44_100.0);
        pv.set_ldr_resistance(100_000.0);
        pv.set_kind(PreampKind::Legacy, Some(5.0));
        assert_eq!(pv.kind(), PreampKind::Legacy);
        for _ in 0..500 {
            let _ = pv.process_sample(0.01);
        }
        let out = pv.process_sample(0.01);
        assert!(out.is_finite());
    }
}
