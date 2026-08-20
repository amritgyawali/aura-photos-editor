// PHASE-20. Frequency separation, three bands, for a device.
//
// The GPU half of `crates/aura-render/src/bands.rs`. Phase 19 shipped `freq_sep.wgsl`, which
// returns two bands and deliberately cannot reach the third; this file returns all three,
// because phase 20 has to *measure* the high band to enforce the texture guarantee.
//
// A LIBRARY, not a stage. It declares no `fn stage_` entry point, and
// `tests/shader_parity.rs::every_entry_point_names_a_stage` is what keeps that true.
//
// Everything here is LINEAR. A band is a difference of blurs of scene-referred values, and
// there is no transfer function anywhere in this file. Invariant 8.
//
// NO ATOMICS and no workgroup reduction. Every invocation writes only the sample it computed;
// the energy sums the guard needs are reduced on the host, in a fixed order, because a
// non-deterministic reduction would make the texture ratio differ between two runs of the same
// plan - and that number is stored, compared and gated.

struct BandParams {
    width: u32,
    height: u32,
    // Blur radii in samples, already derived on the host from the fractions below so that the
    // two paths cannot round them differently.
    low_radius: u32,
    high_radius: u32,
};

@group(3) @binding(0) var<uniform> band_params: BandParams;
@group(3) @binding(1) var<storage, read> band_source: array<f32>;
@group(3) @binding(2) var<storage, read_write> band_low: array<f32>;
@group(3) @binding(3) var<storage, read_write> band_mid: array<f32>;
@group(3) @binding(4) var<storage, read_write> band_high: array<f32>;
@group(3) @binding(5) var<storage, read_write> band_scratch: array<f32>;

// The wide radius, as a fraction of the shorter side of the region. A twelfth: wide enough
// that a cheekbone and the hollow under it land in different places on the low band, narrow
// enough that a jawline survives it. `bands::LOW_RADIUS_FRAC`.
const LOW_RADIUS_FRAC: f32 = 0.0833;

// The narrow radius. A sixtieth - about two pixels on a face crop from a 2048 px proxy, which
// is below the scale of a blotch and above the scale of a pore. That IS the definition of the
// boundary between mid and high. `bands::HIGH_RADIUS_FRAC`.
const HIGH_RADIUS_FRAC: f32 = 0.0167;

// Three passes of a box filter approximate a Gaussian closely and cost the same at any radius.
// Two is visibly boxy at these radii; four costs a third more for a difference no measurement
// in either phase can see. `bands::BOX_PASSES`.
const BOX_PASSES: u32 = 3u;

fn band_index(x: u32, y: u32) -> u32 {
    return y * band_params.width + x;
}

// One horizontal box pass. Edges clamp rather than wrap: a wrapped sample at the edge of a
// face crop is the other side of the face, which is a different plane of skin under different
// light, and it shows as a bright rim - the same failure phase 18 found in its resampler.
@compute @workgroup_size(64)
fn band_blur_h(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= band_params.width * band_params.height) { return; }
    let x = index % band_params.width;
    let y = index / band_params.width;
    let radius = band_params.low_radius;

    let lo = select(x - radius, 0u, x < radius);
    let hi = min(x + radius, band_params.width - 1u);
    var sum = 0.0;
    var i = lo;
    loop {
        if (i > hi) { break; }
        sum = sum + band_source[band_index(i, y)];
        i = i + 1u;
    }
    band_scratch[index] = sum / f32(hi - lo + 1u);
}

@compute @workgroup_size(64)
fn band_blur_v(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= band_params.width * band_params.height) { return; }
    let x = index % band_params.width;
    let y = index / band_params.width;
    let radius = band_params.low_radius;

    let lo = select(y - radius, 0u, y < radius);
    let hi = min(y + radius, band_params.height - 1u);
    var sum = 0.0;
    var i = lo;
    loop {
        if (i > hi) { break; }
        sum = sum + band_scratch[band_index(x, i)];
        i = i + 1u;
    }
    band_low[index] = sum / f32(hi - lo + 1u);
}

// The split, once both blurs are in place: `low` is the wide blur, `mid` is what the narrow
// blur has that the wide one does not, and `high` is everything the narrow blur left behind.
//
// The three sum back to the input exactly, which is the property the whole phase rests on:
// tone evening scales `mid` and reconstructs, so an untouched sample comes back unchanged and
// the texture ratio of an evening-only plan is exactly one.
@compute @workgroup_size(64)
fn band_split(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= band_params.width * band_params.height) { return; }
    let source = band_source[index];
    let narrow = band_scratch[index];
    let low = band_low[index];
    band_mid[index] = narrow - low;
    band_high[index] = source - narrow;
}
