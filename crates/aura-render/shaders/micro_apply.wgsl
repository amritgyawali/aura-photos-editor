// PHASE-21. The micro-retouch operators: flyaway, teeth, sclera and iris, through phase 18's
// regions.
//
// A LIBRARY, not a stage. It declares no `fn stage_` entry point, for the reason
// `inpaint_patch.wgsl` and `freq_bands.wgsl` declare none: the micro operations run inside the
// retouch stage that phase 20 already owns, one dispatch per operation, and a second stage in
// `graph::ORDER` would be a change to a frozen contract for no behavioural reason.
//
// The GPU half of `apply` in `crates/aura-render/src/micro.rs`. The order is fixed there and is
// fixed here - glare, flyaway, clothing, teeth, eyes - because a specular sheet over an eye sets
// the specular exclusion the eye operators read, and an eye under a repaired sheet is an eye that
// can then be corrected normally.
//
// Everything here is LINEAR and nothing encodes. Invariant 8. The one `pow` is `exp2`, which
// turns a count of stops into a multiplier and never touches an encoded value.
//
// THE THREE THINGS THIS FILE DOES NOT HAVE
//
// There is no target colour anywhere in it. The teeth and sclera operators reduce a
// chromaticity's distance to a locus centred on the frame's OWN measured neutral, and a
// chromaticity already inside the locus is returned untouched. There is no path through
// `micro_pull_toward_locus` that moves a colour toward the locus centre.
//
// There is no luminance term in the sclera operator and no colour term in the iris operator.
// Sclera redness reduction re-normalises onto the input's own luminance before it writes, so no
// value of any parameter brightens a sclera; iris clarity is a scale applied equally to all three
// channels, so no value of any parameter changes somebody's eye colour.
//
// There is no displacement, scale or landmark move. Section 11 of docs/plan/CLAUDE.md forbids
// body reshaping, face swapping and eye replacement permanently, and there is nowhere in this
// file to express one.
//
// NO ATOMICS.

struct MicroParams {
    width: u32,
    height: u32,
    // The window this dispatch acts in, in pixels.
    origin: vec2<u32>,
    extent: vec2<u32>,
    // Flyaway: how much of a strand's own contrast against the background is removed.
    // `MAX_FLYAWAY_STRENGTH` bounds it in the contract.
    flyaway_strength: f32,
    // Teeth: the two capped magnitudes from the plan.
    teeth_luma_ev: f32,
    teeth_yellow: f32,
    // Eyes: the two capped magnitudes from the plan.
    sclera_share: f32,
    iris_clarity: f32,
    // The frame's own neutral in CIE u'v', from phase 15. `neutral_valid` is zero when no
    // illuminant was estimated, and the colour half of every operator is then skipped entirely -
    // a locus with no origin describes nothing.
    neutral: vec2<f32>,
    neutral_valid: u32,
    // The locus, as an offset from `neutral` and a radius, all in u'v'.
    locus_centre: vec2<f32>,
    locus_radius: f32,
    // The brightest non-specular skin luminance on this face, measured on the host. The teeth
    // lift is clamped against it, which is what makes "no fluorescent teeth" a comparison against
    // the subject rather than against a number. Zero when there is no skin region.
    brightest_skin: f32,
};

@group(6) @binding(0) var<uniform> micro_params: MicroParams;
// Phase 18's regions, one plane each, `width * height` long. A plane of zeros is a region no
// operator may act through: there is no rectangle fallback here, because a rectangle's edge does
// not follow a person.
@group(6) @binding(1) var<storage, read> micro_hair: array<f32>;
@group(6) @binding(2) var<storage, read> micro_teeth: array<f32>;
@group(6) @binding(3) var<storage, read> micro_sclera: array<f32>;
@group(6) @binding(4) var<storage, read> micro_iris: array<f32>;
// The background estimate the host produced for a flyaway window, and the blurred luminance the
// host produced for an iris window. Both are read-only and both are taken from the buffer as it
// ARRIVED - phase 19's rule that a weight evaluated on a partially edited value is not linear in
// its own strength, and past about half coverage produces a bright rim.
@group(6) @binding(5) var<storage, read> micro_background: array<f32>;
@group(6) @binding(6) var<storage, read> micro_smoothed: array<f32>;
@group(6) @binding(7) var<storage, read_write> micro_pixels: array<f32>;

fn micro_load(index: u32) -> vec3<f32> {
    let base = index * 3u;
    return vec3<f32>(micro_pixels[base], micro_pixels[base + 1u], micro_pixels[base + 2u]);
}

fn micro_store(index: u32, value: vec3<f32>) {
    let base = index * 3u;
    micro_pixels[base] = max(value.x, 0.0);
    micro_pixels[base + 1u] = max(value.y, 0.0);
    micro_pixels[base + 2u] = max(value.z, 0.0);
}

// Rec.709 luminance of a linear triple. The same three coefficients every other shader here uses.
fn micro_luma(rgb: vec3<f32>) -> f32 {
    return 0.2126 * rgb.x + 0.7152 * rgb.y + 0.0722 * rgb.z;
}

