# Bundled person and skin segmenter

- Upstream: Google MediaPipe **Selfie Multiclass** image segmenter
  (`selfie_multiclass_256x256.tflite`, float32), from
  `https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_multiclass_256x256/float32/latest/selfie_multiclass_256x256.tflite`.
  Apache License 2.0 (see `LICENSE`).
- Source TFLite SHA256: `c6748b1253a99067ef71f7e26ca71096cd449baefa8f101900ea23016507e0e0`.
- Converted file: `selfie_multiclass_256x256.onnx`, ONNX opset 13, produced by
  `ml/models/skin/convert_selfie_multiclass.py` (weights unchanged, layout converted from NHWC
  to NCHW where an operator needs it).
  - SHA256: `014bebaf0aedaf951655d4563216359d3da311c6997cfb1d0b40fb7ababf40c8`.
  - BLAKE3 (checked before compilation): `10eee962bb85d9f5d0b292376f595ce70d810becfcef17feeb4762e62d8a7754`.
- AURA pipeline: `mediapipe-selfie-multiclass-256-aura-v1` (`aura_vision::skin`), CPU FP32 on
  aura-infer's pure-Rust interpreter. No network access, no Python at run time.
- Input: 1x3x256x256 RGB, `(v - 127.5) / 127.5`, area-sampled from a square region; outside
  the frame is black. The whole frame is letterboxed; each detected face whose head and torso
  are under 40 model pixels also gets a crop pass blended into the full-frame answer.
- Output: 1x6x256x256 logits, softmaxed by AURA, in this order: background, hair, body skin,
  face skin, clothes, other (accessories). AURA keeps body skin, face skin, hair, clothes and
  background, refines each with a guided filter against the photo's own luminance, gates face
  skin by the *same person's* measured skin colour and brightness (beard, brows, lips and eyes
  drop out), and assigns connected regions to the nearest detected face.
- Parity: the interpreter's output on a fixed synthetic input matches onnxruntime 1.28 on the
  same graph within 2e-3 on every sampled logit and 1e-3 on every class mean
  (`skin::tests::interpreter_matches_onnxruntime_on_a_fixed_pattern`).
- Speed: about 1.7 s per pass on the development laptop with aura-infer at `opt-level = 3`
  (4.2 GMAC per pass). A portrait needs one pass; a group photo one per small face, at most
  seven.
- What it is not: it produces no identity, age, gender or ethnicity information and is not a
  face recogniser. It does not decide anything about a person's appearance; it only says which
  pixels are skin, hair or clothing so a retouch the photographer asked for stays on them.
- Limitations: a person seen from behind, heavily occluded, very small (under about 20 px of
  face) or wearing skin-coloured clothing can be mis-segmented; the planner then falls back on
  landmark geometry and colour sampling and says so in the report. Twenty-two real
  photographs (Pexels, light to deep complexions, beards, glasses, groups) were checked by eye
  during development; this is integration coverage, not a demographic accuracy study.
- `AURA_DISABLE_SKIN_SEGMENTATION=1` disables the segmenter on a device; the "AI skin
  detection" setting disables it per pass.
