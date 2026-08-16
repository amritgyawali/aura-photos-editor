# ADR-0023 - Composition rules, aesthetic evidence, and crop hints

**Status:** accepted  
**Date:** 2026-08-15  
**Phase:** 11 - Composition & Aesthetic AI  
**Supersedes:** nothing. **Amends:** nothing.

---

## 1. Context

Phase 11 describes how a photograph is framed. It measures a horizon, audits cuts at the
frame edge, places the subject, measures visual competition, and provides a bounded
aesthetic reading. It does not crop, straighten, remove objects, cull, or claim that one
style of wedding photography is universally correct.

That distinction is the design constraint. A rule engine can be precise and still punish
an intentional dutch angle, a centred flat-lay, a tight portrait, or a ceremony whose
visual conventions were absent from its data. The result therefore carries facts,
confidence, scene-conditioned bands, exonerations, and evidence regions separately. Later
phases may use that evidence; they may not reinterpret an absent reading as a clean frame.

## 2. Eight contract spellings differ from section 5

`crates/aura-core/src/contract/composition.rs` is the frozen contract. These differences
from the phase document are intentional:

| # | Section 5 | Shipped | Reason |
|---|---|---|---|
| 1 | `ImageId` | `PhotoId`, aliased | One identifier across scene, people, moment, integrity, emotion, and composition contracts. |
| 2 | `Box2` | `CropRect`, aliased | One normalised rectangle convention; evidence uses the rectangle phase 09 already froze. |
| 3 | `JointCut::box` | `JointCut::area` | `box` is a Rust reserved word. |
| 4 | no flag carrier | `CompositionFlags` | Sections 2 and 13 require structured, filterable flags. The fixed bit positions are a storage and wire format. |
| 5 | free-form reason | typed `CompositionCode` | A closed vocabulary can be translated, documented, tested, and rendered without callers inventing claims. |
| 6 | violations only | `CompositionReason::exoneration` | Intentional tilt, deliberate crop, centred symmetry, and missing geometry must not masquerade as defects. |
| 7 | no coverage shape | `CompositionOutline` | Coverage and keypoint-aware coverage keep a partial reading visibly partial. |
| 8 | no entry point | `CompositionService` | Phases 12, 13, 23, and 29 need one stable local interface rather than private database access. |

The schema also stores `project_id`, three provenance versions, the scene used, whether
the neutral rule row was substituted, review state, and a relative-within-moment value.
Those fields make invalidation, project scoping, and user overrides explicit.

## 3. Decision: deterministic evidence leads; taste is bounded

The composite begins with measurable evidence: horizon geometry, keypoint and face crop
audits, placement, balance, edge density, bright regions, structural head merges, and
colour competition. The aesthetic head is scene-conditioned and contributes no more than
the configured cap. A frame is never rejected by this phase, and a score is never exposed
as a keep/reject recommendation.

The checked-in `aesthetic_head` and `pose_keypoints` artifacts are architecture fixtures,
not trained production weights. A successful call to an untrained fixture must not be
labelled learned. The analyser uses the documented reference aesthetic until a card and
training record establish trained provenance. The exit report records this as a Sev 2
condition; synthetic agreement is evidence about the pipeline, not about those weights.

Rejected alternatives:

* a single opaque aesthetic score, because it cannot explain a low result or degrade when
  keypoints are absent;
* pure rule-of-thirds scoring, because symmetry and intentional centre are common wedding
  compositions;
* allowing the aesthetic head to dominate, because taste is the least transferable part
  of the system;
* storing generated English explanations, because they cannot be versioned or translated
  reliably.

## 4. Decision: rules are scene-conditioned and versioned data

`crates/aura-brain-photo/config/composition_rules.toml` contains the neutral row and every
known scene row. It owns tolerances, penalties, the aesthetic cap, and explicit allowances
for intentional tilt, centre, and tight crops. Every row includes rationale and the file
has one `rules_ver`.

Unknown scenes use the conservative neutral row, reduce confidence, set provenance, and
emit `AURA-ML-5047`. A missing or malformed embedded table is `AURA-ML-5046` and blocks the
pass: silently substituting constants would make the result irreproducible.

Model, analysis, and rules versions are separate because they invalidate different work.
A mixed-version project remains readable while being re-analysed, but its outline reports
the oldest versions and emits `AURA-ML-5043`.

## 5. Decision: crop output is descriptive, not executable

`CropHint` may carry a region to preserve, a safe margin, a possible straighten angle,
and confidence. It is evidence for phase 23. There is no command in this phase that moves
a pixel, and migration 11 has no applied-crop or rotation column.

The hint can remain absent when neither subject geometry nor a reliable horizon exists.
That is different from a full-frame crop and is rendered as unavailable. Evidence boxes
remain normalised so the same result overlays every preview size.

## 6. Decision: measured saliency is a conservative substitute

This build measures edge density, luminance blobs, colour competition, and vertical
structure around detected heads. It does not semantically recognise exit signs, rubbish
bins, mirrors, or named objects, and it does not claim to use phase 18's segmentation mask
before that phase exists. Those are explicit exit conditions and a phase 18 re-validation
trigger, not hidden capabilities.

Until semantic masks exist, the generic measurements may flag a visually competing
region but the reason names the measurement, not an object identity. Distraction removal
remains phase 24.

## 7. Decision: dismissal is durable and narrow

A photographer dismisses one flag on one photograph. The store retains the dismissed bit
separately and reapplies it atomically during re-analysis; a new model cannot silently
restore the same note. The remaining result, measurements, and unrelated flags continue
to update. An invalid dismissal is refused as `AURA-ML-5044`.

Within-moment composition is refreshed only after the pass has stored the available
siblings. Ties are deterministic by timeline and photo id. An unscored sibling is absent,
not assigned a neutral score.

## 8. Performance and privacy

The section 11 GPU rows (30 ms per image and 4,000 images in 120 seconds on an RTX 4070)
are waived under ADR-0007 because this build has no GPU execution provider. The waiver is
owned by PERF and CTO and expires when a GPU backend lands. The phase gate reports the
processor path without relabelling it as a GPU result.

The storage row (at most 800 bytes per image) and the non-model arithmetic budget are
asserted against a real SQLite catalog in `composition_budgets.rs`. Reasons store stable
codes, evidence shares one compact JSON object, and the table has one measured index.

All inference and persistence are local. No original, keypoint plane, face box, aesthetic
feature, or composition result is uploaded by this phase.

## 9. Consequences

**Good.** Every judgement is inspectable, missing evidence reduces confidence, creative
exceptions are first-class, and later phases receive one versioned contract.

**Bad.** The proxy background measures are less specific than semantic detection, and a
linear reference aesthetic cannot stand in for photographer preference data.

**Ugly.** The checked-in heads are deterministic placeholders. The executable harness can
prove regressions in geometry, storage, and orchestration, but cannot prove real-photo
quality, demographic fairness, photographer agreement, or the GPU budgets. Those claims
remain conditions until labelled data and reference hardware exist.

## 10. Related

* `docs/adr/ADR-0024-composition-ipc-surface.md` - the application and UI boundary
* `docs/composition-and-framing.md` - the photographer-facing vocabulary
* `docs/progress/PHASE-11-EXIT.md` - measured gates and open conditions
* `crates/aura-catalog/migrations/0011_composition.sql` - durable representation

