# ADR-0049 - Generative cleanup: a filter that runs before a score, real pixels before invented ones, and the four things this phase does not build

**Status:** accepted · **Date:** 2026-08-29 · **Phase:** 24 · **Supersedes:** nothing

Phase 24 section 4 asks for `docs/generative-policy.md` and no ADR by name. It needs two anyway,
and this is the first. Section 5 freezes a proposal whose six supporting types it does not define;
section 6.2 describes a safety engine whose ordering relative to the score decides whether the
whole phase is safe or merely careful; section 2.1 asks for a diffusion inpainting tier that
cannot exist in this build; and section 6.1 asks for a learned detector on a vocabulary that has
no labels here. The second document is [ADR-0050](ADR-0050-cleanup-ipc-surface.md), which covers
the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned to
phase numbers.

## 1. Context

Twenty-three phases have measured a photograph, decided how it should look, and repaired what the
camera got wrong. This is the first that **removes something the camera got right**.

That is a different kind of risk from anything before it, and the difference is not one of degree.
Phase 22 removed noise, which is not information. Phase 23 removed framing, and section 1 of its
own document called that the most dangerous thing in the product because a cropped frame does not
look cropped. This phase removes an *object that was there*, and replaces it with pixels that were
not. When it goes wrong the result is not a photograph edited differently from the one somebody
wanted - it is a photograph containing something that never existed, delivered to a couple who
will keep it for fifty years.

Section 1 states the commercial case and the constraint in the same breath:

> Generative tools fail publicly and embarrassingly. Making safety structural - size limits,
> semantic denylists, identity protection, confidence gating and mandatory disclosure - is the only
> responsible way to ship this.

Four things separate this phase from its predecessors.

**The failure is invisible in the output and obvious in the wild.** A warped railing or a repeated
flagstone reads as fine at gallery size and as a scandal at print size. Every previous phase's
failure mode is visible in the thing it produced; this one's is visible to the client's
brother-in-law with a 27-inch monitor, eight months later.

**Refusing is nearly free and removing is not.** A distraction left in place costs a photographer
two minutes of manual work they were not going to spend anyway. A wrongly removed heirloom costs
them the client. The asymmetry is enormous and it should be visible in every threshold in the
phase, which is why almost every decision below resolves toward doing nothing.

**This is the second phase to composite two photographs, and the first to invent pixels.** Phase 21
established that a borrow may only replace pixels that carry no information - a specular sheet has
destroyed the record, a closed eye *is* the record. This phase inherits that rule and needs a
second one for the case phase 21 never had: what happens when there is no sibling frame and the
pixels must be made up.

**The policy document is a deliverable of this phase, not a description of it.** Section 8 step 1
requires `docs/generative-policy.md` published and co-signed before any code. It is, and this ADR
is written to be consistent with it rather than the other way round.

## 2. Decision: the safety engine is a filter that runs before the score, and the score has no term for safety

`safety::check` runs first. `detect::rank` never sees a candidate that failed it. The proposal
score - salience times removability - has **no term for faces, hands, dresses, rings or cake**, and
no weight anybody could tune to trade one against a cleaner background.

This is phase 23's rule, restated in the phase where it matters most, and phase 12's before that:
a guarantee outranks a preference, and the way you make that true is structural rather than
numerical. A penalty is a trade. Any penalty large enough to be safe across four hundred frames
loses on the one frame where the salience term is most confident - and the frame where a
distraction is most salient is the frame where it is nearest the subject.

The ordering is not a convention that could drift. `SafetyVerdict` is a field of `CleanupProposal`,
`CleanupProposal::new` refuses to construct one whose verdict is absent, and the scoring function
takes a `&SafeCandidate` - a type that can only be produced by `safety::check` returning `allowed`.
A future caller cannot score an unchecked candidate because it cannot obtain the argument.

**Every blocked candidate is recorded with the check that blocked it.** Section 6.2's last line
asks for this and it is worth more than it looks: it makes the safety engine auditable, it is how
the adversarial audit of section 10.1 is scored, and it is how a photographer learns what the
product will never do. A refusal is a row, for the reason phase 22 gave - here the refusal is the
product working.

## 3. Decision: the denylist is an intersection against phase 18's masks, and an absent mask blocks rather than allows

The semantic denylist works by intersecting the candidate region with the masks phase 18 produces
for faces, skin, hands, dress, rings and cake. Overlap above 1 % of the candidate's own area blocks
it.

The consequential half of that sentence is what happens when the mask is not there. Phase 18's
segmenter is a placeholder in this build and `MaskField` is not wired into any pass, so on a real
photograph the denylist would intersect against nothing and find no overlap. **A missing mask is
read as "cannot prove this is safe", not as "no overlap found", and blocks the candidate.**

This inverts the convention every phase from 19 to 23 used, where an absent input *gated* an
operation down to nothing. The difference is that those phases were deciding how much of an
improvement to apply, and the safe direction was less. Here the safe direction is none, and
"gated to zero" and "blocked" happen to coincide - but they are different reason codes, they are
different rows, and only one of them is a claim that the product checked.

