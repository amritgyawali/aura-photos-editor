# Local light, and what AURA does inside a photograph

*What the marks on your photographs mean, in the product's own words.*

Setting the exposure and the colour of a whole photograph gets you most of the way. It does
not fix a face in shadow under a mandap, a window three stops brighter than the couple in
front of it, or a hot spot on somebody's forehead under a spotlight. Those are the problems
you would fix by hand, with a brush or a radial mask, one frame at a time.

This is the part of AURA that does that. It is also the part designed to be **invisible**: if
it is working, the photograph looks better and you cannot say what changed. That is a good
thing in a gallery and an awkward thing in a panel, so this page and the Local panel both go
out of their way to tell you exactly what was done.

## Six things it can do, and it never does all six at once

| What it does | When it fires |
|---|---|
| **Face lighting** | A face is darker or brighter than photographs of this kind should be |
| **Subject presence** | The subject needs a little more separation from what is behind them |
| **Background** | Something behind the subject is competing for the eye |
| **Shine** | There is specular sheen on a forehead or a nose |
| **Shaping** | A face is large enough in frame to deepen and lift the way a retoucher would |
| **Evening out** | Skin has blotchy patches of tone that are not texture |

Each of those has its own strength slider in the Local panel, and each is scaled by what kind
of photograph it is. A family formal gets the full treatment. A dance floor gets a gentle lift
and nothing else, because a dance floor is meant to be dark, moving and coloured, and shaping
a motion-blurred face produces a smear with structure in it.

## The one rule that keeps it invisible: a budget per photograph

Every one of those six adjustments is individually defensible. Six of them at once is a
photograph that looks processed.

So AURA gives each photograph a single allowance - a total amount it is permitted to change,
measured the way an eye measures it - and every operation spends against it. When it runs out,
things get given up in order, and the order is deliberate: **face lighting is the last thing
given up and shaping is the first.** Lighting a face is the thing you asked for; deepening a
jawline is the thing you would not notice was missing.

You will see *"AURA had already changed as much as it allows itself in one photograph, so it
stopped"* on frames where that happened.

## Why a lifted face does not glow

If you lift a face with an exposure slider, everything in it gets brighter by the same amount:
the shadow side of the nose and the lit side of the forehead move together, the contrast stays
exactly where it was, and the result is a face with more light on it rather than better light
on it. That is what "glowing" means, and it is the most common way an automatic local edit
gives itself away.

AURA lifts the **shadow** side and leaves the lit side where it is. The dark parts of a face
move most, the mid-tones move less, and the highlights barely move at all. That reduces the
contrast across the face, which is what reads as better lighting.

## Why it sometimes stops short

Two things cap a face lift, and the panel tells you which one bound:

- **Grain.** Lifting shadows lifts the noise in them. AURA works out how much this particular
  frame, at this ISO, on this camera, can take - and it stops there. You will see *"lifting
  this face further would have brought out grain"*, and the panel shows what it *would* have
  lifted so you can decide for yourself.
- **The bright side.** If lifting further would flatten the bright side of the face, it stops.
  You will see *"lifting this face further would have blown out the bright side of it"*.

Both of those are the judgement you would make by hand. The difference is that AURA makes it
on four thousand frames and writes down which one it was.

## Group photographs, and the promise AURA can and cannot make

In a family formal, everybody should end up lit the same. AURA solves every face in a frame
**together** rather than one at a time, aiming them all at one brightness, so nobody comes out
looking pasted in.

Sometimes it cannot get all the way there, and it is worth knowing why. If one person is
standing two stops down under a doorway, the grain cap will not let their face be lifted far
enough to join everybody else. AURA has two ways to satisfy a promise of "everybody is lit the
same" in that situation, and both are worse than the problem: refuse to touch the photograph
at all, or **darken everybody else to match the person it could not lift**.

It does neither. It lifts that person as far as the frame allows, leaves everybody else where
they should be, and tells you: *"AURA could not even this group out completely; they are 27 %
apart. Nobody was darkened to close the gap."* The group is more even than it was, which is
the honest version of the promise.

## Backgrounds are never brought down on their own

When AURA calms a bright window behind a couple, it lifts the subject by a matching amount at
the same time, so the photograph is no brighter or darker overall than it was. The eye reads
the *relationship* between the subject and the background, not the absolute values - so
changing the relationship without changing the overall brightness is what makes the subject
come forward without the frame looking edited.

It also never fires on a hunch. A background is only calmed when something measurable crosses
a line: it is brighter than the subject by a margin, or it is more colourful than the subject
by a margin, or AURA found a bright patch behind them in its framing analysis. On a photograph
where nothing behind the subject is competing, you will see *"nothing behind the subject was
pulling the eye, so the background was left alone"* - and that is the normal answer on most
frames.

A background that is the wrong *colour* rather than the wrong *brightness* is desaturated
rather than darkened. A red sari behind a couple should stay red and stop shouting; darkening
it makes it brown.

## Shine is a brightness change, not a blur

Sheen on a forehead is the light's own colour reflecting off skin, which is why it is brighter
and *less colourful* than the skin around it. That is how AURA finds it: bright, near-neutral,
small, and near the top of that face's own brightness range.

The reduction is a **brightness** change and nothing else. The texture underneath is untouched
- there is no smoothing anywhere in this part of the product, and no setting that could add
one. Skin texture is a different job and it belongs to a different part of the product.

If a bright area on a face is too large to be sheen, AURA leaves it alone and says *"the bright
area on this face is the lighting rather than shine"*. A face turned toward a window is not a
hot spot.

