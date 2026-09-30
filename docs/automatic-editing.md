# Automatic editing, in the product's own words

Press **Auto enhance photo** and AURA finishes the photograph on its own. It looks at the
picture, decides what it needs, and saves the result as a short list of steps you can walk
back through. Nothing leaves your computer and your original file is never changed.

## What happens, step by step

| Step | What AURA measures | What it changes |
|---|---|---|
| 1. Light & colour | Brightness spread, neutral areas (faces ignored), how colourful the frame already is, haze, noise, whether it is a portrait, landscape, low-light or general scene | Exposure, highlights, shadows, contrast, white balance (only part of a cast is removed, so warm rooms stay warm), vibrance, clarity (never on portraits), dehaze, noise reduction, sharpening (skin masked on portraits) |
| 2. Sky balance | A bright sky above a measured horizon | A soft gradient that darkens only the bright sky pixels |
| 3. Skin | Each face's own skin, sampled from its cheeks and forehead | Texture smoothing that keeps fine detail, tone evening, gentle local light |
| 4. Blemishes | Small spots that are **redder** than the skin around them | Each spot healed separately with nearby skin. Darker marks that are not redder — moles, freckles, beauty marks — are kept and counted |
| 5. Eyes | Whether an eye is open, whether the white of the eye is red, flash red-eye, and whether the under-eye is darker than the same cheek | Subtle iris detail, redness reduction, red-eye correction, under-eye lift — each only when measured |
| 6. Teeth & shine | Visible teeth and their yellow cast; shiny skin highlights | Natural whitening that leaves lips alone; shine softened without flattening the skin |

A step that finds nothing to do is not saved. Running Auto enhance again on an unchanged
photo saves nothing new.

## Going back and editing by hand

- **Undo** walks back one automatic step at a time.
- In **Review every edit on this photo**, each step has **Go back to here**. Going back
  discards nothing: Redo still moves forward, and a new edit simply continues from the step
  you chose.
- In **Retouch**, every automatic operation is marked *Auto (face 1)*, *Auto (face 2)* or
  *Auto (scene)*. You can weaken, disable, move or remove any single one.
- Anything you set yourself is protected. A later Auto enhance never overwrites a slider you
  moved or a retouch stack you edited.
- **Why these settings** and **Automatic decisions by face** explain every decision, with
  the numbers AURA measured.

## What it will not do

- It does not compare anybody's skin with an "ideal" colour or brightness. Every skin
  decision is measured against the same person's own face.
- It does not remove moles, freckles, scars or tattoos, change face or body shape, open
  closed eyes, or generate new content.
- It does not straighten, crop, replace skies or select subjects with a trained
  segmentation model. Those remain manual.
- Faces smaller than about 28 pixels between the eyes (at 2048-pixel analysis size) receive
  skin retouch only, not blemish, eye or teeth finishing.
- The measurements can be wrong on unusual images — a red birthmark, stage lighting, heavy
  make-up. Every result is a proposal you can undo.
