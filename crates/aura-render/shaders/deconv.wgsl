// PHASE-22. Richardson-Lucy deconvolution with edge-aware damping, through a weight plane.
//
// A LIBRARY, not a stage. It declares no `fn stage_` entry point: the deconvolution runs inside
// `Stage::Sharpen`, which phase 14 already owns and which section 2.1 of PHASE-22 requires to be
// the last pixel operation before the output transform.
//
// The GPU half of `deconvolve` in `crates/aura-render/src/restore.rs`. Section 6.2 asks for
// "Richardson-Lucy-style deconvolution with a small iteration count and edge-aware damping to
// avoid ringing", and the three decisions below are what that sentence becomes.
//
// IT RUNS ON LUMINANCE ONLY
//
// Deconvolving three channels independently produces coloured fringes at every edge, because the
// three converge at different rates on the same structure. The chromaticity is carried through
// unchanged by `deconv_set_luma`, which is also what makes this composable with the chroma pass
// in `denoise_tile.wgsl`.
//
// THE DAMPING READS THE INPUT
//
// `deconv_guard` is computed once, from the ORIGINAL luminance, and is constant across the
// iterations. Phase 19's defect and the rule it wrote: a weight evaluated on an already-edited
// value is not linear in its own strength, and the failure mode is an operator that is stronger
// at the edge of its region than at the centre. Here it would be worse than a rim - an iterative
// operator whose damping followed its own output either runs away or stalls.
//
// A PIXEL AT WEIGHT ZERO IS BIT-IDENTICAL TO ITS INPUT
//
// Which is what makes the mask a mask. `weights` is zero over sky and out-of-focus background and
// `1 - skin_attenuation` over skin, and the final blend is `weight * amount`, so an excluded sky
// is genuinely untouched rather than deconvolved and then mostly blended back. "Mostly" is where
// a crunchy sky comes from. ADR-0047 section 4.
//
// WHAT THIS FILE CANNOT DO
//
// There is no scale, no output size and no synthesised region. Section 2.2 puts upscaling beyond
// native resolution and generative reconstruction out of scope for V1, and there is nowhere here
// to express either. Nor is there a motion kernel: a symmetric kernel deconvolving a directional
// blur produces a doubled edge, and `aura_restore::sharpen` refuses those frames before this file
// is ever dispatched.
//
// Everything here is LINEAR and nothing encodes. Invariant 8. NO ATOMICS.

struct DeconvParams {
    width: u32,
    height: u32,
    // The tile this dispatch writes, in frame pixels. The halo is outside it and is read only.
    origin: vec2<u32>,
    extent: vec2<u32>,
    // The estimated blur kernel, in pixels of Gaussian sigma. Inside SHARPEN_KERNEL_LO..HI or
    // `aura_restore::sharpen` refused the frame and this file was not dispatched.
    kernel_sigma: f32,
    // How much of the deconvolution is applied, 0..MAX_SHARPEN_AMOUNT.
    amount: f32,
    // The box radius that approximates `kernel_sigma`, from the host.
    radius: u32,
    // Which iteration this dispatch is, from zero. The host runs one dispatch per iteration and
    // swaps the buffers, because a single dispatch cannot synchronise across workgroups.
    iteration: u32,
    // How many there are in total, at most MAX_DECONV_ITERATIONS.
    iterations: u32,
}

@group(0) @binding(0) var<storage, read> observed: array<f32>;
@group(0) @binding(1) var<storage, read> estimate: array<f32>;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
@group(0) @binding(3) var<storage, read> weights: array<f32>;
@group(0) @binding(4) var<uniform> params: DeconvParams;

// The most Richardson-Lucy iterations one frame gets.
// `aura_core::contract::restore::MAX_DECONV_ITERATIONS`.
const MAX_DECONV_ITERATIONS: u32 = 3u;

// The largest deconvolution amount. `aura_core::contract::restore::MAX_SHARPEN_AMOUNT`.
const MAX_SHARPEN_AMOUNT: f32 = 0.50;

// The share of the amount withheld on skin. `aura_core::contract::restore::SKIN_ATTENUATION`.
// The host folds it into `weights`; the constant is here so that `shader_parity.rs` can hold the
// two files to the same number.
const SKIN_ATTENUATION: f32 = 0.80;

// How hard the ratio is clamped each iteration.
//
// The RL update is a ratio of the observed image to the re-blurred estimate, and near a zero it
// is unbounded. An unclamped ratio at a specular highlight is where a single pixel becomes a
// star, so it is bounded on both sides rather than only above.
const DECONV_RATIO_LO: f32 = 0.25;
const DECONV_RATIO_HI: f32 = 4.0;

fn deconv_luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.262700, 0.677998, 0.059302));
}