// The luminance at or above which a pixel is a reflection of the light source rather than of the
// subject, and is left exactly alone. `micro::SPECULAR_FLOOR`.
const MICRO_SPECULAR_FLOOR: f32 = 0.90;

// The luminance at or above which a pixel records nothing recoverable. Higher than the specular
// floor because the two answer different questions. `micro::CLIPPED_FLOOR`.
const MICRO_CLIPPED_FLOOR: f32 = 0.985;

// The feather at a cleaned or borrowed patch's edge, as a fraction of its radius.
// `micro::PATCH_FEATHER`.
const MICRO_PATCH_FEATHER: f32 = 0.25;

// The ceilings the contract owns. Present here so a shader that drifted from one would fail the
// parity test rather than retouch a wedding differently from the preview a photographer approved.
const MAX_TEETH_LUMA_EV: f32 = 0.20;
const MAX_TEETH_YELLOW: f32 = 0.35;
const MAX_SCLERA: f32 = 0.30;
const MAX_IRIS_CLARITY: f32 = 0.25;
const MAX_FLYAWAY_STRENGTH: f32 = 0.60;
const MAX_GLARE_REDUCE: f32 = 0.70;

// A radial feather over the dispatch window, one in the middle and zero at the edge. Smoothstep,
// so no operator here has a hard edge anywhere.
fn micro_feather(col: u32, row: u32, w: u32, h: u32) -> f32 {
    if (w == 0u || h == 0u) {
        return 0.0;
    }
    let cx = (f32(w) - 1.0) * 0.5;
    let cy = (f32(h) - 1.0) * 0.5;
    let dx = (f32(col) - cx) / max(cx, 1.0);
    let dy = (f32(row) - cy) / max(cy, 1.0);
    let distance = sqrt(dx * dx + dy * dy);
    let inner = 1.0 - MICRO_PATCH_FEATHER;
    if (distance <= inner) {
        return 1.0;
    }
    if (distance >= 1.0) {
        return 0.0;
    }
    let t = clamp((1.0 - distance) / MICRO_PATCH_FEATHER, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Linear sRGB to CIE 1976 u'v'. Mirrors `aura_raw::colour::illuminant::linear_srgb_to_uv`, which
// is where the matrix is argued about; it is duplicated here because a shader cannot call into
// the host and the parity test is what stops the duplicate from drifting.
fn micro_to_uv(rgb: vec3<f32>) -> vec2<f32> {
    let x = 0.4124564 * rgb.x + 0.3575761 * rgb.y + 0.1804375 * rgb.z;
    let y = 0.2126729 * rgb.x + 0.7151522 * rgb.y + 0.0721750 * rgb.z;
    let z = 0.0193339 * rgb.x + 0.1191920 * rgb.y + 0.9503041 * rgb.z;
    let denominator = max(x + 15.0 * y + 3.0 * z, 1e-6);
    return vec2<f32>(4.0 * x / denominator, 9.0 * y / denominator);
}

fn micro_from_uv(uv: vec2<f32>) -> vec3<f32> {
    // u'v' back to xy, then to XYZ at Y = 1, then to linear sRGB. The channels are floored at a
    // small positive value rather than clamped to zero, because a chromaticity outside the sRGB
    // gamut - which most saturated stage lighting is - would otherwise divide by zero below.
    let denominator = max(6.0 * uv.x - 16.0 * uv.y + 12.0, 1e-6);
    let x = 9.0 * uv.x / denominator;
    let y = max(4.0 * uv.y / denominator, 1e-6);
    let xyz = vec3<f32>(x / y, 1.0, (1.0 - x - y) / y);
    var rgb = vec3<f32>(
        3.2404542 * xyz.x - 1.5371385 * xyz.y - 0.4985314 * xyz.z,
        -0.9692660 * xyz.x + 1.8760108 * xyz.y + 0.0415560 * xyz.z,
        0.0556434 * xyz.x - 0.2040259 * xyz.y + 1.0572252 * xyz.z,
    );
    rgb = max(rgb, vec3<f32>(1e-4));
    let l = micro_luma(rgb);
    if (l > 1e-6) {
        rgb = rgb / l;
    }
    return rgb;
}

// Move one colour a bounded share of the way from OUTSIDE a locus to its boundary.
//
// The whole of ADR-0043 section 3, as arithmetic. A colour already inside comes back unchanged; a
// colour outside travels `share` of its own excess and no further. There is no branch that moves
// a colour toward the centre, and the move is at constant luminance.
fn micro_pull_toward_locus(rgb: vec3<f32>, share: f32) -> vec3<f32> {
    if (micro_params.neutral_valid == 0u) {
        return rgb;
    }
    let before = micro_luma(rgb);
    if (before <= 1e-6) {
        return rgb;
    }
    let uv = micro_to_uv(rgb);
    let offset = uv - micro_params.neutral;
    let from_centre = offset - micro_params.locus_centre;
    let distance = length(from_centre);
    let excess = max(distance - micro_params.locus_radius, 0.0);
    if (excess <= 0.0 || distance <= 1e-9) {
        return rgb;
    }
    let travel = excess * clamp(share, 0.0, 1.0);
    let scaled = from_centre * ((distance - travel) / distance);
    let corrected = micro_params.neutral + micro_params.locus_centre + scaled;
    return micro_from_uv(corrected) * before;
}

// Flyaway: attenuate a strand's contrast against the background behind it.
//
// The weight is `1 - hair`, so nothing inside the hair mass moves - the mass is what a bald patch
// is made of - and the background comes from `micro_background`, which the host blurred at a
// radius wider than a strand from the buffer as it arrived.
fn micro_flyaway(index: u32, local: u32, col: u32, row: u32) {
    let outside = 1.0 - clamp(micro_hair[index], 0.0, 1.0);
    if (outside <= 0.0) {
        return;
    }
    let weight = clamp(micro_params.flyaway_strength, 0.0, MAX_FLYAWAY_STRENGTH)
        * outside
        * micro_feather(col, row, micro_params.extent.x, micro_params.extent.y);
    if (weight <= 0.0) {
        return;
    }
    let value = micro_load(index);
    let base = vec3<f32>(
        micro_background[local * 3u],
        micro_background[local * 3u + 1u],
        micro_background[local * 3u + 2u],
    );
    micro_store(index, value - (value - base) * weight);
}

// Teeth: lift the luminance, clamped against this face's own brightest skin, then reduce the
// chromaticity's excess outside the locus. Specular pixels are excluded, so a wet highlight on an
// incisor is not lifted with the rest.
fn micro_teeth_correct(index: u32) {
    let coverage = clamp(micro_teeth[index], 0.0, 1.0);
    if (coverage <= 0.0) {
        return;
    }
    let rgb = micro_load(index);
    let before = micro_luma(rgb);
    if (before >= MICRO_SPECULAR_FLOOR || before <= 1e-6) {
        return;
    }
    let lift = exp2(clamp(micro_params.teeth_luma_ev, 0.0, MAX_TEETH_LUMA_EV));
    var wanted = before * lift;
    if (micro_params.brightest_skin > 0.0) {
        wanted = min(wanted, micro_params.brightest_skin);
    }
    var out = rgb * (wanted / before);
    out = micro_pull_toward_locus(out, clamp(micro_params.teeth_yellow, 0.0, MAX_TEETH_YELLOW));
    micro_store(index, rgb + (out - rgb) * coverage);
}

// Sclera: take a bounded share of the measured redness out, at constant luminance.
fn micro_sclera_clear(index: u32) {
    let coverage = clamp(micro_sclera[index], 0.0, 1.0);
    if (coverage <= 0.0) {
        return;
    }
    let rgb = micro_load(index);
    if (micro_luma(rgb) >= MICRO_SPECULAR_FLOOR) {
        // A catchlight. Excluded by construction rather than by a threshold applied afterwards.
        return;
    }
    let out = micro_pull_toward_locus(rgb, clamp(micro_params.sclera_share, 0.0, MAX_SCLERA));
    micro_store(index, rgb + (out - rgb) * coverage);
}

// Iris: raise the local contrast that is already there, on luminance only.
fn micro_iris_clarify(index: u32) {
    let coverage = clamp(micro_iris[index], 0.0, 1.0);
    if (coverage <= 0.0) {
        return;
    }
    let rgb = micro_load(index);
    let value = micro_luma(rgb);
    if (value >= MICRO_SPECULAR_FLOOR || value <= 1e-6) {
        return;
    }
    let detail = value - micro_smoothed[index];
    let gain = clamp(micro_params.iris_clarity, 0.0, MAX_IRIS_CLARITY) * coverage;
    let wanted = max(value + detail * gain, 0.0);
    micro_store(index, rgb * (wanted / value));
}

// One dispatch, one window. Which operator runs is chosen by the host through the magnitudes it
// sets: an operation the plan did not carry arrives with its magnitude at zero and every branch
// above returns immediately, which is what keeps this file free of a mode enum that a caller
// could set wrongly.
fn micro_pixel(col: u32, row: u32) {
    let x = micro_params.origin.x + col;
    let y = micro_params.origin.y + row;
    if (x >= micro_params.width || y >= micro_params.height) {
        return;
    }
    let index = y * micro_params.width + x;
    let local = row * micro_params.extent.x + col;

    if (micro_params.flyaway_strength > 0.0) {
        micro_flyaway(index, local, col, row);
    }
    if (micro_params.teeth_luma_ev > 0.0 || micro_params.teeth_yellow > 0.0) {
        micro_teeth_correct(index);
    }
    if (micro_params.sclera_share > 0.0) {
        micro_sclera_clear(index);
    }
    if (micro_params.iris_clarity > 0.0) {
        micro_iris_clarify(index);
    }
}
