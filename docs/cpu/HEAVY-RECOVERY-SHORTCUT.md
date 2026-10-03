# Heavy recovery shortcut — item 1, 2026-10-03

Item 1 of the efficiency review is implemented. The Heavy adapter now skips recovery work whose result its existing guard would discard. In five alternating before/after trials on this M1, six-note Heavy at 50% tremolo used **27.4% less render time**, and 64-note Heavy used **47.1% less**. The focused engine comparison produced **998,080 identical f32 samples across 53 cases**. Fast's processing is unchanged; its timing sanity checks show no meaningful gain or regression.

This change removes wasted work. It does not change the sound model, convergence tolerance, iteration limit, oversampling, amplifier guard, or accepted solver result. Items 2–7 remain separate future work. No pull request or original-upstream change is part of this implementation.

## What changed and why it is exact

The generated power amplifier initializes `last_nr_iterations` to `MAX_ITER` (250) before its primary Newton solve. Only primary convergence changes that field. Adaptive substeps and backward-Euler recovery do not clear the sentinel. The adapter rejects values `>= MAX_ITER - 1`, restores the complete cached circuit state and rails, and holds its previous good output. Recovery after a failed primary solve therefore cannot survive into the instrument output or its next sample.

A new crate-private `process_sample_guarded` returns `None` immediately after primary failure. The adapter then performs the same full reset and output hold. Primary convergence on iteration 249 still takes the original generated path and is rejected by the unchanged adapter threshold. Nonfinite and excessive-node-voltage guards are unchanged.

The public generated `process_sample` retains the complete original recovery routine. Cached settling and raw generated diagnostics still use it. The original arithmetic remains in one const-generic implementation, avoiding two drifting copies of thousands of generated lines. This is a maintained patch to generated source: future regeneration must preserve both entry points and rerun the proof.

New saturating counters live outside the resettable circuit state: primary failures, skipped recoveries, attempted recoveries, adapter resets and iteration-249 rejections. They survive amplifier/engine resets. Reconstructing the amplifier, including through an engine sample-rate change, starts a new lifetime; the harness accounts for that boundary. Counters include warmup activity and are event counts, not Newton-iteration totals.

## Preserve and select the original reference

The `reference-full-recovery` DSP feature restores the adapter's original complete recovery processing. The default optimized path and this reference were compared against an untouched frozen baseline. Neither requires fetching an old checkout to retain the original equations.

From the OpenWurli fork checkout, build the retained reference with:

```sh
cargo build -p openwurli-dsp --release --features runtime-models,reference-full-recovery
```

The existing native exponential configuration is also preserved:

```sh
RUSTFLAGS='--cfg melange_precise_exp' cargo build -p openwurli-dsp --release \
  --features runtime-models,reference-full-recovery
```

From the audio-plugins workspace, the corresponding reference library can be built with:

```sh
RUSTFLAGS='--cfg melange_precise_exp' cargo build -p openwurli-ui --release \
  --features openwurli-dsp/reference-full-recovery
```

These commands select reference code; they do not install bundles. The normal installed build uses the exact shortcut and existing default math. Native exponential math was verified with the focused complete-state test below, not with the whole-engine matrix. Greater numerical accuracy alone does not establish greater physical accuracy.

## Paired CPU results

Apple M1 MacBook Air, 8 GB RAM, macOS 26.6.2, rustc 1.98.1, release standalone binaries, 48 kHz. Each 256-sample case renders 24,128 samples after 0.6 seconds of warmup. Five pairs alternate which version runs first. Render timing excludes construction, warmup and note-on; note-on is recorded separately. MLP and hiss off, volume 0.5, speaker character zero, f32 velocity 0.95, native Heavy rails enabled. No other task builds or benchmarks ran during this timing window.

Before/after values below are medians of each version's trials. Reduction is the median of the five paired reductions; it need not equal a ratio of the two separately rounded medians.

| Mode | Voices | Tremolo depth | Before µs/sample | After µs/sample | Paired median reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Heavy | 1 | 0.0 | 9.151 | 9.181 | -0.60% |
| Heavy | 1 | 0.5 | 14.981 | 12.719 | 15.03% |
| Heavy | 6 | 0.0 | 18.283 | 13.351 | 26.91% |
| Heavy | 6 | 0.5 | 21.870 | 15.890 | 27.44% |
| Heavy | 32 | 0.0 | 24.890 | 14.856 | 40.31% |
| Heavy | 32 | 0.5 | 19.792 | 16.488 | 16.63% |
| Heavy | 64 | 0.0 | 67.349 | 27.575 | 59.12% |
| Heavy | 64 | 0.5 | 50.585 | 26.779 | 47.07% |
| Fast | 6 | 0.5 | 1.192 | 1.192 | 0.14% |
| Fast | 64 | 0.5 | 1.901 | 1.901 | -0.04% |

The one-note/depth-zero case has no useful recovery work to remove and measured a 0.6% regression. This short run cannot separate such a small overhead from timing variation. Fast's differences were about +0.14% / −0.04%, with no meaningful improvement established. Different note sets produce different nonlinear trajectories, so these rows are not a controlled linear voice-scaling curve.

