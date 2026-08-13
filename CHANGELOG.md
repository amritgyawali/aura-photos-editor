# Changelog

All notable changes to AURA. One entry per phase, newest first.

## Phase 06 - Face detection, recognition and people intelligence

The app learns who matters at this wedding: it finds every face, groups them into
identities, and ranks the couple, close family and VIPs by evidence rather than by
guesswork. Every later decision gets a subject hierarchy, so sharpness on the bride's face
outranks sharpness on a stranger's elbow.

### Added

- `aura-vision::face`: one decoded frame in, everything phase 06 needs out. Detection with
  a letterbox rather than a centre crop - the faces the tiled pass exists to recover are
  the ones at the edges of a wide ceremony frame - three output strides from one forward
  pass, and faces and bodies predicted by the same anchor, which is why the phase ships
  three models and not four.
- A conditional 2x2 tiled pass that fires on wide-angle frames with several small
  detections, and on frames where bodies were found and faces were not. Its cost is
  recorded per frame in `face_scan.tiled` and reported by `ScanReport::tile_ratio`, because
  "tiled detection doubles cost" is a failure mode to measure rather than assume.
- A bokeh gate that works by geometry rather than by score: a blurred highlight has no
  landmark structure, so its five points collapse towards its centre.
  `Detection::landmark_spread` measures that, which lets the objectness threshold stay low
  and keeps small-face recall.
- ArcFace alignment: a closed-form Umeyama similarity transform onto the published 112 px
  layout, never affine, because an affine fit to five points can shear and a sheared face
  is a different face to a recogniser. Head pose is estimated from the same five landmarks.
- A quality gate that decides which faces may vote on identity: four measured factors -
  sharpness, occlusion, pose, exposure - combined as a weighted **geometric** mean, so a
  perfectly exposed, perfectly frontal, completely out-of-focus face cannot score 0.75 and
  vote, plus two hard cut-offs where the evidence genuinely runs out. A face below the gate
  is detected, stored and displayed; it just does not vote.
- Identity clustering with **exact** average linkage computed from running sums: for unit
  vectors the mean pairwise cosine distance between two clusters is one minus the dot
  product of their unnormalised means, so exact average linkage costs one dot product per
  cluster pair rather than `|A| x |B|`.
- Relative-cohesion verification, which is what actually prevents the chain merge. Two
  looks of one person sit about 1.7 times their own internal spread apart; two siblings sit
  at three times it. A wedding of near-lookalikes records refusals rather than producing
  one identity for six people.
- Sub-centroids for an identity whose members span two looks - the outfit and hairstyle
  change - so a face from either look still matches.
- Role inference from photographic evidence only. **Automation never assigns `bride` or
  `groom`**: the evidence identifies a pair, which of two people is the bride is not a
  photographic fact, and the couple may be same-sex. Confidence is capped at 0.62 while
  scene labels are missing, and the reason string says why.
- Prominence scoring with a versioned weight file, scene-conditioned tables, and
  `subject_focus_score` - the prominence-weighted sharpness phases 09 and 12 use instead of
  naive global sharpness.
- `aura-people`: the sealed biometric store. Templates, centroids and 112 px crops are
  encrypted with a key derived from a per-project secret in the operating system's
  credential store, using BLAKE3 encrypt-then-MAC with a **synthetic nonce** - so
  re-scanning after a model change cannot reuse a keystream, and sealing stays
  deterministic.
- Migration 6: `face_vault`, `face_scan`, `identities`, `faces`, `identity_links`,
  `person_boxes`, `cooccurrence`, and two views. `face_scan` is new in kind: "no faces in
  this frame" is a legitimate result, so the resumability ledger records the *look* rather
  than the finding.
- Merge, split, rename, mark-couple and an importance slider, all undoable, all recorded in
  an append-only journal, and all replayed onto a fresh grouping **by face set rather than
  by identity id** - so a photographer's decision survives a full re-analysis even though
  re-clustering produces new ids.
