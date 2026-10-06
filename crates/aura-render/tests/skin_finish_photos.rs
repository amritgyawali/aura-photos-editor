//! Real-photograph check of frequency healing and the texture graft, run by hand. ADR-0090.
//!
//! `AURA_SKIN_PHOTOS` names a folder holding `NAME_WxH.rgb` (packed sRGB) and, beside each,
//! the `NAME_WxH.PRESET.recipe.json` that `aura-app`'s `auto_retouch_photos` check wrote for
//! it (`AURA_FINISH_PRESET`, default `deep`). `AURA_FINISH_STACK` says what to render, as a
//! comma-separated list applied in order:
//!
//! | Token | Operations |
//! |---|---|
//! | `base` | every operation in the recipe |
//! | `skin` | the recipe without its spot repairs and without its surface finish |
//! | `spots` | the recipe's spot repairs |
//! | `finish` | the recipe's surface finish |
//! | `clear` | one frequency heal over the recipe's surface selection |
//! | `graft` | one texture graft over the recipe's surface selection |
//!
//! `clear` and `graft` take parameters after a colon, separated by semicolons:
//! `clear:radius=0.0045;sens=0.6;tone=1;texture=0.5;dark=0` or `graft:radius=0.003;texture=1`.
//! The result is written as `NAME_WxH.finish.LABEL.rgb` (`AURA_FINISH_LABEL`, default `lab`),
//! and what a `clear` would rebuild on the untouched photograph as `NAME_WxH.marks.LABEL.rgb`.
//! The images are for a person to look at; the assertions are that the stack validates and
//! that nothing outside the selections changes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use aura_recipe::retouch_tools::{self, Edit, Tool};

fn working_pixels(rgb: &[u8]) -> Vec<f32> {
    use aura_raw::colour::matrix::{invert, mul, REC2020_TO_XYZ_D65, SRGB_TO_XYZ_D65};
    let matrix =
        aura_render::colour::narrow(mul(invert(REC2020_TO_XYZ_D65).unwrap(), SRGB_TO_XYZ_D65));
    rgb.chunks_exact(3)
        .flat_map(|p| {
            aura_render::colour::apply_f32(
                matrix,
                [p[0], p[1], p[2]]
                    .map(|v| aura_raw::colour::curve::srgb_decode(f32::from(v) / 255.0)),
            )
        })
        .collect()
}

fn display_pixels(linear: &[f32]) -> Vec<u8> {
    let matrix = aura_render::output::working_to_output(aura_render::OutputColour::Srgb);
    linear
        .chunks_exact(3)
        .flat_map(|p| {
            aura_render::colour::apply_f32(matrix, [p[0], p[1], p[2]]).map(|v| {
                (aura_raw::colour::curve::srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8
            })
        })
        .collect()
}

/// A new operation over the surface selection, with `key=value` overrides.
fn authored(template: &Edit, tool: Tool, id: &str, parameters: &str) -> Edit {
    let mut edit = template.clone();
    edit.id = id.into();
    edit.tool = tool;
    edit.amount = 1.0;
    edit.tone = 1.0;
    edit.source = None;
    edit.preserve_microtexture = false;
    if tool == Tool::FrequencyHeal {
        edit.radius = 0.0045;
        edit.texture = 0.5;
    } else {
        edit.radius = 0.002;
        edit.texture = 0.9;
    }
    for pair in parameters.split(';').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').expect("key=value");
        let value: f32 = value.parse().expect("a number");
        match key {
            "radius" => edit.radius = value,
            "sens" => edit.sensitivity = Some(value),
            "tone" => edit.tone = value,
            "texture" => edit.texture = value,
            "amount" => edit.amount = value,
            "dark" => edit.keep_dark_marks = value > 0.5,
            other => panic!("unknown parameter {other}"),
        }
    }
    edit
}

