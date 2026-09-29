# LUT isolation and disposition

Isolated diagnosis checkout: `/private/tmp/openwurli-lut-diagnosis`, cloned from
controls integration `9cf9976`. No changes were made to the validated checkout
while measuring. The exploratory checkout adds temporary component features and
standalone diagnostic binaries; these are excluded from the production patch.

## Why the original all-table candidate failed

The original optional tables passed dense pointwise mathematical-error bounds
but failed complete instrument output comparisons. In the 8-second
`long-tremolo-hold` case:

- Amplifier exponential+tanh tables only: peak residual **−156.54 dBFS**;
  RMS residual **−199.51 dB relative to reference**.
- BJT exponential table only: peak residual **+13.08 dBFS**;
  RMS residual **−1.59 dB relative**.
- CdS resistance table only: peak residual **+13.27 dBFS**;
  RMS residual **−1.57 dB relative**.

Evidence: `/private/tmp/lut-diag-amp`, `lut-diag-bjt`, and `lut-diag-tremolo`.
Each contains the exact source/provenance, paired output, and numerical report.

Increasing CdS table resolution from 2048 to 32768 segments did not rescue it:
peak instrument residual remained **+12.94 dBFS**. Evidence:
`/private/tmp/lut-diag-tremolo-32k`. This rejected run is retained rather than
silently replaced by the narrower candidate.

Canonical source audit found no independently fitted LDR constants or derivative
wiring error. Tables are generated from native formulas. Newton differentiates
the actual interpolation polynomial. Increasing nominal precision therefore
cannot be assumed to make the stateful circuit output equivalent.

## Sensitivity diagnosis

A standalone preamp comparison at 88.2 kHz, 440 Hz sine input, 8 seconds, and
full tremolo with the fork's response multiplier 0.5, used identical inputs and
native versus table-driven CdS resistance:

- At 0.05 V input amplitude, maximum preamp difference was `6.93e-9 V`.
- At 0.5 V amplitude, maximum preamp difference was `9.64e-9 V`.
- At 5 V amplitude, maximum difference grew to `499.28 V`; the native preamp
  itself reached `490.94 V`, and the candidate reached `491.95 V`.
- Maximum CdS shunt difference was only `4.18e-8 Ω`.

The final point is a deliberately overdriven diagnostic. These voltages are
nonphysical for the model's 14.5 V supply, and occur in the **native** solver as
well. It exposes pre-existing large-signal solver trajectory sensitivity; it is
not evidence that a table should be allowed to change ordinary instrument
output. Isolated component comparisons localize the failed candidate to changes
before/within the preamp. The exact instability boundary for every musical
input was not mapped. No solver iteration limit, physical model, or output clamp
was changed to make the approximation pass.

An earlier hypothesis that the behavioral amplifier was itself sensitive to
minute input perturbations was disproved by a direct sweep. Across 1,000,000
inputs spanning ±10 V, perturbations of `1e-6`, `1e-8`, `1e-10` and `1e-12 V`
produced maximum output differences of about `3.13648` times the perturbation,
with no jumps exceeding 0.1. The diagnosis therefore does not attribute the
failure to a demonstrated amplifier discontinuity.

Diagnostic sources retained under
`/private/tmp/openwurli-lut-diagnosis/tools/cpu-ab/src/bin/`:
`circuit-diag.rs` and `amp-sensitivity.rs`.

## Narrow candidate

The production patch routes only the behavioral amplifier's `tanh` through a
2048-segment cubic Hermite table over `[-12,12)`, with native fallback outside.
The table has 64 KiB of coefficients. Native exponential, BJT kernel, CdS
resistance and complete analytical amplifier forward path remain in source and
callable. Rejected table generators remain recoverable in commit `81c0916`.

The initial tanh-only implementation passed all 70 quick audio cases:
worst peak residual **−144.4944 dBFS**, worst RMS-relative residual
**−178.8126 dB**. Its exploratory command used −100/−100 gate settings, but the
actual maxima also satisfy the project's original −120 dBFS peak / −100 dB
relative acceptance limits. Evidence: `/private/tmp/lut-diag-tanh-quick`.

Its CPU results did not justify enabling it. Across seven workloads and eleven
repetitions, the optimized/shipping time ratio normalized against the native
run worsened by **0.28–2.30%**. Evidence:
`/private/tmp/lut-diag-native-cpu` and `/private/tmp/lut-diag-tanh-cpu`.

A final scoped candidate caches the immutable table reference in `PowerAmp` at
construction, avoiding `OnceLock` access during each Newton evaluation. The
geometry, interpolation and physical equations are unchanged. Evidence:
`/private/tmp/lut-diag-tanh-cached-quick` and
`/private/tmp/lut-diag-tanh-cached-cpu`. The quick run uses the original
−120 dBFS peak / −100 dB relative gates. Final CPU results follow below.

Patch from controls integration HEAD:
`/private/tmp/openwurli-amp-tanh-cached.patch`.

A separate compact-table exploratory audio run completed before the final scope
was narrowed, at `/private/tmp/lut-diag-tanh-compact-quick`. It was not selected
or benchmarked and is not part of the final patch or any performance claim.

## Final cached-reference results

- hold-1-sr-44100-block-256: native 1036.02 ns/sample, cached tanh 1048.99 ns/sample; normalized time change +0.82%.
- hold-6-sr-44100-block-256: native 1206.12 ns/sample, cached tanh 1216.37 ns/sample; normalized time change +0.81%.
- hold-32-sr-44100-block-256: native 1540.95 ns/sample, cached tanh 1536.81 ns/sample; normalized time change +2.55%.
- hold-64-sr-44100-block-256: native 1913.57 ns/sample, cached tanh 1945.99 ns/sample; normalized time change +1.88%.
- tremolo-off-1-sr-44100-block-256: native 1024.49 ns/sample, cached tanh 1026.56 ns/sample; normalized time change +0.97%.
- tremolo-off-6-sr-44100-block-256: native 1190.42 ns/sample, cached tanh 1172.69 ns/sample; normalized time change +1.14%.
- tremolo-off-32-sr-44100-block-256: native 1512.24 ns/sample, cached tanh 1506.93 ns/sample; normalized time change +0.01%.

The 70-case audio run passed: worst peak residual -144.494398 dBFS; worst RMS-relative residual -178.812646 dB.

These results do not establish a repeatable performance improvement. Keep native math as the shipping default; retain the narrow table as an explicit comparison candidate only. No full 1542-case LUT qualification is claimed.

## Durable evidence archive

All `lut-diag-*` runs named above have their `report.json`, `provenance.json`,
and `working-tree.patch` archived under [records](records/) as
`<run>-<filename>.gz`. The temporary paths identify the original runs; they
are not the only retained copies. The [final scoped production patch](records/amp-tanh-cached.patch)
and [diagnostic-only source](records/diagnostic-source/) are retained separately,
because a Git working-tree diff does not contain untracked diagnostic files.
These diagnostic binaries are not part of the plugin build. The archive does
not include every temporary WAV; machine-readable output comparisons and
source provenance are retained.
