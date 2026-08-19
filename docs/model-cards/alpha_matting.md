# Model card - `alpha_matting` `1.0.0`

| Field | Value |
|---|---|
| Name | `alpha_matting` |
| Version | 1.0.0 |
| Task | Solve alpha inside the uncertain band of a trimap, for hair, veils and rim light |
| Class | `segmentation` |
| Owner | MLL (ML Lead - Vision), with SRML and QAIQ |
| Licence | proprietary |
| Opset | 13 |
| Input | `patch [N,4,128,128]`, NCHW, `0..1`, **linear sRGB plus a trimap channel** |
| Output | `alpha [N,1,32,32]`, sigmoid in the graph |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 1 (`model_ver` on every `masks` row) |
| Parameters | 8,001 |
| File size | 32,213 bytes per variant |
| **Trained** | **No. See "Training data".** |

## Purpose

Turn a hard boundary into a soft one where the photograph says it should be soft.

PHASE-18 section 6.1: "build a trimap by eroding/dilating the coarse mask, then run a matting
network only in the uncertain band - this is what makes veils and flyaway hair look correct."
Mask edges are where amateur software reveals itself, and this is the head whose job is that
edge and nothing else.

Its boundaries:

- **It only sees the band.** The input is a 128 px crop of the trimap band and its pixels, never
  a whole photograph. Everything the erosion kept is foreground and everything the dilation did
  not reach is background, and neither is ever passed to this head - which is what bounds both
  the cost and the damage. A matte that could change a pixel in the middle of somebody's face is
  a matte that can put a hole in a face mask.
- **It does not decide the class.** Which side of the band is her hair is the trimap's answer,
  and the trimap is an *input channel* rather than a mask applied to the output. That is the
  whole difference between a matting network and a segmentation one.
- **It does not decide how much it is trusted.** `matting::edge_quality` measures decisiveness,
  gradient agreement and how much of the band was solvable at all, and the last of those is what
  stops a boundary the photograph does not contain from scoring as the cleanest edge in the
  wedding.

## Architecture

A three-stage convolutional stack with a 1x1 head and a sigmoid:

```
patch [N, 4, 128, 128]                 # 3 colour + 1 trimap
  -> Conv 24x4  3x3 s2 -> Relu         # 128 -> 64
  -> MaxPool 2x2 s2                    # 64  -> 32
  -> Conv 32x24 3x3 s1 -> Relu
  -> Conv 1x32  1x1    -> Sigmoid -> alpha [N, 1, 32, 32]
```

**The sigmoid is in the graph**, unlike the segmentation head's softmax, and the reason is the
axis: a logistic is per element and needs no normalisation across anything, so it cannot be
applied to the wrong dimension. An alpha that arrived unbounded would also be an alpha every
caller had to clamp, and a clamp outside the model is a contract nothing checks.

The output is at a quarter of the patch, and the guided filter takes it the rest of the way with
the photograph as the guide. That is the same split the segmentation head makes and for the same
reason: the network decides *how much*, and the guide decides *exactly where*.

## Training data

**None. This artifact is an architecture fixture with deterministic pseudo-random weights.**

There is no consented wedding imagery in this repository, no alpha ground truth, and no GPU
backend. The alpha this network returns for a real band describes a random projection of it.

**The head is therefore never consulted.**
`aura_vision::mask::matting::MATTING_HEAD_TRAINED` is `false` and `matting::alpha_hint` returns
`None` on every call.

What ships in its place is **not** a placeholder. It is a guided filter solved in closed form -
a real matting algorithm, the one most matting networks are refined by, whose failure mode is a
slightly soft edge rather than a confidently wrong one. It could not be a network in this build
in any case: the interpreter implements a documented opset 13 subset with no `Resize` and no
`ConvTranspose` (ADR-0007), so a matting decoder cannot execute here at all.
`docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` decision 3 has the argument.

This is condition C1 of `docs/progress/PHASE-18-EXIT.md` and it is a Sev 2 trigger.

## Latency

Not measured; the head is not executed on any code path and there is no GPU backend. The
processor path's matting cost is inside the masking figure in the phase exit report.

| Reference machine | fp16 | fp32 |
|---|---|---|
| RTX 4070 laptop | | |
| M3 Pro MacBook | | |
| Intel iGPU desktop | | |

## Quality gate

| Metric | Gate | Measured |
|---|---|---|
| No visible halo at 100 % zoom on a veil fixture | human-verified | not measurable here |
| SSIM band metric against ground-truth alpha | >= 0.95 | not measurable here |
| The matte tracks a real boundary rather than sitting at a half | pass | **pass** |
| A boundary the photograph does not contain scores worse than one it does | pass | **pass** |
| The interior and the exterior are never touched | pass | **pass** |

The three that pass are in `crates/aura-vision/src/mask/matting.rs` and
`tests/eval/mask_eval.rs`, measured on authored frames with a known step. The two that do not
need veil fixtures with ground-truth alpha, which is the DATA task that did not happen. They are
left unmeasured rather than filled in.

## Ethical and fairness notes

A matte is a boundary, not a person. This head reads a 128 px crop of an edge and emits an
alpha; there is no channel, output or intermediate in it that could carry an identity, a
demographic or a skin tone.

The one fairness-adjacent property worth stating is that the guided filter that ships in its
place is driven by the *photograph's own* luminance rather than by any prior about what a
subject looks like. A dark-skinned subject against a dark background and a light-skinned subject
against a light background are the same problem to it, and both come back with a low
`edge_quality` rather than with a confident wrong edge.

## Known failure modes

- **A boundary with no contrast.** Where the guide carries no variance the closed form
  degenerates to a blur of the coarse mask, which reads as half-inside several pixels outside the
  true boundary. `matting::VARIANCE_FLOOR` detects it and keeps the coarse answer instead; the
  cost is recorded as low `trust`, which lowers `edge_quality`, which lowers the allowance.
- **A region too small to matte.** Below `trimap::BAND_MIN_PX` there is nothing to solve inside,
  and the mask keeps its coarse boundary with `MaskReason::TooSmallToMatte`.
- **Motion blur across the boundary.** The matte is decisive about the wrong place. This is the
  case the trained head would improve most.

## Fallback

The guided filter in `aura_vision::mask::matting::refine`, which is what ships. An absent or
refused model changes nothing, because nothing calls it.

## Rollback

`aura-models` pins by version and digest. A version bump invalidates every stored boundary
through `masks.analysis_ver` rather than `model_ver` when only the arithmetic moved, and through
both when the head changes; `AURA-ML-5083` is raised either way and hand-edited masks are kept.

## Related

- `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` decisions 3 and 4
- `docs/adr/ADR-0007-inference-runtime.md`
- `docs/masks.md`
- `crates/aura-vision/src/mask/matting.rs`, `crates/aura-vision/src/mask/trimap.rs`
- `crates/aura-render/shaders/mask_upsample.wgsl`
- `ml/models/mask/train_matting.py`, `ml/models/mask/eval_mask.py`
