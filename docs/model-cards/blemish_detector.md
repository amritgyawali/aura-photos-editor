# Model card - `blemish_detector` `1.0.0`

| Field | Value |
|---|---|
| Name | `blemish_detector` |
| Version | 1.0.0 |
| Task | Locate anomalies on a face crop and say how temporary each one looks |
| Class | `retouch` |
| Owner | MLL (ML Lead - Vision), with SRML, COL and DATA |
| Licence | proprietary |
| Opset | 13 |
| Input | `crop [N,3,256,256]`, NCHW, `0..1`, **linear sRGB** |
| Output | `anomalies [N,2,32,32]`, unnormalised logits: objectness, then temporary |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 100 (`model_ver` on every `retouch_plan` row) |
| File size | 156,687 bytes per variant |
| **Trained** | **No. See "Training data".** |

## Purpose

Find the marks on somebody's face, and say which of them are passing through.

PHASE-20 section 6.1: "two-stage detection: find skin anomalies, then classify each as temporary
(pimple, redness, scratch, makeup smudge) or permanent (mole, freckle, scar, birthmark, tattoo,
beauty mark)". This head is the first stage and half of the second: the objectness channel says
*there is something here* and the temporary channel says *it is probably passing*.

Its boundaries matter more than its job:

- **It never decides to remove anything.** `aura_retouch::ops` compares the temporary channel
  against `TEMPORARY_FLOOR`, applies the protect-set veto and the preset, and the texture guard
  can withdraw the result afterwards. Nothing this head emits can raise a strength.
- **It cannot express permanence on its own.** A low temporary score offers a mark to the protect
  set; what actually protects it is cross-frame evidence - the same mark in the same place on the
  same face across hours - and that is arithmetic over a gallery rather than a network.
- **It never sees a whole photograph.** The input is a face crop, so there is no path by which a
  retouch detector reads a room.

## Architecture

A three-stage convolutional trunk with a 1x1 head:

```
crop [N, 3, 256, 256]
  -> Conv 24x3   3x3 s2 -> Relu        # 256 -> 128
  -> MaxPool 2                          # 128 -> 64
  -> Conv 48x24  3x3    -> Relu
  -> MaxPool 2                          # 64 -> 32
  -> Conv 64x48  3x3    -> Relu
  -> Conv 2x64   1x1                    # objectness, temporary
anomalies [N, 2, 32, 32]
```

Stride eight rather than sixteen, which is finer than any other detector in the product. A
blemish is a handful of pixels on a 256 px face crop, and at stride sixteen two marks on one
cheek land in the same cell - which loses exactly the thing the head exists to report.

**No sigmoid in the graph.** Both channels are logits. The calibration that turns a logit into
the probability `TEMPORARY_FLOOR` is compared against belongs to phase 13, which owns
calibration for the whole product; a squash baked into the graph would make an uncalibrated head
look calibrated, and this is the one head in the product where that difference decides whether
somebody's beauty mark is removed.

## Training data

**None. This model is a signed placeholder with deterministic weights.**

PHASE-20 section 8 steps 2 and 3 ask for labelled blemish and permanent-feature data across skin
tones on fifteen thousand faces, with consent. There is no such corpus in this repository, no
consented face data of any kind, and no GPU backend to train on.

`aura_retouch::ops::BLEMISH_HEAD_TRAINED` is `false`, so **this head is never consulted**. What
runs instead is the measured detector in `aura_retouch::blemish`: a difference-of-Gaussians over
the mid band of the face, split by sign, with a colour test against the median chromaticity of
that face's own skin.

`docs/adr/ADR-0041-portrait-retouch-and-texture-protection.md` section 7 records why this phase
ships a measurement underneath its placeholder rather than doing what phases 15, 16 and 18 do and
simply refusing to detect. In those phases a reference model existed below the head; here, a
phase that consulted nothing and had nothing underneath would ship a retoucher that finds no
marks at all.

## Latency

Not measured. The reference-machine rows are deliberately empty rather than filled with plausible
figures - phase 03's rule, and it applies here for a second reason: a latency measured on an
untrained network of this shape says nothing about a trained one that may not have this shape.

| Machine | Batch 1 | Batch 8 |
|---|---|---|
| RTX 4070 laptop | | |
| M3 Pro MacBook | | |
| Intel iGPU desktop | | |

## Quality gate

Section 10.1 asks for blemish recall at or above 0.90 with false removal of permanent features at
or below two per cent, and **zero** for tattoos. `tests/eval/retouch_eval.rs` gate 2 measures all
three - against synthetic faces whose marks are painted into the pixels, which proves the
detector's geometry and its colour test and is not evidence about a wedding photograph.

The per-skin-tone parity row of section 10.1 is **not measured** and is recorded as condition C2
in `docs/progress/PHASE-20-EXIT.md`.

## Ethical and fairness notes

This is the most ethically loaded head in the product, and three things are structural rather
than promised:

- **The threshold is relative to the skin the mark is on.** There is no absolute redness constant
  and no absolute contrast constant in `aura_retouch::blemish`: both are measured against the
  median and the deviation of that face's own skin. A detector calibrated on one skin tone does
  not quietly stop working on another. `docs/skin-fairness.md` says so in the product's voice.
- **Uncertainty leaves the mark alone.** `TEMPORARY_FLOOR` is 0.75 rather than 0.5, and an
  anomaly between the two floors is reported as uncertain and left in place.
- **A protected feature is a veto, not a discount.** No output of this head can reduce a protect
  row to a smaller strength; the candidate is dropped.

**The demographic performance of this head is unknown and no per-bucket number is published.**
Publishing one would need the corpus section 9 asks for.

## Known failure modes

- A shadow edge - the side of a nostril, the crease under a lip - reads as an anomaly. The colour
  test is what usually rejects it, and in low light it is what fails first.
- A mark on the boundary of the skin mask is found and then has no donor patch, so it is reported
  rather than removed.
- Makeup applied deliberately - a beauty spot drawn on, contouring - reads as a temporary mark.
  Cross-frame evidence protects the drawn beauty spot after four frames across an hour.

## Fallback

The measured detector in `aura_retouch::blemish`, which is what runs in this build. A frame with
no detections at all produces a plan that says `no_blemish_found` rather than no plan.

## Rollback

`models.lock` pins this version by sha256 and the manifest is signed. Rolling back is a
`models.lock` edit plus `cargo xtask models`; the stored `model_ver` on every plan changes with
it, `AURA-ML-5090` is raised, and the affected frames are re-planned in the background.

## Related

- `docs/adr/ADR-0041-portrait-retouch-and-texture-protection.md`
- `docs/model-cards/permanent_features.md`
- `docs/retouch.md`
- `docs/skin-fairness.md`
