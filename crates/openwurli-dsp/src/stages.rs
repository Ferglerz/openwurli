//! Shared-stage traits for the post-voice-sum analog chain.
//!
//! Stages implement these traits so `AnalogChain` can sequence them without
//! hard-coding concrete types. Tremolo is a **modulator** (LDR shunt resistance),
//! not an audio stage — it feeds `PreampModel::set_ldr_resistance` inside the
//! preamp feedback loop.

/// Mono sample processor at a single rate (base or oversampled).
pub trait AudioStage {
    fn process_sample(&mut self, input: f64) -> f64;
    fn reset(&mut self);
    /// Rebuild rate-dependent state. Default: no-op (rate-independent stages).
    fn set_sample_rate(&mut self, _sample_rate: f64) {}
}

/// Produces the total LDR shunt-path resistance (Ω) for the preamp each sample.
pub trait LdrModulator {
    fn process(&mut self) -> f64;
    fn set_depth(&mut self, depth: f64);
    fn reset(&mut self);
    fn set_sample_rate(&mut self, sample_rate: f64);
}
