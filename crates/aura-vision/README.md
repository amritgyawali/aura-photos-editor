# aura-vision

One pass over the pixels, five things computed from them: the perceptual
embedding, the difference hash, the colour histogram, the luminance statistics and
the edge summary.

Phase 05. Decisions in
[ADR-0011](../../docs/adr/ADR-0011-embeddings-and-similarity-index.md); the model
card is [`docs/model-cards/wedding_embedding.md`](../../docs/model-cards/wedding_embedding.md).

## Why it is one pass

Opening a 384 px preview costs milliseconds. Doing it five times because five
stages each wanted one number from it is what turns an eight-minute pipeline into
an hour. So `embed::run_one` decodes once and produces everything, and the buffer
is dropped before it returns - a 4,000-image wedding is never 4,000 resident
proxies.

| Module | What it holds |
|---|---|
| `embed::model` | The network, and the preprocessing that is part of it: centre crop, box resize to 384, NCHW, `0..1`. |
| `embed::descriptors` | The HSV histogram, the luminance percentiles, the edge energy, the palette, and the box resampler everything else uses. |
| `embed::hash` | The 64-bit difference hash and its Hamming distance. |
| `embed::batch` | The project walk: batched by the hardware plan, resumable, cancellable, honest about what it skipped. |

## Preprocessing is a version, not a parameter

Section 6.1 of the phase document asks for a fused export - resize, normalise,
backbone, head in one graph - "so preprocessing can never drift between training
and inference". The runtime is a deterministic pure-Rust interpreter with no
`Resize` operator (ADR-0007), so the fusion is achieved the other way round:

- every step is a **constant** of `embed::model`, not an argument;
- every stored row carries `PREPROCESS_VER`;
- a change to the crop, the resampler, the channel order or the normalisation is a
  version bump, and a version bump triggers the same background re-embed a new
  model does (`AURA-ML-5015`).

When a real fused export exists, these steps move into the graph and the version
bumps once more.

## What the shipped model is

`wedding_embedding` 1.0.0 is a **placeholder backbone**: a convolutional 512-d
model, not the ViT-B/16 with a contrastive head section 6.1 describes. There is no
labelled wedding data in this repository and no GPU backend to train one against,
so the alternative to a placeholder is a real-looking model whose numbers describe
nothing.

Everything around it is real and is what phases 06 to 29 consume: the 384 px
preprocessing, the 512-d fp16 storage, the batching, the descriptors, the index,
the eval harness. This is condition C10 in
[`docs/progress/PHASE-05-EXIT.md`](../../docs/progress/PHASE-05-EXIT.md).

## The four invariants this crate is built around

**Resumable (5).** The work remaining is a query - `EmbeddingStore::pending` - not
a journal. Kill the process at 10 %, 50 % or 90 % and the next run asks the catalog
what still has no vector at the current version. There is no state that can
disagree with the rows.

**Three-tier (3).** Embeddings are the cheap pass, so they run on the tier that
costs milliseconds: the 384 px thumbnail derived from the camera's embedded
preview. The tier *and* the pixel source are stored per row, because a score
computed from the camera's own look is not the same measurement as one computed
from ours.

**Determinism (4).** The same pixels through the same model produce the same
vector, bit for bit, whatever the batch size and whatever ran before.
`tests/embedding.rs` asserts equality, not a tolerance.

**No silent failure (9).** A frame whose preview will not decode, or whose vector
comes back all zeros, is counted, coded (`AURA-ML-5013`) and reported. The run
continues; one unreadable frame out of 4,000 is an item failure.

## Cost, measured

Release build, development machine (Intel i5-10300H, fp32, tier 1 thumbnail in and
a stored row out): **69 ms per image**, which is 4.6 minutes for a 4,000-image
wedding. The model alone at batch 4 is 38.9 ms.

Section 11's two GPU throughput budgets are waived - there is no GPU backend - and
the processor-path row in `perf/budgets.toml` stands in for them.

## Reading order

1. `src/embed/mod.rs` - `run_one`, which is the whole argument of the phase in
   thirty lines.
2. `src/embed/model.rs` header - why preprocessing lives here and not in the graph.
3. `src/embed/batch.rs` header - the four invariants, as code.
4. `tests/descriptors.rs` - what each descriptor is for, asserted one property at a
   time.
