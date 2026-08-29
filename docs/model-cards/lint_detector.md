# Model card - `lint_detector` `1.0.0`

| Field | Value |
|---|---|
| Name | `lint_detector` |
| Version | 1.0.0 |
| Task | Name a small mark on a garment: none, lint, thread or stain |
| Class | `retouch` |
| Owner | MLL (ML Lead - Vision), with SRML and DATA |
| Licence | proprietary |
| Opset | 13 |
| Input | `patch [N,3,64,64]`, NCHW, `0..1`, **linear sRGB** |
| Output | `kinds [N,4]`, unnormalised logits: none, lint, thread, stain |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 100 (`model_ver` on every `micro_plan` row) |
| File size | 31,704 bytes per variant |
| **Trained** | **No. See "Training data".** |

## Purpose

Say what the small thing on somebody's lapel is.

PHASE-21 section 6.3: "lint/thread/stain detection as small anomaly detection restricted to the
clothing mask, with inpainting reused from Phase 20; creases and wrinkles are opt-in only, since
removing them can look artificial."

**Four classes, and the two that are missing are the design.** `ClothingIssue` has five variants;
the other two - a visible strap and a crease - are the two section 2.1 marks opt-in only, and they
are absent from this head rather than present and suppressed. There is therefore no accuracy at
which the product starts finding creases in a studio that never asked for them. A studio switches
these operations on and off *by kind*, so a kind the head cannot name is a kind that cannot run.

Its boundaries:

- **It never decides to clean anything.** `aura_retouch::micro::clothing` applies
  `MAX_CLOTHING_AREA`, `MAX_CLOTHING_STRENGTH`, the fabric-texture veto and the studio's per-kind
  switch afterwards.
- **It never runs outside the clothing mask.** Phase 18's region is what supplies the patch, and a
  frame with no clothing region produces no candidates at all.
- **It never sees a whole photograph.** The input is a 64 px patch of fabric.

## Architecture

A two-stage convolutional trunk, global average pooling and a linear head:

```
patch [N, 3, 64, 64]
  -> Conv 24x3   3x3 s2 -> Relu        # 64 -> 32
  -> MaxPool 2                          # 32 -> 16
  -> Conv 32x24  3x3    -> Relu
  -> GlobalAveragePool -> Flatten
  -> Gemm 4x32                          # none, lint, thread, stain
kinds [N, 4]
```

The same shape as phase 20's permanent-feature classifier, one region up, and for the same reason:
the question is about one small thing rather than about where things are, so global pooling costs
nothing and the head is cheap enough to run on every candidate the measurement offers.

The training procedure augments scale by at most a factor of two, deliberately. The size of a mark
relative to the weave around it is most of the signal separating lint from a stain, and heavy
scale augmentation trains it away.

**No sigmoid or softmax in the graph.** Calibration belongs to phase 13.

## Training data

**None. This model is a signed placeholder with deterministic weights.**

PHASE-21 section 8 step 2 asks for labelled lint cases and section 9 gives DATA seven days for
them. There is no such corpus in this repository and no GPU backend to train on.

`aura_retouch::micro::ops::LINT_HEAD_TRAINED` is `false`, so **this head is never consulted**. What
runs instead is the measured detector in `aura_retouch::micro::clothing`: a small high-frequency
anomaly inside the clothing region whose colour departs from the fabric immediately around it,
which is phase 20's blemish shape one region up.
`docs/adr/ADR-0045-micro-retouch-and-cross-frame-borrowing.md` section 6 records why.

`ml/models/micro/train_lint.py --self-test` runs without PyTorch and proves, among other
properties, that the head cannot produce `strap` or `crease` and that a model firing on patterned
fabric fails the gate rather than passing on the mean.

## Latency

Not measured. The reference-machine rows are deliberately empty rather than filled with plausible
figures - phase 03's rule.

| Machine | Batch 1 | Batch 8 |
|---|---|---|
| RTX 4070 laptop | | |
| M3 Pro MacBook | | |
| Intel iGPU desktop | | |

## Quality gate

Section 10.1 asks for lint removal recall at or above 0.85 with no fabric-texture damage at 100 %
zoom. `tests/eval/micro_eval.rs` gate 6 measures the recall, and measures fabric nobody aimed at for
any change at all, against synthetic garments whose marks are painted into the pixels; the 100 % zoom artefact audit is a
human task and is **not run**, recorded as a condition in `docs/progress/PHASE-21-EXIT.md`.

## Ethical and fairness notes

- **The threshold is relative to the fabric the mark is on.** There is no absolute contrast
  constant in `aura_retouch::micro::clothing`: the departure is measured against the median and
  the deviation of that garment's own weave. Embroidery, brocade, zari and beadwork are dense
  patterned fabrics, and `MAX_FABRIC_TEXTURE` refuses to clean anything off them - which matters
  most for exactly the garments a wedding in this product's primary market is likely to contain.
- **A crease is not a defect.** Removing one is opt-in, per studio, per kind, and off by default,
  because a garment with no creases in it does not read as cloth.
- **The demographic performance of this head is unknown and no per-bucket number is published.**

## Known failure modes

- A deliberate design element on plain fabric - a single embroidered dot, a small motif, a pin -
  reads as a mark. The fabric-texture veto does not fire, because the fabric around it is plain.
- A button, a stud or a fastening reads as an anomaly on a plain shirt.
- Heavily patterned fabric produces no candidates at all, which is a miss rather than a harm and
  is the side this phase errs on.

## Fallback

The measured detector in `aura_retouch::micro::clothing`, which is what runs in this build. A
garment above `MAX_FABRIC_TEXTURE` produces `MicroCode::FabricTooTextured` and no operation.

## Rollback

`models.lock` pins this version by sha256 and the manifest is signed. Rolling back is a
`models.lock` edit plus `cargo xtask models`; the stored `model_ver` on every plan changes with it,
`AURA-ML-5102` is raised, and the affected frames are re-planned in the background.

## Related

- `docs/adr/ADR-0045-micro-retouch-and-cross-frame-borrowing.md`
- `docs/model-cards/flyaway_detector.md`
- `docs/model-cards/glare_detector.md`
- `docs/retouch-ethics.md`
