# ADR-0121: Measured colour, skin texture and recoverable run monitoring

Date: 2026-10-14. Status: accepted.

AURA already limits a vision model's exposure proposal to the photograph's measured exposure.
White balance, tint and colour were still accepted across the provider schema's entire range.
Skin sampling also treated a smooth lighting gradient as texture. The two batch panels polled
the same worker independently, and a failed shell status read discarded its cancellation handle.

AI colour now follows the same measurement-first policy as exposure. Measure the original
with neutral white balance, then let the model refine temperature by at most 8 mired, tint by
3 units, and vibrance by 5 units around that measurement. Absolute saturation is limited to
5 units either way. These are conservative product limits, not calibrated quality thresholds.
Every limited proposal adds its requested and accepted values to the existing reasons. Manual
protection still runs through the existing recipe merge. A fresh original-based measurement
keeps cached/repeated proposals from accumulating a white-balance drift.

For each 5-by-5 skin sample, fit a centred least-squares lighting plane and measure texture
from the residual range. Preserve the original mean, chroma and raw-range contamination gate.
This prevents linear shading from increasing smoothing strength while retaining actual fine
texture. It does not classify nonlinear shading, train a new model or establish photographic
preference. Automatic operations keep their stable IDs and remain individually reversible.

The shell now owns the single status poll. The finish panel observes the shared store, including
when reopened. Waiting for a new handle does not restart monitoring of the previous terminal
run. A failed read retains the last status, busy lock and native cancellation handle.
Retry after 1.6 and 3.2 seconds; after three failed reads, pause monitoring with an explicit
Retry status control. Only an observed terminal status releases the run's busy lock. This is
monitoring recovery; native jobs are not made durable across application-process restarts.

No frozen contract, model, catalogue schema or original photograph changes. Regression checks
cover shading across three brightness levels, real texture, contaminated samples, guarded colour
proposals, single-flight polling, temporary disconnects, cancellation, and late replies from an
old run. Competitive superiority still requires representative-photo and photographer evaluation.

Native GUI qualification with two reference photos: three injected status failures retained the
handle and Stop control; Retry status reconnected to a worker still editing. Both full-size
exports verified and independently reopened, with unchanged input hashes and no page errors.
`scripts/test-run-recovery-gui.py` reproduces this check in an isolated test window.
