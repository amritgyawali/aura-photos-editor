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

Natural · Subtle · Soft glow · Polished beauty · Bridal · Groom & men · Editorial ·
Body focus · Studio clean. A preset only sets the controls below; change any of them and it
becomes *Custom*.

## The controls (58)

**Always visible:** what to retouch (face / body skin / both), overall strength, heal
blemishes, soften lines and redness, eyes, teeth.

**Skin detection** - AI skin detection (on: segmenter; off: landmarks and sampled colour),
main subject only, skin mask precision, mask edge softness, protect beard and stubble.

**Skin** - skin smoothing, keep pore texture, smoothing size, even skin tone, even skin light,
micro dodge & burn, refine pores, reduce shine, reduce redness, skin glow, skin brightness,
skin warmth, skin tint.

**Blemishes** - blemish sensitivity, most spots per face, keep freckles.

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

## When the AI cannot see the skin

If the segmenter is off or finds no skin for a face, that face is retouched the earlier way
(landmarks plus this person's sampled colour) and the explanation under the result says so.
Make-up, contour and the extra skin operations need the AI mask and are skipped in that case.
