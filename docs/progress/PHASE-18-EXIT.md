# PHASE-18 exit report - Local Mask AI: Automatic Semantic Masking

**Branch:** `feat/phase-18-local-mask-ai` · **Gate:** `aura-cli verify --phase 18` exits 0 ·
**Status:** implemented **conditionally**, on the six conditions in section 8.

## 1. What shipped

One frozen contract, eleven new modules in an existing crate, one migration, one IPC surface,
one panel, two ADRs, two shaders, four Python scripts, two signed models with cards, one product
document and a gate.

`aura-vision::contract::mask` freezes the shape: `Mask`, `MaskKind` (twenty), `Storage`,
`MaskPayload`, `EdgeQuality`, `MaskReason` (twelve), `MaskOp`, `GpuMask`, `MaskOutline` and
`MaskService`. `aura-core::contract::ids` gains `MaskId`. **The contract is not in `aura-core`**,
and that is the phase's first decision rather than an accident: section 5 freezes
`fn upload_gpu(&self, mask: &Mask, level: RenderLevel) -> GpuMask`, `RenderLevel` belongs to
`aura-render`, and `aura-core` depends on no workspace crate. `SimilarityIndex`, `RenderService`
and `PreviewService` set the precedent - a contract lives in the crate that owns the kind of
thing it describes.

`aura-vision::mask` measures. `segment.rs` finds twenty classes from geometry and colour, seeded
by phase 06's boxes and landmarks. `subject.rs` composes the person classes and bounds them by
the person boxes. `trimap.rs` erodes and dilates into a band whose radius is a fraction of the
region's own size. `matting.rs` solves alpha inside that band with a guided filter in closed
form, and reports how much of the band the photograph could actually determine. `instance.rs`
assigns connected components to identities by *containment* and leaves the ambiguous ones
unassigned. `algebra.rs` is the seven operations later phases and the brush both go through.
`quality.rs` turns the two quality numbers into a strength ceiling for five named operations.
`store.rs` is the codec and migration 18. `api.rs` is the frozen service and the resumable pass.
`fixtures.rs` paints regions into synthetic pixels so the gates have something honest to measure.

Migration 18 adds `masks`, `mask_gate` and `v_mask_coverage`. There is no column in it that could
hold a photograph and no column that could hold a skin colour, and the gate scans for both on
every run.

`crates/aura-render/shaders/mask_upsample.wgsl` and `mask_composite.wgsl` are the GPU half. No
`wgpu` backend is linked, so neither executes; `shader_parity.rs` holds both to the reference and
`colour_discipline.rs` holds the compositing to linear light.

The IPC surface is eight commands and nine shapes (ADR-0038). The panel shows two quality bars
rather than one, names which of the two is limiting in a sentence, and has no button that applies
a mask to a photograph.

## 2. Acceptance criteria (section 13)

| Criterion | Status |
|---|---|
| Every selected frame has semantic, identity-scoped masks with confidence and edge quality | **met in code and measured on painted fixtures.** Twenty classes, identity scoping by containment, two quality numbers on every region. Not exercised on a wedding - C1 |
| Hair, veil and rim-light edges look clean at 100 % zoom | **not verified.** The matte is measured against authored boundaries and the halo metric is implemented and self-tested; there are no veil fixtures with ground-truth alpha and no human audit - C1, C2 |
| Masks can be inspected, brushed and feathered by the user, and those edits are permanent | **met.** Eight commands, four panel actions, and permanence is enforced twice - `user_edited = 0` inside the `DELETE`, and the edited coordinates skipped on re-insert. The gate exercises the whole round trip |
| Downstream phases receive a stable mask algebra API | **met.** `MaskService` is frozen and digested in `contracts.lock`; the seven operations are in `algebra` and the gating in `quality::Operation` |
| Mask generation and storage stay within their budgets on a 1,000-image gallery | **storage met and measured** - 29,149 bytes for all twenty classes of one frame against a 180 KB budget, so a thousand frames is about 28 MB against 180 MB. **Generation time not met against section 11** - C3 |
| Model cards report per-class and per-skin-tone performance | **partially met.** Both cards exist and carry the three headline gates; the per-skin-tone and ethnic-attire rows are left **empty** rather than filled with a plausible figure, because the data does not exist - C1 |

