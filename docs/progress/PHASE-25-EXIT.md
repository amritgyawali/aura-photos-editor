# Phase 25 exit report - Gallery Intelligence Engine

**Status:** implemented conditionally. Four conditions, two of them Sev 2.

Phase 26 may start. Nothing in it may claim a gallery-consistency *quality* result until C1 and C3
close, and nothing anywhere in the product may claim anything about a person's skin being consistent
until C2 closes.

---

## 1. What shipped

| Deliverable | Where |
|---|---|
| Frozen contract | `crates/aura-core/src/contract/gallery.rs`, `NodeId` in `contract/ids.rs` |
| Decisions | `docs/adr/ADR-0051-gallery-consistency-and-normalisation.md`, `ADR-0052-gallery-ipc-surface.md` |
| The engine | `crates/aura-brain-gallery/` - thirteen modules |
| Schema | `crates/aura-catalog/migrations/0025_gallery.sql` - five tables, two views, three triggers |
| Policy | `crates/aura-brain-gallery/config/consistency.toml` - 23 argued-over scene rows |
| IPC | `crates/aura-app/src/gallery_commands.rs` - nine commands |
| Panels | `ui/src/components/gallery/` - four components, fifteen tests |
| Gates | `tests/eval/consistency_eval.rs`, `ml/eval/consistency_eval.py` |
| Budgets | `crates/aura-perf/tests/gallery_budgets.rs`, `perf/budgets.toml` |
| Executable gate | `cargo run --release -p aura-cli -- verify --phase 25` |
| In the product's own voice | `docs/gallery-consistency.md` |

**No model.** The fifth phase since 08 to ship none, and the reason is phase 17's and phase 23's
rather than phase 24's: there is nothing to train. Anchor selection is a ranking over numbers other
phases produced, the solver has a closed form, the change-point detector is a two-sample statistic
and the outlier detector is a threshold. What is missing is not weights but weddings.

**No cloud call.** Section 7 says the gateway stays idle, and `aura-cloud` is absent from this
crate's `Cargo.toml` - a property of the dependency graph rather than a rule somebody follows.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | A whole wedding reads as one coherent gallery, verified by before/after strips | **Partial** | The strips exist and the spreads are measured (78 % warmth, 80 % exposure on fixtures). Whether a photographer would call the result coherent is **unmeasured** - condition C3 |
| 2 | Each scene is anchored to its best frames, and the user can pin anchors | **Met** | `anchors::select`, `Gallery::pin_anchor`; gate step 8 pins an unchosen frame and re-runs the whole pass |
| 3 | The same person's skin looks consistent from morning to late reception | **Not met in this build** | The arithmetic is measured at 0.16 dE00 against a 2.0 ceiling on authored readings. No photograph in this build has a skin region - condition C2 |
| 4 | Intentional lighting changes survive normalisation | **Met** | Gate step 10 and eval gate 6: a 2,400 K flash splits the node and the halves stay 2,400 K apart afterwards. Intentionally-lit frames are excluded from anchoring and from movement entirely |
| 5 | Outliers are listed with quantified deviations, ready for QC | **Met** | `Outlier::describe`, `gallery_outliers`, `OutlierList`; gate step 13 finds exactly the one authored stray |
| 6 | Running consistency twice changes nothing | **Met** | Gate step 5 over 138 stored frames; eval gate 4; `store_and_pins.rs` |

---

## 3. Section 10.1 gates

Every one of them is measured in `tests/eval/consistency_eval.rs` and runs as an ordinary test.

| Gate | Threshold | Measured |
|---|---|---|
| Within-scene WB spread reduced | >= 60 % | 78 to 90 % across three scenes |
| Within-scene exposure spread reduced | >= 50 % | 80 % |
| Per-identity skin dE00 spread | <= 2.0 | 0.16 on authored readings |
| Idempotence | < epsilon | 0.000000 of a bound |
| Bounds respected | no exceedance | 55 of 60 frames clamped, none exceeded |
| Intentional transitions not flattened | survive | two halves stay 2,400 K apart |
| Outliers quantified | described | "+2213 K warmer than the anchors, -0.59 EV darker" |

