use super::*;
use aura_raw::colour::curve::srgb_decode;
use aura_render::reference_grade::linear_srgb_from_oklab;

#[test]
fn palette_transport_moves_missing_blue_hues_into_a_warm_reference_palette() {
    let cool = image(128, 128, |x, y| {
        let light = 0.25 + 0.6 * y as f32 / 127.0;
        let a = -0.035 + 0.025 * x as f32 / 127.0;
        linear_srgb_from_oklab([light, a, -0.08]).map(aura_raw::colour::curve::srgb_encode)
    });
    let warm = image(128, 128, |x, y| {
        let light = 0.25 + 0.6 * y as f32 / 127.0;
        let a = 0.01 + 0.025 * x as f32 / 127.0;
        linear_srgb_from_oklab([light, a, 0.07]).map(aura_raw::colour::curve::srgb_encode)
    });
    let samples = samples(&cool, TARGET_SAMPLES);
    let mut reference = reference_stats(&warm);
    reference.palette_matching = true;
    let fitted = fit_with_intent_checkpoint(
        &reference,
        &samples,
        None,
        super::super::MatchQuality::Precise,
        super::super::MatchIntent::Palette,
        || Ok(()),
    )
    .unwrap();
    assert!(
        fitted.grade.palette.is_some(),
        "missing-hue transfer must use joint colour evidence"
    );
    let after = transformed(&fitted.grade, &samples);
    let mean_b = after.iter().map(|p| p[2]).sum::<f32>() / after.len() as f32;
    assert!(
        mean_b > 0.035,
        "blue must become warm, not merely less saturated: {mean_b}"
    );
    let measured = measure(&after);
    let before = measure(&samples.iter().map(|p| p.lab).collect::<Vec<_>>());
    assert!(distance(&reference, &measured).palette < 0.35 * distance(&reference, &before).palette);
    assert!(after.iter().all(|p| p.iter().all(|v| v.is_finite())));
}

#[test]
fn palette_strength_endpoints_restore_the_entire_previous_map_exactly() {
    use aura_recipe::reference_grade::{palette::EDGE, PaletteMap};
    let mut prior = ReferenceGrade::identity();
    prior.palette = Some(PaletteMap {
        offset: [0.01, -0.02],
        deltas: vec![[0.005, 0.0]; EDGE * EDGE],
    });
    let mut next = ReferenceGrade::identity();
    next.palette = Some(PaletteMap {
        offset: [-0.01, 0.06],
        deltas: vec![[0.0, 0.015]; EDGE * EDGE],
    });
    assert_eq!(mix(&prior, &next, &Scales::uniform(0.0)), prior);
    assert_eq!(mix(&prior, &next, &Scales::uniform(1.0)), next);
    let mut no_warmth = Scales::uniform(1.0);
    no_warmth.temperature = 0.0;
    no_warmth.tint = 0.0;
    let mixed = mix(&prior, &next, &no_warmth);
    assert_eq!(mixed.palette.unwrap().offset, prior.palette.unwrap().offset);
}

/// Deterministic texture, so no two neighbouring pixels are identical.
fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393)
        ^ y.wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65_535.0 - 0.5
}

