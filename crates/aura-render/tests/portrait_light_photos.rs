//! Real-photograph check of portrait light (ADR-0127), run by hand.
//!
//! `AURA_LIGHT_PHOTOS` names a folder of `NAME_WxH.rgb` files (packed 8-bit sRGB).
//! `AURA_LIGHT_VARIANTS` lists what to render, separated by semicolons: `auto` renders what Auto
//! light proposes, and `LABEL:key=..,direction=..,height=..,fill=..,catchlights=..,glow=..` renders
//! those values. Each result is written beside its source as `NAME_WxH.LABEL.rgb`, and the
//! proposal and its reasons are printed. The images are for a person to look at.
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
use aura_recipe::studio_finish::{self, Light, StudioFinish};
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

fn light_of(spec: &str) -> Light {
    let mut light = Light::default();
    for pair in spec.split(',').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').expect("key=value");
        let n: f32 = value.parse().expect("number");
        match key {
            "key" => light.key = n,
            "direction" => light.direction = n,
            "height" => light.height = n,
            "fill" => light.fill = n,
            "catchlights" => light.catchlights = n,
            "glow" => light.glow = n,
            other => panic!("unknown key {other}"),
        }
    }
    light
}

fn render(frame: &Frame, light: Light) -> Vec<u8> {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "photo");
    studio_finish::write(
        &mut recipe,
        &StudioFinish {
            light,
            ..StudioFinish::default()
        },
    )
    .unwrap();
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame.clone())),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    );
    let out = engine
        .render_frame(
            frame,
            &recipe,
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

#[test]
#[ignore = "needs AURA_LIGHT_PHOTOS; writes images for a person to look at"]
fn portrait_light_on_real_photographs() {
    let dir = std::env::var("AURA_LIGHT_PHOTOS").expect("AURA_LIGHT_PHOTOS");
    let variants = std::env::var("AURA_LIGHT_VARIANTS").unwrap_or_else(|_| "auto".to_string());
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
        let (width, height): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
        let bytes = std::fs::read(&path).unwrap();
        let frame = Frame::working(working(&bytes), width, height, "photo");
        let map = aura_render::portrait::parse(&frame, &[]);
        let suggestion = map
            .as_deref()
            .map(|m| aura_render::portrait_light::suggest(&frame.rgb, width, height, m));
        eprintln!(
            "\n{stem}: {} face(s)",
            map.as_ref().map_or(0, |m| m.faces.len())
        );
        if let Some(m) = map.as_deref() {
            for (i, eyes) in aura_render::portrait_light::read_eyes(&frame.rgb, width, height, m)
                .iter()
                .enumerate()
            {
                for e in eyes {
                    eprintln!(
                        "  face {i} iris at ({:.0}, {:.0}) r {:.1} catchlight {}",
                        e.centre[0], e.centre[1], e.radius, e.has_catchlight
                    );
                }
            }
        }
        if let Some(s) = &suggestion {
            eprintln!("  proposal {:?}", s.light);
            for r in &s.reasons {
                eprintln!("  - {r}");
            }
        }
        for variant in variants.split(';').filter(|v| !v.is_empty()) {
            let (label, light) = if variant == "auto" {
                (
                    "auto".to_string(),
                    suggestion.as_ref().map_or_else(Light::default, |s| s.light),
                )
            } else {
                let (label, spec) = variant.split_once(':').unwrap_or((variant, ""));
                (label.to_string(), light_of(spec))
            };
            let out = render(&frame, light);
            std::fs::write(Path::new(&dir).join(format!("{stem}.{label}.rgb")), out).unwrap();
        }
    }
}
