use aura_core::clock::FixedClock;
use aura_recipe::retouch_tools::{self, Edit, Tool};
use aura_render::{fixtures, CpuEngine, OutputSpec, RenderLevel, RenderPurpose};
use std::sync::Arc;

fn edit(tool: Tool) -> Edit {
    Edit {
        id: "test".into(),
        tool,
        enabled: true,
        region: [0.5, 0.5, 0.3, 0.3],
        source: None,
        amount: 0.7,
        feather: 0.5,
        radius: 0.01,
        texture: 1.0,
        tone: 0.5,
        warmth: 0.3,
        tint: -0.2,
    }
}

#[test]
fn every_tool_preserves_pixels_outside_the_selected_region() {
    let frame = fixtures::detail_frame(96, 80);
    for tool in [
        Tool::Heal,
        Tool::Clone,
        Tool::AutoBlemish,
        Tool::Frequency,
        Tool::MicroDodgeBurn,
        Tool::Dodge,
        Tool::Burn,
        Tool::SkinColor,
        Tool::ColorMatch,
        Tool::Mattify,
        Tool::UnderEye,
        Tool::Wrinkle,
        Tool::Teeth,
        Tool::EyeClean,
        Tool::EyeDetail,
        Tool::RedEye,
        Tool::Fabric,
        Tool::Backdrop,
        Tool::Glare,
        Tool::Makeup,
    ] {
        let mut operation = edit(tool);
        operation.source = Some([0.15, 0.15]);
        let mut pixels = frame.rgb.clone();
        aura_render::retouch_tools::apply(&mut pixels, 96, 80, &[operation]);
        assert!(pixels.iter().all(|v| v.is_finite()), "{tool:?}");
        for y in 0..80 {
            for x in 0..96 {
                let dx = ((x as f32 + 0.5) / 96.0 - 0.5) / 0.3;
                let dy = ((y as f32 + 0.5) / 80.0 - 0.5) / 0.3;
                if dx.hypot(dy) >= 1.0 {
                    let i = (y * 96 + x) * 3;
                    assert_eq!(
                        &pixels[i..i + 3],
                        &frame.rgb[i..i + 3],
                        "{tool:?} at {x},{y}"
                    );
                }
            }
        }
    }
}

#[test]
fn neutral_frequency_and_disabled_operations_are_identity() {
    let frame = fixtures::detail_frame(80, 80);
    let mut pixels = frame.rgb.clone();
    let mut op = edit(Tool::Frequency);
    op.tone = 0.0;
    op.texture = 1.0;
    aura_render::retouch_tools::apply(&mut pixels, 80, 80, &[op.clone()]);
    assert_eq!(pixels, frame.rgb);
    op.tool = Tool::Dodge;
    op.enabled = false;
    aura_render::retouch_tools::apply(&mut pixels, 80, 80, &[op.clone()]);
    assert_eq!(pixels, frame.rgb);
    op.enabled = true;
    op.amount = 0.0;
    aura_render::retouch_tools::apply(&mut pixels, 80, 80, &[op]);
    assert_eq!(pixels, frame.rgb);
}

#[test]
fn healing_and_automatic_cleanup_remove_an_isolated_spot() {
    let mut original = vec![0.3; 96 * 96 * 3];
    let center = (48 * 96 + 48) * 3;
    original[center..center + 3].fill(0.02);
    for tool in [Tool::Heal, Tool::AutoBlemish] {
        let mut op = edit(tool);
        op.amount = 1.0;
        if tool == Tool::Heal {
            op.region = [48.5 / 96.0, 48.5 / 96.0, 0.02, 0.02];
        }
        let mut pixels = original.clone();
        aura_render::retouch_tools::apply(&mut pixels, 96, 96, &[op]);
        assert!(
            pixels[center] > 0.2,
            "{tool:?} did not remove spot: {}",
            pixels[center]
        );
    }
}

#[test]
fn clone_uses_frozen_source_pixels_even_when_regions_overlap() {
    let frame = fixtures::ramp_frame(100, 100);
    let mut rgb = frame.rgb.clone();
    let mut op = edit(Tool::Clone);
    op.source = Some([0.45, 0.5]);
    op.amount = 1.0;
    op.feather = 0.0;
    aura_render::retouch_tools::apply(&mut rgb, 100, 100, &[op]);
    let i = (50 * 100 + 50) * 3;
    let source = (50 * 100 + 45) * 3;
    for c in 0..3 {
        assert!((rgb[i + c] - frame.rgb[source + c]).abs() < 0.015);
    }
}

#[test]
fn operations_reach_both_preview_and_export_and_survive_crop() {
    let frame = fixtures::grey_frame(80, 80, 0.2);
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame.clone())),
        FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
    );
    let mut recipe =
        aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "Bench-01");
    let original = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .unwrap();
    retouch_tools::write(&mut recipe, &[edit(Tool::Dodge)]).unwrap();
    let export = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .unwrap();
    let preview = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .unwrap();
    assert_ne!(original.data, export.data);
    assert_eq!(preview.data, export.data);
    assert!(export.stages_run.contains(&"studio_retouch".into()));
    let streamed = aura_render::tiles::render_streamed(
        &engine,
        &frame,
        &recipe,
        RenderLevel::Full,
        RenderPurpose::Export,
        &OutputSpec::default(),
        4096,
    )
    .unwrap();
    assert_eq!(export.data, streamed.data);
    // AURA crops use left/top/right/bottom bounds, not x/y/width/height.
    recipe.geometry.crop = [0.25, 0.25, 0.75, 0.75];
    let cropped = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .unwrap();
    assert_eq!((cropped.width, cropped.height), (40, 40));
    if let (aura_render::RenderedData::Eight(all), aura_render::RenderedData::Eight(crop)) =
        (export.data, cropped.data)
    {
        // Dither position changes after crop; compare center within one quantization step.
        for c in 0..3 {
            assert!(all[(40 * 80 + 40) * 3 + c].abs_diff(crop[(20 * 40 + 20) * 3 + c]) <= 1);
        }
    } else {
        panic!("Expected 8-bit output");
    }
}

#[test]
fn bad_parameters_are_rejected_and_manual_edits_survive_automation() {
    let mut base = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "Bench-01");
    let mut op = edit(Tool::Clone);
    assert!(retouch_tools::validate(&[op.clone()]).is_err());
    op.source = Some([0.1, 0.1]);
    op.amount = f32::NAN;
    assert!(retouch_tools::validate(&[op]).is_err());
    let mut proposal = base.clone();
    retouch_tools::write(&mut proposal, &[edit(Tool::Dodge)]).unwrap();
    let (manual, _) =
        aura_recipe::schema::merge(&base, &proposal, aura_recipe::EditSource::User).unwrap();
    base.global.exposure = 0.2;
    let (automatic, _) =
        aura_recipe::schema::merge(&manual, &base, aura_recipe::EditSource::Ai).unwrap();
    assert_eq!(
        retouch_tools::read(&manual).unwrap(),
        retouch_tools::read(&automatic).unwrap()
    );
    let encoded = serde_json::to_string(&manual).unwrap();
    let decoded: aura_recipe::Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        retouch_tools::read(&decoded).unwrap(),
        retouch_tools::read(&manual).unwrap()
    );
}
