// PHASE-19. Applying a local light plan to the pixels.
//
// Everything in this file works in LINEAR Rec.2020 and nothing in it encodes. Invariant 8,
// and `crates/aura-render/tests/colour_discipline.rs` is a grep as a test, so the second
// module to start baking tone fails the build. The one `pow` here is inside
// `luminosity_mask.wgsl`'s weighting, where it produces a *weight* rather than a colour.
//
// FIVE OPERATIONS, in the order `LocalOp::PRIORITY` names them. Each takes the mask alpha
// already scaled by `mask_strength_scale` and by the governor's own allowance, so nothing here
// re-derives a strength: by the time a pixel reaches this file the decision has been made and
// the only question left is how much of it this pixel receives.
//
// NO ATOMICS. Nothing writes to a location it did not compute.

struct LocalParams {
    // Face lighting: an exposure in stops, a shadow term and a negative highlight term, the
    // three fields a recipe mask carries.
    face_exposure: f32,
    face_shadows: f32,
    face_highlights: f32,
    // Subject enhancement.
    subject_clarity: f32,
    subject_contrast: f32,
    // Background balance. Both are zero or negative: this phase calms a background and never
    // enriches one, because enriching a background is a grade and grades are phase 16's.
    background_exposure: f32,
    background_saturation: f32,
    // Shine. A LUMINANCE reduction and nothing else - there is no radius here, no smoothing
    // strength and no texture field, which is what keeps the obvious wrong fix an ADR away
    // rather than a refactor away.
    shine_exposure: f32,
    // The mask's own quality, already combined.
    mask_scale: f32,
    feather: f32,
};

@group(2) @binding(0) var<uniform> local_params: LocalParams;
@group(2) @binding(1) var<storage, read> mask_alpha: array<f32>;
@group(2) @binding(2) var<storage, read> shaping_low: array<f32>;
@group(2) @binding(3) var<storage, read> shaping_mid: array<f32>;

// How many `shadows` units one stop of shadow-region lift is worth. Measured against the
// reference path rather than assumed; `tests/shader_parity.rs` holds the two to the same
// number, because a renderer change that moved it would silently change how far every face in
// the product gets lifted.
const SHADOWS_PER_EV: f32 = 60.0;

// How many `highlights` units of restraint one stop of lift buys. Negative in use.
const HIGHLIGHTS_PER_EV: f32 = 18.0;

// The shaping grids are stored in units of 1/200 stop.
const SHAPING_UNIT_EV: f32 = 0.005;

// Section 6.1, applied. The exposure term is flat inside the mask; the shadow term is weighted
// by the luminosity mask so shadows move and highlights do not; the highlight term is weighted
// by the opposite curve and is never positive.
fn apply_face_light(rgb: vec3<f32>, alpha: f32) -> vec3<f32> {
    let a = alpha * local_params.mask_scale;
    if (a <= 0.0) { return rgb; }
    var out = rgb * exp2(local_params.face_exposure * a);
    let shadow_ev = (local_params.face_shadows / SHADOWS_PER_EV) * luminosity_weight(out) * a;
    out = out * exp2(shadow_ev);
    let highlight_ev = (local_params.face_highlights / HIGHLIGHTS_PER_EV) * highlight_weight(out) * a;
    return out * exp2(min(highlight_ev, 0.0));
}

// Section 6.2's subject half. Local contrast around a blurred neighbourhood, which is what
// clarity is: pixels move apart around a local mean without the mean moving, so the frame's
// own luminance is unchanged and the pairing's guarantee survives.
fn apply_subject(rgb: vec3<f32>, local_mean: vec3<f32>, alpha: f32) -> vec3<f32> {
    let a = alpha * local_params.mask_scale;
    if (a <= 0.0) { return rgb; }
    let clarity = 1.0 + (local_params.subject_clarity / 100.0) * a;
    let contrast = 1.0 + (local_params.subject_contrast / 100.0) * a;
    let detail = (rgb - local_mean) * clarity;
    let body = (local_mean - vec3<f32>(0.18)) * contrast + vec3<f32>(0.18);
    return max(body + detail, vec3<f32>(0.0));
}

// Section 6.2's background half. Luminance and chroma respond to their own triggers, and the
// saturation term is a mix toward the pixel's own grey rather than a multiply, so a background
// that is calmed does not shift hue on the way down.
fn apply_background(rgb: vec3<f32>, alpha: f32) -> vec3<f32> {
    let a = feathered_alpha(alpha, local_params.feather) * local_params.mask_scale;
    if (a <= 0.0) { return rgb; }
    var out = rgb * exp2(min(local_params.background_exposure, 0.0) * a);
    let grey = vec3<f32>(local_luma(out));
    let saturation = 1.0 + (min(local_params.background_saturation, 0.0) / 100.0) * a;
    return mix(grey, out, max(saturation, 0.0));
}

// Section 6.3's shine control. A luminance-only reduction: the pixel's chromaticity is held
// and only its brightness moves, so the texture under the sheen survives.
fn apply_shine(rgb: vec3<f32>, alpha: f32) -> vec3<f32> {
    let a = alpha * local_params.mask_scale;
    if (a <= 0.0 || local_params.shine_exposure >= 0.0) { return rgb; }
    let before = local_luma(rgb);
    if (before <= 0.0) { return rgb; }
    let after = before * exp2(local_params.shine_exposure * a);
    return rgb * (after / before);
}

// Section 6.3's dodge and burn. The low band is shaped and the mid band is evened; the high
// band is not addressed by either, because `freq_sep.wgsl` never produced it.
fn apply_shaping(rgb: vec3<f32>, index: u32, alpha: f32) -> vec3<f32> {
    let a = alpha * local_params.mask_scale;
    if (a <= 0.0) { return rgb; }
    let low = shaping_low[index] * SHAPING_UNIT_EV;
    let mid = shaping_mid[index] * SHAPING_UNIT_EV;
    return rgb * exp2((low + mid) * a);
}

// The whole plan, in priority order. One function so a caller cannot apply the background
// reduction without the subject enhancement it was solved with - section 6.2's rule, kept by
// the shape of the code rather than by review.
fn apply_local(rgb: vec3<f32>, local_mean: vec3<f32>, index: u32, alpha: f32) -> vec3<f32> {
    var out = apply_face_light(rgb, alpha);
    out = apply_subject(out, local_mean, alpha);
    out = apply_background(out, 1.0 - alpha);
    out = apply_shine(out, alpha);
    out = apply_shaping(out, index, alpha);
    return out;
}
