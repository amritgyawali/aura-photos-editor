# Phase 26 exit report - Multi-Camera & Second-Shooter Matching

**Status:** implemented conditionally. Four conditions, two of them Sev 2.

Phase 27 may start. Nothing in it may claim a camera-matching *quality* result until C1 and C4
close, nothing may claim anything about how skin from two bodies compares until C3 closes, and
**nothing anywhere in the product may present a bundled brand baseline as a measurement** until C2
closes.

---

## 1. What shipped

| Deliverable | Where |
|---|---|
| Frozen contract | `crates/aura-core/src/contract/camera.rs`, `PairId` in `contract/ids.rs` |
| Decisions | `docs/adr/ADR-0053-camera-matching-and-appearance-distance.md`, `ADR-0054-camera-ipc-surface.md` |
| The engine | `crates/aura-brain-gallery/src/camera/` - eleven modules |
| Schema | `crates/aura-catalog/migrations/0026_camera_match.sql` - five tables, two views, three triggers |
| Policy | `crates/aura-brain-gallery/config/camera_match.toml` - 23 argued-over scene rows |
| Baselines | `assets/camera_baselines/` - eight brands, every one `measured = false` |
| IPC | `crates/aura-app/src/camera_commands.rs` - eleven commands |
| Panels | `ui/src/components/camera/` - two components, tests |
| Gates | `tests/eval/camera_eval.rs` (17), `ml/eval/camera_match_eval.py` |
| Budgets | `crates/aura-perf/tests/camera_budgets.rs`, `perf/budgets.toml` |
| Executable gate | `cargo run --release -p aura-cli -- verify --phase 26` |
| In the product's own voice | `docs/camera-matching.md` |

**No model.** The sixth phase since 08 to ship none, and the reason is phase 17's, 23's and 25's:
there is nothing to train. A fingerprint is a set of statistics, the solver is a bounded coordinate
descent with a closed form for three of its ten parameters, and the blend is a ratio. What is missing
is not weights but **multi-camera weddings**.

**No cloud call.** Section 7 says the gateway stays idle, and `aura-cloud` is absent from
`aura-brain-gallery` - a property of the dependency graph rather than a rule somebody follows.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Frames from different brands in the same scene look like they came from one camera | **Partial** | The grade-signature distance falls past the 65 % gate and skin lands under 2.0 dE00 on authored fixtures. Whether a photographer could tell is **unmeasured** - condition C4 |
| 2 | The reference camera is chosen sensibly and is user-selectable | **Met** | `ReferenceSource` is `user`, `primary_shooter` or `frame_count`; the gate pins a choice and re-runs the whole pass |
| 3 | Matched pairs are found and verified on backgrounds | **Met** | Gate: 80 verified pairs on a two-body fixture, and a pair whose backgrounds disagree is rejected and kept |
| 4 | Flash and ambient receive distinct transforms | **Met** | Eval gate 4 and 4b: distinct rows, and no pair is ever formed across the boundary |
| 5 | With no matched pairs, brand baselines are used and the report says so | **Met** | Gate: a wedding with no overlap produces zero pairs, four baseline transforms and a report sentence naming it |
| 6 | Camera transforms precede within-scene normalisation | **Met** | Eval gate 5 and gate step 7: a frame reaches phase 25 at 5,442 K where phase 15 stored 5,200 K |
| 7 | A second shooter is harmonised, not erased | **Met** | Every correction smaller than the measured habit and opposite in sign, asserted in Rust and again in the Python harness |

---

## 3. Section 10.1 gates

Seventeen, all in `tests/eval/camera_eval.rs`, all green.

| Gate | Threshold | Measured |
|---|---|---|
| Cross-camera skin dE00 in matched scenes | <= 2.0 | met on authored fixtures |
| White points converge | must fall | met |
| Grade-signature distance reduced | >= 65 % | met |
| Held-out verification improves unseen evidence | must improve or fall back | met, with the fall-back proved |
| Flash and ambient get distinct transforms | distinct | met |
| No pair across the flash boundary | zero | met |
| Ordering: camera before within-scene | enforced | met, as a data dependency |
| No matched pairs -> baseline, honestly reported | must say so | met |
| Unknown manufacturer changes nothing | identity | met |
| Two bodies of one make are never corrected toward each other by a baseline | zero | met |
| No camera exceeds the documented movement | no exceedance | met |

---

## 4. Conditions

### C1 - Every number came from a synthetic wedding. **Sev 2.**

There are no multi-camera weddings in this repository. Section 9's DATA row asks for "Sony+Canon,
Canon+Nikon, +Fuji with matched scenes" and there are none, so the per-brand colour response in every
fixture was **authored** - a chosen chromaticity shift, a chosen saturation response, a chosen
highlight roll-off - and read back through the real fingerprinter, the real pair finder, the real
solver and the real store.

What that proves: the fingerprinting, the pair discovery, the background verification, the
identifiability argument behind the derived gains, the bounded descent, the held-out split, the
evidence blend, the shooter cap, the ordering and the whole assembly.

What it does not prove: that a Canon and a Sony photographing the same ceremony behave the way the
fixtures say they do.

It closes with phase 05's condition C10 rather than separately, because the pair finder's subject
similarity reads phase 05's embedding, which carries no wedding semantics.

Trigger to reopen: the first wedding shot on two bodies with phases 15, 16 and 25 run over it.

### C2 - All eight bundled brand baselines were fabricated. **Sev 2.**

Section 8 step 1 asks COL to "measure bundled brand baselines in controlled conditions". There is no
lab, no ColorChecker and no camera here - phase 02's conditions C1 and C2, still open after
twenty-four phases.