fn deconv_set_luma(rgb: vec3<f32>, target: f32) -> vec3<f32> {
    let current = deconv_luma(rgb);
    if (current <= 1e-6) {
        return vec3<f32>(max(target, 0.0));
    }
    return rgb * max(target / current, 0.0);
}

fn deconv_load(buffer_index: u32) -> vec3<f32> {
    let base = buffer_index * 3u;
    return vec3<f32>(observed[base], observed[base + 1u], observed[base + 2u]);
}

fn deconv_estimate_luma(buffer_index: u32) -> f32 {
    return deconv_luma(vec3<f32>(
        estimate[buffer_index * 3u],
        estimate[buffer_index * 3u + 1u],
        estimate[buffer_index * 3u + 2u],
    ));
}

// A box mean of the current estimate's luminance, clamped at the frame edge.
//
// The edge sample rather than zero. See `denoise_tile.wgsl` for why, and it matters more here:
// a zero read at a tile boundary would be a dark ring that the ratio then amplifies once per
// iteration.
fn deconv_blur_estimate(centre: vec2<u32>, radius: u32) -> f32 {
    var total = 0.0;
    var count = 0.0;
    let r = i32(radius);
    for (var dy = -r; dy <= r; dy = dy + 1) {
        for (var dx = -r; dx <= r; dx = dx + 1) {
            let sx = u32(clamp(i32(centre.x) + dx, 0, i32(params.width) - 1));
            let sy = u32(clamp(i32(centre.y) + dy, 0, i32(params.height) - 1));
            total = total + deconv_estimate_luma(sy * params.width + sx);
            count = count + 1.0;
        }
    }
    return total / max(count, 1.0);
}

// The same neighbourhood over the observed image, for the damping guard.
fn deconv_blur_observed(centre: vec2<u32>, radius: u32) -> f32 {
    var total = 0.0;
    var count = 0.0;
    let r = i32(radius);
    for (var dy = -r; dy <= r; dy = dy + 1) {
        for (var dx = -r; dx <= r; dx = dx + 1) {
            let sx = u32(clamp(i32(centre.x) + dx, 0, i32(params.width) - 1));
            let sy = u32(clamp(i32(centre.y) + dy, 0, i32(params.height) - 1));
            total = total + deconv_luma(deconv_load(sy * params.width + sx));
            count = count + 1.0;
        }
    }
    return total / max(count, 1.0);
}

// How much of the correction survives at one pixel, 0..1.
//
// A pixel whose own step is large is a pixel on a strong edge, and a strong edge is where ringing
// appears. It is damped rather than excluded, because a strong edge is also the only place there
// is anything to recover. Computed from the OBSERVED image - see the header.
fn deconv_guard(centre: vec2<u32>, radius: u32) -> f32 {
    let value = deconv_luma(deconv_load(centre.y * params.width + centre.x));
    let low = deconv_blur_observed(centre, radius);
    return 1.0 / (1.0 + abs(value - low) * 12.0);
}

@compute @workgroup_size(8, 8, 1)
fn deconv(@builtin(global_invocation_id) id: vec3<u32>) {
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
    let input = deconv_load(index);
    let original = deconv_luma(input);

    let radius = max(params.radius, 1u);
    let current = deconv_estimate_luma(index);
    let reblurred = deconv_blur_estimate(vec2<u32>(x, y), radius);

    var ratio = 1.0;
    if (reblurred > 1e-6) {
        ratio = clamp(original / reblurred, DECONV_RATIO_LO, DECONV_RATIO_HI);
    }
    let damped = 1.0 + (ratio - 1.0) * deconv_guard(vec2<u32>(x, y), radius);
    let recovered = max(current * damped, 0.0);

    // Every iteration but the last writes the estimate back untouched by the mask, because the
    // mask is about what is DELIVERED rather than about what is computed - blending toward the
    // input each iteration would make the result depend on the iteration count in a way the
    // processor reference does not.
    let last = params.iteration + 1u >= min(params.iterations, MAX_DECONV_ITERATIONS);
    if (!last) {
        let out = deconv_set_luma(input, recovered);
        dst[base] = out.r;
        dst[base + 1u] = out.g;
        dst[base + 2u] = out.b;
        return;
    }

    let weight = clamp(weights[index], 0.0, 1.0);
    let mix_amount = weight * min(params.amount, MAX_SHARPEN_AMOUNT);
    if (mix_amount <= 0.0) {
        // Bit-identical to the input. See the header: this is what makes the mask a mask.
        dst[base] = input.r;
        dst[base + 1u] = input.g;
        dst[base + 2u] = input.b;
        return;
    }
    let target = original + (recovered - original) * mix_amount;
    let out = deconv_set_luma(input, max(target, 0.0));
    dst[base] = out.r;
    dst[base + 1u] = out.g;
    dst[base + 2u] = out.b;
}
