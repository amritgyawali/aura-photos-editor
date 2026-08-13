# PHASE-05 progress log

One line per task, in the order they were done. Task codes are from section 9 of
`docs/plan/phases/PHASE-05-EMBEDDINGS-SIMILARITY-INDEX.md`, and section 8's
implementation order is the order below.

| Task | Role | Files touched | Tests added | Notes |
|---|---|---|---|---|
| T1 spec, gates, deviations | MLL, CTO | `docs/adr/ADR-0011-embeddings-and-similarity-index.md` | - | Nine decisions written before any code: where the contract lives, three spellings that cannot compile as printed, why the model is a placeholder, why normalisation is outside the graph, which budgets are waived, how determinism is achieved, why dHash earns its keep, why embeddings are catalog rows, and the biometric note. |
| T2 frozen contract | CTO, SRC | `crates/aura-index/src/contract/index.rs` | `tests/snapshot.rs` (F16 cases) | `ImageEmbedding`, `SimilarityIndex`, `IndexFilter`, `IndexStats`, `LumaStats`, `F16`, `cosine_distance`. `F16` is hand-written: `f16` is nightly on the pinned toolchain and a dependency inside a frozen contract is worse than forty lines of bit twiddling. |
| T3 migration | SRC, DATA | `crates/aura-catalog/migrations/0005_embeddings.sql`, `migrate.rs`, `lib.rs`, `xtask/src/main.rs` | `tests/store.rs` (11) | `embeddings`, `descriptors`, `v_embedding_coverage`. Schema 5. Reversible in three statements, and a test asserts the rollback is recorded in the file. |
| T4 vector store | SRC | `crates/aura-index/src/store.rs` | `tests/store.rs` | The pending-work query is what makes a pass resumable: what is left to do is derivable from the rows, never from a journal that could disagree. |
| T5 the graph | SRC | `crates/aura-index/src/hnsw.rs` | `tests/hnsw.rs` (23) | Section 6.3's parameters. Two departures from the textbook, both for invariant 4: levels from `blake3(image_id)`, and a batched rather than concurrent parallel build. |
| T6 queries | SRC | `crates/aura-index/src/query.rs` | `tests/hnsw.rs` | kNN, radius, time window as a pre-filter, camera, scene, exclusion, medoid, centroid. Distances are evidence; no threshold here is a policy. |
| T7 snapshot | SRC | `crates/aura-index/src/snapshot.rs` | `tests/snapshot.rs` (18) | Six refusals, each with a test. Atomic write. A rejection is a warning and a rebuild. |
| T8 metrics | MLL, QAL | `crates/aura-index/src/metrics.rs` | `tests/eval/embedding_eval.rs` | Exact kNN, recall, mAP, purity, NMI, duplicate PR curve. In the crate rather than in a test file because the gate, the perf suite and the eval harness all need them. |
| T9 error taxonomy | SRC | `crates/aura-index/src/errors.rs`, `errors.toml`, four runbooks | `error_registry` (existing) | `AURA-ML-5013`..`5016`. None of them stops a wedding, and each names its fallback. |
| T10 the model | SRML, MLL | `crates/aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`, `models/`, `docs/model-cards/wedding_embedding.md` | `xtask models` (existing gate) | `wedding_embedding` 1.0.0 at 384 px, 512-d, three precisions, signed. A placeholder backbone, and the card says so in its first paragraph. |
| T11 descriptors | SRC | `crates/aura-vision/src/embed/{descriptors,hash}.rs` | `tests/descriptors.rs` (23) | dHash, HSV histogram, luminance percentiles, edge energy, palette. Each test changes one property of a frame and asserts that the number describing it moves. |
| T12 embedding runner | SRC, SRML | `crates/aura-vision/src/embed/{mod,model}.rs` | `tests/embedding.rs` (13) | Preprocessing is a constant of the crate and a stored version, because the interpreter cannot fuse a resize into the graph. |
| T13 project walk | SRC | `crates/aura-vision/src/embed/batch.rs` | `tests/embedding.rs`, the gate | Batched by the hardware plan, resumable, cancellable, counts what it skipped. |
| T14 dataset spec | DATA, MLL | `ml/models/embed/dataset.py` | `--self-test` | Wedding-level splits, cross-tradition holdout, 20 s positives, same-wedding hard negatives. The augmentation type *cannot express* a flip or a crop; a test asserts the fields do not exist. |
| T15 training | SRML, MLR | `ml/models/embed/train_contrastive.py` | `--dry-run` | The supervised contrastive loss in pure Python, with the two mistakes reimplementations make - anchor in its own positive set, wrong normalising count - tested by asserting that a collapsed batch scores exactly `log(n-1)`. |
| T16 evaluation | QAL, MLL | `ml/models/embed/eval_retrieval.py`, `tests/eval/embedding_eval.rs` | 9 Rust, plus `--self-test` and `--cross-check` | Four gates computed by two independent implementations that must agree to 1e-4. Three are deferred with a printed reason; one is met by the difference hash. |
| T17 export | MLOPS, SRML | `ml/models/embed/export.py` | `--placeholder --verify` | Rebuilds the shipped model byte for byte from the same seed, and refuses any graph using an operator the runtime does not implement - with the operator list parsed out of the Rust source so the two cannot drift. |
| T18 IPC surface | CTO, SFE | `docs/adr/ADR-0012-similarity-ipc-surface.md`, `contract/ipc.rs`, `ui/src/ipc/types.ts` | `index_contract.rs` (9) | Five commands, five DTOs, three events. No command returns a vector, and a test enforces it. |
| T19 app commands | SFE | `crates/aura-app/src/index_commands.rs`, `state.rs` | `index_contract.rs` | One index per project, snapshot-first, rebuilt when a pass embeds anything. |
| T20 debug panel | SFE, UX | `ui/src/components/SimilarPanel.tsx`, `ipc/client.ts` | `SimilarPanel.test.tsx` (17) | Distances *and* percentages, hash distances *and* labels, and a sentence for every reason the index might answer nothing. |
| T21 phase gate | QAL | `crates/aura-cli/src/phase05.rs`, `main.rs`, `justfile`, `ci.yml` | - | `aura-cli verify --phase 05`. Nineteen checks, no network. |
| T22 budgets | PERF | `perf/budgets.toml`, `crates/aura-perf/tests/index_budgets.rs` | `index_budgets.rs` (7) | Three of section 11's rows asserted, two waived, one added. The suite now runs single-threaded, because it was measuring itself. |
| T23 privacy note | SEC | `docs/model-cards/wedding_embedding.md` | `tests/store.rs::deleting_a_project_takes_its_vectors_with_it` | An embedding is not a biometric template; it is also not nothing. Deleted by foreign key, never in a cloud payload, no IPC command returns one. |
| T24 docs | DOC | two crate READMEs, `CHANGELOG.md`, this log, the exit report, `CLAUDE.md` | - | - |

