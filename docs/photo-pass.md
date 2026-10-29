# The photo pass, in the product's own words

When you import photos, AURA edits every one of them on its own: first **Auto enhance** (light,
colour, skin, blemishes, eyes, teeth), then the **photo pass**. The photo pass is a list of checks,
each looking for one fault Auto enhance does not look for. A check measures the photo first and
fixes only what it measured. Most checks find nothing on most photos, and that is the point: a
clean photo is left clean.

The finished pass will have one hundred checks. This release has the first fifty: twenty-five for
capture, light and colour, and twenty-five that look at every face on the edited photograph.
Eyes, hair, clothes and background, and the final look and quality checks follow.

## What every check reports

- **Changed** - it found the fault and fixed it. The fix is its own step in the history, so you
  can go back to just before it.
- **Checked - nothing needed** - it looked and the photo is fine.
- **Not applicable** - it does not apply here (no sky to look at, not a JPEG, nobody in the frame).
- **Kept your setting** - you had set that value by hand, so it was left as you set it.

You can see the list for any photo under **Photo pass** in Develop, and run it again there. Pressing
**Auto** in Develop also runs it.

## The first twenty-five checks

| # | Check | What it looks for | What it does |
|---|---|---|---|
| 1 | Sensor dust | Faint round specks on a plain sky, backdrop or wall - only at f/5.6 or narrower, where dust can show | Heals each speck from clean surface beside it |
| 2 | Clipped colour channels | Areas where one colour has burnt out and changed the hue | Gives them the colour of their surroundings |
| 3 | Colour noise | Coloured speckle in plain areas | Raises colour noise reduction to what was measured |
| 4 | Shadow noise | Shadows much noisier than the rest, made worse by brightening | Denoises the shadows only |
| 5 | Noise on smooth areas | Grain on sky and plain backdrop | Denoises those areas only, edges kept |
| 6 | JPEG compression | A low-quality JPEG (read from the file itself) | Smooths the 8 x 8 compression blocks |
| 7 | Banding | Steps in smooth gradients of an 8-bit file | Smooths the banded gradient |
| 8 | Moiré | Rainbow false colour on fine patterns in clothes | Evens the false colour, keeps the pattern |
| 9 | Lens vignetting | All four corners darker, the same way, with no lens profile | Corrects half of the fall-off |
| 10 | Mixed light | Part of the frame lit by a different colour of light (open sky does not count) | Tells you where; a sunset or open shade looks the same to it, so it does not change it on its own |
| 11 | Shadow tint | Green or magenta shadows the midtones do not have | Shadow tint against it |
| 12 | Shadow colour cast | Blue or yellow shadows the midtones do not have | A shadow colour grade against it |
| 13 | Highlight tint | Coloured highlights the midtones do not have | A highlight colour grade against it |
| 14 | Regional haze | One part of a scene with nobody in it hazier than the rest | Dehazes that part only |
| 15 | Capture sharpening | How soft the lens drew the edges | Matches the sharpening radius to them |
| 16 | Camera shake & focus | A soft frame, and whether the shutter was too slow | Tells you; blur cannot be undone |
| 17 | Green & blue fringes | Green rims along bright edges, on a lens that also left purple ones | Gives them the colour around them, never on people |
| 18 | Over-saturated light colours | Stage or LED colour burnt out at full saturation | Lowers that colour a little so its detail shows |
| 19 | Level horizon | Long straight lines that agree the camera was tilted | Straightens, cropping only the corners |
| 20 | Pale sky | A blue sky that came out washed out | Deepens it gently; grey skies stay grey |
| 21 | Scene texture | A flat-looking scene with nobody in it | Raises Texture a little |
| 22 | Hard corner shading | Black corners from a lens hood or filter (not a dark backdrop) | Crops them off |
| 23 | Dark foreground | Dark ground under a bright sky, nobody in the frame | A graduated lift of the dark ground; lamps and lit windows keep their brightness |
| 24 | Sharpening mask for noisy files | High ISO or measured noise | Keeps sharpening off flat, noisy areas |
| 25 | Flicker bands | Light and dark stripes from LED light and a fast shutter | Tells you; they cannot be removed reliably |

## The face checks (26-50)

Auto enhance already retouches every face. These checks do not retouch again: they look at each
person on the finished edit and compare them with themselves - the skin after the colour grade
with grey and white after it, the retouched under-eye with the same person's cheek, the teeth
with the same person's eyes. When Auto enhance went too far, its own strength slider for that
kind of retouch is moved (you see it under Retouch strength); otherwise one change is made on that
person's skin only. A face behind a coloured veil, or under a coloured light, is left as it is.

| # | Check | What it looks for | What it does |
|---|---|---|---|
| 26 | Face against its background | A face in front of a window or the sky | Tells you, and points to Portrait light; it never brightens a face on its own, because a face in shade and darker skin look alike to a measurement |
| 27 | Skin clipped by the edit | Skin the edit burnt out that had detail | Rolls the brightest skin off below white |
| 28 | Skin hue moved by the grade | Skin pushed further than grey and white by the colour grade | Takes the extra colour back out of the skin |
| 29 | Skin saturation raised by the grade | Skin made much more saturated than the original | Brings it back close to the original |
| 30 | Lips that lost their colour | Lips paler than in the original | Brings their colour back |
| 31 | Pores kept | Skin smoothed so much its texture is gone | Lowers smoothing so the pores show |
| 32 | Skin colour kept by the retouch | Retouch that changed the person's skin colour | Halves tone and redness evening |
| 33 | Beard and stubble kept | Stubble smoothed away | Lowers smoothing |
| 34 | Eyebrows kept | Brows smoothed | Tells you |
| 35 | Hairline kept | Hair round the face smoothed | Tells you |
| 36 | Under-eyes no lighter than the cheek | Under-eyes lifted past the cheek | Lowers the under-eye lift to the cheek |
| 37 | Teeth no whiter than the eyes | Teeth whitened past the whites of the eyes | Lowers teeth whitening |
| 38 | Shine left after mattifying | Shiny skin still there | Raises shine control |
| 39 | Bindi, tika and sindoor kept | Red marks on the forehead and parting losing colour | Tells you |
| 40 | Moles and beauty marks kept | Dark marks removed | Tells you |
| 41 | Both halves of each face alike | One side of a face retouched more | Tells you |
| 42 | Retouch stays on skin | Clothes or background changed by face retouch | Tells you |
| 43 | Noise lifted onto skin | Grain on skin after brightening | Softens the grain on that skin |
| 44 | Eyes in focus | Soft eyes | Tells you whether it is focus or closed/covered eyes |
| 45 | Coloured light on one side of a face | One cheek a different colour from the other, lit as brightly | Evens that side to the forehead's colour |
| 46 | Face and neck colour | A neck a different colour from the face | Evens the neck to the face |
| 47 | Mehndi kept | Henna patterns softened | Tells you |
| 48 | Eye make-up kept | Eyeshadow colour evened away | Tells you |
| 49 | Faces too small to retouch | Guests too small to judge | Tells you how many |
| 50 | Neck in shadow | A neck much darker than the face above it | Lifts it half way |

## What it will never do

- Change a value you set by hand.
- Touch a person's skin in any scene check: people are kept out of them.
- Judge a face against an ideal skin colour: every face check compares the person with themselves.
- Reshape anybody.
- Make up detail: dust is healed from surface beside it, colour comes from what surrounds it.
- Remove people or objects.
