# ADR-0186: The hundred-step photo pass

Status: accepted for batch 1 (steps 1-25) and batch 2 (steps 26-50), 2026-10-28. Batches 2-4 are planned below and are
revised, step by step, when each is measured on real photographs.

## Context

On 2026-10-28 the user asked for "100 logics, techniques and algorithms for each photo to edit,
retouch and advanced edit" when a folder is imported. Asked to choose, they picked a **100-step
automatic pass** that every imported photograph goes through, delivered in **four batches of about
25**, each tested on their own Canon CR3 wedding photographs (`H:\AURA EDIT`) and in the real window
before the next.

Importing already edits every photograph (`App.tsx` `startImport(autoEdit)` -> `prepareCollection`
-> `develop.enhancePhoto` -> `smart_edit::run`): light and colour, sky, purple fringes, skin,
blemishes, lines, eyes, teeth and shine. The renderer has about 150 retouch tools, most of which
only a person places by hand.

## Decision

1. **A new service, `crate::photo_pass`, runs after Auto enhance and never instead of it.** The
   hundred steps are *new* checks: none re-decides a value Auto enhance owns (exposure, white
   balance, the tone sliders, skin smoothing, blemishes, eyes, teeth). Re-labelling those decisions
   as steps would pad the count and create a second service for the same question.
2. **Every step measures before it acts** and reports one of the four outcomes Auto advanced
   retouch already uses (`advanced_retouch::Outcome`): *applied*, *checked - nothing needed*, *not
   applicable*, *kept your setting*. A step whose fault it cannot repair (camera shake) says so and
   changes nothing.
3. **Each applied step is its own history entry**, so a person can step back past any one of them.
4. **Sliders merge as an automatic proposal** (`schema::merge` with `EditSource::Ai`), so a value a
   person set is refused and the step reports *kept your setting*. **Operations** the pass places
   carry the `pass-` prefix; a repeat run replaces only those and keeps every other operation.
   `portrait_auto::group_of` returns `None` for them, so Auto enhance and Auto advanced retouch keep
   them as they keep a person's own.
5. **Measured on the original's 2048 px preview**, the same pixels Auto enhance measured, plus the
   catalogue's EXIF (ISO, shutter, focal length) and, for JPEG files, the file's own quantisation
   tables. Faults that only exist at full resolution (single hot pixels) are not attempted.
6. **The report** is stored in the recipe under `photo_pass_v1` with the pass version, and shown in
   the editor for the photograph that is open.
7. **No frozen contract changes.** The pass uses existing recipe fields and existing retouch tools;
   its IPC shape lives in `ui/src/ipc/photoPass.ts`.

## Batch 1: capture, light and colour (steps 1-25)

People are excluded from every scene step by a person zone (each face box, widened, down to the
bottom of the frame), so no step here touches skin.

