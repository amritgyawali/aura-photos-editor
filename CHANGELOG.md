# Changelog

All notable changes to AURA. One entry per phase, newest first.

## Phase 26 - Camera and shooter matching: appearance, not sliders

Twenty-five phases decided about one photograph, then about one wedding. This one decides about a
**camera body** - a property of a device, inferred from the photographs it took - and about the
person holding it.

**The objective is an appearance distance and every term measures a frame rather than a recipe.**
`3.0 * skin_dE00 + 1.5 * white_point + 1.0 * grade_signature + 0.5 * contrast`, section 6.2's own
weights, over background-verified matched pairs. Matching the *sliders* is what a photographer does
by hand and is why it takes hours: they adjust one thing and judge another. Two bodies can solve to
the same 5,200 K and render skin two dE00 apart, because the temperature answers "what light was
this" and the rendering answers "what does this sensor do with it".

**The three per-channel gains are derived, not fitted, and that is a correction rather than a
shortcut.** Section 5 carries `channel_gain: [f32; 3]` and section 6.2 asks for a bounded least
squares over the whole vector. A matched pair supplies a chromaticity, a white point, a grade
signature and a contrast reading - **none of which separates "the red channel is 3 % hot" from "the
green channel is 3 % cold"**. A least squares over ten parameters with an eight-dimensional
observation has a two-dimensional null space, and a solver run inside it returns whichever point its
initial conditions were nearest: it converges, reports a small residual, and means nothing. The gains
come from the two fingerprints' white points in closed form, and the descent runs over the seven
parameters that are identifiable. Phase 17's defect reached from the other direction.

**A pair is verified on its backgrounds.** Section 6.1's sharpest sentence, and it is right because
two frames of the same face from two bodies differ in exactly the way this phase exists to measure -
scoring a candidate on how alike its subjects look would be scoring the thing under test, and would
reject precisely the pairs that carry the most information. **A rejected pair is written rather than
dropped**, phase 17's rule in its second application: here the rejection is the evidence a
photographer needs when they ask why their second camera was matched from a brand baseline in a
wedding both cameras shot all day.

**Evidence blending is continuous and a held-out set decides.** `weight = pairs / (pairs + half)`
rather than section 6.1's threshold at twelve, because a cliff makes two neighbouring weddings with
eleven and twelve pairs receive materially different corrections for no reason a photographer could
see. A transform that does not improve the appearance distance on pairs it never saw is **discarded**
in favour of the baseline, with `HeldOutRegressed` on the row. The split is by pair id rather than by
a shuffle: invariant 4 needs the same wedding to produce the same transform, and a random split makes
a body's correction depend on a seed.

**A shooter's habit is corrected by less than the whole of it, and the type cannot express one.**
`SHOOTER_HARMONY` is strictly below one and there is no configuration that removes a habit entirely.
The asymmetry with the camera correction is deliberate: a sensor's colour response is not a decision
anybody made and correcting it completely is what the feature is for, while a person's exposure is,
and correcting that completely edits them out of their own work.

**The ordering is a data dependency, not a convention.** Section 6.4 puts camera transforms before
phase 25's within-scene normalisation. `api::field_for` returns a correction that
`collect_frames` **adds to a frame's tone values before phase 25 builds its tree**, so there is no
code path that could run them the other way round - the other way round has nothing to read. The
gate asserts the observable consequence: a frame entering phase 25 at 5,442 K where phase 15 stored
5,200 K.

**Flash and ambient are two populations and a pair never crosses the boundary.** Two frames four
seconds apart in the same node, one flash-lit and one ambient, were shot under two different lights -
phase 25's change-point argument applied to a property the camera records rather than one the product
has to detect.

Also here: `CameraMatchService`, the twenty-second frozen service and the first whose subject is a
device; `PairId`, the fifteenth typed id and the first that names a *relationship* rather than a
thing either photograph owns; migration 26 with five tables, two views and three triggers at 57 B per
image over a thousand frames - **bounded rather than proportional**, which is a first; eight bundled
brand baselines with `measured = false`; eleven IPC commands (ADR-0054) feeding a camera match panel;
and `aura-cli verify --phase 26` as the executable gate.

### What the storage measurement corrected

The budget note said the pair table grew with the square of a wedding's overlap. It was written
before the measurement and was wrong about the **shape** rather than the size: `pairs::find`
truncates at `MAX_PAIRS_PER_CAMERA`, so a two-body wedding stores the same number of pairs at 200
frames and at 4,000. Measured 57,724 B over a thousand photographs and 57,729 B over two thousand,
and the test now asserts that rather than only the size - a size assertion alone would pass on a
build that had quietly removed the cap and happened to be measured on a small fixture. Phase 21's
rule covers the sentence as much as the number.

### What this build cannot claim

**Every number came from a synthetic wedding whose per-brand colour response was authored.** There
are no multi-camera weddings in this repository. Condition C1, Sev 2.

**All eight bundled baselines were fabricated.** Every one carries `measured = false`, no camera was
measured, and the fallback path is proved to run and to report itself honestly while nothing is
proved about the numbers it falls back on. The first measured baseline reopens this phase's criteria
whatever phase is in flight, exactly as the first real camera file reopens phase 02's and the first
measured lens profile reopens phase 23's. Condition C2, Sev 2.

**The skin term of the appearance distance is unmeasured rather than met.** Phase 25's
`SKIN_FIELD_AVAILABLE` is false, so no photograph in this build carries an identity-scoped skin
region. Condition C3, and the report says so in a sentence rather than printing a zero.

**Section 9's blind study did not happen** - can a photographer pick out the second camera after
matching - so the phase's own headline acceptance criterion is unmeasured. Condition C4.

## Phase 25 - Gallery intelligence: a wedding matched to itself

Twenty-four phases decided things about **one photograph at a time**. This one decides about a
wedding, and that is a different problem rather than a bigger version of the same one.

Phase 15 can be inside its own 200 K tolerance on every frame of a ceremony and still produce a
ceremony that visibly warms and cools as somebody scrolls, because 200 K of independent error either
side of a mean is a 400 K swing between two adjacent frames. **Every per-frame gate in this product
can be green while the thing a client actually looks at is wrong.**

**The delta is measured from the un-normalised world, and that is what makes it idempotent.**
`normalise::solve` reads phase 15's `ToneEstimate` and phase 16's `ColourDecision` and never reads a
`NormalisationDelta`. Its input is immutable with respect to its own output, so a second run
computes the same number and writing it again is a no-op. Not achieved by detecting a second run and
not by convergence - a solver that iterated to a fixed point would converge toward the *mean* of the
node, which is the mediocrity section 6.1 exists to avoid, at a rate that depends on floating-point
ordering. The gate is a regression guard; the purity is the mechanism.

**Anchors, not averages.** An average over a ceremony includes the ceremony's mistakes at their true
weight: if a quarter of the frames are half a stop dark, the average is an eighth of a stop dark and
matching everything to it makes the other three quarters worse. Three to five anchors per node,
ranked by a *product* of four terms so no signal rescues another, with a trimmed mean for the
scalars and a component-wise median for anything chromatic - because trimming `u'` and `v'`
independently can produce a point no anchor was anywhere near.

**A change point splits a node before its anchors are chosen.** Section 2.1's candle-lit vow inside
a bright ceremony has exactly two outcomes if it shares a group with the ceremony - the vow is
flattened, or the ceremony is dragged toward it - and no damping factor avoids both; damping makes
both happen a little. So the split runs first, and each side gets its own target. Solving first and
un-moving the frames that went furthest was rejected for the reason phase 23 rejected nudging a
failed crop back inside the safety filter: a correction applied afterwards leaves the frames that
did *not* trip the threshold still normalised toward the wrong target.

**Damping first, bound second**, and the wrong order is subtle: bounding first would make the bound
a *target* rather than a limit, so every distant frame would land at `damping * bound` exactly and a
gallery would grow a visible band of identically-corrected frames at the edge of every transition.

**A clamped frame is less confident, not more.** The instinct is that a big correction is a
confident one; it is the opposite. The bound bit because the frame and the node disagree about what
room they are in, and the likely explanations are a missed change point or a bad anchor - so
`bounded_by` lowers the delta's confidence and makes the frame a candidate for the outlier queue on
the same evidence.

**A node the product could not judge is a different row from one it judged and left alone.**
`NodeUnanchored` and `AlreadyConsistent` both produce five zeroes and mean opposite things. Phase
24's rule - an absent input is ignorance, not permission - in the phase where the two are easiest to
confuse, because both look like nothing happened.

**The skin promise is measured, not asserted.** `gallery_skin_target.spread_after` is a stored
column, so "the same person's skin varies by no more than 2.0 dE00 across the gallery" is
`SELECT MAX(spread_after)` rather than a sentence. The target is that person's own frames; there is
no ideal-skin constant in the contract, the config, the migration or the code, the phase gate scans
the schema for one on every run, and `tests/no_recipe_writes.rs` scans the source.

**Outliers are measured on the residual, never on the raw deviation.** A frame 900 K from its node
that the bound could only move 450 K is an outlier; a frame 300 K away that was corrected in full is
not, even though its raw deviation was larger. Getting this backwards produces a QC queue full of
frames the product already fixed, which is the fastest way to make a photographer stop opening it.

Also here: `GalleryService`, the twenty-first frozen service and the first whose subject is a *set*
of photographs; `NodeId`, the fourteenth typed id; migration 25 with five tables, two views and
three triggers at 330 B per image against a 500 B budget; twenty-three argued-over scene rows in
`consistency.toml`; nine IPC commands (ADR-0052) feeding a consistency view, before-and-after
timeline strips, an anchor picker and an outlier list; `ml/eval/consistency_eval.py` for the catalog
side; and `aura-cli verify --phase 25` as the executable gate.

### Two things this phase got wrong first

**A change-point statistic with a trend in it splits the drift it exists to normalise.** The obvious
statistic divides the difference between two runs' robust means by the spread *within* the runs. On
a flash that works. On a slow drift it also fires, because a chapter that warms 500 K over forty
frames has a tiny frame-to-frame spread and a large difference between its halves - which is the
definition of drift, and drift is what this whole phase exists to remove. The first implementation
cut a forty-frame ceremony into six unanchorable nodes and reported six lighting changes. The
divisor is the **trend** now - the median adjacent-frame difference times the distance between the
two runs' midpoints - so a pure ramp scores about one and a flash scores about forty. Phase 22's
rule, in its second half: a threshold on a measurement is a statement about the instrument as well
as about the world, and the instrument had a slope in it.

And the first fix was half right: the divisor used the *shorter* run's length, so a six-frame head
against a thirty-four-frame tail was divided by three when its medians were twenty frames apart, and
a smooth ramp scored six. It is the distance between the midpoints.

**A spread-reduction gate is only meetable while the spread is inside the bound.** Section 10.1 asks
for the exposure spread to halve. A within-node drift of a full stop cannot halve, because the bound
is 0.35 EV and the arithmetic does not care what the gate says - fifty-three per cent of the frames
clamp. That is not a solver failure and it is not a threshold to lower: it is a *fixture* that
authored a lighting change and called it drift. The gate now measures a realistic third-of-a-stop
drift, and a second test asserts that a wider one is **reported as outliers** rather than silently
half-corrected. Same family as phase 19's edge-gradient halo test, phase 21's chance-corrected
margin and phase 22's sharpening kernel floor.

### What this build cannot claim

**Every gate is measured on synthetic galleries whose drift was authored.** There are no weddings in
this repository and no labelled lighting transitions, so what is proved is the tree, the
change-point detector, the anchor ranking, the robust statistics, the solver, the bounds, the
idempotence, the skin arithmetic and the outlier threshold - the algorithms. That is condition C1
and a Sev 2 trigger, and it closes with phase 05's C10 rather than separately, because the anchor
ranking reads phase 15's white-balance confidence and phase 06's face detector.

**`SKIN_FIELD_AVAILABLE` is false.** Phase 18's segmentation head is untrained, so no photograph in
this build has an identity-scoped skin region, every frame records `SkinMaskAbsent`, and section
6.3's promise ran on authored readings. It is a measurement of the mechanism on five wanderings of a
chromaticity, not on five people. Condition C2, and the panel says so in a sentence rather than
showing a zero that reads as "no problems found".

**No photographer has looked at a before-and-after gallery from this build.** Section 9's QAIQ audit
of five weddings did not happen, so the phase's own headline - that a wedding reads as one coherent
body of work - is unmeasured. Condition C3.

## Phase 24 - Generative cleanup: distraction removal, bounded by a safety engine

The first code in the product that removes something the camera got right. Phase 22 removed
noise, which is not information. Phase 23 removed framing. This removes an **object that was
there** and puts pixels in its place that were not, and when it is wrong the result is not a
photograph edited differently from the one somebody wanted - it is a photograph containing
something that never existed, delivered to a couple who will keep it for fifty years.

So the crate is shaped upside down compared with every phase before it. The usual shape is find
candidates, score them, apply the best. Here it is find candidates, **prove each one is safe**,
discard the ones that cannot be proved, and only then score what is left. `safety::check` was the
first module written and everything else is downstream of it. The ordering is not a convention:
`source::select` takes a `SafeCandidate`, which has no public constructor and can only be obtained
from the safety engine returning `Allowed`, so a caller who wanted to fill an unchecked region
could not construct the argument.

**A penalty would have been simpler and is what most products ship.** Any penalty large enough to
be safe across four hundred frames loses on the one frame where the salience term is most
confident - and the frame where a distraction is most salient is the frame where it is nearest the
subject. The 1 % that design gets wrong is a bride's hands.

**Real pixels first, then this photograph's own texture, then nothing.** `CleanupMethod::preference`
is borrow 0, fill 1, inpaint 2, and nothing configurable reorders it: diffusion is faster than a
homography search across a moment, so a studio that could reorder them would do it for the reason
that makes it worst. The borrow fits an exhaustive least-median homography over 495 four-subsets -
exhaustive rather than sampled because invariant 4 needs the same recipe to produce the same
pixels, and at twelve control points that is cheaper than seeding a generator.

