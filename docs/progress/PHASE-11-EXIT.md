# Phase 11 exit report - Composition & Aesthetic AI

**Date:** 2026-08-15  
**Branch:** `feat/phase-11-composition-aesthetic-ai`  
**Gate:** `cargo run --release --package aura-cli -- verify --phase 11 --work
target/phase11-verify` exits 0  
**Verdict:** the implementation is **conditionally complete**. Eight evidence conditions
remain open; C1 is a Sev 2 release trigger. No synthetic result below is presented as a
real-wedding, photographer, GPU, or production-model measurement.

---

## 1. What shipped

One feature: local, scene-conditioned framing intelligence. It measures horizon, crop
boundaries, subject placement, balance, negative space, background competition, and a
bounded reference-aesthetic signal. It persists structured reasons and evidence, resumes
after interruption, and explains the result over the photograph in the desktop app.

| Area | What landed |
|---|---|
| Contract | Sixteen stable flags, twenty-six typed reason codes, evidence/crop/result/coverage/service shapes, confidence, and three independent provenance versions |
| Analysis | Horizon plus intentional-tilt classification; pose decoding and crop audit; placement, balance, negative space, proxy background measures; capped fusion and calibration machinery |
| Rules | Neutral plus twenty-two scene rows in versioned `composition_rules.toml`, with rationale and explicit creative allowances |
| Models | Signed fp32/fp16 pose fixtures and fp32/fp16/int8 aesthetic fixtures, model cards, deterministic export and evaluation tools; untrained provenance is enforced |
| Persistence | Migration 11, compact evidence JSON, strict decoding, atomic narrow dismissal, version invalidation, deterministic within-moment ranks, and resumable project analysis |
| Application | Typed app state and IPC, native desktop command registration, runtime kill switch, review queue, dismissal, and cancellable analysis off the renderer thread |
| Explain UI | Reachable Composition card over the real thumbnail, thirds/horizon/evidence overlays, crop-preservation hint, degradation language, and accessible dismissal controls |
| Gate | `aura-cli verify --phase 11`, the Rust composition harness, Python metric guard, model/contract checks, and release storage/processor-path budgets |
| Operations | ADR-0023, ADR-0024, public reason guide, module README, two model cards, five runbooks, research record, changelog, progress log, and this report |

The Phase 11 error range is `AURA-ML-5043` through `AURA-ML-5047`; every registered code
has a runbook. Originals remain read-only. This phase has no crop, straighten, removal,
keep, reject, delivery, or gallery-order command.

## 2. Acceptance criteria

| # | Section 13 criterion | Status | Evidence |
|---|---|---|---|
| 1 | Every image carries tilt, headroom, placement, balance, clutter, flags, and evidence | **met for every successfully analysed proxy, with C1/C4** | The release gate stores and reloads eight complete deterministic judgements. A fatal proxy/analyser/store failure writes no clean-looking row; missing keypoints or aesthetic inference writes a visibly degraded result. |
| 2 | Overlay shows exactly why a frame was marked badly composed | **met in automated UI scope, with C7** | The card is rendered from the focused photo in `App`, uses that photo's thumbnail and aspect ratio, and tests percentage-positioned thirds, horizon, evidence, and hint layers plus dismissal and unavailable states. A real desktop visual audit is still C7. |
| 3 | Creative tilts and deliberate tight crops are respected | **met on authored fixtures, with C3** | The diagonal-weave dutch fixture measures -43.29 degrees at 0.377 horizon confidence and is exonerated; paired close-portrait fixtures distinguish allowed and accidental head crops. |
| 4 | Crop hints exist for every portrait-class frame | **met on the authored set** | Release gate: 6/6 portrait fixtures expose a preservation objective. Presence and actionability are separate, so an unsafe crop retains a non-actionable hint. |
| 5 | Agreement and geometric accuracy gates pass in CI | **geometric/reference gates met; photographer gate C2** | Worst horizon error 0.373 degrees, crop F1 1.000, head-merge recall 1.000 and false-positive rate 0.000, pairwise agreement 1.000 over eight authored comparisons. There is no photographer holdout set. |

## 3. Phase-specific quality gates

Measured by `tests/eval/composition_eval.rs` (37 tests), the release verifier, and
`ml/models/composition/eval_composition.py --self-test`.

