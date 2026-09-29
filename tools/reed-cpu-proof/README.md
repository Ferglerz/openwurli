# Temporary reed CPU / audio proof — DELETE AFTER STUDY

This standalone crate retrieves both historical reed implementations from git
into Cargo’s ignored build directory. Historical source is never duplicated
in the maintained tree. The referenced git objects must remain available. It is excluded from the plugin
build and has its own Cargo workspace. Delete this directory only after the
results have been accepted and archived; retain `docs/cpu/records` as evidence.

- Scalar: byte-for-byte reed source materialized from fork commit
  `6614ab9519471e956ecb0c82edc4fcf295f724dc` (original baseline).
- SIMD: byte-for-byte reed source materialized from fork commit
  `612dbda54cad9a8a666aa5dc6496d73f5d5da27d` (already shipping SIMD/mix patch).
- Post: current `crates/openwurli-dsp` through a relative path dependency.
  Archive the exact post commit or source hash alongside every record.

Override `REED_SCALAR_REF` and `REED_SIMD_REF` with commit hashes to compare
other archived baselines. Changing either rebuilds the generated modules.

Run from the fork root:

```sh
cargo run --release --manifest-path tools/reed-cpu-proof/Cargo.toml -- --audio-only
cargo run --release --manifest-path tools/reed-cpu-proof/Cargo.toml -- --json docs/cpu/records/reed-proof-arm64.json
```

The first command compares f64 sample bits for all 64 keys, six velocities
(0, .001, .3, .8, .999, 1), 44.1/48/88.2/96 kHz, deterministic per-case seeds,
and irregular additive buffers including zero and one sample. Three phases
exercise attack/sustain, progressive damping, and repeated note-off. They cross
16-sample jitter and 1024-sample oscillator renormalization boundaries. Each
baseline is compared against 35,168,256 post samples. This is reed proof only:
full engine/pickup/circuit/host comparisons belong in the companion CPU A/B tool.

Timing runs 32 reeds for 750 blocks of 256 samples at 48 kHz (128 voice-seconds).
Initialization is excluded. Each of 12 rounds rotates scalar/SIMD/post order;
two warmup rounds are discarded and ten measurements retained. It reports
median/min/max wall time. This workload measures a held reed, not damper or
full plugin CPU. Run on an idle host without other benchmarks/builds. CPU
frequency, scheduling, temperature, compiler and target affect measurements.
