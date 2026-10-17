//! Real-photograph check of the edge-aware stages, run by hand. ADR-0126.
//!
//! `AURA_LOCAL_PHOTOS` names a folder of `NAME_WxH.rgb` files (packed 8-bit sRGB).
//! `AURA_LOCAL_VARIANTS` lists what to render, separated by semicolons, each as
//! `LABEL:key=value,key=value` with keys `exposure`, `shadows`, `highlights`, `whites`,
//! `blacks`, `contrast`, `clarity`, `texture`, `dehaze`, `vibrance`. A label starting with
//! `legacy` renders Shadows, Highlights, Clarity and Dehaze the way they were before ADR-0126
//! (a per-pixel curve, a blur and a global black-point lift), so the two can be compared side
//! by side. Each result is written beside its source as `NAME_WxH.LABEL.rgb`.
//!
//! The images are for a person to look at. The assertions are that every render completes,
//! that a tiled render of the first variant matches the whole one (skipped when
//! `AURA_LOCAL_NO_TILES` is set, for timing), and that the edge-aware
//! Shadows keeps more of a lifted region's texture than the per-pixel curve does.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use std::path::Path;
use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_recipe::fixtures as recipes;
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose, RenderedData};
use aura_render::cpu::CpuEngine;
use aura_render::{fixtures, spatial, tonemap, Frame};
use time::OffsetDateTime;

fn working(rgb: &[u8]) -> Vec<f32> {
    rgb.chunks_exact(3)
        .flat_map(|p| {
            aura_render::output::display_srgb_to_working(
                [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0),
            )
        })
        .collect()
}

fn engine(frame: Frame) -> CpuEngine {
    CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame)),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    )
}

fn recipe_of(spec: &str) -> aura_recipe::Recipe {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "photo");
    for pair in spec.split(',').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').expect("key=value");
        let n: f32 = value.parse().expect("number");
        let g = &mut recipe.global;
        match key {
            "exposure" => g.exposure = n,
            "shadows" => g.shadows = n as i16,
            "highlights" => g.highlights = n as i16,
            "whites" => g.whites = n as i16,
            "blacks" => g.blacks = n as i16,
            "contrast" => g.contrast = n as i16,
            "clarity" => g.clarity = n as i16,
            "texture" => g.texture = n as i16,
            "dehaze" => g.dehaze = n as i16,
            "vibrance" => g.vibrance = n as i16,
            other => panic!("unknown key {other}"),
        }
    }
    recipe
}

/// What Shadows, Highlights, Clarity and Dehaze did before ADR-0126, applied to the working
/// buffer directly.
fn legacy(rgb: &mut [f32], width: usize, height: usize, recipe: &aura_recipe::Recipe) {
    let g = &recipe.global;
    let stats = spatial::measure(rgb, width, height);
    let tone = tonemap::Tone {
        highlights: f32::from(g.highlights) / 100.0,
        shadows: f32::from(g.shadows) / 100.0,
        whites: 0.0,
        blacks: 0.0,
    };
    for pixel in rgb.chunks_exact_mut(3) {
        let out = tonemap::tone([pixel[0], pixel[1], pixel[2]], tone);
        pixel.copy_from_slice(&out);
    }
    spatial::local_contrast(rgb, width, height, 16, f32::from(g.clarity) / 100.0);
    let lift = stats.dark_floor * (f32::from(g.dehaze) / 100.0).clamp(-1.0, 1.0);
    for v in rgb.iter_mut() {
        *v = (*v - lift).max(0.0) / (1.0 - lift).max(1e-3);
    }
}

fn eight(rendered: &RenderedData) -> Vec<u8> {
    match rendered {
        RenderedData::Eight(bytes) => bytes.clone(),
        RenderedData::Sixteen(_) => panic!("asked for eight bits"),
    }
}

