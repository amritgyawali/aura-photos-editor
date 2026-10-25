//! Known looks applied to real photographs and matched across different scenes (ignored).
//!
//! `AURA_LOOK_PHOTOS` names a folder of neutral photographs of different scenes. Every look is
//! applied to one photograph to make the reference; each other photograph it is matched to is
//! scored against the same look applied to it - the answer no method ever sees. Errors are mean
//! `OKLab` distance times a hundred over the target's samples.
//!
//! Methods: `none` (leave the photograph alone), `natural` and `palette` (the fit as shipped),
//! `paired` (the look learned from the reference's own original - the ceiling of the transform
//! family, not something a photographer has), and diagnostics that swap one half of a fit for the
//! ceiling's (`nat+ptone`, `nat+pcolour`) or for the scene-prior tone estimate (`nat+btone` look
//! only, `nat+rtone` look relative to the target's own departure).
//!
//! `AURA_LOOK_FLAT=1` hands every method a flat render of the target (blacks lifted, whites
//! lowered, as an unedited RAW render arrives) and still scores against the look applied to the
//! finished photograph.
use super::super::tone_prior::{self, ScenePrior, TonePrior};
use super::*;
use aura_recipe::reference_grade::{ReferenceGrade, AXIS_NODES, MAX_CAST, TONE_NODES};
use std::fmt::Write as _;

/// The look learned from the reference's own original, re-aimed at another photograph: the
/// photograph's own cast travels through the look as colour and the look's axis shift is added.
/// A look that removes colour ends on its own axis instead.
fn ceiling(stats: &GradeStats, original: &[Sample], axis: &[[f32; 2]]) -> ReferenceGrade {
    let learned = fit(stats, original, Some(1.0)).grade;
    let identity = ReferenceGrade::identity();
    let (mut kept, mut base) = (0.0, 0.0);
    for (out, input) in learned.chroma.iter().zip(&identity.chroma).skip(1).take(4) {
        kept += out;
        base += input;
    }
    let absolute = 1.0 - (kept / base.max(1e-6_f32)).clamp(0.0, 1.0);
    let mut grade = learned.clone();
    for i in 0..AXIS_NODES {
        let own = axis.get(i).copied().unwrap_or([0.0; 2]);
        let shift = [
            learned.cast_out[i][0] - learned.cast_in[i][0],
            learned.cast_out[i][1] - learned.cast_in[i][1],
        ];
        let end = learned.cast_out[i];
        grade.cast_in[i] = own.map(|v| (v * absolute).clamp(-MAX_CAST, MAX_CAST));
        grade.cast_out[i] = [0, 1]
            .map(|c| ((1.0 - absolute) * shift[c] + absolute * end[c]).clamp(-MAX_CAST, MAX_CAST));
    }
    grade.bounded()
}

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
// The six declared edits of `scripts/test-color-match-twenty.py`, on encoded sRGB.
fn warm_matte(x: [f32; 3]) -> [f32; 3] {
    let y = luma(x);
    let g = [1.07, 1.015, 0.88];
    [0, 1, 2].map(|c| 0.035 + 0.935 * ((y + 0.82 * (x[c] - y)).max(0.0).powf(0.92) * g[c]))
}
fn cool_cyan(x: [f32; 3]) -> [f32; 3] {
    let y = luma(x);
    let g = [0.88, 1.015, 1.10];
    let a = [-0.005, 0.012, 0.025];
    [0, 1, 2].map(|c| (y + 0.86 * (x[c] - y)).max(0.0).powf(1.06) * g[c] + (1.0 - y) * a[c])
}
fn luminous_split(x: [f32; 3]) -> [f32; 3] {
    let y = luma(x);
    let shadow = ((0.65 - y) / 0.65).clamp(0.0, 1.0);
    let high = ((y - 0.35) / 0.65).clamp(0.0, 1.0);
    let s = [-0.035, 0.025, 0.055];
    let h = [0.05, 0.012, -0.04];
    [0, 1, 2].map(|c| (x[c] - 0.5) * 1.08 + 0.5 + shadow * s[c] + high * h[c])
}
fn pastel(x: [f32; 3]) -> [f32; 3] {
    let y = luma(x);
    let o = [0.015, 0.0, 0.018];
    [0, 1, 2].map(|c| 0.055 + 0.90 * (y + 0.62 * (x[c] - y)).max(0.0).powf(0.82) + o[c])
}
fn warm_punch(x: [f32; 3]) -> [f32; 3] {
    let y = luma(x);
    let g = [1.04, 1.0, 0.94];
    [0, 1, 2].map(|c| ((y + 1.3 * (x[c] - y) - 0.5) * 1.18 + 0.5) * g[c])
}
fn mono_contrast(x: [f32; 3]) -> [f32; 3] {
    let y = luma(x);
    [0.015 + 0.97 * ((y - 0.5) * 1.08 + 0.5).clamp(0.0, 1.0); 3]
}

