# ADR-0041 - Portrait retouch: temporary versus permanent, the texture floor and per-identity consistency

**Status:** accepted · **Date:** 2026-08-20 · **Phase:** 20 · **Supersedes:** nothing

Phase 20 section 4 asks for no ADR by name. It needs two anyway, and this is the first: section
5 freezes a contract whose spellings cannot all survive contact with this workspace, section
6.1 makes an ethical claim that has to be a property of the schema rather than of a threshold,
section 6.4 and section 10.1 ask for two things that cannot both be true of one number, and the
two heads section 4 asks for cannot be trained here. All four are decisions, and a decision
nobody wrote down is a decision the next phase re-argues from scratch. The second document is
[ADR-0042](ADR-0042-retouch-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned
to phase numbers.

## 1. Context

Nineteen phases decided which photographs are delivered, what colour the light was, how a
decision becomes pixels, where the regions are and how light is shaped inside a frame. This is
the first phase that changes what a person's **skin** looks like, and section 1 is direct about
the stakes:

> Retouching is the most emotionally sensitive part of wedding delivery: clients want to look
> like themselves on a good day, not like a mannequin.

Three things make it hard, and they are three different difficulties.

**The failure is identity, not quality.** A slightly over-strong grade is a photograph somebody
adjusts. A removed mole is a photograph of somebody else. Section 1's third paragraph -
"freckles, moles, scars and birthmarks are identity, not defects" - is an *ethical* requirement
that happens also to be a quality requirement, and the two do not fail together: a retoucher
that scores well on blemish recall and removes one beauty mark in fifty has passed its gate and
failed its purpose.

**The success condition is invisibility, and it is measurable.** Section 0's headline KPI is a
texture-retention number, which is unusual and is the reason this phase can make a defensible
claim at all. "We don't produce plastic skin" is marketing; "post-retouch high-band energy in
skin regions is at least 0.90 of the original, asserted in CI" is a test.

**It runs across a gallery, not over a frame.** Section 6.4's last bullet - the same person is
retouched with the same strength everywhere - is what separates this from every consumer
retoucher, and it is also the constraint that decides the shape of the strength model. Phase 19
could solve one frame in isolation. This phase cannot.

## 2. Decision: six spellings differ from section 5, and here is each one

Hard rule 8 says frozen contracts are copied in verbatim. Six cannot be, and each substitution
is toward something this workspace already froze.

1. **`Box2` is `aura_core::contract::composition::Box2`**, which is already an alias of phase
   09's `CropRect`. Phase 11 made that substitution and this inherits it. A second normalised
   rectangle is two answers to "where on the photograph", and an evidence crop that phase 13
   renders has to be the same shape whichever phase produced it.

2. **`Blemish { box: .. }` is `Blemish { area: .. }`.** `box` is a reserved word in Rust. Phase
   11 spelled the same field `area` and this follows it rather than inventing `r#box`.

3. **`HashMap<IdentityId, f32>` is `BTreeMap<IdentityId, f32>`.** `HashMap::new` is refused by
   `scripts/check-banned.sh`, and the reason is invariant 4: a map whose iteration order is
   seeded per process cannot produce byte-identical recipe JSON twice. The strength map is
   serialised into the plan and hashed into the recipe, so this is the difference between a
   determinism gate that passes and one that passes on a Tuesday.

4. **`ProtectedFeature` gains `first_seen`, `frames` and `source`.** Section 5 gives it
   `{ box, kind, identity }`. A protected feature whose evidence is one rectangle cannot answer
   the only question a photographer asks about it - *why do you think that is permanent* - and
   section 6.1's own mechanism is cross-frame evidence, which is a count and a span. `source`
   distinguishes the three ways a feature gets protected: measured across frames, classified in
   one frame, or named by a person. The third outranks the other two permanently.

5. **`TextureReport` gains `measured_on`, `resolves` and `withdrawn`.** Section 5 gives it
   `{ band_ratio, floor, passed }`. A report that says `passed: false` and nothing else cannot
   distinguish "we re-solved twice and got there" from "we gave up and applied nothing", and
   those are the two outcomes a photographer needs told apart - the first is a slightly gentler
   retouch and the second is a frame nobody retouched.

6. **`RetouchOp::ShineReduce` stays in the enum and this phase never emits one.** Section 5
   marks it "shared with P19", and phase 19 already reduces specular sheen through
   `ShineReduction`, measured against `SHINE_LUMA_FLOOR`, `SHINE_CHROMA_CEILING` and
   `SHINE_MAX_AREA`. Two phases emitting the same operation is a forehead brought down twice.
   The variant stays because phases 21 and 22 will want it and because removing something from
   a frozen shape needs a *later* ADR, and `RetouchPlan::broken_guarantee` refuses a plan that
   carries one on a frame phase 19 has already reduced.

## 3. Decision: protection is a veto, not a threshold - and a tattoo cannot be scaled

Section 12's second failure mode is "removing permanent features (mole, tattoo, scar)" and
section 10.1 gates it at "false-removal of permanent features <= 2 % (and 0 % for tattoos)".
Two per cent and zero per cent are different *kinds* of number: the first is a measurement, the
second is a promise. A promise implemented as a very small threshold is a promise that fails
quietly when the detector is retrained.

So the protect set is applied as a geometric veto and not as a strength multiplier.
`ProtectedFeature::vetoes` is a rectangle test, and a blemish candidate that intersects a
protected feature is **removed from the candidate list** - before strength, before the preset,
before the texture guard. There is no code path in `aura-retouch` in which a protected region
is inpainted at a low strength, because "inpainted a little" on a mole is a mole that has been
smudged.

`ProtectedKind::Tattoo` goes one step further: `ProtectedKind::is_absolute` is true for it, and
an absolute protection cannot be cleared by a photographer through
`RetouchService::set_protection` either. Someone who wants a tattoo altered is asking for a
different product, and section 11 of `docs/plan/CLAUDE.md` - "we will never build ... any
operation that changes a person's identity" - is the reason. The refusal carries
`AURA-ML-5091` and says so.

**The default on uncertainty is to leave the skin alone.** Section 6.1's last bullet is
explicit that removing a client's mole is a far worse error than leaving a pimple, so
`TEMPORARY_FLOOR` is the temporary probability a candidate must exceed to be touched at all,
and it is 0.75 rather than 0.5. An anomaly between the two is left alone and carries
`RetouchCode::AnomalyUncertain`, which is a withdrawal the panel shows rather than a silence.

## 4. Decision: cross-frame permanence is measured in face-normalised coordinates

Section 6.1's third bullet calls cross-frame evidence "decisive and unique to a gallery-aware
product", and it is the strongest signal available: a mark at the same place on the same
person's face across hours is not a pimple, and a mark on four frames in ninety seconds is a
smudge, a shadow or a fly.

The coordinate system is what makes it work. A frame coordinate is useless - the person moves -
so every anomaly is projected into the **face frame**: the eye-to-eye line is the x axis, the
inter-ocular distance is the unit, and the origin is the midpoint between the eyes. That is the
normalisation phase 06's alignment and phase 10's expression crops already use, and reusing it
is not an economy: two definitions of "the same place on a face" would mean the retoucher and
the expression head disagree about which pixels are a cheek.

The thresholds are `PERMANENCE_MIN_FRAMES = 4` and `PERMANENCE_MIN_SPAN_MIN = 45.0`, and both
must hold. The count alone would call a burst permanent. The span alone would call one
long-lived lighting artefact permanent. A feature that meets both is added to the identity's
protect set with `ProtectedSource::CrossFrame`, and the whole gallery inherits it - including
the frames it was not visible in, which is the point.

**On a build with no face detector this mechanism finds nothing**, and that is an honest
consequence rather than a fault: phase 06's detector is a placeholder, so there are no
identities, no landmarks and no cross-frame correspondence. What survives is the single-frame
classifier and the conservative default, which is why the default matters.

## 5. Decision: the texture guard is a post-condition measured through the real operator

Phase 16 wrote this rule for skin colour and this phase inherits it for skin *texture*: a
guarantee is measured, not asserted. `texture_guard::measure` band-decomposes the skin region
**before** the retouch, applies the plan through `aura_render::retouch` - the same reference
implementation the WGSL is held to by `shader_parity.rs` - and band-decomposes the result. The
number stored on the row is the ratio of the two high-band energies.

The alternative, which every product that has shipped plastic skin implemented, is to apply the
factor to a *parameter*: bound the smoothing strength, bound the inpaint radius, and make the
promise about the pixel. Between the parameter and the pixel sit a patch synthesis whose
frequency content depends on where the donor came from, a band blend that is only as good as
its alignment, and an under-eye correction that is not linear in its own cap.

When the ratio is below the preset's floor the solver **re-solves at a lower strength** rather
than reporting a failure: `TEXTURE_RESOLVE_STEP` is 0.25 of the current strength and
`TEXTURE_MAX_RESOLVES` is 3. If three re-solves do not reach the floor the retouch is
**withdrawn entirely** for that frame - `TextureReport::withdrawn` - and the plan carries
`RetouchCode::TextureFloorUnreachable`. A frame that could not be retouched safely ships
unretouched, which is a product that occasionally does nothing rather than a product that
occasionally produces plastic skin.

`POLISHED_FLOOR = 0.80` is a hard bound on the config file rather than a default in it. The
preset loader refuses a `retouch_presets.toml` whose Polished floor is below it, with
`AURA-ML-5093`, because section 6.3's "never below 0.80 even in Polished" is a claim the
product makes in `docs/retouch.md` and a claim a text file could otherwise quietly retract.

## 6. Decision: strength is per identity and constant; size and scene decide the op set

Section 6.4's first bullet and section 10.1's third gate cannot both be read literally:

> Automatic strength from face size in frame, scene class, identity role and preset.

> Cross-frame consistency: the same identity's retouch strength varies by <= 5 % across a
> gallery.

Face size varies by an order of magnitude between a full-frame portrait and a dance-floor wide,
and scene class varies by definition. A strength that is a continuous function of either cannot
vary by five per cent across a gallery, and a gate measuring it would fail on every real wedding
while the code behaved exactly as section 6.4 describes.

The resolution is that these are two quantities and section 5's own shape already separates
them. `RetouchPlan::per_identity_strength` is the **gallery constant**: one number per identity,
computed once from the identity's role, the preset, the identity's *median* face size across the
gallery and the identity's dominant scene mix - all four of section 6.4's inputs, each taken as
a gallery-level statistic. It is what section 10.1's gate measures, and it varies by zero per
cent by construction, which is a stronger guarantee than the five per cent asked for.

What the per-frame face size and scene decide is **which operations run and how far they may
go**: below `MIN_RETOUCHABLE_FACE` there is no under-eye work, because at that size the
periorbital region is four pixels across and there is nothing there to correct; a scene profile
can cap tone evening; a face phase 19 has already evened is not evened again. The op set
shrinks and the strength does not move, and `docs/retouch.md` says that in the product's voice.

It also answers the case section 6.4 raises - "a background guest almost none" - cleanly,
because a guest's *role* is a gallery constant and their median face size is small, so their
identity strength is low everywhere rather than fluctuating with how near the camera they
happened to walk.

## 7. Decision: the two heads are placeholders and the detector runs anyway

`BLEMISH_HEAD_TRAINED` and `PERMANENT_HEAD_TRAINED` are both false. Section 8's steps 2 and 3
ask for labelled blemish and permanent-feature data across skin tones on fifteen thousand faces,
with consent, and there is no such corpus in this repository and no GPU backend to train on.

Phases 15, 16 and 18 handled the same situation by *not consulting* the placeholder at all, and
that decision does not transfer unchanged, because in those phases a reference model existed
underneath: phase 15 had per-scene luminance bands, phase 16 a deterministic solver, phase 18 a
guided filter. A phase that refused to consult its heads and had nothing else would ship a
retoucher that finds nothing.

So the shipped detector is a **measurement**, in the sense phase 18's matting is: a blemish
candidate is a small, compact, isolated mid-band anomaly whose colour sits on the red side of
the surrounding skin's chromaticity, found by a difference-of-Gaussians over the skin region in
linear light. That is a real algorithm with a real failure mode - it finds fewer things than a
trained network would and it can read a shadow edge as a mark in low light - and the failure
mode is *conservative*, which is the property this phase needs above all others. The heads are
registered, signed and carded so that the day weights exist the swap is a version bump; until
then `RetouchCode::HeadUntrained` is on every plan and nothing describes the output as learned.

## 8. Decision: eleven modules, not the eight section 4 names

Section 4 names `{lib,blemish,permanent,undereye,evening,texture_guard,strength,ops}`. Three
more exist, and each is a thing that would otherwise be smeared across the eight:

* `presets.rs` - the config loader and its refusals, which phases 15 to 19 each keep separate
  because a policy file that half-loads is worse than one that does not load;
* `store.rs` - migration 20 and the codec, kept out of the solvers exactly as phase 19 keeps it;
* `api.rs` - the frozen `RetouchService` and the resumable pass, the shape phases 06 to 19 all
  settled on.

`crossframe.rs` is deliberately *not* a twelfth: cross-frame permanence is what `permanent.rs`
is for, and splitting the single-frame classifier from the gallery evidence would put the
protect set's two sources in two files with one invariant between them. `fixtures.rs` is the
synthetic ground truth, as in every phase since 06, and `errors.rs` and `guard.rs` are this
crate's own halves of the split every phase since 09 has kept.

## 9. Decision: these codes do not enter phase 13's reason registry

The decision phase 19 recorded in ADR-0039 section 9, for the same reason and with one addition.
Phase 13's registry is assembled from phases 09 to 12's frozen enums because those are the
phases that make *decisions about photographs* - what to keep, what to reject. A retouch plan is
an edit, and phase 13's own rule is that analysis is not a decision.

The addition: a protected feature **is** close to a decision about a person, and it is recorded
where a person can see it - `retouch_protected` is a table, the panel lists every feature with
its evidence and its source, and a photographer can add one or clear one that is not absolute.
What phase 13 would add is a ledger row per frame per anomaly, which on a four-thousand-frame
wedding is a ledger nobody can search.

## 10. Decision: what this build's numbers are and are not claims about

Every gate in section 10.1 is measured against synthetic faces in `fixtures.rs` whose blemishes,
moles, freckles, dark circles and pore texture are **painted into the pixels** and read back
through the real pipeline. That proves the detector's geometry, the protect veto, the band
arithmetic, the texture floor, the re-solve, the per-identity constancy and the store.

It is not evidence about a wedding photograph, for four separate reasons that close at four
different times:

* the face detector that would find the faces is phase 06's placeholder (closes with phase 05's
  condition C10);
