# ADR-0032 - The tone IPC surface

**Status:** accepted · **Date:** 2026-08-18 · **Phase:** 15 · **Supersedes:** nothing

## 1. Context

Phase 15 produces three numbers per photograph, plus the argument behind them. The Basic
panel shows those numbers, the review queue shows the frames the solver was least sure
about, and the merge in phase 14 is what turns any of it into pixels. This document freezes
the wire between them.

Three things make this surface different from the eleven before it. It carries a *decision a
person will immediately disagree with* on some frames, which means the override path is a
first-class command rather than an afterthought. It is the first surface where an automated
pass writes a recipe, which means it has to sequence two writes rather than one. And it is
the first that could plausibly carry a per-person appearance measurement, which means it has
to decide not to.

## 2. Decision: seven commands, and what each of them refuses to be

| Command | Answers | Cannot |
|---|---|---|
| `tone_status` | how much of the wedding has a decision, and how much came from a face | name a frame |
| `image_tone` | one photograph's decision, with reasons, alternatives and evidence | change one |
| `tone_review_queue` | which frames the solver was least sure about, weakest first | keep, reject or rank one |
| `reference_frames` | one chapter's anchors for phase 25 | change any other frame to match them |
| `accept_tone` | records that somebody looked and agrees | claim they authored the values |
| `set_tone_override` | records what they set instead, and applies it | be reached by an automated caller |
| `estimate_tone` | runs the resumable pass and carries the result into the recipes | overwrite a protected path |

Three shapes that a reviewer asked for are deliberately not here.

**No `reset_to_ai`.** Phase 14's `history_step` already has `reset_ai`, and it is the right
one: undoing an override is a history operation on the recipe, not a tone operation on the
estimate. A second reset would be a second answer to "what does the AI suggestion mean".

**No batch override command.** The review queue's "apply to these forty" is forty
`set_tone_override` calls from the panel, each of which records its own row and protects its
own paths. A batch command would be one call that either half-succeeds invisibly or needs its
own partial-failure vocabulary, and the only thing it buys is round trips on an operation a
person triggers by hand a few dozen times a wedding.

**No command that returns a skin locus.** Section 4.

## 3. Decision: an override is two writes, in this order

`set_tone_override` writes the estimate row first and the recipe second.

The estimate row records the *disagreement*: `user_edited = 1`, plus whichever of the three
values the photographer typed. The recipe write goes through `aura_recipe::schema::merge` with
`EditSource::User`, which is the only function in the workspace permitted to add to
`user_edited_fields` and therefore the only way `global.exposure`, `global.temperature` and
`global.tint` become protected from every later automated pass.

The order matters because of what a crash between them leaves behind. Estimate-then-recipe
leaves a catalog that says the photographer disagreed and a recipe that has not caught up -
which the next pass reconciles, because `write_recipes` skips a `user_edited` row and the
panel shows the estimate's own numbers. Recipe-then-estimate would leave a protected recipe
path with no record of who protected it or why, which is a support case with no evidence in
it.

The command returns both halves - the estimate and the merged recipe, with `changed` and
`protected` - so the panel never has to guess whether the two agree.

## 4. Decision: a skin locus does not go on the wire

`ToneService::skin_locus` and `skin_loci` exist, are frozen, and are used by the pass. Neither
is reachable from the IPC surface, and adding one needs an ADR.

A locus is a chromaticity, a radius and a luminance measured from one named person's face
across one wedding. It is not a biometric template - it cannot identify anybody and it is not
in the sealed store phase 06 built - but it is a per-person measurement of what somebody's
skin looks like, keyed to an identity a photographer has given a name to. Putting it on the
wire makes it something a panel can render, a screenshot can carry, a support bundle can
accidentally include and a later phase can display beside a face. None of those is a feature
anybody asked for.

What the surface carries instead is **counts**: `constrainedIdentities` on a frame, and
`skinConstrained` over a project. Those answer the only question a photographer actually has -
"did AURA have anything to check this colour against" - and carry nothing about any
individual. Section 6.3's fairness argument needs the loci to *exist*; it does not need them
to be visible.

The evaluation harness measures per-bucket dE00 and never touches this surface either. Phase
06's condition C5 is the same rule read the same way.

## 5. Decision: the panel is told what the pass could not do

Four of `ToneDto`'s fields exist so that a degradation is a sentence rather than a silence:

- `faceAnchored` false means the frame was exposed on its scene rather than on a subject.
  Section 1's entire improvement, per frame, as one bit.
- `constrainedIdentities` zero means nothing bounded the colour except the light itself.
- `mixedLight` means the colour was set for the people and is wrong somewhere else in the
  frame, and phase 18 has not shipped.
- `colouredLight` means a cast was preserved on purpose, so that a photographer who thinks the
  white balance failed can see that it did not.

`ToneStatusDto.faceAnchored` and `.skinConstrained` are the project-level versions, and the
Basic panel renders the second one as a caveat below half. This is phase 05's coverage rule
in its ninth outing, with the refinement that this phase has *two* denominators and the one
that matters is not the obvious one.

## 6. Consequences

- `ui/src/ipc/types.ts` gains eleven types and `contracts.lock` is re-locked. Both files are
  digested; changing either without the other fails CI.
- The desktop shell runs all seven commands on the blocking pool. Six of them are small reads
  today; `estimate_tone` decodes a proxy and runs two heads per frame over a whole wedding,
  and a small read that grows a join later must not be able to become visible jank without
  anybody noticing.
- Phase 16's tone-curve surface will sit beside this one rather than inside it. There is no
  curve, contrast, saturation or HSL field anywhere here, which is section 2.2's boundary made
  structural rather than remembered.
