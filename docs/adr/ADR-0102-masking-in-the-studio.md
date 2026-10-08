# ADR-0102: Masking in the Studio

Status: accepted
Date: 2026-10-08

## Problem

Local adjustments are one of the main things Lightroom offers: select the subject, the sky,
the background or one person's hair, then brighten, warm or add detail to that part alone.
AURA had no masking in the Studio, the editor photographers actually use. The recipe's frozen
`masks` list could not express it either:

- linear and radial masks rendered at a fixed default position, because the list stores no
  placement;
- there was no brush;
- masks could not be combined;
- the clarity, texture and tint sliders a mask carries were ignored;
- subject, background and sky came from a colour model.

That colour model was measured on five photographs. It selected nothing on a beach wedding, a
rectangle on a studio portrait, and nothing on a couple under a veil.

## Decision

### Storage

Masks are a recipe extension, `studio_masks_v1` in `aura_recipe::local_masks`, beside the
retouch stack's `studio_retouch_v1`. The frozen `Recipe::masks` is left as it is. Changing it
would need its own ADR and a re-lock, and the extension map exists for exactly this.

Each mask has:

- a name, an on/off switch, an amount, and the frozen `MaskParams` slider block (absent still
  means "not touched here");
- a list of components, each added, subtracted or intersected with what is built so far, and
  each optionally inverted. The component types are:
  - an **AI selection**, stored as a matte in `studio_masks_mattes_v1`;
  - a **portrait region**, re-measured on every render;
  - a **linear gradient**;
  - a **radial gradient**;
  - **brush strokes**;
  - a **brightness range**.

### Where the AI selections come from

All of them are measured once, when the photographer asks, and stored. A later build can never
move a selection someone has already adjusted.

- **Subject, background, face skin, body skin, hair, clothes** come from the person segmenter
  that already ships (ADR-0082, MediaPipe Selfie Multiclass, Apache-2.0). It also runs on a crop
  around each face. On the five photographs it selected the woman on the road (seen from behind,
  so with no face), the couple under the veil, and every person in the office scene.
- **Sky** is measured, not learned (`aura_vision::sky`): Shen and Wang's horizon search, plus
  checks that turn a wrong answer into "no sky found" with a reason. Those checks are:
  - the region must have ground under it;
  - the border must be a real horizon, with a change of colour across it;
  - the region must be brighter and smoother than the ground.

  It finds open sky above trees and roofs. It declines a sky full of strong cloud edges, a
  night sky, a studio backdrop and a ceiling. On the five photographs it found the road's sky,
  declined the cloudy sunset and the veil, and made no false selection.
- **Eyes, irises, whites of the eyes, brows, lips, teeth, beard** come from the portrait parse
  (ADR-0065), measured from the pixels at render time. They are only offered when a face is
  found.

### Rendering (`aura_render::local_masks`)

Each mask becomes one weight plane:

- AI mattes are refined at render resolution by the guided filter the retouch mattes use, so an
  edge follows hair and leaves at any size.
- Gradients and the ellipse are computed in pixel space.
- Brush strokes use the retouch brush's own painting code.

The sliders are applied at that weight: exposure, contrast, highlights, shadows, whites, blacks,
temperature, tint, saturation, clarity and texture. Each parameter is scaled by the weight
rather than the result being blended, so overlapping masks add, as in Lightroom.

Masks render **after the retouch stack**, and are kept out of the stack's checkpoint key
(ADR-0098). Moving a mask slider on a retouched portrait therefore re-runs only the masks and
what follows them; it does not re-run the 10 s stack. A frame with masks is rendered whole, not
in tiles, because a guided edge and a brush stroke are drawn on the whole photograph.

### The panel

The panel is a **Masking** section in the Studio. It offers:

- one-click Subject, Sky and Background;
- a People & face menu;
- Linear gradient, Radial gradient and Brush, drawn on the photograph. The brush has size,
  feather, flow and erase, and further strokes paint into the same brush;
- for the selected mask: Add, Subtract or Intersect with any selection, Invert, "Bright parts
  only" and "Dark parts only", Amount, the eleven sliders, rename, hide and delete;
- a red overlay showing exactly the coverage the renderer uses, cropped to the photograph as it
  is displayed.

A selection that finds nothing adds no mask and says why.

## What this does not do yet

*Superseded in part by ADR-0103: Objects, a learned sky and a learned subject now run on ONNX
Runtime. The first item below is kept as it was written.*

- **Lightroom's Objects tool** (draw around a car and get the car), **a learned sky model** and
  **a subject model trained for "the main subject" rather than "people"**. Each needs a
  segmentation network of a size the bundled pure-Rust interpreter (ADR-0007) runs too slowly:
  - BiRefNet-lite and U²-Net take seconds to minutes per image on the interpreter;
  - SAM 2.1-tiny is the model for clicked objects.

  The choice is between linking ONNX Runtime, which reverses ADR-0007, and accepting those
  speeds. That is the photographer's call and is recorded as the next step.
- **Per-person masks.** "People" is everyone in the frame.
- **Lightroom's remaining mask sliders:** dehaze, sharpness, noise, moiré, defringe and colour
  overlay.
- **Masks are drawn on the uncropped photograph.** The overlay and the drawing tools map through
  the crop, but not through a rotation or a perspective correction.

## Tests

- `aura-recipe`: masks round-trip; unused mattes are dropped; invalid geometry or values are
  refused.
- `aura-render`: gradients, ellipse, inversion, add, subtract and intersect, and sliders applied
  only inside the mask.
- `aura-vision::sky`: a landscape has sky; a studio backdrop and a ceiling do not.
- `aura-app/tests/local_masks.rs`, end to end:
  - a gradient brightens one side;
  - subtracting an ellipse restores that part;
  - the overlay matches the render;
  - removing the mask restores the photograph byte for byte.
- The same file's ignored test creates every AI selection on real photographs and writes the
  overlays.
- The UI's `MasksPanel.test.tsx`.
