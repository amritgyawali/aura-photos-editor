# Model card - `face_quality` `1.0.0`

| Field | Value |
|---|---|
| Name | `face_quality` |
| Version | 1.0.0 |
| Task | Four independent usability probabilities for an aligned face crop |
| Class | `embedding` |
| Owner | MLL (ML Lead - Vision) |
| Licence | proprietary |
| Opset | 13 |
| Input | `pixels [N,3,112,112]`, NCHW, `0..1`, sRGB, ArcFace-aligned |
| Output | `quality [N,4]` - usability, blur, occlusion, pose - sigmoid in the graph |
| Precision policy | **int8 forbidden**; fp32 and fp16 permitted |
| Stored version integer | 100 (`quality_ver` on every `faces` row) |
| Parameters | 6,276 |
| File size | 26,043 bytes per variant |
| **Trust weight in the shipped build** | **0.0** - see below |

## Purpose

Decide which faces are allowed to vote on identity.

This is the least glamorous model in phase 06 and the one that makes the phase work. A
twelve-hour wedding produces thousands of detections that are technically faces and
practically useless: a cheek at the edge of a dance-floor frame, a profile behind a
champagne flute, a thirty-pixel guest in the back row. Feeding those into clustering does
not produce a slightly worse answer - it produces a different *kind* of wrong answer, the
chain merge, where two guests who both photographed badly join to each other and then to a
third person, and one identity swallows the room.

So section 6.1 asks for a gate with two numbers - 48 source pixels and 0.4 usability - and
a face below either one is **detected, stored and displayed** while not voting. Excluding
is not deleting; the distinction lives in `faces.votes` rather than in whether the row
exists.

## The trust weight is zero, and that is not a bug

`aura_vision::face::quality::QUALITY_MODEL_WEIGHT` is **0.0** in this build.

The shipped weights are a placeholder with no training behind them, so the four outputs are
numbers with no meaning. Folding a meaningless number into the gate that decides which
faces vote would be the worst class of silent error - one that looks like it is working.

The head is still loaded, still run, still batched and still reported in
`FaceQuality::model_usable`, because the cost, the memory and the wiring are all real and
have to be measured. What decides the gate today is four measurements from the pixels, none
of which has a learned weight in it:

| Factor | How it is measured | Weight |
|---|---|---|
| Sharpness | variance of a four-neighbour Laplacian over the crop's luminance, saturating at `BLUR_HALF_VARIANCE = 0.003` | 0.35 |
| Occlusion | fraction of 14 px blocks whose local standard deviation is below 0.02 - a hand, a microphone, a veil or a blown highlight all read the same way | 0.25 |
| Pose | `Pose::frontality_penalty` from the five landmarks: yaw 0.6, pitch 0.3, roll 0.1 | 0.25 |
| Exposure | fraction of crop pixels crushed below 0.02 or blown above 0.98, saturating at 0.25 | 0.15 |

Combined as a **weighted geometric mean**, and that choice is the whole design. An
arithmetic mean lets three good factors outvote one catastrophic factor, so a perfectly
exposed, perfectly frontal, perfectly unoccluded, completely out-of-focus face scores 0.75
and votes on an identity. A geometric mean cannot do that: any factor near zero drags the
result to zero, which is the behaviour a gate is supposed to have.

When a trained head lands, `QUALITY_MODEL_WEIGHT` becomes a number and this section becomes
history. Condition **C2** in `docs/progress/PHASE-06-EXIT.md`. A test asserts that raising
the weight changes the gate, so the wiring is proven rather than assumed.

## Architecture

```text
pixels [N,3,112,112]
  Conv 3->16,  3x3, stride 2, pad 1 -> Relu -> MaxPool 2x2   (112 -> 56 -> 28)
  Conv 16->32, 3x3, stride 2, pad 1 -> Relu                  (28 -> 14)
  GlobalAveragePool -> Flatten                               (-> 32)
  Gemm 32->32 -> Relu
  Gemm 32->4
  Sigmoid
quality [N,4]
```

**The sigmoid is in this graph**, unlike the detector's, and the difference is the rule: an
activation belongs in a graph when it applies to every channel of a tensor, and in the
decoder when it does not. All four outputs here are independent probabilities and none is a
regression.

Deliberately tiny - 6,276 parameters. It runs once per face on a wedding's worth of faces,
it consumes a crop the recogniser has already produced, and a quality head that cost as
much as the recogniser would be a design mistake rather than a thorough one.

Preprocessing is the recogniser's: the same ArcFace-aligned 112 px crop, computed once and
handed to both models. The warp happens exactly once per face.

## Training data

None. Seeded xorshift weights from `crates/aura-infer/src/onnx/fixtures.rs`.

The dataset a trained version needs is specified in `ml/models/face/train_quality.py`.
Section 6.1 asks the head to predict "usability (pose, blur, occlusion, pixel height)",
which is a **regression against a recogniser**, not a human label - and getting that right
is the whole difficulty:

* labels are generated, not annotated. For each face with a known identity, the target
  usability is how well its template matches that identity's centroid computed from its
  *best* frames. A face that verifies is usable; one that does not is not. That makes the
  head learn the recogniser's actual weakness rather than a human's idea of a nice
  photograph;
