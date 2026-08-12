# Phase 01 - exit report

**Status: green on this machine, with two caveats recorded in section 4.**
Every gate runs and passes locally; CI has not yet run the same lanes on macOS
and Linux, and the throughput figure below was measured on synthetic small files
rather than real RAWs.

Measured on: Windows 11, Rust 1.97.1, host toolchain `x86_64-pc-windows-gnu`
(see ADR-0002 section 7), 2026-08-12.

## 1. What shipped

The single feature of this phase: create a wedding project, point it at one to six
folders of RAWs from several cameras, and get a fully indexed, deduplicated,
timeline-ordered catalog with a scrollable grid.

- Cargo workspace of seven crates plus `xtask`, with the lint block, the
  banned-pattern gate, the supply-chain policy and the contract digest checker.
- `aura-core`: one error type, a registry of 20 codes each with a runbook, typed
  ids, a clock that can be frozen and moved backwards, cloud-sync refusal, path
  safety, redaction, consent (denied by default) and cancel/progress primitives.
- `aura-catalog`: schema v1 in `STRICT` tables, forward-only migrations with a
  verified backup, an advisory instance lock, one writer thread, a read pool and
  the six-step refusal chain.
- `aura-ingest`: deterministic scan, full-content BLAKE3, RAW/JPEG/XMP pairing,
  defensive EXIF, a crash-safe journal, quarantine instead of silent drops, and
  multi-camera clock alignment with a confidence gate.
- `aura-jobs`: derived task ids, dependency gating, leases with heartbeats,
  bounded retries ending in quarantine, run cancellation.
- `aura-app` + `ui`: one typed IPC surface, generated TypeScript kept honest by a
  test, and a virtualised grid with keyboard navigation.
- `aura-cli`: fixture generation, import, `verify` (the phase gate) and `info`.
- CI with five lanes, `perf/budgets.toml` as data, and runbooks for every code.

## 2. Gate results

| Gate | Command | Result |
|---|---|---|
| Build | `cargo build --workspace --all-targets` | pass, 0 warnings |
| Tests | `cargo test --workspace --all-targets` | **73 passed, 0 failed** |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| Format | `cargo fmt --all -- --check` | pass |
| Banned patterns | `scripts/check-banned.sh` | `check-banned: clean` |
| Frozen contracts | `cargo xtask contracts --check` | `contracts: 7 entries, all locked` |
| UI types | `npm run lint` (tsc strict) | pass |
| UI tests | `npm test` | **7 passed, 0 failed** |
| Phase gate | `cargo run --release -p aura-cli -- verify` | `phase-01 verify: all fixtures clean` |

Test counts by suite: `aura-core` 25 (error registry 3, ids 5, consent 5, clock
and paths 7, progress 4, crate-dependency guard 1), `aura-catalog` 11,
`aura-ingest` 18, `aura-jobs` 7, `aura-perf` 6, `aura-app` 6.

## 3. Acceptance criteria (section 13 of the phase document)

| Criterion | Proof | Result |
|---|---|---|
| Project, several folders, two bodies, correctly ordered grid | `importing_two_bodies_aligns_the_timeline` | pass |
| 4,000 files indexed inside the budget | measured below | pass, with the caveat in section 4 |
| Re-import is a no-op; moved files relink by hash | `second_import_of_identical_folder_inserts_nothing`, `renaming_a_file_does_not_duplicate_the_photo` | pass |
| Second-shooter frames interleave; offset visible and editable | `clock_alignment_recovers_synthetic_offsets`, `set_camera_label` | pass |
| Killing the app never corrupts the catalog and always resumes | `cancel_keeps_completed_work_and_marks_the_run_cancelled`, `the_journal_records_every_path_it_finished` | pass |
| Every failed file visible with a plain-language reason | `zero_byte_files_are_quarantined_and_the_import_completes`, `ProblemsPanel` | pass |
| Generated TypeScript matches the Rust commands | `crates/aura-app/tests/ipc_contract.rs` | pass |
| `just phase-01-verify` diffs catalog snapshots to zero | `aura-cli verify` output below | pass |
| Grid holds 4,000 items without rendering 4,000 cells | `VirtualGrid.test.tsx` | pass |
| Select-all on 4,000 items under 50 ms | `store.test.ts` | pass |

### Phase gate output

```
hindu_night:      photos=150 files=240 cameras=2
  first : imported=240 photos=150
  second: imported=0 hashed_bytes=0
  digest: 57c6efd0e9f3c00ef542536067840e7d72cc6435f08ce4c294ca7408573331f8
daylight_church:  photos=150 files=200 cameras=2
  first : imported=200 photos=150
  second: imported=0 hashed_bytes=0
  digest: b8da0b3a89f8dc6a8e481596b36d9715a1577a86be33f95aa265489dad0991d0
nepali_reception: photos=150 files=150 cameras=2
  first : imported=150 photos=150
  second: imported=0 hashed_bytes=0
  digest: 6a85b943cfd78773106dbbd94d9bf791e3c145179164e82d290017b7c6763ff9
phase-01 verify: all fixtures clean
```

