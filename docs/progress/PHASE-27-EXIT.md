# Phase 27 exit report - AI Quality Control Agent

**Status:** implemented conditionally. Four conditions, two of them Sev 2.

Phase 28 may start. Nothing in it may claim a QC *quality* result until C1 and C2 close, and
**nothing anywhere in the product may present an empty QC result as a clean gallery** while C3
stands — which it does, on every build shipped so far.

---

## 1. What shipped

| Deliverable | Where |
|---|---|
| Frozen contract | `crates/aura-core/src/contract/qc.rs`, `TicketId` in `contract/ids.rs` |
| Decisions | `docs/adr/ADR-0055-quality-control-tickets-and-the-re-edit-loop.md`, `ADR-0056-qc-ipc-surface.md` |
| The engine | `crates/aura-qc/src/` — ten checks plus twelve modules |
| Schema | `crates/aura-catalog/migrations/0027_qc.sql` — four tables, two views, four triggers |
| Thresholds | `crates/aura-qc/config/qc_thresholds.toml` — 23 argued-over scene rows |
| IPC | `crates/aura-app/src/qc_commands.rs` — nine commands, plus `AppState::qc_frame` |
| Panels | `ui/src/components/qc/` — five components, 12 tests, mounted in `App.tsx` |
| Gates | `tests/eval/qc_eval.rs` (12), `crates/aura-qc/tests/no_pixel_ops.rs` (7), `ml/eval/qc_agreement.py` |
| Budgets | `crates/aura-perf/tests/qc_budgets.rs`, `perf/budgets.toml` |
| Executable gate | `cargo run --release -p aura-cli -- verify --phase 27` |
| In the product's own voice | `docs/how-qc-works.md` |

**No model.** The seventh phase since 08 to ship none, and the reason is neither phase 17's ("there
is nothing to train") nor phase 24's ("there is no data"). It is a *decision*: every check here is a
comparison between numbers phases 08 to 26 already measured, and a measurement finds fewer problems
rather than inventing them. `DETECTOR_TRAINED` is false and is on the wire. ADR-0055 section 3.

**One cloud call, and it cannot act.** `QcPlanner` is `Tier::Reasoning`, capped at $0.06, fires only
on a frame carrying at least three findings, and at most forty times per pass. Its output type is
`ProposedStep`, which has no path into `remedy::validate`; its `local_fallback` is an escalation. An
unreachable provider, an invalid answer, a spent budget and a cautious model all leave the
photograph in the same state.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Every delivered image is re-examined before export | **Met** | The pass walks `CullService::selection`; `QcOutline::coverage` is checked against selected frames, phase 18's denominator |
| 2 | Findings carry a code, a number, a threshold and a reason | **Met** | `QcTicket::is_well_formed`, asserted on every ticket by the gate; migration 27 refuses a ticket with no reason |
| 3 | AURA fixes what it is confident about and escalates the rest | **Met** | Eval gates 4 and 5; `may_act_unattended` is the conjunction of the confidence floor and the band |
| 4 | A fix that does not work is put back | **Met** | Eval gate 6; `realised_share` below 0.50 reverts, and gate 6b covers collateral damage |
| 5 | A better frame replaces a worse one when one exists | **Met** | Eval gate 7; four gates in order, coverage filtering before the score |
| 6 | The loop cannot thrash | **Met** | Eval gate 6b: an oscillating remediator produces one revert and an escalation, never two |
| 7 | A photographer's verdict survives the next pass | **Met** | Eval gate 8 and gate step 9; `take_decisions` plus `qc_ticket_keep_user_status` |
| 8 | The report says what was checked before it says what was found | **Met** | Eval gate 9, `report::to_markdown`, and `QcReport.test.tsx` |
| 9 | Photographers agree with the findings | **Not met** | Unmeasured. Condition C2 |

---

## 3. Section 10.1 gates

Twelve, in `tests/eval/qc_eval.rs`, all green.

| Gate | Threshold | Measured |
|---|---|---|
| Injected defects detected | >= 90 % | 21/21 |
| False tickets on a clean gallery | 0 | 0/200 |
| Every ticket well formed | 100 % | 100 % |
| Tickets per image | <= 8 | never exceeded; `MultiSymptom` above it |
| A confident finding is fixed unattended | must | met |
| A partial repair is kept, a null one reverted | must | met |
| Collateral damage reverts | must | met |
| The loop cannot thrash | 1 revert then escalate | met |
| A swap that breaks coverage is refused | must | refused before scoring |
| A dismissed finding does not return | must | met |
| The report leads with completeness | must | met |
| Storage | <= 1,500 B/image | 421 B worst case |