| # | Step | Measured | Action |
|---|---|---|---|
| 1 | Sensor dust | At f/5.6 or narrower only: faint specks on smooth bright areas (sky, backdrop, wall), found by Auto advanced retouch's own mark search | Heal from clean surface beside each |
| 2 | Clipped colour channels | Areas where one or two channels are at the top and the others are not (the hue has shifted) | Clipped highlight repair over each area |
| 3 | Colour noise | Chroma noise against luma noise on flat areas | Colour noise reduction raised to the measured level |
| 4 | Shadow noise | Luma noise in the shadows against the midtones | Local denoise on the shadows only |
| 5 | Noise on smooth areas | Luma noise on sky and plain backdrop | Local denoise on those areas only |
| 6 | JPEG compression | The JPEG file's quantisation tables (its quality) | JPEG cleanup, frame-wide, scaled by quality |
| 7 | Banding | Steps of one or two code values with flat runs either side, in smooth gradients of 8-bit files | Deband on the banded area |
| 8 | Moiré | On clothes: colour that alternates at the finest scale while brightness does not | Fabric moiré on each area |
| 9 | Lens vignetting | Corner fall-off that all four corners share, with no lens profile | Lens vignette correction, part of the way |
| 10 | Mixed light | Neutral surfaces (not open sky) lit by two colours of light in separate parts of the frame | Reported only, with where to sample Local white balance |
| 11 | Shadow tint | Green or magenta in neutral shadows that neutral midtones do not have, 3 % or more | Calibration shadow tint |
| 12 | Shadow colour cast | Blue or yellow in neutral shadows that neutral midtones do not have | Colour grade, shadows wheel |
| 13 | Highlight tint | Colour in neutral highlights that neutral midtones do not have, 8 % or more | Colour grade, highlights wheel, a third of the way |
| 14 | Regional haze | Frames with nobody in them: part of the scene with lifted darkest tones, flat contrast and little colour | Local dehaze on that part |
| 15 | Capture sharpening | How soft edges are (edge-width of the strongest edges) | Sharpening radius matched to it |
| 16 | Camera shake & focus | Shutter against focal length, and blur with one direction | Reported only; it cannot be undone |
| 17 | Green & blue fringes | Green rims on the dark side of bright edges, off people, when Auto enhance also found purple fringes | Defringe on those rims, people excluded |
| 18 | Over-saturated light colours | Pixels of one hue band that clip at full saturation (stage and LED light) | That band's saturation lowered |
| 19 | Level horizon | Long straight lines that agree on a tilt (`advanced_retouch::measure::tilt`) | Rotation and the crop that hides the corners |
| 20 | Pale sky | Sky that is blue in hue but washed out | Sky blue, gently |
| 21 | Scene texture | Fine detail on a frame with nobody in it | Texture raised a little |
| 22 | Hard corner shading | Black corners from a lens hood or stacked filter: three corners or more, with the middle of every side lit | Crop just inside them |
| 23 | Dark foreground | A bright top and a dark bottom on a frame with nobody in it | A graduated lift of the dark tones of the bottom |
| 24 | Sharpening mask for noisy files | ISO and measured noise | Sharpening masking raised so noise is not sharpened |
| 25 | Flicker bands | Rows that brighten and darken periodically (LED light, electronic shutter) | Reported only |

## Batch 2: faces and skin, checked on the edited photograph (steps 26-50)

The plan below listed skin retouches. Reading what Auto enhance already places changed it: it
smooths and evens skin, heals blemishes, softens forehead, smile and neck lines, lifts under-eyes,
whitens teeth, controls shine, keeps ceremonial marks (`portrait_features/ceremonial_marks.rs`) and
matches body skin to the face. A second service retouching the same skin would break the rule of
one service per concern. What no service did is look at the *result*. Batch 2 does that.

- **Two renders of the edit the capture steps leave**, the same size and both with geometry,
  grain and sharpening off (`advanced_retouch::qc_render`): one with every retouch operation,
  one with none. With the original's preview that is three views of the same pixels: what the
  grade did (original against bare), and what the retouch did (bare against full).
- **The renderer's own parse of the faces** (`aura_render::portrait::parse`, the one its face
  tools are anchored on), each region assigned to the nearest person. Face operations are limited
  by a matte of skin *inside the face*: the parse's skin region alone reached a red bodice and a
  white shirt on real photographs.
- **Every judgement is the person against themselves**: skin after the grade against neutrals
  after it, the retouched under-eye against the same person's cheek, teeth against the same
  person's eyes. Never a face against an ideal complexion.
- **When Auto enhance went too far, its own strength factor moves** (ADR-0130), and a factor a
  person set is kept like any other value. Otherwise one operation, on that person's region.
- **Up to six people** are judged, largest first; faces with fewer than 18 pixels between the eyes
  at 1600 pixels are reported, not judged.