fn image(width: u32, height: u32, paint: impl Fn(u32, u32) -> [f32; 3]) -> Rgb8 {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            data.extend(paint(x, y).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    Rgb8 {
        width,
        height,
        data,
    }
}

fn ellipse(x: u32, y: u32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    ((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2) <= 1.0
}

/// An outdoor portrait: sky, foliage, a face, a white dress and a dark suit.
pub(super) fn outdoor() -> Rgb8 {
    image(192, 128, |x, y| {
        let n = noise(x, y, 1) * 0.04;
        let (fx, fy) = (x as f32 / 192.0, y as f32 / 128.0);
        if ellipse(x, y, 70.0, 52.0, 14.0, 18.0) {
            let shade = 0.08 * (fx - 0.36);
            [0.86 + n - shade, 0.66 + n - shade, 0.53 + n - shade]
        } else if ellipse(x, y, 122.0, 58.0, 11.0, 15.0) {
            [0.55 + n, 0.36 + n, 0.25 + n]
        } else if (55..90).contains(&x) && y > 70 {
            [0.93 + n * 0.5, 0.92 + n * 0.5, 0.9 + n * 0.5]
        } else if (108..138).contains(&x) && y > 74 {
            [0.1 + n, 0.11 + n, 0.13 + n]
        } else if fy < 0.35 {
            [0.45 + 0.3 * fy + n, 0.62 + 0.2 * fy + n, 0.92 + n]
        } else if fy < 0.55 {
            [0.62 + n, 0.47 + n, 0.33 + n * 2.0]
        } else {
            let g = 0.35 + 0.25 * noise(x / 6, y / 6, 7);
            [0.22 * g + n, 0.5 * g + 0.1 + n, 0.12 * g + n]
        }
    })
}

/// An indoor scene: a warm wall, a red dress, a plant, two people, a dark doorway.
fn indoor() -> Rgb8 {
    image(176, 132, |x, y| {
        let n = noise(x, y, 3) * 0.04;
        let (fx, fy) = (x as f32 / 176.0, y as f32 / 132.0);
        if ellipse(x, y, 52.0, 44.0, 12.0, 16.0) {
            [0.78 + n, 0.58 + n, 0.47 + n]
        } else if ellipse(x, y, 118.0, 40.0, 12.0, 16.0) {
            [0.42 + n, 0.27 + n, 0.18 + n]
        } else if (38..68).contains(&x) && y > 62 {
            [0.72 + n, 0.1 + n, 0.12 + n]
        } else if (104..134).contains(&x) && y > 58 {
            [0.85 + n, 0.84 + n, 0.82 + n]
        } else if fx > 0.85 {
            [0.06 + n * 0.5, 0.06 + n * 0.5, 0.07 + n * 0.5]
        } else if fx < 0.12 && fy > 0.3 {
            [0.15 + n, 0.42 + n, 0.18 + n]
        } else {
            [0.8 - 0.2 * fy + n, 0.7 - 0.2 * fy + n, 0.55 - 0.15 * fy + n]
        }
    })
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

fn grade_image(source: &Rgb8, grade: fn([f32; 3]) -> [f32; 3]) -> Rgb8 {
    let data = source
        .data
        .chunks_exact(3)
        .flat_map(|p| {
            let encoded = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
            grade(encoded).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        })
        .collect();
    Rgb8 {
        width: source.width,
        height: source.height,
        data,
    }
}

fn encode(linear: f32) -> f32 {
    let x = linear.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

fn in_linear(rgb: [f32; 3], f: impl Fn([f32; 3]) -> [f32; 3]) -> [f32; 3] {
    f(rgb.map(srgb_decode)).map(encode)
}

fn warm(rgb: [f32; 3]) -> [f32; 3] {
    in_linear(rgb, |l| [l[0] * 1.2, l[1] * 1.03, l[2] * 0.78])
}
fn cool_contrast(rgb: [f32; 3]) -> [f32; 3] {
    in_linear(rgb, |l| [l[0] * 0.86, l[1], l[2] * 1.16]).map(|x| x - 0.07 * (TAU * x).sin())
}
fn matte(rgb: [f32; 3]) -> [f32; 3] {
    let lifted = rgb.map(|x| 0.09 + 0.82 * x);
    let y = luma(lifted);
    lifted.map(|v| y + (v - y) * 0.65)
}
fn split_tone(rgb: [f32; 3]) -> [f32; 3] {
    let y = luma(rgb);
    let shadow = (1.0 - y).powi(2);
    let high = y * y;
    [
        rgb[0] - 0.06 * shadow + 0.07 * high,
        rgb[1] + 0.015 * shadow + 0.015 * high,
        rgb[2] + 0.07 * shadow - 0.07 * high,
    ]
}
fn punchy(rgb: [f32; 3]) -> [f32; 3] {
    let contrasted = rgb.map(|x| 0.5 + (x - 0.5) * 1.22);
    let y = luma(contrasted);
    contrasted.map(|v| y + (v - y) * 1.45)
}
fn mono(rgb: [f32; 3]) -> [f32; 3] {
    let y = encode(luma(rgb.map(srgb_decode)));
    [y; 3]
}
fn sepia(rgb: [f32; 3]) -> [f32; 3] {
    let y = mono(rgb)[0];
    [y * 1.06 + 0.02, y * 0.97 + 0.01, y * 0.8]
}
/// Greens and blues pulled toward teal, warm colours toward orange: a hue remap no global
/// slider can express.
fn teal_orange(rgb: [f32; 3]) -> [f32; 3] {
    let lab = oklab_from_linear_srgb(rgb.map(srgb_decode));
    let chroma = lab[1].hypot(lab[2]);
    let hue = lab[2].atan2(lab[1]);
    let degrees = hue.to_degrees().rem_euclid(360.0);
    let goal = if (100.0..300.0).contains(&degrees) {
        205.0_f32
    } else {
        55.0
    };
    let turned = hue + 0.45 * wrap(goal.to_radians() - hue);
    let c = chroma * 1.2;
    linear_srgb_from_oklab([lab[0], c * turned.cos(), c * turned.sin()]).map(encode)
}

type GradeOperation = fn([f32; 3]) -> [f32; 3];
const GRADES: [(&str, GradeOperation); 8] = [
    ("warm", warm),
    ("cool+contrast", cool_contrast),
    ("matte", matte),
    ("split tone", split_tone),
    ("punchy", punchy),
    ("monochrome", mono),
    ("sepia", sepia),
    ("teal-orange", teal_orange),
];

/// Mean `OKLab` difference times a hundred between a fitted result and the known answer.
fn error(result: &[[f32; 3]], truth: &Rgb8) -> f32 {
    let truth: Vec<[f32; 3]> = samples(truth, usize::MAX).iter().map(|s| s.lab).collect();
    assert_eq!(result.len(), truth.len());
    100.0
        * result
            .iter()
            .zip(&truth)
            .map(|(a, b)| {
                ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
            })
            .sum::<f32>()
        / result.len() as f32
}

fn reference_stats(image: &Rgb8) -> GradeStats {
    let labs: Vec<[f32; 3]> = samples(image, REFERENCE_SAMPLES)
        .iter()
        .map(|s| s.lab)
        .collect();
    measure(&labs)
}

fn matched(target: &Rgb8, reference: &Rgb8, similarity: Option<f32>) -> (Fit, Vec<[f32; 3]>) {
    let pixels = samples(target, usize::MAX);
    let fit = fit(&reference_stats(reference), &pixels, similarity);
    let result = transformed(&fit.grade, &pixels);
    (fit, result)
}

fn unchanged(target: &Rgb8) -> Vec<[f32; 3]> {
    samples(target, usize::MAX).iter().map(|s| s.lab).collect()
}

#[test]
fn the_same_photograph_graded_eight_ways_is_matched_closely() {
    let source = outdoor();
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for (name, grade) in GRADES {
        let truth = grade_image(&source, grade);
        let similarity =
            super::super::content::similarity(&super::super::content::thumbnail(&truth), &source);
        let (fit, result) = matched(&source, &truth, similarity);
        let before = error(&unchanged(&source), &truth);
        let after = error(&result, &truth);
        report.push(format!(
            "{name}: {before:.2} -> {after:.2} (stats {:.2} -> {:.2})",
            fit.before.total, fit.after.total
        ));
        if !(after < 0.4 * before && after < 3.5 && fit.after.total < fit.before.total) {
            failures.push(name);
        }
    }
    println!("{}", report.join("\n"));
    assert!(failures.is_empty(), "Unmet accuracy checks: {failures:?}");
}

#[test]
fn quality_modes_fit_real_samples_with_distinct_iteration_budgets() {
    let source = outdoor();
    let reference = grade_image(&source, punchy);
    let stats = reference_stats(&reference);
    for quality in [
        super::super::MatchQuality::Quick,
        super::super::MatchQuality::Precise,
    ] {
        let target = samples(&source, quality.target_samples());
        let result =
            fit_with_quality_checkpoint(&stats, &target, Some(1.0), quality, || Ok(())).unwrap();
        assert_eq!(result.iterations, quality.iterations());
        assert!(target.len() <= quality.target_samples());
        assert!(result.after.total < result.before.total);
        result.grade.validate().unwrap();
        // Lower computational budgets must not alter the exact zero-strength endpoint.
        let identity = ReferenceGrade::identity();
        assert_eq!(toward(&identity, &result.grade, 0.0), identity);
        let mut checkpoints = 0;
        let cancelled = fit_with_quality_checkpoint(&stats, &target, Some(1.0), quality, || {
            checkpoints += 1;
            if checkpoints >= 2 {
                Err(super::super::invalid("Stopped for quality test"))
            } else {
                Ok(())
            }
        });
        assert!(cancelled.is_err());
    }
}

#[test]
fn a_different_photograph_takes_the_look_of_an_edited_reference() {
    // The reference is one scene edited; the photograph is another scene. The answer is the same
    // edit applied to the photograph, which the fit never sees. This is the case the slider fit
    // ended farther from the look on for split toning and strong grades.
    let reference_scene = indoor();
    let target = outdoor();
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for (name, grade) in GRADES {
        let reference = grade_image(&reference_scene, grade);
        let truth = grade_image(&target, grade);
        let similarity = super::super::content::similarity(
            &super::super::content::thumbnail(&reference),
            &target,
        );
        let (fit, result) = matched(&target, &reference, similarity);
        let before = error(&unchanged(&target), &truth);
        let after = error(&result, &truth);
        report.push(format!(
            "{name}: {before:.2} -> {after:.2} (stats {:.2} -> {:.2})",
            fit.before.total, fit.after.total
        ));
        if !(after < before && fit.after.total < fit.before.total) {
            failures.push(name);
        }
    }
    println!("{}", report.join("\n"));
    assert!(failures.is_empty(), "Unmet accuracy checks: {failures:?}");
}

#[test]
fn explicit_palette_matching_fits_unrelated_tone_without_claiming_scene_correspondence() {
    let reference = reference_stats(&grade_image(&indoor(), warm));
    let target = samples(&outdoor(), TARGET_SAMPLES);
    let natural = fit_with_intent_checkpoint(
        &reference,
        &target,
        None,
        super::super::MatchQuality::Precise,
        super::super::MatchIntent::Natural,
        || Ok(()),
    )
    .unwrap();
    let palette = fit_with_intent_checkpoint(
        &reference,
        &target,
        None,
        super::super::MatchQuality::Precise,
        super::super::MatchIntent::Palette,
        || Ok(()),
    )
    .unwrap();
    println!(
        "unrelated natural {:.3}, palette {:.3}, baseline {:.3}",
        natural.after.total, palette.after.total, palette.before.total
    );
    assert!(palette.after.total < 0.8 * palette.before.total);
    assert!(palette.after.total < natural.after.total);
    assert!(palette.content.abs() < f32::EPSILON);
    assert_eq!(
        palette.iterations,
        super::super::MatchQuality::Precise.iterations()
    );
    palette.grade.validate().unwrap();
    let identity = ReferenceGrade::identity();
    assert_eq!(toward(&identity, &palette.grade, 0.0), identity);
}

#[test]
fn a_batch_matched_to_one_reference_converges_on_one_look() {
    let reference = grade_image(&indoor(), split_tone);
    let stats = reference_stats(&reference);
    let first = outdoor();
    let second = grade_image(&outdoor(), warm);
    let mut failures = Vec::new();
    for (index, target) in [first, second].into_iter().enumerate() {
        let pixels = samples(&target, TARGET_SAMPLES);
        let fit = fit(&stats, &pixels, None);
        // A large histogram decrease can be achieved by repainting different objects.
        // Require fidelity to the known grade instead, retaining descriptor non-regression.
        let truth = grade_image(&target, split_tone);
        let before = error(&unchanged(&target), &truth);
        let after = error(&transformed(&fit.grade, &pixels), &truth);
        println!("batch split tone: {before:.3} -> {after:.3}");
        if !before.is_finite() || !after.is_finite() || after >= 0.6 * before {
            failures.push(format!("target {index}: {before:.3} -> {after:.3}"));
        }
        assert!(
            fit.after.total < fit.before.total,
            "{:?} -> {:?}",
            fit.before,
            fit.after
        );
        fit.grade.validate().unwrap();
    }
    assert!(
        failures.is_empty(),
        "Unmet batch accuracy checks: {failures:?}"
    );
}

#[test]
fn a_monochrome_reference_produces_neutral_greys() {
    let reference = grade_image(&indoor(), mono);
    let stats = reference_stats(&reference);
    assert!(stats.achromatic);
    let (fit, result) = matched(&outdoor(), &reference, None);
    let worst = result
        .iter()
        .map(|l| l[1].hypot(l[2]))
        .fold(0.0_f32, f32::max);
    assert!(worst < 0.005, "chroma {worst}");
    assert!(fit.after.tone < fit.before.tone);
}

#[test]
fn palette_monochrome_matching_does_not_leave_skin_coloured() {
    let reference = reference_stats(&grade_image(&indoor(), mono));
    let pixels = samples(&outdoor(), TARGET_SAMPLES);
    let fit = fit_with_intent_checkpoint(
        &reference,
        &pixels,
        None,
        super::super::MatchQuality::Precise,
        super::super::MatchIntent::Palette,
        || Ok(()),
    )
    .unwrap();
    let worst = transformed(&fit.grade, &pixels)
        .iter()
        .map(|v| v[1].hypot(v[2]))
        .fold(0.0_f32, f32::max);
    assert!(worst < 0.005, "monochrome must include skin: {worst}");
}

#[test]
fn noise_in_a_grey_photograph_is_not_turned_into_colour() {
    let grey = image(96, 96, |x, y| [0.5 + noise(x, y, 9) * 0.02; 3]);
    let reference = grade_image(&outdoor(), punchy);
    let pixels = samples(&grey, usize::MAX);
    let fit = fit(&reference_stats(&reference), &pixels, None);
    let worst = transformed(&fit.grade, &pixels)
        .iter()
        .zip(&pixels)
        .map(|(after, before)| {
            let c = |l: [f32; 3]| l[1].hypot(l[2]);
            c(*after) - 3.0 * c(before.lab)
        })
        .fold(f32::MIN, f32::max);
    // Three times the original chroma, plus a trace, plus whatever cast the reference's neutrals
    // carried - never a saturated colour invented from noise.
    assert!(worst < 0.03, "{worst}");
}

#[test]
fn too_little_content_leaves_the_photograph_alone() {
    let tiny = image(8, 8, |_, _| [0.4, 0.3, 0.2]);
    let reference = grade_image(&outdoor(), warm);
    let fit = fit(
        &reference_stats(&reference),
        &samples(&tiny, usize::MAX),
        None,
    );
    assert!(fit.grade.is_identity());
}

#[test]
fn quantile_mapping_follows_ranks_and_keeps_ties_together() {
    let from = [0.1, 0.2, 0.2, 0.2, 0.6, 0.9];
    let to = [0.0, 0.3, 0.4, 0.5, 0.7, 1.0];
    assert!((quantile_map(&from, &to, 0.2, 1.0, (0.3, 1.5)) - 0.4).abs() < 1e-6);
    assert!((quantile_map(&from, &to, 0.75, 1.0, (0.3, 1.5)) - 0.85).abs() < 1e-6);
    let mut previous = -1.0;
    for i in 0..=100 {
        let y = quantile_map(&from, &to, i as f32 / 100.0, 1.0, (0.3, 1.5));
        assert!(y + 1e-6 >= previous && (0.0..=1.0).contains(&y));
        previous = y;
    }
}

#[test]
fn a_regularised_tone_curve_is_bounded_smooth_and_monotone() {
    let wild: Vec<f32> = (0..TONE_NODES)
        .map(|i| if i < 20 { 0.0 } else { 1.0 })
        .collect();
    let curve = regularise_tone(wild);
    let h = 1.0 / (TONE_NODES - 1) as f32;
    for pair in curve.windows(2) {
        let slope = (pair[1] - pair[0]) / h;
        assert!(
            (MIN_TONE_SLOPE - 1e-3..=MAX_TONE_SLOPE + 1e-3).contains(&slope),
            "{slope}"
        );
    }
    let mut grade = ReferenceGrade::identity();
    grade.tone = curve;
    grade.validate().unwrap();
}

#[test]
fn zero_strength_returns_the_previous_look_exactly_and_full_strength_the_fit() {
    let (fit, _) = matched(&outdoor(), &grade_image(&indoor(), split_tone), None);
    let mut prior = ReferenceGrade::identity();
    prior.tone = prior.tone.iter().map(|v| 0.05 + 0.9 * v).collect();
    prior.cast_out[3] = [0.01, -0.02];
    assert_eq!(mix(&prior, &fit.grade, &Scales::uniform(0.0)), prior);
    assert_eq!(mix(&prior, &fit.grade, &Scales::uniform(1.0)), fit.grade);
    let double = mix(&prior, &fit.grade, &Scales::uniform(2.0));
    double.validate().unwrap();
}

#[test]
fn a_face_that_would_turn_raises_the_skin_protection() {
    let face = image(64, 64, |x, y| {
        let n = noise(x, y, 5) * 0.03;
        [0.86 + n, 0.66 + n, 0.53 + n]
    });
    let points: Vec<[f32; 2]> = (0..400)
        .map(|i| {
            [
                ((i % 20) as f32 + 0.5) / 20.0,
                ((i / 20) as f32 + 0.5) / 20.0,
            ]
        })
        .collect();
    let region = Region {
        name: "guard:person-0 face skin".into(),
        points,
        appearance: aura_look::aggregate::fold(&[]),
    };
    let mut grade = ReferenceGrade::identity();
    grade.hue_shift = vec![0.5; SECTORS];
    grade.cast_out = vec![[-0.05, 0.0]; AXIS_NODES];
    let review = review_skin(&mut grade, &face, &[region], false);
    assert_eq!(review.people, 1);
    assert!((review.protection - 1.0).abs() < 1e-6);
    let mut gentle = ReferenceGrade::identity();
    gentle.cast_out = vec![[0.004, 0.01]; AXIS_NODES];
    let region = Region {
        name: "guard:person-0 face skin".into(),
        points: (0..400)
            .map(|i| {
                [
                    ((i % 20) as f32 + 0.5) / 20.0,
                    ((i / 20) as f32 + 0.5) / 20.0,
                ]
            })
            .collect(),
        appearance: aura_look::aggregate::fold(&[]),
    };
    let review = review_skin(&mut gentle, &face, &[region], false);
    assert!((review.protection - 0.5).abs() < 1e-6, "{review:?}");
}

#[test]
fn stored_measurements_round_trip_and_reject_another_version() {
    let stats = reference_stats(&outdoor());
    assert!(stats.usable());
    let text = serde_json::to_string(&stats).unwrap();
    let back: GradeStats = serde_json::from_str(&text).unwrap();
    assert_eq!(back, stats);
    let mut old = stats;
    old.version = 2;
    assert!(!old.usable());
}

#[test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
fn studio_import_keeps_native_fabric_colours_and_reimport_upgrades_preview_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fabric.png");
    image::RgbImage::from_fn(768, 512, |x, _| {
        image::Rgb(if x.is_multiple_of(2) {
            [200, 50, 40]
        } else {
            [40, 50, 200]
        })
    })
    .save(&path)
    .unwrap();
    let state = AppState::open(&directory.path().join("catalog.sqlite")).unwrap();
    let import = || {
        crate::color_match::import_reference(
            &state,
            &crate::color_match::ImportInput {
                path: path.to_string_lossy().into_owned(),
                cancel_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .unwrap()
    };
    let summary = import();
    let native = recall(&state, &summary.id).unwrap().unwrap();
    assert_eq!(native.reference_sampling, REFERENCE_SAMPLING);
    let preview = crate::color_match::reference_image::decode(&path).unwrap();
    let mut legacy = measure_image(&preview.image);
    assert_ne!(
        native.chroma, legacy.chroma,
        "Resizing should blend the distinct fabric colours"
    );
    legacy.reference_sampling = 0;
    crate::color_match::store(&state, &stats_key(&summary.id).unwrap(), &legacy).unwrap();
    assert_eq!(
        recall(&state, &summary.id).unwrap().unwrap(),
        legacy,
        "Legacy saved evidence remains readable until reimport"
    );
    assert_eq!(
        import().id,
        summary.id,
        "Reimport must retain library identity"
    );
    assert_eq!(
        recall(&state, &summary.id).unwrap().unwrap(),
        native,
        "Reimport must replace the old preview evidence with native observations"
    );
}
