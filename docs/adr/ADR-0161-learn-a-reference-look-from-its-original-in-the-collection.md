# ADR-0161: Learn a reference look from its own original in the collection

Date: 2026-10-23. Extends ADR-0140 (native fitting on the main Match a look screen) and the
original/edited-pair calibration. No frozen contract, migration or dependency changes.

## Context

Match a look was asked to reproduce an edited reference's colour and tone on every photograph
of a new batch. Two routes existed: one finished reference alone (an estimate), and an
original/edited pair chosen by hand (a calibration from corresponding pixels).

The single-reference route was measured on real photographs rather than synthetic scenes. The
harness `crates/aura-app/src/color_match/transfer/tests/looks.rs` (ignored, `AURA_LOOK_PHOTOS`)
applies twelve known looks to 31 real neutral renders (19 MIT-Adobe FiveK, 12 wedding CR3s).
Each look-graded photograph becomes a reference, each other photograph a target, and the score
is the mean OKLab distance x100 to the same look applied to that target. Every third photograph
is only ever a target, and is reported separately as held out.

| Route | Error left, all looks (756 matches) | Held out |
|---|---:|---:|
| Distribution transfer, reference alone (current) | 83.5 % | 84.1 % |
| Distribution fit learned on the reference's original | 44.8 % | 45.2 % |
| Pixel calibration learned on the reference's original | **15.0 %** | **16.7 %** |

On the reference-alone route only monochrome and sepia moved materially (24-25 % left); the
colour looks were left at 86-117 %. A "departures from neutral" estimator was built and swept
component by component. These components were tried: black and white ends, the neutral axis,
shared-colour chroma and shared-colour hue. Each made as many looks worse as better (for
example, tone ends at 0.8 left warm at 140 % and split tone at 150 %), so it was not shipped. On
real photographs every one of these marks is mostly scene content. This is the measured form of
the statement in ADR-0139 that an edited reference cannot uniquely identify its edit.

The reference's original removes the ambiguity, and at a wedding it is usually already in the
collection being matched: the photographer edited one frame and exported it.

## Decision

When a single edited reference is applied to a collection and no original was chosen, the first
application in each collection looks for the reference's original there. Candidates are photos
whose file name the reference's name begins with, followed by nothing or an editor's separator
(`989A2698.CR3` for `989A2698-Edit.jpg`), and photos whose EXIF capture time equals the
reference's to the second. At most six are tried.

Each candidate is rendered from its native decode through the same neutral-plus-automatic
baseline that targets and the manual pair route use. The frame is area-averaged to 1024 px
before rendering, and the render is filtered down to the export's size (at most 512 px). An
export averages sensor pixels, so the first version point-sampled the camera file instead, and
a real CR3 pair was refused at held-out p95 10.8 for texture that was never part of the edit.
After an aspect check, the pair is passed to the existing `calibration::fit`. That fit
refuses non-corresponding structure, and refuses held-out error above mean 3 or p95 8. So a
cropped export, a different frame of a burst and a locally retouched export are all refused by
their pixels, not accepted by their names. The first accepted calibration becomes the
reference's calibration, and is stored in the reference cache with the photo's name. Every
photograph, including ones matched later, then goes through the existing calibrated
application with its strength, skin, highlight and transactional checks. A collection is
searched once (`searched`). When nothing qualifies, matching continues from the reference alone,
as before.

Per-target guards apply at full strength under the photographer's own edit. The pilot skin and
clipping limits are meant to stop an *estimate* from damaging a face. A look learned from the
photographer's edit of a frame in this collection changes skin because the photographer chose
that. At learn time `original::allowance` measures how far the edit moved each person's skin
(`appearance::skin_change`, in multiples of the pilot limits) and the clipped area on its own
original. The learned look may move them 25 % further than that on a target, and never less than
the pilot limits (`gallery_fit::Allowance`). With the pilot limits alone, every CR3 frame,
including the original itself, was cut back to 50 % strength.

`ApplyReferenceReport.learned_from` and the collection result say which photograph the look was
learned from. The history entry names it too. When a candidate was found and refused,
`original_refused` gives the reason, for example "989A2699.CR3 looks like the original, but the
edited photo is cropped or rotated".

## Consequences

