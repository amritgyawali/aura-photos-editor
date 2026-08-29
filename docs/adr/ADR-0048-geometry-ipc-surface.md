# ADR-0048 - The geometry IPC surface

**Status:** accepted · **Date:** 2026-08-28 · **Phase:** 23 · **Supersedes:** nothing

The second of phase 23's two ADRs.
[ADR-0047](ADR-0047-geometry-lens-straightening-and-crop-safety.md) covers the decisions; this
covers the wire.

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen contracts, so every
shape here is in `contracts.lock` and changing one later needs an amendment to this document and a
re-lock, in that order.

## 1. Context

Phase 23 produces a decision with four parts - a lens correction, an angle, an optional
perspective correction and a list of rectangles - plus a report of what was checked before the
delivered rectangle was allowed. Section 4 asks for one panel: "Crop/straighten UI with AI
proposal and revert."

Three things make this surface different from phase 22's.

**One of its commands is an acceptance criterion.** Section 13: "Original framing is always one
click away." No previous phase has had a *specific interaction* promised in its acceptance
criteria; phase 22 promised behaviour and left the interaction to SFE.

**The photographer may set a number this surface cannot bound.** A crop rectangle is a rectangle.
The panel's handles are clamped, but the wire has to carry four floats and the store has to accept
them, so "no strength field anywhere" - which ADR-0044 could say about phase 21 - is not available
here. What is available is a different and stronger line, and section 3 draws it.

**The most important thing this surface carries is a count of things that did not happen.** Most
frames keep their framing, most reason codes are refusals, and a panel that listed only changes
would render an empty box on the eight frames in ten that are working correctly.

## 2. Decision: nine commands, and what each of them may touch

| Command | Reads | Writes |
|---|---|---|
| `geometry_status` | `v_geometry_coverage`, `v_geometry_safety` | nothing |
| `image_geometry` | one `geometry_plan` row and its variants | nothing |
| `geometry_variants` | one photograph's `geometry_crop` rows | nothing |
| `geometry_review_queue` | `geometry_plan` ordered by confidence | nothing |
| `accept_geometry` | - | `geometry_plan.reviewed` |
| `revert_geometry` | - | the plan, back to what was shot |
| `set_geometry_override` | - | the plan, with `user_edited` |
| `geometry_pass` | previews, faces, scenes, horizons | plans and variants |
| `geometry_reason_codes` | the frozen enum | nothing |

`geometry_variants` is separate from `image_geometry` because phase 29 wants that list and nothing
else: an album layout that had to decode a whole plan to take one list out of it is a layout pass
parsing reason codes it never renders. It is the same argument that put `identity_refusals` on
phase 22's surface as its own command.

`geometry_reason_codes` assembles the panel's legend from `GeometryCode::ALL` rather than from a
list in the UI, so a code added to the vocabulary cannot go missing from the panel. Phase 13's
rule about the reason registry, in the small.

## 3. Decision: the revert is its own command

`revert_geometry` takes a photograph id and nothing else.

`GeometryOverride` has a `revert` boolean, so the obvious wire shape is one command with seven
optional fields, one of which is that flag. It is rejected for two reasons.

**A revert is not an override.** It clears the crop, the rotation, the keystone *and*
`user_edited`, so automation resumes on the frame. An override sets `user_edited` so automation
never touches it again. Those are opposite requests, and a photographer who clicks "back to the
original framing" is asking for the first - they want their photograph back, not a hand-set
full-frame crop that a better lens profile will never improve.

**An acceptance criterion should not be assembled by the caller.** "One click away" becomes one
call with one field. A flag inside a payload with six other optional fields is a revert that
every caller has to construct correctly, and the failure - sending `revert: true` alongside a
crop, or forgetting it and sending a full-frame rectangle instead - leaves a frame marked as
hand-framed that nothing will revisit. That failure is silent for the life of the catalog.

The panel renders the button unconditionally, on every plan, whether or not anything was changed.
`GeometryPanel.test.tsx` asserts it.

## 4. Decision: what a photographer may do, and what nobody may do

The line this surface draws is **not** between "a number" and "a switch", as phase 22's was. It is
between *this photograph* and *the rule*.

