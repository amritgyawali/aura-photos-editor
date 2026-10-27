# ADR-0127: Selected material and light effects

Date: 2026-10-19

Status: Implemented; validation in the accompanying material-and-light report.

## Context

The Evoto inventory identifies gaps in clothing moiré, stain correction,
background blur, shadow balancing and glow. Existing skin healing and backdrop
smoothing are not dedicated implementations of these effects.

## Decision

Add five opt-in native retouch operations to the existing recipe extension:
`fabric_moire`, `fabric_stain`, `shadow_balance`, `background_blur` and
`highlight_glow`. These are manual, selection-based tools with no learned
detection claim. They use existing brush, ellipse, gradient and saved matte
support, strength, history and export. No frozen contract or database migration
changes. Older binaries cannot read recipes containing these new enum variants;
remove the new operations with this version before reverting the binary.

Colour implementation: work in scene-linear Rec.2020. Moiré correction filters
chroma while preserving original luminance. Stain correction measures a chosen
clean fabric sample and changes low-frequency channel ratios, retaining the
local fine detail. Shadow balance adjusts a smooth luminance field toward the
selection's measured upper midtones with a user-controlled stop limit. Glow
spreads only highlight energy. Background blur filters selected pixels only.
These effects do not infer missing texture, facial anatomy or depth.

Use normalized convolution over selected support, keeping excluded subjects out
of background averages. Process spatial filters in tiles with a halo covering
all three box passes. Retain an immutable copy of the selected source rectangle
so tiles cannot read already-edited pixels. Scratch storage is bounded by tile
plus halo, and the source copy scales with selection area. Neutral/disabled
operations and zero-coverage pixels remain unchanged. Output is deterministic;
only explicitly authored operations run. Existing automatic portrait policy stays
unchanged. No cloud service, new model, external dependency or original-file write.

## Validation and limits

Test measurable chroma suppression, luminance/texture preservation, selection
isolation, highlight gating, bounded shadow gain, seams, strength monotonicity,
recipe round trips and native history/export integration. Synthetic measurements
do not certify real garment reconstruction or competitor quality. Real-image
review and a complete feature ledger must distinguish manual tools from learned
and generative capabilities. The existing 143-route inventory remains the basis
for tracking the larger request, not a claim that all features are complete.

## Rollback

Remove these operations from affected recipes and rebuild the prior version.
Original files and unrelated saved operations require no restoration.
