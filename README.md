# AURA Wedding AI

> Shoot the wedding. Import the RAWs. Click once. Deliver.

The autonomous AI post-production system for wedding photographers. Import 1,000
to 4,000 RAW files, press one button, and receive a culled, edited, retouched,
quality-checked, consistently graded, export-ready wedding gallery.

**Phase 01 of 30 is implemented here:** create a wedding project, point it at one
to six folders of RAWs from several cameras, and get a fully indexed,
deduplicated, timeline-ordered catalog with a scrollable grid.

## Layout

```
Cargo.toml                 workspace
crates/
  aura-core/               error taxonomy, typed ids, clock, paths, consent, progress
  aura-catalog/            SQLite schema v1, migrations, single writer, verified backups
  aura-ingest/             scan, hash, pair, EXIF, journal, clock alignment, fixtures
  aura-jobs/               task graph, dependencies, leases, heartbeats
  aura-perf/               budget instrumentation and assertions
  aura-app/                typed IPC command surface
  aura-cli/                headless driver: fixtures, import, verify, info
xtask/                     contract digests, fixture and bench entry points
ui/                        React 18 + TypeScript + Vite
ui/src-tauri/              desktop shell (not a workspace member, see ADR-0002)
docs/plan/                 the 30-phase master plan and agent briefs
docs/pdf/                  the blueprint pack the plan was delivered as
docs/adr/                  architecture decision records
docs/runbooks/             one page per error code
docs/progress/             per-phase task log and exit report
perf/budgets.toml          every budget from a phase document, as data
tests/fixtures/            generated reference weddings (not committed)
```

## Getting started

```bash
rustup show                       # installs the pinned toolchain from rust-toolchain.toml
just setup                        # git hooks + npm install
just gates                        # fmt, banned patterns, clippy, frozen contracts
just test                         # Rust + UI tests
just phase-01-verify              # the phase gate on the reference weddings
just dev                          # run the desktop app
```

On Windows the Rust MSVC target needs Visual Studio Build Tools with the
**Desktop development with C++** workload **and a Windows SDK**; without the SDK
`link.exe` cannot find `kernel32.lib` and nothing links. A machine that cannot
install the SDK can build and test locally with the GNU host toolchain instead -
see [ADR-0002 section 7](docs/adr/ADR-0002-toolchain-and-layout.md). MSVC is still
what ships.

## The nine invariants

1. Never mutate a RAW file. Every decision is a row in SQLite plus a JSON recipe.
2. Every AI decision carries `confidence` and `reasons[]`.
3. Three-tier compute: previews, then proxies, then survivors.
4. Determinism: same inputs and model versions produce byte-identical output.
5. Resumability: any job can be killed and resumed without recomputation.
6. Local-first: a full wedding completes with the network unplugged.
7. Scene-conditioned everything: no global thresholds.
8. Colour discipline: linear light, convert once, guard skin.
9. No silent failure: a typed error, a fallback and a telemetry event, always.

## What we will never build

Body reshaping, skin lightening, face or eye swapping, adding people or objects
that were not there, or any operation that changes what a person looks like.
Enforced in code and in CI, not just in documentation.
