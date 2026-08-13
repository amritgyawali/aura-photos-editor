# ADR-0011 - Perceptual embeddings and the similarity index

- **Status:** accepted
- **Date:** 2026-08-13
- **Deciders:** CTO, MLL (ML Lead - Vision), SRC (Senior Engineer - Core Pipeline), PERF, SEC, DATA
- **Phase:** 05

## Context

Phase 05 has to produce the vector substrate that phases 06, 07, 08, 12, 25, 26
and 29 all read: one embedding per image, a handful of cheap descriptors computed
in the same pass, and an index that answers "what looks like this?" in
milliseconds. Section 5 of the phase document freezes an API before any of it
exists, which is the right ordering and also the reason this ADR is written
first: three things in that API cannot be written in Rust as spelled, one of the
model requirements cannot be met by this build, and two performance budgets refer
to hardware that does not exist here.

Recording those five facts before writing code is cheaper than discovering them
in review, and every one of them is the kind of thing that quietly becomes a lie
in an exit report if it is not written down.

## Decision

### 1. The contract lives in `aura-index`, and `aura-vision` depends on it

`ImageEmbedding`, `SimilarityIndex`, `IndexFilter` and their supporting types go
in `crates/aura-index/src/contract/index.rs`, which is a frozen contract in
`contracts.lock`. `aura-vision` produces `ImageEmbedding` values and therefore
depends on `aura-index`; the arrow never points the other way, because an index
that knew how embeddings are computed would have to change when the backbone
does.

### 2. Three deviations from the phase document's spelling, and why

The phase document is authoritative about *shape*. Where the spelling does not
compile on stable Rust or would force a caller into a leak, the shape is kept and
the spelling changes. All three are visible in the contract file's doc comments.

| Phase document | Shipped | Why |
|---|---|---|
| `pub vec: [f16; 512]` | `pub vec: [F16; 512]` | `f16` is still unstable on the pinned 1.97.1 toolchain and on the 1.88 minimum (`error[E0658]: the type f16 is unstable`, tracking issue 116909). `F16` is a `u16` newtype in the same file with `from_f32`/`to_f32`, round-to-nearest-even, no dependency, and the same 1,024 bytes on the wire - so a stored vector is byte-compatible with anything that later reads it as `f16`. |
| `exclude: Option<&'static [ImageId]>` | `exclude: Option<&'a [ImageId]>` on `IndexFilter<'a>` | A `&'static` slice can only be produced at runtime by leaking. Every real caller - burst grouping, duplicate detection, reference-frame selection - builds its exclusion set from a query it just ran. |
| `fn insert(&self, e: &ImageEmbedding)` | unchanged, plus `insert_entry` | `insert` cannot carry the timeline stamp and camera the filters need. It stays exactly as frozen and inserts with whatever metadata the index already knows for that id; `insert_entry` is the inherent method the ingest path uses. A vector with no metadata is invisible to a filtered query and is reported in `IndexStats::unfiltered`, never silently included. |

`CameraId` and `SceneId` are dense `u32` handles assigned by the index, not
catalog ids. The catalog's `camera_id` is `cam_<blake3 hex>` and its scenes do not
exist until phase 07; a filter that compared 68-character strings per candidate
would cost more than the distance computation it filters. The index owns the
mapping and exposes `camera_handle(&str)`. Phase 07 assigns scene handles the
same way.

### 3. The embedding model is a placeholder, and the exit report says so

Section 6.1 asks for a ViT-B/16 backbone with a wedding domain-adaptation head
trained with supervised contrastive loss. This build cannot produce that:

- there is no labelled wedding data in the repository, and none can be
  synthesised honestly - a head trained on generated fixtures would learn the
  fixture generator, not weddings;
- `aura-infer` is a deterministic pure-Rust interpreter over ONNX opset 13
  (ADR-0007), with no GPU backend, so a 86 M parameter transformer at 384 px
  would take minutes per image;
