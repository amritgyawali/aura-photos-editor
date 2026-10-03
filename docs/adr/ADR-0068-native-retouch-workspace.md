# ADR-0068 — Native retouch authoring

- Date: 2026-09-30
- Status: accepted

The user selected AURA's own local retouching tools, without a Retouch4me account. Retouch4me's documented functions are workflow references, not an available implementation or a claim of model parity. No external processing is introduced.

Store a versioned, typed `studio_retouch_v1` operation array in the recipe's existing extension map. Validate its shape and numeric bounds on both authoring and render entry. Ordinary recipe merge, hashes, manual protection, snapshots and history apply. Empty recipes retain their previous serialized form. No frozen contract is modified.

Operations use normalized coordinates on the full, oriented, lens-corrected photograph before crop/perspective. The Retouch workspace therefore previews that full photograph; final crops remain active in normal Develop and export. This keeps edits attached to the same content when the crop changes. User-selected feathered ellipses supply explicit targets without pretending to have an AI segmentation model. Source points support sampled repairs and color matching.

Implement deterministic linear-light healing/cloning, frequency controls, local luminance and chroma corrections, detail work and bounded spot detection inside the selected region. Shared primitives may serve several clearly named workflows; these are not independent learned models. Frequency texture changes are explicitly user-authored, extending the older automatic-retouch restriction on high-band modification. Automatic default presets preserve texture.

Run these operations in both interactive previews and full-resolution CPU output, before capture sharpening and final geometry. Whole-frame execution is required for donor sampling and spatial bands; streamed rendering falls back explicitly for these recipes, as it already does for rotations. This is a documented memory limitation, not a claim of tiled support.

Acceptance requires nonzero pixel effects, unchanged outside-mask pixels, neutral identity, source-boundary handling, saved-history round trips, crop anchoring and native export checks on real portraits. No claim of commercial retouch quality or a completed 100-feature suite is warranted by tool count alone.
