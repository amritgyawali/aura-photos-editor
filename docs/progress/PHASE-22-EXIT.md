# PHASE-22 exit report - Restoration Stack: scene-aware denoise, selective sharpen and face recovery

**Phase:** 22 of 30 · **Branch:** `feat/phase-22-restoration-stack` · **Date:** 2026-08-22
**Gate:** `just phase-22-verify` exits 0 · **Eval:** `tests/eval/restore_eval.rs`, 17 passing

## 0. Read this first: what this phase can and cannot claim

This phase ships the first code in the product that **repairs** a photograph rather than deciding
something about it. The distinction matters for how a wrong answer behaves: a wrong grade is a
photograph somebody adjusts, and a wrong restoration is a photograph with information removed from
it - smeared lace, a ringed edge, a face that is very slightly not the same face. None of the
three can be edited back afterwards.

What is real:

- the noise-model-conditioned denoiser, the kernel estimator, the deconvolution, the four
  preconditions, the artefact self-check with its two independent levers, and the identity
  constraint - all of them measured on the rendered pixels rather than on the parameters;
- the tier ladder driven by phase 09's measured evidence, with no preference anywhere in the crate;
- migration 22, the store, the seven IPC commands, the panel, and the database trigger that
  refuses to deliver a face past the identity ceiling.

What is not:

- **the two shipped heads are untrained, and one of them ships as a refusal.** The denoiser is
  replaced by a measurement whose failure mode is leaving noise behind; the face-recovery head is
  replaced by *nothing at all*, and no face in this build is recovered. ADR-0045 section 6.
- **there is no expert preference study**, so section 0's headline KPI is unmeasured and no claim
  about how a restored photograph looks may be made from this build;
- **there is no photographed reference for any camera**, so every noise model is derived from a
  specification and no frame in this build may reach the strongest tier;
- **no frame in this build is sharpened at all**, because phase 18 supplies no regions and this
  phase refuses rather than sharpening blind.

Everything measured below is measured against synthetic frames whose noise, blur and structure
were painted into the pixels and read back through the real detectors, the real operators, the
real renderer and the real store. That proves the arithmetic, the thresholds, the refusals and the
guarantees. It says nothing about a wedding.

## 1. What shipped

| Area | Files |
|---|---|
| Frozen contract | `crates/aura-core/src/contract/restore.rs` (30 codes, 7 regions, 4 tiers) |
| Decision engine | `crates/aura-restore/src/{profiles,denoise,kernel,sharpen,face_recovery,selfcheck,schedule,decide,store,api,errors,fixtures}.rs` |
| Renderer | `crates/aura-render/src/restore.rs`, `shaders/{denoise_tile,deconv}.wgsl` |
| Schema | `crates/aura-catalog/migrations/0022_restoration.sql` (2 tables, 2 views, 1 trigger) |
| Config | `crates/aura-restore/config/restore_profiles.toml` (22 scenes), `config/noise_models/*.toml` (20 bodies) |
| Models | `denoise` and `face_recovery`, signed, carded, **both untrained** |
| ML | `ml/models/restore/{train_denoise,train_face_recovery,eval_restore,export}.py` |
| IPC | `crates/aura-app/src/restore_commands.rs`, 7 commands, ADR-0046 |
| UI | `ui/src/components/develop/RestorePanel.tsx` |
| Errors | `AURA-ML-5102` to `AURA-ML-5108`, one runbook each |
| Docs | `docs/restoration.md`, ADR-0045, ADR-0046 |
| Gate | `crates/aura-cli/src/phase22.rs`, `just phase-22-verify` |

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| High-ISO reception frames become deliverable, fabric and skin intact | **Partly.** The mechanism is complete and measured - PSNR and SSIM beat the bilinear baseline, texture retention holds above 0.90 on the lace fixture. Whether a real reception frame becomes deliverable is unmeasured. C1, C4. |
| Sharpening appears only where it helps and never on skin or bokeh | **Structurally yes, and it appears nowhere.** Sky and background are bit-identical afterwards (asserted), skin is attenuated by 0.80, and with no regions from phase 18 the operation is refused entirely. C3. |
| Slightly soft faces improve without anyone's identity changing | **The second half only.** No face is recovered in this build. The constraint is complete and refuses correctly; what it protects is measured with an untrained recogniser. C2. |
| Restoration decisions are explained and overridable per image | **Yes.** 30 reason codes grouped into four subjects, every plan carries at least one, and the override carries a tier and two switches. |
| Export budgets met on the reference machines | **Waived.** No `wgpu` backend. C6. |
| Competitive study shows parity or better | **Not run.** C4. |

