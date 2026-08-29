// The stages that read more than one pixel: noise reduction, clarity, texture, dehaze,
// sharpening, masks, retouch, restoration and geometry.
//
// Every one of them reads a neighbourhood, so every one of them declares a halo in
// `graph::Stage::halo` and the tiler decodes it. The three that need a frame-wide number -
// sharpening's normaliser, noise reduction's edge keeper, dehaze's floor - take it as a
// uniform rather than reducing over the buffer, which is the same decision `spatial::Stats`
// records on the processor side and for the same reason: a reduction over a tile is a
// different number in every tile.
//
// NO ATOMICS. Section 6.2 forbids them in colour maths. Nothing in this file writes to a
// location it did not compute, and the blur is separable into two passes that each read one
// buffer and write another.

struct Frame {
    width: u32,
    height: u32,
};

struct Stats {
    gradient_peak: f32,
    dark_floor: f32,
};

@group(0) @binding(0) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(1) var<uniform> frame: Frame;
@group(0) @binding(2) var<storage, read_write> plane: array<f32>;
@group(0) @binding(3) var<storage, read_write> scratch: array<f32>;
@group(0) @binding(4) var<uniform> stats: Stats;

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

// ---------------------------------------------------------------------------
// The separable box blur, three passes, edge-clamped. `blur_rows` and `blur_columns` are
// dispatched alternately by the host; `radius` comes in as a uniform.
// ---------------------------------------------------------------------------
struct BlurParams {
    radius: u32,
};
@group(0) @binding(5) var<uniform> blur_params: BlurParams;

@compute @workgroup_size(64)
fn blur_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let y = index / frame.width;
    let x = index % frame.width;
    let r = blur_params.radius;
    var sum = 0.0;
    var count = 0.0;
    for (var offset = 0u; offset <= r * 2u; offset = offset + 1u) {
        let sx = min(max(i32(x) + i32(offset) - i32(r), 0), i32(frame.width) - 1);
        sum = sum + plane[y * frame.width + u32(sx)];
        count = count + 1.0;
    }
    scratch[index] = sum / max(count, 1.0);
}

@compute @workgroup_size(64)
fn blur_columns(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let y = index / frame.width;
    let x = index % frame.width;
    let r = blur_params.radius;
    var sum = 0.0;
    var count = 0.0;
    for (var offset = 0u; offset <= r * 2u; offset = offset + 1u) {
        let sy = min(max(i32(y) + i32(offset) - i32(r), 0), i32(frame.height) - 1);
        sum = sum + scratch[u32(sy) * frame.width + x];
        count = count + 1.0;
    }
    plane[index] = sum / max(count, 1.0);
}

// ---------------------------------------------------------------------------
// noise_reduction
// ---------------------------------------------------------------------------
struct NoiseReductionParams {
    luminance: f32,
    chroma: f32,
    detail: f32,
};
@group(0) @binding(6) var<uniform> noise_reduction_params: NoiseReductionParams;

@compute @workgroup_size(64)
fn stage_noise_reduction(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let base = luma(rgb);
    let low = plane[index];
    let edge = min(scratch[index] / max(stats.gradient_peak, 1e-6), 1.0);
    let keep = pow(edge, 1.0 + noise_reduction_params.detail * 2.0);
    let blended = low + (base - low) * (keep + (1.0 - keep) * (1.0 - noise_reduction_params.luminance));
    store(index, set_luma(rgb, max(blended, 0.0)));
}

// ---------------------------------------------------------------------------
// clarity and texture - the same operation at two radii, in the log domain
// ---------------------------------------------------------------------------
struct LocalContrastParams {
    amount: f32,
    radius: u32,
};
@group(0) @binding(7) var<uniform> local_contrast_params: LocalContrastParams;

fn local_contrast(index: u32, amount: f32) {
    let rgb = load(index);
    let base = log(max(luma(rgb), 1e-5));
    let low = plane[index];
    let detail = base - low;
    store(index, set_luma(rgb, max(exp(low + detail * (1.0 + amount)), 0.0)));
}

@compute @workgroup_size(64)
fn stage_clarity(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    local_contrast(index, local_contrast_params.amount);
}

