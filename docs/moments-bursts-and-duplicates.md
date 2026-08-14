# Moments, bursts and duplicates

Three words that sound similar, mean different things, and change what AURA does with
your photographs. This page is the difference between them.

---

## The short version

| Word | What it is | What it changes |
|---|---|---|
| **Moment** | One thing that happened, however many times you pressed the shutter | Everything downstream works on moments, not files |
| **Burst** | One press-and-hold of the shutter, inside a moment | How many of those frames are really alternatives |
| **Duplicate** | Frames that are the *same photograph*, not alternatives | Only one of them will ever reach a gallery |

---

## Moments

A moment is one thing that happened: the bouquet leaving her hand, the third family
group, the cake being cut.

AURA groups your files into moments as soon as they have been analysed. A wedding of
3,000 files usually becomes somewhere between 700 and 1,100 moments - about three or
four frames each. In the grid you see them as **stacked cells** with a count badge; open
one and you see every frame inside it.

**Why it matters.** Everything AURA does after this works on moments rather than on
files. When it promises that your gallery contains the first dance, it is promising a
moment. That is the same promise you would make to a client, and it is why rejecting a
whole moment is a serious thing and rejecting one frame of six is not.

### How AURA decides

Four kinds of evidence, weighed together:

* **How close in time** the two frames are - measured against how fast *you* were
  shooting at that point in the day, not against a fixed number. A 10 fps burst and a
  ceremony shot in ones are both handled correctly because of this.
* **How alike they look**, from the perceptual analysis.
* **Whether the same people are in both.**
* **Whether they came off the same camera.**

Plus two hard rules: frames more than twenty seconds apart are never the same moment,
however alike they look, and a moment is never bigger than the size cap for its scene.

Every stack can tell you why. Open it and the reasons are there - "14 frames over 1.4 s",
"the camera recorded one continuous release", "two photographers overlapping 85 % of the
time".

### Two photographers

If two of you shot the same instant, that is **one moment with two sub-groups**. AURA
merges them when their timings overlap substantially and the frames really are of the
same thing. The two cameras stay separable inside, so if it gets it wrong you split it
back exactly along the line it joined.

---

## Bursts

A burst is one press-and-hold of the shutter, inside a moment.

Fourteen frames of the bouquet toss are one moment. If they came off in one continuous
release, they are also one burst. If you tried three times - a short burst, a pause,
another burst - that is one moment with **three** bursts.

**Why it matters.** Three bursts of five means you tried three times, and the three
attempts are genuinely different. One burst of fifteen means you held the button, and
the fifteen frames differ by a blink. Those want different culls, and AURA needs to be
able to tell them apart to give you sensible alternatives.

Inside an opened stack, each burst is a row.

---

## Duplicates

This is the one that matters most, and the one to be clearest about.

**Nothing is ever deleted.** AURA marks; it does not remove. Every frame stays on your
disk, in your catalog, visible in the grid and exportable by hand. Always.

Three kinds:

### The same file, twice

You copied a card twice. AURA noticed at import - the files are byte for byte identical -
and reports it here so you see one explanation in one place.

### The same photograph

Not the same file, but nothing a client could choose between: a bracketed exposure, a
frame and its twin a tenth of a second later with nothing moved. Only one of these
should reach a gallery.

AURA is deliberately hard to convince of this. All three of these must agree:

* the difference hash is within four bits of sixty-four;
* the perceptual distance is within 0.03;
* the people are in the same places.

Three independent tests that must all agree, because getting this wrong in the direction
of "yes" hides a photograph you never knew you had.

### Alternatives

The same moment with a real difference - an expression, an open eye, a reframing. **All
of them stay eligible**, and AURA chooses between them later when you cull. These are
not marked as duplicates because there is nothing to warn you about; they are simply the
frames of a burst.

### "Keep this one"

The duplicate review panel shows the frames side by side and offers to keep one. What
that button does:

* it marks which frame AURA should **start from**;
* it does **not** delete the others;
* it does **not** stop you choosing differently later.

AURA's own suggestion is the technically strongest frame - sharpest on the faces first,
sharpest overall as a tie-break. It says which of the two it used.

---

## Your decisions are permanent

Split a stack, merge two, or pin one, and AURA will not undo it. Re-analysing the
wedding - after an update, after importing another card, after anything - leaves your
grouping exactly as you left it.

They are also undoable. **Undo last grouping change** reverses the most recent one.

One asymmetry worth knowing: **undoing a merge releases it rather than re-splitting it.**
Once two moments are one, the boundary between them is gone, and AURA will not guess
where it was. Releasing hands those frames back to the next grouping pass, which
reconsiders them from scratch. Undoing a split does restore both halves exactly, because
nothing was lost.

---

## When the grouping looks wrong

**Every stack has one frame in it.** Usually one of two things. Either the analysis has
not finished - the header tells you how many photographs are ready to group - or your
camera does not record sub-second times. AURA needs to know *which tenth of a second* a
frame was taken in to see a burst at all; without it, fourteen frames of a burst all
look like they happened at the same instant and then stopped.

**Stacks are far too big.** Check the header for the warning. Most often the perceptual
analysis is finding everything similar, or a camera's clock was never set and its frames
have all landed in one pile.

**One stack is wrong.** Split it. That takes a second, it is permanent, and it is what
the control is for.

---

## What AURA does not do here

It does not choose which photograph you deliver. Grouping decides which frames belong
together; **culling** decides which of them a client sees, and that is a separate step
you control. Nothing on this page rejects a photograph, and nothing on this page can.