## 3. What the section 10.1 gates measured

`cargo test -p aura-vision --test mask_eval` - 22 gates, all green. `cargo test --workspace
--all-targets` - 125 test binaries, all green.

| Gate | Threshold | Measured |
|---|---|---|
| mIoU, skin | >= 0.92 | **1.000**, worst of five reflectances |
| mIoU, face | >= 0.92 | **1.000**, worst of five |
| mIoU, hair | >= 0.88 | **0.982**, worst of five |
| mIoU, subject | >= 0.90 | **0.910**, worst of five |
| Spread across the five reflectances | < 0.05 | **0.000** on skin |
| Sky and greenery found outdoors, absent indoors | pass | **pass** - a grey wall is not called sky, and the class comes back empty with confidence zero |
| A faceless frame invents no person class | pass | **pass** - skin, face and hair all empty with `NoFaces` |
| Per-identity skin does not bleed between adjacent people | overlap < 0.01 | **0.0000** |
| The unscoped region survives beside the scoped ones | pass | **pass** |
| A low-quality mask blocks skin smoothing and still carries a tone move | pass | **pass** - allowance 0.21, smoothing refused, 21 % of a local tone move permitted |
| All classes of one frame within the payload budget | <= 184,320 bytes | **29,149** |
| Run-length payloads round-trip exactly | pass | **pass**, and the cost of the hard form is bounded at >= 0.90 soft agreement |
| Two runs over one frame are identical | pass | **pass** |
| Every class in the vocabulary produces a row | pass | **pass** - an absent class is an empty region, never a missing one |
| A low-contrast boundary lowers edge quality rather than producing a wrong edge | pass | **pass** |
| A matte over a boundary the picture does not contain scores worse than one it does | pass | **pass** |

`just mask-report` - four Python self-tests, all green. The one worth quoting: a one-pixel rim of
0.3 alpha around a boundary scores **0.899** on mIoU and **0.125** on the halo measure. That is
the whole argument for having a second metric.

## 4. Performance

Measured on the reference machine in release, on the 384 x 256 authored fixtures, through the
whole pipeline including matting and the codec.

| Metric | Budget (section 11) | Measured |
|---|---|---|
| All masks per image | <= 120 ms | **not met on the processor path; not measurable on a GPU path that does not exist** - C3 |
| 1,000 selected images total | <= 120 s | as above - C3 |
| Mask payload per image, all classes | <= 180 KB | **29 KB** |
| GPU mask upload + composite per render | <= 4 ms | **waived** - no `wgpu` backend (ADR-0029 section 4) |

Section 11's first two rows are written against a GPU. This build links none, and the same waiver
phase 14 recorded applies for the same reason. What is measurable is that the *storage* budget -
the one section 12 names as a failure mode - is met with a factor of six in hand.

## 5. What is honest about the two shipped models

**Neither is trained and neither is consulted.** `SEG_HEAD_TRAINED` and `MATTING_HEAD_TRAINED`
are both `false`, `class_hint` and `alpha_hint` return `None` on every call, and the phase gate
asserts all four. Two models are registered, signed and carded because the *registration* is
real - the digest, the manifest, the card, the rollback path - and because the day one is trained
should be a weights change rather than a phase.

What ships instead is measurement, and it is worth being precise about how far that goes.

**It is genuinely good at:** skin, face and the eye regions, because phase 06 gives it boxes and
landmarks and the seed is sampled from the frame's own faces. Sky, greenery, water, floor and
windows, because those are colour, position and texture. The subject, because it composes the
person classes and bounds them by the person boxes. Hair, because hair is darker than the same
person's skin and sits in a known place.

**It is honestly weak at:** clothing versus dress, which has no colorimetric signature at all -
a red lehenga comes back as clothing, at a lower confidence than any other class. Blonde hair
against a dark background, which the "darker than her own skin" test misses; it is still in the
subject, so only operations that ask for hair *specifically* are affected. And any boundary the
photograph does not really contain, where the matte falls back to the coarse mask and
`edge_quality` says so.

