# Model card - `sam21_tiny` `2.1 Hiera-Tiny (vietanhdev export 2026-02-21)`

| Field | Value |
|---|---|
| Name | `sam21_tiny` |
| Version | `2.1 Hiera-Tiny (vietanhdev export 2026-02-21)` |
| Task | Object selection inside a box the photographer draws |
| Class | `segmentation` |
| Owner | MLL |
| Licence | Apache-2.0 |
| Opset | 17 |
| Precision policy | fp32 only |

These two files (encoder and decoder) are optional. They are installed beside the application
with ONNX Runtime (ADR-0103), pinned by SHA-256 in `crates/aura-vision/src/ai.rs`, and fetched
by `scripts/fetch-ai-models.sh`. They are never committed.

## Purpose

It powers the **Objects** tool in the Masking panel: the photographer drags a box around a thing
and gets a selection of it. It only ever runs on a box a person drew; nothing automatic calls it.

## Architecture

Segment Anything 2.1 (Meta), Hiera-Tiny, exported to two ONNX graphs by vietanhdev for
AnyLabeling. The files are 109 MB (encoder) and 16 MB (decoder).

**Encoder**

- Input: `image`, [1, 3, 1024, 1024], with ImageNet normalisation.
- Outputs: `image_embed`, `high_res_feats_0` and `high_res_feats_1`.

**Decoder**

- Inputs: the three encoder outputs, plus:
  - `point_coords`: the box's two corners, in the encoder's 1024-pixel space;
  - `point_labels`: 2 and 3, the convention for a box;
  - `mask_input`: zeros, [1, 1, 256, 256];
  - `has_mask_input`: [0].
- Outputs: `masks` (logits at 256 x 256) and `iou_predictions`.

AURA takes the mask with the best predicted IoU and clips it to the box.

## Training data

AURA did not train this model. It was trained by Meta on the SA-1B and SA-V datasets, as
published with SAM 2.

## Latency

| Machine | Provider | Precision | Cold load | Per image | Batch throughput |
|---|---|---|---|---|---|
| Reference laptop (GTX 1650 Max-Q 4 GB, 8 GB RAM, Win 11) | DirectML | fp32 | about 6 s (both graphs) | 0.43 s per box | one at a time |
| RTX 4070 laptop (Win 11, 32 GB) | | | | | |
| M3 Pro MacBook (18 GB) | | | | | |
| Intel iGPU desktop (Win 11, 16 GB) | | | | | |

## Quality gate

No numeric gate exists. It was judged by eye with a fixed box over six photographs. As the box
asked, it selected:

- the bouquet;
- the bride;
- the couple;
- a woman's face and arm.

## Known failure modes

- A loose box returns what is most object-like inside it. That can be the background behind a
  person, as seen with a box drawn over a road and trees.
- The encoder runs once per box. Boxes on the same photograph are not yet cached.

## Fallback

There is none. Without the model, the Objects tool says it is not available. Subject, Sky, the
person parts and the drawn masks still work.
