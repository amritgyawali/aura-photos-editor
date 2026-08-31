# Phase 26 progress - Multi-Camera & Second-Shooter Matching

One line per task, in the order section 8 asks for them.

| Task | Files touched | Tests added | Note |
|---|---|---|---|
| Step 0 - branch | - | - | `feat/phase-26-multi-camera-shooter-matching`, cut and pushed before any code |
| COL - baselines | `assets/camera_baselines/*.toml` | loader tests | Eight brands, every one `measured = false`; the loader refuses a file that claims otherwise without saying who and when |
| CTO - ADR | `docs/adr/ADR-0053-camera-matching-and-appearance-distance.md` | - | Ten decisions; the three gains are derived because they are not identifiable |
| CTO - ADR | `docs/adr/ADR-0054-camera-ipc-surface.md` | - | Eleven commands, the report as a sentence, three things a photographer can say |
| TLC - freeze | `crates/aura-core/src/contract/camera.rs`, `contract/ids.rs` | contract tests | `CameraFingerprint`, `CameraTransform`, `AppearanceDistance`, `FlashState`, `Brand`, `TransformSource`, `MatchedPair`, `ShooterBias`, 32 `CameraCode`s, `CameraMatchService`; `PairId` is the fifteenth typed id |
| SRC - migration | `crates/aura-catalog/migrations/0026_camera_match.sql` | catalog suite | Five tables, two views, three triggers; the four bounds as CHECK constraints |
| PM - policy | `config/camera_match.toml`, `src/camera/policy.rs` | policy tests | Bounds lowerable and evidence thresholds raisable only |
| SRC - fingerprints | `src/camera/fingerprint.rs` | unit | Per body per flash state, from the wedding's own frames |
| SRC - pairs | `src/camera/pairs.rs` | unit | Verified on backgrounds, never on subjects; rejected pairs written |
| MLL - solver | `src/camera/solve.rs`, `transform.rs` | unit | Bounded coordinate descent over seven parameters; three gains in closed form |
| MLL - blending | `src/camera/baseline.rs` | unit | Continuous rather than a threshold; held-out verification decides |
| SRC - shooter | `src/camera/shooter.rs` | unit | Corrected by less than the habit, always, and opposite in sign |
| SRC - report | `src/camera/report.rs` | unit | The sentence is assembled in Rust so the panel and phase 27 agree |
| SRC - store | `src/camera/store.rs`, `api.rs` | integration | Ordering folded into phase 25's frames |
| QAL - gates | `tests/eval/camera_eval.rs` | 17 | Every section 10.1 row, plus the four refusals |
| QAL - grep test | `tests/no_recipe_writes.rs` | extended | The camera module inherits phase 25's five scans |
| SFE - IPC | `crates/aura-app/src/camera_commands.rs`, `contract/ipc.rs`, `state.rs` | - | Eleven commands; fingerprints carry their sample counts |
| SFE - shell | `ui/src-tauri/src/main.rs`, `ui/src/ipc/{client,types}.ts` | - | 210 handlers, 210 registered, 210 client wrappers; `within_moment` registered for the first time since phase 10 |
| SFE - panel | `ui/src/components/camera/{CameraMatchView,CameraMatchPanel}.tsx` | vitest | Reference chooser, per-camera report, matched pairs including the rejected ones |
| PERF - budgets | `crates/aura-perf/tests/camera_budgets.rs`, `perf/budgets.toml` | 4 | 35 ms against a 25 s budget; 57 B/image, bounded rather than proportional |
| QAL - catalog side | `ml/eval/camera_match_eval.py` | self-test | Catches a widened bound and a shooter corrected by more than their whole habit |
| CTO - gate | `crates/aura-cli/src/phase26.rs`, `main.rs`, `justfile`, CI | - | Twelve checks; exits 0 |
| DOC - docs | `docs/camera-matching.md` | - | What matching needs, what it falls back on, how to switch it off |
| EM - registry | `crates/aura-core/errors.toml`, `docs/runbooks/AURA-ML-513{0..5}.md` | registry test | Six codes, six runbooks |

## Benchmark deltas

| Metric | Budget | Measured |
|---|---|---|
| Fingerprinting + pair discovery | 18 s | 1 ms over 1,000 frames |
| Solve per camera | 1 s | 17 ms |
| Total matching pass | 25 s | 35 ms |
| Storage | not in section 11 | 57 B/image, and **bounded** |

The pass is three orders of magnitude inside its budget for phase 25's reason: **it opens no
pixels**. Every number it works on was stored by phases 15, 16 and 25. The budgets keep their full
section 11 values rather than being re-based, because this build measures no skin - when phase 18's
segmenter is trained, every fingerprint gains a masked statistic per contributing frame and the
fingerprinting stage stops being arithmetic over stored rows.

## What the storage measurement corrected

The budget note said the pair table grew with the square of a wedding's overlap. **It was written
before the measurement and was wrong about the shape rather than the size.** `pairs::find` truncates
at `MAX_PAIRS_PER_CAMERA` verified pairs and the same number again of rejected ones, so a two-body
wedding stores a fixed number whether it is 200 frames or 4,000: 57,724 B over a thousand
photographs, 57,729 B over two thousand.

The test now asserts the *bound* as well as the size, by running the same pass over a doubled wedding
and requiring the store not to double. A size assertion alone would pass on a build that had quietly
removed the cap and happened to be measured on a small fixture.

Phase 21's rule - measure before you write the figure down - covers the sentence as much as the
number, which is the part this phase adds to it.

## Two things the gate caught that the unit tests could not

**A pair cannot name a photograph the catalog does not have.** `camera_pair.left_image` and
`right_image` are foreign keys onto `photo`, and the gate's first run failed because its fixture
seeded a project without its photographs. Phase 25's gate made the identical finding about a skin
correction naming an identity that did not exist. Twice in two phases is enough to state it: **a
fixture that seeds a project but not its rows passes every unit test and fails the first time a
foreign key is involved**, because a store test is handed ids rather than making them.

**The `project` table has no `root_path` column.** The gate's seed invented one, and nothing else in
the product inserts a project outside phase 01's own ingest path.
