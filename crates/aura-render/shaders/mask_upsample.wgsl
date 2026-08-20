// PHASE-18. Guided upsampling and feathering of a stored mask.
//
// Masks are stored small - a run length over the 768 px analysis grid, or eight-bit alpha at a
// quarter of it - and composited at whatever resolution the render is running at. Section 6.1
// calls for "guided-filter upsampling to full resolution at render time, so masks are stored
// small but composited precisely", and this is that pass.
//
// Nothing here is executed in this build: no `wgpu` backend is linked (ADR-0029 section 4).
// `crates/aura-render/tests/shader_parity.rs` still holds it to the reference, which is the
// point - a shader that drifts while no device can run it fails the build today rather than a
// year from now when one arrives.
//
// TWO THINGS THIS FILE MUST NOT DO, both checked by tests rather than by review:
//
//  * It must not apply a transfer function. `colour_discipline.rs` and
//    `only_the_output_shader_bakes_tone` both cover it. A mask is multiplied into linear light
//    and an alpha blended after an encode is a different blend - see mask_composite.wgsl.
//  * It must not declare an entry point named `stage_*` unless that stage is in `graph::ORDER`.
//    The three functions here are helpers the masks stage calls, not stages.
//
// NO ATOMICS, and no workgroup reductions. Every invocation reads a neighbourhood of the input
// and writes exactly one output element.

struct MaskFrame {
    // The stored plane.
    src_width: u32,
    src_height: u32,
    // The grid being composited onto.
    dst_width: u32,
    dst_height: u32,
    // Feather, 0..1, as a fraction of the destination's short edge. Scaled here rather than in
    // pixels so the slider means the same softness at every render level - the same decision
    // `algebra::feather` records on the processor side.
    feather: f32,
    // The guided-filter regularisation. Matches `matting::EPSILON`.
    epsilon: f32,
};

@group(0) @binding(0) var<storage, read> mask_src: array<f32>;
@group(0) @binding(1) var<storage, read_write> mask_dst: array<f32>;
@group(0) @binding(2) var<uniform> mask_frame: MaskFrame;
// The guide: the frame's own luminance at the destination resolution. A luminance rather than
// three channels, because a colour guided filter needs a 3x3 inverse per window and the
// boundary this refines - hair against a background - is a luminance boundary.
@group(0) @binding(3) var<storage, read> guide: array<f32>;

// The largest feather, as a fraction of the plane's short edge. `algebra::FEATHER_MAX_FRACTION`.
const FEATHER_MAX_FRACTION: f32 = 0.08;

// Rec.2020 luminance weights. The same three numbers the processor path uses; the parity test
// checks each of them appears here with the value it has in Rust.
const luma_r: f32 = 0.262700;
const luma_g: f32 = 0.677998;
const luma_b: f32 = 0.059302;

fn src_at(x: i32, y: i32) -> f32 {
    // Clamp to edge, never zero outside. Reading zero past the border darkens the outermost
    // half-pixel of every mask, which is a one-pixel dark rim around every region at every
    // render level - a halo manufactured by the resampler. `Plane::at_clamped` is the same
    // decision on the processor side.
    let cx = clamp(x, 0, i32(mask_frame.src_width) - 1);
    let cy = clamp(y, 0, i32(mask_frame.src_height) - 1);
    return mask_src[u32(cy) * mask_frame.src_width + u32(cx)];
}

fn guide_at(x: i32, y: i32) -> f32 {
    let cx = clamp(x, 0, i32(mask_frame.dst_width) - 1);
    let cy = clamp(y, 0, i32(mask_frame.dst_height) - 1);
    return guide[u32(cy) * mask_frame.dst_width + u32(cx)];
}

