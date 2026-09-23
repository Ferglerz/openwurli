# Pleasant wrapper controls

This branch adds host-facing DSP controls for the Pleasant OpenWurli UI wrapper. The upstream `main` branch is retained unchanged; the wrapper pins an exact commit from this branch.

`WurliEngine` now accepts four bounded multipliers, all neutral at `1.0`:

- Reed decay: scales natural modal decay at note-on; larger values sustain longer.
- Hammer hardness: adjusts high-mode dwell attenuation at note-on; larger values sound brighter.
- Pickup drive: scales reed motion into the nonlinear pickup at note-on; larger values increase bark.
- Tremolo response: scales CdS cell attack and release time constants while retaining the Twin-T oscillator's circuit rate and waveform.

The first three are captured by new notes, so moving them does not retune or reshape a sounding voice. Tremolo response takes effect on the shared circuit at the next host parameter update. Every setter bounds values to `0.5..=2.0` and treats non-finite input as `1.0`.

The wrapper opts into `melange-preamp` and disables default features, activating upstream's noise and rail-sag controls. This changes the solver choice and CPU cost compared with OpenWurli's default fast plugin; it does not require changing generated circuit code.