**A photographer may crop one photograph of their own as tightly as they like.** Below the
resolution floor, through a face if they want to. It is their photograph, they are looking at it,
and a product that refused would be overruling the person whose name is on the gallery. The
rectangle is stored with `user_edited = 1` and is never re-cropped - `GeometryStore::pending`
excludes hand-framed rows unconditionally, whatever the versions say, which is the one place in
the product where a version bump does *not* re-derive a row.

**Nobody may change what automation is allowed to do.** There is no field on any input here that
relaxes the safety filter, no per-project setting that lowers the resolution floor, and no way to
say that cutting faces is acceptable in general. That setting is the one that crops the next four
hundred frames through people, and section 12's first failure mode is exactly it.

`crop_rules.toml` is the only place a bound can move and it may only move one way: the loader
refuses a file that raises the resolution floor, lowers the improvement margin, shrinks the safety
margin or raises the rotation ceiling, with `AURA-ML-5112`. A studio may be stricter than AURA and
may not be laxer. Phase 21's rule - a ceiling can be lowered by a studio and raised by nobody -
holds a third time.

**A malformed rectangle is `AURA-ML-5111`, not `AURA-ML-5112`.** The two codes have different
runbooks and different severities: 5112 is run-blocking and says AURA cannot read its own settings
and will not straighten or crop anything, and a photographer who dragged a crop handle past the
edge of their photograph has not broken their installation. The store raised 5112 for this at
first and it is fixed.

## 5. Decision: no command returns pixels

`RenderService` is the only way to turn a recipe into pixels and this surface does not go near it.
The panel's before-and-after is the develop view's existing render asked for twice, at the
delivered rectangle and at the whole frame; what this surface adds is the *rectangle*, so the two
renders are the same photograph asked two questions.

Phase 13's rule - evidence can never be a pixel - therefore holds without any special effort here,
and `CropRectDto` is the only geometry on the wire.

## 6. Decision: both safety numbers travel together, and so does the lens provenance

`GeometryStatusDto` carries `facesChecked` beside `facesCut`, and `CropSafetyDto` carries
`considered` beside `atRisk`. `GeometryPlanDto` carries `lensMeasured` beside every correction.

Neither pair is redundant. On this build the first number of each pair is zero everywhere: phase
06's detector is a placeholder, so nothing is protected, and every lens profile is a reference
model. A surface that carried only `facesCut` would report `0` - the value that means "the
guarantee held" - on a build where the guarantee was never tested. The panel is written to say so
in words rather than to print the zero, and `GeometryPanel.test.tsx` asserts that too.

This is phase 21's rule and phase 08's before it: say what the denominator is. It has never
mattered more than here, because the number it qualifies is the phase's headline promise.

## 7. Decision: the pass is one command with a kill switch, and the panel is not the only caller

`geometry_pass` takes a project, a priority and an `enabled` flag. There is no photograph list on
this surface even though `GeometryPass::run_ids` exists and is the path the job graph uses with
phase 12's keepers - invariant 3, expensive work on survivors. The panel runs a project pass; a
scheduler runs the selected set through the pass object directly.

`enabled: false` still writes a plan per frame, one that does nothing. A frame with no plan and a
frame the studio switched off look identical in a coverage report, and hard rule 8 asks for a kill
switch rather than for a silence.

## 8. Consequences

**Good.** The revert is unambiguous and testable. The panel cannot be built in a way that hides
restraint, because the restraint list is what the status command returns most of. The safety
promise arrives with its denominator, so a build where it means nothing says so.

**Bad.** The surface carries a rectangle, so it carries a number a caller could misuse - a
malicious caller can set a crop that cuts a face on one photograph. That is the photographer's own
authority by design, and the audit trail is `user_edited = 1` plus the ledger row.

**Neutral.** Nine commands is the widest surface since phase 21's nine, and eight of the nine are
one-line wrappers. The ninth, `geometry_pass`, is the only one that does work.

## 9. What this ADR does not cover

The desktop shell registers these nine commands and, from this phase, phase 22's seven as well -
those had never been wired in, so the phase 22 exit report's C5 claim that they were reachable
from the Tauri surface was wrong. `ui/src/ipc/client.ts` gains a typed `geometry` block; phases 20
to 22 still have none, and that half of C5 stays open.