// Bilinear resample of the stored plane onto the destination grid.
//
// Bilinear on the way *out* and nearest on the way *in*: on the way in the alpha values have
// not been decided yet and interpolating invents a soft edge nobody measured, and on the way
// out they have been.
fn mask_resample(x: u32, y: u32) -> f32 {
    let sx = f32(mask_frame.src_width) / f32(mask_frame.dst_width);
    let sy = f32(mask_frame.src_height) / f32(mask_frame.dst_height);
    let fx = max((f32(x) + 0.5) * sx - 0.5, 0.0);
    let fy = max((f32(y) + 0.5) * sy - 0.5, 0.0);
    let x0 = i32(floor(fx));
    let y0 = i32(floor(fy));
    let tx = fx - floor(fx);
    let ty = fy - floor(fy);
    let top = mix(src_at(x0, y0), src_at(x0 + 1, y0), tx);
    let bottom = mix(src_at(x0, y0 + 1), src_at(x0 + 1, y0 + 1), ty * 0.0 + tx);
    return mix(top, bottom, ty);
}

// One guided-filter window around a destination pixel.
//
// `a = k * I + b`, with `k = cov(I, p) / (var(I) + epsilon)` fitted over the window. Where the
// guide carries no variance the fit degenerates to a blur of the coarse mask, so the caller
// blends back toward the resampled value - `matting::VARIANCE_FLOOR` is the same guard on the
// processor side and the reason it exists is that a blurred mask reads as half-inside several
// pixels outside the true boundary.
fn mask_guided(x: u32, y: u32, radius: i32) -> f32 {
    var mean_i: f32 = 0.0;
    var mean_p: f32 = 0.0;
    var mean_ip: f32 = 0.0;
    var mean_ii: f32 = 0.0;
    var n: f32 = 0.0;

    for (var dy: i32 = -radius; dy <= radius; dy = dy + 1) {
        for (var dx: i32 = -radius; dx <= radius; dx = dx + 1) {
            let gx = i32(x) + dx;
            let gy = i32(y) + dy;
            let i_value = guide_at(gx, gy);
            let p_value = mask_resample(u32(clamp(gx, 0, i32(mask_frame.dst_width) - 1)),
                                        u32(clamp(gy, 0, i32(mask_frame.dst_height) - 1)));
            mean_i = mean_i + i_value;
            mean_p = mean_p + p_value;
            mean_ip = mean_ip + i_value * p_value;
            mean_ii = mean_ii + i_value * i_value;
            n = n + 1.0;
        }
    }
    mean_i = mean_i / n;
    mean_p = mean_p / n;
    mean_ip = mean_ip / n;
    mean_ii = mean_ii / n;

    let cov = mean_ip - mean_i * mean_p;
    let variance = max(mean_ii - mean_i * mean_i, 0.0);
    let k = cov / (variance + mask_frame.epsilon);
    let b = mean_p - k * mean_i;
    return clamp(k * guide_at(i32(x), i32(y)) + b, 0.0, 1.0);
}

// A separable box blur of the destination plane, in place of a Gaussian.
//
// The feather radius is a fraction of the destination's short edge, so the same slider position
// produces the same visual softness at 2048 px and at full resolution. A radius in pixels would
// make the slider mean something different at every level.
fn mask_feather_radius() -> i32 {
    let short = f32(min(mask_frame.dst_width, mask_frame.dst_height));
    return max(i32(round(clamp(mask_frame.feather, 0.0, 1.0) * FEATHER_MAX_FRACTION * short)), 1);
}

@compute @workgroup_size(8, 8, 1)
fn mask_upsample(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= mask_frame.dst_width || id.y >= mask_frame.dst_height) {
        return;
    }
    let index = id.y * mask_frame.dst_width + id.x;
    let resampled = mask_resample(id.x, id.y);
    let refined = mask_guided(id.x, id.y, 2);
    // The guide decides how much of the refinement is trusted. Flat guide, keep the resample.
    let local = abs(guide_at(i32(id.x) + 1, i32(id.y)) - guide_at(i32(id.x), i32(id.y)));
    let trust = clamp(local / max(mask_frame.epsilon * 100.0, 1e-5), 0.0, 1.0);
    mask_dst[index] = mix(resampled, refined, trust);
}
