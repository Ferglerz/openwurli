# OpenWurli CPU optimization: implementation and measured evidence

Measured 2026-09-29. **The selected release path uses native analytical circuit
math and the exact reed, mixing, and coefficient-cache optimizations.** Across
1,542 full-engine scenarios, its 85,872,960 output samples were bit-identical to
both the original scalar controls fork and its existing SIMD implementation.
At 48 kHz with 256-sample blocks, measured render time fell 10.64% for 32 held
voices and 15.69% for 64 voices against the scalar version. The additional gain
over the already implemented SIMD version was 1.98% and 3.84%, respectively.
Sparse playing showed small regressions in this run; this is not a universal
CPU improvement claim.

The broad circuit lookup-table candidate is **rejected for release**: although
its function interpolation errors were small, it failed the agreed output
error limits in nine of 70 controls-fork scenarios. An isolated amplifier-only
candidate remains under investigation. Neither candidate is enabled in the
selected release path. No listening or spectral test results are invented or
implied by these numerical measurements.

## 1. Baselines, branch ownership, and sound compatibility

This work preserves the instrument based on **upstream v0.7.0**, commit
`3023a8a6c42c654c5caeec52fd22dcf2cff3cb15`. The optimization branch
`codex/dsp-cpu-optimizations` starts from that actual upstream revision, making
the optimization changes reviewable separately from fork-specific controls.
The controls fork consumes that branch; the UI consumes the integrated DSP.
A later upstream PR is not created or implied by this report.

The relevant source revisions are:

- `3023a8a6c42c654c5caeec52fd22dcf2cff3cb15`: original upstream v0.7.0.
- `6614ab9519471e956ecb0c82edc4fcf295f724dc`: v0.7 controls fork, before the SIMD
  patch; called `scalar` by the full fork harness.
- `612dbda54cad9a8a666aa5dc6496d73f5d5da27d`: controls fork with the existing
  reed SIMD and iterator mix loops; called `shipping` by that harness. The label
  identifies the earlier implementation, not a second currently installed plugin.
- `cab1a759d9839f6002674dd469867eb8b598bec7`: exact optimization changes on
  the actual upstream base.
- `81c0916f15944afefbb8fc9baf793ff1d46a6012`: upstream-based branch with the
  permanent analytical functions and optional table experiment. Native math is
  the default; enabling the experiment changes numerical evaluation.
- `9cf99766173d24ce1cbd53423cb992be5509c6f2`: integration of that branch into
  the controls fork.

The full benchmark ran while the optimized sources were local changes on
`612dbda`, before the clean commits and integration were recorded. Its **source
hashes and saved working-tree patch**, not the HEAD field alone, identify the
measured implementation. Later source cleanup or integration must not be
misrepresented as a new full measurement. The separate actual-upstream proof
below verifies the clean branch's native result against `3023a8a` directly.

The locally fetched latest upstream is **v0.9.0**, commit `531680b`. Its changelog
records different pickup geometry, hammer-derived reed swing, amplitude-dependent
reed loss, removal of random reed pitch drift, revised MLP training, and +14 dB
output alignment in the upstream plugin. Those are deliberate instrument changes.
They are outside this optimization comparison and are not included by updating
this fork. Porting these optimizations onto v0.9 requires a new baseline and
measurements; the cached v0.7 jitter calculation is especially not a v0.9 benefit
when that jitter has been removed.

## 2. What changed, why it can be faster, and its audio consequence

### Existing reed SIMD and mix loops

Modes 0–5 of each seven-mode reed are advanced as three adjacent `f64x2` pairs;
mode 6 remains scalar. Independent mode arithmetic can execute together, while
contributions are summed in the original mode order. Precision, RNG advancement,
onset, damper behavior, and oscillator renormalization remain unchanged. The
engine's voice summation and stolen-voice fades use paired slice iterators.
These changes were already present in `612dbda`; their benefit must not be
attributed entirely to this new work.

Expected audio consequence: the same calculation and summation order. Measured
consequence: zero differing bits in all three full-engine pairwise comparisons.

### Cache the reed's jitter-corrected rotation

The original Ornstein–Uhlenbeck jitter changes once per 16 samples. Its phase
correction operands are constant between updates, but the old implementation
recomputed the same correction each sample. The new implementation evaluates
those same multiply/add/subtract expressions immediately after the original
jitter update, then reuses the result until the next update. Sample-zero
initialization and state across host block boundaries are retained.

