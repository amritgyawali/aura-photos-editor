# ADR-0086: Per-face adaptive retouch and scene intent

Status: Accepted
Date: 2026-10-05

## Context

A batch already edits each photograph independently (ADR-0083) and every correction is
measured. Two things were still shared by every photograph:

- **The retouch settings.** The 54 fine controls scale the measured corrections, and a photo
  without saved choices got the same defaults as every other one. A retoucher does not work
  that way: the same look is turned down on a face forty pixels wide, on a noisy frame and
  in hard light, and turned up on rough skin in a close-up.
- **The tone correction's idea of a good frame.** `photo_enhance::correction` pulls the median
  toward middle grey. Run over real photographs (`crates/aura-app/tests/scene_photos.rs`) it
  darkened a food photograph on a white table by 0.25 EV while raising its whites, added
  0.75 EV to a wet autumn road, a twilight mountain and a sepia print, and treated a sepia
  print and a blue-toned snow scene as colour casts to remove.

## Decision

### Adaptive retouch (`portrait_features/expert.rs`)

`Options.adaptive` (default on, stored with the photo's options like every other choice).
When on, each face is measured before it is planned, and the chosen settings are multiplied
by factors from those readings. The chosen settings stay the style: a control at zero stays
off, a preset still means what it says, and readings in the middle of each range change
nothing. Frame-wide choices (skin detection, main subject, backdrop) are not per face.

Readings, all relative to the same face or to the frame, never to an ideal:

| Reading | How | Decides |
|---|---|---|
| Eye distance in pixels | landmarks at analysis size | Under 45 px: less smoothing, half the eye detail. Over 170 px: more pore texture kept, broader smoothing size |
| Roughness | median over skin patches of a high-pass at 1/30 eye distance | Smoothing and micro dodge & burn, 0.65x to 1.45x; rough skin keeps at least 60 % texture |
| Blotch | robust spread of redness between patches | Tone evening 0.85x to 1.3x, redness 0.9x to 1.25x |
| Light | stops between the 10th and 90th percentile patch | Over 0.7 stops, light evening and face fill fall to 0.45x at 1.8 stops |
| Shine | specular share of the skin | Shine control 0.8x to 1.5x |
| Marks | compact (not elongated) groups redder or darker than their own patch | 26 or more on a face of 100 px or larger: whole-face cleanup (ADR-0085), 60 to 120 spots |
| Lines | quieter eye corner against the smoother cheek | Over 3x: line softening reduced to 0.85x - 0.65x |
| Skin luminance | the face's own median | Under 4.5 %: less micro dodge & burn (shadow noise) |
| Frame noise | `smart_edit::noise_sigma` | Over 1.2 %: less smoothing, more texture, less detail sharpening |
| Faces in frame | detector | 3 or more: overall strength 85 %; 6 or more: 75 % |

The adaptive pass **never switches on dark-mark removal**. Removing a mole is the
photographer's decision (ADR-0085), and the whole-face cleanup it can choose keeps dark marks
and freckle fields protected.

Each face's assessment stores the readings, the strength used, only the controls that
changed (chosen and used value) and one sentence per decision. The notes appear under
*Automatic decisions by face*; the batch result lists each photo's tuned amounts.

### Scene intent (`smart_edit::respect_intent`)

For frames without a face large enough to retouch (frames with people keep
`face_exposure_cap`), the histogram correction is tempered before it is saved:

- Only 70 % of a brightening is applied as exposure, and never more than the room the 98th
  percentile leaves below white. The rest goes to Shadows (30 per stop, at most +30).
- A frame in which nothing is near clipping (98th percentile under 95 %) is never darkened.
- **High-key** (median over 72 % with a real tonal range): exposure never negative, highlights
  no lower than -8, black point anchored by at most -8.
- **Night** (median under 16 %, over 55 % of the frame dark, light sources above 80 %):
  exposure at most +0.15 EV, shadows at most +5, no black point, no clarity, white balance
  kept.
- **Toned monochrome** (colour spread under 0.08 in log-chroma among pixels above 15 %):
  no vibrance and no white-balance correction. Measured on real files: a sepia JPEG 0.057, a
  blue-toned snow scene 0.064, every colour photograph in the set 0.14 or more.
- The black point is halved when dehaze is applied in the same pass, and the stretch is
  capped at whites +20 / blacks -25.

## What was tried and removed

- **A beard reading.** Darkness of the chin and jaw against the cheek read 0.33 to 0.93 on
  women whose chin was in shadow or turned down, and 0.02 on a man with stubble before that.
  It is not in this build; *Protect beard and stubble* stays on by default as before.
- **Marks as a share of pixels.** Smile creases, spectacle rims and hair read as 6 % to 24 %
  "marks" on clear skin. Counting compact components instead separates the acne portrait (31)
  from every clear-skinned face in the set at the same size (2 to 12), with one
  close-up at 23.
- **Roughness as a mean.** Spectacle rims doubled it. The median patch does not move.

## Validation and limits

- Unit tests: rules, bounds, zero stays zero, determinism, a painted face; night, high-key,
  highlight-limited exposure and sepia frames.
- `adaptive_retouch_photos` (opt-in, real photographs): every adapted plan validates and is
  deterministic, a pass with the option off does not adapt, and a set of different portraits
  does not come out with one shared set of settings.
- `scripts/test-adaptive-batch.py` runs the same batch sequence in the desktop application,
  including a repeat pass that must save nothing and a real mouse click on the control.

The thresholds were set on twenty-three photographs of people and eleven scenes. That is enough to show
the mechanism and far too few to claim taste: nobody has compared these results with a
retoucher's, and no per-skin-tone study exists. The marks count is not a diagnosis and
cannot tell acne from freckles; the lines reading falls back to "normal" whenever a temple is
covered by hair. Scene intent reads a histogram and a colour spread, so a deliberately dark
daytime frame with no bright tones is still brightened (by at most 70 % of what the
histogram asked). Every result remains a proposal: each step can be undone and each
operation adjusted.