- Through the native commands, a three-photo collection with a known split-tone look goes from
  6.3-7.0 to 0.22-0.25 mean RGB byte error on every photo. That includes a different scene and a
  portrait-orientation frame matched before the original was reached
  (`tests/reference_original_discovery.rs`).
- Five of the user's wedding CR3s (`H:\AURA EDIT`, ignored test `camera_files_take_a_look_learned_from_one_edited_frame`,
  `AURA_ORIGINAL_SHOOT`) were tested. One frame was given a Lightroom-style preset through AURA's
  renderer (contrast, highlights, shadows, blacks, vibrance, saturation, split toning, HSL) and
  exported as `-Edit`. Mean RGB byte error against the preset on every frame: untouched 3.56,
  reference alone 2.51, learned from its original **1.47**. The original frame went to 0.84.
  The learned route beat the estimate on four of the five frames. On 989A2698 it lost, 2.21 against
  1.83: Highlights and Shadows are edge-aware local operations, and a global mapping learned on
  one frame cannot carry them to a frame with different structure. This preset is built from
  AURA's own sliders, which is the family the reference-alone search fits, so the comparison
  favours that search.
- The first application in a collection costs one native decode per candidate, at most six per
  reference.
- A reference folder contributes every reference whose original is in the collection, up to
  eight pairs, pooled into one calibration (`calibration::fit_pairs`). In the look harness
  (`AURA_LOOK_PAIRS`), one, two and three pairs left 15.0 %, 11.6 % and 11.1 % of the difference
  (held out: 16.7 %, 12.3 %, 11.3 %), and teal-orange fell from 92 % to 65 %. If the pooled fit is
  refused, for example because one reference was retouched locally, the single pair that best
  explains its own reference is used instead. One counter-measurement is recorded too: the
  AURA-slider preset on 16 real photos (`AURA_ORIGINAL_EDITS=3`) improved the mean from 2.13 to
  1.77 RGB, but only because three edited frames now match. On the seven frames that were not
  originals, the error rose from 1.67 to 2.09. With local tone in the edit, each scene's global
  version of it differs, and a pooled compromise can transfer worse than one pair. Those FiveK
  scenes are far more varied than the frames of one wedding.
- A renamed export with no capture time, or an original outside the collection, still falls back
  to the estimate. The manual pair route remains for those cases.
- Local tone operations in the photographer's edit (Highlights, Shadows, Clarity) are carried only
  as far as a global mapping can carry them.

## Tried and rejected, with measurements

- **Fitting AURA's tone settings on the pair.** Exposure, Contrast, Highlights, Shadows, Whites,
  Blacks, Clarity and Dehaze were fitted, then the colour mapping was calibrated on top. A
  coordinate search recovered Shadows with the wrong sign. Levenberg-Marquardt on per-pixel
  lightness found settings that scored *better* than the true preset (0.00266 against 0.00307). At
  export size, lightness cannot separate these settings from the colour moves and from
  resolution-dependent local operators. On 8 real photos the result was 4 better and 4 worse
  (mean 2.23 against 2.24), so it was not shipped.
- **Anchoring unseen colour cells to the affine part of the edit.** Total error left rose from
  15.0 % to 18.9 % (cool and contrast 12 % to 29 %, film 21 % to 53 %). The cubic extrapolates
  curve-shaped looks better than a straight line does.
- **Measuring the skin allowance on all skin-coloured pixels**, not only detected people, so an
  original without people would still show how the look treats skin. The result was neutral
  (2.26 against 2.24). The one frame it released to more strength got worse, so the guard was
  right about that frame.
- **Choosing between pooling and the best single pair by leaving one pair out.** It picked a single
  pair whose look transferred badly (mean 2.84 against 1.77 pooled). Pair-level held-out error
  did not predict native results here.
- A global calibration cannot express a discontinuous hue remap (teal-orange measured at 92 %
  left) or recover clipped channels (punchy: 10 of 21 calibrations refused). Local edits in the
  reference are refused rather than approximated.
- The learned map converts AURA's baseline rendering to the photographer's finished rendering.
  It therefore carries the difference between AURA's raw rendering and the photographer's raw
  converter as well as the look. That is the intended behaviour: targets end up looking like the
  photographer's exports, not like AURA's baseline plus a look.
