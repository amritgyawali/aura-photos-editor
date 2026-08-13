# Model card - `face_embed` `1.0.0`

| Field | Value |
|---|---|
| Name | `face_embed` |
| Version | 1.0.0 |
| Task | 512-d face recognition template from an aligned crop |
| Class | `embedding` |
| Owner | MLL (ML Lead - Vision) |
| Licence | proprietary |
| Opset | 13 |
| Input | `pixels [N,3,112,112]`, NCHW, `0..1`, sRGB, ArcFace-aligned |
| Output | `embedding [N,512]`, L2-normalised outside the graph |
| Precision policy | any: fp32, fp16 and int8 are all permitted |
| Stored version integer | 100 (`embed_ver` on every `faces` row) |
| Parameters | 209,072 |
| File size | 837,393 bytes per variant |
| At rest | **sealed** - see the biometric note below |

## Purpose

One 512-d template per face, so that "is this the same person" can be answered across
twelve hours in which the light changed four times, the hair came down and the jacket
came off - and answered **no** for two sisters who look alike. It is the input to
identity clustering, which is the input to the subject hierarchy every later phase ranks
by.

**A face template is not a perceptual embedding.** The phase 05 vector in
`docs/model-cards/wedding_embedding.md` is trained to notice the venue, the light and the
dress. This one has to ignore all three. The two are different models, in different
tables, sealed differently, and **never compared with each other** - a distance between
one of each is a number that means nothing.

**This 1.0.0 is a placeholder and its templates carry no identity information.** Section
2.1 specifies an ArcFace-class recogniser; there is no labelled identity data in this
repository and no GPU backend, so shipping a trained-looking model would mean shipping an
F1 score that describes nothing. Condition **C1** in
`docs/progress/PHASE-06-EXIT.md`, a Sev 2 trigger.

What is real: the ArcFace 112 px alignment geometry, the similarity-transform warp, the
L2 normalisation and its rounding, the fp16 storage, the sealed envelope, the exact
average-linkage clustering, the rank-order verification, the sub-centroids, and the
second-pass assignment margin. Clustering F1 is measured against synthetic templates with
controlled separation, including a deliberate lookalike-relative case.

## Architecture

An ArcFace-shaped convolutional trunk with a two-layer projection head, generated from a
fixed seed:

```text
pixels [N,3,112,112]
  Conv 3->24,  3x3, stride 2, pad 1 -> Relu -> MaxPool 2x2   (112 -> 56 -> 28)
  Conv 24->48, 3x3, stride 2, pad 1 -> Relu                  (28 -> 14)
  Conv 48->96, 3x3, stride 2, pad 1 -> Relu                  (14 -> 7)
  GlobalAveragePool -> Flatten                               (-> 96)
  Gemm 96->256 -> Relu                                       (the trunk)
  Gemm 256->512                                              (the projection)
embedding [N,512]
```

The head is two layers with a rectifier between them so that the exported graph has the
same fusion boundary a trained ArcFace head will have.

**L2 normalisation is not in the graph.** The interpreter has no reduction operator
(ADR-0007), so it happens in `aura_vision::face::embed::FaceEmbedder::run_batch`,
immediately after the projection and before quantisation - and the quantisation reuses
`aura_index::contract::index::F16`, phase 05's frozen half-precision rule. There is
therefore exactly one answer in this product to "what does 0.031 25 become in sixteen
bits", which matters because two implementations would disagree once and then produce two
identity graphs.

### Alignment is part of the model, and its geometry is not ours

The input must be a 112x112 crop warped so the five landmarks sit on
`aura_vision::face::align::ARCFACE_REFERENCE` - the published ArcFace layout. Every
trained recogniser in this family expects it, and changing a coordinate by a pixel
degrades every template without failing anything. The constant carries a warning rather
than a tuning comment.

The warp is a **similarity** transform - scale, rotation, two translations - fitted by the
closed-form Umeyama solution over the five points. Not affine: an affine fit to five
points can shear, and a sheared face is a different face to a recogniser trained on
unsheared crops. Closed-form rather than iterative: an SVD from a linear-algebra crate
would put invariant 4 at the mercy of a `cargo update`.

