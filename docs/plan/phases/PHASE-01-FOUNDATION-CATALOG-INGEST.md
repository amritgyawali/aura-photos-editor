# Phase 01 - Project Foundation, Catalog & Wedding Project Ingest

> **Single feature shipped by this phase:** Create a wedding project, point it at 1-6 folders of RAWs from multiple cameras, and get a fully indexed, deduplicated, timeline-ordered catalog with a scrollable grid.
>
> **Mission:** Build the skeleton that every other phase bolts onto: monorepo, typed IPC, SQLite catalog, ingest walker, EXIF normalisation, multi-camera clock alignment and a virtualised grid that stays at 60 fps with 4,000 images.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 01 of 30 |
| Epic | E1 - Foundation |
| Feature | Create a wedding project, point it at 1-6 folders of RAWs from multiple cameras, and get a fully indexed, deduplicated, timeline-ordered catalog with a scrollable grid. |
| Depends on | Nothing |
| Unlocks | All phases |
| Duration | 2 weeks |
| Primary owners | Chief Architect / CTO Agent, Tech Lead - Imaging Core (Rust), Senior Engineer - Core Pipeline (Rust), Senior Frontend Engineer (Tauri + React), DevOps / Release Engineer |
| Risk level | Medium - foundational mistakes are expensive later |
| Headline KPI | 4,000 files indexed in < 90 s from a fast SSD; grid scroll >= 60 fps; catalog open < 400 ms |
| Competitor being beaten | Lightroom import speed and Narrative Select project setup |

## 1. Why this phase exists

Every competitor's real bottleneck is the boring part: getting thousands of files from several cards, several cameras and two photographers into one coherent, queryable timeline. If ingest is slow or lossy, no amount of AI later can save the product.

A wedding is a *story ordered in time*. That ordering is created here, not later: cameras with drifting clocks must be aligned, second-shooter files must be interleaved correctly, and the capture sequence must survive filename chaos (`IMG_0001` from two bodies).

This phase also fixes the shape of the whole codebase: crate boundaries, the typed Rust<->TypeScript bridge, migration strategy, error taxonomy, logging, CI and the fixture weddings that every later phase tests against.

## 2. Scope contract

### 2.1 In scope

- Monorepo scaffold: Cargo workspace of crates + Tauri desktop app + React/TypeScript UI + Python `ml/` tree + `docs/`.
- `aura-catalog`: SQLite (WAL) schema, versioned migrations, prepared-statement layer, transactional batch insert.
- Ingest walker: recursive scan, extension allow-list, symlink safety, network-volume detection, per-file xxhash3 content hash, resumable ingest journal.
- EXIF/maker-note extraction: camera body + serial, lens, focal length, ISO, shutter, aperture, flash fired, orientation, capture timestamp with sub-second field, GPS.
- Multi-camera clock alignment: cluster by body serial, estimate per-body offset from overlapping bursts, expose a manual nudge, write `timeline_ts` separate from `exif_ts`.
- Sidecar and duplicate-file handling: RAW+JPEG pairs, `.xmp` discovery, already-imported detection by hash, missing-file relink.
- Virtualised grid UI: windowed rendering, keyboard navigation, filmstrip, project switcher, empty/loading/error states.
- Typed IPC layer: one Rust command registry, generated TypeScript types, cancellable long-running commands with progress events.
- Reference fixtures: three anonymised mini-weddings (150 images each) committed via Git LFS for CI.

### 2.2 Explicitly out of scope (do not build it here)

- RAW decoding and preview generation (Phase 02).
- Any AI model, embedding or score (Phases 05+).
- Editing, recipes or rendering (Phase 14+).
- Cloud sync, licensing, telemetry upload (Phase 30).
- Pretty visual design polish - functional, accessible UI only; UX system lands progressively.

## 3. Architecture and data flow

```text
  Card / folder(s)              aura-core (domain types)
        |                              ^
        v                              |
  IngestWalker  --file events-->  IngestPipeline  --batch-->  aura-catalog (SQLite WAL)
        |                              |                             |
   xxhash3 + stat              ExifExtractor (exiv2 FFI)        migrations/*.sql
        |                              |                             |
        +-----> IngestJournal <--------+                             v
                (resume, dedupe)                            ClockAligner -> timeline_ts
                                                                     |
                                                                     v
  React UI  <--typed IPC events (progress, rows)--  aura-ipc  <--  query API
```

