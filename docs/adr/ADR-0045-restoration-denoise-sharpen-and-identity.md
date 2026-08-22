# ADR-0045 - Restoration: two stages rather than one, evidence-chosen strength, and an identity constraint that can only skip

**Status:** accepted · **Date:** 2026-08-21 · **Phase:** 22 · **Supersedes:** nothing

Phase 22 section 4 asks for no ADR by name. It needs two anyway, and this is the first. Section
5 freezes a plan whose four supporting types it does not define; section 2.1 states an
order-of-operations requirement that the render graph phase 14 froze cannot satisfy if
restoration is one stage; section 2.1 also asks for a cloud offload that section 7 forbids in
the same document; and the two models section 4 asks for cannot be trained in this repository.
The second document is [ADR-0046](ADR-0046-restore-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned
to phase numbers.

## 1. Context

Twenty-one phases have decided what a photograph *is* and how it should *look*. This is the
first that repairs one. The distinction matters for the design, because a repair has a failure
mode none of the previous phases has: it can succeed at its own objective and destroy the
photograph anyway. A denoiser that removes every trace of noise from a bride's lace has met its
metric and ruined the frame. A sharpener that recovers every edge has also recovered a halo
around every edge. A face-recovery model that restores a plausible face has restored *a* face.

Section 1 states the commercial argument and then states the constraint in the same breath:

> Restraint is the differentiator: applying denoise and sharpening everywhere is what makes
> AI-processed images look synthetic.

Four things separate this phase from its predecessors.

**The heavy compute is real and it is not on the interactive path.** Section 11's budget is
2.5 s for one 45 MP denoise on a reference GPU. Every previous pixel phase produced a decision
in milliseconds and left the pixels to the renderer. This one has to be scheduled.

**Two of the three operations already have a stage in the frozen render graph, and they are not
adjacent.** Phase 14 froze `ORDER` with `NoiseReduction` at index 6 and `Sharpen` at index 20,
thirteen stages apart, with `Masks`, `Retouch` and `Restoration` between them. Section 2.1 of
this phase requires denoise *before* local retouch and sharpening *last*. Those two sentences
are satisfiable, and section 2 of this document is how.

**The identity constraint is the first guarantee in the product that can only ever refuse.**
Phase 16's skin guard re-solves the grade. Phase 20's texture guard re-solves and then
withdraws. This one re-solves, and then withdraws, and *there is no version of it that produces
a face at reduced strength once the distance is exceeded* - because a face that has drifted a
little is a face that has drifted.

**No model in this phase can be trained here.** Section 8 step 2 asks for paired noisy/clean
captures across twenty bodies and six ISO steps. There are no camera files in this repository at
all; phase 02's exit report has been carrying that condition since phase 03. Section 6 of this
document is what ships instead, and it is not the same answer phase 20 gave.

## 2. Decision: restoration occupies two stages of the frozen graph, and the tier routes to the first

Phase 14 froze this order and documented `Stage::Restoration` as "Denoise, face recovery,
deblur":

```text
... NoiseReduction(6) ... Masks(17), Retouch(18), Restoration(19), Sharpen(20), Geometry, OutputTransform
```

Section 2.1 of this phase asks for "denoise before local retouch and sharpening; sharpening as
the last pixel operation before output transform", and section 10.1 makes it a test: "Order of
operations enforced (denoise before retouch, sharpen last) - render-graph test." Running denoise
inside `Stage::Restoration` fails that test by thirteen stages.

**Decision.** The three operations of this phase are placed where the graph already has room for
them, and none of `ORDER` moves:

| Operation | Stage | Recipe path |
|---|---|---|
| Denoise | `NoiseReduction` (6) | `global.noise.{luminance,colour,detail,model}` |
| Face recovery | `Restoration` (19) | `restoration.face_recovery` |
| Deconvolution sharpen | `Sharpen` (20) | `global.sharpen.{amount,radius,detail,masking}` |

`restoration.denoise` carries the **tier name** - the audit record of which of the four tiers
this frame was given - and is the field the panel binds to. `restoration.deblur` stays at zero
for the whole phase, because deblur is motion-blur removal and section 2.2 puts that out of
scope explicitly: "Motion-blur removal on heavily blurred frames (rejected in Phase 12 instead
of rescued)."

