# Heavy recovery shortcut: reproducible proof

This folder is self-contained. DSP archives reconstruct the original baseline, the candidate used in audio proof, and the candidate used in CPU proof. The scripts create standalone renamed packages; they never edit production sources, test/check vendored crates, install a plugin, use REAPER or create a PR. Cargo dependencies must already be cached for offline builds.

From this directory:

```sh
python3 prepare.py
python3 build.py
python3 run.py audio --out new-audio
python3 build.py --full
python3 run.py audio-full --filter '48000-heavy,prior-review-heavy' --out new-full
# Stop other builds/rendering before CPU measurement.
python3 run.py cpu --out new-cpu --repeats 5
```

Scripts resolve defaults relative to their own location, so the folder can move. Output directories must be new. `--work PATH` selects another build directory on each script. `prepare.py --candidate-source PATH` evaluates a new candidate. To recreate the exact historical audio source instead of the final test-inclusive candidate:

```sh
python3 prepare.py --work audio-build --candidate-archive audio-candidate-dsp.tar.gz --harness-source measured-src/main.rs
python3 build.py --work audio-build
python3 run.py audio --work audio-build --out repeat-historical-audio
python3 build.py --work audio-build --full
python3 run.py audio-full --work audio-build --filter '48000-heavy,prior-review-heavy' --out repeat-historical-full
```

The equivalent direct release build commands after preparation are:

```sh
CARGO_TARGET_DIR="$PWD/build/target" cargo build --offline --locked --release --manifest-path build/Cargo.toml -p recovery-proof
CARGO_TARGET_DIR="$PWD/build/target-full" cargo build --offline --locked --release --manifest-path build/Cargo.toml -p recovery-proof --features recovery-candidate/reference-full-recovery
```

Both sides compile default `legacy-power-amp` and `runtime-models`. The second build additionally enables `recovery-candidate/reference-full-recovery` on the candidate only. The main manifest sets release debug information to 1; it does not enable custom LTO or native CPU tuning. Raw provenance records rustc, architecture, binary hash and `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_TARGET`, `RUSTC_WRAPPER`. All four environment flag strings were empty for these recorded runs. This harness uses the default generated exp approximation. Native precise-exp full-engine equivalence is not claimed; the parent's separate focused state test is separate evidence.

`archives-sha256.json` identifies each compressed archive. Per-file original source hashes are in the corresponding `*-source-hashes.json`. DSP package names are the only non-test modifications made by preparation. The audio snapshot preceded the final focused test additions in `src/power_amp.rs`; CPU snapshot includes those additions under `cfg(test)`. Release implementation is identical. The historical audio archive was reconstructed by removing precisely those tests and verified against every recorded original file hash before archiving. Thus both recorded sources can be reconstructed exactly.

The source baseline was captured before production implementation. It is the local v0.7-derived DSP, not a new upstream model. Full generated recovery remains available in the candidate; the feature selects that complete path in the adapter too.

## Diagnostic accounting correction

The original 53-case audio matrix and CPU evidence are retained without source-hash changes. A review found that inferring a rate rebuild only when a counter decreases can miss counts when the new warmup total is already larger. The corrected harness records each Rate boundary explicitly. A targeted replay of just the two mode/rate/reset cases compared another 120,000 samples with exact output/public state and frozen-source provenance. Their counters changed from 48 to 53 and 30 to 37. Corrected aggregate totals add 12 primary/skipped/guard events: 4,627 / 4,627 / 0 / 4,647 / 20. No production code or runtime audio behavior changed. The full eight-case filter has no rate cases and is unaffected.

`results/counter-reconciliation.json` records both totals and per-case replacements. The CSV summary uses corrected counters, with a counter-evidence column identifying the replay rows. Original JSON reports remain intact. `measured-src/main.rs` is the exact original measured harness, SHA-256 `88d57ddd0ad3d833b55cfefa1af13cb966d377dfaa325c5e7a38283b7e102dd5`, verified against every original run. `src/main.rs` is corrected; `rate-counter-accounting.patch` shows the only difference. The historical reproduction commands select both historical candidate and harness. For an exact historical CPU source, use `prepare.py --harness-source measured-src/main.rs` with the final candidate archive.

