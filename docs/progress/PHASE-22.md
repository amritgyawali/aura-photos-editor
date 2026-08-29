# Phase 22 progress - Restoration Stack: scene-aware denoise, selective sharpen and face recovery

One line per task group, in the order section 8 asks for them. Files touched, tests added,
benchmark delta.

## T1 - COL: noise models for the top twenty camera bodies (section 8 step 1)

Files: `crates/aura-restore/config/noise_models/*.toml` (20), `crates/aura-restore/src/profiles.rs`.
Each file carries the two figures a photon transfer curve needs - read noise in electrons and
full-well capacity - and the loader normalises them into `variance = read^2 + shot * signal` in
the units the renderer works in. **Every one is `measured = false`**: there are no camera files in
this repository, so all twenty are derived from published specifications exactly as phase 09's
calibration table is. `NoiseModel::tier_ceiling` turns that into a cap at `DenoiseTier::Standard`,
and ADR-0047 section 3 records why the asymmetry runs that way rather than the other. Condition C7.
Tests: 8 unit tests in `profiles.rs`, plus `boundaries.rs` checking that a file on disk cannot be
missing from `EMBEDDED_NOISE`.

## T2 - DATA: paired noisy/clean captures across twenty bodies and six ISO steps (section 8 step 2)

**Not done, and it cannot be done here.** Section 9 budgets twelve days for "paired noisy/clean
captures across 20 bodies and 6 ISO steps; soft-face labelled set". There are no camera files in
this repository at all, and no consented face data. What shipped instead is
`crates/aura-restore/src/fixtures.rs` - frames whose noise, blur and structure are painted in at
known amplitudes, with the clean plate kept so a gate can measure PSNR against the frame the noise
was actually added to rather than against a blurred approximation. Conditions C1 and C4.

## T3 - SRML/MLL: train and export the denoiser (section 8 step 3)

Files: `ml/models/restore/{train_denoise,export}.py`, `crates/aura-infer/src/onnx/fixtures.rs`,
`xtask/src/models.rs`, `docs/model-cards/denoise.md`. One head registered, signed and carded;
untrained and **not consulted**. The training procedure carries three decisions that are about
safety rather than accuracy and self-tests that each can fail: a clean tile must be left alone, the
model must actually read the noise plane, and structure at three sigma or more must survive.
The fourth is architectural - the output is a **residual**, so an under-trained model leaves noise
behind rather than inventing texture. Tests: 5 Python properties plus a PSNR gate.

Two things this task got wrong first and both are recorded in the file: a fixed learning rate
diverged to `nan` because dividing the loss by sigma multiplies the curvature by `1/sigma^2`
across four ISO steps, and a property comparing a chroma weight against a luminance weight was
comparing two different models' units rather than their behaviour.

## T4 - SRC: kernel estimation and masked deconvolution (section 8 step 4)

Files: `crates/aura-restore/src/{kernel,sharpen}.rs`, `crates/aura-render/src/restore.rs`,
`crates/aura-render/shaders/deconv.wgsl`. The kernel is measured from the frame's own edges - the
full width at half maximum of a gradient ridge, taken at a low quantile so a soft shadow does not
drag the estimate. Richardson-Lucy at three iterations with edge-aware damping computed from the
*input*, through a weight plane that is zero over sky and background. Tests: 6 kernel unit tests,
11 sharpen unit tests, `restore_eval.rs` rows 4 and 5.

**The defect this task shipped first is worth reading**: `SHARPEN_KERNEL_LO` was 0.55, and a Sobel
gradient ridge across a mathematically perfect step edge is two samples wide, which is a sigma of
0.849. Nothing can measure below that, so every frame in every wedding would have passed the kernel
precondition. Found by the phase's own fixtures - a synthetic chequerboard came back needing
sharpening. ADR-0047 section 11.1, and
`kernel::tests::the_contract_floor_sits_above_the_estimator_own_floor` holds the two numbers
against each other so a change to the estimator fails rather than re-opening it.

## T5 - SRML/MLL: face recovery and the identity constraint (section 8 step 5)

Files: `crates/aura-restore/src/face_recovery.rs`, `ml/models/restore/train_face_recovery.py`,
`docs/model-cards/face_recovery.md`, `crates/aura-infer/src/onnx/fixtures.rs`. One head registered,
signed and carded; untrained, **not consulted, and with no measured fallback** - the only
placeholder in the product that ships as a refusal rather than as a measurement. ADR-0047 section 6
records why: the measurement that would stand in for a face prior is unsharp masking on a face,
which is a different operation with the same name.

The constraint itself is complete and exercised end to end. `enforce` renders through the real
renderer, embeds before and after through the caller's probe, and reduces then **skips**. Tests:
10 unit tests, `restore_eval.rs` row 3, and the phase gate's own end-to-end check.

The training loop's two ends of the band are handled oppositely and it is the task's main design
decision: a face below the floor is **removed from the set** so the head can never be asked about
one, and a face above the ceiling is **kept as a hard negative** so the head learns to do nothing
rather than extrapolating. The first version excluded both, and property 2 caught it - the fitted
model moved a sharp face by 0.015 because nothing had told it not to.

## T6 - SRC: the decision logic, tied to phase 09 evidence (section 8 step 6)

