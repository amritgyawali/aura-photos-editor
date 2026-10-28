//! The hundred cull checks on real photographs, for calibration. ADR-0185.
//!
//! Ignored. `AURA_CULL_PHOTOS` names a folder of JPEGs; `AURA_CULL_LIMIT` caps how many are
//! read (default 300); `AURA_CULL_OUT` writes one CSV row per photograph. Each photograph is
//! read the way the cull reads it - the long edge at 2048 for the checks, 512 for the faces -
//! and the run prints, for every check, how often it measured, the spread of its raw values
//! and how its scores fell. A check that fails most of a folder of good photographs is a
//! check whose threshold describes the instrument rather than the photograph.
//!
//! ```text
//! AURA_CULL_PHOTOS=F:/photos cargo test -p aura-app --test cull_checks_photos -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use aura_app::cull_checks::{self, Category, Sample, Settings, CHECKS, COUNT};

fn resized(image: &image::RgbImage, edge: u32) -> image::RgbImage {
    let (w, h) = image.dimensions();
    let scale = f64::from(edge) / f64::from(w.max(h));
    if scale >= 1.0 {
        return image.clone();
    }
    let (nw, nh) = (
        (f64::from(w) * scale).round().max(1.0) as u32,
        (f64::from(h) * scale).round().max(1.0) as u32,
    );
    image::imageops::resize(image, nw, nh, image::imageops::FilterType::Triangle)
}

/// The ways a photograph goes wrong, applied to a good one: `AURA_CULL_DEGRADE=1` measures
/// each photograph as shot and as each of these, so the run shows whether the score falls
/// when the photograph does.
const DAMAGE: [&str; 5] = [
    "as shot",
    "defocused",
    "motion smear",
    "underexposed",
    "blown out",
];

fn damaged(image: &image::RgbImage, kind: usize) -> image::RgbImage {
    match kind {
        1 => image::imageops::blur(image, 6.0),
        2 => {
            let (w, h) = image.dimensions();
            let radius = 14_i64;
            image::RgbImage::from_fn(w, h, |x, y| {
                let mut sum = [0_u32; 3];
                for dx in -radius..=radius {
                    let sx = (i64::from(x) + dx).clamp(0, i64::from(w) - 1) as u32;
                    let p = image.get_pixel(sx, y);
                    for c in 0..3 {
                        sum[c] += u32::from(p[c]);
                    }
                }
                let n = (2 * radius + 1) as u32;
                image::Rgb([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8])
            })
        }
        3 => image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
            let p = image.get_pixel(x, y);
            image::Rgb([p[0] / 8, p[1] / 8, p[2] / 8])
        }),
        4 => image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
            let p = image.get_pixel(x, y);
            image::Rgb(p.0.map(|v| (u32::from(v) * 3).min(255) as u8))
        }),
        _ => image.clone(),
    }
}

fn measure(path: &PathBuf) -> Option<(String, Vec<Option<f32>>, usize)> {
    measure_as(path, 0)
}

fn measure_as(path: &PathBuf, kind: usize) -> Option<(String, Vec<Option<f32>>, usize)> {
    let original = image::open(path).ok()?.to_rgb8();
    let image = if kind == 0 {
        original
    } else {
        damaged(&resized(&original, 2048), kind)
    };
    let detail = resized(&image, 2048);
    let thumb = resized(&image, 512);
    let faces = aura_vision::portrait::detect(thumb.as_raw(), thumb.width(), thumb.height())
        .unwrap_or_default();
    let values = cull_checks::measure(&Sample {
        rgb: detail.as_raw(),
        width: detail.width() as usize,
        height: detail.height() as usize,
        faces: &faces,
        settings: Settings {
            megapixels: Some(image.width() as f32 * image.height() as f32 / 1e6),
            ..Settings::default()
        },
    })?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some((name, values, faces.len()))
}

fn percentile(sorted: &[f32], fraction: f32) -> f32 {
    if sorted.is_empty() {
        return f32::NAN;
    }
    sorted[((sorted.len() - 1) as f32 * fraction).round() as usize]
}