## Shaping, and what it is not allowed to touch

On a face large enough in frame, AURA does what a retoucher does: lifts under the eyes,
brightens the cheekbone a little, deepens the hollow under it and the jawline a little. The
moves are small - about a sixth of a stop at the very most - and they follow the light that is
already on the face rather than inventing a new direction. A flatly lit face gets less, because
deepening a shadow that is not there is painting one on.

Two things it can never do:

- **Touch skin texture.** The face is split into three bands before anything happens, and the
  finest band - pores, fine lines, the things that make skin look like skin - is not produced
  at all. There is no operation in this part of the product that could reach it.
- **Put a shadow where a lift belongs.** Under the eyes, the cheekbone and the bridge of the
  nose can only ever be *lifted*. That is a property of how the moves are defined rather than a
  rule the code follows, so no adjustment can reverse it.

Evening out is the honest alternative to skin softening: it flattens blotchy patches of tone
without reducing texture, and it is bounded so that the measured texture in a face cannot move
by more than five per cent whatever the slider says.

## Where AURA does nothing, and why

Most of the time the answer is that the photograph did not need anything, and the panel says
which of those it was:

- *"the faces here were already lit the way this kind of photograph should be"*
- *"nothing behind the subject was pulling the eye"*
- *"there was no shine to reduce here"*
- *"photographs of this kind are left largely as shot"*

There is one more, and on this release it is the common one:

> *"AURA could not work out where the subject ends and the background begins here, so it did
> not make any local adjustments."*

Local adjustments need to know exactly where the subject is, down to the strand of hair against
a bright window. That is a separate piece of the product and it is not in this release yet.
Until it is, AURA declines to make local adjustments rather than guessing at the edges - because
an adjustment applied through an approximate outline leaves a visible bright rim beside the
person, and a bright rim is far worse than no adjustment at all.

The Local panel shows exactly which adjustments were left out and why, and the project header
shows how much of the wedding it affected.

## Everything is reversible, and nothing has moved a file

Every adjustment on this page is stored as a mask and a set of numbers in the edit recipe.
Nothing is baked into a photograph, no original is touched, and turning a slider to zero
removes it completely. If you set a strength by hand, AURA records that it is yours and will
not change it back on a later pass.

## The full list of notes

Every note AURA can leave, grouped by what it is about. Fourteen of the thirty describe
something AURA *declined* to do, which is the point: an editor that shapes every photograph it
can is an editor that shapes photographs it should have left alone.

### About lighting a face

- **the light on this face was lifted to match the rest of this part of the day**
- **the faces here were already lit the way this kind of photograph should be, so nothing was
  changed** *(nothing was done)*
- **lifting this face further would have brought out grain, so AURA stopped where it did**
  *(stopped short)*
- **lifting this face further would have blown out the bright side of it, so AURA stopped where
  it did** *(stopped short)*
- **AURA had already changed enough in this photograph, so it did not lift the face the whole
  way**
- **everybody in this photograph was lit together, so nobody looks pasted in**
- **one person would have ended up noticeably brighter than everybody else, so AURA held them
  back**
- **this face is small in the frame, so AURA adjusted its brightness but did not shape it**
  *(nothing was shaped)*

### About the subject and the background

- **the subject was given a little more presence than what is behind them**
- **the subject was lifted and the background brought down by the same amount, so the
  photograph is no brighter overall**
- **nothing behind the subject was pulling the eye, so the background was left alone**
  *(nothing was done)*
- **the background here was brighter than the subject, so it was brought down**
- **a strong colour behind the subject was competing with them, so it was calmed**
- **a bright patch behind the subject was brought down**
- **AURA kept the overall brightness of the photograph where it was**

### About shaping

- **the shape of the face was deepened very slightly, the way a retoucher would**
- **uneven patches of tone on the skin were evened out without touching the texture**
- **AURA held back the shaping here to keep the skin texture exactly as it was** *(held back)*
- **AURA could not find the features of this face reliably enough to shape it, so it only
  adjusted the brightness** *(nothing was shaped)*
- **photographs of this kind are left largely as shot, so AURA did very little here** *(nothing
  was shaped)*

### About shine

- **shine on the skin was brought down without softening it**
- **there was no shine to reduce here** *(nothing was done)*
- **the bright area on this face is the lighting rather than shine, so AURA left it alone**
  *(nothing was done)*

### About what stopped AURA

- **AURA could not work out where the subject ends and the background begins here, so it did
  not make any local adjustments** *(nothing was done)*
- **AURA is not certain where the edges of the subject are here, so it made a gentler
  adjustment than usual**
- **AURA had already changed as much as it allows itself in one photograph, so it stopped**
- **photographs of this kind get a lighter touch, and this one was adjusted accordingly**
- **AURA is using its built-in guidance for how faces should be lit rather than anything
  learned from edits** *(built-in guidance)*
- **local light adjustments are switched off for this wedding** *(nothing was done)*
- **you set these strengths by hand, and AURA has not changed them** *(yours)*

## How to tune it

Every strength in the Local panel is a slider from nothing to full, per photograph. Setting one
marks that photograph as yours: AURA records what it would have done, keeps your number, and
will not change it on any later pass. That disagreement is also what the product learns from
over time.

For a whole wedding, the per-scene strengths live in a settings file that a photographer or
their editor can change - `local_light.toml` - with a written reason on every row explaining
why each kind of photograph gets the treatment it does. Changing it re-checks the wedding in
the background and keeps everything you set by hand.
