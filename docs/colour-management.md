# How AURA handles colour

A plain-language explanation of what happens between the light that hit your sensor and the
pixels on your screen. If a render ever surprises you, the answer is on this page.

## One space, entered once and left once

Every adjustment AURA makes happens in **linear light with Rec.2020 primaries**. Two words
matter there.

**Linear** means the numbers are proportional to light, not to brightness as your eye sees
it. Doubling a number doubles the light. That is what makes an exposure slider exactly one
stop per unit, and it is what stops a shadow lift turning your shadows grey.

**Rec.2020** is a wide gamut - wider than sRGB, wider than Adobe RGB. It is not what your
photograph is delivered in; it is what it is *worked* in. A red sari, a stage light, a
saturated sunset: all of them exist inside Rec.2020 and some of them do not fit inside sRGB.
Working in the wider space means AURA never throws a colour away before the grade has
started.

Your photograph enters that space once, at the beginning, through your camera's own colour
matrix. It leaves once, at the end, through the output transform. Nothing in between converts
back and forth.

## The order things happen in, and why

```
your RAW file
  → black and white levels, then demosaic          (the sensor's own numbers)
  → highlight recovery                             (before white balance, deliberately)
  → white balance                                  (temperature and tint)
  → your camera's colour matrix                    (into the working space)
  → exposure, tone, contrast, curve, HSL
  → clarity, texture, dehaze
  → vibrance and saturation
  → black and white conversion
  → local masks
  → retouch and restoration
  → crop, straighten, perspective
  → output transform                               (the only place tone is baked)
  → sRGB, Adobe RGB or Display P3, 8 or 16 bit
```

Two of those positions are worth explaining.

**Highlight recovery happens before white balance.** When a highlight is bright enough to
blow out, it usually blows the green channel first - green is the most sensitive on nearly
every sensor. If AURA corrected the colour balance first and reconstructed the green
afterwards, every white dress in a window and every candle flame would come back magenta.
Recovering first, while the sensor's own numbers are still intact, is what keeps a blown veil
white.

**The output transform is last, and it is the only place tone is baked.** Everything before
it is reversible. That is why changing your output space from sRGB to Display P3 does not
re-grade the photograph - it re-*converts* it, once, at the very end.

## Highlights that are brighter than white

In linear scene-referred light, "white" is not the maximum. A specular highlight on a ring,
on a sequin, on wet glass, is genuinely brighter than a white wall - and AURA keeps that
extra range all the way through the pipeline. Your display cannot show it, so the output
transform rolls it off with a gentle shoulder rather than clipping it flat.

This is why a highlights slider can recover detail that looks completely gone in the preview:
it was never gone, it was just above what your screen can display.

## Camera profiles

Every camera body sees colour slightly differently. A profile is a measured 3x3 matrix that
describes how *your* body responds, and it is what turns your sensor's numbers into real
colour.

**AURA does not yet ship a profile for any real camera body.** Building one honestly requires
photographing a colour target under known light, and this build has not done that. Until it
has, an unfamiliar body renders through a neutral reference profile and AURA tells you so:
`AURA-RENDER-8008`, "AURA does not have a colour profile for this camera yet, so it used a
neutral one."

That is deliberate. A photograph that looks slightly flat and is *labelled* uncalibrated is
better than one rendered confidently through a matrix somebody guessed at, because nobody
checks the second kind.

## What comes out

| Space | Use it for | Transfer |
|---|---|---|
| sRGB | The web, email, most clients. The default. | sRGB's own curve |
| Adobe RGB (1998) | Print, when your lab asks for it. | gamma 2.2 |
| Display P3 | Modern phones and laptops. | sRGB's curve, wider primaries |

Every export is colour-managed and carries its profile name, so what you see on a calibrated
screen is what a viewer that respects profiles will show.

At 8 bits AURA adds a very small **ordered dither** - a fixed 4x4 pattern, not random noise -
so that a smooth sky does not band. It is positional, which means the same pixel gets the
same treatment every time, including across the tile boundaries of a very large export. At 16
bits there is nothing to band and no dither is added.

## Two promises about reproducibility

**The same edit renders the same everywhere.** Same RAW, same recipe, same version of AURA,
same output settings: the same file, byte for byte, on any machine. Every export records the
four things that produced it, so a delivered file can be re-created years later.

**A large file renders exactly like a small one.** A hundred-megapixel export is processed in
pieces so it never has to fit in memory at once, and the result is bit-identical to
processing it whole. Crossing a memory threshold never changes how your photograph looks.

## What this version does not have

**No graphics card acceleration.** AURA develops on the processor. The result is identical -
the processor path is the reference every other path is measured against - but it is slower
than the product's stated targets. You will see `AURA-RENDER-8001` in Settings, then
Hardware.

**No measured camera profiles**, as above.

**No lens correction models.** Distortion and chromatic aberration correction need per-lens
profiles that arrive with a later version. Vignette correction, which is a single number
rather than a model, works now.

Every one of those is reported rather than silently skipped. If AURA could not do something
to your photograph, it says so on the photograph.
