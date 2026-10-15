# ADR-0123: A finishing look measured per photograph, folder runs that use the machine, and scene colour that is not a cast

Status: accepted. Date: 2026-10-15.

A thousand Indian wedding photographs from Wikimedia Commons (free licences, 9.8 MP median,
Hindu, Sikh, Muslim and Christian ceremonies, a dozen regional traditions) were run through
**Finish a whole folder** in the real desktop window. Three things were wrong, and this records
what replaced them.

## 1. Every photograph got the same edit

Of 996 edited frames, exposure was left alone on 931, white balance on 931, tint on 939,
contrast on 880; saturation and texture on all 996; the tone curve, the HSL mixer and colour
grading were never used. What nearly every frame received was the same small shadow lift
(median +8) and a little vibrance (median +4). That is ADR-0120 working as designed - it repairs
faults and leaves a finished photograph alone, and most of these are finished camera JPEGs - but
it is half an edit. A photographer, and Evoto, also add depth and colour that suit the picture.

`crates/aura-app/src/adaptive_look.rs` is that other half, run after the corrections in the same
first history step:

| Part | Measured from | Bound |
|---|---|---|
| S-curve (parametric lights/darks) | the frame's tonal spread against the usual spread of its kind of scene (0.23 portrait, 0.25 general, 0.26 outdoor, 0.17 high-key); gentler with people | 22 |
| Opening hard light | spread more than 0.06 above that | 10 |
| Low light | median and dark share: the parametric shadows region lifts the deepest tones, and positive contrast is withdrawn (section 2) | 20 |
| Colour per HSL band | each band's share of the coloured pixels outside faces and its mean saturation in the working space; a dull band is enriched, a band past 85% (stage LEDs) calmed; orange is never pushed; a band is held back by the share of skin in it | 20 |
| Vibrance | mean saturation outside the faces | 12 |
| Split tone | the frame's own warmth (red minus blue in its midtones): a warm room is not warmed further, a cool stage gets warmth back in its highlights | 8 / 6 |

**Skin is measured, not trusted.** The colour half (bands, vibrance, grade) is run through the
renderer's own `hsl`, `vibrance_saturation` and `GradePlan::apply` on the frame's own skin samples
in the working space, and scaled back in quarters until the mean CIE76 difference is at most
1.5 and the mean hue rotation at most 2 degrees. The renderer's vibrance is not skin-aware, so
it is inside the measurement, not beside it.

The look **sets** its fields rather than adding to them, so pressing Auto twice gives one recipe
(`tests/photo_pixels.rs` caught the first version doubling); a value a person set is still
protected by `schema::merge`. `AURA_ADAPTIVE_LOOK=0` turns it off for comparisons.

## 2. A low-light lift that darkened the photograph

The planner's "shadows +10 for a low-light frame" plus contrast +7 exported a night procession
darker in exactly those tones (source level 5 to 4.3, 10 to 8.9, 20 to 18.2). The shadows slider's
band (`tonemap::tone`, centred at 0.33 with half-width 0.26) does not reach below about sRGB 20,
and contrast, a power about mid grey, darkens what is under it by up to a fifth. The look now
withdraws positive contrast on a frame that is mostly dark and lifts with the parametric shadows
region, which does reach those tones.

## 3. Saris and painted walls read as casts

A pink wall behind a woman in a pink dupatta was corrected to 4,798 K and tint -40, and a crowd in
yellow and orange saris to 3,626 K. In both, the grey-pixel and edge estimates are pulled by one
colour (which ADR-0120 already guards), and the third witness - the brightest near-neutral tones -
is a pale version of the same colour, so it "confirms". `scene_colour_explains` in
`smart_edit.rs`: for a finished file, when strongly coloured things cover a quarter of the frame
outside the faces and the cast points within about 45 degrees of their mean colour, the colour is
the scene's and white balance is kept. A real cast tints everything, so it does not line up with
what people are wearing. Camera files are unaffected.

## 4. A folder run used one core in twelve