- Ingest is a bounded producer/consumer: one walker thread, N hashing/EXIF workers (N = cores-2), one writer thread owning the SQLite connection. Never write to SQLite from multiple threads.
- Every batch insert is one transaction of <= 500 rows so the UI sees rows appear continuously and a crash loses at most one batch.
- `timeline_ts` is the single source of truth for ordering everywhere else in the product; `exif_ts` is preserved untouched for forensics.

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `Cargo.toml` (workspace) | Declares all crates, shared dependency versions, release profile with LTO. |
| `crates/aura-core/src/{lib,ids,image,project,error,progress}.rs` | Domain types: `ImageId`, `ProjectId`, `CaptureMeta`, `AuraError`, progress/cancel primitives. |
| `crates/aura-catalog/src/{lib,schema,migrate,images,projects,query,tx}.rs` | SQLite access layer, migrations, typed row structs, batch writer. |
| `crates/aura-catalog/migrations/0001_init.sql` | Initial schema: projects, images, cameras, ingest_journal, settings. |
| `crates/aura-ingest/src/{lib,walker,hash,exif,clock,journal,pipeline}.rs` | Scan, hash, EXIF, clock alignment, resumable journal, orchestration. |
| `crates/aura-ipc/src/{lib,commands,events,types}.rs` | Tauri command registry, event bus, `ts-rs` type export. |
| `apps/desktop/src-tauri/src/main.rs` | App bootstrap, state, single-instance guard, panic hook, logging. |
| `apps/desktop/src/{app,routes,state}/*` | React shell, router, Zustand store, IPC client with generated types. |
| `apps/desktop/src/components/grid/{VirtualGrid,Cell,Filmstrip}.tsx` | Windowed grid, selection model, keyboard navigation. |
| `tests/fixtures/weddings/{hindu_night,daylight_church,nepali_reception}/` | Reference mini-weddings + expected-catalog JSON snapshots. |
| `justfile` | `just dev`, `just test`, `just bench`, `just phase-01-verify`. |
| `.github/workflows/ci.yml` | Matrix CI: Windows/macOS/Linux, fmt, clippy, tests, fixture ingest benchmark. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Catalog core schema (migration 0001)**

```sql
CREATE TABLE projects (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  couple_names TEXT,
  event_date   TEXT,
  created_at   TEXT NOT NULL,
  schema_ver   INTEGER NOT NULL,
  settings_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE cameras (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  make          TEXT, model TEXT, body_serial TEXT,
  shooter_label TEXT,              -- 'primary' | 'second' | free text
  clock_offset_ms INTEGER NOT NULL DEFAULT 0,
  UNIQUE(project_id, body_serial)
);

CREATE TABLE images (
  id           TEXT PRIMARY KEY,
  project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  camera_id    TEXT REFERENCES cameras(id),
  abs_path     TEXT NOT NULL,
  rel_path     TEXT NOT NULL,
  file_name    TEXT NOT NULL,
  ext          TEXT NOT NULL,
  bytes        INTEGER NOT NULL,
  content_hash TEXT NOT NULL,          -- xxhash3-128 hex
  exif_ts      TEXT,                   -- as recorded
  timeline_ts  TEXT,                   -- clock-aligned, authoritative order
  sub_sec      INTEGER,
  iso          INTEGER, shutter REAL, aperture REAL, focal_len REAL,
  lens         TEXT, flash_fired INTEGER, orientation INTEGER,
  width INTEGER, height INTEGER,
  gps_lat REAL, gps_lon REAL,
  status       TEXT NOT NULL DEFAULT 'indexed', -- indexed|missing|error
  error_text   TEXT,
  created_at   TEXT NOT NULL,
  UNIQUE(project_id, content_hash)
);
CREATE INDEX idx_images_timeline ON images(project_id, timeline_ts);
CREATE INDEX idx_images_camera   ON images(project_id, camera_id, timeline_ts);

CREATE TABLE ingest_journal (
  project_id TEXT NOT NULL, abs_path TEXT NOT NULL,
  phase TEXT NOT NULL,          -- discovered|hashed|exif|inserted|failed
  updated_at TEXT NOT NULL,
  PRIMARY KEY(project_id, abs_path)
);
```