Resampling is bilinear. Nearest neighbour would make a template depend on sub-pixel
landmark noise; anything wider would blur a 48 px face into mush at the moment the quality
gate is deciding whether it is sharp enough to vote.

### No clothing, no context, no frame

Section 6.2 forbids clothing features in the identity signal, and the enforcement is
architectural rather than by discipline: `aura_vision::face::embed` only ever receives the
112 px aligned crop. It has no access to the frame, so it cannot accidentally learn that
the person in the blue lehenga at 6 pm is the person in the blue lehenga at 7 pm.

## Training data

None. Seeded xorshift weights from `crates/aura-infer/src/onnx/fixtures.rs`, byte-identical
on every machine. No photograph was involved, so there is no consent scope and no
identity-level split to enforce.

The dataset a trained version needs is specified in `ml/models/face/eval_identity.py` and
section 10.1:

* **wedding-level splits**, and within a wedding, identity-level - the same person must
  never appear in train and test;
* a **lookalike-relatives subset** with labelled siblings, because that is the failure the
  rank-order verification exists to survive and it cannot be measured without one;
* an **outfit-and-hairstyle-change subset**, spanning at least two looks per identity,
  because that is what the sub-centroids in `identities.sub_centroids` are for;
* **balanced skin tones with consent records** - see the fairness note.

Augmentation is photographic only: exposure, white balance, mild noise, JPEG artefacts.
No horizontal flips of ritual scenes, for the reason phase 05's card gives: handedness
carries meaning in the rituals this product is built for.

## Latency

Measured on 2026-08-13, release build, `1.97.1-x86_64-pc-windows-gnu`, batch 8, via
`aura-cli infer --model models/face_embed_1.0.0.<precision>.onnx --input face-crop
--batch 8`.

| Machine | Provider | Precision | Per face | 10,000 faces |
|---|---|---|---|---|
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp32 | 9.1 ms | ~91 s |
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | int8 | 10.5 ms | ~105 s |
| RTX 4070 laptop (Win 11, 32 GB) | | | not measured - no such machine | |
| M3 Pro MacBook (18 GB) | | | not measured - no such machine | |
| Intel iGPU desktop (Win 11, 16 GB) | | | not measured - no such machine | |

Ten thousand faces is a 4,000-image wedding at the fixture set's 2.4 faces per frame.

int8 being slower is the interpreter's dequantise-to-compute path, not a finding about
int8; see the same note on `face_detect`.

## Quality gate

Section 10.1's identity gates:

| Gate | Threshold | Status |
|---|---|---|
| Identity clustering F1, pairwise | >= 0.93 | **met against synthetic templates** (`tests/eval/identity_eval.rs`); **deferred (C1)** against photographs |
| No cluster contains two labelled people | 0 impure clusters | **met against synthetic templates**, including the lookalike-siblings case; **deferred (C1)** |
| Siblings are not merged | 0 sibling merges | **met against synthetic templates** at 0.72 blend; **deferred (C1)** |
| Verification threshold calibrated per project | tuned sweep | **implemented, untuned** - `tune_threshold` and `ml/models/face/tune_cluster.py` exist and run; the shipped 0.55 default is the ArcFace-literature operating point, not a measurement on this model |

The synthetic templates are not a substitute for real ones and the distinction is
explicit: `aura_vision::face::fixtures::synthetic_template` produces directions with
*controlled* intra-identity spread and inter-identity separation. That is what makes them
usable as ground truth - a clustering failure against them is a failure of the algorithm
rather than of the fixture - and it is also what makes them silent about whether a trained
model separates two real sisters.

Mechanical gates in CI on every commit:

- `cargo xtask models` - signature, digest, card sections, opset;
- `crates/aura-infer/tests/parity.rs` - fp16 within 1e-3 of fp32, int8 within 1e-2;
- `tests/eval/identity_eval.rs` - clustering F1, impurity, the sibling case, and
  bit-identical templates across two runs;
