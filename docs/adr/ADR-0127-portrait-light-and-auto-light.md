# ADR-0127: Portrait light - a key light shaped by the head, a measured fill, placed catchlights, a glow - and Auto light

Status: accepted. Date: 2026-10-17.

Portrait editors such as Evoto let a photographer change the light on a face after the shoot: move
a soft key light round the head, open a harsh shadow side, put a sparkle in dull eyes, add a soft
glow. AURA had none of that. Its nearest tools were dodge and burn (ADR-0039, ADR-0093), which paint
brightness where a person or an automatic pass puts it and know nothing about which way a cheek
faces, so a "light from the left" made with them looks like a gradient laid over somebody.

This adds a **Light** tab to the Portrait studio (ADR-0108): key light, light direction, light
height, fill light, catchlights and glow, four light positions in one click, two presets
(*Studio portrait light*, *Dreamy glow*), and an **Auto light** button that measures how each face
is lit and proposes a light.

## 1. It is a finishing tool, and nothing runs it automatically

The values live in the Studio finish extension, `studio_finish_v1.light`
(`crates/aura-recipe/src/studio_finish.rs`), so they are part of the recipe, undoable one step at
a time, and re-create a delivered file from the same four values as every other edit. Auto enhance
and Auto advanced retouch never write this extension, so a light is only ever on a photograph
because somebody chose it. **Auto light** is a button, not a pass: `suggest_portrait_light` stores
nothing, and the panel saves what it proposes as one step labelled *Auto light* that can be undone.

A finish whose light is the default is serialized **without** the `light` key, so a recipe saved
before this change keeps its exact bytes and hash.

## 2. A head is round

`crates/aura-render/src/portrait_light.rs`. Each detected face is given a shape: an ellipsoid placed
and sized from its own landmarks - centred 0.55 eye-distances below the eye line, 1.05 eye-distances
wide each side, 1.5 tall, as deep as it is wide, turned by the eye line's tilt. A pixel's surface
normal comes from where it falls on that shape, and the light shades it by a **wrapped Lambert
term** (wrap 0.45), so the light rolls round the side of the head the way a large soft source does
rather than cutting off at the edge of the face. The shade reaches 1.45 times past the ellipse with
a smooth fall-off, so ears, hairline and the top of the neck go with the face.

- **The key light is a gain in stops**, up to 1.6 times the existing light at full travel, and 75 %
  of the brightness it adds is taken back by renormalising against the shade's mean over the face.
  So a key light *models* a face - the near cheek rises, the far cheek and jaw fall into shadow -
  much more than it brightens it.
- **Every gain keeps the pixel's chromaticity** (`colour::set_luma`), the rule every tone stage in
  this renderer keeps. A light changes how bright skin is and never what colour it is, which is the
  same promise the rest of the product makes about skin tone.
- **Direction and height are where the light is**, seen from the camera: direction -180..180 degrees
  (negative is the photograph's left, ±180 is behind the subject - a rim light), height -30..80
  degrees above the eyes. They change nothing while the key light and catchlights are zero.
- The light goes on **before any reshape**, so it follows the face as photographed and then moves
  with it, and the glow goes on after the warp, from the finished frame.

## 3. Fill measures the shadow that is there

The fill does not assume where the shadow is. Each face's skin brightness is smoothed at a fifth of
its eye distance, the median is taken over that person's own skin, and skin below the median is
lifted - fully from 1.5 stops below, by up to a stop at full travel. Skin above the median is left
alone, and eyes, brows, lips and hair are not skin, so they keep their depth. On a painted face lit
a stop and a half harder on one side, fill 100 closes the gap by more than 0.4 stops while the lit
cheek moves by less than 0.15 (`tests/portrait_light.rs`).

## 4. Catchlights are placed, never painted at random

A catchlight is the light reflected in the cornea, so it goes on the side of the iris facing the key
light (up and to the camera's left when there is no key light, which is where most portrait light
comes from), in the same place in both eyes of a face. Its size is 0.14 of the iris radius, its
brightness follows the white of that same eye, and its colour leans toward the colour of that white,
so it takes the colour of the light the eye already shows.

The parse's iris region is soft - it reaches onto the lid and the white - so an eye is read in two
passes: the iris's median brightness first, then only the pixels near it (at most three times the
median) as its core, and the white of the eye is read as the interquartile mean of the sclera
plane. A pixel 3.5 times the iris's median and as bright as the white counts as an existing
catchlight. **An eye too small to read** (iris radius under 1.8 render pixels), an iris that is not
there, or **a face where either eye already has a catchlight is left alone** - one eye with a real
reflection and one with a painted one is worse than either.