- Biometric erasure that deletes the credential-store entry *first*, so a crash mid-erasure
  leaves unreadable data rather than readable data, then the crops, then the rows, then
  verifies that nothing survived. Culling and edit decisions are untouched.
- `CoupleHint`, the one cloud call phase 06 may make, behind an ambiguity trigger and a
  two-call cap. Candidates are opaque handles, so a model that answers with a description
  of a person - or volunteers a gender - fails validation rather than being stored.
- Three signed models with cards: `face_detect`, `face_embed`, `face_quality`. `int8` is
  forbidden on the detector, because quantising a box regression moves a 40 px face by
  several pixels, and on the quality head, because quantising four sigmoids destroys the
  resolution the 0.4 gate needs.
- The people IPC surface and the People panel, plus `aura-cli verify --phase 06` as the
  gate: thirteen checks, from the migration to an erasure that leaves nothing behind.
- Nine error codes with runbooks: `AURA-ML-5017` to `AURA-ML-5021` and `AURA-SEC-9001` to
  `AURA-SEC-9005`.

### Changed

- `aura-core` gained the frozen people contract - `Role`, `SubjectHierarchy`,
  `ImageSubjects`, `PeopleService` - and two typed ids, `FaceId` and `IdentityId`. It still
  depends on no other workspace crate.

### Known limitations

- **The three shipped models are placeholders.** The detector finds no faces in a
  photograph and the recogniser's templates carry no identity information. Every gate in
  section 10.1 is measured against synthetic ground truth with a known answer, which proves
  the algorithms and says nothing about the weights. Condition C1, a Sev 2 trigger.
- The quality head's trust weight is 0.0, so the gate is four measured factors. Condition
  C2.
- No demographic analysis is published: the fixtures use one skin tone, and a fairness
  number computed from them would describe a renderer. Condition C5, a Sev 2 trigger.
- The two GPU throughput budgets are waived with an expiry condition; a measured
  processor-path row replaces them.

## Phase 05 - Perceptual embeddings and the wedding similarity index

Every image gets a compact perceptual embedding plus a fast similarity index, so
the app can answer "what looks like this?" across a wedding in milliseconds. It is
the shared vector substrate that scene clustering, burst grouping, duplicate
detection, people grouping, reference-frame selection and consistency checks all
reuse - computed once, in one pass.

### Added

- `aura-index`: the frozen `SimilarityIndex` contract and a deterministic HNSW
  graph behind it - `M = 32`, `ef_construction = 200`, `ef_search = 64`, cosine
  distance on L2-normalised fp16 vectors. Levels come from `blake3(image_id)`
  rather than a generator, every tie breaks by `timeline_ts` then `image_id`, and
  the parallel build is batched rather than concurrent, so two machines with
  different core counts produce byte-identical graphs.
- Filtered queries: k-nearest neighbours, radius search, time-windowed search as a
  pre-filter over a sorted timeline (not a post-filter, which is what keeps a burst
  query under a millisecond), camera restriction, exclusion sets, medoids and
  centroids.
- `aura-vision`: one decode, five results. The embedding, a 64-bit difference hash,
  an 8x8x8 HSV histogram, six luminance statistics and an edge-energy summary all
  come out of the same buffer, which is then dropped - a 4,000-image wedding is
  never 4,000 resident proxies.
- A persisted graph snapshot with six named refusals - missing, wrong magic, wrong
  format, wrong graph parameters, wrong model or preprocessing version, failed
  digest - each of which is a warning and a rebuild rather than a failure to open
  the project. A second open of a 4,000-image wedding is a 23 ms read.
- `wedding_embedding` 1.0.0, signed into `models.lock` with a model card. **It is a
  placeholder backbone**: there is no labelled wedding data in this repository and
  no GPU backend, so a ViT-B/16 with a contrastive head cannot be trained or run
  here. Everything around it is real. See ADR-0011 section 3, and condition C10 in
  the phase 05 exit report.
