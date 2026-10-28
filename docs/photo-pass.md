# The photo pass, in the product's own words

When you import photos, AURA edits every one of them on its own: first **Auto enhance** (light,
colour, skin, blemishes, eyes, teeth), then the **photo pass**. The photo pass is a list of checks,
each looking for one fault Auto enhance does not look for. A check measures the photo first and
fixes only what it measured. Most checks find nothing on most photos, and that is the point: a
clean photo is left clean.

The finished pass will have one hundred checks. This release has the first twenty-five, for
capture, light and colour. Skin and face, eyes, hair, clothes and background, and the final look
and quality checks follow.

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

## What it will never do

- Change a value you set by hand.
- Touch a person's skin in any scene check: people are kept out of them.
- Make up detail: dust is healed from surface beside it, colour comes from what surrounds it.
- Remove people or objects.
