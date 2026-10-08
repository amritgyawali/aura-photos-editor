# Edit profiles

The first screen of AURA is three steps:

1. **Choose a look** - an edit profile, or *Auto only*.
2. **Match a photographer's Instagram** - optional. Paste a public profile link, or use a folder of
   saved reference photos.
3. **Upload a photo or a whole folder.** Import starts, and when it finishes every photo is edited
   automatically with the look you chose, then with the reference on top if you added one.

Nothing on disk is changed. Every edit is a recipe that can be undone, and any slider you move by
hand is never overwritten by a profile, a reference match or a later automatic pass.

## What a profile is

A Lightroom preset writes absolute numbers: *exposure +0.7, shadows +40*. That is right for the
photo the preset was built on and wrong for every other one - it over-brightens an indoor frame
and flattens a backlit one. An AURA profile is a **residual on top of a measurement**:

```text
neutral develop of your photo
  -> AURA measures it: exposure, highlights, shadows, contrast     what this photo needs
  -> + the profile's creative adjustments x strength               what the look adds
  -> six scene guards, each of which can only make the look gentler
  -> merge that refuses every setting you changed by hand
```

Because every application starts again from the neutral develop, applying a profile twice, or
switching from one profile to another, never compounds.

The six guards, each reported in a sentence when it acts (in the preview and in the photo's edit
history):

| Guard | When | What it does |
|---|---|---|
| Highlight headroom | a brightening look on a frame whose brightest percentile is near white | applies only the brightening that fits |
| Low-key frames | a darkening look on a frame that is already dark (median below 22 %) | darkens it 60 % less |
| Colour lean | a warming look under tungsten, or a cooling look at blue hour | scales the white-balance offset down, never below 20 % |
| Already vivid | a colour boost on a frame whose 90th-percentile chroma is high (stage light, neon) | halves the boost |
| RAW-learned look on a JPEG | a learned profile on a photo the camera already developed | applies it at half strength, because the camera already added the contrast and colour the retoucher lifted out of the RAW |
| Skin | any colour profile | orange - the band every skin tone sits in - is held to a ±6 hue rotation and -20..+15 saturation |

Strength runs from 0 to 150 %. At 0 the result is exactly *Auto only*; above 100 % the creative
half is extrapolated, the curve and the black-and-white mix are not.

## The profiles

Sixteen **researched** profiles translate published before-and-after walkthroughs of well-known
looks into AURA's recipe units. Each one lists its sources in the gallery (*How this look is
built*).

| Profile | Category | The look |
|---|---|---|
| True Natural | Everyday | Balanced light, a touch of vibrance and crispness |
| Light & Airy | Bright | Lifted exposure, soft contrast, pastel greens and yellows |
| Dark & Moody | Moody | Lower exposure, matte blacks, muted darkened greens and blues, vignette |
| Cinematic Teal & Orange | Cinematic | Greens and blues rotated to teal, warm skin, lifted-black S-curve |
| Portra Film | Film | Peachy skin, olive greens, soft highlight roll-off, warm grey blacks |
| Vintage Fade | Film | Both ends of the curve faded, warm muted colour, vignette |
| Golden Hour Glow | Warm | Strong guarded warmth, glowing yellows, softened clarity |
| Classic Monochrome | Black & White | Bright skin, darker skies, full tonal range |
| Film Noir | Black & White | Red-filter mix, steep curve, deep blacks, heavy vignette |
| Vivid Landscape | Nature | Recovered skies, open foregrounds, clarity, dehaze, rich blues and greens |
| Soft Portrait | Portrait | Negative texture for skin, masked sharpening for eyes |
| Neon Night | Cinematic | Cool shadows, enriched city lights, high-ISO noise reduction |
| Editorial Matte | Editorial | Matte curve, calm muted colour |
| Fresh & Crisp | Bright | Food and product: texture, clean whites, rich warm colours |
| Romantic Wedding | Wedding | Bright but not washed out, sage greens, dress highlights held |
| Nordic Cool | Moody | Cool, desaturated, greens toward teal |

Five **learned** profiles were measured rather than written - one per FiveK retoucher - as research.
They are **not shipped**: FiveK is licensed for research only (see the limits below). See the next
section for how. What each one does, read back off the fitted medians, and how much closer it brings
AURA to that retoucher's finished photograph on RAW files it never learned from (mean ΔE00, lower is
closer):

| Profile | What the retoucher does beyond AURA's correction | Pairs learned / held out | Auto → auto + profile |
|---|---|---|---|
| Pro Retoucher A | Saturation +48, contrast +15, blacks -24, whites +12, neutral white balance | 13 / 5 | 14.03 → 13.16 |
| Pro Retoucher B | +400 K warmer, tint +6, saturation +16, gentle contrast, lifted lower midtones | 13 / 5 | 13.21 → 11.55 |
| Pro Retoucher C | Saturation and vibrance +40, shadows +24, blacks -36, whites -24, lifted lower midtones | 25 / 9 | 9.95 → 9.33 |
| Pro Retoucher D | +400 K warmer, shadows +32, blacks -30, contrast +17 | 13 / 5 | 16.74 → 13.91 |
| Pro Retoucher E | +800 K warmer, saturation +40, shadows +25, blacks -36 | 13 / 5 | 16.01 → 13.96 |

