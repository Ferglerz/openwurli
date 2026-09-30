# Temporary runtime-mode switch proof

Standalone evidence tool; not part of plugin processing. **Retain this proof and
regeneration tool in Git**, together with accepted reports and their provenance.
Generated historical checkouts and build artifacts are disposable; the source
needed to reproduce the evidence is preserved.
It depends on the current DSP through a relative path with `runtime-models`.
No historical DSP implementation is copied into this tool.

This tool records allocation requests, audio trajectories and aligned listening
excerpts. It does **not** measure CPU speed, set an audibility threshold, or
assert that Fast and Heavy sound alike. No quiet-host timing conclusions may be
drawn from these runs. Schedule performance timing separately on an idle host.

## Build and run

```sh
cargo build --release --offline --manifest-path tools/cpu-mode-switch/Cargo.toml
# Quick pilot: silence and one held/released note at 48 kHz.
tools/cpu-mode-switch/target/release/cpu-mode-switch \
  --rates 48000 --voices 0,1 --output /tmp/openwurli-switch-pilot
# Default coverage: 30 cases, with three matched renders per case.
tools/cpu-mode-switch/target/release/cpu-mode-switch \
  --output /tmp/openwurli-switch-study
```

Optional `--kind held|release-return|both` and `--hidden-seconds 1.0` select the
sequence and paused interval. Durations are bounded to 0.05–5 seconds; the
default is one second. `--rates` supports 44100, 48000, 96000 and `--voices`
supports 0, 1, 6, 12, 32. Use small filters for initial diagnosis. Full coverage
is intentionally bounded, but Heavy's runtime depends on solver convergence.

Independent DSP study candidates can be selected with corresponding Cargo
features: `cpu-study-heavy-matrices`, `cpu-study-preamp-reuse`,
`cpu-study-tremolo-zero`, `cpu-study-damper-recurrence`. These do not imply
acceptance. Record the exact combination for each run.

The build captures compiler/version/target/profile, feature and rustflag
environment, source Git blob hashes, commit, local production diff and lockfiles.
Execution checks that those source files still match, both before and after
measurement. Rebuild after changing any relevant source; do not move the checkout
or delete its Git data between build and execution. This tool records build
provenance, not an assertion that the current branch is clean.

## Scenarios and controls

Every rate/polyphony case renders the same MIDI/parameter sequence three times:
a switching engine, continuously Fast, and continuously Heavy. These are offline
comparison runs; the runtime engine itself keeps its single shared voice bank.
All three have the same notes, velocities, gain, tremolo, speaker setting and
noise-off setting. Heavy's physical rail sag retains its default enabled state.
Wrapper `FastFinish` hiss/extra Sag is excluded, so these are raw-engine checks.

- **Held/reversals:** play while Fast, leave Heavy paused for the hidden interval,
  then switch and reverse several times both during and after the 20 ms fade.
- **Release/silence/return:** start Heavy and excite its history, switch to Fast,
  release all keys, leave Heavy paused while the active chain decays, then return
  to Heavy. This exposes possible reappearance of paused tails. This sequence
  uses keys 33–91, excluding the naturally undamped top keys; held-note cases
  span 33–96. Single-note cases use middle C.
- **Zero voices:** run the same mode timelines in silence.

The render block sequence includes 1, 15, 127, 257 and 1024 frames, split exactly
at every event. Setters must not alter held/active voice counts. Restoring and
resetting engines happens before measurement, never as part of a mode event.

## Allocation scope and interpretation

A forwarding `GlobalAlloc` wrapper counts allocation, deallocation, reallocation
requests and bytes. Counting is enabled exactly around `set_circuit_mode` and
`render`, separately. Construction, warmup, MIDI/parameter setup, WAV writing,
serialization and report assembly are outside the counters. This is a single
threaded executable; do not add background work during a measured scope.

Reports retain complete totals, plus the first 256 render calls with any
allocation activity and their frame ranges. A nonzero setter allocation, a setter
changing voice counts, or non-finite output makes the run fail after writing its
report. Render allocations are recorded rather than automatically rejected:
Heavy already has a fault-recovery path that clones a boxed solver state.

The current amplifier diagnostic tuple is saved before and after each render,
but it is not a durable divergence-reset count: existing resets can clear its
counters. Allocation counts alone do not prove the cause of each allocation.
Zero allocations in a render scope rule out that boxed-reset allocation in the
measured scope; they do not establish safety for every future input. If render
allocations appear, correlate their frame ranges with recovery instrumentation
before calling them a switching regression.

## Audio output and limits

