# Automatic editing, in the product's own words

Press **Auto enhance photo** and AURA finishes the photograph on its own. It looks at the
picture, decides what it needs, and saves the result as a short list of steps you can walk
back through. Nothing leaves your computer and your original file is never changed.

## What happens, step by step

| Step | What AURA measures | What it changes |
|---|---|---|
| 1. Light & colour | Brightness spread, neutral areas (faces ignored), how colourful the frame already is, haze, noise, whether it is a portrait, landscape, low-light or general scene; how far the lens put red and blue out of register with green at the corners | Exposure, highlights, shadows, contrast, white balance (only a cast the whites confirm, and only part of it, so warm rooms stay warm), vibrance, clarity (never on portraits), dehaze, noise reduction, sharpening (skin masked on portraits), and the lens's colour fringing when it is visible |
| 2. Sky balance & purple fringes | A bright sky above a measured horizon; purple rims on the dark side of bright edges | A soft gradient that darkens only the bright sky pixels; each purple rim pixel given the colour of its surroundings |
| 3. Skin | Each face's own skin, sampled from its cheeks and forehead | Texture smoothing that keeps fine detail, tone evening, gentle local light |
| 4. Blemishes | Small spots that are **redder** than the skin around them | Each spot healed separately with nearby skin. Darker marks that are not redder — moles, freckles, beauty marks — are kept and counted |
| 5. Lines & redness | Line texture beside the eyes and on the forehead compared with the same person's cheek; smile-line depth compared with the cheek beside it; redness beside the nose compared with the cheek | Fine lines softened with fine skin texture kept; smile lines lifted, never erased; redness evened toward the person's own cheek colour |
| 6. Eyes | Whether an eye is open, whether the white of the eye is red, flash red-eye, and whether the under-eye is darker than the same cheek | Subtle iris detail, redness reduction, red-eye correction, under-eye lift — each only when measured |
| 7. Teeth & shine | Visible teeth and their yellow cast; shiny skin highlights | Natural whitening that leaves lips alone; shine softened without flattening the skin |

A step that finds nothing to do is not saved. Running Auto enhance again on an unchanged
photo saves nothing new.

### Every photo gets its own settings

Skin sampling separates smooth directional light from fine texture before choosing smoothing
strength. A lit cheek is therefore not treated as rougher skin merely because its brightness
changes across the sample. The original brightness and skin colour are kept for the other decisions.

When a connected vision model proposes an edit, AURA also bounds white balance, tint, vibrance
and saturation around its measurement of the original. The explanation shows any values that
were limited. Repeating the same proposal does not accumulate a white-balance shift.

Batch progress has one shared monitor across workspaces. If a status request fails, the saved
progress and Stop control remain available while AURA retries. After three failed requests,
press **Retry status** to reconnect. Completed edits remain saved throughout.

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
- The faces: with people in the frame, how bright the rest of it is says nothing about them,
  so the histogram on its own never moves their exposure. Skin at or near white on 6% of a
  face is overexposure whatever else is in the frame, and is brought back by 0.2 to 0.7 EV. A subject already brighter than a bright frame around it is
  a low-key portrait and keeps its mood. A dark complexion in a dark room is not lightened
  because the room is dark - but when the brightest things in the frame are not the people
  (a shirt, a blanket, a page, a wall) and they are still well short of white, the whole
  frame was exposed too low, and it is lifted.
- A photograph whose very brightest tones already come near white is exposed correctly,
  however dark its midtones are. Its exposure is not lifted, and its shadows only lightly.

**Highlights** come down only as far as something says detail was lost: the people's skin
when there are people, how much of the frame sits at white when there are not, and an open
sky is left to the histogram. A white blanket, a dress or a studio wall is the photograph's
own brightness, and lowering it only turns it grey.

**Shadows and Highlights work on regions, not pixels.** AURA first finds the regions of the
photograph - a wall, a sky, a window, a face in shadow - by smoothing its brightness while keeping
the edges between them, and each slider then moves a whole region by one amount. The grain of a
lifted wall, the clouds in a recovered sky and the detail in a shadowed face are carried along
unchanged, so nothing flattens and a white cloud never ends up darker than the blue around it. A
region that is clipped - a blown-out sky in a JPEG - has nothing in it to recover and is left white
rather than turned grey.

Auto enhance sizes the two sliders from the same regions:

- A photograph that is **mostly shadow** - an interior exposed for its windows, a backlit scene
  exposed for the sky - is opened up firmly. A **dark background or black fur** is opened only
  lightly, so it stays a dark background. A **black backdrop** is left black.
- **Bright regions that still carry detail** are brought down most when they are a sky, a window
  or a cloud, and least when they are the subject itself: pulling a white building or a bright room
  down only turns it grey.
