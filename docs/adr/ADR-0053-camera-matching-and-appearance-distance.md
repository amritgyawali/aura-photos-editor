# ADR-0053 - Camera matching: appearance rather than parameters, evidence rather than brand, and the three gains that are derived instead of fitted

**Status:** accepted · **Date:** 2026-08-31 · **Phase:** 26 · **Supersedes:** nothing

Phase 26 section 4 names no ADR. It needs two, and this is the first. Section 5 freezes
`CameraFingerprint` and `CameraTransform` whose six supporting types it does not define; section 6.2
gives an objective with four weighted terms and a "bounded least squares" that is under-determined
as written; section 6.1 asks for matched pairs "verified by comparing background statistics rather
than subjects" without saying what happens when there are none; and section 2.1's bundled brand
baselines are asked to be "measured in the lab" in a repository with no lab. The second document is
[ADR-0054](ADR-0054-camera-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned to
phase numbers.

## 1. Context

Twenty-five phases have decided things about one photograph, and then - in phase 25 - about one
wedding. This phase decides about **one camera body**, which is a third kind of subject and the
first that is not a set of photographs at all: it is a property of a *device*, inferred from the
photographs it took.

Section 1 states the commercial case in one line - "your second shooter's files will finally match
yours" - and the technical formulation in another, which is the important one:

> Matching *appearance* is the correct formulation: the goal is that skin, whites and blacks agree,
> not that the sliders agree.

Everything below follows from taking that literally. Two bodies given identical white balance and
exposure produce different photographs, because a camera's colour response is a function of its
sensor, its colour filter array and its manufacturer's rendering. Matching the sliders is what a
photographer does by hand and is why it takes hours: they are adjusting one thing and judging
another.

Three properties make this phase harder than phase 25, whose arithmetic it superficially resembles.

**The evidence is scarce and unevenly distributed.** Phase 25 has every frame of a chapter to
measure a node's target from. This phase has only the moments where *two bodies photographed the
same thing under the same light*, and a wedding where the second shooter worked a different room all
afternoon supplies none at all.

**The thing being corrected is entangled with the thing being preserved.** A second shooter's files
differ from the primary's because of the camera *and* because of how that person exposes. Section
6.3 asks for the habit to be corrected too, and the failure mode of over-correcting is that a
photographer who was hired for their eye is edited into somebody else.

**The correction is applied before phase 25 runs.** Section 6.4 makes camera transforms precede
within-scene normalisation, which means an error here is an error phase 25 then normalises a whole
gallery toward.

## 2. Decision: the objective is an appearance distance, and every term is a measurement of the frame rather than of the recipe

`transform::appearance_distance` is section 6.2's weighted sum, unchanged in its weights:

```text
3.0 * skin_dE00 + 1.5 * white_point_distance + 1.0 * grade_signature_distance + 0.5 * contrast_distance
```

Every one of the four is measured on what a frame *looks like*, not on the parameters that produced
it. The grade-signature term reuses phase 25's eight-number descriptor and its distance function
rather than defining a second one - `stats::GradeSignature` was built in phase 25 to answer "do these
two frames read as one look", which is exactly the question here asked across bodies instead of
across a chapter.

Two alternatives were rejected.

*Minimise the difference between the two cameras' solved white balances.* This is matching
parameters, which the phase document explicitly rules out, and it is wrong in a way that shows: two
bodies can solve to the same 5,200 K and render skin two dE00 apart, because the temperature is the
answer to "what light was this" and the rendering is the answer to "what does this sensor do with
it".

*Minimise a perceptual distance over whole rendered frames.* Rejected because it would need the
renderer on the critical path of a solver that runs a coordinate descent, and because two frames of
the same scene from two positions differ in composition far more than in colour - the metric would
be dominated by parallax.

## 3. Decision: the three per-channel gains are derived from the fingerprints, not fitted

Section 5's `CameraTransform` carries `channel_gain: [f32; 3]` and section 6.2 asks for a bounded
least squares over the whole transform vector. Fitting the gains is what the phase document implies
and it is **not identifiable from the evidence this phase has**.

The reason is worth stating exactly, because it is the kind of thing that produces a solver which
converges, reports a small residual and is meaningless. A matched pair supplies a handful of
aggregate statistics: a skin chromaticity, a white point, a grade signature, a contrast reading.
None of those separates "the red channel is 3 % hot" from "the green channel is 3 % cold" - both
produce the same chromaticity shift, and the appearance distance cannot tell them apart. A least
squares over ten parameters with an eight-dimensional observation has a two-dimensional null space,
and a solver run inside it returns whichever point the initial conditions happened to be nearest.

So the gains are **derived**: `transform::gains_from` computes them in closed form as the ratio
between the two fingerprints' white points, expressed per channel. That is a defined quantity with
one answer, and the coordinate descent runs over the seven parameters that *are* identifiable.

This is the same defect phase 17 found and recorded - "the regression's slopes are fitted and then
discarded" - reached from the other direction: there, a fitted quantity was not identified at the
point it would be applied; here it is not identified at all.

## 4. Decision: a pair is verified on its backgrounds, and a rejected pair is written

Section 6.1's "verify by comparing background statistics rather than subjects" is the single
sharpest sentence in the phase document, and it is right for a reason worth writing down: **two
frames of the same face from two bodies differ in exactly the way this phase exists to measure.**
Scoring a candidate pair on how alike its subjects look would be scoring the thing under test, and
would reject precisely the pairs that carry the most information.

So `pairs::verify` compares the frames' background statistics - the histogram away from the salient
region, the luminance distribution, the edge character - and a pair whose backgrounds disagree is
two cameras pointing at two different things, whatever their subjects scored.

**A rejected pair is written rather than dropped.** Phase 17 established this for its rejected style
pairs and the argument is the same one: here the rejection *is* the evidence a photographer needs
when they ask why their second camera was matched from a brand baseline in a wedding both cameras
shot all day. `camera_pair.verified = 0` is a row.

## 5. Decision: evidence blending is proportional and continuous, and a held-out set decides

Section 6.1 asks for a minimum pair count "default 12" and blending below it. What ships is a
continuous blend rather than a threshold with a cliff:

```text
weight = pairs / (pairs + EVIDENCE_HALF)
```

A body with 12 pairs is weighted toward its solved transform and away from its brand baseline in the
same proportion as one with 11 or 13; there is no count at which the answer jumps. A threshold would
make two neighbouring weddings - one with 11 pairs and one with 12 - receive materially different
corrections for no reason a photographer could see.

**Held-out verification is what decides whether the solve is used at all.** `solve::fit` splits the
verified pairs deterministically, fits on one part and measures on the other, and a transform that
does not improve the appearance distance on evidence it never saw is **discarded** in favour of the
baseline, with `CameraCode::HeldOutRegressed` on the row. Section 6.2 asks for this and it is the
one defence against the failure section 12 names third: a bounded solver over seven parameters and
fourteen pairs can still fit noise.

The split is deterministic - by pair id, not by a shuffle - because invariant 4 requires the same
wedding to produce the same transform, and a random split makes a body's correction depend on a
seed.

## 6. Decision: flash and ambient are two populations and never one

Section 2.1 asks for separate transforms and section 6.1 says why: brand differences are amplified
under flash. `FlashState` is on the fingerprint, on the transform, on the pair and in the primary
key of all three.

**A pair is never formed across the boundary.** Two frames four seconds apart in the same node, one
flash-lit and one ambient, were shot under two different lights - which is the same argument phase 25
makes for its change points, applied to a property the camera records rather than one the product
has to detect.

The consequence is that a body which used flash for a quarter of a wedding needs enough pairs in
*both* populations, and usually has them in only one. That body gets a solved ambient transform and
a baseline-blended flash transform, and the report says so per state rather than per body.

## 7. Decision: a shooter's habit is corrected by less than the whole difference, always

Section 6.3 asks for a per-shooter exposure bias correction "capped so a deliberately moodier second
shooter is harmonised, not erased".

`shooter::correct` measures the median subject-luminance offset per scene class and applies
`SHOOTER_HARMONY` of it - strictly less than one, and the type cannot express one. There is no
configuration that fully removes a shooter's exposure habit, in the same way that phase 21's
ceilings cannot be raised by a studio.

The argument is not symmetric with the camera correction and should not be. A camera's colour
response is not a decision anybody made; a shooter's exposure is. Correcting the first completely is
what the feature is for, and correcting the second completely edits a person out of their own work.

## 8. Decision: camera transforms are folded into phase 25's frames, not applied beside them

Section 6.4 requires camera transforms to precede within-scene normalisation. That could have been a
convention, an ordering in a job graph, or a test. It is none of those: `api::field_for` returns a
correction that `aura_brain_gallery::api::collect_frames` **adds to a frame's tone values before
phase 25's tree is built**.

So the ordering is a data dependency rather than a rule. Phase 25's solver reads a frame whose
temperature already carries its camera's correction, and there is no code path that could run them
the other way round because the other way round has nothing to read.

The gate asserts the observable consequence - a frame's temperature entering phase 25 differs from
phase 15's stored value by exactly the camera transform - which is a statement about numbers rather
than about call order.

## 9. Decision: the bundled baselines are fabricated, declared, and can only be replaced

Section 8 step 1 asks COL to "measure bundled brand baselines in controlled conditions". There is no
lab, no ColorChecker and no camera in this repository - phase 02's conditions C1 and C2, still open.

So all eight `assets/camera_baselines/<brand>.toml` files carry `measured = false`, and that field is
read rather than decorative: `BaselineLibrary::load` refuses a file claiming `measured = true`
without a `measured_by` and a `measured_at`, and the panel and the report both say which of the two a
correction came from.

**An unknown manufacturer changes nothing rather than guessing.** `Brand::Unknown` composes to the
identity transform at zero confidence. A body whose make the catalog does not recognise is left
alone, which is the same call phase 24 made about an unclassified distraction: the honest answer to
"I do not know what this is" is to do nothing.

The first *measured* baseline reopens this phase's criteria whatever phase is in flight, exactly as
the first real camera file reopens phase 02's and the first measured lens profile reopens phase 23's.

## 10. Consequences

`CameraMatchService` is the twenty-second frozen service. Phase 27 reads a camera transform when it
explains why a frame looks different from its neighbours, phase 28 acts on one unattended, and phase
30's delivery report lists which bodies were matched from evidence and which from a baseline. No
phase may keep its own camera fingerprint or its own idea of what two bodies agreeing means.

`PairId` is added to `ids.rs`, the fifteenth typed id and the fourth that names a part of something
rather than a whole thing, after `MaskId`, `ProposalId` and `NodeId`. A pair is a *relationship*
between two photographs rather than a thing either of them owns, which is why neither photograph's
id can name it.

Two version columns, matching phase 25's: `analysis_ver` for the arithmetic and `policy_ver` for
`camera_match.toml`. There is no `model_ver` because this phase ships no model - the sixth since
phase 08. `AURA-ML-5131` is the version-drift code, the eleventh of its kind.

The camera module lives **inside `aura-brain-gallery`** rather than in a crate of its own. Section 4
puts it at `crates/aura-brain-gallery/src/camera/`, and the reason to keep it there rather than split
it out is the appearance distance: it is defined over phase 25's grade signature, and two crates
would mean either a circular dependency or a second copy of the descriptor that decides whether two
frames read as one look.
