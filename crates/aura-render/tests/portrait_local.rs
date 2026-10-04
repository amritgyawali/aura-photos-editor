//! Before and after on photographs that are not in this repository.
//!
//! ```text
//! AURA_PORTRAIT_EVAL_DIR=/path/to/ppm AURA_PORTRAIT_EVAL_OUT=/path/to/out \
//!     cargo test -p aura-render --release --test portrait_local -- --ignored --nocapture
//! ```
//!
//! An instrument, not a gate: it writes side-by-side images for a person to look at, which is
//! the only evaluation of a retouch that means anything.

use std::path::PathBuf;
use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_raw::colour::{curve, matrix, working_space};
use aura_recipe::fixtures as recipes;
use aura_recipe::RetouchOp;
use aura_render::contract::render::{RenderLevel, RenderPurpose};
use aura_render::cpu::{CpuEngine, Frame};
use aura_render::fixtures;
use aura_render::graph::{self, Capabilities, InputKind};
use time::OffsetDateTime;

fn read_ppm(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = std::fs::read(path).ok()?;
    let mut fields = Vec::new();
    let mut at = 0;
    while fields.len() < 4 {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let start = at;
        while bytes.get(at).is_some_and(|b| !b.is_ascii_whitespace()) {
            at += 1;
        }
        fields.push(String::from_utf8_lossy(bytes.get(start..at)?).to_string());
    }
    at += 1;
    if fields.first()? != "P6" {
        return None;
    }
    Some((
        fields.get(1)?.parse().ok()?,
        fields.get(2)?.parse().ok()?,
        bytes.get(at..)?.to_vec(),
    ))
}

#[test]
#[ignore = "needs AURA_PORTRAIT_EVAL_DIR and AURA_PORTRAIT_EVAL_OUT"]
fn natural_retouch_before_and_after() {
    let (Some(dir), Some(out)) = (
        std::env::var_os("AURA_PORTRAIT_EVAL_DIR").map(PathBuf::from),
        std::env::var_os("AURA_PORTRAIT_EVAL_OUT").map(PathBuf::from),
    ) else {
        return;
    };
    let to_working = matrix::mul(working_space::xyz_d65_to_rec2020(), matrix::SRGB_TO_XYZ_D65);
    let to_srgb = working_space::rec2020_to_srgb();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.sort();
    for path in paths {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with("full_") || path.extension().is_none_or(|e| e != "ppm") {
            continue;
        }
        let Some((w, h, bytes)) = read_ppm(&path) else {
            continue;
        };
        let rgb: Vec<f32> = bytes
            .chunks_exact(3)
            .flat_map(|p| {
                let lin = [0, 1, 2].map(|c| f64::from(curve::srgb_decode(f32::from(p[c]) / 255.0)));
                matrix::apply(to_working, lin).map(|v| v as f32)
            })
            .collect();
        let frame = Frame::working(rgb, w, h, "Bench-01");
        let engine = CpuEngine::new(
            Arc::new(fixtures::StaticSource::new(frame.clone())),
            FixedClock::at(OffsetDateTime::UNIX_EPOCH),
        );
        let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
        for (op, strength) in [
            ("skin_smooth", 0.6),
            ("skin_even", 0.5),
            ("blemish_clear", 0.7),
            ("under_eye_lift", 0.5),
            ("shine_control", 0.5),
            ("face_light", 0.2),
            ("eye_brighten", 0.5),
            ("iris_enhance", 0.5),
            ("sclera_whiten", 0.5),
            ("teeth_whiten", 0.6),
            ("lip_enhance", 0.3),
            ("hair_define", 0.4),
            ("background_blur", 0.5),
        ] {
            recipe.retouch.push(RetouchOp {
                op: op.to_string(),
                strength,
                protect_texture: 0.7,
                mask: None,
                borrowed_from: None,
            });
        }
        let caps = Capabilities {
            mask_generators: true,
            retouch_operators: true,
            geometry_models: true,
            ..Capabilities::default()
        };
        let started = std::time::Instant::now();
        let plan = graph::plan(&recipe, RenderPurpose::Export, InputKind::Working, caps);
        let (after, _, _, notes) =
            engine.working_buffer(&frame, &recipe, &plan, RenderLevel::Full, None);
        println!(
            "{name}: {:.0} ms {:?}",
            started.elapsed().as_secs_f32() * 1000.0,
            notes.iter().map(|n| n.reason).collect::<Vec<_>>()
        );
        let encode = |buffer: &[f32]| -> Vec<u8> {
            buffer
                .chunks_exact(3)
                .flat_map(|p| {
                    let lin =
                        matrix::apply(to_srgb, [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
                    lin.map(|v| {
                        (curve::srgb_encode(v as f32) * 255.0)
                            .round()
                            .clamp(0.0, 255.0) as u8
                    })
                })
                .collect()
        };
        let a = encode(&frame.rgb);
        let b = encode(&after);
        let mut side = Vec::with_capacity(a.len() * 2);
        for y in 0..h as usize {
            let row = y * w as usize * 3..(y + 1) * w as usize * 3;
            side.extend_from_slice(&a[row.clone()]);
            side.extend_from_slice(&b[row]);
        }
        let mut file = format!("P6\n{} {h}\n255\n", w * 2).into_bytes();
        file.extend_from_slice(&side);
        let _ = std::fs::write(out.join(format!("{name}_retouch.ppm")), file);
    }
}