**Ingest API (frozen)**

```rust
pub struct IngestOptions {
    pub roots: Vec<PathBuf>,
    pub include_jpeg_pairs: bool,
    pub follow_symlinks: bool,
    pub worker_threads: Option<usize>,
}

#[derive(Clone, serde::Serialize)]
pub enum IngestEvent {
    Discovered { total_hint: u64 },
    Progress { done: u64, total: u64, current: String },
    Batch { rows: Vec<ImageRowLite> },
    Warning { path: String, message: String },
    Finished { inserted: u64, skipped: u64, failed: u64, elapsed_ms: u64 },
}

pub trait Ingestor: Send + Sync {
    fn ingest(
        &self,
        project: ProjectId,
        opts: IngestOptions,
        cancel: CancelToken,
        sink: &dyn Fn(IngestEvent),
    ) -> Result<IngestSummary, AuraError>;
}
```

**Typed IPC surface generated for the UI**

```typescript
export type ImageRowLite = {
  id: string; fileName: string; timelineTs: string | null;
  cameraId: string | null; width: number; height: number; status: 'indexed' | 'missing' | 'error';
};

export const api = {
  createProject: (input: { name: string; coupleNames?: string; eventDate?: string }) => invoke<{ id: string }>('create_project', input),
  startIngest:   (input: { projectId: string; roots: string[] }) => invoke<{ jobId: string }>('start_ingest', input),
  cancelJob:     (input: { jobId: string }) => invoke<void>('cancel_job', input),
  listImages:    (input: { projectId: string; offset: number; limit: number; orderBy?: 'timeline' | 'filename' }) => invoke<ImageRowLite[]>('list_images', input),
  setCameraLabel:(input: { cameraId: string; shooterLabel: string; clockOffsetMs: number }) => invoke<void>('set_camera_label', input),
};
```

## 6. Algorithm, model and implementation design

### 6.1 Ingest pipeline mechanics

- Walk with `jwalk` (parallel directory iteration) and stream paths into a bounded channel of capacity 1,024 to keep memory flat.
- Hash with xxhash3-128 over the first 1 MiB + last 1 MiB + file size for speed; upgrade to full-file hash lazily when a collision is detected.
- Extract EXIF using an `exiv2`/`rexiv2` FFI wrapper; never decode the image here. Target < 8 ms per file.
- Insert in transactions of 500 rows; emit a `Batch` event so the grid grows visibly during import.
- Journal every path transition so a crash or cancel resumes exactly where it stopped.

### 6.2 Multi-camera clock alignment (a real competitive detail)

- Group images by `body_serial`; the body with the most frames becomes the reference clock.
- For every other body, build a histogram of nearest-neighbour timestamp deltas against the reference within +/- 30 minutes; the mode of that histogram is the coarse offset.
- Refine with least-squares on matched high-activity windows (ceremony/dance) where both bodies fire densely; store `clock_offset_ms` on the camera row.
- Expose the offset in the UI with a two-frame comparison so the photographer can nudge it; recomputing `timeline_ts` is a single UPDATE and must take < 1 s for 4,000 rows.
- Guard: if confidence in the estimated offset is < 0.6, keep offset 0, flag the camera, and tell the user which two frames to compare.

### 6.3 Grid performance strategy

- Virtualise on both axes; render only visible cells plus two rows of overscan.
- Cells subscribe to a thumbnail store keyed by `ImageId`; in this phase the store returns placeholders, Phase 02 fills real pixels without any UI change.
- Selection is a roaring-bitmap-style index, not an array, so select-all on 4,000 images is instant.
- All IPC list calls are paginated and cancelled on scroll direction change.

### 6.4 Error taxonomy and safety