Two things went wrong first, both on real photographs: the iris plane's bleed onto the lid read as a
"catchlight already there" on almost every face, and catchlights at a third of the iris looked like
stickers and took a pink cast from the lid. The two-pass core and the 0.14 size are the fixes.

## 5. Glow

A soft bloom from the brightest parts of the frame: pixels from 0.45 to 1.6 in working-space
luminance contribute, blurred over 1.5 % of the shorter edge on a reduced grid (so its cost does not
grow with the radius), added back at up to 0.6, with 12 % of the frame softened toward its own blur.
The darks are left alone. Glow needs no face and is never proposed by Auto light: it is a look, not
a correction.

## 6. Auto light reads the light on the largest face

A group is lit by one light, and the person nearest the camera shows it best, so `suggest` reads the
largest face's skin: the difference between its two sides and between its upper and lower halves, in
stops.

| What it reads | What it proposes |
|---|---|
| One side more than a stop darker (harsh side light, flash off-axis) | A fill sized to the difference (15 to 60), **no key light** - the light is already there |
| Both sides within a third of a stop (flat light, on-camera flash, overcast) | A soft key light of 30 from the side already a little brighter, or from the camera's left |
| In between | A light key of 18 from where the light already comes from, a fill of 12 |
| A face with readable eyes and no catchlight | Catchlights of 45 |

Every decision comes back as a sentence, and the panel shows them. When the photograph has no face
nothing is proposed or saved and the panel says so. The glow a photographer already set is kept.

## 7. What was checked

- Seven integration tests on the painted portrait (`crates/aura-render/tests/portrait_light.rs`): a
  light that adds nothing renders bit-identically to no light; a key from the left lifts the left
  cheek and keeps the ground; fill closes a half-lit face's gap; catchlights land up and left for a
  light from the camera's left and move nothing outside the eyes; an eye with a reflection is left
  alone; Auto light proposes a fill and catchlights for a half-lit face and a key for an even one;
  glow alone needs no face. Five unit tests on the shading and the glow.
- The command end to end through ingest, the preview, the parse and the recipe store
  (`crates/aura-app/tests/studio_finish.rs`): it finds the face, proposes a key, stores nothing, and
  the saved proposal reaches the render.
- By eye on seven public photographs (two dlib face-detection samples, two face-recognition library
  portraits, a two-person photo and two others), through the ignored harness
  `crates/aura-render/tests/portrait_light_photos.rs` (`AURA_LIGHT_PHOTOS`, `AURA_LIGHT_VARIANTS`).
  A key light at 60 to 80 follows the curve of the face; fill evens a side-lit face without
  flattening it; catchlights read as reflections; Auto light gave a flatly lit portrait a key of 30,
  gave two flash snapshots a light key and fill, and left alone the eyes of a portrait that already
  had studio catchlights.

## What is not claimed

- No study of photographers' preferences, and no comparison against Evoto's own output: its
  relighting could not be run from the build environment. The tuning was done by eye on seven small
  JPEGs, none of them a raw file and none of them a wedding.
- The head is an ellipsoid. A strong key light from far to the side does not cast a nose shadow or
  respect the eye sockets the way a real light does; at the values Auto light proposes the
  difference is not visible on the photographs it was tried on, and at full travel from 90 degrees it
  can be.
- Faces come from the Haar-cascade parse (ADR-0065). A face it misses gets no light; drawing the face
  in Portrait retouch first is the remedy, and the light then uses that face.
- Speed was not measured on its own. The light reads the parse every portrait tool already reads,
  works per pixel inside each face's reach, and the glow blurs on a reduced grid, so neither grows
  with anything but the frame; a measurement belongs with the next profiling run (ADR-0123
  section 10).
