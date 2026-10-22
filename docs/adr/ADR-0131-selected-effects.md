# ADR-0131: Twenty-nine selected effects - liquify, optics, colour, light and portrait

Status: accepted. Date: 2026-10-20.

A photographer comparing AURA with Evoto asked for every tool Evoto has and fifty more.
`docs/evoto-module-map.md` sorts Evoto's effect modules into what AURA has, what it can build with
measurement and classical image processing, and what needs a generative model this build cannot
run. This records the second group, plus creative and practical tools common to professional
editors, built as twenty-nine new retouch operations. Nothing in Evoto's installation beyond the
names of its effect folders was used; every algorithm here is a textbook one, written for this
renderer.

## 1. They are retouch operations, not a new system

Each tool is a new `Tool` variant: the same `Edit`, the same selection (ellipse, brush, gradient,
brightness range, inverted, AI matte), the same saved stack, Undo, history, retouch strength
slider for its own operation, and the same place in the render. The renderer dispatches all of
them to one module, `crates/aura-render/src/retouch_effects.rs`, through a single line in
`retouch_tools::apply_from`. They read the `Edit` fields that already exist - `amount`, `tone`,
`warmth`, `tint`, `texture`, `radius`, `source`, `target_color` - and each variant's comment says
which. No field was added to the recipe and no migration was needed.

| Group | Tools |
|---|---|
| Liquify | push, bloat (enlarge), pucker (shrink), twirl |
| Blur and optics | spin blur, pixelate, privacy blur, soft focus, Orton glow, local sharpen, local noise reduction (and a defringe, since withdrawn - see the note below) |
| Colour | hue shift, invert, chroma key (green screen), vein reduction, colour unify, local white balance |
| Light | light leak, glitch, haze, starburst, film halation |
| Portrait and scene | catchlight, eye glow fix (pets and red-eye), lip gloss, freckles, floor reflection, drop shadow |

The window adds seven named shortcuts (enlarge eyes, hair volume, slim a nose, lift a jawline,
hide a face or plate, dreamy glow, sharpen the eyes). They are starting values for the tools
above, not further tools, and the picker says so by showing the tool they choose.

## 2. Rules every effect keeps

Tested for all twenty-nine at once in `retouch_effects::tests`:

- **It works inside its selection.** Nothing further than a feather outside the region moves - the
  one exception is the drop shadow, whose whole purpose is to land beside its subject, and which
  never darkens the subject itself.
- **Zero strength, or disabled, changes nothing**, bit for bit.
- **It is deterministic.** Freckles and glitch slices come from a hash of the cell and the
  operation's placement, not from a random generator, so the same recipe is the same picture on
  every machine and every core count (invariant 4). Moving the selection makes a different
  freckle field; re-rendering the same one never does.
- **It reads the frame as it was before it ran**, through a snapshot of its own area, and writes
  rows in parallel.
- **The colour tools keep luminance.** Hue shift, chroma key's despill, vein reduction and colour
  unify work on two colour differences that convert back with the luminance exactly preserved,
  so changing a colour never makes it lighter or darker.

## 3. The warps

A warp moves pixels, so `amount` is how far rather than an opacity - blending a warped copy over
the original would leave a ghost. The displacement is the selection's coverage times
`(1 - d^2)^2`, where `d` is the distance across the region's ellipse, so even a hard-edged
selection warps to nothing at its boundary and never tears. An inverted selection is refused for
all of them, as it already was for local proportions (`Tool::is_warp`). They are never planned by
automatic retouch: `advanced_retouch::stage_of` returns no stage for any effect, and
`docs/retouch-ethics.md` still holds - nobody is reshaped unless a person places the warp.

## 4. Choices that will be re-argued

**Superseded on merging `main` (2026-10-20):** `main` had meanwhile shipped its own `Tool::Defringe`
(ADR-0125), which measures the lens's lateral colour from the photograph and gives each rim pixel
the colour of its calm surroundings. That is the stronger tool and the two could not share a name,
so this module's defringe was removed; twenty-eight effects remain here. The note below is kept
because the lesson - a fringe sits beside a near-clipped highlight, not beside any sharp edge -
is a test for any defringe.


- **Defringe acts only beside a highlight.** A purple dress is not a fringe, and neither is the
  sharp edge of a lavender print on it. The fringe test is purple (red and blue both above green,
  relative to luminance) *and* a local contrast step *and* a highlight at or near clipping within
  four pixels, which is where lateral colour fringes actually appear. The first version had only
  the first two terms and, on a wedding guest's printed purple shirt in the real window, changed
  23,109 pixels of pattern edge; two tests now hold the line - a flat purple object and a purple
  pattern with nothing bright beside it are both left exactly as they were.
- **Vein reduction acts only where the skin is bluer than its surroundings.** It measures each
  pixel's colour against a blur of the area at the vein's own width and pulls only the bluish,
  darker deviations toward it. Skin that is not bluer than what is around it is untouched, so it
  is not a smoothing tool by another name.
- **Lip gloss lifts highlights a third of its size or larger.** Comparing raw luminance with its
  surroundings lifted every pore glint on the first real photograph and read as sweat.
- **The catchlight is as bright as the brightest thing in the eye, and never dimmer than 0.85
  linear.** A catchlight is a reflection of the light source; one dimmer than the eye's existing
  highlights reads as a grey smudge.
- **The eye-glow fix needs very strong colour.** Its first version accepted anything bright and
  moderately saturated, and on the real photograph it left a dark patch on a neck: in linear light
  skin measures up to about 0.65 on the saturation it uses, so the threshold now starts above
  that, and a test asserts bright skin is untouched. It also keeps the eye's shading. It scales each glowing pixel down to a pupil level
  rather than painting a flat disc, and only acts on pixels that are both bright and strongly
  coloured, so a dark pupil is left alone; a brightly lit, saturated iris inside the selection
  is not, which is why the selection belongs on the pupil.
- **Floor reflection mirrors around the selection's top edge.** The person selects the floor; what
  stands above it is what a glossy floor or still water would show.

## What this does not do

These are tools a person places. None of them detects a face, an eye, a vein, a floor or a sky by
itself - pick the area with the ellipse, the brush or an AI selection. None generates content:
the reflection and shadow are made from the photograph's own pixels, and the chroma key replaces
a colour with a flat chosen colour, not a new background image. It is not Evoto's implementation,
and visual parity with Evoto has not been measured.
