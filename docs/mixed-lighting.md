# Mixed lighting, and what AURA does about it

*What the marks on your photographs mean, in the product's own words.*

Most of a wedding is lit by more than one thing. The reception has tungsten uplighters and a
window; the ceremony has daylight through a door and an LED panel the videographer brought;
the dance floor has a purple wash over all of it. There is no single correct colour for a
photograph lit by two different lights, which is why auto white balance fails there and why
this is the part of editing photographers do by hand.

This page explains what AURA does instead, and what each of the notes on a photograph means.

## Exposure is set for the people, not for the room

Every automatic exposure you have used measures the whole frame and moves the brightness
until the average hits a target. In a dark reception that lifts the room and leaves the faces
where they were. On a bright beach it does the opposite.

AURA measures **the faces**, weighted by how prominent each person is in the frame, and moves
the exposure until they land where a photograph of that kind wants them. A ceremony wants
faces brighter than a dance floor does; a dance floor is *supposed* to be moody, and lifting
it until it looks like a ceremony is not a correction.

Two things stop it going too far:

- **Highlights.** If lifting the faces would blow out the background beyond what that kind of
  photograph tolerates, AURA stops at the limit and says so. You will see *"held back to keep
  the highlights"*. This is the same judgement you would make by hand, and it binds most often
  on frames where the subject is much darker than the background.
- **Shadows.** Lifting a dark frame lifts its noise with it. AURA will not push past what your
  camera body can take at the ISO it was shot at.

When there is nobody in the photograph — the rings, the venue, a flat-lay — there is no face
to expose for. AURA says so with *"no faces here, so this was exposed for the kind of
photograph it is"*, and the panel tells you how much of the wedding was exposed that way.

## Colour is worked out from four kinds of evidence at once

AURA does not have one method for white balance. It has four, and they fail in different
ways, which is the point:

| Evidence | What it is | Where it fails |
|---|---|---|
| The camera's own guess | What your body recorded | Confidently wrong under tungsten |
| Grey-world | The frame averages to neutral | A bride in a white dress; a red mandap |
| White-patch | The brightest thing is white | A champagne flute catching a highlight |
| Known neutrals | A dress, a tablecloth, printed paper | A reception with no white object in shot |

Each one proposes an answer. AURA then asks a fifth question of every proposal: **does this
make the skin of the people in this photograph look like their skin looks in the rest of this
wedding?** An answer that does not is rejected, however good its own reasoning was.

That last check is the one that matters most, and the next section is about how it works.

## Skin is measured, never assumed

AURA has no idea of what skin "should" look like. There is no ideal skin colour anywhere in
the product — not in the code, not in the settings, not in the database. This is deliberate,
and it is worth being plain about why.

A white balance has a free parameter, and skin is the most visible surface it acts on. If you
give an editor a fixed target for skin, it will move everybody's skin toward that target. If
that target came from one kind of skin, then everybody else's skin gets corrected *away from
itself* — which is what skin lightening looks like when it is built by someone who believed
they were removing a colour cast.

So AURA measures instead. For each person it recognises, it collects how their skin actually
looks in the frames of this wedding where the light was easiest to read, and builds a small
region — **their** region — from that. Every white-balance answer is then checked against it.

Three things follow, and you will see all three in the panel:

- **Early frames have no reference.** Nobody has been measured yet, so the colour comes from
  the light alone. The panel says *"AURA has not seen enough well-lit photographs of these
  people yet"*. This is normal at the start of a wedding and it resolves as the pass goes on.
- **It is per person, not per group.** There are no skin-tone categories in the product, and
  nothing is ever recorded about anybody's appearance beyond the measurement itself.
- **A weak reference is worse than none.** Below five good frames of a person, AURA does not
  build a region at all, because a region fitted to two photographs looks like evidence and
  is noise.

## When there are two lights: the mixed-light mark

When AURA finds two different-coloured lights in one frame, and they disagree about different
parts of the picture, it marks the photograph **mixed light** and tells you:

> There is more than one colour of light in this frame. AURA has set the colour for the light
> on the people; the rest can be corrected separately later.

That is exactly what it does. It picks the light falling on the *subject* and balances for
that, so the people are right. The wall, or the window, or the far side of the room will
still carry the other light. That is not a mistake and it is not something a single global
white balance can fix — it needs a correction applied to part of the frame, which a later
version of AURA will do for you and which you can do by hand today.

The mark is stored, so those frames can be found again in one click rather than spotted one
at a time.

## When the light is meant to be coloured

A purple dance floor is supposed to be purple. A stage wash, coloured uplighters, candlelight
at a Nepali reception — these are decisions somebody made, and neutralising them is not
correcting a photograph, it is undoing it.

AURA detects a saturated, deliberate light and **keeps it**. It corrects only far enough to
keep skin believable, and no further. The panel says:

> This is coloured stage light and AURA has kept it. It has corrected only far enough to keep
> skin believable.

Which scenes count as "the light is a choice here" is a setting a person decides, not a
threshold buried in the code — it lives in `exposure_targets.toml` with a written reason on
every row.

The opposite trap is a red canopy over a Hindu ceremony, or a room with red walls. That is a
*surface* that is red, not a light. Neutralising it would drain the ceremony; treating it as a
cast would leave everybody orange.

AURA separates the two with two things at once: how far the light sits from the colours real
light actually comes in — a tungsten bulb is very warm and entirely ordinary, a purple wash is
barely warm and entirely deliberate — and whether the scene was recognised as a staged one in
the first place. A red mandap is a ceremony, not a stage, so its red reads as a surface no
matter how strongly the room measures red. That second half matters: the measurements that see
"red" cannot themselves tell a red light from a red wall, and the scene is what does.

## Backlit frames

If the subject is lit from behind — an exit, a sparkler send-off, a window portrait — exposing
for the whole frame makes a silhouette. AURA exposes for the subject and lets the background
go bright on purpose, and says so:

> Backlit. AURA exposed for the subject and let the background go bright on purpose.

How much background it will let go is a per-scene setting, because an exit shot tolerates a
blown sky and a family portrait does not.

## Everything here is a suggestion you can overrule

Every number AURA sets is a starting point. Change it and it becomes yours: the control gets a
dot beside it, and **no automatic pass will ever move it again**, including passes in versions
of AURA that do not exist yet. If you want AURA's answer back, "Reset to AI suggestion" gives
it to you.

When you do change something, AURA keeps its own answer beside yours rather than forgetting
it. That is how the review queue can show you where the two disagreed, and how AURA learns
what you actually want.

## The one thing to check before you trust any of this

Two numbers in the panel matter more than the rest:

- **How much of the wedding has been worked out at all.** A gallery with a four-hour hole in
  it looks exactly like a gallery that was decided.
- **How much of it was exposed for a face.** If that number is low, the improvement described
  at the top of this page did not happen on most of your photographs, and you are looking at
  ordinary automatic exposure with a better explanation attached.

Both are shown, always, on every project.

---

*See also: [How colour works](colour-management.md), [How AURA culls](how-aura-culls.md),
[How confidence works](how-confidence-works.md).*
