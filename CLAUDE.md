# CLAUDE.md - operating manual for this repository

The full operating manual, the nine invariants and the phase ritual live in
[docs/plan/CLAUDE.md](docs/plan/CLAUDE.md). Read that first. This file records what
is specific to the checked-out repository.

## Reading order for an agent

1. `docs/plan/CLAUDE.md` - invariants, phase ritual, hard rules for code.
2. `docs/plan/12-ENGINEERING-CONSTITUTION.md` - the binding engineering rules.
3. `docs/adr/` - every recorded decision; the newest ADR wins over older prose.
4. The single phase file you are implementing, and nothing else.

Never load two phase files into one session.

## Where things are

| Concern | Location |
|---|---|
| Error registry | `crates/aura-core/errors.toml` (one runbook per code in `docs/runbooks/`) |
| Pinned model set | `models/models.lock` + `models/manifest.sig`, checked by `cargo xtask models` |
| Model cards | `docs/model-cards/` (template plus one card per shipped model) |
| Frozen contracts | `crates/*/src/contract/**`, `crates/aura-catalog/migrations/0001_init.sql`, `ui/src/ipc/types.ts` |
| Contract digests | `contracts.lock`, checked by `cargo xtask contracts --check` |
| Budgets | `perf/budgets.toml`, asserted by `cargo test -p aura-perf` |
| Phase progress | `docs/progress/PHASE-0N.md` and `PHASE-0N-EXIT.md` |
| Camera coverage | `docs/camera-support.md` (what decodes, what falls back) |
| Preview troubleshooting | `docs/runbooks/previews.md` |
| Hardware troubleshooting | `docs/runbooks/hardware.md` |
| Adding a model | `docs/runbooks/adding-a-model.md` |
| Cloud AI policy | `docs/adr/ADR-0009-cloud-ai-policy.md` |
| Using your own AI key | `docs/using-your-own-ai-key.md` |
| Recorded provider responses | `tests/cloud/cassettes/` |
| Embedding and index decisions | `docs/adr/ADR-0011-embeddings-and-similarity-index.md` |
| Embedding evaluation gates | `tests/eval/embedding_eval.rs` + `ml/models/embed/eval_retrieval.py` |

## Non-negotiables enforced by the build

- `scripts/check-banned.sh` fails on `unwrap()`, `expect(`, `panic!`, `HashMap::new`,
  `SystemTime::now`, `Instant::now` and `any` in UI source, outside tests, benches,
  `xtask` and `main.rs`.
- Every crate root carries the lint block, including `#![forbid(unsafe_code)]`.
- `aura-core` depends on no other workspace crate; a test asserts it.
- Changing a frozen contract requires an ADR and a re-lock, in that order.

## Building on this machine

The Windows SDK is absent, so the MSVC linker is not available. Use the GNU host
toolchain for everything:

```bash
RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-gnu cargo test --workspace --all-targets
```

`cargo run --release --package xtask` is the one exception that does not link:
`windows-sys` needs `dlltool` for a release import library and MinGW is not
installed. Run xtask in debug (`cargo xtask ...`, which is what the alias does).

## Current state

Phase 01 is implemented: workspace, error taxonomy, catalog schema v1 with the
six-step refusal chain, idempotent ingest with clock alignment, the job graph with
leases, the typed IPC surface, the virtualised grid, the fixture generator, CI and
the runbooks.

Phase 02 is implemented: `aura-raw` (containers, the three decode tiers and the
colour pipeline, pure Rust with no LibRaw - see ADR-0004), `aura-cache`
(content-addressed, budgeted, self-healing), `aura-preview` (the frozen
`PreviewService`, strict-priority scheduling), the preview IPC surface (ADR-0005),
real pixels in the grid, and `aura-cli verify --phase 02` as the gate. Its exit
report is `docs/progress/PHASE-02-EXIT.md`, which lists three conditions - real
camera files, a photographed ColorChecker, and the CI matrix - before phase 03
starts. Nothing in `docs/plan/phases/PHASE-03-*.md` may be built until then.

A follow-up inside phase 02 (section 7b of the exit report) added the
manufacturer mosaic codecs in `crates/aura-raw/src/codecs/` - Nikon compressed
NEF, Sony ARW2, Olympus compressed ORF - plus X-Trans, and made the decode path
parallel over output rows. Canon CRX, Panasonic RW2 and compressed RAF remain
undecoded. **A new codec must ship with an encoder** in `fixtures.rs`: with no
camera files in the repository, a round trip is the only real proof, and
`tests/codecs.rs` is where it goes.

Phase 03 is implemented: `aura-infer` (the frozen `InferService`, a hardware
probe and plan, execution-provider negotiation with a per-machine set-aside list,
a session pool, a batch scheduler with a memory ledger, cancellation and warmup)
running on a deterministic pure-Rust interpreter over a documented ONNX opset 13
subset - ONNX Runtime is deliberately not linked, see ADR-0007; `aura-models`
(ed25519 then sha256 then model card, offline, in that order; resumable
transfers; verify-then-rename installs; automatic rollback; `AURADLT1` deltas);
`tools/model-sign`; two signed placeholder models with cards; the hardware IPC
surface (ADR-0008) and its Settings panel; and `aura-cli verify --phase 03` as
the gate. Its exit report is `docs/progress/PHASE-03-EXIT.md`.