- `ml/models/embed/`: the dataset specification as executable code - wedding-level
  splits, a cross-tradition holdout, positive and hard-negative mining, and an
  augmentation policy that *cannot express* a flip or a heavy crop - plus the
  contrastive loss, the training schedule, the four evaluation gates and an
  exporter that reproduces the shipped model byte for byte.
- Migration 5: `embeddings` and `descriptors`, 1,623 bytes per image against a
  1.6 KB budget, reversible in three statements.
- The similarity IPC surface (ADR-0012): five commands, five DTOs and three
  telemetry events, plus `ui/src/components/SimilarPanel.tsx` - the debug "find
  similar" panel section 8 calls "invaluable for later phases". No command returns a
  vector, and a test enforces that.
- `aura-cli verify --phase 05` and `just phase-05-verify`: two cards of RAW
  fixtures, a cancelled pass that does nothing, a real pass, the index, a
  five-millisecond query, a time window, a camera filter, the snapshot and its
  refusals, an incremental second card, and determinism through the whole path.
- Four error codes with runbooks: `AURA-ML-5013` (unusable vector),
  `AURA-ML-5014` (snapshot rejected), `AURA-ML-5015` (embedding version drift),
  `AURA-ML-5016` (project past the documented in-memory ceiling).

### Changed

- `cosine_distance` accumulates into eight fixed lanes rather than one, so the
  compiler can vectorise it. With borrowed neighbour lists in place of cloned ones
  this took a 4,000-vector build from 13.3 s to 2.74 s. The lane count is fixed, so
  determinism is unaffected.
- `just budgets` and the CI budget lane run with `--test-threads=1`. A budget suite
  whose cases race each other measures the harness.
- `aura-catalog` gains `repo::set_capture_time`, for a body that recorded no clock -
  and for the phase 05 gate, which needs a wedding-shaped timeline over fixtures
  that carry make and model but no capture time, and says so in its output.
- `aura-cli infer` gained `--input wedding` and a stopwatch, so a 384 px model can
  be timed from the command line.

### Known limits

- The embedding carries no wedding semantics yet, so the purity, NMI and retrieval
  gates from section 6.4 are **deferred**, not passed. The duplicate gate is met and
  is not deferred: it is answered by the difference hash, which has no learned
  component. The evaluation harness computes all four and proves it would fail a
  head that learned nothing.
- Section 11's two GPU throughput budgets are waived - there is no GPU backend - and
  the 400 ms cold-build budget is waived for the build and met for the load. Both
  waivers carry expiry conditions in ADR-0011 section 5.

## Phase 04 - Cloud AI gateway and the agentic reasoning runtime

Paste one API key and the app gains a governed reasoning layer. It is a bonus
tier and never a dependency: with the network unplugged a full wedding still
completes, every decision marked `local_fallback`.

### Added

- `aura-cloud`: the frozen `CloudTask` contract and the seven-step gateway -
  policy, render, inspect, cache, govern, call, settle. It is the only crate in
  the product allowed to open a socket, and `scripts/check-banned.sh` enforces
  that the way it already enforces one runtime for models.
- Four providers behind one shape - Anthropic Messages, OpenAI Chat Completions,
  Google `generateContent`, and OpenAI-compatible self-hosted servers - with
  three-tier model aliasing, so a task names a capability and never a vendor.
- Three transports: a hand-written HTTP/1.1 client, a cassette replayer, and an
  offline refusal. The HTTP client does **not** speak TLS, so this build reaches
  `http://` endpoints - a local Ollama, LM Studio or studio gateway - and not the
  public HTTPS providers. The waiver and its expiry condition are in
  `docs/adr/ADR-0009-cloud-ai-policy.md`.
- Keys in the operating system's own credential store, by command invocation
  rather than FFI, with the secret written to the child's **stdin** and never to
  `argv`. A test asserts that for all three platforms' command shapes at once.
