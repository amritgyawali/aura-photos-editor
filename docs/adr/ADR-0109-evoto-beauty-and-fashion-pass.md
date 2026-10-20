# ADR-0109: Evoto's "Beauty & Fashion" pass, measured from its homepage, and the bright-backdrop lift

Status: accepted.
Date: 2026-10-11

## Problem

The owner asked for AURA to apply "the same logic and algorithm" as the before/after examples on
Evoto's homepage. The homepage itself cannot be reached from the build environment (the network
policy refuses evoto.ai), so the one example available is the screenshot the owner supplied: the
*Beauty & Fashion* card, three people on a grey seamless, captioned "High-end skin work, clothing
wrinkle removal, and hair refinement handled in a single automated pass", with a before/after
divider about 143 px from the left edge of a 1247 x 746 image.

## What was measured

Content is continuous across the divider, so the columns just left of it (before) and just right
of it (after), on the same rows, show the edit. Four findings:

1. **The backdrop is lifted, neutrally.** On every clean backdrop row, before is sRGB 204-213 and
   after is 231-239: a factor of 1.127-1.141 on red, green and blue alike. That is about +0.37 to
   +0.41 EV in linear light, applied as one gain, so the backdrop's own top-to-bottom fall-off is
   kept (206 → 210 before, 231 → 238 after) and nothing is clipped (the brightest after-backdrop
   pixel is about 245, not 255). Its fine noise is unchanged (about one code value either side).
2. **Stray hairs over the backdrop are removed.** The before strip carries strands beside the
   left-hand head (local deviation 37 code values on rows that read about 1 on clean backdrop);
   the after side has none there. The right-hand woman's soft wisps at the edge of her hair are
   still visible: refined, not stripped.
3. **Clothing creases are softened.** On the white shirt, the 95th percentile of high-frequency
   detail falls from 11.3 to 9.3 (about 18 %), while the mean stays (the weave is kept).
4. **Skin is worked and keeps its pores, and each person keeps their own tone.**

AURA's Auto advanced retouch (ADR-0093) already did 2, 3 and 4. It did not do 1: its background
stage only ever *lowers* a background brighter than the face, and leaves a darker one "exactly as
it was lit".

## Decision

1. A new fine control, **Bright backdrop** (`backdrop_lift`, `0..1`, default `0`). In the
   background-toning stage, `measure::backdrop_lift` reads the separated background and lifts it
   only when it is **plain** (75th percentile of fine texture under 0.012), **neutral** (channel
   spread of its median under 0.06) and **not low-key** (median at least 0.45). The target is
   Evoto's own landing point, sRGB 0.915, reached in linear light after the exposure the
   foundation stage already applies, so the two are not added together. Half strength is half the
   stops. The lift is the existing Dodge tool on the background matte - one linear gain, the same
   on every channel - so it is one editable operation and its own history step. When a backdrop
   is lifted it is not also lowered.
2. A backend-owned preset, `beauty_fashion` (`beauty_fashion_options`): the workflow's
   professional defaults with face and body scope, fabric creases 0.5, backdrop smoothing 0.5,
   hair detail and shine, and the lift at full strength. The panel offers it as **Beauty &
   Fashion (Evoto-style)** beside **Professional**, and the automatic retouch settings carry the
   same preset.
3. Nothing about identity changes: moles, freckles and every person's skin tone are kept, as in
   the default, and a test asserts it.

## Evidence and limits

- `crates/aura-app/tests/beauty_fashion.rs` runs the preset end to end on a painted portrait
  over a sRGB 206 seamless: the backdrop lands at 232.5 on all three channels (Evoto: about 233),
  +0.41 EV. That is a painted face; it says the arithmetic and the plumbing are right, not that a
  real studio frame will look like Evoto's.
- Only one homepage example could be measured. The others (if the homepage shows more use cases)
  were not seen; each needs its own screenshot before its "logic" can be claimed.
- The measurement is of an 8-bit WebP screenshot. Code values carry about ±1 of compression
  noise, which is why every finding above is stated as a range.
