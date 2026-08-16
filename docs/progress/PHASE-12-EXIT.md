# Phase 12 exit report - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing

**Date:** 2026-08-16  
**Branch:** `feat/phase-12-culling-engine-coverage`  
**Gate:** `cargo run --package aura-cli -- verify --phase 12 --work target/phase12-verify`
exits 0  
**Verdict:** the implementation is **conditionally complete**. Seven evidence conditions
remain open; C1 is a Sev 2 release trigger. No number below is presented as a real-wedding,
photographer, GPU or trained-model measurement.

---

## 1. What shipped

One feature: a decision engine that turns a wedding into a gallery, with a guarantee that
the wedding's story survives the decision, and a reason for every photograph on both sides
of the line.

| Area | What landed |
|---|---|
| Contract | Three modes, twelve must-haves, three coverage states, twenty-four typed reason codes, keep/rejection/report/outline/service shapes, three provenance versions plus a config digest |
| Fusion | Weighted geometric mean in log space, three hard vetoes read off phase 09 measurements, four documented confidence penalties, calibration machinery at the identity map |
| Passes | Moment winners with diversity-driven `k` and peak protection; chapter quotas with a bounded local search; the coverage guard, twice; three diversity caps; size reconciliation |
| Rules | 22 scene weight rows and three mode rows in `cull_weights.toml`; twelve guarantees, per-identity minimums, nine chapter bands, diversity caps and the veto policy in `coverage_rules.toml`; every row carries a written rationale |
| Persistence | Migration 12: five tables, two views, an override table that survives a rebuild, reasons stored as codes |
| Integration | Six frozen services, five optional, none of their crates depended on |
| Application | Seven typed commands, native desktop registration, all off the renderer thread |
| Cull UI | Coverage panel with the three states as words, size slider that is honest about overshoot, rejection drawer with a runner-up and a one-click override |
| Gate | `aura-cli verify --phase 12`, a 24-test Rust harness, a Python agreement harness with its own self-test, and two asserted performance budgets |
| Operations | ADR-0025, ADR-0026, `docs/how-aura-culls.md`, six runbooks, four label files, changelog, progress log and this report |

The phase 12 error range is `AURA-ML-5048` through `AURA-ML-5053`; every registered code
has a runbook. **Originals remain read-only and nothing in this phase deletes, moves or
renames a file.** A rejection is a row.

## 2. Acceptance criteria

| # | Section 13 criterion | Status | Evidence |
|---|---|---|---|
| 1 | Clicking Cull on a 3,000-image wedding produces a complete gallery in under 8 minutes | **the selection passes are met; the analysis budget is C5** | The six passes run over 4,000 analysed frames inside section 11's 1.5 s, asserted by `cull_budgets.rs`. The eight-minute figure is the *analysis* - phases 06 to 11 - on an RTX 4070 this build does not have. |
| 2 | The coverage panel shows every must-have as covered, weakly covered or genuinely missing | **met** | Twelve rows on every culled project, rendered as words with an explanation. The gate asserts `missing` appears exactly where no candidate existed and never where one did. |
| 3 | Every decision is explained, and every keeper offers a runner-up to compare | **met** | `SelectionResult::is_explained` is asserted on every fixture in all three modes; runner-ups are computed against the finished gallery, so `None` means every alternative was delivered rather than that none was found. |
| 4 | Moving the size slider instantly re-selects without breaking coverage | **met** | 66-68 ms per move at 500, 800 and 1,100 against a 2,000 ms budget, coverage intact at every step. |
| 5 | Two runs on two machines with the same inputs produce byte-identical selections | **met on one machine; two machines is C7** | All four fixtures reproduce their hash and their exact keeper and rejection lists. The engine is pure - no clock, no database, no network, `BTreeMap` only, every sort tie-broken on the photo id - which is what makes this a unit test rather than a field report. A second architecture has not been run. |
| 6 | Blind-study agreement meets the gate and the report is archived | **synthetic agreement met; the blind study is C4** | 0.929 to 0.958 against a 0.85 gate on four synthetic weddings with authored labels. Four real weddings culled by photographers do not exist. |

## 3. Phase-specific quality gates

Measured by `tests/eval/cull_eval.rs` (24 tests) and the release verifier.

| Gate | Threshold | Result | Measured against |
|---|---:|---:|---|
| Photographer agreement, Jaccard at moment level | >= 0.85 | **0.929 / 0.939 / 0.938 / 0.958** | four synthetic weddings with a documented scene-relative label model |
| Missed must-haves where candidates exist | 0 | **0**, in all three modes, on all four weddings | fixture ground truth |
| Coverage claimed where no candidate exists | 0 | **0** | the elopement fixture reports six rules missing, honestly |
| Every close-family identity appears >= 3 times | exact | **met**, bounded by what was photographed | fixture casts |
| Determinism: identical hash and identical selection | exact | **met** on all four | two runs of the same question |
| Slider 500 to 1,200 re-selects and never breaks coverage | <= 2 s | **68 ms worst** | the dance-heavy fixture |
| Aggressive mode satisfies every coverage rule | exact | **met** | all four weddings |
| Every rejected frame has a reason | exact | **met** | 24-test harness, all modes |
| The closed-eye veto cannot fire on a kiss | exact | **met, structurally** | the loader refuses a rule table that lists `kiss` as posed |
| Keeper rate | 22-38 % band | **28.6 % to 45.6 %** across modes; **35.8 % to 38.0 %** in `Balanced` | section 6.4's stated band, which describes `Balanced` |

## 4. Performance

