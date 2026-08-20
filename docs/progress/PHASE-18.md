# PHASE-18 progress - Local Mask AI: Automatic Semantic Masking

Branch `feat/phase-18-local-mask-ai`. One line per task, in the order of section 9.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| MLL - class taxonomy and matting strategy | `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md`, `docs/adr/ADR-0038-mask-ipc-surface.md` | - | Eleven decisions and five refusals. Decision 1 puts the contract in `aura-vision` rather than `aura-core`, because `RenderLevel` is in the frozen `upload_gpu` signature and `aura-core` depends on no workspace crate. |
| CTO - freeze the section 5 interfaces | `crates/aura-vision/src/contract/mask.rs`, `crates/aura-core/src/contract/ids.rs` | 0 | `Mask`, `MaskKind` (twenty), `MaskPayload`, `MaskOp`, `GpuMask`, `MaskOutline`, `MaskService`, plus `MaskId`. Two additions to section 5's printed shape - `reasons` for invariant 2 and `edge` for the panel - both recorded in the ADR. |
| SRC - the plane and the algebra | `crates/aura-vision/src/mask/algebra.rs` | 9 | Union is a maximum and intersection a minimum, not the probabilistic pair: `min`/`max` are idempotent, which is the property a photographer's mental model has. The clamped sampler was added after an upsample test found a one-pixel dark rim - a halo manufactured by the resampler. |
| SRML - the segmenter | `crates/aura-vision/src/mask/segment.rs` | 4 | Twenty classes, measured. `SEG_HEAD_TRAINED = false`. The skin seed is sampled from each face in *this* frame; the file contains no skin colour and a test greps for one. |
| SRML - the trimap and the matte | `crates/aura-vision/src/mask/trimap.rs`, `crates/aura-vision/src/mask/matting.rs` | 8 | Guided filter in closed form, band radius a fraction of the region's own size. `VARIANCE_FLOOR` was added after measuring what a wide band does on a low-contrast boundary: the closed form degenerates to a blur of the coarse mask, which reads as half-inside ten pixels out. |
| SRC - the subject | `crates/aura-vision/src/mask/subject.rs` | 2 | Composes the person classes rather than re-measuring them. The no-faces fallback carries a confidence chosen so it falls under `AGGRESSIVE_FLOOR` without a special case. |
| SRC - instance scoping | `crates/aura-vision/src/mask/instance.rs` | 3 | Containment, not IoU. A face is a small ellipse inside a large body box, so an IoU floor would leave every face in the wedding unassigned while looking like a careful threshold. |
| SRC - quality and the downstream gate | `crates/aura-vision/src/mask/quality.rs` | 4 | Geometric mean of the two numbers, five named operations, two of them refused below the floor. `AURA-ML-5081` is the first code in the product that constrains a later phase. |
| SRC - compression and the store | `crates/aura-vision/src/mask/store.rs`, `crates/aura-catalog/migrations/0018_masks.sql` | 8 | RLE for sixteen classes, quarter-resolution alpha for four. `put` reads the edited coordinates first: the `DELETE` alone was not enough, because `INSERT OR REPLACE` deletes the row it conflicts with and `masks` has a unique key on `(image_id, kind, identity_id)`. |
| SRC - the frozen service and the resumable pass | `crates/aura-vision/src/mask/api.rs` | 3 | `compose` is total, per section 5's signature; an invalid program produces the empty mask rather than a full-frame one. `Masks::read_only` exists so six of the eight commands do not open a preview cache to answer a query over one table. |
| SRG - the GPU path | `crates/aura-render/shaders/mask_upsample.wgsl`, `crates/aura-render/shaders/mask_composite.wgsl`, `crates/aura-render/src/shaders.rs` | 1 amended | Two files because they have two lifetimes - upsample once per session, composite on every parameter change. `every_shader_declares_the_frame_uniform` widened from `struct Frame` to "a block with a width and a height", because the mask shaders take two grids. |
| COL - linear-light compositing | `crates/aura-render/shaders/mask_composite.wgsl` | covered by `colour_discipline.rs` | `out = a*edited + (1-a)*base` on linear Rec.2020 and nothing else. A 50 % mask blended after the transfer function is a 73 % blend in light, largest exactly where the mask is soft. |
| DATA - labels | - | - | **Not done and not doable here.** No consented wedding imagery. `crates/aura-vision/src/mask/fixtures.rs` paints regions into synthetic pixels instead; C1. |
| SRML - models | `crates/aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`, `models/models.lock`, `docs/model-cards/semantic_segment.md`, `docs/model-cards/alpha_matting.md` | `cargo xtask models` | Two signed placeholders with cards, 19 models and 44 files verified. Neither is consulted. |
| MLOPS - training and evaluation | `ml/models/mask/{train_seg,train_matting,eval_mask,export}.py` | 4 self-tests | Every metric proves it can *fail*. `eval_mask` includes a halo measure precisely because mIoU averages a halo away - 0.899 on a frame the halo score catches at 0.125. |
| QAL - the gates | `tests/eval/mask_eval.rs` | 22 | mIoU across five reflectances, group-photo bleed, storage budget, codec round trip, determinism, and the phase 20 stub as `quality::Operation`. |
| SFE - the panel | `ui/src/components/develop/MaskPanel.tsx`, `ui/src/ipc/types.ts`, `ui/src/ipc/client.ts` | 8 | Two bars, never one. `allowance` arrives on the wire rather than being recomputed in TypeScript. |
| SRC - the IPC surface | `crates/aura-app/src/contract/ipc.rs`, `crates/aura-app/src/mask_commands.rs`, `crates/aura-app/src/state.rs`, `crates/aura-app/src/preview_commands.rs` | 4 | Eight commands, nine shapes. No `apply_mask`. `from_base64` added beside the encoder for the brush stroke. |
| SEC - the biometric boundary | `crates/aura-vision/tests/no_template_writes.rs`, `crates/aura-vision/Cargo.toml` | 2 | `aura-vision` gained a catalog, so phase 06's structural claim became a grep-as-a-test - the third in the repository. |
| QAL - the phase gate | `crates/aura-cli/src/phase18.rs`, `justfile` | gate | Thirteen sections, exits 0. |
| DOC - the product document | `docs/masks.md` | - | What the regions are, what the two numbers mean, and what is honestly measured rather than learned in this build. |

## Amendments to earlier phases

Two, both small and both recorded here because a later reader will meet them as surprises.

**`crates/aura-app/src/style_commands.rs`** - the phase 17 "this surface cannot return a pixel"
grep scanned from its own marker to the end of `ipc.rs`, so phase 18's mask overlay (a base64
alpha plane) failed it. The scan is now bounded at the phase 18 marker. The check is about *that*
block and had to say which block it is about.

**`crates/aura-render/src/shaders.rs`** - `every_shader_declares_the_frame_uniform` required a
literal `struct Frame`. The two mask shaders carry two grids, and a block called `Frame` would
have had to mean one of them. It now requires a uniform with a width and a height.
