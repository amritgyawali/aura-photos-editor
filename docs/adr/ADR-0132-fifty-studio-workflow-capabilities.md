# ADR-0132: Fifty studio workflow capabilities

Date: 2026-10-20. Extends ADR-0130 and ADR-0131.

The user requests fifty additional, distinct tools and working methods drawn
from Evoto. An inventory excludes existing AURA retouch tools, global curves,
HSL, B&W, grading, camera calibration and semantic selection buttons. The
implementation ledger counts complete operations, not every colour channel,
slider, reset button or preset as another feature.

Implement four coherent areas: validated native colour operators and local
colour controls; composition-aware selection editing; durable reference group
and batch management; and explicit framing and inspection workflows. Documented
Evoto counterparts and AURA extensions are labelled separately. Feature-category
documentation does not identify proprietary algorithms or prove identical pixels.
No competing editor's code, image assets, weights or tuning arrays are shipped.

Colour settings use a validated, default-neutral `studio_color_v1` recipe
extension. Local settings refer to existing mask IDs. They use the same native
renderer in previews and exports; a full-frame fallback avoids tile seams for
extended local operators. New settings invalidate render/checkpoint hashes.
Saving a colour draft checks the expected recipe hash and creates one manual
history step. Measurements propose drafts without saving them.

Selection operations evaluate the native composed mask, then freeze the result
in the existing bounded matte format. Whole-mask inversion complements the
composed plane; it does not invert each component independently. Refinements
preserve manual parameters and unrelated masks. Duplicate also copies the
mask's extended colour settings. Explicit replacement prunes obsolete mattes;
canonical no-ops add no history. Cancellation tokens are registered before
queueing the worker. Coverage rendering itself retains the existing synchronous
cancellation limitation, disclosed in the guide.

Reference management uses catalogue key/value data without a migration. Batch
actions and revision checks share a writer transaction. Bounded descriptor-only
interchange validates versions, engine, dimensions, ranges, identifiers and
collisions. It excludes images, thumbnails, point coordinates and source paths.
Reference changes never edit target-photo recipes or matching sessions. The
128-reference and 64-group bounds are explicit, including ordinary import and
capture admission.

Framing measurements return source dimensions, measured faces and available
subject silhouettes. Unavailable evidence is explained; measured eye alignment
requires both eyes. Silhouette-axis alignment is not skeletal pose inference.
Crop proposals remain drafts until one geometry save. Physical sizes calculate
pixels at a chosen PPI and use the existing verified export pipeline; they do
not certify ID-photo rules, embed printer-resolution metadata or upscale.
Preview inspection and colour-only comparison never save an edit.

Validation combines native pixel/history/original-preservation tests with
frontend interaction tests, strict lint/contracts and a rebuilt desktop smoke.
Synthetic fixtures qualify mechanics rather than portrait quality. Performance
on the three reference machines and paired Evoto photographic exports remain
unmeasured; this branch makes no full-parity or superiority claim. No frozen
contract, schema migration or new model dependency is introduced.
