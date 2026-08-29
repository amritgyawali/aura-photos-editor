# What AURA does to a photograph's framing

This is the plain-language version of phase 23. It is written for a photographer rather than for
an engineer, and everything in it is true of the build you have rather than of a plan.

## The short version

AURA corrects what the lens did, levels horizons that are off by a little, and **usually leaves
your framing exactly as you shot it**. When it does propose a tighter crop, it will not cut a
face, and the original framing is always one click away.

Most of this page is about what AURA will not do. That is the point of the feature.

## Cropping

AURA proposes a tighter crop only when it clearly improves the photograph, and "clearly" is a
number rather than an opinion: the new framing has to score meaningfully better than the framing
you chose. If it is merely different, or slightly better, your framing wins.

In a normal wedding **most photographs keep the framing you shot**. If yours does not, something
is wrong and the Framing panel will show you what.

Before any crop is even scored, it has to pass a check that has no override:

- every face AURA found is fully inside it;
- the couple's hands and joined hands are inside it;
- it keeps at least 60 % of the photograph's long edge;
- the thing that makes the moment is still in frame.

A rectangle that fails is **not scored at all**. It is not given a penalty and weighed against a
nicer composition - it is out. That distinction matters, because any penalty large enough to be
safe on four hundred frames is small enough to lose on one of them, and the one it loses on is a
couple portrait.

When a crop is refused you still see it. AURA records the square crop it could not make and the
reason, so "why is there no square version of this one" has an answer.

### What you can do that AURA cannot

You can crop any photograph of yours as tightly as you like, including through a face. It is your
photograph and you are looking at it. AURA stores it as yours and never re-crops it - not when a
new lens profile arrives, not when the software is updated.

What nobody can do - not you, not your studio, not a setting - is tell AURA that cutting faces is
acceptable **in general**. That is the setting that quietly crops the next four hundred frames
through people. Your studio can make AURA more careful than it is by default. Nothing can make it
less.

## Levelling

AURA levels a horizon when three things are true: it can see one, it is confident about it, and
the correction is between a fifth of a degree and eight degrees.

**Under a fifth of a degree the photograph is already level.** Rotating it would re-draw every
pixel to move a horizon by a fraction of one, which costs sharpness and gains nothing.

**Over eight degrees it was a decision.** A hard tilt is a choice, and AURA leaves it exactly
alone rather than "correcting" it to eight. This is the one thing photographers most often ask
about, so it is worth being precise: there is no angle at which AURA partly straightens a Dutch
tilt.

**Levelling costs pixels.** Rotating a photograph means keeping the largest upright rectangle
inside the rotated one, which is smaller than the frame. If that rectangle would cut somebody or
fall below the resolution floor, AURA levels *less* than it wanted to, or not at all - and the
panel tells you both numbers, the angle it wanted and the angle it used. A photograph that is
1.1° off level when AURA wanted 3.4° is a photograph where straightening the rest would have cost
somebody's shoulder.

Perspective correction - converging verticals in an architectural frame - works the same way, and
is refused entirely when it would stretch the frame by more than 12 %.

## Lenses

Where AURA has a profile for your lens it corrects distortion, vignetting and colour fringing
automatically, in that order and before anything creative happens to the photograph.

**No lens profile in this build has been measured on a real lens.** Every one is a reference model
for a lens class - a plausible correction for a 35 mm prime rather than a measurement of yours.
The Framing panel says so on any photograph that was corrected through one, and every such
photograph is findable later, so when measured profiles arrive nothing is left silently corrected
by an approximation.

If your file carries the manufacturer's own correction data, that is used first. If AURA has
neither, it can estimate distortion from the straight edges in the photograph itself, and it does
not attempt to guess a vignette or a fringe from one frame - a single photograph does not carry
enough evidence for those.

The Framing panel lists the lenses AURA had nothing for. That list is the one thing on the panel
your studio can act on.

## Aspect variants

Alongside the framing that will be delivered, AURA works out 4:5, 5:4, square and 16:9 versions
where they are safe - for an album spread or a social post. **They are alternatives, not
deliveries.** Your gallery keeps the framing you shot; the variants sit beside it for the album
stage to use, and no file is duplicated to hold one.

A variant that could not be made without cutting somebody is stored as a refusal rather than
quietly missing.

## Getting your framing back

Every photograph has one button: **back to the original framing**. It clears the crop, the
rotation and the perspective correction together, and it also tells AURA that it may look at this
photograph again - which is different from cropping it back to full frame by hand, because a
hand-set crop is yours and AURA will not revisit it.

## What this build cannot do yet

Three gaps, stated plainly because they change what the promises above are worth.

**AURA's face detection does not find faces in this build.** The safety check runs, and on a real
photograph it currently has nothing to protect. That means "no crop cut a face" is arithmetic
rather than evidence, and the panel says so in words instead of showing you a reassuring zero. It
also means almost nothing is auto-cropped: with no subject identified, AURA does not search for a
better rectangle at all.

**Hands are never protected.** The keypoints that would find them are not trained yet. The
mitigation is that automatic cropping is switched off entirely in the ten kinds of photograph
where hands matter most - ring exchanges, garland ceremonies, hand details - rather than left on
and hoping.

**Nobody has compared AURA's crops against a photographer's.** The safety filter and the
improvement margin are measured; whether you would prefer AURA's rectangle to yours is not. Until
that study exists, treat every crop proposal as a suggestion from something that has never been
told whether it was right.

## Where the numbers live

- `crates/aura-geometry/config/crop_rules.toml` - per-kind-of-photograph cropping rules, and the
  ten that switch cropping off. Your studio can make these stricter.
- `assets/lens_profiles/profiles.toml` - the lens database, and `ATTRIBUTION.md` beside it.
- `docs/adr/ADR-0047-geometry-lens-straightening-and-crop-safety.md` - why each rule is what it is.
- `docs/progress/PHASE-23-EXIT.md` - what was measured, and what was not.
