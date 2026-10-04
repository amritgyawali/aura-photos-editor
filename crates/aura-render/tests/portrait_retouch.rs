//! Portrait masks and portrait operators, executed by the reference renderer on a painted face.
//!
//! The face is `aura_portrait::fixtures`' synthetic portrait, so where its teeth, skin and
//! background are is known by construction. What these tests prove is that the renderer finds
//! them through the parse, changes what an operator names and nothing else, keeps a person's
//! skin tone where it was, and says so when there is no face to work on. They are not evidence
//! about a real photograph; `crates/aura-portrait/tests/local_eval.rs` is the instrument for
//! that.

use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_portrait::fixtures::{portrait, Portrait, PortraitSpec};
use aura_raw::colour::{matrix, working_space};
use aura_recipe::fixtures as recipes;
use aura_recipe::{Mask, MaskKind, MaskParams, Recipe, RetouchOp};
use aura_render::contract::render::{RenderLevel, RenderPurpose, SkipReason};
use aura_render::cpu::{CpuEngine, Frame};
use aura_render::graph::{self, Capabilities, InputKind, Stage};
use aura_render::{fixtures, RenderNote};
use time::OffsetDateTime;

fn caps() -> Capabilities {
    Capabilities {
        mask_generators: true,
        retouch_operators: true,
        geometry_models: true,
        ..Capabilities::default()
    }
}

/// A painted portrait as a working-space frame.
fn frame_of(p: &Portrait) -> Frame {
    let to_working = matrix::mul(working_space::xyz_d65_to_rec2020(), matrix::SRGB_TO_XYZ_D65);
    let linear = p.linear_srgb();
    let mut rgb = Vec::with_capacity(linear.len());
    for px in linear.chunks_exact(3) {
        let out = matrix::apply(
            to_working,
            [f64::from(px[0]), f64::from(px[1]), f64::from(px[2])],
        );
        rgb.extend(out.iter().map(|v| *v as f32));
    }
    Frame::working(rgb, p.width, p.height, "Bench-01")
}

fn render(frame: &Frame, recipe: &Recipe, purpose: RenderPurpose) -> (Vec<f32>, Vec<RenderNote>) {
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame.clone())),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    );
    let plan = graph::plan(recipe, purpose, InputKind::Working, caps());
    let (rgb, _, _, notes) = engine.working_buffer(frame, recipe, &plan, RenderLevel::Full, None);
    (rgb, notes)
}

fn with_op(op: &str, strength: f32) -> Recipe {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.retouch.push(RetouchOp {
        op: op.to_string(),
        strength,
        protect_texture: 0.8,
        mask: None,
        borrowed_from: None,
    });
    recipe
}

fn luma(p: &[f32]) -> f32 {
    0.2627 * p[0] + 0.678 * p[1] + 0.0593 * p[2]
}

fn saturation(p: &[f32]) -> f32 {
    let max = p[0].max(p[1]).max(p[2]);
    let min = p[0].min(p[1]).min(p[2]);
    if max <= 1e-6 {
        0.0
    } else {
        (max - min) / max
    }
}

/// Mean of a per-pixel measure over the pixels a truth plane covers above a half.
fn mean_over(rgb: &[f32], truth: &aura_portrait::Plane, f: impl Fn(&[f32]) -> f32) -> f32 {
    let mut total = 0.0;
    let mut n = 0.0;
    for (i, w) in truth.values.iter().enumerate() {
        if *w >= 0.5 {
            total += f(&rgb[i * 3..i * 3 + 3]);
            n += 1.0;
        }
    }
    assert!(n > 0.0, "the truth plane is empty");
    total / n
}

fn corner_unchanged(before: &[f32], after: &[f32], width: usize) {
    // The top-left corner is ground, far from the head.
    for y in 0..12 {
        for x in 0..12 {
            let i = (y * width + x) * 3;
            for c in 0..3 {
                assert!(
                    (before[i + c] - after[i + c]).abs() < 1e-6,
                    "a ground pixel moved at {x},{y}"
                );
            }
        }
    }
}

