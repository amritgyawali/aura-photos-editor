# ADR-0039 - Local light sculpting: the mask port, the fairness guarantee and the halo test

**Status:** accepted · **Date:** 2026-08-19 · **Phase:** 19 · **Supersedes:** nothing

Phase 19 section 4 asks for no ADR by name. It needs two anyway, and this is the first of
them: section 5 freezes a contract that consumes a phase which has not shipped, section 6.4's
scaling rule has two readings and only one of them is defensible, section 10.1's fairness gate
cannot be met as written, and its halo gate cannot be *measured* as written. All four are
decisions, and a decision nobody wrote down is a decision the next phase re-argues from
scratch. The second document is [ADR-0040](ADR-0040-local-ipc-surface.md), which covers the
wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned
to phase numbers.

## 1. Context

Eighteen phases decided which photographs are delivered, what colour the light was and how a
decision becomes pixels. Phase 15 set an exposure and a white balance for the whole frame.
This is the first phase that moves light *inside* one, and section 1 is direct about why it is
worth a phase:

> Global adjustments cannot fix the most common wedding lighting problems: a face in shadow
> under a mandap, a bright window behind the couple, a hot spot on a forehead. Local light
> shaping is where perceived quality jumps.

Three things make it hard, and they are not the same difficulty.

**Subtlety is the deliverable.** Section 0's risk line is "Medium-High - subtlety is the whole
point", and section 1's second paragraph says what failure looks like: "done badly it looks
obvious - haloes, glowing faces, muddy backgrounds". No other phase in this product has a
success condition that is *the absence of a visible effect*. A culling engine that is a little
too aggressive is a gallery somebody adds frames back to; a local editor that is a little too
strong is a gallery that looks processed, and nobody can point at the frame where it went
wrong.

**It consumes a phase that has not shipped.** Every operation here is local, which means every
operation needs a mask, and masks are phase 18's. This repository is at phase 15. That is not
a detail to work around - it is the largest single fact about what this phase can and cannot
claim, and section 5 of this document is about it.

**Its own arithmetic can produce the artefact it exists to avoid.** A halo is not something
that happens to a local edit from outside; it is what a local edit *is* when its own falloff
is wrong. Section 6 of this document records one that the evaluation harness found in this
phase's first implementation.

## 2. Decision: five spellings differ from section 5, and here is each one

Section 5 freezes a struct. The frozen shape in `crates/aura-core/src/contract/local.rs`
differs from it in five places. Step 2 of the phase ritual says an interface change after the
design review needs an ADR amendment; this is that amendment, made before any of the code was
written rather than after.

| Section 5 | What shipped | Why |
|---|---|---|
| `face_light: Vec<(IdentityId, FaceLightDelta)>` | `Vec<(Option<IdentityId>, FaceLightDelta)>` | At most weddings the majority of the people in frame are guests phase 06 has not named. A required identity would mean either inventing one or declining to light most of the room. |
| `dodge_burn: Option<DodgeBurnMaps>` with two flat maps | `DodgeBurnMaps { faces: Vec<FaceShaping> }` | One struct with two maps cannot express a group formal at all, and section 6.1's group-fairness rule means a group formal is exactly the frame this phase must not get wrong. |
| `gated_by_mask_quality: Vec<MaskKind>` | `Vec<(LocalOp, MaskKind)>` | The kind alone says a mask was missing and not what it cost. "The background mask was unavailable" and "the background balance did not run because the background mask was unavailable" are the same fact and only the second is a sentence a panel can render. |
| — | `scene`, `strengths`, `user_edited`, `reviewed` | Invariant 7 needs the scene stored: a decision that does not say which scene it assumed is not reproducible. The strengths are what the panel's six sliders read. The two flags are phase 15's own shape, for the reason its ADR gives. |
| — | `model_ver`, `analysis_ver`, `policy_ver`, `shaping_ver` | Four version columns rather than three, and section 3 of this document is about the fourth. |

## 3. Decision: a fourth version column, because the shaping is derived twice over

Every phase since 06 has carried version columns that invalidate different things. This one
carries four, which is one more than any before it, and the fourth exists because of a
storage decision rather than a modelling one.

