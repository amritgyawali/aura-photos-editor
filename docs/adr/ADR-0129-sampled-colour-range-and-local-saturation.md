# ADR-0129: Sampled colour range and local saturation

Date: 2026-10-19

Status: Implemented

## Decision

Extend the optional native retouch selection with a sampled colour range. Store
an image-relative sample coordinate, tolerance, falloff and lightness sensitivity.
Read the sample from the immutable image immediately before this operation, so
the mask follows upstream edits. It is an image-specific reference, not a fixed
display colour and not a learned object classifier. Keep it separate from clone
sources, sampled skin and zoom centres.

Measure the range in D65 CIE Lab using the existing working-space conversion.
Euclidean Lab distance is a fast mask metric, not a photographic quality score.
Weight only the lightness difference by the user's sensitivity. Smoothly taper
coverage outside the tolerance. Multiply by the existing geometric/painted
selection and luminance range, then by any attached segmentation matte.
Shape inversion happens before colour/brightness refinement; it must not turn
excluded colours into selected pixels. Preview and delivery share this logic.

Add explicit manual `color_saturation` with separate signed saturation (`warmth`)
and vibrance (`tint`) controls. Adjust chroma around working luminance; vibrance
has progressively less effect on already saturated pixels. Limit the chroma gain
uniformly to avoid creating negative working channels. Preserve neutral pixels
and luminance; do not independently clip channels. No inferred skin protection.

No dependency, model, migration or frozen-contract change. Presets omit the
image-specific colour sample. Automatic portrait planning does not add this tool.
Older binaries reject the new selection field/tool: remove affected operations
with the current binary before downgrading.

## Verification

Check hue discrimination, lightness sensitivity, feathered falloff, geometric
and brightness intersections, inversion order, saved matte restriction, source
independence, serialization, malformed inputs, neutral/gamut/luminance handling,
native history/export and actual desktop authoring.
