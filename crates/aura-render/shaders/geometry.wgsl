// PHASE-23. The three geometry resamples: lens distortion, lateral chromatic aberration, and the
// crop-rotate-perspective stage.
//
// The GPU half of `crates/aura-render/src/geometry.rs` and of `spatial::crop_rotate`.
// `crates/aura-render/tests/shader_parity.rs` holds the two paths to the same constants, and
// `crates/aura-render/tests/geometry_order.rs` holds this file to owning exactly the three entry
// points below.
//
// THIS FILE TOOK THREE ENTRY POINTS OVER FROM TWO OTHERS.
//
// `stage_lens_distortion` and `stage_lens_ca` were identity pass-throughs in `colour.wgsl`, and
// `stage_geometry` was the crop-and-rotate in `spatial.wgsl`. Phase 14 wrote all three so that
// `every_stage_has_an_entry_point` could pass before an operator existed; phase 20 retired its
// own pass-through when `retouch_apply.wgsl` landed and wrote down why, and this is the same
// move. Two entry points with one name, one of which does nothing, is exactly the drift the
// parity test exists to catch.
//
// WHY THESE THREE ARE ONE FILE
//
// Because they are one *idea*: every one of them is a coordinate map followed by a bilinear
// read, and the only thing that differs is where the coordinate comes from. Splitting them by
// pipeline position would put three copies of `sample_bilinear` in three files, and the day one
// of them acquired an edge-clamp the other two did not is the day a corrected frame has a dark
// rim on one operator and not the others - which is phase 18's `Plane::resize_bilinear` defect,
// written into a shader instead of a resampler.
//
// EVERYTHING HERE IS LINEAR
//
// Invariant 8. A coordinate map has no opinion about tone, and there is no encoding in this file.
//
// NEITHER LENS STAGE IS TILEABLE
//
// A distortion model is written against the FRAME'S radius and displaces a corner by a percent or
// two of the half-diagonal - tens of pixels at export size - so no fixed halo covers it.
// `graph::Stage::halo` still reports 8 for both, and `tiles::render_streamed` never consults it:
// a frame with either stage scheduled is rendered whole, exactly as a rotated one is. A device
// backend must take the same branch, which is why this file reads `frame.width` and
// `frame.height` and has no notion of a tile origin at all.

struct Frame {
    width: u32,
    height: u32,
};

@group(0) @binding(0) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(1) var<uniform> frame: Frame;

// The read-only copy the three maps sample from. Every operator here reads a pixel it does not
// write, so an in-place version would race: the source of an output pixel is somewhere else in
// the same buffer, and on a device "somewhere else" is another workgroup that may or may not have
// run. `crate::geometry` takes the same copy for the same reason.
@group(0) @binding(13) var<storage, read> geometry_source: array<f32>;

fn source_at(index: u32, channel: u32) -> f32 {
    return geometry_source[index * 3u + channel];
}

fn store(index: u32, value: vec3<f32>) {
    let base = index * 3u;
    pixels[base] = value.x;
    pixels[base + 1u] = value.y;
    pixels[base + 2u] = value.z;
}

// Bilinear read of one channel, CLAMPED at the edge.
//
// Clamped rather than zeroed, and the difference is the whole reason this helper is written out.
// An out-of-range read here is half a pixel of rounding on an operator that keeps the frame's
// size; zeroing it is a one-pixel dark rim on every corrected frame in the product, which is the
// defect phase 18 found in `Plane::resize_bilinear` and fixed. `stage_geometry` is the one
// exception and it does its own bounds test, because there an out-of-range read is a corner the
// rotation opened and the crop is about to remove.
fn sample_bilinear(x: f32, y: f32, channel: u32) -> f32 {
    let max_x = f32(frame.width) - 1.0;
    let max_y = f32(frame.height) - 1.0;
    let cx = clamp(x, 0.0, max_x);
    let cy = clamp(y, 0.0, max_y);
    let x0 = u32(floor(cx));
    let y0 = u32(floor(cy));
    let x1 = min(x0 + 1u, frame.width - 1u);
    let y1 = min(y0 + 1u, frame.height - 1u);
    let fx = cx - floor(cx);
    let fy = cy - floor(cy);
    let top = mix(source_at(y0 * frame.width + x0, channel),
                  source_at(y0 * frame.width + x1, channel), fx);
    let bottom = mix(source_at(y1 * frame.width + x0, channel),
                     source_at(y1 * frame.width + x1, channel), fx);
    return mix(top, bottom, fy);
}

// ---------------------------------------------------------------------------
// lens_distortion
//
// r_source = r_output * (1 + k1 r^2 + k2 r^4 + k3 r^6), in a radius normalised so the frame's
// corner sits at 1.0. `fill` is `geometry::fill_scale` - one for a barrel model, and below one
// for a pincushion one so that the corrected corner still reads inside the source. It is solved
// on the processor path and passed in rather than solved here: it is one number per render, and
// forty steps of bisection per pixel to recompute it would be forty steps of bisection per pixel.
// ---------------------------------------------------------------------------
struct LensDistortionParams {
    k1: f32,
    k2: f32,
    k3: f32,
    fill: f32,
};
@group(0) @binding(14) var<uniform> lens_distortion_params: LensDistortionParams;

fn distorted_radius(r: f32) -> f32 {
    let r2 = r * r;
    return r * (1.0
        + lens_distortion_params.k1 * r2
        + lens_distortion_params.k2 * r2 * r2
        + lens_distortion_params.k3 * r2 * r2 * r2);
}

