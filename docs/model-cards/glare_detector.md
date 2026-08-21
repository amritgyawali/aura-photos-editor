# Model card - `glare_detector` `1.0.0`

| Field | Value |
|---|---|
| Name | `glare_detector` |
| Version | 1.0.0 |
| Task | Say whether an eye region is a specular sheet, and what share of it has clipped |
| Class | `retouch` |
| Owner | MLL (ML Lead - Vision), with SRML and SRG |
| Licence | proprietary |
| Opset | 13 |
| Input | `region [N,3,64,64]`, NCHW, `0..1`, **linear sRGB** |
| Output | `glare [N,2]`, unnormalised: sheet logit, then clipped share |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 100 (`model_ver` on every `micro_plan` row) |
| File size | 31,443 bytes per variant |
| **Trained** | **No. See "Training data".** |

## Purpose

Find the reflection sitting on somebody's glasses, and say how much of what was behind it is gone.

PHASE-21 section 6.3: "detect specular sheets overlapping the eye region; if a sibling frame from
the same moment has the same face without glare and closely matching geometry, borrow that region
with alignment and frequency blending; otherwise reduce highlight intensity conservatively."

**The second output is the consequential one, and it is deliberately not a decision.** The head
reports what share of the region has clipped - a measurable property of the photograph - and
`aura_core::contract::micro::MIN_SPECULAR_FRACTION` turns that into permission to borrow pixels
from another frame. Training a single output that said "this may be rebuilt from another frame"
would put an ethical decision inside a learned weight, where nobody can read it.

Its boundaries:

- **It never decides to composite anything.** `aura_retouch::micro::borrow` requires the clipped
  share, an alignment score above `MIN_ALIGNMENT`, a region below `MAX_BORROW_AREA`, a sibling in
  the same moment, and the studio's borrowing switch. Any one of those refuses.
- **A catchlight is not a sheet.** The head is trained with catchlights as hard negatives rather
  than as ignored regions, and `CATCHLIGHT_FLOOR` measures afterwards what the plan did to the
  peak iris luminance. Both layers exist because a catchlight is the thing this phase most wants
  to keep.
- **It never sees a whole photograph.** The input is a 64 px region around one eye.

## Architecture

A two-stage convolutional trunk, global average pooling and a linear head:

```
region [N, 3, 64, 64]
  -> Conv 24x3   3x3 s2 -> Relu        # 64 -> 32
  -> MaxPool 2                          # 32 -> 16
  -> Conv 32x24  3x3    -> Relu
  -> GlobalAveragePool -> Flatten
  -> Gemm 2x32                          # sheet logit, clipped share
glare [N, 2]
```

Two outputs of **different kinds**, which is unusual and is the point: the first is a judgement
about the region and the second is an estimate of a quantity that exists in the photograph whether
or not anybody is judging it. A model that collapsed them would be reporting confidence as
evidence, and the third self-test property in `ml/models/micro/train_glare.py` is what catches it.

No exposure jitter in the training procedure, deliberately. The signal separating a blown sheet
from a bright sheen is where it sits relative to clipping, which is an absolute property of the
sensor's range; jittering exposure teaches the head to ignore exactly the thing it is for.

**No sigmoid in the graph.** Calibration belongs to phase 13.

## Training data

**None. This model is a signed placeholder with deterministic weights.**

PHASE-21 section 8 step 2 asks for labelled glare cases and section 9 gives DATA seven days for
them. There is no such corpus in this repository and no GPU backend to train on.

`aura_retouch::micro::ops::GLARE_HEAD_TRAINED` is `false`, so **this head is never consulted**.
What runs instead is the measurement in `aura_retouch::micro::glare`, and here that is not a
compromise: a specular sheet *is* a connected region of near-clipped, near-neutral pixels over an
eye. The measurement is the definition, and a placeholder head would add nothing to it.
`docs/adr/ADR-0043-micro-retouch-and-cross-frame-borrowing.md` section 6 records the argument.

`ml/models/micro/train_glare.py --self-test` runs without PyTorch and proves four properties,
including that a catchlight is never scored as a sheet at any accuracy, and that a model reporting
a soft sheen as fully clipped cannot pass - which is the failure that would turn a reduction into
a composite.

## Latency

Not measured. The reference-machine rows are deliberately empty rather than filled with plausible
figures - phase 03's rule.

| Machine | Batch 1 | Batch 8 |
|---|---|---|
| RTX 4070 laptop | | |
| M3 Pro MacBook | | |
| Intel iGPU desktop | | |

## Quality gate

Section 10.1 asks that borrowed regions align within tolerance and are always disclosed in the
recipe and the Explain panel. `tests/eval/micro_eval.rs` gates 7 and 8 measure the alignment floor,
the disclosure, and the information rule that separates a repairable sheet from a closed eye -
against synthetic frames whose sheets are painted into the pixels.

The naturalness audit of section 10.1 is **not run** and is recorded as a condition in
`docs/progress/PHASE-21-EXIT.md`.

## Ethical and fairness notes

- **A borrow may only replace pixels that carry no information.** That rule is what separates a
  glare repair from the eye swap section 2.2 forbids: a specular sheet has destroyed the record,
  and a closed eye *is* the record. `MIN_SPECULAR_FRACTION` is the rule as a number, and it reads
  this head's second output rather than its first.
- **Every borrow is disclosed in five places** - the recipe, the plan, the project header, the
  composites view and the delivery report - and the database refuses an undisclosed one.
- **The demographic performance of this head is unknown and no per-bucket number is published.**
  Spectacles, and the frequency with which a photograph contains them, are not evenly distributed
  across any population, and a detector that worked less well on one frame style would show up as
  uneven quality rather than as an error.

## Known failure modes

- A bright window reflected across an entire lens is detected but is too large to borrow, so it is
  reduced conservatively instead. That is the intended behaviour and it is visible as a partial
  fix.
- A photographed light source directly behind a head can produce a near-clipped neutral region
  that overlaps the eye box without being on a lens.
- Sunglasses are a sheet by every measurement this head makes. `aura_retouch::micro::glare`
  requires the region to overlap an iris landmark, which is what refuses them.

## Fallback

The measurement in `aura_retouch::micro::glare`, which is what runs in this build, and a
conservative highlight reduction bounded by `MAX_GLARE_REDUCE` when no sibling frame qualifies.

## Rollback

`models.lock` pins this version by sha256 and the manifest is signed. Rolling back is a
`models.lock` edit plus `cargo xtask models`; the stored `model_ver` on every plan changes with it,
`AURA-ML-5096` is raised, and the affected frames are re-planned in the background.

## Related

- `docs/adr/ADR-0043-micro-retouch-and-cross-frame-borrowing.md`
- `docs/adr/ADR-0044-micro-ipc-surface.md`
- `docs/model-cards/flyaway_detector.md`
- `docs/retouch-ethics.md`
