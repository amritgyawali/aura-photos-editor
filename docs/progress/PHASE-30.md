# Phase 30 progress - Delivery, Integrations, Learning Loop & Release Engineering

One line per task, in the order section 9 asks for them.

| Task | Files touched | Tests added | Note |
|---|---|---|---|
| Step 0 - branch | - | - | `claude/phase-30-implementation-cys6p9`, cut and pushed before any code |
| CTO - ADR | `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` | - | Verification is a read-back and not a checksum of the buffer; the manifest is sealed once; a guarantee is not learnable; the provider ships without a socket and says so |
| CTO - ADR | `docs/adr/ADR-0062-delivery-ipc-surface.md` | - | Seventeen commands; no `export_destination` on the wire that automation could set, no strength field on an update, and `preview_names` so a photographer sees collisions before files exist |
| TLC - freeze | `crates/aura-core/src/contract/delivery.rs` | contract tests | `FileFormat`, `DeliveryColour`, `Resize`, `OutputSharpen`, `NamingTemplate`, `MetadataPolicy`, `Destination`, `ExportSet`, `ExportJob`, `ExportedFile`, `DeliveryManifest`, `UploadState`, `UploadItem`, 30 `DeliveryCode`s of which three stop a job, `ExportService`, `DeliveryService` |
| TLC - freeze | `crates/aura-core/src/contract/learn.rs` | contract tests | `Learnable` closed at 15 members with **no `Other`**, `Correction`, `CorrectionBucket`, `Aggregate`, `HeldOut`, `LearningUpdate`, `AbComparison`, `Consent`, 20 `LearnCode`s, `LearnService`; `MIN_CORRECTIONS`, `MIN_PROJECTS`, `HELD_OUT_SHARE`, `MAX_STEP_SHARE`, `ROLLBACK_DEPTH` |
| EM - registry | `crates/aura-core/errors.toml`, 21 runbooks in `docs/runbooks/` | registry test | `AURA-RENDER-8020..8025`, `AURA-DLV-10001..10006`, `AURA-LRN-11001..11005`, `AURA-REL-12001..12004`; 235 codes, three new domains |
| SRC - migration | `crates/aura-catalog/migrations/0030_delivery.sql` | catalog suite | 13 tables, 3 views, 5 triggers; a verified file must carry a digest, a manifest cannot be updated, an upload cannot claim to have sent more than it has, and an update cannot adopt itself |
| SRC - naming | `crates/aura-export/src/naming.rs` | unit | Seven tokens, the whole plan made before a byte is written, collisions suffixed deterministically, and a template that names a folder refused |
| SRC - pixels | `src/resample.rs`, `src/icc.rs`, `src/metadata.rs` | unit | Downscale in linear light, sharpen on encoded samples after it; v4 matrix/TRC profiles synthesised with the creation date zeroed; a metadata block **built** rather than copied forward |
| SRC - writers | `src/jpeg.rs`, `src/tiff.rs`, `src/png.rs` | unit | Three formats, 8 and 16 bit where the format allows, every one carrying its profile |
| SRC - verification | `src/verify.rs` | unit | Write, flush, `sync_all`, re-open, read, hash. The digest stored is the digest of the file |
| SRC - manifest | `src/manifest.rs`, `src/store.rs`, `src/api.rs` | integration | The sealed record and its travelling copy; `last_spec` reads a job's shape back without its photographs |
| PM - presets | `crates/aura-export/config/export_presets.toml`, `src/sets.rs` | unit | Six presets with a written reason each; three widened bounds refused rather than clamped |
| SRC - backup | `crates/aura-delivery/src/backup.rs` | unit | Matched, missing, **diverged** - and diverged halts rather than overwriting |
| SRC - providers | `src/providers/`, `src/mapping.rs` | unit | The `Transport` port, a folder transport, a scripted one, per-set mapping; `NETWORK_TRANSPORT_AVAILABLE` is false and is on the wire |
| SRC - resume | `src/resume.rs`, `src/store.rs`, `src/api.rs` | integration | 4 MiB chunks, the offset taken from the far end rather than from local state, three attempts, and a wrong digest that is corrupt rather than failed |
| SRC - capture | `crates/aura-learn/src/capture.rs`, `src/attribute.rs` | unit | A correction written beside the decision it corrected; fifteen learnables and **no way to name a guarantee** |
| SRC - aggregate | `src/aggregate.rs` | unit | Deterministic held-out split by the correction's own id, trimmed median, MAD with a mean-absolute fallback, and both floors |
| SRC - update | `src/update.rs`, `src/review.rs` | unit | The half-step bound, the ceiling, the held-out improvement, and the 2 % floor below which nothing is offered |
| SRC - rollback | `src/rollback.rs`, `src/store.rs`, `src/api.rs` | integration | Ten versions kept, exact restore, and an update that was rolled back is not offered again |
| QAL - grep test | `crates/aura-learn/tests/no_guarantee_learning.rs` | 5 | The tenth grep-as-a-test: no guarantee among the learnables, no renderer, no file, no socket, no threshold this crate could move |
| SFE - plugins | `plugins/lightroom/aura.lrdevplugin/`, `plugins/photoshop/aura-uxp/` | - | Selection and recipe round trip; a layered TIFF hand-off where the operation can be expressed as one |
| DEVOPS - ops | `ops/release/`, `ops/sign/`, `ops/notarise/`, `ops/update/`, `ops/crash/`, `ops/flags/` | - | Nine executable release gates and four sign-offs; staged rollout with a crash-free floor; cleanup and learning off by default |
| SFE - IPC | `crates/aura-app/src/delivery_commands.rs`, `learn_commands.rs`, `contract/ipc.rs` | - | Seventeen commands, fifteen DTOs, the `Field` and `Source` ports over `AppState` |
| SFE - shell | `ui/src-tauri/src/main.rs`, `ui/src/ipc/{client,types}.ts` | - | 259 handlers, 259 registered, 259 client wrappers - asserted by the gate and by `scripts/check-ipc-surface.sh` |
| MFE - panels | `ui/src/components/delivery/` | 15 vitest | Five pure views and a container; mounted in `App.tsx` |
| SFE - autopilot | `crates/aura-app/src/autopilot_commands.rs`, `crates/aura-jobs/src/stages/deliver.rs`, `config/autopilot.toml` | gate check 12 | The export stage runs the job this wedding was already given and skips with `NoInput` when there is none; `AppRunner::availability` is empty for the first time |
| QAL - gates | `tests/eval/delivery_eval.rs` | 21 | Section 10.1's rows: verification catches a corruption, 4,000 names are unique, metadata is built not copied, the two bounds hold, an update improves on held-out corrections |
| PERF - budgets | `crates/aura-perf/tests/delivery_budgets.rs`, `perf/budgets.toml` | 5 | Two rows measured, three waived with their reasons printed on every run, plus the storage figure with its bound asserted |
| CTO - gate | `crates/aura-cli/src/phase30.rs`, `main.rs`, `justfile` | - | Twelve checks; exits 0, and prints the six conditions it did not prove on every run |
| DOC - docs | `docs/delivery.md`, `docs/learning-loop.md`, `docs/release-process.md`, `docs/privacy.md` | - | What a delivery promises, what AURA will never learn, how a release ships, and where everything goes - which is nowhere |
| EM - lock | `xtask/src/main.rs`, `contracts.lock` | contract check | Migration 30 added to `EXTRA_CONTRACTS`; 81 entries locked |

