# Portrait retouch

What AURA finds in a portrait, what it will change, what it will never change, and how well
it was measured to work - in the product's own words.

## What AURA finds

Open **Portrait retouch** and AURA reads the photograph once and shows what it found, laid over
the picture:

| Region | What it is |
|---|---|
| Face | The oval from hairline to chin, features included, hair excluded |
| Skin | Every visible patch of skin on a person AURA found: face, neck, arms, hands |
| Eyes | The openings between the lids |
| Iris | The coloured ring, pupil included |
| Whites of the eyes | The sclera |
| Eyebrows | The brows |
| Under the eyes | The crescent below each eye |
| Nose | The nose |
| Lips | The lips, teeth excluded |
| Teeth | The visible teeth - nothing when the mouth is closed |
| Mouth | Lips, teeth and the dark between them |
| Beard and moustache | Facial hair, when there is enough of it to be a beard |
| Neck | The neck |
| Hair | The hair on the head |
| Arms and shoulders | Skin below the neck |
| Clothing | What a person is wearing |
| Person | The whole person |
| Background | Everything that is not a person |
| Sky | Open sky reaching the top of the frame |

Every face is shown with its eyes, nose and mouth marked, so you can see what AURA measured
before you change anything.

### How it finds a face

AURA finds faces with a boosted cascade - OpenCV's published, BSD-licensed Haar cascades,
evaluated by AURA's own code - scanned six ways: upright, with the head tilted twenty degrees
either way, turned to one side and the other, and on a locally equalised copy that recovers faces
in a dark corner. A candidate becomes a face only with evidence: a skin-coloured centre (unless
the photograph has no colour) and either many agreeing windows or an eye where an eye should be.

It then measures - rather than predicts - everything else, relative to the face it found: the eyes
from the eye cascade and the darkest compact blob where an iris should be, the mouth from a
colour map that makes lips stand out, the brows as what is darker than *that person's* forehead,
the teeth as what is brighter and less coloured than *that person's* lips.

### A face AURA missed

Choose **Draw a face AURA missed** and drag over it. The box is saved with your edit, so the
preview, the export and every later render find the same face.

## How skin is found - and why your skin tone is never a target

AURA never compares anybody's skin with an "ideal" colour. A deliberately broad rule only decides
which parts of a *detected face* to sample; every skin region is then a distance from that
person's own cheeks, forehead and chin. The model a guest with dark skin is segmented against is
the colour of their own face.

The same rule governs retouching. **Even skin tone** moves each pixel toward the local average of
the same person's own skin - it reduces blotches and redness and leaves the tone where it was.
There is no constant anywhere in the code that a person's skin could be compared with.

## What AURA will change

Fourteen adjustments, each from 0 to 100:

| Adjustment | What it does | What it keeps |
|---|---|---|
| Smooth skin | Softens blotches and uneven texture | Pores: texture is a separate band and survives |
| Even skin tone | Evens redness toward the person's own tone | The tone itself |
| Clear blemishes | Heals small dark or red marks | Moles, freckles and nostrils - very dark, compact marks are left alone |
| Brighten under-eyes | Lifts shadow toward the cheek below | The skin's texture |
| Reduce shine | Softens specular shine on the face | Colour |
| Face fill light | Lifts shadows on the face | Highlights |
| Brighten eyes | A little more light in the eyes | Nothing is pushed to white |
| Iris detail | Brings out iris texture and colour | The iris's own colour |
| Clear eye whites | Reduces redness in the sclera | Never paper white |
| Define brows | Slightly deeper, crisper brows | Their shape |
| Whiten teeth | Takes the yellow out | A bound: brighter, never blank |
| Lip colour | A touch more colour and shape | The lip colour itself |
| Define hair | Crisper strands and texture | - |
| Blur background | A shallow-focus look | The person stays sharp; the blur is held back from their edge |

You can also **adjust one region** - brightness, contrast, saturation and warmth for the hair, the
background, the clothing, the eyes or any other region on its own.

**Auto retouch** measures the face first - how uneven the skin is, how much darker the skin under
the eyes is than the cheek, how yellow the teeth are, how red the whites of the eyes are - and sizes
each adjustment from that. A closed mouth gets no whitening; white eyes get none either. Each
decision comes with a sentence saying why. Choose *Subtle*, *Auto retouch* or *Polished* for less or
more.

Once you move a slider yourself, the automatic retouch will not change that photograph again, and
it says so.

## What AURA will never change

- **Nobody is reshaped.** There is no slimming, no liquify, no enlarging, no feature moving - not
  as a setting and not in the code.
- **Nobody's skin tone is changed.** Smoothing, evening and fill light work on blotches, shadows
  and shine, never on the tone of a person's skin.
- **Nothing is invented.** Every adjustment changes the tone or colour of a pixel that was already
  there.

## How a retouch reaches your files

A retouch is part of your edit, not a separate layer. The renderer re-derives the regions from the
photograph itself every time it renders, so the preview you see, the export and a re-export a year
later all find the same faces. The parse version is part of the renderer's engine string, so a
change to how regions are found is recorded as a change to the engine. A portrait retouch is always
rendered as one piece rather than in tiles, because a face cut by a tile boundary is not a face.

## How well it was measured to work

There is no consented face data in this repository and there will not be, so nothing here is a
claim about accuracy on a real wedding. What was measured:

- **The cascade evaluator agrees with OpenCV.** On 22 photographs, all 80 face boxes AURA's
  evaluator found matched the boxes OpenCV itself found on the same grey frames to within two
  pixels and two agreeing windows.
- **The evidence rule removed every false positive the extra passes produced** on those 22
  photographs (a wall clock, a badge, a flag, a table top) without losing a face the plain pass
  found.
- **Every Monk Skin Tone is found.** The painted test face is detected, and its face region agrees
  with the truth to the same degree, at all ten published Monk swatches on three grounds. An
  earlier painted eye with a large bright white and no eye socket made the two darkest tones look
  undetectable; that was the painting's fault, and the darkest tone is still found only by the
  equalised pass - which is why that pass exists. **This is a measurement on painted faces, not on
  people, and says nothing about real skin.**
- **Speed:** 45-190 ms to find faces and 110-540 ms for the whole parse at a 640 px frame on a
  four-core container, cached per photograph after the first time.

`crates/aura-portrait/tests/local_eval.rs` and `crates/aura-render/tests/portrait_local.rs` are the
instruments: point them at a folder of your own photographs and they print detections and write
overlays and before-and-after images for a person to look at, which is the only evaluation of a
retouch that means anything.

## What it cannot do yet

- **Profile and strongly turned faces** are found less reliably than frontal ones; draw them.
- **Very small faces** - under about 3 % of the frame's long edge - are not looked for.
- **Body and background** come from colour models seeded at the chest and beside the shoulders.
  A sleeve in deep shadow or a wall the colour of someone's hair can be put on the wrong side, so
  **Blur background** is never part of the automatic retouch.
- **Lens distortion correction** moves pixels after the regions are found; with strong correction
  a region can sit a few pixels off.
- **No real-photograph fairness study has been done.** The painted-face measurement proves the
  mechanism is per-person; it does not prove equal accuracy on real people at every skin tone.
