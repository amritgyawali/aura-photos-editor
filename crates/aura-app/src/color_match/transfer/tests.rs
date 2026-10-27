use super::*;
use aura_raw::colour::curve::srgb_decode;
use aura_render::reference_grade::linear_srgb_from_oklab;

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

/// A studio portrait: a grey paper backdrop, a dark jacket, a light shirt, deep skin.
fn studio() -> Rgb8 {
    image(160, 160, |x, y| {
        let n = noise(x, y, 11) * 0.03;
        let (fx, fy) = (x as f32 / 160.0, y as f32 / 160.0);
        if ellipse(x, y, 80.0, 58.0, 17.0, 22.0) {
            [0.46 + n, 0.31 + n, 0.22 + n]
        } else if fy > 0.62 && (0.25..0.75).contains(&fx) {
            if (0.44..0.56).contains(&fx) {
                [0.88 + n, 0.88 + n, 0.86 + n]
            } else {
                [0.12 + n, 0.12 + n, 0.16 + n]
            }
        } else {
            let falloff = 0.62 - 0.35 * ((fx - 0.5).powi(2) + (fy - 0.4).powi(2)).sqrt();
            [falloff + n, falloff + n, falloff + 0.01 + n]
        }
    })
}

/// Evening on water: an orange-to-violet sky, a dark shore, a reflection, a couple in silhouette.
fn golden() -> Rgb8 {
    image(200, 120, |x, y| {
        let n = noise(x, y, 13) * 0.04;
        let fy = y as f32 / 120.0;
        if (88..104).contains(&x) && (40..80).contains(&y) {
            [0.05 + n * 0.5, 0.04 + n * 0.5, 0.05 + n * 0.5]
        } else if fy < 0.45 {
            let t = fy / 0.45;
            [0.95 - 0.35 * t + n, 0.62 - 0.3 * t + n, 0.35 + 0.25 * t + n]
        } else if fy < 0.66 {
            [0.1 + n, 0.12 + n, 0.08 + n]
        } else {
            let t = (fy - 0.66) / 0.34;
            [0.7 - 0.3 * t + n, 0.45 - 0.2 * t + n, 0.35 + 0.1 * t + n]
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

/// The edit learned from a photograph and its own edited version, as the product learns it:
/// fitted from statistics, then refined on corresponding thumbnail pixels.
fn learned(original: &Rgb8, reference: &Rgb8) -> ReferenceGrade {
    let similarity =
        super::super::content::similarity(&super::super::content::thumbnail(reference), original);
    let fitted = fit(
        &reference_stats(reference),
        &samples(original, 20_000),
        similarity,
    )
    .grade;
    let pairs = Pairs::new(
        &thumbnail(original).unwrap(),
        &thumbnail(reference).unwrap(),
    )
    .unwrap();
    refine_on_pairs(&fitted, &pairs, || Ok(())).unwrap().0
}

fn unchanged(target: &Rgb8) -> Vec<[f32; 3]> {
    samples(target, usize::MAX).iter().map(|s| s.lab).collect()
}

#[test]
fn the_same_photograph_graded_eight_ways_is_matched_closely() {
    let source = outdoor();
    let mut report = Vec::new();
    for (name, grade) in GRADES {
        let truth = grade_image(&source, grade);
        let edit = learned(&source, &truth);
        let pixels = samples(&source, usize::MAX);
        let stats = reference_stats(&truth);
        let before = error(&unchanged(&source), &truth);
        let after = error(&transformed(&edit, &pixels), &truth);
        let (from, to) = predicted(&stats, &edit, &pixels);
        report.push((name, before, after, from.total, to.total));
    }
    for (name, before, after, from, to) in &report {
        println!("{name}: {before:.2} -> {after:.2} (stats {from:.2} -> {to:.2})");
    }
    // Matte - a lift made in display RGB - turns dark colours' hue by an amount that depends on
    // their lightness, which the sectors cannot; it is the weakest look the model represents.
    for (name, before, after, _, _) in report {
        assert!(after < 0.55 * before, "{name}: {before:.2} -> {after:.2}");
        assert!(after < 3.0, "{name}: {before:.2} -> {after:.2}");
    }
}

#[test]
fn a_different_photograph_takes_the_look_of_an_edited_reference() {
    // The reference is one scene edited; the photograph is another. Without the reference's
    // original, the fit can only take part of the reference's appearance: every pair must move
    // toward it, and the looks that act on the whole frame - a matte finish, monochrome, sepia -
    // must also end nearer the edit itself. The flat synthetic scenes are a harder case than a
    // real shoot; ADR-0160 records the real measurements.
    let scenes = [outdoor(), indoor(), studio(), golden()];
    let mut report = Vec::new();
    for (name, grade) in GRADES {
        let (mut before_sum, mut after_sum, mut improved, mut pairs) = (0.0, 0.0, 0, 0);
        for (ri, reference_scene) in scenes.iter().enumerate() {
            let reference = grade_image(reference_scene, grade);
            let stats = reference_stats(&reference);
            for (ti, target) in scenes.iter().enumerate() {
                if ri == ti {
                    continue;
                }
                let truth = grade_image(target, grade);
                let similarity = super::super::content::similarity(
                    &super::super::content::thumbnail(&reference),
                    target,
                );
                let pixels = samples(target, usize::MAX);
                let fit = fit(&stats, &pixels, similarity);
                assert!(
                    fit.after.total <= fit.before.total + 1e-4,
                    "{name}: {:?} -> {:?}",
                    fit.before,
                    fit.after
                );
                let before = error(&unchanged(target), &truth);
                let after = error(&transformed(&fit.grade, &pixels), &truth);
                before_sum += before;
                after_sum += after;
                improved += usize::from(after < before);
                pairs += 1;
            }
        }
        report.push(format!(
            "{name}: mean {:.2} -> {:.2}, closer on {improved}/{pairs}",
            before_sum / pairs as f32,
            after_sum / pairs as f32
        ));
        if ["matte", "monochrome", "sepia"].contains(&name) {
            assert!(
                after_sum < before_sum,
                "{name}: {before_sum} -> {after_sum}"
            );
        }
    }
    println!("{}", report.join("\n"));
}

#[test]
fn an_edit_learned_from_its_original_carries_to_other_scenes() {
    // With the reference's original the edit itself is learned, and the same edit is applied to
    // every other scene. These flat scenes share almost no colours, so every learned edit has to
    // extrapolate - the hard end; ADR-0160 records the real shoot. What must hold is the order:
    // the learned edit ends nearer the right answer than doing nothing and than matching the
    // reference's appearance without its original.
    let scenes = [outdoor(), indoor(), studio(), golden()];
    let mut totals = [0.0_f32; 3];
    let mut report = Vec::new();
    for (name, grade) in GRADES {
        let mut sums = [0.0_f32; 3];
        let mut pairs = 0;
        for (ri, original) in scenes.iter().enumerate() {
            let reference = grade_image(original, grade);
            let edit = learned(original, &reference);
            let stats = reference_stats(&reference);
            for (ti, target) in scenes.iter().enumerate() {
                if ri == ti {
                    continue;
                }
                let truth = grade_image(target, grade);
                let pixels = samples(target, usize::MAX);
                let appearance = fit(&stats, &pixels, None).grade;
                let errors = [
                    error(&unchanged(target), &truth),
                    error(&transformed(&edit, &pixels), &truth),
                    error(&transformed(&appearance, &pixels), &truth),
                ];
                for (sum, e) in sums.iter_mut().zip(errors) {
                    *sum += e;
                }
                pairs += 1;
            }
        }
        for (total, sum) in totals.iter_mut().zip(sums) {
            *total += sum;
        }
        let n = pairs as f32;
        report.push(format!(
            "{name}: none {:.2}, learned edit {:.2}, appearance {:.2}",
            sums[0] / n,
            sums[1] / n,
            sums[2] / n
        ));
    }
    println!("{}", report.join("\n"));
    let [none, edit, appearance] = totals;
    assert!(
        edit < none && edit < appearance,
        "{none} {edit} {appearance}"
    );
}

/// Real expert edits, each photograph matched to every other one's finished version (ADR-0160).
///
/// `AURA_GRADE_PAIRS` names a folder of `NAME-before.png` / `NAME-after.png` written by the
/// `reference_grade_pairs` harness. `AURA_GRADE_TRUST=ends,tone,cast,chroma,hue` replaces the
/// different-scene priors, and `AURA_GRADE_HALF=even|odd` keeps every other photograph, so a
/// prior chosen on one half can be checked on the other.
#[test]
#[ignore = "needs AURA_GRADE_PAIRS"]
#[allow(clippy::too_many_lines)]
fn real_expert_edits_matched_across_photographs() {
    let folder = std::env::var("AURA_GRADE_PAIRS").expect("AURA_GRADE_PAIRS");
    let priors = std::env::var("AURA_GRADE_TRUST").ok().map(|text| {
        let v: Vec<f32> = text
            .split(',')
            .map(|t| t.trim().parse().expect("a number"))
            .collect();
        [v[0], v[1], v[2], v[3], v[4]]
    });
    let mut stems: Vec<String> = std::fs::read_dir(&folder)
        .expect("folder")
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix("-before.png").map(str::to_owned)
        })
        .collect();
    stems.sort();
    if let Ok(half) = std::env::var("AURA_GRADE_HALF") {
        let keep = usize::from(half == "odd");
        stems = stems
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == keep)
            .map(|(_, s)| s)
            .collect();
    }
    let load = |stem: &str, side: &str| {
        let image = image::open(format!("{folder}/{stem}-{side}.png"))
            .expect("pair")
            .to_rgb8();
        Rgb8 {
            width: image.width(),
            height: image.height(),
            data: image.into_raw(),
        }
    };
    let photos: Vec<(Rgb8, Rgb8)> = stems
        .iter()
        .map(|s| (load(s, "before"), load(s, "after")))
        .collect();
    // The edit each photograph received, learned from its own before and after.
    let priors_only = std::env::var("AURA_GRADE_PRIORS_ONLY").is_ok();
    let edits: Vec<ReferenceGrade> = photos
        .iter()
        .map(|(before, after)| {
            if priors_only {
                ReferenceGrade::identity()
            } else {
                learned(before, after)
            }
        })
        .collect();
    let mut same = Vec::new();
    let mut cross = Vec::new();
    let mut transferred: Vec<Vec<(f32, f32, f32, f32)>> = vec![Vec::new(); 3];
    for (ri, (_, reference)) in photos.iter().enumerate() {
        let stats = reference_stats(reference);
        for (ti, (before, truth)) in photos.iter().enumerate() {
            let pixels = samples(before, 20_000);
            let similarity = super::super::content::similarity(
                &super::super::content::thumbnail(reference),
                before,
            );
            // The reference's own photograph goes the product's same-frame way: learned.
            let fitted = match priors {
                _ if ri == ti => Fit {
                    grade: edits.get(ri).expect("edit").clone(),
                    before: Distance::default(),
                    after: Distance::default(),
                    goal_after: Distance::default(),
                    iterations: 0,
                    content: 1.0,
                },
                Some([ends, tone, cast, chroma, hue]) => {
                    let content = content_of(similarity);
                    let towards = |prior: f32| prior + (1.0 - prior) * content;
                    let trust = Trust {
                        ends: towards(ends),
                        tone: towards(tone),
                        cast: towards(cast),
                        chroma: towards(chroma),
                        hue: towards(hue),
                    };
                    fit_with(&stats, &pixels, content, trust, || Ok(())).unwrap()
                }
                None => fit(&stats, &pixels, similarity),
            };
            // The sampler places its points by position, so the same budget over two images of
            // one size reads the same pixels of both.
            let everything = samples(before, 100_000);
            let truth_labs: Vec<[f32; 3]> = samples(truth, 100_000).iter().map(|s| s.lab).collect();
            let delta = |labs: &[[f32; 3]]| {
                100.0
                    * labs
                        .iter()
                        .zip(&truth_labs)
                        .map(|(a, b)| {
                            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2))
                                .sqrt()
                        })
                        .sum::<f32>()
                    / labs.len() as f32
            };
            let row = (
                delta(&everything.iter().map(|s| s.lab).collect::<Vec<_>>()),
                delta(&transformed(&fitted.grade, &everything)),
                distance(
                    &stats,
                    &measure(&everything.iter().map(|s| s.lab).collect::<Vec<_>>()),
                )
                .total,
                distance(&stats, &measure(&transformed(&fitted.grade, &everything))).total,
            );
            if ri == ti {
                same.push(row);
            } else {
                cross.push(row);
                let identity = ReferenceGrade::identity();
                let edit = edits.get(ri).expect("edit");
                let mut without_tone = edit.clone();
                without_tone.tone.clone_from(&identity.tone);
                let mut half_tone = edit.clone();
                half_tone.tone = edit
                    .tone
                    .iter()
                    .zip(&identity.tone)
                    .map(|(e, i)| 0.5 * (e + i))
                    .collect();
                for (slot, variant) in
                    transferred
                        .iter_mut()
                        .zip([edit.clone(), without_tone, half_tone])
                {
                    let result = transformed(&variant, &everything);
                    slot.push((
                        row.0,
                        delta(&result),
                        row.2,
                        distance(&stats, &measure(&result)).total,
                    ));
                }
            }
        }
    }
    let summary = |rows: &[(f32, f32, f32, f32)]| {
        let n = rows.len().max(1) as f32;
        let mean = |pick: fn(&(f32, f32, f32, f32)) -> f32| rows.iter().map(pick).sum::<f32>() / n;
        let closer = rows.iter().filter(|r| r.1 < r.0).count();
        let mut ratios: Vec<f32> = rows.iter().map(|r| r.1 / r.0.max(1e-6)).collect();
        ratios.sort_by(f32::total_cmp);
        format!(
            "edit error {:.2} -> {:.2} (median ratio {:.2}, closer {closer}/{}), look distance {:.2} -> {:.2}",
            mean(|r| r.0),
            mean(|r| r.1),
            ratios.get(ratios.len() / 2).copied().unwrap_or(1.0),
            rows.len(),
            mean(|r| r.2),
            mean(|r| r.3)
        )
    };
    println!("photographs: {}, priors {priors:?}", photos.len());
    println!("same photograph:      {}", summary(&same));
    println!("different photograph: {}", summary(&cross));
    for (label, rows) in ["edit transferred", "edit without tone", "edit, half tone"]
        .iter()
        .zip(&transferred)
    {
        println!("{label:<20}: {}", summary(rows));
    }
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
fn noise_in_a_grey_photograph_is_not_turned_into_colour() {
    let grey = image(96, 96, |x, y| {
        [
            0.5 + noise(x, y, 9) * 0.012,
            0.5 + noise(x, y, 10) * 0.012,
            0.5 + noise(x, y, 12) * 0.012,
        ]
    });
    let reference = grade_image(&outdoor(), punchy);
    let pixels = samples(&grey, usize::MAX);
    let fit = fit(&reference_stats(&reference), &pixels, None);
    // A cast the reference's neutrals carry may move the whole grey; what must not happen is the
    // noise around that grey growing into colour. Spread about the mean colour, before and after.
    let spread = |labs: &[[f32; 3]]| {
        let n = labs.len() as f32;
        let mean = labs
            .iter()
            .fold([0.0_f32; 2], |m, l| [m[0] + l[1] / n, m[1] + l[2] / n]);
        labs.iter()
            .map(|l| (l[1] - mean[0]).hypot(l[2] - mean[1]))
            .sum::<f32>()
            / n
    };
    let before = spread(&pixels.iter().map(|s| s.lab).collect::<Vec<_>>());
    let after = spread(&transformed(&fit.grade, &pixels));
    assert!(after <= 3.0 * before + 0.002, "{before} -> {after}");
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
    old.version = 0;
    assert!(!old.usable());
}

