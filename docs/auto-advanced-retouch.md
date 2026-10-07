# Auto advanced retouch, in the product's own words

Open a photo in **Retouch** and press **Auto advanced retouch**. AURA works through the whole
professional retouching workflow on its own, in the order a high-end portrait retoucher
works, one step after another. You watch each step run. Nothing is skipped: every step is
inspected and reported, and every step that changes your photo is saved as its own entry in
the history, so you can go back to any point - "after the skin cleanup", "before the grade" -
and carry on by hand from there. Your original file is never changed and nothing leaves your
computer.

## The eighteen steps

| # | Step | What AURA checks | What it may change |
|---|---|---|---|
| 1 | RAW foundation | Brightness spread, highlights, shadows, neutral areas with faces ignored, noise, the kind of scene | Exposure, highlights, shadows, whites, blacks, white balance, dehaze, noise reduction. Contrast stays moderate: the foundation is neutral, not a look |
| 2 | Lens & perspective | Long straight lines in the background - a horizon, a door frame, a wall | Levels a tilt between 0.35 and 5 degrees when the lines agree, cropping only enough to hide the corners. Lines on people never count. Larger angles are treated as deliberate |
| 3 | Background cleanup | Dust and specks on a plain backdrop at 100 % | Each speck healed from clean backdrop beside it. A textured background is left alone, and objects and people are never removed |
| 4 | Hair cleanup | Strands outside the hair shape over a quiet background | Clear, separate strays faded toward the background; hair detail and shine. A soft hair edge is the hairstyle and is kept |
| 5 | Skin cleanup | Every mark against the clean skin around it | Pimples, redness and flakes cleared with the pores kept. Moles, beauty marks and freckles are kept |
| 6 | Selective frequency separation | Uneven tone versus fine texture, at a size measured from each face | Uneven tone corrected, never a blur; lines softened, not removed; the person's own pores restored where healing flattened them |
| 7 | Micro dodge & burn | The skin in black and white with exaggerated contrast (the retoucher's visual aid) | Small dark and bright irregularities evened, colour untouched |
| 8 | Medium dodge & burn | Cheeks, forehead, under the eyes, jaw, shiny hot spots | Patchy light evened, under-eye shadows lifted toward the cheek, shine softened |
| 9 | Global dodge & burn | The light that was there | Faint cheekbone light and jaw shadow along that light. No make-up is painted with light |
| 10 | Skin colour | Redness, blotches and casts against this person's own skin - never an ideal tone | Colour evened with brightness untouched; body skin matched to the face |
| 11 | Eyes, lips & teeth | Red eye whites, iris, teeth colour, lips | Redness out of the whites (not painted white), iris and catchlight lifted a little, teeth less yellow (not white), lip texture kept. Brows and lashes are not filled |
| 12 | Clothing | Creases, lint, threads and small stains on the clothes | Creases softened, small marks healed from the fabric beside them. Patterned fabric and hair ends lying on it are left alone |
| 13 | Jewellery & reflections | Burnt-out reflections on the outfit and in the necklace and earring area | Tamed so the metal or stone shows again; sparkle and sequins are kept |
| 14 | Background toning | Whether the background is brighter than the person's face; a bright sky | A background that pulls the eye away is lowered gently; a bright sky balanced. A white backdrop stays white |
| 15 | Colour grade | - | Vibrance (and clarity on scenes without people), only now that the photo is corrected. AURA does not invent a signature look: add yours with an edit profile in Develop |
| 16 | Grain | Whether the photo is clean and parts were smoothed or healed | A fine, even grain that ties retouched and untouched areas together |
| 17 | Output sharpening | - | Sharpening with skin masked out, after every retouch |
| 18 | Quality control & export | The finished photo against the corrected one: clipping, skin colour drift, how much fine skin texture was kept, and whether one half of a face was changed more than the other | If too much texture was lost the skin smoothing is softened, and if skin colour drifted the colour step is halved - then measured again |

Every step reports one of four results: **done** (saved as its own step), **checked -
nothing needed**, **not applicable** (for example, no person in a landscape) or **kept your
own settings** (a value you set by hand is never overwritten).

## What it will never do

- Reshape a face or a body. There is no liquify in this workflow.
- Remove moles, beauty marks or freckles, or measure your skin against an ideal colour.
- Blur the skin. Texture is kept, restored and measured.
- Remove people or objects from the background.
- Choose where your photo is exported. Export the master as a 16-bit TIFF in Adobe RGB (the
  *album* preset) and web or social copies in sRGB from the Export step.

## When a step does nothing

Several steps are cautious on purpose. On a background with any texture, at 100 % its grain
looks like hundreds of specks, so nothing is cleaned off it; a soft hair edge looks like
hundreds of strays, so the outline is kept; a print or a weave looks like lint. In each case
the step says so. A speck you still see is for you to remove with the Heal tool or the
Blemish brush - the opposite mistake, a notched hairline or a healed hair end, is damage.

## Running it again

Running it again replans every step. If nothing about the photo has changed, nothing new is
saved. Your own retouch operations are kept, before AURA's.

## How it was checked

`crates/aura-app/tests/advanced_retouch_photos.rs` imports real photographs into an isolated
catalog, runs the workflow through the application and checks that all eighteen steps are
reported in order, that progress arrives twice per step, that each saved step is its own
history entry and that a second run changes no retouch step. The renders it writes were
inspected at 100 % and 200 %. See ADR-0093 for the measurements.