* the skin masks the operators run through are phase 18's, and no mask generator is wired into
  this pass on this build (closes when phase 18's planes reach a `MaskField`);
* the two heads here are untrained (section 7 above);
* no blind study against Retouch4me, Evoto or Aperty has been run, so section 13's last
  acceptance criterion is recorded as unmet rather than estimated.

`docs/progress/PHASE-20-EXIT.md` carries all four as conditions. **No later phase may claim a
retouch quality result until they close.**

## 11. Consequences

* `RetouchService` joins the fifteen services before it: it is the only way to ask what was done
  to somebody's skin. Phase 21 retouches hair, teeth and eyes and must not re-smooth what this
  phase smoothed; phase 25 normalises a gallery of these decisions; phase 27 has to be able to
  say why a face looks worked on.
* `aura-retouch` depends on `aura-render`, which is phase 16's precedent and this phase's
  requirement: the texture guard measures what the *renderer* does, not what a copy of it would
  do. It does not depend on `aura-recipe`, and `tests/no_recipe_writes.rs` fails the build if it
  ever calls `schema::merge` - writing a recipe is phase 14's rule and stays in `aura-app`.
* `aura-retouch` does not depend on `aura-cloud`, because section 7 says there is no cloud call
  in this phase, and `tests/no_network.rs` is what keeps that from being a memory.
* Migration 20 adds three tables and one view, and the protect set is the first table in this
  product whose rows a photographer creates directly.
* The perceptual allowance phase 19 introduced is **shared rather than duplicated**: this
  phase's operations spend against `aura_core::contract::local::PERCEPTUAL_BUDGET` through the
  same arithmetic, which is what phase 19's own rule promised the seventh operation would do.