#[test]
#[ignore = "diagnostic"]
fn which_component_carries_the_error_between_scenes() {
    let scenes = [
        ("outdoor", outdoor()),
        ("indoor", indoor()),
        ("studio", studio()),
        ("golden", golden()),
    ];
    for (gname, grade) in [GRADES[0], GRADES[3], GRADES[7]] {
        for (rname, rscene) in &scenes {
            for (tname, target) in &scenes {
                if rname == tname {
                    continue;
                }
                let reference = grade_image(rscene, grade);
                let truth = grade_image(target, grade);
                let pixels = samples(target, usize::MAX);
                let stats = reference_stats(&reference);
                let fitted = fit(&stats, &pixels, None);
                let identity = ReferenceGrade::identity();
                let g = &fitted.grade;
                let mut tone = identity.clone();
                tone.tone.clone_from(&g.tone);
                let mut cast = identity.clone();
                cast.cast_in.clone_from(&g.cast_in);
                cast.cast_out.clone_from(&g.cast_out);
                let mut chroma = identity.clone();
                chroma.chroma.clone_from(&g.chroma);
                chroma.cast_in.clone_from(&g.cast_in);
                chroma.cast_out.clone_from(&g.cast_in);
                let mut sectors = identity.clone();
                sectors.hue_gain.clone_from(&g.hue_gain);
                sectors.hue_shift.clone_from(&g.hue_shift);
                sectors.hue_light.clone_from(&g.hue_light);
                sectors.cast_in.clone_from(&g.cast_in);
                sectors.cast_out.clone_from(&g.cast_in);
                let mut zones = identity.clone();
                zones.zone_gain.clone_from(&g.zone_gain);
                let e = |m: &ReferenceGrade| error(&transformed(m, &pixels), &truth);
                println!("{gname} {rname}->{tname}: none {:.2} all {:.2} | tone {:.2} cast {:.2} chroma {:.2} sectors {:.2} zones {:.2} | casts in {:?} out {:?}",
                    e(&identity), e(g), e(&tone), e(&cast), e(&chroma), e(&sectors), e(&zones),
                    g.cast_in.iter().map(|c| format!("{:.3},{:.3}", c[0], c[1])).collect::<Vec<_>>(),
                    g.cast_out.iter().map(|c| format!("{:.3},{:.3}", c[0], c[1])).collect::<Vec<_>>());
            }
        }
    }
}