#[test]
fn teeth_whitening_whitens_the_teeth_and_touches_nothing_far_away() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let neutral = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    let (before, _) = render(&frame, &neutral, RenderPurpose::Export);
    let (after, notes) = render(&frame, &with_op("teeth_whiten", 1.0), RenderPurpose::Export);
    assert!(
        notes
            .iter()
            .all(|n| n.reason != SkipReason::MaskGeneratorAbsent),
        "{notes:?}"
    );
    let sat_before = mean_over(&before, &p.teeth, saturation);
    let sat_after = mean_over(&after, &p.teeth, saturation);
    assert!(
        sat_after < sat_before * 0.8,
        "teeth saturation {sat_before} -> {sat_after}"
    );
    let l_before = mean_over(&before, &p.teeth, luma);
    let l_after = mean_over(&after, &p.teeth, luma);
    assert!(
        l_after > l_before * 1.05,
        "teeth luminance {l_before} -> {l_after}"
    );
    // Bounded: a whitened tooth is brighter, not paper white.
    assert!(l_after < l_before * 1.3);
    corner_unchanged(&before, &after, p.width as usize);
}

#[test]
fn skin_smoothing_reduces_blotches_and_keeps_the_ground() {
    let mut p = portrait(&PortraitSpec::default());
    // Paint mid-frequency blotches onto the skin: what smoothing exists to reduce.
    for y in 0..p.height as usize {
        for x in 0..p.width as usize {
            let i = y * p.width as usize + x;
            let skin = p.face.values[i] * (1.0 - p.teeth.values[i]) * (1.0 - p.lips.values[i]);
            let blotch = ((x as f32 * 0.55).sin() * (y as f32 * 0.5).cos() * 14.0 * skin) as i32;
            for c in 0..3 {
                let v = i32::from(p.rgb[i * 3 + c]) + blotch;
                p.rgb[i * 3 + c] = v.clamp(0, 255) as u8;
            }
        }
    }
    let frame = frame_of(&p);
    let neutral = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    let (before, _) = render(&frame, &neutral, RenderPurpose::Export);
    let (after, _) = render(&frame, &with_op("skin_smooth", 1.0), RenderPurpose::Export);
    // The cheek: below the left eye, beside the nose.
    let cheek = |rgb: &[f32]| -> f32 {
        let cx = (p.eyes[0][0]) as usize;
        let cy = (p.eyes[0][1] + 22.0) as usize;
        let mut values = Vec::new();
        for y in cy - 6..cy + 6 {
            for x in cx - 6..cx + 6 {
                let i = (y * p.width as usize + x) * 3;
                values.push(luma(&rgb[i..i + 3]).max(1e-5).ln());
            }
        }
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
    };
    let var_before = cheek(&before);
    let var_after = cheek(&after);
    assert!(
        var_after < var_before * 0.7,
        "cheek variance {var_before} -> {var_after}"
    );
    corner_unchanged(&before, &after, p.width as usize);
}

#[test]
fn evening_never_moves_a_persons_skin_tone() {
    for tone in [
        aura_portrait::fixtures::MONK[1],
        aura_portrait::fixtures::MONK[5],
        aura_portrait::fixtures::MONK[9],
    ] {
        let p = portrait(&PortraitSpec {
            skin: tone,
            ..PortraitSpec::default()
        });
        let frame = frame_of(&p);
        let neutral = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
        let (before, _) = render(&frame, &neutral, RenderPurpose::Export);
        let (after, _) = render(&frame, &with_op("skin_even", 1.0), RenderPurpose::Export);
        for channel in 0..3 {
            let ratio =
                |rgb: &[f32]| mean_over(rgb, &p.face, |px| px[channel] / luma(px).max(1e-5));
            let b = ratio(&before);
            let a = ratio(&after);
            assert!(
                (a - b).abs() < 0.02 * b.max(0.05),
                "tone {tone:?} channel {channel}: {b} -> {a}"
            );
        }
        let l_b = mean_over(&before, &p.face, luma);
        let l_a = mean_over(&after, &p.face, luma);
        assert!(
            (l_a - l_b).abs() < 0.01 * l_b.max(1e-3),
            "evening moved brightness {l_b} -> {l_a}"
        );
    }
}

#[test]
fn a_background_mask_darkens_the_ground_and_leaves_the_face() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.masks.push(Mask {
        id: "bg".to_string(),
        kind: MaskKind::Background,
        target: None,
        invert_of: None,
        feather: 0.0,
        params: MaskParams {
            exposure: Some(-1.0),
            ..MaskParams::default()
        },
    });
    let neutral = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    let (before, _) = render(&frame, &neutral, RenderPurpose::Export);
    let (after, notes) = render(&frame, &recipe, RenderPurpose::Export);
    assert!(
        notes
            .iter()
            .all(|n| n.reason != SkipReason::MaskGeneratorAbsent),
        "{notes:?}"
    );
    let corner = |rgb: &[f32]| luma(&rgb[0..3]);
    assert!((corner(&after) / corner(&before) - 0.5).abs() < 0.08);
    let centre = |rgb: &[f32]| {
        let i = ((p.centre[1] as usize) * p.width as usize + p.centre[0] as usize) * 3;
        luma(&rgb[i..i + 3])
    };
    assert!((centre(&after) / centre(&before) - 1.0).abs() < 0.05);
}

