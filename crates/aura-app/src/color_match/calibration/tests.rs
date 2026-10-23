use super::*;

fn chart(seed: u32, grade: impl Fn([f32; 3]) -> [f32; 3]) -> Rgb8 {
    let mut data = Vec::new();
    for y in 0..96_u32 {
        for x in 0..128_u32 {
            let noise = ((x.wrapping_mul(374_761_393)
                ^ y.wrapping_mul(668_265_263)
                ^ seed.wrapping_mul(2_246_822_519))
                >> 16) as f32
                / 65_535.0;
            let rgb = [
                0.06 + 0.84 * x as f32 / 127.0,
                0.06 + 0.84 * y as f32 / 95.0,
                0.08 + 0.8 * noise,
            ];
            data.extend(grade(rgb).map(|v| (255.0 * v.clamp(0.0, 1.0)).round() as u8));
        }
    }
    Rgb8 {
        width: 128,
        height: 96,
        data,
    }
}

fn grade(rgb: [f32; 3], kind: usize) -> [f32; 3] {
    let [r, g, b] = rgb;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    match kind {
        0 => [r * 1.08 + 0.01, g * 1.01, b * 0.88],
        1 => rgb.map(|v| 0.07 + 0.85 * (y + 0.65 * (v - y))),
        2 => [
            r - 0.045 * (1.0 - y).powi(2) + 0.05 * y * y,
            g + 0.01,
            b + 0.06 * (1.0 - y).powi(2) - 0.05 * y * y,
        ],
        3 => [
            0.90 * r + 0.07 * g,
            0.04 * r + 0.94 * g + 0.04 * b,
            0.10 * g + 0.88 * b,
        ],
        4 => [y; 3],
        _ => rgb.map(|v| v + 0.18 * v * (1.0 - v) * (2.0 * v - 1.0)),
    }
}

#[test]
fn known_grades_generalise_to_held_out_colours_without_matching_scene_histograms() {
    let original = chart(1, |rgb| rgb);
    for kind in 0..6 {
        let edited = chart(1, |rgb| grade(rgb, kind));
        let (model, report) = fit(&original, &edited, &CancelToken::new()).unwrap();
        let evaluator = Evaluator::new(&model);
        let target = chart(11, |[r, g, b]| [b, 0.2 + 0.5 * g, r]);
        let mut before = 0.0;
        let mut after = 0.0;
        for p in target.data.chunks_exact(3) {
            let rgb = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
            let truth = grade(rgb, kind).map(|v| v.clamp(0.0, 1.0));
            before += pixel_error(rgb, truth);
            after += pixel_error(evaluator.encoded_srgb(rgb), truth);
        }
        let count = (target.width * target.height) as f32;
        before /= count;
        after /= count;
        println!("paired grade {kind}: held-out target OKLab error {before:.4} -> {after:.4}; pair validation {:.4}", report.mean_error);
        assert!(
            after < 0.2 && after < 0.15 * before,
            "grade {kind}: {before} -> {after}"
        );
        assert!(report.validation_samples > 1000 && report.mean_error < 0.2);
        model.validate().unwrap();
    }
}

#[test]
fn unrelated_reframed_and_local_edit_pairs_are_refused() {
    let original = chart(1, |rgb| rgb);
    let mut reframed = original.clone();
    reframed.width = 96;
    reframed.height = 128;
    assert!(fit(&original, &reframed, &CancelToken::new()).is_err());
    let unrelated = chart(9, |[r, g, b]| [g, b, r]);
    assert!(fit(&original, &unrelated, &CancelToken::new()).is_err());
    let mut local = original.clone();
    for y in 0..96_usize {
        for x in 0..128_usize {
            if (x / 8 + y / 8) % 2 == 0 {
                for c in 0..3 {
                    local.data[(y * 128 + x) * 3 + c] =
                        local.data[(y * 128 + x) * 3 + c].saturating_add(60);
                }
            }
        }
    }
    assert!(fit(&original, &local, &CancelToken::new()).is_err());
}

#[test]
fn calibration_is_deterministic_and_cancelled_work_is_not_published() {
    let original = chart(1, |rgb| rgb);
    let edited = chart(1, |rgb| grade(rgb, 0));
    let (one, _) = fit(&original, &edited, &CancelToken::new()).unwrap();
    let (two, _) = fit(&original, &edited, &CancelToken::new()).unwrap();
    assert_eq!(one, two);
    let cancelled = CancelToken::new();
    cancelled.cancel();
    assert!(fit(&original, &edited, &cancelled).is_err());
}

#[test]
fn nonlinear_linear_light_wb_and_s_curve_transfer_to_a_different_palette() {
    use aura_raw::colour::curve::srgb_encode;
    let original = chart(1, |rgb| rgb);
    let wb = |rgb: [f32; 3]| {
        let [r, g, b] = rgb.map(srgb_decode);
        [r * 1.2, g * 1.03, b * 0.78].map(srgb_encode)
    };
    let contrast = |rgb: [f32; 3]| rgb.map(|v| v - 0.07 * (std::f32::consts::TAU * v).sin());
    for (name, operation) in [
        ("linear-light WB", wb as fn([f32; 3]) -> [f32; 3]),
        ("sinusoidal contrast", contrast),
    ] {
        let edited = chart(1, operation);
        let (model, _) = fit(&original, &edited, &CancelToken::new()).unwrap();
        let evaluator = Evaluator::new(&model);
        let target = chart(21, |[r, g, b]| [0.15 + 0.7 * b, r, 0.1 + 0.65 * g]);
        let mut before = 0.0;
        let mut after = 0.0;
        for p in target.data.chunks_exact(3) {
            let rgb = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
            let truth = operation(rgb).map(|v| v.clamp(0.0, 1.0));
            before += pixel_error(rgb, truth);
            after += pixel_error(evaluator.encoded_srgb(rgb), truth);
        }
        let n = (target.width * target.height) as f32;
        before /= n;
        after /= n;
        println!("paired {name}: held-out target OKLab error {before:.4} -> {after:.4}");
        assert!(
            after < 0.75 && after < 0.4 * before,
            "{name}: {before} -> {after}"
        );
    }
}
