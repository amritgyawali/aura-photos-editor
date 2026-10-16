# ADR-0120: Measured light across shooting conditions, and a one-click finish that matches the calibrated one

Status: Accepted
Date: 2026-10-14

Until now automatic light and colour were checked against six published before-and-after
pairs as delivered. Those are finished photographs: they say whether Auto edit leaves a good
photograph alone and nothing about whether it repairs a bad one. This records a condition
matrix, what it found, and what changed.

## 1. The matrix

`scripts/make-condition-variants.py` derives seven conditions from every pair, in linear
light where a camera makes them: as delivered, one stop under, 0.6 of a stop over (clipping as
a camera clips), a warm cast, a cool cast, a green cast, and a flat, veiled frame. The finished
half is kept, so a correct automatic edit of a damaged frame comes back towards it. Scored as
mean absolute error against it.

Two sets: the six published pairs (42 frames), and ten public photographs of other subjects -
a dog, food, a sunset landscape, groups, outdoor and studio portraits on coloured backdrops -
whose own file is the finished version (70 frames). The second set is the one that found the
largest fault, because it holds kinds of photograph the first does not.

`crates/aura-app/tests/reference_pairs.rs` mode `light` runs the light and colour half of Auto
edit headlessly over a folder in a few minutes; `AURA_PAIR_REVERT` re-renders with one group of
controls back at neutral, which is how each control's share of a result was measured before
anything was changed.

## 2. What was wrong, and what replaced it

| Finding | Change |
|---|---|
| A flat or veiled frame came back worse than it went in (153.5 to 158.6). Whites and blacks are band sliders of at most 1.6x and cannot take a pedestal off. | `levels`: when the darkest tones sit above about a sixth of the range and the brightest stop short of white, the black and white points are set on every channel's curve alike. Per channel puts colour back with contrast; one curve for all three leaves white balance alone. |
| The first levels curve, four corner points, came out as an S: the monotone interpolant flattens at both corners. | The run between the end points is drawn with eight points, which keeps it straight. |
| Highlights were taken down whenever the brightest twentieth of a frame was bright: a white blanket, a dress, a studio wall, greyed. | `highlights_on_evidence`: only as far as skin (with people) or the share of the frame at white (without) says detail was lost. An open sky is left to the histogram. |
| Overexposure two thirds of a stop over was not recognised: skin luminance rarely reaches the clipping test before red clips. | `Skin::ceiling`: 3% of the skin with a channel at the top of the range is overexposure. Measured at most 0.1% on every delivered photograph and 4% to 30% on the same frames over. **Dropped in the second pass (section 4)**: it did not hold on photographs it had not seen. |
| A finished photograph's own colour - a warm backdrop, a cool studio grey - was removed as a cast (a group headshot moved 600 K). | `CAST_BY_CHOICE`: casts up to 0.10 in log chroma are kept, and a larger one is reduced by only the part beyond that. Every delivered photograph read 0.05 to 0.12. |
| A one-ended stretch brightened skin in frames with nothing white in them (a grey backdrop, a low-key portrait). | Whites are stretched only when the very brightest tones (99.8th percentile) stay short of 90%, and not when the people are lit brighter than the scene around them. |
| A dark but finished photograph without people was lifted half a stop (a dog on a dark floor, a sunset). | When the very brightest tones already reach 85%, exposure gets at most 0.15 EV and the shadows at most +15. |
| A lifted floor behind people was deepened by up to -25: most often a soft finish. | Black point behind people at most -12. |

## 3. Results

Total error over each matrix, before and after; lower is closer to the finished photograph.

| Condition | Published pairs, before | after | Other subjects, before | after |
|---|---|---|---|---|
| As delivered (untouched 30.8 / 0.0) | 38.2 | 33.4 | 75.0 | 41.5 |
| One stop under | 65.1 | 68.3 | 172.8 | 175.2 |
| 0.6 stop over | 122.9 | 97.5 | 116.8 | 110.5 |
| Warm cast | 51.7 | 48.1 | 138.6 | 113.4 |
| Cool cast | 47.6 | 47.0 | 85.9 | 63.1 |
| Green cast | 47.2 | 42.5 | 86.1 | 49.5 |
| Flat / veiled | 158.6 | 60.0 | 83.5 | 86.7 |
| **All** (untouched 680.0 / 936.6) | **531.3** | **396.8** | **758.8** | **640.0** |

The one-click retouch now uses the finish calibrated per detail band in ADR-0118 (smoothing
0.05, texture 0.9, pore refinement 0.8, no dodge and burn, four healing passes), keeping
freckles and dark marks: removing those is a decision about somebody's face, and the Portrait
skin finish preset makes it on request. On the four retouch pairs it changes the mean error by
under 0.02 and reproduces more of the published change (tone share 0.05-0.17 to 0.10-0.22).
A vision model's skin advice may set how hard marks are looked for, within two to four passes
and a sensitivity of at least one half, and no longer sets texture handling.