**The diffusion tier is declared, refused and reachable.** `inpaint::solve` returns
`InpaintUnavailable` on every call and there is no fallback under it, because the fallback would be
the classical fill the selector already tried - so it would be the product doing what it had just
decided was insufficient, and then writing `method = inpaint` on the row. A stored disclosure saying
a model ran means a model ran.

**The cloud call can only say no.** `CleanupJudgement` is the first task in the product whose output
type has no approving variant: it can turn a proposed removal into a refusal, and it cannot raise a
confidence, move a band or reach a candidate that failed a mechanical check. An unreachable
provider, an invalid response, a spent budget and a cautious model all leave the photograph in the
same state. Phase 12 declined to build its tie-breaker and wrote down why; this one is built, and
the difference is the direction it can move a decision.

**A disclosure is written in the same transaction as the removal and can never be edited.** Three
triggers in migration 24: one aborts `applied = 1` with no disclosure, one aborts every UPDATE on a
disclosure, one refuses to delete one while the removal stands. `Recipe.cleanup[]` is the other half
- the fourth amendment to a frozen contract in the product's history - because phase 14's rule is
that a delivered file is re-creatable from four values, and a removal that appeared in none of them
is a photograph that cannot be audited.

**This build proposes no removals on a real photograph**, for two independent reasons, and both are
in the outline rather than hidden. There is no trained detector, so `detect::candidates` names
nothing and everything it finds is `Unclassified`, which cannot be shown to be extraneous. And phase
18's twenty mask classes contain **no word for a ring or a cake**, so a coverage assembled from them
is never complete and every candidate is refused with `protection_unknown` - which is the second
finding of this phase and the one that survives a trained segmenter.

Six things were got wrong first and are written down in `docs/progress/PHASE-24.md`. Two are worth
repeating here. **A normalised correlation is undefined over a flat window and returns zero**, and
most wedding distractions are close to flat - so the check that refused a sibling frame containing
the *same object* read "identical" as "completely different" and borrowed anyway, replacing the exit
sign with the exit sign. And **both removal modules feathered toward the object they were
removing**: the seam feather ran inward from the region's boundary, so the code that exists to hide
a seam left a rim of the distraction behind. That is phase 18's resampler defect in a different
module, and `pixels::feather_out` is where it is written down.

Sixteen of thirty-one reason codes are refusals, which is the highest proportion in the product and
is the phase working rather than failing. `aura-cli verify --phase 24` makes three hundred
adversarial attempts to get a removal past the engine and none succeeds; the exit report records
that the *human* audit section 9 asks for did not happen, which is condition C4.

- **Added:** `aura-generative` (fifteen modules), migration 24, `Stage::Cleanup` at index 18,
  `cleanup_paste.wgsl`, `CleanupJudgement`, `Recipe.cleanup[]`, `ProposalId`, nine IPC commands,
  three panels, three ML scripts, ADR-0049 and ADR-0050, error codes ML 5115-5122.
- **Changed:** `contracts.lock` re-locked for `cleanup.rs`, `ids.rs`, `recipe.rs`, `render.rs` and
  `types.ts`; `Capabilities` gains `cleanup_patches`; `SkipReason` gains `CleanupPatchAbsent`;
  `denylist::Coverage` gains a resolved set, so an unaskable kind is `Unknown` rather than `Clear`.
- **Not built:** a trained detector, a diffusion tier, a creative-fill mode, and any way to reorder
  the three sources.

## Tooling - the phase ritual becomes branch-first, and landing becomes one command

Not a phase. A change to how every phase after phase 24 is started and finished, recorded
here because it changes the shape of the repository's history rather than any of its code.

**Step 0 is new.** A phase now cuts its branch and pushes it to `origin` *before* the
kickoff, the ADR and the first line of code - `scripts/phase-branch.sh NN <slug>`. The rule
it replaces said "commit and push at the end of the phase", and the consequence was that a
month of decisions, a migration, a frozen contract and an exit report lived on exactly one
disk for as long as the phase took. An empty branch costs one round trip and buys a name
everybody can see, a place for the pull request to hang off from the first commit rather
than the last, and a commit to bisect back to.

**Step 9 now reaches `main`.** `scripts/phase-land.sh` commits what is left, pushes, opens
the pull request over the GitHub REST API, reads the checks, merges into `main` and leaves
the checkout on an up-to-date `main`. A phase used to be finished when it was pushed; it is
finished now when `main` carries it. `just phase-start` and `just phase-ship` are the two
commands with less to type, and `docs/runbooks/phase-landing.md` is the runbook.

Three properties of the tooling are deliberate. **It refuses to merge on a failed check**,
and `--ignore-check NAME` excuses one named job while `--force-merge` excuses all of them -
`benchmarks` has been red on `main` since the render backend was waived, and naming it is a
much narrower statement than waving everything through. **It never force-pushes**, in any
mode: if `origin` has moved, that is somebody else's work and a landing script is not where
it is decided what happens to it. And **it never runs the phase gate** - the gate is step 7
and has exited 0 before landing starts; running the suite again here would double a phase's
slowest hour and hide which of the two runs was the one that counted.

`gh` is not required. The token comes from `GH_TOKEN`, `GITHUB_TOKEN`, `gh auth token` or
the OS credential manager, in that order, and is never printed, never passed as a
command-line argument and never written anywhere that outlives the call.

## Phase 23 - Geometry Suite (lens corrections, straightening, smart crop)

> **Ordering note.** This phase was written on top of phase 19 and reached `main` before
> phases 20, 21 and 22, which were written in order on a branch of their own. They are now
> merged behind it, renumbered onto the migration, the error codes and the ADR numbers this
> phase had already claimed. Two sentences below were true on the day this entry was written
> and are not true of the shipped product: geometry is not the first phase that takes
> something away from a photograph - phase 20 removes a blemish, phase 21 removes a flyaway
> and phase 22 removes noise - and it is the nineteenth service rather than the sixteenth.
> What is still true, and is the point the sentences were making, is that a crop is the only
> one of those that removes the *evidence* that anything was removed.

The first phase in the product that takes away part of the frame itself. Every phase before
it has decided what is delivered, what it is of, whether it worked, how it should look, how
light moves inside it and what to repair; none of them changes where the edges are.

That asymmetry runs through every decision here. A wrong exposure looks wrong on the
screen it was decided on. A frame with somebody's hand missing from the edge looks like a
frame, until it is printed.

So the headline behaviour of this phase is **restraint**: seven photographs in ten are
delivered exactly as they were shot, fourteen of the twenty-three kinds of wedding
photograph AURA recognises are never cropped at all, and eleven of the twenty-four reason
codes describe something the product declined to do. The Geometry panel renders those
*first*, because a panel that reads as empty on most photographs is a panel nobody opens.

### Added

- **`aura-core::contract::geometry`**: the frozen `GeometryPlan`, `LensCorrection`,
  `Keystone`, `CropVariant`, `CropPurpose`, `Aspect`, `CropSafetyReport`,
  `ProtectedRegion`, twenty-four reason codes, `GeometryOutline`, `GeometryOverride` and
  `GeometryService`. **The frame as you shot it is index zero of every plan**, inserted by
  the only constructor - so "original framing is always one click away" is the shape
  rather than a button somebody has to maintain.
- **`aura-geometry`**: fourteen modules. Three routes to a lens correction, an edge
  tracker and a filtered fit for lenses nobody has profiled, a straightening solve that
  reduces the angle rather than cropping into somebody, a keystone that is refused past
  its cap instead of halved, a hard safety filter that runs *before* the composition
  objective, a bounded crop search, and the aspect variants.
- **Migration 20**: `geometry_plan`, `geometry_crop` and `v_geometry_coverage`. 839 bytes
  per photograph, measured.
- **`crop_rules.toml`**: 23 scene rows with a written reason each. The loader may only
  make a safety rule stricter, never looser.
- **`assets/lens_profiles/`**: the bundled table, with attribution required on every row.
- **`aura_raw::colour::lens`**: the optics transform, in the lowest crate the decision and
  the renderer both reach.
- **`geometry.wgsl`** and `aura-render::geometry`: the corrections applied, in linear
  light, before the creative operations, once.
- **Six IPC commands** (ADR-0042) and the Geometry panel.
- **`docs/geometry-and-cropping.md`**: what all of it means, in the product's own words.
- **`aura-cli verify --phase 23`**, 23 evaluation gates and four performance rows.

### Changed

- **`aura_recipe::Lens` carries its coefficients.** A frozen contract amended, recorded in
  ADR-0041 section 4. The tidier alternative - look the profile up at render time - fails
  phase 14's rule that a delivered file can be re-created from four values: a coefficient
  living only in a profile table is a fifth, and updating that table would silently change
  what an already-delivered photograph looks like.
- **`Capabilities::geometry_models` is true on the reference path**, so a perspective
  correction is applied rather than reported as absent.

### Fixed

- **An edge tracker that found nothing on a plate made of nothing but straight lines.** It
  died at every intersection, because the gradient *along* one edge collapses where
  another crosses it. A crossing is not an ending.
- **A robust fit that threw away exactly the evidence it needed.** Trimming the worst
  residuals keeps the chains nearest the optical centre - the ones that see no distortion
  at all - and discards the chains at the edge. What separates junk from signal is not the
  size of a residual but whether any coefficient removes it.
- **A one-pixel re-acquire window that flattened the curve at every crossing**, biasing
  the recovered lens coefficient low by about a sixth. Every chain agreed with every other
  chain about the wrong answer, which is what makes that class of bug survive review.
- **A straighten that reframed.** The largest inscribed rectangle changes shape with the
  angle, so levelling a 3:2 photograph by two degrees delivered 1.72:1. The frame keeps
  its own shape now.
- **An objective that preferred slicing a bright window in half to leaving it whole.**

### Known limitations

Four conditions in `docs/progress/PHASE-23-EXIT.md`, two of them Sev 2.

- **C1**: there are no wedding photographs and no expert crop labels here. Every gate
  measures a geometry that was painted into the pixels and read back. It proves the
  arithmetic and says nothing about whether a photographer would agree with a crop.
- **C2**: every bundled lens profile was fabricated. No lens was measured. The panel says
  so on every photograph they touch.
- **C3**: there is no pose estimate, so no crop in this build has ever been checked
  against a pair of hands. The mechanism is built and the set it runs over is empty.
- **C4**: the crop objective's weights and the improvement margin are authored rather than
  fitted, because fitting them needs C1's labels.
## Phase 22 - Restoration Stack (scene-aware denoise, selective sharpen, face recovery)

The first phase that repairs a photograph rather than deciding something about it, and the one
where a wrong answer removes information instead of changing an opinion.

Noise reduction is chosen from the noise that is actually in the frame, measured against what that
kind of photograph carries well, and conditioned on what the camera's own sensor does at that ISO -
so the same tier removes visibly different amounts on two bodies, and a frame with no noise in it
gets nothing however permissive its scene row is. After deciding, AURA renders the result and
measures how much fine texture it removed; lace that lost too much steps the tier down and is
measured again.

Sharpening refuses far more often than it runs, and all four of its preconditions are refusals
with reason codes rather than reduced amounts: the softness has to be recoverable rather than gross
or already at the sensor's limit, it has to be focus rather than movement, the focus has to have
landed on the subject, and phase 18 has to have said where the skin, the sky and the out-of-focus
background are. That last one has no fallback on purpose - those three regions are where sharpening
is visible *as damage* and almost nowhere else, so an unmasked global sharpen spends its whole
artefact budget on the three places a photographer looks first.

**The guarantee is that AURA stops rather than changes what somebody looks like.** A face is only
considered inside a narrow band of softness - never one too blurred for a prior to be constrained
by, because what comes back then is the prior. Inside the band, the recovery is rendered, the face
is embedded before and after, and if the person has started to measure as even slightly different
the strength drops and it renders again; still different, and **the face is put back the way it
was**. There is no fourth outcome and no setting that disables it. The measured distance is stored
on every face whether it passed or not, so "no delivered face was changed" is a query rather than
a sentence, and a database trigger aborts the update that would deliver one anyway.

Restoration never runs while you are editing. `RestoreWhen` has two variants and no interactive
one, the render graph refuses independently, and denoising is the exception only because it sits
at stage 6 with the rest of the tonal pipeline.

**Both shipped heads are untrained, and one ships as a refusal rather than as a measurement**: no
face in this build is recovered at all, because the substitute for a face prior would be unsharp
masking on a face - a different operation with the same name. **No camera noise model has been
measured**, so every body is capped below the strongest tier and named in the panel. **No frame in
this build is sharpened**, because no region reaches the pass. Conditions C1 to C7 of
`docs/progress/PHASE-22-EXIT.md`, and the expert preference study that would be this phase's
headline result is C4.

## Phase 21 - Micro-Retouch Suite (hair, teeth, eyes, clothing, glare)

The small fixes a retoucher makes without being asked, and the first phase in which AURA can put
pixels from one photograph into another.

Stray hairs are calmed rather than erased, and only where the background behind them is quiet
enough that a measurement can tell a strand from a twig. Teeth are evened toward their own
brighter half and moved a little way back toward a locus centred on the frame's own neutral -
never toward a colour, because there is no ideal-teeth constant anywhere in the code. Sclera
redness comes out as chroma only, iris definition goes up a little, and the catchlights are
excluded from both operations by construction rather than by a threshold applied afterwards. Lint,
threads and small stains come off a garment; a visible strap and a crease are opt-in per studio,
off by default, and refused by the database if anything tries to insert one anyway.

Glasses glare is the operation that composites. Where a specular sheet has destroyed the record -
more than half of it at or above the clipped floor - a sibling frame from the same moment may
repair that small region, aligned and blended. Where the record survived, it never may: a closed
eye *is* the record, and borrowing one is excluded permanently rather than deferred. **Every
borrow is disclosed in five places** - the operation, the plan, the project header, the composites
view and the delivery report - and a database trigger aborts any attempt to take a borrow's source
away.

