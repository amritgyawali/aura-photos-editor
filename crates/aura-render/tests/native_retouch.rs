use aura_core::clock::FixedClock;
use aura_recipe::retouch_tools::{self, Edit, Tool};
use aura_recipe::retouch_tools::{BrushMask, BrushStroke};
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
        mask: None,
        skin: None,
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
        Tool::SkinSmooth,
        Tool::SkinUniformity,
        Tool::PortraitDodgeBurn,
        Tool::PatchHeal,
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

fn stroke(erase: bool, points: Vec<[f32; 3]>) -> BrushStroke {
    BrushStroke {
        erase,
        radius: 0.1,
        opacity: 0.5,
        points,
    }
}

#[test]
fn painted_masks_are_continuous_and_independent_of_pointer_event_density() {
    let frame = fixtures::grey_frame(100, 100, 0.2);
    let mut op = edit(Tool::Dodge);
    op.amount = 1.0;
    op.feather = 0.0;
    op.mask = Some(BrushMask {
        strokes: vec![stroke(false, vec![[0.2, 0.5, 1.], [0.8, 0.5, 1.]])],
    });
    let mut sparse = frame.rgb.clone();
    aura_render::retouch_tools::apply(&mut sparse, 100, 100, &[op.clone()]);
    let Some(mask) = op.mask.as_mut() else {
        panic!("mask fixture");
    };
    mask.strokes[0].points = (0..=60).map(|i| [0.2 + i as f32 * 0.01, 0.5, 1.]).collect();
    let mut dense = frame.rgb.clone();
    aura_render::retouch_tools::apply(&mut dense, 100, 100, &[op]);
    assert_eq!(sparse, dense);
    for x in 20..80 {
        assert!(sparse[(50 * 100 + x) * 3] > 0.2);
    }
    assert_eq!(sparse[(30 * 100 + 50) * 3], 0.2);
}

#[test]
fn every_tool_respects_painted_and_erased_pixels() {
    let frame = fixtures::detail_frame(100, 100);
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
        Tool::SkinSmooth,
        Tool::SkinUniformity,
        Tool::PortraitDodgeBurn,
        Tool::PatchHeal,
    ] {
        let mut op = edit(tool);
        op.source = Some([0.1, 0.1]);
        op.feather = 0.;
        let mut erase = stroke(true, vec![[0.5, 0.5, 1.]]);
        erase.radius = 0.04;
        erase.opacity = 1.;
        op.mask = Some(BrushMask {
            strokes: vec![stroke(false, vec![[0.2, 0.5, 1.], [0.8, 0.5, 1.]]), erase],
        });
        let mut pixels = frame.rgb.clone();
        aura_render::retouch_tools::apply(&mut pixels, 100, 100, &[op]);
        assert!(pixels.iter().all(|v| v.is_finite()));
        for y in 0..100 {
            for x in 0..100 {
                let outside = !(40..60).contains(&y) || !(10..90).contains(&x);
                let erased = (x as f32 + 0.5 - 50.).hypot(y as f32 + 0.5 - 50.) < 4.;
                if outside || erased {
                    let i = (y * 100 + x) * 3;
                    assert_eq!(&pixels[i..i + 3], &frame.rgb[i..i + 3], "{tool:?} {x},{y}");
                }
            }
        }
    }
}

#[test]
fn pressure_changes_brush_width_and_empty_masks_are_neutral() {
    let frame = fixtures::grey_frame(100, 100, 0.2);
    let mut op = edit(Tool::Dodge);
    op.feather = 0.;
    op.mask = Some(BrushMask {
        strokes: vec![
            stroke(false, vec![[0.25, 0.5, 0.2]]),
            stroke(false, vec![[0.75, 0.5, 1.]]),
        ],
    });
    let mut pixels = frame.rgb.clone();
    aura_render::retouch_tools::apply(&mut pixels, 100, 100, &[op.clone()]);
    assert_eq!(pixels[(55 * 100 + 25) * 3], 0.2);
    assert!(pixels[(55 * 100 + 75) * 3] > 0.2);
    op.mask = Some(BrushMask { strokes: vec![] });
    let mut empty = frame.rgb.clone();
    aura_render::retouch_tools::apply(&mut empty, 100, 100, &[op]);
    assert_eq!(empty, frame.rgb);
}

#[test]
fn old_recipes_keep_their_shape_and_invalid_masks_are_rejected() {
    let op = edit(Tool::Dodge);
    let json = serde_json::to_value(&op).unwrap();
    assert!(json.get("mask").is_none());
    assert!(json.get("skin").is_none());
    assert_eq!(serde_json::from_value::<Edit>(json).unwrap(), op);
    let mut invalid = op;
    invalid.mask = Some(BrushMask {
        strokes: vec![stroke(false, vec![[0.5, 0.5, f32::NAN]])],
    });
    assert!(retouch_tools::validate(&[invalid.clone()]).is_err());
    invalid.mask = Some(BrushMask {
        strokes: vec![stroke(
            false,
            vec![[0.5, 0.5, 1.]; retouch_tools::MAX_POINTS + 1],
        )],
    });
    assert!(retouch_tools::validate(&[invalid]).is_err());
}

#[test]
fn automatic_spot_cleanup_works_with_low_opacity_brushes() {
    let mut pixels = vec![0.45; 96 * 96 * 3];
    let center = (48 * 96 + 48) * 3;
    pixels[center..center + 3].fill(0.02);
    let mut op = edit(Tool::AutoBlemish);
    let mut brush = stroke(false, vec![[0.5, 0.5, 1.0]]);
    brush.opacity = 0.3;
    op.mask = Some(BrushMask {
        strokes: vec![brush],
    });
    aura_render::retouch_tools::apply(&mut pixels, 96, 96, &[op]);
    assert!(pixels[center] > 0.04, "Low opacity disabled spot detection");
    assert!(
        pixels[center] < 0.3,
        "Opacity was not applied to the repair"
    );
}