This is not a workaround for the frozen order; it is what the order already says. Denoising is a
*sensor-domain* operation on linear data with the camera's noise model as conditioning, and
every stage between index 6 and index 20 reads texture as signal: clarity, texture and dehaze
amplify it, retouch's frequency separation splits it into bands, and sharpening multiplies it.
Denoising after any of them means retouching noise and then removing the retouched noise, and it
means phase 20's texture guard measuring a band ratio over grain. `Stage::NoiseReduction`'s
position in phase 14's order was correct before this phase existed.

Two edits to `crates/aura-render/src/graph.rs` follow, and neither is a frozen file:

1. `wants_restoration` drops `recipe.restoration.denoise != "off"`. A tier alone no longer
   enables `Stage::Restoration`, because with denoise at stage 6 there would be nothing for
   stage 19 to do and the plan would report a stage that ran and changed nothing.
2. `stage_for("restoration.denoise")` returns `Stage::NoiseReduction` rather than
   `Stage::Restoration`.

The second is a **bug fix as much as a routing change**. `earliest_affected` is phase 14's cache
invalidation rule: it answers "from which stage must this render be recomputed". Today a change
to `restoration.denoise` answers "from stage 19", which would let a cache serve the buffer it
had already denoised at stage 6 under the previous tier. Nobody has hit it because nothing
writes the field yet. This phase is what writes it.

### 2.1 Five spellings differ from section 5, and here is each one

Section 5 freezes:

```rust
pub struct RestorePlan {
    pub image_id: ImageId,
    pub denoise: DenoiseTier,           // Off | Light | Standard | Strong
    pub denoise_reason: Vec<Reason>,
    pub sharpen: Option<SharpenSpec>,   // { kernel_sigma, amount, mask, skin_attenuation }
    pub face_recovery: Option<f32>,     // strength 0..0.4, capped
    pub run_where: RunWhere,            // LocalGpu | LocalCpu | Cloud
    pub selfcheck: Option<ArtefactReport>,
    pub confidence: f32,
}
```

Everything named survives. Five things are spelled differently, and the reason in each case is a
rule an earlier phase already wrote.

**`denoise_reason: Vec<Reason>` becomes `reasons: Vec<RestoreReason>`.** There are three
decisions in this plan and the frozen field explains one of them. A refused sharpen and a
skipped face recovery are exactly the outcomes a photographer asks about - "why is this frame
still soft" is the commonest question this phase will ever generate - and invariant 2 says every
AI decision carries reasons. `RestoreCode::subject()` names which of the three decisions each
reason is about, and `RestorePlan::denoise_reasons()` returns section 5's subset, so a caller
that wanted the frozen field still has it.

**`face_recovery: Option<f32>` keeps its name and gains `recovered: Vec<RecoveredFace>`
beside it.** The scalar is the plan-wide strength that survived, which is what the recipe
carries and what section 5 asks for. It cannot carry the per-face record, and the per-face
record is the whole guarantee: a frame with four faces where one drifted and was skipped must
not read as a frame where the strength was lowered. Phase 21 made the same split for the same
reason, per family rather than per plan.

**`sharpen: Option<SharpenSpec>` is verbatim and `SharpenSpec::mask` is a `SharpenMask`.**
Section 5 names the field and not its type. It is not a mask payload - a plan is a decision and
`aura-core` has held no pixels since phase 01 - but the record of *which regions the
deconvolution was withheld from*, its coverage of the frame, and whether phase 18 supplied the
regions at all. That last flag is the difference between "there was nothing to exclude" and
"AURA could not see where the sky was", which is phase 18's rule and phase 19's outline field.

**`DenoiseTier` gains `DenoiseSpec` beside it on the plan.** The tier is the decision and the
spec is the three numbers it became under this frame's noise model, plus the measured sigma and
the model that conditioned it. A tier alone is not reproducible: the same `Standard` on two
bodies at two ISOs is two different renders, and a plan that cannot say which one it was is a
plan phase 27 cannot audit. `denoise_spec` is `Some` exactly when the tier is not `Off`, and
`RestorePlan::broken_guarantee` refuses a plan where it is not.

