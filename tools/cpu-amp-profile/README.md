# Behavioral amplifier entry-gate profile

This tool estimates the share of Fast render time spent in the original,
memoryless behavioral amplifier. It does not measure a table optimization.
Heavy's stateful generated amplifier is outside the experiment.

`generate.py` reconstructs the entire DSP at the installed reference revision
`fd4f603dd8deef7a57d4449826f2c9d5dd53757e`. One generated crate is unchanged;
the other captures actual amplifier inputs during an **untimed** render. The
captured inputs are replayed through the original amplifier. Only the unchanged
engine and independent replay are timed, with eleven alternating repetitions.
Both generated copies and build artifacts are ignored and disposable. Original
source hashes are retained in `reference-source.json`.

```sh
python3 tools/cpu-amp-profile/generate.py
cargo run --offline --locked --release --manifest-path tools/cpu-amp-profile/Cargo.toml
```

Profiles use 1, 6 and 12 held notes, velocity 100/127, 48 kHz, 256-frame buffers,
MLP off, volume 0.7, tremolo 0.5 and speaker 0.5. Each timed render lasts half a
second; setup, 0.6-second circuit settling and input capture are outside timing.

Replay is a stage-cost estimate, not an instrumented call-stack profiler. It
includes loop/black-box overhead and does not reproduce stage scheduling or
instruction-cache context. The measured share was 3.76–3.90%, well below the
approved 10% entry gate. Consequently the whole-amplifier LUT was not built.
This result does not establish the amplifier share for every setting or host.
