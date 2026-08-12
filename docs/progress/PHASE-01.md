# Phase 01 - task log

One line per task: what landed, which files, which tests, and what is still open.

| Task | Deliverable | Files | Tests added |
|---|---|---|---|
| T1 | Workspace scaffold, pinned toolchain, lint policy, banned-pattern gate, supply-chain policy, contract digests, git hooks | `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.gitignore`, `.gitattributes`, `scripts/check-banned.sh`, `.githooks/pre-commit`, `xtask/` | `xtask contracts --check` in CI; `no_upward_deps` |
| T2 | Error taxonomy: `AuraError`, `Severity`, `Recovery`, `ErrorCode`, the registry and its constructors | `crates/aura-core/src/contract/error.rs`, `src/errors/{io,db,job}.rs`, `errors.toml` | `tests/error_registry.rs` (well-formed, in-range, no leaked detail, every used code registered, runbook exists) |
| T3 | Typed ids, content hash, clock, path safety, redaction, consent, progress and cancellation | `crates/aura-core/src/contract/{ids,consent}.rs`, `src/{clock,paths,redact,progress}.rs` | `tests/ids.rs`, `tests/consent.rs`, `tests/clock_and_paths.rs`, `tests/progress.rs` |
| T4 | Catalog schema v1, frozen | `crates/aura-catalog/migrations/0001_init.sql` | `tests/catalog.rs::fresh_catalog_reaches_current_version_and_is_clean`, `strict_tables_reject_wrong_types` |
| T5 | Pragmas, advisory instance lock, single-writer actor, read pool | `crates/aura-catalog/src/{db,guard,writer,lib}.rs` | `second_instance_is_refused_with_3005`, `lock_is_released_when_the_first_catalog_is_dropped`, `writer_is_single_threaded_under_contention` |
| T6 | Forward-only migrations, verified backups, the six-step refusal chain | `crates/aura-catalog/src/{migrate,backup}.rs` | `newer_schema_is_refused_not_downgraded`, `corrupt_catalog_is_detected_before_any_read`, `cloud_synced_path_is_refused_with_1008`, `hot_queries_use_indexes_not_table_scans` |
| T7 | Deterministic scan with skip list, settle window and quarantine | `crates/aura-ingest/src/{scan,util}.rs` | `scan_order_is_stable_across_runs`, `unknown_extensions_are_ignored_not_guessed`, `zero_byte_files_are_quarantined_and_the_import_completes` |
| T8 | Full-content BLAKE3 hashing with a storage-aware thread pool | `crates/aura-ingest/src/{hash,tuning}.rs` | covered by the idempotence and quarantine tests; benched in `benches/ingest_throughput.rs` |
| T9 | Pairing and EXIF extraction | `crates/aura-ingest/src/{pair,exif}.rs` | `raw_and_jpeg_pair_become_one_photo`, `same_stem_in_different_folders_are_two_photos`, `adobe_double_extension_sidecars_group_with_their_raw`, `a_file_with_no_exif_still_imports`, `exif_datetime_parsing_handles_offsets_and_junk` |
| T10 | Idempotent import, journal, absent marking, multi-camera clock alignment | `crates/aura-ingest/src/{import,clock_align}.rs`, `crates/aura-catalog/src/repo.rs` | `second_import_of_identical_folder_inserts_nothing`, `renaming_a_file_does_not_duplicate_the_photo`, `clock_alignment_recovers_synthetic_offsets`, `clock_alignment_refuses_to_guess_when_evidence_disagrees`, `importing_two_bodies_aligns_the_timeline`, `unplugging_a_drive_marks_files_absent_rather_than_deleting_them` |
| T11 | Job graph: derived task ids, dependency gate, retry budget, cancellation | `crates/aura-jobs/src/graph.rs` | `planning_is_idempotent`, `dependencies_gate_readiness`, `retries_are_bounded_and_end_in_quarantine`, `cancelling_a_run_stops_every_unfinished_task` |
| T12 | Leases and heartbeats | `crates/aura-jobs/src/lease.rs` | `two_workers_cannot_claim_the_same_task`, `an_expired_lease_is_reclaimed_and_the_work_is_not_lost`, `a_live_worker_keeps_its_lease` |
| T13 | Tauri shell: window, panic hook, logging, command registry | `ui/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,src/main.rs}` | built out of the workspace, see ADR-0002 |
| T14 | Typed IPC v1 and the generated TypeScript | `crates/aura-app/src/{contract/ipc.rs,commands.rs,state.rs}`, `ui/src/ipc/{types.ts,client.ts}` | `crates/aura-app/tests/ipc_contract.rs` (every field and every event variant declared in TypeScript) |
| T15 | React shell: project switcher, import wizard, virtualised grid, filmstrip, problems panel | `ui/src/**` | `ui/src/state/store.test.ts`, `ui/src/components/grid/VirtualGrid.test.tsx` |
| T16 | Fixture generator: three reference weddings with real EXIF, two bodies each, deliberate clock drift | `crates/aura-ingest/src/fixtures.rs` | `generated_fixtures_carry_readable_exif` |
| T17 | Full test suite and the phase-gate driver | `crates/aura-cli/src/main.rs` (`verify`) | `just phase-01-verify` |
| T18 | CI lanes, local gates, budget instrumentation | `.github/workflows/ci.yml`, `justfile`, `perf/budgets.toml`, `crates/aura-perf/**` | `crates/aura-perf/tests/budgets.rs` |
| T19 | Runbooks: one page per error code, plus the ingest runbook | `docs/runbooks/**` | asserted by `error_registry::every_registered_code_is_well_formed_and_has_a_runbook` |
| T20 | Evidence pack and phase gate | `docs/progress/PHASE-01-EXIT.md` | see that file |

## Defects the gates caught

Listed in full, with the test that found each one, in
[PHASE-01-EXIT.md](PHASE-01-EXIT.md) section 5:

1. `idx_task_claimable` was partial and therefore unused - the scheduler's hottest
   query scanned the table.
2. The settle window skipped every file whose mtime was ahead of our clock.
3. Absence was decided by a timestamp comparison instead of by import identity.
4. Nearest-neighbour clock alignment aliased on similar cadences and could not see
   an offset larger than the frame gap.
5. A `useMemo` named `window` shadowed the global and broke the grid's resize
   fallback.

## Recorded decisions

- [ADR-0001](../adr/ADR-0001-architecture.md) - stack and crate boundaries.
- [ADR-0002](../adr/ADR-0002-toolchain-and-layout.md) - toolchain pin, migration
  path, `schema_version` insert, `unicode-normalization`, DB code numbering, and
  why the desktop shell is not a workspace member.
