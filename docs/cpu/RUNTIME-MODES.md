# Runtime Fast / Heavy release

The optional `runtime-models` feature exposes `CircuitMode::{Fast, Heavy}`,
`WurliEngine::new_with_circuit_mode`, `set_circuit_mode` and `circuit_mode`.
The UI enables this feature and saves the selected model. Fast is the default;
old presets without a mode restore Fast, even when loaded into a Heavy instance.
The instrument retains the upstream v0.7 sound-model basis.

## Processing and switching

Both models share one voice bank, MIDI/sustain state, reed sum, tremolo and
parameter clocks. Fast uses the native legacy preamp and behavioral amplifier.
Heavy uses the full generated preamp/shadow pair and generated power amplifier.
They have separate circuit, oversampler and speaker histories and can sound
different. Fixed-mode audio comparisons must use the same model on both sides.

Activating a stopped circuit streams 100 ms of current input through it while
the previous circuit remains audible, then crossfades over 20 ms. This addresses
released-note history returning when an inactive circuit resumes. The change
therefore takes about 120 ms. Both circuit chains run during preparation/fading;
the voice bank still runs once. The inactive chain stops afterward. Canceling
preparation keeps the audible model; reversing an active fade preserves its
current blend. Setters allocate no memory, reset no notes, and perform no
blocking settling loop. Preparation/reset outside processing warms both chains
and starts directly in the selected mode.

The existing Heavy divergence recovery previously cloned a boxed solver state.
It now restores every field from the same original settled cache into existing
storage, including reusing the cold-state Box. Original initialization,
rate conversion, rail reset and last-good behavior remain. An exhaustive field
list forces review if regenerated state gains fields. Generated solver source
and all original analytical equations remain intact.

Heavy retains its physical rail sag. The UI's optional **Extra Sag** and **Hiss**
remain shared post-engine effects with their previous meanings.

## What is shipped, and what is deferred

This release ships the two-engine integration and retains the prior CPU
optimizations. It does **not** include the new final-current reuse, zero-depth
LDR skip, Heavy matrix-sharing, or damper-recurrence candidates. Quiet CPU gates
were not completed before the user asked to stop testing and package the plugin.
No additional CPU gain or universal inaudibility is claimed.

The research is preserved on the Ferglerz fork at tag
[`cpu-pass2-study-2026-09-29`](https://github.com/Ferglerz/openwurli/tree/cpu-pass2-study-2026-09-29),
including original/native comparison source, regeneration tools, compressed
reports, spectra and exact source hashes. Fast study candidates passed 1,639
cases / 94,521,540 samples bit for bit; Heavy candidates passed 87 cases /
4,368,000 samples. The recurrence failed both engines. These results describe
those study builds, not the final shipping source.

Completed integration checks include default/runtime compilation, initial
fixed-mode audio equivalence, preset migration, sample-rate/buffer capacity,
preparation/cancellation/reversal scheduling, exhaustive reset-state bits and
post-reset audio equivalence. Focused portable tests and Clippy passed. The
original 30-case switch screen exposed tails up to −46.07 dBFS; the later 100 ms
settling fix has focused scheduling coverage but its full transition audio and
allocation study was not repeated after the user's stop request. The final
plugin is built, installed and bundle-verified by the wrapper installer.

No upstream pull request is authorized or created by this release.
