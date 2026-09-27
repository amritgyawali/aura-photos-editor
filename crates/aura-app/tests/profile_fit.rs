//! Learning an edit profile from real before/after pairs: camera RAW in, a retoucher's final out.
//!
//! This is the tool behind every `origin: "learned"` profile in `config/edit_profiles.json`. It
//! is an ignored test rather than a binary because it needs exactly the renderer, the RAW decoder
//! and the profile builder the product ships, and nothing else - and a test target is the one
//! place in this workspace that may read a directory named on the command line.
//!
//! For every pair it:
//!
//! 1. decodes the DNG with AURA's own pure-Rust decoder (tier 2: black level, as-shot white
//!    balance, demosaic, camera matrix, linear Rec.2020) - the "before";
//! 2. reads the retoucher's final, converted to sRGB by `ml/edit-profiles/fetch_fivek_pairs.py`;
//! 3. measures what AURA's automatic correction would do to the frame;
//! 4. recovers, by coordinate descent through the real renderer, the twelve recipe settings that
//!    reproduce the final (`aura_style::fit`) - the retoucher's settings in AURA's units.
//!
//! The profile is the median, over the training pairs, of what the retoucher did *beyond* the
//! automatic correction. It is then measured on held-out pairs it never saw: the mean dE00 from
//! the final with the automatic correction alone, and with the correction plus the profile.
//!
//! ```text
//! python ml/edit-profiles/fetch_fivek_pairs.py --out D:/aura-data/fivek --expert c --count 40
//! AURA_FIVEK_DIR=D:/aura-data/fivek AURA_FIVEK_EXPERT=c \
//!   cargo test -p aura-app --test profile_fit -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_app::edit_profiles::{self, AutoCorrection, EditProfile, ProfileAdjust, SceneStats};
use aura_raw::codec::decode_png;
use aura_raw::colour::curve::linear_u16_to_scene;
use aura_raw::demosaic::{self, RgbF32};
use aura_raw::{DecodeLimits, PixelData};
use aura_recipe::Recipe;
use aura_render::{CpuEngine, Frame, OutputSpec, RenderLevel, RenderPurpose, RenderedData};
use aura_style::fit::{self, optimise::Axis, Budget};
use serde_json::json;

/// The fit's long edge. The fitter's own constant is 512; 384 keeps a debug-build run of a
/// hundred pairs under an hour and changes the recovered medians by less than their spread.
const EDGE: u32 = 384;

/// Pairs whose best fit is further than this from the final are not learned from.
const LEARN_BELOW_DE00: f32 = 5.0;

struct Pair {
    name: String,
    frame: Frame,
    target: Vec<u8>,
    auto: AutoCorrection,
    stats: SceneStats,
}

fn load(dng: &Path, png: &Path, clock: &dyn aura_core::clock::Clock) -> Result<Pair, String> {
    let bytes = std::fs::read(dng).map_err(|e| e.to_string())?;
    let meta = aura_raw::read_meta(&bytes, dng).map_err(|e| e.detail)?;
    let proxy = aura_raw::proxy::tier2(&bytes, &meta, DecodeLimits::default(), clock, None, dng)
        .map_err(|e| e.detail)?;
    let linear = &proxy.pair.linear;
    let PixelData::Linear16(codes) = &linear.data else {
        return Err("tier 2 produced no linear buffer".into());
    };
    let working = RgbF32 {
        width: linear.width,
        height: linear.height,
        data: codes.iter().map(|c| linear_u16_to_scene(*c)).collect(),
    };
    let (w, h) = demosaic::fit_long_edge(working.width, working.height, EDGE);
    let working = demosaic::resize(&working, w, h);

    let final_png = decode_png(
        &std::fs::read(png).map_err(|e| e.to_string())?,
        DecodeLimits::default(),
    )
    .map_err(|e| e.detail)?;
    let (a, b) = (
        f64::from(w) / f64::from(h),
        f64::from(final_png.width) / f64::from(final_png.height),
    );
    if (a - b).abs() / a > 0.01 {
        return Err(format!("the final is cropped ({a:.3} vs {b:.3})"));
    }
    let target = demosaic::to_srgb8(&demosaic::resize(
        &demosaic::from_srgb8(&final_png.data, final_png.width, final_png.height),
        w,
        h,
    ));

    let Some(preview) = proxy.pair.srgb.as_srgb8() else {
        return Err("tier 2 produced no sRGB preview".into());
    };
    Ok(Pair {
        name: png
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        frame: Frame::working(working.data, w, h, ""),
        target,
        auto: AutoCorrection::measure(preview).map_err(|e| e.detail)?,
        stats: SceneStats::measure(preview).map_err(|e| e.detail)?,
    })
}