**`ArtefactReport` is three independent measurements with three independent verdicts**, not one
score. Section 6.4 asks for "band-energy smearing, ringing near edges and identity drift", and
the three are fixed by three different parameters: smearing by the denoise tier, ringing by the
sharpen amount and drift by the face-recovery strength. Collapsing them into one number would
make a plan that over-sharpened and a plan that over-smoothed indistinguishable, and the
automatic reduction section 6.4 requires would not know which lever to pull. Phase 21 settled
this argument for its three families; this is the same conclusion reached from the same place.

## 3. Decision: the strength is chosen from measured evidence, and an unmeasured camera lowers the ceiling

Section 6.1's last bullet is the rule: "Strength selection is evidence-based: the tier is chosen
from the measured sigma relative to the scene tolerance, not from a global preference."

Phase 09 already produces exactly that number. `IntegrityResult::noise_sigma_rel` is the noise
sigma **relative to what this scene tolerates at this ISO on this body**, where `1.0` is exactly
the tolerance - so the same absolute noise on a dance floor and in a family formal is already
two different numbers, and invariant 7 is satisfied by the input rather than by a threshold
table bolted on top of it.

**Decision.** The tier comes from `noise_sigma_rel` against four bands, then moves by at most one
tier under two modifiers, then is clamped by the scene's own ceiling:

| `noise_sigma_rel` | Tier |
|---|---|
| below 1.00 | `Off` - the scene already tolerates what is there |
| 1.00 to 1.60 | `Light` |
| 1.60 to 2.60 | `Standard` |
| 2.60 and above | `Strong` |

The two modifiers are section 6's own: **subject prominence**, because a frame whose subject
fills it is a frame whose noise is on somebody's face rather than on a wall, and **output long
edge**, because noise that is invisible in a 1,000 px web gallery is visible in a 24-inch print.
Each may raise the tier by one step and neither may raise it past the scene ceiling in
`restore_profiles.toml`.

**An unmeasured camera body caps the tier at `Standard`.** Section 8 step 1 asks COL to measure
noise models for the top twenty bodies and there are no camera files here, so every model that
ships is synthetic and every one of them is marked `measured = false`. A synthetic read-noise
figure that is too low tells the denoiser there is less noise than there is, and the result is
under-denoising, which is recoverable. A synthetic figure that is too high tells it there is
more, and `Strong` on a body whose real noise is a third of the model's is the smeared lace this
phase exists to avoid. The cap is the asymmetry written down. `AURA-ML-5103` says so, and
`docs/restoration.md` says it to a photographer.

This is phase 14's rule for camera profiles, reached independently: every real body renders
through the neutral reference profile and says so. Here every real body denoises through an
unmeasured noise model, says so, and is not allowed the strongest tier while it does.

## 4. Decision: sharpening is refused more often than it is applied, and the mask is a precondition rather than an attenuation

Section 6.2 has four bullets and three of them are refusals. Read together they describe an
operation whose default answer is no:

> if blur is dominated by motion or gross defocus, do not sharpen ... mask out skin, sky and
> bokeh regions from Phase 18 ... Cap the amount by the noise level after denoising

**Decision.** Deconvolution sharpening requires all four of the following, and any one of them
missing is a refusal with a reason code rather than a reduced amount:

1. The estimated kernel sigma is inside `SHARPEN_KERNEL_LO ..= SHARPEN_KERNEL_HI`. Below it
   there is nothing to recover; above it the blur is gross and Richardson-Lucy on a gross kernel
   is where ringing comes from.
2. Phase 09's `MotionKind` is not `SubjectMotion` or `CameraShake`. Motion blur is directional
   and a symmetric kernel deconvolving it produces a doubled edge, which reads as a worse
   photograph rather than a softer one. Section 2.2 puts motion-blur removal out of scope and
   this is that exclusion at the operator.
3. Phase 09's `focus_offset` is inside a small band around zero. Front and back focus are gross
   defocus by another name.
