# ADR-0065: Measured portrait parsing, and portrait retouch executed by the renderer

- Status: accepted
- Date: 2026-10-04
- Supersedes: nothing. Amends no frozen contract.

## Context

Phases 18 to 22 built masks, local light, skin retouching and micro-retouching against input ports
- `MaskField`, `RetouchPass::with_masks`, `MicroPass::with_regions` - and on a real photograph
nothing filled them: phase 06's face detector is a placeholder that finds no faces (its condition
C1), so every retouch stage was correct, tested, and gated to zero on every frame a photographer
would open. The renderer reported `mask_generator_absent` for every face, skin, subject or
background mask and `operator_absent` for every retouch operator.

A photographer asked for a retouch section that finds skin, faces, bodies, hair, eyes, mouths and
teeth "easily and accurately".

## Decision

### 1. A new crate below the renderer: `aura-portrait`

A pure function from pixels to faces and nineteen soft regions. It depends on `aura-raw` (colour
maths, invariant 8), `rayon` and `serde`, and on nothing that stores, renders or reaches a model
runtime. `aura-render` and `aura-app` depend on it; it depends on neither.

### 2. Faces come from OpenCV's Haar cascades, evaluated by our own code

A pure-Rust Viola-Jones evaluator, written to follow OpenCV 4's `CascadeClassifier` step for step
(pyramid of resized images, inner-window variance normalisation with the flat-window rejection,
tree descent, `THRESHOLD_EPS`, the two-pixel step and the skip after a first-stage rejection,
`groupRectangles`). The three cascades - frontal `alt2`, profile and eye - are converted to a line
format by a standard-library script and embedded with `include_str!`.

Why a cascade rather than a network: `aura-infer` interprets an ONNX subset with no `Resize` or
`ConvTranspose`, the shipped face models are untrained placeholders, and there is no consented face
data to train one. A cascade needs integral images and comparisons, its trained weights are
published under OpenCV's BSD-style Intel licence, and it is the first detector in the product that
finds a face in a real photograph. The evaluator was compared with OpenCV on 22 photographs: all 80
boxes agreed to within two pixels and two neighbours.

The cascades are not ONNX models and do not go through `aura-models` or `models.lock`; they are
text constants compiled into the binary. `docs/model-cards/haar_cascades.md` is their card anyway,
because "no model card, no model" is about knowing what a model is, not about where it is stored.

### 3. Six scans, one evidence rule

Frontal upright on the detection grid; frontal with the frame turned plus and minus twenty degrees,
frontal on a CLAHE-equalised copy, and profile on the frame and its mirror - those five at half
resolution. Candidates are clustered first and only the best three of each cluster measured. A
candidate is a face only with evidence: a skin-coloured centre (unless the frame has no colour) and
either many agreeing windows or a measured eye and mouth. The extra passes are held to stricter
versions. The equalised pass exists because a luma cascade reads less structure on very dark skin;
it is what finds the darkest painted Monk tone.

### 4. Every region is geometry times evidence, measured against the same person

Facial regions are sized in interocular distances in the face's own rotated frame. Skin is a
Mahalanobis distance from a model fitted to *that person's* cheeks, forehead, nose bridge and chin;
the broad prior only chooses where to sample. Lips are redder or darker than that skin; teeth
brighter and less saturated than those lips; brows darker than that forehead. Hair, body and
background are colour models seeded beside the head and at the chest, held against models of the
ground beside the person, and kept only where connected to their seeds.

There is no skin target anywhere in the crate. Phase 15's rule - a skin target is measured, never
assumed - applies to segmentation.

### 5. The renderer re-derives the regions; it does not read them from the catalog

Phase 14's rule is that a delivered file can be re-created from four values. So `aura-render`
parses the frame itself (`portrait::parse`), from the frame as it arrived and before any slider, and
caches the parse by the analysis canvas's content. `aura_portrait::PARSE_VER` is folded into the
engine string (`...+profiles.N+portrait.1`), so a change to the parse is a change to the engine. A
photographer's own face box travels in the recipe as a mask with `target: "hint:x,y,w,h"`.

A recipe mask of kind `face`, `skin`, `subject`, `background` or `sky` resolves to its region; a
mask whose `target` names a region slug (`hair`, `teeth`, ...) resolves to that region. `brush`
stays a generator this renderer does not have. The recipe schema is unchanged: `Mask.target` was
documented as "interpretation is the mask generator's" from phase 14.

`CpuEngine::new` therefore declares `mask_generators` and `retouch_operators` true. The default
`Capabilities` - what phase 14 shipped - is unchanged, and the streamed path renders a portrait
recipe whole, as it already did a rotation.

### 6. Fourteen operators, none of which reshapes or changes a skin tone

`skin_smooth`, `skin_even`, `blemish_clear`, `under_eye_lift`, `shine_control`, `face_light`,
`eye_brighten`, `iris_enhance`, `sclera_whiten`, `brow_define`, `teeth_whiten`, `lip_enhance`,
`hair_define`, `background_blur`. Each is a tone or colour change in place, in linear light, with
luminance processed as its logarithm and colour re-applied as a ratio. Smoothing keeps the pore
band; evening moves toward the local average of the same skin; blemish clearing leaves very dark
compact marks alone; whitening is bounded. The names are distinct from phase 20's and 21's, whose
operators carry per-mark data a recipe cannot; those stay named as absent rather than being
reinterpreted. Portrait operators run on the interactive path, because they are a handful of
running-sum blurs sized by the face.

### 7. Four IPC commands outside the frozen contract

`analyse_portrait`, `portrait_retouch`, `set_portrait_retouch`, `auto_portrait_retouch`, with their
own DTOs in `aura-app::portrait_commands` (not `contract/ipc`, not `ui/src/ipc/types.ts`). The
automatic pass goes through `schema::merge` with `EditSource::Ai`, so once a person has set the
retouch on a photograph it is refused and says so.

## Consequences

- On a real photograph the product now finds faces and regions and retouches them, in the preview
  and in the export.
- Render hashes change once for every recipe, because the engine string changed.
- `aura-render` gains a dependency. Its colour discipline grep is unaffected: the parse encodes to
  sRGB inside `aura-portrait`, not inside the renderer.
- Nothing here closes phase 06's C1, phase 18's C1/C2 or phase 20's C2: the quality claims are
  measurements on painted faces and an informal look at 22 public photographs, not a study.

## What it does not claim

No accuracy on a real wedding, no per-skin-tone parity on real people, no naturalness study.
`docs/portrait-retouch.md` says the same in the product's voice.
