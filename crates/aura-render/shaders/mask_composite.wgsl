// PHASE-18. Compositing an edited buffer back through a mask, in linear light.
//
// This is the COL sign-off in section 9 of the phase document, as a file rather than as a
// review note: *"Verify mask compositing happens in linear light without gamma errors."*
//
// out = a * edited + (1 - a) * base
//
// on linear Rec.2020 values, and on nothing else. A 50 % mask blended after the transfer
// function is a 73 % blend in light, and the error is largest exactly where the mask is soft -
// which is to say, in the halo. `crates/aura-render/tests/colour_discipline.rs` is the grep
// that keeps this true: no `encode_srgb`, no `encode_gamma`, nothing that applies a display
// transfer function appears anywhere in this file.
//
// Nothing here is executed in this build: no `wgpu` backend is linked (ADR-0029 section 4).
//
// THE ALLOWANCE IS IN THE UNIFORM, DELIBERATELY.
//
// `strength` is the caller's own strength already multiplied by the mask's
// `Mask::allowance()` - phases 19 to 24 do that multiplication, in Rust, through
// `mask::quality::allowance`. It is not recomputed here from the confidence and the edge
// quality, because two implementations of a gating rule is two answers to "may this mask carry
// skin smoothing", and the one on the GPU is the one nobody tests against a fixture.
//
// NO ATOMICS, no workgroup reductions, no dependence on neighbours: every invocation reads
// three values at its own index and writes one.

struct Composite {
    width: u32,
    height: u32,
    // The caller's strength times the mask's allowance, 0..1.
    strength: f32,
    // 1 when the mask is inverted, 0 otherwise. A uniform rather than a second shader, because
    // an inverted pair has to cover the frame exactly and two shaders is two roundings.
    invert: u32,
};

@group(0) @binding(0) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(1) var<storage, read> edited: array<f32>;
@group(0) @binding(2) var<storage, read> alpha: array<f32>;
@group(0) @binding(3) var<uniform> composite: Composite;

// Rec.2020 luminance weights, for the debug overlay below. The same three numbers the processor
// path uses; the parity test checks each of them appears with the value it has in Rust.
const luma_r: f32 = 0.262700;
const luma_g: f32 = 0.677998;
const luma_b: f32 = 0.059302;

fn load_linear(buffer: ptr<storage, array<f32>, read>, index: u32) -> vec3<f32> {
    let base = index * 3u;
    return vec3<f32>((*buffer)[base], (*buffer)[base + 1u], (*buffer)[base + 2u]);
}

fn mask_at(index: u32) -> f32 {
    let a = clamp(alpha[index], 0.0, 1.0);
    if (composite.invert == 1u) {
        return 1.0 - a;
    }
    return a;
}

@compute @workgroup_size(8, 8, 1)
fn mask_composite(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= composite.width || id.y >= composite.height) {
        return;
    }
    let index = id.y * composite.width + id.x;
    let base_index = index * 3u;

    let a = mask_at(index) * clamp(composite.strength, 0.0, 1.0);
    let base = vec3<f32>(pixels[base_index], pixels[base_index + 1u], pixels[base_index + 2u]);
    let over = load_linear(&edited, index);

    // Linear light, one multiply-add per channel, no transfer function anywhere.
    let out = base * (1.0 - a) + over * a;

    pixels[base_index] = out.x;
    pixels[base_index + 1u] = out.y;
    pixels[base_index + 2u] = out.z;
}

// The panel's overlay: tint the masked region so a photographer can see where it is.
//
// A separate entry point rather than a flag on the one above, because this one is allowed to
// be wrong about colour and that one is not. It draws a red wash whose strength is the alpha;
// it is a diagnostic, it never runs on an export, and `mask_composite` is what a delivered
// photograph goes through.
@compute @workgroup_size(8, 8, 1)
fn mask_overlay(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= composite.width || id.y >= composite.height) {
        return;
    }
    let index = id.y * composite.width + id.x;
    let base_index = index * 3u;
    let a = mask_at(index) * 0.5;
    let base = vec3<f32>(pixels[base_index], pixels[base_index + 1u], pixels[base_index + 2u]);
    let luma = base.x * luma_r + base.y * luma_g + base.z * luma_b;
    let wash = vec3<f32>(luma * 1.6, luma * 0.5, luma * 0.55);
    let out = base * (1.0 - a) + wash * a;
    pixels[base_index] = out.x;
    pixels[base_index + 1u] = out.y;
    pixels[base_index + 2u] = out.z;
}