The `hindu_night` numbers are the pairing proof: 240 files become 150
photographs, because one body writes an XMP sidecar beside every frame.

## 4. Performance

Measured with 4,000 distinct files across 20 folders, imported twice:

| Metric | Budget | Measured |
|---|---|---|
| Index 4,000 files | <= 90,000 ms | **3,268 ms** (0.82 ms per file) |
| Re-import 4,000 unchanged files | <= 5,000 ms | **98 ms**, 0 bytes hashed |
| Catalog open plus counts | <= 400 ms | **142 ms** |
| Catalog size for 4,000 images | <= 12 MB | **8.9 MB** |

**Caveat 1 - file size.** The 4,000 fixture files are a few hundred bytes each,
so this measures walk, EXIF, insert and journal cost, not hashing throughput. A
real wedding is roughly 220 GB, and BLAKE3 at 1 GB/s adds roughly 25 s of hashing
that this run does not include. The ingest benchmark
(`cargo bench -p aura-ingest`) measures hashing separately; a full-size run on the
reference NVMe machine still has to be recorded before the 90-second budget can be
called proven for a real wedding.

**Caveat 2 - one platform.** Everything above ran on Windows only, and on the GNU
host toolchain rather than MSVC, because this machine has no Windows SDK
(ADR-0002 section 7). The CI matrix (Windows, macOS, Linux, MSVC) has not been
executed yet.

## 5. Defects found and fixed while proving the gates

These are the bugs the gates caught. Each one is now covered by the test that
found it.

1. **The claimable-task index was never used.** `idx_task_claimable` was a partial
   index with `WHERE state IN ('ready','pending')`, and SQLite cannot prove that a
   query saying `state = 'ready'` satisfies that predicate, so the scheduler's
   hottest query planned as `SCAN task`. The index is now unconditional.
   Caught by `hot_queries_use_indexes_not_table_scans`.
2. **A frozen clock could skip an entire card.** The settle-window check treated a
   negative age (file stamped later than our clock) as "still being written", so a
   camera with a wrong date, or a test with a fixed clock, silently imported
   nothing. The window now only applies to a positive age inside it.
   Caught by `the_journal_records_every_path_it_finished`.
3. **Absence was decided by comparing timestamps.** `mark_absent_unseen` marked
   rows whose `last_seen_at` was older than the run's timestamp, which does the
   wrong thing on a frozen or corrected clock. `photo_file` now carries
   `last_seen_import`, and absence means "the current import did not see it".
   Caught by `unplugging_a_drive_marks_files_absent_rather_than_deleting_them`.
4. **Nearest-neighbour clock alignment aliased.** Two bodies shooting at a similar
   cadence made a one-hour offset look like a four-second offset, and any offset
   larger than the frame gap was invisible. The estimator now pairs every sampled
   frame against every sampled reference frame, so the true offset is the tallest
   peak whatever its size. Caught by `clock_alignment_recovers_synthetic_offsets`.
5. **The grid shadowed `window`.** A `useMemo` named `window` hid the global, so
   the resize fallback threw on mount in any environment without `ResizeObserver`.
   Caught by `VirtualGrid.test.tsx`.

## 6. Known issues and deliberate gaps

- Ingest events are logged rather than pushed to the UI event bus; `IngestEvent`
  is defined and typed on both sides, and the Tauri emitter is a small change in
  `crates/aura-app/src/commands.rs` once the shell runs.
- `ImportMode::CopyVerified` is accepted by the plan type but the importer only
  implements `Reference` in this phase.
- Embedded preview extraction (`extract_embedded_previews`) is a flag with no
  effect until phase 02 owns preview generation.
- Fixtures are generated JPEGs, not camera RAWs; RAW-specific EXIF quirks arrive
  with the decoder in phase 02.
- The desktop shell (`ui/src-tauri`) compiles against the same command surface but
  has not been launched on this machine; it is outside the workspace by design
  (ADR-0002 section 6).

## 7. Rollback

Phase 01 is the foundation, so rollback means reverting the branch. Within the
phase: the catalog migration is the only irreversible step, and it is preceded by
a verified backup in `<catalog>/backups/`. Deleting the catalog and re-importing
reconstructs every row from the source folders, because nothing in this phase is
derived from anything but the files themselves.

## 8. Gate decision

Phase 02 may start once the CI matrix has run these same lanes on all three
operating systems and a full-size (220 GB class) ingest has been timed on the
reference NVMe machine. Everything else on the phase-01 checklist is proven above.
