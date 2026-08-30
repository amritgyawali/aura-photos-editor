# Phase 25 progress - Gallery Intelligence Engine

One line per task, in the order section 8 asks for them.

| Task | Files touched | Tests added | Note |
|---|---|---|---|
| Step 0 - branch | - | - | `scripts/phase-branch.sh 25 gallery-intelligence-engine`, pushed before any code |
| CTO - ADR | `docs/adr/ADR-0051-gallery-consistency-and-normalisation.md` | - | Eleven decisions; the delta is a residual measured from the un-normalised world |
| CTO - ADR | `docs/adr/ADR-0052-gallery-ipc-surface.md` | - | Nine commands, two denominators, four things a photographer can say |
| TLC - freeze | `crates/aura-core/src/contract/gallery.rs`, `contract/ids.rs`, `lib.rs` | 15 contract tests | `SceneNode`, `NodeTarget`, `NormalisationDelta`, `SkinTarget`, `SkinCorrection`, `Bound`, `Outlier`, 26 `GalleryCode`s, `GalleryService`; `NodeId` is the fourteenth typed id |
| SRC - migration | `crates/aura-catalog/migrations/0025_gallery.sql`, `migrate.rs` | catalog suite | Five tables, two views, three triggers; the five bounds as CHECK constraints |
| PM - policy | `crates/aura-brain-gallery/config/consistency.toml`, `src/policy.rs` | 8 | 23 argued-over scene rows; a widened bound is refused with `AURA-ML-5129` |
| COL - statistics | `src/stats.rs` | 9 | Trimmed means, component-wise median, circular hue median, the eight-number grade signature |
| SRC - tree | `src/tree.rs` | 7 | Segments plus time sub-clustering; a segment is never divided into parts too small to anchor |
| MLL - change points | `src/changepoint.rs` | 11 | Two rules: a step the trend does not explain, and a span no target can cover |
| SRC - anchors | `src/anchors.rs` | 10 | Four terms multiplied; a pin is a veto and a rejection is as durable |
| SRC - solver | `src/normalise.rs` | 13 | Damped then bounded; exposure moves in stops; a clamped frame is less confident |
| COL - skin | `src/skin_consistency.rs` | 10 | Per-identity targets from that person's own frames; the cap falls with the mood but never to zero |
| COL - scene | `src/scene_consistency.rs` | 6 | Contrast and saturation scaled by the character gap; off in four scenes where the variation is the point |
| MLL - outliers | `src/outlier.rs` | 8 | Measured on the residual, never on the raw deviation |
| SRC - store | `src/store.rs`, `src/api.rs` | 13 integration | Membership is the delta table; a reason set is one integer |
| QAL - grep test | `tests/no_recipe_writes.rs` | 6 | No recipe, no file, no provider, no tone solver, no ideal-skin constant |
| QAL - gates | `tests/eval/consistency_eval.rs` | 10 | Seven section 10.1 gates, plus the out-of-bound case and the doc-coverage check |
| SFE - IPC | `crates/aura-app/src/gallery_commands.rs`, `contract/ipc.rs`, `state.rs` | - | Nine commands; both denominators on the wire |
| SFE - shell | `ui/src-tauri/src/main.rs`, `ui/src/ipc/{client,types}.ts` | - | 199 handlers, 199 registered, 199 client wrappers |
| SFE - panels | `ui/src/components/gallery/{ConsistencyView,TimelineStrips,AnchorPicker,OutlierList}.tsx` | 15 vitest | Strips are numbers, not thumbnails |
| PERF - budgets | `crates/aura-perf/tests/gallery_budgets.rs`, `perf/budgets.toml` | 4 | 74 ms for 1,000 images against 60 s; 330 B/image against 500 B |
| QAL - catalog side | `ml/eval/consistency_eval.py` | self-test | Catches a catalog written by a build whose bounds had widened |
| CTO - gate | `crates/aura-cli/src/phase25.rs`, `main.rs`, `justfile` | - | Thirteen checks; exits 0 |
| DOC - docs | `docs/gallery-consistency.md` | 1 | Every one of the 26 codes and its sentence, asserted by a test |
| EM - registry | `crates/aura-core/errors.toml`, `docs/runbooks/AURA-ML-512{3..9}.md` | registry test | Seven codes, seven runbooks |

