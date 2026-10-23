# ADR-0150: A hundred measured portrait, attire, background and workflow tools in the style of Evoto

- Status: accepted
- Date: 2026-10-22
- Supersedes: nothing. Extends ADR-0108 (the Studio's finishing tools), ADR-0127 (portrait light)
  and ADR-0131/0135 (effects placed by hand).

## Context

The photographer asked for a hundred more of Evoto's tools, ideas and ways of working in AURA,
tested on their own photographs (27 Canon CR3 files of a Nepali wedding in `H:\AURA EDIT`), with a
report on how well each works.

Evoto is a closed product built on trained models. Its binary and its model files were **not**
opened, decompiled or copied for this work, as on 2026-10-19: what each tool should do is taken
from Evoto's public feature pages and from how a portrait retoucher works, and every algorithm here
is AURA's own. Pixel-for-pixel parity with Evoto is therefore not claimed and cannot be measured.

Three inventories were read first so that nothing is a duplicate: AURA's own panels (ADR-0108's
13 face and 7 body sliders, makeup colours, portrait light, the retouch stack), the 88 hand-placed
effects of ADR-0131 to 0135, and about five hundred laboratory operators and measurements in the
fifty-capability and laboratory work of ADR-0132 to 0138 (most of them generic image processing).
What was missing was mostly what Evoto is *for*: sliders that find every face and act on it.

## Decision

### 1. Evoto's paradigm: a slider finds what it acts on

Ninety of the hundred are sliders or hand-placed tools that measure their own target:

| Group | Tools | Where |
|---|---|---|
| A, B face shape | eye height, tilt, inner and outer corners, **measured eye symmetry**, nose bridge, tip, lift, nostrils, cheek apples; upper and lower lip, mouth position, brow height, arch and thickness, temples, lower-face length, chin width, hair volume | `FaceShape` (20 fields), `studio_finish::more_face_warps` |
| C body | thighs, calves, **flat stomach on the side the silhouette shows it**, hip lift, shoulder line, torso length, height, neck slim, double chin (warp and fold), **level head from the measured eye line** | `BodyShape` (10 fields), `more_body_warps`, `head_and_neck_warps` |
| D skin | aegyo-sal, lip lines, dewy glow, stubble shadow, rosy complexion, even teeth, sharpen features, beard definition, clothing colour on skin, refine pores | `SkinFinish`, `studio_evoto::skin` |
| E makeup | eyeliner and wing **on the measured lid line**, fuller lashes, lower lashes, lip liner, ombre lips, highlighter, bronzer, enhance worn makeup, fill brows | `MakeupFinish`, `studio_evoto::makeup` |
| F hair and body | hair shine, fill parting, hair colour depth, calm hair edge, body definition; by hand: body hair, cellulite, tan lines, hands, knuckles and elbows | `HairBodyFinish`, `studio_evoto::hair`, `retouch_effects::evoto` |
| G attire | gold jewellery, recover clipped red fabric, veil clarity, embroidery, garment edges, mehndi, white garments, dark suits, outfit colour; by hand: glasses-coating reflection | `AttireFinish`, `studio_evoto::attire` |
| H background | backdrop light, background saturation, brightness and warmth, depth blur, background noise, light wrap, **colour that keeps shadows**, **sky with clouds**; by hand: remove a cast shadow | `SceneFinish`, `studio_evoto::scene`, two `BackgroundMode`s |
| I people | people exposure, warmth and clarity, **balance group faces**, **match group skin colour**, relight, soften harsh shadows, backlight rescue, face-aware vignette, face brightness | `SubjectFinish`, `studio_evoto::subject` |

Ten are workflows (`aura_app::evoto_workflow`): a face picker that limits face, skin and makeup work
to chosen faces (`StudioFinish::faces`), ID photographs at 35x45 mm, 2x2 in and 33x48 mm on white,
measured headshot, half-body, full-body, square and story crops, levelling the eye line without
empty corners, a transparent PNG cut-out, an Instagram grid numbered in posting order, carousel
slides, a before-and-after, a 2x Lanczos enlargement that does not ring, and a check over a
collection for faces showing no white of the eye (section 8 says why it is not a blink detector).

### 2. Measured against the photograph, never a constant

The repository's rule from phase 15 holds throughout: a skin target is measured, never assumed.
Stubble is evened toward *that person's* cheeks; reflected colour is removed along the direction
from the face's own centre colour toward the clothing actually beside it; group balance lifts darker
faces toward the *group's* median face and never darkens anyone; rosy complexion rotates a person's
own skin colour a few degrees and keeps its luminance exactly. No ideal skin colour or brightness
appears anywhere.

### 3. Halo discipline

Two defects were found by the unit tests and fixed before any photograph was opened:

- A **feathered** hair mask let gloss and colour depth stretch the background's contrast beside the
  hair into a halo. Adjustments that expand contrast about a local level use the parse's own edge;
  only tone moves (exposure, warmth, saturation) use a feather, and that feather is one analysis pixel.
- Local levels near features (lips, nostrils, teeth edges) are measured with a **masked blur** over
  the region alone, so a skin or tooth level is never pulled down by the lips beside it, and pore
  refinement refuses anything deeper than a pore.

### 4. The veil is physics, not dehaze

A dark-channel dehaze would add contrast to every face, veiled or not. A net veil adds its own light
to everything behind it, so `veil_clarity` removes each channel's black level measured on the face
and restores the face's highlight with **one** scale for all three channels. A per-channel scale was
tried first and put the veil's red back. On a face without a veil it is a measured black point.

