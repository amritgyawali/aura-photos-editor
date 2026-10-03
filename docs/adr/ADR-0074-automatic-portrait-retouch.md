# ADR-0074: Offline, editable automatic portrait retouch

Status: accepted, 2026-09-30.

The existing face pack is a fixture model and cannot guide production portrait
retouch. Bundle the MIT-licensed OpenCV YuNet 2023mar weights, verify their BLAKE3
digest, and run them through AURA's native CPU ONNX interpreter. This separate,
versioned portrait pack does not replace the face identity pipeline or claim GPU
parity. No Python, network service, account, or optional download is required.

The fully convolutional graph uses a fixed 320-square input with aspect-preserving
RGB-to-BGR preprocessing and right/bottom padding. The weights are unchanged;
only the graph input declaration is adapted. Add and test the ONNX nearest,
asymmetric, floor Resize subset; refuse unsupported coordinate modes. Verify
native model outputs against OpenCV and detect faces on five real portraits.
Sources: https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet,
https://onnx.ai/onnx/operators/onnx__Resize.html.

Detected boxes and five landmarks guide conservative cheek/forehead skin regions.
Sampled skin affinity and edge protection refine those regions without assuming
one complexion. This is landmark-guided skin targeting, not a trained semantic
skin segmentation model. Low-confidence, tiny or unusable faces are skipped.
Restrained smoothing, tone uniformity and dodge/burn become ordinary editable
retouch operations; permanent marks are not automatically removed.

Auto enhance measures exposure and adds portrait retouch in one history step.
Retouch also exposes an automatic portrait action. Collection automation includes
the same portrait action when applying a chosen profile. Originals remain read
only. AI merge protects manually authored retouch stacks and global controls;
repeat runs replace stable automatic IDs rather than accumulating effects.
Before saving, compare canonical recipe hashes as well as merge changes: stored
coordinates are rounded, so comparing raw inference floats otherwise creates an
identical history entry after every reload. A skipped save returns the stored
recipe, including its actual provenance.
Fresh thumbnail/proxy requests also decode their encoded JPEG before publishing
pixels. Previously a first request returned uncompressed pixels while a restart
returned the cached JPEG, subtly changing previews and ML input for an unchanged
original. Both paths now use identical pixels; the lossless linear companion and
full-resolution export decode are unaffected. Tests cover both preview tiers.
Persist a versioned report with face counts, confidence, reasons and skips, and
show completion/failure feedback. Undo/redo and existing per-operation editing
remain the route to manual control. An explicit environment kill switch disables
the bundled portrait analysis without disabling measured global enhancement.

The CPU runtime is bounded to one small input and serialized inference. Model
failure must be reported, never passed off as successful detection. Face detection
confidence is not a probability that a photographic correction is desirable.
Validation includes no-face input, multiple faces, aspect ratio, deterministic
reruns, manual protection, and actual save/undo/edit/render behavior.
