# Automatic retouch settings

Retouch → **Automatic retouch** finds every face, finds each person's face skin and body skin
with the bundled AI segmenter, measures what the skin needs, and adds ordinary retouch
operations you can adjust, switch off or remove. Pick what to retouch (Face, Body skin,
Face + body skin), choose a preset, and open any group to fine-tune it. Running it again
replaces only the automatic operations; anything you added yourself is kept.

Strengths run from 0 % (off) to 100 %. For corrections AURA measures - smoothing, tone, light,
lines, under-eyes, eyes, teeth, shine - **50 % is the measured amount** and 100 % doubles it
within each tool's limits. Everything else is off until you turn it up.

## Presets

Natural · Deep acne cleanup · Professional retouch · Subtle · Soft glow · Polished beauty ·
Bridal · Groom & men · Editorial · Body focus · Studio clean. A preset only sets the controls
below; change any of them and it becomes *Custom*.

### Professional retouch

The order a retoucher works in, as one press:

1. **Frequency healing** rebuilds the tone under measured compact marks from nearby clean
   skin while retaining pore texture. It runs first on the original photograph, with
   eye and nose exclusions applied before planning residual repairs.
2. **Skin smoothing, tone and light evening, micro dodge & burn** - gentle: 35 % smoothing
   and no pore refinement. Healing does not guarantee every blemish was removed.
3. **Local donor spot repairs** address remaining supported marks. Donors, lighting fits,
   full skin clearance and protected structures are checked before a repair is saved.
4. **The frequency-separation finish**, which evens mid-scale unevenness and keeps the
   fine band.
5. **Restore skin texture** last: original pore detail is restored where smoothing removed
   it, with glints and deep pits limited. Healed blemishes can borrow clean donor texture.

The manual **Blemish brush** uses the newer Acne Clear renderer for painted selections.
The automatic planner currently retains the independently validated frequency-healing and
local-donor route. Historical Acne Clear automatic benchmark results describe a different
planner revision. See [current workflow status](evoto-style-workflows.md) and the
[native validation record](retouch-recovery-validation.md).

Steps 1, 4 and 5 work on the whole face, including the side in shadow. A segmenter is often
unsure of skin in deep shadow - most often on darker skin - so where it left skin out, any
skin-coloured area inside the outline of the detected face that touches the rest of the
skin is included as well. A neck or an ear outside that outline is not.

Dark-mark removal is part of this preset and can also remove a freckle or a beauty mark;
switch *Remove dark marks* off to keep them. Every step is an ordinary operation in the
list below the photograph: open it to change it, or switch it off.
[ADR-0090](adr/ADR-0090-frequency-healing-and-the-texture-graft.md).

## Reuse and refine detected skin

Run **Automatic retouch** with **Face + body skin**, then choose **Use detected skin**
in the retouch controls. Each available face/body selection is listed by person. The chosen
mask and its sampled reference can be reused with any tool; changing the selection does not
change that tool's strength. **Preview selection mask** shows actual coverage.

An attached AI mask limits the brush, ellipse and brightness selections. **Remove AI mask
restriction** removes that limit while keeping the authored shape; **Select entire photo**
also removes it. Use the eraser to protect additional details inside a detected region.

Changing or disabling an automatic step makes it a manual override. A later automatic pass
keeps that override and does not create a second copy of the same automatic step. Its mask
is snapshotted so a new detection cannot move the manual correction. Undo restores the prior
step. Removing the override allows a later pass to plan that step again.

Body skin can be smoothed and evened from its own samples when face sampling fails. Only
**Match body to face** requires a usable face sample. Eyes, teeth and blemish finishing can
run independently with skin smoothing, tone evening and light evening switched off.

## Reference approaches

The local detector uses the open-source [MediaPipe person segmenter](https://developers.google.com/edge/mediapipe/solutions/vision/image_segmenter),
with separate face-skin and body-skin classes. The controls follow the useful separation of
mask refinement and tone/texture controls in [SkinFiner](https://www.photo-toolbox.com/product/skinfiner/),
targeted healing/light balancing in [Retouch4me](https://global.retouch4.me/retouchplugins),
and restrained finishing styles in [Imagen](https://support.imagen-ai.com/hc/en-us/articles/36160335937949-Retouch-all-faces-in-your-gallery).
Those products are workflow references; their proprietary code and models are not used.

## The controls (60)

**Always visible:** what to retouch (face / body skin / both), overall strength, heal
blemishes, soften lines and redness, eyes, teeth.

**Skin detection** - AI skin detection (on: segmenter; off: landmarks and sampled colour),
main subject only, skin mask precision, mask edge softness, protect beard and stubble.

**Skin** - skin smoothing, keep pore texture, smoothing size, even skin tone, even skin light,
micro dodge & burn, refine pores, reduce shine, reduce redness, skin glow, skin brightness,
skin warmth, skin tint, restore skin texture.

**Blemishes** - deep blemish cleanup, remove dark marks, acne clear, blemish
sensitivity, most spots per face, keep freckles.

*Acne clear* is on in Acne only, Deep acne cleanup and Professional retouch, and off at 0%
elsewhere; *Restore skin texture* is on only in Professional retouch. Acne clear's strength is
how completely the tone under a mark is rebuilt; restore skin texture runs from 60% of the detail the photograph had (at the lowest
setting) to all of it.

**Lines & wrinkles** - forehead lines, crow's feet, smile lines, under-eye lines, neck lines.

**Under eyes** - dark circles, eye bags.

**Eyes & brows** - whiten eyes, eye vessels, iris detail, iris brilliance, fix red-eye, lash
definition, brow definition.

**Mouth** - whiten teeth, lip colour, lip definition.

**Portrait volumes & make-up** - contour, highlight, blush, face fill light.

**Body** - body smoothing, even body tone, match body to face, body shine, red hands &
elbows, body blemishes.

**Hair, clothes & backdrop** - hair detail, hair shine, fabric creases, clean backdrop.

## What AURA will not do

- Change the shape of a face or a body.
- Compare anybody's skin with an ideal colour. Tone evening and "match body to face" move
  a person's skin toward *their own* skin; warmth, tint and brightness are neutral unless you
  move them.
- Remove a mole, a birthmark or a tattoo automatically. Darker marks that are not redder than
  the skin around them are kept and counted; a field of many small marks is treated as
  freckles and kept unless you switch that off.
- Smooth beard, stubble, brows or lashes: they are outside the skin mask and protected again
  when the photograph is rendered.
- Invent skin texture. Restore skin texture borrows pore detail from the same face in the
  same photograph, and does nothing for skin that has none.

## When the AI cannot see the skin

If the segmenter is off or finds no skin for a face, that face is retouched the earlier way
(landmarks plus this person's sampled colour) and the explanation under the result says so.
Make-up, contour and the extra skin operations need the AI mask and are skipped in that case.