fn render(engine: &CpuEngine, frame: &Frame, recipe: &Recipe) -> Vec<u8> {
    match engine
        .render_frame(
            frame,
            recipe,
            RenderLevel::Screen(frame.width, frame.height),
            RenderPurpose::Analysis,
            &OutputSpec::default(),
        )
        .expect("render")
        .data
    {
        RenderedData::Eight(bytes) => bytes,
        RenderedData::Sixteen(_) => panic!("8-bit output expected"),
    }
}

fn median(mut values: Vec<f32>) -> f32 {
    values.sort_by(f32::total_cmp);
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

fn round_i16(v: f32) -> i16 {
    v.round().clamp(-100.0, 100.0) as i16
}

#[test]
#[ignore = "needs downloaded FiveK pairs; see the module documentation"]
fn learn_a_profile_from_raw_and_retoucher_pairs() {
    let root = PathBuf::from(std::env::var("AURA_FIVEK_DIR").expect("set AURA_FIVEK_DIR"));
    let expert = std::env::var("AURA_FIVEK_EXPERT").unwrap_or_else(|_| "c".into());
    let limit: usize = std::env::var("AURA_FIVEK_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let clock = aura_core::clock::SystemClock::default();
    let engine = CpuEngine::new(
        aura_render::fixtures::StaticSource::shared(aura_render::fixtures::grey_frame(2, 2, 0.18)),
        Arc::new(aura_core::clock::SystemClock::default()),
    );

    let mut finals: Vec<PathBuf> = std::fs::read_dir(root.join(format!("expert_{expert}")))
        .expect("expert directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    finals.sort();
    finals.truncate(limit);

    let neutral = aura_recipe::fixtures::neutral(&"0".repeat(64), "");
    let mut rows = Vec::new();
    let mut fitted = Vec::new();
    for png in &finals {
        let stem = png
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dng = root.join("dng").join(format!("{stem}.dng"));
        let pair = match load(&dng, png, &clock) {
            Ok(pair) => pair,
            Err(why) => {
                println!("skip {stem}: {why}");
                continue;
            }
        };
        let result = fit::fit(
            &engine,
            &pair.frame,
            &neutral,
            &pair.target,
            Budget::default(),
        )
        .expect("fit");
        if let Ok(dump) = std::env::var("AURA_FIVEK_DUMP") {
            let save = |tag: &str, data: Vec<u8>| {
                let jpeg = aura_raw::codec::encode_jpeg(
                    &aura_raw::codec::Rgb8 {
                        width: pair.frame.width,
                        height: pair.frame.height,
                        data,
                    },
                    90,
                )
                .expect("jpeg");
                std::fs::write(Path::new(&dump).join(format!("{stem}_{tag}.jpg")), jpeg)
                    .expect("dump");
            };
            save("before", render(&engine, &pair.frame, &neutral));
            save(
                "fit",
                render(
                    &engine,
                    &pair.frame,
                    &result.candidate.to_params().into_recipe(&neutral),
                ),
            );
            save("final", pair.target.clone());
        }
        println!(
            "{stem}: residual {:.2} dE00 after {} renders; exposure {:+.2}, temp {:.0} K",
            result.residual_de00,
            result.iterations,
            result.candidate.get(Axis::Exposure),
            result.candidate.get(Axis::Temperature)
        );
        rows.push(json!({
            "name": pair.name,
            "residualDe00": result.residual_de00,
            "rejected": result.rejection().map(|c| format!("{c:?}")),
            "fitted": Axis::ALL.iter().map(|a| (a.as_str(), result.candidate.get(*a))).collect::<std::collections::BTreeMap<_, _>>(),
            "auto": { "exposure": pair.auto.exposure, "highlights": pair.auto.highlights,
                      "shadows": pair.auto.shadows, "contrast": pair.auto.contrast },
        }));
        fitted.push((pair, result));
    }
    assert!(fitted.len() >= 4, "too few usable pairs: {}", fitted.len());

    // Every fourth pair is held out, deterministically by name order. A rejected fit - local
    // retouching the develop pipeline cannot express - teaches nothing about global settings.
    let (train, held): (Vec<_>, Vec<_>) = fitted.iter().enumerate().partition(|(i, _)| i % 4 != 3);
    let train: Vec<_> = train
        .into_iter()
        .map(|(_, p)| p)
        // The develop fitter's own gate (3 dE00) is for learning a photographer's *residual*
        // from their own archive. A retoucher's final carries local work everywhere, so the
        // gate here is looser and only concentrated residuals - dodging, a sky swap - are
        // excluded. The median does the rest.
        .filter(|(_, r)| r.residual_de00 <= LEARN_BELOW_DE00 && !r.localised())
        .collect();
    let beyond = |axis: Axis, auto: &dyn Fn(&AutoCorrection) -> f32| {
        median(
            train
                .iter()
                .map(|(p, r)| r.candidate.get(axis) - auto(&p.auto))
                .collect(),
        )
    };
    let plain = |axis: Axis| median(train.iter().map(|(_, r)| r.candidate.get(axis)).collect());
    let low = plain(Axis::CurveLow).clamp(-40.0, 40.0);
    let high = plain(Axis::CurveHigh).clamp(-40.0, 40.0);
    let curve_low = (64.0 + low).round().clamp(1.0, 180.0) as u16;
    let curve_high = (192.0 + high)
        .round()
        .clamp(f32::from(curve_low) + 1.0, 254.0) as u16;
    let adjust = ProfileAdjust {
        exposure: (beyond(Axis::Exposure, &|a| a.exposure) * 100.0).round() / 100.0,
        contrast: round_i16(beyond(Axis::Contrast, &|a| f32::from(a.contrast))),
        highlights: round_i16(beyond(Axis::Highlights, &|a| f32::from(a.highlights))),
        shadows: round_i16(beyond(Axis::Shadows, &|a| f32::from(a.shadows))),
        whites: round_i16(plain(Axis::Whites)),
        blacks: round_i16(plain(Axis::Blacks)),
        temperature: (plain(Axis::Temperature) - 5500.0)
            .round()
            .clamp(-3000.0, 3000.0) as i32,
        tint: round_i16(plain(Axis::Tint)),
        vibrance: round_i16(plain(Axis::Vibrance)),
        saturation: round_i16(plain(Axis::Saturation)),
        curve: if (low.abs() + high.abs()) < 2.0 {
            Vec::new()
        } else {
            vec![[0, 0], [64, curve_low], [192, curve_high], [255, 255]]
        },
        ..ProfileAdjust::default()
    };
    let learned = EditProfile {
        id: format!("fivek-expert-{expert}"),
        name: String::new(),
        category: String::new(),
        tagline: String::new(),
        description: String::new(),
        best_for: Vec::new(),
        technique: Vec::new(),
        origin: "learned".into(),
        sources: Vec::new(),
        evidence: None,
        swatch: vec!["#000000".into()],
        adjust: adjust.clone(),
    };

    let (mut auto_sum, mut profile_sum, mut neutral_sum, mut oracle_sum) = (0.0, 0.0, 0.0, 0.0);
    for (_, (pair, result)) in &held {
        let (w, h) = (pair.frame.width as usize, pair.frame.height as usize);
        let de = |recipe: &Recipe| {
            fit::grid_de00(&render(&engine, &pair.frame, recipe), &pair.target, w, h).0
        };
        let (auto_recipe, _, _) =
            edit_profiles::build(&neutral, None, 1.0, pair.auto, pair.stats).expect("auto");
        let (profile_recipe, _, _) =
            edit_profiles::build(&neutral, Some(&learned), 1.0, pair.auto, pair.stats)
                .expect("profile");
        neutral_sum += de(&neutral);
        auto_sum += de(&auto_recipe);
        profile_sum += de(&profile_recipe);
        oracle_sum += result.residual_de00;
    }
    let n = held.len().max(1) as f32;
    let evidence = json!({
        "dataset": format!("MIT-Adobe FiveK, expert {}, camera RAW (DNG) to retoucher final", expert.to_uppercase()),
        "trainingPairs": train.len(),
        "heldOutPairs": held.len(),
        "autoDe00": ((auto_sum / n) * 100.0).round() / 100.0,
        "profileDe00": ((profile_sum / n) * 100.0).round() / 100.0,
    });
    println!(
        "held out {}: neutral {:.2}, auto {:.2}, auto+profile {:.2}, per-pair fit {:.2} dE00",
        held.len(),
        neutral_sum / n,
        auto_sum / n,
        profile_sum / n,
        oracle_sum / n
    );
    let out = json!({
        "expert": expert,
        "adjust": adjust,
        "evidence": evidence,
        "heldOut": { "neutralDe00": neutral_sum / n, "autoDe00": auto_sum / n,
                     "profileDe00": profile_sum / n, "perPairFitDe00": oracle_sum / n },
        "pairs": rows,
    });
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ml/edit-profiles")
        .join(format!("fivek_expert_{expert}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&out).expect("json")).expect("write");
    println!("wrote {}", path.display());
}

#[test]
#[ignore = "diagnostic: prints what the decoder read from one DNG"]
fn print_dng_meta() {
    let path = PathBuf::from(std::env::var("AURA_DNG").expect("set AURA_DNG"));
    let bytes = std::fs::read(&path).expect("read");
    let meta = aura_raw::read_meta(&bytes, &path).expect("meta");
    let mut shown = meta.clone();
    if let Some(m) = shown.mosaic.as_mut() {
        m.segments.truncate(2);
    }
    println!("{shown:#?}");
}
