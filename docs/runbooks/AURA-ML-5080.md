# AURA-ML-5080 - A mask payload exceeded its storage budget and was stored more coarsely

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing, unless they look: the mask is present and usable, and the panel shows a small note on
that region saying it is stored at reduced resolution. At 100 % zoom its boundary is one
analysis pixel coarser than the others.

## What actually happened

`mask::store::encode` produced a run-length payload longer than `RLE_MAX_BYTES`, halved the
bitmap and re-encoded once. A run length costs *perimeter* rather than area, so this only
happens to a region that is genuinely broken up - a lace veil, a crowd behind a bokeh gate,
foliage through a window - where the perimeter is enormous relative to the area.

The alternative was to let one frame blow the 180 KB budget in section 11. That budget is a
guarantee rather than a target: at 4,000 frames the difference between meeting it and missing it
by a factor of five is the difference between 700 MB and 3.5 GB of catalog.

**The coarsening is recorded rather than silent.** A mask that quietly halved its own resolution
is a boundary that is suddenly a pixel wide at 100 % zoom for a reason nobody can find later.

## Operator steps

1. None are required. The mask works.
2. If this fires on most frames of a wedding, the class in question is being over-segmented -
   check `docs/masks.md` for which classes are stored as run lengths and whether the one firing
   should be an alpha class instead. That is an ADR-level change to `MaskKind::stored_as`.

## What would make this impossible

Storing every class as alpha. It was rejected in ADR-0037 decision 5: twenty quarter-resolution
planes is 1.3 MB per photograph and 1.3 GB for a thousand-image gallery, which is the failure
mode section 12 names.