- `AuraError` variants: `Io`, `UnsupportedFormat`, `CorruptMetadata`, `Catalog`, `Cancelled`, `Internal` - each with a user-facing message and a machine code.
- Files that fail EXIF still get indexed with `status='error'` and the reason: never silently drop a photographer's file.
- Read-only file opens everywhere; a unit test asserts no write handle is ever requested on a fixture RAW.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Create the workspace, crates, Tauri app, `justfile`, CI, and a `docs/adr/ADR-0001-architecture.md` recording the stack choice.
2. Write `aura-core` domain types and the error taxonomy; no logic yet.
3. Write migration `0001_init.sql` plus the migration runner and a round-trip test on a temp DB.
4. Implement the walker and hashing with a benchmark on 4,000 synthetic files.
5. Implement EXIF extraction; snapshot-test against fixture files from Canon CR3, Sony ARW, Nikon NEF, Fuji RAF and DNG.
6. Implement the batch writer and the ingest journal; add kill/resume tests.
7. Implement clock alignment with unit tests using synthetic offsets of -90 s, +12 min and +7 h.
8. Expose IPC commands, generate TypeScript types, wire the React shell and virtual grid.
9. Add the three reference fixture weddings and the expected-catalog snapshots.
10. Run `just phase-01-verify`, record the benchmark table, write the exit report.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `CTO` | Chief Architect / CTO Agent | Write ADR-0001 (Tauri + Rust core + ONNX Runtime + SQLite) and freeze crate boundaries | ADR + crate map | 6 h |
| `PM` | Product Manager Agent | Write the ingest user stories and the 'import a two-shooter wedding' acceptance script | Story set + QA script | 5 h |
| `EM` | Engineering Manager / Delivery Lead Agent | Break this phase into tracker tasks, set WIP limits, schedule the design review | Sprint board | 4 h |
| `TLC` | Tech Lead - Imaging Core (Rust) | Design `aura-core` types, `AuraError`, progress/cancel primitives; review every PR in this phase | Frozen core API | 2 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement walker, hashing, EXIF FFI, journal, batch writer, clock aligner | `aura-ingest` + tests | 6 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Implement catalog schema, migrations and query API | `aura-catalog` + tests | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Build the Tauri shell, typed IPC client, virtual grid, filmstrip and project switcher | Working app shell at 60 fps | 5 d |
| `MFE` | Mid-Level Frontend Engineer | Build project-create wizard, folder picker, camera/shooter labelling panel, progress UI | UI panels + component tests | 3 d |
| `UX` | UX / UI Designer | Flow for 'new wedding -> pick folders -> label shooters -> grid', plus loading/error states | Wireframes + states | 2 d |
| `DATA` | Data Engineer / Dataset Curator | Assemble and anonymise the three reference fixture weddings, document licence/consent | Fixture set in Git LFS | 3 d |
| `QAL` | QA Lead - Automation | Build the test harness, fixture runner, catalog snapshot diffing, kill/resume tests | CI suite green | 4 d |
| `PERF` | Performance Engineer | Benchmark ingest throughput and grid frame time; publish the budget dashboard | Benchmark report | 2 d |
| `DEVOPS` | DevOps / Release Engineer | CI matrix, Git LFS, caching, artefact upload, crash-log capture | `ci.yml` green on 3 OSes | 3 d |
| `SEC` | Security & Privacy Engineer | Threat model v1; enforce read-only opens; decide where the DB and caches live per OS | Threat model doc | 2 d |
| `DOC` | Technical Writer | Write `docs/runbooks/ingest.md` and the module READMEs | Docs merged | 1 d |

### 9.1 Handoff chain for this phase

```text
CTO/ADR -> TLC (core types frozen)
            |
            +-> SRC (ingest + catalog) --------+
            +-> SFE/MFE (shell + grid, stubs)  +--> QAL (fixtures, snapshots) -> PERF (benchmarks)
            +-> DATA (fixture weddings) -------+                                   |
                                                                                   v
                                                                       EM/PM/CTO phase gate
```

### How this agent team runs a phase (identical every time)