@compute @workgroup_size(64)
fn stage_lens_distortion(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }

    let cx = (f32(frame.width) - 1.0) * 0.5;
    let cy = (f32(frame.height) - 1.0) * 0.5;
    let max_r = max(length(vec2<f32>(cx, cy)), 1e-6);
    let d = vec2<f32>(f32(index % frame.width) - cx, f32(index / frame.width) - cy);
    let r = length(d) / max_r;

    let fill = lens_distortion_params.fill;
    var gain = fill;
    if (r >= 1e-6) {
        gain = fill * distorted_radius(r * fill) / (r * fill);
    }
    let s = vec2<f32>(cx, cy) + d * gain;
    store(index, vec3<f32>(
        sample_bilinear(s.x, s.y, 0u),
        sample_bilinear(s.x, s.y, 1u),
        sample_bilinear(s.x, s.y, 2u)));
}

// ---------------------------------------------------------------------------
// lens_ca
//
// Red and blue scaled radially about the frame's centre; GREEN IS FIXED. Green fixed rather than
// all three moved, because green is where the luminance is: a model that moved all three would
// resample the sharpest channel in the frame in order to correct a defect in the other two.
//
// The scales are in the fourth decimal place - a fringe is a pixel or two even when it is
// obvious - so the sub-pixel read is the entire operation. A nearest-neighbour version of this
// entry point would be the identity on every frame it was given, while appearing to run.
// ---------------------------------------------------------------------------
struct LensCaParams {
    red: f32,
    blue: f32,
};
@group(0) @binding(15) var<uniform> lens_ca_params: LensCaParams;

@compute @workgroup_size(64)
fn stage_lens_ca(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }

    let cx = (f32(frame.width) - 1.0) * 0.5;
    let cy = (f32(frame.height) - 1.0) * 0.5;
    let d = vec2<f32>(f32(index % frame.width) - cx, f32(index / frame.width) - cy);

    let red_at = vec2<f32>(cx, cy) + d * (1.0 + lens_ca_params.red);
    let blue_at = vec2<f32>(cx, cy) + d * (1.0 + lens_ca_params.blue);
    store(index, vec3<f32>(
        sample_bilinear(red_at.x, red_at.y, 0u),
        source_at(index, 1u),
        sample_bilinear(blue_at.x, blue_at.y, 2u)));
}

// ---------------------------------------------------------------------------
// geometry - the perspective warp, the crop and the rotation
//
// One dispatch and one read per output pixel. The processor reference keeps the perspective warp
// as its own resample so that the crop rectangle a plan stored is the rectangle that is taken;
// here the two maps are composed, because composing two coordinate maps costs nothing and
// resampling twice costs a generation of detail. `crates/aura-render/tests/parity.rs` is what
// holds the composed form to the reference within `PARITY_TOLERANCE`.
//
// KEYSTONE_MAX_P is 0.20 and the magnification that hides the corners the warp opens is
// `1 / (1 - |p| - |q|)`, which is also `geometry::stretch_of` - the minimum zoom that keeps the
// frame filled and the anisotropy at the point where the warp is strongest are the same number.
// Both are computed on the processor path and passed in; recomputing them per pixel would be
// three divisions to arrive at a value that is constant over the dispatch.
// ---------------------------------------------------------------------------
struct GeometryParams {
    crop: vec4<f32>,
    rotate: f32,
    out_width: u32,
    out_height: u32,
    // The projective pair, already scaled by the frame's own aspect ratio, and the magnification
    // that fills the corners. `p = 0, q = 0, magnify = 1` is a frame with no keystone on it.
    p: f32,
    q: f32,
    magnify: f32,
};
@group(0) @binding(11) var<uniform> geometry_params: GeometryParams;
@group(0) @binding(12) var<storage, read_write> geometry_out: array<f32>;

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

    let radians = -radians(geometry_params.rotate);
    let s = sin(radians);
    let c = cos(radians);
    // Pixel centres, not edges. See `spatial::crop_rotate`.
    let ccx = (left + right - 1.0) * 0.5;
    let ccy = (top + bottom - 1.0) * 0.5;
    let ocx = (f32(out_w) - 1.0) * 0.5;
    let ocy = (f32(out_h) - 1.0) * 0.5;

    let dx = f32(index % out_w) - ocx;
    let dy = f32(index / out_w) - ocy;
    var sx = ccx + dx * c - dy * s;
    var sy = ccy + dx * s + dy * c;

    // The perspective warp, composed onto the crop and rotation rather than run before them.
    let fcx = (f32(frame.width) - 1.0) * 0.5;
    let fcy = (f32(frame.height) - 1.0) * 0.5;
    let u = (sx - fcx) / max(fcx, 1e-6);
    let v = (sy - fcy) / max(fcy, 1e-6);
    let denom = 1.0 + geometry_params.p * v + geometry_params.q * u;
    if (abs(denom) >= 1e-6) {
        sx = fcx + (u / (denom * geometry_params.magnify)) * fcx;
        sy = fcy + (v / (denom * geometry_params.magnify)) * fcy;
    }

    let base = index * 3u;
    if (sx < 0.0 || sy < 0.0 || sx > f32(frame.width) - 1.0 || sy > f32(frame.height) - 1.0) {
        // Outside the frame. Left black rather than clamped: a straightened frame whose corners
        // were filled by smearing the edge pixel is a frame whose corners are a lie, and the crop
        // is supposed to remove them. PHASE-23 section 2.2 puts filling them in phase 24.
        geometry_out[base] = 0.0;
        geometry_out[base + 1u] = 0.0;
        geometry_out[base + 2u] = 0.0;
        return;
    }
    geometry_out[base] = sample_bilinear(sx, sy, 0u);
    geometry_out[base + 1u] = sample_bilinear(sx, sy, 1u);
    geometry_out[base + 2u] = sample_bilinear(sx, sy, 2u);
}
