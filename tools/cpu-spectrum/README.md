# Rejected damper candidate: bounded spectral follow-up

This offline analysis reads immutable `cpu-next` audio exports. It does not render
DSP, build Rust, or measure CPU. NumPy and Pillow are required; no dependency
installation is part of the tool.

```sh
python3 tools/cpu-spectrum/analyze.py /path/to/cpu-next-run --output /path/to/empty-output
```

The input must include `report.json`, `provenance.json`, and both raw little-endian
float32 waveform files for each failing audio case. The output includes
`analysis.json`, `residual-analysis.png`, and the exact script used. Input files,
report, provenance, and script are SHA-256 identified. The output directory must
be empty so an earlier analysis is not silently overwritten.

## Measurements

- Residual is candidate minus reference after exact conversion of float32 samples
  to float64. There is no alignment, gain matching, DC removal, filtering, or
  clipping. Raw output amplitude 1 corresponds to 0 dBFS; values above 1 remain
  unchanged.
- All failing cases have whole-render and sliding-window metrics. Windows are
  20 ms with a 10 ms hop, including final partial windows. Recomputed failing
  window counts must match the input report.
- Six-note repeated chords, pedal bursts, and long releases receive detailed
  attack, release, peak-residual, and final-tail analysis. The long-release-tail
  case is included when it fails, to distinguish relative errors near silence.
- Numeric frequency bands use a rectangular one-sided DFT and Parseval power
  accounting, including DC. Summed band power equals residual mean-square power.
  Spectral leakage is possible; this is not harmonic or alias identification.
- The static figure shows a 30 ms waveform neighborhood and a 20 ms Hann FFT
  centered on the largest residual. FFT amplitude is corrected for Hann coherent
  gain. Logarithmic display bins retain their maximum, and the display floor is
  −160 dBFS. The current plot axis is intended for the selected 48 kHz cases.

These are numerical rejection diagnostics. They do not constitute listening
tests or claims about inaudibility. A relative gate failure near silence and a
large absolute transient error require different interpretations.

## Recorded runs

Fast analysis: `fast-damper-spectrum/final/` in the evidence archive. Of 87 cases,
12 fail the window-inclusive gate; two also fail whole-render gates with peak
residuals above +13 dBFS. The other ten have whole-render residual peaks no higher
than −138.47 dBFS and fail relative gates in near-silent tails.

Heavy analysis: `heavy-damper-spectrum/` in the evidence archive. All three
screened six-note cases fail whole-render gates, with residual peaks from
−30.71 to −0.112 dBFS. Initial 60 ms attacks are exact in the selected cases;
later release/retrigger behavior diverges. The candidate remains rejected for
both engines; these analyses do not justify further CPU qualification.