fn render(frame: &Frame, recipe: &aura_recipe::Recipe) -> Vec<u8> {
    let engine = engine(frame.clone());
    let out = engine
        .render_frame(
            frame,
            recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("render");
    eight(&out.data)
}

/// The mean absolute log-luminance difference between horizontally adjacent pixels inside
/// the darkest fifth of the frame: how much texture its shadows carry.
fn shadow_texture(rgb: &[u8], reference: &[u8], width: usize) -> f32 {
    let lum = |p: &[u8]| {
        let v = |c: u8| aura_raw::colour::curve::srgb_decode(f32::from(c) / 255.0);
        (0.2126 * v(p[0]) + 0.7152 * v(p[1]) + 0.0722 * v(p[2])).max(1e-4)
    };
    let pixels = reference.len() / 3;
    let mut lums: Vec<f32> = (0..pixels)
        .map(|i| lum(&reference[i * 3..i * 3 + 3]))
        .collect();
    let mut sorted = lums.clone();
    sorted.sort_by(f32::total_cmp);
    let cut = sorted[pixels / 5];
    lums.truncate(pixels);
    let mut sum = 0.0f32;
    let mut count = 0usize;
    for i in 0..pixels - 1 {
        if (i + 1) % width == 0 || lums[i] > cut || lums[i + 1] > cut {
            continue;
        }
        let a = lum(&rgb[i * 3..i * 3 + 3]);
        let b = lum(&rgb[(i + 1) * 3..(i + 1) * 3 + 3]);
        sum += (a.ln() - b.ln()).abs();
        count += 1;
    }
    sum / count.max(1) as f32
}

#[test]
#[ignore = "needs AURA_LOCAL_PHOTOS; writes images for a person to look at"]
fn edge_aware_stages_on_real_photographs() {
    let dir = std::env::var("AURA_LOCAL_PHOTOS").expect("AURA_LOCAL_PHOTOS");
    let variants = std::env::var("AURA_LOCAL_VARIANTS").unwrap_or_else(|_| {
        "local:shadows=70,highlights=-70;legacy:shadows=70,highlights=-70".to_string()
    });
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("folder")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "rgb")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| !s.contains('.'))
        })
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no NAME_WxH.rgb files in {dir}");
    for path in entries {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        let size = stem.rsplit('_').next().unwrap();
        let (w, h) = size.split_once('x').unwrap();
        let (width, height): (usize, usize) = (w.parse().unwrap(), h.parse().unwrap());
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), width * height * 3, "{stem}");
        let frame = Frame::working(working(&bytes), width as u32, height as u32, "photo");
        let r = aura_render::edge_aware::regions(&frame.rgb, width, height, 0.0);
        eprintln!(
            "{stem} regions: low {:.1} median {:.1} high {:.1} spread {:.1} | dark {:.0}% at {:.1} | bright {:.0}% | clipped {:.0}%",
            r.low,
            r.median,
            r.high,
            r.spread(),
            r.dark_share * 100.0,
            r.dark_depth,
            r.bright_share * 100.0,
            r.clipped_share * 100.0
        );
        for (index, variant) in variants.split(';').filter(|v| !v.is_empty()).enumerate() {
            let (label, spec) = variant.split_once(':').unwrap_or((variant, ""));
            let recipe = recipe_of(spec);
            let out = if label.starts_with("legacy") {
                let mut rgb = frame.rgb.clone();
                legacy(&mut rgb, width, height, &recipe);
                let mut rest = recipe.clone();
                rest.global.shadows = 0;
                rest.global.highlights = 0;
                rest.global.clarity = 0;
                rest.global.dehaze = 0;
                render(
                    &Frame::working(rgb, width as u32, height as u32, "photo"),
                    &rest,
                )
            } else {
                let out = render(&frame, &recipe);
                if index == 0 && std::env::var_os("AURA_LOCAL_NO_TILES").is_none() {
                    let engine = engine(frame.clone());
                    let streamed = aura_render::tiles::render_streamed(
                        &engine,
                        &frame,
                        &recipe,
                        RenderLevel::Full,
                        RenderPurpose::Export,
                        &OutputSpec::default(),
                        64 * 1024,
                    )
                    .expect("streamed");
                    assert_eq!(eight(&streamed.data), out, "{stem}: tiled render differs");
                }
                out
            };
            if aura_render::perf::enabled() {
                for (step, ms) in aura_render::perf::take() {
                    eprintln!("  {step}: {ms} ms");
                }
            }
            eprintln!(
                "{stem} {label}: shadow texture {:.4} (original {:.4})",
                shadow_texture(&out, &bytes, width),
                shadow_texture(&bytes, &bytes, width)
            );
            let target = Path::new(&dir).join(format!("{stem}.{label}.rgb"));
            std::fs::write(target, out).unwrap();
        }
    }
}