The headline claim is three measurements rather than a promise. Every photograph carries what the
plan did to its catchlights, to its hair region's edge energy, and to how far its teeth sit from
the locus, all measured by running the plan through the real renderer. A family that misses its
floor is re-solved at three quarters strength up to three times and then **withdrawn** - per
family, so a frame whose teeth could not be evened safely still gets its lint removed.

A studio chooses which of these operations run. It cannot choose how far any of them goes: there
is no strength field on the wire, and the config file can lower a ceiling and never raise one.
`docs/retouch-ethics.md` is the list of what this product will not do, and every item on it is
enforced by there being nowhere to express it.

**All three shipped heads are untrained and none is consulted, phase 06 finds no faces, and no
region reaches this pass from phase 18** - so on this build nothing is micro-retouched on a real
photograph. That is condition C1 of `docs/progress/PHASE-21-EXIT.md`, and the naturalness audit
that would be this phase's headline result is condition C2.

## Phase 20 - Portrait Retouch AI (blemishes, protected features, texture protection)

The first phase that changes what a person's skin looks like, and the one with the least room
for error: a slightly over-strong grade is a photograph somebody adjusts, and a removed mole is a
photograph of somebody else.

Temporary marks are removed by borrowing skin from a couple of millimetres away - the same person
under the same light - and putting that skin's own texture back on top. Dark circles are lifted a
quarter of a stop at most, measured against the cheek beside them rather than against a target.
Blotchy patches are calmed by moving one band of detail while the band that holds the pores comes
back untouched.

Everything that is the person stays. Freckles, moles, birthmarks, scars and dimples are found,
listed with their evidence, and vetoed out of the candidate list before any strength is
consulted - a veto rather than a discount, because a partly inpainted mole is a smudged one.
**Tattoos can never be unprotected**, by any setting, and the refusal lives in the type, in the
service and in a database trigger.

The headline claim is a measurement rather than a promise. Every photograph carries the ratio of
its skin's fine-detail energy after the retouch to the same energy before it, measured by running
the plan through the real renderer. Below the preset's floor the strength is reduced and measured
again; if three attempts do not reach it, **the retouch is withdrawn and the frame ships
unretouched**.

**Both shipped heads are untrained and neither is consulted, phase 06 finds no faces, and no skin
mask reaches this pass** - so on this build nothing is retouched on a real photograph. That is
conditions C1 to C3 of `docs/progress/PHASE-20-EXIT.md`.

### Added

- **`aura-core::contract::retouch`**: the frozen `RetouchOp`, `RetouchPlan`, `ProtectedFeature`,
  `ProtectedKind` and its absolute member, `TextureReport`, `RetouchPreset`, `FreqBand` -
  which has no `High` variant, so no operator can name the band that holds the pores -
  twenty-six reason codes, `RetouchOutline`, `RetouchOverride` and `RetouchService`. There is no
  field anywhere in it for reshaping, lightening or swapping a face.
- **`aura-retouch`**: eleven modules. A measured detector that reads colour as well as luminance,
  cross-frame permanence in face-normalised coordinates, a capped under-eye correction,
  mid-band evening that cannot reach a pore, the texture guard with its re-solve and withdrawal,
  one gallery-constant strength per person, and the store.
- **`aura-render::bands` and `aura_render::retouch`**: the three-band separation, moved out of
  phase 19 for its second consumer, and the processor reference for the retouch stage. Three new
  WGSL files, and the phase 14 pass-through `stage_retouch` retired from `spatial.wgsl`.
- **Migration 21**: `retouch_plan`, `retouch_identity`, `retouch_protected`, `retouch_op` and
  `v_retouch_coverage`. The first table in this product whose rows a photographer creates
  directly and whose subject is a person, plus two triggers that abort any attempt to delete a
  protected tattoo.
- **Two signed models with cards** - `blemish_detector` and `permanent_features` - both untrained
  and neither consulted, and four Python scripts that self-test without PyTorch.
- **Eight IPC commands** (ADR-0044) and a retouch panel that shows what was left alone as
  prominently as what was done.
- **Six error codes** `AURA-ML-5096` to `5101`, with runbooks. One of them,
  `AURA-ML-5101`, is registered so the texture guard is *visible* when it fires rather than
  because anything is wrong.
- **`docs/retouch.md`**, the product's own account, including every one of the twenty-six reason
  sentences - which two gates assert.

### Changed

- `Capabilities::retouch_operators` still ships false: the operator exists and is tested, and
  nothing wires a phase 18 matte into the render graph yet.
- `perf/budgets.toml` gains `retouch_plan_frame` (57.6 ms per image measured, including at least
  one full render) and `retouch_store_per_1000_images` (659 B per image measured against 1,000).
- `contracts.lock` gains migration 21 and the three new shaders.

### Known gaps

- No per-skin-tone parity study and no blind expert comparison. Both are named in the eval
  harness so a missing gate cannot look like a passing one. Conditions C2 and C4.
- The desktop shell does not build in this repository - `ui/src-tauri` needs an icon that is not
  checked in - so the eight new commands are not compile-checked here. Condition C5, pre-existing.

## Phase 19 - Local Light Sculpting (face lighting, subject enhancement, dodge and burn)

The first phase that moves light *inside* a photograph rather than across all of it. Faces
under a mandap are lifted through their shadows so they do not glow, a window behind the
couple is brought down while the subject is lifted by the same amount so the frame is no
brighter overall, sheen on a forehead is reduced as brightness rather than blurred away, and
form is deepened the way a retoucher would without any operator in the product being able to
reach skin texture at all.

Its success condition is that none of that is visible, which is a hard thing to put in a
panel. So the Local panel goes the other way: it shows what each face was moved by **and what
stopped it**, shows an operation that could not run as *unavailable* rather than as off, and
tells a photographer when a group could not be evened out completely and that nobody was
darkened to close the gap.

**Phase 18 has not shipped, so on this build every operation is gated and nothing is edited.**
That is condition C1 of `docs/progress/PHASE-19-EXIT.md`, it is visible in the panel and in
`LocalOutline::mask_covered`, and it is not a fault to investigate.

### Added

- **`aura-core::contract::local`**: the frozen `LocalLightPlan`, `MaskField`, `LocalOp` and its
  priority order, `FaceZone`'s ten named moves, `FaceLightDelta`, `SubjectEnhanceDelta`,
  `BackgroundBalanceDelta`, `DodgeBurnMaps`, `ShineReduction`, thirty reason codes,
  `LocalOutline`, `LocalOverride` and `LocalService`. **There is no field anywhere in it that
  could hold image data**, which is what makes "all local work is reversible and inspectable"
  a property of the shape.
- **`aura-brain-photo::local`**: fifteen modules. The per-scene policy table, one measurement
  pass, the luminosity split that stops a lifted face glowing, a joint face solve across every
  face in a frame, the paired subject/background move with three measured triggers,
  three-band frequency separation whose finest band is never produced, zone-based dodge and
  burn, specular shine detection with a luminance-only reduction, and one per-image perceptual
  allowance that every operation spends against.
- **Migration 19**: `local_light_plan`, `local_light_face`, `local_light_gate` and
  `v_local_coverage`. There is no mask column, no matte and no blur, and the phase gate scans
  the schema for one on every run.
- **`local_light.toml`**: 22 scene rows with a written reason each. The loader refuses a row
  with no reason and a row that shapes form harder than it lights faces.
- **Three shaders and a processor reference**: `luminosity_mask.wgsl`, `freq_sep.wgsl` and
  `local_apply.wgsl` - the first shader *libraries* in the product - held to
  `aura_render::local` by six shared constants in `shader_parity.rs`.
- **Six IPC commands** (ADR-0040) and the Local panel. No command can return a mask.
- **`docs/local-light.md`**: what every one of the thirty notes means, in the product's own
  words, with the group-fairness guarantee stated as what it actually is.
- **`aura-cli verify --phase 19`**, 38 evaluation gates and two performance budgets.

### Fixed

- **A halo made by arithmetic that looked conservative.** `apply_face_light` evaluated its
  luminosity weights on the partially-edited pixel, so the highlight restraint grew
  quadratically in the matte while the lift grew linearly. Past about half coverage the
  restraint overtook, and a bright pixel received *more* lift at the mask's edge than at its
  centre - a bright rim. Both weights now read the input pixel and the whole edit is linear in
  the matte, on the processor path and in the shader.
- **A cap detector that could never fire.** The joint solve reported whether a lift had been
  capped by comparing against the group's converged target, which has already absorbed the
  caps. It now compares against the scene's band.
- **A joint solve that could brighten a face past the band.** One blown face dragged the common
  target above the scene's band and everybody else was lifted to meet it. Every move is now
  clamped to lie between the face and the band.
- **`0019_local_light.sql` is locked.** Phase 19 also found that `0015_tone.sql` was missing
  from the frozen contract list - a phase 15 oversight rather than a decision - but phase 16
  had already noticed and fixed the same thing on `main`, so only the new migration is added
  here.
- **CI had been red on `main` for five days, and the cause was a budget that assumed CI was
  fast.** Phase 14's proxy guardrail says it "leaves room for a slower CI machine" at 450 ms
  against a 210 ms development figure. It does not: three GitHub runners measured 497, 669 and
  1,123 ms. Timing budgets are now multiplied by `AURA_PERF_HOST_SCALE`, which CI sets to 4
  and a developer does not, so the tight assertion survives on the machine anybody develops on
  and CI asserts a looser but still real bound. **Sizes, counts and costs are never scaled** -
  a slow runner is not a reason to store more, call more or spend more - and the factor is
  clamped so a budget cannot be switched off from the environment.
- **That scale then broke the tests that assert what a budget *means*.** It was read inside
  `Budgets::check`, so the case proving 900 ms breaches a 400 ms budget stopped breaching the
  moment CI exported a scale of four. `check` is now a wrapper over `check_at_scale`, which
  takes the scale as an argument; the cases that assert the rule pin their own and nothing but
  `host_scale` reads the environment. A measurement's verdict depends on the host, the rule
  does not.
- **Phase 09's storage budget had been failing on packing drift.** Also red on `main`, and
  masked behind the guardrail above. It was measured with whole-file `PRAGMA page_count`,
  which quantises to 4 KiB, and then recorded as "1,024 B, met, **exactly**" - pinned with no
  headroom in a number that can only move in 4 KiB steps. It moved two pages and began failing
  on nothing anybody had written. The test now counts `dbstat` payload over migration 9's two
  tables and their indexes (927 B per image), and asserts the page overhead separately as a
  bounded ratio (1.11x measured, 1.40x ceiling) so a structural regression still fails. No
  schema changed and nothing was made cheaper: the same rows are counted with an instrument
  that moves by the bytes actually added. **A budget measured with a quantised instrument must
  not be set at its own measurement.**
- **Two CI steps re-ran a budget suite in parallel and measured the contention.** The step
  that runs the whole suite passes `--test-threads=1`, with a comment saying why - "a budget
  suite that races itself reports a different number every run" - and the two dedicated steps
  that re-run one suite each to print its figures did not. Five renders racing across a
  runner's cores read 737 ms per unit against a 532 ms allowance where the same machine read
  286 ms serially. Both steps now pass the flag the rule four lines above them already stated.

### Changed

- **The group-fairness guarantee is about the edit, not about the frame.** Section 10.1's
  absolute spread threshold is unachievable on a family formal where one person is two stops
  down under a doorway, and the two ways to satisfy it anyway - refuse to plan the frame, or
  darken everybody else - are both worse than the problem. What is guaranteed: reach the
  threshold whenever the caps allow, and never make a group less even than you found it.
  ADR-0039 section 6.
- **The shaping is stored as four numbers per face rather than as ten zones.** Every zone is a
  pure function of the face region, the light direction and the strength. This took the table
  from 2,236 to 1,064 bytes per image, and the panel still shows every zone by name because
  they are regenerated on read - which is why `shaping_ver` exists.

### Not done

- **This phase was written on top of phase 15, before 16, 17 and 18 existed**, and merged into
  them afterwards. It still reads phase 15's per-scene luminance bands rather than phase 16's
  refined ones and reads no phase 17 style profile (condition C4), and phase 18's `MaskService`
  is not wired into `LocalPass`, so every operation is still gated (condition C1). Its
  migration, error codes and ADRs were renumbered on merge - migration 19, `AURA-ML-5084` to
  `5089`, ADR-0039 and ADR-0040.
- **The expert subtlety study and the four-hundred-frame halo audit do not exist**, so the
  headline KPI of this phase is unmeasured. Condition C3.
- **The learned targets are untrained and never consulted**; there is no corpus of expert edits
  in this repository. Condition C2.

## Phase 18 - Local Mask AI: automatic semantic masking

Every photograph you keep is split into the twenty regions the rest of the product edits inside:
skin, face, eyes, teeth, hair, clothing, dress, subject, background, sky, greenery and the rest -
tied to the people in the frame, feathered at the edges, and editable by hand. Each region says
two things about itself, how sure AURA is what it *is* and how well it could find its *boundary*,
and those two numbers decide how far any later change through it may go.

### Added

- **`aura-vision::contract::mask`**: the frozen `Mask`, `MaskKind` (twenty), `Storage`,
  `MaskPayload`, `EdgeQuality`, `MaskReason`, `MaskOp`, `GpuMask`, `MaskOutline` and
  `MaskService`; `aura-core::contract::ids` gains `MaskId`. The contract lives in `aura-vision`
  rather than `aura-core` because `RenderLevel` is in the frozen `upload_gpu` signature and
  `aura-core` depends on no workspace crate - the precedent `SimilarityIndex` and `RenderService`
  set.
- **`aura-vision::mask`**: eleven modules. A twenty-class segmenter seeded by phase 06's faces
  and grown by colour through connected regions; a salient-subject pass bounded by the person
  boxes; a trimap whose band is a fraction of the region's own size; a guided-filter matte solved
  in closed form inside that band, which reports how much of the boundary the photograph could
  actually determine; identity scoping by *containment* rather than IoU; seven algebra operations;
  a quality gate; a run-length and quarter-resolution-alpha codec; the store; and the resumable
  lazy pass over the frames the cull kept.
