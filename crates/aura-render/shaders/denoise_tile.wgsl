// PHASE-22. The noise-model-conditioned denoiser, tiled.
//
// A LIBRARY, not a stage. It declares no `fn stage_` entry point, for the reason
// `inpaint_patch.wgsl`, `freq_bands.wgsl` and `micro_apply.wgsl` declare none: denoising runs
// inside `Stage::NoiseReduction`, which phase 14 already owns, and a second stage in
// `graph::ORDER` would be a change to a frozen contract for no behavioural reason.
//
// The GPU half of `denoise` in `crates/aura-render/src/restore.rs`. Two passes over separated
// planes, and the thing that makes it phase 22's rather than phase 14's is that both read SIGMA.
//
// THE ONE IDEA
//
// The decision about what to keep is made in units of the SENSOR'S OWN UNCERTAINTY rather than in
// units of the frame's contrast. `denoise_edge_keep` compares a local step against
// `EDGE_SIGMAS * sigma`, where sigma is what the camera's photon-transfer curve predicts at this
// signal level and this ISO. A step of three sigma is a step the sensor could not have produced
// by noise, so it is structure and is left where it is; a step under one sigma is noise. That is
// why the same tier removes visibly different amounts from a clean ISO 100 frame and a dance
// floor, and why a frame with no noise in it is not blurred however strong the tier.
//
// A DENOISER WITH NO SIGMA DOES NOTHING
//
// `params.sigma <= 0.0` returns the input unchanged rather than falling back to a fixed radius.
// A denoiser that does not know how much noise to expect is a blur, and the processor reference
// takes the same branch - `RestoreApplied::unconditioned` counts it and the plan says
// `restore_no_noise_reading`.
//
// CHROMA IS WIDER AND HARDER, AND THOSE ARE TWO DIFFERENT ASYMMETRIES
//
// The chroma RADIUS is `CHROMA_RADIUS_RATIO` times the luminance radius, because chroma noise is
// spatially low-frequency and needs the width to reach across it. The chroma AMOUNT is larger
// too, but that is decided in `aura_restore::denoise` rather than here - this file receives two
// amounts and does not know which is which. Section 6.1 asks for both and they pull in opposite
// directions on fabric, which is why the edge guard is applied to the chroma pass as well.
//
// TILING
//
// `MAX_DENOISE_RADIUS` bounds both radii, and three box passes reach three times a radius, so the
// halo `graph::Stage::halo` sums for `NoiseReduction` covers the worst case. A tile is denoised
// with its halo decoded and the halo is discarded, which is what makes a tiled render
// bit-identical to a whole-frame one.
//
// Everything here is LINEAR and nothing encodes. Invariant 8. NO ATOMICS.

struct DenoiseParams {
    width: u32,
    height: u32,
    // The tile this dispatch writes, in frame pixels. The halo is outside it and is read only.
    origin: vec2<u32>,
    extent: vec2<u32>,
    // The predicted sensor sigma at diffuse white, in linear working-space units. Zero means no
    // noise model resolved, and this shader then writes its input.
    sigma: f32,
    // Luminance reduction, 0..1.
    luminance: f32,
    // Chroma reduction, 0..1. Never below `luminance`; the contract refuses a plan where it is.
    colour: f32,
    // How much fine detail is protected against the luminance pass, 0..1.
    detail: f32,
    // The two radii, in samples, already clamped to MAX_DENOISE_RADIUS by the host.
    luma_radius: u32,
    chroma_radius: u32,
}

@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<f32>;
@group(0) @binding(2) var<uniform> params: DenoiseParams;

// How many multiples of the predicted sigma count as certainly an edge.
// `restore::EDGE_SIGMAS` on the processor path.
const EDGE_SIGMAS: f32 = 3.0;

// How much wider the chroma radius is than the luminance radius.
// `restore::CHROMA_RADIUS_RATIO` on the processor path. The host applies it; the constant is
// here so that `shader_parity.rs` can hold the two files to the same number.
const CHROMA_RADIUS_RATIO: f32 = 2.5;

// The largest radius either half will use. `restore::MAX_DENOISE_RADIUS`.
const MAX_DENOISE_RADIUS: u32 = 12u;

fn denoise_luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.262700, 0.677998, 0.059302));
}

fn denoise_set_luma(rgb: vec3<f32>, target: f32) -> vec3<f32> {
    let current = denoise_luma(rgb);
    if (current <= 1e-6) {
        return vec3<f32>(max(target, 0.0));
    }
    return rgb * max(target / current, 0.0);
}

fn denoise_load(index: u32) -> vec3<f32> {
    let base = index * 3u;
    return vec3<f32>(src[base], src[base + 1u], src[base + 2u]);
}

