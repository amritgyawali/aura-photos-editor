# Phase 12 progress - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing

One row per implementation lane. The measured verdict and open evidence are in
`docs/progress/PHASE-12-EXIT.md`.

| Task | Deliverable | Status and evidence |
|---|---|---|
| PM/CTO - freeze the vocabulary and the guarantees | `aura-core::contract::cull`, `coverage_rules.toml`, ADR-0025 | Complete. Three modes, twelve must-haves, three coverage states, twenty-four typed reason codes, keep/rejection/report/outline/service shapes, three provenance versions plus a config digest. Ten contract spellings are recorded in ADR-0025 section 2. External photographer sign-off on the rule set remains a condition. |
| MLL - fusion, vetoes and calibration | `cull/fusion.rs`, `cull_weights.toml` | Complete as arithmetic. Weighted geometric mean in log space, so no signal can rescue another; three hard vetoes read off phase 09 measurements rather than re-derived; confidence penalties for four kinds of missing input. The per-scene isotonic calibration ships as the identity map at `calibration_ver = 0` (C2). |
| SRC - moment pass | `cull/moment_pass.rs` | Complete. `k` from moment diversity and significance, capped per scene and per mode; near-identical suppression from phase 08's duplicate sets with a pointer to the frame that won; peak protection with an explicit `peak_rejected` when a peak loses. |
| MLR - chapter quotas and local search | `cull/chapter_pass.rs` | Complete. Section 6.2's square-root volume formula with per-chapter importance and min/max bands; a bounded improvement pass that trades a chapter's weakest second keeper for another moment's first, capped at 512 swaps. Measured, the largest fixture fires far fewer. |
| SRC - coverage guard | `cull/coverage.rs`, `cull/rules.rs` | Complete. Twelve declarative rules matched on scene *or* interaction; force-add with `covered_weak`; `missing` only when no candidate existed; per-identity minimums from the phase 06 hierarchy; the guard only ever adds, so running it twice is safe and idempotent. |
| SRC - diversity and sizing | `cull/diversity.rs`, `cull/sizing.rs` | Complete. Three sliding-window caps - total, framing bucket, dominant identity - none of which may touch a protected frame. The gallery-size model has section 6.4's feature vector, an output clamped into the stated 22-38 % band, and authored rather than fitted coefficients (C3). |
| SRC - modes and determinism | `cull/modes.rs`, `cull/explain.rs`, `cull/engine.rs` | Complete. `Tuning` has four fields and none of them is a rule, so "Aggressive cannot drop a must-have" is a property of the type rather than a review comment. The determinism hash covers inputs *and* both config digests; two runs reproduce byte-identically on every fixture. |
| SRC - persistence | `cull/store.rs`, migration 12 | Complete. Five tables and two views; the selection is rebuilt wholesale in one transaction while `cull_override` is untouched, which is why the photographer's decision lives in its own table rather than in a column. Reasons store codes and only store text when it differs. |
| SRC - integration | `cull/gather.rs`, `cull/api.rs` | Complete. Six frozen services, five of them optional, none of their crates in `Cargo.toml`. Resize, mode change and override all re-run the same six passes, so a guarantee holds for all three by construction rather than by three separate checks. |
| SFE/MFE - app and cull view | `aura-app::cull_commands`, IPC DTOs/client, `CullView`, `SizeSlider`, `CoveragePanel`, `RejectReasons`, ADR-0026, Tauri wrappers | Complete in unit scope. Seven typed commands; `null` is "nobody has decided"; the three coverage states are words rather than colours; an unanalysed frame offers no override; nothing on the surface can delete, move, export or upload. Component suite is 16 tests. A real desktop visual audit remains C7. |
| QAL - gates | `tests/eval/cull_eval.rs`, `aura-cli::phase12` | Complete. The Rust harness has 24 deterministic tests; the CLI gate exercises migration 12, both config tables, all three modes on four weddings, agreement, determinism, the slider budget and a full store round trip including an override that survives a re-selection. |
| DATA - keeper labels | `tests/fixtures/labels/keepers_*.json`, `crates/aura-cull/examples/emit_labels.rs` | Complete for the synthetic corpus. Four labelled weddings with a documented, scene-relative label model, emitted deterministically so a change to the ground truth appears as a diff. Human labels for eight real weddings do not exist (C4). |
| PERF | `crates/aura-perf/tests/cull_budgets.rs`, `perf/budgets.toml` | Implemented. The two rows this phase owns - the passes over 4,000 frames and a slider move - are asserted, plus a storage row section 11 does not state. The two "full analysis + cull" rows name GPUs this build does not have and retain ADR-0007's waiver (C5). |
| AGT - cloud tie-breaker | section 7 | **Not built.** See the exit report, section 5: the trigger is two candidates within 0.02 of each other, and the sub-scores that would produce those two numbers all come from placeholder heads, so every call would be a paid question about noise. Recorded as C6 rather than stubbed. |
| DOC/OPS | two ADRs, six runbooks, `docs/how-aura-culls.md`, changelog and exit report | Complete. All six registered phase 12 errors have recovery pages, and the public reason vocabulary is guarded by a contract test that fails when a code ships without a sentence. |

## Defects found while completing the gate

1. **The size reconciliation could put back a frame the photographer had removed.** The
   pool it drew from was every eligible frame, so a gallery that came out short of its
   target could re-add a rejection to hit the number - silently overruling the one decision
   in this phase that is supposed to be unbeatable. The pool now excludes them, and
   `a_forced_reject_can_degrade_a_guarantee_but_never_silently` is the test that would have
   caught it.

2. **`runner_up` was computed too early.** The moment pass named the best non-winner, but
   four passes ran afterwards and that frame was often delivered itself - so a third of the
   keepers offered an "alternative" that was already in the gallery. It is now computed
   against the finished selection, and `None` is the honest answer when every alternative
   was delivered.

3. **The fixtures compressed a whole wedding into twenty minutes.** Moments were four
   seconds apart, so the diversity pass's two-minute window saw thirteen of them at once and
   culled entire moments out of the gallery. Agreement on `hindu_night` was 0.786. Scene-
   dependent gaps - twenty seconds on a dance floor, ninety in a ceremony - fixed both the
   fixture and the measurement, and `gap_for` now documents why the number matters.

4. **The first agreement label model could not fail.** "One frame from every moment" is
   also roughly what the engine produces, so the gate returned 1.000 on every wedding and
   measured nothing. The second attempt - the top 30 % of frames by quality, globally - was
   worse: it scored 0.50 to 0.63 because it deleted whole chapters shot in bad light, which
   is the failure this engine exists to prevent. The shipped model is scene-relative and
   skips the weakest tenth of each scene's moments; it lands at 0.929 to 0.958 with real
   margin above the 0.85 gate and it can move.

## What was deliberately not built

* **The cloud tie-breaker of section 7.** C6 in the exit report.
* **Anything that removes a file.** Migration 12 has no path column, the IPC surface has no
  delete command, and `CullView` has no control that could reach one.
* **Zero-Touch.** Section 2.1 names it and section 2.2 puts it in phase 28; `CullMode` has
  three variants and not four.
* **Hero, album and social picks** (phase 29), **QC replacement** (phase 27) and **editing
  the survivors** (phase 14).