1. **Kickoff (PM + CTO + EM).** PM restates the feature as user stories, CTO writes/updates the ADR, EM cuts the task list from section 9 into the tracker.
2. **Design review (CTO + TLC + MLL + COL + UX).** Interfaces from section 5 are frozen before code. Any change after freeze needs an ADR amendment.
3. **Build in parallel lanes.** Core lane (TLC/SRC/SRG), ML lane (MLL/SRML/MLR/MLOPS), agent lane (AGT), UI lane (SFE/MFE/UX), data lane (DATA), platform lane (DEVOPS/SEC).
4. **Contract-first handoff.** A lane may only consume another lane's work through the frozen interface, using a stub/fixture until the real implementation lands.
5. **Code review chain.** Author -> peer in same lane -> lane lead -> CTO for anything touching an invariant. Two approvals minimum, one must be a lead.
6. **QA gate (QAL + QAIQ + PERF).** Unit + integration + golden-image + perceptual + performance suites must be green on the reference weddings.
7. **Phase gate (CTO + PM + EM).** All acceptance criteria in section 13 pass, telemetry is live, docs updated, demo recorded. Only then does the next phase start.
8. **Escalation.** Any blocker older than one working day goes to EM; any invariant conflict goes to CTO; any "we should ship it slightly broken" goes to PM and is written down.

### Branch, commit and PR rules

- Branch: `feat/phase-NN-<slug>`; one PR per task group, never one giant PR per phase.
- Conventional Commits (`feat(core): ...`, `fix(ml): ...`, `perf(render): ...`, `test(qa): ...`, `docs: ...`).
- Every PR states: what changed, which acceptance criterion it advances, benchmark delta, and screenshots or golden-image diffs when pixels change.
- CI must be green: `fmt`, `clippy -D warnings`, `cargo test`, `pytest`, `vitest`, golden-image diff, benchmark regression guard (<= 5 % slower), model-hash check.


## 10. Test plan

### 10.1 Phase-specific tests

- Ingest 4,000 mixed-format files: exact row count, zero duplicates, zero silent drops.
- Re-run ingest on the same folders: 100 % skipped by hash, < 5 s.
- Kill the process mid-ingest at 10/50/90 %: resume completes with an identical final catalog snapshot.
- Clock alignment: synthetic offsets recovered within +/- 500 ms; low-confidence case correctly refuses to guess.
- Corrupt/truncated RAW, zero-byte file, unicode and emoji filenames, 300-character paths, files on a slow network share.
- Grid: scroll 4,000 items with frame time <= 16.6 ms p95; select-all < 50 ms.
- Migration test: 0001 applied to an empty DB and to a v0 DB, then rolled back.

### 10.2 Standing test matrix (applies to every phase)

| Layer | What it proves |
|---|---|
| Unit | Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy. |
| Property/fuzz | Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects. |
| Golden image | Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed. |
| Perceptual (human) | QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required. |
| Performance | Throughput, wall clock, peak RAM, peak VRAM on the three reference machines. |
| Resume/kill | Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption. |
| Regression | Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress. |

Reference machines: RTX 4070 laptop (Win 11, 32 GB), M3 Pro MacBook (18 GB), Intel iGPU desktop (Win 11, 16 GB, DirectML fallback).

## 11. Performance budget and telemetry

| Metric | Budget |
|---|---|
| Index 4,000 RAWs (NVMe) | <= 90 s wall clock |
| Per-file EXIF + hash | <= 20 ms average |
| Catalog open (existing 4,000-image project) | <= 400 ms |
| Grid frame time p95 | <= 16.6 ms |
| Peak RAM during ingest | <= 800 MB |
| SQLite file size for 4,000 images | <= 12 MB |

Telemetry events (local-first, opt-in aggregation):

- `ingest.started` {file_count_hint, root_count, volume_type}
- `ingest.finished` {inserted, skipped, failed, elapsed_ms, files_per_sec}
- `ingest.file_failed` {reason_code, ext}
- `clock.aligned` {camera_count, offsets_ms, confidence}
- `ui.grid_fps` {p50, p95, item_count}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| EXIF library gaps on new camera bodies | Vendor-neutral maker-note fallback + a `unknown_camera` telemetry event and monthly camera-support sprint. |
| SQLite write contention stalls the UI | Single writer thread, WAL mode, 500-row transactions, all reads on pooled read-only connections. |
| Network volumes murder throughput | Detect volume type, reduce worker count, warn the user, offer 'copy locally first'. |
| Clock alignment guesses wrong and scrambles the story | Confidence gate + manual nudge + `exif_ts` preserved so the operation is always reversible. |
| Foundation churn slows later phases | Interfaces frozen in section 5 and changed only via ADR amendment. |

