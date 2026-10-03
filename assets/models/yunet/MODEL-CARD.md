# Bundled portrait detector

- Upstream: [OpenCV Zoo YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet).
- File: `face_detection_yunet_2023mar.onnx`, MIT license (see `LICENSE`).
- SHA256: `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`.
- BLAKE3 (checked before native compilation): `3d5938c4cd5a02dc416f1cd1f7fc1f662a22adc370477112c871954587e63431`.
- AURA pipeline: `yunet-2023mar-aura320-v1`, CPU FP32, original model weights.
- Input: oriented sRGB, half-pixel bilinear resize preserving aspect ratio, long
  edge 320, BGR float 0–255, NCHW, zero padding right/bottom. The fully convolutional
  graph input declaration is adapted from 640 to 320 at load time.
- Output: normalized face boxes and five landmarks, score threshold 0.85, IoU NMS
  0.3, minimum model-space face 20×24 pixels, at most 16 faces. No identity data,
  embeddings or demographic labels. Face probability is not retouch quality.
- Limitations: small, cropped, highly rotated, occluded and profile faces may be
  skipped. Skin regions are geometric and sample-relative, not neural segmentation.
  Five real test portraits are integration coverage, not a demographic accuracy study.
- `AURA_DISABLE_AUTO_PORTRAIT=1` disables this analysis; measured global correction
  and all manual tools remain available.

The model is bundled so inference works offline. No external Python/OpenCV runtime
is needed. Native raw outputs were compared with OpenCV DNN on a real portrait;
maximum absolute difference across all twelve outputs was below 0.000007.
