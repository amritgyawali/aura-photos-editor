# aura-index

The wedding similarity index: one fp16 vector per photograph, a deterministic
HNSW graph over them, and the filtered queries seven later phases ask.

Phase 05. Contract frozen in `src/contract/index.rs`; decisions in
[ADR-0011](../../docs/adr/ADR-0011-embeddings-and-similarity-index.md).

## What this crate is for

Almost every wedding-intelligence question is a similarity question:

| Question | The call |
|---|---|
| Which frames are this burst? | `knn` with `IndexFilter::within_seconds(2)` |
| Which of these forty ceremony shots is *the* one? | `medoid` over the set |
| Is this a duplicate of that? | `dhash_within`, no graph involved |
| What did this room look like to the second camera? | `knn` with `with_camera` |
| Has this moment already been covered? | `radius` over the scene |

Computing one good embedding per image and reusing it for all of them is the
difference between an eight-minute pipeline and an hour-long one. This crate owns
everything after the vector exists; `aura-vision` produces them.

## Layout

| Module | What it holds |
|---|---|
| `contract::index` | **Frozen.** `ImageEmbedding`, `SimilarityIndex`, `IndexFilter`, `F16`, `LumaStats`, `cosine_distance`. |
| `store` | The two catalog tables migration 5 adds, and the pending-work query that makes a pass resumable. |
| `hnsw` | The graph: `M = 32`, `ef_construction = 200`, `ef_search = 64`, cosine distance, batched parallel build. |
| `snapshot` | The persisted graph, its six refusals, and an atomic write. |
| `query` | `find_similar`, medoids, centroids, and the near-duplicate reporting boundary. |
| `metrics` | The brute-force truth the graph is measured against, plus purity, NMI, mAP and the duplicate PR curve. |
| `errors` | Four `AURA-ML-5xxx` codes, each with a fallback and a runbook. |

## The three things worth knowing before you change it

**Determinism is the design constraint.** A textbook HNSW draws each node's level
from a random generator, which means two runs of the same import build two
different graphs and every clustering decision downstream becomes
irreproducible. Here the level comes from `blake3(image_id)`, every tie breaks by
`timeline_ts` then `image_id`, neighbour lists are sorted before they are stored,
and the dot product accumulates into a fixed eight lanes. Two machines return the
same neighbours in the same order, which is why the eval harness can assert
equality rather than a tolerance.

**The parallel build is batched, not concurrent.** Nodes are inserted in
geometrically growing chunks: every member of a chunk searches the same frozen
graph, in parallel, and then the chunk's links are committed in order. A
lock-per-node concurrent insert would make the result depend on thread
scheduling. Two machines with different core counts produce byte-identical
graphs.

**A distance is evidence, not a verdict.** Nothing here decides anything. Whether
two frames are "the same shot" is phase 08's policy and whether they are "the same
scene" is phase 07's. The one number that looks like a threshold,
`query::NEAR_DUPLICATE_HAMMING`, is a label for the debug panel and says so.

## Cost, measured

Release build, development machine (Intel i5-10300H, 4 cores), 4,000 vectors:

| Operation | Measured | Budget |
|---|---|---|
| Cold graph build | 2.74 s | 400 ms - **waived**, see ADR-0011 section 5 |
| Snapshot load | 23 ms | 400 ms |
| `knn` at k = 32 | 0.28 ms | 5 ms |
| Time-windowed `knn` | under 1 ms | 1 ms |
| Storage per image | 1,623 bytes | 1,638 |
| Recall at 10 against the flat scan | 0.998 | 0.95 |

The in-memory ceiling is 20,000 vectors (`hnsw::IN_MEMORY_CEILING`). Past it,
`AURA-ML-5016` is raised and queries fall back to the exact flat scan - slower per
query, and more accurate, since it is the answer the graph approximates.

## Reading order

1. `src/contract/index.rs` - the shapes, and the three places the spelling differs
   from the phase document.
2. `src/hnsw.rs` header - the two ways this is not textbook HNSW.
3. `tests/hnsw.rs` - what a caller is entitled to assume.
4. `tests/eval/embedding_eval.rs` at the repository root - the section 6.4 gates,
   and which of them are honestly deferred.
