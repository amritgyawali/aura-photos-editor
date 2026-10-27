//! Real hardware qualification. Run with --ignored --test-threads=1 on the reference GPU.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
use std::sync::Arc;
use std::time::Instant;

use aura_core::clock::FixedClock;
use aura_render::{
    output, CpuEngine, Frame, FrameSource, OutputColour, OutputSpec, RenderLevel, RenderPurpose,
    RenderService, RenderedData,
};

fn pixels(width: u32, height: u32) -> Vec<f32> {
    (0..width as usize * height as usize * 3)
        .map(|index| {
            let seed = (index as u32)
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            (seed % 65536) as f32 / 16384.0 - 0.05
        })
        .collect()
}

fn difference(reference: &RenderedData, actual: &RenderedData) -> (u32, f64, usize) {
    let pairs: Vec<(u32, u32)> = match (reference, actual) {
        (RenderedData::Eight(a), RenderedData::Eight(b)) => a
            .iter()
            .zip(b)
            .map(|(a, b)| (u32::from(*a), u32::from(*b)))
            .collect(),
        (RenderedData::Sixteen(a), RenderedData::Sixteen(b)) => a
            .iter()
            .zip(b)
            .map(|(a, b)| (u32::from(*a), u32::from(*b)))
            .collect(),
        _ => panic!("different bit depths"),
    };
    let max = pairs.iter().map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
    let sum: u64 = pairs.iter().map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
    let changed = pairs.iter().filter(|(a, b)| a != b).count();
    (max, sum as f64 / pairs.len() as f64, changed)
}

#[test]
#[ignore = "requires a real GPU; does not accept software adapters"]
fn hardware_output_matches_all_spaces_depths_hdr_and_stripe_origins() {
    assert!(
        output::gpu_identity().is_some(),
        "No qualified hardware GPU"
    );
    for (width, height) in [(257, 129), (2049, 1025)] {
        let source = pixels(width, height);
        for space in [
            OutputColour::Srgb,
            OutputColour::AdobeRgb,
            OutputColour::DisplayP3,
        ] {
            for depth in [8, 16] {
                let origin = (11, 23);
                let reference = output::transform(&source, width, height, space, depth, origin);
                let actual = output::transform_gpu(&source, width, height, space, depth, origin)
                    .expect("GPU transform");
                let (max, mean, changed) = difference(&reference, &actual);
                eprintln!("parity {width}x{height} {space:?} {depth}: max={max} mean={mean:.8} changed={changed}");
                assert!(
                    max <= if depth == 8 { 1 } else { 4 },
                    "output rounding exceeded the published tolerance"
                );
                assert!(mean < if depth == 8 { 0.02 } else { 0.2 });
                let repeated = output::transform_gpu(&source, width, height, space, depth, origin)
                    .expect("repeat");
                assert_eq!(actual, repeated, "same GPU must produce the same codes");
            }
        }
    }
}

#[test]
#[ignore = "requires a real GPU; reports upload, compute and readback together"]
fn benchmark_output_including_all_transfers() {
    eprintln!(
        "GPU fallback reason: {:?}",
        output::gpu_unavailable_reason()
    );
    eprintln!("device {}", output::gpu_identity().expect("GPU device"));
    for (width, height) in [(512, 512), (1024, 1024), (2048, 3072)] {
        let source = pixels(width, height);
        // Warm allocation and shader execution before alternating the measurements.
        output::transform_gpu(&source, width, height, OutputColour::Srgb, 8, (0, 0)).unwrap();
        for round in 0..3 {
            let start = Instant::now();
            let reference =
                output::transform(&source, width, height, OutputColour::Srgb, 8, (0, 0));
            let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
            let start = Instant::now();
            let gpu = output::transform_gpu(&source, width, height, OutputColour::Srgb, 8, (0, 0))
                .unwrap();
            let gpu_ms = start.elapsed().as_secs_f64() * 1000.0;
            let (max, mean, _) = difference(&reference, &gpu);
            eprintln!("benchmark {width}x{height} round={round} cpu_ms={cpu_ms:.3} gpu_ms={gpu_ms:.3} speedup={:.3} max={max} mean={mean:.8}", cpu_ms/gpu_ms);
        }
    }
}

#[derive(Debug)]
struct Source(Frame);
impl FrameSource for Source {
    fn frame(&self, _: &aura_core::PhotoId, _: RenderLevel) -> aura_core::AuraResult<Frame> {
        Ok(self.0.clone())
    }
}

#[test]
#[ignore = "requires a real GPU"]
fn enabled_engine_uses_gpu_and_separates_its_cache_from_the_reference() {
    let frame = Frame::working(pixels(1024, 1024), 1024, 1024, "fixture");
    let recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
    let source = Arc::new(Source(frame.clone()));
    let cpu = CpuEngine::new(source.clone(), Arc::new(FixedClock::default()));
    let gpu = CpuEngine::new(source, Arc::new(FixedClock::default())).with_gpu();
    let reference = cpu
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .unwrap();
    let actual = gpu
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .unwrap();
    assert_eq!(actual.backend, "wgpu");
    assert_ne!(actual.render_hash, reference.render_hash);
    assert!(gpu.capabilities().engine.contains("wgpu.output"));
    assert!(difference(&reference.data, &actual.data).0 <= 1);
    assert!(output::transform_gpu(&[f32::NAN; 3], 1, 1, OutputColour::Srgb, 8, (0, 0)).is_err());
    assert!(output::transform_gpu(&[0.2; 3], 2, 1, OutputColour::Srgb, 8, (0, 0)).is_err());
    assert!(
        output::gpu_identity().is_some(),
        "invalid input must not disable a good device"
    );
}
