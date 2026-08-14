# ADR-0016 - The story IPC surface

**Status:** accepted
**Date:** 2026-08-14
**Phase:** 07 - Wedding Scene AI & Story Timeline Segmentation
**Deciders:** CTO, Senior Frontend Engineer, ML Lead - Vision, Product Manager

## Context

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are a frozen pair: both
are digested in `contracts.lock`, and changing one without the other fails CI. Phase 07
adds nine commands, thirteen types and one event enum to them.

This ADR records the four decisions that shaped what crosses the boundary. Unlike
ADR-0014, none of them is about privacy - a scene label is a fact about a photograph, not
about a person. They are about **who owns a decision**, which is the thing this surface
is most likely to get wrong: the timeline is the first screen in the product where a
photographer routinely disagrees with the automation and edits it directly.

## Decision

### 1. The outline is one command, and it carries its own coverage

`story_outline(projectId)` returns every chapter, the review list, the coverage and the
two version numbers, in one call.

Not one call per chapter, and not a paginated list. Section 11 budgets **200 ms** for the
timeline opening, a wedding has between six and twenty chapters by construction
(`changepoint::CHAPTER_BAND`), and `v_chapter_summary` aggregates them in one indexed
read. A surface that made the strip fetch chapters lazily would be optimising a list that
cannot exceed twenty entries.

`coverage` is on the DTO rather than behind a second command because of the rule phases 05
and 06 both wrote down: **report coverage when you report a result**. A story drawn over a
40 %-classified wedding is a story about 40 % of a wedding, and a panel that has to make a
second call to find that out is a panel that will render first and ask later.

### 2. Four edit commands, and every one of them locks

`set_chapter`, `move_chapter_boundary`, `split_chapter` and `merge_chapters`.

All four set `segments.user_locked`, and `move_chapter_boundary` sets it on **both**
chapters either side of the boundary. That is not symmetry for its own sake: a boundary is
shared, and locking only the earlier chapter would let the next re-analysis move it back
from the later one's side. Section 6.4 is explicit - "anything the user touches is
`user_locked` and re-analysis never overwrites it" - and "touches" includes the chapter on
the other end of the edit.

There is deliberately **no `unlock` command**. Undoing a lock means asking the automation
to overrule a decision the photographer made, and the honest way to ask for that is to
re-run the analysis on a chapter the photographer has explicitly reset - which is a
different, louder action than a toggle in a context menu. If the need turns out to be
real, it arrives with its own ADR.

### 3. A scene label crosses as text, and its reasons cross with it

`SceneDto` carries `scene`, `sceneConf`, the padded three-entry `top3`, the attribute
*names* rather than the bitfield, and `reasons`.

Three choices there, and each one is the same choice made three times: **the wire carries
what a person needs to read, not what the database happens to store.**

* `attributes: string[]` rather than `attrs: number`. A UI that had to know that bit 7 is
  `stage` is a UI that will be wrong the first time an attribute is added, and the bit
  layout is an implementation detail of `AttrFlags` rather than a contract with a screen.
* `attributesMeasured: boolean` beside it, because an empty array means "outdoors, no
  flash, daylight, nobody around" - a description - and "the head did not run" is not,
  and a panel that showed both as "no attributes" would be lying.
* `reasons` on `ChapterDto` are stored text, not re-derived. They are a *record of what
  the automation concluded at the time*, unlike `FaceBoxDto.reasons` in ADR-0014 which are
  re-derived from stored measurements. The difference is that a face's quality verdict is
  a pure function of numbers still in the row, and a chapter's reasoning cites a penalty
  and a merge pass that a later re-analysis will have replaced.

### 4. The profile registry is readable, with its rationale

`scene_profiles(projectId)` returns every scene's tolerances **and the sentence explaining
them**.

This is the least obvious command in the surface and the one section 12 asks for most
directly. Its third failure mode is that `scene_profiles.toml` becomes a dumping ground of
magic numbers, and the counter-measure is that every value has a written rationale and an
owner. A rationale that only exists in a TOML file on the developer's machine is a
rationale nobody reads; on the wire it is an answer to "why is my dance floor being judged
this way", which is a question photographers ask and invariant 2 says has to have an
answer.

The command is read-only in phase 07. Writing a per-project override is a settings surface
that belongs with the phase that acts on the numbers - phase 12 for culling, 15-17 for
grading - and shipping a writer before there is a visible effect would be a control with
no feedback.

## Consequences

* Nine commands, thirteen DTOs, one event enum. `contracts.lock` is re-locked for both
  files.
* `StoryEvent` is typed on both sides and **not yet emitted**, for the same reason
  `IngestEvent`, `InferEvent`, `CloudEvent`, `IndexEvent` and `PeopleEvent` are not: the
  Tauri shell has not been launched on the development machine, so an emitter would be
  code nobody has run. Section 11's four telemetry events are emitted as `tracing` spans
  today and the enum is their wire shape for when the shell runs.
* The Explain panel gains a scene section. Acceptance criterion 2 - "every image carries a
  scene label, attributes and confidence, visible in the Explain panel" - is met by
  `image_scene(photoId)` plus the existing panel.
* No command in this surface can change a photograph. Four of them change a *chapter*, one
  changes a *label*, and four are reads. Nothing here writes a pixel, which keeps
  invariant 1 structural rather than remembered.

## Alternatives considered

**A single `story_edit(action, args)` command.** Rejected: one command with a `kind`
discriminant makes the permission surface, the audit line and the TypeScript union all
harder to read, and the four edits have genuinely different arguments and different
failure modes. `AURA-ML-5025`'s runbook can list which rule fired precisely because the
four are separate.

**Returning `attrs` as a number and decoding in TypeScript.** Rejected in decision 3.

**Making `scene_profiles` writable now.** Rejected in decision 4: a control with no
visible effect trains photographers to ignore it.
