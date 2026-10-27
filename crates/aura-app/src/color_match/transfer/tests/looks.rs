//! Known looks applied to real photographs, matched across different scenes (ignored).
//!
//! `AURA_LOOK_PHOTOS` names a folder of PNG or JPEG photographs, each a neutral rendering of a
//! different scene. Every look is applied to one photograph to make the reference, and each other
//! photograph it is matched to is scored against the same look applied to it - the answer the fit
//! never sees. The errors are mean `OKLab` distance times a hundred over every pixel.
use super::*;

fn airy(rgb: [f32; 3]) -> [f32; 3] {
    let lifted = in_linear(rgb, |l| l.map(|v| v * 1.35)).map(|x| 0.04 + 0.92 * x);
    let y = luma(lifted);
    let soft = lifted.map(|v| y + (v - y) * 0.85);
    [soft[0] + 0.012, soft[1] + 0.004, soft[2] - 0.01]
}
fn moody(rgb: [f32; 3]) -> [f32; 3] {
    let dark = in_linear(rgb, |l| l.map(|v| v * 0.7)).map(|x| (x - 0.03) / 0.97);
    let y = luma(dark);
    let flat = dark.map(|v| y + (v - y) * 0.7);
    let shadow = (1.0 - y).powi(3);
    [
        flat[0] - 0.02 * shadow,
        flat[1] + 0.005 * shadow,
        flat[2] + 0.03 * shadow,
    ]
}
fn film(rgb: [f32; 3]) -> [f32; 3] {
    let curve = rgb.map(|x| 0.06 + 0.86 * (x - 0.06 * (TAU * x).sin()).clamp(0.0, 1.0));
    let y = luma(curve);
    let shadow = (1.0 - y).powi(2);
    let high = y * y;
    let muted = curve.map(|v| y + (v - y) * 0.85);
    [
        muted[0] - 0.01 * shadow + 0.04 * high,
        muted[1] + 0.03 * shadow + 0.01 * high,
        muted[2] + 0.01 * shadow - 0.04 * high,
    ]
}
fn clean_cool(rgb: [f32; 3]) -> [f32; 3] {
    let cooled = in_linear(rgb, |l| [l[0] * 0.92, l[1] * 0.99, l[2] * 1.1]);
    let y = luma(cooled);
    cooled.map(|v| y + (v - y) * 0.8)
}

const MORE: [(&str, GradeOperation); 4] = [
    ("airy", airy),
    ("moody", moody),
    ("film", film),
    ("clean cool", clean_cool),
];

const SAMPLES: usize = 30_000;

fn load(path: &std::path::Path) -> Option<Rgb8> {
    let rgb = image::open(path).ok()?.to_rgb8();
    Some(Rgb8 {
        width: rgb.width(),
        height: rgb.height(),
        data: rgb.into_raw(),
    })
}

