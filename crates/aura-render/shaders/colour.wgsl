// The input half: highlight recovery, white balance, the camera matrix and lens vignette.
//
// PHASE-14 section 4 asks for a compute shader per stage. These are those shaders, and they
// are a *specification* rather than decoration: this build links no wgpu (ADR-0029 section
// 4), and `crates/aura-render/tests/shader_parity.rs` asserts that every stage in
// `graph::ORDER` has an entry point here and that each entry point's uniform fields match
// the Rust reference's parameter list field for field.
//
// A shader that drifts from the reference therefore fails the build today, before a GPU
// exists to run it. That is the whole point of shipping them absent a backend.
//
// PRECISION. Storage is `f32` in this file, not `f16`. ADR-0029 section 3: an `f16` has
// about eleven bits of mantissa, so two roundings can exceed the 1/1024 parity tolerance the
// gate is written against. A future backend may store textures as `rgba16float` and must
// still compute in `f32`.

struct Frame {
    width: u32,
    height: u32,
};

@group(0) @binding(0) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(1) var<uniform> frame: Frame;

fn load(index: u32) -> vec3<f32> {
    let base = index * 3u;
    return vec3<f32>(pixels[base], pixels[base + 1u], pixels[base + 2u]);
}

fn store(index: u32, value: vec3<f32>) {
    let base = index * 3u;
    pixels[base] = value.x;
    pixels[base + 1u] = value.y;
    pixels[base + 2u] = value.z;
}

// Rec.2020 luminance. The second row of REC2020_TO_XYZ_D65, matching `colour::luma`.
fn luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.262700, 0.677998, 0.059302));
}

fn set_luma(rgb: vec3<f32>, target: f32) -> vec3<f32> {
    let current = luma(rgb);
    if (current <= 1e-6) {
        return vec3<f32>(max(target, 0.0));
    }
    return rgb * max(target / current, 0.0);
}

// ---------------------------------------------------------------------------
// highlight_recovery
// ---------------------------------------------------------------------------
struct HighlightRecoveryParams {
    clip: vec3<f32>,
    strength: f32,
};
@group(0) @binding(2) var<uniform> highlight_recovery_params: HighlightRecoveryParams;

@compute @workgroup_size(64)
fn stage_highlight_recovery(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let p = highlight_recovery_params;
    if (p.strength <= 0.0) { return; }

    var rgb = load(index);
    let clipped = vec3<f32>(
        select(0.0, 1.0, rgb.x >= p.clip.x),
        select(0.0, 1.0, rgb.y >= p.clip.y),
        select(0.0, 1.0, rgb.z >= p.clip.z),
    );
    let count = clipped.x + clipped.y + clipped.z;
    if (count == 0.0 || count == 3.0) { return; }

    let ratios = rgb / max(p.clip, vec3<f32>(1e-6));
    let unclipped = 3.0 - count;
    let mean_ratio = dot(ratios * (vec3<f32>(1.0) - clipped), vec3<f32>(1.0)) / unclipped;

    let estimate = max(p.clip * mean_ratio, p.clip);
    rgb = mix(rgb, max(estimate, rgb), clipped * p.strength);
    store(index, rgb);
}

// ---------------------------------------------------------------------------
// white_balance
// ---------------------------------------------------------------------------
struct WhiteBalanceParams {
    multipliers: vec3<f32>,
};
@group(0) @binding(3) var<uniform> white_balance_params: WhiteBalanceParams;

@compute @workgroup_size(64)
fn stage_white_balance(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    store(index, load(index) * white_balance_params.multipliers);
}

// ---------------------------------------------------------------------------
// camera_matrix
// ---------------------------------------------------------------------------
struct CameraMatrixParams {
    matrix: mat3x3<f32>,
};
@group(0) @binding(4) var<uniform> camera_matrix_params: CameraMatrixParams;

@compute @workgroup_size(64)
fn stage_camera_matrix(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    store(index, camera_matrix_params.matrix * load(index));
}

// ---------------------------------------------------------------------------
// lens_vignette
// ---------------------------------------------------------------------------
struct LensVignetteParams {
    amount: f32,
};
@group(0) @binding(5) var<uniform> lens_vignette_params: LensVignetteParams;

@compute @workgroup_size(64)
fn stage_lens_vignette(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let x = f32(index % frame.width);
    let y = f32(index / frame.width);
    let cx = (f32(frame.width) - 1.0) * 0.5;
    let cy = (f32(frame.height) - 1.0) * 0.5;
    let max_r = max(length(vec2<f32>(cx, cy)), 1e-6);
    let r = length(vec2<f32>(x - cx, y - cy)) / max_r;
    store(index, load(index) * (1.0 + 0.6 * lens_vignette_params.amount * r * r));
}

// ---------------------------------------------------------------------------
// lens_distortion and lens_ca moved to geometry.wgsl in PHASE-23.
//
// They stopped being point-wise the moment they became real: both gather from somewhere else
// in the source, and a stage that gathers cannot share a buffer with one that maps. Every
// other entry point in this file reads index `i` and writes index `i`.
// ---------------------------------------------------------------------------

