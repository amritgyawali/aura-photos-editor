# ADR-0101: Keep the colour and the depth of a face

Status: accepted
Date: 2026-10-08

## Problem

Auto advanced retouch was run on 28 photographs: 23 portraits, a group, five wedding frames
with small or turned faces, and the acne portrait. The test was
`crates/aura-app/tests/advanced_retouch_photos.rs`, with each face compared before and after at
100 % and 200 %. Marks were cleared and pores kept, but many faces came out **paler, greyer and
flatter**, which no photographer would deliver. Three separate defects caused it.

1. **White balance moved colour that belonged to the scene.** On a pink studio backdrop the
   automatic white balance cooled the frame by 625 K, added a tint of -19, and turned the skin
   grey. On a sunset beach it cooled the sunset. On a portrait against a blue-grey painted wall it
   warmed the frame by 970 K. In all three, the two estimates in `smart_edit::white_balance`
   (grey pixels and grey edges) agreed because one coloured surface dominated both. The frame's
   whites did not confirm a cast, but the correction was applied anyway, at 65 %.
2. **The acne pass's redness evening also evened brown.** Brown is what skin turns where it is
   shaded, what contour and bronzer are, and what a tan is. Evening it flattened contoured noses,
   lightened the edge of faces and took out cheek blush. Rendering each operation alone showed
   that this single step caused almost all of the flattening: on the contoured beauty portrait,
   switching the evening off removed the difference entirely.
3. **The smoothing finish could reach a nostril.** Its selection left out a disk around the
   nose landmark, which is too small on a turned face. On a dark-skinned portrait, the finish
   filled one nostril opening with skin tone.

## Decision

1. **Correct a cast only when the frame's whites show it** (`smart_edit::white_balance`). When
   the brightest neutral tones do not show the same cast as the two estimates, the colour is the
   scene's (a backdrop, a wall, golden light) and is kept, with a sentence saying so. A
   confirmed cast is removed at 85 %, or at 70 % when it is strong, as before.
2. **Even redness only** (`retouch_acne::even_redness`). Red and brown are both still used to
   find the clean skin, so a brown patch never becomes someone's reference, but only the red
   axis is moved. A brown mark small enough to be a mark is still rebuilt by the mark pass.
   Two narrower alternatives were tried and rejected. Evening brown only near rebuilt marks
   still paled a contoured face, because the mark pass finds "marks" in the texture of make-up.
   Evening it only where it is patchy did the same. Subtracting the colour that comes with
   shading kept part of the depth but could mistake a lone dark blotch for shading. On the acne
   portrait, red-only evening removes as much as the original at 100 % and 200 %.
3. **Leave out the nostrils wherever they are** (`deep_blemish::surface_selection`). Across
   the base of the nose, wherever the face has turned it, any cell whose brightest channel is
   under a fifth of the local skin's in linear light (about half the sRGB code value) is
   excluded, with a margin of 5 % of the eye distance. This is the brow guard's idea. Two
   choices matter, and the acne portrait failed on each until they were made:
   - The test uses the **brightest channel**, not luminance. An inflamed mark loses its green
     and can be as dark as a shadow in luminance, but it keeps its red; a nostril opening loses
     every channel.
   - The test applies **only across the base of the nose**. The shadowed side of a nose above
     it can be as dark, and its marks belong to acne clear.

Tests:

- a cast the whites confirm is corrected;
- a colour the whites do not confirm is kept;
- a broad brown contour is left alone by the evening (this test fails on the old code: the
  contour's brown moved from 0.49 to 0.37);
- a nostril away from the landmark is left out, while a red mark beside the nose, a dark
  inflamed one across its base, and one on its shadowed side all stay in.

## Measured

The same 28 photographs were run end to end after the change, and every face was compared at
100 % and 200 % with the original and with the previous result:

- **All 28 runs pass** the pipeline's own checks: every stage in order, and a second run
  changes nothing.
- **Contoured beauty portrait.** The previous result's flat nose and pale face edge are gone;
  the contour and blush stay.
- **Woman with blush.** The cheeks keep their colour.
- **Pink backdrop.** The skin keeps its colour. The automatic white balance had moved the
  face's red-green balance by -0.18 (log ratio); it is now left alone.
- **Dark-skinned portrait.** Both nostrils keep their openings.
- **Acne portrait.** Every mark, the dense cluster beside the nose and the redness around them
  are still removed, as before.

The scripts used are `crates/aura-app/tests/advanced_retouch_photos.rs` and the per-operation
renders described above.

These are 28 public photographs covering a range of complexions, deep skin included. That is
a spot check judged by eye, not the per-tone study `docs/skin-fairness.md` asks for.
