# AURA-ML-5142 - Stored curation notes came from a build with a different reason vocabulary

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The curation panels draw, and one or more picks carry a note AURA cannot render. The banner offers
to re-run curation.

## What actually happened

A row in `curate_bw_reason`, `curate_hero_reason`, `album_spread_reason` or `social_pick_reason`
carries a slug that `CurateCode::parse` does not know. The only way that happens is a catalog
written by a newer release: the vocabulary is a closed enum compiled into the build, and every
writer in the product goes through `CurateCode::as_str`.

This is **degraded**, not an item failure. Curation is a proposal, and the honest response to a note
this build cannot read is to re-derive the proposal rather than to refuse to draw the panel - the
album, the heroes and the sets are all still there and all still correct.

## What to do

1. Re-run curation for the project (`curate_project`, or the Curate panel's own button). The pass
   rewrites every reason row from this build's own vocabulary.
2. If it recurs on a freshly curated project, the catalog was opened by a newer AURA and downgraded.
   `aura-cli info --catalog FILE` reports the schema version; a catalog above this build's
   `APP_SCHEMA_VERSION` is refused at open with `AURA-DB-3004` and this code should be unreachable.

## What not to do

Do not delete the reason rows. A pick with no reason is a pick that violates invariant 2, and
migration 29's `curate_hero_reason_count` and `curate_bw_reason_count` triggers refuse one anyway.

## Related

* `docs/adr/ADR-0059-curation-selection-and-album-composition.md`
* `docs/curation.md`
