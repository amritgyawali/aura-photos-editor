# What AURA means by a region

Every change AURA makes to *part* of a photograph happens inside a region: brightening a face
without brightening the wall behind it, warming a dress without warming the grass, smoothing
skin without smoothing an eyelash. This page is what those regions are, how good they are, and
what you can do about it when one is wrong.

## The twenty regions

AURA looks for twenty things in every photograph you keep.

**People.** Skin, face, eyes, eye whites, irises, teeth, lips, eyebrows, hair, facial hair,
clothing and dress.

**The picture as a whole.** Subject, background, sky, greenery, water, floor, and window or
light source.

**One that is not for you.** The skin-safe zone is a slightly enlarged version of everyone's
skin, and it exists so that the rest of AURA can check itself: when AURA grades a photograph it
measures what it did to the pixels inside that zone, and undoes the grade if anybody's skin
moved. You will never need to select it, and it is in the list because a region that is doing
work should be visible.

A photograph with no people in it has no skin region - not an empty one, and not a guessed one.
AURA says nobody was found rather than finding somebody.

## The regions belong to people

Where AURA knows who is in the photograph, each person gets their own regions: *the bride's
skin*, *the groom's hair*. That is what makes it possible to brighten one person's face at a
table of eleven, and it is what keeps her skin looking the same all night in the gallery.

When two people overlap so much that AURA cannot tell whose is whose, the region is not given to
either of them. It is still a skin region and you can still use it; it just is not tied to a
name. Guessing would be worse: a mask that quietly includes the guest standing behind the bride
is a mask that lightens a stranger every time you lighten her.

## Two numbers, and why there are two

Every region carries **certainty** and **edge quality**, and the panel shows both.

**Certainty** is how sure AURA is about what the region *is*. It is low when the answer is
genuinely ambiguous - a dark suit against a dark background, a face at the back of a crowd.

**Edge quality** is how well AURA could find the *boundary*. It is low when the photograph does
not really contain one: a veil backlit against a bright wall, a subject in motion, hair against
a background of nearly the same brightness.

They are shown separately because they are fixed by different things. You can redraw a boundary
with the brush. You cannot redraw what a region is.

## What a low number actually does

It does not stop AURA editing. It **reduces how far** any change through that region can go, and
it takes two operations off the table entirely: skin smoothing and generative cleanup. Those two
are the ones that produce a visible artefact when the region is slightly wrong, and they are the
ones AURA will not attempt through a region it is unsure of.

Everything else is scaled rather than refused. A veil at middling edge quality carries a third of
a stop of local brightening perfectly well.

The panel says which of the two numbers is holding a region back, in a sentence, because "amber"
does not tell you what to do.

## Editing a region by hand

Four things, and each is one undo step:

- **Brush** adds to or removes from a region.
- **Feather** softens its edge. The slider means the same softness at every zoom level, so a
  region that looks right on screen looks right on the export.
- **Refine edge** closes the pinholes a colour-grown region picks up along a busy boundary,
  without moving the boundary.
- **Reset to AURA's version** throws your edit away and lets AURA measure the region again.

**A region you have touched is never regenerated.** When AURA improves how it finds regions - a
new model, a better boundary - it redoes every region in your wedding *except* the ones you drew.
Those are kept exactly as you left them, and the only thing that changes them is you pressing
reset.

## Where regions come from, in this build

Two parts of this are worth being straight about.

**The learned segmentation model is not trained.** It ships, it is signed, it has a model card,
and nothing in this build asks it anything. What actually finds your regions is measurement:
AURA reads the faces it detected, samples the actual colour of each person's skin *in that
photograph*, and grows outwards through pixels of the same colour that are connected to them.
Sky, greenery, water, floor and windows are found the same way, from colour, position and
texture. It is the approach the phase document describes for skin, applied to everything it
applies to.

That has consequences you can see. Blonde hair against a dark background is often missed by the
hair region - it is still in the subject, so brightening the subject still reaches it. And AURA
calls a garment a dress only when it is bright, unsaturated and reaches the bottom of the frame;
a red lehenga comes back as clothing, which is a region that works, rather than as a dress, which
would be a region that lied.

**There is no ideal skin tone anywhere in AURA.** The skin region is grown from a colour measured
in your photograph, from the faces in it. There is no reference skin colour in the code, in the
database, or in any configuration file, and the release gate scans for one every time it runs.
`docs/skin-fairness.md` says more about why that matters.

## What regions cost

All twenty regions of one photograph fit in 180 KB. Regions are found only for the photographs
you keep, after culling, because a rejected frame never needs one - and that is a large part of
why the whole thing finishes in the time it does.

## Related

- `docs/skin-fairness.md` - how AURA handles skin, and what it refuses to assume
- `docs/how-confidence-works.md` - what a confidence number means across the product
- `docs/model-cards/semantic_segment.md` and `docs/model-cards/alpha_matting.md`
- `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md`