Sampled every 20 s, a run used 8.2-8.9% of the machine. `image_work_gate`, added in 8e97cf6 to
keep previews, exports and retouch planning from each holding a full-size frame, allowed one pass
at a time in the whole app. It is now `aura_render::WorkGate`, a counting gate: one permit below
8 hardware threads, two at 8-11, three at 12 or more, `AURA_IMAGE_WORKERS` (1-8) to override.
Live previews still skip when every permit is held. The folder run analyses and edits that many
photographs at once and keeps every answer in photo order; `photo-edits.json` is still written
a chunk at a time. AI-assisted edits stay one at a time (one child job, a per-minute allowance).
`ExportPass::with_workers` renders, resizes, sharpens and encodes in parallel and writes, reads
back, hashes and records strictly in plan order, so names, the manifest and the point a failing
destination stops the job are unchanged.

Measured on 100 of the photographs, debug build, 12 threads:

| Stage | One at a time | Three at a time |
|---|---|---|
| Analyse | 2.4 s a photo | 0.94 s |
| Edit and retouch | 11.2 s | 5.3 s |
| Export, read back | 13.2 s | 8.5 s |
| Whole run | 2,675 s | 1,480 s (1.8x) |

One export already keeps about three threads busy, so export gains least. Peak working set rose
from about 2.4 GB to 5.4 GB. On 20 of the photographs, run once with one worker and once with
three, every recipe hash and every exported file's digest is identical.

## 5. A long job that said nothing

During the 1,000-photo export the window read "0 verified exports" for two hours while files
were being written: `export_run` reported only when it returned. `ExportPass::with_progress`
now reports after every frame is written and read back (or left out) and before each batch is
rendered, and the folder run shows "Exporting photo 421 of 998: source_0421.jpg written and read
back".

`crates/aura-app/src/activity.rs` is a registry of work in progress. Folder runs, exports, Auto
edit and Auto retouch (with the step they are on and "waiting for a free image worker" before
they start), AI requests and imports appear in it while they run; each registration is a guard
dropped with the work, so nothing is left behind by an error or a cancellation. `activity_status`
returns it with the image workers in use, and `ui/src/components/workflow/StatusBar.tsx` shows
it at the foot of every screen, polled each second, saying "Idle" only when nothing runs and
"status unavailable" rather than "idle" when it cannot ask. The progress panel no longer counts
a photograph that could not be opened as "left out by the cull" or as a failed edit. The types
live in `ui/src/ipc/activity.ts`, outside the frozen `types.ts`; the IPC surface is 320 = 320 = 320.

## 6. Crowded groups were not retouched at all

