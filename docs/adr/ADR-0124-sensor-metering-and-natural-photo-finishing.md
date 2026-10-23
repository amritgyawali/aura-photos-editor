# ADR-0124: Meter the rendered RAW and preserve finished photographs

Date: 2026-10-16

Status: Accepted

## Problem

The 50-photo Canon EOS 5D Mark IV run in `F:\to edit by aura` exposed a mismatch:
automatic light analysis used the embedded camera JPEG, while export developed the
sensor data. A bright camera preview could suppress a needed exposure correction and
also classify a RAW as a finished JPEG. The resulting exports were dark and flat.
Four full-length portraits received no automatic retouch because their small faces
were discarded or missed at the detector's first crop scale. The progress summary
nevertheless described every edited image as retouched.

The default adaptive creative grade also changed already finished JPEG/PNG colours
when the requested action was a natural correction. Full-size Bayer reconstruction
used bilinear interpolation, losing fine detail unnecessarily.

## Decision

* When an actual demosaiced proxy is available, derive the small analysis image from
  that proxy in linear light. Both automatic editing paths use it for metering,
  white balance and RAW/finished-file classification. Keep the existing fallback
  when sensor decoding is unavailable. Embedded JPEGs remain useful for the library.
* Keep measured light, colour and local corrections for JPEG/PNG, but do not add the
  optional creative adaptive grade by default. Explicit profiles and reference
  matching retain their separate paths. RAW development keeps its adaptive grade.
* Allow smaller confident face boxes and inspect closer overlapping crops when the
  first crop scale finds no faces. Keep the 0.85 confidence threshold, landmark
  geometry checks, overlap suppression and existing retouch protection masks.
* Use the 5x5 gradient-corrected Bayer filters of Malvar, He and Cutler for full-size
  reconstruction. Preserve measured samples, all four Bayer phases and global tile
  coordinates. Keep bounded bilinear reconstruction at the physical two-pixel
  sensor boundary, and the existing X-Trans/unknown-layout fallback.
* Version full-size reconstruction independently (`DEMOSAIC_VER = 2`) and include
  it in the render engine identity. The frozen analysis/model proxy pipeline and
  its version remain unchanged.
* Report actual retouch coverage from saved recipes in the folder-run notes. The
  UI summary counts edits; it does not infer retouch success from edit success.
* Read original filenames, source paths and textual capture times from the actual
  catalogue schema. Propagate frame-query errors instead of silently returning
  empty metadata. Capture dates remain stable in UTC.

The reconstruction filters are described in the authors' [ICASSP 2004 paper](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/Demosaicing_ICASSP04.pdf).
The paper's reported performance is not a measurement of this application.

## Validation and limits

The audit lives in `.work-checks/quality-fix-2026-10-16`. Regression checks cover
sensor metering despite conflicting embedded DNG previews, measured Bayer samples,
constant colours in every Bayer phase, synthetic luminance-detail reconstruction,
exact tiled/whole reconstruction, source preservation and catalogue export names.
The native application was exercised on fresh working copies of all 50 RAWs, with
full-resolution JPEG read-back, original-file hashes, retouch pixel comparisons,
history restoration and visual contact sheets. A separate comparison uses 33 local
subjects under seven controlled conditions (231 cases), against the previous build.
The audit report records the final counts and measurements.

This is evidence for these fixtures and photographs, not a promise of the best edit
for every image. JPEG reference distance is a useful regression measure, not a
universal aesthetic score. Low-confidence faces remain untreated automatically;
severely clipped data and unusual creative lighting still need a photographer's
judgement. Existing manual recipe fields retain the normal merge protections.
