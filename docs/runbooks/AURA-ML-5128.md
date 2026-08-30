# AURA-ML-5128 - A person's gallery skin target could not be built

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

That person's skin was left exactly as it was in every photograph. Nothing about them was matched
across the wedding.

## What actually happened

Fewer than five well-lit frames of that person contributed a usable skin reading, so no target was
built and `GalleryCode::SkinTargetAbsent` was recorded on every frame of theirs.

A reading contributes only when all three hold: its luminance is between 2 % and 75 %, its mask
quality is at or above 0.60, and the frame's light is not one phase 15 calls intentional.

## Why five, and why nothing rather than a weak target

Phase 15's argument for `MIN_LOCUS_SAMPLES`, unchanged. A target fitted to two frames is a target
fitted to **one lighting condition**, and correcting a person's skin across a whole wedding toward
it would move every frame of them toward how they looked in one room.

A weak target is worse than none because it looks like evidence. `SkinTarget::is_usable` is false
below five and nothing downstream consults it.

## This build always raises it

`SKIN_FIELD_AVAILABLE` is false. Phase 18's segmentation head is untrained, so no photograph has an
identity-scoped skin region and no reading exists to accumulate. Every frame therefore records
`GalleryCode::SkinMaskAbsent` - **not** this code, and the difference is the point:

* `SkinMaskAbsent` says the product could not look.
* `SkinTargetAbsent` says it looked and this person was not in enough well-lit frames.

Phase 24's rule. They are separate codes, separate rows and separate runbooks, and only the second
is a statement about a person. Exit report condition C2.

## What to check

    SELECT identity_id, frames, spread_before, spread_after FROM gallery_skin_target
     WHERE project_id = '<project>';

An identity missing from that table has no target. The promise section 6.3 makes - a person's skin
dE00 spread at or below 2.0 across the gallery - is `SELECT MAX(spread_after)` over the rows that
are there, and it says nothing about the people who are not.
