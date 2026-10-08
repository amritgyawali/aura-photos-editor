# Model card - `skyseg` `1.0.0 (JianyuanWang/skyseg)`

| Field | Value |
|---|---|
| Name | `skyseg` |
| Version | `1.0.0 (JianyuanWang/skyseg)` |
| Task | Sky selection for masking, including clouds and sunsets |
| Class | `segmentation` |
| Owner | MLL |
| Licence | MIT |
| Opset | 11 |
| Precision policy | fp32 only |

This model is optional. It is installed beside the application with ONNX Runtime (ADR-0103),
pinned by SHA-256 in `crates/aura-vision/src/ai.rs`, and fetched by
`scripts/fetch-ai-models.sh`. It is never committed.

## Purpose

It selects **the sky** when a photographer clicks Sky in the Masking panel. It is not used to
classify scenes or to decide anything automatically.

## Architecture

U²-Net (Qin et al., 2020), trained for sky segmentation by xiongzhu666
("Sky-Segmentation-and-Post-processing") and republished on Hugging Face by JianyuanWang. The
file is 176 MB.

- **Input:** `input.1`, [1, 3, 320, 320] float32, with ImageNet normalisation.
- **Output read:** the first of seven outputs, a probability [1, 1, 320, 320].

## Training data

AURA did not train this model. Its author trained it on public sky-segmentation data derived
from ADE20K. AURA has no breakdown of its training set.

## Latency

| Machine | Provider | Precision | Cold load | Per image | Batch throughput |
|---|---|---|---|---|---|
| Reference laptop (GTX 1650 Max-Q 4 GB, 8 GB RAM, Win 11) | DirectML | fp32 | about 8 s | 0.08 s | one at a time |
| RTX 4070 laptop (Win 11, 32 GB) | | | | | |
| M3 Pro MacBook (18 GB) | | | | | |
| Intel iGPU desktop (Win 11, 16 GB) | | | | | |

## Quality gate

No numeric gate exists. It was judged by eye (ADR-0103):

- on a cloudy sunset over the sea, it selected the whole sky and none of the sea;
- on a clear sky between trees, it selected the sky correctly.

## Known failure modes

- It answers "sky" for a plain studio backdrop and for the out-of-focus background of a
  close-up. `aura_vision::ai::sky` therefore accepts an answer only when the region reaches the
  top of the frame and does not run down to the bottom (`plausible_sky`). Both cases were
  refused on the test photographs.
- Night skies are not tested.

## Fallback

The measured horizon detector (`aura_vision::sky`, ADR-0102). It finds open sky above trees and
roofs, and reports "no sky found" for a cloudy sky it cannot follow.