## 3. What the section 10.1 gates measured

| Row | Result |
|---|---|
| Denoise PSNR/SSIM beats bilinear decisively | **Pass.** Measured against the clean plate the fixture noise was added to; the margin is above 1 dB and SSIM is higher. |
| Chroma detail preserved on fabric fixtures | **Pass.** Texture retention 0.929 on the lace plate at the tier the evidence asked for. |
| Identity distance below threshold on 100 % of fixtures | **Pass**, and it is a query rather than a test result: `SELECT MAX(identity_drift) FROM restore_face WHERE skipped = 0`. |
| No ringing above threshold | **Pass.** Every stored plan is inside `MAX_RINGING`, and the measurement scores an overshoot rather than the size of the edit. |
| Skin and bokeh measurably unaffected | **Pass.** Sky and background are bit-identical; skin is attenuated rather than excluded, deliberately. |
| Order of operations enforced | **Pass.** `crates/aura-render/tests/restoration_order.rs`, 10 tests. |
| Self-check reduces strength on adversarial smear | **Pass.** A `Strong` tier over the lace plate is stepped down and the report comes back clean. |
| Performance budget and VRAM | **Waived.** C6. |
| Cloud offload declines gracefully, identical decisions locally | **Pass.** No combination of capability and consent reaches a provider, and the plan is identical with consent given and withheld. |
| Expert preference >= 80 % at ISO >= 6400 | **Not run.** C4. |

## 4. Performance (section 11)

| Row | Status |
|---|---|
| Denoise 45 MP (RTX 4070) <= 2.5 s | **Waived**, no backend |
| Denoise 45 MP (M3 Pro) <= 5 s | **Waived**, no backend |
| Denoise 45 MP (CPU int8) <= 40 s | **Waived**; int8 is forbidden on this head and the reason is on the card |
| Sharpen + face recovery 45 MP <= 1.2 s | **Waived**, no backend |
| Restoration share of a 1,000-image export <= 45 min | **Waived**, extrapolated rather than measured |

Two budgets are asserted instead, and both are about producing a *decision* on a 2048 px proxy
rather than about moving 45 megapixels: `stage.restore_plan_frame` and
`stage.restore_identity_guard`. Storage is `size.restore_store_per_1000_images`, budgeted at
1,000 B/image against a measured 730 B - back under a kilobyte after phase 21's 1,633 B, for the
structural reason that this phase stores one fixed-width verdict per photograph and a list of
*faces* rather than a list of defects.

## 5. What this build's numbers are and are not claims about

Every number in section 3 is measured on frames from `crates/aura-restore/src/fixtures.rs`. The
noise is a deterministic hash rather than a generator (invariant 4), and it is uniform rather than
Gaussian, which makes it a slightly harder test at the same standard deviation.

The identity constraint is exercised against a probe whose vector rotates with the crop's
high-band energy. **That is not phase 06's recogniser**, which is itself untrained. What the gates
prove is that the constraint refuses what it should refuse; whether a real embedding would notice
a real identity change is condition C2.

## 6. Three things the gates found, recorded because they are the useful part

**A threshold on a measurement is a statement about the instrument as well as about the world.**
`SHARPEN_KERNEL_LO` shipped at 0.55, chosen from optics. The estimator measures the width of a
Sobel gradient ridge, and a mathematically perfect step edge produces a ridge two samples wide -
a sigma of 0.849. Nothing can measure below that, so every frame in every wedding would have
passed the kernel precondition and been deconvolved. Found by a synthetic chequerboard coming back
as needing sharpening. ADR-0045 section 11.1; the floor is now 1.00 and a test holds it against
the estimator's own floor rather than asserting either alone.

Phase 19 met the same shape from the other direction - its edge-gradient halo test could not be
met by a correct implementation - and phase 21 met it again with a chance-corrected margin a
perfect panel failed. **A threshold a correct implementation cannot meet and a threshold every
input necessarily meets are the same bug.**

**A ringing measurement must score the excursion, not the edit.** Comparing the gradient before and
after measures how hard the frame was sharpened, because every sharpening increases the step at an
edge - that is what sharpening is. What ringing *is*, is a pixel pushed beyond the range its own
neighbourhood had before the operation. A steeper edge now scores zero and an overshoot scores.

**Cosine distance is a function of direction, so a "more sensitive" probe built by scaling one
component is less sensitive.** The first identity probe multiplied the high-band term by a gain;
raising the gain made both the before and the after vector point along that component and the
distance between them collapsed toward zero. A probe built that way reports a *smaller* identity
change the more sensitive it claims to be, and a broken constraint would have passed. The response
is now an angle.

