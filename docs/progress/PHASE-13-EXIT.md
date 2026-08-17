# Phase 13 exit report - Explain My Edit, Confidence Calibration & Decision Ledger

**Date:** 2026-08-17  
**Branch:** `feat/phase-13-explainability-confidence-ledger`  
**Gate:** `cargo run --package aura-cli -- verify --phase 13 --work target/phase13-verify`
exits 0  
**Verdict:** the implementation is **conditionally complete**. Six evidence conditions
remain open; C1 and C2 are Sev 2 release triggers. No number below is presented as a real
photographer, real wedding, GPU or trained-model measurement.

---

## 1. What shipped

One feature: every decision the product makes can be opened up - why, how sure, what it
looked at - and every one of them is recorded in a ledger that cannot be rewritten.

| Area | What landed |
|---|---|
| Contract | Six decision kinds, four subjects, four evidence variants, four autonomy bands, three sources, the reason, the decision, the outline, the service, and `DecisionId` |
| Ledger | Append-only, enforced by a database trigger; corrections supersede; compaction bounded by a policy that cannot remove a photographer's own decision |
| Calibration | Isotonic by pool-adjacent-violators, temperature by bounded scan, ECE over ten bins, Brier, reliability bins, an SVG diagram, and a CI gate in both directions |
| Autonomy | Section 6.4's bands verbatim in a PM-owned file, five per-kind rows with written reasons, two named risk multipliers and a third this build argues for |
| Reasons | 93 codes assembled from four frozen vocabularies plus five of this phase's own; an unregistered code is refused rather than stored |
| Explanations | A deterministic template that is correct by construction, and a cloud task with no field a new reason could go in and a validator that refuses an invented number |
| Replay | A port rather than a dependency, distinguishing a determinism defect from an upgrade |
| Support | An anonymised bundle with every identifier replaced, scanned for keys and paths |
| Persistence | Migration 13: three tables, one trigger, one view, three indexes |
| Application | Eight typed commands, native desktop registration, all off the renderer thread |
| Explain UI | Six tabs, two of which say why they are empty; top three reasons with "show all"; evidence crops; the alternative comparison with both breakdowns |
| Gate | `aura-cli verify --phase 13`, a 30-test Rust harness, four asserted budgets, and a Python calibration harness with its own self-test |
| Operations | ADR-0027, ADR-0028, `docs/reason-codes.md`, `docs/how-confidence-works.md`, six runbooks, changelog, progress log and this report |

The phase 13 error range is `AURA-ML-5054` through `AURA-ML-5059`; every registered code has
a runbook. **Nothing in this phase decides, edits, deletes or delivers anything.** It records
what the phases that do decide have done.

## 2. Acceptance criteria

| # | Section 13 criterion | Status | Evidence |
|---|---|---|---|
| 1 | Opening any image shows why it was kept or rejected, with the runner-up and score breakdown | **met** | `explain_image` returns six tabs, the recorded decision and both frames' four sub-scores. `AlternativeCompare` renders the breakdown and carries no control that could swap them; 15 component tests, one of which asserts that absence. |
| 2 | The Edit tab lists the exact parameters and masks that were applied | **structurally ready; the producer is phase 14** | `Evidence::Params` carries named deltas end to end - contract, migration, DTO, panel - and a round-trip test asserts `temperature_k -610` survives the catalog. The tab says the develop engine arrives later rather than rendering blank. |
| 3 | Confidence badges appear across the app and map to documented autonomy bands | **met on the Explain panel; the grid badge is C5** | The badge shows the band's own title, its sentence and the calibrated number, and says plainly when nothing has been calibrated. `preview_band` exists so a grid can band four thousand thumbnails without writing a ledger row; the grid itself has not been wired. |
| 4 | Calibration report is published and ECE gates pass | **met against synthetic outcomes; real outcomes are C2** | `ml/eval/calibration_report.py --self-test` passes; the gate asserts a calibrated predictor measures 0.0117 and an overconfident one 0.0959 against a 0.05 threshold, and that a fitted map brings held-out ECE to 0.0318. |
| 5 | Any decision can be replayed from the ledger with an identical result | **met** | 196 of 196 machine decisions replay identically on the gate's wedding, at under a millisecond each against a one-second budget. The 197th is the photographer's own, which has no engine to re-run and is skipped rather than faked. |
| 6 | A support bundle can be exported that contains no client imagery | **met** | 197 decisions, 394 identifiers replaced, zero forbidden strings, zero raw ids. Three guarantees and only one of them is a filter: no `Evidence` variant can hold bytes, no column stores a name or a path, and the scan runs anyway. |

