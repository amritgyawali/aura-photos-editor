# ADR-0190: White balance on every photo, measured and checked on the render

Status: accepted.
Date: 2026-10-29

## Problem

The photographer exported the 69 photos of a 100-photo wedding run and found the whites cream
and the photos flat and grey. The catalogue of that run said why:

* **White balance was corrected on none of the 69 photos.** The Auto enhance measurement the
  advanced retouch's foundation uses (`smart_edit::white_balance`) refuses whenever it is unsure:
  53 photos "the frame's whites do not show the same cast", 7 "the neutral areas and the edges
  disagree", 5 "not enough reliable neutral areas", 3 already neutral, 1 strong colour without
  people. It is also called with `developed = true` for camera files, the strictest floor.
* **A blown highlight was taken as the light.** The groom's white shirt in `5I7A8910.CR2` is
  clipped in the camera file. The renderer rebuilds a clipped highlight as an exact grey -
  (238, 238, 238) at any white balance - so every neutral estimate read "already neutral".
* **The whites ended grey.** Exposure came down 0.70 EV because the bride's face was near
  clipping, and nothing brought the brightest tones back: they reached 75 % of white.

The photographer's own Lightroom catalogues on this machine (26,494 photos, 20,371 hand-developed
RAWs, `docs/lightroom-catalogue-study-2026-10-29.md`) show what they do instead: a custom white
balance on 86-100 % of photos in every lighting condition, varied photo by photo (about 590 K
between the 10th and 90th percentile inside one shoot), and the Whites slider moved per photo.

## Decision

The full automatic edit (`full_edit::run`) gains two measured steps between the advanced retouch
and calibration, both decided and checked on the 960 px render calibration already uses:

### Stage 2, Colour correction: white balance, every photo (`balance_white`)

1. **Measured from the camera's own balance** (5500 K / tint 0, which multiplies by one), so a
   second run measures the same pixels and decides the same thing.
2. **Neutral references** (`read_neutrals`): pixels outside the faces (boxes grown 20 %), not
   clipped, not dark, saturation at most 0.30 - and not a **rebuilt clipped highlight**
   (channels within 2 levels at 140 or brighter), which shows the reconstruction, not the light.
   The search starts from the brightest fifth of them (whites take the light's colour and little
   else) and narrows onto the cluster around them (radius 0.2, 0.12, 0.08 in log chroma), so a red
   sari, gold work or skin does not pull it. At least 0.4 % of the frame must be references.
3. **One vote per part of the frame** (3x3 grid): a large canopy must not outvote the rest by its
   area. When fewer than 60 % of the parts agree within 0.10, the light is mixed and half the
   correction is made.
4. **The goal**: remove the cast, keeping 15 % of a warm one (a tungsten room still reads warm,
   its whites white); the magenta-green part comes off at most 0.08 in one edit, because a pink
   canopy or green lawn fools every neutral estimate the same way; at most 0.6 in total.
5. **Solved and checked**: temperature and tint are solved from the renderer's own multipliers
   (`aura_render::colour::white_balance`), the photo is rendered, and the same reference pixels
   are read again; up to three renders step by what is left. A correction is kept only when the
   references moved at least 15 % closer to neutral, the faces kept a skin hue and turned at
   most 5°, and blown highlights covering 2 % or more of the frame (a sky) stayed within 0.07 of
   white. Reductions scale towards the camera's balance, never towards the value in place.
6. A look that was applied (an edit profile or provider) keeps its own white balance; a person's
   own white balance is kept by `schema::merge` and reported as kept.

### Stage 1, Basic edit: whites short of white (`lift_whites`)

When the brightest tones outside the faces (99.5th percentile) reach less than 80 % of white in a
frame that is not dark (median at least 25 %) and nothing is clipped, the Whites slider is lifted
towards 92 %, stepping by its measured response, at most +70, and never past 0.4 % clipped.

Both steps report under their stage, are their own history steps, and send a live picture
(ADR-0187 addendum) when someone is watching.

## Measured

Twelve real photos (ten CR2 from the 100-photo wedding, two CR3), before and after, with the old
edit beside them (`AURA_FULL_EDIT_VERBOSE=1` on `full_edit_photos.rs` prints every decision):

* White balance corrected on 11 of 12 (the twelfth measured already balanced); before: 0.
* `5I7A8910`: the blown shirt no longer counts; warm cast 0.073 corrected to 0.018; whites
  75 % to 85 %.
* `5I7A8961` (pink canopy, daylight behind): the full correction turned the sky cyan; the
  blown-highlight check held it to 5228 K / tint -12 and the sky stays white.
* `5I7A8986` (warm cast 0.53): the groom's outfit white instead of cream; faces natural.
* A repeat run changes no white balance or whites on any of them.

Unit tests in `full_edit.rs`: the solver round-trips temperature and tint; a warm shirt beside a
red sari reads the shirt's light; a rebuilt clipped highlight is not a witness and is recorded
as blown; faces are not references; the goal keeps warmth and limits magenta-green; every part of
a frame under one light votes for it; whites short of white are read outside the faces.

## Consequences

* Every photo gets a measured white balance; the Auto enhance measurement is unchanged and still
  used by Auto enhance itself.
* Two to four more 960 px renders a photo (about 2-5 s on a CR2).
* The renderer still tints a blown highlight by the white balance multipliers (it clips after
  balance). Keeping blown highlights white whatever the balance is a renderer change with a
  `PIPELINE_VER` bump, left for its own ADR; until then the check above bounds it.