| Row | Budget | Result |
|---|---:|---:|
| Selection passes over 4,000 analysed images | <= 1,500 ms | asserted, met in release; reported in debug |
| Slider re-selection | <= 2,000 ms | 66-68 ms measured on the fixtures |
| Stored selection per 1,000 images | <= 700 B (this phase's own row) | asserted by SQLite page accounting |
| Full analysis + cull, 3,000 images, RTX 4070 | < 8 min | **waived - C5** |
| Full analysis + cull, 3,000 images, M3 Pro | < 14 min | **waived - C5** |

The two waived rows budget the *analysis*, which is phases 06 to 11 on hardware this build
does not have. ADR-0007 waives every GPU row in this product and each of those phases
carries its own share of the eight minutes.

## 5. Open conditions

**C1 - Sev 2. Every sub-score underneath every decision comes from a placeholder head.**
Phase 06's detector finds no faces, phase 09's focus head describes a random projection,
phase 10's expression head says nothing about faces and phase 11's aesthetic head is
untrained. The arithmetic in this phase is real, measured and tested; the numbers it works
on are not yet claims about photographs. **No figure in this phase may be presented as a
quality result about a real wedding until phase 05's condition C10 closes**, and this
condition closes with it rather than separately.

**C2 - the per-scene calibration is the identity map.** Section 6.1 asks for isotonic
calibration so that a keep score means the same thing in every scene. Fitting it needs
labelled keeper/reject pairs from real weddings. `KeepScore::calibration_ver` is `0` so a
fitted table can never be confused with an unfitted one, and `Engine::with_calibration`
exists so that adding one is a data change rather than a design change.

**C3 - the gallery-size regression is authored, not trained.** Section 6.4 asks for a fit
over sixty real delivered galleries. What ships has the same feature vector, an output
clamped into section 6.4's own 22-38 % band, and a written argument for the sign of every
coefficient. The slider exists precisely so that a wrong prediction costs one drag.

**C4 - the blind study of section 13 does not exist.** Four weddings culled by hand by
photographers, compared against the engine. The harness is built and self-tested
(`ml/eval/cull_agreement.py --self-test`), the label format is fixed, and the four
synthetic label files are checked in. The people and the weddings are not here.

**C5 - the two end-to-end performance rows are waived.** Carried forward from ADR-0007 and
from phases 03, 09, 10 and 11. They expire when a GPU backend and the three reference
machines exist.

**C6 - the cloud tie-breaker of section 7 was not built.** Deliberately, and this is the
condition worth reading twice.

Section 7's trigger is "top-2 candidates within 0.02 `keep_score` AND the moment is
significant". Those two scores are the product of four placeholder heads. A difference of
0.02 between two such numbers is noise, not a tie - so every call the trigger fired would
spend a photographer's money asking a vision model to arbitrate between two random
projections, and would then record the answer in an audit trail as though it meant
something.

The offline fallback section 7 specifies - "higher subject sharpness, then peak proximity,
then earlier timestamp" - is what ships, and it is what the deterministic tie-break in
`moment_pass` already does. The task will be built when the trigger can fire on real
numbers. Nothing in the pipeline is stubbed for it: `CloudTask` is phase 04's contract and
adding `CullTieBreak` touches no frozen shape in this phase.

**C7 - two machines, and a real desktop audit.** Determinism is asserted twice on one
machine and one architecture. The cull view is covered by 16 component tests and has not
been looked at on a real screen by a person.

## 6. Rollback

| Layer | Switch |
|---|---|
| Feature | Do not call `cull_project`. Nothing else in the product reads `selection`; phases 14, 27, 29 and 30 do not exist yet. |
| Catalog | Migration 12's down script is a list of drops, in the migration header. **Export `cull_override` first** - it is the only thing in the migration that is not recomputable, and it is deliberately the smallest and most portable table there: two ids, one verb and a timestamp. |
| Config | Both TOML tables are versioned and embedded; an installation override can be removed and the shipped table returns. |
| Contract | `contracts.lock` covers `cull.rs`, migration 12, the IPC surface and `ui/src/ipc/types.ts`. Changing any of them needs an ADR and a re-lock, in that order. |

## 7. What this phase adds that every later phase inherits

* **`CullService` is the only way to ask what is being delivered.** Eighth phase, eighth
  time. Phase 14 edits survivors, phase 27 swaps in runner-ups, phase 29 builds albums from
  keepers and phase 30 uploads them; two answers to "what is in this gallery" is a delivery
  that does not match the album that does not match the invoice.
* **A decision is reversible, and nothing on disk moves.** There is no path column, no file
  operation and no `deleted` flag anywhere in migration 12 or the IPC surface.
* **A guarantee outranks a preference, always.** Modes, sliders, quotas and diversity caps
  are preferences; must-haves and identity minimums are guarantees. `modes.rs` cannot see
  the rule table, so section 10.1's "Aggressive mode still satisfies all coverage rules" is
  a property of the type system.
* **Say what the gallery was chosen *from*.** `CullOutline::coverage` is the fraction of the
  project that carried a technical verdict, and it is the most consequential denominator in
  the product: a cull over 60 % of a wedding is a gallery with a four-hour hole in it that
  looks exactly like a gallery with a decision in it.

## 8. Conditions carried forward from earlier phases

Phase 02's three exit conditions (real camera files, a photographed ColorChecker, a
three-OS CI run) remain open, as does phase 05's C10 - the placeholder embedding - which
C1 above is bound to. Phase 06's C1 and C5, phase 07's C1 and C5, phase 09's C1, phase 10's
C1 and C2 and phase 11's C1 are all still open and are all upstream of every number in this
report.

**The first real camera file remains a Sev 2 trigger that reopens phase 02's criteria
whatever phase is in flight.**
