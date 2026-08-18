// The creative point-wise half: exposure, tone, contrast, curve, HSL, vibrance and the
// black-and-white mix.
//
// Every one of these mirrors a function in `crate::tonemap` and is held to it by
// `crates/aura-render/tests/shader_parity.rs`. The constants - middle grey at 0.18, the
// curve-domain gamma at 2.2, the four band centres, the 0.6 travel limit - appear in both
// places and the test compares them, because a constant that drifts between the two is the
// hardest kind of parity failure to find: the image is nearly right.

struct Frame {
    width: u32,
    height: u32,
};

@group(0) @binding(0) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(1) var<uniform> frame: Frame;

const MID_GREY: f32 = 0.18;
const CURVE_GAMMA: f32 = 2.2;

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

// The curve domain: an invertible change of coordinates, not an output transform. See
// `tonemap`'s module documentation.
fn curve_domain_encode(linear: f32) -> f32 {
    return select(pow(linear, 1.0 / CURVE_GAMMA), 0.0, linear <= 0.0);
}

fn curve_domain_decode(encoded: f32) -> f32 {
    return select(pow(encoded, CURVE_GAMMA), 0.0, encoded <= 0.0);
}

fn band(x: f32, centre: f32, width: f32) -> f32 {
    let d = abs((x - centre) / width);
    if (d >= 1.0) { return 0.0; }
    let t = 1.0 - d;
    return t * t * (3.0 - 2.0 * t);
}

// ---------------------------------------------------------------------------
// exposure
// ---------------------------------------------------------------------------
struct ExposureParams {
    stops: f32,
};
@group(0) @binding(2) var<uniform> exposure_params: ExposureParams;

@compute @workgroup_size(64)
fn stage_exposure(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    store(index, load(index) * exp2(exposure_params.stops));
}

// ---------------------------------------------------------------------------
// tone
// ---------------------------------------------------------------------------
struct ToneParams {
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
};
@group(0) @binding(3) var<uniform> tone_params: ToneParams;

@compute @workgroup_size(64)
fn stage_tone(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let l = luma(rgb);
    if (l <= 0.0) { return; }
    let e = clamp(curve_domain_encode(l), 0.0, 2.0);

    let gain = (1.0 + 0.6 * tone_params.whites * band(e, 0.82, 0.30))
             * (1.0 + 0.6 * tone_params.highlights * band(e, 0.62, 0.26))
             * (1.0 + 0.6 * tone_params.shadows * band(e, 0.33, 0.26))
             * (1.0 + 0.6 * tone_params.blacks * band(e, 0.12, 0.22));

    store(index, set_luma(rgb, max(l * gain, 0.0)));
}

// ---------------------------------------------------------------------------
// contrast
// ---------------------------------------------------------------------------
struct ContrastParams {
    amount: f32,
};
@group(0) @binding(4) var<uniform> contrast_params: ContrastParams;

@compute @workgroup_size(64)
fn stage_contrast(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let l = luma(rgb);
    if (l <= 0.0) { return; }
    let pivot = curve_domain_encode(MID_GREY);
    let e = curve_domain_encode(l);
    let amount = contrast_params.amount;
    let slope = select(1.0 / (1.0 - amount), 1.0 + amount, amount >= 0.0);
    let shifted = pivot * pow(max(e / max(pivot, 1e-6), 0.0), slope);
    store(index, set_luma(rgb, curve_domain_decode(shifted)));
}

// ---------------------------------------------------------------------------
// curve
//
// The lookup table is uploaded rather than evaluated: the Fritsch-Carlson fit is done once
// on the processor in `tonemap::CurveLut::build`, so both paths read the same 1,024 entries
// and cannot disagree about the interpolation.
// ---------------------------------------------------------------------------
struct CurveParams {
    entries: u32,
};
@group(0) @binding(5) var<uniform> curve_params: CurveParams;
@group(0) @binding(6) var<storage, read> curve_table: array<f32>;

@compute @workgroup_size(64)
fn stage_curve(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let l = luma(rgb);
    if (l <= 0.0) { return; }
    let x = curve_domain_encode(l);
    let last = curve_params.entries - 1u;

    var mapped: f32;
    if (x >= 1.0) {
        let top = curve_table[last];
        let below = curve_table[last - 1u];
        mapped = top + (x - 1.0) * (top - below) * f32(last);
    } else {
        let position = max(x, 0.0) * f32(last);
        let i = u32(floor(position));
        let frac = position - f32(i);
        mapped = mix(curve_table[i], curve_table[min(i + 1u, last)], frac);
    }
    store(index, set_luma(rgb, curve_domain_decode(mapped)));
}