This removes repeated work from every active mode without approximating the
oscillator, skipping jitter updates, or changing the random sequence. The
separate reed proof compared 35,168,256 **f64** output samples per pair across
all 64 keys, six velocities, four sample rates, irregular/zero/one-sample buffers,
damper progression, and repeated note-off: zero mismatches.

For 32 held reeds at 48 kHz, the isolated reed medians were 63.934 ms scalar,
41.072 ms existing SIMD, and 32.820 ms cached SIMD. This is 48.7% less reed time
against scalar and 20.1% less against existing SIMD. These are **reed-only**
figures; the full engine still spends time in the shared circuits. The full
engine numbers in section 4 are the appropriate UI evidence.

### Cache the tremolo depth divider

The divider's fixed resistance terms depend on tremolo depth, so they are
recomputed when depth changes and reused otherwise. The Twin-T oscillator,
CdS response, resistance law, parameter smoothing, and depth-update timing
remain the original calculations. Automation still updates the cached terms
as the smoothed depth changes; this is most useful after a control has settled.

Expected audio consequence: none, because unchanged operands reuse the same
result. The full native matrix, including parameter moves and multiple tremolo
cycles, rendered identical output. No isolated whole-engine CPU percentage is
claimed for this cache alone.

### Cache the legacy preamp's corrected kernels

The preamp reuses its Sherman–Morrison scale and corrected kernels while accepted
LDR conductance is unchanged. When the existing resistance update accepts a
change, the original expressions update the cache. The existing 0.01-ohm update
deadband, history conductance, physical constants, solver iteration limits, and
signal gain remain unchanged.

This removes repeated matrix-related work, particularly when tremolo is off.
It is not a new deadband or a reduced-rate circuit simulation. The native matrix
passed bit identity; the three extra tremolo-off CPU cases are retained in the
raw report. The combined measurements do not isolate an independent speedup
for this cache, and low-polyphony results do not demonstrate a benefit.

### Retain the analytical functions permanently

Native `circuit_math::exp` and `circuit_math::tanh`, preamp
`bjt_analytical`, amplifier `forward_path_analytical`, and the analytical LDR
resistance path remain callable without enabling a reference-engine feature.
The same physical-model code accepts native or table evaluators, avoiding a
separate maintained copy of the circuit. Table construction and out-of-domain
fallback evaluate the retained native functions and shared circuit constants.

These functions are **permanent source of truth**, not cleanup candidates.
Future tables must be regenerated and compared when the source equations change.
Only reconstructed historical benchmark engines are disposable.

## 3. Audio proof and coverage

The full controls-fork matrix has 1,536 single-note cases: MIDI 33–96, velocities
1/64/127, 44.1/48/88.2/96 kHz, and MLP off/on. The sample-rate set crosses the
engine's oversampling boundary. Blocks of 1/64/257/1024 samples and three
parameter profiles are distributed across these cases. This is not the full
Cartesian product of every block size and parameter combination.

The profiles include volume 0.25/0.7/1, tremolo 0/0.5/1, speaker 0/0.5/1,
reed decay/hammer hardness/pickup drive 0.5/1/2, tremolo response 0.5/1/2, and
rail sag off/on. Each lifecycle case applies note-on, pedal, note-off,
same-note retrigger, parameter moves, pedal release, and final note-off at
exact sample positions that split ordinary host blocks. Six additional cases
cover 6/32/64/80 allocations, an eight-second release tail, and an eight-second
held chord; 80 allocations force voice stealing.

The three implementations receive identical ordered MIDI, note-counter-derived
noise seeds, parameters, and block boundaries. Optional preamp noise is disabled
(a no-op in the default legacy preamp). All three pairwise comparisons produced:

- **85,872,960 samples per pair; zero differing sample bits.**
- Peak and RMS null residual exactly zero; residual level is negative infinity
  dBFS. JSON represents those infinite/undefined dB values as `null`, so the raw
  zero amplitudes and mismatch counts are the decisive fields.
- Zero nonfinite output pairs and zero reported voice-output NaN guards.

For these identical sample arrays, the audio waveform and any identically
computed spectrum are identical. There is no residual to evaluate perceptually
in those tested renders. This is stronger and narrower than a claim that every
possible input on every system will sound unchanged. No listener study was run.

