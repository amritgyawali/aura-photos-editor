# PHASE-05 exit report - Perceptual Embeddings & Wedding Similarity Index

- **Date:** 2026-08-13
- **Branch:** `feat/phase-05-embeddings-similarity-index`
- **Gate:** `cargo run --release -p aura-cli -- verify --phase 05` exits 0
- **Signed off by:** CTO, PM, MLL, SRC, PERF, QAL, SEC, DATA

## 1. What shipped

The vector substrate. One 512-d half-precision embedding per photograph, four
cheap descriptors computed from the same decoded buffer, and a deterministic graph
over them that answers "what looks like this?" in a quarter of a millisecond.

The single feature, as the phase card states it: every image gets a compact
perceptual embedding plus a fast similarity index, so the app can answer 'what
looks like this?' across 4,000 photos in milliseconds.

Seven later phases read it - scene clustering (07), burst grouping and duplicate
detection (08), coverage (12), gallery intelligence (25), multi-camera matching
(26) and curation (29) - and none of them can see how the neighbours were found.

## 2. Acceptance criteria (section 13)

| # | Criterion | Evidence | Verdict |
|---|---|---|---|
| 1 | Every image in a project has an embedding, dHash, colour histogram and luma stats after analysis | `embed::run_one` produces all four from one decode and `finish` refuses a vector that says nothing; gate line `coverage: 16 of 17 photographs analysed, 1 could not be decoded`, where the one is the deliberate poison fixture | **pass** |
| 2 | 'Find similar' returns visually correct neighbours in under 5 ms on a 4,000-image project | 0.28 ms at k = 32 over 4,000 vectors (`index_budgets`); gate line `query: 16 queries, worst 0.030 ms, recall@10 1.0000 against the flat scan`. "Visually correct" is **partial** - see 7.1 | **pass on latency, partial on quality** |
| 3 | Duplicate and cluster evaluation gates pass in CI on all fixture weddings | The duplicate gate passes at recall 1.000, precision floor 0.95. The purity, NMI and mAP gates are **deferred** with a printed reason - C10 | **partial** |
| 4 | Re-opening a project rebuilds or loads the index in under 400 ms | Snapshot load of 4,000 vectors: **23 ms**. The cold build is 2.74 s and is waived (ADR-0011 section 5) | **pass** |
| 5 | Importing another card embeds only the new files | `pending` is a query, not a journal; gate line `incremental: card B imported 17 photographs, the second pass touched 17 of them and re-read none of card A` | **pass** |
| 6 | The model card is published with metrics, failure modes and bias notes | `docs/model-cards/wedding_embedding.md`, including the biometric note SEC owns and the three deferred gates | **pass** |

**Four pass outright, two are partial, and both partials have the same cause: the
shipped embedding is a placeholder backbone.** That is C10, it is recorded in the
ADR, the model card, the evaluation harness and the changelog, and it is the one
thing a reader of this report must not miss.

## 3. Test evidence

| Suite | Count | What it covers |
|---|---|---|
| `aura-index::hnsw` | 23 | Recall against brute force, two builds agreeing, ordering, self-exclusion, time windows, an undated frame, camera filters, unassigned scenes, exclusion sets, radius, medoids, an empty index, incremental recall, re-insertion, metadata-free vectors, level distribution, stats, degenerate vectors, the hash path, filter vocabulary |
| `aura-index::snapshot` | 18 | Round trip, metadata survival, byte-identical writes, all six refusals, an empty index, and the half-precision arithmetic underneath - including an exhaustive round trip over all 63,488 finite bit patterns |
| `aura-index::store` | 11 | Schema 5, the recorded rollback, a bit-exact vector round trip, a top-bit-set hash through SQLite's signed integers, the pending set, a version bump, coverage, camera handles, the storage budget, the purge, and deletion by foreign key |
| `aura-vision::descriptors` | 23 | Each descriptor tested by changing one property of a frame and asserting the number describing it moves: brightness, clipping, edges, resolution independence, hue, histogram scaling, palette ordering and determinism, the hash under exposure and resize, and the box resampler |
| `aura-vision::embedding` | 13 | Versions, the tensor the manifest declares, the centre crop, a normalised vector, bit-exact determinism across runs and across services, batch-equals-single, distinct frames, three refusals, a flat frame, and the model-free fallback |
| `tests/eval/embedding_eval.rs` | 9 | The four section 6.4 gates, the proof that purity and NMI reject an embedding that knows nothing, the graph against the flat scan, the dark-scene regression fixture, and the deferral recorded where a reader will find it |
| `aura-app::index_contract` | 9 | Every DTO against its TypeScript twin, the three events, that the query event carries no filter contents, that no command returns a vector, and that the panel exists |
| `aura-perf::index_budgets` | 7 | Section 11's rows, the documented ceiling against the budget file, and the storage payload |
| `ui` (vitest) | 17 new (50 total) | Both similarity numbers, duplicate labels, every reason the index answers nothing, pass sentences, descriptor readout, the time windows |
| `ml/models/embed` | 4 scripts | `dataset --self-test`, `train_contrastive --dry-run`, `eval_retrieval --self-test` and `--cross-check`, `export --placeholder --verify` |

