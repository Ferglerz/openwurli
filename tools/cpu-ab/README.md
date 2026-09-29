# Temporary three-way CPU and audio proof

**Reference implementations are generated, gitignored comparison artifacts.**
`run.py` materializes `upstream/` and `exact/` from pinned Git revisions before Cargo.
Do not commit those directories. Delete them after review sign-off; regenerate on demand.
Retain the measured JSON, provenance, harness, and protocol as release evidence.
The candidate implementation is `crates/openwurli-dsp`; it is the shipping code and must
not be deleted with these baselines. No baseline is linked into the plugin.

The upstream snapshot is the actual original v0.7.0 tag commit `3023a8a` (full SHA in
`baseline-manifest.json`). The exact optimization snapshot is `cab1a75`; the current
production DSP provides the candidate, currently `81c0916`. The third implementation
runs native analytical math by default and generated LUTs only with the opt-in feature.
This matrix deliberately uses **only original upstream APIs**: it does not depend on
fork-specific reed-decay, hardness, pickup-drive, or tremolo-response controls.
Six potentially changed modules are materialized separately in each temporary baseline crate. Other
modules are shared through source paths to avoid copying generated circuit solvers.
`baseline-manifest.json` records their original git blob hashes and the snapshot SHA-256s.
The build fails if shared sources or snapshots change. When a shared module needs a future
edit, copy its **old** contents from each baseline git revision into that baseline first,
update its `#[path]`, then update the manifest deliberately. Never bless a changed shared
file by silently replacing its old hash. The two separate packages prevent Cargo feature
unification from accidentally optimizing a baseline.

## Run

```sh
python3 tools/cpu-ab/run.py --mode quick --output /tmp/cpu-ab-quick
python3 tools/cpu-ab/run.py --mode full --repeats 7 --export-all --output /tmp/cpu-ab-exact
python3 tools/cpu-ab/run.py --mode full --repeats 7 --export-all --experimental-circuit-lut --output /tmp/cpu-ab-lut
```

Use `--audio-only` or `--cpu-only` to rerun one stage; reports retain the count of
actually executed scenarios (a CPU-only run is not audio evidence).

Use an empty output directory for each run. Cargo runs offline against locally cached
dependencies; provision those dependencies before measurement if starting fresh.
For native-versus-LUT proof, run the exact native matrix first, then the LUT matrix
without editing any DSP/harness source between them. `--export-all` retains every
candidate engine waveform as little-endian f32 (~300 MB per full run), enabling direct
comparison of current native analytical equations with their generated LUT candidate:

```sh
python3 tools/cpu-ab/compare_runs.py /tmp/cpu-ab-exact /tmp/cpu-ab-lut --output /tmp/native-vs-lut.json
```

The comparator requires identical source hashes/scenarios and valid runs. It compares
current candidate outputs directly, so it remains useful even when future native math
intentionally changes. For historical upstream/SIMD gates, update pinned refs deliberately
when such a change is accepted; never silently bless changed baseline hashes.

Use an empty output directory for each run. Run on an otherwise idle target machine,
with the same power state, release compiler/flags, and native CPU settings for all variants.
Do not compile other crates or run other timing jobs simultaneously. The tool records
hardware, OS, toolchain, Cargo.lock, revisions, features, source hashes, and the local diff.
Sources changing during a run invalidate it. Allow several minutes for the full matrix;
compilation and the 0.6-second engine initialization/settling passes add wall time.

Default runs return failure for **any** bit mismatch, nonfinite sample, or NaN guard.
The experimental feature permits finite differences for measurement; it makes no
perceptual claim. Optional `--max-peak-dbfs VALUE --max-rms-relative-db VALUE` enforce
reviewer-selected engineering limits for exact-to-candidate output. Record the chosen
limits and rationale *before* interpreting a candidate as accepted. Add `--require-exact`
to demand bit identity even with the experimental feature. Passing any such numeric
bound is not proof of inaudibility.

## Audio protocol