The practical consequence is that **this build proposes no removals on a real photograph at all**,
and that is the correct behaviour rather than a limitation to work around. It is condition C1 of
the exit report.

## 4. Decision: sources are tried in a fixed order - borrow, fill, inpaint - and the order is in the type

`CleanupMethod` is `BorrowFrom(ImageId) | ClassicalFill | Inpaint { model }`, and
`source::select` tries them in that order, always, with no configuration that reorders them.

Borrowed pixels are a record of the room. Filled pixels are texture that is already in the
photograph, moved. Inpainted pixels are a guess. The ordering is the policy document's "real pixels
first" as an algorithm, and it is fixed rather than tunable because a studio that could reorder it
would eventually reorder it for speed.

`ClassicalFill` is preferred over `Inpaint` for a reason worth stating precisely: it **cannot
hallucinate structure**. It copies patches from the surrounding texture, so its failure mode is a
visible seam or a repeated tuft of grass - ugly, findable, and not a fabrication. A diffusion model
asked to fill the same region can produce a beautiful railing that was never there. The first
failure is caught by a photographer glancing at a thumbnail; the second is not caught at all.

## 5. Decision: the diffusion tier is declared, refused and reachable - it is not stubbed out

Section 2.1 asks for local diffusion inpainting via a model pack, or the phase 04 cloud path with
consent. Neither exists here: there is no diffusion model in `models.lock`, the interpreter of
phase 03 implements a documented ONNX opset 13 subset with no `ConvTranspose` and no `Resize`, and
TLS is waived so the cloud path reaches no public provider.

The tier is **in the frozen contract anyway**, and `inpaint::solve` returns
`Err(CleanupCode::InpaintUnavailable)` on every call.

This is phase 22's shape for face recovery rather than phase 20's for blemish detection, and the
argument is the same one turned up: phase 20 shipped a measurement in place of an untrained model
because a difference-of-Gaussians is a real detector whose failure is finding fewer marks. There is
no measurement that stands in for a diffusion model. What would stand in is the classical fill,
which is *already the tier below it* and is already tried first - so a "fallback" from inpaint to
fill would be the product doing what it had already decided was insufficient, and calling the
result an inpaint.

Refusing keeps the disclosure honest. `CleanupMethod::Inpaint` in a stored row means a diffusion
model ran. There is no build in which it means something else.

## 6. Decision: the detector is a measurement, and its vocabulary is a closed enum rather than a model output

Section 6.1 asks for a learned detector on a labelled wedding-distraction vocabulary. Section 9's
DATA row asks for that vocabulary on 10,000 frames. There are no wedding photographs here, so there
are no labels, so there is no detector to train.

What ships is `detect::candidates`, which finds regions that are: small, high in local contrast
against a low-variance surround, far from every subject box phase 06 produced, and near the frame
edge or in the background plane. That is not a bin detector. It is an *unexplained-salience*
detector, which section 6.1 asks for as the second half of the pair, and it is the half that can be
built from measurement.

`DistractionClass` is a closed enum in the frozen contract - `ExitSign`, `Bin`, `Cable`,
`GafferTape`, `Bottle`, `Chair`, `PhoneScreen`, `StrayHand`, `BackgroundPerson`, `Unclassified` -
and the measurement returns `Unclassified` for everything it finds. The enum is frozen now rather
than when a detector exists because phase 13's ledger, phase 27's QC and the delivery report all
name a class, and a vocabulary that arrives with the model is a vocabulary three phases have
already stored strings from.

`Unclassified` is not a null. It is what makes the cautious path correct: **a candidate whose class
is unknown cannot be story-irrelevant**, so it never reaches the confidence needed for unattended
application, and it always requires review.

## 7. Decision: the cloud editorial judgement is built, and it is the first cloud task that can only say no

Section 7 asks for a vision-reasoning call on candidates that pass every mechanical check but whose
removability confidence sits between 0.6 and 0.9. Phase 12 declined to build its cloud tie-breaker
and recorded why: with four placeholder heads underneath, a 0.02 score difference is noise, and
every call would have spent a photographer's money arbitrating between two random projections.

This one is built, and the difference is the direction it can move a decision. `CleanupJudgement`
can turn a *proposed removal* into a *refusal*. It cannot turn a refusal into a removal, it cannot
raise a confidence, and it cannot reach a candidate that failed a mechanical check. Its offline
fallback is "do not remove", which is also its answer when it is uncertain, which is also what
happens when the key is absent.

A cloud call that can only ever make the product do less is a cloud call whose failure modes are
all safe. That is the property phase 12's tie-breaker lacked, and it is why the same repository
reaches opposite conclusions about two superficially similar features.

## 8. Decision: the self-check measures the excursion, not the edit, and it reverts rather than warns