- A night scene keeps its darkness and a bright-by-design frame keeps its white.
- Where the histogram and the regions both ask for something, the stronger answer is used, never
  the two added together. **Why these settings** gives the shares and depths measured.

**Clarity** strengthens the texture inside each region and steps aside at strong edges, so it
leaves no glow round branches against a sky or a dark line along a building. **Dehaze** measures
the colour of the haze and how thick it is in each part of the frame, so the distance is cleared
more than the foreground; a negative value adds atmosphere. All four look the same in a preview
and in a full-size export. ADR-0126.

**A veiled frame** - haze, flare, a flat picture profile, a scan of a faded print - lifts its
blacks and lowers its whites together, and no slider takes that off. When the darkest tones
sit above about a sixth of the range and the brightest stop short of white, AURA sets the
black and white points on every colour channel alike, which puts back contrast and colour
together without moving the white balance. A lifted black on its own is a matte or soft
finish and is left to the photograph; so is a top that never reaches white in a frame whose
blacks are black.

**White balance** is corrected only when independent readings agree: the near-neutral areas,
the average colour of edges, and the brightest unclipped tones. If the brightest tones are
neutral, the light is neutral and the colour elsewhere belongs to the scene. From a camera file the colour is the camera's guess rather than anybody's choice, so a
smaller cast is corrected, a smaller neutral area will do, and under mixed light the whites
may settle which of the other two readings to follow. A cast is removed in a finished
photograph only when all three show it, and only the part of it beyond what a finished
photograph carries by choice: a warm backdrop, a cool studio grey and the hour all read as a faint cast,
and a faint cast is left alone. What is left after a correction is as much colour as a
choice would be, so warm rooms stay warm. When only the first two agree, the colour is kept: one large coloured surface - a
pink backdrop, a painted wall, golden-hour light - moves both of them the same way. A strong
colour with no people in frame is kept as the light's mood, and so is a toned monochrome.

What this cannot know: a blouse that is pale pink rather than white, a wall that is cream
rather than lit warm, or a matte look that keeps its whites low on purpose. When such a
surface is the brightest neutral thing in the frame, it is read as light and corrected. Check
coloured backdrops and deliberate looks before delivery.

**Skin**, when one click retouches it, gets the finish calibrated against published
retouching of the same photographs: very little smoothing, pores refined rather than
blurred, marks and blemishes healed, and freckles and dark marks kept. Removing those is a
decision about somebody's face; the **Portrait skin finish** preset makes it when you ask.

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

## Colour fringing from the lens

Away from the middle of the frame a lens can image red, green and blue at slightly different
sizes, which leaves a red rim on one side of every edge and a blue or cyan one on the other. AURA
reads that from the photograph itself - how far apart the three colours of the same edges are,
toward the corners and in every direction round the frame - and when the shift is visible and
the same all the way round, it brings the colours back into register (most of the way, never
past it), then checks the corrected photograph the same way before keeping the change. No lens
profile is needed. A camera's own JPEG has usually been corrected already, and is then left as it
is. The correction is in the lens settings in Develop, where you can switch it off.

Purple rims on the dark side of a bright edge - a branch against a bright sky, a backlit veil, a
dark suit beside a white shirt - are a different fault, and they are removed by colour: each rim
pixel takes the colour of the calm area around it, at its own brightness. A purple dress, a
magenta flower and the glow round a purple uplight are purple where nothing is near an edge too,
and keep their colour. Green rims are left alone automatically, because the edge of every leaf
against a bright sky is green as well; the **Defringe** tool in Retouch removes them when you ask.

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

## The finishing look

After the corrections, Auto enhance adds the part of an edit a photographer adds to a photograph
that has nothing wrong with it: a little depth, richer colour where the colour is dull, and a light
warm-highlight, cool-shadow split. None of it is a preset. Each amount is measured from the
photograph in front of it, so a flat haldi frame, a dark night procession and a vivid stage under
coloured lights each get a different finish, and a frame that already has all three gets almost
none. A black-and-white photograph gets the depth and nothing else: no split tone, no richer
colour, no vibrance, because any of them would tint it. A sepia or other toned print keeps the one
colour it was printed in in the same way.

Skin is measured through the look before it is kept: the colour changes are tried on the faces in
the frame, and scaled back until the skin moves by less than you could see side by side. Orange,
the band skin of every tone falls in, is never pushed. Every value lands in the ordinary curve,
HSL, vibrance and colour grading controls, in the same undoable "Light & colour" step, so you can
see it, change it or take it out.

Colourful clothing and decor are not treated as a colour cast. When the colour every estimate
reads lines up with what fills the frame - pink saris, a pale pink wall, a crowd in yellow - the
white balance is left as it was shot.
