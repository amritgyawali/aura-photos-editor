// PHASE-20. Patch synthesis: removing one temporary mark without losing the texture of skin.
//
// The GPU half of `heal` in `crates/aura-render/src/retouch.rs`. A LIBRARY rather than a
// stage: `retouch_apply.wgsl` calls it once per blemish, and it declares no `fn stage_` entry
// point of its own.
//
// Everything here is LINEAR and nothing encodes. Invariant 8.
//
// WHAT THIS DOES, AND THE MISTAKE IT DOES NOT MAKE
//
// A blemish is a colour event with ordinary skin texture on top of it. The obvious composition
// - take the donor tone and put the ORIGINAL texture back - puts a third of the mark back with
// it, because the edge of a spot is high-frequency content of the spot rather than of the skin.
// The processor reference shipped that once and its own unit test caught it.
//
// So both halves come from the donor, which is the same person a couple of millimetres away
// under the same light, and the donor texture is rescaled so the repaired patch carries the
// same texture energy as the ring of skin around the mark. That is what section 6.2 means by
// "matching frequency content", and it is why the texture guard measures ENERGY rather than
// pixel identity.
//
// NO ATOMICS. Each invocation writes one sample of the repaired patch.

struct PatchParams {
    // Frame dimensions.
    width: u32,
    height: u32,
    // The mark, in pixels: top-left and size of the window being repaired.
    patch_x: u32,
    patch_y: u32,
    patch_w: u32,
    patch_h: u32,
    // Where the donor was found, chosen on the host by the same eight-direction search the
    // reference runs, so both paths borrow from the same skin.
    donor_x: u32,
    donor_y: u32,
    // The blur radius that separates tone from texture, in samples.
    transplant: u32,
    // Tone shift and texture scale, both measured on the ring outside the mark.
    shift: vec3<f32>,
    texture_scale: vec3<f32>,
    // How strongly this operation runs, already multiplied by the allowance phase 18 gave the
    // mask. Nothing here re-derives a strength.
    strength: f32,
};

@group(4) @binding(0) var<uniform> patch_params: PatchParams;
@group(4) @binding(1) var<storage, read> patch_source: array<f32>;
@group(4) @binding(2) var<storage, read> patch_smooth_target: array<f32>;
@group(4) @binding(3) var<storage, read> patch_smooth_donor: array<f32>;
@group(4) @binding(4) var<storage, read> patch_skin: array<f32>;
@group(4) @binding(5) var<storage, read_write> patch_out: array<f32>;

// How far away a donor patch is looked for, as a multiple of the radius of the mark. Close
// enough that the skin is the same person under the same light, far enough that the donor is
// outside the mark and outside its own halo. `retouch::DONOR_DISTANCE`.
const DONOR_DISTANCE: f32 = 2.20;

// The largest difference in linear luminance a donor may have from its target. Above this the
// donor is a different plane of the face, a different light or a different person, and healing
// from it leaves a patch with the right texture and the wrong tone. `retouch::DONOR_MAX_DELTA`.
const DONOR_MAX_DELTA: f32 = 0.06;

// Where the boundary between tone and texture sits, as a fraction of the radius of the mark.
// A fraction rather than a fixed number of samples, so a five-pixel spot on a proxy and a
// fifty-pixel one at full resolution are separated at the same perceptual scale - which is
// what makes the preview agree with the export. `retouch::TRANSPLANT_FRACTION`.
const TRANSPLANT_FRACTION: f32 = 0.35;

// The feather at the edge of a healed patch, as a fraction of its radius. Wider than the
// texture boundary, or the transplant shows as a ring; narrower than the patch, or the centre
// never fully heals. `retouch::PATCH_FEATHER`.
const PATCH_FEATHER: f32 = 0.25;

fn patch_smooth(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// A radial feather over the patch: one in the middle, zero at the edge, smooth between.
fn patch_feather(col: u32, row: u32) -> f32 {
    if (patch_params.patch_w == 0u || patch_params.patch_h == 0u) { return 0.0; }
    let cx = (f32(patch_params.patch_w) - 1.0) * 0.5;
    let cy = (f32(patch_params.patch_h) - 1.0) * 0.5;
    let dx = (f32(col) - cx) / max(cx, 1.0);
    let dy = (f32(row) - cy) / max(cy, 1.0);
    let distance = sqrt(dx * dx + dy * dy);
    let inner = 1.0 - PATCH_FEATHER;
    if (distance <= inner) { return 1.0; }
    if (distance >= 1.0) { return 0.0; }
    return patch_smooth((1.0 - distance) / PATCH_FEATHER);
}

@compute @workgroup_size(64)
fn inpaint_patch(@builtin(global_invocation_id) id: vec3<u32>) {
    let local = id.x;
    if (local >= patch_params.patch_w * patch_params.patch_h) { return; }
    let col = local % patch_params.patch_w;
    let row = local / patch_params.patch_w;

    let fx = patch_params.patch_x + col;
    let fy = patch_params.patch_y + row;
    if (fx >= patch_params.width || fy >= patch_params.height) { return; }
    let pixel = fy * patch_params.width + fx;

    // Phase 18 decides where an operator may act. No mask, no edit - there is no fallback that
    // draws a rectangle, because an operator that falls back to one edits the wall behind
    // somebody's ear.
    let coverage = patch_skin[pixel];
    let weight = patch_feather(col, row) * clamp(patch_params.strength, 0.0, 1.0) * coverage;
    if (weight <= 0.0) {
        patch_out[local] = 0.0;
        return;
    }

    let donor_pixel = (patch_params.donor_y + row) * patch_params.width
                    + (patch_params.donor_x + col);

    var composed = vec3<f32>(0.0, 0.0, 0.0);
    for (var channel = 0u; channel < 3u; channel = channel + 1u) {
        let donor_tone = patch_smooth_donor[donor_pixel * 3u + channel];
        let donor_texture = patch_source[donor_pixel * 3u + channel] - donor_tone;
        composed[channel] = donor_tone
            + patch_params.shift[channel]
            + donor_texture * patch_params.texture_scale[channel];
    }

    for (var channel = 0u; channel < 3u; channel = channel + 1u) {
        let original = patch_source[pixel * 3u + channel];
        patch_out[pixel * 3u + channel] =
            max(original * (1.0 - weight) + max(composed[channel], 0.0) * weight, 0.0);
    }
}
