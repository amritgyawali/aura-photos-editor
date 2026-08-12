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

## Non-negotiables enforced by the build

- `scripts/check-banned.sh` fails on `unwrap()`, `expect(`, `panic!`, `HashMap::new`,
  `SystemTime::now`, `Instant::now` and `any` in UI source, outside tests, benches,
  `xtask` and `main.rs`.
- Every crate root carries the lint block, including `#![forbid(unsafe_code)]`.
- `aura-core` depends on no other workspace crate; a test asserts it.
- Changing a frozen contract requires an ADR and a re-lock, in that order.

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
