# AURA-ML-5144 - One project's curation pass could not run

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The Curate panel says AURA could not work out an album, a portfolio or a set of posts. The gallery
and every edit in it are exactly as they were: this phase writes nothing to a recipe and nothing to
disk, so a failed curation pass leaves no partial state anywhere except its own tables.

## What actually happened

`aura_curate::api::Curate::run` returned an error. The likely causes, in the order they are worth
checking:

* **Nothing has been selected.** Curation's input is the gallery phase 12 produced. A project with
  no `SelectionResult` produces `CurationResult::empty()` rather than this error, so if you are
  seeing this code the cull *did* run - but check `curate_status.selected` anyway, because a cull
  that selected two frames is not a wedding an album can be composed from.
* **The catalog is locked or unreadable.** Every other symptom is a `AURA-DB-3xxx` code; this one
  wraps them when the failure happened inside the pass rather than inside a single read.
* **A partial write was rolled back.** The pass writes its whole result in one transaction, so a
  failure leaves the previous curation in place rather than half of a new one.

## What to do

1. Read the detail on the error. It names the stage - `bw`, `hero`, `album`, `social`, `teaser` or
   `store` - that failed.
2. Re-run the pass. It is idempotent: it replaces the previous result wholesale, except for rows a
   photographer owns.
3. If it fails at `store`, run `aura-cli info --catalog FILE` and check the disk has room. Migration
   29's tables are small - a few hundred bytes per selected photograph - but a full disk fails a
   transaction like any other write.

## What is preserved across a failure and a re-run

Everything a photographer set. `curate_override` and the stored album order are not touched by the
pass, and the `curate_album_no_reorder` trigger refuses a pass that would overwrite an order
somebody set by hand. A re-run reports what it *would* have done instead.

## Related

* `docs/adr/ADR-0059-curation-selection-and-album-composition.md`
* `docs/curation.md`
