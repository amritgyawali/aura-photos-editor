//! Real-photograph check of the automatic retouch, run by hand. ADR-0082.
//!
//! `AURA_SKIN_PHOTOS` names a folder of `NAME_WxH.rgb` files (packed sRGB, made from JPEGs by
//! `ml/models/skin/to_raw.py`). For every photograph and every preset named in
//! `AURA_RETOUCH_PRESETS` (default `natural`) this plans the automatic retouch exactly as the
//! app does, applies it with the renderer's retouch stage, and writes
//! `NAME.PRESET.after.rgb` and `NAME.selection.rgb` (what the face and body skin operations
//! select) beside the input. The images are for a person to look at; the assertions are that
//! the plan validates, stays inside the operation limit and changes the photograph only
//! where it selected something.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use aura_app::{portrait_auto, portrait_features, retouch_settings::Settings};
use aura_recipe::retouch_tools;

fn decode(v: u8) -> f32 {
    aura_raw::colour::curve::srgb_decode(f32::from(v) / 255.0)
}

fn encode(v: f32) -> u8 {
    (aura_raw::colour::curve::srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8
}

fn working_pixels(rgb: &[u8]) -> Vec<f32> {
    use aura_raw::colour::matrix::{invert, mul, REC2020_TO_XYZ_D65, SRGB_TO_XYZ_D65};
    let matrix =
        aura_render::colour::narrow(mul(invert(REC2020_TO_XYZ_D65).unwrap(), SRGB_TO_XYZ_D65));
    rgb.chunks_exact(3)
        .flat_map(|p| {
            aura_render::colour::apply_f32(matrix, [decode(p[0]), decode(p[1]), decode(p[2])])
        })
        .collect()
}

fn preset(name: &str) -> portrait_features::Options {
    let mut options = portrait_features::Options {
        scope: portrait_features::Scope::FaceAndBody,
        ..portrait_features::Options::default()
    };
    let s = &mut options.settings;
    match name {
        "deep" => {
            s.deep_blemish_cleanup = true;
            s.remove_dark_marks = true;
            s.keep_freckles = false;
            s.max_spots = 220;
            s.blemish_sensitivity = 0.9;
            s.smoothing = 0.8;
            s.texture = 0.85;
            s.tone_evenness = 0.7;
            s.micro_dodge_burn = 0.65;
            s.pore_refine = 0.3;
            s.shine = 0.85;
            s.hair_detail = 0.4;
            s.hair_shine = 0.2;
        }
        // Heal, frequency separation, dodge and burn, then real texture back. ADR-0090.
        "pro" => {
            s.deep_blemish_cleanup = true;
            s.remove_dark_marks = true;
            s.keep_freckles = false;
            s.max_spots = 220;
            s.blemish_sensitivity = 0.8;
            s.frequency_heal = 1.0;
            s.texture_graft = 0.75;
            s.smoothing = 0.8;
            s.texture = 0.85;
            s.tone_evenness = 0.7;
            s.light_evenness = 0.5;
            s.micro_dodge_burn = 0.65;
            s.pore_refine = 0.3;
            s.shine = 0.85;
            s.hair_detail = 0.4;
            s.hair_shine = 0.2;
        }
        "soft" => {
            s.smoothing = 0.75;
            s.texture = 0.4;
            s.glow = 0.4;
            s.eye_whitening = 0.4;
            s.blush = 0.25;
        }
        "beauty" => {
            s.smoothing = 0.85;
            s.tone_evenness = 0.75;
            s.micro_dodge_burn = 0.6;
            s.contour = 0.5;
            s.highlight = 0.5;
            s.lip_colour = 0.4;
            s.lash_definition = 0.5;
            s.brow_definition = 0.4;
            s.iris_brightness = 0.4;
            s.hair_detail = 0.4;
            s.hair_shine = 0.3;
            s.fabric = 0.5;
            s.backdrop = 0.4;
            s.match_body_to_face = 0.6;
            s.body_redness = 0.4;
            s.neck_lines = 0.5;
        }
        "off" => {
            *s = Settings {
                smoothing: 0.0,
                tone_evenness: 0.0,
                light_evenness: 0.0,
                micro_dodge_burn: 0.0,
                shine: 0.0,
                redness: 0.0,
                ..Settings::default()
            };
        }
        _ => {}
    }
    options
}

#[test]
#[ignore = "needs AURA_SKIN_PHOTOS; writes before/after images for a person to look at"]
fn retouches_real_photographs() {
    let dir = std::env::var("AURA_SKIN_PHOTOS").expect("AURA_SKIN_PHOTOS");
    let presets = std::env::var("AURA_RETOUCH_PRESETS").unwrap_or_else(|_| "natural".into());
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "rgb")
                && p.file_stem()
                    .is_some_and(|s| !s.to_string_lossy().contains('.'))
        })
        .collect();
    assert!(
        !entries.is_empty(),
        "No NAME_WxH.rgb inputs found in AURA_SKIN_PHOTOS"
    );
    entries.sort();
    for path in entries {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let dims = stem.rsplit('_').next().unwrap();
        let (w, h) = dims.split_once('x').unwrap();
        let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
        let rgb = std::fs::read(&path).unwrap();
        let mut faces = aura_vision::portrait::detect(&rgb, w, h).unwrap();
        faces = aura_vision::portrait::detect_small_faces(&rgb, w, h, &faces).unwrap_or(faces);
        for name in presets.split(',') {
            let options = preset(name);
            let recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
            let started = std::time::Instant::now();
            let plan = portrait_auto::plan_with_faces(
                &recipe,
                &rgb,
                w,
                h,
                None,
                0.0,
                Some(faces.clone()),
                &options,
                true,
            )
            .unwrap();
            let planning = started.elapsed();
            let mut stack =
                portrait_auto::staged(&[], &plan.groups, portrait_auto::Group::Finishing);
            // Exercise every intermediate history step on a repeat pass, not only the
            // final stack: a misgrouped operation can otherwise be duplicated mid-pass.
            let mut repeated = stack.clone();
            for group in portrait_auto::Group::ALL {
                repeated = portrait_auto::staged(&repeated, &plan.groups, group);
                retouch_tools::validate(&repeated).unwrap();
                assert_eq!(repeated, stack, "Repeat pass changed {stem} at {group:?}");
            }
            // `AURA_RETOUCH_ONLY=texture,tone` keeps only operations whose id contains one of
            // the words, to see what each one does.
            if let Ok(only) = std::env::var("AURA_RETOUCH_ONLY") {
                stack.retain(|e| only.split(',').any(|w| e.id.contains(w)));
            }
            retouch_tools::validate(&stack).unwrap();
            assert!(stack.len() <= retouch_tools::MAX_EDITS);
            let mut with = recipe.clone();
            retouch_tools::write_with_mattes(&mut with, &stack, &plan.mattes).unwrap();
            aura_recipe::schema::Validation::check(&with).unwrap();
            std::fs::write(
                path.with_file_name(format!("{stem}.{name}.recipe.json")),
                serde_json::to_vec_pretty(&with).unwrap(),
            )
            .unwrap();
            std::fs::write(
                path.with_file_name(format!("{stem}.{name}.report.json")),
                serde_json::to_vec_pretty(&plan.report).unwrap(),
            )
            .unwrap();
            let mattes = retouch_tools::read_mattes(&with).unwrap();
            let mut linear: Vec<f32> = working_pixels(&rgb);
            let source = linear.clone();
            let mut selected = vec![false; (w * h) as usize];
            for edit in &stack {
                let mask = aura_render::retouch_tools::selection_mask_with_mattes(
                    &source, w as usize, h as usize, edit, &mattes,
                );
                for (selected, alpha) in selected.iter_mut().zip(mask) {
                    *selected |= alpha > 0.0;
                }
            }
            let started = std::time::Instant::now();
            aura_render::retouch_tools::apply_with_mattes(
                &mut linear,
                w as usize,
                h as usize,
                &stack,
                &mattes,
            );
            let rendering = started.elapsed();
            for (i, (before, after)) in source
                .chunks_exact(3)
                .zip(linear.chunks_exact(3))
                .enumerate()
            {
                if !selected[i] {
                    assert_eq!(before, after, "Changed unselected pixel {i} in {stem}");
                }
                assert!(after.iter().all(|v| v.is_finite()));
            }
            let matrix = aura_render::output::working_to_output(aura_render::OutputColour::Srgb);
            let after: Vec<u8> = linear
                .chunks_exact(3)
                .flat_map(|p| {
                    aura_render::colour::apply_f32(matrix, [p[0], p[1], p[2]]).map(encode)
                })
                .collect();
            std::fs::write(
                path.with_file_name(format!("{stem}.{name}.after.rgb")),
                &after,
            )
            .unwrap();
            if name == presets.split(',').next().unwrap_or("natural") {
                // What the face and body skin operations select, as magenta and cyan.
                let source: Vec<f32> = working_pixels(&rgb);
                let mut overlay = rgb.clone();
                for edit in stack
                    .iter()
                    .filter(|e| e.id.ends_with("-texture") || e.id.ends_with("-body-texture"))
                {
                    let colour = if edit.id.contains("-body-") {
                        [0.0, 220.0, 255.0]
                    } else {
                        [255.0, 0.0, 210.0]
                    };
                    let mask = aura_render::retouch_tools::selection_mask_with_mattes(
                        &source, w as usize, h as usize, edit, &mattes,
                    );
                    for (i, a) in mask.iter().enumerate() {
                        let a = a * 0.6;
                        for c in 0..3 {
                            let v = &mut overlay[i * 3 + c];
                            *v = (f32::from(*v) * (1.0 - a) + colour[c] * a).round() as u8;
                        }
                    }
                }
                std::fs::write(
                    path.with_file_name(format!("{stem}.selection.rgb")),
                    &overlay,
                )
                .unwrap();
            }
            let changed = rgb
                .chunks_exact(3)
                .zip(after.chunks_exact(3))
                .filter(|(a, b)| a != b)
                .count();
            println!(
                "{stem} [{name}]: {} faces, {} ops, {} mattes, {} passes, plan {:.2}s, render {:.2}s, {:.1}% pixels changed",
                faces.len(),
                stack.len(),
                plan.mattes.len(),
                plan.report.segmentation.as_ref().map_or(0, |s| s.passes),
                planning.as_secs_f32(),
                rendering.as_secs_f32(),
                changed as f32 / (w * h) as f32 * 100.0
            );
            for assessment in &plan.report.assessments {
                println!(
                    "    face {} {}: {}",
                    assessment.face, assessment.status, assessment.reason
                );
                for finding in assessment.findings.iter().take(2) {
                    println!("    face {}: {finding}", assessment.face);
                }
            }
            if faces.is_empty() {
                assert!(stack.is_empty());
            }
        }
    }
}