The matting is the one place where "not a network" is not a compromise. A guided filter solved in
closed form is a real matting algorithm, it is what most matting networks are refined by, and
its failure mode is a slightly soft edge rather than a confidently wrong one. It also could not
have been a network in this build: the interpreter implements an opset 13 subset with no `Resize`
and no `ConvTranspose` (ADR-0007).

## 6. Two defects this phase found and fixed

Both were found by a measurement rather than by review, and both are the kind that ship silently.

**The resampler manufactured a halo.** `Plane::resize_bilinear` read zero outside the plane, so
every upsampled mask came back with its outermost half-pixel darkened - a one-pixel dark rim
around every region, at every render level, on a boundary nobody had segmented wrongly. It is
exactly the artefact section 10.1 audits for at 100 % zoom, produced by the code that was
supposed to deliver the boundary rather than by the code that found it.
`Plane::at_clamped` is the fix and `an_upsample_does_not_manufacture_a_dark_rim` is the test.

**`INSERT OR REPLACE` would have destroyed a photographer's mask through a constraint.** `put`
started with `DELETE ... WHERE user_edited = 0`, which is the rule this repository has enforced
three times. That was not enough: `masks` has `UNIQUE (image_id, kind, identity_id)`, and an
`INSERT OR REPLACE` **deletes the row it conflicts with** - so the hand-edited row survived the
`DELETE` and was destroyed on the way back in, by a constraint nobody was looking at. `put` now
reads the edited coordinates first and skips them entirely. The gate caught it on its first run.

## 7. Rules this phase adds that every later phase inherits

- **`MaskService` is the only way to ask what is where in a photograph.** Fourteenth service of
  its kind, and the rule matters more here than in any of the thirteen: phases 19 to 24 each edit
  a region, and two answers to "where is her face" is a gallery where the light sculpting and the
  retouching disagree about the same pixels. That reads as neither, and it is unfixable
  afterwards because nothing records which answer each stage used.
- **A mask is evidence about a region; the phase that edits owns the edit.** Sixth phase running.
  Nothing in `aura-vision::mask` moves a pixel, there is no `apply_mask` on the IPC surface, and
  no field on any shape here could carry one.
- **A region says how much may be done with it, and a later phase multiplies.**
  `Mask::allowance` is the geometric mean of two independent uncertainties, and
  `quality::allowance` is the one place a gating decision is made. This is new: it is the first
  time a phase has constrained what a *later* phase may do, and it exists because a wrong mask is
  *silent* - a wrong exposure looks wrong, and a face mask that includes the wall behind somebody's
  ear looks fine until phase 20 brightens it.
- **Two quality numbers, never one.** Confidence and edge quality fail independently and are fixed
  by different things - a photographer can re-brush a boundary and cannot re-brush a class. Any
  later phase that collapses them has thrown away which of the two a photographer is looking at.
- **The coverage denominator is *selected* frames, and both numbers are on the wire.** Every
  outline since phase 09 has counted against every photograph. This one does not, and the reason
  is real: a mask over a rejected frame is not a gap, it is a frame nobody asked about. Phase 08's
  rule is the one being followed - say what the denominator is.
- **A photographer's region is never regenerated, and it takes two statements to guarantee it.**
  The flag in the `DELETE`'s `WHERE`, and the edited coordinates skipped on re-insert. Section 6
  above is why the second one exists.

## 8. Conditions

Six, of which two are Sev 2 triggers.

**C1 - Sev 2. Both shipped heads are placeholders, and every number in this phase is measured on
painted synthetic frames.** Section 9 gives DATA a twelve-day task - "segmentation labels on 12k
wedding frames incl. veils, ethnic attire, varied skin tones" - and there is no consented wedding
imagery in this repository, so it did not happen and cannot happen here. The mIoU gates, the
group-photo test, the storage figure and the determinism check are all measured against frames
whose regions were painted into the pixels and read back through the real pipeline. That proves
the arithmetic recovers something genuinely in the buffer. It is not evidence about a wedding
photograph. **No later phase may claim a mask-quality result that depends on a region being
correct on real pixels until this closes.** It closes with labelled data and a trained
segmentation head, not with either alone.