#[test]
#[ignore = "needs AURA_CULL_PHOTOS, a folder of real photographs"]
fn the_hundred_checks_on_real_photographs() {
    let Some(dir) = std::env::var_os("AURA_CULL_PHOTOS") else {
        eprintln!("AURA_CULL_PHOTOS is not set");
        return;
    };
    let limit: usize = std::env::var("AURA_CULL_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("readable folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        })
        .collect();
    paths.sort();
    // An even sample across the folder rather than its first files, which are one scene.
    let step = (paths.len() / limit.max(1)).max(1);
    let paths: Vec<PathBuf> = paths.into_iter().step_by(step).take(limit).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let mut rows: Vec<(usize, String, Vec<Option<f32>>, usize)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(path) = paths.get(i) else { break };
                        if let Some((name, values, faces)) = measure(path) {
                            mine.push((i, name, values, faces));
                        }
                    }
                    mine
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker"))
            .collect()
    });
    rows.sort_by_key(|r| r.0);
    assert!(!rows.is_empty(), "no photograph could be read");
    let values: Vec<Vec<Option<f32>>> = rows.iter().map(|r| r.2.clone()).collect();
    let scored = cull_checks::score(&values);
    if std::env::var_os("AURA_CULL_DEGRADE").is_some() {
        // Every damaged copy is scored in one collection with the originals, as a cull would.
        let sample: Vec<&PathBuf> = paths.iter().take(60).collect();
        let mut all = Vec::new();
        let mut kinds = Vec::new();
        for (kind, _) in DAMAGE.iter().enumerate() {
            for path in &sample {
                if let Some((_, v, _)) = measure_as(path, kind) {
                    all.push(v);
                    kinds.push(kind);
                }
            }
        }
        let damaged_scores = cull_checks::score(&all);
        for (kind, label) in DAMAGE.iter().enumerate() {
            let mut overall: Vec<f32> = damaged_scores
                .iter()
                .zip(&kinds)
                .filter(|(_, k)| **k == kind)
                .map(|(s, _)| s.overall)
                .collect();
            overall.sort_by(f32::total_cmp);
            println!(
                "damage {:<14} n {:>3}  p10 {:>5.1}  p50 {:>5.1}  p90 {:>5.1}  below 25: {:>3}",
                label,
                overall.len(),
                percentile(&overall, 0.1),
                percentile(&overall, 0.5),
                percentile(&overall, 0.9),
                overall.iter().filter(|s| **s < 25.0).count()
            );
        }
        // The share of each kind under each candidate line: a strictness level's low-score
        // line should catch damage without leaving out photographs as shot.
        for line in [15.0_f32, 25.0, 35.0, 45.0, 50.0, 55.0] {
            let shares: Vec<String> = DAMAGE
                .iter()
                .enumerate()
                .map(|(kind, label)| {
                    let of_kind: Vec<f32> = damaged_scores
                        .iter()
                        .zip(&kinds)
                        .filter(|(_, k)| **k == kind)
                        .map(|(s, _)| s.overall)
                        .collect();
                    let under = of_kind.iter().filter(|s| **s < line).count();
                    format!(
                        "{label} {:.0}%",
                        under as f32 * 100.0 / of_kind.len().max(1) as f32
                    )
                })
                .collect();
            println!("under {line:.0}: {}", shares.join(", "));
        }
    }
    println!(
        "{} photographs, {} with a face",
        rows.len(),
        rows.iter().filter(|r| r.3 > 0).count()
    );
    println!(
        "{:<32} {:>5} {:>10} {:>10} {:>10} {:>6} {:>6}",
        "check", "n", "p10", "p50", "p90", "score", "<50%"
    );
    for (index, check) in CHECKS.iter().enumerate().take(cull_checks::CONTEXT) {
        let mut raw: Vec<f32> = values.iter().filter_map(|v| v[index]).collect();
        raw.sort_by(f32::total_cmp);
        let scores: Vec<f32> = scored
            .iter()
            .filter_map(|s| s.checks[index].map(f32::from))
            .collect();
        let mean = scores.iter().sum::<f32>() / scores.len().max(1) as f32;
        let failing =
            scores.iter().filter(|s| **s < 50.0).count() as f32 / scores.len().max(1) as f32;
        println!(
            "{:<32} {:>5} {:>10.4} {:>10.4} {:>10.4} {:>6.0} {:>5.0}%",
            check.id,
            raw.len(),
            percentile(&raw, 0.1),
            percentile(&raw, 0.5),
            percentile(&raw, 0.9),
            mean,
            failing * 100.0
        );
    }
    for (index, category) in Category::ALL.iter().enumerate() {
        let mut scores: Vec<f32> = scored
            .iter()
            .filter_map(|s| s.categories[index].map(f32::from))
            .collect();
        scores.sort_by(f32::total_cmp);
        println!(
            "group {:<22} n {:>4}  p10 {:>5.0}  p50 {:>5.0}  p90 {:>5.0}",
            category.label(),
            scores.len(),
            percentile(&scores, 0.1),
            percentile(&scores, 0.5),
            percentile(&scores, 0.9)
        );
    }
    let mut overall: Vec<f32> = scored.iter().map(|s| s.overall).collect();
    overall.sort_by(f32::total_cmp);
    println!(
        "overall (own ninety checks)  p10 {:.0}  p50 {:.0}  p90 {:.0}  below 25: {}",
        percentile(&overall, 0.1),
        percentile(&overall, 0.5),
        percentile(&overall, 0.9),
        overall.iter().filter(|s| **s < 25.0).count()
    );
    if let Some(out) = std::env::var_os("AURA_CULL_OUT") {
        let mut csv = String::from("file,faces,overall");
        for check in &CHECKS {
            csv.push(',');
            csv.push_str(check.id);
        }
        csv.push('\n');
        for (row, scores) in rows.iter().zip(&scored) {
            csv.push_str(&format!("{},{},{:.1}", row.1, row.3, scores.overall));
            for value in row.2.iter().take(COUNT) {
                csv.push(',');
                if let Some(v) = value {
                    csv.push_str(&format!("{v:.4}"));
                }
            }
            csv.push('\n');
        }
        std::fs::write(out, csv).expect("csv written");
    }
}