## Measurements taken during the phase

All release builds on the development machine - Intel i5-10300H, 8 GB, Windows 11,
`1.97.1-x86_64-pc-windows-gnu`. Not one of the three reference machines, and every
figure below says what it describes.

| What | Measured | Where |
|---|---|---|
| `wedding_embedding` fp32, batch 4, model only | 38.9 ms per image | `aura-cli infer --input wedding` |
| `wedding_embedding` int8, batch 4, model only | 46.2 ms per image | as above |
| Whole embedding pass, tier 1 thumbnail in, row out | 69 ms per image | `index_budgets::the_embedding_pass_is_inside_the_processor_path_budget` |
| Whole pass on the gate's 17-frame card | 53 ms per image | `aura-cli verify --phase 05` |
| Cold graph build, 4,000 vectors | 2.74 s | `index_budgets::the_index_builds_inside_its_budget` |
| Snapshot load, 4,000 vectors | 23 ms | `index_budgets::loading_a_snapshot...` |
| `knn` at k = 32, 4,000 vectors | 0.28 ms | `index_budgets::a_nearest_neighbour_query...` |
| Time-windowed `knn`, 4,000 vectors | under 1 ms | `index_budgets::a_time_windowed_query...` |
| Storage per image | 1,623 bytes | `store::one_image_costs_less_than_the_section_11_storage_budget` |
| Graph recall at 10 against the flat scan | 0.998 | `embedding_eval::the_graph_agrees_with_the_flat_scan...` |
| Duplicate recall at precision 0.95 | 1.000 | `embedding_eval::duplicate_detection_meets_its_gate` |
| Snapshot size, 4,000 vectors | 5.2 MB | `index_budgets` printout |

### The build optimisation, recorded because the numbers are the argument

The first working build took **13.3 s** for 4,000 vectors against a 400 ms budget.
Two changes, applied together and measured together at **2.74 s** - a factor of
4.9. They were not measured separately, so no split is claimed:

1. **Eight-lane accumulation in `cosine_distance`.** A single-accumulator reduction
   cannot be vectorised by any compiler, because floating-point addition is not
   associative and each add depends on the one before it. Eight independent lanes
   give the optimiser eight chains to fill a register with.
2. **Borrowed neighbour lists instead of cloned ones.** A layer-zero expansion
   reads 64 neighbours and a 4,000-vector build performs about a million
   expansions; cloning each list into a fresh `Vec` was a million allocations.

The remaining factor of seven needs SIMD intrinsics, which needs `unsafe`, which
the workspace forbids. Waived in ADR-0011 section 5 with an expiry.

### The measurement that was measuring the wrong thing

The first budget run reported 6.1 s for the graph build and 167 ms per embedded
image - both roughly twice the honest figures. Two causes, both in the harness:

- `cargo test` runs cases concurrently, so three 4,000-vector index builds were
  competing for four cores. The suite now shares one graph through a `OnceLock` and
  runs with `--test-threads=1`, in `just budgets` and in CI.
- The embedding budget was feeding a 1,024 px proxy, which is not what the pass
  reads: `EMBED_LEVEL` asks the preview service for a 384 px thumbnail. Measuring
  the larger buffer reported the descriptors as three times more expensive than
  they are.

Both are written down here because a budget that measures the harness is worse
than no budget: it fails at random and gets raised until it stops.