- `crates/aura-people/tests/vault.rs` - a sealed template round-trips, and a tampered one
  is refused.

## Known failure modes

- **The templates carry no identity information.** The dominant failure mode; condition
  C1.
- **An all-zero or non-finite template is refused, not stored.** It would otherwise sit at
  distance 1.0 from everything and become its own identity, then its own guest.
  `aura_vision::face::embed::is_degenerate` catches it and the face is stored with
  `embed = NULL` and `votes = 0`.
- **Alignment failure poisons the template silently.** Five coincident landmarks - which
  a bokeh highlight produces - yield a degenerate transform, a black crop and a
  meaningless template. Mitigated upstream: the landmark-spread gate in
  `aura_vision::face::detect::nms` rejects the detection before it is ever warped.
- **Twelve hours of light change is the hard case, and sub-centroids are a partial
  answer.** An identity whose members span two very different looks has a main centroid
  halfway between them that matches neither; `identities.sub_centroids` adds the two looks
  as additional match targets. It does not solve the case where one look was never
  clustered in the first place.
- **A distance is not a probability.** Nothing downstream may treat `1 - distance` as a
  confidence. The confidence a decision reports is the deciding stage's.
- Never seen a photograph, an unaligned crop, or a batch larger than 32.

## Fallback

**Identity grouping has no fallback model, and that is the honest position.** Without a
recogniser there are no templates, so there are no identities. The face pass still runs:
detection, landmarks, pose, quality, bodies and people counts are all computed and stored,
because none of them involves this model. What is lost is *who* - the boxes are there and
the names are not.

Downstream, that means `SubjectHierarchy::weights` is empty, `coverage` reports what was
scanned, and every consumer falls back to its non-people path. The wedding still culls,
edits and delivers.

**Version rollback:** an `embed_ver` bump makes stored templates incomparable with new
ones. `AURA-ML-5018` reports the mismatch, the rows are kept, and a background re-scan
replaces them. Two versions are never compared.

## Fairness and demographic performance

Not published, and not approximated. Section 12's second failure mode is exactly this
model's risk, and the synthetic fixtures use one skin tone - so a per-group F1 computed
from them would be a number about a renderer.

Condition **C5** in the phase 06 exit report requires, before this section can be filled:

* a balanced evaluation set with per-group consent records;
* per-group verification true-accept rate at a fixed false-accept rate, which is the
  metric a recogniser's disparity actually shows up in - not accuracy;
* per-group clustering F1 and impurity, reported separately;
* the **stricter-margin** mitigation, which is already implemented rather than promised:
  `ClusterConfig::assign_margin` leaves an ambiguous face unassigned instead of assigning
  it, so a fairness failure becomes a visible gap rather than a wrong name on somebody's
  grandmother.

## Privacy and biometric note

**This model's output is a biometric identifier, and it is the most sensitive data this
product holds.**

- **Sealed at rest.** `faces.embed` never holds a vector. Every template is sealed by
  `aura_people::vault` with a key derived from a per-project secret in the operating
  system's credential store. A catalog copied off the machine without the keychain entry
  has no biometric data in it. The construction, and why it is BLAKE3 rather than
  `chacha20poly1305`, is documented in that module and in
  `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` section 4.
- **Project-scoped, permanently.** Section 2.2 forbids cross-wedding identity
  persistence. Enforced by the schema, by `PeopleStore::assert_same_project`, and by a
  test; `AURA-SEC-9004` halts rather than degrades.
- **Erasable, and verified.** `PeopleStore::erase` deletes the credential-store entry
  first, so a crash mid-erasure leaves unreadable data rather than readable data, then the
  sealed crops, then the rows, then checks that nothing survived. Culling and edit
  decisions are untouched.
- **Never uploaded.** `aura-cloud` has no dependency on `aura-people`, the payload builder
  cannot carry a template, and no IPC command returns one.
- **Aligned crops are sealed too.** A 112 px crop of a face is a recognisable image of a
  person, so it is treated as biometric data and not as a cache of pixels.
