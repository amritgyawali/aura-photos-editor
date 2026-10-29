# ADR-0191: Tone and colour to the photographer's own result, solved in a model of the renderer

Status: accepted. Amends ADR-0190: the whites lift is replaced, and the white-balance goal changes.
Date: 2026-10-29

## Problem

The photographer asked for the automatic edit to work on any kind of photograph and to edit
every one as well as it can be edited. "As well as it can be" needs a measure, and the machine
had one nobody had used.

**The pairs.** 7,904 camera files on this machine have a finished JPEG the photographer exported
from Lightroom, found by file name and checked by capture time. They cover:

* nine cameras: Canon EOS 5D Mark IV, EOS R and 6D Mark II; Sony A7 III, A7 IV and A7R III;
  Nikon D780, D7500 and Z 50;
* day and night, flash and ambient light, ISO 100 to 12,800.

**How a photograph is scored.**

* `crates/aura-app/tests/photographer_pairs.rs` renders each camera file as shot and after AURA's
  edit.
* `scripts/photographer-pairs.py` aligns the photographer's JPEG onto the render (they crop and
  straighten), then compares the two in CIE L*a*b*:
  * the mean colour difference per pixel (dE);
  * lightness at five percentiles;
  * the skin in the middle of each face, found by the YuNet model AURA ships;
  * the colour cast and the colour strength (chroma).

**What the baseline showed.** On 66 photographs stratified by camera and light, **the full
automatic edit was further from the photographer's photograph than the camera file left
alone**: dE 21.9 against 20.0 as shot.

* Skin was 16.9 L* darker than the photographer's skin.
* The middle of the frame was 15 L* darker.
* AURA's colour was greener and bluer, and weaker.

The causes:

* **Exposure.** The foundation decides exposure on the camera's embedded preview
  (`photo_enhance::correction` on the 512 px tier-1 thumbnail), not on AURA's render of the raw
  data. It is limited to ±0.75 EV, and to nothing at all when the median is between 45 % and
  78 %. The photographer brightens by +0.4 to +0.9 EV on nearly every photograph
  (`docs/lightroom-catalogue-study-2026-10-29.md`), and AURA's render of a raw file is darker
  than the camera's preview.
* **Colour.** Skin hue was already right (43.3° against the photographer's 43.5°). Skin chroma
  was low because skin was dark: chroma in L*a*b* follows lightness.
* **White balance.** At the same neutral references, the photographer keeps about a fifth of a
  warm cast, about seven tenths of a cool one and half of a magenta-green one. ADR-0190 kept
  15 %, none and none.

## Decision

Stage 1 of the full automatic edit ends with two measured steps, after stage 2's white balance
and before calibration. They are `shape_tone` and `enrich_colour` in `full_edit.rs`; the
arithmetic is in `crates/aura-app/src/tone_target.rs`. They replace ADR-0190's whites lift.

### Tone (`shape_tone`, `tone_target::shape`)

1. **The light as shot.** The photograph is rendered at 960 px (the measuring render) with
   exposure, contrast, highlights, shadows, whites and blacks at zero and no automatic levels
   curve. It is read as:
   * L* at the 2nd, 10th, 50th, 90th and 98th percentiles;
   * the median L* of the skin in the middle of each face;
   * median chroma.

   Because this reading ignores the tone in place, a second run reads and decides the same
   thing.
2. **The photographer's lightness for that light.** A ridge regression learned from 651 pairs
   predicts the five percentiles and the skin L* of the photographer's finished photograph.
   * Ten-fold, on photographs the fit did not see, it lands within 4.3 to 6.2 L* of the
     photographer (skin 6.1). One fixed target for every photograph lands 6.8 to 12.1 away.
   * Trained on 231 pairs, it predicted 420 later ones as well as it predicted its own (skin
     7.0).
3. **Intent.** The pairs are nearly all of people, so a frame without people goes only part of
   the way:

   | Frame | Share of the way |
   |---|---|
   | Ordinary scene | 80 % |
   | High-key (and it is never darkened) | 60 % |
   | Low-key | 50 % |
   | Night scene lit by its own lights | 35 % |

   Bounds on the targets: no target more than 30 L* above or 15 below the light as shot; skin
   between L* 48 (or where it was, if darker) and 74; the white point below L* 97.
4. **Solved in a model of the renderer.**
   * The tone-neutral render is sampled at 8,000 points, each with the luminance of its region.
     The samples are taken back to the working space, and the renderer's own functions are
     applied in the renderer's order: `tonemap::exposure`, `edge_aware::tone_pixel` (local
     shadows and highlights on the region), `tonemap::tone` (whites and blacks),
     `tonemap::contrast`, `output::map_one`.
   * Coordinate descent over the six sliders, with paired moves (exposure up with highlights or
     whites down) and halving steps, minimises the squared L* distance to the targets.
   * Skin counts five times a percentile; the middle of the frame counts twice when there are
     no people.
   * Every slider costs a little, and skin past L* 80 is heavily penalised.
   * Clipping past what the camera did plus 0.3 % costs more the further it goes. A pixel that
     was already bright as shot (L* 85: a curtain, a wall, a window) counts 0.15. A saturated
     colour at the top of one channel (a red sari, a pink canopy) counts 0.05. Only a white
     burnt out in every channel counts fully. The photographer lets backgrounds go to white for
     the people in front of them, never a face.
   * A slider a person set themselves is held where they left it.
