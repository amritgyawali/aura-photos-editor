# Model card - `wedding_embedding` `1.0.0`

| Field | Value |
|---|---|
| Name | `wedding_embedding` |
| Version | 1.0.0 |
| Task | Fixed-length perceptual embedding of a whole frame, for similarity |
| Class | `embedding` |
| Owner | MLL (ML Lead - Vision) |
| Licence | proprietary |
| Opset | 13 |
| Input | `pixels [N,3,384,384]`, NCHW, `0..1`, sRGB |
| Output | `embedding [N,512]`, L2-normalised outside the graph |
| Precision policy | any: fp32, fp16 and int8 are all permitted |
| Stored version integer | 100 (`model_ver` on every `embeddings` row) |
| Preprocessing version | 1 (`preprocess_ver` on every `embeddings` row) |

## Purpose

One 512-d vector per photograph, so that "what looks like this?" can be answered
across a 4,000-image wedding in milliseconds. Seven later phases read it: scene
clustering (07), burst grouping and duplicate detection (08), coverage (12),
gallery intelligence (25), multi-camera matching (26) and curation (29).

**This 1.0.0 is a placeholder backbone, and every number below says so.** Section
6.1 of the phase 05 document specifies a ViT-B/16 image encoder with a wedding
domain-adaptation head trained with supervised contrastive loss. This build cannot
produce that model, for three reasons that are all recorded in
`docs/adr/ADR-0011-embeddings-and-similarity-index.md` section 3:

* there is no labelled wedding data in the repository, and a head trained on
  generated fixtures would learn the fixture generator rather than weddings;
* `aura-infer` is a deterministic pure-Rust interpreter with no GPU backend
  (ADR-0007), so an 86 M parameter transformer at 384 px would take minutes per
  frame;
* the interpreter's operator subset has no attention primitives, no
  `LayerNormalization` and no `Resize`, so a ViT graph would not load.

What is real, and is what phases 06 to 29 actually consume: the 384 px
preprocessing, the 512-d fp16 storage, the batching, the descriptors computed in
the same pass, the deterministic index, the filtered query API, the snapshot and
the evaluation harness. What is a placeholder is the semantic content of the
vector.

**Any phase whose quality gate depends on the vector being wedding-discriminative
must say so in its own exit report.** This is condition C10 in
`docs/progress/PHASE-05-EXIT.md`.

## Architecture

A strided convolutional trunk with a two-layer projection head, generated from a
fixed seed:

```text
pixels [N,3,384,384]
  Conv 3->32,  3x3, stride 4, pad 1  -> Relu -> MaxPool 2x2   (384 -> 96 -> 48)
  Conv 32->64, 3x3, stride 2, pad 1  -> Relu                  (48 -> 24)
  Conv 64->96, 3x3, stride 2, pad 1  -> Relu                  (24 -> 12)
  GlobalAveragePool -> Flatten                                (-> 96)
  Gemm 96->256 -> Relu                                        (the trunk)
  Gemm 256->512                                               (the projection)
embedding [N,512]
```

231,392 parameters, 926 KB per variant file. Two choices are deliberately faithful
to the real design rather than convenient:

* **The stem is strided by four.** A real vision transformer patchifies `16x16`. A
  convolutional stand-in running at full 384 px resolution would spend most of its
  arithmetic in the first layer and tell a false story about where the cost of an
  embedding model is.
* **The head is two layers with a rectifier between them**, matching section 6.1's
  `768 -> 1024 -> 512` shape at a width the interpreter can execute, so the
  exported graph has the fusion boundary the trained head will have and
  `ml/models/embed/export.py` has something to match.

**L2 normalisation is not in the graph.** The interpreter has no reduction
operator, so it happens in `aura_vision::embed::model::run_batch`, immediately
after the projection and before quantisation. It is covered by
`PREPROCESS_VER`, so it cannot drift silently.

**Preprocessing is part of the model even though it is not in the file.** Section
6.1 asks for a fused export; ADR-0011 section 4 records why that is impossible
here and what replaces it. The steps, all constants of
`aura_vision::embed::model`, are: centre crop to a square, box-filter resize to
384, planar NCHW, divide by 255, no mean or standard deviation subtracted. A change
to any of them bumps `PREPROCESS_VER` and triggers a background re-embed.

## Training data

None. The weights come from the same seeded xorshift generator that produced the
phase 03 placeholders, implemented in `crates/aura-infer/src/onnx/fixtures.rs`, so
every machine writes byte-identical files and `models.lock` verifies everywhere.

There is therefore no consent scope, no licensing question and no wedding-level
split to enforce - no photograph was involved in producing this file.

The dataset this model *will* be trained on is specified rather than assumed, in
`ml/models/embed/dataset.py`: wedding-level train/validation/test splits, a
cross-tradition holdout so a head cannot pass by learning one tradition's palette,
positives drawn from same-scene same-wedding frames within 20 s, hard negatives
from different scenes of the same wedding, and photographic-only augmentation -
exposure +/- 1.5 EV, white balance +/- 800 K, mild noise, JPEG artefacts. **Never
horizontal flips of ritual scenes**, because handedness carries meaning in the
rituals this product is built for, and never heavy crops.

## Latency

Measured on 2026-08-13, release build, `1.97.1-x86_64-pc-windows-gnu`, batch 4,
via `aura-cli infer --model models/wedding_embedding_1.0.0.<precision>.onnx
--input wedding --batch 4`.

| Machine | Provider | Precision | Per image | 4,000 images |
|---|---|---|---|---|
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | fp32 | 38.9 ms | ~156 s |
| Intel i5-10300H, 8 GB, Win 11 (development machine) | cpu | int8 | 46.2 ms | ~185 s |
| RTX 4070 laptop (Win 11, 32 GB) | | | not measured - no such machine | |
| M3 Pro MacBook (18 GB) | | | not measured - no such machine | |
| Intel iGPU desktop (Win 11, 16 GB) | | | not measured - no such machine | |

