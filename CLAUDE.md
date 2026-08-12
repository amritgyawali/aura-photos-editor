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
| Phase progress | `docs/progress/PHASE-01.md` and `PHASE-01-EXIT.md` |

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
the runbooks. Phase 02 (RAW decode and the preview pyramid) has not started, and
nothing in `docs/plan/phases/PHASE-02-*.md` may be built until the phase 01 gate in
`docs/progress/PHASE-01-EXIT.md` is fully green.