const MORE: [(&str, GradeOperation); 10] = [
    ("airy", airy),
    ("moody", moody),
    ("film", film),
    ("clean cool", clean_cool),
    ("warm matte", warm_matte),
    ("cool cyan", cool_cyan),
    ("lum split", luminous_split),
    ("pastel", pastel),
    ("warm punch", warm_punch),
    ("mono contrast", mono_contrast),
];

fn load(path: &std::path::Path) -> Option<Rgb8> {
    let rgb = image::open(path).ok()?.to_rgb8();
    Some(Rgb8 {
        width: rgb.width(),
        height: rgb.height(),
        data: rgb.into_raw(),
    })
}

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Mean |dL| and mean |d(a,b)|, times a hundred.
fn split_error(a: &[[f32; 3]], b: &[[f32; 3]]) -> (f64, f64) {
    let n = a.len().max(1) as f64;
    let (mut tone, mut colour) = (0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b) {
        tone += f64::from((x[0] - y[0]).abs());
        colour += f64::from((x[1] - y[1]).hypot(x[2] - y[2]));
    }
    (100.0 * tone / n, 100.0 * colour / n)
}

fn mean_error(a: &[[f32; 3]], b: &[[f32; 3]]) -> f64 {
    100.0
        * a.iter()
            .zip(b)
            .map(|(x, y)| {
                f64::from(
                    ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt(),
                )
            })
            .sum::<f64>()
        / a.len().max(1) as f64
}

/// Tone from one transform, every colour operation from another.
fn hybrid(tone: &ReferenceGrade, colour: &ReferenceGrade) -> ReferenceGrade {
    let mut out = colour.clone();
    out.tone.clone_from(&tone.tone);
    out
}

fn methods() -> Vec<String> {
    env(
        "AURA_LOOK_METHODS",
        "none,natural,paired,nat+ptone,btone,nat+btone".to_owned(),
    )
    .split(',')
    .map(str::to_owned)
    .collect()
}

/// Everything a method may read about one reference and one target.
struct Case<'a> {
    stats: &'a GradeStats,
    original: &'a [Sample],
    /// The look learned pixel by pixel from the reference and its own original, when the
    /// calibration accepted the pair.
    calibrated: Option<&'a aura_render::calibrated_look::Evaluator>,
    pixels: &'a [Sample],
    plain: &'a [[f32; 3]],
    axis: &'a [[f32; 2]],
    similarity: Option<f32>,
    scenes: Option<&'a ScenePrior>,
}

fn tone_settings() -> TonePrior {
    TonePrior {
        edit_sigma: env("AURA_TONE_SIGMA", tone_prior::LOOK.edit_sigma),
        no_edit: env("AURA_TONE_SPIKE", tone_prior::LOOK.no_edit),
        evidence: env("AURA_TONE_EVIDENCE", tone_prior::LOOK.evidence),
    }
}

