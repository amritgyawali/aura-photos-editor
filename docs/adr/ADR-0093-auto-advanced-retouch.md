# ADR-0093: Auto advanced retouch - the professional workflow, step by step

Status: accepted
Date: 2026-10-07

## Problem

The automatic retouch (ADR-0081, ADR-0086, ADR-0090 to ADR-0092) already measures and
finishes a portrait, but it saves its work in six groups shaped by how the code grew - light
and colour, skin, blemishes, lines and redness, eyes, teeth and shine - rather than in the
order a high-end retoucher works. A photographer asked for one button that does the whole
professional workflow automatically, **one step at a time and without missing a step**:

RAW development -> lens and perspective -> background -> hair -> skin cleanup -> selective
frequency separation -> micro dodge and burn -> medium dodge and burn -> global dodge and
burn -> skin colour -> eyes, lips and teeth -> clothing -> jewellery -> background toning ->
colour grade -> grain -> output sharpening -> quality control and export.

Several of those stages had no automatic implementation at all (levelling, background
dust, stray hair, clothing marks, jewellery reflections, background toning, grain, quality
control), and the existing ones were not separated the way the workflow separates them -
for example, colour evening and luminance evening of skin lived in one history step.

## Decision

### One command, eighteen stages (`crates/aura-app/src/advanced_retouch.rs`)

`advanced_retouch::run` plans everything first, then walks the eighteen `Stage`s in order.
Each stage that changes the photograph is saved as **its own history entry**
("Auto advanced retouch 5/18 · Skin cleanup: ..."), so a photographer can go back to
"after the skin cleanup" with the history's *Go back to here*. Every stage - including the
ones that changed nothing - is written into a report stored in the recipe
(`studio_advanced_retouch_v1`) with what it **checked**, what it **changed**, and an outcome:
`applied`, `unchanged` (inspected, nothing needed), `not_applicable` (for example, no person)
or `protected` (the change would have overwritten a value a person set). "Do not miss any
step" is a property of the report: there is no code path that omits a stage from it.

The Tauri command `auto_advanced_retouch` streams an `advanced-retouch` event before and
after every stage, so the window shows the run step by step.

### Existing operations, re-filed by stage

The automatic portrait planner is reused unchanged in what it decides; `stage_of` files each
of its operations under the workflow stage it belongs to, by the operation's stable id and,
for anything unrecognised, by its tool:

| Stage | Operations |
|---|---|
| Skin cleanup | acne clear, frequency healing, spot repairs, body spots |
| Selective frequency separation | skin smoothing (low band only, pores kept), pore refine, surface finish, texture restore, line softening |
| Micro dodge and burn | micro dodge and burn, smile-line folds |
| Medium dodge and burn | skin dodge and burn (light evenness), under-eye lift, shine, skin brightness |
| Global dodge and burn | contour and highlight sculpting, face light, glow |
| Skin colour | tone evenness (chroma only), redness colour match, skin colour, body tone and match |
| Eyes, lips and teeth | eye whites, vessels, iris, red-eye, lashes and brows, lips, teeth |

`staged` writes the retouch stack in stage order, a person's own operations first, and only
the stage being saved changes - so the stack after step *n* is exactly "every stage up to
*n*". The skin colour stage is a separate step from the dodge and burn stages because the
workflow separates colour from luminosity; the underlying tools already did
(`SkinUniformity` removes luminance drift, `PortraitDodgeBurn` preserves RGB proportions).

### New measured stages (`advanced_retouch/measure.rs`)

- **RAW foundation.** The existing histogram, highlight and white-balance measurements, with
  contrast held to +-15 and vibrance, clarity and sharpening deferred to the end: the
  foundation is neutral and flexible, not a look.
- **Lens and perspective.** A horizon is levelled only when at least three long straight
  edges, tracked by phase 23's edge tracker, agree within half a degree on a tilt between
  0.35 and 5 degrees, covering at least 60 % of the measured line length. Edges running over
  people (segmented, or a generous region around and below each face) are excluded, because
  stripes on a shirt are not a horizon. More than five degrees reads as a deliberate angle.
  The crop is the largest same-aspect rectangle inside the rotated frame. No lens profile is
  measured, so distortion, vignetting and chromatic aberration are left as shot and the
  report says so. **There is no liquify**: correct photographic distortion, never redesign
  the person.
- **Background cleanup and clothing.** Phase 21's anomaly detector (`micro::clothing`) on the
  segmented background (eroded away from the person) and on each person's clothes, with a
  donor chosen beside each mark on the same clean surface (inside the region, clear of other
  marks, closest in tone to a ring around the mark and lowest in variation), healed with the
  ordinary `Heal` tool. Dust must be under 0.004 % of the frame. A search that reaches the
  detector's cap is the surface's own weave, grain or print and **cleans nothing**; marks
  next to hair are hair ends and are left. Distracting objects and people are never removed.
- **Hair.** Phase 21's flyaway detector, accepted only for thin strands clear of the hair mass
  over a quiet background, faded toward the background (contrast reduced, never erased).
  Measured on real portraits, a soft hair edge yields hundreds of candidates along the
  silhouette; above forty the edge is treated as the hair's own outline and nothing is faded,
  because fading it notched the outline in testing.
