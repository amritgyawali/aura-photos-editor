# ADR-0080: Adaptive portrait planning and trustworthy manual history

Accepted, 2026-09-30.

Extend the local YuNet portrait path with bounded quarter-turn fallbacks only
when the upright pass finds no faces. Transform boxes and landmarks back into
the original coordinate space; do not rotate the photograph or change identity.
Keep the pinned model unchanged and version the detection/preprocessing policy.
Fallbacks increase CPU cost on no-face images; inference stays serialized.

For each suitable face, measure candidate cheek/forehead patch luminance,
variation and normalized color. Select a representative low-variation sample
using consensus rather than simply choosing the smoothest patch. Reject extreme
clipping/darkness and color outliers. Use bounded measured strengths for texture,
tone uniformity and light balancing. This is an explainable heuristic refinement,
not semantic skin segmentation or a claim of competitor quality parity.
Persist per-face decisions, reasons and strengths in the existing optional recipe
report, with backward-compatible defaults. Display details using native disclosure
controls. Existing operation IDs, manual protection, reversible history and the
automatic-retouch kill switch remain authoritative.

The capability audit found that reset ignored removed extension fields. Compare
the union of before/after recipe paths when recording changes, so retouch-only
reset and snapshot restoration produce real history entries. A merge explicitly
authored by the user must carry user provenance instead of inheriting the prior
AI pass's source. This amends provenance handling without changing frozen types.
Originals remain read-only. Validate rotation mapping, patch adaptation, reset
removal, provenance, real portrait edits and exact Undo/Redo/export behavior.