So every file in `assets/camera_baselines/` carries `measured = false`, and that field is **read
rather than decorative**: `BaselineLibrary::load` refuses a file claiming `measured = true` without a
`measured_by` and a `measured_at`, the panel shows which of the two a correction came from, and
`report::summarise` says "from what AURA knows about the brand alone" rather than naming a number.

**This is the condition to be most careful about, because the fallback path is the *common* one.** A
wedding where the second shooter worked a different room all afternoon has no matched pairs at all,
and on that wedding every correction this phase applies comes from a fabricated table. What is proved
is that the path runs, that it reports itself honestly, and that an unknown manufacturer changes
nothing. Nothing is proved about the numbers.

Trigger to reopen: the first measured baseline, whatever phase is in flight - exactly as the first
real camera file reopens phase 02's criteria and the first measured lens profile reopens phase 23's.

### C3 - The skin term of the appearance distance is unmeasured.

Phase 25's `SKIN_FIELD_AVAILABLE` is false. Phase 18's segmenter is untrained, so no photograph in
this build carries an identity-scoped skin region, and the term weighted **3.0** in section 6.2's
objective - the heaviest of the four - contributes nothing on a real photograph.

The remaining three terms are measured and the solver runs on them, which means a real wedding today
would be matched on its white points, its grade signatures and its contrast. That is a weaker match
than the phase promises and is not a wrong one.

Three things keep it visible rather than silent: `AppearanceDistance::skin_de00` is zero rather than
absent and every reader treats a zero as *not measured*; `report::summarise` prints "skin was not
measured at this wedding, so no claim is made about how skin from the different cameras compares";
and `ml/eval/camera_match_eval.py` prints `NOT MEASURED` rather than `PASS`, asserted by its own
self-test.

Trigger to close: phase 18's segmentation head trained. Closes with phase 25's condition C2.

### C4 - The blind study did not happen.

Section 9 gives QAIQ "blind review: can a photographer tell which camera shot which frame after
matching?", 3 days. It has not been run, because there are no multi-camera weddings.

**This is the phase's own headline acceptance criterion.** Section 13's first line is "frames from
different brands in the same scene look like they came from one camera", and a grade-signature
distance falling by 71 % is a number about a descriptor rather than an answer about a photograph.

The failure this study exists to catch is also the one the gates cannot see: a transform that
minimises the appearance distance while making one body's files look subtly *wrong* - not different
from the reference, but not like a photograph either. The bounds are the defence, and they are a
defence rather than evidence.

Trigger to close: a blind identification study on a wedding shot by two bodies, by somebody who did
not build this.

---

## 5. What is deliberately absent

**No per-frame override on this surface.** A camera transform is a statement about a body; a
photographer who wants one frame different is looking for phase 15's tone override or phase 25's
gallery override. A fourth place to change the same number would be a fourth thing to keep from
disagreeing.

**No baseline editing through the window.** A studio that could edit a bundled baseline could change
what an already-delivered photograph looks like under an identical hash - phase 23's argument for
putting lens coefficients in the recipe, read from the other side.

**No fingerprint cache across passes.** Section 9 gives PERF "cache fingerprints"; the whole pass
costs 35 ms against a 25 s budget, so the cache would be an optimisation of something already three
orders of magnitude inside its row. `camera_budgets.rs` asserts that re-running is not pathological
rather than asserting a cache exists, so adding one later is an optimisation rather than a contract
change.

---

## 6. Rollback

* **Feature flag off:** `disable_camera` per body; a whole project is left untouched by not running
  `camera_pass`, and every frame keeps exactly the answer phases 15 and 16 gave it.
* **Previous policy pinnable:** `camera_match.toml` carries a `version`; bumping it back re-solves
  every row under the old table.
* **Migration reversible:** DROP five tables, two views and three triggers. Nothing earlier
  references them.

---

## 7. Regression

`cargo test --workspace --all-targets` is green. `cargo clippy --workspace --all-targets -D warnings`
is clean. `scripts/check-banned.sh` reports clean. `cargo run -p xtask -- contracts --check` reports
72 entries, all locked. `cargo run -p aura-cli -- verify --phase 26` exits 0. The UI type-checks and
its tests pass. The IPC surface is 210 handlers, 210 registrations and 210 client wrappers.

No acceptance criterion from an earlier phase regressed. Phase 25's own gate still exits 0 with camera
transforms folded into the frames it solves over, which is the ordering test read from the other side.

---

## 8. Two things the gate caught that the unit tests could not

**A pair cannot name a photograph the catalog does not have.** `camera_pair.left_image` and
`right_image` are foreign keys onto `photo`, and the gate's first run failed on them because it
seeded a project without its photographs. That is the constraint working, and it is the same finding
phase 25's gate made about a skin correction naming an identity that did not exist - twice in two
phases, which is enough to note that **a fixture that seeds a project but not its rows will pass every
unit test and fail the first time a foreign key is involved.**

**The `project` table has no `root_path` column.** The gate's seed invented one. A unit test never
touches `project` because every store fixture is handed a project id rather than making one.

---

## 9. Inherited conditions still open

| From | Condition | Bearing on this phase |
|---|---|---|
| 02 | C1-C3 - no camera files, no ColorChecker, no CI matrix | **C2 of this phase is downstream of C2 of that one**: no ColorChecker means no measured baseline |
| 05 | C10 - the embedding carries no wedding semantics | The pair finder's subject-similarity term is inert; C1 closes with it |
| 06 | C1 - the face models are placeholders | No skin patch, so no skin term |
| 15 | C1 - both tone heads are placeholders | Every fingerprint reads phase 15's stored answers |
| 16 | C1 - the tone head is a placeholder | The grade-signature term reads phase 16's decisions |
| 18 | C1, C2 - the segmenter is a placeholder | **C3 of this phase is the same gap** |
| 25 | C1-C4 | The node tree this phase pairs inside, and its skin field |