- the interpreter's operator set has no `Resize`, no `LayerNormalization` and no
  attention primitives, so a ViT graph would not load.

So phase 05 ships `wedding_embedding` 1.0.0: a convolutional 512-d embedding
model built by the same deterministic generator that produced the phase 03
placeholders, signed into `models.lock`, carded, and exercised end to end at all
three precisions. Everything around it - the runner, the batching, the
descriptors, the store, the index, the queries, the eval harness, the snapshot,
the incremental insert - is real and is what phases 06 to 29 consume.

The training and evaluation code in `ml/models/embed/` is written against the
real design in section 6.1 and is the thing that runs when data and a GPU exist.
It is not decoration: `export.py` emits the same manifest entry shape the Rust
loader reads, and `eval_retrieval.py` computes the same three gates
`tests/eval/embedding_eval.rs` asserts, so the day the head is trained the two
numbers can be compared rather than argued about.

**This is a Sev 2 carried condition, not a completed deliverable.** It is C10 in
the phase 05 exit report and it reopens when labelled data and a GPU backend
exist.

### 4. L2 normalisation happens in Rust, not in the graph

Section 6.1 says "export fused (resize + normalise + backbone + head) so
preprocessing can never drift". The intent is that no caller can apply a
different preprocessing than training did. The interpreter has neither `Resize`
nor a reduction operator, so the fusion is achieved a different way: the
`EmbeddingRunner` owns resize, colour handling and L2 normalisation, they are
constants of the crate rather than parameters, and `descriptors::PREPROCESS_VER`
is stored on every row. A preprocessing change bumps that version and triggers a
re-embed exactly as a model change does. When a real fused export exists, the
runner's steps collapse into the graph and the version bumps once more.

### 5. Three of the five section 11 budgets are waived, two are asserted, and one
### more is added

| Budget | Verdict | Measured |
|---|---|---|
| 4,000 embeddings on an RTX 4070 in <= 150 s | **waived** - no GPU backend exists (ADR-0007). | - |
| 4,000 embeddings on an M3 Pro in <= 300 s | **waived** - same reason, plus no Apple hardware in CI (condition C3). | - |
| HNSW build for 4,000 vectors <= 400 ms | **waived for the cold build**, asserted for the path acceptance criterion 4 describes | 2.74 s cold; **23 ms** from a snapshot |
| kNN query (k=32) <= 5 ms | asserted | **0.28 ms** |
| Storage per image <= 1.6 KB | asserted | **1,623 bytes** |
| *added:* one embedding on the processor path | asserted, budget 110 ms | **69 ms** |

Two of these need their reasoning stated rather than assumed.

**The GPU rows** follow exactly what ADR-0007 did with the phase 03 throughput
budgets: a budget nothing can measure is a wish, and writing a plausible number
into a budget file is worse than leaving it out, because the next person believes
it. In their place, `perf/budgets.toml` gains a processor-path row measured from a
real run, so a regression in the embedding pass has something to fail against.

**The 400 ms build row is breached by the cold build and met by the path the
acceptance criterion actually describes.** Criterion 4 reads "re-opening a project
rebuilds *or* loads the index in under 400 ms". Loading a snapshot takes 23 ms.
Building the graph from scratch takes 2.74 s, and the reason is arithmetic rather
than sloppiness: `ef_construction = 200` with 64 neighbours at layer zero is
roughly 30 GFLOP of 512-wide dot products, computed in safe scalar Rust on four
cores. Two rounds of optimisation - eight-lane accumulation so the compiler can
vectorise the dot product, and borrowing neighbour lists instead of cloning them -
took it from 13.3 s to 2.74 s. The remaining factor of seven needs SIMD
intrinsics, which needs `unsafe`, which the workspace forbids everywhere.

*Waived by PERF and CTO.* The cold build happens once, immediately after an
embedding pass that takes minutes, and every subsequent open reads the snapshot.
*Expiry:* when `std::simd` stabilises, or when a GPU backend lands and the distance
computation moves off the processor - whichever comes first.