#[test]
fn a_region_named_in_the_target_is_the_region_that_moves() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.masks.push(Mask {
        id: "hair".to_string(),
        kind: MaskKind::Subject,
        target: Some("hair".to_string()),
        invert_of: None,
        feather: 0.0,
        params: MaskParams {
            exposure: Some(1.0),
            ..MaskParams::default()
        },
    });
    let neutral = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    let (before, _) = render(&frame, &neutral, RenderPurpose::Export);
    let (after, _) = render(&frame, &recipe, RenderPurpose::Export);
    let hair_gain = mean_over(&after, &p.hair, luma) / mean_over(&before, &p.hair, luma);
    let teeth_gain = mean_over(&after, &p.teeth, luma) / mean_over(&before, &p.teeth, luma);
    assert!(hair_gain > 1.4, "hair gain {hair_gain}");
    assert!((teeth_gain - 1.0).abs() < 0.02, "teeth gain {teeth_gain}");
}

#[test]
fn without_a_face_an_operator_says_so_and_changes_nothing() {
    let frame = Frame::working(vec![0.2; 64 * 48 * 3], 64, 48, "Bench-01");
    let recipe = with_op("teeth_whiten", 1.0);
    let (after, notes) = render(&frame, &recipe, RenderPurpose::Export);
    assert!(after.iter().all(|v| (v - 0.2).abs() < 1e-6));
    assert!(notes
        .iter()
        .any(|n| n.reason == SkipReason::MaskGeneratorAbsent
            && n.detail.as_deref().is_some_and(|d| d.contains("no face"))));
}

#[test]
fn portrait_operators_run_on_the_interactive_path_and_foreign_ones_are_named() {
    let mut recipe = with_op("skin_smooth", 0.5);
    recipe.retouch.push(RetouchOp {
        op: "blemish".to_string(),
        strength: 0.5,
        protect_texture: 0.8,
        mask: Some("skin".to_string()),
        borrowed_from: None,
    });
    let plan = graph::plan(
        &recipe,
        RenderPurpose::Interactive,
        InputKind::Working,
        caps(),
    );
    assert!(plan.stages.contains(&Stage::Retouch));
    assert!(plan
        .notes
        .iter()
        .any(|n| n.detail.as_deref() == Some("blemish")));
    // Without the capability every operator is named as absent, as phase 14 shipped.
    let bare = graph::plan(
        &recipe,
        RenderPurpose::Export,
        InputKind::Working,
        Capabilities::default(),
    );
    assert!(!bare.stages.contains(&Stage::Retouch));
    assert!(bare
        .notes
        .iter()
        .any(|n| n.reason == SkipReason::OperatorAbsent));
}

#[test]
fn a_hint_mask_runs_nothing_by_itself() {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.masks.push(Mask {
        id: "hint1".to_string(),
        kind: MaskKind::Face,
        target: Some("hint:0.2000,0.2000,0.6000,0.6800".to_string()),
        invert_of: None,
        feather: 0.0,
        params: MaskParams::default(),
    });
    let plan = graph::plan(&recipe, RenderPurpose::Export, InputKind::Working, caps());
    assert!(!plan.stages.contains(&Stage::Masks));
    assert!(!aura_render::portrait::wants_parse(&recipe));
}

#[test]
fn the_same_inputs_render_the_same_pixels() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let mut recipe = with_op("skin_smooth", 0.6);
    recipe.retouch.push(RetouchOp {
        op: "background_blur".to_string(),
        strength: 0.7,
        protect_texture: 0.0,
        mask: None,
        borrowed_from: None,
    });
    let (a, _) = render(&frame, &recipe, RenderPurpose::Export);
    let (b, _) = render(&frame, &recipe, RenderPurpose::Export);
    assert_eq!(a, b);
}

#[test]
fn every_operator_runs_and_stays_finite() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    for op in aura_render::portrait::OPERATORS {
        let (after, _) = render(&frame, &with_op(op, 1.0), RenderPurpose::Export);
        assert!(
            after.iter().all(|v| v.is_finite() && *v >= 0.0),
            "{op} produced a bad pixel"
        );
    }
}
