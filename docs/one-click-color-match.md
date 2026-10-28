# AI Color Match

> **Not in the product.** This guide describes the one-click screen of ADR-0170, which was
> superseded before it was merged. Reference matching is the **Color Match** panel; see
> [Match a look](match-a-look.md).

Make a photo take the colour and tone of an edited photo you like, in one click.

## Where it is

Open a photo, choose **Reference matching** in the editing panels. The
**AI Color Match** panel is at the top.

## Matching a photo

1. **Upload References** adds one or more edited JPEG, PNG or TIFF photos to
   your library. **Create Reference** saves the photo you are editing, as it
   looks now, as a reference for other photos.
2. Click a reference. With **Real-time** on (the default), the photo is matched
   straight away. With it off, press **Apply match**.
3. The canvas switches to **Color Match**: the reference on the left, your photo
   on the right. **Reference** shows the reference alone; **Edited** returns to
   your photo.
4. Change **Color Match Preferences** to make the match lighter or stronger:
   Light, Natural, Standard, Strong or Extreme.

If the photo has never been edited, AURA first saves how it looks now, so
**Undo** takes you back to it. **Remove matched look** returns the photo to its
edit from before matching.

The reference must be a different photograph. Choosing the photo you are editing,
a copy of it, or a reference you made from it with **Create Reference** is
refused, and nothing changes. Sync leaves that photo alone and says why.

## What is matched

AURA follows the reference's look, not its exact pixel colours: its warmth or
coolness, its overall tint in shadows, midtones and highlights, its contrast and
how light or matte its blacks and whites are, how colourful it is, and how each
colour is treated. Your photo keeps its own exposure on the subject, so a dark
moody reference does not turn a bright portrait into a silhouette.

People stay themselves. Skin keeps its brightness and most of its colour; it
can warm or cool with the look but never turns green, grey or magenta. Highlights
are protected: if the look would blow out bright areas, AURA eases the top of the
look until it does not.

## How strong is Extreme?

Extreme goes past the reference look (160%). Before anything is saved, AURA
renders the photo at full resolution and checks that highlights are not newly
blown and that skin colour has not been pushed. If a level fails that check,
AURA tries smaller amounts and saves the strongest one that passes, then tells
you which protection held it back, for example "Reduced to 81% of the reference
look so highlights keep their detail."

When the full look cannot bring a photo closer to the reference at all - its light
is too different - AURA uses a lighter, cautious version of the look instead and
says so. Only if even that would harm the photo is nothing changed, and AURA says why,
for example "This reference's look does not suit this photo: even a light match
would blow out highlights." A reference shot in light closer to your photo's
works better.

## Recommended, My References and Presets

- **Recommended** lists your references in order of how alike they are to this
  photo: the same subjects (people or not), similar brightness and
  colourfulness, the same shape. Hover a reference to see why it is suggested.
  This is a guide, not a promise that the match will look good.
- **My References** is your whole library. Advanced Mode, under
  **Detailed matching**, lets you rename, group, favourite and remove them.
- **Save Preset** keeps the chosen reference with its preference. Click a
  preset to use it on any photo.

## Sync

**Sync** matches other photos to the same reference and preference. By default
it uses the photos you selected in the library; the ⚙ button switches to the
whole collection. Each photo is measured and checked on its own, so a dark
church photo and a bright garden photo each get their own correction toward the
same look. Settings you changed by hand on a photo are kept.

## Advanced Mode

Advanced Mode adds four strengths (overall, tone, colour, warmth) and
**Detailed matching**: separate strengths for skin, hair, clothes and
background, twelve fine controls, and library import and export.

## What it does not do

- It matches colour and tone. It does not copy retouching, cropping or local
  edits from the reference.
- A reference of a very different scene (a beach for a candle-lit hall) can
  only be followed loosely. A reference shot in light like your photo's works
  best.
- Matching runs on this computer. References and photos are never uploaded.