### 5. Every field is written, because the merge keeps what a proposal leaves out

This section first said the opposite: new fields were skipped while zero so that a finish written
before ADR-0150 kept its bytes and its hash. That was wrong, and the real-photograph run found why.
`schema::merge` keeps every leaf a proposal does not mention, so a slider moved back to zero was
*left out* of the proposal and the old value survived: resetting a slider did nothing. The same
defect was already in ADR-0108's own controls - Reset all, Keep original background, Clear liquify
and setting the light back to zero all failed through the merge.

`studio_finish::write` now writes every field of every group, a cleared list as an empty list and
no background as an explicit `null`, and `through_merge::every_reset_survives_the_merge` puts each
way back to neutral through the real merge. A finish written before ADR-0150 still reads exactly as
it did (every new field has a serde default); the first time it is saved again it is written whole,
so its hash changes once.

### 6. Nothing new runs automatically

Every one of the hundred needs a photographer to move a slider, press a button or place a tool.
The automatic passes never write the studio finish extension, the seven hand-placed tools are in
`advanced_retouch::stage_of`'s manual-only list, and exports are written only into a folder the
photographer chose, never over an existing file.

### 7. What the photographs found

The 27 photographs were run through every tool by `crates/aura-app/tests/evoto100_photos.rs`, which
imports them into an isolated catalogue, applies each tool through the application's own commands,
renders the result with `render_image` and writes a before-and-after crop. Each crop was looked at.
What was wrong, and what changed:

- **ID photographs on white** cut the person out with the parse's colour models, which left veil
  and hair edges grey. A tool that tells a person from the background now uses the learned subject
  selection (ADR-0103), measured when the finish is saved and **frozen into the recipe** as
  `StudioFinish::subject_matte` (RLE, at most 512 x 512 cells), so a re-render needs no model and
  the four values a delivered file is re-created from still decide every pixel.
- **The parse**, `PARSE_VER` 3 then 4. The mouth was a redness centroid, which a bride's red veil
  could pull onto itself (now the heaviest single blob, a blob at the window's side discounted) and
  which followed the lit half of a mouth lit from one side (now centred on the dark line between the
  lips, trusted outright when it agrees with the face's centre line and otherwise allowed to move the
  centre by a fifth of the mouth). Lips must be more magenta than the skin as well as redder, and
  end at the mouth's corners: blush beside a corner was taken for lip. A white of the eye must be
  paler than the palest skin around that eye and lie on rows as dark as the iris: concealer and lid
  shimmer read as white of the eye on closed eyes.
- **Eyeliner and lashes** follow the measured lash line - the first sample, down each column, two
  fifths of the way from the lid above the eye to the column's darkest - rather than the eye
  region, which is an ellipse of the same size open or shut and put a liner across a closed lid.
- **Lip tools** are confined to an ellipse around the measured mouth (`near_mouth`): the lips region
  alone reached a red veil beside the face.
- **Chin, neck and double chin** use a measured chin - the strongest light-to-dark step down the
  face's centre line below the mouth - instead of a fixed distance, which put them on the necklace
  of a bride looking down. **Body sliders** in a group act on the person under the chosen face.
- **Stubble** only where the skin is finely textured like stubble (sindoor and powder are not);
  **beard definition** only on hair darker than the cheeks; **hair colour depth** into the body of
  the hair, not strands already lit (they turned orange); **body definition** only on body skin the
  colour of the face (red silk the parse took for an arm); **red recovery** only inside solid red,
  near the frame's own red ceiling (a border with embroidery drew a dark halo); **dewy glow** made
  visible at ordinary strengths.

### 8. A blink verdict needs a trained eye-state model

The parse's eye region is the same ellipse open or shut, so the first blink check, which summed it,
flagged none of the 27 photographs - including a bride with both eyes closed. With the whites of the
eyes measured properly (section 7) the check separates *most* closed eyes from open ones, but not
all: pale shimmer on a closed lid reads as white, and squints, glare on glasses, red veils and small,
shaded older eyes read as none. Hand-built features (dark-band aspect, iris height) were tried and
overlapped just as much. Phase 09's eye-state head is still a placeholder and no other model in this
build sees eyes.

So the command reports what it measures: faces showing no white of either eye, below 0.002 of the
interocular distance squared, set on these 27 photographs. The panel calls it a list to look
through, names what lands on it, and says that telling them apart needs a trained eye-state model.

## Consequences

- The studio finish extension grows six groups and a list of chosen faces; `StudioFinish` remains
  in the recipe's extension map (ADR-0102's reason) and no frozen contract changes.
- The parse (`aura_portrait`, Haar cascades and measured regions) is the limit of every face and
  region tool. Where it misses a face - small faces in group photographs - the tools do nothing to
  that face; where a region is coarse, so is the tool. On the 27 photographs the lips are right on
  52 of 56 faces; the misses are natural lips under two moustaches, a profile and one chin spot.
- `PARSE_VER` 4 re-parses every frame, so the sclera and lips regions of ADR-0065's portrait
  operators move too: sclera whitening no longer greys a closed lid.
- The PNG cut-out uses the learned subject selection (ADR-0103) when its model is installed and
  says so; otherwise it uses the parse's person region and says that too.
- Real-photograph results are in `docs/reports/AURA-evoto-hundred-2026-10-22.md`.
