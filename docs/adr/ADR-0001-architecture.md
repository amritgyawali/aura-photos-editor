# ADR-0001 - Tauri shell, Rust core, SQLite catalog, ONNX Runtime later

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** CTO (Chief Architect agent), TLC, DEVOPS
- **Phase:** 01

## Context

AURA has to move 1,000 to 4,000 RAW files per wedding through a long pipeline on
a photographer's own laptop, offline, without ever mutating an original file. The
foundation choices made in phase 01 are the ones every later phase inherits, and
they are expensive to revisit once thirty crates depend on them.

## Decision

1. **Desktop shell: Tauri 2 + React 18 + TypeScript + Vite.** The Rust core runs
   in the same process tree as the window, so there is no IPC serialisation of
   image data across a process boundary, and the shipped binary stays small.
2. **Core engine: Rust, stable, pinned exactly.** Memory safety with C-like speed
   and a concurrency story that survives 4,000-file pipelines.
3. **Catalog: SQLite in WAL mode, one writer thread, `STRICT` tables.** Zero
   admin, transactional, portable, and readable during a support call.
4. **Inference (phase 03 onward): ONNX Runtime.** One model artefact per model,
   many execution providers, so the same build serves NVIDIA, DirectML and Apple
   Silicon.
5. **Crate boundaries frozen now:** `aura-core`, `aura-catalog`, `aura-ingest`,
   `aura-jobs`, `aura-perf`, `aura-app`, `aura-cli`, plus `xtask`. `aura-core`
   depends on no other workspace crate, and a test asserts it.

## Consequences

- Every fallible operation returns `AuraError` with a registered code, so the UI
  never renders a Rust type name at a photographer.
- The single-writer rule is enforced twice: by the type system (only `Writer` owns
  a writable connection) and by SQLite itself (readers are `query_only`).
- Choosing SQLite means the catalog must never live in a synced folder; that is a
  hard refusal in `aura_core::paths`, not a warning.
- Tauri means the UI is a web view, so the grid must be virtualised to hold 60 fps
  with 4,000 items. That constraint is designed into `VirtualGrid` from day one.

## Alternatives considered

- **Electron:** larger binary, no in-process Rust, worse memory behaviour on the
  reference machines.
- **Native per-platform UI:** two UIs to build and test for one small team.
- **Postgres or an embedded key/value store:** admin burden or loss of relational
  queries the culling and QC phases will need.
