# Phase 08 progress - Smart Burst Grouping & Duplicate Detection

One line per task group, in the order section 8 asks for them. The exit report is
`docs/progress/PHASE-08-EXIT.md`.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| CTO - freeze section 5 before any code | `crates/aura-core/src/contract/moment.rs`, `crates/aura-core/src/contract/ids.rs`, `crates/aura-core/src/lib.rs`, `docs/adr/ADR-0017-*.md` | in `tests/moments.rs` | `Moment`, `DuplicateSet`, `DuplicateKind`, `CameraId`, `MomentOutline`, `MomentService`, `MomentEdit`, plus `MomentId`. In `aura-core` for ADR-0015's reason applied a third time: seven later phases operate on moments and none of them needs the cadence estimator. Six spellings differ from the phase document; ADR-0017 section 2 records all six. |
| PM/MLR - the threshold table, authored before the engine | `crates/aura-brain-wedding/config/moment_profiles.toml` | in `tests/moments.rs` | Ten scenes carry an override and twelve take the defaults, and the file names which twelve so a reader can see what was actually argued over. A rationale under nine characters does not load - `AURA-ML-5031`. A threshold above 0.85 does not load either, because a pair with no shared faces cannot reach it and the table would silently disable grouping for detail shots. |
| SRC - migration 8 and the store | `crates/aura-catalog/migrations/0008_moments.sql`, `crates/aura-catalog/src/{migrate,lib}.rs`, `crates/aura-brain-wedding/src/moments/moment.rs` | in the gate, `moment_budgets.rs` | Four tables, two views, three version columns. `user_locked = 0` is inside the `DELETE` a re-grouping starts with, and the locked frames are subtracted from the pass's input rather than reconciled after it. Shaped by section 11's 200-byte budget before being measured against it: no `project_id` on the membership table, a burst as a column, one index. |
| SRC - cadence estimation | `crates/aura-brain-wedding/src/moments/cadence.rs` | `tests/moments.rs` (10) | Section 6.1's `clamp(2.5 x median, 0.7 s, 8 s)`, per camera, from a rolling median over a 60 s neighbourhood. Two shooters do not halve each other's windows, which is the two-shooter failure phase 07 guards one layer up. Drive evidence in three tiers: the camera said so, two frames share a second, or the gap is under 250 ms. |
| MLR - the similarity graph and its thresholds | `crates/aura-brain-wedding/src/moments/graph.rs` | `tests/moments.rs` (10) | Section 6.2's four weights unchanged, plus a proximity multiplier and the scene-mismatch penalty; ADR-0017 section 3 records both and the failure that made the first necessary. Candidates come from a bounded forward sweep, never all pairs. Union-find is deterministic by size then by index. |
| SRC - bursts, and the two-tier structure | `crates/aura-brain-wedding/src/moments/burst.rs` | `tests/moments.rs` (7) | A partition of the moment, always, so `burst_count` reconciles with what a photographer counts. Per camera and contiguous: a burst is a fact about one shutter, and two shooters cannot share one. Four evidence tiers, the fourth being the only one that consults appearance. |
| MLL - duplicate classification | `crates/aura-brain-wedding/src/moments/duplicate.rs` | `tests/moments.rs` (10) | Section 6.3's three thresholds as a **conjunction** - a hash blind to a blink, an embedding blind to a stop of exposure, a face overlap blind to everything else, and all three must agree. The face test is skipped rather than failed when there is no face, and the skip costs the set confidence rather than being free. |
| SRC - cross-camera merging and the editing API | `crates/aura-brain-wedding/src/moments/merge.rs` | `tests/moments.rs` (9) | Section 6.2's 60 % overlap and 0.12 medoid distance, measured against the *shorter* span. Two family groups shot simultaneously from two sides do not merge, and the medoid test is what stops them. `plan_split` and `plan_merge` are pure, so every refusal is testable without a catalog. |
| SRC - the service and the pass | `crates/aura-brain-wedding/src/moments/api.rs`, `crates/aura-brain-wedding/src/moments/mod.rs`, `crates/aura-brain-wedding/src/lib.rs` | in the gate | Eight steps, and step 2 - subtract the locked frames - is before step 3 for the reason phase 06 replays decisions before building its graph and phase 07 smooths before it segments. Cross-camera merging runs to a fixed point, because a three-body wedding needs two passes. |
| DATA - the burst ground truth | `crates/aura-brain-wedding/src/fixtures.rs`, `tests/fixtures/labels/bursts_*.json` | `burst_eval.rs::the_label_files_and_the_rust_fixtures_agree` | Section 8 step 9's four patterns plus the bracketed detail case, in two places - Rust for the gates, JSON for the Python metrics - with a test that fails if they drift apart. Every label file carries a `why` a photographer would agree with, and the test asserts it is there. |
| SFE/MFE - the moments view | `ui/src/components/grid/{MomentStack,DuplicatePanel}.tsx`, `ui/src/ipc/{types,client}.ts`, `crates/aura-app/src/{moment_commands,contract/ipc,state}.rs`, `docs/adr/ADR-0018-*.md` | `MomentStack.test.tsx` (24) | Nine commands, eight types. A collapsed stack carries a count and at most one other badge, because a photographer scans a wedding's worth of them. The duplicate panel promises in three separate places that nothing is deleted, and three tests assert the sentences say so. |
| QAL/PERF - gates, budgets and the eval harnesses | `crates/aura-cli/src/phase08.rs`, `crates/aura-perf/tests/moment_budgets.rs`, `perf/budgets.toml`, `tests/eval/burst_eval.rs`, `ml/eval/burst_eval.py`, `justfile` | `burst_eval.rs` (16), `moment_budgets.rs` (4) | `just phase-08-verify` runs ten checks and exits 0, at ARI 1.000 on all five patterns. Three of section 11's four budget rows are met by two to three orders of magnitude; the storage row is waived at 340 against a measured 319, in ADR-0017 section 8. |
| DOC - runbooks, ADRs and the help page | `docs/runbooks/AURA-ML-503{0,1,2}.md`, `docs/runbooks/AURA-ML-502{8,9}.md`, `docs/adr/ADR-001{7,8}-*.md`, `docs/moments-bursts-and-duplicates.md`, `docs/progress/PHASE-08*.md`, `CHANGELOG.md`, `CLAUDE.md` | the error registry test | Five new codes, five runbooks, two ADRs, and a help page whose job is one distinction: a moment is a thing that happened, a burst is one press of the shutter, and a duplicate is a photograph that exists twice. |