#[test]
#[ignore = "needs AURA_SKIN_PHOTOS; writes images for a person to look at"]
fn finishes_real_photographs() {
    let dir = std::env::var("AURA_SKIN_PHOTOS").expect("AURA_SKIN_PHOTOS");
    let preset = std::env::var("AURA_FINISH_PRESET").unwrap_or_else(|_| "deep".into());
    let stack_spec = std::env::var("AURA_FINISH_STACK").unwrap_or_else(|_| "clear,graft".into());
    let label = std::env::var("AURA_FINISH_LABEL").unwrap_or_else(|_| "lab".into());
    let mut inputs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "rgb")
                && p.file_stem()
                    .is_some_and(|s| !s.to_string_lossy().contains('.'))
        })
        .collect();
    assert!(!inputs.is_empty(), "No NAME_WxH.rgb inputs found");
    inputs.sort();
    for path in inputs {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let (w, h) = stem.rsplit('_').next().unwrap().split_once('x').unwrap();
        let (w, h): (usize, usize) = (w.parse().unwrap(), h.parse().unwrap());
        let rgb = std::fs::read(&path).unwrap();
        assert_eq!(rgb.len(), w * h * 3);
        let recipe_path = path.with_file_name(format!("{stem}.{preset}.recipe.json"));
        let recipe: aura_recipe::Recipe =
            serde_json::from_slice(&std::fs::read(&recipe_path).expect("the planned recipe"))
                .unwrap();
        let planned = retouch_tools::read(&recipe).unwrap();
        let mattes = retouch_tools::read_mattes(&recipe).unwrap();
        let surface = planned
            .iter()
            .find(|e| e.id.ends_with("-surface-finish"))
            .expect("a surface finish to take the selection from");
        let is_spot = |e: &&Edit| e.id.contains("-spot-");
        let is_finish = |e: &&Edit| e.id.ends_with("-surface-finish");
        let mut stack: Vec<Edit> = Vec::new();
        for (n, token) in stack_spec.split(',').enumerate() {
            let (name, parameters) = token.split_once(':').unwrap_or((token, ""));
            match name {
                "base" => stack.extend(planned.iter().cloned()),
                "skin" => stack.extend(
                    planned
                        .iter()
                        .filter(|e| !is_spot(e) && !is_finish(e))
                        .cloned(),
                ),
                "spots" => stack.extend(planned.iter().filter(is_spot).cloned()),
                "finish" => stack.extend(planned.iter().filter(is_finish).cloned()),
                "clear" => stack.push(authored(
                    surface,
                    Tool::FrequencyHeal,
                    &format!("lab-{n}-frequency-heal"),
                    parameters,
                )),
                "graft" => stack.push(authored(
                    surface,
                    Tool::TextureGraft,
                    &format!("lab-{n}-texture-graft"),
                    parameters,
                )),
                other => panic!("unknown stack token {other}"),
            }
        }
        retouch_tools::validate(&stack).unwrap();
        let source = working_pixels(&rgb);
        let mut selected = vec![false; w * h];
        for edit in &stack {
            let mask = aura_render::retouch_tools::selection_mask_with_mattes(
                &source, w, h, edit, &mattes,
            );
            for (selected, alpha) in selected.iter_mut().zip(mask) {
                *selected |= alpha > 0.0;
            }
        }
        if let Some(clear) = stack.iter().find(|e| e.tool == Tool::FrequencyHeal) {
            let marks =
                aura_render::retouch_tools::frequency_heal_marks(&source, w, h, clear, &mattes);
            let covered = marks.iter().filter(|m| **m > 0.5).count();
            let mut overlay = rgb.clone();
            for (pixel, m) in overlay.chunks_exact_mut(3).zip(&marks) {
                for (value, colour) in pixel.iter_mut().zip([0.0, 255.0, 120.0]) {
                    *value = (f32::from(*value) * (1.0 - m * 0.75) + colour * m * 0.75) as u8;
                }
            }
            std::fs::write(
                path.with_file_name(format!("{stem}.marks.{label}.rgb")),
                &overlay,
            )
            .unwrap();
            println!(
                "{stem} [{label}]: frequency heal would rebuild {:.2}% of the frame",
                covered as f32 / (w * h) as f32 * 100.0
            );
        }
        let mut linear = source.clone();
        let started = std::time::Instant::now();
        aura_render::retouch_tools::apply_with_mattes(&mut linear, w, h, &stack, &mattes);
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
        std::fs::write(
            path.with_file_name(format!("{stem}.finish.{label}.rgb")),
            display_pixels(&linear),
        )
        .unwrap();
        println!(
            "{stem} [{label}]: {} operations, rendered in {:.2}s",
            stack.len(),
            rendering.as_secs_f32()
        );
    }
}