The complete-engine comparison observes final mono **f32 engine output**;
internal circuits remain f64, but internal state equality is not established
by f32 output alone. The separate reed comparison does observe f64 output.
Voice NaN guards do not count every internal circuit reset, so finite output
alone is not a proof of solver convergence.

### Direct proof against original upstream

`upstream-native-quick` compares `3023a8a` → `cab1a75` → native `81c0916` using
only original upstream APIs. All 70 scenarios and **3,235,200 samples per pair**
were bit-identical, finite, and free of reported voice guards. This run was
audio-only: it supplies no separate upstream CPU result. It is a quick matrix,
not a claim that the 1,542-case controls-fork matrix was rerun on the clean branch.

## 4. Whole-engine CPU results

Measurements use Apple M1, 8 GiB RAM, macOS 26.6.2 build 25G83,
`aarch64-apple-darwin`, rustc 1.98.1 (`48a229cea`, 2026-09-01), LLVM 22.1.8,
release builds, and default compiler target settings. `RUSTFLAGS`,
`CARGO_ENCODED_RUSTFLAGS`, and `CARGO_BUILD_TARGET` were empty. CPU/RAM were
verified separately on the host; the benchmark sandbox denied `sysctl`, and
its provenance records that limitation rather than inventing hardware metadata.

At **48 kHz, 256-sample blocks, seven repetitions**, median render time per
sample was as follows. A lower value means less render CPU time.

- **1 held voice:** scalar 1,029.48 ns; existing SIMD 1,025.03 ns; final native
  1,034.74 ns. Final time increased 0.51% versus scalar and 0.95% versus existing
  SIMD. The final median corresponds to 264.89 microseconds per 256 samples.
- **6 held voices:** scalar 1,222.29 ns; existing SIMD 1,198.52 ns; final native
  1,206.03 ns. Final time decreased 1.33% versus scalar but increased 0.63%
  versus existing SIMD. Final median: 308.74 microseconds per 256 samples.
- **32 held voices:** scalar 1,711.63 ns; existing SIMD 1,560.30 ns; final native
  1,529.47 ns. Final time decreased **10.64% versus scalar**, including the
  existing SIMD benefit, and **1.98% versus existing SIMD**. Final median:
  391.55 microseconds per 256 samples.
- **64 held voices:** scalar 2,253.56 ns; existing SIMD 1,976.05 ns; final native
  1,900.09 ns. Final time decreased **15.69% versus scalar** and **3.84% versus
  existing SIMD**. Final median: 486.42 microseconds per 256 samples.

For context, the older scalar→SIMD patch alone reduced the 32/64-voice medians
by 8.84%/12.31%. These gains are already included in the total reductions above;
percentages from successive changes must not simply be added.

The full **39-case** CPU matrix covers 1/6/32/64 held voices, 44.1/48/96 kHz,
64/256/1024-sample blocks, plus three tremolo-off cases at 44.1 kHz/256.
Against scalar, final median reductions ranged from **−1.20% to +21.98%**;
30 of 39 cases improved. Against existing SIMD, reductions ranged from
**−2.12% to +6.16%**; 22 of 39 improved. These ranges describe this run, not
confidence intervals. Small differences can reflect scheduling, code layout,
cache state, or real overhead; there is no statistical proof here that every
small regression is measurement noise.

### Timing method and interpretation

Each case renders 0.5 seconds through the normal release `WurliEngine::render`
function, with `black_box` preserving outputs. Engine construction, circuit/table
initialization, the 0.6-second settling pass, allocations, note-on/MLP setup,
MIDI handling, and parameter setters are outside timed intervals. This is
render cost, not total plugin lifecycle or note-on cost. Variant order cycles
through all six permutations, with fresh identically initialized engines and
an untimed warmup before measured repeats. Raw per-block durations are retained.

Seven measured repetitions yield a median and p95 of **repetition-average
ns/sample**. With seven observations, the reported p95 is the maximum; it is
not a measured worst-case audio callback deadline percentile. Timer overhead
is included and affects small blocks proportionally more. CPU reduction means
`100 * (1 - candidate_time/reference_time)`. It is not percentage speedup, a
DAW CPU-meter measurement, a guarantee of available polyphony, or a result for
another processor.