- **Migration 18**: `masks`, `mask_gate` and `v_mask_coverage`. No column that could hold a
  photograph, no column that could hold a skin colour, and the gate scans for both on every run.
- **Two shaders**: `mask_upsample.wgsl` and `mask_composite.wgsl`. Nothing executes them - no
  `wgpu` backend is linked - and `shader_parity.rs` and `colour_discipline.rs` hold both to the
  reference so they cannot drift while no device can notice.
- **Six error codes**, `AURA-ML-5078` to `AURA-ML-5083`, with runbooks. `AURA-ML-5081` is the
  first code in the product that constrains what a *later* phase may do.
- **The mask IPC surface** (ADR-0038): eight commands, nine shapes, and no `apply_mask`. The
  overlay crosses the wire as a capped quarter-resolution alpha plane; there is no field on the
  surface that could hold a photograph.
- **`MaskPanel.tsx`**: two quality bars rather than one, a sentence naming which of the two is
  limiting, a feather slider that means the same softness at every zoom, refine edge, and reset
  to AURA's version.
- **Two signed models with cards** - `semantic_segment` and `alpha_matting` - and four Python
  scripts whose self-tests prove every metric can *fail*, including a halo measure that catches
  what mIoU averages away.
- **`docs/masks.md`**: what the regions are and what the two numbers mean, in the product's own
  voice.
- **`aura-cli verify --phase 18`** and `just phase-18-verify`, `just mask-report`.

### Changed

- **`aura-vision` gained `aura-catalog`.** Section 4 of the phase document puts the mask store in
  this crate, so phase 06's sentence - "this crate has no catalog dependency, so it *cannot*
  write a face template" - stopped being true. `crates/aura-vision/tests/no_template_writes.rs`
  replaces it, and the phase gate runs the same grep. Third grep-as-a-test in the repository.
- `crates/aura-render/src/shaders.rs`: `every_shader_declares_the_frame_uniform` widened from a
  literal `struct Frame` to "a uniform block carrying a width and a height", because the two mask
  shaders take two grids and a block called `Frame` would have had to mean one of them.
- `crates/aura-app/src/style_commands.rs`: phase 17's "this surface cannot return a pixel" grep
  is now bounded at the phase 18 marker. It scanned to the end of `ipc.rs` and would have failed
  on a mask overlay, which is derived geometry about a region rather than a photograph.

### Fixed

- **The mask resampler manufactured a halo.** `Plane::resize_bilinear` read zero outside the
  plane, darkening the outermost half-pixel of every upsampled region - a one-pixel dark rim
  around every mask at every render level, produced by the code that delivers a boundary rather
  than by the code that finds it. `Plane::at_clamped` is the fix.
- **`INSERT OR REPLACE` would have destroyed a hand-edited mask through a constraint.** The
  `DELETE ... WHERE user_edited = 0` was not enough on its own: `masks` has
  `UNIQUE (image_id, kind, identity_id)`, and an `INSERT OR REPLACE` deletes the row it conflicts
  with. `MaskStore::put` now reads the edited coordinates first and skips them entirely.

### Known limitations

- **Both shipped heads are placeholders and neither is consulted.** `SEG_HEAD_TRAINED` and
  `MATTING_HEAD_TRAINED` are `false`; regions are measured from the photograph rather than
  predicted. Every gate is measured on synthetic frames whose regions were painted into the
  pixels. Condition C1 of the phase 18 exit report, and a Sev 2 trigger.
- **The 100 % zoom artefact audit did not happen.** There are no photographs to audit; the halo
  metric exists and has never been run on data. Condition C2, and the criterion most likely to be
  wrong in a way nothing here would catch.
- **The 120 ms budget is not met**, because it is written against a GPU and no `wgpu` backend is
  linked. The storage budget - the failure mode section 12 names - is met with a factor of six in
  hand at 29 KB per frame against 180 KB. Condition C3.
- **The render graph still cannot evaluate a semantic mask on its own.**
  `SkipReason::MaskGeneratorAbsent` stays reachable; wiring the resolved planes into the graph is
  phase 19's first task and changes no shape frozen here. Condition C4.
- Clothing versus dress has no colorimetric signature: a red lehenga comes back as clothing, at a
  lower confidence than any other class, which is a region that works rather than one that lied.

## Phase 17 - Style Learning: scene-conditional personal AI profiles ("Teach My AI")

Point AURA at weddings you have already edited and it learns your look - not as one style,
but as a tree of eighty leaves, one per scene and kind of light. What it learns is a
*residual*: the difference between what phases 15 and 16 decided and what you actually did,
so an empty profile changes nothing and a taught one moves the answer rather than replacing
it. The report leads with a measured error per leaf and names the one wedding worth adding
next.

### Added

- **`aura-core::contract::style`**: the frozen `StyleProfile`, `StyleDelta`, `CurveShift`,
  `SkinBias`, `SceneGroup`, `LightingBucket`, `StyleBucket`, `BucketModel`,
  `ProfileDiagnostics`, `FallbackLevel`, `MatchMethod`, `ExtractSource`, `StylePair`,
  `StyleQuery`, `StyleAdvice`, `StyleOutline`, twenty reason codes and `StyleService`;
  `ids.rs` gains `ProfileId`. **There is no field anywhere in it for a skin colour**, for
  the third phase running.
- **`aura-style`**: thirteen modules. Four pair-matching strategies with a refusal for an
  ambiguous match; XMP parameters read exactly when they exist; a coordinate-descent fitter
  that reproduces a delivered JPEG over twelve parameters **through the real renderer** and
  rejects what it cannot explain; eighty-leaf bucketing; ridge regression with Huber
  reweighting, James-Stein shrinkage toward the parent and a cap on what one wedding may
  contribute; held-out diagnostics measured against the baseline as well as against the
  ceiling; hierarchical inference that always answers; versioning, adoption and a signed
  `.auraprofile` bundle; the store and the resumable pass.
- **Migration 17**: `profiles`, `profile_buckets`, `style_pairs`, `project_style` and
  `v_style_coverage`. No skin colour anywhere in it, and the two skin *lean* columns carry
  CHECKs below phase 16's own ceilings; the phase gate scans the schema for both on every run.
- **`aura-brain-photo::{tone,colour}::style`**: the shift lands on the **solved** parameters
  and before every guard, so phase 15's clipping bound and skin-locus constraint and phase
  16's clipping guard and skin guard all re-run on the styled answer. Both `ANALYSIS_VER`s
  1 -> 2.
- **Eleven IPC commands** (ADR-0036) and four panels: a Teach My AI wizard that shows what a
  folder contains before anything is fitted, a profile report that leads with a measurement
  rather than a ready state, a bucket matrix that distinguishes a taught leaf from a borrowed
  one, and an A/B comparison in numbers.
- **`docs/style-profiles.md`**: how style learning works, what it needs, and what the
  signature does and does not prove, in the product's own words.
- **`aura-cli verify --phase 17`**, 19 evaluation gates, three Python self-tests and
  `tests/no_network.rs` - a grep as a test that fails the build if this crate ever gains a
  way to reach a network.

### Fixed

- The archive cap scaled one wedding's weight by `cap / share`, which leaves it **above** the
  cap: shrinking a weight also shrinks the total it is a share of. The measured influence was
  48 % against a documented 35 %, which is the worst kind of defect - the guarantee reads
  correct and measures wrong. Now `w = cap * rest / (1 - cap)`, and `gate_5` is what found it.
- The regression's slopes were applied at inference. A slope fitted on eleven samples spanning
  ISO 1600 to 4000 is not identified at ISO 400, and the frame it would be applied to is
  exactly the ISO 400 one. The slopes now do the job they are good at - keeping a confound out
  of the intercept - and the intercept is what ships.
- `strength()` read an unevaluated profile's `overall_de00` of zero as an accuracy of zero
  rather than as an absence, so a profile trained ten seconds ago showed "nothing learned".
- `contracts.lock` carried a stale digest for `crates/aura-core/src/contract/colour.rs`, so
  `cargo xtask contracts --check` would have failed on `main`. Phase 16 re-locked before a
  final edit to the contract.
- The justfile had no `phase-16-verify` recipe, so the only way to run that gate was to
  remember the argument.

### Known limits

**There are no photographers' archives in this repository**, so no number in this phase is
about a photograph: every figure is measured on synthetic archives whose look was chosen,
applied through the real renderer and recovered. This is a different gap from every
placeholder-weights condition before it - this phase ships real code waiting for real
*weddings* rather than real *weights*, and the fit has a closed form, so nothing needs
training. The bundle signature proves integrity and not provenance, and the panel never says
"verified". The desktop shell has no archive-import flow yet, so `train_profile` refuses with
`AURA-ML-5073` rather than quietly succeeding, and both consuming passes resolve style at
`LightingBucket::Unknown` - which is recorded on every decision rather than hidden. See
`docs/progress/PHASE-17-EXIT.md` section 8.

## Phase 16 - Tone AI, adaptive curves, HSL AI and skin-tone protection

What a photograph should look like, decided per scene: the five tone parameters solved from
the histogram and the subject's own spread, a monotone curve fitted to the contrast this kind
of photograph wants, and the eight hue bands moved according to what is actually in the frame.
Then the promise that costs the most to keep - **the grade is rendered, the skin pixels are
measured, and the whole thing is solved again until nobody's colour has moved.**

### Added

- **`aura-core::contract::colour`**: the frozen `ColourDecision`, `ToneCurve`,
  `HslAdjustments`, `HslBand`, `SkinGuardReport`, `ColourVariant`, `ContentBand`,
  `BandReading`, 29 reason codes, `ColourOutline`, `ColourOverride` and `ColourService`.
  Monotonicity is structural: `ToneCurve::new` is the only constructor and it refuses a set of
  control points that is not monotone, so no solver, override or stored document can produce a
  posterised or inverted curve. **There is no field anywhere in it for an ideal skin colour.**
- **`aura-brain-photo::colour`**: thirteen modules. The tone solver, the curve fitter under
  three constraints, the content reader, the harmony objective, the HSL expression of it, a
  clipping guard, a subtlety cap, and the skin guard - which grades this frame's own skin
  **through the real renderer**, measures the hue and chroma it actually moved, and re-solves
  or withdraws until both are inside their ceilings.
- **Migration 16**: `image_colour_decision` and `v_colour_coverage`. No skin-target column and
  nowhere to put one; the gate scans both the schema and the config file on every run.
- **`tone_intent.toml`**: 22 argued-over scene rows with a written reason each.
- **One signed model with a card** (`tone_head`), and it is **never consulted** - see below.
- **Seven IPC commands** (ADR-0034), a Tone panel that reports the guarantee as a measurement,
  a curve editor that draws AURA's curve over the identity with the renderer's own
  interpolation, and an HSL panel with the protected-skin indicator.
- **`docs/tone-and-colour.md`**: what AURA changes about how a photograph looks, in the
  product's own words.
- **`aura-cli verify --phase 16`** and 27 evaluation gates, six of which exist to prove the
  harness can fail.

### Fixed

- Migrations 15 and 16 were both absent from `contracts.lock`. `docs/plan/CLAUDE.md` has listed
  every migration as a frozen contract since phase 01, and 15 had been omitted when it shipped.
- The curve fitter clamped a node that wanted to sit above white, which produced a flat top -
  a posterised band and new clipping in one move. It bounds its gain instead.

### Known limits

The tone head is an untrained placeholder and **is never consulted**: a random projection
blended at any weight is a random contribution at that weight, and it would be
indistinguishable in the panel from a learned one. What ships is a deterministic solver, and
every gate is measured on synthetic frames whose foliage hue, dress luminance and subject
contrast were painted in. The fairness gate measures five reflectances, not five people.
Generating the three alternatives costs about 3x against a 15 % budget, and the content bands
are inferred from colour statistics rather than segmented - which is on every adjusted frame as
`ContentInferred` and closes with phase 18. See `docs/progress/PHASE-16-EXIT.md` section 8.

## Phase 15 - Exposure AI and White Balance AI (mixed lighting mastery)

The first phase that decides what a photograph should look like. Exposure is set for the
faces in the frame rather than for its average brightness, colour is worked out from four
kinds of evidence at once and then checked against what each person's skin actually looks
like in this wedding, a frame lit by two different lights says so instead of being badly
corrected as though it were lit by one, and a purple dance floor stays purple.

### Added

- **`aura-core::contract::tone`**: the frozen `ToneEstimate`, `Illuminant`, `SkinLocus`,
  `ToneAlternative`, `ToneReason`, `ReferenceFrame`, `ToneOutline`, `ToneOverride` and
  `ToneService`. **There is no field anywhere in it for an ideal skin value**, which is the
  phase's central design decision rather than a courtesy - see `docs/skin-fairness.md`.
- **`aura-brain-photo::tone`**: twelve modules. Per-scene exposure targets, one statistics
  pass, known-neutral detection, per-identity skin locus accumulation, four illuminant
  hypothesis generators, skin- and neutral-scored hypothesis selection, a constrained solve
  that walks from "leave the light alone" to "remove it completely" and stops at the first
  point every person in frame is plausible again, an exposure clamp against clipping and
  shadow noise, per-chapter reference frames for phase 25, the store and the resumable pass.
- **Migration 15**: `image_tone_estimate`, `identity_skin_locus`, `segment_reference_frames`
  and `v_tone_coverage`. There is no skin-target column and nowhere to put one; the phase
  gate scans the schema for one on every run.
- **`exposure_targets.toml`**: 22 scene rows with a written reason each, including which
  scenes treat a saturated light as a creative choice rather than a fault.
- **Two signed models with cards**: `white_balance` (a chromaticity from a 64 px *linear*
  thumbnail - not a temperature, because most reception lighting is nowhere near the
  Planckian locus) and `exposure_scene` (the faceless frames). int8 is forbidden on both.