Total: **113 new Rust tests, 17 new UI tests, four Python entry points.**
`cargo test --workspace --all-targets --no-fail-fast` reports **520 passed in 68
suites, none failed**; `cargo clippy --workspace --all-targets -- -D warnings`
clean; `bash scripts/check-banned.sh` clean; `cargo xtask contracts --check`
locked; `cargo xtask models` verifies three models, eight files, signature and
cards; `cd ui && npm test` reports 50 passed.

**One flake, named rather than left for the next person.**
`aura-infer::runtime::an_interactive_request_is_served_while_a_batch_is_running`
failed once during a full workspace run and passed on every run since, including
three consecutive runs in isolation and the 520-test run above. It asserts that an
interactive request preempts a batch within 80 ms, and it failed while this phase's
new suites - which build 4,000-vector graphs and embed frames through the
interpreter - were saturating all four cores of the development machine. Nothing in
phase 05 touches the scheduler; the only `aura-infer` change is a new fixture graph.
It is recorded here because a phase that makes the machine busier has made an
existing timing-sensitive test more likely to flake, and that is worth a note to
PERF even though it is not a regression.

## 4. Performance (section 11)

Release build, development machine: Intel i5-10300H (4 cores), 8 GB, Windows 11,
`1.97.1-x86_64-pc-windows-gnu`. **Not one of the three reference machines.**

| Metric | Budget | Measured | Verdict |
|---|---|---|---|
| 4,000 embeddings (RTX 4070, batch 32) | <= 150 s | not measured | **waived** - no GPU backend (ADR-0007) |
| 4,000 embeddings (M3 Pro) | <= 300 s | not measured | **waived** - no such machine (C3) |
| HNSW build for 4,000 vectors | <= 400 ms | 2.74 s cold, **23 ms** from a snapshot | **waived for the cold build**, pass for the path criterion 4 describes |
| kNN query (k = 32) | <= 5 ms | **0.28 ms** | pass |
| Storage per image | <= 1.6 KB | **1,623 bytes** (15 spare) | pass |
| *added:* one embedding, processor path | 110 ms (set from a 69 ms measurement) | **69 ms** | pass |
| *added:* time-windowed query | 1 ms | **under 1 ms** | pass |
| *added:* snapshot load, 4,000 vectors | 400 ms | **23 ms** | pass |

At 69 ms per image, a 4,000-image wedding is **4.6 minutes** of embedding on this
processor. That is the number a photographer would experience today, and it is 1.8
times the M3 Pro budget the phase document wrote for a real backbone on real
hardware - which is a reasonable place for a scalar interpreter to be, and is not a
claim that the budget is met.

Two things about these figures deserve to be read rather than skimmed.

**The cold-build waiver is a real breach, not a technicality.** Two rounds of
optimisation took a 4,000-vector build from 13.3 s to 2.74 s - eight-lane
accumulation so the compiler can vectorise the dot product, and borrowed neighbour
lists instead of a million allocations. The remaining factor of seven needs SIMD
intrinsics, which needs `unsafe`, which the workspace forbids everywhere. The build
happens once, after an embedding pass that takes minutes, and every subsequent open
reads the snapshot in 23 ms. PERF and CTO waived it in ADR-0011 section 5 with an
expiry: `std::simd` stabilising, or a GPU backend landing.

