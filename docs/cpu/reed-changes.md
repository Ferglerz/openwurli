# Reed implementation and audio evidence

The production reed keeps the original seven `f64` modes, OU jitter, onset,
progressive damper, renormalization schedule, and summation order. The existing
SIMD change processes modes 0–5 in pairs and mode 6 as scalar, then sums in
original mode order. It does not combine notes or approximate an oscillator.

The new change caches the jitter-corrected sine/cosine rotation immediately
after each 16-sample OU update. The jitter drift and base phase increment are
constant between updates, so the former per-sample expressions would return
exactly the same floating point values on every intervening sample. The same
multiply/subtract/add operations now run once per update. RNG advancement and
its mode order are unchanged. Sample zero initializes the cache after its
original jitter update, before the first oscillator step. Cache values persist
across host buffer boundaries, including one-sample and empty buffers.

## Evidence

`records/reed-proof-arm64.json` records a local ARM64 release run. Comparing
original scalar, shipping SIMD, and final cached SIMD yielded zero differing
bits across **35,168,256 samples per pair**, across all 64 keys, six velocities,
four sample rates, irregular block boundaries, onset/sustain, damper and repeated
note-off. Thus the sampled reed output difference is exactly zero; this is
stronger evidence than a listening opinion for those cases. It is not proof
for every platform, compiler, input, or the complete plugin chain.

For 32 held reeds, 192,000 samples per reed at 48 kHz, ten interleaved measured
rounds after two warmup rounds yielded median elapsed time of 63.934 ms for
original scalar, 41.072 ms for shipping SIMD, and 32.820 ms for cached SIMD.
That is 48.7% less reed time against the original scalar implementation and
20.1% less against the shipping SIMD implementation. These are reed-only
figures. Use the whole-engine study for plugin CPU claims.

The historical C4 golden checksum was generated on a different platform.
This host's original scalar, shipping SIMD and final cached implementations
all give `12062458625402485202`, while the historical fixed test expects
`6843218719074185147`. Comparing matching toolchains directly avoids treating
platform math-library rounding as a DSP change. The production test now checks
block-size independence (one render versus chunks of 1, 15, 257 and 1024 samples)
instead of enforcing that historical architecture-specific hash.

## Evaluated proposals

- **Damper recurrence:** left unchanged. Multiplying a running exp multiplier
  changes rounding and accumulated envelope error relative to the original
  per-sample exp expression. It is not an exact algebraic replacement in f64.
- **Reuse equal capped damper rates:** an exact candidate was implemented and
  passed the same audio comparison, but increased held reed elapsed time from
  about 32.9 to 40.2–40.8 ms on this compiler. Splitting a helper and eliminating
  an extra state field did not recover the held-loop gain. Reverted rather
  than ship a steady-state regression for a short note-off benefit.
- **Cross-voice SIMD / compact list:** engine already skips free slots before
  rendering. A list only removes up to 64 state checks per block; the expensive
  work is inside active voices. Reorganizing all state adds masking for onset,
  damper, noise, pickup and stolen-voice fades and risks changing summation
  order. It remains deferred, with no performance benefit claimed.
- **Reciprocal rewrites in reed/voice/mix:** none shipped. Data-dependent pickup
  and crossfade divides preserve original rounding; note-on-only divisions do
  not warrant a change to the per-sample model. Reassociating divisions with a
  stored reciprocal would require its own numerical gate.
- **Wavetable and f32 modes:** deliberately excluded from this exact path because
  they alter the instrument's time evolution or precision. No fidelity claim
  is made for these proposals.

## Reproduction and cleanup

`tools/reed-cpu-proof` generates historical sources from git objects into its
ignored Cargo build directory. It maintains no second DSP implementation.
Its README documents the versions and commands. The comparison tool is marked
**TEMPORARY — DELETE AFTER STUDY**; keep the record and documentation after the
study is accepted. Full-chain comparison, circuit candidates, and UI status are
covered by the accompanying engine study.