Two photographs with fifteen faces each failed Auto edit outright with AURA-RENDER-8002 ("invalid
or too many mattes"): fifteen faces needed 72 and 91 skin selections, and a recipe holds 64
(`retouch_tools::MAX_MATTES`). `fit_matte_budget` keeps the automatic operations of the largest
faces, whole, while their selections fit beside any a person added, drops the smallest faces'
operations, and says so in the portrait report. Both photographs now edit.

## 7. Two good photographs refused as damaged

Two 8 MP phone JPEGs that every other decoder opens were refused with AURA-RAW-2002: inside
their entropy data is an `FF` the encoder did not stuff, followed by a byte that is no marker
(`FF A6`), and `container::jpeg::probe` read it as a segment with a nonsense length. The walker
now treats `FF` plus a non-marker byte inside a scan as data, and once a frame header and a scan
have been read, a malformed segment after them ends the walk instead of refusing a picture
that is already readable. A stream that breaks before its scan is still corrupt.

## 8. A busy machine set photographs aside

With two runs decoding three photographs each, a 12.6 MP frame ran past its decode deadline
(AURA-RAW-2004) and was set aside. A folder run now tries such a photograph once more on its
own, after the parallel batch, before giving up on it.

## 9. Brightening pushed saturated colour to clipping

Of 998 delivered frames, 49 gained more than 2% new clipping: a pink backdrop, an orange tent
and red saris went to flat, full colour - 20% of the frame on four of them - under exposure
lifts of +0.5 to +0.6 EV, whites +20 and the look's S-curve. All three read brightness, and a
saturated colour clips in its strongest channel long before its luminance nears white.
`colour_clip_cap` (in `guard_tone`, so AI proposals pass through it too) runs the frame's own
samples through `display_srgb_to_working` and `output::map_one` at the proposed exposure and
holds the lift where it would newly push more than 2% of the frame to the top of a channel, 8%
for a frame whose median is below 30%, where the lift matters more than a clipped corner of a
sari. Whites are not stretched, and the S-curve's lights are capped at 3, when more than 5% of
the frame is a saturated colour already near the top of its channel. On the six worst frames
new clipping went from 20% to 1.5-2.4% on the backdrops, and the two dark frames were still
lifted +0.6 and +0.7 EV.

## 10. Seven hours for a thousand photographs

The 1,000-photograph run took seven to eight hours in the debug build the test harness uses. A
release build, profiled in-process on twenty photographs from the set
(`crates/aura-app/tests/batch_profile.rs`, `AURA_PERF=1`), spent 13.4 s a photograph on one
worker and 56 processor-seconds a photograph in all. Where it went, and what was done:

- **Person segmentation, 2.7 s a photograph**, was the interpreter running a 256x256 network on
  one thread: a 16x16 layer with 384 channels and a 3x3 kernel is a small output and a third of
  a billion multiply-adds, and the parallel threshold counted outputs. It counts multiply-adds
  now; 1x1 convolutions, most of the network, run as a matrix product four output channels at
  a time; `ConvTranspose` tests its taps once per operator instead of once per sample; `Resize`
  and the element-wise operators share rows across cores; and the compiled graph is no longer
  behind a lock, since running it only reads it. Every output sample still adds its terms in
  the definition's order, so all of it is bit-identical, and three tests say so. 912 ms an
  inference became 257 ms; a twelve-face group went from 8.2 s to 2.6 s.
- **Frequency healing, 1.5 s each time it ran**, was some sixty box blurs a pass over a face.
  The vertical pass of each blur summed a band as tall as a face on one core; bands are cut into
  column strips now, each writing its own pieces of the rows (bit-identical, tested). And on a
  large face - a frequency radius of eight pixels or more, which is a full-size export - the
  marks are found and the tone rebuilt on a grid two to four times coarser, because every
  measurement doing that is a mean over 0.3 to 24 radii; the mark's own tone, the pores put back
  and the relief they are limited by stay at full resolution. Held to the full-resolution result
  at four radii by `a_large_face_healed_on_the_working_grid_matches_full_resolution`: mean
  difference under 0.1% of the skin's tone, pore energy within 0.6%, and no more than 1.3 points
  more of a spot's redness left. A 2.2 MP face went from 3.1 s to 0.4 s.
- **Clarity, 0.7 s**, used a blur that summed each window sample by sample; the loops are
  interchanged so a whole row of windows is summed at once, bit-identical to the definition,
  which is what a tile and the whole frame agreeing bit for bit depends on.
- **Export** rendered a batch, then wrote, synced and read back that batch one file at a time
  while every core waited. `crate::ahead` keeps the workers rendering up to two photographs each
  ahead of the writer, which still writes in the job's order.
- Skin smoothing, texture restore and the per-person mattes were per-pixel loops on one thread;
  they share rows across cores, with the same arithmetic.
- A machine with twelve threads or more is given four image workers rather than three.

Measured on the same twenty photographs in release: 9.8 s a photograph on one worker, 5.5 s with
four. In the real app, on all 1,000 photographs plus 57 CR2 files (an optimised build with debug
assertions, so the isolated test catalogue is honoured), the folder run took **2 h 1 min**:
import 50 s, analysis 70 s, edit and retouch 29 min, export 89 min, with the processor at 91-95%
throughout export and 9 GB at peak. It had taken seven to eight hours.

**The hour is not met, and the reason is measured.** The slowest exports are full-length
portraits: body-skin spot healing runs at a frequency radius of a few pixels over a body
rectangle of five megapixels or more, about 3.5 s a pass, two passes, roughly 11 s a photograph.
The working grid cannot help there - the radius is the scale of the spots - and neither can more
workers, because the processor is already saturated. Restricting that work to the skin inside
the rectangle, rather than the whole rectangle, is the next change.

A camera file whose mosaic this build cannot decode (`AURA-RAW-2007`, Canon CR2 here) is edited
from its embedded preview but cannot be rendered at full size. A folder run now names those
files and leaves them out before the export starts, so the rest is delivered as a complete,
sealed set; before, the 57 CR2 files turned a run that delivered 1,000 verified photographs
into one reported as failed.

## What is not claimed

The thresholds were set on these photographs and judged by eye and by measurement, not by
photographers; no preference study has compared this look with Evoto's. The 1,000 photographs are
free-licensed Commons uploads, most already finished JPEGs, and include some that are not
weddings (award ceremonies, a monitor lizard, vintage cars). Skin limits are measured on detected
faces only; a face the detector misses is not protected by the skin check.
