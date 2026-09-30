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
list forces review if regenerated state gains fields. All original analytical
equations remain intact; the generated solvers' CPU edits are described below.

Heavy retains its physical rail sag. The UI's optional **Extra Sag** and **Hiss**
remain shared post-engine effects with their previous meanings.

## Exact CPU pass

The DSP source measured here is `f70325c`; later commits add only these
notes and records. In both modes its audio is bit-identical to the 0.1.1 engine (`3103753`): every change performs the
original floating-point operations in their original order, or skips work whose
result is already known or cannot reach the output. Physical equations,
constants, Newton iteration limits, tolerances and fallbacks are unchanged.

Heavy:

- **Shared preamp matrix rebuilds.** Each LDR update made the main and shadow
  preamps each invert two 13×13 matrices, and at this rate the trapezoidal and
  backward-Euler matrices are the same bits. One inversion now serves all four
  when both states hold identical matrices. If they differ or an inversion
  fails, each state rebuilds itself exactly as before. The inversion solves all
  identity columns together, and the Schur products skip exact-zero incidence
  entries.
- **No repeated BJT evaluations.** After the generated parasitic-BJT solve
  converges, it re-evaluated the transistor at unchanged voltages; that result
  is now kept. The power amplifier likewise reuses the evaluation from a failed
  residual check as the next Newton iteration's evaluation.
- **Lockstep transistor solves.** The power amplifier's seven transistors and
  the preamp's two advance their inner Newton loops together, so independent
  division and exponential chains overlap. Quotients of fixed device
  parameters are computed once per sample instead of on every evaluation.

Fast:

- **Legacy preamp currents.** When the DK preamp's Newton loop exits early, the
  final current evaluation at the unchanged voltage is reused.

Both:

- **Zero-depth vibrato.** At depth zero the LDR branch is grounded, so its
  power-law resistance mapping is skipped. The oscillator and LDR envelope keep
  running, and the mapping is refreshed as soon as depth rises above zero.

Not included: the damper-envelope recurrence (fails bit identity in both
modes) and division-to-reciprocal rewrites (change rounding).

### Evidence

Audio: `tools/cpu-exact` renders the candidate and a
reference built from the complete pinned `3103753` source tree, both with
runtime modes, and requires identical output bits. The full matrix covers
MIDI 33–96, three velocities, 44.1/48/88.2/96 kHz, MLP off/on, blocks of
1/64/257/1024 samples, three parameter profiles, dense/stolen lifecycles,
long tails, rate resets, and live mode switch, reverse and cancel.

- Heavy: 1,651 cases / 95,025,900 samples per pair, zero differing
  bits; power-amp convergence and fallback counters also identical.
- Fast: 1,651 cases / 95,025,900 samples per pair, zero differing bits.

Focused bit-identity tests compare the new inversion, Schur products, matrix
rebuild and lockstep transistor solve with reference copies of the original
generated code, including singular, non-finite and signed-zero inputs, Ebers–
Moll/PNP/no-parasitic device variants and drives that exhaust the inner Newton
iterations. The legacy-preamp and tremolo changes each keep a test against the
original trajectory.

CPU: median render time per host sample over eleven repetitions of 0.65 s
renders at 48 kHz with 256-sample blocks and MLP on. The candidate and the
0.1.1 reference run in the same process in alternating order. Measured on a
4-vCPU KVM Intel Xeon cloud VM (x86-64, rustc 1.98.1) with nothing else
running; Apple M1 was not measured for this pass. CPU reduction is
`100 × (1 − new/0.1.1)`. Real time at 48 kHz is 20,833 ns per sample.

| Held voices, vibrato 0.5 | Heavy 0.1.1 | Heavy new | Heavy reduction | Fast 0.1.1 | Fast new | Fast reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 26,991 ns | 11,929 ns | 55.8% | 1,111 ns | 1,038 ns | 6.5% |
| 6 | 31,825 ns | 14,574 ns | 54.2% | 1,524 ns | 1,492 ns | 2.1% |
| 12 | 38,166 ns | 20,000 ns | 47.6% | 2,258 ns | 2,219 ns | 1.7% |
| 32 | 46,008 ns | 27,610 ns | 40.0% | 4,075 ns | 4,010 ns | 1.6% |
| 64 | 53,114 ns | 34,383 ns | 35.3% | 7,289 ns | 7,320 ns | −0.4% |

| Workload, 1 / 6 / 12 voices | Heavy reduction | Fast reduction |
| --- | --- | --- |
| Repeated chord with releases | 55.1 / 52.7 / 42.1% | 3.7 / 2.5 / 2.2% |
| Sustain-pedal bursts | 55.5 / 52.1 / 38.9% | 3.9 / 2.2 / 1.6% |
| Held, vibrato depth 0 | 36.4 / 38.3 / 34.6% | 13.2 / 8.7 / 5.2% |
| Vibrato depth 0 → 1 → 0 → 0.37 | 51.7 / 50.3 / 45.4% | 6.6 / 4.4 / 4.2% |

Heavy improves in all 17 cases, most at low polyphony, where the circuits
rather than the reeds dominate. Fast improves in 16 of 17. Its gain is largest
with vibrato off. At 64 voices, where reeds dominate, its eleven paired
repetitions range from 4.5% faster to 4.0% slower, so no change is shown. The
depth-0 rows also set the engine's amplifier rail-sag flag off and speaker
character to 1. These are
render-time medians from one machine, not DAW meter readings or a polyphony
guarantee.

The earlier study remains on the Ferglerz fork at tag
[`cpu-pass2-study-2026-09-29`](https://github.com/Ferglerz/openwurli/tree/cpu-pass2-study-2026-09-29).
This pass's compressed reports, provenance and source patches are
`records/exact-pass2-*`. Per-callback timings are omitted from the CPU reports.
Reproduce with, for example:

```sh
python3 tools/cpu-exact/run.py --engine heavy --mode full --audio-only --require-exact --output /tmp/heavy-full-audio
python3 tools/cpu-exact/run.py --engine fast --mode quick --cpu-only --output /tmp/fast-quick-cpu
```

Run CPU phases alone; a source change during a run invalidates it.

This pass was built and checked on Linux. The UI's `scripts/install.sh` is
macOS-only (`~/Library` bundles and `codesign`), so it was not run.

## 0.1.1 integration checks

Completed integration checks include default/runtime compilation, initial
fixed-mode audio equivalence, preset migration, sample-rate/buffer capacity,
preparation/cancellation/reversal scheduling, exhaustive reset-state bits and
post-reset audio equivalence. Focused portable tests and Clippy passed. The
original 30-case switch screen exposed tails up to −46.07 dBFS; the later 100 ms
settling fix has focused scheduling coverage but its full transition audio and
allocation study was not repeated after the user's stop request. The final
plugin is built, installed and bundle-verified by the wrapper installer.

No upstream pull request is authorized or created by this release.
