# ADR-0094: Measured dark-circle correction

Status: accepted
Date: 2026-10-07

## Problem

After Auto advanced retouch (ADR-0093), a photographer reported that the dark circle below
the eye was "not perfect - it is showing more white there". Looking at the renders at 100 %
showed three defects working together:

1. **The correction never reached the circle.** ADR-0091's eye guard intersects every
   automatic face operation with the whole orbital socket - which by design includes the
   tear trough. The under-eye lift was one of those operations, so its zero core sat exactly
   on the dark circle and it acted only in the guard's outer transition, on the cheek top.
2. **The cheek around it was brightened.** Smoothing, light evening, face light and
   exposure all reach the cheek and stop at the socket. The circle stayed as dark as it was
   while the skin below it - and the guard's transition band - got lighter: a pale band under
   a dark circle, which is the "white" the photographer saw.
3. **The lift itself whitened.** `UnderEye` raised every pixel toward its neighbourhood's
   average, so pores and fine lines were lifted with the shadow (flattening them), the
   purple or brown cast stayed (an ashy, grey-white patch once lighter), and nothing stopped a
   pixel ending brighter than the cheek. It was also measured on a capsule a little too low to
   cover the darkest part of a circle, which sits just below the lower lashes, so on the acne
   portrait no correction was planned at all.

## Decision

### A dark-circle operator measured against the cheek (`aura-render/src/retouch_undereye.rs`)

An `UnderEye` operation that carries a `source` is now the measured correction; one without
a source keeps the original local lift, so saved hand-painted corrections render exactly as
before.

- **Reference.** The cheek on a disk around `source` (three low-band radii across), read at
  render time - so the target follows whatever exposure and evening ran before - and averaged
  over skin only and anchored at the cheek's 40th percentile: lashes, hair and marks below it
  and glints and sheen above it count for nothing. A lit cheek often carries a sheen that is
  lighter and greyer than the skin under it; on a dark-skinned portrait in the validation set
  the cheek disk was mostly sheen, and matching the shadow to it would have left an ashy,
  whitish patch.
- **Shadow.** The selection is low-passed at the operation's `radius` (a skin-only weighted
  mean, so lashes, brow hair, spectacle rims and glints are left out) and at a third of it;
  each pixel is corrected from the lighter of the two, so lit skin beside a shadow - which the
  wide band sees as partly shadow - is never lifted into a bright rim.
- **Correct.** Where the low band is more than 1.5 % darker than the cheek, every channel is
  scaled so the low band moves toward the cheek's brightness and, by `tone` and in proportion
  to how dark it is, toward the cheek's colour. The scale is smooth, so pores and fine lines
  keep their relative contrast. `amount` is the share of the shadow removed. No channel is
  scaled below 0.6x or above 2x; when one would need more, the whole correction is shortened
  so every channel fits. Clamping that one channel instead changes the hue: on the acne
  portrait a deep red-brown lid crease, whose green and blue could not rise far enough, came
  out salmon in the first version of this operator. Three hard limits: **nothing ends brighter than the cheek unless it already was**; pixels far darker
  than the skin around them (lashes) are never lifted; pixels far brighter (a glint, lit skin
  beside the shadow) are lifted less and then not at all.

### Planned where the circle is (`portrait_features::eyes`)

- The circle is measured on a crescent that follows the lower lid - from just below the lower
  lashes to a fifth of the eye distance down, dipping toward the nose along the tear trough -
  against the cheek half an eye distance straight below the eye. Both are read on this
  person's skin with the same statistics (median and lower quartile), so pores darken both
  alike; at least 60 % of the cheek must be the colour of the skin right above it, read in
  display values (a turned face's far cheek can leave the face, onto hair or the background).
  The first version compared the cheek with the whole face's skin colour and brightness in
  linear light, which refused the shadow side of every side-lit portrait - linear light
  exaggerates the warmth of a shadow - so the acne portrait's darker eye was never corrected.
  Lashes, rims and glints are left out of both.
- A correction is planned when the crescent is at least 7 % darker than the cheek at the
  default strength (the *Dark circles* control moves the threshold from 8.5 % to 4.5 %), or at
  least 3 % darker with a visible cast. The decision is per face, not per eye: once one
  circle qualifies, the other open eye is corrected too by its own measurement (when it is at
  least 3 % darker), because measured eye by eye the first version corrected exactly one eye
  on every one of five portraits - one eye finished and the other left as shot. It removes 50
  to 72 % of the measured shadow - most of it, never all: skin under an eye is thinner, and a
  trace of shadow is what keeps an eye socket from looking flat.
- The mask is the crescent plus a wider band below it into the cheek, feathered. Because the
  operator only lifts what is darker than the cheek, the generous band costs nothing on even
  skin and blends the correction into the cheek the other operations brightened.

### Kept off the eye by a lid guard, not out of the socket (`eye_guard::lid`)

A measured dark-circle correction gets its own exclusion: the eye opening, both lids and the
lower lashes (zero to just below the lashes, fully free a tenth of the eye distance below the
landmark), rotating with the face. It is applied whether or not *Protect eye area* is on, so
the correction can never reach the eye itself. Every other automatic operation keeps
ADR-0091's full socket guard.

### After every light change, and reported (`advanced_retouch`)

In Auto advanced retouch the dark circle is corrected in the eyes stage (11), after the
medium and global dodge and burn and the skin colour stage, so it is matched to the cheek as
that cheek finally looks. Eye-bag evening stays with the medium dodge and burn. The eyes
stage's report now carries what was measured at each face's eyes - how much darker each
corrected circle was than its cheek, eyes left alone because they were closed.

## Measured

Auto advanced retouch was run through the application (`crates/aura-app/tests/advanced_retouch_photos.rs`)
on 23 photographs - the acne portrait and the 22 Pexels portraits in `D:\aura-skin\photos`,
including a dark-skinned man with a strong cheek sheen, two people in glasses, a side-lit
face and three photographs with no usable face - and every corrected eye was inspected before
and after at about 100 %. Dark circles were corrected on 11 faces (14 eyes), each measured at
10 to 40 % darker than the cheek below it; the other 9 faces needed none. On every photograph all 18
stages reported, quality control passed, fine skin texture kept was 83 to 100 % and skin
colour drift at most 0.007 (limit 0.012). Four defects were found by looking and fixed before
this was accepted: the salmon crease (whole-correction shortening), the shadow side of a
side-lit face refused (cheek checked by colour in display values), exactly one eye corrected
per face (decided per face), and a sheen-dominated cheek as the target (40th-percentile
anchor).

Unit tests paint a purple shadow band with a pore pattern
and check that it is lifted toward the cheek and never past it, that its cast moves at least
70 % of the way to the cheek's, that pores keep their ratio to the skin beside them, that a
lash and cheek-coloured skin next to the shadow are untouched (no rim), that `amount` is the
share removed, that a deep red crease moves toward the cheek without its hue swinging, that a
sheen over half the cheek is not the target, and that without a cheek nothing changes. Planner tests check that a painted
circle under one eye is corrected on that eye only, measured against the cheek straight
below, and that the lid guard keeps the eye, the lids and the lashes while freeing the trough.

## Consequences

- Dark circles are corrected where they are, toward the same person's own cheek, in
  brightness and colour; the area under the eye can no longer end lighter than the cheek.
- Recipes saved before this keep their pixels until retouch is run again (the old
  operations have no `source`).
- Not attempted: puffiness (eye bags) is still only evened, never flattened; a dark circle
  hidden behind spectacle lenses is measured through the lens; deep hollows lit from above
  keep some shadow, by design.