// How much of a local step is kept, given the sensor's own uncertainty.
//
// The whole of "conditioned on the noise model", in five lines. Zero at a step the sensor could
// easily have produced by noise, one at EDGE_SIGMAS and above. `detail` sharpens the transition
// rather than moving it: a photographer asking for more detail is asking to be more suspicious
// that a step is real, not to redefine what the sensor can do.
fn denoise_edge_keep(step: f32, sigma: f32, detail: f32) -> f32 {
    if (sigma <= 0.0) {
        return 1.0;
    }
    let ratio = clamp(step / (sigma * EDGE_SIGMAS), 0.0, 1.0);
    return pow(ratio, 1.0 / (0.35 + clamp(detail, 0.0, 1.0) * 1.3));
}

// A box mean of one plane over a square neighbourhood, clamped at the frame edge.
//
// The edge sample rather than zero, always. Phase 18's defect: a filter that reads zero outside
// a plane darkens the outermost half-pixel of everything it produces, which is a rim around every
// tile boundary and is exactly the artefact tiling exists to avoid.
fn denoise_box_luma(centre: vec2<u32>, radius: u32) -> f32 {
    var total = 0.0;
    var count = 0.0;
    let r = i32(min(radius, MAX_DENOISE_RADIUS));
    for (var dy = -r; dy <= r; dy = dy + 1) {
        for (var dx = -r; dx <= r; dx = dx + 1) {
            let sx = u32(clamp(i32(centre.x) + dx, 0, i32(params.width) - 1));
            let sy = u32(clamp(i32(centre.y) + dy, 0, i32(params.height) - 1));
            total = total + denoise_luma(denoise_load(sy * params.width + sx));
            count = count + 1.0;
        }
    }
    return total / max(count, 1.0);
}

// The same neighbourhood over the two chroma differences.
fn denoise_box_chroma(centre: vec2<u32>, radius: u32) -> vec2<f32> {
    var total = vec2<f32>(0.0, 0.0);
    var count = 0.0;
    let r = i32(min(radius, MAX_DENOISE_RADIUS));
    for (var dy = -r; dy <= r; dy = dy + 1) {
        for (var dx = -r; dx <= r; dx = dx + 1) {
            let sx = u32(clamp(i32(centre.x) + dx, 0, i32(params.width) - 1));
            let sy = u32(clamp(i32(centre.y) + dy, 0, i32(params.height) - 1));
            let pixel = denoise_load(sy * params.width + sx);
            let l = denoise_luma(pixel);
            total = total + vec2<f32>(pixel.r - l, pixel.b - l);
            count = count + 1.0;
        }
    }
    return total / max(count, 1.0);
}

@compute @workgroup_size(8, 8, 1)
fn denoise_tile(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = params.origin.x + id.x;
    let y = params.origin.y + id.y;
    if (id.x >= params.extent.x || id.y >= params.extent.y) {
        return;
    }
    if (x >= params.width || y >= params.height) {
        return;
    }
    let index = y * params.width + x;
    let base = index * 3u;
    let input = denoise_load(index);

    // See the header. No sigma, no denoising - not a fallback radius.
    if (params.sigma <= 0.0) {
        dst[base] = input.r;
        dst[base + 1u] = input.g;
        dst[base + 2u] = input.b;
        return;
    }

    var current = input;
    let l = denoise_luma(input);

    if (params.colour > 0.0) {
        let chroma = vec2<f32>(input.r - l, input.b - l);
        let blurred = denoise_box_chroma(vec2<u32>(x, y), params.chroma_radius);
        // The chroma guard reads the CHROMA difference against the sigma, not the luminance one:
        // a red-to-green step at constant luminance is exactly what chroma noise looks like, and
        // exactly what a coloured thread looks like. The sensor's own sigma is the only thing
        // that separates them.
        let step = max(abs(chroma.x - blurred.x), abs(chroma.y - blurred.y));
        let keep = denoise_edge_keep(step, params.sigma, params.detail);
        let mix_amount = params.colour * (1.0 - keep);
        let r = l + chroma.x + (blurred.x - chroma.x) * mix_amount;
        let b = l + chroma.y + (blurred.y - chroma.y) * mix_amount;
        current = denoise_set_luma(vec3<f32>(r, current.g, b), l);
    }

    if (params.luminance > 0.0) {
        let low = denoise_box_luma(vec2<u32>(x, y), params.luma_radius);
        // The weight reads the INPUT luminance rather than the partially corrected value. Phase
        // 19's defect and the rule it wrote: a weight evaluated on an already-edited value is not
        // linear in its own strength, and the failure mode is an operator that is stronger at the
        // edge of its region than at the centre.
        let keep = denoise_edge_keep(abs(l - low), params.sigma, params.detail);
        let mix_amount = params.luminance * (1.0 - keep);
        let target = l + (low - l) * mix_amount;
        current = denoise_set_luma(current, max(target, 0.0));
    }

    dst[base] = current.r;
    dst[base + 1u] = current.g;
    dst[base + 2u] = current.b;
}