Files: `crates/aura-restore/src/{denoise,decide}.rs`, `crates/aura-restore/config/restore_profiles.toml`.
The tier comes from phase 09's `noise_sigma_rel` - the measured sigma relative to what *this scene*
tolerates at this ISO on this body - against four bands, then at most one step from the two
modifiers section 6.1 names, then the scene ceiling and the camera ceiling. 22 argued-over scene
rows with a written reason each. There is no preference anywhere in the crate to express. Tests:
9 denoise unit tests, 10 decide unit tests.

## T7 - SRC: the artefact self-check with automatic reduction (section 8 step 7)

Files: `crates/aura-restore/src/selfcheck.rs`, `crates/aura-render/src/restore.rs`. Two
measurements taken on the rendered result with two independent levers - texture retention steps
the *tier* down, ringing reduces the *amount* and then withdraws - plus the identity distance
carried from T5 rather than re-measured. Tests: 7 unit tests, 6 render-side unit tests.

The ringing measurement is the part worth reading. The naive version compares the gradient before
and after, which measures *the size of the sharpening* - phase 19 made exactly this mistake with
its halo test. What ringing is, is a pixel pushed beyond the range its own neighbourhood had
*before* the operation, and
`restore::tests::ringing_scores_zero_for_a_steeper_edge_and_more_for_an_overshoot` is that
distinction as a test.

## T8 - SRC/PERF: scheduling with cancellation (section 8 step 8)

Files: `crates/aura-restore/src/{schedule,api}.rs`, `perf/budgets.toml`. `RestoreWhen` has two
variants and no third, `graph::plan` refuses `Stage::Restoration` on the interactive path
independently, and `Stage::is_heavy` was written by phase 14 for this. The pass is resumable in the
shape every pass since phase 06 uses - the work remaining is a query over three version columns.
Tests: 5 schedule unit tests, plus `restoration_order.rs` (10).

## T9 - AGT: the cloud offload path (section 8 step 9)

**Deliberately not built.** Section 2.1 lists a cloud offload and section 7 of the same document
says "No cloud AI call in this phase. The phase must work with the network cable unplugged."
ADR-0047 section 7 resolves it in favour of section 7 and records the two arguments: there is no
provider, no measured cost, no cassette and no local GPU figure to be faster than, and the data an
offload would send is not a thumbnail or a crop but the photograph. `RunWhere::Cloud` exists in the
contract because section 5 freezes it; nothing returns it, `aura-restore` has no dependency that
could reach a provider, and `boundaries.rs` fails the build if one appears. Tests: an exhaustive
sweep over capability and consent in `schedule.rs` and again in `restore_eval.rs`.

## T10 - SFE: the Restore panel (section 8 step 10, first half)

Files: `ui/src/components/develop/RestorePanel.tsx`, `ui/src/ipc/types.ts`,
`crates/aura-app/src/restore_commands.rs`, `crates/aura-app/src/contract/ipc.rs`,
`docs/adr/ADR-0048-restore-ipc-surface.md`. Seven commands. Four tiers as buttons and **no slider
anywhere on the component** - a test asserts there is no range or number input. A face declined to
keep somebody looking like themselves gets its own block, its own wording and the measured
distance. Tests: 12 vitest cases.

## T10 - QAIQ: the expert preference study (section 8 step 10, second half)

**Not done, and it cannot be done here.** Section 9 budgets five days for a competitive study at
ISO 3200/6400/12800/25600 against DxO DeepPRIME, Topaz Photo AI and Lightroom AI Denoise. There is
no panel, no reference wedding and no competitor output in this repository. What shipped instead is
`ml/models/restore/eval_restore.py` - the estimators the study would be read through, with four
self-tested properties including phase 21's chance-corrected agreement correction. Condition C4.

## T11 - QAL: the CI gates

Files: `tests/eval/restore_eval.rs` (17), `crates/aura-cli/src/phase22.rs`, `justfile`. Five of
section 10.1's rows are measured, two are named as unmeasurable at the end of every gate run
rather than skipped quietly.

## T12 - DOC: the documentation

Files: `docs/restoration.md`, `docs/model-cards/{denoise,face_recovery}.md`,
`docs/runbooks/AURA-ML-510{2,3,4,5,6,7,8}.md`, `CHANGELOG.md`. The product-voice document leads
with the identity guarantee and is explicit that no face is recovered in this build.

## Benchmark delta

No previous-phase budget moved. Two new rows in `perf/budgets.toml`
(`stage.restore_plan_frame`, `stage.restore_identity_guard`) and one storage row
(`size.restore_store_per_1000_images`, budgeted at 1,000 B/image against a measured 730 B).

## Two changes outside this phase's file list

**`crates/aura-render/src/graph.rs`.** ADR-0047 section 2: `restoration.denoise` now invalidates
from `Stage::NoiseReduction` rather than from `Stage::Restoration`, and a denoise tier alone no
longer enables `Stage::Restoration`. The first of those is a latent cache-invalidation bug that
nothing had hit because nothing wrote the field.

**`crates/aura-render/src/shaders.rs`.** Two new library shaders and six new shared constants held
to the processor reference by `shader_parity.rs`.
