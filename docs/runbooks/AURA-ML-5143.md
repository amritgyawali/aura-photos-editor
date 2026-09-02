# AURA-ML-5143 - A curation choice could not be recorded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A message saying AURA could not record that choice, and an unchanged panel. Nothing about any
photograph has changed, because nothing in phase 29 changes a photograph at all.

## What actually happened

One of the surface's three refusals fired. All three are deliberate and none of them is anything a
photographer did wrong.

**An album order that reorders chapters.** `CurateService::set_order` compares the submitted order
against the album's own `chapter_map` and refuses one whose chapters do not appear in wedding order.
A wedding album whose ceremony follows its reception is not an album with an unusual sequence; it is
an album that is wrong, and the optimiser, the drag handler and the cloud validator are all held to
the same rule. Reordering *within* a chapter is always allowed.

**An order or a pick naming an image that is not in this project's gallery.** Curation's subject is
the gallery phase 12 selected. An id from another project, or one phase 12 rejected, has nowhere to
go.

**A note above `MAX_NOTE`.** Five hundred bytes.

`ExportFormat::parse` and `ExportSubject::parse` also raise this code when a caller asks for a
format or a subject that does not exist, which in practice means a shell and a build that disagree.

## What to do

1. If it fired on a reorder: the chapter order is fixed. Move the frame inside its own part of the
   day, or change which frames are in the album.
2. If it fired on a decision: reopen the panel. The pick list is re-fetched and a stale card - one
   whose frame left the gallery because the cull was re-run - disappears.
3. If it fired on an export: check the shell and the core are the same build.

## What not to do

Do not write the row by hand. `curate_override` has a foreign key onto `photo` and a trigger that
refuses a kind outside `PickKind::ALL`; a row inserted around them is a decision the panel will not
show and the learning loop in phase 30 will read.

## Related

* `docs/adr/ADR-0060-curate-ipc-surface.md` sections 4 and 6
* `docs/curation.md`