Phase 04 is implemented: `aura-cloud` (the frozen `CloudTask` contract and the
seven-step gateway; four providers behind one shape; three transports - a
hand-written HTTP/1.1 client, a cassette replayer and an offline refusal; keys in
the OS credential store by command invocation with the secret only ever on stdin;
a JSON Schema subset validator with exactly one repair retry; a payload builder
that cannot upload an original; a cost governor that prices before it calls; a
response cache; an audit trail with a row for every decision including the ones
that never reached a model; bounded agent primitives; and `SegmentNaming` as the
reference task), migration 4, the cloud IPC surface (ADR-0010) and its Settings
panel, and `aura-cli verify --phase 04` as the gate. Its exit report is
`docs/progress/PHASE-04-EXIT.md`. TLS is waived (ADR-0009), so this build reaches
`http://` OpenAI-compatible endpoints and not the public HTTPS providers.

Phase 05 is implemented: `aura-index` (the frozen `SimilarityIndex` contract, a
deterministic HNSW graph at `M = 32` / `ef_construction = 200` / `ef_search = 64`,
filtered queries with the time window as a pre-filter over a sorted timeline, a
persisted snapshot with six named refusals, medoids, and the metrics the gates are
measured with), `aura-vision` (one decode, five results: the embedding, a 64-bit
difference hash, an 8x8x8 HSV histogram, six luminance statistics and an edge
summary), migration 5, `wedding_embedding` 1.0.0 signed into `models.lock` with a
card, the training and evaluation code in `ml/models/embed/`, the similarity IPC
surface (ADR-0012) and its debug panel, and `aura-cli verify --phase 05` as the
gate. Its exit report is `docs/progress/PHASE-05-EXIT.md`.

**The shipped embedding is a placeholder backbone and carries no wedding
semantics.** There is no labelled wedding data in this repository and no GPU
backend, so the ViT-B/16 with a contrastive head that phase 05 section 6.1
specifies cannot be trained or run here. Everything around it is real. This is
condition C10 in the phase 05 exit report, it is a Sev 2 trigger, and **no later
phase may claim a quality result that depends on the vector being
wedding-discriminative until it closes.**

Five rules that phase 05 added and every later phase inherits:

- **`SimilarityIndex` is the only way to ask what looks like something.** No phase
  may keep its own vector store or its own graph. A second index is a second answer
  to "are these two frames the same shot", and the two will disagree.
- **A distance is evidence; the deciding phase owns the threshold.** Nothing in
  `aura-index` decides anything, and `query::NEAR_DUPLICATE_HAMMING` is a label in
  a debug panel rather than a policy. Phase 07 owns scene thresholds, phase 08 owns
  duplicate policy.
- **Bump `PREPROCESS_VER` on any change to the pixels the model sees, and
  `MODEL_VER` on any change to the model.** Comparing a vector from one version
  with a vector from another returns a plausible number that means nothing;
  `AURA-ML-5015` exists so that never happens silently.
- **Report coverage when you report a result.** A grouping conclusion drawn over a
  40 %-embedded project is a conclusion about 40 % of a wedding, and
  `IndexStatusDto.coverage` is how a caller finds out.
- **Descriptors are computed once.** The histogram, the luminance percentiles, the
  edge energy and the palette are in the catalog from phase 05 onward. A phase that
  recomputes one of them is opening a file that did not need opening.

Four rules that phase 04 added and every later phase inherits:

- **`CloudAiGateway` is the only way to reach a model provider.** No phase may
  open a socket; `scripts/check-banned.sh` enforces it exactly as it does for the
  inference runtime.
- **A task without a local fallback does not compile**, and neither does one
  whose `Output` cannot state its confidence and reasons. Invariants 2 and 6 are
  trait bounds, not review items.
- **Bump `CloudTask::VERSION` on any prompt, schema or ceiling change.** The
  cache key contains it, and a stale answer served under a contract that no
  longer exists is worse than no answer.
- **Cloud proposes; deterministic code decides.** A cloud answer may not overrule
  a local decision at confidence 0.90 or above unless it cites contradicting
  visual evidence, and the conflict is logged.

Four rules that phase 03 added and every later phase inherits:

- **`InferService` is the only way to run a model.** No phase may link a runtime
  directly; `scripts/check-banned.sh` enforces it. The `Backend` port inside
  `aura-infer` is deliberately *not* frozen, so a GPU backend can be added
  without an ADR and without touching a caller.
- **No model card, no model.** `cargo xtask models` refuses an unsigned manifest,
  a digest that moved, and a card that is missing a required section. It runs in
  CI lane 1 beside the contract check.
- **A model is pending until it has worked once.** A version that fails its first
  real use is rolled back automatically and recorded as rejected; the
  photographer keeps the quality they had that morning.
- **Numbers come from runs.** The GPU throughput budgets in the phase document
  are *waived*, with an expiry condition, rather than filled in with plausible
  figures. Model cards leave unmeasured reference-machine rows empty.

Phase 03 started under a written waiver (ADR-0006): phase 02's three exit
conditions - real camera files, a photographed ColorChecker, and a three-OS CI
run - need inputs that do not exist in the repository. They are carried forward
in section 8 of the phase 03 exit report, and **the first real camera file is a
Sev 2 trigger that reopens phase 02's criteria whatever phase is in flight.**

Two rules that phase 02 added and every later phase inherits:

- **`PIPELINE_VER` is a contract.** It keys both the preview cache and every
  training dataset. Bumping it needs ML-lead sign-off and a model re-validation.
- **Pixels carry their provenance.** `PixelSource` says whether a buffer came
  from the camera's own JPEG or from AURA's documented render. Never mix the two
  in a score without recording which one it was.