## 3. Phase-specific quality gates

Measured by `tests/eval/explain_eval.rs` (30 tests) and the release verifier.

| Gate | Threshold | Result | Measured against |
|---|---:|---:|---|
| Every decision has a reason and a calibrated confidence | 100 % | **1.000** on 197 decisions | a real cull of the elopement fixture, recorded end to end |
| A decision with no reason is refused rather than stored | exact | **met** | `AURA-ML-5054`, and the ledger is left empty |
| A reason code nothing documents is refused | exact | **met** | the registry is assembled from the four frozen vocabularies |
| ECE on a calibrated predictor | <= 0.05 | **0.0117** | 4,000 synthetic outcomes with a known answer |
| ECE on an overconfident predictor | must fail | **0.0959, gate failed correctly** | 4,000 synthetic outcomes |
| Held-out ECE after an isotonic fit | <= 0.05 | **0.0959 → 0.0318** | fit on 4,000, measured on a disjoint 4,000 |
| A calibration never reorders two confidences | exact | **met** | monotone by construction, asserted across the whole range |
| Replay reproduces a stored decision | exact | **196 / 196** | the gate's own wedding |
| Replay tells a determinism defect from an upgrade | exact | **met** | two synthetic drifting sources |
| A summary invents no number | exact | **met** on 20 decisions | the template path, checked by `Grounding` |
| The grounding check catches an invented measurement | exact | **met** | a fabricated shutter speed and exposure delta |
| Ledger size per 1,000 decisions | <= 6 MB | **0.33 MB** | measured, not estimated |
| The support bundle carries no identifier | exact | **met** | scan plus a per-decision id check |
| An `UPDATE` on the ledger is refused | exact | **met** | by the database, not by the code |
| Compaction keeps the photographer's own decision | exact | **met** | even when a later machine decision superseded it |

## 4. Performance

| Row | Budget | Result |
|---|---:|---:|
| Ledger write per decision | <= 0.4 ms amortised | asserted at 4,000 decisions; met in release, reported in debug |
| Reading one decision back | <= 25 ms (this phase's own row for the panel's read) | asserted |
| Replay of one decision | <= 1,000 ms | **< 1 ms** on the gate's wedding |
| Ledger size per 1,000 images | <= 6 MB | **344 KB** measured |
| Explain panel open, with crops | <= 250 ms | **the read is 0 ms; the rendering is C5** |

The panel row is a rendering budget: it includes a web view laying out a panel and a preview
service returning crops, neither of which exists inside a Rust test. What is asserted is the
part this phase owns.

## 5. Open conditions

**C1 - Sev 2. Every decision this ledger records was made from placeholder heads.** Phase
06's detector finds no faces, phase 09's focus head is a random projection, phase 10's
expression head says nothing about faces, phase 11's aesthetic head is untrained, and phase
12 fuses all four. The ledger records those decisions faithfully, which is exactly what it is
for - and no explanation in this build is a claim about a photograph. This closes with phase
05's condition C10 rather than separately.

**C2 - Sev 2. Nothing is calibrated, and the ECE gate is measured on synthetic outcomes.**
Section 6.1 asks for per-decision-type isotonic calibration fitted on labelled outcomes plus
user overrides, with the gate applying at 500 samples. The fitter, the estimator, the
reliability diagram and the gate all exist and are tested; what they need is a corpus of
decisions somebody recorded the correctness of. `CalibrationSet::shipped` is the identity map
at version 0 for every pair, `AURA-ML-5058` says so once per run, and
`uncalibrated_raises = true` makes the product ask more often while it is true.