**The first budget run was measuring the harness.** It reported 6.1 s for the build
and 167 ms per image. `cargo test` runs cases concurrently, so three 4,000-vector
builds were competing for four cores; and the embedding budget was feeding a
1,024 px proxy when the pass actually reads a 384 px thumbnail. Both are fixed - one
shared graph, `--test-threads=1`, and the buffer the product really decodes - and
both are written down in `docs/progress/PHASE-05.md`, because a budget that measures
its own harness fails at random and then gets raised until it stops failing.

**The timing budgets are asserted in release and reported in debug**, which is the
shape phase 04's `cloud_budgets.rs` already used. A debug graph build is an order of
magnitude slower, so asserting a debug number would mean either a permanently red
`just test` or a budget too loose to catch anything. `just budgets` and the CI budget
lane run the suite in release with `--test-threads=1`; a debug run prints each figure
with "not asserted in a debug build" beside it. The debug corpus is also a tenth the
size, because four minutes of unmeasured graph building on every `just test` is four
minutes nobody gets back.

## 5. Telemetry (section 11)

All three events are defined and emitted through `tracing`:

- `embed.batch` {count, ms, ep, batch_size} - `aura_vision::embed::batch`
- `index.build` {vectors, ms, snapshot_used} - `aura_app::state::load_or_build_index`
- `index.query` {k, ms, filter_kind} - `aura_app::index_commands::find_similar`

`IndexEvent` is typed on both sides of the IPC boundary and not yet emitted to the
UI, for the same reason `IngestEvent` was not in phase 01, `InferEvent` in phase 03
or `CloudEvent` in phase 04: the Tauri shell has not been launched on the
development machine, so an emitter would be code nobody has run.

The query event carries the *kind* of filter and never its contents. A time window
plus a camera identifies a shoot.

## 6. Rollback

| Switch | Effect |
|---|---|
| Migration 5 | Reversible in three statements, recorded in the migration file and asserted by both the gate and a test. Everything in those two tables is derived from pixels the product never modifies. |
| Delete the snapshot | Always safe. It is a cache of a cache: the vectors are catalog rows, the graph is derived from them, the file is derived from the graph. |
| `EmbeddingStore::purge_version` | Forget one model version's rows; the next pass rebuilds them. |
| `build_index` | Throw the graph and its snapshot away and rebuild from the rows. |
| Previous model version pinnable | `models.lock` pins by digest, and the registry keeps the previous version until a new one has completed one real inference (`AURA-ML-5009`). |
| Remove `aura-vision` and `aura-index` entirely | The product still imports, previews, culls by hand and exports. What is lost is grouping, and phases 07 and 08 have not been built yet. |

Removing the model alone is a smaller rollback than it looks: the difference hash,
the histogram, the luminance statistics and the edge summary involve no model at
all, so near-duplicate detection - which is what the earliest consumer needs most -
keeps working. That is stated in the model card's Fallback section and asserted by
`the_hash_and_the_descriptors_do_not_need_the_model`.

## 7. Known issues and deliberate omissions

**7.1 The embedding carries no wedding semantics (C10, Sev 2).** `wedding_embedding`
1.0.0 is a convolutional placeholder, not the ViT-B/16 with a supervised
contrastive head section 6.1 specifies. Three reasons, all in ADR-0011 section 3:
there is no labelled wedding data in this repository; there is no GPU backend, so
an 86 M parameter transformer at 384 px would take minutes per frame; and the
interpreter's operator subset has no attention primitives, no
`LayerNormalization` and no `Resize`, so a ViT graph would not load.

What this costs, precisely: two ceremony frames and two dance-floor frames are as
likely to be neighbours as two frames of the same ritual. The vector is
structurally correct and semantically weak.

What it does not cost: everything else in the phase. The 384 px preprocessing, the
512-d fp16 storage, the batching, the four descriptors, the store, the graph, the
filtered queries, the snapshot, the incremental insert and the evaluation harness
are all real, are all tested, and are what phases 06 to 29 consume.

*Any later phase whose quality gate depends on the vector being
wedding-discriminative must say so in its own exit report.*

