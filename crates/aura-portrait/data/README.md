# Cascade data

Three of OpenCV's trained Haar cascades, from the `opencv/opencv` repository's `data/haarcascades`
directory, converted from OpenCV's XML storage format with `convert_opencv_cascade.py` (standard
library only):

```text
python3 convert_opencv_cascade.py haarcascade_frontalface_alt2.xml frontalface_alt2.cascade
python3 convert_opencv_cascade.py haarcascade_profileface.xml      profileface.cascade
python3 convert_opencv_cascade.py haarcascade_eye.xml              eye.cascade
```

Numbers are copied as text, never re-printed, so the converted files carry exactly the values
OpenCV reads. Each file keeps the original licence (the Intel License Agreement for the Open Source
Computer Vision Library, a three-clause BSD-style licence) verbatim in its header; it is also in
`THIRD-PARTY-NOTICES.md`. The card is `docs/model-cards/haar_cascades.md`.
