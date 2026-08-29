# What AURA will and will not generate

This is the product's public statement about generated pixels. It is written before the code that
generates any, because a policy written afterwards is a description of what was built rather than
a constraint on it. Phase 24 section 8 makes publishing this the first task of the phase and
requires the CTO, the PM and the Security & Privacy Engineer to co-sign it before a line of
removal code exists.

It is a companion to [`docs/retouch-ethics.md`](retouch-ethics.md), which says what AURA will do
to a person's appearance. This one says what AURA will do to a photograph's content.

## The one sentence

**AURA removes small, extraneous clutter from the edges of a photograph. It never adds anything to
a wedding that was not photographed at that wedding.**

Everything below is that sentence made specific enough to test.

## What this is for

A wedding photographer delivers four hundred photographs and has time to clean up perhaps ten.
The exit sign glowing over the first dance, the gaffer tape across the aisle, the caterer's crate
at the edge of a portrait, the water bottle under a chair - each takes two minutes in Photoshop
and none of them is worth two minutes, so they ship. This phase is those two minutes, taken four
hundred times, on the eighty per cent of cases where the answer is obvious.

It is deliberately not a creative tool. There is no prompt box.

## What AURA will remove

Only objects that are all four of these at once:

1. **Extraneous.** Not part of the event. A bin is; a candle is not. A cable is; a garland is not.
   Signage naming the couple stays; a fire exit sign may go.
2. **Small.** At most 4 % of the frame's area by default, and a studio may only lower that number.
   A region larger than the cap is never automated - it becomes a manual action a person takes
   while looking at the photograph.
3. **Separable.** Not touching a person, a dress, a ring, the cake, or anything on the semantic
   denylist. Overlap above 1 % blocks the removal outright.
4. **Reconstructible without invention.** The pixels behind it can be borrowed from another
   frame of the same moment, or filled from the texture immediately around it. A region that
   would require inventing architecture, a repeating pattern, or any part of a person is refused.

## What AURA will never do

These are not defaults. They are absent from the product, and section "How this is enforced"
below says which mechanism makes each one absent rather than merely switched off.

- **Never add content that was not photographed.** No sky replacement, no added decor, no added
  guests, no extended backgrounds, no "generative fill" of an empty area with invented material.
  There is no prompt, no text input, and no field anywhere on the surface that could carry one.
- **Never alter a person.** No face swaps, no expression changes, no body reshaping, no removing
  or adding people to a group. Phase 21's ethics document covers appearance; this covers presence.
- **Never remove a guest because somebody dislikes them.** Removing a person is a human decision
  about a human being. It is available only as a manual tool, on one photograph at a time, with an
  explicit confirmation, and it is never proposed, never automated and never applied in bulk.
- **Never remove something the wedding was about.** Ritual items, gifts, cake, decor, signage that
  names the couple, and anything a guest is interacting with are on the denylist. When the product
  is unsure whether an object is part of the story, it leaves it. A distraction that ships is a
  small disappointment; a removed heirloom is not recoverable.
- **Never hide that it happened.** Every removed region is recorded in the edit recipe, in the
  decision ledger, in the Explain panel and in the delivery report the photographer can hand to
  their client. There is no mode that performs a removal without recording it.

## Real pixels are preferred to generated ones, always

Where another frame of the same moment shows the same background without the distraction, AURA
aligns that frame and borrows the actual pixels. This is tried first, every time, because borrowed
pixels are a record of the room and generated pixels are a guess about it.

When no sibling frame helps, a classical content-aware fill runs on small textured regions -
grass, carpet, a painted wall. It copies texture that is already in the photograph and cannot
invent structure, which is exactly why it is preferred to a diffusion model.

Diffusion inpainting is the last resort. It is bounded by every check above, it is off unless a
model pack is installed or the photographer has switched on the cloud path, and it is disclosed
differently from the other two because it is different: those two move real pixels, and this one
makes new ones.

## Nothing ships that the product cannot check

After a removal is performed, a detector runs over the result looking for the ways inpainting
fails - repeated texture, warped straight lines, ghost limbs. A region that fails reverts itself
before anybody sees it, and the proposal is recorded as *not safely removable* rather than as a
success. A photographer never reviews an artefact that the product could have caught.

## By default, a person looks first

Removals are proposals, not actions. They arrive in a review queue with a before and an after, and
nothing is applied until somebody accepts it.

The one exception is narrow and is a studio's own decision to make: in Zero-Touch mode, removals
that borrowed real pixels or used the classical fill may apply unattended when their calibrated
confidence is at or above 0.97. Diffusion inpainting always requires review unless a studio has
explicitly opted into unattended generation, which is a separate switch that is off when the
product is installed.

## How this is enforced

A promise that lives only in a document lasts until somebody writes a second caller. Each of these
is a mechanism instead:

| The promise | What makes it true |
|---|---|
| Nothing is generated from a description | There is no text field in the contract, the IPC surface or the panel. A prompt cannot be passed because no type can carry one. |
| Nothing overlapping a person is removed | The denylist intersection runs **before** any candidate reaches a score, in the same shape phase 23 gave its crop safety filter: a filter, never a term. There is no weight anybody could tune to trade a face against a cleaner background. |
| A size cap cannot be raised | The contract owns the ceiling; `cleanup_policy.toml` may only lower it. There is no strength or size field anywhere on the IPC surface. |
| A removal cannot happen through a new code path | One choke-point API, a property test that sweeps it, and a grep-as-a-test that fails the build if fill or inpaint is called directly. |
| A removal is always disclosed | The disclosure is written in the same statement that writes the removal, and a database trigger aborts any statement that would take it away - the shape phase 21 used for its borrow disclosure. |
| A blocked removal is visible | Every refusal records which check failed. A photographer can see what the product declined to do and why, which is also how they learn what it will never do. |

## What this document does not claim

This build has no trained distraction detector and no labelled wedding-distraction vocabulary,
because there are no wedding photographs in this repository. It has no diffusion model pack. The
safety engine, the denylist, the caps, the borrow search, the classical fill, the self-check and
the disclosure records are real, are tested, and are measured against frames whose distractions
were painted into the pixels.

So this document is a statement about what the product is built to do and what it is built to
refuse. It is not evidence about how well it finds a bin in a real reception hall. The phase 24
exit report says which of the two any given number is.

## If you think AURA got this wrong

A removal you disagree with is one click to revert, and the delivery report lists every one so you
can check them all at once. If the product removed something it should not have, that is a defect
rather than a preference, and the reason code on the proposal is what to quote when reporting it.
