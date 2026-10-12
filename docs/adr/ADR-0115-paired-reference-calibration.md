# ADR-0115: Calibrate one finished version of the same photo

Status: Accepted
Date: 2026-10-12

The existing reference-folder workflow learns an aggregate look from at least
eight different photos. A public before/after comparison supplies one aligned
pair instead. Repeating that after image eight times would not add evidence,
and copying its pixels into an export would not implement AURA editing.

Add a separate `match_paired_reference` command and studio action. Keep the
saved healing stack and manual settings, search a bounded set of nine native
tone/color parameters using coarse-to-fine rendered candidates, and require
lower RGB error again at full export resolution before saving. Candidate
geometry, restoration, retouch and unknown recipe fields remain unchanged.
The target only supplies scoring pixels and a content hash, never recipe
bitmap data or an alternate rendering path.

The workflow is local, cancellable, limited to 16 MP / 64 MB, and uses the
same image-work gate as export and automatic retouch planning. Dimensions
must match, and a signed luminance-gradient heuristic rejects an unaligned
scene. Save accepted parameters through normal history with an atomic
baseline-hash condition. Cancellation, failure, no improvement and a newer
manual edit leave the measured baseline untouched.

Missing JPEG/PNG catalogue dimensions are verified by a bounded original decode
and content hash instead of treating missing EXIF as a failed photo. Native
Windows validation uses the actual owned file picker, without replacing
protected Tauri IPC or weakening the Content Security Policy.

Whole-image error alone rewarded a background improvement while worsening two
faces in the public three-person pair. The fitter therefore prioritizes the
detected face regions, refuses worse face candidates, and checks every face
again at full resolution before saving. Report both regional and whole-image
errors; neither is a perceptual-quality or blemish-removal certification.

Report mean RGB byte error and strict equality independently of blemish
removal or perceptual quality. The heuristic and compressed public examples
do not establish shared camera provenance, recovery of proprietary models,
complete defect removal, or feature parity. See the
[workflow and reproduction instructions](../paired-reference-matching.md).
