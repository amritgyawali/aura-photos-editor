# Phase 06 exit report - Face Detection, Recognition & People Intelligence

**Date:** 2026-08-13
**Branch:** `feat/phase-06-people-intelligence`
**Gate:** `just phase-06-verify` exits 0
**Verdict:** the phase is implemented and **conditionally** complete. Five conditions are
open, they are listed in section 5, and **C1 and C5 are Sev 2 triggers**.

---

## 1. What shipped

One feature: the app learns who matters at this wedding. It finds every face, groups them
into identities, and ranks the couple, close family and VIPs by evidence rather than by
guesswork.

| Area | What landed |
|---|---|
| Migration 6 | `face_vault`, `face_scan`, `identities`, `faces`, `identity_links`, `person_boxes`, `cooccurrence`, and two views |
| `aura-vision::face` | detection with letterbox and three strides, ArcFace alignment, pose from landmarks, the quality gate, recognition templates, bodies and face-to-body association, exact average-linkage clustering with cohesion verification, role inference, prominence, redaction, and the synthetic ground truth every gate is measured against |
| `aura-people` | the sealed biometric store, the project walk, the co-occurrence graph, identity timelines, the importance model, and `People` - the one implementation of the frozen `PeopleService` |
| `aura-core` | the frozen section 5 contract, plus `FaceId` and `IdentityId` |
| Models | `face_detect`, `face_embed`, `face_quality` - signed into `models.lock` with three model cards |
| Cloud | `CoupleHint`, the one call phase 06 may make, with three cassettes |
| IPC and UI | eleven commands, fifteen types, the People panel with merge, split, rename, mark-couple, the importance slider, and the erase control |
| Gate | `aura-cli verify --phase 06`, thirteen checks, exit 0 |

**Nine new error codes**, each with a runbook: `AURA-ML-5017` to `AURA-ML-5021` and
`AURA-SEC-9001` to `AURA-SEC-9005`.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | The People panel shows clean identities with face counts and a suggested couple | **met, with C1** | `just phase-06-verify` prints `group: N identities, ... coverage 100%`; the panel's logic is unit-tested in `ui/src/components/people/PeoplePanel.test.tsx` (32 tests) |
| 2 | Marking someone as the bride updates every downstream weight and survives a full re-analysis | **met** | gate step 11: `decision: the bride kept her role and her name across a regroup`; `a_role_set_by_the_photographer_survives_a_regroup` |
| 3 | Wide ceremony frames detect guests reliably; back-of-head cases still register a person | **met against synthetic ground truth, C1 against photographs** | recall 1.0000 at IoU 0.5 in the gate; `tiling_fires_when_there_are_bodies_and_no_faces`; `people_count` counts headless bodies |
| 4 | Every image exposes `subject_focus_score` and per-identity prominence | **met** | gate step 10: `subjects: 24/24 frames scored`; `subject_focus_is_weighted_by_who_is_in_the_frame` |
| 5 | Biometric data is encrypted, project-scoped and erasable, and never leaves the machine by default | **met** | `crates/aura-people/tests/privacy.rs`, 21 tests; gate steps 8, 12 and 14 |
| 6 | Fairness analysis published in the model cards with per-group metrics | **NOT MET - condition C5** | the fixtures use one skin tone; a number computed from them would describe a renderer |

---

## 3. Section 10.1's gates

Measured by `tests/eval/identity_eval.rs` and by the phase gate.

| Gate | Threshold | Result | Against |
|---|---|---|---|
| Detection recall at IoU 0.5 | >= 0.97 | **1.0000** | synthetic ground truth |
| Small-face recall | >= 0.90 | **1.0000** | synthetic ground truth |
| False positives on bokeh | < 1 % | **0.00 %** | synthetic ground truth |
| Identity clustering F1, pairwise | >= 0.93 | **1.0000** | synthetic templates at recogniser-realistic spread |
| Clusters holding two labelled people | 0 | **0** | including the lookalike-siblings case at 0.497 separation |
| Siblings are not merged | 0 merges | **0** | `siblings_are_not_merged` |
| Merge, split, rename are undoable and survive a re-analysis | - | **met** | `a_merge_moves_faces_and_can_be_undone`, and criterion 2 above |
| Face store unreadable without the keychain entry | - | **met** | `two_projects_get_unrelated_keys`, `a_tampered_record_is_refused` |
| Erase removes all faces | - | **met** | `erasure_removes_the_key_first_and_verifies_the_rest` |
| No cross-project query is possible | - | **met** | `AURA-SEC-9004` halts; `an_identity_from_another_project_is_refused` |
| Couple identification on 20 audit weddings | >= 19/20 | **deferred - C3** | needs real weddings and a human audit |
| Dominant subject matches human judgement >= 90 % | - | **deferred - C4** | needs labelled test frames |

