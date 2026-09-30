# Second CPU study: immutable installed baseline, paired heavy/fast engines

`cpu-next` is a standalone evidence tool. It is not linked into the plugin.
`reference.py` reconstructs **all** public and private DSP source files from
`fd4f603dd8deef7a57d4449826f2c9d5dd53757e`, the installed baseline for this study,
into the ignored `reference/` crate. The manifest pins SHA-256s for every file
and the renamed Cargo manifest. Generation and builds fail on drift, missing
files, or unexpected extra files. No historical engine copy is maintained in
Git. Delete generated reference/build directories after accepting the study;
retain the manifest, generator and recorded evidence.

## Engine selection and independent candidates

`--engine heavy` is the default: both implementations disable dependency defaults
and enable `melange-preamp`, selecting generated preamp and power-amp models.
`--engine fast` selects `legacy-power-amp` on both, with the legacy preamp.
Reference and candidate always select the same backend. Neither backend's
numbers are evidence for the other. The runner uses an offline locked build and
records the exact compiler, source, features, environment, locks and local diff.

`--features` is a comma-separated set applying **only to the candidate**:

- `cpu-study-heavy-matrices`
- `cpu-study-preamp-reuse`
- `cpu-study-tremolo-zero`
- `cpu-study-damper-recurrence`
- `cpu-study-amp-transfer`

Features are independent experiments, not an acceptance statement. Use
`--require-exact` for changes claiming bit identity; approximation runs use the
numerical gates below. Empty `--features` supplies the current native candidate
and is useful for checking harness/backend equivalence first.

## Runtime-mode equivalence

`--runtime-modes` enables `openwurli-dsp/runtime-models` **only on the candidate**.
The candidate is constructed with `new_with_circuit_mode` and explicitly selects
`CircuitMode::Heavy` or `CircuitMode::Fast` from `--engine`; the unchanged fd4f603
reference remains a fixed-mode compile. This prevents `WurliEngine::new`'s Fast
default from silently contaminating a Heavy comparison. Coverage, provenance and
combined reports record candidate runtime support, selected initial mode, and the
absence of runtime support in the reference.

Use this switch with `--require-exact` to check that selecting a steady runtime
mode preserves its corresponding original compiled engine. The harness does not
switch mode mid-render; crossfade/transition verification is a separate task.
Reset and sample-rate scenarios do exercise mode persistence. Algorithm-only
studies should omit this switch until their independent results are established.

```sh
python3 tools/cpu-next/run.py --engine heavy --runtime-modes --audio-only --scenario-filter repeat-chord-1- --require-exact --output /tmp/runtime-heavy-pilot
python3 tools/cpu-next/run.py --engine fast --runtime-modes --audio-only --scenario-filter repeat-chord-1- --require-exact --output /tmp/runtime-fast-pilot
```

## First screens and full commands

Freeze DSP and harness sources before measuring; do not run builds or other CPU
studies simultaneously. Pilot the heavy backend first to estimate local runtime.

```sh
python3 tools/cpu-next/run.py --engine heavy --audio-only --scenario-filter repeat-chord-1- --require-exact --output /tmp/heavy-native-audio
python3 tools/cpu-next/run.py --engine heavy --cpu-only --scenario-filter hold-1-,hold-6- --repeats 3 --output /tmp/heavy-native-pilot
python3 tools/cpu-next/run.py --engine heavy --features cpu-study-heavy-matrices --require-exact --mode full --repeats 11 --export-all --output /tmp/heavy-matrices-full
python3 tools/cpu-next/run.py --engine fast --features cpu-study-damper-recurrence --mode full --repeats 11 --export-all --output /tmp/fast-damper-full
```

Output directories must be empty. `--audio-only` and `--cpu-only` avoid repeating
unrelated phases. `--scenario-filter` accepts comma-separated substrings (OR).
`--list` builds the chosen executable then prints its coverage without creating
an engine or rendering audio; `--verify-only` checks/regenerates baseline source
without compiling. Filtered/phase-only results explicitly record their coverage.
A CPU-only report contains `audio_verified: false`, not a vacuous audio pass.

## Audio matrix and gates

The prior 1,542-case matrix is retained: all 64 keys, velocities 1/64/127,
44.1/48/88.2/96 kHz, MLP off/on; distributed 1/64/257/1024-frame blocks and
three parameter profiles; pedal/retrigger/release/stealing and eight-second
held/released cases. Profiles exercise native and fork control ranges as in the
first study. Optional preamp noise is disabled identically; note noise uses
identical note-counter-derived seeds.

Additional cases cover repeated 1/6/12-note chords, pedal bursts, repeated
note-off, tremolo zero→active→zero→active transitions, four-second release tails,
reset and sample-rate changes, and silence. Extra full cases use 44.1/48/96 kHz
and MLP off/on; quick extras use 48 kHz and MLP on. Totals: **1,639 full audio
cases or 87 quick cases**. Reset/sample-rate scenarios restart the 20 ms window
grid at rate changes and preserve each segment's rate in metadata. Raw f32 files
with that metadata are the authoritative representation; a single-rate WAV is
not exported for mixed-rate scenarios.