@compute @workgroup_size(64)
fn stage_texture(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    local_contrast(index, local_contrast_params.amount);
}

// ---------------------------------------------------------------------------
// dehaze
// ---------------------------------------------------------------------------
struct DehazeParams {
    amount: f32,
};
@group(0) @binding(8) var<uniform> dehaze_params: DehazeParams;

@compute @workgroup_size(64)
fn stage_dehaze(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let lift = stats.dark_floor * clamp(dehaze_params.amount, -1.0, 1.0);
    let rgb = load(index);
    store(index, max(rgb - vec3<f32>(lift), vec3<f32>(0.0)) / max(1.0 - lift, 1e-3));
}

// ---------------------------------------------------------------------------
// sharpen
// ---------------------------------------------------------------------------
struct SharpenParams {
    amount: f32,
    detail: f32,
    masking: f32,
    radius: f32,
};
@group(0) @binding(9) var<uniform> sharpen_params: SharpenParams;

@compute @workgroup_size(64)
fn stage_sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let base = luma(rgb);
    let low = plane[index];
    let high = base - low;
    let ceiling = 0.02 + sharpen_params.detail * 0.48;
    let limited = clamp(high, -ceiling, ceiling);
    let edge = scratch[index] / max(stats.gradient_peak, 1e-6);
    var mask = 1.0;
    if (sharpen_params.masking > 0.0) {
        mask = min(edge / max(sharpen_params.masking, 1e-3), 1.0);
    }
    store(index, set_luma(rgb, max(base + limited * sharpen_params.amount * mask, 0.0)));
}

// ---------------------------------------------------------------------------
// masks - the geometric ones only
//
// Linear and radial are pure geometry and run here. Face, subject, background, sky, skin and
// brush need a generator phase 18 ships; `graph::plan` refuses to schedule them and emits
// `SkipReason::MaskGeneratorAbsent`, so a caller is told rather than a mask silently doing
// nothing.
// ---------------------------------------------------------------------------
struct MaskParams {
    kind: u32,
    feather: f32,
    exposure: f32,
    saturation: f32,
};
@group(0) @binding(10) var<uniform> mask_params: MaskParams;

@compute @workgroup_size(64)
fn stage_masks(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let x = f32(index % frame.width) / f32(max(frame.width, 1u));
    let y = f32(index / frame.width) / f32(max(frame.height, 1u));

    var raw: f32;
    if (mask_params.kind == 0u) {
        raw = y;
    } else {
        let d = vec2<f32>((x - 0.5) / 0.35, (y - 0.5) / 0.35);
        raw = 1.0 - min(length(d), 1.0);
    }
    let feather = clamp(mask_params.feather, 0.01, 1.0);
    let weight = smoothstep(0.5 - feather * 0.5, 0.5 + feather * 0.5, raw);
    if (weight <= 0.0) { return; }

    var rgb = load(index);
    rgb = rgb * exp2(mask_params.exposure * weight);
    let l = luma(rgb);
    rgb = mix(vec3<f32>(l), rgb, 1.0 + mask_params.saturation * weight);
    store(index, rgb);
}

// ---------------------------------------------------------------------------
// retouch, restoration
//
// The identity, and deliberately so: the operators arrive with phases 20, 21 and 22.
// `graph::plan` emits `SkipReason::OperatorAbsent` and `SkipReason::RestorationAbsent`
// rather than scheduling either, so these entry points exist to keep the stage table
// exhaustive and to be filled in without touching the graph.
// ---------------------------------------------------------------------------
// PHASE-20 moved `stage_retouch` into `retouch_apply.wgsl`, where the operators live. What
// stood here was phase 14 pass-through: a stage that copied its input so that
// `every_stage_has_an_entry_point` would pass while nothing could retouch. Leaving it would
// mean two entry points with the same name and one of them doing nothing, which is exactly the
// drift `shader_parity.rs` exists to catch.


@compute @workgroup_size(64)
fn stage_restoration(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    store(index, load(index));
}

// ---------------------------------------------------------------------------
// geometry moved to geometry.wgsl in PHASE-23, together with the keystone it grew.
// ---------------------------------------------------------------------------

