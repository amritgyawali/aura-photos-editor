//! Retouch checkpoints change how much work a render does, never what it produces. ADR-0098.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use aura_core::clock::FixedClock;
use aura_recipe::retouch_tools::{self, Edit, Tool};
use aura_render::retouch_cache::Checkpoints;
use aura_render::{
    fixtures, CpuEngine, OutputSpec, RenderLevel, RenderPurpose, RenderRequest, RenderService,
};
use std::sync::Arc;

fn edit(id: &str, tool: Tool, region: [f32; 4]) -> Edit {
    Edit {
        id: id.into(),
        tool,
        enabled: true,
        region,
        source: None,
        amount: 0.7,
        feather: 0.5,
        radius: 0.01,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        sensitivity: None,
        keep_dark_marks: false,
        texture: 1.0,
        tone: 0.5,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        matte: None,
        mask: None,
        skin: None,
    }
}

fn engines() -> (CpuEngine, CpuEngine) {
    let frame = fixtures::detail_frame(120, 90);
    let source = Arc::new(fixtures::StaticSource::new(frame));
    let clock = FixedClock::at(time::OffsetDateTime::UNIX_EPOCH);
    (
        CpuEngine::new(source.clone(), clock.clone()),
        CpuEngine::new(source, clock).with_checkpoints(Checkpoints::new()),
    )
}

fn render(
    engine: &CpuEngine,
    image: aura_core::PhotoId,
    edits: &[Edit],
    level: RenderLevel,
) -> Vec<u8> {
    let mut recipe =
        aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "Bench-01");
    recipe.global.exposure = 0.3;
    recipe.global.sharpen.amount = 40;
    retouch_tools::write(&mut recipe, edits).unwrap();
    let image = engine
        .render(RenderRequest {
            image_id: image,
            recipe,
            level,
            output: OutputSpec::default(),
            purpose: RenderPurpose::Interactive,
        })
        .unwrap();
    match image.data {
        aura_render::RenderedData::Eight(bytes) => bytes,
        aura_render::RenderedData::Sixteen(words) => words.iter().map(|w| (w >> 8) as u8).collect(),
    }
}

#[test]
fn every_way_a_render_can_start_gives_the_same_pixels() {
    let (plain, cached) = engines();
    let image = aura_core::PhotoId::new();
    let mut acne = edit("acne", Tool::AcneClear, [0.5, 0.5, 0.3, 0.3]);
    acne.radius = 0.02;
    let mut graft = edit("graft", Tool::TextureGraft, [0.5, 0.5, 0.35, 0.35]);
    graft.preserve_microtexture = true;
    let stack = vec![
        edit("smooth", Tool::Frequency, [0.4, 0.5, 0.3, 0.3]),
        acne,
        graft,
        edit("light", Tool::MicroDodgeBurn, [0.6, 0.4, 0.3, 0.3]),
    ];
    let mut longer = stack.clone();
    longer.push(edit("dodge", Tool::Dodge, [0.3, 0.3, 0.1, 0.1]));
    // The same last operation under another name: the draft becoming the saved operation.
    let mut renamed = longer.clone();
    if let Some(last) = renamed.last_mut() {
        last.id = "saved".into();
    }
    let mut changed_middle = longer.clone();
    changed_middle[0].amount = 0.4;
    // A texture restore that reads the skin acne clear left, after the checkpoint holds the
    // acne clear: the render must start from before the stack.
    let mut regrafted = longer.clone();
    regrafted.push({
        let mut graft = edit("graft-2", Tool::TextureGraft, [0.5, 0.5, 0.3, 0.3]);
        graft.preserve_microtexture = true;
        graft
    });
    for level in [RenderLevel::Full, RenderLevel::Screen(60, 60)] {
        for edits in [
            &stack,
            &longer,
            &renamed,
            &Vec::new(),
            &changed_middle,
            &regrafted,
            &stack,
        ] {
            assert_eq!(
                render(&cached, image, edits, level),
                render(&plain, image, edits, level),
                "{} operations at {level:?}",
                edits.len()
            );
        }
    }
}

#[test]
fn a_live_preview_carries_the_last_retouch_over_and_is_close_to_the_exact_render() {
    let (plain, cached) = engines();
    let image = aura_core::PhotoId::new();
    let stack = vec![
        edit("smooth", Tool::Frequency, [0.4, 0.5, 0.3, 0.3]),
        edit("light", Tool::MicroDodgeBurn, [0.6, 0.4, 0.3, 0.3]),
        edit("dodge", Tool::Dodge, [0.3, 0.3, 0.1, 0.1]),
    ];
    let request = |exposure: f32, edits: &[Edit]| {
        let mut recipe =
            aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "Bench-01");
        recipe.global.exposure = exposure;
        retouch_tools::write(&mut recipe, edits).unwrap();
        RenderRequest {
            image_id: image,
            recipe,
            level: RenderLevel::Full,
            output: OutputSpec::default(),
            purpose: RenderPurpose::Interactive,
        }
    };
    // Nothing rendered yet: there is nothing to carry over.
    assert!(cached.render_live(&request(0.3, &stack)).unwrap().is_none());
    cached.render(request(0.3, &stack)).unwrap();
    // A different stack is not carried over.
    assert!(cached
        .render_live(&request(0.6, &stack[..2]))
        .unwrap()
        .is_none());
    let live = cached.render_live(&request(0.6, &stack)).unwrap().unwrap();
    let exact = plain.render(request(0.6, &stack)).unwrap();
    assert_eq!((live.width, live.height), (exact.width, exact.height));
    let (aura_render::RenderedData::Eight(a), aura_render::RenderedData::Eight(b)) =
        (&live.data, &exact.data)
    else {
        panic!("eight-bit output");
    };
    let mean = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(x.abs_diff(*y)))
        .sum::<f64>()
        / a.len() as f64;
    // An exposure change scales the frame before the stack, so the carried-over effect is
    // nearly exact.
    assert!(mean < 1.5, "mean difference {mean}");
}
