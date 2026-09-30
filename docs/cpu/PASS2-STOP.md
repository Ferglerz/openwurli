# Second pass stopped for a two-engine release

At the user’s request, further testing and CPU qualification stopped on
2026-09-29. The release includes saved Fast/Heavy selection, old-preset migration,
100 ms streamed incoming-circuit settling plus a 20 ms crossfade, and a Heavy
reset that reuses allocated solver storage. It retains the previous optimized
native equations. The new preamp-current, zero-depth LDR, and Heavy matrix-sharing
candidates are **not enabled or integrated into the release**; timing gates were
not completed. The recurrence failed both engines’ audio gates. The complete
behavioral amplifier table did not meet its profiling entry gate.

Completed research: 1,639 Fast cases / 94,521,540 samples matched bit for bit with
current-reuse + zero-depth candidates and the initial runtime architecture.
Heavy’s 87-case exact-candidate screen / 4,368,000 samples also matched, including
available amplifier diagnostic trajectories. These are study results, not a
full qualification of the final release source. The first switch architecture
failed released-tail checks; the later settling change passed focused scheduling
checks but did not receive a repeated 30-case transition study. The in-place
reset passed exhaustive state-bit and post-reset output tests. UI parameter,
old-preset migration, and initial-mode audio checks passed before final packaging.

Compressed raw reports and source hashes are in `pass2-evidence/`. They retain
pre/post definitions, strict gates and rejected results. Full raw listening
renders remain locally under `/private/tmp/openwurli-pass2-evidence/`; regenerate
from the preserved study source and recorded commands before removing those
local artifacts. The historical original switch proof tool is archived alongside
its reports because the current tool adds analysis for the later settling design.

No quiet three-run CPU acceptance study was completed, no new CPU speedup is
claimed, and no upstream PR was opened. Resume the exact candidates from the
archived study after validating final transitions and measuring quiet CPU runs.
