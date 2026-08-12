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
| Frozen contracts | `crates/*/src/contract/**`, `crates/aura-catalog/migrations/0001_init.sql`, `ui/src/ipc/types.ts` |
| Contract digests | `contracts.lock`, checked by `cargo xtask contracts --check` |
| Budgets | `perf/budgets.toml`, asserted by `cargo test -p aura-perf` |
| Phase progress | `docs/progress/PHASE-0N.md` and `PHASE-0N-EXIT.md` |
| Camera coverage | `docs/camera-support.md` (what decodes, what falls back) |
| Preview troubleshooting | `docs/runbooks/previews.md` |

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

Two rules that phase 02 added and every later phase inherits:

- **`PIPELINE_VER` is a contract.** It keys both the preview cache and every
  training dataset. Bumping it needs ML-lead sign-off and a model re-validation.
- **Pixels carry their provenance.** `PixelSource` says whether a buffer came
  from the camera's own JPEG or from AURA's documented render. Never mix the two
  in a score without recording which one it was.
