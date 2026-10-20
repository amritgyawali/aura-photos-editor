# Framing and colour inspection

In **Color & light**, open **Crop & framing studio**. Free crop handles and
numeric edges prepare a draft on an uncropped original preview. Choose **Apply
framing** to save crop and rotation as one history edit. **Discard framing
draft** restores the saved geometry.

Choose **Measure source and subjects** for original pixel dimensions, available
faces and subject silhouette. Measurements are discarded if the photo or saved
recipe changes. A measurement alone saves nothing. Missing subjects or eyes
disable the corresponding placement/alignment options.

- Custom ratio accepts your own width and height; **Follow original orientation**
  chooses the portrait/landscape inverse ratio for this photo.
- Exact pixel crop makes the requested source-sized rectangle. It refuses sizes
  larger than the original and fractional pixel dimensions.
- Physical print sizing converts millimetres and PPI to pixel dimensions and
  prepares a matching crop. Apply framing, then export the saved print to a
  separate destination. The export fits within the requested dimensions,
  never upscales, and must complete its native verification before reporting
  success. PPI here is a sizing calculation, not an embedded resolution tag or
  certification of passport/identity-document compliance.
  The native export service accepts requested fit dimensions of 240 to 20,000
  pixels; smaller cropped originals remain smaller because export never upscales.
- Subject and face placement centre the requested ratio on measured content.
  Portrait rules adjust face size, top margin and horizontal position together.
  Infeasible ratios that would cut the measured face are refused. These rules
  use the face box; they do not identify the extent of hats or headwear.
- Eye alignment requires two measured eyes. Subject-axis alignment uses the
  silhouette's long axis and may be inappropriate for leaning people or groups.
  Review rotation corners. These are explicit photographer actions.
- Framing templates store ratios, orientation and portrait rules locally on this
  device. Invalid stored data is reported and can be cleared. There is no cloud
  sync or competitor preset interchange.

Source-coordinate crop placement is disabled with active rotation/perspective;
reset those first to align the original-preview handles with the saved crop.

Above the photo, **Colour only** renders a temporary full-quality view with
colour adjustments retained and retouch, cleanup, restoration, reshaping,
sharpening, texture, clarity, grain and vignette omitted. Crop and rotation stay
so comparisons keep the composition. No saved recipe is changed.

Choose **Inspect RGB / Lab point**, then click the displayed photo. The readout
shows the rendered sRGB bytes and CIE Lab relative to D65. It is not a RAW sensor
reading. Split comparison disables point inspection to avoid ambiguous halves.

These are independent counterparts/extensions to Evoto's documented
[crop workflows](https://support.evoto.ai/crop/) and
[colour-only view](https://support.evoto.ai/color-adjustments-module/).
Source-orientation handling applies to the current photo; batch crop rules,
generative expansion, headwear parsing and exact Evoto behaviour remain gaps.
