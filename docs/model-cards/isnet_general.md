# Model card - `isnet_general` `1.0.0 (rembg release v0.0.0)`

| Field | Value |
|---|---|
| Name | `isnet_general` |
| Version | `1.0.0 (rembg release v0.0.0)` |
| Task | Subject selection for masking: the main subject of a photograph |
| Class | `segmentation` |
| Owner | MLL |
| Licence | Apache-2.0 |
| Opset | 11 |
| Precision policy | fp32 only. The processor fallback has no fp16 kernels for every operator. |

This model is optional. It is installed beside the application with ONNX Runtime (ADR-0103),
pinned by SHA-256 in `crates/aura-vision/src/ai.rs`, and fetched by
`scripts/fetch-ai-models.sh`. It is never committed.

## Purpose

It selects **the subject** when a photographer clicks Subject or Background in the Studio's
Masking panel (ADR-0102, ADR-0103). The subject is usually the people, but it is an animal, a
bouquet or a car when that is what the photograph is of. It also takes people out of a sky
selection.

It is **not** a person detector and not a skin model. It says nothing about who is in a frame or
which pixels are skin, and nothing downstream may read it that way. Face and body parts come from
the person segmenter (ADR-0082).

## Architecture

IS-Net (Qin et al., "Highly Accurate Dichotomous Image Segmentation", 2022), with the general-use
weights published by the rembg project. It is a U²-Net-style encoder-decoder with intermediate
supervision. It has 44 M parameters and the file is 178 MB.

- **Input:** `input_image`, [1, 3, 1024, 1024] float32. RGB is scaled to 0..1, then centred
  (mean 0.5, standard deviation 1.0).
- **Output read:** `output_image`, [1, 1, 1024, 1024], a probability (already passed through a
  sigmoid).
- **Outputs ignored:** five side outputs. `Model::run_for` copies out only the first output.

It runs on ONNX Runtime (ADR-0103), never on the bundled interpreter, which has no `Resize`.

## Training data

AURA did not train this model. Its authors trained it on DIS5K (5,470 images in 225 categories),
and it was released with the rembg project. It includes no wedding photographs, and no
skin-tone breakdown is published. No AURA data was used, and the model is never trained or
adapted on photographers' images.

## Latency

| Machine | Provider | Precision | Cold load | Per image | Batch throughput |
|---|---|---|---|---|---|
| Reference laptop (GTX 1650 Max-Q 4 GB, 8 GB RAM, Win 11) | DirectML | fp32 | about 10 s (SHA-256 check and graph compile) | 0.33 s at 1024 | one at a time |
| RTX 4070 laptop (Win 11, 32 GB) | | | | | |
| M3 Pro MacBook (18 GB) | | | | | |
| Intel iGPU desktop (Win 11, 16 GB) | | | | | |

## Quality gate

No numeric gate exists yet. It was judged by eye on public photographs (ADR-0103). It selected:

- the couple in a night wedding, including the train of the dress;
- the bride under a veil;
- the woman in the foreground of an office, not the people behind her;
- a woman seen from behind.

The per-skin-tone spread required by rule M4 is unmeasured. The model changes no skin colour;
it only makes a selection.

## Known failure modes

- It selects the most salient thing, which is not always the person. In a close-up of a kiss
  behind a bouquet, it selected the bouquet.
- Very small subjects, such as a couple far away on a beach, come back as a small, soft region.
- It has no abstention. When a selection covers almost nothing, the photographer is told "no
  subject found".

## Fallback

When the runtime or the file is missing, or the SHA-256 does not match, Subject and Background
come from the bundled person segmenter (ADR-0082). That selects everyone in the frame, and
nothing that is not a person.