## Recorded evidence

- Default policy: 53 cases, 998,080 compared f32 samples, zero differing bits, public-state mismatches or nonfinite samples. 4,627 primary failures and skipped recoveries, zero recovery attempts, 4,647 guard resets, 20 final-iteration rejections.
- Complete reference policy: eight focused cases, 121,984 exact samples. 1,240 primary failures/recovery attempts, zero skipped recoveries, 1,242 guard resets, two final-iteration rejections.
- Exact prior-review six-note fixture: default counters `[13,13,0,14,1]`; complete policy `[13,0,13,14,1]`, ordered primary/skipped/attempted/guard/final-iteration. It actually exercises rejection on iteration 249.
- CPU: 14 workload rows, five alternated pairs per row. Every paired waveform hash and final public state matched. Six-note Heavy, tremolo 0.5: 21.870 -> 15.890 µs/sample, 27.44% median paired reduction. 64-note Heavy: 50.585 -> 26.779 µs/sample, 47.07% reduction. Fast 6/64 sanity reductions were +0.14% / −0.04%.
- Six-note Heavy median trial p99 callback: 29.241 -> 6.016 ms; maximum callback: 29.576 -> 6.823 ms. 64-note p99: 58.943 -> 13.283 ms; maximum: 72.657 -> 15.960 ms. Candidate p99 still exceeds the 5.333 ms deadline for both cases.

CSV files make every case and paired trial directly reviewable. JSON reports retain full snapshots; provenance and logs are beside each report. CPU reduction is the median of per-pair candidate/reference ratios, not the ratio of separate medians. P99 summaries are medians of five per-trial p99s; maxima are over all five trials. No unrelated builds or rendering overlapped the 93.5-second CPU run. Background scheduling and thermal state were not controlled, and this is bounded CLI wall-clock evidence, not DAW-meter or deadline compliance proof.

## Coverage and limits

The default matrix covers 44.1/48/88.2/96 kHz, Fast/Heavy, MLP off/on, held 6/64 voices, all 128 extended MIDI notes, f32 velocities 0.001/0.25/0.95/1, decay 20×, sustain/release/steal, hardness/pickup/volume/speaker/response/rail controls, runtime cancellation/preroll/crossfade/reversal, sample-rate rebuild/reset, and a five-second tail. Blocks 1/64/257/1024 occur across the matrix. This is focused coverage, not a full Cartesian product. Extended-note APIs are exercised directly; the UI's toggle gating is outside this DSP harness.

Every output sample compares f32 bits and finite status. Every callback compares public amp counters, f64 peak bits, NaN guard, voice counts and sustain state. Default lifetime invariants require primary==skipped, attempted==0; complete mode requires skipped==0, attempted==primary. Guards must cover primary plus final-iteration rejection counts. The corrected harness explicitly starts a new circuit lifetime after each Rate event, including all warmup counts, then resumes ordinary callback deltas. Private circuit/rail fields are outside this harness; the parent's focused exact-state test directly verifies them. Subsequent render trajectories, mode switches and rate resets screen state restoration here.

CPU covers Heavy 1/6/32/64 voices at tremolo 0/0.5, Fast 6/64 sanity rows, and Heavy six-note callback blocks 1/64/257/1024. Main trials render 94 × 256 samples; block rows render 8,192 samples. Each fresh engine gets 0.6 seconds warmup, excluded from time. Construction and note-on are excluded from render timing; note-on cost is separately recorded. Settings: 48 kHz, exact 0.95 f32 velocity, MLP/hiss off, volume 0.5, speaker 0, native rails. The two prior-review audio cases use exactly the same workload.
