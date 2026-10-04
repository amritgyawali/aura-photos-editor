# Model card - OpenCV Haar cascades (frontal `alt2`, profile, eye)

Three boosted cascades, embedded as text in `crates/aura-portrait/data/` and evaluated by
`crates/aura-portrait/src/cascade.rs`. They are not ONNX models, are not in `models.lock`, and do
not run through `aura-infer`; this card exists because knowing what a model is does not depend on
where it is stored. See `docs/adr/ADR-0065-measured-portrait-parsing-and-portrait-retouch.md`.

## Purpose

Find faces - frontal, tilted up to about twenty degrees, and turned to one side - in a real
photograph, and find eyes inside a face already found. Everything else the portrait parse measures
(mouth, brows, teeth, skin, hair, body) is measured from the pixels relative to these faces rather
than predicted.

## Architecture

Viola-Jones boosted cascades over Haar-like rectangle features on a 20x20 window, evaluated on an
image pyramid with variance normalisation:

| Cascade | Stages | Weak classifiers | Features | Origin |
|---|---|---|---|---|
| `frontalface_alt2` | 20 | 1,047 two-split trees | 2,094 | Rainer Lienhart, gentle AdaBoost |
| `profileface` | 26 | 2,609 stumps | 2,609 | David Bradley, Princeton University |
| `eye` | 24 | 1,066 stumps | 1,066 | Shameem Hameed |

No tilted features. The evaluator follows OpenCV 4's `CascadeClassifier` step for step; the one
deliberate difference is the resampler, which is bilinear and centre-aligned like OpenCV's but not
bit-identical to it.

## Training data

Trained by their authors in the early 2000s on datasets this repository does not have and cannot
describe in detail. That is the most important limitation on this card: **nothing is known here
about the demographic balance of the training data**, and face detectors of this generation have
been reported to have lower recall on darker skin and on faces that are not frontal.

## Latency

Measured on a four-core development container in release, on 22 photographs at a 640 px long
edge, all six scans plus measurement: 45-190 ms per frame for faces, 110-540 ms for the whole
parse (24 faces at the top of that range). The detection grid is at most 720 px on the long edge.
Cached per photograph by content after the first parse.

## Quality gate

- `crates/aura-portrait/src/cascade.rs` tests: the three cascades parse, flat and noise frames have
  no faces, grouping matches `groupRectangles`.
- `crates/aura-portrait/src/parse.rs` tests: the painted portrait is found with its eyes within 15 %
  and its mouth within 20 % of an interocular distance; **every one of the ten Monk Skin Tone
  swatches is found** and parsed with a face IoU spread under 0.25; a tilted head is found with its
  tilt.
- `crates/aura-portrait/tests/local_eval.rs` (ignored; run on a folder of your own photographs):
  compared with OpenCV's boxes on the same grey frames - 80 of 80 boxes on 22 photographs agreed to
  within two pixels and two neighbours.

## Known failure modes

- Strongly turned faces, faces under about 3 % of the long edge, and heavily occluded faces are
  missed. A photographer can draw any face that is missed.
- Without the evidence rule the extra passes find wallpaper, clocks, badges and flags; with it, none
  of those were accepted on the evaluation set, but a face-like pattern with skin-coloured centre and
  dark eye-like blobs can still be.
- The darkest painted Monk tone is found only by the locally equalised pass. On real dark skin in
  low light the cascades may miss faces the painted test cannot predict.
- No real-photograph fairness study has been done.

## Fallback

A face the cascades miss can be drawn; it is carried in the recipe as a hint and parsed exactly
like a detected face. A frame with no face gets no skin, eye, mouth or hair regions - never a guess
at them - and every operator that needs a face says so in the render notes.
