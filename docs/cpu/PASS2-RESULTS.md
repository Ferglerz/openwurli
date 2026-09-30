# Second CPU pass — qualification in progress

This is a working evidence record, not an acceptance or speedup claim. The
reference is the installed optimized controls engine at
`fd4f603dd8deef7a57d4449826f2c9d5dd53757e`. Every comparison selects the same
circuit model on both sides. Earlier scalar/SIMD gains do not describe Heavy.
The sound-model basis remains upstream v0.7; later upstream sound changes are
outside this pass.

## Candidate decisions so far

### Reuse final Fast-preamp transistor currents

Release LLVM and ARM64 inspection confirms that the old solver evaluated the
transistors again after its final convergence check. The candidate reuses
currents only when the corresponding voltages did not change. It recalculates
after the final permitted voltage update, preserving iteration limits and
failure behavior. Focused exact tests passed. Full audio and CPU qualification
are pending. The original native transistor function remains the source of truth.

### Skip unused LDR mapping at zero tremolo depth

The oscillator and CdS envelope still advance at their original rate. At exactly
zero depth the resistance cannot change the returned divider impedance, so the
candidate skips only the power-law mapping. A zero-to-positive depth setter
refreshes that mapping before an immediate query. Native equations remain
unchanged. Focused exact tests passed; broader qualification is pending.

### Share Heavy-preamp coefficient rebuilding

The main signal and silent shadow solvers build identical coefficient matrices
when their conductance, capacitance, sample rate and preserved inverse values
match bit for bit. The candidate builds once and copies exactly the coefficient
write set. Signal histories, voltage estimates, noise RNG, chord factorization
and all nonlinear solver state remain independent. Mismatched configurations
fall back to the original two rebuilds. Generated solver source is unchanged.
The prior inverse comparison also preserves failed-inversion fallback behavior.
A focused main/shadow-state test and the 87-case Heavy engine screen were
exact: 4,368,000 output samples per pair, no differing bits, and matching
available amplifier diagnostic trajectories. Full qualification is pending.

### Damper recurrence: rejected for both modes

The recurrence uses native formula values to seed its multipliers and reanchors
every 32 samples. An isolated reed proof showed extremely small errors, but the
87-case Fast engine screen failed in 12 cases. Ten fail only relative limits
in near-silent tails, with whole-render residual peaks no greater than about
−138 dBFS. Two six-note cases diverge substantially through the nonlinear
circuit: pedal bursts have +13.37 dBFS residual peak and −19.97 dB relative RMS;
a long release has +13.35 dBFS peak and −11.28 dB relative RMS. These positive
peak levels are raw floating-point engine levels before any host clipping.

This demonstrates why reed-level precision cannot stand in for full-engine
qualification. The approximation remains disabled. All three representative Heavy six-note
cases also fail: repeated chords (−30.71 dBFS peak / −21.55 dB relative RMS),
pedal bursts (−0.112 / −20.80 dB), and a long release (−2.52 / −25.63 dB).
Available Heavy counters showed no additional observed clamp or iteration-limit
events, but internal peak trajectories differed. Bounded spectral evidence is
recorded separately. No lower accuracy
threshold, gain matching or time alignment is applied to rescue this candidate.

### Full behavioral-amplifier table: entry gate not met

Replay of real amplifier inputs from the unchanged Fast engine estimated stage
cost at 3.76–3.90% of render time across 1/6/12 held notes, below the required 10%.
Consequently no complete-transfer table was implemented. The profiler times an
unmodified engine and an independent native-amplifier replay; the instrumented
capture copy is never timed. This is a stage-cost estimate with replay overhead,
not an exact call-stack attribution. These preliminary measurements were not
three quiet acceptance runs. See `tools/cpu-amp-profile/README.md` for settings
and reproducibility. Heavy's amplifier is stateful and outside this experiment.

## Required evidence before release

Passing candidates must satisfy whole-render and overlapping 20 ms residual
RMS of at most −60 dB relative to the reference, residual peak at most −60 dBFS,
and exact silence for exactly silent reference windows. Exact candidates must
also be bit-identical. The complete matrix retains the previous 1,542 cases
and adds 97 musical/transition cases. CPU acceptance requires a reproducible
3% targeted median reduction in three quiet runs of eleven interleaved repeats,
without reproducible regressions above 2% elsewhere.

Available power-amp diagnostics are sampled outside timed CPU runs. Heavy exposes
real solver counters; Fast's existing diagnostic API returns zeros, so parity
there is not internal solver evidence. Snapshot comparisons do not establish
complete lifetime fault counts across resets. Focused internal-state tests and
these limitations must accompany the output comparison.

Runtime Fast/Heavy switching is a separate integration change, documented in
[PASS2-MODES.md](PASS2-MODES.md). It does not imply the two models null against
each other. No numerical pass is proof of universal inaudibility.

## Runtime integration findings before release

The first 30-case switch screen found stale released-chord tails on returning
from Fast to a paused Heavy chain. Six-, twelve- and 32-note damped chords
exceeded −60 dBFS at 44.1/48/96 kHz, with a worst observed return peak of
−46.07 dBFS. A 20 ms fade alone did not remove the recalled history. The revised
integration prepares the incoming circuit by streaming 100 ms of current input
while the old circuit remains audible, then applies the 20 ms crossfade. This
adds about 120 ms of requested-mode response and extends the time both circuits
run. It is pending the same transition evidence; fixed-mode sound is unchanged
by the preparation design.

The screen also observed existing Heavy recovery allocations, including one
immediately after activation. The adapter fix restores all generated state
fields from the original settled cache in place, reusing its boxed cold state.
Generated source and native initialization remain intact. Exhaustive field
matching makes future generated-state additions a compile error until reviewed.
Focused state-bit and post-reset output proofs passed on the portable branch;
complete scoped allocation and transition checks remain pending.
