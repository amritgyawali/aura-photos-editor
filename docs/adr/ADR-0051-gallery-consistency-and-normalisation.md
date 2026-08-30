# ADR-0051 - Gallery consistency: anchors rather than averages, a solver that must be idempotent, and a correction that is a residual on a residual

**Status:** accepted · **Date:** 2026-08-30 · **Phase:** 25 · **Supersedes:** nothing

Phase 25 section 4 names no ADR. It needs two, and this is the first. Section 5 freezes three
shapes - `SceneNode`, `NodeTarget`, `NormalisationDelta` - whose five supporting types it does not
define; section 6.2 asks for a solver that is simultaneously damped, bounded, change-point aware
and *idempotent*, and three of those four pull against the fourth; section 6.3 makes a measurable
promise about a person's skin across a whole wedding; and section 2.1 asks for the normalisation
to preserve a candle-lit vow inside a bright ceremony, which is the same sentence as "do not do
your job" read from one angle. The second document is
[ADR-0052](ADR-0052-gallery-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned to
phase numbers.

## 1. Context

Twenty-four phases have decided things about *one photograph at a time*. Phase 15 asked what colour
the light was in this frame. Phase 16 asked how this frame should be graded. Phase 20 asked what to
do to the skin in this frame. Every one of them is a function whose domain is a photograph.

This phase's domain is **a wedding**. That is not a bigger version of the same problem; it is a
different problem, and three properties change with it.

**The failure is invisible frame by frame and obvious in sequence.** Phase 15 can be within its own
200 K tolerance on every single frame of a ceremony and still produce a ceremony that visibly
warms and cools as the gallery scrolls, because 200 K of independent error either side of a mean is
a 400 K swing between adjacent frames. Every per-frame gate in the product can be green while the
thing a client actually looks at is wrong. Section 1's own words: "galleries where skin tone and
warmth drift from frame to frame look amateur even when every individual frame is fine."

**The correction has no natural size.** A per-frame decision is bounded by the photograph - there is
only so much exposure a frame wants. A consistency correction is bounded by *nothing*, because the
target is another frame and the distance to it can be arbitrarily large. A solver with no bound
will happily move a frame 1,800 K to match its node and destroy the reason it was shot.

**Running it twice must be a no-op, and that is not free.** Every earlier pass is idempotent
trivially: it recomputes the same answer from the same pixels. This one computes a *delta from a
target derived from the frames it is about to move*. Naively implemented, the second run measures
the already-moved frames, derives a new target from them, and moves them again. A gallery run
through consistency four times would converge on a colour nobody chose. Section 6.2 makes
idempotence a test rather than an aspiration and section 12 names solver drift as a failure mode;
this ADR makes it a property of where the delta is measured from.

Two further constraints come from outside the phase document.

The nine invariants require every decision to carry `confidence` and `reasons[]`, and require
determinism. A solver that iterates to a fixed point in floating point, over a set whose iteration
order comes from a hash map, satisfies neither.

And phase 15's own header says the thing this phase most needs to remember: **ask the room, not the
winner.** Phase 15 shipped with a scene description derived from whichever illuminant hypothesis
won a cost race, and the symptom was a label that was right on a project's first frame and absent
on its four-hundredth. This phase derives a *target for a whole node* from a handful of its frames,
which is the same shape of computation, and inherits the same trap.

## 2. Decision: the delta is measured from the un-normalised estimate, always, and that is what makes it idempotent

`normalise::solve` reads `ToneEstimate` and `ColourDecision` - phase 15's and phase 16's stored
per-frame answers - and never reads a `NormalisationDelta`. The target is computed from the
anchors' *estimates*, not from their normalised values. The delta is

```text
d = damping * (target - estimate)
```

and both `target` and `estimate` are properties of the un-normalised world.

So the second run computes exactly the same number as the first, and writing it a second time
changes nothing. Idempotence is not achieved by detecting that the pass has already run, and it is
not achieved by convergence: it is achieved because **the input to the solver is immutable with
respect to the solver's own output**. `tests/eval/consistency_eval.rs` asserts a second run moves
every frame by less than `IDEMPOTENCE_EPSILON`, which is a regression guard rather than the
mechanism.

Three alternatives were considered.

*Iterate to a fixed point.* Rejected. It converges toward the mean of the node, which is precisely
the mediocrity section 6.1 exists to avoid, and it converges at a rate that depends on floating
point ordering.

*Detect that a frame is already normalised and skip it.* Rejected: it makes the answer depend on
what has previously been written, which breaks resumability - a run killed at 50 % would produce a
gallery whose second half was solved against different inputs from its first.

*Store the pre-normalisation estimate on the delta row and re-solve from it.* This is what actually
happens, but as a **record** rather than a mechanism: `NormalisationDelta` carries `from_cct_k`,
`from_tint` and `from_exposure_ev` so a panel can show the movement and an audit can check it. The
solver does not read them back; it reads the tone store.

The consequence a later phase must respect: **a normalisation delta is a residual on top of phase
15 and 16's residual on top of the camera.** Three layers, applied in that order, at merge time.
Phase 26 adds a fourth. Any phase that finds itself computing an absolute temperature from a delta
has misunderstood the shape - this is phase 17's rule about style profiles, in a second place.

## 3. Decision: anchors are the best-judged frames, the statistics over them are robust, and a node with too few is left alone

Section 6.1 is unambiguous that averaging produces mediocrity, and this phase follows it exactly.
`anchors::select` ranks a node's frames by a weighted product of phase 15's white-balance
confidence, subject-exposure quality, primary-identity presence and the *absence* of mixed light,
and keeps between `MIN_ANCHORS` (3) and `MAX_ANCHORS` (5).

The statistics over those anchors are a **trimmed mean for the scalars and a component-wise median
for the chromaticity**, because one anchor that is wrong should not move the target and a mean over
three samples has no resistance to that at all.

A node with fewer than `MIN_ANCHORS` usable frames gets **no target and no deltas**, and
`GalleryCode::NodeUnanchored` says so. This is the same shape as phase 15's own refusal - its
`reference_frames` returns empty for a segment with fewer than three candidates - and the argument
is stated there: normalising a chapter toward a frame that is not representative is worse than
leaving it alone. A node the product could not anchor and a node that needed no correction must
never be the same query, so they are separate codes and separate rows.

A pinned anchor is authoritative. `Gallery::pin_anchor` sets `user_pinned` and the re-selection
statement excludes pinned rows, exactly as `identities.user_locked`, `segments.user_locked`,
`moments.user_locked` and `masks.user_edited` are excluded. Phase 18's lesson applies here too and
is why the protection is two statements rather than one: the `DELETE` is guarded, *and* a pinned
image is skipped on re-insert, because the unique key would otherwise let `INSERT OR REPLACE`
delete the row it conflicts with.

## 4. Decision: bounds are hard, damping is soft, and the bound is recorded on the row

`NormalisationDelta::bounded_by` is `Option<Bound>` and names which of the five bounds bit. The
defaults, all owned by `consistency.toml` and floored/ceilinged by the contract:

| Bound | Default | Why |
|---|---|---|
| `Bound::Cct` | 450 K | Section 6.2's own number. About the largest shift that reads as "the same room" rather than "a different white balance". |
| `Bound::Tint` | 12 units | Scaled from the CCT bound at the tint sensitivity phase 15's tolerance implies (200 K : 4 units). |
| `Bound::Exposure` | 0.35 EV | Section 6.2's own number, and a third of a stop is under the just-noticeable difference for a sequence. |
| `Bound::Contrast` | 8 recipe units | An eighth of the subtlety ceiling phase 16 already enforces per frame. |
| `Bound::Saturation` | 6 recipe units | Lower than contrast, because a saturation move is the one most visible on skin. |

Damping is applied first and the bound clamps afterwards, so a bound bites only when a frame is a
long way from its node. That ordering is deliberate: damping-after-bounding would make the bound a
target rather than a limit, and every distant frame would land exactly on it, which is a visible
band of identically-corrected frames.

The config file may **lower** a bound and may not raise one. `policy::Consistency::load` refuses a
file that widens what the contract owns - the rule phase 21 wrote as "a ceiling can be lowered by a
studio and raised by nobody", and phase 22 inherited. There is no strength field anywhere on the
IPC surface.

## 5. Decision: a change point splits a node before anchors are chosen, not after deltas are solved

Section 2.1's candle-lit vow inside a bright ceremony is the whole difficulty of this phase in one
sentence. If the vow is in the same normalisation group as the ceremony, the only two outcomes are
that the vow is flattened toward the ceremony or that the ceremony is dragged toward the vow.
Neither is acceptable, and no damping factor makes them acceptable - damping makes both happen a
little.

So `changepoint::split` runs **before** `anchors::select`, over the node's frames in capture order,
on a three-channel signal: the estimated CCT, the estimated tint and the subject luminance, each
normalised by its own bound so the three are commensurable. A boundary is declared where the
robust mean either side differs by more than `SPLIT_SIGMA` times the within-run spread and both
sides are at least `MIN_RUN` frames long. Each side then becomes its own node with its own anchors
and its own target.

The three signals section 2.1 names explicitly are all detected by this: a flash toggled on shows
as a step in CCT and subject luminance together, a sunset as a ramp that accumulates past the
threshold, a venue change as a step in all three. Two are also detected *for free* from phase 15's
own output rather than being re-derived: `IlluminantKind::is_intentional` is true for stage and
candle light, and a frame whose dominant illuminant is intentional is excluded from anchor
candidacy and given `GalleryCode::MoodPreserved` with a zero delta. Phase 15 already decided that a
purple dance floor stays purple; this phase does not get a second opinion on it.

The alternative - solve first, then detect that some frames moved a long way and un-move them -
was rejected for the reason phase 23 rejected nudging a rejected crop back inside the safety
filter: a correction applied after the fact leaves the frames that did *not* trip the threshold
still normalised toward the wrong target.

## 6. Decision: the skin promise is measured per identity, against that person's own frames, and the correction is capped by the light

Section 6.3 promises the same person's skin dE00 spread across the gallery is at or below 2.0. That
is a claim about a measurement, so `skin_consistency` measures it: `SkinTarget` is the robust
central tendency of one identity's skin chromaticity and luminance over their **best-lit frames**,
and `skin_consistency::correct` produces a `SkinCorrection` inside phase 18's identity-scoped skin
mask only.

Four properties are structural rather than promised.

**The target is that person's own frames.** There is no ideal-skin constant in this contract, in
`consistency.toml`, in migration 25 or anywhere in the code path, exactly as there is none in phase
15's. The gate scans for one on every run. A fixed target is how an editor lightens dark skin while
believing it is correcting a cast.

**The correction is capped by the light rather than by a global number.** `SKIN_CHROMA_CAP` bounds
how far a frame's skin may be moved in `u'v'`, and the cap is *reduced* toward zero as the frame's
dominant illuminant becomes more intentional. A candle-lit face may stay warm; it may not go
magenta. That is section 6.3's own sentence as an arithmetic rule.

**It is a residual on the phase 16 grade, and phase 16's skin guard re-runs after it.** Phase 17
wrote this rule - the shift happens before the guards, and every guard re-runs after it - and this
phase is its third application. A consistency correction that would move somebody's skin outside
phase 16's hue and chroma ceilings is a correction phase 16's guard withdraws.

**Below `MIN_SKIN_FRAMES` an identity gets no target.** Phase 15's argument for `MIN_LOCUS_SAMPLES`
applies unchanged: a target fitted to two frames is a target fitted to one lighting condition, and
a weak target is worse than none because it looks like evidence.

## 7. Decision: an outlier is what is left after normalisation, and it is a row rather than a counter

`outlier::detect` runs on the **post-delta** residual, not on the raw deviation. A frame 900 K from
its node that the bound could only move 450 K is an outlier with a 450 K residual; a frame 300 K
away that was fully corrected is not an outlier at all. Section 6.4 asks for the deviation
quantified - "+310 K warmer than node anchors, magenta skin cast 4.2 dE00" - and that sentence is
only true of the residual.

Outliers are rows in `gallery_outlier` with their four residual components and a reason code, not a
count. Phase 24 made the same choice about refusals and paid forty per cent of its storage for it;
here it is cheaper and the argument is the same: phase 27's QC queue is *fed* from these rows, and
"which frames drifted" is unanswerable from a counter.

## 8. Decision: this phase writes no recipe and moves no pixel

`aura-brain-gallery` does not depend on `aura-recipe` and does not depend on `aura-render`.
`crates/aura-brain-gallery/tests/no_recipe_writes.rs` is the sixth grep-as-a-test in the repository,
after `colour_discipline.rs`, `no_recipe_writes.rs`, `no_template_writes.rs`, `no_render_calls.rs`
and `one_choke_point.rs`. It fails the build if this crate calls `schema::merge`, opens a file, or
grows its own tone solver.

The deltas are stored. `aura-app` merges them, through `aura_recipe::schema::merge`, which is the
only function in the workspace permitted to write a recipe and the only place `user_edited_fields`
is honoured. A frame a photographer has set the temperature on by hand is a frame this phase's
delta cannot move, and that is enforced where it has been enforced since phase 14 rather than a
second time here.

## 9. Decision: no cloud call, and no model

Section 7 is explicit that no cloud AI call happens in this phase and the gateway stays idle.
`aura-brain-gallery` does not depend on `aura-cloud`, so that is a property of the dependency graph
rather than a rule somebody follows.

This phase also ships **no model** - the fifth since phase 08, after 17, 23 and 24 - and the reason
is phase 17's and phase 23's rather than phase 24's: there is nothing to train. Anchor selection is
a ranking over numbers other phases already produced, the solver has a closed form, the change-point
detector is a two-sample statistic and the outlier detector is a threshold. What is missing is not
weights but **weddings**: section 9's DATA row asks for labelled intentional lighting transitions on
fixture weddings, and there are none in this repository, so every gate in section 10.1 is measured
against synthetic galleries whose drift was authored and whose transitions are known by
construction.

## 10. Consequences

`GalleryService` is the twenty-first frozen service and the first whose subject is a *set of
photographs*. Phase 26 matches a second camera into these nodes, phase 27 reads these outliers as
its QC input, phase 28 acts on them unattended and phase 29 builds albums out of a gallery this
phase has already made coherent. No phase may keep its own scene-node tree, its own anchor
selection or its own idea of what a consistent gallery is.

`NodeId` is added to `aura-core`'s `ids.rs`, the fourteenth typed id and the third that names a part
of something rather than a whole thing, after `MaskId` and `ProposalId`. A node is a sub-range of a
segment; `(segment_id, ordinal)` would name it, and that was the alternative. It is not what shipped
because a node can be split by a change point, merged by a photographer and re-parented as the tree
grows, and an ordinal that renumbers on every one of those is not something an anchor row, a delta
row or an outlier row can point at across a re-analysis.

Two version columns rather than three. `analysis_ver` invalidates the tree, the anchors, the target
and the deltas, because all four come from this build's arithmetic. `policy_ver` invalidates every
number that was compared against a **bound or a damping factor**, because those are a product
decision a release can move without changing a line of solver code. There is no `model_ver`,
because there is no model - and adding one for symmetry would be a column that can never change,
which is a column that will eventually be compared against and mean nothing. `AURA-ML-5127` is the
version-drift code and it is the tenth of its kind.

## 11. What was deliberately not built

**No incremental re-solve of the whole project on an anchor change.** Section 11 budgets 6 s for a
re-solve after one anchor change and what ships re-solves *that node* and no other, which is faster
than the budget and correct: a node's target depends on its own anchors and on nothing outside it.
The budget is met by the structure rather than by an optimisation.

**No cross-node harmonisation.** Two adjacent nodes of the same segment are normalised
independently, and nothing pulls them toward each other. That is what change-point splitting is
*for*: a node boundary exists because the light genuinely changed, and harmonising across one would
undo the split. Phase 26 crosses cameras and phase 29 crosses chapters; neither is this phase.

**No automatic acceptance.** Every delta is stored and none is merged into a recipe by this phase.
The gallery panel's batch accept is a photographer's action, on the wire, recorded. Phase 28 is
where anything acts unattended, and it cannot ship until a calibration does - phase 13's condition
C2.