**Read the "against" column.** Every number in the top half is a real measurement of the
*algorithms* - the anchor decode, the suppression, the bokeh gate, the linkage, the
cohesion guard, the second-pass margin - on ground truth whose answer is known by
construction. None of them is a measurement of the *shipped weights*, which carry no face
semantics at all. That is condition C1, and it is why
`the_clustering_metric_rejects_a_useless_recogniser` exists: it proves the harness would
fail a model that had learned nothing.

---

## 4. Section 11's budgets

Measured in release on the development machine (Intel i5-10300H, 8 GB, Win 11), asserted
by `crates/aura-perf/tests/people_budgets.rs`.

| Row | Section 11 | This build | Status |
|---|---|---|---|
| Face pipeline, 4,000 images, RTX 4070 | <= 240 s | not measurable | **waived**, ADR-0013 section 6 |
| Face pipeline, 4,000 images, M3 Pro | <= 480 s | not measurable | **waived**, ADR-0013 section 6 |
| One frame, processor path | (replaces the two above) | ~180 ms | budget 420 ms, **met** |
| Clustering 25,000 faces | <= 12 s | 2.1 s at the 4,096 skeleton cap | **met** |
| People panel open | <= 300 ms | 0 ms | **met** |
| Face store per 1,000 images | <= 25 MB | 12.8 MB | **met** |

Component measurements, `aura-cli infer`, release:

| Model | Precision | Per unit |
|---|---|---|
| `face_detect`, one 640 px pass | fp32 | 148 ms |
| `face_detect`, one 640 px pass | fp16 | 160 ms |
| `face_embed`, per face | fp32 | 9.1 ms |
| `face_embed`, per face | int8 | 10.5 ms |
| `face_quality`, per face | fp32 | 3.8 ms |

At 180 ms per frame a 4,000-image wedding is about **twelve minutes** without tiling. The
tiled pass costs five detector passes rather than one; `ScanReport::tile_ratio` measures
how often it fires and the budget caps it at 25 % of frames.

fp16 being slower than fp32 is the interpreter's widen-to-compute path, not a finding
about fp16. It is recorded in the model cards so nobody spends a day on it.

---

## 5. Open conditions

### C1 - the three models are placeholders (**Sev 2 trigger**)

`face_detect`, `face_embed` and `face_quality` 1.0.0 have the architecture of an SCRFD, an
ArcFace and a quality head, and none of their training. **The shipped detector finds no
faces in a photograph, and the shipped recogniser's templates carry no identity
information.**

Everything around them is real and measured: the letterbox, the three-stride anchor decode,
the channel layout, the joint face-and-person head, suppression across passes, the
conditional tiled pass, the landmark-spread bokeh gate, the ArcFace alignment, the pose
estimate, the four quality measurements, the exact average linkage, the cohesion guard, the
sub-centroids, the second-pass margin, the sealed store, the co-occurrence graph, the role
inference, the prominence weights, the IPC surface and the panel.

**No later phase may claim a quality result that depends on face detection or recognition
being accurate until this closes.** The first trained model reopens section 10.1's gates
against photographs.

### C2 - the quality head is not trusted

`QUALITY_MODEL_WEIGHT` is 0.0. The gate is four measured factors - sharpness, occlusion,
pose, exposure - combined as a weighted geometric mean, plus two hard cut-offs. The head is
loaded, run, batched and reported, so the cost and the wiring are real;
`the_model_head_is_wired_even_though_it_is_not_trusted` asserts both that the zero is a
no-op and that the blend works.

Closing it needs the label design in `ml/models/face/train_quality.py --plan`, which is
written and is the non-obvious half.

### C3 - couple identification is not audited