Plus seven architectural tests in `crates/aura-qc/tests/no_pixel_ops.rs`, six budget tests, 227 unit
tests, and 12 panel tests.

---

## 4. Conditions

### C1 — Every reading this phase judges comes from a placeholder head. **Sev 2.**

Ten checks, and every number in every one of them was produced by a phase whose model is untrained.
Phase 06's detector finds no faces, phase 09's focus head is a random projection, phase 15's and
16's heads are never consulted, phase 18's segmenter is untrained, phase 22's face recovery returns
`None` on every frame. The fixtures in `crates/aura-qc/src/fixtures.rs` are sets of **readings this
repository authored**, not photographs.

What is proved: the arithmetic, the triage, the loop's bounds, the refusals, the store and the
guarantees. What is not: that any of it describes a wedding.

**Closes with phase 05's C10** rather than separately.

### C2 — The photographer-agreement study did not happen. **Sev 2.**

Section 10.1 asks for the false-ticket rate to stay under five per cent. What is measured is zero
false tickets against **two hundred frames this repository authored as clean**, which is a test of
the thresholds against themselves. Whether a photographer looks at a finding and thinks "yes, that
frame is wrong" is the headline question of the phase and it is unanswered.

`ml/eval/qc_agreement.py` is the harness that would answer it, and it computes the rate against
findings somebody actually reviewed. On a catalog nobody has worked, it reports **NOT MEASURED**
rather than zero, for the same reason the panel does.

**Closes when** five weddings have been worked through the queue by their own photographers and the
rate is reported per category.

### C3 — Most checks skip, so an empty result is not a clean gallery.

`QcOutline::inspection_completeness` is well below one on any real project this build could produce,
because the skin, mask, crop and sharpness checks depend on inputs that do not exist yet. This is
reported honestly in five places — the outline, the report, the Markdown export, the category chips
and the panel headline — and `QcReport.test.tsx` asserts a clean gallery is never claimed while
anything was skipped.

It is listed as a condition rather than as a design note because **a future contributor who renders
a skip as a pass would silently turn this feature into a liability**, and the tests that stop them
need to be visible in this list.

**Closes with** phases 06 and 18's model conditions.

### C4 — The planner has never reached a provider.

TLS is waived (ADR-0009), so no public reasoning-tier endpoint is reachable from this build. What is
tested is the schema validator, the one repair retry, the cost ceiling, the escalating fallback and
the fact that `ProposedStep` cannot become a `Remedy`. No recorded cassette of a real answer exists,
so nothing is known about whether a reasoning model produces useful plans for a multi-symptom frame.

**Closes when** a cassette of a real answer is recorded and the plan quality is measured against a
human's plan for the same frames.

---

## 5. What is deliberately absent

**No `qc_apply` command.** A remedy is applied by `qc_run` with `remediate: true`, or by
`qc_decide` with `applyRemedy` on one finding. A third route would be a third place the autonomy
band could be bypassed.

**No threshold on the IPC surface, in either direction.** Nothing on the wire can read or write a
threshold. A studio edits `qc_thresholds.toml`, the loader holds it to the ceilings the code owns,
and a file that loosens one is refused. ADR-0056 section 8.

**No bulk remedy authorisation.** ADR-0056 section 5, asserted in `TicketQueue.test.tsx`.

**No PDF.** Section 2.1 says "PDF/Markdown". Markdown ships; a PDF writer is a font stack, a layout
engine and a dependency, for a document whose whole content is a table. ADR-0055 section 10.

**No re-run of phases 15, 16 and 19 inside the loop.** `Remediator` is a trait, and in this build
`AppField` supplies no implementation that re-solves for real — so `qc_run` with `remediate: true`
records what it *would* do. Wiring the three deciding passes behind the trait is phase 28's
orchestration and changes no shape frozen here.

---

## 6. Rollback

* **Feature flag off:** never call `qc_run`. Nothing else in the product reads a QC row.
* **Previous thresholds pinnable:** `qc_thresholds.toml` carries a `version`; bumping it back
  re-inspects every frame under the old table, and `AURA-ML-5141` refuses a comparison across two.
* **Migration reversible:** DROP four tables, two views and four triggers. Nothing earlier
  references them.