## Three things the harness and the gate found that review would not have

Recorded because each one was a real defect in code that read correctly, and because two
of the three were only reachable end to end.

1. **Time proximity was a gate and never evidence.** Section 2.1 lists it first among the
   grouping signals; section 6.2's four-term score has no time term at all. Without one,
   a ceremony shot at one frame every eight seconds chains into a single moment for as
   long as the photographer keeps shooting - every pair is inside the eight-second clamp
   and every pair looks alike, because the altar has not moved. The `slow_ceremony`
   fixture came out as one moment where a photographer counts six. Fixed with a
   proximity multiplier that leaves section 6.2's four weights untouched; ADR-0017
   section 3.
2. **EXIF has whole-second resolution, so phase 08 could not see a burst at all.** Every
   unit test passed and the gate failed: `photo.timeline_time` comes from
   `DateTimeOriginal`, which has no fraction, so fourteen frames of a 10 fps burst carry
   one timestamp between them. The fraction is in `SubSecTimeOriginal`, which phase 01
   stores separately in `photo.sub_sec` - and section 6.1's "sub-second EXIF, where
   present" is that column. `moment::sub_sec_ms` reconstructs it. Grouping ARI went from
   0.000 to 1.000 on two of the five patterns.
3. **A drifting difference hash saturated and turned late frames into duplicates.** The
   fixture generator set bits cumulatively, so past the sixty-fourth flip every frame had
   the same hash and the last frames of a long burst were classified as copies of each
   other. One spurious pair in `bouquet_toss`, nine in `dance_floor`. Toggling rather than
   setting fixed it, and the duplicate precision gate is what caught it.

The storage figure is a fourth, of a different kind: the first draft's constant claimed
189 bytes per image and the catalog said 720. Four schema decisions took it to 319, and
the remaining gap is recorded as a waiver rather than closed - see below.

## What this phase did not do, and why

**Phase 06's two face signals are not wired in.** `PassContext::subject_focus` and
`face_overlap` sit at their documented neutrals. Feeding them would mean either a
`PeopleService` call per frame - four thousand queries on a wedding - or this crate
recomputing them from `faces`, which is the rule phase 06 wrote its two-crate split to
prevent. `PeopleService` has no bulk accessor for either, and adding one is a phase 06
contract change. It is condition **C4** in the exit report rather than a shortcut taken
here, and the degradation is visible: the keep hint falls through to edge energy and the
set's reasons say "no face pass has run".

**Phase 06's clustering budget still fails on this machine**, at 21.7 s against 12 s,
exactly as phase 07 recorded it. It was ruled out as a phase 07 effect by measurement
there, and nothing in phase 08 touches `aura_vision::face::cluster`. It is carried
forward again rather than repaired from inside a later phase.