---

## 4. Conditions

### C1 - Every gate is measured on synthetic galleries. **Sev 2.**

There are no weddings in this repository and no labelled lighting transitions, so the drift, the
transitions and the skin wander in every fixture were authored and read back through the real code.

What that proves: the tree construction, the sub-clustering, the change-point statistic, the anchor
ranking, the robust statistics, the solver, the five bounds, the idempotence, the skin arithmetic,
the outlier threshold, the store and the whole assembly.

What it does not prove: that any number here describes a photograph.

**It closes with phase 05's condition C10 rather than separately.** The anchor ranking multiplies
phase 15's white-balance confidence by phase 06's identity prominence, and both are
placeholder-backed - phase 06's detector finds no faces and phase 15's learned illuminant hypothesis
is never generated. "The best-judged frames" is therefore a claim about the *ranking* and not about
which photographs are best.

Trigger to reopen: the first real wedding with phases 06, 07, 15 and 16 run over it.

### C2 - No photograph in this build has an identity-scoped skin region. **Sev 2.**

`aura_brain_gallery::SKIN_FIELD_AVAILABLE` is `false`. Phase 18's `SEG_HEAD_TRAINED` is false, so
`MaskService::ensure` produces no identity-scoped `MaskKind::Skin` region and there is nothing for a
correction to apply inside. `AppState::gallery_pass` attaches no skin field, every frame records
`GalleryCode::SkinMaskAbsent`, and `gallery_skin_target` is empty on every real project.

Three things make this a *visible* gap rather than a silent one:

* `SkinMaskAbsent` and `SkinTargetAbsent` are separate codes with separate runbooks. One says the
  product could not look; the other says it looked and the person was not in enough well-lit frames.
  Phase 24's rule.
* `skinFieldAvailable` is on the wire, and `ConsistencyView` renders a sentence rather than a zero.
  A panel that inferred it from `skinTargeted == 0` would eventually say "everybody's skin is
  consistent across this wedding" for a build that cannot look at skin.
* `ml/eval/consistency_eval.py` prints `NOT MEASURED` rather than `PASS` for a project with no skin
  targets, and its self-test asserts that.

**Section 6.3's promise is measured on five wanderings of a chromaticity, not on five people.**
`docs/skin-fairness.md` says the same thing in the product's own words. No per-skin-tone figure is
published and none should be inferred.

Trigger to reopen: phase 18's segmentation head trained, or any consented data that produces an
identity-scoped skin region.

### C3 - The perceptual audit did not happen.

Section 9 gives QAIQ "full-gallery review of 5 weddings before/after; hunt for flattened mood", 4
days. It has not been run, because there are no weddings.

**This is the phase's own headline KPI.** Section 0 states it as "a whole wedding reads as one
coherent body of work", and nothing measured here answers it: a spread reduction is a number about
kelvin, and coherence is a judgement about a gallery. The two are related and are not the same, and
the specific failure this audit exists to catch - a gallery that is *more* uniform and *less* alive -
would show as a **better** number on every gate above.

The four structural defences against it are real and are not a substitute: damping below one, hard
bounds, change-point splitting, and the exclusion of intentional light from both anchoring and
movement.

Trigger to close: a blind before-and-after review of five weddings by a photographer who did not
build this.

### C4 - The change-point detector has never seen a labelled transition.

Section 9 gives DATA "label intentional lighting transitions on fixture weddings for change-point
validation", 3 days. The fixtures' transitions are authored, so the detector is measured against
boundaries it was given rather than found.

Two specific unknowns:

* **The false-positive rate on a real reception.** A room with people walking in front of lights
  produces a signal this detector has never been shown. Its two rules - a step the trend does not
  explain, and a span no target can cover - are both conservative, and `MAX_SPLITS` caps a node at
  seven parts as a bug detector, but neither is evidence.
