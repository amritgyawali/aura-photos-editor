# Phase 11 progress - Composition & Aesthetic AI

One row per implementation lane. The measured verdict and open evidence are in
`docs/progress/PHASE-11-EXIT.md`.

| Task | Deliverable | Status and evidence |
|---|---|---|
| PM/CTO - freeze vocabulary and rules | `aura-core::contract::composition`, `composition_rules.toml`, ADR-0023 | Complete. Sixteen stable flags, twenty-six reason codes, evidence/crop/result/coverage/service shapes, neutral plus 22 scene rows, three invalidation versions, and explicit intentional-style/degradation exonerations. Eight contract spellings are recorded in ADR-0023 section 2. External photographer approval remains a condition. |
| MLR - horizon and intent | `composition/horizon.rs` | Complete on authored fixtures. Angle error gate, source/confidence, gravity hook, gradient/vanishing fallback, and intentional dutch logic. Rho coherence was added after an angle-only histogram treated repeated diagonal texture as a perfect horizon. Gravity metadata is not present in the catalog and is an exit condition. |
| SRML - keypoints | `composition/keypoints.rs`, signed `pose_keypoints` artifacts/card | Integration complete; training incomplete. The decoder validates shape and confidence and degrades visibly on absence/error. Checked-in weights are an untrained architecture fixture whose global pooling cannot establish real spatial quality (C1). |
| SRC - crop audit and placement | `composition/{crop_audit,placement}.rs` | Complete on authored reference geometry. Joint cuts, mid-limb cuts, head crops, scene-aware deliberate close crops, headroom, thirds/centre, negative space and balance all return evidence. Every portrait-class fixture publishes a preservation objective, including a non-actionable one when there is no safe room to tighten. |
| SRC - background evidence | `composition/background.rs` | Complete as the documented Phase 11 proxy: edge energy, luminance blobs, vertical head merges and colour competition. Neutral-subject colour fallback and head-first subject sampling cover the white-dress/red-sign case. Semantic exit-sign/bin/mirror/rubbish labels and phase 18 masks are not claimed (C6). |
| SRML/MLL - aesthetic and fusion | `composition/{aesthetic,score,analyse}.rs`, `ml/models/composition/`, signed `aesthetic_head` artifacts/card | Pipeline, bounded influence, pairwise trainer/evaluator/exporter and calibration machinery complete. The artifact and 4,000 photographer pairs do not exist as trained evidence; reference aesthetic is used and the result is not labelled learned (C1/C2). |
| SRC - persistence and pass | `composition/{store,api}.rs`, migration 11 | Complete. One row per reading, compact evidence JSON, atomic dismissals, current-version pending query, resumable pass, deterministic moment ranks, coverage/flag views and typed telemetry. Malformed stored evidence is returned as an error rather than defaulted. |
| SFE/MFE - app and overlay | `aura-app::composition_commands`, IPC DTOs/client, `CompositionCard`, ADR-0024, Tauri wrappers | Complete in unit/integration scope. Five typed commands; null is unanalysed; backend-derived exoneration/actionability; thirds/horizon/evidence overlays in percentages; one-note dismissal; analysis moved off the renderer thread. Component suite is 22 tests. A real desktop visual audit remains C7. |
| QAL - gates | `composition_eval.rs`, `aura-cli::phase11`, `just phase-11-verify` | Complete. The Rust harness has 37 deterministic tests after the audit additions; the CLI gate exercises migration, rules, model registry, authored geometry/intent/background/ranking, persistence, cancellation/resume and override semantics. `just` is unavailable on this machine, so its cargo command is run directly. |
| PERF | `composition_budgets.rs`, `perf/budgets.toml` | Implemented. Storage and processor-path arithmetic are asserted. The two GPU/reference-machine rows retain ADR-0023's explicit waiver until a GPU backend and the three reference machines exist (C5). |
| DOC/OPS | two ADRs, five runbooks, public reason guide, module README, research record, changelog and exit report | Complete. All registered Phase 11 errors have recovery pages and the public reason vocabulary is guarded by a contract test. |

## Defects found while completing the gate

1. **Intentional dutch texture was called a certain horizon.** The angle histogram saw a
   concentrated -43.29 degree diagonal weave with confidence 1.000. Requiring support to
   form a coherent line in rho reduced it to 0.377 and correctly exonerated the tilt.
2. **A white subject made colour competition impossible.** The algorithm returned early
   when subject chroma was zero, contradicting the red-exit-sign acceptance case. Neutral
   subjects now use saturated background energy and the subject sample starts at the
   dominant head; the fixture moves from 0.000 to 0.753.
3. **The crop threshold disagreed with its own severity.** A hip mid-limb case computed
   0.297 against a 0.30 flag threshold, producing recall 0.500 and F1 0.667. The documented
   mid-limb ratio now produces 0.304; precision, recall and F1 are 1.000.
4. **An unlocated reference pose moved the subject.** A pose with no located points still
   contributed its extrapolated person box, pulling face-only placement down. Only poses
   with located points now vote; faces remain the fallback.
5. **Successful placeholder inference was called learned.** The aesthetic artifact is an
   untrained deterministic fixture. Provenance now controls `learned`; the shipped path
   uses the explicit reference reading and `aesthetic_unavailable` caveat.
6. **Model errors and stored evidence could disappear.** Pose failures were collapsed into
   empty geometry and malformed JSON into empty arrays. Both paths now retain typed errors
   or explicit degradation instead of manufacturing a clean result.
7. **Dismissal could leave contradictory state.** Clearing a bit without rebuilding its
   explanation left the visible flag and reason disagreeing, and read-then-write admitted
   a race. The store now validates one present defect and updates the dismissal, visible
   flags, reasons, review state, and timestamp in one transaction. The measured composite
   remains immutable; ADR-0023 defines dismissal as a review projection, not a re-score.
8. **Portrait hints vanished when no action was available.** `is_actionable == false`
   discarded the preservation objective, failing the “all portrait-class frames” handoff.
   Presence and actionability are now separate facts.

## Deliberately not represented as complete

No real wedding files, external photographers, labelled demographic/cultural slices, GPU
backend, three reference machines, or recorded desktop demo are available in this
workspace. Their absence is not converted into fixture evidence. The exit report assigns
each one an owner, severity, mitigation, and closure test.
