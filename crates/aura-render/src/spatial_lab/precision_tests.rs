use super::analytical_tests::{chart, operation};
use super::*;

#[test]
fn native_path_repairs_a_three_pixel_selection_that_coarse_sampling_misses() {
    let (w, h) = (1_024, 64);
    let mut pixels = vec![0.3; w * h * 3];
    let centre = 32 * w + 512;
    pixels[centre * 3..centre * 3 + 3].fill(0.95);
    let mut weights = vec![0.0; w * h];
    for y in 31..=33 {
        for x in 511..=513 {
            weights[y * w + x] = 1.0;
        }
    }
    // The old 256-edge decimation samples x=2+4k,y=2+4k: no tap
    // lands in this authored 3x3 selection, so the old path returned unchanged.
    let original = pixels.clone();
    let mut op = operation(Tool::AdaptiveMedianImpulse);
    op.amount = 1.0;
    process(&mut pixels, w, h, &weights, &op);
    assert!((pixels[centre * 3] - 0.3).abs() < 1e-6);
    for (i, weight) in weights.iter().enumerate() {
        if *weight <= 0.0 {
            assert_eq!(&pixels[i * 3..i * 3 + 3], &original[i * 3..i * 3 + 3]);
        }
    }
}
#[test]
fn immutable_native_halos_match_monolithic_processing_across_stripe_and_tile_seams() {
    for (w, h) in [(397, 13), (13, 397)] {
        let grid = chart(w, h);
        let input: Vec<_> = grid.pixels.iter().flatten().copied().collect();
        for tool in [
            Tool::HampelImpulse,
            Tool::ParabolicBackground,
            Tool::DirectionalScratch,
            Tool::GrayEdgeAdapt,
            Tool::CoherenceDetail,
        ] {
            let op = operation(tool);
            let expected = evaluate(&grid, &op);
            let mut output = input.clone();
            process(&mut output, w, h, &grid.weight, &op);
            for i in 0..w * h {
                for c in 0..3 {
                    let value =
                        grid.pixels[i][c] + (expected[i][c] - grid.pixels[i][c]) * op.amount;
                    assert!(
                        (output[i * 3 + c] - value).abs() < 2e-6,
                        "{tool:?} at {i} in {w}x{h}"
                    );
                }
            }
        }
    }
}
