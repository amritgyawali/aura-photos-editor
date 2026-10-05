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
| 5. Lines & redness | Line texture beside the eyes and on the forehead compared with the same person's cheek; smile-line depth compared with the cheek beside it; redness beside the nose compared with the cheek | Fine lines softened with fine skin texture kept; smile lines lifted, never erased; redness evened toward the person's own cheek colour |
| 6. Eyes | Whether an eye is open, whether the white of the eye is red, flash red-eye, and whether the under-eye is darker than the same cheek | Subtle iris detail, redness reduction, red-eye correction, under-eye lift — each only when measured |
| 7. Teeth & shine | Visible teeth and their yellow cast; shiny skin highlights | Natural whitening that leaves lips alone; shine softened without flattening the skin |

A step that finds nothing to do is not saved. Running Auto enhance again on an unchanged
photo saves nothing new.

### Every photo gets its own settings

Before a face is retouched AURA measures it: how large it is in the frame, how even its skin
texture and colour are, how directional the light is, how much of it is shiny, how many small
marks it has, and how noisy the frame is. The retouch settings you chose are then turned up
or down *for that face*. Smooth skin is left nearly alone; rough skin in a close-up gets more
smoothing with its pores kept; a small face in a group gets a light touch; hard light is not
flattened; a noisy frame is not given plastic skin. Your settings remain the style - a
control you set to zero stays off.

Under **Automatic decisions by face** every tuned face lists what was changed and why, and a
batch result shows each photo's amounts. Untick **Adapt to each face** in Retouch to have
your settings used exactly as set.

The same idea applies to light and colour when nobody is in the frame. A night scene is not
brightened into grey, a bright white scene is not darkened, a sepia or toned black-and-white
keeps its colour, and when the brightest parts of a photo are already near white the dark
parts are lifted with Shadows rather than Exposure.

### Choosing what is retouched, and how much

In **Retouch**, the **Automatic retouch** box offers three choices:

- **Face**: skin, blemishes, lines, eyes and teeth on each detected face.
- **Body skin**: neck, shoulders, chest and arms below each face; the face is left as it is.
- **Face + body skin**: both.

Pick one and press **Auto retouch**. Body skin is found by sampling skin below the face that
matches the same person's face colour, and only pixels close to that sample are smoothed and
evened; the face is masked out so it is never smoothed twice. Under **Strength and details**
choose *Subtle* (25-75 %), *Natural* (about 100 %) or *Polished* (up to 150 %) and switch
blemish healing, lines and redness, eyes or teeth on or off. Every result appears in the list
below, tagged *Auto (face 1)* or *Auto (body 1)*, and you can change, disable or remove it.
Running it again replaces the automatic operations; operations you added yourself are kept,
and the next **Auto enhance** remembers your choice.

There is no body segmentation model. A background close to the person's skin colour (beige
walls, wood, sand) can be softened slightly; the report says so when much of the area matches,
and each body operation has an ordinary brush mask you can erase.

## How exposure and white balance are decided

**Exposure** has three witnesses, and none of them is a target brightness for skin.

- The histogram: a frame whose midtones sit far from the middle is moved part of the way.
- The highlights: a photograph normally has something near white in it. When the very
  brightest tones stop more than half a stop short of white and the frame is not a night
  scene, it is lifted by most of that room, at most one stop. With people in frame the lift
  stops before any face would clip; without people it is smaller.
- The faces: skin at or near clipping is overexposure whatever else is in the frame, and is
  brought back by 0.2 to 0.7 EV. A subject already brighter than a bright frame around it is
  a low-key portrait and keeps its mood.

**White balance** is corrected only when independent readings agree: the near-neutral areas,
the average colour of edges, and the brightest unclipped tones. If the brightest tones are
neutral, the light is neutral and the colour elsewhere belongs to the scene. If all three
show the same cast, most of it is removed; if only two do, about half to two thirds. A strong
colour with no people in frame is kept as the light's mood, and so is a toned monochrome.

What this cannot know: a blouse that is pale pink rather than white, a wall that is cream
rather than lit warm, or a matte look that keeps its whites low on purpose. Each of those is
read as light, and corrected. Check coloured backdrops and deliberate looks before delivery.

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