- **Seven IPC commands** (ADR-0032), the Basic panel with two confidences, the protected dot,
  the mixed-light indicator and reset-to-AI, and a per-scene review queue that accepts a
  whole scene in one action.
- **`docs/mixed-lighting.md`** and **`docs/skin-fairness.md`**: what the marks mean and what
  has not been proven, in the product's own words.
- **`aura-cli verify --phase 15`**, 22 evaluation gates and two performance budgets.

### Fixed

- The white-balance confidence penalised hypotheses that **agreed** with each other, reading
  the cost gap between the top two candidates as evidence. Two independent estimators landing
  on the same chromaticity is the strongest evidence available and scored as "undecided", so
  every frame fell below the skin-sample threshold, no locus was ever built, and the skin
  constraint bound on nothing - silently. Replaced with an agreement term over the two
  answers' chromaticity distance.
- Migration 15's foreign keys named `identities(identity_id)` and `segments(segment_id)`;
  both tables key on `id`. Every locus and reference-frame write failed.
- An override was written to three columns that nothing read, so a photographer's own
  exposure and colour could not be recovered through the service.
- Stored floats were rounded in `f32` and widened to `f64`, so `0.263` serialised as
  `0.263000011444091796875` and the three evidence documents cost half the per-image budget
  in noise.
- The note saying a coloured light had been kept on purpose depended on how much of the
  wedding had been analysed. It keyed on whichever illuminant hypothesis won, and the winner
  changes as a project accumulates skin loci - so a project's first dance-floor frame was
  labelled and its four-hundredth was not, while the pixels barely differed. It now reads the
  light falling on the frame, which does not change.
- The correction between two lights interpolated a **colour temperature**, which walks along
  the Planckian locus. A coloured light is off that locus by definition, so the mechanism
  built to preserve one could not land on one: it exhausted its twenty candidates, corrected
  fully, and recorded the reason code meaning the mood had been sacrificed on frames whose
  mood was kept. It walks in `u'v'` now, as invariant 8 already said it did.
- Phase 08's burst-label agreement test found its ground truth by editing the *Windows*
  spelling of a path out of `CARGO_MANIFEST_DIR`; on Linux and macOS it looked in a directory
  that does not exist and the phase 08 gate failed.

### Known limits

Both learned heads are untrained placeholders and **neither is consulted**; every figure this
phase reports is about the solver, measured on synthetic frames whose illuminant and subject
luminance were painted in. The fairness gate measures five reflectances, not five people.
Section 11's 600 B storage row is not met - the measurement is 806.9 B, recorded with its
decomposition in `perf/budgets.toml`. Two of five coloured-light frames still go unlabelled
because the fixture's own light sits below `Illuminant::SATURATED_ABOVE`; moving that constant
is a frozen-contract change that wants a photograph rather than a synthetic fixture. See
`docs/progress/PHASE-15-EXIT.md` section 8.

## Phase 14 - Non-destructive edit recipe and the develop engine

The first phase that produces pixels rather than a judgement. An edit is a JSON document with
a canonical form and a hash; a render is 23 stages in one ordered array over linear Rec.2020;
and a delivered file can be re-created from four values - the RAW's content hash, the recipe,
the engine string and the output spec.

### Added

- **`aura-recipe`**: edit recipe schema v1 frozen field for field, the canonical form and its
  hash, the migration framework and its never-remove-a-field rule, XMP and AURA sidecars
  written atomically with a backup, undo/redo with snapshots, and **the merge** - the one
  function in the workspace that writes one recipe into another, and therefore the only place
  a parameter a person set can be protected.
- **`aura-render`**: highlight recovery before white balance, one linear Rec.2020 working
  space, a Fritsch-Carlson curve that cannot overshoot, the neighbourhood stages with the
  frame-wide statistics they are forbidden to measure themselves, a tiler whose output is
  bit-identical to a whole-frame render, the WGSL sources and the parity harness, and an
  output transform that is **the only place tone is baked**.
- **Migration 14**: `edit_recipes`, `edit_history`, `edit_snapshots`, `export_renders` and
  `v_develop_coverage`. There is no path column and no `deleted` flag anywhere in it.
- **Eight synthetic camera profiles**, nine IPC commands (ADR-0030) and a Develop panel that
  renders the protected dot, the caveats in plain words and the engine that drew the frame.
- **`docs/recipe-schema-v1.md`** and **`docs/colour-management.md`**.
- **`aura-cli verify --phase 14`**, the golden suite and
  `crates/aura-render/tests/colour_discipline.rs` - a grep as a test, so the second module to
  start encoding fails the build.

### Known limits

**This build links no `wgpu` backend** (ADR-0029 section 4), so four of the five performance
rows are waived and the interactive budget of 60 ms is not met: the reference path renders a
2048 px proxy in about 210 ms in release. There are no camera files and no photographed
ColorChecker here, so the golden suite runs over authored synthetic pixels and eight
*synthetic* bench profiles - a determinism and regression gate, not a claim about colour
accuracy. Every real camera body renders through the neutral reference profile and says so
(`AURA-RENDER-8008`). See `docs/progress/PHASE-14-EXIT.md` section 8.

## Phase 13 - Explain My Edit, confidence calibration and the decision ledger

Every decision the product makes can be opened up - why, how sure, what it looked at - and
every one of them is written to a ledger that cannot be rewritten. A correction is a new
entry pointing at the old one, and nothing in the product can update a row that says what
happened.

### Added

- **`aura-explain`**: the ledger, with append-only semantics the database enforces and a
  compaction policy that cannot remove a photographer's own decision; a decision builder
  whose canonical JSON and inputs hash exist so a replay compares the question rather than a
  rounding difference; isotonic and temperature calibration with ECE, Brier and reliability
  bins; the autonomy policy; the reason registry; the grounded summariser; the replay port;
  and the anonymised support bundle.
- **Migration 13**: `decisions`, `decision_reasons` and `calibration_models`, one trigger
  that aborts every `UPDATE`, one coverage view and three indexes. `reason_count` is a
  denormalised column with a CHECK, which is how invariant 2 becomes something SQLite
  refuses to break.
- **`autonomy_bands.toml`**: section 6.4's bands verbatim, five per-kind rows each with a
  written reason, and a loader that refuses a row with no reason or thresholds that do not
  descend. `irreversible` is read from the enum and never from the file.
- **`docs/reason-codes.md`**: 93 codes across five vocabularies, generated from the registry
  so the public reference cannot disagree with the product.
- **`docs/how-confidence-works.md`**: what the number means, what it does not mean yet, and
  what AURA is allowed to do at each level.
- **The Explain panel and typed IPC surface**: eight commands, six tabs, evidence crops, the
  alternative comparison with both score breakdowns, and a confidence badge that says plainly
  when nothing has been calibrated.
- **`aura-cli replay`**: re-derives a stored decision from the catalog as it stands now and
  says whether the answer moved - and if it did, whether that is an upgrade or a determinism
  defect.
- **`ExplainSummary`**: the one cloud call this phase may make. No images, no field a new
  reason could go in, and a validator that refuses any number absent from the input.
- **`ml/eval/calibration_report.py`**: the same arithmetic as the Rust side, plus a
  reliability diagram written as SVG with no plotting dependency.
- Six error codes, `AURA-ML-5054` to `AURA-ML-5059`, each with a runbook.

### Changed

- `aura-core` freezes `contract::ledger` and gains `DecisionId` in the frozen `ids.rs`,
  alongside phases 06, 07 and 08's ids. ADR-0027 records the five spellings that differ from
  the phase document.
- `contracts.lock` covers `ledger.rs`, `ids.rs`, migration 13, the IPC surface and
  `ui/src/ipc/types.ts`.

### Known limits

- **Nothing is calibrated.** Every model is the identity map at version 0, the ECE gate is
  measured against synthetic predictors whose error is authored, and `AURA-ML-5058` says so
  once per run. While that is true, every decision is raised one band toward review - so
  nothing in this build acts unattended, and phase 28 cannot ship until a calibration does.
- **Every decision recorded here was made from placeholder heads**, because phases 06, 09, 10
  and 11 all ship them. The ledger records those decisions faithfully; none of them is a
  claim about a photograph.
- **The cloud summary has a cassette and no live provider.** The paragraph a photographer
  sees today is the deterministic template, which is correct by construction.
- **The pixel opt-in of section 2.1 was deliberately not built.** It would be the one code
  path in the product that can put a photograph into a file which is then emailed.

## Phase 12 - Autonomous culling engine, story coverage guard and gallery sizing

A wedding becomes a gallery. Every photograph on both sides of the line carries a reason,
twelve parts of the wedding are guaranteed against every threshold in the product, and
nothing is deleted: a rejection is a row, and it is one click from being overturned.

### Added

- **`aura-cull`**: score fusion as a weighted geometric mean, so no signal can rescue
  another; three hard vetoes read off phase 09 measurements rather than re-derived; a
  moment pass whose keeper count follows how much the moment varied; chapter quotas with a
  bounded local search that trades a second keeper for an unrepresented moment; the
  coverage guard, run twice; three sliding-window diversity caps; and a gallery-size model
  with a reconciliation that adds runner-ups rather than lowering the bar.
- **Migration 12**: `cull_run`, `selection`, `rejections`, `coverage_report` and
  `cull_override`, two views, and three provenance versions plus a digest of the two
  configuration files. The photographer's own keeps and removals live in their own table
  because a re-selection rebuilds every other one.
- **`cull_weights.toml`**: 22 scene rows and three mode rows, every one with a written
  rationale, and a loader that refuses a row weighting framing above whether the photograph
  worked.
- **`coverage_rules.toml`**: twelve declarative guarantees, per-identity minimums, nine
  chapter bands, the diversity caps and the veto policy. An unknown must-have slug is a
  refusal rather than a default, and a table that lists the kiss as a posed scene - which
  would let AURA veto it for closed eyes - is refused outright.
- **The cull view and typed IPC surface**: coverage, gallery, one photograph's decision, run,
  resize, mode switch and a three-valued override. The three coverage states are rendered as
  words rather than colours, and an unanalysed photograph offers no override at all.
- **`docs/how-aura-culls.md`**: what the engine does, what it guarantees, what every reason
  code means, and the one number to check before delivering.
- **Gates**: `aura-cli verify --phase 12`, a 24-test harness, a self-testing Python
  agreement harness, four checked-in keeper label files and two asserted budgets.

### Changed

- `aura-core` gained the frozen `cull` contract; `CullService` is now the only way any
  phase may ask what is being delivered.
- The catalog schema version is 12.

### Known limitations

- Every sub-score underneath every decision comes from a placeholder head (phases 06, 09,
  10 and 11). The arithmetic is real and tested; the numbers it works on are not yet claims
  about photographs. Condition C1 in `docs/progress/PHASE-12-EXIT.md`, and it closes with
  phase 05's C10.
- The per-scene calibration ships as the identity map, and the gallery-size regression is
  authored rather than trained on real delivered galleries.
- The blind photographer study of section 13 does not exist; agreement is measured against
  four synthetic weddings with documented labels.
- The optional cloud tie-breaker was not built. Its trigger is two scores within 0.02 of
  each other, and with placeholder heads underneath that is noise rather than a tie - so
  every call would be a paid question about nothing. Condition C6.

## Phase 11 - Composition and aesthetic AI

Every photograph now carries an explainable framing reading: whether a reliable horizon
is level, what the edge cuts, how the subject is placed, whether visual weight is balanced,
and which measured background regions compete for attention. It is evidence for culling
and geometry phases, not a crop or a selection.

### Added

- **`aura-brain-photo::composition`**: rho-coherent horizon measurement with intentional
  dutch-angle handling; pose/face-aware headroom and crop auditing; thirds, centre,
  negative-space and balance measures; background edge energy, bright regions, head merges
  and colour competition; a bounded aesthetic term; stable reasons, evidence rectangles,
  crop hints, persistence, dismissal, resume, telemetry, and relative-within-moment score.
- **Migration 11**: `image_composition`, one review-queue index, coverage and flag views,
  three provenance versions, compact evidence JSON, and photographer dismissals that
  survive re-analysis.
- **`composition_rules.toml`**: a neutral fallback and 22 scene-conditioned rows with
  rationales, including explicit allowances for centred details, deliberate close crops,
  and intentional tilt.
- **Two signed architecture fixtures**, `pose_keypoints` and `aesthetic_head`, with model
  cards; guarded training/evaluation/export tools in `ml/models/composition/`.
- **The Composition card and typed IPC surface**: project status, one-photo reading,
  flagged review queue, one-note dismissal, resumable analysis, and normalised evidence
  overlays. The card explicitly distinguishes clean, exonerated, unavailable, and
  unanalysed states.
- **Five error codes and runbooks**, `AURA-ML-5043` to `AURA-ML-5047`; ADR-0023 for the
  rules/contract and ADR-0024 for the application boundary.
- **`aura-cli verify --phase 11`** and a composition performance/storage suite. The
  algorithm evaluation contains 37 authored synthetic regression tests.

### Changed

- Horizon confidence now requires a coherent line in both angle and offset, preventing a
  repeated diagonal texture from being called a strong horizon.
- Neutral or white subjects can still receive a colour-competition reading from saturated
  background energy, and subject colour is sampled from the dominant head before using a
  coarser body region.
- Mid-limb crop severity and unlocated reference poses now agree with the crop gate and
  placement semantics instead of silently falling just below the flag boundary.

### Not built, deliberately

This phase does not crop, straighten, remove a distraction, keep, reject, or order a
gallery. Crop hints are advisory data for phase 23. Generic background measurements do
not claim to recognise an exit sign, bin, mirror, or rubbish; semantic re-validation waits
for phase 18 and removal belongs to phase 24.

### Known limits

Both checked-in heads are untrained deterministic placeholders, so the analyser does not
claim their output is learned. All quality numbers are against authored synthetic frames
or reference geometry, not the three reference weddings or a photographer panel. No GPU
backend or three-machine CI is available here, so the two GPU budgets retain ADR-0007's
waiver. Calibration, demographic/cultural slices, the 300-frame perceptual audit, semantic
background categories, and the real-wedding demo remain explicit conditions in
`docs/progress/PHASE-11-EXIT.md`; the placeholder-model condition is a Sev 2 trigger.

