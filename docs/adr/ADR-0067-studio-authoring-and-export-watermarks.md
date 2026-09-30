# ADR-0067: Studio authoring tools and delivery watermarks

Status: accepted for this implementation batch, 2026-09-30.

## Context

The user requested implementation of the 100-feature comparison roadmap. This batch
adds directly usable tools on the current recipe, history and export architecture.
The comparison is a scope inventory, not a claim of full product parity.

## Decision

Keep frozen recipe and IPC contracts unchanged. Add optional settings-group selection
to the existing non-contract sync input, and additive native commands for a neutral
white-balance sample and watermarked export. Existing callers retain their behavior.
Reject empty/unknown explicit groups; deduplicate targets and check collection membership.
Copy lens correction switches while preserving each target's optical profile and coefficients.
Masks, cleanup and identity edits are never transferred by settings sync.

White balance samples a 7x7 patch of the oriented original in linear Rec.2020, through
the existing frame source. Reject unusable samples. Fit the existing renderer's gains
over its supported temperature/tint range, then merge both values as one manual edit.
The picker protects both explicitly chosen controls, including an unchanged fitted value;
ordinary settings sync retains the existing changed-fields-only protection semantics.
No inference model, network service, new dependency, or fabricated segmentation is used.

Clipping warnings mark near-black and channel-clipped pixels of the display preview.
They are diagnostics, not sensor saturation measurements, and never affect exports.
Named snapshots use the existing durable snapshot/history store.

Watermark text uses installed browser fonts; PNG logos are decoded locally. The UI
submits bounded sRGB RGBA pixels, placement and opacity. Native export validates the
payload before opening a job, transforms its linear RGB to the chosen output primaries,
resamples premultiplied alpha, and blends after resizing/sharpening. Both 8-bit and 16-bit
outputs retain their depth. The transformed render hash includes the graphic and settings;
the export manifest records the watermark engine version. Existing exports remain identical
when no watermark is requested. Watermarks do not mutate recipes or originals.
Archive the exact graphic/settings in a content-addressed JSON file beside the delivery;
record its relative name and read-back BLAKE3 in the manifest's engine metadata. Reuse an
identical archive, refuse a mismatched existing file, and never overwrite it. This makes
the extra watermark hash input recoverable even after closing the editor.

## Verification

Meaningful checks cover pixel overlays, letterbox coordinates, empty selections, snapshot
command routing, synthetic neutral-patch recovery through the actual renderer, alpha
compositing in linear light, output color spaces, bit depth and invalid payloads. The native
portrait workflow checks real commands and export read-back on five public portrait photos.
Results and remaining roadmap scope are recorded in the implementation report.
