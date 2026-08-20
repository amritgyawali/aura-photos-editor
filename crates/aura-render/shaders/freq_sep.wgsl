// PHASE-19. Frequency separation: taking a face apart so the form can be shaped and the pores
// cannot.
//
// Three bands, not two. A retoucher's separation is low and high, because the two things they
// want are to move form and to keep pores. This phase needs a third: section 6.3's
// mid-frequency evening reduces blotchy tonal patches without smoothing, and a blotch is not
// form and is not a pore. A two-band split puts it in whichever band the radius happened to
// catch.
//
// So `low` is a wide Gaussian, `mid` is the difference between a narrow Gaussian and the wide
// one, and `high` is everything the narrow one left behind. THE HIGH BAND IS NEVER WRITTEN
// AND NEVER RETURNED. There is no function in this file that produces it, no binding that
// holds it and no operator anywhere in this phase that could touch it - which is what makes
// "dodge and burn shapes form without touching skin texture" a property of the decomposition
// rather than a promise about the operators.
//
// GAUSSIAN RATHER THAN BILATERAL. Section 6.3 offers either. A bilateral filter is
// edge-preserving, which sounds better and is worse here: the edge it preserves on a face is
// the shadow terminator, and the terminator is precisely what the low band has to contain for
// shaping to be able to move it. A bilateral low band leaves the terminator in `mid`, and
// shaping then moves the fill without moving the edge - a lit face with a shadow painted on.
//
// The Rust reference is `aura_brain_photo::local::freqsep`, which measures the bands to decide
// how far the evening may go. This is the application side.
//
// NO ATOMICS. The blur is separable into two passes that each read one buffer and write
// another.

// The wide radius, as a fraction of the face region's shorter side. A twelfth: wide enough
// that a cheekbone and the hollow under it land in different places on the low band, narrow
// enough that the jawline survives it.
const LOW_RADIUS_FRAC: f32 = 0.08333333;

// The narrow radius. A sixtieth - about two pixels on a face crop from a 2048 px proxy, which
// is below the scale of a blotch and above the scale of a pore.
const HIGH_RADIUS_FRAC: f32 = 0.01666667;

struct FaceRegion {
    // The face box in frame coordinates.
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    // The frame's dimensions, so a radius in fractions becomes a radius in samples.
    width: u32,
    height: u32,
};

@group(1) @binding(0) var<storage, read_write> band_low: array<f32>;
@group(1) @binding(1) var<storage, read_write> band_mid: array<f32>;
@group(1) @binding(2) var<storage, read_write> band_scratch: array<f32>;
@group(1) @binding(3) var<uniform> region: FaceRegion;

fn region_side() -> f32 {
    return min(region.w * f32(region.width), region.h * f32(region.height));
}

fn low_radius() -> u32 {
    return max(u32(round(region_side() * LOW_RADIUS_FRAC)), 1u);
}

fn high_radius() -> u32 {
    return max(u32(round(region_side() * HIGH_RADIUS_FRAC)), 1u);
}

// One horizontal box pass. Three of these plus three vertical ones is a very good
// approximation of a Gaussian and is O(n) in the radius rather than O(r), which is what keeps
// section 11's 80 ms reachable on a four-hundred-pixel face crop.
fn box_h(src: ptr<storage, array<f32>, read_write>, index: u32, radius: u32, w: u32) -> f32 {
    let x = index % w;
    let row = index - x;
    let lo = select(x - radius, 0u, x < radius);
    let hi = min(x + radius, w - 1u);
    var sum = 0.0;
    for (var i = lo; i <= hi; i = i + 1u) {
        sum = sum + (*src)[row + i];
    }
    return sum / f32(hi - lo + 1u);
}

fn box_v(src: ptr<storage, array<f32>, read_write>, index: u32, radius: u32, w: u32, h: u32) -> f32 {
    let x = index % w;
    let y = index / w;
    let lo = select(y - radius, 0u, y < radius);
    let hi = min(y + radius, h - 1u);
    var sum = 0.0;
    for (var i = lo; i <= hi; i = i + 1u) {
        sum = sum + (*src)[i * w + x];
    }
    return sum / f32(hi - lo + 1u);
}

// The mid band's energy contribution from one sample: mean absolute deviation.
//
// Mean absolute rather than RMS, because RMS is dominated by the handful of samples at a
// shadow terminator and this has to describe the whole crop. The reduction itself happens on
// the processor side - a reduction over a tile is a different number in every tile, which is
// the same reason `spatial::Stats` exists.
fn mid_energy_sample(index: u32) -> f32 {
    return abs(band_mid[index]);
}