* **The false-negative rate at a slow venue change.** Walking from a foyer into a ceremony space
  over thirty seconds is a ramp rather than a step, and it is caught by the span rule only if the
  total range exceeds two bounds. A 700 K move over a minute would be normalised rather than split,
  which is the correct behaviour and is also indistinguishable from missing it.

Not Sev 2, because the failure is bounded in both directions: an over-eager split produces
unanchorable nodes that normalise nothing, and a missed split produces a node whose distant frames
are clamped and reported as outliers. Both are visible and neither damages a photograph.

Trigger to close: labelled transitions on any real wedding.

---

## 5. What is deliberately absent

**No incremental whole-project re-solve.** Section 11 budgets 6 s for a re-solve after one anchor
change; what ships re-solves that node and no other, at 1 ms. That is a property of the structure -
a node's target depends on its own anchors and nothing outside it - rather than an optimisation.

**No cross-node harmonisation.** Two adjacent nodes of the same segment are normalised
independently. That is what change-point splitting is *for*: a boundary exists because the light
genuinely changed, and harmonising across one would undo it.

**No batch accept of deltas**, despite section 9 giving MFE "batch accept". A delta is not a thing
to accept - it is already stored, and what a photographer accepts is the anchor choice that produced
it. Four hundred rows saying somebody looked at something they scrolled past would be worse than
none. What ships is multi-select pinning.

**No node editing.** A node is a lighting group the product measured; a chapter is the photographer's
narrative, and phase 07's `StoryService` already has `split_segment`, `merge_segments` and
`set_chapter`. A second editable tree would be a second answer to what a wedding's shape is.

**No apply.** There is no command that writes a recipe. `aura-app` merges an accepted delta through
`aura_recipe::schema::merge`, and `crates/aura-brain-gallery/tests/no_recipe_writes.rs` is the sixth
grep-as-a-test in the repository.

---

## 6. Rollback

* **Feature flag off:** `Gallery::set_enabled(image, false)` per frame; a whole project is left
  untouched by simply not running `gallery_pass`, and every frame keeps exactly the per-frame answer
  phases 15 and 16 gave it.
* **Previous policy pinnable:** `consistency.toml` carries a `version`; bumping it back re-solves
  every row under the old table.
* **Migration reversible:** DROP five tables, two views and three triggers. Nothing earlier
  references them, so a downgrade loses this phase's decisions and no other.

---

## 7. Regression

`cargo test --workspace --all-targets` is green. `cargo clippy --workspace --all-targets -D warnings`
is clean. `scripts/check-banned.sh` reports clean. `cargo run -p xtask -- contracts --check` reports
70 entries, all locked. `cargo run -p aura-cli -- verify --phase 25` exits 0. The UI type-checks and
its 15 new tests pass. The IPC surface is 199 handlers, 199 registrations and 199 client wrappers.

No acceptance criterion from an earlier phase regressed.

**Three of those checks were red on `main` when this branch was cut**, and all three were phase 24's:
`cargo fmt` failed on fifteen files including a frozen contract, `cargo clippy -D warnings` failed
with 122 errors in `aura-generative`, and `aura-cloud` had no test-lint exemption for the first
inline test module it acquired. `docs/progress/PHASE-25.md` records what was changed and why. The
frozen contract re-lock is the one worth remembering: `cargo xtask contracts` hashes bytes, and a
formatter changes bytes.

---

## 8. Inherited conditions still open

Phase 25 closes none of them and depends on three.

| From | Condition | Bearing on this phase |
|---|---|---|
| 02 | C1-C3 - no camera files, no ColorChecker, no CI matrix | Unchanged |
| 05 | C10 - the embedding carries no wedding semantics | C1 closes with it |
| 06 | C1 - the face models are placeholders | The anchor ranking's identity term is inert |
| 15 | C1 - both tone heads are placeholders | Every input to the solver comes from them |
| 16 | C1 - the tone head is a placeholder | The grade half harmonises values from a deterministic solver |
| 18 | C1, C2 - the segmenter is a placeholder, no artefact audit | **C2 of this phase is the same gap** |
| 24 | C1, C2 - no mask vocabulary for a ring, no trained detector | Unchanged |