4. **Phase 18 supplied regions.** This is the one that will be argued with, because the obvious
   alternative is to sharpen the whole frame at a lower amount when there is no mask. That is
   what every restoration tool that has ever produced a crunchy sky does. Skin, sky and bokeh are
   not regions where sharpening is less welcome; they are regions where it is *visible as damage*
   and nowhere else, so an unmasked global sharpen concentrates its entire artefact budget on the
   three places a photographer looks first. Phase 19 wrote the general rule - a phase that
   consumes another phase's output owns no fallback for it - and this is the sharpest case of it
   in the product.

Skin is the exception to the exception, and it is an attenuation rather than an exclusion.
`SharpenSpec::skin_attenuation` withholds most of the amount and not all of it, because a face
with literally zero sharpening inside a frame that was sharpened reads as soft rather than as
protected. Sky and bokeh are excluded outright: there is no edge in either that a photographer
wants recovered.

The amount is then capped by the residual noise after denoising, which is the noise model's
sigma scaled by what the tier removed. This is the only coupling between the two operations and
it runs in one direction: sharpening reads what denoising left, and denoising never reads what
sharpening wants.

## 5. Decision: the identity constraint measures through the renderer, and its only two outcomes are "reduce" and "skip"

Section 6.3 is unusually specific and it is worth quoting because the implementation is the
sentence:

> Hard identity constraint: compute the Phase 06 face embedding before and after; if cosine
> distance exceeds a small threshold, reduce strength and retry, and if it still fails, skip and
> record the reason. This is the guarantee that the product never changes what someone looks
> like.

**Decision.** `face_recovery::enforce` renders the plan through `aura_render::restore::apply` -
the same code the delivered JPEG goes through - crops the face through phase 10's shared 112 px
two-point warp, embeds both crops through phase 06's frozen `PeopleService`, and compares. Above
`MAX_IDENTITY_DRIFT` the strength drops by `RESOLVE_STEP` and it renders again, at most
`MAX_RESOLVES` times. Still above, and the face is **skipped** - removed from the plan
entirely - with `RestoreCode::IdentityDriftSkipped` and the measured distance stored on the row.

Three properties follow and all three are deliberate.

**A guarantee about a pixel is enforced on the pixel.** Phase 16 wrote this for skin colour,
phase 20 for skin texture, phase 21 for catchlights and hairlines, and this is the fourth time.
The alternative - bound the strength parameter and assert the identity property - is what every
product that has shipped a stranger's face did.

**The band is narrow at both ends, and the lower end is the one that matters.** Section 6.3:
"only when the face is slightly soft (a narrow band of measured sharpness), never on heavily
blurred faces where the model would hallucinate". A heavily blurred face contains too little
information to constrain a prior, so what comes back is the prior - a plausible face, which is
to say somebody else's. `SOFT_FACE_LO` is the floor below which this phase does nothing at all,
and it is checked before any model is consulted rather than after.

**The distance is stored whether it passed or not.** `restore_face.identity_drift` is on every
row, so section 10.1's "face embedding distance after face recovery below threshold on 100 % of
fixtures" is `SELECT MAX(identity_drift) FROM restore_face WHERE skipped = 0` over a wedding
rather than a sentence in a document. Phase 16 established that a guarantee you cannot query is
a guarantee you cannot find out you have lost.

## 6. Decision: neither model is consulted, and unlike phase 20 there is no measurement that replaces one of them

Phases 15, 16 and 18 refused to consult a placeholder head and fell back on a reference solver.
Phase 20 could not do that and shipped a *measurement* instead - a difference-of-Gaussians with a
colour test - whose failure mode is finding fewer marks rather than confidently wrong ones. This
phase has to give two different answers to the same question.

**Denoising ships as a measurement, and it is a good one.** What runs is a noise-model-conditioned
edge-preserving filter over separated luminance and chroma planes, with the chroma radius wider
than the luminance radius because chroma noise is spatially low-frequency and chroma *detail* is
what wedding fabric is made of. It is not a learned denoiser and it will not beat DeepPRIME. Its
failure mode is leaving noise behind, which a photographer can see and can correct, rather than
inventing texture, which they cannot. Section 10.1's PSNR/SSIM gate is measured against a
bilinear baseline exactly as written, and the reference path clears it.