/// One shoot, one look: every frame of a real shoot given the same known edit, each frame matched
/// to another frame's edited version (ADR-0160). This is the batch a photographer brings: the
/// reference is one finished frame and the others share its light and its people.
///
/// `AURA_GRADE_SHOOT` names a folder of `NAME-before.png` neutral renders from the
/// `reference_grade_pairs` harness.
#[test]
#[ignore = "needs AURA_GRADE_SHOOT"]
#[allow(clippy::too_many_lines)]
fn one_shoot_one_look() {
    let folder = std::env::var("AURA_GRADE_SHOOT").expect("AURA_GRADE_SHOOT");
    let mut stems: Vec<String> = std::fs::read_dir(&folder)
        .expect("folder")
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix("-before.png").map(str::to_owned)
        })
        .collect();
    stems.sort();
    if let Some(skip) = std::env::var("AURA_GRADE_FROM")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        stems.drain(..skip.min(stems.len()));
    }
    if let Some(limit) = std::env::var("AURA_GRADE_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        stems.truncate(limit);
    }
    let frames: Vec<Rgb8> = stems
        .iter()
        .map(|stem| {
            let image = image::open(format!("{folder}/{stem}-before.png"))
                .expect("frame")
                .to_rgb8();
            Rgb8 {
                width: image.width(),
                height: image.height(),
                data: image.into_raw(),
            }
        })
        .collect();
    let full = Trust {
        ends: 1.0,
        tone: 1.0,
        cast: 1.0,
        chroma: 1.0,
        hue: 1.0,
    };
    println!("frames: {}", frames.len());
    let mut totals = [0.0_f32; 4];
    let mut count = 0.0;
    for (name, grade) in GRADES {
        let graded: Vec<Rgb8> = frames.iter().map(|f| grade_image(f, grade)).collect();
        let priors_only = std::env::var("AURA_GRADE_PRIORS_ONLY").is_ok();
        let edits: Vec<ReferenceGrade> = if priors_only {
            vec![ReferenceGrade::identity(); frames.len()]
        } else {
            frames
                .iter()
                .zip(&graded)
                .map(|(before, after)| learned(before, after))
                .collect()
        };
        let mut sums = [0.0_f32; 4];
        let mut closer = [0usize; 4];
        let mut pairs = 0;
        for (ri, reference) in graded.iter().enumerate() {
            let stats = reference_stats(reference);
            for (ti, target) in frames.iter().enumerate() {
                if ri == ti {
                    continue;
                }
                let truth: Vec<[f32; 3]> =
                    samples(&graded[ti], 60_000).iter().map(|s| s.lab).collect();
                let pixels = samples(target, 60_000);
                let delta = |labs: &[[f32; 3]]| {
                    100.0
                        * labs
                            .iter()
                            .zip(&truth)
                            .map(|(a, b)| {
                                ((a[0] - b[0]).powi(2)
                                    + (a[1] - b[1]).powi(2)
                                    + (a[2] - b[2]).powi(2))
                                .sqrt()
                            })
                            .sum::<f32>()
                        / labs.len() as f32
                };
                let similarity = super::super::content::similarity(
                    &super::super::content::thumbnail(reference),
                    target,
                );
                let fit_pixels = samples(target, 20_000);
                let default = match std::env::var("AURA_GRADE_TRUST").ok() {
                    Some(text) => {
                        let v: Vec<f32> = text
                            .split(',')
                            .map(|t| t.trim().parse().expect("number"))
                            .collect();
                        let content = content_of(similarity);
                        let towards = |prior: f32| prior + (1.0 - prior) * content;
                        let trust = Trust {
                            ends: towards(v[0]),
                            tone: towards(v[1]),
                            cast: towards(v[2]),
                            chroma: towards(v[3]),
                            hue: towards(v[4]),
                        };
                        fit_with(&stats, &fit_pixels, content, trust, || Ok(()))
                            .unwrap()
                            .grade
                    }
                    None => fit(&stats, &fit_pixels, similarity).grade,
                };
                let content = content_of(similarity);
                let whole = if priors_only {
                    ReferenceGrade::identity()
                } else {
                    fit_with(&stats, &fit_pixels, content, full, || Ok(()))
                        .unwrap()
                        .grade
                };
                let errors = [
                    delta(&pixels.iter().map(|s| s.lab).collect::<Vec<_>>()),
                    delta(&transformed(&default, &pixels)),
                    delta(&transformed(&whole, &pixels)),
                    delta(&transformed(&edits[ri], &pixels)),
                ];
                for k in 0..4 {
                    sums[k] += errors[k];
                    closer[k] += usize::from(errors[k] < errors[0]);
                }
                pairs += 1;
            }
        }
        let n = pairs as f32;
        println!(
            "{name:<14} none {:.2} | priors {:.2} ({}/{pairs}) | full trust {:.2} ({}/{pairs}) | edit transfer {:.2} ({}/{pairs})",
            sums[0] / n, sums[1] / n, closer[1], sums[2] / n, closer[2], sums[3] / n, closer[3]
        );
        for k in 0..4 {
            totals[k] += sums[k];
        }
        count += n;
    }
    println!(
        "overall        none {:.2} | priors {:.2} | full trust {:.2} | edit transfer {:.2}",
        totals[0] / count,
        totals[1] / count,
        totals[2] / count,
        totals[3] / count
    );
}

