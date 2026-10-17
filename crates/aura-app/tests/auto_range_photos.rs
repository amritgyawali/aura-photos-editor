//! Real-photograph check of Auto enhance's light step with Shadows and Highlights sized from
//! the frame's regions, run by hand. ADR-0126.
//!
//! `AURA_RANGE_PHOTOS` names a folder of `NAME_WxH.rgb` files (packed 8-bit sRGB). For each,
//! the light half of Auto enhance is planned the way the app plans it - the histogram's
//! correction, the scene's intent, then [`aura_app::smart_edit::analyse`] - and rendered
//! twice: once with the histogram's Shadows and Highlights alone, once with the regions'
//! answer merged in. `NAME.range.png` is the original, the histogram-only render and the
//! region render side by side; the decisions are printed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use std::sync::Arc;

use aura_app::portrait_features::Pixels;
use aura_app::smart_edit;
use aura_core::clock::FixedClock;
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose, RenderedData};
use aura_render::cpu::CpuEngine;
use aura_render::{fixtures, Frame};
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

fn render(frame: &Frame, recipe: &aura_recipe::Recipe) -> Vec<u8> {
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame.clone())),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    );
    let out = engine
        .render_frame(
            frame,
            recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("render");
    match out.data {
        RenderedData::Eight(bytes) => bytes,
        RenderedData::Sixteen(_) => panic!("asked for eight bits"),
    }
}

fn write_png(path: &std::path::Path, width: usize, height: usize, rgb: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgb)
        .unwrap();
}

#[test]
#[ignore = "needs AURA_RANGE_PHOTOS; writes images for a person to look at"]
fn auto_light_with_region_sized_shadows_and_highlights() {
    let dir = std::env::var("AURA_RANGE_PHOTOS").expect("AURA_RANGE_PHOTOS");
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
    for path in entries {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        let (w, h) = stem.rsplit('_').next().unwrap().split_once('x').unwrap();
        let (width, height): (usize, usize) = (w.parse().unwrap(), h.parse().unwrap());
        let bytes = std::fs::read(&path).unwrap();
        let px = Pixels::new(&bytes, width as u32, height as u32).expect("pixels");
        let faces =
            aura_vision::portrait::detect(&bytes, width as u32, height as u32).unwrap_or_default();
        let mut tone = aura_app::photo_enhance::correction(&bytes).unwrap();
        let note = smart_edit::respect_intent(&mut tone, &px, &faces);
        let frame = Frame::working(working(&bytes), width as u32, height as u32, "photo");
        let plan = smart_edit::analyse(&px, Some(&frame), &faces, tone.0);

        let recipe_with = |highlights: i16, shadows: i16| {
            let mut recipe =
                aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "photo");
            let g = &mut recipe.global;
            g.exposure = tone.0;
            g.highlights = highlights;
            g.shadows = shadows;
            g.contrast = tone.3;
            g.whites = plan.whites;
            g.blacks = plan.blacks;
            g.vibrance = plan.vibrance;
            g.clarity = plan.clarity;
            g.dehaze = plan.dehaze;
            if let Some((kelvin, tint)) = plan.white_balance {
                g.temperature = kelvin;
                g.tint = tint;
            }
            recipe
        };
        let histogram = (tone.1, (tone.2 + plan.extra_shadows).clamp(-100, 100));
        let regions = plan.highlights_shadows(tone);
        let before = render(&frame, &recipe_with(histogram.0, histogram.1));
        let after = render(&frame, &recipe_with(regions.0, regions.1));
        eprintln!(
            "\n{stem}: {} | exposure {:+.2} | histogram H{:+} S{:+} -> regions H{:+} S{:+}",
            plan.kind.label(),
            tone.0,
            histogram.0,
            histogram.1,
            regions.0,
            regions.1
        );
        if let Some(note) = note {
            eprintln!("  {note}");
        }
        for decision in &plan.decisions {
            eprintln!("  {decision}");
        }
        let mut sheet = vec![0u8; width * 3 * height * 3];
        for y in 0..height {
            for (k, image) in [&bytes, &before, &after].into_iter().enumerate() {
                let src = &image[y * width * 3..(y + 1) * width * 3];
                let start = (y * width * 3 + k * width) * 3;
                sheet[start..start + width * 3].copy_from_slice(src);
            }
        }
        write_png(
            &std::path::Path::new(&dir).join(format!("{stem}.range.png")),
            width * 3,
            height,
            &sheet,
        );
    }
}
