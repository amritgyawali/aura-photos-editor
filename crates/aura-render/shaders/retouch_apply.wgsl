// PHASE-20. The retouch stage: under-eye correction and tone evening, through a skin mask.
//
// The GPU half of `apply` in `crates/aura-render/src/retouch.rs`. Blemishes are healed by
// `inpaint_patch.wgsl`, one dispatch per mark, before this runs; the order is fixed - heal,
// then under-eye, then even - because a blemish still present when the mid band is scaled is a
// blemish that gets spread rather than removed.
//
// Everything here is LINEAR and nothing encodes. Invariant 8. The one `pow` is `exp2`, which
// turns a count of stops into a multiplier and never touches an encoded value.
//
// THE TWO THINGS THIS FILE DOES NOT HAVE
//
// There is no smoothing radius and no blur strength anywhere in it. Tone evening is a scale on
// the mid band and a reconstruction; the high band is added back untouched, so pores cannot be
// reached by any value of any parameter. That is section 6.3 as a property of the arithmetic
// rather than as a promise about the arithmetic.
//
// There is no target colour and no target luminance either. Both halves of the under-eye
// correction are measured against the ring of skin around the eye, which is phase 15 rule: a
// fixed target is how an editor lightens dark skin while believing it is correcting a shadow.
//
// NO ATOMICS.

struct RetouchParams {
    width: u32,
    height: u32,
    // Eye centres in pixels for the face being corrected, left then right. A face with no
    // landmarks is not dispatched at all - an unknown landmark must never be read as the
    // origin, which is phase 09 rule and `FaceRef::has_eyes`.
    left_eye: vec2<f32>,
    right_eye: vec2<f32>,
    // The two capped under-eye magnitudes, from the plan.
    undereye_luma_ev: f32,
    undereye_chroma: f32,
    // Tone evening strength, already multiplied by the allowance phase 18 gave the mask.
    evening_strength: f32,
    // The skin around the eye: its mean chromaticity and its mean linear luminance, measured
    // on the host over the ring outside the region.
    ring_chroma: vec3<f32>,
    ring_luma: f32,
};

@group(5) @binding(0) var<uniform> retouch_params: RetouchParams;
@group(5) @binding(1) var<storage, read> retouch_skin: array<f32>;
@group(5) @binding(2) var<storage, read> retouch_band_low: array<f32>;
@group(5) @binding(3) var<storage, read> retouch_band_mid: array<f32>;
@group(5) @binding(4) var<storage, read> retouch_band_high: array<f32>;
@group(5) @binding(5) var<storage, read_write> retouch_pixels: array<f32>;

// Interleaved linear RGB, the same layout `spatial.wgsl` uses. Named apart from that file
// accessors because the two are separate modules in this build; a backend that composes them
// into one would otherwise have two functions called `load` and no way to say which it meant.
fn retouch_load(index: u32) -> vec3<f32> {
    let base = index * 3u;
    return vec3<f32>(retouch_pixels[base], retouch_pixels[base + 1u], retouch_pixels[base + 2u]);
}

fn retouch_store(index: u32, value: vec3<f32>) {
    let base = index * 3u;
    retouch_pixels[base] = max(value.x, 0.0);
    retouch_pixels[base + 1u] = max(value.y, 0.0);
    retouch_pixels[base + 2u] = max(value.z, 0.0);
}

// How far below the eye the periorbital region reaches, as a multiple of the eye separation. A
// fifth: the region a retoucher lightens is the tear trough and the orbital rim, not the cheek.
// Taking it lower is the classic tell, because it flattens the transition into the cheekbone
// phase 19 has just shaped. `retouch::UNDEREYE_DROP`.
const UNDEREYE_DROP: f32 = 0.20;

// How wide that region is, as a multiple of the eye separation. `retouch::UNDEREYE_WIDTH`.
const UNDEREYE_WIDTH: f32 = 0.34;

