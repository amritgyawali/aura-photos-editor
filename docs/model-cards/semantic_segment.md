# Model card - `semantic_segment` `1.0.0`

| Field | Value |
|---|---|
| Name | `semantic_segment` |
| Version | 1.0.0 |
| Task | Assign one of twenty semantic classes to every cell of a 48x48 grid over a photograph |
| Class | `segmentation` |
| Owner | MLL (ML Lead - Vision), with SRML, SRG and DATA |
| Licence | proprietary |
| Opset | 13 |
| Input | `pixels [N,3,768,768]`, NCHW, `0..1`, **linear sRGB** |
| Output | `logits [N,20,48,48]`, unnormalised; the softmax is per pixel in Rust |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 1 (`model_ver` on every `masks` row) |
| Parameters | 77,132 |
| File size | 309,249 bytes per variant |
| **Trained** | **No. See "Training data".** |

## Purpose

Say what each part of a photograph *is*, so that phases 19 to 24 can edit a region rather than
a rectangle.

PHASE-18 section 6.1: "a single multi-class segmentation network at 768 px with a lightweight
decoder; classes chosen for editing utility, not academic completeness." The twenty classes are
`aura_vision::contract::mask::ALL_KINDS` and they were chosen against what phases 19 to 24
actually need - which is why `sclera` and `teeth` are in the list and `handbag` is not.

Its boundaries are as interesting as its job:

- **It does not produce a full-resolution mask.** The output is a 48x48 grid of class logits at
  stride 16, and the upsample to render resolution is a guided filter that can see the
  photograph's own edges. Section 6.1 asks for exactly this: "guided-filter upsampling to full
  resolution at render time, so masks are stored small but composited precisely." A decoder in
  the graph would also be impossible in this build - the interpreter implements no `Resize` and
  no `ConvTranspose` (ADR-0007).
- **It does not decide identity.** Which of the two people in frame a skin component belongs to
  is an overlap test against phase 06's boxes, in `mask::instance`, and this head has no way to
  express the answer.
- **It does not decide what may be done with a region.** That is `mask::quality::allowance`,
  from the confidence and the edge quality, and nothing this head emits can raise it.

## Architecture

A four-stage convolutional trunk with a 1x1 head:

```
pixels [N, 3, 768, 768]
  -> Conv 24x3   3x3 s4 -> Relu        # 768 -> 192
  -> MaxPool 2x2 s2                    # 192 -> 96
  -> Conv 48x24  3x3 s1 -> Relu
  -> MaxPool 2x2 s2                    # 96  -> 48
  -> Conv 64x48  3x3 s1 -> Relu
  -> Conv 64x64  3x3 s1 -> Relu        # the trunk
  -> Conv 20x64  1x1     -> logits [N, 20, 48, 48]
```

Three shapes here are decisions rather than conveniences.

**The stem strides by four before anything else runs.** A network that ran even one full-width
convolution at 768 px would spend most of its arithmetic before it had any features, and would
tell a false story about where the cost of a masking pass is.

**There is no softmax in the graph.** The interpreter's `Softmax` normalises the *last* axis and
the class axis here is the second, so a softmax in the graph would normalise across columns of
the logit grid - a plausible tensor that means nothing. The normalisation is per pixel in
`aura_vision::mask::segment`, beside the argmax that consumes it.

**The head is 1x1 over a shared trunk.** Adding a class is a channel count rather than a new
network, and `SEGMENT_CLASSES` disagreeing with `ALL_KINDS` is a shape error at load time rather
than a silent reinterpretation of one class as another.

## Training data

**None. This artifact is an architecture fixture with deterministic pseudo-random weights.**

Section 9 of the phase document gives DATA a twelve-day task: "segmentation labels on 12k
wedding frames incl. veils, ethnic attire, varied skin tones". There is no consented wedding
imagery in this repository and no GPU backend, so that task did not happen and cannot happen
here. The weights in the shipped file are a seeded pseudo-random fill; the class posterior over
a real photograph describes a random projection of it and nothing else.

**The head is therefore never consulted.** `aura_vision::mask::segment::SEG_HEAD_TRAINED` is
`false` and `segment::class_hint` returns `None` on every call, so no photograph in this build
is segmented by a random projection. What ships instead is deterministic geometry and colour
arithmetic seeded by phase 06's face boxes and landmarks - which is what section 6.1 describes
for skin, generalised to the classes it generalises to.
`docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` decision 2 has the argument.

This is condition C1 of `docs/progress/PHASE-18-EXIT.md` and it is a Sev 2 trigger.