/// The estimated tone of the look, as a transform that changes nothing else. With `own`, the
/// target's own departure from an ordinary photograph is undone first.
fn estimated_tone(case: &Case<'_>, own: bool) -> ReferenceGrade {
    let mut grade = ReferenceGrade::identity();
    if let Some(scenes) = case.scenes {
        let look = tone_prior::estimate(&case.stats.lightness, scenes, tone_settings(), TONE_NODES);
        let curve = if own {
            let target = measure(case.plain).lightness;
            let mut settings = tone_settings();
            settings.no_edit = env("AURA_TONE_OWN_SPIKE", settings.no_edit);
            settings.edit_sigma = env("AURA_TONE_OWN_SIGMA", settings.edit_sigma);
            let own = tone_prior::estimate(&target, scenes, settings, TONE_NODES);
            tone_prior::relative(&look, &own, TONE_NODES)
        } else {
            look
        };
        grade.tone = regularise_tone(curve);
    }
    grade
}

/// A finished photograph as a flat, unedited render shows it: blacks lifted, whites lowered.
fn flatten(rgb: [f32; 3]) -> [f32; 3] {
    rgb.map(|v| 0.16 + 0.68 * v)
}

fn run(method: &str, case: &Case<'_>) -> Vec<[f32; 3]> {
    // The colour half of a fit, without the scene-prior tone, for the hybrids.
    let natural = || fit_without_scene_tone(case.stats, case.pixels, case.similarity).grade;
    let current = || fit(case.stats, case.pixels, case.similarity).grade;
    let paired = || ceiling(case.stats, case.original, case.axis);
    let grade = match method {
        "none" => return case.plain.to_vec(),
        "calib" => {
            let Some(evaluator) = case.calibrated else {
                return case.plain.to_vec();
            };
            return case
                .pixels
                .iter()
                .map(|p| {
                    let encoded = p.linear.map(aura_raw::colour::curve::srgb_encode);
                    oklab_from_linear_srgb(evaluator.encoded_srgb(encoded).map(srgb_decode))
                })
                .collect();
        }
        "natural" => current(),
        "shipped" => natural(),
        "palette" => {
            fit_with_intent_checkpoint(
                case.stats,
                case.pixels,
                case.similarity,
                super::super::super::MatchQuality::Precise,
                super::super::super::MatchIntent::Palette,
                || Ok(()),
            )
            .unwrap()
            .grade
        }
        "paired" => paired(),
        "nat+ptone" => hybrid(&paired(), &natural()),
        "nat+pcolour" => hybrid(&natural(), &paired()),
        "btone" => estimated_tone(case, false),
        "nat+btone" => hybrid(&estimated_tone(case, false), &natural()),
        "rtone" => estimated_tone(case, true),
        "nat+rtone" => hybrid(&estimated_tone(case, true), &natural()),
        other => panic!("unknown method {other}"),
    };
    transformed(&grade, case.pixels)
}

