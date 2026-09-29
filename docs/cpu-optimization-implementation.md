# DSP CPU optimization implementation

The implementation preserves the upstream circuit equations and uses `f64`
throughout. The default path uses native transcendental functions. The optional
`experimental-circuit-lut` feature is a numerical candidate for paired
measurement, not an accepted claim of identical audio.

## Exact work

- Reed mode arrays hold adjacent `f64x2` pairs for modes 0–5; mode 6 stays scalar.
  Contributions are still accumulated in original mode order. Damper ramps,
  onset calculation, RNG draw order and quadrature renormalization are unchanged.
- The jitter-corrected rotation is cached until the next 16-sample jitter update.
  The original correction expression is evaluated when its operands change.
- Mix loops use paired slice iterators, including the existing steal fade.
- The tremolo depth divider's fixed terms update only when depth changes.
- The legacy preamp caches its Sherman–Morrison scale and corrected kernels
  until accepted LDR conductance changes. The existing `0.01 Ω` update deadband
  and history conductance are unchanged.

The architecture-dependent C4 checksum from the earlier patch is not retained.
Paired renders against the upstream base, on the same machine and toolchain,
are the evidence for exactness. No physical-model or gain-staging changes belong
in that comparison.

## Analytical functions remain the source of truth

`circuit_math::exp` and `circuit_math::tanh` evaluate native `f64` functions and
return native derivatives. `dk_preamp_legacy::bjt_analytical` and the behavioral
amplifier's `forward_path_analytical` remain callable regardless of the selected
feature. There is one physical-model implementation with a generic evaluator
for each mathematical function. Generic specialization avoids a function pointer
call for each sample and avoids maintaining a second copy of the circuit.

`tremolo::ldr_resistance_analytical` retains the original log-resistance formula
and accepts the instance's precomputed logarithms and gamma. The LDR table
builder calls `tremolo::ldr_law_sample`, which uses that same formula and the
module's physical constants. No independent copy of the component values or
fitted curve is embedded in the table module. Runtime native evaluation and
out-of-domain fallback call the retained analytical function.

Retain these native analytical paths permanently. They are the reference for
future table changes, sample-rate coverage and numerical investigations.
Generated benchmark worktrees can be deleted after evidence review; the
analytical source cannot.

## Optional tables

The candidate uses 2048-segment cubic Hermite interpolation, with native
fallback outside the following half-open intervals:

- Exponential: `[-40, 40)`.
- Hyperbolic tangent: `[-12, 12)`.
- Nominal CdS resistance: `[1/64, 1)` in envelope-drive units.

Keeping the LDR dark tail native avoids its singular power-law slope at zero
and preserves the original threshold branch. Four polynomial coefficients per
segment consume 192 KiB across all three tables. They are immutable and shared;
construction initializes them once, before sample processing. Runtime lookup
performs no allocation.

Newton derivatives are derivatives of the actual interpolated polynomial:
`p(t)=a+t*(b+t*(c+t*d))`, `p'(x)=(b+t*(2*c+t*3*d))/step`.
The BJT's charge and recombination chain rules, and the amplifier's crossover
and rail chain rules, use those returned derivatives. Using `1-tanh_table(x)^2`
would differentiate a different function; the implementation does not do that.

A targeted off-grid sweep of 100,000 positions observed maximum exponential
relative value error `6.06e-9`, exponential relative derivative error `4.83e-7`,
tanh absolute value error `2.01e-10`, tanh absolute derivative error `5.27e-8`,
and LDR relative error `6.40e-11`. These sampled bounds are not an exhaustive
proof or an audibility claim. Feedback and solver stopping thresholds must be
assessed using complete pre/post output and timing comparisons.

## Acceptance and exclusions

Native default changes require bitwise output comparison. Candidate tables
require residual amplitude, harmonic/alias, stability and repeated CPU results
before promotion. Function error alone cannot justify enabling a candidate.

Damper recurrence replacement and non-power-of-two reciprocal rewrites are not
part of the exact patch: they change rounding. Cross-voice packing and wavetable
instruments are separate designs. Generated circuit output, precision, solver
iteration limits, physical constants and parameter APIs are unchanged.

## Reproducible upstream comparison

`tools/cpu-ab` reconstructs the actual upstream v0.7 baseline (`3023a8a`) and
exact patch (`cab1a75`) from Git into ignored, disposable directories. Original
engine sources remain in Git and original analytical functions remain in the
production crate. Do not delete those functions when retiring generated baselines.

The archived `docs/cpu/records/upstream-native-quick-*` run compared 70 cases:
all rendered samples were bit-identical. `upstream-lut-quick-*` tested the first
three-table candidate against the same cases: worst peak residual -138.47 dBFS,
worst residual RMS -131.71 dB relative to reference. This is a narrow audio
screen, with no CPU timing or listening conclusion. The controls fork exposes
larger input ranges and **failed** the three-table candidate's full-engine
screen despite these upstream-only results. Consequently tables are disabled
by default. Passing isolated function error or a narrower parameter matrix is
insufficient to ship them.

This branch is based on the original instrument's v0.7 revision. Upstream v0.9
changes reed physics, pickup and amplifier behavior; porting this work to current
upstream is a separate sound-model migration. A future PR against current main
requires that port and renewed measurements.