// ---------------------------------------------------------------------------
// hsl
// ---------------------------------------------------------------------------
struct HslParams {
    hue: array<vec4<f32>, 2>,
    saturation: array<vec4<f32>, 2>,
    luminance: array<vec4<f32>, 2>,
};
@group(0) @binding(7) var<uniform> hsl_params: HslParams;

fn to_hsv(rgb: vec3<f32>) -> vec3<f32> {
    let mx = max(rgb.x, max(rgb.y, rgb.z));
    let mn = min(rgb.x, min(rgb.y, rgb.z));
    let delta = mx - mn;
    if (delta <= 1e-9 || mx <= 0.0) {
        return vec3<f32>(0.0, 0.0, mx);
    }
    var hue: f32;
    if (mx == rgb.x) {
        hue = (rgb.y - rgb.z) / delta;
        hue = hue - 6.0 * floor(hue / 6.0);
    } else if (mx == rgb.y) {
        hue = (rgb.z - rgb.x) / delta + 2.0;
    } else {
        hue = (rgb.x - rgb.y) / delta + 4.0;
    }
    return vec3<f32>(hue / 6.0, delta / mx, mx);
}

fn band_weight(hue: f32, index: u32) -> f32 {
    let centre = f32(index) / 8.0;
    var d = abs(hue - centre);
    if (d > 0.5) { d = 1.0 - d; }
    let w = max(1.0 - d * 8.0, 0.0);
    return w * w * (3.0 - 2.0 * w);
}

fn shift_at(block: array<vec4<f32>, 2>, index: u32) -> f32 {
    let v = block[index / 4u];
    switch (index % 4u) {
        case 0u: { return v.x; }
        case 1u: { return v.y; }
        case 2u: { return v.z; }
        default: { return v.w; }
    }
}

@compute @workgroup_size(64)
fn stage_hsl(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    var rgb = load(index);
    let hsv = to_hsv(rgb);
    if (hsv.y <= 1e-4) { return; }

    var total = 0.0;
    var h_shift = 0.0;
    var s_shift = 0.0;
    var l_shift = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        let w = band_weight(hsv.x, i);
        total = total + w;
        h_shift = h_shift + w * shift_at(hsl_params.hue, i);
        s_shift = s_shift + w * shift_at(hsl_params.saturation, i);
        l_shift = l_shift + w * shift_at(hsl_params.luminance, i);
    }
    if (total > 1e-6) {
        h_shift = h_shift / total;
        s_shift = s_shift / total;
        l_shift = l_shift / total;
    }

    let l = luma(rgb);
    let saturated = mix(vec3<f32>(l), rgb, 1.0 + s_shift / 100.0);
    store(index, set_luma(saturated, max(l * (1.0 + l_shift / 100.0 * 0.5), 0.0)));
}

// ---------------------------------------------------------------------------
// vibrance
// ---------------------------------------------------------------------------
struct VibranceParams {
    vibrance: f32,
    saturation: f32,
};
@group(0) @binding(8) var<uniform> vibrance_params: VibranceParams;

@compute @workgroup_size(64)
fn stage_vibrance(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    var rgb = load(index);
    if (vibrance_params.vibrance != 0.0) {
        let s = to_hsv(rgb).y;
        let weight = max(1.0 - s, 0.0);
        let l = luma(rgb);
        rgb = mix(vec3<f32>(l), rgb, 1.0 + vibrance_params.vibrance * weight * weight);
    }
    if (vibrance_params.saturation != 0.0) {
        let l = luma(rgb);
        rgb = mix(vec3<f32>(l), rgb, 1.0 + vibrance_params.saturation);
    }
    store(index, rgb);
}

// ---------------------------------------------------------------------------
// monochrome
// ---------------------------------------------------------------------------
struct MonochromeParams {
    mix: array<vec4<f32>, 2>,
};
@group(0) @binding(9) var<uniform> monochrome_params: MonochromeParams;

@compute @workgroup_size(64)
fn stage_monochrome(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let rgb = load(index);
    let l = luma(rgb);
    let hsv = to_hsv(rgb);
    if (hsv.y <= 1e-4) {
        store(index, vec3<f32>(l));
        return;
    }
    var total = 0.0;
    var gain = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        let w = band_weight(hsv.x, i);
        total = total + w;
        gain = gain + w * shift_at(monochrome_params.mix, i) / 100.0 * 0.5;
    }
    if (total > 1e-6) { gain = gain / total; }
    store(index, vec3<f32>(max(l * (1.0 + gain), 0.0)));
}