`selfcheck::inspect` runs over the *rendered* result and looks for three failures: texture repeated
at a spatial period that does not occur elsewhere in the frame, straight lines whose direction
changes inside the patched region, and gradients that terminate at the patch boundary. A region
that fails reverts itself and the proposal is stored as `not_safely_removable`.

Phase 19 learned that a halo test cannot be a before/after gradient ratio, because every local
brightening increases the step at its own boundary and the ratio scores the edit's size. Phase 22
learned the same lesson for ringing and stated the general form: what a defect is, is a pixel
pushed **beyond the range its own neighbourhood had before the operation**. Both are the same trap,
and this phase has the third instance of it: an inpaint necessarily changes the pixels inside its
own region, so anything that measures how much they changed measures the removal rather than the
artefact.

So each of the three checks compares the patch against the *rest of the frame* rather than against
its own before-state. A repeated texture is only evidence if that period is absent elsewhere. A
warped line is only evidence if the line was straight where it enters the patch.

**The revert is automatic and happens before a person sees the proposal**, which is a rule about
where the check sits rather than about what it measures: a self-check that ran after review would
be asking a photographer to catch what the product already knew.

## 9. Decision: nothing in this phase applies a removal by itself, and the exception is narrow enough to name

Removals are proposals. `CleanupQueue` holds them, the panel shows a before and an after, and
`apply` takes an accepted proposal id. There is no code path from `plan` to a written recipe.

Section 6.4 permits one exception and it is a studio's own decision: in Zero-Touch mode, a
`BorrowFrom` or `ClassicalFill` proposal at calibrated confidence >= 0.97 may apply unattended.
`Inpaint` never may, unless a studio sets a separate switch that is off at installation.

Phase 13's autonomy policy is raised one band for this phase, which is section 5's `autonomy` field
and is why it exists on the proposal rather than being computed by the caller. And phase 13's
`uncalibrated_raises` still applies underneath: nothing in this build is calibrated, so every
proposal moves one further band toward review, so **nothing in this build can apply unattended at
all**. That is a property of the composition of two rules neither of which was written for this
phase.

## 10. Decision: `ProposalId` names the proposal, including a refusal

Phase 24 section 5 freezes `CleanupProposal.id: ProposalId`, but the frozen identifier contract did
not contain that type. `crates/aura-core/src/contract/ids.rs` therefore gains
`typed_id!(ProposalId, "prp")`.

The identifier belongs to the proposal rather than to an applied cleanup. Blocked and rejected
proposals are evidence for the delivery report and the adversarial audit, and a photographer's
rejection must survive a resumed or repeated pass. Issuing a `CleanupId` only when pixels changed
would leave refusals anonymous and let the same rejected proposal return as though it were new.

This **amends a frozen contract**. The amendment is recorded here before `contracts.lock` is
re-locked, following the same ADR-then-re-lock rule used for phase 09's `FaceRef` amendment and
phase 23's recipe coefficients.

## 11. Consequences

- `aura-generative` is the twenty-seventh crate. It depends on `aura-vision` for masks, `aura-core`
  for the contract, `aura-catalog` for the store and `aura-render` for the self-check's rendered
  input. It does **not** depend on `aura-cloud`; the judgement task lives behind `aura-core`'s
  frozen `CloudTask` shape like every other.
- Migration 24 stores proposals, their safety verdicts and their disclosures. A disclosure is
  written in the same statement as the removal and a trigger aborts any statement that would
  remove it, which is phase 21's shape for its borrow disclosure.
- `crates/aura-generative/tests/one_choke_point.rs` is the fifth grep-as-a-test in the repository,
  after `colour_discipline.rs`, `no_recipe_writes.rs`, `no_template_writes.rs` and
  `no_render_calls.rs`. It fails the build if `fill::` or `inpaint::` is called from anywhere but
  `source::select`.
- Phases 27 and 28 consume this. Phase 27 has to be able to say why a background looks smeared;
  phase 28 must know what ran unattended. Neither needs the detector, the patch search or the
  self-check's internals, which is why the contract is in `aura-core`.

## 12. What was considered and rejected

**A removability score with a large safety penalty.** Rejected in section 2, and it is worth
recording that it is the design most products actually ship, because it is simpler and it is right
99 % of the time. The 1 % is a bride's hands.

**Treating an absent mask as no overlap.** Rejected in section 3. It would have let this build
propose removals on real photographs, which would have looked like the feature working.

**A "creative fill" mode behind a warning dialog.** Rejected. A warning is a thing a user clicks
past on the second use. The policy document's claim that AURA never adds content is worth more than
the feature, and a switch that turns the claim off makes it a default rather than a promise.

**Letting a studio reorder borrow, fill and inpaint.** Rejected in section 4. Diffusion is faster
than a homography search across a moment, so the reordering would be chosen for the reason that
makes it worst.

**Stubbing `Inpaint` to fall back on `ClassicalFill`.** Rejected in section 5. It would put a
`method = inpaint` disclosure on a row where no model ran, which is the one kind of dishonesty this
phase cannot afford.