#[test]
#[ignore = "needs AURA_LOOK_PHOTOS, a folder of neutral photographs"]
#[allow(clippy::too_many_lines)]
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
    assert!(n >= 6, "need at least six photographs in {folder}");
    let target_samples: usize = env("AURA_LOOK_SAMPLES", 12_000);
    let scene_samples: usize = env("AURA_SCENE_SAMPLES", 6_000);
    let stride: usize = env("AURA_LOOK_STRIDE", 3);
    let steps: Vec<usize> = env("AURA_LOOK_STEPS", "1,4,9,16".to_owned())
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let only = std::env::var("AURA_LOOK_ONLY").ok();
    let looks: Vec<_> = GRADES
        .iter()
        .chain(MORE.iter())
        .filter(|(name, _)| {
            only.as_deref()
                .is_none_or(|o| o.split(',').any(|o| name.contains(o)))
        })
        .collect();
    let methods = methods();
    let scene_pixels: Vec<Vec<Sample>> = photos.iter().map(|p| samples(p, scene_samples)).collect();
    // In flat mode every target arrives as a flat render of itself and is still scored against
    // the look applied to the finished photograph.
    let flat = env("AURA_LOOK_FLAT", 0_u8) == 1;
    let target_pixels: Vec<Vec<Sample>> = photos
        .iter()
        .map(|p| {
            if flat {
                samples(&grade_image(p, flatten), target_samples)
            } else {
                samples(p, target_samples)
            }
        })
        .collect();
    let scene_lightness: Vec<Vec<f32>> = scene_pixels
        .iter()
        .map(|p| measure(&p.iter().map(|s| s.lab).collect::<Vec<_>>()).lightness)
        .collect();
    let started = std::time::Instant::now();
    // Per look and method: summed total, tone and colour error, and targets made worse.
    let rows: Vec<(&str, Vec<[f64; 4]>, f64)> = std::thread::scope(|scope| {
        let handles: Vec<_> = looks
            .iter()
            .map(|(name, grade)| {
                let (photos, scene_pixels, target_pixels, methods, steps) =
                    (&photos, &scene_pixels, &target_pixels, &methods, &steps);
                let scene_lightness = &scene_lightness;
                scope.spawn(move || {
                    let mut sums = vec![[0.0_f64; 4]; methods.len()];
                    let mut count = 0.0;
                    for i in (0..n).filter(|i| i % stride == 0) {
                        let reference = grade_image(&photos[i], *grade);
                        let stats = reference_stats(&reference);
                        let thumb = super::super::super::content::thumbnail(&reference);
                        // The scene prior never holds the reference's own original.
                        let scenes = ScenePrior::from_scenes(
                            (0..n)
                                .filter(|&k| k != i)
                                .map(|k| scene_lightness[k].as_slice()),
                        );
                        let calibrated = methods
                            .iter()
                            .any(|m| m == "calib")
                            .then(|| {
                                super::super::super::calibration::fit(
                                    &photos[i],
                                    &reference,
                                    &aura_core::progress::CancelToken::new(),
                                )
                                .ok()
                            })
                            .flatten()
                            .map(|(model, _)| aura_render::calibrated_look::Evaluator::new(&model));
                        for step in steps {
                            let j = (i + step) % n;
                            if j == i {
                                continue;
                            }
                            let pixels = &target_pixels[j];
                            let truth: Vec<[f32; 3]> =
                                samples(&grade_image(&photos[j], *grade), target_samples)
                                    .iter()
                                    .map(|s| s.lab)
                                    .collect();
                            let plain: Vec<[f32; 3]> = pixels.iter().map(|s| s.lab).collect();
                            let axis = measure(&plain).axis;
                            let case = Case {
                                stats: &stats,
                                original: &scene_pixels[i],
                                calibrated: calibrated.as_ref(),
                                pixels,
                                plain: &plain,
                                axis: &axis,
                                similarity: super::super::super::content::similarity(
                                    &thumb, &photos[j],
                                ),
                                scenes: scenes.as_ref(),
                            };
                            let base = mean_error(&plain, &truth);
                            for (m, method) in methods.iter().enumerate() {
                                let result = run(method, &case);
                                let total = mean_error(&result, &truth);
                                let (tone, colour) = split_error(&result, &truth);
                                let s = &mut sums[m];
                                s[0] += total;
                                s[1] += tone;
                                s[2] += colour;
                                s[3] += f64::from(u8::from(total > base + 1e-9));
                            }
                            count += 1.0;
                        }
                    }
                    (*name, sums, count)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let width = methods.iter().map(String::len).max().unwrap_or(8).max(22);
    let cell = |s: &[f64; 4], c: f64| {
        format!(
            "{:.2} [{:.2}/{:.2}] {:.0}",
            s[0] / c,
            s[1] / c,
            s[2] / c,
            s[3]
        )
    };
    let mut header = format!("{:>14}", "look");
    for m in &methods {
        let _ = write!(header, " | {m:>width$}");
    }
    println!(
        "{header}\n{:>14}   (total [tone/colour] targets-made-worse)",
        ""
    );
    let mut total = vec![[0.0_f64; 4]; methods.len()];
    let mut count = 0.0;
    for (name, sums, c) in &rows {
        let mut line = format!("{name:>14}");
        for (m, s) in sums.iter().enumerate() {
            let _ = write!(line, " | {:>width$}", cell(s, *c));
            for (t, v) in total[m].iter_mut().zip(s) {
                *t += v;
            }
        }
        count += c;
        println!("{line}");
    }
    let mut line = format!("{:>14}", "ALL");
    for s in &total {
        let _ = write!(line, " | {:>width$}", cell(s, count));
    }
    println!("{line}");
    println!(
        "pairs {count}, photos {n}, {:.0} s",
        started.elapsed().as_secs_f32()
    );
}

/// Print the lightness landmarks of every photograph in `AURA_LOOK_PHOTOS` as Rust rows, for
/// the built-in scene prior.
#[test]
#[ignore = "needs AURA_LOOK_PHOTOS"]
fn print_scene_landmarks() {
    let Ok(folder) = std::env::var("AURA_LOOK_PHOTOS") else {
        return;
    };
    let mut paths: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    for path in paths {
        let Some(photo) = load(&path) else { continue };
        let stats = reference_stats(&photo);
        let row: Vec<String> = tone_prior::LANDMARKS
            .iter()
            .map(|&i| format!("{:.3}", stats.lightness[i]))
            .collect();
        println!(
            "    [{}], // {}",
            row.join(", "),
            path.file_stem().unwrap().to_string_lossy()
        );
    }
}

/// Write before/after images for a few reference-target pairs to `AURA_LOOK_OUT`: the target,
/// the reference, the fit without and with the scene-prior tone, and the true look.
#[test]
#[ignore = "needs AURA_LOOK_PHOTOS and AURA_LOOK_OUT"]
fn write_look_examples() {
    let (Ok(folder), Ok(out)) = (
        std::env::var("AURA_LOOK_PHOTOS"),
        std::env::var("AURA_LOOK_OUT"),
    ) else {
        return;
    };
    let references: Vec<String> = env("AURA_LOOK_REFS", String::new())
        .split(',')
        .map(str::to_owned)
        .collect();
    let targets: Vec<String> = env("AURA_LOOK_TARGETS", String::new())
        .split(',')
        .map(str::to_owned)
        .collect();
    let only = env("AURA_LOOK_ONLY", String::new());
    let path = |name: &str| std::path::Path::new(&folder).join(format!("{name}.png"));
    let save = |image: &Rgb8, name: &str| {
        image::RgbImage::from_raw(image.width, image.height, image.data.clone())
            .unwrap()
            .save(std::path::Path::new(&out).join(name))
            .unwrap();
    };
    let render = |grade: &ReferenceGrade, source: &Rgb8| -> Rgb8 {
        let pixels = samples(source, usize::MAX);
        let labs = transformed(grade, &pixels);
        let data = labs
            .iter()
            .flat_map(|lab| {
                aura_render::reference_grade::linear_srgb_from_oklab(*lab)
                    .map(|v| (encode(v) * 255.0).round().clamp(0.0, 255.0) as u8)
            })
            .collect();
        Rgb8 {
            width: source.width,
            height: source.height,
            data,
        }
    };
    for (look, grade) in GRADES
        .iter()
        .chain(MORE.iter())
        .filter(|(n, _)| only.split(',').any(|o| n == &o))
    {
        for reference_name in &references {
            let reference = grade_image(&load(&path(reference_name)).unwrap(), *grade);
            let stats = reference_stats(&reference);
            let tag = look.replace(' ', "-");
            save(
                &reference,
                &format!("{tag}__{reference_name}__reference.png"),
            );
            for target_name in &targets {
                let target = load(&path(target_name)).unwrap();
                let pixels = samples(&target, usize::MAX);
                let thumb = super::super::super::content::thumbnail(&reference);
                let similarity = super::super::super::content::similarity(&thumb, &target);
                let base = format!("{tag}__{reference_name}__{target_name}");
                save(&target, &format!("{base}__1-target.png"));
                save(
                    &render(
                        &fit_without_scene_tone(&stats, &pixels, similarity).grade,
                        &target,
                    ),
                    &format!("{base}__2-shipped.png"),
                );
                save(
                    &render(&fit(&stats, &pixels, similarity).grade, &target),
                    &format!("{base}__3-new.png"),
                );
                save(
                    &grade_image(&target, *grade),
                    &format!("{base}__4-truth.png"),
                );
            }
        }
    }
}

/// The twenty assessment targets, each matched to its cyclically assigned edited reference by
/// Palette and by Natural: prints each transform's stretch and writes the matched images to
/// `AURA_LOOK_OUT` so banding can be measured independently (ADR-0181 calibration).
#[test]
#[ignore = "needs AURA_LOOK_PHOTOS (the finished folder) and AURA_LOOK_OUT"]
fn assessment_transforms_stretch() {
    let (Ok(folder), Ok(out)) = (
        std::env::var("AURA_LOOK_PHOTOS"),
        std::env::var("AURA_LOOK_OUT"),
    ) else {
        return;
    };
    let targets = [
        "33380315", "16646551", "18688314", "15069828", "30154480", "29209958", "11008002",
        "3455934", "26651573", "37164686", "7808457", "7322073", "17000433", "37461742",
        "20699589", "36701322", "32972578", "20788937", "20756240", "31714366",
    ];
    let looks: [(&str, &str, GradeOperation); 6] = [
        ("warm-matte", "13822354", warm_matte),
        ("cool-cyan", "12779583", cool_cyan),
        ("teal-orange", "1652312", luminous_split),
        ("pastel", "33202132", pastel),
        ("warm-punch", "34704802", warm_punch),
        ("monochrome", "23891642", mono_contrast),
    ];
    let path = |id: &str| std::path::Path::new(&folder).join(format!("p{id}.png"));
    let save = |labs: &[[f32; 3]], like: &Rgb8, name: String| {
        let data: Vec<u8> = labs
            .iter()
            .flat_map(|lab| {
                aura_render::reference_grade::linear_srgb_from_oklab(*lab)
                    .map(|v| (encode(v) * 255.0).round().clamp(0.0, 255.0) as u8)
            })
            .collect();
        image::RgbImage::from_raw(like.width, like.height, data)
            .unwrap()
            .save(std::path::Path::new(&out).join(name))
            .unwrap();
    };
    for (index, id) in targets.iter().enumerate() {
        let (style, reference_id, grade) = looks[index % 6];
        let target = load(&path(id)).unwrap();
        let reference = grade_image(&load(&path(reference_id)).unwrap(), grade);
        let stats = reference_stats(&reference);
        let pixels = samples(&target, usize::MAX);
        let sampled = samples(&target, TARGET_SAMPLES);
        // A real look in the grade family: learned from the reference's own original.
        let original = samples(&load(&path(reference_id)).unwrap(), TARGET_SAMPLES);
        let real = fit(&stats, &original, Some(1.0)).grade;
        println!(
            "{:02} {style:>12}     real stretch {:.2} (on its original {:.2})",
            index + 1,
            super::super::stretch(&real, &sampled),
            super::super::stretch(&real, &original)
        );
        for (intent, name) in [
            (super::super::super::MatchIntent::Palette, "palette"),
            (super::super::super::MatchIntent::Natural, "natural"),
        ] {
            let fitted = fit_with_intent_checkpoint(
                &stats,
                &sampled,
                None,
                super::super::super::MatchQuality::Precise,
                intent,
                || Ok(()),
            )
            .unwrap();
            let stretch = super::super::stretch(&fitted.grade, &sampled);
            let mut without = fitted.grade.clone();
            without.palette = None;
            let mut tone_only = ReferenceGrade::identity();
            tone_only.tone.clone_from(&fitted.grade.tone);
            println!(
                "{:02} {style:>12} {name:>8} stretch {stretch:.2}  without palette map {:.2}  tone only {:.2}",
                index + 1,
                super::super::stretch(&without, &sampled),
                super::super::stretch(&tone_only, &sampled)
            );
            save(
                &transformed(&fitted.grade, &pixels),
                &target,
                format!("{:02}.{name}.{style}.png", index + 1),
            );
        }
        save(
            &pixels.iter().map(|s| s.lab).collect::<Vec<_>>(),
            &target,
            format!("{:02}.before.png", index + 1),
        );
        save(
            &samples(&grade_image(&target, grade), usize::MAX)
                .iter()
                .map(|s| s.lab)
                .collect::<Vec<_>>(),
            &target,
            format!("{:02}.truth.{style}.png", index + 1),
        );
    }
}
