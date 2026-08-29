// PHASE-21. Glare: the conservative reduction, and the composite of an aligned sibling patch.
//
// A LIBRARY, not a stage. `micro_apply.wgsl` is the other half and neither declares a
// `fn stage_` entry point; both run inside phase 20's retouch stage.
//
// THE ONE THING TO UNDERSTAND ABOUT THIS FILE
//
// It composites. It does not decide what may be composited. The specular test, the area cap, the
// alignment search and the refusal all happen in `aura_retouch::micro::borrow` before a patch
// reaches this shader, and the patch arrives as pixels rather than as a source image and a
// transform precisely so that the decision cannot be re-made here by accident.
//
// The rule those decisions implement, from ADR-0045 section 4: **you may only borrow pixels that
// carry no information.** `MIN_SPECULAR_FRACTION` of the target region has to sit at or above
// `MICRO_CLIPPED_FLOOR` before a borrow is permitted at all, and `MIN_ALIGNMENT` is the floor on
// how well the sibling matched. Both constants are here so that a backend that re-derived the
// gate would fail the parity test rather than deliver an undisclosed composite.
//
// Everything here is LINEAR and nothing encodes. Invariant 8. NO ATOMICS.

struct BorrowParams {
    width: u32,
    height: u32,
    // The window in the target frame, in pixels.
    origin: vec2<u32>,
    extent: vec2<u32>,
    // Conservative reduction: how far the sheet is pulled toward its own unclipped surround.
    // `MAX_GLARE_REDUCE` bounds it in the contract. Zero when this dispatch is a composite.
    reduce_strength: f32,
    // The mean luminance of the ring outside the window, excluding anything clipped. Measured on
    // the host: a whole-region mean is a reduction, and no shader in this product performs one.
    surround_luma: f32,
    // One when this dispatch composites `borrow_patch`, zero when it reduces.
    is_borrow: u32,
    // How well the sibling aligned. Carried for the same reason the operation carries it: a
    // borrow that lost its provenance is an undisclosed composite.
    alignment: f32,
};

@group(7) @binding(0) var<uniform> borrow_params: BorrowParams;
// The aligned donor region, interleaved linear RGB, `extent.x * extent.y * 3` long.
@group(7) @binding(1) var<storage, read> borrow_patch: array<f32>;
@group(7) @binding(2) var<storage, read_write> borrow_pixels: array<f32>;

fn borrow_load(index: u32) -> vec3<f32> {
    let base = index * 3u;
    return vec3<f32>(borrow_pixels[base], borrow_pixels[base + 1u], borrow_pixels[base + 2u]);
}

fn borrow_store(index: u32, value: vec3<f32>) {
    let base = index * 3u;
    borrow_pixels[base] = max(value.x, 0.0);
    borrow_pixels[base + 1u] = max(value.y, 0.0);
    borrow_pixels[base + 2u] = max(value.z, 0.0);
}

fn borrow_luma(rgb: vec3<f32>) -> f32 {
    return 0.2126 * rgb.x + 0.7152 * rgb.y + 0.0722 * rgb.z;
}

// The two gates a borrow passes on the host, present here so a backend cannot re-derive them
// differently. `micro::MIN_SPECULAR_FRACTION` and `micro::MIN_ALIGNMENT`.
const MIN_SPECULAR_FRACTION: f32 = 0.55;
const MIN_ALIGNMENT: f32 = 0.82;

// A radial feather over the window. The same shape `micro_apply.wgsl` uses; duplicated rather
// than shared because the two files are separate modules in this build and a backend that
// composed them into one would otherwise have two functions with one name.
fn borrow_feather(col: u32, row: u32, w: u32, h: u32) -> f32 {
    if (w == 0u || h == 0u) {
        return 0.0;
    }
    let cx = (f32(w) - 1.0) * 0.5;
    let cy = (f32(h) - 1.0) * 0.5;
    let dx = (f32(col) - cx) / max(cx, 1.0);
    let dy = (f32(row) - cy) / max(cy, 1.0);
    let distance = sqrt(dx * dx + dy * dy);
    let inner = 1.0 - 0.25;
    if (distance <= inner) {
        return 1.0;
    }
    if (distance >= 1.0) {
        return 0.0;
    }
    let t = clamp((1.0 - distance) / 0.25, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Pull a sheet's luminance toward the surround the host measured, and stop.
//
// It cannot reconstruct anything, deliberately. A sheet that has clipped completely comes out
// grey rather than resolved, which is honest: the alternative is inventing an eye, and that is
// the operation `docs/retouch-ethics.md` refuses.
fn borrow_reduce(index: u32, col: u32, row: u32) {
    let weight = borrow_feather(col, row, borrow_params.extent.x, borrow_params.extent.y)
        * borrow_params.reduce_strength;
    if (weight <= 0.0) {
        return;
    }
    let rgb = borrow_load(index);
    let value = borrow_luma(rgb);
    if (value <= borrow_params.surround_luma || value <= 1e-6) {
        return;
    }
    let wanted = value + (borrow_params.surround_luma - value) * weight;
    borrow_store(index, rgb * (wanted / value));
}

// Composite one aligned donor pixel, feathered.
fn borrow_composite(index: u32, local: u32, col: u32, row: u32) {
    if (borrow_params.alignment < MIN_ALIGNMENT) {
        // Never reached in a well-formed dispatch, because the host refuses the borrow before it
        // builds one. Present because a promise enforced in one layer is a promise until somebody
        // writes a second caller.
        return;
    }
    let weight = borrow_feather(col, row, borrow_params.extent.x, borrow_params.extent.y);
    if (weight <= 0.0) {
        return;
    }
    let base = local * 3u;
    let donor = vec3<f32>(borrow_patch[base], borrow_patch[base + 1u], borrow_patch[base + 2u]);
    let target = borrow_load(index);
    borrow_store(index, target + (donor - target) * weight);
}

fn borrow_pixel(col: u32, row: u32) {
    let x = borrow_params.origin.x + col;
    let y = borrow_params.origin.y + row;
    if (x >= borrow_params.width || y >= borrow_params.height) {
        return;
    }
    let index = y * borrow_params.width + x;
    let local = row * borrow_params.extent.x + col;
    if (borrow_params.is_borrow == 1u) {
        borrow_composite(index, local, col, row);
    } else {
        borrow_reduce(index, col, row);
    }
}