## Phase 10 - Expression, emotion and moment ranking

The app finds the moments that matter - genuine smiles, laughter, tears, hugs, kisses,
reactions and ritual peaks - and ranks every frame by emotional value. Phase 09 decided
what is *acceptable*; this decides what is *worth delivering*, and the two are separate
numbers that a later phase combines.

The whole phase is shaped by one risk, and like phase 09's it is not a technical one: an
emotion model built somewhere else learns that a moment is a big smile, and delivers a
Hindu ceremony as an empty gallery. So composure is a **positive** reading rather than the
absence of one, in the four ceremony scenes it is weighted at or above a smile, three
traditions raise it further, and the file that does all of that is a table a person can
read with a written reason on every row.

### Added

- **`aura-brain-wedding::emotion`**: eight continuous readings per face from an aligned
  crop; gaze measured from phase 06's eye landmarks rather than predicted; nine
  interactions from the whole frame with a person-prior plane; a smoothed peak curve per
  moment that refuses to name an apex when there is not one; reaction linking across
  cameras inside a four-second window; and a nine-feature Bradley-Terry ranker whose
  coefficients are a list somebody can argue with.
- **Migration 10**: `image_interaction`, `face_expression`, `moment_peak`,
  `reaction_links`, `emotion_preferences` and two coverage views. 733 bytes per image
  against a 900-byte budget.
- **`emotion_weights.toml`**: 22 scene rows, 5 tradition rows, 9 ranker coefficients and 2
  calibration tables. The loader refuses eight things, including a row with no rationale
  and a calibration map that would reorder frames.
- **Two signed models**: `expression_head` (112 px crop, eight sigmoids, int8 forbidden)
  and `interaction_head` (160 px frame in four planes, nine sigmoids, int8 permitted).
  Both untrained; both carry cards that say so at the top.
- **`MomentSignificance`**, the one cloud call this phase may make: six 768 px thumbnails,
  anonymised role handles, at most 25 calls a wedding, and a validator that refuses a
  reason containing any of twenty appearance or psychology words.
- **The Emotion card and the moment browser**: face crops with eight bars each, interaction
  chips, a three-state peak indicator and a reaction pair viewer. Seven IPC commands, five
  of them reads.
- **Five error codes** with runbooks, `AURA-ML-5038` to `AURA-ML-5042`, and two ADRs.
- **`docs/emotion-and-moments.md`**, whose first section is titled "AURA describes
  photographs. It does not read minds."

### Changed

- **Phase 09's third eye-intent rule now fires.** `IntegrityPass::with_emotion` fills
  `IntentInput::tears` through `aura-core`'s frozen trait, so a tearful closed-eye
  photograph carries `EYES_CLOSED_OK` instead of `EYES_CLOSED`. This closes condition C4 of
  the phase 09 exit report; `analysis_ver` went from 1 to 2, which makes every stored
  technical verdict pending so the background pass re-measures.
- **The 112 px two-point face warp moved into `aura-vision`.** Phase 10's expression head
  became its second consumer, and two copies of a warp is two crops that drift apart while
  looking identical. Phase 09's 26 eval gates and 11 calibration tests pass unchanged.
- `Interaction::from_str` is spelled `from_slug`, because a `from_str` that is not
  `FromStr` is a method that gets called by accident.

### Not built, deliberately

Final selection is phase 12 and album sequencing is phase 29, so nothing here keeps,
rejects, delivers or builds a gallery - `EmotionService::ranked` returns an *ordering*, the
moment browser says "An ordering, not a shortlist" in its own header, and a test asserts no
label in it says keep, reject, deliver or cull.

Any claim about a person's inner emotional state is out of scope permanently. The twenty
things this phase can say about a photograph are a closed list, call sites do not write
sentences, and the cloud task's output has no field a description of somebody could go in.

### Known limits

Both heads are placeholders with the right architecture and no training, so every number in
section 10.1 is measured against synthetic frames whose answer is painted into the pixels.
The ranker is fitted on eight authored comparisons rather than ten thousand photographers'
ones, and four of its nine coefficients are unidentifiable from that data and set by
argument instead. Gaze is head direction rather than eye direction. The per-scene
calibration ships as the identity. The four named peak kinds are derived from the scene and
the interaction rather than trained. All five are in `docs/progress/PHASE-10-EXIT.md`
section 5, and the first is a Sev 2 trigger.

## Phase 09 - Frame integrity: focus, motion, exposure, noise and eye state

Every frame gets an honest technical verdict where it matters. Not "is this photograph
sharp" - a soft background is usually the point - but **is the right subject sharp**, was
the blur a decision, can the exposure be brought back, how noisy is it for this kind of
photograph, and are the eyes that matter open.

The whole phase is shaped by one risk, and it is not a technical one: a product that
throws away a frame it should have kept is a product a photographer stops using. So two of
the fourteen technical marks describe something *right* with a photograph, eight of the
twenty-one reason codes withdraw a claim rather than making one, and the learned focus
head is allowed to exonerate a frame and forbidden from convicting one.

### Added

- `aura-brain-photo`: a new crate, and the first that judges pixels rather than reading
  rows. Subject-aware sharpness from three classical measures over eye, face, body and
  background regions; motion intent from a structure tensor, because motion blur is
  directional and defocus is not; recovery-aware exposure with a specular-highlight
  exclusion, so a candle flame is a light source and a blown dress is a loss; noise
  measured in flat regions and expressed against what the scene tolerates; eye state with
  section 6.4's four intent rules.
- **A camera calibration table for twenty bodies.** "Sharp" means sharp *for this gear*: a
  61 MP body and a 24 MP body produce different edge detail in the preview AURA reads, and
  without the division the more expensive camera would win every comparison. A body with
  no row is judged more cautiously and the panel says so.
- **Closed eyes are often the photograph.** A kiss, a prayer, a first look, somebody crying
  at a toast - `EYES_CLOSED_OK` marks those as right rather than wrong, and only the people
  a photograph is *about* have their eyes judged at all.
- Migration 9: `image_integrity` and `face_eye_state`, plus a coverage view and a flag
  histogram. Nothing in the schema can reject a photograph, and "not checked" is
  deliberately distinguishable from "clean".
- Two signed models with cards - `focus_head` and `eye_state` - and the training,
  evaluation and export scripts in `ml/models/integrity/`.
- The integrity IPC surface (ADR-0020): six commands, five of which are reads. The
  Integrity card shows the crop that caused each penalty; the filter chips offer soft,
  blinked, blown and noisy, and read their names from the backend rather than keeping a
  second copy of the flag list.
- `docs/frame-integrity.md`: every mark in the words the product uses, and a build that
  fails if a reason code is added without one.
- `aura-cli verify --phase 09`, eleven checks, exit 0.

### Changed

- `FaceRef` gains a bounding box and the two eye landmarks. Phase 09 cannot measure an eye
  region or show the crop behind a closed-eye mark without them; the nose and mouth
  corners stay out, which is what keeps that type's promise that it carries nothing a
  recogniser needs. ADR-0019 section 3.
- The moments view's error toasts said `undefined`. Five call sites read a field the wire
  type does not have; fixed here because it was a one-word change.

### Known limitations

- Both learned heads ship **untrained**. Every accuracy figure in this phase is measured
  against images whose answer was known in advance, which proves the arithmetic and says
  nothing about photographs.
- The twenty calibration rows are derived from published specifications rather than
  measured from bodies, because there are still no camera files in this repository.
- Clipping is measured on the preview rather than on the RAW histogram.
- The "there are tears here" intent rule needs phase 10 and is wired through as always
  false. A tearful closed-eye frame may reach a review queue; it will not reach a delivery
  decision, because this phase makes none.

## Phase 08 - Smart burst grouping and duplicate detection

Three thousand loose files become a few hundred moments. From this phase onward the
product works on **moments** rather than on files, and the difference is not efficiency:
rejecting a burst is a moment lost, whereas rejecting individual frames is tidying, and
phase 12's coverage guarantees are written against the first of those.

The first phase since 02 that ships no model. Grouping is arithmetic over phase 05's
vectors and phase 01's timestamps, which is why three of section 11's four budget rows
are met by two to three orders of magnitude rather than by a margin.

### Added

- `aura-brain-wedding::moments`: seven modules that turn a timeline into a two-tier
  structure. A **moment** is one thing that happened; a **burst** is one press-and-hold of
  the shutter inside it. Fourteen frames of a bouquet toss are one moment, and the six
  that came off at 10 fps as it left her hand are one burst inside that.
- An adaptive cadence estimator, per camera. The burst window is
  `clamp(2.5 x median_interval, 0.7 s, 8 s)` over a rolling 60-second neighbourhood, so
  a 10 fps burst and a ceremony shot in ones and twos are both handled by the same rule.
  Two photographers interleaved on one timeline have a combined median of roughly half
  of either's, which would halve the window for both - so cadence is estimated per body
  and the merge happens later, where it can be justified rather than inferred from an
  arithmetic accident.
- A time-windowed similarity graph, never all pairs. A 4,000-frame wedding has eight
  million pairs and about sixty thousand candidates, and only the second number gets
  scored - which is the whole of why 4,000 images group in ten milliseconds against a
  six-second budget.
- **Time proximity became evidence rather than only a gate.** Section 2.1 lists it first
  among the grouping signals and section 6.2's four-term score has no time term at all;
  without one, a ceremony shot at one frame every eight seconds chains into a single
  moment for as long as the photographer keeps shooting, because every consecutive pair
  is inside the eight-second clamp and every consecutive pair looks alike - the altar has
  not moved. The four documented weights are untouched and their sum is scaled by a
  proximity factor. ADR-0017 section 3.
- Scene-conditioned grouping thresholds in `moment_profiles.toml`, a sibling of
  `scene_profiles.toml` with the same rules: no rationale, no load. Ten scenes are argued
  over and twelve take the defaults, and the file names which twelve so a reader can see
  what was actually decided. `dance_floor` groups at 0.60 and `family_portrait` at 0.76,
  because two consecutive family groups are visually almost identical and are two
  different deliverables.
- Duplicate classification as a **conjunction** of three independent tests, not a
  disjunction: a difference hash within four bits, an embedding distance within 0.03, and
  the faces in the same places. A hash is blind to a blink, an embedding is blind to a
  stop of exposure, and the face overlap is blind to everything else - three blind tests
  that must all agree is a far stronger claim than one confident one, which is what
  section 10.1's demand for 0.98 recall at 0.95 precision actually asks for.
- Cross-camera merging on temporal overlap above 60 % and medoid distance under 0.12,
  measured against the *shorter* of the two spans - a two-second burst inside a
  forty-second sequence overlaps their union by 5 % and is entirely inside it. The merged
  moment keeps its per-camera bursts intact, so a bad merge is split back along the line
  it was joined on.
- Migration 8: `moments`, `moment_images`, `duplicates` and `moment_edits`, with three
  version columns because they invalidate three different things.
- Nine IPC commands, a stacked moments grid and a side-by-side duplicate review panel.
- Five error codes, `AURA-ML-5028` to `AURA-ML-5032`, each with a runbook.

### Fixed

- **AURA could not see a burst at all on a real camera file.** EXIF's `DateTimeOriginal`
  has whole-second resolution, so fourteen frames of a 10 fps burst carry one timestamp
  between them; the fraction lives in `SubSecTimeOriginal`, which phase 01 stores
  separately in `photo.sub_sec`. Every unit test passed and the phase gate failed.
  Reconstructing the fraction took grouping accuracy from 0.000 to 1.000 on two of the
  five regression patterns. It is the most consequential thing found in this phase and no
  synthetic fixture would ever have exposed it.

### Changed

- `catalog.count` accepts the four new tables.
- `photo.camera_serial` is now a documented fallback when a `camera` row does not exist
  yet, so a project part-way through import does not look like a single-body wedding.

### Known limits

- **The embedding underneath is a placeholder** (phase 05 condition C10) and it is the
  largest term in the grouping score. Every number in this phase is measured against
  authored ground truth, and none of them is a claim about a real wedding's pixels. This
  is condition C1 in `docs/progress/PHASE-08-EXIT.md` and it is a Sev 2 trigger.
- Phase 06's two face signals are not wired in. `PeopleService` has no bulk accessor for
  either, and adding one is a phase 06 contract change. Every resulting degradation is in
  the safe direction: a skipped face test makes a near-identical claim *harder*.
- Extra storage per image is 319 bytes against a 200-byte budget, waived at 340 by PERF
  and CTO in ADR-0017 section 8. Four schema decisions took it down from 720; the
  remaining gap is 40-character text ids and the reasons invariant 2 requires.
- Nobody has looked at a moment stack for a wedding they attended.

### Not built here, deliberately

Choosing the winner of a burst. That is phase 12, and the boundary is structural rather
than remembered: no `culled` column, no rank, no rejection anywhere on the IPC surface,
and `keep_hint` spelled *hint* in the contract, the schema, the wire and the panel.

## Phase 07 - Wedding scene AI and story timeline segmentation

The app reads the wedding as a story. Every photograph gets a scene label, fourteen
attributes and a confidence; the day is split into ordered chapters with boundaries,
counts and durations. From this phase onward no threshold in the product is global - a
dark dance frame and a formal family portrait are judged by different rows of the same
table, which is invariant 7 finally becoming a lookup instead of a promise.

### Added

- `aura-brain-wedding`: the scene half and the story half of the wedding brain. Nothing
  in it opens a pixel. The classifier is a small adapter on the *frozen* phase 05
  embedding - section 6.1's design - which is why scene inference for four thousand
  images fits in eight milliseconds of arithmetic where phase 06's face pass needs twelve
  minutes for the same wedding.