---

## 7. Regression

`cargo test --workspace --all-targets` is green — 160 test binaries, no failures.
`cargo clippy --workspace --all-targets -- -D warnings` is clean. `scripts/check-banned.sh` reports
clean. `cargo run -p xtask -- contracts --check` reports 74 entries, all locked.
`cargo run --release -p aura-cli -- verify --phase 27` exits 0, and so do the phase 25 and phase 26
gates. The UI type-checks and its 376 tests pass. The IPC surface is 220 handlers, 220 registrations
and 220 client wrappers — **asserted by a gate for the first time**, in section 11 of `phase27.rs`.

No acceptance criterion from an earlier phase regressed.

**One pre-existing test was fixed rather than worked around.**
`crates/aura-app/tests/index_contract.rs::the_query_event_carries_the_filter_kind_and_not_the_filter`
sliced the `IndexEvent` declaration out of `types.ts` by searching for `"\n\n"`. `types.ts` is
`text=auto eol=lf` in `.gitattributes` and is nevertheless CRLF in a Windows working tree, so the
search never matched, the slice ran to the end of the file, and the test failed on a `cameraId`
three hundred lines away that phase 26 had added. It passes on CI because CI checks out LF. **A
separator that only exists on one platform is not a separator**, and a test that reads a file's
layout has to read it the way the platform wrote it.

---

## 8. Three things this phase got wrong first

**An escalated ticket was still eligible for a second round.** `TicketStatus::is_open()` is true for
`Open`, `Escalated` and `Reverted`, and the triage and the loop both read it — so a finding already
handed to a person consumed its second attempt. Gate 6 caught it; every unit test passed, because
each exercised one round. **A predicate named for one question was reused for a second one it
answers wrongly**: "is this finding outstanding" and "may automation still act on it" are not the
same question.

**Two greps matched the prose documenting the rule they enforce.** A test asserting the skin module
holds no fixed skin target failed on its own test name. Then the gate's schema scan failed on
migration 27's four paragraphs explaining why there is no `diagnosis` column — `sqlite_master.sql`
stores a migration verbatim, comments included. Both strip comments before scanning now. **A check
that reads documentation as if it were code fails hardest on the codebases that document themselves
best.**

**Two readings could not be honestly filled, and the struct had to say so.** `ExposureReading`
originally carried `subject_luma: f32` and `shadow_headroom: f32`. Nothing in the product stores the
luminance a finished frame actually landed on — phase 15 stores the band it solved toward and phase
25 the move it still owes — and no frozen contract carries a finished frame's remaining shadow room.
Both are `Option` now, and both are `None` in this build, so those halves of the exposure check
skip. Filling them from a proxy would have reported every frame as sitting exactly on its target,
which is a clean bill of health nobody measured. Same fix, and the same reason, as
`NodeReading::frame_signature`.

---

## 9. Inherited conditions still open

| From | Condition | Bearing on this phase |
|---|---|---|
| 02 | C1-C3 — no camera files, no ColorChecker, no CI matrix | Every reading this phase judges was produced from synthetic pixels |
| 05 | C10 — the embedding carries no wedding semantics | The duplicate check reads a dhash rather than the vector, so it is the one check unaffected; C1 closes with this |
| 06 | C1 — the face models are placeholders | The skin, crop and retouch checks skip on nearly every frame |
| 09 | C1 — the focus and eye heads are placeholders | The sharpness check reads `subject_sharpness` from them |
| 13 | C2 — nothing in this build is calibrated | `uncalibrated_raises` moves every QC decision one band toward review, so **nothing in this phase acts unattended on a real build** |
| 15 | C1, C2 — the tone heads are placeholders; fairness is measured on reflectances | The exposure and consistency checks read phase 15's stored answers |
| 18 | C1, C2 — the segmenter is untrained and no artefact audit ran | The mask check skips entirely |
| 22 | C1, C2 — face recovery never runs; the identity constraint is measured with an untrained recogniser | `identity_drift` is zero on every frame because no face was recovered |
| 24 | C1, C2 — no mask word for a ring or a cake; no distraction detector | The cleanup check has nothing to inspect, because nothing is ever removed |
| 25 | C2 — `SKIN_FIELD_AVAILABLE` is false | The skin check's per-identity half reads phase 25's corrections, which do not exist |
| 26 | C2 — every bundled brand baseline was fabricated | The consistency check judges frames a fabricated transform may have moved |
