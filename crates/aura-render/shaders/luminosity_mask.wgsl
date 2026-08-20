// PHASE-19. The luminosity mask, and the mask-quality gate in front of it.
//
// This file declares no compute entry point on purpose. A luminosity mask is not a stage; it
// is what `stage_masks` multiplies a generated mask's alpha by before it applies anything,
// and phases 20 to 22 will each want the same weighting from the same place. Putting it in
// `spatial.wgsl` beside the geometric masks would have made it look like a fifth mask kind.
//
// THE ONE IDEA. A flat exposure lift inside a face mask moves every tone by the same number
// of stops: the shadow side of the nose and the lit side of the forehead rise together, the
// ratio between them is preserved, and the result is a face with the same contrast and more
// brightness - which is what "glowing" means. What a retoucher does is lift the shadow side
// and leave the lit side, which reduces the ratio and reads as better lighting rather than as
// more light. `luminosity_weight` is that curve.
//
// The Rust reference is `aura_render::local`, and `tests/shader_parity.rs` holds the two to
// the same constants. The decision side is `aura_brain_photo::local::luminosity`, which
// splits a lift into the three fields a mask carries; this is the *application* side, which
// weights them per pixel.
//
// NO ATOMICS, and nothing here writes to a location it did not compute.

// The luminance a face is pivoted around. Faces are not mid-grey: a well-exposed face sits
// between 0.30 and 0.60 in perceptual units, so a curve pivoted at 0.5 would treat a
// correctly lit ceremony face as a shadow and put the whole lift where the highlights are.
const FACE_PIVOT: f32 = 0.48;

// Below this mask confidence an operation does not run at all.
const MIN_MASK_CONFIDENCE: f32 = 0.35;

// At or above this mask confidence an operation runs at its full scene strength.
const FULL_MASK_CONFIDENCE: f32 = 0.80;

// Rec.2020 luminance, the working space's own coefficients.
fn local_luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.262700, 0.677998, 0.059302));
}

// The perceptual encoding the decision side measures in. 2.2 rather than the exact sRGB
// piecewise curve: the difference is below one part in a hundred above the toe and every
// number this weights is a mean over thousands of pixels.
fn local_encode(linear: f32) -> f32 {
    return pow(max(linear, 0.0), 1.0 / 2.2);
}

// How much of a lift this pixel should receive, 0..1.
//
// One at black, zero at and above the pivot, smooth in between. Smoothstep rather than a
// linear ramp because the derivative is what a photographer sees: two pixels a hundredth of a
// unit apart getting noticeably different treatments looks like a bug even when both are
// defensible.
fn luminosity_weight(rgb: vec3<f32>) -> f32 {
    let encoded = local_encode(local_luma(rgb));
    let x = clamp((FACE_PIVOT - encoded) / FACE_PIVOT, 0.0, 1.0);
    return x * x * (3.0 - 2.0 * x);
}

// The highlight side of the same curve: one at and above the pivot, zero at black.
//
// Used by the negative highlight term a face lift always carries. It is deliberately NOT
// `1.0 - luminosity_weight`: the shadow curve reaches zero at the pivot and the highlight
// curve starts there, so between black and the pivot they do not sum to one and the two terms
// cannot both act on the same pixel at full strength.
fn highlight_weight(rgb: vec3<f32>) -> f32 {
    let encoded = local_encode(local_luma(rgb));
    let x = clamp((encoded - FACE_PIVOT) / (1.0 - FACE_PIVOT), 0.0, 1.0);
    return x * x * (3.0 - 2.0 * x);
}

// Section 6.4's rule, in the one place the GPU path can read it: a poor mask produces a gentle
// edit instead of an artefact, and a hopeless one produces none.
//
// The confidence ramps linearly between the floor and full, and the result is multiplied by
// the edge quality. Two numbers rather than one because they fail differently: a mask can be
// confidently the right region and have a terrible boundary - hair against a bright window is
// the standard case - and one number would have to pick which failure to hide.
fn mask_strength_scale(confidence: f32, edge_quality: f32) -> f32 {
    if (confidence < MIN_MASK_CONFIDENCE) { return 0.0; }
    let span = FULL_MASK_CONFIDENCE - MIN_MASK_CONFIDENCE;
    let ramp = clamp((confidence - MIN_MASK_CONFIDENCE) / span, 0.0, 1.0);
    return clamp(ramp * clamp(edge_quality, 0.0, 1.0), 0.0, 1.0);
}

// Soften a generated mask's alpha by a feather, in the same shape the geometric masks use.
//
// A wider feather for a worse edge, which is the rule that surprises people: the visible
// artefact is the *gradient* across the boundary, which is the edit's magnitude over the
// transition's width. Reducing the magnitude and widening the transition both help; widening
// it is free.
fn feathered_alpha(alpha: f32, feather: f32) -> f32 {
    let f = clamp(feather, 0.01, 1.0);
    return smoothstep(0.5 - f * 0.5, 0.5 + f * 0.5, alpha);
}
