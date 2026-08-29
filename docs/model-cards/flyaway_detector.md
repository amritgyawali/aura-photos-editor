# Model card - `flyaway_detector` `1.0.0`

| Field | Value |
|---|---|
| Name | `flyaway_detector` |
| Version | 1.0.0 |
| Task | Say how strand-like each cell of a hairline tile is |
| Class | `retouch` |
| Owner | MLL (ML Lead - Vision), with SRML and DATA |
| Licence | proprietary |
| Opset | 13 |
| Input | `tile [N,3,128,128]`, NCHW, `0..1`, **linear sRGB** |
| Output | `strands [N,1,16,16]`, unnormalised logits |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 100 (`model_ver` on every `micro_plan` row) |
| File size | 156,423 bytes per variant |
| **Trained** | **No. See "Training data".** |

## Purpose

Find the few strands of hair that are lying across the background rather than on somebody's head.

PHASE-21 section 6.1: "detect flyaways as thin high-contrast structures outside the hair alpha but
connected to it; require a clean, low-detail background, otherwise skip". This head is the first
half of that sentence. The second half is a gate the runtime applies whether or not the head is
consulted.

Its boundaries matter more than its job:

- **It never decides to attenuate anything.** `aura_retouch::micro::hair` applies the area cap, the
  background gate and `MAX_FLYAWAY_STRENGTH`, and the naturalness guard measures the hair region's
  edge energy afterwards and can withdraw the whole family. Nothing this head emits can raise a
  strength.
- **It cannot reach inside the hair mass.** A candidate whose centre sits where phase 18's hair
  alpha is above `INSIDE_MIN` is dropped before anything else happens, in the runtime and in the
  training loop alike. That is not a threshold to be traded against recall: it is what "no bald
  patches" means.
- **It never sees a whole photograph.** The input is a tile straddling one stretch of hairline.

## Architecture

A three-stage convolutional trunk with a 1x1 head:

```
tile [N, 3, 128, 128]
  -> Conv 24x3   3x3 s2 -> Relu        # 128 -> 64
  -> MaxPool 2                          # 64 -> 32
  -> Conv 48x24  3x3    -> Relu
  -> MaxPool 2                          # 32 -> 16
  -> Conv 64x48  3x3    -> Relu
  -> Conv 1x64   1x1                    # strand-ness
strands [N, 1, 16, 16]
```

**One output channel, where phase 20's blemish detector has two**, and the difference is the
subject of this whole phase. A mark on skin can be temporary or permanent and the product treats
those two completely differently, so the blemish head has to say which. A strand of hair is never
temporary. There is no second question, and a second channel would be a number nothing could read.

The background is a **feature rather than a filter**. The same local contrast means something
different against a plastered wall and against foliage, so the trunk sees the background and can
learn a different threshold per background; a design that detected strands everywhere and
discarded the busy ones afterwards can only ever learn one threshold plus a veto. The runtime
still applies the veto as well - `MAX_FLYAWAY_BACKGROUND_DETAIL` - because the learned half is not
shipped.

**No sigmoid in the graph.** The output is a logit; calibration belongs to phase 13.

## Training data

**None. This model is a signed placeholder with deterministic weights.**

PHASE-21 section 8 step 2 asks for labelled flyaway cases with hair-type diversity coverage, and
section 9 gives DATA seven days for it. There is no such corpus in this repository, no consented
wedding photographs, and no GPU backend to train on.

`aura_retouch::micro::ops::FLYAWAY_HEAD_TRAINED` is `false`, so **this head is never consulted**.
What runs instead is the measured detector in `aura_retouch::micro::hair`: thin high-contrast
structures adjacent to the hair alpha, scored against the detail of the background immediately
behind them, capped by area and by strength.

`docs/adr/ADR-0045-micro-retouch-and-cross-frame-borrowing.md` section 6 records why this phase
ships measurements underneath its placeholders, and why this one is deliberately the most
conservative of the three: a measurement cannot tell a strand from a twig, so where the background
is busy the operation is skipped rather than guessed.

`ml/models/micro/train_flyaway.py` is the training procedure that would fit this head. Its
`--self-test` runs without PyTorch and proves four properties, two of which are about safety
rather than accuracy: that the background gate changes what is learned, and that a model firing
inside the hair mass cannot pass at any accuracy.

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

Section 10.1 asks for no bald patches or hairline damage on any fixture, and for the area cap never
to be exceeded. `tests/eval/micro_eval.rs` gates 1, 2 and 3 measure both - against synthetic frames
whose strands are painted into the pixels, which proves the detector's geometry and its background
gate and is not evidence about a wedding photograph.

The per-hair-type coverage row of section 10.1 is **not measured** and is recorded as a condition
in `docs/progress/PHASE-21-EXIT.md`. `ml/models/micro/eval_micro.py` holds the arithmetic that
would score it.

## Ethical and fairness notes

- **Hair type is a coverage report, never an input.** The head never sees a hair-type label, so
  there is no channel through which it could apply a different standard to different hair. What
  hair type is used for is checking the *result* per bucket, which is the only use of a
  demographic label this product permits.
- **Reduce, never remove.** `MAX_FLYAWAY_STRENGTH` is 0.60, so the strongest permitted edit
  attenuates a strand's contrast against its background and leaves the strand there. A hairline
  with no strays does not read as hair.
- **The demographic performance of this head is unknown and no per-bucket number is published.**

## Known failure modes

- A thin bright object that is not hair - a wire, a twig, a stem - reads as a strand. The
  background gate rejects most of these by rejecting the background they sit on; against a plain
  sky it is the failure mode that survives.
- A veil, a loose thread of a dupatta or a strand of beadwork reads as hair. Both are structures
  crossing a quiet background at the edge of a head.
- Wet or gelled hair with a hard edge produces no candidates at all, which is a miss rather than a
  harm.

## Fallback

The measured detector in `aura_retouch::micro::hair`, which is what runs in this build. A frame
whose background is too busy produces `MicroCode::BackgroundBusy` and no operation, rather than
a guess.

## Rollback

`models.lock` pins this version by sha256 and the manifest is signed. Rolling back is a
`models.lock` edit plus `cargo xtask models`; the stored `model_ver` on every plan changes with it,
`AURA-ML-5102` is raised, and the affected frames are re-planned in the background.

## Related

- `docs/adr/ADR-0045-micro-retouch-and-cross-frame-borrowing.md`
- `docs/model-cards/glare_detector.md`
- `docs/model-cards/lint_detector.md`
- `docs/retouch-ethics.md`
