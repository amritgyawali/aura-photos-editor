# Tone and colour, in the product's own words

*What AURA changes about how a photograph looks, why it stops where it stops, and the one
promise it measures rather than makes.*

Phase 15 made your photographs *correct*: the faces are at the brightness that kind of
photograph wants, and the colour of the light in the room has been taken out. This is the
phase that makes them look like photographs somebody was paid for - and it is the phase with
the most to get wrong, so most of what follows is about the limits rather than the effects.

---

## The five things AURA sets

**Contrast** is how far apart the light and dark parts of the *subject* are - the faces, when
there are faces in the frame. Not the whole photograph: a dark reception hall with a well-lit
bride is not a low-contrast photograph, and treating it as one is how a well-exposed frame
comes back looking flat.

**Highlights** and **shadows** recover the two ends. Highlights come down when there is
something at the top of the range worth keeping - a dress, a cake, a window. Shadows open up
when there is something at the bottom, and they stop where the noise starts.

**Whites** and **blacks** place the two endpoints - where the brightest real tone sits and
where the darkest one does. They are the smallest adjustments AURA makes, because an endpoint
moved far has redefined what the photograph's darkest tone *is*.

## The curve

A tone curve is where the separation actually comes from. AURA draws one for each photograph,
with between four and eight control points, and it is **pivoted on the subject's own
brightness** rather than on middle grey. A curve pivoted at middle grey applied to a bride at
35 % brightness darkens her while adding the contrast it was asked for - the frame comes back
looking underexposed by the step that just exposed it correctly.

Three things bound it, and all three can stop the curve short of what the scene asked for:

1. **The shadows may not be opened further than the camera can carry.** AURA takes that number
   from what it measured about your camera at that ISO, and it will not argue with it.
2. **The shoulder may not reach white when there is something bright and near-white in the
   frame.** A dress with a fold in it is a gradient, and a curve that flattens the top of the
   range flattens the fold out of it.
3. **Nothing may reach the very top or the very bottom.** A control point at pure white turns
   everything above it into one flat tone, which is a band you can see.

**Every curve AURA draws only ever goes up.** That is not a preference, it is a property of
how the curve is built: it cannot be made to go backwards, so it can never invert the contrast
in part of a photograph or produce the banding that looks like a bug in the renderer. The
Curve panel draws AURA's curve over the straight line it departed from, because that gap is
the thing worth judging.

## Colour, by what is in the frame

AURA works out what a photograph contains from its colours - greenery, sky, wood and warm
surfaces, bright near-white areas, decor, and skin - and adjusts each one toward what that
kind of photograph usually wants.

The one everybody notices is **greenery**. Cameras record foliage too yellow-green and too
saturated, more or less always, and pulling it back toward green is the single most-noticed
colour correction at an outdoor wedding.

The one section 2.1 of the plan names is **the exit sign**. When there is one saturated colour
competing with the people in a frame, AURA reduces *that colour* rather than desaturating the
whole photograph. A frame with a fluorescent green sign in the corner does not need to be less
colourful; it needs the sign to stop shouting.

Three rules bound all of it:

- **A colour that matches the subject is harmony, not competition.** A red sari behind a bride
  in red is left alone.
- **A room somebody lit is left alone.** A purple dance floor stays purple - the same decision
  the exposure step makes about the light, made again about the grade so it cannot be undone
  by another route.
- **Bright near-white areas may lose a colour cast and can never gain one.** A dress that has
  picked up a tint is the second thing a photographer notices, after skin.

### AURA is working from colour, not from outlines

This is the caveat worth reading. AURA is not drawing around the greenery - it is working out
from the colours that a large green-yellow region low in the frame is probably foliage. It is
usually right and it is sometimes not, so:

- every adjustment is **small on purpose**;
- every band carries how sure AURA was, and a band it is not sure about **is not adjusted at
  all**;
- every photograph that had a colour adjusted says so in its reasons.

When AURA can outline things properly, these adjustments get better and the caveat goes away.

---

## The promise about skin

**Grading never moves anybody's skin colour measurably. AURA checks, on every photograph.**

After the grade is worked out, AURA looks at the skin in that photograph and measures how far
the colour adjustments moved it: at most **2 degrees** of hue and **6 %** of colour intensity.
If it moved further, AURA works the colour out again more gently. If even the gentlest version
still moves it too far, **the colour adjustments are dropped entirely** - the photograph keeps
its contrast and its curve, and the greenery stays as the camera recorded it.

That trade is deliberate: slightly flatter decor beats skin that has moved.

Two details matter and both are in the panel.

**It is measured against this photograph's own skin.** Not against a target and not against
anybody else. There is no ideal skin colour anywhere in AURA - not in the settings, not in the
catalog, and nowhere it could be added without somebody noticing.

**"Nobody in this frame" is not a perfect score.** A photograph of the rings has no skin to
protect and no measurement to report, and the panel says exactly that. The project header
carries the largest movement anywhere in the wedding, because an average hides the one frame
that matters.

[The skin fairness statement](skin-fairness.md) has the longer version, including what has and
has not been measured yet.

---

## When AURA stops itself

Four things can reduce a grade after it has been worked out, and each one says so:

| What | When |
|---|---|
| **the noise limit** | opening the shadows further would show the noise your camera recorded |
| **the clipping guard** | the grade would brighten more of the frame to pure white than this kind of photograph accepts |
| **the subtlety cap** | everything together would have looked processed |
| **the skin guard** | the colour adjustments moved somebody's skin too far |

The clipping guard is about what a photograph **gains**. A sparkler exit that arrived with a
blown highlight keeps it: that is not the grade's fault, and darkening every face in the frame
to recover a sparkler is the wrong trade.

The subtlety cap is the one that keeps the product from looking like a filter. Each kind of
photograph has its own ceiling, and the ceremonies and family portraits have the lowest ones
in the product - those are the frames most likely to be printed and least likely to survive a
fashionable grade.

---

## The three alternatives

Every graded photograph carries up to three complete alternatives - **flatter**, **punchier**
and **warmer** - and switching to one is instant, because they were all worked out at the same
time.

They are complete, not shortcuts: each one has been through the clipping guard and the skin
guard exactly like the main grade. Switching to "punchier" cannot give you a photograph nobody
checked.

Choosing one counts as **your** decision. AURA will not change it afterwards, and it undoes
the same way any setting you set by hand does: "reset to AI suggestion".

"Warmer" is a colour adjustment, not a change of light. If a room's light was the wrong
temperature, that is the exposure and white balance step, and it is a different control.

An alternative that would have looked the same as the main grade is dropped rather than
offered. Three buttons that all do the same thing is worse than none.

---

## What this is not yet

The part of this that is a *learned model* has not been trained: there is no library of
photographs paired with expert edits in this repository. So AURA is not consulting a model at
all here - every number comes from a deterministic solver that a colour scientist wrote and a
product manager approved, measured against the intent recorded for each kind of photograph.

Everything above works and is measured. What none of it is yet is a claim about how a real
wedding *looks*, because every measurement was taken on photographs AURA drew itself, with
the answer painted in. `docs/progress/PHASE-16-EXIT.md` conditions C1 and C2 say so in the
engineering register, and this is the same sentence in plainer words.

---

*See also: [Colour management](colour-management.md), [Mixed lighting](mixed-lighting.md),
[The skin fairness statement](skin-fairness.md),
[ADR-0033](adr/ADR-0033-tone-curves-hsl-and-skin-protection.md).*
