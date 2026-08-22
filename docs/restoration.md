# What AURA does to a noisy or soft photograph

This is the plain-language version of phase 22. It is written for a photographer rather than for
an engineer, and everything in it is true of the build you have rather than of a plan.

## The short version

Wedding receptions are dark. AURA cleans up the frames that need it, sharpens the ones that can be
helped, and leaves everything else exactly as you shot it. It does not upscale, it does not invent
detail that was not there, and **it will stop rather than change what somebody looks like**.

## How much noise reduction a photograph gets, and why it is not a setting

AURA does not have a noise reduction slider that applies to your gallery. It measures how much
noise is actually in each photograph, compares that against what *that kind of photograph* carries
well, and picks one of four amounts:

| | What it means |
|---|---|
| **None** | there is no more noise here than this kind of photograph carries well |
| **Light** | just past that point |
| **Standard** | the ordinary reception answer |
| **Strong** | a dance floor at ISO 12800 and very little else |

Three things then hold that choice back.

**The kind of photograph.** A detail shot of the rings and a dance-floor frame do not get the same
treatment, and it is not because one is noisier - it is because a ring shot is made of texture and
a dance-floor frame is made of movement and light. Every kind of photograph has a written ceiling
in a settings file your studio can lower.

**Your camera.** AURA works out how much noise to remove from what your *sensor* does at that ISO,
not from an average of every camera. Where it has not measured a body, it holds back from the
strongest setting on purpose - see "The cameras AURA has not measured" below.

**What it did to the photograph.** After deciding, AURA renders the result and measures how much
fine texture it removed. If lace, a weave or hair lost too much, it steps the amount down and
tries again. That check runs on the actual pixels rather than on the setting, which is the only
way to catch it.

You can change the amount on any photograph. What you cannot do is push it past the ceiling for
that kind of photograph, and that is deliberate: those ceilings are the promises this page makes.

## Sharpening, and why AURA usually does not

AURA sharpens far less often than you might expect, and the reason is that sharpening is only
safe in some places. It measures how soft each photograph actually is - from the frame's own
edges, not from a guess - and then refuses unless **all four** of these are true:

1. the softness is the kind that can be recovered, rather than a frame that is already as sharp as
   the lens managed or one that is far too soft to fix;
2. the softness is **focus rather than movement**. AURA does not try to undo movement. A blurred
   photograph of a moment that happened is a blurred photograph of a moment that happened;
3. the focus landed on the subject rather than in front of or behind them;
4. AURA knows where the skin, the sky and the out-of-focus background are.

The fourth one surprises people, so it is worth being direct about it. Skin, sky and bokeh are not
places where sharpening is *less welcome* - they are the three places it is **visible as damage**
and almost nowhere else. A crunchy sky, a gritty out-of-focus background and a face with sharpened
pores are the three things that make a photograph look processed. So when AURA cannot tell where
those are, it does not sharpen at a lower amount. It does not sharpen.

Skin is the one exception to the exception: it gets a small amount rather than none, because a
face with literally no sharpening inside a frame that was sharpened looks soft rather than
protected.

And after sharpening, AURA measures the edges for the pale outline that over-sharpening produces.
If it finds one, it sharpens more gently, and if it still finds one it does not sharpen that
photograph at all.

## Faces

**AURA will stop rather than change what somebody looks like.** This is the guarantee of this part
of the product, and it is worth explaining exactly how it works, because it is not a promise - it
is a measurement.

Recovering detail in a slightly soft face is possible. Recovering detail in a *very* soft face is
not: there is too little left to work from, so anything that came back would be invented rather
than recovered - a plausible face, which is to say somebody else's. AURA only ever considers faces
in a narrow band of softness, and that check happens before anything else.

For a face that is in the band, here is what happens. AURA measures who the face is - the same
measurement it uses to group people across your wedding - then applies the recovery, then measures
again. If the person has started to measure as even slightly different:

* it eases off and measures again, up to three times;
* if the face still measures as a different person, **it puts the face back the way it was** and
  tells you.

There is no fourth outcome. There is no setting that turns this off, and there is no strength at
which a face that has drifted is delivered anyway. The measured distance is stored on every face
whether it passed or not, so "no face we delivered was changed" is something you can check rather
than something we say.

### On this build, no face is recovered at all

The model that would recover a soft face is not trained in this build, and there is deliberately
no stand-in for it. The obvious substitute - sharpening the face - is not a weaker version of face
recovery. It is a different operation with a worse result and the same name, and telling you AURA
had improved a soft face while handing you a sharpened soft face would be a lie about the one
thing this part of the product promises not to lie about.

So every face records that it was looked at and left alone, and the panel says so.

## The cameras AURA has not measured

AURA knows what a sensor does to a photon count from a measurement of that specific camera body.
**None of the twenty bodies in this build has been measured.** What ships is derived from published
specifications, which is honest and is not the same thing.

The consequence is deliberate and it runs one way: a model that under-estimates the noise removes
too little, which you can see and correct, while one that over-estimates it smears lace, which you
cannot. So an unmeasured body is **capped below the strongest setting**, and the Restore panel
names every body in your wedding that is on that list. When a body gets measured, it comes off the
list and its dance-floor frames become eligible for the strongest setting.

## When this runs

Never while you are editing. Restoration is the most expensive thing in the product and it runs at
export, or as a background pass you start, with progress and a cancel button. Noise reduction is
the exception: it is cheap enough to run with the rest of the editing pipeline, so what you see
while you work is already denoised.

## What AURA will never do here

- **It will not upscale.** Nothing in this part of the product makes a photograph larger than the
  sensor made it. There is nowhere in the design to express one.
- **It will not invent content.** Removing a distraction from a photograph is a different feature
  and it is not this one.
- **It will not undo movement.** A symmetric correction applied to a directional blur produces a
  doubled edge, which is worse rather than softer.
- **It will not change what somebody looks like.** See "Faces".
- **It will not send your photographs anywhere.** This part of the product makes no network call
  of any kind, with any setting, and the code that would do it is not linked into the application.

## Where the numbers are

- Per-photograph decisions, what was measured afterwards, and every face that was left alone: the
  Restore panel.
- Which kinds of photograph may be cleaned up how far, with a written reason for every row:
  `crates/aura-restore/config/restore_profiles.toml`. Your studio may lower any ceiling in it and
  may not raise one.
- What each camera's sensor does: `crates/aura-restore/config/noise_models/`.
- The arguments behind all of it:
  `docs/adr/ADR-0045-restoration-denoise-sharpen-and-identity.md`.
- What this build cannot yet prove about any of it: `docs/progress/PHASE-22-EXIT.md`.

## One simplification worth knowing about

AURA scales the predicted noise of a sensor from the ISO its model was recorded at to the ISO your
photograph was taken at, using a single ratio for both the read noise and the shot noise. On every
sensor in this table those two do move together closely enough for the purpose, and on some
sensors - particularly ones with a dual-gain design that switches at a specific ISO - they do not.
It is written here rather than buried because it is the kind of approximation that would otherwise
be discovered by somebody wondering why one body behaves oddly at one ISO.
