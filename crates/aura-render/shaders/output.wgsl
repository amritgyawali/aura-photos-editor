// The output transform. The only shader in this crate that bakes tone.
//
// Four steps, in this order and no other: tone map in the working space, rotate into the
// output primaries, apply the transfer function, quantise with an ordered dither.
//
// Rolling off *before* the gamut rotation rather than after is the decision that matters
// here. After the rotation, a channel the target space cannot hold is already out of range
// and clamping it shifts the hue of every specular highlight - the ring, the sequins, the
// candle. `output::map_one` does the same three steps in the same order.
//
// THE DITHER IS NOT RANDOM. A 4x4 Bayer matrix indexed by absolute pixel position, so the
// same pixel gets the same offset whichever tile it arrived in. Section 6.2 asks for "fixed
// random seeds for dither"; this is stronger, because it is reproducible across tile
// boundaries and a sequence-based dither is not.

struct Frame {
    width: u32,
    height: u32,
};

struct OutputParams {
    matrix: mat3x3<f32>,
    // 0 = sRGB piecewise, 1 = gamma 2.2. Display P3 uses the sRGB transfer with P3
    // primaries, which is the whole difference between them.
    transfer: u32,
    bit_depth: u32,
    origin_x: u32,
    origin_y: u32,
};

@group(0) @binding(0) var<storage, read> pixels: array<f32>;
@group(0) @binding(1) var<uniform> frame: Frame;
@group(0) @binding(2) var<uniform> output_params: OutputParams;
@group(0) @binding(3) var<storage, read_write> quantised: array<u32>;

// Phase 02's shoulder, inherited rather than re-invented: a proxy rendered by the preview
// service and an export rendered here must roll off identically. Two shoulders would be two
// products. Mirrors `aura_raw::colour::curve::filmic_lite`.
const KNEE: f32 = 0.6;

fn filmic_lite(x: f32) -> f32 {
    if (x <= KNEE) { return x; }
    let over = x - KNEE;
    return KNEE + (1.0 - KNEE) * over / (over + (1.0 - KNEE));
}

fn encode_srgb(linear: f32) -> f32 {
    let x = clamp(linear, 0.0, 1.0);
    if (x <= 0.0031308) { return 12.92 * x; }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

fn encode_gamma_2_2(linear: f32) -> f32 {
    return pow(clamp(linear, 0.0, 1.0), 1.0 / 2.19921875);
}

fn bayer(x: u32, y: u32) -> f32 {
    let table = array<f32, 16>(
        0.0 / 16.0,  8.0 / 16.0,  2.0 / 16.0, 10.0 / 16.0,
       12.0 / 16.0,  4.0 / 16.0, 14.0 / 16.0,  6.0 / 16.0,
        3.0 / 16.0, 11.0 / 16.0,  1.0 / 16.0,  9.0 / 16.0,
       15.0 / 16.0,  7.0 / 16.0, 13.0 / 16.0,  5.0 / 16.0,
    );
    return table[(y % 4u) * 4u + (x % 4u)];
}

@compute @workgroup_size(64)
fn stage_output_transform(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= frame.width * frame.height) { return; }
    let base = index * 3u;

    let rgb = vec3<f32>(pixels[base], pixels[base + 1u], pixels[base + 2u]);

    // 1. Tone map, on all three channels, in the working space.
    let rolled = vec3<f32>(
        filmic_lite(max(rgb.x, 0.0)),
        filmic_lite(max(rgb.y, 0.0)),
        filmic_lite(max(rgb.z, 0.0)),
    );
    // 2. Gamut.
    let rotated = output_params.matrix * rolled;
    // 3. Transfer.
    var encoded: vec3<f32>;
    if (output_params.transfer == 0u) {
        encoded = vec3<f32>(encode_srgb(rotated.x), encode_srgb(rotated.y), encode_srgb(rotated.z));
    } else {
        encoded = vec3<f32>(
            encode_gamma_2_2(rotated.x),
            encode_gamma_2_2(rotated.y),
            encode_gamma_2_2(rotated.z),
        );
    }

    // 4. Quantise. Sixteen-bit output carries no dither: at 65,536 levels there is nothing
    // to band, and a dither there would make a render depend on where it sat.
    if (output_params.bit_depth == 16u) {
        for (var channel = 0u; channel < 3u; channel = channel + 1u) {
            quantised[base + channel] = u32(clamp(encoded[channel] * 65535.0 + 0.5, 0.0, 65535.0));
        }
        return;
    }

    let x = (index % frame.width) + output_params.origin_x;
    let y = (index / frame.width) + output_params.origin_y;
    let threshold = bayer(x, y);
    for (var channel = 0u; channel < 3u; channel = channel + 1u) {
        let value = encoded[channel] * 255.0;
        quantised[base + channel] = u32(clamp(round(value + threshold - 0.5), 0.0, 255.0));
    }
}