* blur, occlusion and pose targets come from the measured factors above, so the head learns
  to *predict* them from a crop rather than to replace them - which is what makes it useful
  on the cases the measurements are wrong about, and why the two are blended rather than one
  replacing the other;
* the training set must include the wedding-specific occluders: veils, microphones, glasses
  with flash reflection, hands, hair across a face, and champagne flutes;
* balanced skin tones with consent records, because a quality head that systematically
  scores one group lower is a fairness failure that presents as a coverage failure.

## Latency

Measured on 2026-08-13, release build, `1.97.1-x86_64-pc-windows-gnu`, batch 8, via
`aura-cli infer --model models/face_quality_1.0.0.fp32.onnx --input face-crop --batch 8`.

| Machine | Provider | Precision | Per face | 10,000 faces |
|---|---|---|---|---|
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp32 | 3.8 ms | ~38 s |
| RTX 4070 laptop (Win 11, 32 GB) | | | not measured - no such machine | |
| M3 Pro MacBook (18 GB) | | | not measured - no such machine | |
| Intel iGPU desktop (Win 11, 16 GB) | | | not measured - no such machine | |

The measured factors cost about 0.6 ms per face on top, dominated by the Laplacian pass over
12,544 pixels.

## Quality gate

Section 10.1 does not state a separate gate for this head, because its effect is measured
through the two that matter:

| Gate | Threshold | Status |
|---|---|---|
| Identity clustering F1 | >= 0.93 | **met against synthetic templates**; the gate is what keeps unusable faces out of the skeleton |
| No cluster contains two labelled people | 0 | **met against synthetic templates**, including the lookalike-siblings case |
| Gate monotonicity | a blurrier, more occluded or more turned face never scores higher | **met** - asserted directly in `crates/aura-vision/tests/face_quality.rs` |
| Model-weight wiring | raising `QUALITY_MODEL_WEIGHT` changes the fused score | **met** - asserted, so the zero is a policy rather than dead code |

The monotonicity test is the one worth naming. A quality gate that is not monotonic in its
inputs is worse than no gate: it would sometimes admit the worse of two faces, and nobody
would notice for months.

Mechanical gates in CI: `cargo xtask models`, `crates/aura-infer/tests/parity.rs` (fp16
within 1e-3 of fp32).

## Known failure modes

- **The head's four outputs mean nothing in this build.** Condition C2. The gate is
  currently four measurements and a geometric mean.
- **int8 is forbidden.** Four sigmoid outputs quantised per tensor lose most of their
  resolution near 0 and 1, and this head's whole job is to decide whether a face is above
  0.4 - a gate whose inputs are quantised to sixteen levels is a coin toss for anything
  near the boundary. `PrecisionPolicy::no_int8` is in `models.lock`, so `aura-models`
  refuses the variant rather than trusting this paragraph.
- **The occlusion measurement cannot tell a hand from a blown highlight.** Both are
  featureless regions. It does not need to: both mean the same thing for identity, and the
  reason string says "no facial structure" rather than naming a cause it cannot see.
- **A dark face in a candle-lit ceremony is not penalised for being dark**, only for being
  *crushed*. `Measured::mean_luma` is reported and not gated on, deliberately: a dark
  ceremony frame is the photograph, not a fault. This is also where a fairness failure would
  first appear, which is why the distinction is in the code rather than in a comment.
- **Pose is a geometric estimate from five points**, accurate enough to decide whether a
  face is turned too far to vote and **not** accurate enough to drive a retouch decision.
  Phase 20 must not use it for one.
- **The 48 px gate is in source pixels, not preview pixels.** A face 48 px tall on a
  384 px thumbnail is 256 px on the sensor; gating on the preview would throw away most of
  a wedding's guests. `faces.px_height` records the source figure.

## Fallback

**The gate is the fallback.** When this model is unavailable, `model_usable` is `None`, the
fused score is the four measurements alone - which is exactly what the shipped build already
does at weight 0.0 - and the pass continues unchanged. This is the only model in phase 06
whose absence costs nothing today.

When a trained head exists, its absence will cost the difference between a measured
approximation and a learned one, and the fallback will still be the measurements. That is
the shape invariant 6 asks for: a local path that keeps the pipeline complete.

**Version rollback:** a `quality_ver` bump does not invalidate templates - it invalidates
*gate decisions*, so `faces.votes` is recomputed on the next scan and the identities are
regrouped. The templates are untouched, which is why `quality_ver` is a separate column from
`embed_ver`.

## Fairness and demographic performance

Not published, and not approximated - the same position as the other two cards, and the risk
here is subtler than either.

A quality head is a **gatekeeper**. If it scores one group's faces systematically lower,
those faces do not vote, those people are under-clustered, they appear in fewer identities,
and the failure presents as "the product did not group my cousins properly" rather than as a
bias metric. Nobody would look for a fairness bug there.

Condition **C5** in the phase 06 exit report requires, before this section can be filled:

* per-group distribution of the fused usability score, not just its mean - a shifted
  distribution with an equal mean still gates unequally;
* per-group **vote rate**: the fraction of detected faces that pass the gate. This is the
  number that matters, and it is not the same as accuracy;
* per-group vote rate on the dark-scene subset specifically, because that is where the
  exposure and sharpness factors interact with skin tone most strongly;
* a documented decision about what an acceptable disparity is, agreed with SEC, rather than
  a number reported without a threshold.