**The consequence is worth stating plainly: nothing in this build acts unattended.** On the
gate's own wedding all 197 decisions land in `require_review`. Phase 28 cannot ship until a
calibration does, and that is the correct order.

**C3 - the cloud summary path has a cassette but no live provider.** `ExplainSummary` is
implemented with section 7's prompt, schema, cost ceiling and validator, and cassette
`300-anthropic-explain-summary.json` exercises the happy path. TLS is waived by ADR-0009, so
this build reaches `http://` OpenAI-compatible endpoints only, and the summary a photographer
sees today is the deterministic template. That is not a degradation - the template is correct
by construction - but the cloud path has not run against a real model.

**C4 - the pixel opt-in of section 2.1 was deliberately not built.** Section 2.1 allows image
data in a support bundle "unless the user opts in". Building it would create the one code path
in the product that can put a photograph into a file which is then emailed, and nothing in
this phase needs it. Recorded rather than stubbed; adding it later needs an ADR.

**C5 - the UI is unit-tested and has not been seen.** 15 component tests cover the panel, the
reason rows, the crops and the comparison. Nobody has opened it on a real screen. The
confidence badge across the grid and the per-band review queue are wired on the backend
(`review_queue`, `preview_band`) and not yet drawn.

**C6 - the ledger has never held a full pipeline.** Section 11's 3-6 KB per image assumes six
decision kinds; five of them have no producer, so the measured 344 KB per thousand is one
kind's share. The budget holds with room at six times the load, and that is arithmetic rather
than a measurement.

## 6. Rollback

| Layer | Switch |
|---|---|
| Feature | Do not call `record_decisions`. Nothing else reads the ledger; phases 27, 28 and 30 do not exist yet. The Explain panel degrades to the four analysis tabs, which read their own services. |
| Catalog | Migration 13's down script is a list of drops, in the migration header. **Export `decisions` and `decision_reasons` first.** This is the first migration in the product whose contents are *not* recomputable: re-running the pipeline produces today's decisions, not the ones a client's gallery was built from. |
| Config | `autonomy_bands.toml` is versioned and embedded; an installation override can be removed and the shipped table returns. |
| Contract | `contracts.lock` covers `ledger.rs`, `ids.rs`, migration 13, the IPC surface and `ui/src/ipc/types.ts`. Changing any of them needs an ADR and a re-lock, in that order. |

## 7. What this phase adds that every later phase inherits

* **`ExplainService` is the only way to record what happened.** Ninth phase, ninth time.
  Phase 27 writes QC decisions here, phase 28 reads the bands, phase 30's learning loop reads
  the whole table. Two ledgers is two answers to "what did the product do", and the one thing
  a support case cannot survive is a product that disagrees with itself about its own history.
* **A decision that cannot explain itself is not recorded.** Invariant 2 stops being a
  convention: `AURA-ML-5054` refuses it, and migration 13's `reason_count` CHECK refuses it
  again if anything ever gets past the first check.
* **The record is append-only, and the database enforces it.** A correction is a new row
  pointing backwards. There is no `UPDATE` path, and the one thing that deletes is a
  compaction policy that cannot touch a photographer's own decision.
* **Confidence is two numbers and the band is stored.** A caller cannot grant itself autonomy:
  `record` overwrites whatever band it was handed, and a test proves it by recording a
  decision that claims `Auto` at 0.1.
* **Evidence can never be a pixel.** No variant of `Evidence` can hold image bytes, which is
  what makes a support bundle safe by construction rather than by filtering.

## 8. Conditions carried forward from earlier phases

Phase 02's three exit conditions (real camera files, a photographed ColorChecker, a three-OS
CI run) remain open, as does phase 05's C10 - the placeholder embedding - which C1 above is
bound to. Phase 06's C1 and C5, phase 07's C1 and C5, phase 09's C1, phase 10's C1 and C2,
phase 11's C1 and phase 12's C1 through C7 are all still open and are all upstream of every
decision this ledger records.

**The first real camera file remains a Sev 2 trigger that reopens phase 02's criteria
whatever phase is in flight.**