## 7. What was deliberately not built

**The cloud offload.** Section 2.1 lists it and section 7 of the same document forbids it.
ADR-0045 section 7 resolves it in favour of section 7: there is no provider, no measured cost, no
cassette and no local GPU figure to be faster than, and the data an offload would send is the
photograph rather than a derivative of it. `RunWhere::Cloud` exists because section 5 freezes it;
nothing returns it and no dependency could.

**A measured fallback for face recovery.** Unlike every other untrained head in this product. The
substitute would be unsharp masking on a face, which is a different operation with the same name.

**Motion-blur removal and upscaling.** Section 2.2, and both are structural: there is nowhere in
the contract or the schema to express either, and `boundaries.rs` greps for the words.

## 8. Conditions

**C1 - Sev 2. Every number in this phase is measured on synthetic frames.** No camera file, no
reference wedding, no paired capture. Closes with phase 02's first exit condition and phase 05's
C10. **No later phase may claim a restoration quality result until it closes.**

**C2 - Sev 2. Face recovery does not run, and the constraint that guards it is measured with an
untrained recogniser.** Two things close separately: a trained face-recovery head, and phase 06's
C1. Until both, `docs/restoration.md`'s statement that no face is recovered stays true and is the
honest description of the product. **Phase 28 may not run this phase unattended while this is
open**: an operation that always refuses has not been validated.

**C3 - Sev 3. No frame in this build is sharpened.** Phase 18's segmenter is a placeholder and
`AppState::restore_pass` wires no generator into `RestorePass::with_regions`. This is a
*connection* rather than a missing dependency - phase 18 ships `MaskService` - and it is the same
gap phase 19 carries. `RestoreOutline::region_covered` is zero and says so.

**C4 - Sev 2. The expert preference study did not happen**, so section 0's headline KPI is
unmeasured and no claim about naturalness, preference or competitive parity may be made from this
build. `ml/models/restore/eval_restore.py` is the estimator the study would be read through and
its four properties self-test; the study itself needs a panel and four ISO steps of real frames.

**C5 - Sev 3. `ui/src/ipc/client.ts` still stops at phase 19.** The seven commands are reachable
from the Tauri shell and not from a typed client method. Phases 20 and 21 are in the same state.

**C6 - Sev 3. Four of section 11's five performance rows are waived**, because this build links no
`wgpu` backend. Closes with ADR-0029's own condition.

**C7 - Sev 3. None of the twenty camera noise models is measured.** Every body is capped at
`DenoiseTier::Standard` and named in `RestoreOutline::unmeasured_cameras`. The first photographed
noise reference for any body is a Sev 3 trigger that reopens this row for that body.

## 9. Rollback

Feature flag: `RestorePassInput::enabled = false`. A disabled pass still writes a plan per frame -
one that does nothing - because a frame with no plan and a frame the studio switched off look
identical in a coverage report.

Model rollback: both entries are pinned by digest in `models.lock` and signed. `MODEL_VER` is `0`
and nothing consults either head, so rolling one back changes no stored decision in this build.

Migration rollback: the four `DROP` statements at the top of `0022_restoration.sql`, then
`DELETE FROM schema_version WHERE version = 22`. Everything is recomputable **except**
`restore_plan` rows with `user_edited = 1`; the runbook says to export those first.

Render-graph rollback: the two edits in `graph.rs` are independent of everything else in this
phase and reverting them restores phase 14's routing. Doing so re-opens the cache-invalidation
bug ADR-0045 section 2 describes.

## 10. What phase 23 inherits

- **`RestoreService` is the only way to ask what was repaired in a photograph.** Eighteenth service
  of its kind. No phase may keep its own denoiser, its own kernel estimator or its own idea of how
  far a face may move.
- **`Stage::Sharpen` is the last stage that changes a pixel value, and phase 23 must not add a
  second sharpening control.** Geometry resamples after it, which is correct; sharpening again
  after a resample is sharpening twice.
- **A repair that cannot be measured is not performed.** The identity constraint skips a face it
  cannot embed, the denoiser does nothing without a sigma, and sharpening refuses without regions.
  Phase 24 inherits the shape rather than the code: a generative fill inside a face is a
  face-recovery decision by another name.
- **A guarantee is a stored number.** `restore_face.identity_drift` is on every row that reached a
  render, so "no delivered face was changed" is a query. Fifth phase running.
