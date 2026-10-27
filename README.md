# AURA Wedding AI

> Shoot the wedding. Import the RAWs. Click once. Deliver.

The autonomous AI post-production system for wedding photographers. Import 1,000
to 4,000 RAW files, press one button, and receive a culled, edited, retouched,
quality-checked, consistently graded, export-ready wedding gallery.

**All thirty phases are implemented here**, conditionally: import to delivery, one
button end to end, with every phase's exit report naming what it could not prove.

Read those before believing a number. The short version is that the shape of the
product is complete and the evidence underneath it is not: there are no camera
files, no GPU backend, no consented wedding data and no photographers in this
repository, so every model ships as a placeholder or as a refusal, every quality
gate is measured against a fixture whose answer this repository chose, and the
build says so on the wire rather than in a footnote.
`docs/progress/PHASE-30-EXIT.md` section 7 is the whole open list.

## Layout

```
Cargo.toml                 workspace
crates/                    thirty-three crates. The ones to start from:
  aura-core/               error taxonomy, typed ids, clock, and every frozen contract
  aura-catalog/            SQLite schema, thirty migrations, single writer, verified backups
  aura-ingest/             scan, hash, pair, EXIF, journal, clock alignment, fixtures
  aura-raw/                containers, three decode tiers, the colour pipeline
  aura-preview/            the preview service and its strict-priority scheduler
  aura-infer/              the one way to run a model; hardware probe and session pool
  aura-cloud/              the one way to reach a provider; seven-step gateway
  aura-index/  aura-vision/  aura-people/     similarity, pixels, faces, identities
  aura-brain-wedding/      scenes, story, moments, emotion, gallery consistency
  aura-brain-photo/        integrity, tone, colour, composition, local light
  aura-cull/  aura-curate/                    what is delivered, and what is shown
  aura-recipe/ aura-render/                   the edit recipe and the only renderer
  aura-retouch/ aura-restore/ aura-geometry/ aura-generative/
  aura-style/  aura-explain/ aura-qc/         learned look, the ledger, the QC agent
  aura-export/ aura-delivery/ aura-learn/     files, where they went, what it learned
  aura-jobs/               the autopilot: task graph, DAG, checkpoints, governor
  aura-perf/               budget instrumentation and assertions
  aura-app/                typed IPC command surface - 259 commands
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

On Windows, open **Start AURA.cmd** from this folder. It opens the native app
with the UI bundled inside, without a browser or development server. Add Desktop
and Start menu shortcuts with `& '.\Start AURA.cmd' -InstallShortcuts` in PowerShell.
The launcher builds automatically if the executable is missing or source files
have changed. Close AURA and open it again to pick up code changes; `-Rebuild`
forces a rebuild when needed. Building requires the tools listed below.

For the desktop import → automatic edit → export workflow, see
[Photo editing quickstart](docs/photo-editing-quickstart.md). Cloud vision editing
uses the configured provider; local enhancement works without an API key.

See [Photo Studio](docs/photo-studio.md) for automatic editing after import,
reviewing every edit, matching reference colors, and rendering the final photos.
See [Adjustable finishing and reference colour](docs/adjustable-finishing-and-reference-colour.md)
for post-edit strength sliders and the reusable edited-photo reference library.
The [colour-match test report](docs/reports/AURA-reference-colour-match-2026-10-22.md)
adds original-resolution Internet references, native RAW verification and 18
known-grade comparisons, including cases that need further manual adjustment.
The [export verification](docs/reports/AURA-exported-colour-verification-2026-10-22.md)
compares delivered JPEG/PNG files with saved renders and references, including a
full-resolution camera file and adjusted matching strengths.
The [adaptive matcher report](docs/reports/AURA-adaptive-colour-matching-2026-10-22.md)
covers signed search, tone-dependent casts, monochrome references and native checks,
with measured photographic results and remaining limits.
See [Editing panels](docs/editing-panels.md) for the editor sections and
[native research](docs/reports/AURA-Evoto-native-research-2026-10-20.md) for regional
reference matching, REA evidence and remaining qualification work.
The [fifty-capability continuation](docs/reports/AURA-fifty-capabilities-2026-10-20.md)
adds native Levels/selective colour, mask refinements, reference-library batch
management, framing and colour inspection, with a numbered implementation ledger.
The [2026-10-22 additions](docs/additional-editing-methods-2026-10-22.md) expand
colour, spatial repair and selection catalogues to 75 methods each and add
delivery, surface and lighting review screens with 70 inspection measurements.
See the [validation report](docs/reports/AURA-validation-2026-10-22.md) for
per-tool results, edited-photo exports and the remaining Evoto parity limits.
The [comprehensive desktop audit](docs/reports/AURA-comprehensive-test-2026-10-22.md)
records fresh tool/control results, all 27 supplied RAW edits, command coverage,
and the unchanged-colour-save history repair. Functional passes do not establish
exact Evoto equivalence.
The [advanced continuation](docs/advanced-editing-methods-2026-10-22.md) adds 100
further capabilities, native local-detail processing for supported spatial tools,
lower-memory colour stencils and three more review screens. See its
[validation report](docs/reports/AURA-advanced-validation-2026-10-22.md).
The app opens on three steps - pick one of 21 [edit profiles](docs/edit-profiles.md)
(16 researched looks and 5 learned from professional RAW before-and-afters), optionally
paste an Instagram profile, then upload photos or a folder.

```bash
rustup show                       # installs the pinned toolchain from rust-toolchain.toml
just setup                        # git hooks + npm install
just gates                        # fmt, banned patterns, clippy, frozen contracts
just test                         # Rust + UI tests
just phase-01-verify              # any phase's gate: phase-01-verify .. phase-30-verify
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