- A JSON Schema validator that refuses a keyword it does not implement rather
  than ignoring it, reports every failing rule at once in a stable order, and
  writes its complaint for a model to act on. Exactly one repair round trip, then
  the local answer.
- A payload builder that cannot upload an original: a full-resolution tiled
  decode and a scene-linear buffer are both refused by type, tiles are capped at
  768 px, and the EXIF summary is an allow-list with no GPS, no filename, no
  serial number and no absolute time. Optional pre-upload face blur.
- A cost governor that prices every call **before** it is made, drops a tier
  rather than a decision when the budget runs low, and stops at the cap without
  stopping the gallery.
- A response cache keyed on task, version, prompt hash, image content hashes and
  model, so re-running a wedding is nearly free and produces identical decisions.
- An audit trail with a row for every decision **including the ones that never
  reached a model**, which are usually the ones worth reading.
- Bounded agent primitives - step cap, deterministic tool ordering, structured
  scratchpad, four limits checked before each step, cancel within one step - for
  phases 27 and 29 to build on.
- `SegmentNaming`, the reference task, with section 7's prompt and schema copied
  verbatim and a controlled vocabulary of eighteen scenes, eighteen rituals and
  eight traditions.
- Migration 4: `cloud_calls`, `cloud_cache`, `cloud_budget`. The consent gate
  frozen in phase 01 has its first caller.
- Ten IPC commands and a Settings > AI keys panel: key entry, Check, caps, the
  privacy switches, a live spend meter and the audit viewer
  (`docs/adr/ADR-0010-cloud-ipc-surface.md`). No command returns a key.
- 14 error codes with runbooks: `AURA-CLOUD-6001..6014`.
- `aura-cli verify --phase 04`: sixteen checks, no network.

### Changed

- Budget assertions now run in release. A budget is a claim about the binary a
  photographer runs, and the payload builder is roughly ten times slower
  unoptimised.
- `aura-perf` gained count and cost budget kinds. Not everything worth budgeting
  is a duration or a size.

### Measured

Gateway overhead 0.08 ms per call (budget 15 ms). 75 calls and USD 1.04 for a
3,000 image wedding (budgets 75 and USD 1.50). 100 % cache hit rate on a re-run
(budget 70 %). A total cloud outage costs 9 ms against a 135 s pipeline floor
(budget 3 %).

### Rules every later phase inherits

- **`CloudAiGateway` is the only way to reach a model provider.** No phase may
  open a socket; the lint enforces it.
- **A task without a local fallback does not compile**, and neither does one
  whose answer cannot state its confidence and reasons.
- **Bump `CloudTask::VERSION` on any prompt, schema or ceiling change.** The
  cache key contains it, and a stale answer is worse than no answer.
- **Cloud proposes; deterministic code decides.** A cloud answer may not overrule
  a local decision at confidence 0.90 or above unless it cites contradicting
  visual evidence, and the conflict is logged.

## Phase 03 - Inference runtime and the signed model registry

One local AI runtime behind one frozen interface, and a model registry that
refuses anything it cannot verify. Nothing in phases 01 and 02 calls it yet;
every AI phase from 05 onwards calls nothing else.

### Added

- `aura-infer`: the frozen `InferService`, a hardware probe that measures a
  machine and writes `hardware_plan.json`, execution-provider negotiation with a
  per-machine set-aside list, a session pool, a batch scheduler with a memory
  ledger, cooperative cancellation, and warmup with visible progress.
- A deterministic interpreter over a documented subset of ONNX opset 13:
  nineteen operators, a protobuf reader *and* writer, and three genuinely
  different numeric paths (fp32, fp16, int8). Pure safe Rust. ONNX Runtime is
  **not** linked - see `docs/adr/ADR-0007-inference-runtime.md` for the four
  reasons and for how a backend is added later without touching a caller.
