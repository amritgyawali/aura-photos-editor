# Phase 11 composition metric specification and ablation record

**Status:** implemented metric specification; external photographer approval pending  
**Date:** 2026-08-15  
**Owners:** MLL (metric), PM (rule-table semantics), CTO (contract)  
**Decision record:** ADR-0023

## 1. Measurement contract

The evaluation unit is a photograph plus its scene, known face/body geometry, and authored
evidence labels. A missing measurement abstains; it never supplies a favourable zero.

| Measure | Definition | Gate |
|---|---|---|
| Horizon error | absolute wrapped difference between estimated and labelled horizon angle, only when a source is measurable | worst <= 0.4 degrees |
| Intentional tilt | a frame above 6 degrees that is centred, lacks a strong coherent horizon, and uses an allowed candid/dance scene | no `horizon_tilted` defect |
| Crop audit | joint/head/limb flag compared with labelled cuts; a joint is worse than a mid-limb cut | F1 >= 0.90 |
| Head merge | vertical-structure flag around labelled head centroids | recall >= 0.85 and false-positive rate < 0.10 |
| Aesthetic order | sign of score difference for a labelled pair from the same scene/moment | agreement >= 0.78 |
| Composite dominance | aesthetic contribution is capped by the scene rule and cannot overturn the later phase's integrity ordering | structural cap plus phase 12 integration |

Calibration is reported as expected calibration error over photographer-labelled
probabilities. With no such labels, the mapping is the identity and the result is
“unmeasured”, not zero error.

## 2. Dataset boundaries

The checked-in Rust harness is an authored synthetic set. Geometry and evidence are known
by construction, making it useful for deterministic regression tests and useless for a
claim about real-photo prevalence. The Python evaluator accepts a separate JSON document,
checks the same gates, supports wedding-level splits, and can fit isotonic calibration.

The required real dataset remains:

* tilt and crop labels from the three reference weddings;
* at least 4,000 composition-focused pair choices, split by wedding, from at least three
  photographers and more than one tradition;
* a 300-frame blinded flag/no-flag audit with demographic and scene slices;
* a held-out calibration set that shares no wedding with training.

No row from phase 10's authored preference fixture is relabelled as a photographer choice.

## 3. Recorded implementation ablations

These comparisons were produced by the same 33-test authored harness before and after one
isolated change. They are regression evidence, not model research results.

| Change | Before | After | Decision |
|---|---:|---:|---|
| Angle-only horizon concentration -> angle plus rho line coherence | intentional dutch texture: -43.29 degrees, confidence 1.000, wrongly flagged | -43.29 degrees, confidence 0.377, intentional and exonerated | keep rho coherence; repeated diagonal texture is not one horizon |
| Require subject hue -> neutral-subject saturated-background fallback | red sign behind neutral subject: competition 0.000 | competition 0.753 | keep fallback and sample the dominant head first |
| mid-limb severity ratio 0.45 -> 0.46 | crop precision 1.000, recall 0.500, F1 0.667 | precision 1.000, recall 1.000, F1 1.000 | keep 0.46; the previous 0.297 missed a 0.30 threshold by rounding the design against itself |
| include unlocated reference pose in placement -> require at least one located keypoint | reference-only pose displaced the subject box | faces remain the fallback subject geometry | keep; absent coordinates cannot vote on placement |

The current authored results are worst horizon error 0.373 degrees, crop F1 1.000,
head-merge recall 1.000 with false-positive rate 0.000 on one positive/four negatives, and
aesthetic pair agreement 1.000 over eight authored pairs. The small denominators and
authored labels are printed alongside the numbers by the gate.

## 4. Approval boundary

ADR-0023 accepts the software contract, bounded influence, and rule-table ownership. It
does not stand in for the photographer consultant, data collection, blind audit, or
reference-hardware sign-off. Those remain exit conditions; closing them requires attaching
their dataset version and report rather than editing this sentence.