| # | Step | Measured | Action |
|---|---|---|---|
| 26 | Face against its background | A background near white (a window, the sky) behind a face | Reported, pointing to Portrait light: never applied (see below) |
| 27 | Skin clipped by the edit | Burnt-out skin after the edit that the original had detail in | Highlight roll-off starting just below that skin's own brightness |
| 28 | Skin hue moved by the grade | The skin's colour shift against the neutrals' shift | Skin colour against the extra shift, most of the way |
| 29 | Skin saturation raised by the grade | Skin saturation against the original's | Saturation back to within a tenth |
| 30 | Lips that lost their colour | Lip saturation against the original's | Lip saturation back to within a twentieth |
| 31 | Pores kept | Fine skin texture kept by the retouch (`measure::texture_retention`) | Smoothing strength lowered to keep 70 % |
| 32 | Skin colour kept by the retouch | Skin chromaticity moved by the retouch (`measure::skin_shift`) | Tone and redness strength halved |
| 33 | Beard and stubble kept | Detail kept on facial hair | Smoothing strength lowered |
| 34 | Eyebrows kept | Detail kept on the brows | Reported |
| 35 | Hairline kept | Detail kept on the hair round the face | Reported |
| 36 | Under-eyes no lighter than the cheek | Under-eye after the retouch against the same person's cheek | Under-eye strength lowered to reach the cheek, not pass it |
| 37 | Teeth no whiter than the eyes | Teeth after whitening against the whites of the same person's eyes | Teeth strength lowered |
| 38 | Shine left after mattifying | Shiny skin left after the retouch | Shine strength raised |
| 39 | Bindi, tika and sindoor kept | Strong red above the eyes, before and after the retouch | Reported |
| 40 | Moles and beauty marks kept | Compact dark marks, before and after | Reported |
| 41 | Both halves of each face alike | `measure::mirror_balance` | Reported |
| 42 | Retouch stays on skin | Change on clothes and background beside each face | Reported |
| 43 | Noise lifted onto skin | Grain on the skin after the edit | Local denoise on that skin |
| 44 | Eyes in focus | Detail in the eyes against the rest of the face | Reported: soft focus, or closed/covered eyes |
| 45 | Coloured light on one side of a face | The two cheeks, lit as brightly, in different colours | That side toward the forehead's colour |
| 46 | Face and neck colour | Neck against cheek, when Auto enhance placed no body match | Neck toward the cheek's colour |
| 47 | Mehndi kept | Henna pattern detail on body skin | Reported |
| 48 | Eye make-up kept | Eyelid colour before and after the retouch | Reported |
| 49 | Faces too small to retouch | Faces under 18 px between the eyes, or beyond the six largest | Reported |
| 50 | Neck in shadow | Neck against cheek brightness | Neck lifted half the gap |

## Batches 3-4 (planned)

**3. Eyes, hair, clothing and background (51-75):** catchlights, iris clarity, eye-white evenness,
glasses glare, tinted lenses, lash and lid shadow, flyaways against the sky, hair colour cast, hair
shine hot spots, scalp show-through, white outfit neutrality, fabric colour under tungsten, fabric
creases, lint on dark clothes, gold colour, stone sparkle, bright distractions, exit signs and LED
panels, background clutter, background people, floor and grass colour, wall seams and stains,
stage light balance, fog-machine haze, wires across the sky.

**4. Light, look, series and quality control (76-100):** subject brighter than the background,
face light balance, rim light kept, portrait vignette, composition (reported), cut-off limbs
(reported), closed eyes (reported), exposure, white balance, skin and look consistency across a
sequence, colour harmony, black-and-white candidates (reported), skin-safe vibrance, midtone
curve, highlight roll-off, output sharpening, grain, and quality control on the rendered result:
halos, banding, over-smoothed skin, colour drift, clipping and amplified noise introduced, and
that every step can be undone.

## What the real photographs changed

Every threshold above was first set on painted fixtures and then checked, one step at a time, on
the user's 27 Canon CR3 wedding photographs (`crates/aura-app/tests/photo_pass_photos.rs`,
`AURA_PASS_EACH=1` renders each applied step alone beside the frame before it). Five steps acted on
things that were not faults, and each was narrowed by what the photographs showed rather than by
a looser number:

- **Sensor dust** healed the tip of a palm leaf and a bokeh shape. Dust on a sensor is only sharp
  enough to see at small apertures; the step now runs at f/5.6 or narrower, and not at all when the
  aperture is not recorded. On these wide-open wedding frames it no longer applies.
- **Green fringes** (17 of 27) recoloured a green garland, the edges of a teal dress and leaves
  between white flowers. Rims on people no longer count and the correction is kept off them by a
  matte (`OUTSIDE_PEOPLE`); and since a lens's axial colour leaves purple rims in front of focus as
  well as green ones behind it, green rims are only removed on a photograph where Auto enhance
  confirmed purple ones.
- **Mixed light** (3 of 27) took the blue sky for a second light and reached a bride's veil. Open
  sky is no longer a neutral surface, and the correction is kept off people.
- **Regional haze** (3 of 27) took white flower garlands and pale curtains for haze, and a soft
  background for a hazy one. Haze now has to be flat and colourless as well as bright in its
  darkest tones, and behind people - where a wide aperture always leaves the background soft - the
  step does not apply.