- A 22-class scene head and fourteen independent attribute sigmoids on one adapter. The
  abstention is deliberately **not** a softmax slot: a model cannot usefully be trained to
  say "I am not sure" through an output that competes with the classes it is unsure
  between, so `SceneId::Unknown` is a decoder decision from the top-1 margin - and the
  margin, not the confidence floor, is what actually rejects.
- Four of the fourteen attributes are **decided rather than predicted**. `flash`,
  `night`, `tungsten` and `indoor` are recorded exactly by the camera or by the phase 05
  luminance statistics, and where a measurement exists it beats the head. A trained model
  will still be wrong about flash on a frame lit by a window at 1/200th; the EXIF will
  not.
- A tradition-conditioned ritual head with **two** abstention mechanisms, because they
  answer different questions. Slot 0 is "no rite" and competes in the same softmax as
  every rite; the margin handles the case where the head has correctly identified a fire
  circumambulation and cannot tell whether to call it `saptapadi_pheras` or `saat_phera`.
  Naming either at 0.36 would put a Nepali wedding's rites under Hindu names in a
  client-facing timeline.
- Forty-eight rites across five traditions - Hindu, Nepali, Christian, Muslim, civil - in
  editable config files, with `docs/adding-a-tradition.md` as the procedure a
  photographer's consultant can follow without a compiler. The rite's authored id **is**
  the model's output slot, which is why a duplicate is refused rather than resolved.
- HMM smoothing over nine chapters before segmentation rather than after. A single
  misclassified frame in the reception is a wrong label; fed to a change-point detector it
  is a two-frame "Getting Ready" chapter between the speeches and the cake, and by then no
  amount of smoothing helps.
- PELT change-point detection over a three-term fused signal, with the penalty **searched
  in log space** rather than fixed. A penalty tuned on a ten-hour Hindu wedding gives two
  chapters for a registry office and forty for a three-day Nepali wedding; the search is
  what makes section 10.1's 6-to-20 chapter band hold on all three.
- `scene_profiles.toml`: twenty-two scenes, each with tolerances, weights, an editing
  intent, a coverage flag and **a written rationale**. The loader refuses a profile
  without one. That friction is the point - somebody who cannot write a sentence
  explaining why the dance floor tolerates three times the ceremony's noise has not
  finished deciding it.
- Migration 7: `image_scenes`, `segments`, `segment_images` and `scene_profiles`, plus two
  views. The user-override guards are inside the statements that would overwrite them, not
  around them: a read-then-write leaves a window in which a photographer loses a race with
  a background pass.
- Nine IPC commands and the story timeline. Chapter cards are sized by **duration**, not
  by frame count - a ninety-minute dinner and a six-minute cake cutting with forty
  photographs each are not the same shape of event. Moving a boundary locks both chapters
  either side of it, because a boundary is shared.
- The `SegmentNaming` cost policy: at most sixteen calls per wedding, least-confident
  first, locked chapters never priced, and phase 04's rule enforced - a cloud answer may
  not overrule a local decision at 0.90 or above without citing visible evidence, and the
  conflict is logged.
- Two signed placeholder models with cards, six error codes with runbooks, two ADRs, and
  four training and evaluation scripts under `ml/models/scene/`.

### Changed

- `aura-people` now receives real scene labels. **Half of phase 06's condition C3
  closes**: the couple contest's getting-ready, ceremony and portrait terms turn on,
  `RoleOutcome::scene_starved` is false on a classified wedding, and
  `SCENELESS_CONFIDENCE_CEILING` stops capping the couple decision at 0.62.
- `xtask` learned that a model can take a feature vector rather than pixels. The two scene
  heads declare `[N, 528]` and `[N, 536]` with an `NC` layout and an `unbounded` range, so
  the runtime's shape check passes and the manifest documents a normalisation nobody
  performs.
- The catalog's countable-table list gained the four new tables.

### Fixed

Three bugs the evaluation harness found in code that read correctly, recorded because each
one is an argument for building the fixtures before the gates.

- The penalty search never reached its own range. Linear bisection of `0.0005..40` spends
  its first ten steps between 40 and 0.04, and one fixture's answer is 0.008 - so it fell
  back to gap-only segmentation and produced three chapters against a six-chapter floor.
- Masking the ritual head by tradition made it abstain *more*, not less. Zeroing another
  tradition's slots without renormalising left the distribution summing to under one, so
  establishing the tradition made naming a rite harder rather than easier.
- The per-image storage estimate was 25 % low: 330 bytes claimed, 410 measured, against a
  400-byte budget. Writing the top-3 as pairs rather than as objects closed it - the words
  "scene" and "score" repeated three times per photograph were a fifth of the budget.

### Known limitations

Both models are **placeholders with no training**, which is condition C1 of
`docs/progress/PHASE-07-EXIT.md` and a Sev 2 trigger. Every number in section 10.1 is
measured against synthetic ground truth whose answer is known by construction: that proves
the algorithms and says nothing about the weights. No later phase may claim a quality
result that depends on scene classification being accurate until it closes.

One phase 06 budget - `identity_cluster_skeleton` - does not reproduce on the development
machine: 21.7 s against a 12 s budget where the phase 06 report records 2.1 s. It was
ruled out as a phase 07 effect by measurement and is recorded in section 4 of the phase 07
exit report for PERF to resolve against phase 06.

Per-tradition accuracy is **not published and not approximated** - condition C5, the
second Sev 2 trigger. The disparity this phase risks is cultural rather than demographic,
which is precisely the gap section 1 claims as a competitive moat, and an unmeasured
version of that claim is one the product cannot support.

## Phase 06 - Face detection, recognition and people intelligence

The app learns who matters at this wedding: it finds every face, groups them into
identities, and ranks the couple, close family and VIPs by evidence rather than by
guesswork. Every later decision gets a subject hierarchy, so sharpness on the bride's face
outranks sharpness on a stranger's elbow.

### Added

- `aura-vision::face`: one decoded frame in, everything phase 06 needs out. Detection with
  a letterbox rather than a centre crop - the faces the tiled pass exists to recover are
  the ones at the edges of a wide ceremony frame - three output strides from one forward
  pass, and faces and bodies predicted by the same anchor, which is why the phase ships
  three models and not four.
- A conditional 2x2 tiled pass that fires on wide-angle frames with several small
  detections, and on frames where bodies were found and faces were not. Its cost is
  recorded per frame in `face_scan.tiled` and reported by `ScanReport::tile_ratio`, because
  "tiled detection doubles cost" is a failure mode to measure rather than assume.
- A bokeh gate that works by geometry rather than by score: a blurred highlight has no
  landmark structure, so its five points collapse towards its centre.
  `Detection::landmark_spread` measures that, which lets the objectness threshold stay low
  and keeps small-face recall.
- ArcFace alignment: a closed-form Umeyama similarity transform onto the published 112 px
  layout, never affine, because an affine fit to five points can shear and a sheared face
  is a different face to a recogniser. Head pose is estimated from the same five landmarks.
- A quality gate that decides which faces may vote on identity: four measured factors -
  sharpness, occlusion, pose, exposure - combined as a weighted **geometric** mean, so a
  perfectly exposed, perfectly frontal, completely out-of-focus face cannot score 0.75 and
  vote, plus two hard cut-offs where the evidence genuinely runs out. A face below the gate
  is detected, stored and displayed; it just does not vote.
- Identity clustering with **exact** average linkage computed from running sums: for unit
  vectors the mean pairwise cosine distance between two clusters is one minus the dot
  product of their unnormalised means, so exact average linkage costs one dot product per
  cluster pair rather than `|A| x |B|`.
- Relative-cohesion verification, which is what actually prevents the chain merge. Two
  looks of one person sit about 1.7 times their own internal spread apart; two siblings sit
  at three times it. A wedding of near-lookalikes records refusals rather than producing
  one identity for six people.
- Sub-centroids for an identity whose members span two looks - the outfit and hairstyle
  change - so a face from either look still matches.
- Role inference from photographic evidence only. **Automation never assigns `bride` or
  `groom`**: the evidence identifies a pair, which of two people is the bride is not a
  photographic fact, and the couple may be same-sex. Confidence is capped at 0.62 while
  scene labels are missing, and the reason string says why.
- Prominence scoring with a versioned weight file, scene-conditioned tables, and
  `subject_focus_score` - the prominence-weighted sharpness phases 09 and 12 use instead of
  naive global sharpness.
- `aura-people`: the sealed biometric store. Templates, centroids and 112 px crops are
  encrypted with a key derived from a per-project secret in the operating system's
  credential store, using BLAKE3 encrypt-then-MAC with a **synthetic nonce** - so
  re-scanning after a model change cannot reuse a keystream, and sealing stays
  deterministic.
- Migration 6: `face_vault`, `face_scan`, `identities`, `faces`, `identity_links`,
  `person_boxes`, `cooccurrence`, and two views. `face_scan` is new in kind: "no faces in
  this frame" is a legitimate result, so the resumability ledger records the *look* rather
  than the finding.
- Merge, split, rename, mark-couple and an importance slider, all undoable, all recorded in
  an append-only journal, and all replayed onto a fresh grouping **by face set rather than
  by identity id** - so a photographer's decision survives a full re-analysis even though
  re-clustering produces new ids.
- Biometric erasure that deletes the credential-store entry *first*, so a crash mid-erasure
  leaves unreadable data rather than readable data, then the crops, then the rows, then
  verifies that nothing survived. Culling and edit decisions are untouched.
- `CoupleHint`, the one cloud call phase 06 may make, behind an ambiguity trigger and a
  two-call cap. Candidates are opaque handles, so a model that answers with a description
  of a person - or volunteers a gender - fails validation rather than being stored.
- Three signed models with cards: `face_detect`, `face_embed`, `face_quality`. `int8` is
  forbidden on the detector, because quantising a box regression moves a 40 px face by
  several pixels, and on the quality head, because quantising four sigmoids destroys the
  resolution the 0.4 gate needs.
- The people IPC surface and the People panel, plus `aura-cli verify --phase 06` as the
  gate: thirteen checks, from the migration to an erasure that leaves nothing behind.
- Nine error codes with runbooks: `AURA-ML-5017` to `AURA-ML-5021` and `AURA-SEC-9001` to
  `AURA-SEC-9005`.

### Changed

- `aura-core` gained the frozen people contract - `Role`, `SubjectHierarchy`,
  `ImageSubjects`, `PeopleService` - and two typed ids, `FaceId` and `IdentityId`. It still
  depends on no other workspace crate.

### Known limitations

- **The three shipped models are placeholders.** The detector finds no faces in a
  photograph and the recogniser's templates carry no identity information. Every gate in
  section 10.1 is measured against synthetic ground truth with a known answer, which proves
  the algorithms and says nothing about the weights. Condition C1, a Sev 2 trigger.
- The quality head's trust weight is 0.0, so the gate is four measured factors. Condition
  C2.
- No demographic analysis is published: the fixtures use one skin tone, and a fairness
  number computed from them would describe a renderer. Condition C5, a Sev 2 trigger.
- The two GPU throughput budgets are waived with an expiry condition; a measured
  processor-path row replaces them.

## Phase 05 - Perceptual embeddings and the wedding similarity index

Every image gets a compact perceptual embedding plus a fast similarity index, so
the app can answer "what looks like this?" across a wedding in milliseconds. It is
the shared vector substrate that scene clustering, burst grouping, duplicate
detection, people grouping, reference-frame selection and consistency checks all
reuse - computed once, in one pass.

### Added

- `aura-index`: the frozen `SimilarityIndex` contract and a deterministic HNSW
  graph behind it - `M = 32`, `ef_construction = 200`, `ef_search = 64`, cosine
  distance on L2-normalised fp16 vectors. Levels come from `blake3(image_id)`
  rather than a generator, every tie breaks by `timeline_ts` then `image_id`, and
  the parallel build is batched rather than concurrent, so two machines with
  different core counts produce byte-identical graphs.
- Filtered queries: k-nearest neighbours, radius search, time-windowed search as a
  pre-filter over a sorted timeline (not a post-filter, which is what keeps a burst
  query under a millisecond), camera restriction, exclusion sets, medoids and
  centroids.
- `aura-vision`: one decode, five results. The embedding, a 64-bit difference hash,
  an 8x8x8 HSV histogram, six luminance statistics and an edge-energy summary all
  come out of the same buffer, which is then dropped - a 4,000-image wedding is
  never 4,000 resident proxies.
- A persisted graph snapshot with six named refusals - missing, wrong magic, wrong
  format, wrong graph parameters, wrong model or preprocessing version, failed
  digest - each of which is a warning and a rebuild rather than a failure to open
  the project. A second open of a 4,000-image wedding is a 23 ms read.
- `wedding_embedding` 1.0.0, signed into `models.lock` with a model card. **It is a
  placeholder backbone**: there is no labelled wedding data in this repository and
  no GPU backend, so a ViT-B/16 with a contrastive head cannot be trained or run
  here. Everything around it is real. See ADR-0011 section 3, and condition C10 in
  the phase 05 exit report.
- `ml/models/embed/`: the dataset specification as executable code - wedding-level
  splits, a cross-tradition holdout, positive and hard-negative mining, and an
  augmentation policy that *cannot express* a flip or a heavy crop - plus the
  contrastive loss, the training schedule, the four evaluation gates and an
  exporter that reproduces the shipped model byte for byte.
- Migration 5: `embeddings` and `descriptors`, 1,623 bytes per image against a
  1.6 KB budget, reversible in three statements.
- The similarity IPC surface (ADR-0012): five commands, five DTOs and three
  telemetry events, plus `ui/src/components/SimilarPanel.tsx` - the debug "find
  similar" panel section 8 calls "invaluable for later phases". No command returns a
  vector, and a test enforces that.
- `aura-cli verify --phase 05` and `just phase-05-verify`: two cards of RAW
  fixtures, a cancelled pass that does nothing, a real pass, the index, a
  five-millisecond query, a time window, a camera filter, the snapshot and its
  refusals, an incremental second card, and determinism through the whole path.
- Four error codes with runbooks: `AURA-ML-5013` (unusable vector),
  `AURA-ML-5014` (snapshot rejected), `AURA-ML-5015` (embedding version drift),
  `AURA-ML-5016` (project past the documented in-memory ceiling).