## Latency

Not measured. There is no GPU backend in this build (ADR-0029 section 4) and the head is not
executed on any code path, so a figure here would be a measurement of a model nobody runs.

The processor reference path's masking pass is measured instead, in
`docs/progress/PHASE-18-EXIT.md`, and it is the number section 11's 120 ms budget is currently
compared against.

| Reference machine | fp16 | fp32 |
|---|---|---|
| RTX 4070 laptop | | |
| M3 Pro MacBook | | |
| Intel iGPU desktop | | |

## Quality gate

| Metric | Gate | Measured |
|---|---|---|
| mIoU, skin and face | >= 0.92 | see below |
| mIoU, hair | >= 0.88 | see below |
| mIoU, subject | >= 0.90 | see below |
| Per-class mIoU on a dark-skin subset | no worse than the mean by 0.03 | not measurable here |
| Per-class mIoU on an ethnic-attire subset | no worse than the mean by 0.03 | not measurable here |

The three mIoU gates **pass** in `tests/eval/mask_eval.rs`, and what they are measured on has to
be said plainly: synthetic frames whose regions were painted into the pixels and read back
through the real pipeline, across five skin *reflectances*. That proves the arithmetic recovers
something genuinely in the buffer. It says nothing about a wedding photograph, and it says
nothing about this model, which is not consulted.

The two subset rows are not measurable without the labelled data section 9 asks for. They are
left empty rather than filled with a plausible figure - the rule phase 03 established.

## Ethical and fairness notes

**There is no skin colour anywhere in the code path this head would join.** The skin class is
seeded from *this frame's own faces* - `mask::segment::SkinSeed` takes the median chroma and
luminance inside each detected face box - and grown by distance from that measured seed. There
is no ideal-skin constant in `aura-core`, in migration 18, in this crate or anywhere the
segmenter can reach, and the phase gate scans for one on every run. It is the third module in
the product to make that structural argument, after phase 15's `SkinLocus` and phase 17's
`SkinBias`, and `docs/skin-fairness.md` states it in the product's own words.

The five reflectances in `mask::fixtures::SKIN_REFLECTANCES` are **reflectances, not people**.
They prove the mechanism is measured rather than assumed. They prove nothing about a real
person, and the phase exit report says so.

**Nothing this head emits can identify anybody.** A class label on a grid cell is not a
biometric. Identity scoping happens elsewhere, against phase 06's boxes, and the templates that
carry identity never enter this crate - `crates/aura-vision/tests/no_template_writes.rs` is the
grep that keeps that true now that this crate has a catalog.

## Known failure modes

- **Clothing versus dress.** A dark green saree and a dark green jacket are the same pixels, and
  no colorimetric rule separates them. The deterministic path calls a garment a dress only when
  it is predominantly bright, low-chroma and reaches the bottom of the frame, at a lower
  confidence than any other class. A trained head is what would fix this.
- **Blonde hair against a dark background.** The measured hair class is "darker than this
  person's own skin", which misses it. The subject matte still covers it, so only operations
  that ask for *hair specifically* are affected.
- **A boundary the photograph does not contain.** A dark suit against a dark wall segments
  plausibly and mattes badly; `edge_quality` comes back low and
  `mask::quality::allowance` reduces what may be done through it. That is the designed response
  rather than a defect.

## Fallback

The deterministic path in `aura_vision::mask::segment`, which is what actually ships. An absent
or refused model changes nothing about this build's behaviour, because nothing calls it.

When the head is trained, `SEG_HEAD_TRAINED` becomes `true` and `class_hint` becomes the
argmax; the deterministic path becomes the prior it is blended against, and every consumer -
the store, the algebra, the gating, the panel - is unchanged.

## Rollback

`aura-models` pins by version and digest. Reverting to a previous version is a manifest edit and
a re-lock; because nothing consults this head, a rollback has no visible effect in this build.

A **model version bump does** invalidate every stored class assignment: `masks.model_ver` is on
every row, `AURA-ML-5083` is raised, and the background pass re-masks. Masks a photographer
edited by hand are kept - `user_edited = 0` is inside the `DELETE`'s own `WHERE`.

## Related

- `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md`
- `docs/adr/ADR-0007-inference-runtime.md` - why the opset subset has no `Resize`
- `docs/masks.md` - what the mask kinds mean, in the product's own words
- `docs/skin-fairness.md`
- `crates/aura-vision/src/mask/segment.rs`
- `ml/models/mask/train_seg.py`, `ml/models/mask/eval_mask.py`, `ml/models/mask/export.py`
