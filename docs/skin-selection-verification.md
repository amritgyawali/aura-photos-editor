# Skin selection regression checks

This follow-up to ADR-0077 fixes selection ownership, manual mask preservation and independent
finishing controls. It uses the existing bundled MediaPipe Selfie Multiclass weights; only
the preprocessing/ownership policy changes to `mediapipe-selfie-multiclass-256-aura-v2`.

## Reproduce

```powershell
cargo test -p aura-vision --lib skin::
cargo test -p aura-recipe --lib retouch_tools::
cargo test -p aura-render --lib retouch_
cargo test -p aura-app --lib
cd ui
npm run build
npm test -- --maxWorkers=2 --minWorkers=1
```

The regression cases cover touching people, edge pixels and panoramas, body retouch without
a face sample, teeth-only finishing, disabled manual overrides, immutable mask snapshots,
missing/corrupt masks, reusable per-person skin selection and removing the AI restriction
when selecting the entire photo.

On 2026-10-04: 79 application unit tests, six detector tests, two recipe-matte tests and two
renderer-matte tests passed. The full UI suite passed 566 tests; the additional full-photo
selection regression then passed with all 19 tests in the two affected component suites.
The production UI build, banned-code check and IPC consistency check (285 commands) passed.
Clippy remains blocked by five existing `indexing_slicing` errors on the unchanged expression
at `crates/aura-app/src/smart_edit.rs:315`; no new errors were reported in this change.

## Real photographs

Five existing local portrait fixtures were used (Pexels IDs 1239291, 220453, 2379004, 774909,
8386841; at most 640 pixels on the long edge). They include beards, glasses and light-to-dark
skin. The segmenter returned a face and body matte on all five, with one inference pass each.
Observed detection time was 3.82–4.23 seconds per photo in the development build; this is a
spot measurement, not a comparative performance benchmark.

```powershell
$env:AURA_SKIN_PHOTOS = 'absolute/path/to/NAME_WxH.rgb/files'
cargo test -p aura-vision --test skin_photos -- --ignored --nocapture
$env:AURA_RETOUCH_PRESETS = 'natural'
cargo test -p aura-app --test auto_retouch_photos -- --ignored --nocapture
python ml/models/skin/to_raw.py --png $env:AURA_SKIN_PHOTOS
```

Both checks reject an empty fixture set, ignore their generated outputs on subsequent runs,
and accept folder names containing dots. Retouch checks convert sRGB into linear Rec.2020
before processing, validate saved recipes and assert finite output and unchanged unselected
pixels. Review the generated `*.selection.png` and `*.natural.after.png` beside each source.

## Limits

These are regression checks and visual spot-checks, not a demographic accuracy study or a
comparison proving parity with Imagen, Retouch4me or SkinFiner. Body ownership uses proximity
to detected faces. Overlapping limbs, missing faces, very small subjects and skin-colored
clothing can still need manual correction. The manual selection tools and mask preview remain
part of the workflow.