#[test]
#[ignore = "diagnostic"]
fn where_the_matte_residual_is() {
    let source = outdoor();
    let truth = grade_image(&source, matte);
    let pixels = samples(&source, usize::MAX);
    let fitted = fit(&reference_stats(&truth), &pixels, Some(1.0));
    let result = transformed(&fitted.grade, &pixels);
    let truth_labs = unchanged(&truth);
    let (mut dl, mut dc, mut dh, n) = (0.0_f32, 0.0_f32, 0.0_f32, result.len() as f32);
    for (a, b) in result.iter().zip(&truth_labs) {
        dl += (a[0] - b[0]).abs();
        let (ca, cb) = (a[1].hypot(a[2]), b[1].hypot(b[2]));
        dc += (ca - cb).abs();
        let da = (a[1] - b[1]).hypot(a[2] - b[2]);
        dh += (da * da - (ca - cb) * (ca - cb)).max(0.0).sqrt();
    }
    println!(
        "matte residual per pixel x100: lightness {:.2}, chroma {:.2}, hue {:.2}",
        100.0 * dl / n,
        100.0 * dc / n,
        100.0 * dh / n
    );
    println!(
        "grade: zone {:?} gain {:?} shift {:?} light {:?}",
        fitted.grade.zone_gain,
        fitted.grade.hue_gain,
        fitted.grade.hue_shift,
        fitted.grade.hue_light
    );
    println!("chroma curve {:?}", fitted.grade.chroma);
}
