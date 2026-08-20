# PHASE-17 progress - Style Learning: Scene-Conditional Personal AI Profiles

One line per task group, in the order section 8 lists them. Branch
`feat/phase-17-style-learning-personal-ai`.

| # | Task | Files | Tests | Notes |
|---|---|---|---|---|
| T0 | Kickoff and ADRs | `docs/adr/ADR-0035-style-learning-and-personal-profiles.md`, `docs/adr/ADR-0036-style-ipc-surface.md` | - | Nine decisions and six, five of them about what the phase refuses to do |
| T1 | Freeze the section 5 contract | `crates/aura-core/src/contract/style.rs`, `contract/ids.rs` | `cargo xtask contracts` | `StyleProfile`, `StyleDelta`, `CurveShift`, `SkinBias`, `StyleBucket`, `BucketModel`, `ProfileDiagnostics`, 20 reason codes, `StyleService`, `ProfileId`. `BTreeMap` in place of section 5's `HashMap` - ADR-0035 decision 3 |
| T2 | Error registry and runbooks | `crates/aura-core/errors.toml`, `docs/runbooks/AURA-ML-5072..5077.md` | - | Six codes. `AURA-ML-5076` is the first in the product that refuses an artefact a photographer chose to import |
| T3 | Pair matching | `crates/aura-style/src/pairs.rs` | 9 unit | Four strategies in order of trust; an ambiguous stem or a shared capture second is a **refusal** rather than a coin toss |
| T4 | Parameter extraction | `crates/aura-style/src/extract.rs` | 6 unit | XMP read through `aura_recipe::xmp`, exact and free. One XMP pair in twenty is fitted as a check on the sidecars |
| T5 | The recipe fitter | `crates/aura-style/src/fit.rs`, `src/fit/optimise.rs` | 12 unit | Coordinate descent over twelve parameters against a **real render**. Two loss terms; the histogram one exists because dE00 is blind to a black point |
| T6 | Bucketing and features | `crates/aura-style/src/bucket.rs` | 8 unit | Eight groups by ten lights. Golden hour is a bucket and not an illuminant; fluorescent and LED are one. **No skin-group feature, ever** |
| T7 | The tree | `crates/aura-style/src/tree.rs` | 10 unit | Ridge on seven columns, forty-one targets, Huber by IRLS, James-Stein shrinkage, archive cap. The intercept is kept and the slopes are discarded - the module header has the argument |
| T8 | Diagnostics | `crates/aura-style/src/diagnostics.rs` | 10 unit | Held-out evaluation against the baseline as well as against the ceiling, weak buckets, one generated sentence |
| T9 | Inference | `crates/aura-style/src/infer.rs` | 8 unit | Bucket, group, global, factory. **Always answers**; a project with no profile gets a complete explained answer that changes nothing |
| T10 | Versioning and the bundle | `crates/aura-style/src/profile.rs` | 11 unit | Canonical form, ed25519, five refusals in cost order. What the signature proves is integrity, and the panel says so |
| T11 | Migration 17 and the store | `crates/aura-catalog/migrations/0017_style.sql`, `src/store.rs` | 6 unit + gate | Four tables, one view, four indexes. No skin colour anywhere and CHECKs below phase 16's own ceilings |
| T12 | The frozen service and the pass | `crates/aura-style/src/api.rs` | 4 unit + gate | `StyleService` plus the resumable walk. The resume unit is a **pair**, keyed on the original's hash |
| T13 | Fixtures | `crates/aura-style/src/fixtures.rs` | 5 unit | Three synthetic photographers whose finals are rendered by the real engine, so a recovery failure is a real optimiser defect |
| T14 | Wiring into phases 15 and 16 | `crates/aura-brain-photo/src/tone/style.rs`, `src/colour/style.rs`, both `analyse.rs` and both `api.rs` | 13 unit | The shift lands on the **solved** parameters and before every guard. Both `ANALYSIS_VER`s 1 -> 2 |
| T15 | The evaluation harness | `tests/eval/style_eval.rs`, `ml/models/style/{train_residual,eval_style,export}.py` | 19 eval + 19 self-test | Section 10.1's seven gates plus six checks that the harness can fail |
| T16 | The IPC surface | `crates/aura-app/src/contract/ipc.rs`, `src/style_commands.rs`, `ui/src-tauri/src/main.rs`, `ui/src/ipc/{types,client}.ts` | 3 unit | Eleven commands, fourteen shapes, and no field anywhere that could hold a pixel |
| T17 | The panels | `ui/src/components/style/{TeachMyAi,ProfileReport,BucketMatrix,AbCompare}.tsx` + test | 17 vitest | A measurement rather than a ready state, `null` rather than zero for unmeasured, and never the word "verified" |
| T18 | Privacy as a property | `crates/aura-style/tests/no_network.rs` | 3 unit | A grep as a test: no cloud crate, no socket, nothing that could carry an image |
| T19 | The gate | `crates/aura-cli/src/phase17.rs`, `justfile` | `aura-cli verify --phase 17` | Thirteen checks, exits 0. The justfile also gained the phase 16 recipe it had been missing |
| T20 | Docs and re-lock | `docs/style-profiles.md`, `contracts.lock`, `CLAUDE.md` | `cargo xtask contracts --check` | Migration 17 added to the frozen set; a stale `colour.rs` entry from phase 16 corrected |

## Four things that changed direction during the phase

**The archive cap was wrong the first time, and the gate caught it.** Scaling one archive's
weight by `cap / share` leaves it *above* the cap, because shrinking its weight also shrinks the
total it is a share of. The measured influence was 48 % against a documented 35 %, which is the
worst kind of defect: the guarantee reads correct and measures wrong. The fix is
`w = cap * rest / (1 - cap)`, and `gate_5` in `tests/eval/style_eval.rs` is what found it.

**The slopes are fitted and then discarded.** The first design applied the regression's slopes
at inference, which is what section 6.2 literally asks for. A slope fitted on eleven samples
spanning ISO 1600 to 4000 is not identified at ISO 400, and the frame it would be applied to is
exactly the ISO 400 one. The slopes now do the job they are actually good at - keeping a
confound out of the intercept - and the intercept is what ships. `crates/aura-style/src/tree.rs`
has the argument in full.

**`strength()` reported zero for a profile nobody had measured.** An unevaluated profile has an
`overall_de00` of zero, and the first version read that as an accuracy of zero rather than as an
absence. A meter that shows "nothing learned" for a profile that was trained ten seconds ago is
a meter nobody trusts afterwards. The mean is now taken over the terms that exist.

**The tone half applies the style to the *solved* answer and re-runs phase 15's own bounds**,
rather than shifting the target band. Shifting the band was the first design and it is cleaner
on paper; it also makes a style profile change what "correctly exposed" means, which is phase
15's decision and not this phase's. The style now moves the answer and the same clipping bound
and the same skin-locus constraint decide how much of the move survives.
