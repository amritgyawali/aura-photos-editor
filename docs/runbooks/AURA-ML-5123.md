# AURA-ML-5123 - One wedding's consistency pass could not run

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The Consistency panel says the wedding could not be matched together. Every photograph still has
the edit phases 15 and 16 gave it on its own; nothing is lost and nothing is wrong with any
individual frame.

## What actually happened

`ConsistencyPass::run` failed part-way, or was cancelled. **Nothing was written**: the pass writes
its whole tree in one transaction, so a run that did not finish leaves the catalog exactly as it
found it.

That is different from every phase between 06 and 24, whose passes are per-photograph and resume
per row. Here a delta is a statement about a *node*, and a node whose first half was solved against
one target and whose second half was solved against another has a target that describes neither -
so there is no partial state worth keeping. ADR-0051 section 2.

## Recovery is `retry`, and the fallback is not silent

There is no fallback. Without this pass every frame keeps exactly the per-frame answer phases 15
and 16 gave it, which is the state the product was in before this phase existed: a worse gallery,
not a broken one. What must never happen is a panel reporting that as a *matched* gallery, which is
why `GalleryOutline::coverage` is zero rather than absent when nothing ran.

## What to check

1. `SELECT COUNT(*) FROM gallery_node WHERE project_id = ?` - zero means nothing was written, which
   is the expected state after this error.
2. The detail line names the step: "cancelled while building the tree" and "cancelled while
   solving" are a photographer closing the panel, not a defect.
3. Whether phases 07 and 15 have run. A project with no segments produces no nodes and a project
   with no tone estimates produces nodes whose frames all carry `tone_estimate_absent`. Neither
   raises this error, and both look like a pass that did nothing.

## Re-running

Safe at any time and idempotent: the solver reads phases 15 and 16, never its own output, so a
second run over an unchanged project writes the same numbers. Pins, rejections, overrides and
per-frame switches are read out before the tree is cleared and put back afterwards.