Rows for machines that do not exist are left empty rather than estimated, which is
the rule phase 03 set and this card follows.

Two notes on the numbers that are here:

* **int8 is slower than fp32 on this interpreter.** That is not a mistake and not
  a finding about int8: the reference backend dequantises to `f32` to compute, so
  the quantised variant pays a conversion the fp32 variant does not. On a real
  accelerated runtime the ordering reverses. It is recorded because a reader who
  finds int8 slower and assumes a bug will waste a day.
* **The whole embedding pass is more than the model.** The measured end-to-end cost
  including preview decode, the four descriptors and the store write is in section
  4 of `docs/progress/PHASE-05-EXIT.md`.

Section 11's two throughput budgets - 4,000 embeddings in 150 s on an RTX 4070 and
300 s on an M3 Pro - are **waived** with an expiry condition, per ADR-0011 section
5, because this build has no GPU backend.

## Quality gate

Section 6.4 of the phase document sets three gates, and none of them can be
measured against this placeholder because all three require human scene labels
that do not exist in the repository:

| Gate | Threshold | Status |
|---|---|---|
| Cluster purity against human scene labels | >= 0.85 | **deferred (C10)** - needs labels |
| Normalised mutual information | >= 0.80 | **deferred (C10)** - needs labels |
| Retrieval mAP@10 over the raw backbone | >= +8 points | **deferred (C10)** - needs a trained head and a backbone to beat |
| Duplicate recall at precision >= 0.95 | >= 0.98 | **met, by the difference hash** - see below |

The fourth is met and is not deferred, because it does not depend on the vector:
near-duplicate detection is answered by the 64-bit difference hash, which is a
deterministic function of the pixels and has no learned component. The
measurement is in `tests/eval/embedding_eval.rs` and is quoted in the exit report.

The gates that *are* enforced on this file today are mechanical, and they run in
CI on every commit:

- `cargo xtask models` - signature, digest, card sections, opset, and that every
  variant compiles;
- `crates/aura-infer/tests/parity.rs` - fp16 within 1e-3 of fp32, int8 within
  1e-2;
- `tests/eval/embedding_eval.rs` - two runs of the same input produce bit-identical
  vectors, and the index returns bit-identical neighbours.

Writing a plausible number into the three deferred rows would be worse than
leaving them deferred: the next person would believe it.

## Known failure modes

- **The vector carries no wedding semantics.** The dominant failure mode and the
  reason for C10. Two ceremony frames and two dance-floor frames are as likely to
  be neighbours as two frames of the same ritual. Structurally correct,
  semantically weak.
- **Centre crop discards the long edges.** A 3:2 frame loses a third of its width.
  A subject at the extreme edge of a landscape frame is not in the tensor. This is
  the standard trade every open image encoder makes, and it is the reason the
  descriptors are computed on the *whole* frame rather than on the crop.
- **Dark scenes.** Section 12 of the phase document names dark dance floors as the
  case generic embeddings collapse. Untested here for want of real dark-scene
  fixtures; the regression fixtures are specified in section 10.1 and are carried
  with C10.
- **A uniformly black or blown frame produces an all-zero vector**, which is
  refused as `AURA-ML-5013` rather than stored. The photograph stays in the
  catalog and never joins a group.
- **No calibration and no confidence.** A distance is not a probability. Nothing
  downstream may treat `1 - distance` as a confidence; the confidence a decision
  reports is the deciding stage's, computed from evidence that includes this
  distance among other things.
- Never seen a photograph, a non-square aspect ratio inside the graph, or a batch
  larger than 64.

## Fallback

Similarity has one, and it is not a smaller model.

**The difference hash is the fallback.** When the model is unavailable - not
installed, rolled back, or its version superseded - `aura_vision::embed` still
computes the 64-bit dHash, the HSV histogram, the luminance statistics and the edge
summary, because none of them involve the model. Near-duplicate detection, which is
what the earliest consumer (phase 08) needs most, works entirely without it.

What degrades is semantic grouping: without vectors, phase 07 has no scene
clustering signal and falls back to the timeline, and phase 25 has the histograms
rather than the embeddings for consistency. The wedding still completes, which is
invariant 6.

**Version rollback:** `models.lock` pins by digest, the registry keeps the previous
version until a new one has completed one real inference (`AURA-ML-5009`), and
`EmbeddingStore::purge_version` forgets one version's rows so the next pass
rebuilds them. A superseded version's vectors are never compared with a current
one's - that refusal is `AURA-ML-5015`.

## Privacy and biometric note

SEC's phase 05 task, recorded here rather than in a separate document so it travels
with the model.

**A 512-d whole-frame embedding is not a biometric template.** There is no face
detector behind it, no per-person structure in it, and no way to reconstruct a
recognisable image from 512 halves - the information is four orders of magnitude
short of the 384x384x3 tensor that produced it, let alone the original frame.

**What it can do is match two photographs of the same room, and that is not
nothing.** In aggregate, embeddings from two projects could establish that two
events happened in the same venue. So:

- embeddings never leave the machine. `aura-cloud` has no dependency on
  `aura-index`, the payload builder cannot carry a vector, and no IPC command
  returns one (ADR-0012);
- they are deleted with the project, by foreign key, in the same transaction;
- they are derived from pixels that are never modified, and the original RAW is
  opened read-only.

Face embeddings are phase 06: a different model, a different index, and a different
consent question, which is why they are explicitly out of scope here (section 2.2).
