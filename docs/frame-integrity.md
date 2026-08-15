# What the technical marks mean

AURA checks every photograph for five things: whether the right subject is sharp, whether
the blur was a decision, whether the exposure can be brought back, how noisy it is, and
whether the eyes that matter are open.

This page is the reference for every mark it can leave, in the words the product uses.

**Before anything else, one thing that is true of every mark on this page: none of them
throws a photograph away.** AURA does not delete, hide or reject anything here. A mark is
a note about a photograph, the crop that caused it is shown beside it, and you can tell
AURA it is wrong. Choosing what a client sees happens later, and it is a different part of
the product.

---

## The five things AURA measures

### Is the right subject sharp

Not "is this photograph sharp" - a soft background is usually the point. AURA measures the
eyes first, then the face, then the body, then the background bands, and weights them by
how much the photograph is *about* each person.

It also normalises for your camera. A 61-megapixel body and a 24-megapixel body produce
different amounts of edge detail in the preview AURA reads, and without correcting for it
the more expensive camera would win every comparison. AURA has measurements for twenty
bodies; if yours is not among them it says so and judges more cautiously.

### Was the blur a decision

A dance-floor drag, a panned exit and a missed shutter speed all produce a smeared
subject. AURA looks at the *direction* of the blur to tell them apart: motion blur smears
along one axis and a focus miss smears in every direction equally. Then it looks at where
the smear is - the whole frame moving is the camera, a smeared subject against a sharp
background is the subject, and a smeared background behind a held subject is a pan.

**AURA never marks intentional motion as a fault.**

### Can the exposure be brought back

Not how dark or bright the photograph is - what fixing it would cost. Two stops under on a
2023 body is fine; the same photograph on a 2016 body is not, because lifting it shows
noise the newer sensor does not have.

Clipped highlights that are *lights* - a candle, a sparkler, a bulb in shot - are not
counted against a photograph. A clipped wedding dress is.

### How noisy it is, for this kind of photograph

Noise is measured in the flat parts of the frame, so a lace veil is not mistaken for
grain, and it is judged against what the scene allows. A dance floor at ISO 12800 is a
dance floor. A family portrait at ISO 12800 is a mistake. The same measured noise produces
two different verdicts.

### Are the important eyes open

Only the people the photograph is *about*. A guest blinking in row four is not a fault; a
partner blinking in a portrait is.

And closed eyes are often the photograph. A kiss, a prayer, a first look, somebody crying
at a toast - AURA knows the moments where closed eyes belong, and marks them as *right*
rather than wrong.

---

## Every mark, and what it means

Marks in the first table are faults. Marks in the second table are not - they are AURA
telling you it understood what you did, or that it could not judge something.

### Faults

| Mark | What it means | What usually causes it |
|---|---|---|
| **Soft** | The subject is softer than your camera should manage in this kind of photograph. | A focus miss, a shutter speed too slow for the lens, or a subject that moved between focus and shutter. |
| **Focus behind** | What is behind the subject is sharper than the subject is. | Focus landed on the background - a common autofocus failure with a busy scene behind somebody. |
| **Focus short** | Something in front of the subject is sharper than the subject is. | Focus landed on a foreground object - flowers, a hand, a chair back. |
| **Shaken** | The whole frame is smeared in one direction. | The camera moved. If AURA can read the shutter speed it says how many stops below hand-holdable it was. |
| **Subject moved** | The subject is smeared and the background is not. | They moved during the exposure. Sometimes this is the photograph; if the scene is one where motion is expected, AURA marks it as deliberate instead. |
| **Blown** | Highlights are clipped past what your camera can bring back. | Overexposure, or a bright window behind the subject. Lights in shot are excluded. |
| **Crushed** | Shadows are below your camera's noise floor at this ISO. | Underexposure at high ISO. Lifting them would show more noise than detail. |
| **Noisy** | More noise than this kind of photograph carries well. | High ISO in a scene where you would not usually accept it. |
| **Blinked** | Somebody the photograph is about has their eyes closed, and nothing about the moment explains it. | A blink. See the next table for the times AURA decides it was not one. |
| **Squinting** | Somebody important is squinting. | Sun, or a flash about to fire. |
| **Mixed light** | Two different colours of light on the same scene. | A tungsten room with daylight through a window, or LED uplighters with flash. One white balance will not correct all of it. |
| **Several blinking** | More than a third of the people in a group have their eyes closed at once. | A group shot that may need a different frame entirely rather than one face fixed. |