| Gate | Threshold | Result | Measured against |
|---|---:|---:|---|
| Worst horizon angle error | <= 0.40 degrees | **0.373 degrees** | authored architecture and seascape-style raster fixtures |
| Intentional dutch false flag | none | **none** | dedicated candid/dance-style texture fixture |
| Limb/joint crop F1 | >= 0.90 | **1.000** | authored joint and edge geometry; reference poses, not the placeholder head |
| Head-merge recall | >= 0.85 | **1.000** | one authored positive |
| Head-merge false-positive rate | < 0.10 | **0.000** | four authored negatives |
| Aesthetic pairwise agreement | >= 0.78 | **1.000** | eight authored pairs through the linear reference aesthetic, not photographers |
| Taste cannot erase a geometric defect | exact | **met** | the bounded fusion fixture |
| Out-of-focus beauty loses in Phase 12 | exact | **deferred - C8** | Phase 12 does not exist; Phase 11 cannot honestly test its consumer |
| Determinism | identical | **met** | duplicate complete analyses plus persisted reload |
| Degenerate evaluator is rejected | exact | **met** | Python self-test trips horizon, crop, head-merge, and agreement guards |

The first six numbers validate algorithms and authored ground truth. They do not validate
the checked-in model weights or establish a human preference ceiling.

## 4. Performance, storage, and portability

Release measurement on the available Windows development machine:

| Row | Section 11 budget | Measured | Status |
|---|---:|---:|---|
| Composition analysis per image, GPU | <= 30 ms | unavailable | **waived - C5**, no GPU execution provider |
| 4,000 images, RTX 4070 | <= 120 s | unavailable | **waived - C5**, same reason |
| Available processor path | diagnostic guard <= 500 ms/image | **428.17 ms/image** (5,138 ms / 12) | **met** |
| Processor-path extrapolation | diagnostic only | **1,713 s / 4,000** | reported, never relabelled as the GPU row |
| SQLite storage | <= 800 B/image | **733.2 B/image** (733,184 B / 1,000) | **met** |

ADR-0023 records the GPU waiver and its expiry. The development machine lacks the Visual
Studio SDK/linker path, so Rust verification used the pinned GNU host toolchain. The
standalone Tauri shell is source-checked where the available toolchain permits; the final
three-machine NVIDIA, DirectML, and Apple Silicon matrix remains C5.

## 5. Safety, failure, and rollback

- Placeholder pose and aesthetic outputs execute in the verifier to prove their signed
  tensor contracts, then product analysis refuses to treat them as trained evidence. The
  release gate observes 51 pose values and one aesthetic value, but trusts zero placeholder
  subjects and emits `keypoints_unavailable` plus `aesthetic_unavailable`.
- A missing optional head produces a persisted degraded result; a fatal proxy, analyser,
  validation, or SQLite failure produces a typed error and no plausible clean default.
- Dismissal is a durable review projection. It atomically removes one visible defect and
  its reason while preserving the original measurements and composite. Re-analysis
  reapplies the photographer's dismissal.
- `model_ver`, `analysis_ver`, and `rules_ver` invalidate independently; the release gate
  verifies that each bump makes 8/8 rows pending.
- The runtime composition flag defaults on and can disable analysis/card exposure without
  changing originals, edits, or selection. `models.lock` pins every artifact digest and a
  model-version bump is reversible. Migration 11 is additive; reviewed dismissal bits must
  be exported before an operator intentionally drops it.
- No image, filename, face geometry, keypoint plane, or identity data is emitted in the
  Phase 11 tracing payloads.

## 6. Definition of Done audit

| Requirement | Verdict |
|---|---|
| Section 13 acceptance criteria | **conditional** - implementation criteria met; C1-C4 and C7 bound real-world claims |
| Three reference weddings | **not met - C3** |
| Unit/integration/golden/perceptual/performance on three OS/hardware lanes | **automated local suites met; full matrix not met - C3/C5** |
| Performance budget or signed waiver | **met** - storage passes; GPU rows waived in ADR-0023 |
| Telemetry visible in dashboard and aggregate pipeline | **not met - C7** |
| Confidence, reasons, and Explain rendering | **met** |
| Module README, model cards, help, changelog | **met** |
| Rollback flag, model pin, migration path | **met**, subject to exporting human review bits before destructive rollback |
| Real 3,000-image demo recording | **not met - C7** |

## 7. Open conditions

### C1 - registered model weights are untrained (**Sev 2 trigger**)

