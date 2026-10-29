# How the photographer edits: a study of the Lightroom catalogues on this machine

Date: 2026-10-29. Every drive (C:, F:, G:, H:, I:) was searched for `.lrcat` files; each was
copied to `I:/aura-lr/copies/` and read only from the copy, so no catalogue was opened by
Lightroom or written.

## The catalogues

| Catalogue | Years | Photos | RAW | Custom white balance | Originals still on disk |
|---|---|---|---|---|---|
| `C:/Users/Admin/Pictures/Lightroom/Lightroom Catalog.lrcat` | 2026 | 5,575 | 5,270 | 47 % | 692 (F:, H:, I:, C:) |
| `I:/New folder (2)/Lightroom/Lightroom Catalog-v11-3.lrcat` | 2022-23 | 15,471 | 12,968 | 84 % | none (D: and E:, not connected) |
| `I:/New folder (2)/Lightroom/Lightroom Catalog-v12.lrcat` | 2022-23 | 5,454 | 4,533 | 97 % | none |
| three small catalogues in the same folder | 2022 | 63 | 62 | - | none |

26,494 different photos (by file and capture time), 20,371 RAW photos developed by hand.
Cameras: Canon EOS 5D Mark IV (16,382), Nikon D750, Canon 5D Mark III, Sony A7 IV, A7R V, Nikon D780.
Profile: Adobe Color on Adobe Standard for nearly all. Flash fired on 72 % of photos.

## How each photo is edited

The history of every photo has the same shape: **Import → Synchronize Settings** (one photo
edited, its settings synced to the rest of the shoot) → **per-photo touches** (Temperature,
Exposure, Crop) → Export. So the style is a shoot-wide base plus a few sliders moved per photo.

**Moved per photo** (10th to 90th percentile inside one shoot and lighting, median over 48 groups):
Temperature about 590 K, Exposure 0.9 EV, Whites 31, Highlights 20, straightening about 6°,
Tint 9, Contrast 9, Vibrance 8. **Synced and left**: Shadows, Blacks (5 or less).

**White balance is set on nearly every photo**: custom on 86-100 % of photos in every
condition except bright daylight without flash, where the camera's balance was often right.
The chosen temperature is mostly 4150-4650 K (Lightroom's scale): warm venue and tungsten light
cooled, rarely to fully neutral.

## By lighting

Medians of hand-developed RAW photos; night means captured 18:00-06:00.

| Condition | Photos | Temp | Tint | Exposure | Contrast | Highlights | Shadows | Whites | Blacks | Vibrance | Custom WB |
|---|---|---|---|---|---|---|---|---|---|---|---|
| night, flash | 11,567 | 4150 | -3 | +0.53 | +11 | -68 | +28 | -56 | -12 | +15 | 91 % |
| night, ambient, ISO ≤ 400 | 2,983 | 4250 | -3 | +0.44 | +11 | -67 | +21 | -67 | -7 | +14 | 98 % |
| night, ambient, ISO 400-1600 | 1,665 | 4105 | -3 | +0.44 | +11 | -74 | +21 | -68 | -5 | +14 | 99 % |
| day, flash | 2,917 | 4650 | -3 | +0.53 | +11 | -65 | +35 | -47 | -14 | +14 | 86 % |
| day, ambient, ISO ≤ 400 | 662 | 4250 | +3 | +0.88 | +11 | -63 | +21 | -53 | 0 | +14 | 93 % |
| day, ambient, ISO 400-1600 | 450 | 4531 | 0 | +0.24 | +17 | -81 | +21 | -72 | -14 | +4 | 98 % |

The tone recipe is the same everywhere: **brighten** (exposure +0.4 to +0.9), then **pull the
highlights and whites down** hard (-65 to -81, -47 to -72) so nothing burns, lift the shadows,
anchor the blacks a little, add a little contrast and vibrance.

## Colour

* **Skin**: orange luminance +12 (on 46 % of photos), orange hue -20 on the 2022-23 shoots.
* Red saturation -13, yellow saturation -23 to -40, greens varied by shoot (hue +16 to +32).
* Colour grading and the tone curve's shape are almost never used beyond the Adobe Color look.
* Masks on 6 % of photos (28 % in 2026); no spot healing in Lightroom (retouching happens elsewhere).

## How the style changed

| | 2022-23 | 2026 |
|---|---|---|
| Contrast | +11 to +16 | +0 |
| Clarity | +5 to +11 | -8 to -25 (softer skin) |
| Texture | rarely | +20 to +24 |
| Saturation | +12 to +19 | 0 |
| Whites | -56 to -67 | often 0, moved per photo |
| Dehaze | +9 | +7 to +14 |

## What AURA learned, and what changed because of it

* **White balance is measured on every photo** (ADR-0190): the catalogues show it set on nearly
  every photo and varied per photo; AURA had corrected it on none of a 69-photo run.
* **Whites are moved per photo**: AURA's full edit now lifts whites that stop short of white
  and measures it on the render (ADR-0190). The photographer usually lowers whites after
  brightening; AURA brightens less, so its whites move the other way for the same reason - to
  put the brightest tones just under white.
* **A personal style can be learned now**: AURA's Personal Style (ADR-0104) read all three large
  catalogues (harness `crates/aura-app/tests/lightroom_style.rs`). On the 2026 catalogue it
  measured 346 originals and found exposure, highlights and shadows each adapting to the photo,
  9-12 % closer to the photographer's own setting than one fixed value on photos the fit never
  saw. The older catalogues have no originals connected, so only their taste can be learned
  (half of the tonal moves kept, the rest measured per photo). Learning it into the real
  catalogue is a click on the Personal Style screen ("Learn from Lightroom"), choosing the
  `.lrcat`.

Reproduce (on copies of the catalogues): `python scripts/lightroom-catalogue-study.py out.json a.lrcat [b.lrcat ...]`, then
`python scripts/lightroom-catalogue-conditions.py out.json`;
`AURA_LRCAT=<copy>.lrcat cargo test -p aura-app --release --test lightroom_style -- --ignored --nocapture`.