- **Jewellery and reflections.** Small, clipped specular highlights much brighter than their
  surroundings, on the clothes and in the necklace and earring band below each face (never on
  bare arms, where a highlight is skin shine), are tamed by the `Glare` tool at half strength.
  A reflection must be darker on at least ten of twelve sides (a white stripe or collar runs
  on past the spot and is not one) and must not be background seen through a gap. More than
  forty is the garment's sparkle or print and is kept.
- **Background toning.** A background clearly brighter than the subject's face (by more than
  6 % of display luminance) is lowered by half the difference, at most 12 %, through a burn
  on the eroded background matte; a white or near-white backdrop (above 85 %) is a high-key
  look and is kept. A bright sky on a landscape gets the existing sky gradient.
- **Dodge and burn visual aid.** The face skin is read in black and white with a steep
  contrast curve and measured at a micro and a medium scale; the micro and medium dodge and
  burn strengths are scaled by the square root of the reading against a typical value, within
  0.7x to 1.3x.
- **Colour grade, grain, output sharpening.** Vibrance (and clarity on non-portraits) only
  now; a fine grain (6, size 15) only when parts of a clean photograph were smoothed or
  healed; the existing skin-masked sharpening last.
- **Quality control.** After saving, the photograph is rendered at screen size with crop,
  grain and sharpening left out three times - original, foundation, retouched - and measured:
  clipped pixels (the 25 % view), skin colour drift on the segmented face skin (50 %), the
  fine skin texture kept (100 %), and how differently the two halves of each face were
  changed (the mirror check). Texture kept below 60 % softens the frequency-separation stage
  and drift above 0.012 chromaticity halves the skin colour stage, then the photograph is
  measured again and the correction is saved with the quality-control step.

### Workflow defaults (`advanced_retouch::options`)

The Professional preset's acne clear and texture restore, with identity kept: dark marks
(moles, beauty marks) and freckle fields kept, smoothing 0.35 with texture 0.9, lines
softened rather than removed, eye whites cleaned of redness rather than whitened (0.1), no
lip colour or blush, contour and highlight faint (0.15), face and body, hair, fabric and
backdrop finishing on. A caller may pass its own options.

## Measured on real photographs

`crates/aura-app/tests/advanced_retouch_photos.rs` on nine photographs - the Unsplash acne
portrait from ADR-0092 and eight Pexels photographs in `D:\aura-skin\photos` (two scenes
without a usable face, a studio white backdrop, a brick wall, a patterned shirt, glasses, a
bearded man) - through the application with an isolated catalog. Every run reported all
eighteen stages in order with two progress events each; every saved stage was its own
history entry; a second run saved no retouch stage. Release-equivalent timing (dependencies
at opt-level 2) was 1.8 to 22 s per photograph including the three quality-control renders.

| Photograph | Stages applied | Texture kept | Skin colour drift | Notes |
|---|---|---|---|---|
| Acne portrait (2048 x 3072) | 12 | 83 % | 0.005 | acne cleared, pores kept at 100 % and 200 %; soft hair edge kept |
| Studio, yellow backdrop | 11 | 92 % | 0.003 | backdrop grain refused for dust |
| Pink backdrop, blouse | 13 | 94 % | 0.001 | blouse weave refused for lint |
| Bearded man, block wall | 13 | 91 % | 0.007 | the wall seen through a gap by the arm was first taken for a reflection - fixed: the background is excluded |
| Beach, no face | 3 | - | - | horizon measured level (-0.33 degrees) |
| Striped shirt, white studio | 12 | 98 % | 0.000 | shirt stripes no longer read as a 2.4 degree tilt; white backdrop kept white; 7 tiny glints on the shirt tamed (no visible change) |
| Brick wall | 11 | 100 % | 0.001 | one lint speck healed |
| Smiling woman, patterned shirt | 14 | 88 % | 0.002 | background lowered 6 % |
| Man with glasses | 13 | 86 % | 0.001 | - |

Four defects were found by looking at the renders rather than at the numbers, and fixed
before this was accepted: phase 21's flyaway detector fading the hair's own soft edge, which
notched the outline; shirt stripes levelling a portrait by 2.4 degrees; a white studio
backdrop being lowered toward grey; and bright wall seen through a gap, or the edge of a white
stripe, taken for a reflection. Each is now a refusal with a sentence in the report.

## Consequences

- One click reaches every stage, in order, and each saved stage is undoable on its own.
- Running it again replans the same stages; operations whose values differ only below the
  stored precision (six decimals) are not re-saved, and provenance alone is not a change.
- Measured on nine real photographs (see `docs/auto-advanced-retouch.md`). The new detectors
  are deliberately conservative on real pixels: on most portraits background dust, stray
  hair and clothing marks report a textured surface or a soft hair edge and change nothing.
  That is the intended failure direction - a missed speck is left for the photographer; a
  notched hairline or a healed hair end is damage.
- Not attempted: lens profile measurement, perspective (keystone) correction, removing
  people or objects, filling brow or lash gaps, reshaping, a signature look. The report names
  each where it applies.
