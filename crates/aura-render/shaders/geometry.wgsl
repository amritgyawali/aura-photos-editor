// Geometry - lens distortion, lateral chromatic aberration, keystone, crop and rotate.
// PHASE-23.
//
// Four entry points that resample rather than transform a pixel in place, which is why they
// live in their own file: every other shader in the product reads index `i` and writes index
// `i`, and these read somewhere else entirely. A stage that gathers cannot share a buffer with
// one that maps, and mixing the two in one file is how the first GPU backend discovers that
// `stage_lens_distortion` was reading its own output half the time.
//
// Held to `aura_render::geometry` and `aura_raw::colour::lens` by `shader_parity.rs`. This
// build links no `wgpu`, so nothing here is executed - the value is entirely in the timing.
// Shipping shaders nobody compiles is how a GPU backend arrives a year later against a
// reference that has moved twice.
//
// THE CONVENTION EVERY COEFFICIENT IS EXPRESSED IN
//
// Radius is normalised by the HALF-DIAGONAL, so r = 1 is exactly the corner whatever the
// aspect ratio. That is what makes a coefficient measured on a 3:2 body usable on the same
// lens mounted to a 4:3 one, and it is what `assets/lens_profiles/` documents.
//
// NOTHING IS EVER FILLED
//
// A barrel correction pulls content in from beyond the frame edge and a keystone opens two
// corners. Both are handled by scaling until nothing samples outside, and whatever still falls
// out is left black - never smeared from the edge pixel. A corner filled by smearing is a
// corner that is a lie, and the crop is what removes it. Generating content is phase 24.

struct Frame {
    width: u32,
    height: u32,
};
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;

// Shared with `aura_raw::colour::lens::BISECTION_STEPS`.
const BISECTION_STEPS: i32 = 24;

fn frame_aspect() -> f32 {
    return f32(frame.width) / max(f32(frame.height), 1.0);
}

fn half_diagonal(aspect: f32) -> f32 {
    return sqrt(aspect * aspect + 1.0) * 0.5;
}

// The radial distortion model. Maps an undistorted radius to the distorted one it should be
// sampled from - the direction a resampler walks. Positive k1 is barrel.
fn radial(k: vec3<f32>, r: f32) -> f32 {
    let r2 = r * r;
    return r * (1.0 + k.x * r2 + k.y * r2 * r2 + k.z * r2 * r2 * r2);
}

// Where one normalised point in the corrected frame comes from in the source.
fn source_of(point: vec2<f32>, k: vec3<f32>, aspect: f32, scale: f32) -> vec2<f32> {
    let hd = half_diagonal(aspect);
    let d = vec2<f32>((point.x - 0.5) * aspect * scale, (point.y - 0.5) * scale);
    let r = length(d) / hd;
    if (r <= 1e-7) { return vec2<f32>(0.5, 0.5); }
    let ratio = radial(k, r) / r;
    return vec2<f32>(0.5 + d.x * ratio / aspect, 0.5 + d.y * ratio);
}

// Bilinear sample of one channel at a normalised position. Outside the frame is black.
fn sample_channel(at: vec2<f32>, channel: u32) -> f32 {
    let w = f32(frame.width);
    let h = f32(frame.height);
    let nx = at.x * w - 0.5;
    let ny = at.y * h - 0.5;
    if (nx < -0.5 || ny < -0.5 || nx > w - 0.5 || ny > h - 0.5) { return 0.0; }
    let x0 = u32(max(floor(nx), 0.0));
    let y0 = u32(max(floor(ny), 0.0));
    let x1 = min(x0 + 1u, frame.width - 1u);
    let y1 = min(y0 + 1u, frame.height - 1u);
    let fx = clamp(nx - f32(x0), 0.0, 1.0);
    let fy = clamp(ny - f32(y0), 0.0, 1.0);
    let a = mix(src[(y0 * frame.width + x0) * 3u + channel],
                src[(y0 * frame.width + x1) * 3u + channel], fx);
    let b = mix(src[(y1 * frame.width + x0) * 3u + channel],
                src[(y1 * frame.width + x1) * 3u + channel], fx);
    return mix(a, b, fy);
}

fn pixel_centre(index: u32) -> vec2<f32> {
    return vec2<f32>(
        (f32(index % frame.width) + 0.5) / f32(frame.width),
        (f32(index / frame.width) + 0.5) / f32(frame.height),
    );
}

// ---------------------------------------------------------------------------
// lens_distortion
//
// `scale` is `aura_raw::colour::lens::valid_scale`, computed once on the host: it is a binary
// search over the destination boundary and it is the same for every pixel in the frame, so
// running it per invocation would be sixty-four boundary evaluations per pixel to arrive at
// the number the host already had.
// ---------------------------------------------------------------------------
struct LensDistortionParams {
    k: vec3<f32>,
    scale: f32,
};
@group(0) @binding(3) var<uniform> lens_distortion_params: LensDistortionParams;

@compute @workgroup_size(64)
fn stage_lens_distortion(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let from = source_of(
        pixel_centre(index),
        lens_distortion_params.k,
        frame_aspect(),
        lens_distortion_params.scale,
    );
    let base = index * 3u;
    dst[base + 0u] = sample_channel(from, 0u);
    dst[base + 1u] = sample_channel(from, 1u);
    dst[base + 2u] = sample_channel(from, 2u);
}