- **Highlight tint** (8 of 27) answered the ordinary difference between sunlit highlights and
  skylit midtones. It now needs an 8 % difference and removes a third of it; shadow tint needs 3 %.

Steps whose pairs showed what they claimed - clipped colour channels on burnt red and pink
highlights, a gently deepened pale sky, moiré on a woven check, colour noise on high-ISO night
frames - were kept as they were. Capture sharpening's edge width is interpolated between samples;
the half-pixel steps it first measured in made every photograph look either crisp or soft.

### On held-out photographs

The same harness then ran on photographs no threshold was set on: 28 MIT-Adobe FiveK raw files
(landscapes, interiors, people; research licence, kept out of the repository) and 20 JPEGs from a
1,000-photograph Indian wedding set. Green fringes, banding, clipped colour channels, shadow cast,
sensor dust (only at small apertures now) and level horizon (0.8-3.9 degrees, most of the lines
agreeing) held. Four did not, and changed:

- **Moiré** fired on six real fine colours - pinwheels, flowers, windmill sails, hair against a
  window - and on no moiré. It now looks only at clothes, where a camera's moiré forms.
- **Mixed light** took a sunset band for a second light. A sunset, open shade under a canopy and a
  lamp-lit corner measure alike, and only the last is a fault: the step reports what it measured
  and where to sample Local white balance, and changes nothing.
- **Hard corner shading** cropped a product shot on a black backdrop. It now needs three shaded
  corners and the middle of every side lit.
- **Dark foreground** lifted lit shop windows and a boat's lamps into clipping with the dark ground.
  The lift is limited to the dark tones.

### Batch 2 on the photographs

Run one step at a time on the 27 CR3 portraits, three face steps acted on things that were not
faults, and changed:

- **Skin hue and skin saturation** "corrected" a bride's face seen through a red net veil, turning
  it green, and the operation reached her red bodice through the parse's skin region. Face
  operations now use skin inside the face only, and a face whose skin is saturated beyond skin
  (median above 0.55: a veil, a coloured light) is reported and left alone. A reading larger than
  any grade makes is taken for the two pipelines disagreeing, not the edit.
- **Coloured light on one side** took sun on one cheek and shade on the other for a coloured lamp.
  It now needs both cheeks about as bright; sun and shade differ in colour by nature.
- **Skin clipped by the edit** placed a roll-off that changed nothing: the renderer compresses
  luminance, and a sunlit cheek with a burnt-out red channel is far below white in luminance. The
  roll-off now starts just below the clipped skin's own brightness.
- **Face against its background** lifted a man whose skin is simply darker than the wall behind
  him. From these measurements a face in shade and a darker complexion in good light read the
  same, so any rule that lifts "dark" faces lightens people for their skin. The step reports back
  light (told by the background, not the skin) and points to Portrait light, and a test over all
  ten Monk Skin Tone swatches holds that no face is ever brightened by it.
- **The renderer's parse found a face in a drum's rope pattern.** A face is judged only when Auto
  enhance's own detector found it too.
- **Neck in shadow** lifted a baby's face held below a father's chin: the parse's neck is placed
  below the chin by geometry. The neck is now neck *and* skin, and most of it must be the colour of
  the same person's cheek.
- **Eyes in focus** flagged a bride looking down and a groom in sunglasses as soft. It now compares
  the eyes with the rest of the same face and says which it is.

Pores (94-97 % kept), skin colour (moved at most 0.002), ceremonial marks (every one kept) and
retouch leak (none) confirmed Auto enhance's own work on every photograph.

## Consequences

- An imported photograph gets up to 25 more history entries in batch 1, one per applied step.
- The pass adds work to a folder run; its cost is measured per step (`perf::step`, `AURA_PERF=1`
  in the harness prints `pass.plan`, `pass.render_faces`, `pass.plan_faces`). Measured alone on
  this machine: batch 1 about 1.7 s a photograph; with batch 2, 4.0-4.8 s on group photographs,
  of which the two 1600 px face renders are 1.2-2.0 s and the face steps 0.5-0.6 s (each person's
  pixels are found once and shared by every step). Auto enhance takes 17-39 s on the same files.
- Thresholds were set on painted fixtures and checked on real photographs; any that mis-fires on
  a held-out photograph is raised, not argued with.