5. **Checked on the render.** The solve is rendered and read. When skin (or the middle, without
   people) misses by more than 1.5 L*, the targets are moved by what the model got wrong and the
   solve runs again: at most three renders. The closest render wins. It is kept only when it is
   closer to the photographer's tone than the tone already in place.

### Colour strength (`enrich_colour`)

After the tone, because chroma follows lightness:

* The frame's median chroma is brought to the photographer's for that light. That chroma is
  predicted by the same regression, 4.5 from theirs on unseen photographs; one fixed value
  lands 8.7 away.
* Vibrance moves first, saturation only when vibrance is not enough.
* Skin chroma stays below 34, and nothing new is pushed past the edge of the colour space.
* The result is checked on the render.
* Black-and-white and one-colour frames are left as they are.
* It runs with no look chosen.

### White balance (`balance_goal`, amending ADR-0190)

The goal keeps what the photographer keeps, measured at the same neutral references on 650
pairs:

* 20 % of a warm cast, or 50 % when there are no people, because without skin to judge by,
  warm light is most often the picture;
* 70 % of a cool cast;
* half of a magenta-green cast, of at most 0.15 read.

ADR-0190's checks (faces, blown sky, mixed light) are unchanged.

### Faces

The measured stages now read the small faces of a crowd from the proxy as well
(`detect_small_faces`), as the advanced retouch already did.

### Reporting

Each step reports under Basic edit:

* what moved;
* skin, shadows, middle, highlights and white point: as shot, now, and the target;
* the share clipped.

Each step is its own history step and sends a live picture. A chosen look keeps its own tone and
colour.

## Measured

On **80 photographs from the pairs the model never saw** (nine cameras, day and night, flash and
ambient), against the photographer's own finished photograph:

| | Mean dE | Middle (L* miss) | Skin (L* miss, mean) | Skin (\|miss\|) |
|---|---|---|---|---|
| As shot | 21.9 | 19.7 | -20.5 | 20.5 |
| White balance, tone and colour (`basics_only`) | 13.8 | 7.0 | -4.1 | 7.8 |
| The full automatic edit, all ten stages | 15.1 (13.9 on the 68 it did not crop) | 7.2 | -4.0 | 7.7 |

* Every camera and every lighting condition came closer.
* Twelve of the 80 were straightened or cropped by stage 5, which the alignment (fitted on the
  photograph as shot) cannot follow. On the other 68, the full edit lands at dE 13.9, beside the
  13.5 of the measured basics alone.
* The red-green cast now matches the photographer's (da -0.08). AURA stays a little bluer (db
  -1.3).

On the 66-photograph baseline set:

| | dE |
|---|---|
| The old full edit | 21.9 |
| White balance, tone and colour (`basics_only`) | 14.9 |

Eleven photographs of other kinds (aerial, landscape, wildlife, studio and window-light portraits,
a mountain at dusk) were checked by eye before and after. Nothing broke. The golden-dust elephant
lost its warmth under the old white-balance goal, and the scene rule above keeps it.

A repeat run (`AURA_FULL_EDIT_REPEAT=1` on `full_edit_photos.rs`, 12 photographs) changes no
white balance, tone or colour on any of them. Two things made that hold: saturation starts from
zero on every run, as calibration and grade do (the foundation never sets it), and a tone an
earlier run solved is kept while its render is as close to the targets as a new solve would come.

## Consequences

* The foundation's exposure, highlights, shadows, whites and blacks are now the starting point
  the full edit replaces. Auto enhance and the advanced retouch on their own are unchanged.
* Two to four more 960 px renders a photograph, and about a second of solving.
* The targets are this photographer's. A learned personal style (ADR-0104) or a reference look
  still applies afterwards.
* Retrain with `scripts/photographer-pairs.py` when more pairs exist. The coefficients and their
  errors are in `tone_target::MODEL` and `CHROMA_MODEL`.
* `tests/tone_solve_debug.rs` replays the solve on saved renders and prints what holds it back.
