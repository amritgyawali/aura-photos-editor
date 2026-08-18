//! The property section 6.3 promises: a tiled render is identical to a whole one.
//!
//! Not close. Identical. A photographer exporting a 24 MP file and a 100 MP file from the
//! same recipe must not get two different looks because one of them crossed a memory
//! threshold - and the threshold is invisible from the outside, which is what makes a "close
//! enough" tiler a defect nobody can report.

use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_recipe::fixtures as recipes;
use aura_render::contract::render::{OutputColour, OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::CpuEngine;
use aura_render::{fixtures, tiles};
use time::OffsetDateTime;

fn engine(frame: aura_render::Frame) -> CpuEngine {
    CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame)),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    )
}

#[test]
fn tiled_render_equals_whole_render() {
    // Every spatial stage at once, on a frame with detail at three scales, at a size that
    // produces a grid rather than a single tile.
    let frame = fixtures::detail_frame(700, 540);
    let engine = engine(frame.clone());

    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.global.exposure = 0.35;
    recipe.global.contrast = 18;
    recipe.global.shadows = 30;
    recipe.global.highlights = -25;
    recipe.global.clarity = 45;
    recipe.global.texture = 30;
    recipe.global.vibrance = 20;
    recipe.global.sharpen.amount = 80;
    recipe.global.sharpen.masking = 30;
    recipe.global.noise.luminance = 25;
    recipe.global.noise.colour = 35;
    recipe.lens.vignette = 40;

    let whole = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("whole");

    let streamed = tiles::render_streamed(
        &engine,
        &frame,
        &recipe,
        RenderLevel::Full,
        RenderPurpose::Export,
        &OutputSpec::default(),
        4096,
    )
    .expect("streamed");

    assert_eq!(
        (whole.width, whole.height),
        (streamed.width, streamed.height)
    );
    assert_eq!(
        whole.data, streamed.data,
        "a tiled render must be bit-identical to a whole one"
    );
    assert_eq!(whole.render_hash, streamed.render_hash);
}

#[test]
fn tiled_render_equals_whole_render_in_every_output_space() {
    let frame = fixtures::detail_frame(600, 300);
    let engine = engine(frame.clone());
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-03");
    recipe.global.clarity = 35;
    recipe.global.sharpen.amount = 60;

    for space in [
        OutputColour::Srgb,
        OutputColour::AdobeRgb,
        OutputColour::DisplayP3,
    ] {
        for bit_depth in [8u8, 16] {
            let spec = OutputSpec {
                colour_space: space,
                bit_depth,
                icc: None,
            };
            let whole = engine
                .render_frame(
                    &frame,
                    &recipe,
                    RenderLevel::Full,
                    RenderPurpose::Export,
                    &spec,
                )
                .expect("whole");
            let streamed = tiles::render_streamed(
                &engine,
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &spec,
                4096,
            )
            .expect("streamed");
            assert_eq!(whole.data, streamed.data, "{space:?} at {bit_depth} bits");
        }
    }
}

#[test]
fn a_hundred_megapixel_export_is_streamed_rather_than_held_whole() {
    // The budget arithmetic, not the render: allocating 100 MP of `f32` in a test would be
    // the very thing the streaming exists to avoid.
    let hundred_mp = tiles::working_bytes_for(11_648, 8_736);
    assert!(hundred_mp > aura_render::cpu::DEFAULT_WORKING_BYTES);
    assert!(
        tiles::working_bytes_for(8_192, 5_464) < aura_render::cpu::DEFAULT_WORKING_BYTES,
        "a 45 MP frame still fits whole"
    );
    assert_eq!(tiles::tile_count(11_648, 8_736, tiles::TILE_SIZE), 23 * 18);
}

#[test]
fn the_halo_is_large_enough_for_every_enabled_stage() {
    use aura_render::graph::{self, Capabilities, InputKind};

    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.global.clarity = 40;
    recipe.global.texture = 20;
    recipe.global.sharpen.amount = 50;
    recipe.global.noise.luminance = 20;
    recipe.global.dehaze = 30;

    let plan = graph::plan(
        &recipe,
        RenderPurpose::Export,
        InputKind::Working,
        Capabilities::default(),
    );
    let expected: u32 = plan.stages.iter().map(|s| s.halo()).sum();
    assert_eq!(tiles::halo_for(&plan), expected);
    assert!(expected >= 48 + 12 + 12 + 16 + 24);
}