Section 10.1 asks for the right couple on 19 of 20 real weddings across traditions. There
are no real weddings here. The scorer is measured on constructed evidence
(`crates/aura-vision/tests/face_roles.rs`, 24 tests), the ambiguity margin and the
confidence ceiling are asserted, and the panel asks rather than assumes.

Note that this is **also blocked on phase 07**: without scene labels the portrait and
ceremony terms are zero, and the confidence is capped at 0.62 for that reason.

### C4 - prominence is not compared with human judgement

Section 10.1 asks for the dominant subject to match human judgement 90 % of the time. The
formula, the scene conditioning and the weight versioning are all implemented and tested;
what is missing is the labelled frames.

### C5 - no demographic analysis (**Sev 2 trigger**)

Section 12's second failure mode is that accuracy varies across skin tones. The model cards
have a fairness section and it says, in each case, that the number is **not published and
not approximated** - the synthetic fixtures use one skin tone, and a per-group metric
computed from them would be a number about a renderer that somebody would then quote.

What is needed, and is written into all three cards:

- a balanced evaluation set with per-group consent records;
- per-group **recall** for the detector, per-group **true-accept rate at a fixed
  false-accept rate** for the recogniser, and per-group **vote rate** for the quality head -
  not accuracy, which hides the failure;
- the same figures on the dark-scene and small-face subsets, where a disparity appears
  first;
- an agreed maximum disparity, decided with SEC rather than reported without a threshold.

The **mitigation is already implemented rather than promised**: where evidence is weaker,
the clustering leaves a face unassigned instead of assigning it, which turns a fairness
failure into a visible gap rather than a wrong name on somebody's grandmother.

---

## 6. Carried forward from earlier phases

Phase 02's three exit conditions are still open and are carried again: real camera files, a
photographed ColorChecker, and a three-OS CI run. **The first real camera file is a Sev 2
trigger that reopens phase 02's criteria whatever phase is in flight** (ADR-0006).

Phase 05's condition C10 - the perceptual embedding is a placeholder - is unchanged. Phase
06 does not depend on it: face templates are a different model in a different table, and
the two are never compared.

---

## 7. Rollback

| Switch | How |
|---|---|
| Feature off | Do not call `scan_faces`. Nothing else in the product reads `faces`; `SubjectHierarchy::coverage` reports 0.0 and every consumer falls back to its non-people path. |
| Model rollback | `models.lock` pins by digest; the registry keeps the previous version until a new one has completed one real inference (`AURA-ML-5009`). A `model_ver` bump makes every `face_scan` row stale, so the next pass re-scans. Two versions are never compared - `AURA-ML-5018`. |
| Migration reversible | Yes. The down migration is seven drops, written out at the top of `0006_people.sql`. It destroys every byte of biometric data, which is the correct direction for this rollback to be lossy in. |
| Grouping rollback | `group_people` rebuilds identities from the stored templates and replays the decision journal. It touches no pixels. |
| Biometric erasure | `erase_biometrics`, which is not a rollback but the photographer's own control, and is verified rather than assumed. |

---

## 8. What phase 07 inherits

Five rules, and every later phase inherits them:

- **`PeopleService` is the only way to ask who is in a photograph.** No phase may keep its
  own face store, its own clustering or its own idea of who the couple are.
- **A photographer's decision is unbeatable.** `user_locked` is checked inside the
  statement that would overwrite it, and the decision journal is replayed onto every fresh
  grouping *before* any conclusion is drawn from it.
- **Bump `model_ver` on any change to the pixels the detector sees, `embed_ver` on any
  change to the recogniser, and `quality_ver` on any change to the gate.** Three columns
  because the three invalidate different things: frames, templates and votes.
- **Report coverage when you report a result.** A grouping conclusion drawn over a
  40 %-scanned wedding is a conclusion about 40 % of a wedding, and
  `SubjectHierarchy::coverage` is how a caller finds out.
- **Never infer anything about a person.** The evidence identifies a pair; gender,
  ethnicity, religion and any relationship beyond couple, close family and guest are out of
  scope permanently, and the cloud task's output type cannot express them.

Phase 07 in particular closes half of C3: scene labels turn on the portrait and ceremony
terms in the couple contest, and `SCENELESS_CONFIDENCE_CEILING` stops applying.