### Not faults

| Mark | What it means |
|---|---|
| **Deliberate blur** | The blur runs in one direction and the subject is held. AURA read this as a pan or a drag rather than a mistake. |
| **Eyes closed on purpose** | Closed eyes that belong to the moment - a kiss, the vows, a ritual, a first look, a speech, a first dance - or both partners closing their eyes at once, or a wide smile with a tilted head, which is a laugh rather than a blink. |
| **Deliberate shallow focus** | The background is soft and the subject is sharp. AURA read this as an aperture choice, not a missed focus. |
| **Highlights recoverable** | Some highlights are clipped and your camera can bring them back. |
| **Lights in shot** | The brightest parts are light sources rather than blown detail, so they are not counted against the photograph. |
| **Noise as expected** | The photograph is noisy, and no noisier than this kind of photograph at this ISO usually is. |
| **No subject found** | AURA found no face or person, so the sharpness reading is about the whole frame rather than about somebody. Common and correct on detail shots. |
| **Camera not measured** | AURA has not measured your camera model yet, so every reading on those photographs is a cautious one. |
| **Nothing to report** | Focus, motion, exposure, noise and eyes all read normally. |

---

## The score

Each photograph gets one number between 0 and 1. It is a combination of the five
measurements, weighted by what the scene is about - a portrait is mostly about whether the
person is sharp, a detail shot mostly is not.

It is **multiplied rather than averaged**, on purpose. A photograph that is perfectly
exposed, clean, well framed and completely out of focus on the bride is not a good
photograph, and an average would say it was three-quarters of one.

**0.8 means the same thing in a ceremony as on a dance floor.** Every measurement is taken
against what that kind of photograph allows before it reaches the score, so the number is
comparable across a whole wedding.

---

## When AURA is unsure

Every verdict carries a confidence, and AURA lowers it and says why:

* it found no face or person to judge against;
* it has not measured your camera model;
* the photograph has no scene label yet, so it was judged neutrally;
* there was no flat area large enough to measure noise in.

A photograph AURA has **not checked yet** shows "not checked" rather than a clean verdict.
Those are different things, and the panel never blurs them.

---

## Telling AURA a mark is wrong

Every fault has a "this is wrong" button. Pressing it:

* removes the mark;
* remembers that you disagreed, so a later re-check does not put it back;
* changes nothing about whether the photograph is delivered.

The marks that are not faults have no button. There is nothing to forgive.

---

## What this build cannot do yet

Said plainly, because the alternative is a photographer finding out by being surprised.

* **The two learned parts - the focus judgement and the eye-state reading - are not
  trained yet.** The measurements around them are real; the models are placeholders. Every
  accuracy figure AURA's tests report is measured against images whose answer was known in
  advance, which proves the arithmetic and says nothing about photographs.
* **Twenty camera bodies have measurements, and they are derived from published
  specifications rather than from measuring the cameras.** Yours may be judged slightly
  strictly or slightly loosely.
* **Clipping is measured on the preview rather than on the RAW file's own histogram**, so
  the estimate of what lies above the clip point is approximate.
* **The "there are tears here" rule for closed eyes is not implemented yet.** It needs a
  later part of the product. Until then, a tearful photograph with closed eyes may be
  marked as a blink - which will show up in your review queue rather than anywhere worse.

## Related

* `docs/runbooks/AURA-ML-5037.md` - what happens when your camera has not been measured
* `docs/runbooks/AURA-ML-5035.md` - what happens when one photograph cannot be checked
* `docs/moments-bursts-and-duplicates.md` - how AURA groups the frames of one moment