At 256 samples the deadline is 5.333 ms. Six-note Heavy at depth 0.5 improved from 29.241 to 6.016 ms for the **median of per-trial p99 block times**; the maximum over all five runs improved from 29.576 to 6.823 ms. For 64 notes those values were 58.943 to 13.283 ms (median p99), and 72.657 to 15.960 ms (maximum). Heavy still misses deadlines in these cases. Primary Newton work and preamp costs remain.

A separate six-note block-size sweep (1, 64, 257 and 1024 samples; only 8,192 samples per trial) showed 34.9–36.8% paired render-time reductions. Its shorter musical window prevents direct comparison with the 24,128-sample rows. At block size 1, p99 did not improve even though maximum latency fell sharply: this removes rare expensive recovery, not every expensive primary solve.

All 14 CPU configurations × five pairs matched waveform hashes and public final state. These are short CLI wall-clock measurements, not DAW CPU-meter readings or a guarantee under load. Scheduling and long-term thermal conditions remain uncontrolled.

## Fidelity and verification

The baseline is the untouched vendored DSP from Ferglerz/openwurli `b51f8dad3c5fb62070a20a11e20bf2886b8ff860`, with its existing standalone manifest and local full-MIDI APIs. Baseline/candidate archives, per-file hashes, binary hashes, feature flags, raw results and harness source accompany this report.

- **Default shortcut:** 53 cases, 998,080 f32 output samples, zero differing bits, zero nonfinite outputs and zero mismatched public diagnostic/voice summaries at callback boundaries. Coverage includes 44.1/48/88.2/96 kHz, Fast/Heavy, MLP off/on, six/64 notes, all MIDI 0–127, selected velocity/control extremes, 20× decay, sustain/release/stealing, tremolo changes, repeated mode changes, sample-rate changes, reset, varied block sizes and one five-second tail. This is selected coverage, not a full Cartesian matrix or full decay-duration proof.
- **Actual rejection events:** the default matrix observed 4,627 primary failures and exactly 4,627 skipped recoveries, zero attempted recoveries, 4,647 adapter resets and 20 actual iteration-249 rejections. Thus the boundary check is exercised rather than merely comparing two zero counters. These totals include a targeted 120,000-sample replay of the two rate-change cases after correcting harness lifetime accumulation; output remained exact. Original per-run evidence is preserved.
- **Retained complete reference:** eight focused cases, 121,984 identical f32 samples, 1,240 attempted recoveries, zero skipped recoveries, and two iteration-249 rejections. The retained full path matches the untouched baseline.
- **Private-state proof:** `heavy_recovery_shortcut_preserves_audio_and_complete_state` compares every generated circuit field, every rail field, held output and output bits after every sample across four internal rates (44.1/88.2/96/192 kHz), rail sag on/off, smooth/transient/polarity-changing and nonfinite inputs, and reset. It also checks stable cold-state allocation. A captured real failure proves the public generated solver still executes recovery while the guarded entry point does not.
- **Native math:** the same private-state test passed with `--cfg melange_precise_exp` as well as default math. The full engine matrix and CPU timings used default math.
- **Installed:** the required `openwurli-ui/scripts/install.sh` rebuilt the plugin, installed its CLAP and VST3 bundles, compared their executables with the fresh build, and verified their code signatures. Reload the plugin in the host to use the replacement.
- **Build/review:** `cargo check -p openwurli-ui`, targeted source-fork release unit test and `cargo clippy -p openwurli-dsp --features runtime-models` passed. Independent Sol review found no actionable defects. No vendored crate was directly checked or tested; standalone comparison binaries linked frozen copies. No REAPER inspection or listening test was performed.

Focused tests run from the source fork (not the vendor directory):

```sh
cargo test -p openwurli-dsp --release --lib --features runtime-models heavy_recovery --quiet
RUSTFLAGS='--cfg melange_precise_exp' CARGO_TARGET_DIR=/private/tmp/openwurli-recovery-precise-target \
  cargo test -p openwurli-dsp --release --lib --features runtime-models heavy_recovery --quiet
```

## Reproduction and provenance

See the [frozen proof package](../../tools/recovery-proof/README.md) for extraction, offline builds, exact comparison and timing commands. Use a fresh output directory and an otherwise idle machine for timing. Cached Cargo dependencies are required. All original source/licensing is retained in the snapshots. Fresh-directory extraction and the packaged offline build were verified, then the historical six/64-note cases reproduced 48,256 identical samples.

The harness originally inferred a new counter lifetime from a numerical drop, which undercounted rate changes when the new warmup count exceeded the previous count. Explicit rate-event accounting added 12 events to primary/skipped/reset totals. Only the two affected default cases were replayed; the eight reference cases and CPU results were unaffected. The original measured harness and the accounting-only patch are retained.

The final CPU candidate includes an added test-only fixture absent from the earlier full-engine comparison build. The packaged reconciliation records that only test code changed; release processing was identical. Historical per-run source hashes remain intact. Runtime changes after this report require fresh comparison, not reuse of these results.

Next proposed item is immutable device-math caching, especially in the common tremolo solver. It requires its own measured proof in both modes.
