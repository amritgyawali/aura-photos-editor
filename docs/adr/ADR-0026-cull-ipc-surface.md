# ADR-0026 - The culling IPC surface

**Status:** accepted  
**Date:** 2026-08-16  
**Phase:** 12 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing  
**Supersedes:** nothing. **Amends:** nothing.

---

## 1. Context

Eleven phases have added read surfaces. This is the first that adds a surface people will
*act* on, and the actions are the ones that decide what a couple receives.

That changes what the boundary has to prevent. Earlier surfaces had to stop a web view
reinterpreting a measurement as a verdict. This one has to stop three things: a view that
can delete, a view that decides for itself what a guarantee means, and a view that renders
"nobody has decided" as "deliver nothing".

## 2. Commands

| Command | Kind | Purpose |
|---|---|---|
| `cull_status` | read | Coverage of the analysis, guarantee counts, override counts, mode and versions. |
| `gallery` | read | One complete selection: keepers, rejections, coverage report. `null` when never culled. |
| `image_decision` | read | What was decided about one photograph, in either direction, with reasons. |
| `cull_project` | work | Run or re-run the cull. Returns a typed pass report. |
| `resize_gallery` | write | Move the size slider. |
| `set_cull_mode` | write | Switch autonomy mode. |
| `override_decision` | write | Keep, remove, or withdraw an earlier choice. |

There is deliberately **no** delete, move, trash, export, upload, album, hero or
gallery-order command. Phase 14 edits the survivors, phase 27 swaps in runner-ups, phase 29
curates and phase 30 delivers.

## 3. Decision: `null` is "nobody has decided"

`gallery` returns `null` when the project has never been culled. It never returns an empty
selection to mean the same thing, and `CullView` renders the two states with different
copy.

The failure this prevents is specific and expensive: phase 30 will one day call this to
find out what to upload. A `null` read as "deliver nothing" would upload nothing and report
success.

## 4. Decision: the backend sends its own meanings

`satisfied`, `protected`, `keep`, `veto`, `vetoed` and `wasPeak` are booleans the engine
computed. The interface renders them; it does not recompute them from slugs.

This matters more here than on the composition surface. A web view that decided for itself
that `covered_weak` was close enough to `covered` would be a web view that could tell a
photographer their gallery was complete when the engine had said it was not - and the
photographer would find out from their client.

For the same reason, the three coverage states are rendered as **words** with an
explanation, never as a colour alone. The difference between "weakly covered" and "missing"
is the difference between a photograph worth a second look and a part of the wedding that
does not exist.

## 5. Decision: three write commands, all of which re-run the whole selection

`resize_gallery`, `set_cull_mode` and `override_decision` all go through one path:
re-gather, re-run the six passes, re-store. There is no cheaper path that only flips a row.

Two reasons. A forced keep changes a chapter's remaining quota and a forced reject can
leave a guarantee short, so a partial update would produce a gallery whose numbers no
longer added up and whose coverage panel was quietly wrong. And one path means the coverage
guard runs after all three by construction, rather than by three separate remembered
checks.

Section 6.4's "the slider re-runs only the allocation passes (not analysis)" is satisfied:
the analysis is phases 06 to 11 and none of it runs. Section 11 budgets two seconds for the
six passes and `crates/aura-perf/tests/cull_budgets.rs` asserts it.

## 6. Decision: `action` is three-valued, not a boolean

`override_decision` takes `keep`, `reject` or `clear`. "I have no opinion" is a distinct
statement from "I want this out", and a nullable boolean on a wire is how the two get
conflated by the third caller.

## 7. Decision: the hash crosses as hex

`deterministicHash` is a string. JavaScript cannot hold a 64-bit integer exactly, and a
support case that quoted a rounded hash would be a support case about a run that never
happened.

## 8. Decision: a rejection's sub-scores do not cross

The four numbers under a rejected frame stay in the catalog. Phase 13's ledger will show
them; the rejection drawer leads with the sentence instead.

A drawer that opened with "technical 0.31" would start an argument about a number rather
than about a photograph, and every reason the engine produces already names something
visible. Section 12's fourth failure mode is users distrusting automation, and the
mitigation named there is a reason plus a runner-up plus a one-click override - all three of
which this surface carries.

## 9. Consequences

* Seven commands, all off the renderer thread, because four of them re-run six passes over
  a whole wedding.
* The web view keeps no threshold, no flag semantics and no coverage vocabulary of its own.
* An unanalysed photograph offers no override control at all: overriding a decision nobody
  made is not something a photographer should be invited to do, and `AURA-ML-5050` is the
  code that explains why the frame is absent.