- `aura-models`: `models.lock` verified by ed25519 then sha256 then model card,
  in that order and entirely offline; resumable transfers against a transport
  port; verify-then-rename installs; a pending/active/rejected state machine that
  rolls a model back automatically when it fails its first real use; and the
  `AURADLT1` block delta with its encoder.
- `tools/model-sign`: offline signing. The release key never enters the
  repository or CI.
- Two placeholder models with model cards, and `cargo xtask models` as the CI
  gate that refuses a model without one (Article VI rule M1).
- `ml/export_onnx`: a second implementation of the file format in Python, which
  produces byte-identical files to the Rust generator - and, where onnxruntime
  happens to be installed, compares our interpreter against it (worst difference
  1.6e-7 on the placeholder models).
- Six IPC commands and a Settings > Hardware panel that lists unavailable
  providers *with their reasons* rather than hiding them
  (`docs/adr/ADR-0008-inference-ipc-surface.md`).
- 17 error codes with runbooks: `AURA-GPU-4001..4005`, `AURA-ML-5001..5012`.
- `aura-cli verify --phase 03`: model integrity, probe, warmup, throughput,
  parity, a forced memory squeeze, cancellation, a misbehaving provider and a
  real rollback, in one run.

### Changed

- `Priority` moved to `aura-core` so the runtime does not depend on preview
  infrastructure. Phase 02's copy is untouched, and a test keeps the two in step.
- `Clock` gained `monotonic_us`, because a 0.4 ms budget measured in whole
  milliseconds can only ever read 0 or 1.
- `scripts/check-banned.sh` refuses any use of ONNX Runtime outside `aura-infer`.

### Known gaps

- No GPU backend, so two of the phase's throughput budgets are unmeasurable and
  are waived with an expiry condition in ADR-0007.
- The models are placeholders; the first trained weights arrive in phase 05.
- No network transport: nothing in the workspace opens a socket yet.
- `InferEvent` is typed on both sides and not emitted, like `IngestEvent` and
  `PreviewEvent` before it, because the Tauri shell has never been launched here.

## Phase 02.1 - Proprietary mosaic codecs, X-Trans, and a parallel decode path

A follow-up to phase 02 that closes most of the camera-coverage gap ADR-0004
opened, and narrows the performance waiver it recorded. No frozen contract
changed, so `pipeline_ver` is unchanged and cached previews stay valid.

### Added

- `aura-raw::codecs`: independent safe-Rust implementations of three formats the
  first cut refused - Nikon's compressed NEF (Huffman coding plus the body's
  linearisation curve, read from MakerNote `0x0096`/`0x008C`), Sony's ARW2 block
  coding, and Olympus's adaptive predictive ORF. Each ships with an **encoder**,
  so every decoder is tested by round trip rather than by assertion.
- X-Trans support end to end: the 6x6 array is read from a DNG's `CFAPattern` or
  a RAF's block directory, binning uses a 3x3 block instead of a 2x2 quad, and
  interpolation widens to 5x5 because a 3x3 window on X-Trans can contain no red
  at all. Tiled tier 3 stays bit-identical to a whole-image decode.
- Fujifilm RAF block-directory parsing: sensor dimensions and colour layout, plus
  the uncompressed mosaic.
- `MosaicScheme`: which decoder a mosaic needs, decided once during the container
  walk. A file that declares no compression but stores too few bytes for its own
  bit depth is now recognised as compressed, which is how Olympus marks its
  scheme.
- 16 tests in `crates/aura-raw/tests/codecs.rs`, and the new encodings added to
  the tier-2 equivalence test and to the `verify --phase 02` cycle.

### Changed

- Demosaic, area-average resize, the colour rotation and the mosaic unpack are
  parallel over output rows. Each row writes into its own slice, so output is
  bit-identical whatever the thread count - invariant 4 rules out a parallel
  float reduction here. Small images stay serial.
- `docs/camera-support.md` and ADR-0004 rewritten around the new matrix.