What *is* measured on the processor path, and recorded in the exit report and the
model card, is the per-image cost of the shipped placeholder, so that a real
backbone can be compared against a real floor.

### 6. Determinism: level assignment, tie-breaking and accumulation

HNSW is normally built with a random level per node. Random levels mean two runs
build different graphs, and invariant 4 says identical inputs produce identical
outputs.

- Level comes from `blake3(image_id)`, not a generator: same id, same level, on
  every machine and in any insertion order.
- Candidates are ordered by distance, then by `timeline_ts`, then by `image_id`.
  Every tie is broken by data that is already in the row.
- Neighbour lists are sorted before they are stored, so a snapshot's bytes do not
  depend on the order the builder happened to visit.
- Distances accumulate in a fixed order over the 512 dimensions.

The consequence is that `knn` returns the same list on two machines, and the eval
harness can assert exact equality rather than "close enough".

### 7. dHash answers the trivial questions

An exact or near-exact duplicate is found by 64-bit Hamming distance in
nanoseconds. The index is never asked. This is not an optimisation of the index;
it is what keeps the index's recall claim meaningful, because a 4,000-image
wedding contains hundreds of frames that differ by nothing a vector can see.

### 8. Embeddings are catalog rows and are not photographic truth

They live in the catalog because they must be transactional with the photo rows
and must survive a restart, but migration 5 is reversible in three statements and
dropping those tables loses nothing that cannot be recomputed. The HNSW snapshot
is a cache file under the cache directory and is self-healing exactly as the
preview cache is.

### 9. Privacy: an embedding is not a face and is not reversible

SEC's section 9 task. A 512-d global embedding of a whole frame is not a
biometric template: it has no face detector behind it, no per-person structure,
and 512 halves cannot reconstruct a 2048 px image. What it *can* do is match two
photographs of the same room, and in aggregate that is a link between two events.
So:

- embeddings never leave the machine - they are not in the cloud payload builder,
  and `aura-cloud` has no dependency on `aura-index`;
- they are deleted with the project, by foreign key;
- `docs/model-cards/wedding_embedding.md` carries the biometric note in full.

Face embeddings are phase 06, a different model, a different index and a
different consent question.

## Consequences

- Two new crates, `aura-vision` and `aura-index`, and one new frozen contract.
- `contracts.lock` gains `crates/aura-index/src/contract/index.rs` and
  `crates/aura-catalog/migrations/0005_embeddings.sql`.
- `APP_SCHEMA_VERSION` becomes 5.
- `models.lock` gains a third entry and is re-signed.
- Phases 06 to 29 may assume: every analysed image has a vector, a dHash, a
  histogram and luma statistics; queries are deterministic; and a filtered query
  never returns a frame it lacks metadata for.
- Phases 06 to 29 may **not** assume that the shipped embedding is
  wedding-discriminative. Until C10 closes, the vectors are structurally correct
  and semantically weak, and any phase whose quality gate depends on real
  semantics must say so in its own exit report.

## Alternatives considered

**Train a small head on generated fixtures.** Rejected. It would produce a number
for the section 6.4 gates that describes the fixture generator. A gate that
passes for the wrong reason is worse than a gate that is honestly deferred.

**Use a flat brute-force index and skip HNSW.** 4,000 vectors × 512 dimensions is
2 M multiply-adds per query, roughly 1 ms - inside the 5 ms budget. Rejected
anyway: the budget is for 4,000 images and the product's ceiling is 20,000+, the
graph is where the determinism work has to happen, and building it later means
building it under time pressure. The flat scan is kept as `metrics::exact_knn`
and is what the recall test measures the graph against.

**Store vectors as f32.** Rejected: 2 KB per image breaks the 1.6 KB storage
budget on its own, and the phase document specifies fp16.

**Put the contract in `aura-core`.** Rejected: `aura-core` depends on no other
workspace crate and a test asserts it, and pulling embedding types down there
would make every crate in the workspace recompile when a vector width changes.
