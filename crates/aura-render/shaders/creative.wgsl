// ADR-0070. Lightroom's remaining creative panels: calibration, colour grading, the post-crop
// vignette and film grain.
//
// Every entry point mirrors a function in `crate::creative` and shares its constants with it -
// the grade tint 0.60, the grade luminance travel 0.60 stops, the vignette's 1.50 stops at the
// corner and the grain strength 0.10 - which `shader_parity` compares on every build. The
// parametric and RGB curves need no entry point of their own: the host composes them into the
// tables `stage_curve` already samples.

struct Frame {
    width: u32,
    height: u32,
    // Where this dispatch sits inside the delivered (cropped) frame, for the two post-crop
    // effects. Zero and the dispatch's own size for a whole-frame render.
    origin_x: u32,
    origin_y: u32,
    full_width: u32,
    full_height: u32,
};

@group(0) @binding(0) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(1) var<uniform> frame: Frame;

const CURVE_GAMMA: f32 = 2.2;
const GRADE_TINT: f32 = 0.60;
const GRADE_LUMA_STOPS: f32 = 0.60;
const VIGNETTE_STOPS: f32 = 1.50;
const GRAIN_STRENGTH: f32 = 0.10;

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

fn encode(linear: f32) -> f32 {
    return select(pow(linear, 1.0 / CURVE_GAMMA), 0.0, linear <= 0.0);
}

fn decode(encoded: f32) -> f32 {
    return select(pow(encoded, CURVE_GAMMA), 0.0, encoded <= 0.0);
}

fn smooth_edge(x: f32, e0: f32, e1: f32) -> f32 {
    let t = clamp((x - e0) / max(e1 - e0, 1e-6), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// ---------------------------------------------------------------------------
// calibration: a white-preserving 3x3 built on the host, plus a shadow tint
// ---------------------------------------------------------------------------
struct CalibrationParams {
    row0: vec4<f32>,
    row1: vec4<f32>,
    row2: vec4<f32>,
    shadow_tint: f32,
};
@group(0) @binding(2) var<uniform> calibration_params: CalibrationParams;

@compute @workgroup_size(64)
fn stage_calibration(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let v = load(index);
    var out = vec3<f32>(
        dot(calibration_params.row0.xyz, v),
        dot(calibration_params.row1.xyz, v),
        dot(calibration_params.row2.xyz, v),
    );
    let l = luma(out);
    let shadow = 1.0 - smooth_edge(encode(max(l, 0.0)), 0.0, 0.5);
    out.y = max(out.y * (1.0 - 0.25 * calibration_params.shadow_tint * shadow), 0.0);
    store(index, set_luma(out, l));
}

// ---------------------------------------------------------------------------
// colour grading: four wheels, weights that sum to one
// ---------------------------------------------------------------------------
struct GradeParams {
    // xyz is the luminance-normalised tint offset, w the saturation; one per wheel.
    shadows: vec4<f32>,
    midtones: vec4<f32>,
    highlights: vec4<f32>,
    global_wheel: vec4<f32>,
    // x shadows, y midtones, z highlights, w global luminance.
    luminance: vec4<f32>,
    pivot: f32,
    soft: f32,
};
@group(0) @binding(3) var<uniform> grade_params: GradeParams;

@compute @workgroup_size(64)
fn stage_colour_grade(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let l = luma(rgb);
    if (l <= 1e-6) { return; }
    let t = clamp(encode(l), 0.0, 1.0);
    let ws = 1.0 - smooth_edge(t, grade_params.pivot - grade_params.soft, grade_params.pivot);
    let wh = smooth_edge(t, grade_params.pivot, grade_params.pivot + grade_params.soft);
    let wm = max(1.0 - ws - wh, 0.0);
    let offset = ws * grade_params.shadows.w * grade_params.shadows.xyz
        + wm * grade_params.midtones.w * grade_params.midtones.xyz
        + wh * grade_params.highlights.w * grade_params.highlights.xyz
        + grade_params.global_wheel.w * grade_params.global_wheel.xyz;
    let stops = ws * grade_params.luminance.x + wm * grade_params.luminance.y
        + wh * grade_params.luminance.z + grade_params.luminance.w;
    let tinted = max(rgb * (vec3<f32>(1.0) + GRADE_TINT * offset), vec3<f32>(0.0));
    store(index, set_luma(tinted, l * exp2(stops * GRADE_LUMA_STOPS)));
}

// ---------------------------------------------------------------------------
// the post-crop vignette
// ---------------------------------------------------------------------------
struct VignetteParams {
    amount: f32,
    lo: f32,
    hi: f32,
    scale_u: f32,
    scale_v: f32,
    power: f32,
    highlights: f32,
};
@group(0) @binding(4) var<uniform> vignette_params: VignetteParams;

@compute @workgroup_size(64)
fn stage_post_crop_vignette(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let px = f32(index % frame.width + frame.origin_x) + 0.5;
    let py = f32(index / frame.width + frame.origin_y) + 0.5;
    let u = px / f32(max(frame.full_width, 1u)) * 2.0 - 1.0;
    let v = py / f32(max(frame.full_height, 1u)) * 2.0 - 1.0;
    let p = vignette_params.power;
    let d = pow(pow(abs(u / vignette_params.scale_u), p) + pow(abs(v / vignette_params.scale_v), p), 1.0 / p)
        / pow(2.0, 1.0 / p);
    let rgb = load(index);
    var weight = smooth_edge(d, vignette_params.lo, vignette_params.hi);
    if (vignette_params.highlights > 0.0 && vignette_params.amount < 0.0) {
        weight = weight * (1.0 - vignette_params.highlights * smooth_edge(encode(max(luma(rgb), 0.0)), 0.55, 0.95));
    }
    store(index, rgb * exp2(vignette_params.amount * VIGNETTE_STOPS * weight));
}

// ---------------------------------------------------------------------------
// grain: value noise over a lattice hashed on the host's constants
// ---------------------------------------------------------------------------
struct GrainParams {
    amount: f32,
    cell: f32,
    rough: f32,
};
@group(0) @binding(5) var<uniform> grain_params: GrainParams;
// The host uploads the lattice values for the region a dispatch covers, because WGSL has no
// 64-bit integers to run the processor path's hash in. One value per lattice point, two octaves.
@group(0) @binding(6) var<storage, read> lattice_fine: array<f32>;
@group(0) @binding(7) var<storage, read> lattice_coarse: array<f32>;

@compute @workgroup_size(64)
fn stage_grain(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let l = luma(rgb);
    if (l <= 1e-6) { return; }
    // The noise value itself is interpolated on the host into the lattice buffers at pixel
    // resolution for this dispatch; the shader applies it exactly as `creative::grain` does.
    let n = lattice_fine[index] * (1.0 - 0.5 * grain_params.rough)
        + lattice_coarse[index] * 0.5 * grain_params.rough;
    let t = clamp(encode(l), 0.0, 1.0);
    let midtones = 4.0 * t * (1.0 - t);
    let shifted = clamp(t + n * grain_params.amount * GRAIN_STRENGTH * midtones, 0.0, 1.0);
    store(index, set_luma(rgb, decode(shifted)));
}