The full run was recorded 20:43:55–20:54:03 UTC on 2026-09-29. Its provenance
reports unchanged source hashes during execution and unchanged dependency locks.

## 5. Lookup-table experiments and acceptance decisions

### Broad circuit LUT: rejected for release

The initial opt-in `experimental-circuit-lut` candidate uses 2,048-segment cubic
Hermite tables for exponential, tanh, and nominal LDR resistance, generated
from the retained native equations. Domains are `[-40,40)`, `[-12,12)`, and
`[1/64,1)` respectively; outside those half-open intervals, native evaluation
is retained. The LDR dark tail remains native. Four coefficients per segment
use approximately 192 KiB across all three shared tables. Construction occurs
before audio processing; lookup performs no allocation. Newton derivatives
come from the actual interpolated polynomial, not a derivative formula for a
different function.

The limits were fixed before the candidate render: peak null residual no greater
than **−120 dBFS** and RMS null residual no greater than **−100 dB relative to
reference RMS**, in every case. These are conservative engineering limits,
not universal hearing thresholds. Finite output, stable event behavior, no new
reported guards, spectral/alias review, and useful repeatable CPU improvement
are additional requirements.

Despite small direct function errors, `cpu-ab-lut-quick` failed those output
limits in **9 of 70 controls-fork scenarios**. Across 3,235,200 output samples,
2,193,904 samples had different bit patterns. The worst peak null residual was **+14.37
dBFS**, and the worst relative RMS residual was **−1.33 dB**, both in the long
held tremolo chord. Other failures included forte note/MLP cases and dense
lifecycle/stealing scenarios. Outputs remained finite with zero reported voice
guards. Thus a finite-output check would have missed the unacceptable change.
The process returned failure, and no full LUT release comparison was pursued
on that failing configuration.

This does not establish the exact mechanism of every failure. It establishes
that small pointwise function error is insufficient evidence in the complete
nonlinear feedback model, especially across the fork's extended controls.
Its CPU results are retained for diagnosis, not advertised as accepted gains.

### Original-upstream LUT quick result: passes limited numerical gates only

The same broad candidate on original upstream controls, `upstream-lut-quick`,
passed the selected limits in 70 audio-only cases. Of 3,235,200 samples,
1,979,645 differed from native. The largest peak residual was **−138.47 dBFS**;
the worst relative RMS residual was **−131.71 dB**. All output remained finite
with zero reported voice guards. This is useful evidence that the original
control range behaved differently from the extended fork cases; it does not
reverse the rejection for the consuming UI/fork. No CPU cases, listening tests,
or dedicated harmonic/alias comparison were performed in that run.

### Amplifier-only LUT: candidate decision pending

**Not shipping; final trial pending.** The current experiment narrows the feature to
one 2,048-segment tanh table over `[-12,12)` (64 KiB) used only in the amplifier;
exponential, BJT, and LDR evaluation remain native. Its quick output comparison
reported a worst peak residual of −144.49 dBFS and worst relative RMS residual
of −178.81 dB. An eleven-repeat CPU comparison was 0.28–2.30% slower after
normalization against the unchanged reference; that result does not justify
promotion. A final cached-table-reference trial remains under investigation.
Record its final source, evidence paths, bounds, spectral/alias status, and CPU
result here before closing the candidate decision. Do not reuse the rejected
broad candidate's measurements as proof for this narrower implementation.
Native analytical circuit evaluation remains the release default.

## 6. Other proposals and their dispositions

- **Damper recurrence:** not implemented in the release path. Repeatedly
  multiplying a decay factor changes rounding and accumulated envelope behavior
  compared with the original per-sample exponential. It needs a separate
  numerical and audio case, so the original damper remains.
- **Reuse equal capped damper rates:** implemented experimentally and found
  bit-identical, but worsened held-reed timing from about 32.9 ms to
  40.2–40.8 ms on this compiler. Reverted. Exactness alone does not justify
  a performance optimization that loses on the measured workload.
- **Cross-voice SIMD/compact active list:** deferred. Free slots are already
  skipped before rendering; reorganizing active voice state adds masks for
  onset, damping, noise, pickup, and steals, and can change summation order.
  No speedup or audio guarantee is claimed for unimplemented code.