// How far below the surrounding skin a pixel must sit to count as a full shadow: about half a
// stop. A tear trough on a well-lit face is a fifth to a third down; past half a stop it is a
// socket rather than a circle, and lifting one of those flattens the face.
// `retouch::SHADOW_SPAN`.
const SHADOW_SPAN: f32 = 0.35;

// The most tone evening may take out of the mid band, as a fraction of its own energy. A third:
// beyond that the modelling of the face leaves along with the blotches, which is the difference
// between a face that is evenly lit and a face that is a mask.
// `aura_core::contract::retouch::MAX_EVENING_MID`.
const MAX_EVENING_MID: f32 = 0.33;

const LUMA_R: f32 = 0.262700;
const LUMA_G: f32 = 0.677998;
const LUMA_B: f32 = 0.059302;

fn retouch_luma(rgb: vec3<f32>) -> f32 {
    return LUMA_R * rgb.r + LUMA_G * rgb.g + LUMA_B * rgb.b;
}

fn retouch_smooth(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// The elliptical falloff of one periorbital region, zero outside it.
fn undereye_weight(x: u32, y: u32, eye: vec2<f32>, separation: f32) -> f32 {
    let half_w = separation * UNDEREYE_WIDTH * 0.5;
    let drop = separation * UNDEREYE_DROP;
    let cx = eye.x;
    let cy = eye.y + drop * 0.5;
    let ex = (f32(x) - cx) / max(half_w, 1.0);
    let ey = (f32(y) - cy) / max(drop * 0.5, 1.0);
    let radial = 1.0 - sqrt(ex * ex + ey * ey);
    if (radial <= 0.0) { return 0.0; }
    return retouch_smooth(radial);
}

@compute @workgroup_size(64)
fn stage_retouch(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= retouch_params.width * retouch_params.height) { return; }

    let coverage = retouch_skin[index];
    if (coverage <= 0.0) { return; }

    let x = index % retouch_params.width;
    let y = index / retouch_params.width;

    var rgb = retouch_load(index);

    // --- tone evening: scale the mid band, put the other two back untouched ----------------
    let strength = clamp(retouch_params.evening_strength, 0.0, 1.0);
    if (strength > 0.0) {
        let scale = 1.0 - strength * MAX_EVENING_MID;
        var evened = vec3<f32>(0.0, 0.0, 0.0);
        for (var channel = 0u; channel < 3u; channel = channel + 1u) {
            let slot = index * 3u + channel;
            evened[channel] = retouch_band_low[slot]
                + retouch_band_mid[slot] * scale
                + retouch_band_high[slot];
        }
        rgb = mix(rgb, max(evened, vec3<f32>(0.0, 0.0, 0.0)), coverage);
    }

    // --- under-eye: a capped lift on the shadow, and a capped move toward the skin ---------
    let separation = length(retouch_params.right_eye - retouch_params.left_eye);
    if (separation > 0.0 && retouch_params.ring_luma > 0.0) {
        let weight = coverage * max(
            undereye_weight(x, y, retouch_params.left_eye, separation),
            undereye_weight(x, y, retouch_params.right_eye, separation)
        );
        if (weight > 0.0) {
            // How much darker than the surrounding skin this pixel is, as a fraction of a full
            // shadow. Relative, never absolute: see the header.
            let own = max(retouch_luma(rgb), 1e-6);
            let depth = clamp(
                (retouch_params.ring_luma - own) / max(retouch_params.ring_luma * SHADOW_SPAN, 1e-6),
                0.0,
                1.0
            );
            rgb = rgb * exp2(retouch_params.undereye_luma_ev * weight * retouch_smooth(depth));

            let lifted = max(retouch_luma(rgb), 1e-6);
            let chroma = abs(retouch_params.undereye_chroma) * weight;
            let own_chroma = rgb / lifted;
            rgb = lifted * (own_chroma + (retouch_params.ring_chroma - own_chroma) * chroma);
        }
    }

    retouch_store(index, rgb);
}