## 4. Second pass: a held-out set and real camera files

The first pass set its thresholds on the same 112 frames it was scored on. The second pass
added two sets nothing was tuned on:

- **22 further public photographs** (154 frames in the same seven conditions): portraits of
  several skin tones, black-and-white portraits, landscapes, a cat, an abstract.
- **10 camera files from MIT-Adobe FiveK** (`ml/edit-profiles/fetch_fivek_pairs.py`, research
  licence, not committed): real exposure and colour mistakes, scored against a professional's
  finished version. Five more were cropped by the expert and are skipped by the scorer.
  `crates/aura-app/tests/reference_pairs.rs` takes `NAME-before.dng` for these.

What changed, and why:

| Finding | Change |
|---|---|
| Underexposed people were lifted at most 0.25 EV whenever they were brighter than the dark around them: the rule that keeps a dark complexion in a dark room from being lightened also kept every underexposed portrait dark. | A neutral object brighter than the people and well short of white (four fifths of a stop of room or more) is a witness the people cannot be: `white_object_short_of_white`. Neutral, because skin of every tone carries colour. |
| The underexposure lift took 80% of the room. | 90%. |
| Dark finished photographs without people were still lifted a little, which took a dark scene two thirds of a stop over further away. | No exposure lift once the brightest tones reach near white; at most +10 to the shadows. |
| A clipped colour channel on skin looked like a clean overexposure witness on the first 112 frames (none as delivered, 4% to 30% two thirds of a stop over). On the held-out photographs, bright commercial portraits exposed as intended had 3% to 14%, warm light raised it further, and no threshold separated them. | Dropped. Overexposure with people is read from skin near white in luminance, as before this ADR. |
| A camera file's white balance is the camera's guess, and it was treated as a finished photograph's choice. | For camera files only: the cast kept by choice is 0.04 rather than 0.10, a small neutral area will do, and when the neutral areas and the edges disagree (mixed light) the whites may side with one of them. |
| The advanced retouch planned its foundation with its own copy of the exposure guards. | It calls the shared ones, so the veil and the evidence rules apply there too. |

Totals after the second pass; lower is closer to the finished version.

| Set | Untouched | First pass | Second pass |
|---|---|---|---|
| Held-out photographs, 154 frames | 2020.0 | 1318.2 | **1232.7** |
| Camera files against an expert, 10 | 299.7 (neutral render) | 222.5 | **220.1** |
| Ten other subjects, 70 frames | 936.6 | 640.0 | **588.2** |
| Published pairs, 42 frames | 680.0 | 396.8 | 410.8 |

By condition on the held-out set: as delivered 0 to 73.8 (3.4 a photograph), one stop under
714.4 to 179.5, two thirds over 462.8 to 403.2, warm 92.5 to 120.2, cool 93.4 to 135.2, green
31.0 to 88.8, flat 625.8 to 231.9.

## 5. Third pass: exposure on evidence, and any kind of file

Re-rendering 32 unchanged photographs with one control at a time put back at neutral showed
that exposure did half of what was still changing them: a capped histogram lift of 0.1 to
0.25 EV on finished portraits with dark surroundings. The rule that kept those lifts small
already said the surroundings were no reason to lift the people; it still lifted them.