- **Reciprocal/algebraic rewrites:** no additional data-dependent pickup or
  crossfade reciprocal substitution is shipped. Such reassociation can change
  f64 rounding and requires its own comparison.
- **Wavetable or f32 quality modes:** excluded from this exact release path.
  They change evolution or precision and would require explicit user-visible
  mode semantics plus separate fidelity evidence.

## 7. Reproduction, evidence retention, and cleanup

The engine harness is `tools/cpu-ab`, in its own Cargo workspace. It materializes
separate baseline crates from pinned full Git hashes; immutable manifests verify
both generated snapshots and source files shared with the current DSP. Separate
crates prevent feature unification from silently applying optimizations to the
reference. A source change during measurement invalidates the run. Preserve
both baseline commits in reachable Git history after deleting the old planning
branch; a missing Git object prevents reconstruction.

For the controls-fork full native proof:

```sh
python3 tools/cpu-ab/run.py --mode full --repeats 7 --export-all --output /tmp/native-proof
```

For a table candidate, run the same immutable source set with the relevant
explicit feature and the agreed limits. The command below selects the current
opt-in experiment; its implementation may be narrower than the historical broad
candidate. To reproduce the rejected broad run, first restore the exact source
recorded in its archived patch/provenance:

```sh
python3 tools/cpu-ab/run.py --mode full --repeats 7 --export-all --experimental-circuit-lut --max-peak-dbfs -120 --max-rms-relative-db -100 --output /tmp/lut-proof
python3 tools/cpu-ab/compare_runs.py /tmp/native-proof /tmp/lut-proof --output /tmp/native-vs-lut.json
```

Run default and candidate sequentially without competing builds or benchmarks.
`--audio-only` and `--cpu-only` support focused reruns; a zero-case section is
not evidence for that phase. The portable actual-upstream harness uses
`3023a8a`/`cab1a75` references and omits only fork-specific setters.

The canonical fork evidence directory is `docs/cpu/records`. It retains:

- `native-full-report.json.gz`, `native-full-provenance.json.gz`, and
  `native-full-working-tree.patch.gz` for the full accepted native comparison.
- `lut-rejected-quick-report.json.gz`, `lut-rejected-quick-provenance.json.gz`,
  and `lut-rejected-quick-working-tree.patch.gz` for the rejected broad candidate.
- `reed-proof-arm64.json` for the separate f64 reed study.

Raw run directories additionally contain `audio.json`, `cpu.json`, combined
`report.json`, provenance, selected unnormalized float WAVs and difference WAVs;
`--export-all` preserves every candidate waveform as little-endian f32. The
original full run is `/private/tmp/cpu-ab-native-full`; the rejected run is
`/private/tmp/cpu-ab-lut-quick`; direct upstream runs are
`/private/tmp/upstream-native-quick` and `/private/tmp/upstream-lut-quick`.
Temporary paths are not durable archives: retain necessary records alongside
this report before removing them. Amplifying a difference WAV is useful for
diagnosis but is not its level during ordinary playback.

Generated reference directories are gitignored and explicitly disposable after
review; they are not duplicate maintained implementations and never link into
the plugin. The reed proof is marked **“TEMPORARY — DELETE AFTER STUDY”**.
Delete generated historical code/build artifacts after evidence acceptance;
retain manifests, results, regeneration instructions, and the permanent native
analytical functions. No code named `DELETE_AFTER_BENCHMARK` is required or
assumed by this protocol.

## 8. UI communication and remaining limits

The ENGINE view should identify native analytical processing and present the
32/64-voice **whole-engine** figures with 48 kHz, 256-sample blocks, Apple M1,
and the scalar baseline stated. It should distinguish the smaller incremental
gain over the existing SIMD implementation, link this report, and show that
experimental table processing is inactive. These are static measured release
figures, not a callback profiler or a newly adjustable sound-quality parameter.

These results establish the tested default legacy-preamp/behavioral-power-amp
engine behavior. They do not establish equivalent results for optional generated
circuit models, every automation trajectory, every compiler/CPU, or a DAW's
plugin wrapper and scheduling. No REAPER interaction, perceptual listening
study, or dedicated LUT spectrum/alias audit is claimed. For the native cases,
bit-identical output establishes zero waveform/spectrum difference in those
renders. For any approximation, complete numerical, spectral, stability, and
performance evidence remains necessary before changing the default.