### Known gaps

- Canon CRX (CR3) and Panasonic RW2 are still not decoded, and compressed RAF
  is not either. Reasons per format are in ADR-0004; all three fall back to the
  embedded preview with `AURA-RAW-2007`.
- A compressed NEF whose decode table we cannot read is refused rather than
  rendered through an invented curve.
- Sony's linearisation curve lives in an encrypted sub-directory. When it is not
  reachable the render uses a documented linear expansion.
- The ADR-0004 performance waiver is renewed, not closed: parallelism made tier 3
  2.1x faster and tier 2 1.4x faster at 25 MP, which is not enough to bring a
  45 MP frame inside budget. Measurements and the two remaining routes are in
  the ADR.

## Phase 02 - RAW decode engine and the three-tier preview pyramid

**Shipped:** instant, colour-correct previews for every RAW - the camera's
embedded JPEG for triage, a 2048 px proxy for AI, and on-demand full-resolution
decode for final render.

### Added

- `aura-raw`: container parsers (TIFF/EXIF, JPEG, ISO base media, Fujifilm RAF),
  format sniffing by magic bytes, CFA unpacking for 8/10/12/14/16-bit and
  lossless JPEG (SOF3), half-size and full demosaic, tiled full-resolution
  decode, EXIF orientation, and a per-file watchdog with memory ceilings.
- `aura-raw::colour`: linear Rec.2020 working space, Bradford adaptation, the
  neutral `filmic_lite` preview curve, the camera-profile resolution chain and a
  CIEDE2000 implementation checked against published worked examples.
- `aura-cache`: content-addressed preview cache keyed by BLAKE3 plus
  `pipeline_ver`, with LRU eviction, a hard budget, digest verification on read
  and an index that rebuilds itself by scanning.
- `aura-preview`: the frozen `PreviewService` trait, strict-priority scheduling
  with de-duplication and promotion, a worker pool that leaves one core free for
  the person, and the catalog-backed source.
- IPC: `get_preview`, `prefetch_previews`, `cancel_previews`, `preview_stats`,
  `set_cache_budget`, `purge_cache`, plus the `PreviewEvent` stream.
- UI: real pixels in the grid, an LRU thumbnail store with cancel-on-scroll, and
  a cache settings panel showing "previews use X GB of Y".
- `aura-cli`: `raw-fixtures`, `previews`, and `verify --phase 02`.
- Synthetic RAW fixtures: eight bench bodies, three mosaic encodings and a
  colour chart, so the decoder is tested without a single camera file.
- Docs: `docs/camera-support.md`, `docs/runbooks/previews.md`, ADR-0003
  (colour pipeline), ADR-0004 (decode backend), ADR-0005 (preview IPC).

### Changed

- `aura-catalog`: `preview` table now written and read (`upsert_preview`,
  `preview_row`, `count_previews`, `photos_without_preview`,
  `primary_file_for_photo`).
- `perf/budgets.toml`: phase 02 stage budgets, plus size budgets for the cache
  and for peak resident memory.
- Frozen contracts re-locked for the preview IPC additions (ADR-0005).

### Known gaps

- Proprietary mosaic compressions (compressed NEF and ARW, RW2, Canon CRX,
  X-Trans) are not decoded; those files render tier 2 from the embedded preview
  and are flagged `AURA-RAW-2007`. See `docs/camera-support.md`.
- The scalar CPU decoder misses the per-image budget at 45 MP; waived for this
  phase in ADR-0004 with measurements.
- No GPU path, no HEIF.

## Phase 01 - Foundation, catalog and wedding ingest

Workspace, error taxonomy with runbooks, SQLite catalog with the six-step
refusal chain, idempotent ingest with multi-camera clock alignment, the job
graph with leases, the typed IPC surface, the virtualised grid, fixtures, CI and
budgets. See `docs/progress/PHASE-01-EXIT.md`.