| Change | Why |
|---|---|
| With people in the frame, the histogram alone never moves exposure. It moves on evidence: skin near white (down), or nothing in the frame reaching white with a witness that is not the people (up). | Unchanged photographs on the held-out set now move 1.7 on average instead of 3.4. |
| Skin near white counts as overexposure from 6% of the face, not 12%. | Two thirds of a stop over put 2.5% to 9.5% there; no photograph exposed as intended had more than 4.1%. |
| The underexposure witness is either a neutral object brighter than the people, or the brightest tones of the frame lying outside the people (a cream blanket, a page under warm light). | Either one says the frame's top is not skin, which is what keeps a dark complexion in a dark room from being lifted. |
| JPEG, PNG and TIFF decodes get a deadline in proportion to the pixels the file declares (0.4 s a megapixel, at least the tier's own, at most 60 s). | A 35-megapixel JPEG failed as "took too long to open" against a fixed 5 s deadline; a 45-megapixel camera's files are ordinary. |
| `aura-raw` is optimised in development builds, as `aura-portrait` and `aura-render` already were. | Every JPEG and PNG is decoded there; the desktop shell already built it optimised. |

Eleven awkward files were run through the light correction and the full one-click edit:
48 x 32 pixels, 35 megapixels, a 6000 x 900 panorama, greyscale, a PNG with transparency, a
16-bit PNG, a CMYK JPEG, solid black, solid white, one flat colour, and pure noise. All
eleven open, edit and render at their own size. HEIF and HEIC are not decoded by this build.

Totals after the third pass:

| Set | Untouched | Second pass | Third pass |
|---|---|---|---|
| Held-out photographs, 154 frames | 2020.0 | 1232.7 | **1124.4** |
| Camera files against an expert, 10 | 299.7 | 220.1 | **212.4** |
| Ten other subjects, 70 frames | 936.6 | 588.2 | **547.3** |
| Published pairs, 42 frames | 680.0 | 410.8 | 443.7 |

The published pairs are the one set that moved back: one newborn one stop under (11.0 to
36.0), on which neither underexposure witness fires and which the histogram alone used to
lift, and three portraits two thirds of a stop over that the histogram used to pull down by
a quarter stop without evidence. Both were the histogram being right by luck on frames where
it is wrong just as often on finished photographs.

## 6. Fourth pass: what the vision models can tell, camera files, and HEIC

**Can a vision model tell overexposed from bright on purpose?** Measured rather than assumed:
the configured providers graded the 22 held-out photographs as delivered and the same frames
two thirds of a stop over (`reference_pairs.rs` mode `ai`). Of 31 answers, the models asked to
darken 4 of 16 good photographs and 5 of 15 overexposed ones; one model answered -0.5 almost
whatever it was shown. On the same frames the measurement, which moves exposure only on
evidence, darkened every overexposed copy it saw by 0.24 to 0.5 EV and none of the good ones.
A model is not a better witness than the pixels here, so it does not get to overrule them.
What changed is the opposite case: a model answering "leave it" used to undo an overexposure
the measurement had found. `confirmed_exposure` now starts from the measurement and lets the
model move it by at most 0.15 EV either way.

**Camera files get a base contrast of +15** when nothing asks for more: a neutral rendering
of a sensor is flatter than any camera's own picture, because every camera and converter lays
a base curve over it. The value was chosen beforehand and only checked: on the 16 FiveK files
of the earlier passes 211.8 without it, 209.0 with it; on 12 new FiveK files nothing had seen
115.8 without, 112.1 with (5 that could be scored at the time). Not fitted to FiveK, whose
research licence ADR-0106 keeps out of anything shipped.

**One of the new camera files is truncated** (its tile table ends 0.9 MB past the end of the
file; a second decoder fails on it too). AURA refuses it as damaged, which is correct.
`crates/aura-raw/tests/probe_file.rs` prints the decoder's own detail for one file, and the
pair harness now records a file it cannot open and carries on with the rest.

**HEIC is refused with its own code**, `AURA-RAW-2009`, which says what to do (export as JPEG,
or set the iPhone to Most Compatible) instead of "AURA does not recognise this file as a
photo". The pixels are HEVC-coded; this build has no HEVC decoder, and the Windows one is an
optional codec that is not installed on the reference machine.

**A final check on 30 photographs nothing had seen** (210 frames in the same seven
conditions, run once): 2814.8 untouched, 1826.2 after. One stop under 1044.2 to 251.1; flat
788.9 to 389.8; two thirds over 663.0 to 609.2; as delivered 0 to 87.2 (2.9 a photograph);
warm, cool and green casts 136.7, 138.0, 44.0 untouched to 179.0, 185.6, 124.4. On the 8 new
camera files that could be scored, 262.1 to 174.0.

## What this does not establish

That Auto edit is right in every condition. On photographs it has never seen it changes a
good photograph by about 3 on average, more than on the sets it was developed on, and every
colour-cast variant of the final unseen set came back further from the original than it went
in: finished photographs' own colour and a cast are not separable from pixels, and the vision
models tried here do not separate them either. Overexposure of two thirds of a stop is
recognised only where skin reaches white or the frame is bright by measurement; a bright
portrait slightly over and one exposed brightly on purpose read the same from pixels. A
portrait on a saturated backdrop one stop under is not lifted, because nothing in it is
neutral. Casts on finished photographs are still corrected only partly or not at all, and on
average the cast variants of the held-out set came back further from the original than they
went in: what a finished photograph's colour is meant to be cannot be read from it. A soft
finish under a veil loses some of its softness with the veil. On camera files the largest
remaining differences from the expert are brightness and contrast choices, which are taste.

The conditions are synthetic except on the ten camera files, and those are ten. The held-out
set was used once to choose between two versions of the overexposure rule, so it is no
longer entirely unseen. The published photographs were retouched, not regraded, so
"closer" there partly means "changed less".
