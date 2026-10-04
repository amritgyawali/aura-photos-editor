// Tests assert by unwrapping; a panic here is a failed test, never a photographer's crash.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

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
        selection: None,
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

#[test]
fn advanced_masks_protect_excluded_pixels_for_every_tool() {
    use aura_recipe::retouch_tools::{Gradient, LuminanceRange, Selection};
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
        for inverted in [false, true] {
            let mut op = edit(tool);
            op.source = Some([0.1, 0.1]);
            op.selection = Some(Selection {
                inverted,
                gradient: Some(Gradient {
                    start: [0.3, 0.5],
                    end: [0.7, 0.5],
                }),
                luminance: Some(LuminanceRange {
                    low: -1.,
                    high: 1.,
                    softness: 0.3,
                }),
            });
            retouch_tools::validate(&[op.clone()]).unwrap();
            let mask = aura_render::retouch_tools::selection_mask(&frame.rgb, 96, 80, &op);
            assert!(mask.contains(&0.));
            assert!(mask.iter().any(|v| *v > 0.));
            let mut out = frame.rgb.clone();
            aura_render::retouch_tools::apply(&mut out, 96, 80, &[op]);
            for (i, value) in mask.iter().enumerate() {
                if *value == 0. {
                    assert_eq!(
                        &out[i * 3..i * 3 + 3],
                        &frame.rgb[i * 3..i * 3 + 3],
                        "{tool:?} {i}"
                    );
                }
            }
            assert!(out.iter().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn gradient_uses_pixel_aspect_and_inversion_is_complementary() {
    use aura_recipe::retouch_tools::{Gradient, Selection};
    let mut op = edit(Tool::Dodge);
    op.selection = Some(Selection {
        gradient: Some(Gradient {
            start: [0., 0.],
            end: [1., 1.],
        }),
        ..Selection::default()
    });
    let rgb = vec![0.18; 200 * 100 * 3];
    let mask = aura_render::retouch_tools::selection_mask(&rgb, 200, 100, &op);
    let t: f32 = (100.5 * 200. + 0.5 * 100.) / (200. * 200. + 100. * 100.);
    assert!((mask[100] - t * t * (3. - 2. * t)).abs() < 1e-6);
    op.selection.as_mut().unwrap().inverted = true;
    let inverted = aura_render::retouch_tools::selection_mask(&rgb, 200, 100, &op);
    for (a, b) in mask.iter().zip(inverted) {
        assert!((a + b - 1.).abs() < 1e-6);
    }
    op.selection = Some(Selection {
        inverted: true,
        ..Selection::default()
    });
    let outside = aura_render::retouch_tools::selection_mask(&rgb, 200, 100, &op);
    assert_eq!(outside[50 * 200 + 100], 0.);
    assert_eq!(outside[0], 1.);
}

#[test]
fn luminance_ranges_are_linear_stops_with_smooth_falloff_and_black_hdr_support() {
    use aura_recipe::retouch_tools::{LuminanceRange, Selection};
    let mut op = edit(Tool::Dodge);
    op.region = [0.5, 0.5, 1., 1.];
    op.feather = 0.;
    op.selection = Some(Selection {
        luminance: Some(LuminanceRange {
            low: -1.,
            high: 1.,
            softness: 1.,
        }),
        ..Selection::default()
    });
    let rgb: Vec<f32> = [-3., -1.5, 0., 1.5, 3.]
        .into_iter()
        .flat_map(|ev| [0.18 * 2.0_f32.powf(ev); 3])
        .collect();
    let mask = aura_render::retouch_tools::selection_mask(&rgb, 5, 1, &op);
    for (a, b) in mask.iter().zip([0., 0.5, 1., 0.5, 0.]) {
        assert!((a - b).abs() < 1e-5);
    }
    op.selection.as_mut().unwrap().luminance = Some(LuminanceRange {
        low: -16.,
        high: 16.,
        softness: 0.,
    });
    let extreme = [0., 0., 0., 100000., 100000., 100000.];
    assert_eq!(
        aura_render::retouch_tools::selection_mask(&extreme, 2, 1, &op),
        vec![1., 1.]
    );
    let old = edit(Tool::Dodge);
    assert!(serde_json::to_value(old)
        .unwrap()
        .get("selection")
        .is_none());
}

#[test]
fn inverted_painted_masks_include_erased_and_empty_areas() {
    use aura_recipe::retouch_tools::Selection;
    let mut op = edit(Tool::Dodge);
    op.feather = 0.;
    let mut paint = stroke(false, vec![[0.5, 0.5, 1.]]);
    paint.opacity = 1.;
    let mut erase = paint.clone();
    erase.erase = true;
    erase.radius = 0.025;
    op.mask = Some(BrushMask {
        strokes: vec![paint, erase],
    });
    op.selection = Some(Selection {
        inverted: true,
        ..Selection::default()
    });
    let rgb = vec![0.18; 100 * 100 * 3];
    let mask = aura_render::retouch_tools::selection_mask(&rgb, 100, 100, &op);
    assert_eq!(
        mask[50 * 100 + 50],
        1.,
        "erased center is selected after inversion"
    );
    assert_eq!(
        mask[50 * 100 + 55],
        0.,
        "painted ring is protected after inversion"
    );
    assert_eq!(mask[0], 1., "outside painted bounds is selected");
    op.mask = Some(BrushMask { strokes: vec![] });
    assert!(
        aura_render::retouch_tools::selection_mask(&rgb, 100, 100, &op)
            .iter()
            .all(|v| *v == 1.)
    );
}

#[test]
fn invalid_selections_and_unsafe_automatic_sources_are_rejected() {
    use aura_recipe::retouch_tools::{Gradient, LuminanceRange, Selection};
    let mut op = edit(Tool::Dodge);
    op.selection = Some(Selection {
        gradient: Some(Gradient {
            start: [0.5, 0.5],
            end: [0.5, 0.5],
        }),
        ..Selection::default()
    });
    assert!(retouch_tools::validate(&[op.clone()]).is_err());
    op.selection = Some(Selection {
        luminance: Some(LuminanceRange {
            low: 2.,
            high: -2.,
            softness: 0.,
        }),
        ..Selection::default()
    });
    assert!(retouch_tools::validate(&[op.clone()]).is_err());
    for tool in [Tool::Heal, Tool::PatchHeal] {
        op.tool = tool;
        op.selection = Some(Selection {
            inverted: true,
            ..Selection::default()
        });
        assert!(retouch_tools::validate(&[op.clone()]).is_err());
        op.source = Some([0.1, 0.1]);
        assert!(retouch_tools::validate(&[op.clone()]).is_ok());
        op.source = None;
    }
}

#[test]
fn selection_preview_uses_only_preceding_operations_and_never_changes_recipe() {
    use aura_recipe::retouch_tools::{LuminanceRange, Selection};
    let frame = fixtures::grey_frame(80, 80, 0.18);
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame)),
        FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
    );
    let image = aura_core::PhotoId::new();
    let mut recipe =
        aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "Bench-01");
    let mut first = edit(Tool::Dodge);
    first.id = "first".into();
    first.region = [0.5, 0.5, 1., 1.];
    first.feather = 0.;
    first.amount = 1.;
    let mut draft = first.clone();
    draft.id = "second".into();
    draft.selection = Some(Selection {
        luminance: Some(LuminanceRange {
            low: 0.1,
            high: 16.,
            softness: 0.,
        }),
        ..Selection::default()
    });
    let before = engine
        .retouch_selection(&image, &recipe, &draft, None)
        .unwrap();
    retouch_tools::write(&mut recipe, &[first, draft.clone()]).unwrap();
    let snapshot = recipe.clone();
    let at_second = engine
        .retouch_selection(&image, &recipe, &draft, Some("second"))
        .unwrap();
    let at_first = engine
        .retouch_selection(&image, &recipe, &draft, Some("first"))
        .unwrap();
    assert_eq!(before, at_first);
    assert_ne!(at_second.0, at_first.0);
    assert_eq!(snapshot, recipe);
    assert!(engine
        .retouch_selection(&image, &recipe, &draft, Some("missing"))
        .is_err());
    recipe.geometry.crop = [0.25, 0.25, 0.75, 0.75];
    recipe.global.sharpen.amount = 100;
    assert_eq!(
        at_second,
        engine
            .retouch_selection(&image, &recipe, &draft, Some("second"))
            .unwrap()
    );
}
