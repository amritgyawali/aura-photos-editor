# Model card - `face_detect` `1.0.0`

| Field | Value |
|---|---|
| Name | `face_detect` |
| Version | 1.0.0 |
| Task | Single-stage face and person detection with five-point landmarks |
| Class | `segmentation` |
| Owner | MLL (ML Lead - Vision) |
| Licence | proprietary |
| Opset | 13 |
| Input | `pixels [N,3,640,640]`, NCHW, `0..1`, sRGB, letterboxed |
| Output | `head_8 [N,20,80,80]`, `head_16 [N,20,40,40]`, `head_32 [N,20,20,20]` |
| Precision policy | **int8 forbidden**; fp32 and fp16 permitted |
| Stored version integer | 100 (`model_ver` on every `faces` and `face_scan` row) |
| Preprocessing version | 1 (`preprocess_ver` on every `face_scan` row) |
| Parameters | 58,860 |
| File size | 237,006 bytes per variant |

## Purpose

Find every face in a wedding photograph, with five landmarks, plus a body box for
each face and for the people whose faces are not visible. Ten later phases depend on
it: eye and blink analysis (09), emotion (10), composition (11), culling and coverage
(12), the explainability ledger (13), masks and retouch (18, 20, 21, 22), gallery
intelligence (25), the QC agent (27) and curation (29).

**This 1.0.0 is a placeholder and it finds no faces in a photograph.** Section 6.1 of
the phase 06 document specifies an SCRFD-class detector; this build has no labelled
face data and no GPU backend, so the alternative to a placeholder is not a trained
detector but a real-looking one whose recall number describes nothing.

What is real, and is what everything downstream actually exercises: the letterbox
preprocessing, the three-stride anchor decode, the 20-channel head layout, the joint
face-and-person prediction, non-maximum suppression across passes, the conditional
2x2 tiled pass, the landmark-spread bokeh gate, and the batch memory ledger at 640 px.

**Recall is measured, but against synthetic ground truth**, using the deterministic
reference detector in `aura_vision::face::fixtures`. That harness exists so that when
a trained SCRFD arrives, the thing that measures it has already been run. This is
condition **C1** in `docs/progress/PHASE-06-EXIT.md`, it is a Sev 2 trigger, and no
later phase may claim a quality result that depends on face detection being accurate
until it closes.

## Architecture

A strided convolutional trunk with three detection heads, generated from a fixed
seed:

```text
pixels [N,3,640,640]
  Conv 3->16,  3x3, stride 4, pad 1 -> Relu -> MaxPool 2x2   (640 -> 160 -> 80)
  Conv 16->32, 3x3, stride 1, pad 1 -> Relu                  (80,  stride 8)
  Conv 32->32, 3x3, stride 1, pad 1 -> Relu   = P8
    Conv 32->20, 1x1                                         -> head_8  [N,20,80,80]
  Conv 32->48, 3x3, stride 2, pad 1 -> Relu   = P16           (40, stride 16)
    Conv 48->20, 1x1                                         -> head_16 [N,20,40,40]
  Conv 48->64, 3x3, stride 2, pad 1 -> Relu   = P32           (20, stride 32)
    Conv 64->20, 1x1                                         -> head_32 [N,20,20,20]
```

### The channel layout is the contract

| Channels | Meaning |
|---|---|
| 0 | face objectness logit |
| 1 | person objectness logit |
| 2..6 | face box: left/top/right/bottom distance from the anchor centre, in stride units |
| 6..10 | person box, same encoding |
| 10..20 | five landmarks: x/y offset from the anchor centre, in stride units |

`aura_vision::face::detect::decode_stride` is the only reader of it. Landmarks are
last so that a future model which drops them is a channel-count change the decoder
refuses, rather than a silent reinterpretation of a box as a landmark.

### Four decisions worth stating

* **Three strides, one forward pass.** A wedding is the pathological case for a
  single-scale detector: the bride at a first look fills a third of the frame and a
  guest four rows back is forty pixels tall, and the receptive field that sees the
  first swallows the second whole.
* **Faces and bodies from the same anchor.** This is why phase 06 ships three models
  and not four. Two boxes predicted by one anchor are one person by construction, so
  face-to-body association is free in the common case, and a person channel that
  fires where the face channel does not is exactly the back-of-head case section 6.1
  asks for.