**C2 - Sev 2. The 100 % zoom artefact audit did not happen.** Section 9 gives QAIQ three days to
audit 300 masks for halos, veil edges, dark-suit boundaries and hair detail, and section 13's
second criterion is that they look clean. There are no photographs to audit. What exists instead
is the halo *metric*, implemented and self-tested in `ml/models/mask/eval_mask.py`, and the two
structural defences the metric exists to make visible - the clamped resampler and the matting
variance floor. Neither is a substitute for a person looking at a veil. **This is the criterion
most likely to be wrong in a way nothing here would catch**, because a mask can pass every mIoU
gate in this document and still show a rim a photographer sees immediately.

**C3 - the 120 ms budget is not met and the GPU rows are waived.** No `wgpu` backend is linked
(ADR-0029 section 4), so section 11's first, second and fourth rows have no device to be measured
on. The processor path is the reference and it is slower than the budget. This is the same waiver
phase 14 recorded, carried forward, and it closes when a backend exists.

**C4 - the render graph still cannot evaluate a semantic mask.** `SkipReason::MaskGeneratorAbsent`
remains reachable, and correctly so: a renderer that was not handed mask planes cannot resolve
`MaskKind::Face` on its own. Wiring `upload_gpu`'s output into the graph is phase 19's first task
and it changes no shape frozen here. This is deliberate - section 2.2 puts every *use* of a mask
in phases 19 to 24 - and it is written down because a reader of phase 14's exit report will expect
this phase to have closed it.

**C5 - the per-subset fairness rows are unmeasured.** `subset_report` is implemented and its
self-test proves it both finds a disparity and stays quiet when there is none. It has never been
run on data. The five reflectances in the fixtures are **reflectances, not people**: they prove
the skin seed is measured from the frame rather than compared against a constant, and they prove
nothing about a real person. `docs/skin-fairness.md` states this in the product's own words, and
the two rows in `docs/model-cards/semantic_segment.md` are left empty rather than filled.

**C6 - `aura-vision` now depends on the catalog, and phase 06's structural claim is a test.**
Phase 06 wrote that a face template could not be written from this crate because it had no
catalog dependency. Section 4 of this phase document puts the mask store here, so that sentence
has stopped being true. `crates/aura-vision/tests/no_template_writes.rs` replaces it and the phase
gate runs the same grep, which makes it the third grep-as-a-test in the repository. It is a
weaker guarantee than the one it replaces, and it is written down here so that nobody rediscovers
that in three phases' time.

## 9. Rollback

Feature flag: nothing in phases 01 to 17 calls `MaskService`, so removing the eight IPC commands
removes the feature entirely.

Migration: `DROP VIEW v_mask_coverage; DROP TABLE mask_gate; DROP TABLE masks; DELETE FROM
schema_version WHERE version = 18;` returns the catalog to schema 17. Everything is recomputable
from the photographs, phase 06's faces and this build's arithmetic - **with one exception, and it
is the important one**: a mask a photographer brushed by hand is not derivable from anything.
`SELECT * FROM masks WHERE user_edited = 1` is the whole of what needs exporting first.

Models: `aura-models` pins by version and digest. Because neither head is consulted, rolling
either back has no visible effect in this build.

## 10. What phase 19 should read first

`aura_vision::mask::quality::allowance` and `docs/adr/ADR-0037` decision 6. Phase 19 is the first
consumer of a mask, and the one thing it must not do is apply its own strength directly: the
ceiling is a multiplier, `Operation::LocalTone` is its operation, and a region below
`AGGRESSIVE_FLOOR` is a region it must still be able to use gently rather than one it should skip.

The second thing to read is C4. `upload_gpu` produces a resolved plane at any render level and
the two shaders are written; what does not exist is the wiring from `RenderRequest` to those
planes, and that wiring is phase 19's, not a gap in this one.
