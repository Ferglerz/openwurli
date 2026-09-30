# Fast and Heavy circuit modes

This pass adds a runtime choice between the fork's existing circuit models.
It preserves the fork's v0.7 sound-model basis; it does not import upstream
v0.9's changed reed, pickup, calibration, or amplifier feedback model.
CPU measurements and candidate acceptance are recorded separately. This page
makes no speedup or Fast-versus-Heavy audio-equivalence claim.

## What the modes select

**Fast** uses the hand-written nine-node preamp and the behavioral amplifier's
native nonlinear forward function and Newton feedback solve. **Heavy** uses
the generated full preamp, its separate silent shadow solver for pump
cancellation, and the generated seven-transistor Class AB power amplifier.
The modes therefore represent different circuit implementations. A CPU
optimization must be compared with its own mode's original implementation;
Fast and Heavy are not expected to produce the same waveform.

Both modes use the same seven-mode reed, hammer, pickup, 64-slot voice bank,
MIDI handling, sustain state, and per-note RNG sequence. A switch does not
retrigger a note, create a second instrument, or run a second voice bank.
Tremolo and the volume, tremolo-depth, and speaker-character parameter clocks
advance once. Each circuit mode owns its preamp, amplifier, oversampler,
speaker state, and preallocated scratch buffers. The shared reed sum and
shared per-sample LDR values feed whichever circuit chains are active.

Below 88.2 kHz, each active circuit chain processes the preamp and amplifier
at twice the host rate and downsamples before its speaker. At higher rates it
uses the host rate. This retains the existing engine's scheduling and output
gain operations in each constant mode.

## Switching and saved settings

The optional `runtime-models` feature makes `CircuitMode::{Fast, Heavy}` and
`WurliEngine::{new_with_circuit_mode, set_circuit_mode, circuit_mode}` available.
Without this feature, the existing compile-time circuit selection is retained.
With it, `WurliEngine::new` starts in Fast. The wrapper's new mode parameter
must also default to Fast, so presets saved before the parameter existed keep
their previous circuit selection.

A live change starts a 20 ms linear crossfade between the two circuit outputs.
A reversal keeps the current blend and ramps toward the newly requested mode;
it does not jump to either endpoint. The voice bank continues once throughout.
Both circuits run during the transition. Processing splits at fade completion,
then stops the hidden circuit immediately. An inactive circuit retains its
last state; it does not continuously follow the input while hidden. A later
switch therefore reintroduces a paused circuit history under the crossfade.
This is a deliberate CPU/state tradeoff, not a claim that every switch is
inaudible.

Constructor allocation and solver preparation belong outside normal audio
processing. `new` and `new_with_circuit_mode` retain the historical engine's
initial state: adapter constructors prepare their native settled DC states,
and the host then calls `warm_up`, `reset`, or `set_sample_rate` during
preparation. The engine's existing approximately 0.6-second internal silence
render warms **both** circuit chains with the same tremolo sequence. Reset and
sample-rate preparation snap the mix to the requested mode before this warmup,
so restoring Heavy does not open with an unintended Fast-to-Heavy fade.

The wrapper should load only the saved mode before `set_sample_rate`, then
synchronize its other parameters in the existing order. Moving unrelated
parameter updates before warmup would change historical startup behavior.

## Hiss and the two meanings of sag

The wrapper's existing `FastFinish` remains a common post-engine stage for
both modes. Its Hiss is the existing output-noise effect. Its Sag control is
an additional output-envelope effect with its existing attack/release and
gain formula. Switching circuit mode does not reset these common effects.

Heavy also retains the generated amplifier adapter's physical rail dynamics,
enabled by default. That behavior belongs to the Heavy circuit itself and is
separate from the wrapper's extra Sag effect. Turning off the wrapper's Sag
does not disable Heavy's physical rail sag. Fast retains its existing
behavioral circuit treatment. This preserves previous preset meanings rather
than silently reassigning the common effects controls to different physics.

## Original equations and comparison paths

The analytical functions remain maintained source: the native BJT currents
and derivatives, behavioral amplifier forward function, LDR resistance law,
and original damper multiplier. Experimental lookup tables derive their data
from those functions; they do not replace the reference equations with a
second hand-maintained approximation. Study features remain separate from
runtime mode selection. Selecting Heavy or Fast is not itself a request to
enable every experimental approximation.

Historical complete engines used for pre/post comparison are materialized
from Git into temporary or ignored build locations. They are measurement
inputs, not maintained duplicate DSP trees. Retain source revisions, features,
compiler/host conditions, and measurement records when removing study tools.

## Completed routing proof and its limits

A local release-mode proof compared the new constant-mode scheduler against
the historical engine source at `3257c22157becd2ef4abc477e27cef1ab5250bdf`,
using the corresponding native circuit types in each case:

- Fast: **213,600 output samples** bit-identical at 44.1 and 96 kHz.
- Heavy: **213,600 output samples** bit-identical at 44.1 and 96 kHz.
- The sequences include warmup, six-note playing, volume/tremolo/speaker
  automation, sustain, release, reset, and empty/1/15/127/257/1024-frame blocks.

The focused `runtime_model_switch_preserves_notes_and_stops_hidden_chain`
test also passed. It checks finite transition output, two held voices and
unchanged note age through switching, no jump in the blend on reversal, and
that hidden circuit processing clocks stop outside the transition.

These checks establish the tested routing behavior. They are **not a listening
or inaudibility proof for live switching**, not a proof that the circuit
models sound identical, and not a cross-platform guarantee. The switch test
currently does not count amplifier divergence-guard resets. Their occurrence
cannot be inferred from finite output or endpoint equality. Per-mode audio
error limits and measured CPU results belong in the accompanying study.

## Allocation and reset limits

`set_circuit_mode` only updates the mode and fade target. It allocates no
memory and does not reset or settle either solver. Runtime rendering uses
preallocated buffers and chunks oversized output slices without growing
those buffers. Both statements concern the new mode-selection/routing code.

The existing Heavy amplifier fault guard has a separate limitation. When its
solver produces a non-finite, unconverged, or physically implausible state,
`PowerAmp::process` calls the adapter's `reset`. That reset restores
`init_state(sample_rate)`, which clones the cached generated `CircuitState`.
The generated state contains `Box<CircuitStateCold>`; cloning it allocates a
new box and replacing the prior state frees the old one. At non-native rates
it also rebuilds the rate-dependent matrices. This behavior predates runtime
switching and remains unchanged in this pass. The guard retains `last_good`
to hold the previous valid output through recovery.

Explicit `WurliEngine::reset` additionally resets voices and both circuits,
resets tremolo, and runs the approximately 0.6-second internal warmup. It is
not a constant-cost operation for the audio callback. `set_sample_rate` also
reconstructs circuit objects and their buffers. Hosts should schedule these
preparation operations appropriately; neither is called by mode selection.

Consequently this pass does **not** claim the entire Heavy processing callback
is allocation-free. A scoped allocation/guard study is still needed to
establish whether particular switching sequences exercise existing recovery.
Simply changing `CircuitState::clone` to `clone_from` would not establish a
fix: the generated state's derived `Clone` uses the default whole-value
`clone_from`, which still clones its box. Any future in-place recovery change
must prove identical restored state and output against the native reset.
