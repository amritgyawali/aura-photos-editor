# ADR-0128: Selected optical and colour tools

Date: 2026-10-19

Status: Implemented and verified

## Decision

Add explicit native `motion_blur`, `zoom_blur`, `color_despill` and
`highlight_compress` variants to the existing retouch extension. They use the
standard authoring, mask, preview, history and delivery paths. No schema migration,
frozen contract change, external model or dependency is required.

Motion blur uses `radius` for travel as a fraction of the short edge and `warmth`
for signed angle divided by 180 degrees. Zoom blur uses `radius * 10` as fractional
radial travel and optional `source` as the focus centre (default image centre).
Both integrate a line kernel over immutable, premultiplied selected pixels.
A bounded 65-tap quadrature samples a coverage-weighted image pyramid at a level
chosen from sample spacing. This prefilter prevents long trails from degenerating
into sparse copies. Excluded pixels contribute neither colour nor weight.
Rows run independently; sample/reduction order is fixed. Whole selection storage
and its pyramid scale with selected area, not blur radius.

Colour despill projects chroma onto a chosen display-sRGB spill colour after
converting it to linear Rec.2020. It reduces only positive, hue-aligned chroma,
keeps luminance and guards nonnegative source channels. `tone` controls hue
selectivity. Near-neutral target colours are rejected: they specify no hue.
Highlight compression uses a continuous shoulder above `0.18 * 2^(tone * 4)`;
`texture / 2` controls compression, and channel ratios are retained. It compresses
existing light; it cannot reconstruct clipped detail.

All tools are manual opt-ins and receive no automatic portrait stage assignment.
Mask reuse must not overwrite a zoom centre. Presets retain settings but omit
image-specific centres and masks, following existing preset semantics. Tool
switching chooses explicit new defaults. Existing tool behavior is unchanged.

## Verification

Measure directionality, focus-centre behavior, long-trail continuity, mask
isolation, neutral inputs, exact zero/disabled identity, colour/luminance
preservation, monotonic highlight shoulder, recipe validation and round trips.
Exercise save/update, undo/redo, delivery and original preservation, plus real
desktop authoring. Performance and photographic quality across all hardware and
subjects remain separate acceptance work.

Results: 475 recipe/render tests passed (12 ignored), 818 UI tests passed, and
84 focused editor tests passed after the centre-picker refinement. Both native
integration workflows passed. The rebuilt desktop passed eight actual-window
checks, including each tool's native pixel output, invalid authoring inputs,
Undo/Redo and source-byte preservation. See the
[validation report](../reports/AURA-optical-and-colour-tools-2026-10-19.md).

## Rollback

Remove these four new operation types using the new binary before reopening
affected recipes in an older binary. Originals require no restoration.