* **No sigmoid in the graph.** Two channels want a logistic and fourteen regressions
  do not; separating them needs `Split`, which the interpreter does not implement
  (ADR-0007). The logistic is applied per channel in the decoder, where the anchor
  decode already lives.
* **The stem strides by four and pools once.** A detector that ran even one
  full-width convolution at 640 px would spend most of its arithmetic before it had
  any features and would tell a false story about where the cost of a face pass is.
  The cost here is concentrated at stride 8, which is where a real SCRFD's cost is
  too.

### Preprocessing is part of the model even though it is not in the file

Constants of `aura_vision::face::detect`: **letterbox** to 640x640 preserving aspect
ratio, black padding, area-average (box) resampling, planar NCHW, divide by 255, no
mean or standard deviation subtracted.

**Letterbox, never centre-crop** - and this is the one place the face path deliberately
differs from the embedding path in `aura_vision::embed::model`. An embedding is about
what a photograph is *of*, so cropping the long edges of a portrait frame is right. The
faces the tiled pass exists to recover are exactly the ones at the left and right edges
of a wide ceremony frame, so cropping them off before inference would make the whole
multi-scale design pointless.

A change to any preprocessing step bumps `PREPROCESS_VER`, which makes every
`face_scan` row stale and triggers a re-scan.

## Training data

None. The weights come from the seeded xorshift generator in
`crates/aura-infer/src/onnx/fixtures.rs`, so every machine writes byte-identical files
and `models.lock` verifies everywhere. No photograph was involved in producing this
file, so there is no consent scope, no licensing question and no wedding-level split
to enforce.

The dataset a trained version needs is specified rather than assumed, in
`ml/models/face/train_quality.py` and section 10.1 of the phase document:

* wedding-level train/validation/test splits, never frame-level, or the same face
  appears on both sides;
* a **small-face subset** with faces under 3 % of frame height, which is where section
  10.1's separate 0.90 recall gate is measured;
* a **dark-scene subset**, because an indoor night ceremony is where detectors fail
  and it is a third of this product's market;
* a **bokeh subset**, because a string of out-of-focus fairy lights is the canonical
  wedding false positive;
* **balanced skin tones with consent records**, which section 12 requires and which
  the synthetic fixtures explicitly cannot substitute for - see the fairness note
  below.

## Latency

Measured on 2026-08-13, release build, `1.97.1-x86_64-pc-windows-gnu`, via
`aura-cli infer --model models/face_detect_1.0.0.<precision>.onnx --input face-detect
--batch 2`.

| Machine | Provider | Precision | Per 640 px pass | Per frame, no tiling | Per frame, tiled |
|---|---|---|---|---|---|
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp32 | 148 ms | ~180 ms | ~772 ms |
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp16 | 160 ms | ~192 ms | ~800 ms |
| RTX 4070 laptop (Win 11, 32 GB) | | | not measured - no such machine | | |
| M3 Pro MacBook (18 GB) | | | not measured - no such machine | | |
| Intel iGPU desktop (Win 11, 16 GB) | | | not measured - no such machine | | |

The per-frame columns add the recogniser and quality head at the fixture set's measured
2.4 faces per frame; the tiled column is five detector passes rather than one.

Two notes:

* **fp16 is slower than fp32 on this interpreter.** Not a mistake and not a finding
  about fp16: the reference backend widens to `f32` to compute, so the half-precision
  variant pays a conversion the fp32 variant does not. On an accelerated runtime the
  ordering reverses. Recorded because a reader who finds fp16 slower and assumes a bug
  will waste a day.
* Section 11's budgets - 4,000 images in 240 s on an RTX 4070 and 480 s on an M3 Pro -
  are **waived** with an expiry condition per
  `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` section 6. What
  replaces them is `[stage.face_scan_image_cpu]` in `perf/budgets.toml`, asserted on the
  processor path.

## Quality gate

Section 10.1 sets three detection gates:

| Gate | Threshold | Status |
|---|---|---|
| Detection recall at IoU 0.5, overall | >= 0.97 | **met against synthetic ground truth** (`tests/eval/identity_eval.rs`); **deferred (C1)** against photographs |
| Detection recall on the small-face subset | >= 0.90 | **met against synthetic ground truth**; **deferred (C1)** against photographs |
| False positives on bokeh highlights | < 1 % | **met against synthetic ground truth**; **deferred (C1)** against photographs |