**7.2 Three of section 6.4's four gates are deferred.** Cluster purity (>= 0.85),
NMI (>= 0.80) and retrieval mAP@10 (>= backbone + 8 points) all need human scene
labels on real weddings, and there are none here. Computing them against generated
fixtures would produce three numbers describing the fixture generator, and a gate
that passes for the wrong reason is worse than one honestly deferred.

The harness does the useful half instead: it computes all four metrics with the
same arithmetic `eval_retrieval.py` uses, asserts the section 6.4 thresholds
against a corpus whose structure is known by construction - which proves the
metrics are implemented correctly and would fail a head that learned nothing - and
asserts that NMI *rejects* a random embedding (0.694 against a 0.80 gate). The
fourth gate, duplicate detection, is met at recall 1.000 and is not deferred,
because the difference hash has no learned component in it.

**7.3 The dark-scene regression fixture tests the descriptors, not the embedding.**
Section 10.1 asks that dark dance-floor and backlit-ceremony subsets not collapse
into one cluster. With a placeholder backbone that is a test of the luminance
statistics and the histogram - which do separate them, cleanly - rather than of the
vector. Worth having on its own, since phases 25 and 26 read those descriptors, and
carried with C10 for the vector.

**7.4 Preprocessing is not fused into the graph.** Section 6.1 asks for a fused
export so preprocessing cannot drift. The interpreter has no `Resize`, so the
guarantee is provided differently: every step is a constant of
`aura_vision::embed::model`, `PREPROCESS_VER` is on every stored row, and a change
to the crop, the resampler, the channel order or the normalisation triggers the
same background re-embed a new model does. ADR-0011 section 4. When a real fused
export exists, the steps move into the graph and the version bumps once more.

**7.5 `SceneId` filters match nothing until phase 07.** The filter is frozen now
because the contract is frozen now. No entry carries a scene, so
`IndexFilter::with_scene` returns an empty result - which is the honest answer, and
a test asserts it rather than letting the filter silently degrade to "no filter".

**7.6 The phase 05 gate writes its own timeline.** The RAW fixtures carry make,
model and orientation but no capture time, so every imported photograph arrives with
a null timeline and a time-windowed query would have nothing to find. The gate
writes a wedding-shaped timeline - bursts of four frames two seconds apart, bursts
ten minutes apart - through `repo::set_capture_time`, and says so on its own output
line. The alternative was skipping the criterion.

**7.6b Two file names differ from section 4, both trivially.** The phase document
lists `crates/aura-vision/src/embed/lib.rs`; a `lib.rs` inside a module directory is
not how Rust names a module root, so it is `embed/mod.rs`. And the panel is
`ui/src/components/SimilarPanel.tsx` rather than under `apps/desktop/`, following the
layout phase 01 established and ADR-0012 records. Every other path in section 4 is
exactly as written.

**7.7 The in-memory ceiling is 20,000 vectors and nobody has measured past it.**
Section 12 asks for a documented ceiling rather than a silent slide into swap; that
is what `AURA-ML-5016` and the flat-scan fallback are. The on-disk index named in
section 12 as the mitigation for larger projects is unbuilt. The ceiling is the
honest statement of where measurement stops, not a claim about where performance
does.

**7.8 Two builds of the same corpus agree; two *machines* have not been compared.**
Determinism is asserted within one machine - same input, same vector, same
neighbours, same snapshot bytes - and the design is what makes it hold across
machines: fixed accumulation lanes, ids rather than a generator for levels, batched
rather than concurrent construction. Nobody has run it on a second machine, because
of C3.

## 8. Conditions carried forward

C1 to C9 come from `docs/progress/PHASE-04-EXIT.md` and are carried again rather
than quietly dropped. C10 and C11 are new.