**Face recovery ships as a refusal.** `FACE_RECOVERY_HEAD_TRAINED` is false and
`face_recovery::solve` returns `None` with `RestoreCode::RecoveryHeadUntrained` on every frame in
this build. There is no measured fallback and there is deliberately not one, because the
measurement that would stand in for a face prior is unsharp masking on a face, and that is not a
weaker version of face recovery - it is a different operation with a worse result and the same
name. A photographer told "AURA improved this soft face" who received a sharpened soft face has
been lied to about the one thing this phase promised not to lie about.

So the phase ships two of its three operations and refuses the third by name, in the panel, in
the plan and in the exit report. That is a smaller product than section 13 describes and it is
the honest one. Condition C2 of `docs/progress/PHASE-22-EXIT.md` is what closes it.

## 7. Decision: the cloud offload is not built, and section 7 is why

Section 2.1 lists "cloud offload option for heavy restoration when the local GPU is weak (via
Phase 04 governance) with local fallback and explicit user consent per project", and section 9
gives AGT two days for it. Section 7 of the same document says:

> No cloud AI call in this phase. The phase must work with the network cable unplugged; the
> Cloud AI Gateway from Phase 04 stays idle here.

**Decision.** `RunWhere::Cloud` exists in the contract, because section 5 freezes it and a
variant that is absent cannot be added later without a contract change. Nothing in this build can
return it: `schedule::where_to_run` has no arm that produces `Cloud`, `aura-restore` does not
depend on `aura-cloud`, and `crates/aura-restore/tests/no_network.rs` fails the build if the
dependency ever appears - the pattern phase 20 established.

The argument is phase 12's, reached the same way. An offload path needs a provider that can
accept a 45 MP linear buffer, a measured cost per call, a cassette to test against and a local
GPU figure to be faster than. This build has none of the four, and the local GPU figure is
missing because ADR-0029 section 4 links no `wgpu` backend. Plumbing that has never carried
traffic is plumbing that leaks the first time it does. Adding `RestoreOffload` later touches no
shape frozen here.

There is a second reason and it is the stronger one. The data a restoration offload would send
is not a thumbnail, a crop or a statistic - it is the photograph. Section 9 of
`docs/plan/CLAUDE.md` says to "send derivative data (thumbnails, crops, statistics), never
original RAW files", and a linear 45 MP buffer is a derivative of a RAW in the same sense that a
print is. The consent flow that would make it acceptable is a bigger design than the two days
section 9 allows, and it belongs in a phase that can give it that.

## 8. Decision: restoration never runs on the interactive path, and the type says so

Section 6.4: "Restoration never runs on the interactive path; it runs during export or as an
explicit background enhancement pass with progress and cancellation."

**Decision.** `RestoreWhen` has two variants, `Export` and `Background`, and there is no third.
`RestorePass` accepts one and the render graph already refuses the rest: `graph::plan` computes
`skip_heavy` from `RenderPurpose` and marks `Stage::Restoration` and `Stage::Retouch`
`InteractivePath` when it is set. `Stage::is_heavy` returns true for `Restoration`, `Retouch` and
`Dehaze` and was written by phase 14 for exactly this.

The pass is resumable in the shape every pass since phase 06 uses: the work remaining is a query
over version columns rather than a journal, so a kill at 10 %, 50 % or 90 % resumes without
recomputation and a `profile_ver` bump heals itself.

## 9. Consequences

- Phase 23 straightens and crops after this phase sharpens, which is `Stage::Geometry` at index
  21 and already correct. It must not re-sharpen after a resample; `docs/restoration.md` says so
  and `Stage::Sharpen`'s position is the enforcement.
- Phase 24 fills generatively at whatever stage it claims, and it inherits the identity
  constraint's shape rather than its code: a fill inside a face is a face-recovery decision by
  another name.
- Phase 25 normalises a gallery whose frames were denoised at four different tiers. A gallery
  where every frame got `Strong` and one got `Off` is a gallery with one noisy frame in it, and
  `v_restore_coverage` is the query that finds it.
