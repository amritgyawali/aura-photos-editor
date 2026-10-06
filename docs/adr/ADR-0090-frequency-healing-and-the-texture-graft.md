# ADR-0090: Frequency healing and the texture graft

Status: accepted
Date: 2026-10-06

## Problem

Deep cleanup (ADR-0085, ADR-0089) repairs a dense acne portrait with up to 220 patch
heals and one frequency-separation finish. Rendered on the synthetic acne portrait it
removes most marks and leaves three visible faults:

- **Patchwork.** Each repaired disk is flatter than the skin beside it, and the skin
  beside it keeps every glint it had. Between two hundred flat disks the untouched
  texture reads as flaking skin.
- **Marks the selection never contained.** The segmenter leaves a dark mark at the edge
  of a face, and the whole of a jaw in deep shadow, out of the face skin. A mark outside
  the selection cannot be repaired by any operation that uses it.
- **An untreated patch above each brow.** Brows were protected by a disk a fifth of the
  eye distance in radius, which also covers the forehead above the brow. On a lit
  forehead that disk is a visible edge.

The request was the workflow a retoucher follows by hand: remove the marks, dodge and
burn, separate frequencies, and put texture back so the skin is still skin.

## Decision

Two native retouch operations, one shared selection, and an order.

### Frequency healing (`Tool::FrequencyHeal`, `aura-render/src/retouch_clear.rs`)

One operation over a skin selection rather than one per spot.

1. **Find.** At three neighbourhood sizes a pixel is compared with the selected skin
   around it: how much darker, how much redder. Both are ratios. The thresholds are
   multiples of the selection's own robust spread (1.4826 x the median absolute
   deviation), 3.0 at the default sensitivity and 1.8 at the highest, with a floor.
   Glints are clipped out of the estimate of the surrounding tone, and a region's own
   glint strength is added to its darkness threshold, so the skin between glints is
   not a mark. Connected groups are kept only when they are compact: longer than 5.5
   times their width is a line, and a long thin group also bars the pieces of itself
   that a stricter threshold would break off. And a group is a mark only when it is
   darker - or redder - than the skin **on every side** of it: a ring just outside it
   is read in eight sectors, at least six must be skin, and all but one of those must
   be clearly brighter (or less red). The pocket of shadow beside an eye, the side of a
   nose and the edge of a jaw are darker on one side only.
2. **Rebuild.** The tone under a mark becomes a weighted mean of unmarked selected
   skin, taken at the smallest of five neighbourhoods that has enough of it.
3. **Keep.** Detail finer than the frequency radius stays. A mark's own tone is
   measured over the mark's cells and the skin's over the skin's, so a small mark is not
   averaged with its surroundings and left over as "detail". Under a mark, relief beyond
   1.5 robust spreads of the selection's ordinary pore contrast is limited; `texture`
   is how much of that relief is kept.

It runs twice inside one operation: with the strongest marks gone, the skin between
them is a truer reference. A pixel no group reached is not written at all.

`sensitivity` and `keep_dark_marks` are new optional fields of a retouch operation.
Both are absent from recipes that do not use them, so an old recipe reads back
byte-identical. With `keep_dark_marks`, a mark that is not also redder than its
surroundings is left alone; the automatic pass sets it unless *Remove dark marks* is on.

### Texture restore (`Tool::TextureGraft`, `aura-render/src/retouch_texture.rs`)

Runs last, over the segmented face skin - the nose included.

1. **Own detail, in place.** For every pixel the target is the fine detail (about an
   eightieth of the eye distance) the photograph had **at that pixel** before any retouch
   operation ran. Pores that healing and smoothing removed come back exactly where they
   were photographed, at the strength they had. Nothing is moved, repeated or invented.
2. **Limit.** That detail is first compressed where it goes beyond 2.0 robust spreads
   (bright) or 2.6 (dark) of this skin's own pore contrast - the ridges of an oily
   highlight, the deepest pits - with a knee at 60 % of the limit, so ordinary pores pass
   through unchanged. How firmly follows the shine control.
3. **Borrow only where a blemish was.** Where an earlier operation rebuilt the tone, the
   photograph's own detail there was the blemish's rim, and putting it back would bring the
   blemish back. Those places are found by a *localized* change of tone: the change at three
   pore radii minus the same change at twelve, so dodge and burn, tone and light evening -
   which move whole regions - do not count. There, detail is borrowed as overlapping tiles
   from clean skin in the same selection, scaled to the level this skin's smoother third has.

The change multiplies luminance. It moves no tone and no colour, and for skin with no
measurable texture it does nothing.

### One surface selection (`portrait_features/deep_blemish.rs`)

Frequency healing, the surface finish and the graft share one stored matte.

- A guarded morphological closing fills notches narrower than a third of the eye
  distance, and the selection grows up to 0.35 eye distances into cells where the
  segmenter saw *some* skin and the photograph is skin-coloured. "Skin-coloured" is
  relative to the same face: at least 5 % of its median luminance and 60 % of its
  red-over-blue share. On the test portrait shadowed marks kept 80 % of that share,
  black hair a quarter and a grey backdrop none.
- A brow is excluded where it is: cells inside the brow area that are below 62 % of
  the face's median luminance, with a margin, plus a narrow band along the brow line
  for brows that are no darker than the skin.

