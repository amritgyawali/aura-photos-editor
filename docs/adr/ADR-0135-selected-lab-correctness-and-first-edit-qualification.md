# ADR-0135: Selected lab correctness and first-edit qualification

Date: 2026-10-21. Status: accepted.

## Context

The hundred-capability expansion in ADR-0134 already exists in the working tree.
A new correctness audit found that selected spatial filters could sample excluded
pixels, tied ranks could darken flat areas, and partial wavelet blocks duplicated
image edges. Review plans compared a target against another photo's ancestor,
and unedited photos could not receive their first revision-guarded batch edit.
The requested Evoto equivalence still lacks paired native exports and recovered
trained models. More tool names would not resolve these gaps.

## Decision

Preserve the 100 distinct additions and repair their behavior. Bilinear spatial
sampling renormalizes selected taps; flat ranks use tie midpoints; incomplete
Haar blocks remain unchanged. Reference coordinates consistently refer to pixel
centres. The render identity includes `labs.selected-v2` so previous cached
pixels cannot masquerade as results of the corrected kernels.

Three-way transfer checks each target's own checkpoint. Rollback binds to its
actual originating checkpoint. Relative tone transfer includes texture, clarity
and dehaze. Reopening an issue reopens dependent issues and clears affected
checklist passes. Selection skeletons include frame boundaries; chroma-key can
clear a fully matching selection; split regions honor their maximum area.

`RecipeStore::save_first` checks absence and writes recipe/history in one
transaction. All recipe saves recheck collection membership inside that same
transaction. Existing `save_if_current` continues to reject missing revisions;
the workflow explicitly chooses first-save semantics for genuinely unedited
targets. Extension saves rely on the existing user merge once, keeping history
paths unique and preserving manual protection.

Qualification requires a fresh catalogue/output folder, exact import identities,
successful individual edits, complete verified exports, unchanged source hashes
and no page errors. Results retain their executable identity. A successful native
call or a synthetic pixel change is reported with its scope; neither certifies
Evoto equivalence or photographic quality.

## Consequences

Old recipes remain valid. Older saved plans with no checkpoint binding can still
be read; preparing a fresh plan is required before rollback. Spatial work remains
bounded to its existing correction grid. Batch recipe application is still per
photo and reports partial conflicts, rather than claiming a collection-wide
transaction. Original files remain read-only. No proprietary code, network
weights, training set, or trained skin assets are imported from Evoto.
