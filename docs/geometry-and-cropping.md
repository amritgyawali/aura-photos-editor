# Finishing the frame

*What AURA does to a photograph's lens, its horizon and its edges - and, far more often, what
it decides not to do.*

Most of this page is about restraint. Seven photographs in ten leave this part of AURA exactly
as you shot them, and that is the design rather than a shortfall: cropping is the one thing an
editor does that cannot be undone by looking at it harder, and the frame you chose in the room
is usually the right one.

---

## Lens corrections

Every lens bends light slightly wrong. Wide zooms bow straight lines outward at their short end.
Fast primes darken the corners wide open. High-contrast edges pick up a coloured rim, because
red and blue do not land in quite the same place as green.

AURA fixes what it can measure, in this order.

**Your camera's own data, when it wrote any.** Manufacturers measure their own lenses, and if
that measurement is in the file it is the best one available.

**AURA's bundled profile table**, matched on the lens name and the focal length. A zoom is
interpolated between the focal lengths it was measured at - in a way that follows the field of
view rather than the millimetres, because 24 mm to 34 mm is a much bigger change in what you can
see than 60 mm to 70 mm.

**An estimate from the photograph itself**, when there is no profile. AURA finds the long
straight edges in the frame - a doorway, a wall, a window - and works out how much they are
bowed. It is a real correction and it is not as good as a measured one:

* it gets the direction right and the amount to within about a third;
* it **always under-corrects**, deliberately. Leaving a slight bow is something nobody notices.
  Over-correcting turns barrel into pincushion, which reads as a mistake because it is one;
* it corrects **distortion only**. Fringing is left alone, because a fringing correction worked
  out from the same edges it is meant to clean will happily invent a rim of the opposite colour,
  and a purple edge you did not have before is worse than a green one you did. Corner darkening
  is left alone too - AURA cannot tell optical falloff from a dark wall.

**And nothing at all**, when there is no profile and not enough straight lines to work from - a
dance floor, most of a reception. AURA says so rather than guessing.

> The Geometry panel names the profile it used, and says whether anybody measured it.

**The lens profiles that ship with this build were not measured.** They have the right shape and
the right size for the lenses they are named after, and they are estimates. That is written on
the panel, on every photograph they touch.

---

## Levelling

A tilted horizon is one of the most common complaints about a delivered gallery, and it is one
of the easiest things to fix at scale. AURA levels a frame when three things are all true.

**It is confident about where level is.** Below 70 % confidence nothing turns. A frame with no
horizon, no strong verticals and no gravity tag from the camera has nothing to level against,
and AURA says "there was no reliable horizon to measure" rather than turning it by a guess.

**The tilt is between a fifth of a degree and eight degrees.** Under that, turning the
photograph costs a re-sample and buys a change nobody can see. Over it, the tilt reads as a
decision - and AURA leaves deliberate tilts alone anywhere in the band too, when the way the
frame is built says the tilt was a choice.

**Turning it does not crop into anybody.** This is the one worth knowing about. Levelling a
photograph means cropping to the rectangle that still fits inside the turned frame, and on a
family formal with somebody at each end of the row, that rectangle can cut a person out. When
that happens AURA **turns the frame less far** - as far as it can without losing anybody - and
tells you it did. If no angle works at all, it leaves the photograph exactly as you shot it.

The rectangle it crops to keeps your frame's own shape. A 3:2 photograph stays 3:2 after
levelling.

---

## Squaring up architecture

Point a camera up at a church and its walls lean together. AURA can straighten that, from at
least three strong vertical lines in the frame.

It is **capped**, and past the cap it is refused rather than reduced. Squaring up a severe
convergence means stretching one end of the frame by more than a quarter, and past that a
squared-up doorway stops looking squared up and starts looking like a photograph taken through
a letterbox. A half-correction is the worst of both - the walls still lean, and the photograph
has been stretched and cropped to achieve it - so AURA does not do half.

Three lines, not two. Two lines always meet somewhere, and calling that a vanishing point is how
an automatic tool squares up a frame containing one door frame and a guest.

---

## Cropping

**Most photographs are not cropped.** A tighter frame has to be *clearly* better - not slightly
better - before AURA takes it, and fourteen of the twenty-three kinds of wedding photograph AURA
recognises are never cropped at all:

| Never cropped | Why |
|---|---|
| The kiss | The photograph the wedding is bought for, and the cost of a mistake is the highest in the day. |
| Family and group formals | Somebody at each end of the row is exactly as important as the couple and much easier to clip. |
| The ceremony, the entrance, the exit | The room, the guests and the light are what make it a ceremony. |
| Any rite | A rite has participants, implements and an order AURA does not understand, and the thing at the edge may be the thing that made it that rite. |
| The first look, the first dance | The frame *is* the distance between two people, or the room watching them. |
| The dance floor | There is a limb at every edge. |
| Anything AURA could not name | A photograph AURA cannot identify is a photograph AURA has no business re-framing. |

Where cropping is allowed - getting ready, details, speeches, cake, portraits, candids, the
rings, the venue - AURA looks for a tighter frame that improves the composition, and takes it
only when the improvement is clear.

### What a crop can never do

These are not preferences that a setting can relax. They are checked **before** a crop is even
considered as a candidate, which is why they cannot be traded off against a better-looking
frame:

* **Cut a face.** Any face, anybody's, whether or not AURA knows who they are.
* **Cut the couple's hands.** Joined hands, a ring being put on, hands on a cake.
* **Throw away more than about 40 % of the frame's long edge**, and more than that on the
  photographs likely to be printed large.
* **Remove what the photograph is about.**

When a crop fails one of these, AURA does not adjust it until it passes - it discards it and
tries a different one. The panel tells you how many it discarded and for which reason.

### What AURA has not checked

**Faces.** AURA can only protect faces it found. On a photograph where it found none, no crop
has been checked against a face - and the panel says exactly that rather than showing a tick.
There is a real difference between "nothing was cut" and "nothing was checked".

**Hands.** This build cannot see hands at all. Every crop in it has been checked against faces
and against what the photograph is about, and none has been checked against a pair of hands.

---

## Extra crops for albums and social

Where it makes sense, AURA prepares extra framings alongside the delivered one - a portrait crop
for an album page, a square for a feed, a wide one for a gallery header.

These are **not extra files**. They are rectangles stored with the edit, so a wedding with four
framings per photograph is a wedding, not five copies of one.

They do not have to be better than your original framing - they are prepared because you asked
for that shape, and a square crop of a wide reception photograph is essentially never "better"
than the photograph it came from. They **do** have to pass every safety rule, which is why some
photographs produce none.

---

## Getting your framing back

The frame as you shot it is the first entry in the list, on every photograph, always. It is not
stored as a saved copy that could be lost - it is worked out from your photograph and the angle,
so there is no state of AURA in which it is missing.

Choosing it, or dragging your own crop, records that **you** decided. AURA will not change it
afterwards, and a later re-check will not quietly undo it.

---

## Where the numbers live

| Thing | File |
|---|---|
| What each kind of photograph may have removed | `crates/aura-geometry/config/crop_rules.toml` |
| The bundled lens profiles, and who measured them | `assets/lens_profiles/` |
| The decisions behind all of it | `docs/adr/ADR-0041-geometry-lens-straightening-and-crop-safety.md` |
| Why a photograph was left alone | The Geometry panel, in AURA's own words |

Every row in the crop rules file carries a written reason. A threshold nobody can explain is a
decision nobody made.