A dodge-and-burn map is a 32×32 grid per band per face. Four faces at two bands is 8 KB per
photograph, and a wedding's worth of that is a catalog nobody can back up - so the grid is
**derived** rather than stored. Phase 13's rule, "evidence can never be a pixel", applied to a
decision rather than to evidence.

The first implementation stored the *zones* the grid is generated from: ten named moves per
face, each a centre, a radius and a gain. That is legible - a support engineer reads "jaw,
−0.04 EV" and knows what happened - and it measured 1,286 bytes per image, which is more than
half of everything this phase stores. So the catalog now stores one step further back: the
face region, the measured light direction and the strength, from which `zones_for` reproduces
the zones exactly, from which `grid` reproduces the map exactly. That is 114 bytes, and the
panel still shows every zone by name because they are regenerated on read.

The consequence is that **a change to either derivation moves delivered pixels without moving
a single stored number**. Nothing else in the product has that property, and nothing else
needs `shaping_ver`. `AURA-ML-5084` is raised when a comparison would cross any of the four.

## 4. Decision: this phase does not own a mask, and has no fallback that draws one

Phase 18 owns masks. `MaskField` in the frozen contract is the input port this phase reads
them through: a kind, an optional identity, a coarse alpha field, a confidence and an edge
quality. There is no mask generator, no segmentation model and no geometric fallback anywhere
in `aura-brain-photo::local`.

The tempting alternative is obvious and was rejected. A face box from phase 06 and a subject
box from phase 11 would give a rectangle each, and a rectangle with a wide feather is a
perfectly usable mask for an exposure lift measured over a region. It would let this phase do
something today rather than gate everything.

It is wrong for one reason and the reason is the whole phase. A rectangle's edge does not
follow a person, so an edit through it is applied at full strength to a strip of background
beside them - which is a bright rim, which is a halo, which is section 12's first failure mode
and the thing section 1 says destroys credibility. And it would be a *second* answer to "where
does the subject end", disagreeing with phase 18's when it arrives, so a gallery edited before
and after phase 18 ships would have two different edits in it that nobody could tell apart by
looking.

So when a field does not arrive, the operations that needed it are **gated rather than
guessed**:

* below `MIN_MASK_CONFIDENCE` the operation is skipped and named in
  `gated_by_mask_quality`;
* between there and `FULL_MASK_CONFIDENCE` the strength ramps linearly and is multiplied by
  the edge quality;
* an unreadable field is treated as absent.

**Two numbers rather than one**, because they fail differently: a mask can be confidently the
right region and have a terrible boundary - hair against a bright window is the standard case -
and that combination is safe for an exposure lift measured over the region and dangerous for
anything with a falloff. One number would have to pick which failure to hide.

On this build every frame is gated, `LocalOutline::mask_covered` reads zero, and the panel says
so. That is condition C1 of the exit report and it is a *state*, not a fault.

## 5. Decision: section 6.4's scaling order, read the way that protects the lift

Section 6.4 says:

> when the budget is exhausted, operations are scaled down in priority order (face lighting
> first, dodge/burn last).

That sentence has two readings. Either face lighting is the first thing given up, or face
lighting has the first claim on the budget and dodge and burn is the first thing given up.

The second is what shipped, and the argument is what a photographer would miss. Face lighting
is the operation section 1 exists for - "a face in shadow under a mandap" is its first
example. Dodge and burn is both the most decorative and the most artefact-prone: it is the
operation that reads as "edited" first when it is too strong, and the one whose absence
nobody notices. A budget that protected the shaping and gave up the lift would be spending a
photographer's one allowance on the part of the edit they would not miss, in order to skip the
part they asked for.

`LocalOp::PRIORITY` is the order, there is no second list, and `governor::allocate` walks it
once. A scene policy row that gave `dodge_burn_low` a higher strength than `face_light` is
refused by the loader with `AURA-ML-5087`, so a config file cannot reverse it either.

## 6. Decision: the group-fairness guarantee is about the edit, not about the frame

Section 10.1 asks for "inter-face luminance spread after lighting <= a documented threshold".
Read as an absolute claim about the finished photograph, that is a promise no arithmetic can
keep.