The distinction matters and is not a hedge. The three numbers *are* measured, on ground
truth that `aura_vision::face::fixtures` generates with known boxes, known identities
and deliberately face-coloured structureless highlights - so the anchor decode, the
suppression, the tiling and the bokeh gate are all genuinely exercised. What is not
measured is whether the *shipped weights* find a face, and they do not.

The gates enforced on this file today, in CI on every commit:

- `cargo xtask models` - signature, digest, card sections, opset, and that every variant
  compiles;
- `crates/aura-infer/tests/parity.rs` - fp16 within 1e-3 of fp32;
- `tests/eval/identity_eval.rs` - the recall, small-face and bokeh measurements above,
  plus bit-identical detections across two runs of the same pixels.

Writing a plausible number into a deferred row would be worse than leaving it deferred.

## Known failure modes

- **The weights carry no face semantics.** The dominant failure mode and the reason for
  C1. Structurally correct, semantically empty.
- **int8 is forbidden, and the reason is the box regression.** A detection head
  regresses distances in stride units; per-tensor int8 quantisation of a regression
  output moves a 40 px face's box by several pixels, which is the difference between
  recall and a missed guest. `PrecisionPolicy::no_int8` is set in `models.lock` and
  `aura-models` refuses the variant rather than trusting a comment.
- **Tiling costs five passes.** Section 12's fifth failure mode. Mitigated by the
  conditional trigger in `aura_vision::face::detect::should_tile`, which fires on
  wide-angle frames with several small detections and on frames with bodies but no
  faces; the ratio is recorded per frame in `face_scan.tiled` and reported by
  `ScanReport::tile_ratio`.
- **Letterbox padding is black**, so a face whose crop overlaps the padding is measured
  against black rather than against the frame's edge. Deliberate: clamping would smear a
  border colour across the crop and make it look like hair or a wall.
- **A landmark inside a profile is a guess.** Past a three-quarter turn one eye is
  occluded and its landmark is regressed rather than observed. The pose estimate in
  `aura_vision::face::align` saturates at 90 degrees rather than extrapolating, and the
  quality gate excludes the face from identity voting.
- Never seen a photograph, a non-letterboxed input, or a batch larger than 8.

## Fallback

**There is no smaller detector, and the fallback is the absence of the feature rather
than a worse version of it.** When the model is unavailable - not installed, rolled
back, or its version superseded - the face pass does not run, `face_scan` gains no rows,
`SubjectHierarchy::coverage` reports 0.0, and every later phase reads that coverage and
falls back to its own non-people path. The wedding still culls, edits and delivers, which
is invariant 6.

What is explicitly *not* a fallback: running the pass and treating an empty result as
"this wedding has no people". That is why coverage is on the hierarchy rather than
inferred from an identity count.

**Version rollback:** `models.lock` pins by digest, the registry keeps the previous
version until a new one has completed one real inference (`AURA-ML-5009`), and a
`model_ver` bump makes every `face_scan` row stale so the next pass re-scans. Faces from
two detector versions are never compared - that refusal is `AURA-ML-5018`.

## Fairness and demographic performance

Section 12's second failure mode is "recognition accuracy varies across skin tones", and
the phase document requires a demographic evaluation in this card.

**It is not here, and it is not approximated.** The synthetic fixtures use one skin tone
(`aura_vision::face::fixtures::SKIN`), so a per-group metric computed from them would be
a number about a renderer. Publishing it would be worse than publishing nothing, because
it would be quoted.

What is required before this row can be filled, and is recorded as condition **C5** in
the phase 06 exit report:

* a balanced evaluation set with per-group consent records;
* per-group recall at IoU 0.5, reported separately for the small-face and dark-scene
  subsets, because those are where a disparity appears first;
* the **stricter-margin** mitigation already implemented rather than promised: where
  evidence is weaker, `aura_vision::face::cluster` leaves a face unassigned instead of
  assigning it, which converts a fairness failure into a visible gap rather than a wrong
  name.

## Privacy and biometric note

A face box, five landmarks and a pose estimate are **not** a biometric template - they
cannot be matched against a person. They are stored in the clear in `faces`, and that is
deliberate: the People panel, the prominence scoring and every support investigation need
them, and encrypting them would put the panel behind the keychain for no gain.

The **template** produced from the crop this model locates is a biometric identifier, and
it is sealed. See `docs/model-cards/face_embed.md` and `aura_people::vault`.

Nothing this model produces leaves the machine. `aura-cloud` has no dependency on
`aura-people`, the payload builder cannot carry a template, and the optional couple hint
in section 7 sends counts and blurred thumbnails only.