### Changed

- `cosine_distance` accumulates into eight fixed lanes rather than one, so the
  compiler can vectorise it. With borrowed neighbour lists in place of cloned ones
  this took a 4,000-vector build from 13.3 s to 2.74 s. The lane count is fixed, so
  determinism is unaffected.
- `just budgets` and the CI budget lane run with `--test-threads=1`. A budget suite
  whose cases race each other measures the harness.
- `aura-catalog` gains `repo::set_capture_time`, for a body that recorded no clock -
  and for the phase 05 gate, which needs a wedding-shaped timeline over fixtures
  that carry make and model but no capture time, and says so in its output.
- `aura-cli infer` gained `--input wedding` and a stopwatch, so a 384 px model can
  be timed from the command line.

### Known limits

- The embedding carries no wedding semantics yet, so the purity, NMI and retrieval
  gates from section 6.4 are **deferred**, not passed. The duplicate gate is met and
  is not deferred: it is answered by the difference hash, which has no learned
  component. The evaluation harness computes all four and proves it would fail a
  head that learned nothing.
- Section 11's two GPU throughput budgets are waived - there is no GPU backend - and
  the 400 ms cold-build budget is waived for the build and met for the load. Both
  waivers carry expiry conditions in ADR-0011 section 5.

## Phase 04 - Cloud AI gateway and the agentic reasoning runtime

Paste one API key and the app gains a governed reasoning layer. It is a bonus
tier and never a dependency: with the network unplugged a full wedding still
completes, every decision marked `local_fallback`.

### Added

- `aura-cloud`: the frozen `CloudTask` contract and the seven-step gateway -
  policy, render, inspect, cache, govern, call, settle. It is the only crate in
  the product allowed to open a socket, and `scripts/check-banned.sh` enforces
  that the way it already enforces one runtime for models.
- Four providers behind one shape - Anthropic Messages, OpenAI Chat Completions,
  Google `generateContent`, and OpenAI-compatible self-hosted servers - with
  three-tier model aliasing, so a task names a capability and never a vendor.
- Three transports: a hand-written HTTP/1.1 client, a cassette replayer, and an
  offline refusal. The HTTP client does **not** speak TLS, so this build reaches
  `http://` endpoints - a local Ollama, LM Studio or studio gateway - and not the
  public HTTPS providers. The waiver and its expiry condition are in
  `docs/adr/ADR-0009-cloud-ai-policy.md`.
- Keys in the operating system's own credential store, by command invocation
  rather than FFI, with the secret written to the child's **stdin** and never to
  `argv`. A test asserts that for all three platforms' command shapes at once.
- A JSON Schema validator that refuses a keyword it does not implement rather
  than ignoring it, reports every failing rule at once in a stable order, and
  writes its complaint for a model to act on. Exactly one repair round trip, then
  the local answer.
- A payload builder that cannot upload an original: a full-resolution tiled
  decode and a scene-linear buffer are both refused by type, tiles are capped at
  768 px, and the EXIF summary is an allow-list with no GPS, no filename, no
  serial number and no absolute time. Optional pre-upload face blur.
- A cost governor that prices every call **before** it is made, drops a tier
  rather than a decision when the budget runs low, and stops at the cap without
  stopping the gallery.
- A response cache keyed on task, version, prompt hash, image content hashes and
  model, so re-running a wedding is nearly free and produces identical decisions.
- An audit trail with a row for every decision **including the ones that never
  reached a model**, which are usually the ones worth reading.
- Bounded agent primitives - step cap, deterministic tool ordering, structured
  scratchpad, four limits checked before each step, cancel within one step - for
  phases 27 and 29 to build on.
- `SegmentNaming`, the reference task, with section 7's prompt and schema copied
  verbatim and a controlled vocabulary of eighteen scenes, eighteen rituals and
  eight traditions.
- Migration 4: `cloud_calls`, `cloud_cache`, `cloud_budget`. The consent gate
  frozen in phase 01 has its first caller.
- Ten IPC commands and a Settings > AI keys panel: key entry, Check, caps, the
  privacy switches, a live spend meter and the audit viewer
  (`docs/adr/ADR-0010-cloud-ipc-surface.md`). No command returns a key.
- 14 error codes with runbooks: `AURA-CLOUD-6001..6014`.
- `aura-cli verify --phase 04`: sixteen checks, no network.

### Changed

- Budget assertions now run in release. A budget is a claim about the binary a
  photographer runs, and the payload builder is roughly ten times slower
  unoptimised.
- `aura-perf` gained count and cost budget kinds. Not everything worth budgeting
  is a duration or a size.

### Measured

Gateway overhead 0.08 ms per call (budget 15 ms). 75 calls and USD 1.04 for a
3,000 image wedding (budgets 75 and USD 1.50). 100 % cache hit rate on a re-run
(budget 70 %). A total cloud outage costs 9 ms against a 135 s pipeline floor
(budget 3 %).

### Rules every later phase inherits

- **`CloudAiGateway` is the only way to reach a model provider.** No phase may
  open a socket; the lint enforces it.
- **A task without a local fallback does not compile**, and neither does one
  whose answer cannot state its confidence and reasons.
- **Bump `CloudTask::VERSION` on any prompt, schema or ceiling change.** The
  cache key contains it, and a stale answer is worse than no answer.
- **Cloud proposes; deterministic code decides.** A cloud answer may not overrule
  a local decision at confidence 0.90 or above unless it cites contradicting
  visual evidence, and the conflict is logged.

## Phase 03 - Inference runtime and the signed model registry

One local AI runtime behind one frozen interface, and a model registry that
refuses anything it cannot verify. Nothing in phases 01 and 02 calls it yet;
every AI phase from 05 onwards calls nothing else.

### Added

- `aura-infer`: the frozen `InferService`, a hardware probe that measures a
  machine and writes `hardware_plan.json`, execution-provider negotiation with a
  per-machine set-aside list, a session pool, a batch scheduler with a memory
  ledger, cooperative cancellation, and warmup with visible progress.
- A deterministic interpreter over a documented subset of ONNX opset 13:
  nineteen operators, a protobuf reader *and* writer, and three genuinely
  different numeric paths (fp32, fp16, int8). Pure safe Rust. ONNX Runtime is
  **not** linked - see `docs/adr/ADR-0007-inference-runtime.md` for the four
  reasons and for how a backend is added later without touching a caller.
- `aura-models`: `models.lock` verified by ed25519 then sha256 then model card,
  in that order and entirely offline; resumable transfers against a transport
  port; verify-then-rename installs; a pending/active/rejected state machine that
  rolls a model back automatically when it fails its first real use; and the
  `AURADLT1` block delta with its encoder.
- `tools/model-sign`: offline signing. The release key never enters the
  repository or CI.
- Two placeholder models with model cards, and `cargo xtask models` as the CI
  gate that refuses a model without one (Article VI rule M1).
- `ml/export_onnx`: a second implementation of the file format in Python, which
  produces byte-identical files to the Rust generator - and, where onnxruntime
  happens to be installed, compares our interpreter against it (worst difference
  1.6e-7 on the placeholder models).
- Six IPC commands and a Settings > Hardware panel that lists unavailable
  providers *with their reasons* rather than hiding them
  (`docs/adr/ADR-0008-inference-ipc-surface.md`).
- 17 error codes with runbooks: `AURA-GPU-4001..4005`, `AURA-ML-5001..5012`.
- `aura-cli verify --phase 03`: model integrity, probe, warmup, throughput,
  parity, a forced memory squeeze, cancellation, a misbehaving provider and a
  real rollback, in one run.

### Changed

- `Priority` moved to `aura-core` so the runtime does not depend on preview
  infrastructure. Phase 02's copy is untouched, and a test keeps the two in step.
- `Clock` gained `monotonic_us`, because a 0.4 ms budget measured in whole
  milliseconds can only ever read 0 or 1.
- `scripts/check-banned.sh` refuses any use of ONNX Runtime outside `aura-infer`.

### Known gaps

- No GPU backend, so two of the phase's throughput budgets are unmeasurable and
  are waived with an expiry condition in ADR-0007.
- The models are placeholders; the first trained weights arrive in phase 05.
- No network transport: nothing in the workspace opens a socket yet.
- `InferEvent` is typed on both sides and not emitted, like `IngestEvent` and
  `PreviewEvent` before it, because the Tauri shell has never been launched here.

## Phase 02.1 - Proprietary mosaic codecs, X-Trans, and a parallel decode path

A follow-up to phase 02 that closes most of the camera-coverage gap ADR-0004
opened, and narrows the performance waiver it recorded. No frozen contract
changed, so `pipeline_ver` is unchanged and cached previews stay valid.

### Added

- `aura-raw::codecs`: independent safe-Rust implementations of three formats the
  first cut refused - Nikon's compressed NEF (Huffman coding plus the body's
  linearisation curve, read from MakerNote `0x0096`/`0x008C`), Sony's ARW2 block
  coding, and Olympus's adaptive predictive ORF. Each ships with an **encoder**,
  so every decoder is tested by round trip rather than by assertion.
- X-Trans support end to end: the 6x6 array is read from a DNG's `CFAPattern` or
  a RAF's block directory, binning uses a 3x3 block instead of a 2x2 quad, and
  interpolation widens to 5x5 because a 3x3 window on X-Trans can contain no red
  at all. Tiled tier 3 stays bit-identical to a whole-image decode.
- Fujifilm RAF block-directory parsing: sensor dimensions and colour layout, plus
  the uncompressed mosaic.
- `MosaicScheme`: which decoder a mosaic needs, decided once during the container
  walk. A file that declares no compression but stores too few bytes for its own
  bit depth is now recognised as compressed, which is how Olympus marks its
  scheme.
- 16 tests in `crates/aura-raw/tests/codecs.rs`, and the new encodings added to
  the tier-2 equivalence test and to the `verify --phase 02` cycle.

### Changed

- Demosaic, area-average resize, the colour rotation and the mosaic unpack are
  parallel over output rows. Each row writes into its own slice, so output is
  bit-identical whatever the thread count - invariant 4 rules out a parallel
  float reduction here. Small images stay serial.
- `docs/camera-support.md` and ADR-0004 rewritten around the new matrix.

### Known gaps

- Canon CRX (CR3) and Panasonic RW2 are still not decoded, and compressed RAF
  is not either. Reasons per format are in ADR-0004; all three fall back to the
  embedded preview with `AURA-RAW-2007`.
- A compressed NEF whose decode table we cannot read is refused rather than
  rendered through an invented curve.
- Sony's linearisation curve lives in an encrypted sub-directory. When it is not
  reachable the render uses a documented linear expansion.
- The ADR-0004 performance waiver is renewed, not closed: parallelism made tier 3
  2.1x faster and tier 2 1.4x faster at 25 MP, which is not enough to bring a
  45 MP frame inside budget. Measurements and the two remaining routes are in
  the ADR.

## Phase 02 - RAW decode engine and the three-tier preview pyramid

**Shipped:** instant, colour-correct previews for every RAW - the camera's
embedded JPEG for triage, a 2048 px proxy for AI, and on-demand full-resolution
decode for final render.

### Added

- `aura-raw`: container parsers (TIFF/EXIF, JPEG, ISO base media, Fujifilm RAF),
  format sniffing by magic bytes, CFA unpacking for 8/10/12/14/16-bit and
  lossless JPEG (SOF3), half-size and full demosaic, tiled full-resolution
  decode, EXIF orientation, and a per-file watchdog with memory ceilings.
- `aura-raw::colour`: linear Rec.2020 working space, Bradford adaptation, the
  neutral `filmic_lite` preview curve, the camera-profile resolution chain and a
  CIEDE2000 implementation checked against published worked examples.
- `aura-cache`: content-addressed preview cache keyed by BLAKE3 plus
  `pipeline_ver`, with LRU eviction, a hard budget, digest verification on read
  and an index that rebuilds itself by scanning.
- `aura-preview`: the frozen `PreviewService` trait, strict-priority scheduling
  with de-duplication and promotion, a worker pool that leaves one core free for
  the person, and the catalog-backed source.
- IPC: `get_preview`, `prefetch_previews`, `cancel_previews`, `preview_stats`,
  `set_cache_budget`, `purge_cache`, plus the `PreviewEvent` stream.
- UI: real pixels in the grid, an LRU thumbnail store with cancel-on-scroll, and
  a cache settings panel showing "previews use X GB of Y".
- `aura-cli`: `raw-fixtures`, `previews`, and `verify --phase 02`.
- Synthetic RAW fixtures: eight bench bodies, three mosaic encodings and a
  colour chart, so the decoder is tested without a single camera file.
- Docs: `docs/camera-support.md`, `docs/runbooks/previews.md`, ADR-0003
  (colour pipeline), ADR-0004 (decode backend), ADR-0005 (preview IPC).

### Changed

- `aura-catalog`: `preview` table now written and read (`upsert_preview`,
  `preview_row`, `count_previews`, `photos_without_preview`,
  `primary_file_for_photo`).
- `perf/budgets.toml`: phase 02 stage budgets, plus size budgets for the cache
  and for peak resident memory.
- Frozen contracts re-locked for the preview IPC additions (ADR-0005).

### Known gaps

- Proprietary mosaic compressions (compressed NEF and ARW, RW2, Canon CRX,
  X-Trans) are not decoded; those files render tier 2 from the embedded preview
  and are flagged `AURA-RAW-2007`. See `docs/camera-support.md`.
- The scalar CPU decoder misses the per-image budget at 45 MP; waived for this
  phase in ADR-0004 with measurements.
- No GPU path, no HEIF.

## Phase 01 - Foundation, catalog and wedding ingest

Workspace, error taxonomy with runbooks, SQLite catalog with the six-step
refusal chain, idempotent ingest with multi-camera clock alignment, the job
graph with leases, the typed IPC surface, the virtualised grid, fixtures, CI and
budgets. See `docs/progress/PHASE-01-EXIT.md`.