The full matrix renders all 64 physical keys (MIDI 33–96) at velocities 1, 64, and 127,
44.1/48/88.2/96 kHz, with MLP on and off: 1,536 single-note scenarios. Block sizes
1, 64, 257, and 1024 and three parameter profiles are distributed over that matrix;
this is **not** the Cartesian product of all block sizes and parameter settings.
Profiles cover the original upstream controls: tremolo 0/0.5/1, speaker 0/0.5/1,
volume 0.25/0.7/1, and rail sag on/off. Original reed/pickup voicing and CdS response
remain exactly as defined by the v0.7 upstream engine. The engine's deterministic note-counter seed is identical in each
implementation; optional preamp noise is disabled (a no-op in the legacy circuit).

Each lifecycle scenario applies note-on, sustain, note-off, same-note retrigger,
parameter changes, pedal release, and final note-off at exact sample positions. Event
positions deliberately split ordinary host blocks. Six additional scenarios exercise
6/32/64/80 allocations (80 forces voice stealing), an 8-second release tail, and an
8-second held chord through multiple tremolo cycles. Quick mode uses 8 distributed keys,
2 velocities, 2 sample rates, and both MLP states, plus the same six extended scenarios.
MLP affects note setup, which is covered by output comparison; MIDI setup is excluded
from the render CPU benchmark.

All three pairwise comparisons report bit mismatch count/first mismatch, nonfinite
count, peak and RMS null residual, residual dBFS, residual relative to reference RMS,
and the engine’s voice-output NaN guards. These guards do not count all internal
circuit resets; finite output alone is not a convergence proof. These are final mono **f32 engine outputs**, not internal f64
states or a host/plugin render; clipping or conversion can conceal internal differences.
`null` dB values mean exact zero (-infinity), or an undefined zero reference; inspect
raw amplitudes and nonfinite counts. The longest examples also export unnormalized
32-bit float WAVs of each implementation and candidate-minus-exact. Play the pair
at identical gain; a difference WAV can be amplified for diagnosis, but amplified
residual listening does not represent the actual level during normal playback.

## CPU protocol and interpretation

The tool uses a separate excluded Cargo workspace; ordinary builds do not require
reference sources. Both pinned revisions must exist in local Git history.

The full CPU matrix has 1/6/32/64 held voices at 44.1/48/96 kHz with 64/256/1024 sample
blocks, each rendering 0.5 seconds, plus three 44.1-kHz/256-sample tremolo-off cases
(1/6/32 voices) to measure the fixed-LDR cache path. Quick mode uses 44.1 kHz/256 only. All variants are
in one release executable, call the same engine API, and use equal MIDI/parameter setup.
Engine construction, circuit settling, buffer allocation, MIDI events and parameters
are outside the timed intervals. The measured region is the real uninstrumented
`WurliEngine::render`; `black_box` preserves its outputs. Each block uses the same
monotonic timer. Timer overhead is included and is most relevant for very short blocks.
An untimed pass warms each implementation before measured runs.

Seven repeats default. All six engine orders alternate cyclically to reduce systematic
order/cache/thermal bias. Each repetition starts a fresh identically settled engine.
Raw block durations and render ns/sample are preserved; median and p95 summarize the
**per-repetition ns/sample**, not audio callback deadline percentiles. Seven samples give
only a coarse p95 (the maximum), so do not use this as a worst-case realtime guarantee.
The report gives candidate/reference time ratios for upstream→exact, exact→candidate, and
upstream→candidate. CPU reduction is `100 * (1 - candidate_time / reference_time)`.
It is not the same as percentage speedup or DAW CPU-meter reduction.

## Limits and cleanup

The exact default matrix can establish identical rendered samples for these scenarios,
compiler, CPU, and DSP feature set. It does not establish equality for every host,
compiler, parameter trajectory, or opt-in generated preamp/power-amp model. Approximate
LUT candidates require spectral/harmonic analysis and controlled listening as additional
evidence before a shipping decision; this tool alone does not supply those conclusions.

Once the user accepts the evidence: preserve docs/results; delete the generated ignored
`upstream/`, `exact/`, and target directories. The harness can regenerate the references
when another comparison is required. The production branch contains a single DSP path
plus explicitly optional LUT code; historical engine implementations stay in Git history.