- Phase 27's QC agent reads `restore_face.identity_drift` to answer "does this face look
  different", and reads `restore_plan.ringing` to answer "why does this edge look crunchy".
- Phase 28 may not run this phase unattended while `FACE_RECOVERY_HEAD_TRAINED` is false,
  because an operation that always refuses is not an operation that has been validated.
- The two `graph.rs` edits change which stages a plan reports for a recipe that sets
  `restoration.denoise`. No recipe in the repository sets it, so no golden image moves;
  `crates/aura-render/tests/restoration_order.rs` covers the new routing.

## 10. What was considered and rejected

**Denoise inside `Stage::Restoration`, and amend section 2.1.** Rejected. The section 10.1 test
is written the other way round, and the reason the order matters is physical rather than
stylistic: every stage between 6 and 19 treats noise as signal.

**Move `Stage::NoiseReduction` later in `ORDER` so the two are adjacent.** Rejected, and it was
the first idea. It would put denoising after white balance, after the camera matrix, after
exposure and after the tone curve - which is to say after four operations that scale noise
non-uniformly, so the sigma the noise model predicts would no longer be the sigma in the buffer.
The whole point of section 6.1 is that the denoiser knows how much noise to expect.

**One `ArtefactReport` score with a single threshold.** Rejected; see section 2.1.

**Sharpen the whole frame at a reduced amount when phase 18 supplies nothing.** Rejected; see
section 4, bullet 4.

**A learned denoiser trained on synthetic noise alone.** Rejected. Synthetic noise matched to a
*synthetic* noise model is a network trained to invert a function this repository wrote, and its
gates would measure the round trip rather than the photograph. The measurement in section 6 is
worth less and claims less, and the gap between what it claims and what it does is zero.

**A `strength` slider on the IPC surface.** Rejected, and the reason is phase 21's: a ceiling can
be lowered by a studio and raised by nobody. The panel offers the four tiers and an off switch,
and every ceiling that bounds them is in the contract.

## 11. Amendments made during implementation

Amendments are appended here with the defect that prompted them, as ADR-0043 section 11 does.

### 11.1 A threshold on a measurement must sit above the instrument's own floor

`SHARPEN_KERNEL_LO` shipped at 0.55 and is now 1.00.

Section 4 of this document bounds deconvolution to a band of estimated blur kernels, and the
lower end is meant to mean "this frame is already as sharp as the lens and the sensor made it".
The number was chosen from optics: a well-corrected lens on a current sensor produces an edge
whose Gaussian sigma is somewhere around half a pixel, so 0.55 looked like the point below which
there is nothing to recover.

It is the wrong kind of number. `aura_restore::kernel` does not measure the *lens*; it measures
the width of a **Sobel gradient ridge**, and a Sobel operator has a width of its own. A
mathematically perfect step edge - the sharpest thing that can exist in a sampled image - produces
a ridge whose full width at half maximum is exactly two samples, which is a sigma of
`2 / 2.35482 = 0.849`. **Nothing can measure below that**, so a floor of 0.55 is a floor no
photograph is ever under.

The consequence is not a subtle bias. Every frame in every wedding would have passed the kernel
precondition, and the only things standing between a gallery and a global deconvolution would
have been the motion test, the focus test and phase 18's regions - none of which is about
sharpness. The defect was found by the phase's own fixtures: a synthetic chequerboard with no
blur applied to it at all came back needing sharpening.

The floor is now one pixel of sigma - a full width at half maximum of 2.35 samples, which is a
frame that really is slightly soft - and `kernel::tests::the_contract_floor_sits_above_the_estimator_own_floor`
asserts the two numbers **against each other** rather than asserting either alone. A later change
to the estimator that moved its floor - a different operator, a different interpolation, a
different sub-sample fit - fails that test instead of quietly re-opening this.

The general rule is worth carrying: **a threshold on a measurement is a statement about the
instrument as well as about the world.** Phase 19 met the same shape from the other direction -
its edge-gradient halo test could not be met by a correct implementation - and phase 21 met it
again with a chance-corrected margin that a perfect panel failed. A threshold a correct
implementation cannot meet, and a threshold every input necessarily meets, are the same bug.