Default limits apply to **both the complete render and sliding 20 ms windows
with 50% overlap**:

- Peak residual ≤ −60 dBFS.
- Residual RMS ≤ −60 dB relative to the corresponding reference RMS.
- A reference with exactly zero energy requires exactly zero residual energy.
- Every output sample must be finite; voice-output NaN guards must remain zero.
- `--require-exact` additionally requires equal f32 sample bits everywhere.

The limits are configurable with `--max-peak-dbfs` and
`--max-rms-relative-db`. Gates use linear amplitudes, so JSON's inability to
represent −infinity cannot silently pass a nonzero residual against silence.
Signed-zero changes are counted as bit mismatches even though their residual
energy is zero. Final partial windows are included so tail samples are covered.
Worst peak/relative windows, failure counts and the first 20 failing windows are
retained, including their sample positions. A passing average cannot mask a
failing transient window.

These are conservative engineering bounds, not universal hearing thresholds.
Final output comparison does not prove equal internal f64 state or stable
internal Newton solves. Voice guards do not count every internal circuit reset.
Separate diagnostic instrumentation may examine those states, but diagnostic
build timing is not a CPU result from this tool.

## Callback, render and event timing

Quick CPU coverage has **17 workloads** at 48 kHz/256 frames: held 1/6/12/32/64
voices, plus repeated chord/release, pedal bursts, settled depth zero and
zero→active tremolo for 1/6/12 voices. Full coverage repeats these at
44.1/48/96 kHz and 64/256-frame host blocks: **102 workloads**.

Eleven repeats default, alternating reference-first/candidate-first order every
repeat. Each implementation receives identical schedules and fresh engine state.
Engine construction, preallocation, initial parameters, circuit settling and
output-buffer allocation happen before timing; an untimed pass warms code first.
The 0.65-second musical render includes scheduled note-ons, including initial
attacks, in its callback/event cost. Thus event-cost results include real voice
construction/MLP work that the earlier render-only study excluded.

Host callback boundaries remain at the requested block grid. Events split render
segments **inside** each callback; they do not redefine subsequent host callback
boundaries. Three measurements are recorded per callback:

1. The normal, uninstrumented DSP `render()` segments, summed as render cost.
2. Scheduled event batches, including note-on/off, pedal and parameter setters.
3. The complete callback, including event processing and render dispatch.

Nested monotonic timers and small local counters add harness overhead to the
callback measurement. Their cost is present for both implementations and is not
subtracted. Per-callback record storage occurs after the outer timer. No timer
is inserted into a circuit stage or solver loop. `black_box` preserves samples.
Reset/rate-change warm-up is covered in audio cases and is not part of the CPU
workload matrix.

Raw callback durations, sample counts, event counts and render-call counts are
preserved. Reports include actual callback median/p95/p99/max duration and
callback/deadline ratios, event-bearing callback cost, per-repetition ns/sample,
and paired time ratios. Repetition statistics and callback tails are labeled
separately: eleven repetitions do not establish a worst-case realtime guarantee.
Incomplete final blocks remain in raw data and use their actual frame count for
deadline ratios. CPU reduction is `100*(1-candidate/reference)`; a negative
number is a regression. OS scheduling and thermal effects still matter.

## Evidence files and invariants

`coverage.json` records exact scenarios, features and filters. `audio.json`,
`cpu.json`, and `report.json` retain the results; `provenance.json` and
`working-tree.patch` identify the source. `--export-all` adds reference and
candidate little-endian f32 waveforms. Selected long/transition cases export
unnormalized float WAV pairs and residuals for offline investigation. A source
hash change during a run invalidates it and returns failure.

The runner hashes complete reference/candidate/harness source and dependency
locks before and after measurement, and records RUSTFLAGS and Cargo target
settings. It reports restricted hardware queries honestly. Untracked production
source content is not necessarily included in Git's diff: preserve it in a
commit or explicit archive together with the hashes before discarding a checkout.
No benchmark result, acceptance claim, or backend promotion is implied merely
by successful compilation of this tool.

## Untimed power-amplifier trajectory diagnostics

Audio comparisons sample the existing `power_amp_diag()` API after initialization,
after every render segment, and immediately after reset/sample-rate events.
This captures clamp counts, Newton maximum-iteration counts and peak output volts
before later resets can erase previously observed values. Comparison records
counter mismatches, peak-bit mismatches, maximum extra candidate counts, observed
counter activity, first 20 differing snapshots and final values. `--export-all`
retains the complete paired trajectory JSON beside each case's waveforms.

`--require-exact` fails on any diagnostic trajectory difference as well as output
bit differences. Approximate runs fail on additional candidate clamp/iteration
counts, nonfinite peaks or misaligned trajectories; other diagnostic differences
are explicitly marked for review. Existing baseline/candidate solver events also
set review-required status even when matching, so a passing output null is not
presented as proof that the physical solver is sound. Counter accumulation records
positive increments and detects drops across resets; hidden resets within a single
render/event call cannot be reconstructed through this public snapshot API.

No diagnostic sampling occurs in CPU timed renders **or their warmups**. These
are power-amplifier diagnostics only; the API does not expose preamp Newton/reset
counters, and this tool does not invent them or modify generated reference source.
