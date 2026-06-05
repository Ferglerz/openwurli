//! OpenWurli DSP library — Wurlitzer 200A signal chain modules.
//!
//! Pure DSP math with no audio framework dependencies.

pub mod circuit;
pub mod dsp;
pub mod engine;
pub mod physics;

// Re-exports for ergonomic single-import use:
//
//     use openwurli_dsp::{WurliEngine, VoiceState};
//
// Without these, callers would need `openwurli_dsp::engine::WurliEngine`.
pub use engine::engine::{VoiceState, WurliEngine};

