# ADR-0160: Reference colour matching by learned edit and distribution transfer

Date: 2026-10-23. Extends ADR-0130 and ADR-0131; replaces the slider fit for references measured
by this build.

## Context

The request was for colour and tone - grading, white balance, colour tone and colour effects - to
match an edited reference every time, on any photograph, and across a new batch.

The slider fit before this (ADR-0130) searched bounded recipe parameters through the renderer. It
could only move as far as those parameters reach: no hue rotation, five tone sliders, a white
balance capped near 800 K. The 2026-10-22 controlled experiment recorded the consequence: on new
photographs a split tone and a strong grade ended *farther* from the known edit, and a monochrome
reference stayed about 25 ΔE away.

Two facts set the design, and both were measured rather than assumed:

1. **One finished photograph of another scene does not contain its edit.** A beige wall and a
   warm grade measure the same; a dark doorway and crushed blacks measure the same. On nineteen
   MIT-Adobe FiveK photographs, matching each photograph to another's expert edit by appearance
   moved its appearance halfway to the reference and its error from the expert's own edit from
   9.71 to 12.01 - and applying the expert's *own* edit of one photograph to another did no
   better (10.0). No method can be correct there; one can only be safe and say what it did.
2. **When the reference's original is available, the edit is.** The same photograph matched to
   its own edited version came to 2.0 from 9.7, and on one real wedding shoot (the user's own
   CR3 files, eight known looks, every frame matched to every other frame's edited version)
   learning the edit from the reference's original and applying it to the other frames left
   1.9 - 2.6 ΔE from the right answer, where matching appearance left 3.2 - 3.7 and doing
   nothing 4.8.

## Decision

A matched look is a new recipe extension, `reference_grade_v1` (`aura_recipe::reference_grade`),
executed by one renderer stage (`aura_render::reference_grade`) after every other colour stage.
It is a display-referred transform in OKLab: a 33-node tone curve; the photograph's and the
target's neutral axis at nine lightnesses (white balance and split toning in one shape); a
chroma curve; twelve hue sectors of saturation, hue and lightness; five lightness zones of chroma;
and a skin-protection strength. The stage moves each working pixel to display colour through
the output transform's own shoulder and matrix and back through their inverse - a change of
coordinates undone inside the stage, so `colour_discipline.rs` holds - and leaves a pixel the
transform does not move untouched. Colours pushed out of gamut lose chroma at constant hue and
lightness instead of clipping a channel.

The fitter (`color_match/transfer.rs`) and the renderer share one evaluator, so a prediction is the
render. Three paths, chosen per photograph and reported on the wire as `transfer.method`:

- **Same frame** (structural similarity ≥ 0.9 with the reference): the edit is fitted from the
  reference's statistics and then refined on corresponding thumbnail pixels
  (`transfer/pairs.rs`), and kept as the reference's learned edit.
- **Learned edit**: the edit learned from the reference's original - this photograph, or one in
  the same collection whose file name the reference's name contains (`989A2698-edit.jpg` names
  `989A2698.CR3`) and whose structure confirms it (`color_match/edit.rs`) - applied unchanged.
- **Appearance**, when no original exists: each part of the reference's appearance is taken by a
  measured trust - black and white points 0.2, tonal shape 0.1, cast 0.3 (as one smooth tint and
  slope between the two photographs' greys, weighted by the evidence that the reference's greys
  are greys), colourfulness 0.4, per-hue character 0.15 - and all of it when the two are the same
  scene. The fit never leaves a photograph farther from the reference's appearance than it
  started. Four candidate sets were compared on four sets of real photographs; this was the only
  one that improved on every set, and the set first chosen on half a shoot made the other half
  worse (4.82 to 5.44).

Two axes are measured per photograph, because two different questions need them. The *measuring*
axis follows each lightness's densest third, or a clearly separate grey cluster where one exists:
it must be consistent between a photograph and its own edit, and anchored on greys so a learned
edit describes what happened to greys rather than to a red sari. The *neutral* axis is the
strict estimate of where the greys are, with evidence that they are greys; only it decides how
much cast one scene takes from another.

Strengths move from the photograph's previous match (or none) to the new one, component by
component; zero is the previous look exactly. Matching a second reference replaces the first
instead of stacking. Apply reports the native-resolution distance and does not refuse a
strength for being far from the reference's appearance: a learned edit is the right answer even
where the scene keeps the photograph unlike the reference. Per-person skin is measured on the
fitting render; a face that would turn more than nine degrees or change chroma beyond 0.6 - 1.45
raises the skin protection rather than refusing.

References imported before this build keep the slider fit until imported again; the measurement
is content-addressed under `colour_reference_grade_v1:<id>` and the learned edit under
`colour_reference_edit_v1:<id>`.

## What was measured, and against what

| Set | Case | Error from the right answer (mean OKLab x100, about dE) |
|---|---|---|
| Synthetic, eight looks | reference is the same frame, learned and refined | 0.10 - 0.84; matte 2.73 (from 5.38) |
| FiveK, 19 expert edits | same photograph, learned | 9.71 to 2.55 (even half), 13.05 to 1.66 (odd half) |
| FiveK | different photograph, appearance | 9.71 to 9.32, 13.05 to 12.62 |
| Wedding shoot (user's CR3s), frames 1 - 6 x 8 looks | other frames, learned edit | 4.79 to 1.70 |
| Wedding shoot, frames 7 - 12 (held out) | other frames, learned edit | 4.82 to 1.77 |
| Wedding shoot, frames 1 - 6 / 7 - 12 | other frames, appearance only | 4.79 to 3.49 / 4.82 to 4.40 |
| End to end through the renderer | another frame / the reference's own frame | 4.12 to 1.00 / 4.17 to 0.49 |

Per look on the held-out frames with the learned edit: warm 2.58 to 1.48, cool and contrast 4.12
to 2.01, matte 5.07 to 1.64, split tone 1.78 to 1.47, punchy 6.77 to 4.12, monochrome 8.25 to
0.37, sepia 7.36 to 0.89, teal-orange 2.59 to 2.16.

Harnesses: `color_match::transfer::tests::one_shoot_one_look` (`AURA_GRADE_SHOOT`),
`real_expert_edits_matched_across_photographs` (`AURA_GRADE_PAIRS`), both ignored and fed by
`tests/reference_grade_pairs.rs` (`AURA_GRADE_RAWS`, `AURA_GRADE_OUT`).

## Consequences

- "Every time" is true of *producing* a match, of the safety properties, and of the same-frame
  and learned-edit paths. It is not true of a reference of an unrelated scene with no original,
  and the panel says so in that case and says what to import to get the exact edit.
- Matte - a lift made in display RGB - is the weakest look the model represents: its hue shift
  depends on lightness, and the sectors' does not.
- Bringing each frame's greys to the original's before the edit was built and measured; it left
  the real batch farther from the right answer and was removed. So was a penalty on refinement
  moves the frame barely constrains: it helped synthetic scenes and cost the real shoot.
- The trust priors are authored and chosen on two sets; they should be revisited when a set of
  real batches with photographer-made references exists.