## Benchmark deltas

| Metric | Budget | Measured |
|---|---|---|
| Consistency pass, 1,000 images | 60 s | 74 ms |
| Incremental re-solve after one anchor change | 6 s | 1 ms |
| Timeline strip query, 100-frame node | 400 ms | under 1 ms |
| Storage per image | 500 B | 330 B |

The pass is three orders of magnitude inside its budget for a structural reason: **it opens no
pixels**. Every number it works on was stored by phases 07, 15 and 16. The budget keeps its whole
60 s rather than being re-based, because the skin half will not be free in the same way once phase
18's segmenter is trained - every selected frame will gain one proxy decode and one masked statistic.

## Two defects this phase found in its own work

**The change-point statistic had a trend in it**, so it split the slow drift the phase exists to
normalise: a 500 K wander over forty frames became six unanchorable nodes reported as six lighting
changes. The divisor is the trend now. The first fix was half right and used the shorter run's
length rather than the distance between the two runs' midpoints, which scored a smooth ramp at six.

**The exposure gate's fixture authored a lighting change and called it drift.** A full stop of
within-node variation cannot have its spread halved when the bound is 0.35 EV. The gate measures a
realistic third of a stop now, and a second test asserts that a wider drift is *reported as
outliers* rather than silently half-corrected.

## Two things the gate caught that the unit tests could not

**A schema scan matched the column it existed to protect.** The check for an absolute temperature on
`gallery_delta` searched for the substring `cct_k`, which is inside `from_cct_k` - the column that
makes a residual auditable. It scans column names now, with a control that asserts the residual and
its origin are both present, so a clean pass means the scan read something.

**A skin correction cannot name an identity the catalog does not have.**
`gallery_delta.skin_identity` is a foreign key onto `identities`, and the gate's first run failed on
it. That is the constraint working: a stored correction that named nobody would be a statement about
what was done to a person who does not exist.

## What phase 25 fixed that was not phase 25's

`main` was red on CI lane 1 when this branch was cut, and the phase ritual refuses a merge on a
failed check. Four things, none of them this phase's work, all of them phase 24's:

**`cargo fmt --all -- --check` failed on fifteen files.** Phase 24 landed unformatted, including
`crates/aura-core/src/contract/cleanup.rs` - a **frozen contract**, which means its digest in
`contracts.lock` was over unformatted text. Reformatting it changed the digest, so this phase
re-locked it. Worth remembering: `cargo xtask contracts` hashes bytes, and a formatter is a thing
that changes bytes.

**`cargo clippy --workspace --all-targets -- -D warnings` failed with 122 errors**, every one of them
in `aura-generative`. 112 were the numeric-cast family, which every other pixel crate in the
workspace already allows crate-wide with a written reason - `aura-raw` since phase 02,
`aura-retouch`, `aura-restore` - and which this crate does exactly the same kind of arithmetic to
need. The other ten were real: four `indexing_slicing` denials inside `borrow.rs`'s exhaustive
homography search, a manual `Debug` impl that omitted `studio_opted_in` (the flag that decides
whether the cloud judge may be consulted at all, so a support bundle was quietly less useful than it
looked), two loop-variable indexings, three `% 2 == 0`, and a doc line missing backticks.

**`aura-cloud` had no `cfg_attr(test, allow(...))` block.** `cleanup_judgement.rs` arrived with the
first inline test module in that crate, so `expect()` in a test was a denied lint at the crate root.

**Every command on `aura-app`'s surface takes its input DTO by value**, which is what the Tauri
handler has, and clippy's `needless_pass_by_value` names phase 24's four because theirs happen not to
be moved into a call. The allow is at the crate root with the argument written down, because it
describes the whole surface rather than four commands.

None of these are phase 25 work and all of them stood between this phase and a green merge. Phase 17
did the same for phase 16's stale digest and phase 21 for phase 20's missing icons; recording it here
is the third time.
