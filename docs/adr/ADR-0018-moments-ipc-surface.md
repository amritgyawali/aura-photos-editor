# ADR-0018 - The moments IPC surface

**Status:** accepted
**Date:** 2026-08-14
**Phase:** 08 - Smart Burst Grouping & Duplicate Detection
**Supersedes:** nothing. **Amends:** nothing.
**Deciders:** CTO (surface shape), SFE (the stacked grid), MFE (the duplicate panel),
UX (the wording of everything a photographer reads), PM (what the panel promises)

---

## 1. Context

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are one frozen contract
in two languages, digested together in `contracts.lock`. Phase 08 adds nine commands,
eight types and one event to it. This ADR records the five decisions that shaped them.

## 2. Decision 1 - nothing on this surface can reject a photograph

Nine commands: five change a grouping, one moves a hint, three are reads.

There is no `cull`, no `reject`, no `delete` and no rank anywhere on the wire. Section
2.2 puts every question about a photograph's fate in phase 12, and this is how that
boundary is kept structural rather than remembered - a phase 09 or phase 11 author
reaching for "just mark it rejected" finds nothing to call.

The closest the surface comes is `set_keep_hint`, and it deliberately is not a culling
decision: it moves where phase 12 *starts from* and changes nothing about what phase 12
may look at. Every other frame in the set stays exactly as eligible as it was.

## 3. Decision 2 - a split locks both halves

Phase 07 locks both chapters either side of a moved boundary, because a boundary is
shared and locking one side would let the next re-analysis move it back from the other.
The same argument applies one level down: a split is **one statement about two moments**,
and locking only the original would let the next grouping pass re-absorb the new half.

The frames of a locked moment are then *subtracted from the pass's input* rather than
reconciled afterwards, which is stronger than preservation: the pass cannot produce a
grouping that contradicts a decision already made, because it never sees the frames.

## 4. Decision 3 - coverage is measured against groupable frames, not photographs

`MomentStatusDto` carries `photos`, `groupable` and `grouped` as three separate numbers,
and `coverage` is `grouped / groupable`.

A frame with no embedding cannot be grouped by any amount of trying. Reporting it as a
grouping failure would send a photographer - or a support engineer - looking in phase 08
for a phase 05 gap. So the denominator is what this phase can be held to, and the
photograph total travels beside it so nobody concludes that 1,800 photographs have
vanished. `coverageSentence` in `MomentStack.tsx` puts both in one sentence, and a test
asserts it.

This is phase 05's rule inherited a fourth time, with the refinement that the
*denominator* is now part of the rule.

## 5. Decision 4 - the duplicate panel promises, in the interface, that nothing is deleted

Section 12's fourth failure mode is "duplicate deletion anxiety". Photographers have
been burned by software that tidied their card, and a product that marks frames as
duplicates has to answer that before it is asked.

So it is answered three times, in three different places a photographer looks:

* `setConsequence` - "Nothing has been deleted; you can still see and export every one
  of them."
* `keepConsequence` - "This marks which frame AURA should start from. It does not delete
  anything, and it does not stop you choosing a different frame later."
* `duplicateSentence` in the header - "Nothing has been deleted, and you can see every
  frame."

Three tests assert those sentences say so. That is unusual - the UI test suite does not
normally assert copy - and it is justified because the promise is the product decision,
not the wording.

## 6. Decision 5 - reasons and confidences are on the wire, on both objects

`MomentDto.reasons`, `MomentDto.confidence`, `DuplicateSetDto.reasons` and
`DuplicateSetDto.confidence`. Invariant 2 applies to both objects because both are
decisions: "these six frames are one moment" and "these two frames are the same
photograph" are separately arguable, and a photographer disagreeing with either needs to
see the evidence for that one.

The confidence is rendered as a **word** with the number beside it (`confidenceLabel`),
for `chapterStrip`'s reason: somebody deciding whether to look wants to know whether to
look, and "0.83" does not answer that.

## 7. What is deliberately not on the surface

* **A regroup-one-moment command.** Grouping is a whole-project pass; a per-moment
  regroup would need a boundary condition ("what may it take from its neighbours?") that
  has no good answer.
* **The threshold table.** Phase 07 puts `scene_profiles` on the wire because a
  photographer asks "why is my dance floor being judged this way". Nobody asks that about
  a grouping threshold - they split the stack. If that turns out to be wrong, adding it
  is additive.
* **An emitter for `MomentEvent`.** The three events are typed on both sides and not
  emitted, for the reason `PeopleEvent` and `StoryEvent` are not: the Tauri shell has not
  been launched on the development machine, so an emitter would be code nobody has run.
  They are `tracing` spans today.

## 8. Blast radius

```text
crates/aura-app/src/contract/ipc.rs   +8 types, +1 event    FROZEN, re-locked
ui/src/ipc/types.ts                   +8 types, +1 event    FROZEN, re-locked
crates/aura-app/src/moment_commands.rs  new                 9 commands
crates/aura-app/src/state.rs          +4 methods            store, service, context, median
ui/src/ipc/client.ts                  +10 methods
ui/src/components/grid/MomentStack.tsx      new
ui/src/components/grid/DuplicatePanel.tsx   new
ui/src/components/grid/MomentStack.test.tsx new             24 tests
```

Both frozen files changed together and `contracts.lock` was regenerated in the same
commit, which is what `cargo xtask contracts --check` enforces.
