# ADR-0002 - Toolchain pin, repository layout and deliberate deviations from the blueprint

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** DEVOPS, CTO (Chief Architect agent)
- **Phase:** 01

## Context

The phase 01 blueprint (`docs/pdf/P01-01 … P01-10`) specifies the scaffold down to
individual file contents. Three of its instructions could not be followed
literally, and the blueprint itself requires that any such divergence is recorded
here rather than absorbed silently.

## Decisions

### 1. The toolchain is pinned to 1.97.1, not 1.83.0

The blueprint pins `1.83.0`. Every current version of the dependencies it also
specifies (`time`, `uuid`, `proptest`, `indexmap`, `getrandom`) now declares a
minimum supported Rust version above 1.83, so that combination cannot resolve.

We keep the *principle* - the toolchain is a contract, pinned exactly, and a bump
requires re-running the determinism suite - and update the number.
`workspace.package.rust-version` is `1.88`, the highest MSRV in the dependency
graph, so a contributor on an older toolchain gets a clear error instead of a
mysterious build failure.

### 2. Migrations live under the crate that owns them

The blueprint's `include_str!` path implies a workspace-root `migrations/`
directory, while its own contract list and `CLAUDE.md` both name
`crates/aura-catalog/migrations/0001_init.sql`. We use the crate-local path: a
migration is part of the catalog crate's contract, and keeping it there means
`cargo test -p aura-catalog` works from a fresh checkout with no path assumptions.

### 3. `0001_init.sql` does not contain its own `schema_version` insert

The blueprint's SQL ends with a parameterised `INSERT INTO schema_version … VALUES
(1, :applied_at, …)`. Named parameters cannot be bound through `execute_batch`,
and the runner has to record the app version and the migration hash anyway. The
insert therefore lives in `migrate.rs`, immediately after the batch, inside the
same transaction. The observable result is identical; the schema file stays pure
DDL and stays hashable as a frozen contract.

### 4. `unicode-normalization` added to the workspace dependency list

`util::compare_key` needs NFC normalisation to recognise a macOS NFD filename and
a Windows NFC filename as one file. The blueprint's own `util.rs` uses this crate
but its workspace manifest omits it. Added, with no other new dependencies.

### 5. Error-code numbering in the DB domain

`P01-02` registers `AURA-DB-3005` (second writer) and `P01-03`'s tests assert
`AURA-DB-3004` for a newer schema and `AURA-DB-3003` for corruption. The registry
in `crates/aura-core/errors.toml` follows those assertions:

| Code | Meaning |
|---|---|
| `AURA-DB-3001` | catalog could not be opened, or is not an AURA catalog |
| `AURA-DB-3002` | migration failed and was rolled back |
| `AURA-DB-3003` | integrity check failed |
| `AURA-DB-3004` | catalog written by a newer build |
| `AURA-DB-3005` | another instance holds the write lock |
| `AURA-DB-3006` | statement or transaction failed |
| `AURA-DB-3007` | pre-migration backup could not be verified |
| `AURA-DB-3008` | busy past the write timeout |

Every code has a runbook in `docs/runbooks/` and a test proves it.

### 6. The desktop shell is not a workspace member

`ui/src-tauri` is excluded from the Cargo workspace. `cargo test --workspace` must
stay headless and fast in CI; pulling the Tauri toolchain into every test run
costs minutes and adds system dependencies (WebKitGTK on Linux) that the core
crates do not need. The shell is built explicitly with `just dev` or
`cargo build --manifest-path ui/src-tauri/Cargo.toml`.

### 7. A local GNU host toolchain is an accepted developer workaround

The pinned target for shipping on Windows is `x86_64-pc-windows-msvc`, and CI
builds it. A developer machine without a Windows SDK cannot link MSVC at all, and
installing one needs an elevated installer.

Such a machine may build and test locally with the GNU host toolchain instead:

```powershell
$env:RUSTUP_TOOLCHAIN = "1.97.1-x86_64-pc-windows-gnu"
$env:PATH = "$env:LOCALAPPDATA\aura-toolchain\mingw64\bin;$env:PATH"
cargo test --workspace --all-targets
```

`RUSTUP_TOOLCHAIN` takes precedence over `rust-toolchain.toml`, so the pin stays
correct for everyone else and nothing in the repository changes. This is a local
convenience only: the MSVC build is the one that ships, and any result that
differs between the two toolchains is a determinism bug to be investigated, not a
toolchain preference.

### 8. `idx_task_claimable` is not a partial index

The blueprint declares it as `... WHERE state IN ('ready','pending')`. SQLite's
partial-index prover cannot establish that a query term `state = 'ready'`
satisfies that `IN` predicate, so the scheduler's hottest query planned as
`SCAN task`. The index is unconditional in `0001_init.sql`, the reason is a
comment on the statement, and `hot_queries_use_indexes_not_table_scans` asserts
the plan rather than trusting it.

### 9. `photo_file.last_seen_import` decides absence

The blueprint marks unseen files absent by comparing `last_seen_at` against the
run's timestamp. That is wrong whenever the clock is frozen, corrected backwards,
or shared between two runs in the same second: an entire card can be marked
missing, or a genuinely missing file can stay present. The column
`last_seen_import` records which import last saw each file, and absence means
"the current import did not see it" - a fact, not a time comparison.

## Consequences

- `contracts.lock` covers `crates/*/src/contract/**`, the migration and
  `ui/src/ipc/types.ts`. Changing any of them without an ADR fails CI.
- A future contributor reading the blueprint PDFs and the repository side by side
  will find every difference explained here rather than having to guess which one
  is authoritative.
