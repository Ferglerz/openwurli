# Archived diagnostic source — not production code

These two standalone investigation binaries were used in the isolated LUT diagnosis.
They are retained as evidence, are not Cargo targets in this location, and are not
linked into the plugin. They do not duplicate an engine implementation.

- `circuit-diag.rs`: native/table-driven preamp comparison at stated drive levels.
- `amp-sensitivity.rs`: native amplifier response to small input perturbations.

See `../../lut-diagnosis.md` and the adjacent compressed per-run source patches,
provenance and reports for the exact investigation environment and limitations.