### Order

Frequency healing is saved with the skin step, so it is the first operation on a face
and sees the photograph as taken. Spot repairs are then planned on the pixels it
leaves. The texture restore is saved with the finishing step and runs last.

Both are **off by default** (`frequency_heal`, `texture_graft` in the automatic
settings). Every existing preset plans what it planned before. A new
**Professional retouch** preset turns both on together with deep cleanup.

## What was tried and removed

The first version of this change was shown to the person who asked for it, and they said
the result looked like a cartoon: no detail on the nose and the whole face blurred. They
were right, and three things caused it.

- **Donor texture everywhere.** The first texture step borrowed tiles of clean skin and
  scaled them to a target level over the whole face. Clean skin on this portrait is oily,
  and its ridges, copied onto every cheek and the nose, read as crawling worms at 100 %.
  It now restores each pixel's own detail and borrows only inside rebuilt blemishes.
- **Rebuilding shadows.** Frequency healing took the pocket of shadow beside the inner
  corner of an eye for a mark and rebuilt it from the lit nose bridge: a brown blotch.
  The enclosure test above is the fix.
- **Too much smoothing.** The preset asked for 80 % skin smoothing and 30 % pore
  refinement, which the adaptive pass raised further, on top of a 95 % frequency finish.
  It now asks for 35 % and no pore refinement; healing removes the blemishes, so the
  smoothing only has to even the skin.
- **A heal detector that read dodge and burn as healing.** Detecting rebuilt marks by how
  far the tone moved also caught micro dodge and burn and tone evening, so rough borrowed
  texture landed on the smooth nose bridge. Only a localized change counts now.

Earlier attempts, before that review:

- **A plain mean as the surrounding tone.** On an oily forehead the gaps between
  glints measured as dark and the whole highlight was marked. With firm relief
  limiting the result was bleached patches. Clipping glints out of the estimate and
  raising the threshold where skin glints removed them.
- **The typical level as the graft's target.** Aiming at the median, with donors up to
  1.6 times it, rebuilt the roughness of acne-scarred skin across the whole face.
- **Bounding each rebuilt tone by the mark's own measured darkness, and removing the broad
  part of the change.** Both were aimed at the shadow blotch; both left outlines around
  merged marks. Not finding the shadow in the first place was simpler and better.
- **Donor tiles that were 85 % clean.** The edge of a nostril was copied onto a cheek.
- **Closing alone for shadow skin.** A jaw the segmenter left out entirely is not a
  notch; nothing on the far side of it is selected.
- **A mark's relief tied to *Keep pore texture*.** At 85 % the dark core of every small
  mark survived its own repair.
- **A `tanh` limiter with no knee.** It compressed ordinary pores by 5 to 15 %, which
  the graft then tried to put back.
- **One tone band for marks and skin together.** A mark 2.5 frequency radii across has
  more than half its contrast in the "detail" left after a blur at that radius. The
  relief limit then left every repaired mark about 4 % darker than its surroundings: a
  faint core. Measuring the two tones separately brought that to about 1 %.

## Limits

- Marks are found on the pixels being rendered. A preview and a full-size export
  measure different pixels and can differ in which faint marks are rebuilt.
- A compact bright spot that is not also redder than the skin around it is kept. It
  may be a whitehead; it may be a piercing.
- Frequency healing rebuilds tone from clean skin *nearby*. A region with no clean
  skin within reach keeps its own tone.
- The restore puts back this photograph's own detail, so it can also put back texture a
  person would rather lose - acne-scar pitting, for instance - limited to the skin's
  ordinary range but not removed. Inside a healed blemish it borrows from clean skin, and
  it cannot recover the pores the blemish covered.
- A mark at the very edge of the skin selection has fewer than six sectors of skin around
  it and is left for the spot repairs.
- Every threshold was set on one synthetic acne portrait and on painted fixtures.
  Nobody has compared a result with a retoucher's, there is no study across skin
  tones on real people, and "compact mark" is a measurement, not a diagnosis.

## Validation

- `crates/aura-render/tests/skin_finish.rs`: marks 38 % darker than the skin around
  them rebuilt to within 2.5 % of it (measured: 1.0 to 1.3 %); a crease and every unmarked pixel bit-identical; the same repair at
  0.08x and 3x exposure on three skin tones within 2 %; a dark mark kept when asked;
  nothing changed outside the selection; a flattened patch back to between 60 % and
  130 % of its original relief with mean luminance within 1 % and channel ratios
  within 1e-4; glints reduced; smooth skin left exactly smooth.
- Frequency healing and the texture restore were then rendered on the synthetic acne
  portrait at 100 % and 200 % and looked at, and the application was driven through the
  Professional retouch preset. `docs/professional-retouch-validation.md` records it.
- `portrait_features::deep_blemish` unit tests: the selection reaches a shadowed mark
  and hinted shadow skin, refuses hair, a grey wall and background, excludes the brow
  and includes the forehead above it; both operations are off by default and share
  one selection; eyes, lips and unselected pixels keep their bytes.
- `crates/aura-render/tests/skin_finish_photos.rs` and
  `crates/aura-app/tests/auto_retouch_photos.rs` (preset `pro`) are the by-hand checks
  on real pixels. `docs/professional-retouch-validation.md` records what was run.