// ---------------------------------------------------------------------------
// lens_ca
//
// A pure radial scale per channel about the frame's centre. GREEN IS NEVER SCALED: it is the
// channel the sensor has twice as many of and the one a focus system was aimed with, so
// scaling it would move the whole image rather than register the other two against it.
// ---------------------------------------------------------------------------
struct LensCaParams {
    ca_red: f32,
    ca_blue: f32,
};
@group(0) @binding(4) var<uniform> lens_ca_params: LensCaParams;

fn radial_scale(point: vec2<f32>, scale: f32, aspect: f32) -> vec2<f32> {
    let d = vec2<f32>((point.x - 0.5) * aspect, point.y - 0.5);
    return vec2<f32>(0.5 + d.x * scale / aspect, 0.5 + d.y * scale);
}

@compute @workgroup_size(64)
fn stage_lens_ca(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let point = pixel_centre(index);
    let aspect = frame_aspect();
    let base = index * 3u;
    dst[base + 0u] = sample_channel(radial_scale(point, lens_ca_params.ca_red, aspect), 0u);
    dst[base + 1u] = src[base + 1u];
    dst[base + 2u] = sample_channel(radial_scale(point, lens_ca_params.ca_blue, aspect), 2u);
}

// ---------------------------------------------------------------------------
// geometry - keystone, then crop and rotate, in one gather
//
// The keystone is applied SYMMETRICALLY - the narrow end widened by the square root of the
// stretch and the wide end narrowed by the same - so the frame's own scale does not move and
// the inscribed rectangle is bounded by exactly that square root.
// ---------------------------------------------------------------------------
struct GeometryParams {
    crop: vec4<f32>,
    rotate: f32,
    keystone_v: f32,
    keystone_h: f32,
    keystone_scale: f32,
    out_width: u32,
    out_height: u32,
};
@group(0) @binding(5) var<uniform> geometry_params: GeometryParams;
@group(0) @binding(6) var<storage, read_write> geometry_out: array<f32>;

fn keystone_source(point: vec2<f32>, vertical: f32, horizontal: f32, scale: f32) -> vec2<f32> {
    let s = select(scale, 1.0, abs(scale) < 1e-7);
    let c = vec2<f32>((point.x - 0.5) / s, (point.y - 0.5) / s);
    let v = vertical / 100.0;
    let h = horizontal / 100.0;
    let width_at = 1.0 + v * (-2.0 * c.y);
    let height_at = 1.0 + h * (-2.0 * c.x);
    let sx = select(c.x / width_at, c.x, abs(width_at) < 1e-4);
    let sy = select(c.y / height_at, c.y, abs(height_at) < 1e-4);
    return vec2<f32>(0.5 + sx, 0.5 + sy);
}

@compute @workgroup_size(64)
fn stage_geometry(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    let out_w = geometry_params.out_width;
    let out_h = geometry_params.out_height;
    if (index >= out_w * out_h) { return; }

    let left = geometry_params.crop.x * f32(frame.width);
    let top = geometry_params.crop.y * f32(frame.height);
    let right = geometry_params.crop.z * f32(frame.width);
    let bottom = geometry_params.crop.w * f32(frame.height);

    let rad = -radians(geometry_params.rotate);
    let s = sin(rad);
    let c = cos(rad);
    // Pixel centres, not edges. Half a pixel of shift on every crop is a resample nobody
    // asked for on an untouched frame. See `spatial::crop_rotate`.
    let ccx = (left + right - 1.0) * 0.5;
    let ccy = (top + bottom - 1.0) * 0.5;
    let ocx = (f32(out_w) - 1.0) * 0.5;
    let ocy = (f32(out_h) - 1.0) * 0.5;

    let dx = f32(index % out_w) - ocx;
    let dy = f32(index / out_w) - ocy;
    var sx = ccx + dx * c - dy * s;
    var sy = ccy + dx * s + dy * c;

    // The keystone runs in normalised coordinates, before the crop is applied: a crop is
    // expressed against the CORRECTED frame, which is what `aura_geometry` decided it against.
    if (abs(geometry_params.keystone_v) > 1e-7 || abs(geometry_params.keystone_h) > 1e-7) {
        let normalised = vec2<f32>(sx / f32(frame.width), sy / f32(frame.height));
        let moved = keystone_source(
            normalised,
            geometry_params.keystone_v,
            geometry_params.keystone_h,
            geometry_params.keystone_scale,
        );
        sx = moved.x * f32(frame.width);
        sy = moved.y * f32(frame.height);
    }

    let base = index * 3u;
    if (sx < 0.0 || sy < 0.0 || sx > f32(frame.width) - 1.0 || sy > f32(frame.height) - 1.0) {
        geometry_out[base + 0u] = 0.0;
        geometry_out[base + 1u] = 0.0;
        geometry_out[base + 2u] = 0.0;
        return;
    }
    let at = vec2<f32>((sx + 0.5) / f32(frame.width), (sy + 0.5) / f32(frame.height));
    geometry_out[base + 0u] = sample_channel(at, 0u);
    geometry_out[base + 1u] = sample_channel(at, 1u);
    geometry_out[base + 2u] = sample_channel(at, 2u);
}