Each case JSON contains peaks, finiteness, setter events, allocation events,
40 ms transient windows, and post-fade 20 ms recovery windows against the
continuously running Heavy reference. Transient records include the step at the
mode event, preceding/following local maximum sample steps, corresponding stable
mode steps, and differences from the continuously running target mode.

Recovery is descriptive: the first complete 20 ms window below a peak difference
of 1e-4 (-80 dBFS), remaining below that bound through all later complete windows
with at least three such windows. Null means that criterion was not observed
within the recorded excerpt. It is not an inaudibility verdict or shipping gate.
Larger differences while transitioning between different models are expected;
paused-history tails and long recovery still require explicit evaluation.

For audition, every case writes matching mono float WAV excerpts for switching,
stable Fast and stable Heavy. They start at the same frame 100 ms before the
main return/switch and retain identical gain without normalization. File names,
frame alignment and metrics are in each case JSON and the combined `report.json`.
Listen at a fixed gain; do not compare independently normalized files.

## Completed 48 kHz pilot

A release build with `cpu-study-heavy-matrices,cpu-study-preamp-reuse,cpu-study-tremolo-zero`
completed all four 48 kHz cases for zero/one voice on 2026-09-29. Evidence was
written to `/private/tmp/openwurli-pass2-evidence/mode-switch-pilot/report.json`
and the adjacent JSON/WAV files. Build provenance is embedded in that report;
source hashes matched before and after execution. This is a small pilot,
not the full rate/polyphony matrix.

All output was finite, mode setters allocated nothing and preserved held/active
voice counts. **The release/return scenario exposed a paused tail:** returning
to Heavy after a one-second hidden interval produced a peak of 0.000267278
(−71.46 dBFS) versus an approximately 1e-9 continuously Heavy reference. Its
20–40 ms post-switch window still had a peak difference of 0.000123614;
the descriptive −80 dBFS recovery criterion was met from 40 ms onward.
The immediate event-edge step was 0.0000443394. These observations require
explicit transient evaluation; the tool's contract checks do not establish
inaudibility or approval to ship the switching behavior.

Render allocation activity also occurred: continuously Heavy allocated four
24,224-byte blocks in the held-note case and three in the release case. The
switching release case allocated two such blocks at the initial note attack
(frames 143–1424), before the first mode event at frame 4800. No allocation
occurred around the later return to Heavy in this pilot. Held-note switching
and both silent timelines had zero render allocations. This supports the
existing Heavy recovery-allocation limitation; durable guard instrumentation
would still be needed to attribute every allocation conclusively.

No CPU timing was measured. The subsequent full run below was explicitly
scheduled to qualify chord sizes and sample rates after these pilot findings.

## Full frozen-source run: switching audio not accepted

The same binary and candidate features completed all 30 cases at 44.1/48/96 kHz.
`/private/tmp/openwurli-pass2-evidence/mode-switch-full/report.json` records
matching source hashes before/after, all finite outputs, and no setter contract
violations. `assessment.json` is a concise derived summary tied to the report's
SHA-256. WAV excerpts accompany every case.

**Released-chord returns exceed the −60 dBFS absolute bound at every rate.**
Six voices reached −55.93/−55.90/−56.88 dBFS (44.1/48/96 kHz); twelve reached
−49.19/−49.90/−46.63 dBFS; thirty-two reached −48.52/−46.07/−47.52 dBFS.
The descriptive −80 dBFS recovery criterion was met from 60–80 ms for these
chords. These are reintroduced paused histories after a one-second hidden
interval with released damped keys and noise off. One-note returns were
−70.80 to −71.90 dBFS; silent return sequences stayed below −179 dBFS.
This supports rejecting the current stale-history switching behavior against
the requested absolute bound; it is not a listening-based audibility judgment.

Immediate event-edge steps stayed below the larger corresponding stable-mode
40 ms maximum. However, subsequent transient steps were larger in two held-note
cases: twelve voices at 44.1 kHz reached a step of 0.00858337 (2.59 times the
larger stable-mode maximum), and thirty-two voices at 48 kHz reached 0.00851069
(1.76 times). Existing recovery allocations occurred in the same transition
windows. A linear blend alone therefore does not establish clean switching.

Across the 30 cases, switching renders allocated 99 times, continuously Heavy
382 times, and continuously Fast zero times. Some switching allocations were
immediately adjacent to returning to Heavy, including six voices at 44.1 kHz
(frames 49393–49408 after the mode event at 49392). Removing the existing
Heavy reset's Box replacement is a separate allocation fix; it does not fix
paused tails or change these audio acceptance findings. No CPU timing was
collected and no generated solver source was modified during this run.
