//! The Studio's finishing tools (ADR-0108), executed by the reference renderer on a painted face.
//!
//! The face is `aura_portrait::fixtures`' synthetic portrait, so where its eyes, hair and ground
//! are is known by construction. These prove the renderer finds them through the parse and moves
//! or recolours what a control names; they are not evidence about a real photograph.

#![allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_portrait::fixtures::{portrait, Portrait, PortraitSpec};
use aura_raw::colour::{matrix, working_space};
use aura_recipe::fixtures as recipes;
use aura_recipe::studio_finish::{self, Background, BackgroundMode, StudioFinish, Tint};
use aura_recipe::Recipe;
use aura_render::contract::render::{RenderLevel, RenderPurpose};
use aura_render::cpu::{CpuEngine, Frame};
use aura_render::fixtures;
use aura_render::graph::{self, Capabilities, InputKind};
use time::OffsetDateTime;

fn frame_of(p: &Portrait) -> Frame {
    let to_working = matrix::mul(working_space::xyz_d65_to_rec2020(), matrix::SRGB_TO_XYZ_D65);
    let mut rgb = Vec::new();
    for px in p.linear_srgb().chunks_exact(3) {
        let out = matrix::apply(
            to_working,
            [f64::from(px[0]), f64::from(px[1]), f64::from(px[2])],
        );
        rgb.extend(out.iter().map(|v| *v as f32));
    }
    Frame::working(rgb, p.width, p.height, "Bench-01")
}

fn render(frame: &Frame, recipe: &Recipe) -> Vec<f32> {
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame.clone())),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    );
    let caps = Capabilities {
        mask_generators: true,
        retouch_operators: true,
        ..Capabilities::default()
    };
    let plan = graph::plan(recipe, RenderPurpose::Export, InputKind::Working, caps);
    engine
        .working_buffer(frame, recipe, &plan, RenderLevel::Full, None)
        .0
}

fn finished(finish: &StudioFinish) -> Recipe {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    studio_finish::write(&mut recipe, finish).unwrap();
    recipe
}

fn mean_over(rgb: &[f32], truth: &aura_portrait::Plane, c: usize) -> f32 {
    let (mut s, mut n) = (0.0, 0.0);
    for (i, w) in truth.values.iter().enumerate() {
        if *w >= 0.5 {
            s += rgb[i * 3 + c];
            n += 1.0;
        }
    }
    assert!(n > 0.0);
    s / n
}

fn changed(a: &[f32], b: &[f32], i: usize) -> f32 {
    (0..3)
        .map(|c| (a[i * 3 + c] - b[i * 3 + c]).abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_neutral_finish_renders_the_same_photograph() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let neutral = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    assert_eq!(
        render(&frame, &neutral),
        render(&frame, &finished(&StudioFinish::default()))
    );
}

#[test]
fn bigger_eyes_move_pixels_around_the_eyes_and_not_in_the_corner() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let before = render(&frame, &finished(&StudioFinish::default()));
    let mut finish = StudioFinish::default();
    finish.face.eye_size = 100.0;
    let after = render(&frame, &finished(&finish));
    let w = p.width as usize;
    let near: f32 = p
        .eyes
        .iter()
        .map(|e| {
            // The eye's rim, where an enlargement moves the most.
            let (x, y) = ((e[0] + 6.0) as usize, e[1] as usize);
            changed(&before, &after, y * w + x)
        })
        .sum();
    assert!(near > 1e-3, "the eyes did not change: {near}");
    assert!(
        changed(&before, &after, 2 * w + 2) < 1e-6,
        "a far corner moved"
    );
}

#[test]
fn a_white_background_whitens_the_ground_and_keeps_the_face() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let before = render(&frame, &finished(&StudioFinish::default()));
    let finish = StudioFinish {
        background: Some(Background {
            mode: BackgroundMode::Colour,
            colour: [1.0, 1.0, 1.0],
            colour2: [1.0, 1.0, 1.0],
            amount: 100.0,
            feather: 10.0,
        }),
        ..StudioFinish::default()
    };
    let after = render(&frame, &finished(&finish));
    let w = p.width as usize;
    // The bottom corner is ground, far from the head and hair. The face mean moves a little
    // because its painted oval's rim is where the feathered edge blends.
    let i = (p.height as usize - 3) * w + 3;
    assert!(
        after[i * 3 + 1] > before[i * 3 + 1] + 0.2,
        "{} -> {}",
        before[i * 3 + 1],
        after[i * 3 + 1]
    );
    let face_before = mean_over(&before, &p.face, 1);
    let face_after = mean_over(&after, &p.face, 1);
    assert!(
        (face_before - face_after).abs() < 0.15 * face_before.max(0.01),
        "{face_before} -> {face_after}"
    );
}

#[test]
fn a_hair_colour_moves_the_hair_toward_it() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let before = render(&frame, &finished(&StudioFinish::default()));
    let mut finish = StudioFinish::default();
    finish.colours.hair = Some(Tint {
        colour: [0.8, 0.2, 0.1],
        amount: 100.0,
    });
    let after = render(&frame, &finished(&finish));
    let red = |rgb: &[f32]| mean_over(rgb, &p.hair, 0) / mean_over(rgb, &p.hair, 2).max(1e-5);
    assert!(
        red(&after) > red(&before) * 1.2,
        "{} -> {}",
        red(&before),
        red(&after)
    );
}