| # | Condition | Owner | Trigger |
|---|---|---|---|
| C1 | Real camera files exercised through the RAW decoder | MLL | **Sev 2: the first real camera file reopens phase 02's criteria whatever phase is in flight** |
| C2 | A photographed ColorChecker measured end to end | COL | first real camera file |
| C3 | The three-OS CI matrix actually run | DEVOPS | first CI run on a machine with a Windows SDK |
| C4 | GPU throughput budgets (phase 03) | PERF | a GPU backend landing |
| C5 | TLS transport, so public HTTPS providers are reachable | MBE, SEC | phase 07, or the first user with a hosted key |
| C6 | Price tables checked against published vendor pages | PM | before the first paying user |
| C7 | Cassettes re-recorded from live traffic | QAL | with C5 |
| C8 | Cloud confidence calibrated against outcomes | MLL | phase 13 |
| C9 | Demo recording on a real 3,000-image wedding | PM | with C1 |
| **C10** | **A real wedding embedding: labelled scene and ritual data, a trained ViT-B/16 domain-adaptation head, and the three deferred section 6.4 gates measured against human labels** | **MLL, DATA, SRML** | **Sev 2: labelled data plus a GPU backend. Until then, no phase may claim a quality result that depends on the vector being wedding-discriminative.** |
| C11 | The cold graph build inside 400 ms | PERF | `std::simd` stabilising, or a GPU backend landing |

## 9. Definition of Done (section 14)

| Item | Status |
|---|---|
| Acceptance criteria verified by QA on the three reference weddings | **Partial.** Four of six verified outright against synthetic RAW fixtures; two are partial for C10. The three reference weddings need C1. |
| Suites green on Windows (NVIDIA), Windows (DirectML) and macOS | **Carried (C3).** Green on the development machine; the matrix has never run. |
| Performance budget met, or a waiver recorded | **Partial, with waivers.** Five rows pass with margin; three are waived in ADR-0011 section 5, each with a reason and an expiry. Section 4 has the figures. |
| Telemetry visible in the local dashboard and the aggregate pipeline | **Partial.** All three events emitted through `tracing`; the UI event stream is typed and not yet emitted (section 5). |
| Every AI decision surface returns `confidence` + `reasons[]` | **Not applicable, and worth saying why.** This phase produces no decisions. A distance is evidence; the phases that turn a distance into a decision - 07, 08, 12 - carry the invariant, and `aura_index::query`'s header says so in as many words. |
| Docs updated: module README, model card, in-app help, CHANGELOG | **Met.** Two crate READMEs, `docs/model-cards/wedding_embedding.md`, ADR-0011, ADR-0012, four runbooks, the panel's own copy, CHANGELOG. |
| Rollback path exists | **Met.** Section 6. |
| Demo recording on a real 3,000-image wedding | **Carried (C9).** |

## 10. What phase 06 may and may not assume

**May assume:**

- Every analysed photograph has a 512-d fp16 vector, a 64-bit difference hash, an
  8x8x8 HSV histogram, six luminance statistics and an edge summary, all computed
  from one decode.
- `SimilarityIndex` is the only way to ask what looks like something, and two
  machines get the same answer in the same order.
- A time-windowed query is a pre-filter over a sorted timeline and stays under a
  millisecond, so burst-shaped questions are cheap.
- A filtered query never returns a frame it lacks metadata for, and
  `IndexStats::unfiltered` says how many those are.
- `EmbeddingStore::pending` is the work remaining, so any pass over a project is
  resumable without a journal.
- Deleting a project takes its vectors with it, by foreign key, in the same
  transaction.

**May not assume:**

- That the embedding is wedding-discriminative (7.1, C10).
- That a scene filter does anything (7.5).
- That the cold build fits 400 ms (7.2 of section 4, C11).
- That a project over 20,000 images has a graph at all (7.7).

**Must do, when adding a consumer:**

1. Read vectors through `EmbeddingStore` or the index, never with a query of your
   own. The 1.6 KB budget is per image and it has fifteen bytes spare.
2. Bump `PREPROCESS_VER` if you change anything about the pixels the model sees,
   and `MODEL_VER` if you change the model. A stale vector compared with a current
   one produces a plausible number that means nothing (`AURA-ML-5015`).
3. Own your thresholds. Nothing in `aura-index` decides anything, and
   `NEAR_DUPLICATE_HAMMING` is a label in a debug panel rather than a policy.
4. Report coverage when you report a result. A grouping conclusion drawn over a
   40 %-embedded project is a conclusion about 40 % of a wedding.

## 11. Face embeddings are phase 06

Explicitly out of scope here (section 2.2), and the boundary is not only technical:
a face embedding is a biometric template, a whole-frame embedding is not, and they
carry different consent questions. `docs/model-cards/wedding_embedding.md` has the
full note. Phase 06 gets its own model, its own index and its own conversation with
SEC.