## Benchmark deltas

| Metric | Budget | Measured |
|---|---|---|
| Export 1,000 45 MP JPEGs (reference GPU) | 12 min | **waived** - no `wgpu` backend |
| Export throughput | 1.4 images/s | **waived** - same measurement |
| Hash verification overhead | 8 % of export time | 2 % (release) |
| Upload 1,000 images at 100 Mbps | 35 min | **waived** - no network transport |
| Learning update, 1,800 corrections across 45 buckets | 90,000 ms | under 1 ms |
| Store, per delivered file (20 files) | 1,200 B | 594 B |
| Store, per delivered file (200 files) | 1,200 B | 566 B |

Three of section 11's five rows are waived, and the budget suite prints all three with their
reasons on every run rather than omitting them — phase 28's rule, written after its own first gate
printed a wall clock over a scripted runner.

The two that are measured are the two that are entirely this phase's own work. The verification
overhead is the one that decides whether anybody switches the guarantee off, and at 2 % nobody will.

The store's shape is **flat**: one row per delivered file plus a bounded per-job header, so ten
times the files cost 9.5 times the bytes. That is neither phase 29's falling figure nor phase 26's
growing one, and the bound is asserted as well as the number.

## What did not happen

Section 10.1's headline rows need things this repository does not have.

**No profile has been fitted from a real photographer's corrections.** The fifteen per cent
style-match improvement is unmeasured, `FITTED_ON_REAL_CORRECTIONS` is false, and it is on the wire.

**No upload has reached a real gallery**, because no network transport ships. The state machine, the
resume, the per-set mapping and the digest comparison are exercised against a transport that drops
on demand.

**Nothing has been signed, notarised or rolled out.** The procedure is here; the certificate, the
Apple account and the install base are not.

**There has been no closed beta**, so the 99.5 % crash-free rate is a floor with no measurement
behind it.

All four are conditions in the exit report and all four are printed by the phase gate on every run.

One further note about the release checklist, because it is the sort of thing that reads as a
regression and is not. `ops/release/check.sh` runs nine gates and eight are green; `budgets` fails on
this container on **phase 14's** rows, not on this phase's — the processor-path proxy render takes
801 ms against a 450 ms budget measured on a machine about four times faster, on a build with no GPU
backend. Every budget in the workspace passes at `AURA_PERF_HOST_SCALE=4`, which is what that
variable is for. The budget was not moved.
