# ADR-0082: Segmented face and body skin, and fifty-two automatic retouch settings

Accepted, 2026-10-01.

## Context

Automatic retouch found skin with a guess. Face skin was an oval drawn from YuNet's five
landmarks, gated at render time by colour similarity to one sampled cheek patch; body skin was
a flood fill of similar colour grown from the neck. Photographers reported the result as "not
detecting face and body skin properly": the oval reached hair, beard and background, shadowed
skin fell outside the colour tolerance, a necklace or strap stopped the body fill, and turned,
tilted or partly covered faces were skipped outright because the landmark sampler refused
them. The automatic pass also exposed only four switches and a strength, where the tools
photographers compare it with (Imagen, Retouch4me, Skinfiner) expose dozens of named controls.

## Decision

1. **A bundled, open-source person segmenter finds the skin.** Google MediaPipe's Selfie
   Multiclass segmenter (Apache-2.0) labels each pixel background, hair, body skin, face skin,
   clothes or other. It is converted to ONNX opset 13 by
   `ml/models/skin/convert_selfie_multiclass.py` and runs on aura-infer, offline, like YuNet.
   aura-infer gains `ConvTranspose`, `ReduceSum` and half-pixel bilinear `Resize`, all
   deterministic (fixed accumulation order, parallel over output rows only), and its `Conv`,
   broadcasting and `Transpose` loops were restructured without changing any result bit.
   Parity with onnxruntime is a unit test.
2. **Measured on the photograph, per person.** `aura_vision::skin::analyse` runs the whole
   frame plus a head-and-torso crop for each small face, refines every class with a guided
   filter on the image's own luminance, gates face skin by the *same person's* measured skin
   colour and brightness (so beard, stubble, brows, lips and eyes drop out - never by a
   reference complexion), re-gates after refinement so thin hair is not filled back in, and
   assigns connected body-skin, hair and clothes regions to the nearest detected face.
3. **Mattes travel in the recipe, so rendering stays deterministic.** A matte is stored once
   under `studio_retouch_mattes_v1` (at most 256 cells on the long side, 16-level run-length,
   base64) and operations refer to it by id through the new optional `Edit::matte`. The
   renderer runs no model: it upsamples the matte, re-derives the uncertain edge band from the
   photograph's colours (a two-colour projection, then a small guided filter) at whatever
   resolution it renders, and multiplies it into the operation's coverage. A missing matte
   skips the operation rather than applying it to its whole region. Writing a stack drops
   mattes no operation uses. Old recipes have no mattes and render exactly as before.
4. **Hair is never retouched as skin.** Inside a matte, smoothing and every guarded tool skip
   pixels far darker than the region's own reference (lashes, brow hairs, beard), because a
   256-cell matte is coarser than a lash.
5. **Fallbacks are explicit.** If the segmenter is switched off, disabled on the device
   (`AURA_DISABLE_SKIN_SEGMENTATION=1`), fails, or finds no face skin for a face, that face
   uses the previous landmark and colour-sampling path, and the report says which was used. A
   face the landmark sampler refuses (oblique, occluded, small, textured) but the segmenter
   found is now retouched from the segmented skin; landmark-measured features (spots, eyes,
   teeth, lines) still need a frontal face and are left alone on it.
6. **Fifty-two named settings** (`retouch_settings::Settings`, `#[serde(default)]`) plus the
   existing strength, four switches and scope. Measured corrections take a multiplier where
   `0.5` is the measured strength; added operations are off unless chosen. Groups: skin
   detection (5), skin (13), blemishes (3), lines (5), under eyes (2), eyes and brows (7),
   mouth (3), portrait volumes and make-up (4), body (6), hair, clothes and backdrop (4). The UI
   adds nine presets (Natural, Subtle, Soft glow, Polished beauty, Bridal, Groom & men,
   Editorial, Body focus, Studio clean). Every result is an ordinary operation with a stable
   id, grouped into the existing history steps.
7. **Limits that are rules, not defaults.** No setting reshapes a face or a body. Skin colour
   settings are neutral by default and move a person's skin relative to itself. Body skin is
   matched only toward the *same person's* face. Contour, highlight, blush and the extra skin
   operations exist only with a segmented face, because without one they would reach hair and
   background. Backdrop smoothing runs only on a measured plain backdrop, eroded away from the
   subject so no colour bleeds across; fabric works on eroded clothing.
8. **Red-eye is red with green and blue about equal.** Both the detector and the renderer now
   require it, after the real-photo check found a brown iris read as red-eye and painted green.

## Consequences

- Skin selection follows jaw lines, hairlines and shoulders and excludes beards and
  backgrounds on the 22 development photographs; turned faces are retouched instead of skipped.
- Analysis costs about 1.7 s per network pass (4.2 GMAC) with aura-infer at `opt-level = 3`,
  which the workspace dev profile now sets; group photos add a pass per small face (at most
  six crops).
- Recipes that use automatic retouch grow by a few kilobytes per person for the mattes.
- The model adds 16 MB to the binary. `include_bytes!` is a `static`, so it is not copied into
  dependent crates' metadata.
- Not proven: a demographic accuracy study of the segmenter. The model card says so.