A family formal where one person stands two stops down under a doorway arrives with a 0.34
spread. The noise cap allows a 0.6 EV lift on that face and no more - lifting further reveals
grain, which is section 6.1's own dynamic cap doing its job. So the spread after the best
possible lighting is 0.27, and the threshold is 0.08. There are exactly two ways to satisfy
the absolute reading, and both are worse than the problem:

* **refuse to plan the frame.** The most common difficult group photograph at a wedding then
  gets no local work at all, which is the frame the phase most needed to help with.
* **darken everybody else to match.** One person nobody could light decides the brightness of
  eleven others, and a family formal comes back uniformly two stops down.

So the guarantee is about the *edit*: **the lighting reaches the threshold whenever the caps
allow, and it can never make a group less even than it found it.** A frame that arrives inside
the threshold must stay inside it; a frame that arrives outside must come closer or stay where
it is. `LocalLightPlan::group_is_fair` is that predicate and `AURA-ML-5086` refuses a plan
that breaks it.

The second half is implemented structurally rather than promised: `face_light::enforce_spread`
only ever moves a face *down* toward the group, and never below where the photograph put it.
It can give back a lift AURA applied and nothing more. The panel says
`GroupSpreadCapped` and, when the gap could not be closed, tells the photographer that nobody
was darkened to close it.

## 7. Decision: the halo test is not an edge-gradient ratio, and here is what it found

Section 10.1 asks for "an automated edge-gradient test finding no artefact on 99 % of
fixtures". Four readings of that were implemented; three were discarded and the fourth found a
real defect.

**A before/after gradient ratio measures the edit's size.** *Every* local brightening
increases the step at its own boundary - that is what "local" means. A face lifted half a stop
out of a dark reception has a larger step against the room than it did, whether the mask was
feathered beautifully or cut out with scissors. On the fixtures this scored 1.3 to 3.1 and
called a correct edit an artefact.

**Peak-over-mean gradient scores a hard edge perfectly.** A hard edge puts its whole transition
into one sample, so its peak and its mean are the same number.

**The transition width of the difference image breaks on a good matte.** When a mask's edge
coincides with a content edge - which is exactly what a subject matte is for - the difference
image steps there no matter how wide the feather is, because the same lift applied to skin at
0.18 and to the wall behind it at 0.30 are different sizes.

**What a halo actually is, is an edit that is stronger further from the subject than nearer to
it.** So the gate is on the two properties that make that impossible, both measured through
`aura_render::local`: the falloff never overshoots, and the edit is monotonic in the matte and
never exceeds its value at full coverage.

That gate failed on the first implementation, and the defect is worth recording because it is
the kind that looks conservative. `apply_face_light` evaluated its luminosity weights on the
*partially edited* pixel - the shadow weight after the exposure had moved, the highlight
weight after both. It reads naturally. But the weight then grows with the matte at the same
time as the term it scales does, so the highlight restraint grew quadratically while the lift
grew linearly, and past about half coverage the restraint overtook. Measured on a mid-bright
pixel, the edit peaked at 0.022 at half coverage and fell to 0.014 at full: **a bright pixel
received more lift at the mask's edge than at its centre**, which is a bright rim.

Both weights now read the input pixel, evaluated once, so the whole edit is linear in the
matte. The same change is in `local_apply.wgsl` and `tests/shader_parity.rs` holds the two
together.

**A pixel-level halo audit over four hundred real frames is a different thing and does not
exist here.** It is condition C3 of the exit report, beside the expert subtlety study, because
a synthetic fixture whose face box was painted as a rectangle cannot stand in for a photograph
of somebody's hair against a window.

## 8. Decision: fifteen modules, not the six section 4 names

Section 4 names `{face_light, subject, background, dodgeburn, shine, governor}.rs`. Fifteen
shipped. The nine additions are not subdivisions of the six:

| Module | Why it is not part of one of the six |
|---|---|
| `policy` | The per-scene table and its loader. Six modules read it; putting it in any one of them makes the other five depend on that one. |
| `measure` | One pass over the pixels producing every statistic the six need. Section 11's 80 ms is only reachable because the frame is read once. |
| `luminosity` | The split that stops a lifted face glowing. Section 6.1's first bullet, used by `face_light` today and by phases 20 to 22 later. |
| `freqsep` | Three-band separation. Owned by neither `dodgeburn` nor `shine`, read by both. |
| `guard` | Turns the contract's predicates into this phase's errors. `aura-core` owns the shapes; `aura-brain-photo` owns the registry. |
| `plan` | The analyser: one decoded frame in, one plan out. |
| `store` | Migration 19's tables. |
| `api` | The frozen `LocalService` and the resumable walk. |
| `fixtures` | The synthetic ground truth every gate is measured against. |

## 9. Decision: the thirty codes do not enter phase 13's reason registry

`aura_explain::reason::Catalog::shipped` walks four vocabularies - phase 09's `ReasonCode`,
phase 10's `EmotionCode`, phase 11's `CompositionCode` and phase 12's `CullCode` - and
`docs/reason-codes.md` is the public reference generated from them. Phase 15's twenty-five
`ToneCode`s are not in it, and phase 19's thirty `LocalCode`s are not either.

That is a decision rather than an omission, and it is phase 13's own: the ledger records
*decisions about a photograph's fate*, and the four vocabularies in the registry are the ones
that feed a cull. A local light plan is a decision about pixels, and its explanation belongs on
the surface that shows the pixels - the Local panel, and `docs/local-light.md`, which
`local_eval` asserts contains a sentence for every one of the thirty.

The line to watch is phase 27. A QC agent that asks "why does this one look edited" is asking
about a local light plan, and if it needs those reasons in the ledger to answer, this is the
decision to revisit - with phase 15's twenty-five, since the same argument applies to both.

## 10. Decision: what this build's numbers are and are not claims about

Three conditions, and the exit report carries all three.

**C1 - every mask is a fixture's.** Phase 18 has not shipped. Every gate that involves a mask
was measured against a field aligned exactly with a painted region, at confidence one and edge
quality one. That proves the gating arithmetic and says nothing about a photograph of a person
against a background.

**C2 - the learned targets are never consulted.** `TARGET_HEAD_TRAINED` is false, so
`Analyser::learned_targets` returns `None` and phase 15's own per-scene bands are what runs.
The plan carries `TargetHeadUnavailable` so nobody mistakes one for the other. Section 8 step
1 asks for targets extracted from expert difference maps and there is no corpus of expert
edits in this repository; `ml/models/local/train_light_targets.py` is the extraction, written
and self-tested and unable to run on anything real here.

**C3 - the subtlety and halo audits are human studies and do not exist.** Section 10.1's
seventh gate is "expert subtlety rating >= 4.2/5 with no 'obviously edited' flags" over an
audit set, and section 9 gives QAIQ four hundred frames to hunt haloes in. Neither exists here
and no arithmetic substitutes for either.

## 11. Consequences

**Good.** The rule that matters is structural: this phase cannot invent a mask, so it cannot
produce the artefact that comes from a wrong one. The budget is a stored number with a schema
check on it, so six defensible adjustments cannot silently become one obvious one. The
shaping is three numbers per face rather than eight kilobytes, and the panel is no worse for
it. And the halo gate found a real defect on its first run, which is the only evidence worth
having that a gate is doing anything.

**Bad.** On this build the phase does very little: every operation is gated, `mask_covered` is
zero, and a photographer would see a Local panel full of "not available". That is honest and
it is not useful, and it will stay that way until phase 18 ships.

**Ugly.** The group-fairness guarantee is weaker than section 10.1's words, and somebody
reading the phase document and then the code will notice. Section 6 above is the argument;
`docs/local-light.md` says the same thing in the product's own voice, because a photographer
who is told "everybody in a group photo is lit consistently" and then sees a family formal that
is not needs to be able to find out why without reading an ADR.

**The thing to watch.** Four version columns is a lot, and the fourth is load-bearing in a way
that is easy to forget: `shaping_ver` is the only thing standing between a tweak to `zones_for`
and every delivered gallery in the field quietly changing. A build that edits that function
without bumping it will pass every test in this repository.