## 13. Acceptance criteria

- [ ] A photographer can create a wedding project, add three folders from two bodies, and see a correctly time-ordered grid.
- [ ] 4,000 files index in <= 90 s on the reference NVMe machine with peak RAM <= 800 MB.
- [ ] Re-import is a no-op; moved files relink by hash without duplicating rows.
- [ ] Second-shooter frames interleave correctly after automatic clock alignment; the offset is visible and editable.
- [ ] Killing the app at any point never corrupts the catalog and always resumes.
- [ ] Every failed file is visible in a 'Problems' list with a plain-language reason.
- [ ] Generated TypeScript types match the Rust commands; a type mismatch fails CI.
- [ ] `just phase-01-verify` runs ingest on all three fixture weddings and diffs catalog snapshots to zero.

## 14. Definition of Done (phase gate)

- [ ] All acceptance criteria in section 13 verified by QA on the three reference weddings (indoor Hindu night ceremony, outdoor daylight Christian wedding, mixed-light Nepali reception).
- [ ] Unit, integration, golden-image, perceptual and performance suites green in CI on Windows (NVIDIA), Windows (integrated/DirectML) and macOS (Apple Silicon).
- [ ] Performance budget in section 11 met or a signed waiver from PERF + CTO recorded in the ADR.
- [ ] Telemetry events from section 11 visible in the local metrics dashboard and in the opt-in aggregate pipeline.
- [ ] Every new AI decision surface returns `confidence` + `reasons[]` and is rendered in the Explain panel.
- [ ] Docs updated: module README, model card (if a model shipped), in-app help string, CHANGELOG entry.
- [ ] Rollback path exists: feature flag off, previous model version pinnable, catalog migration reversible.
- [ ] Demo recording of the feature running on a real 3,000-image wedding attached to the phase gate.

Inherited invariants that this phase must not break:

- **Never mutate a RAW file.** Every decision is a row in SQLite plus a JSON edit recipe. Originals are opened read-only.
- **Every AI decision carries `confidence` (0-1) and `reasons[]`.** A decision without an explanation is a bug.
- **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on 2048 px proxies, expensive work only on survivors.
- **Determinism.** Same inputs + same model versions + same seed = byte-identical recipe JSON. All models are pinned by hash.
- **Resumability.** Any job can be killed at any moment and resumed without recomputing finished work.
- **Local-first.** The product must complete a full wedding with no network. Cloud AI is an accelerator, never a dependency.
- **Scene-conditioned everything.** No threshold is global; every threshold is a function of the detected scene and subject role.
- **Colour discipline.** Work in linear scene-referred space, convert once, and never let a grade move skin outside its guarded region.
- **No silent failure.** Every module emits a typed error, a fallback path and a telemetry event.

## 15. Claude Code execution prompt (copy-paste this)

```text
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 01 - Project Foundation, Catalog & Wedding Project Ingest.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-01-FOUNDATION-CATALOG-INGEST.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Create a wedding project, point it at 1-6 folders of RAWs from multiple cameras, and get a fully indexed, deduplicated, timeline-ordered catalog with a scrollable grid.

Rules:
  - Do not start Phase 2. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `Cargo.toml` (workspace), `crates/aura-core/src/{lib,ids,image,project,error,progress}.rs`, `crates/aura-catalog/src/{lib,schema,migrate,images,projects,query,tx}.rs`, `crates/aura-catalog/migrations/0001_init.sql`, `crates/aura-ingest/src/{lib,walker,hash,exif,clock,journal,pipeline}.rs`, `crates/aura-ipc/src/{lib,commands,events,types}.rs`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-01-foundation-catalog-ingest and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-01.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-01-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-01-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-01-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 01 of 30 - Project Foundation, Catalog & Wedding Project Ingest - part of the AURA Wedding AI master build plan.*