Read the last column honestly. Each photograph's own best-fit settings reproduce the retoucher's
final to about 2-3 ΔE00, so the settings exist in AURA's renderer; what one *constant* profile
cannot do is know that this particular frame needed another stop. The profile closes 6-17 % of the
gap on unseen photographs, which is the retoucher's consistent taste; the rest is per-photo
judgement. The held-out sets are small (5 to 9 photographs), so treat the numbers as indicative.

Profiles use Lightroom's own panels (ADR-0070): colour grading for split-toned looks (teal and
orange, film, moody, monochrome toning), film grain, camera calibration (the landscape "blue
primary" trick), the parametric curve, and a highlight-priority post-crop vignette. Colour grading
in a profile is capped on the midtones (20) and highlights (30) wheels, because those wheels colour
every face in the frame. Every Lightroom panel is also in the photo studio's Develop panel for a
photographer who wants to take over - see [lightroom-parity.md](lightroom-parity.md).

## Learned profiles: copying a retoucher's settings from RAW before-and-afters

The [MIT-Adobe FiveK dataset](https://data.csail.mit.edu/graphics/fivek/) is 5,000 camera RAW
files, each retouched by five professional retouchers (A to E). That is exactly the "before" and
"after" a profile should be learned from: the RAW is what came off the sensor and the final is what
a professional delivered.

```bash
# 1. Download pairs: the DNG, and one retoucher's final converted to sRGB.
python ml/edit-profiles/fetch_fivek_pairs.py --out D:/aura-data/fivek --expert c --count 40

# 2. Recover the settings and measure the result on held-out pairs.
AURA_FIVEK_DIR=D:/aura-data/fivek AURA_FIVEK_EXPERT=c \
  cargo test -p aura-app --test profile_fit learn -- --ignored --nocapture
```

For every pair, `crates/aura-app/tests/profile_fit.rs`:

1. decodes the DNG with **AURA's own decoder** - the same pixels a photographer's RAW produces;
2. measures what AURA's automatic correction would do to it;
3. recovers, by coordinate descent **through the real renderer**, the twelve recipe settings that
   reproduce the retoucher's final (`aura_style::fit`). These are the retoucher's settings in
   AURA's units, not a guess at Lightroom's;
4. takes the median, over the training pairs, of what the retoucher did *beyond* the automatic
   correction - that is the profile;
5. measures, on every fourth pair (never used for learning), the mean ΔE00 from the retoucher's
   final with the automatic correction alone and with the correction plus the profile.

The result is written to `ml/edit-profiles/fivek_expert_<x>.json` with every per-pair fit, and the
profile's `evidence` block carries the held-out numbers. The gallery shows them on the profile.

Limits, stated plainly:

* A retoucher also dodges, burns and crops. A pair whose residual shows local work is rejected
  rather than learned from; what is left is the retoucher's *global* style.
* A median over a few dozen photographs is a starting point. It is not the retoucher.
* **Learned profiles are not shipped.** FiveK's images - including the `LicenseAdobeMIT` subset,
  whose name suggests otherwise - are licensed for research only, "not in any manner intended for or
  directed toward commercial advantage" ([licence](https://data.csail.mit.edu/graphics/fivek/legal/)).
  A profile fitted on them is kept in `ml/edit-profiles/fivek-research-profiles.json` for research
  and evaluation, and `scripts/third-party-notices.py` fails the installer build if one reappears
  in `config/edit_profiles.json`. ADR-0106.

## Your own style, learned from your Lightroom catalogue

On the start screen, **Teach AURA your style** asks for a Lightroom Classic catalogue (`.lrcat`)
and a name. AURA copies the catalogue, reads how you developed every photograph in it, and builds
a profile of your own, badged **Yours**, listed first wherever profiles are offered.

What it learns is what you consistently do: your contrast, highlight, shadow, white and black
handling, texture, clarity and dehaze, vibrance and saturation, HSL, tone curves, colour grading,
calibration, grain, sharpening, noise reduction, vignette, and black and white. Each is reported
as a sentence with how often you use it.

What it does not learn is the light. White balance and most of the exposure are measured on every
new photograph, as they always are, because a value you set for one wedding's light says nothing
about the next one's.

**Connect the original photographs before learning, if you can.** With the originals where the
catalogue expects them, AURA measures up to 400 of them and learns how your tonal sliders follow
the photograph - harder highlight recovery on a bright sky, more shadow lift in a dark church -
and keeps that only where it predicts your own settings better than one fixed value, on
photographs it never trained on. On FiveK that took the result from 10.2 to 8.2 ΔE00 from the
retoucher's final, where one fixed value per slider managed 10.6 to 10.2. Without the originals
you get one consistent look, which is still yours.

Nothing is written to the catalogue or next to your photographs. Learning again with the same name
replaces the old profile. Decisions and measurements: ADR-0104.

## Adding or changing a profile

Profiles live in `crates/aura-app/config/edit_profiles.json`. The table is validated when it loads
and by the unit tests: unique ids, every slider in range, a monotone curve, known band names, a
researched profile must name its sources and a learned profile must carry its measurement. The
tests also render every profile through the real renderer and fail if two profiles look the same or
if one barely changes the photo.