#[test]
#[ignore = "needs AURA_LOOK_PHOTOS, a folder of neutral photographs"]
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
fn known_looks_carry_across_real_photographs() {
    let Ok(folder) = std::env::var("AURA_LOOK_PHOTOS") else {
        return;
    };
    let mut paths: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("jpg"))
        })
        .collect();
    paths.sort();
    let photos: Vec<Rgb8> = paths.iter().filter_map(|p| load(p)).collect();
    let n = photos.len();
    assert!(n >= 4, "need at least four photographs in {folder}");
    // Held out: every third photograph is only ever a target, never a reference, and is
    // reported separately so a change tuned on the rest can be checked on scenes it never saw.
    let only = std::env::var("AURA_LOOK_ONLY").ok();
    let pair = std::env::var_os("AURA_LOOK_PAIR").is_some();
    let calib = std::env::var_os("AURA_LOOK_CALIB").is_some();
    let pairs: usize = std::env::var("AURA_LOOK_PAIRS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let looks: Vec<_> = GRADES
        .iter()
        .chain(MORE.iter())
        .filter(|(name, _)| only.as_deref().is_none_or(|o| name.contains(o)))
        .collect();
    let mean = |a: &[[f32; 3]], b: &[[f32; 3]]| {
        100.0
            * a.iter()
                .zip(b)
                .map(|(x, y)| {
                    f64::from(
                        ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2))
                            .sqrt(),
                    )
                })
                .sum::<f64>()
            / a.len().max(1) as f64
    };
    // before, after, worse, count, refused, held before, held after, held count
    let rows: Vec<(&str, [f64; 9])> = std::thread::scope(|scope| {
        let handles: Vec<_> = looks
            .iter()
            .map(|(name, grade)| {
                let photos = &photos;
                scope.spawn(move || {
                    let mut r = [0.0_f64; 9];
                    for i in (0..n).filter(|i| i % 3 != 2) {
                        let reference = grade_image(&photos[i], *grade);
                        let stats = reference_stats(&reference);
                        let thumb = super::super::super::content::thumbnail(&reference);
                        // With the reference's own original in hand, the look is learned on it
                        // once and the same transform is applied to every other photograph.
                        // Corresponding pixels: the original/edited calibration, where it qualifies.
                        // Further edited frames of the same look, never one of this
                        // reference's targets, pooled into the calibration.
                        let extra: Vec<(Rgb8, Rgb8)> = [3, 7, 9]
                            .iter()
                            .take(pairs.saturating_sub(1))
                            .map(|k| {
                                let x = &photos[(i + k) % n];
                                (x.clone(), grade_image(x, *grade))
                            })
                            .collect();
                        let calibrated = calib
                            .then(|| {
                                let mut all = vec![(&photos[i], &reference)];
                                all.extend(extra.iter().map(|(o, e)| (o, e)));
                                super::super::super::calibration::fit_pairs(
                                    &all,
                                    &aura_core::progress::CancelToken::new(),
                                )
                                .ok()
                                .map(|(model, _)| {
                                    aura_render::calibrated_look::Evaluator::new(&model)
                                })
                            })
                            .flatten();
                        if calib && calibrated.is_none() {
                            r[8] += 1.0;
                        }
                        let learned = pair.then(|| {
                            let original = samples(&photos[i], SAMPLES);
                            let similarity =
                                super::super::super::content::similarity(&thumb, &photos[i]);
                            fit(&stats, &original, similarity).grade
                        });
                        for step in [1, 5, 11] {
                            let j = (i + step) % n;
                            let target = &photos[j];
                            let truth: Vec<[f32; 3]> =
                                samples(&grade_image(target, *grade), SAMPLES)
                                    .iter()
                                    .map(|s| s.lab)
                                    .collect();
                            let similarity =
                                super::super::super::content::similarity(&thumb, target);
                            let pixels = samples(target, SAMPLES);
                            let mut fit = fit(&stats, &pixels, similarity);
                            if let Some(grade) = &learned {
                                fit.grade = grade.clone();
                            }
                            let plain: Vec<[f32; 3]> = pixels.iter().map(|s| s.lab).collect();
                            let b = mean(&plain, &truth);
                            let result = match &calibrated {
                                Some(evaluator) => pixels
                                    .iter()
                                    .map(|p| {
                                        let encoded =
                                            p.linear.map(aura_raw::colour::curve::srgb_encode);
                                        oklab_from_linear_srgb(
                                            evaluator.encoded_srgb(encoded).map(srgb_decode),
                                        )
                                    })
                                    .collect(),
                                None => transformed(&fit.grade, &pixels),
                            };
                            let a = mean(&result, &truth);
                            r[0] += b;
                            r[1] += a;
                            r[2] += f64::from(u8::from(a > b));
                            r[3] += 1.0;
                            // The native check refuses a match that moves the reference distance up.
                            r[4] += f64::from(u8::from(fit.after.total > fit.before.total + 1e-4));
                            if j % 3 == 2 {
                                r[5] += b;
                                r[6] += a;
                                r[7] += 1.0;
                            }
                        }
                    }
                    (*name, r)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut total = [0.0_f64; 9];
    for (name, r) in &rows {
        println!(
            "{name:>14}: {:6.2} -> {:6.2}  ({:5.1}% left, {}/{} worse, {} refused; held-out {:5.1}% left; {} uncalibrated)",
            r[0] / r[3],
            r[1] / r[3],
            100.0 * r[1] / r[0],
            r[2],
            r[3],
            r[4],
            100.0 * r[6] / r[5],
            r[8]
        );
        for (t, v) in total.iter_mut().zip(r) {
            *t += v;
        }
    }
    let r = total;
    println!(
        "{:>14}: {:6.2} -> {:6.2}  ({:5.1}% left, {}/{} worse, {} refused; held-out {:5.1}% left; {} uncalibrated)",
        "all",
        r[0] / r[3],
        r[1] / r[3],
        100.0 * r[1] / r[0],
        r[2],
        r[3],
        r[4],
        100.0 * r[6] / r[5],
        r[8]
    );
}