`pose_keypoints` and `aesthetic_head` are deterministic architecture fixtures, not learned
models. The product path withholds their outputs and exposes reduced coverage/confidence.
No later phase may claim real-photo pose or learned-aesthetic quality until trained,
signed replacements pass parity, real-data accuracy, calibration, fairness, and model-card
review. **Owner:** SRML/MLL/MLOPS. **Close:** replace both artifacts, set verified trained
provenance, and pass this gate on a locked real-photo holdout.

### C2 - photographer preference data and calibration do not exist

Section 9 requests 4,000 composition pairs and section 10.1 requests held-out photographer
agreement. The repository has eight authored pairs and a reference linear score. There is
no human ceiling, wedding-level holdout, multi-photographer slice, learned-head uplift, or
keeper calibration. **Owner:** DATA/MLR/PM. **Close:** collect consented, multi-photographer
pairs split by wedding; publish slice agreement and ECE; train a head that beats the frozen
reference and reaches >= 0.78.

### C3 - real-wedding perceptual and fairness evidence is absent

There are no three named reference weddings, 300-frame blind QAIQ audit, competitor A/B,
demographic/cultural slice, or evidence-box false-positive review in this workspace.
Authored rasters are not substitutes. **Owner:** QAIQ/QA/PM. **Close:** run the locked
protocol on the indoor Hindu, outdoor Christian, and mixed-light Nepali references, record
>= 60% preference, publish violations and fairness slices, and attach the reviewed report.

### C4 - available subject/gravity inputs are incomplete

The catalog does not yet carry camera gravity metadata or person/body boxes. Pose crops
are derived from faces, so back-facing people, children, seated subjects, and crowds are
under-covered. **Owner:** TLC/SRML. **Close:** wire gravity when available, integrate a
trained person/pose source, and pass labelled architecture, crowd, child, seated, and
back-facing suites without weakening the current abstention semantics.

### C5 - reference hardware and GPU evidence are absent

No GPU execution provider or NVIDIA/DirectML/Apple Silicon CI fleet is available. The
30 ms and 120 s rows remain the explicit ADR-0023 waiver; the 428.17 ms CPU result is only
a regression guard. **Owner:** PERF/DEVOPS/CTO. **Close:** run release builds on all three
reference machines, meet the rows or amend the signed waiver with measured evidence, and
attach peak RAM/VRAM.

### C6 - background analysis is proxy-only

The analyser measures edges, luminance, saturated colour, and vertical structures. It does
not recognise exit signs, bins, mirrors, rubbish, or reflection semantics; phase 18 masks
do not exist. **Owner:** MLL/SRC at Phase 18. **Close:** revalidate with segmentation masks
and a labelled wedding-distraction set while preserving generic evidence for abstentions.

### C7 - production telemetry and real desktop evidence are absent

`composition.scored` and `composition.tilt` are local structured tracing events. They are
not visible in a metrics dashboard or opt-in aggregate consumer, and no real 3,000-image
desktop demo/visual review is attached. **Owner:** DEVOPS/SFE/QAIQ. **Close:** wire the
privacy-reviewed aggregate pipeline, demonstrate both events without image/path data, run
the desktop visual audit, and attach the recording.

### C8 - Phase 12 consumer proof is necessarily deferred

Phase 11 structurally caps aesthetics, but the required proof that a beautifully composed
out-of-focus frame loses is a Phase 12 integration test. Implementing selection here would
violate scope. **Owner:** Phase 12 SRC/QAL. **Close:** combine `technical_score` and
composition in the frozen Phase 12 contract and retain the exact regression fixture.

## 8. Reproduction

On this Windows checkout, use the available GNU host toolchain:

```powershell
$env:RUSTUP_TOOLCHAIN = '1.97.1-x86_64-pc-windows-gnu'
cargo test -p aura-brain-photo
cargo test -p aura-core
cargo test -p aura-catalog
cargo run --release --package aura-cli -- verify --phase 11 --work target/phase11-verify
cargo test --release -p aura-perf --test composition_budgets -- --nocapture
cargo xtask models --check
cargo xtask contracts --check
python ml/models/composition/eval_composition.py --self-test
```

The `just phase-11-verify` recipe wraps the release verifier on hosts with `just`
installed. Open conditions above require external data, people, hardware, CI, or the
future Phase 12/18 consumers; they are not silently converted into passing fixtures.
