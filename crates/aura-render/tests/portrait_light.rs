//! Portrait light (ADR-0127), executed by the reference renderer on a painted face.
//!
//! The face is `aura_portrait::fixtures`' synthetic portrait, so where its cheeks, eyes and
//! ground are is known by construction. These prove the renderer finds them through the parse
//! and lights what a control names; they are not evidence about a real photograph.

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
use aura_recipe::studio_finish::{self, Light, StudioFinish};
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

fn lit(light: Light) -> Recipe {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    studio_finish::write(
        &mut recipe,
        &StudioFinish {
            light,
            ..StudioFinish::default()
        },
    )
    .unwrap();
    recipe
}

fn luma(rgb: &[f32], w: usize, x: usize, y: usize) -> f32 {
    let i = (y * w + x) * 3;
    0.2627 * rgb[i] + 0.678 * rgb[i + 1] + 0.0593 * rgb[i + 2]
}

/// The mean luminance of a small square, in log2.
fn patch(rgb: &[f32], w: usize, at: [f32; 2], half: usize) -> f32 {
    let (cx, cy) = (at[0] as usize, at[1] as usize);
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in cy - half..=cy + half {
        for x in cx - half..=cx + half {
            sum += luma(rgb, w, x, y).max(1e-6).log2();
            n += 1.0;
        }
    }
    sum / n
}

/// A cheek: below and outside an eye.
fn cheek(p: &Portrait, eye: usize) -> [f32; 2] {
    let e = p.eyes[eye];
    let centre = (p.eyes[0][0] + p.eyes[1][0]) * 0.5;
    let out = if e[0] < centre { -6.0 } else { 6.0 };
    [e[0] + out, e[1] + 26.0]
}

#[test]
fn a_light_that_adds_nothing_changes_nothing() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let neutral = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    let aimed = lit(Light {
        direction: 70.0,
        height: 10.0,
        ..Light::default()
    });
    assert_eq!(render(&frame, &neutral), render(&frame, &aimed));
}

#[test]
fn a_key_light_from_the_left_lights_the_left_cheek_and_keeps_the_ground() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let w = p.width as usize;
    let before = render(&frame, &lit(Light::default()));
    let after = render(
        &frame,
        &lit(Light {
            key: 80.0,
            direction: -80.0,
            height: 20.0,
            ..Light::default()
        }),
    );
    let (l, r) = (cheek(&p, 0), cheek(&p, 1));
    let lifted = patch(&after, w, l, 2) - patch(&before, w, l, 2);
    let dropped = patch(&after, w, r, 2) - patch(&before, w, r, 2);
    assert!(lifted > 0.2, "the lit cheek moved {lifted} stops");
    assert!(lifted - dropped > 0.4, "lit {lifted} vs shadow {dropped}");
    // The ground, far from the head, is not on the person.
    assert!(
        (luma(&after, w, 4, p.height as usize - 5) - luma(&before, w, 4, p.height as usize - 5))
            .abs()
            < 1e-4
    );
}

#[test]
fn fill_opens_the_shadow_side_of_a_half_lit_face() {
    let p = portrait(&PortraitSpec::default());
    let mut frame = frame_of(&p);
    let w = p.width as usize;
    // Light the face from the left: the right half of the frame falls a stop and a half.
    let centre = (p.eyes[0][0] + p.eyes[1][0]) * 0.5;
    for y in 0..p.height as usize {
        for x in 0..w {
            let t = ((x as f32 - centre) / 20.0).clamp(0.0, 1.0);
            let gain = 1.0 - 0.65 * t;
            for c in 0..3 {
                frame.rgb[(y * w + x) * 3 + c] *= gain;
            }
        }
    }
    let before = render(&frame, &lit(Light::default()));
    let after = render(
        &frame,
        &lit(Light {
            fill: 100.0,
            ..Light::default()
        }),
    );
    let (l, r) = (cheek(&p, 0), cheek(&p, 1));
    let gap_before = patch(&before, w, l, 2) - patch(&before, w, r, 2);
    let gap_after = patch(&after, w, l, 2) - patch(&after, w, r, 2);
    assert!(
        gap_before > 1.0,
        "the fixture is not half lit: {gap_before}"
    );
    assert!(
        gap_after < gap_before - 0.4,
        "fill closed the gap from {gap_before} only to {gap_after}"
    );
    // The lit side, above the face's median, is barely touched.
    assert!((patch(&after, w, l, 2) - patch(&before, w, l, 2)).abs() < 0.15);
}

#[test]
fn catchlights_land_in_the_iris_on_the_side_of_the_light() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let w = p.width as usize;
    let before = render(&frame, &lit(Light::default()));
    let after = render(
        &frame,
        &lit(Light {
            catchlights: 100.0,
            direction: -45.0,
            height: 35.0,
            ..Light::default()
        }),
    );
    for eye in p.eyes {
        // Up and to the left of the iris centre, where a light from the camera's left reflects.
        let mut best = (0.0f32, 0isize, 0isize);
        for dy in -5isize..=5 {
            for dx in -5isize..=5 {
                let (x, y) = (
                    (eye[0] as isize + dx) as usize,
                    (eye[1] as isize + dy) as usize,
                );
                let gain = luma(&after, w, x, y) - luma(&before, w, x, y);
                if gain > best.0 {
                    best = (gain, dx, dy);
                }
            }
        }
        assert!(best.0 > 0.2, "no catchlight in the eye at {eye:?}");
        assert!(
            best.1 <= 0 && best.2 <= 0,
            "the catchlight is not up and left: {best:?}"
        );
    }
    // Nothing outside the eyes moved.
    let far = cheek(&p, 0);
    assert!((patch(&after, w, far, 2) - patch(&before, w, far, 2)).abs() < 1e-4);
}

#[test]
fn an_eye_that_already_has_a_catchlight_is_left_alone() {
    let p = portrait(&PortraitSpec::default());
    let mut frame = frame_of(&p);
    let w = p.width as usize;
    // Paint a bright reflection into each iris.
    for eye in p.eyes {
        for dy in -1i32..=0 {
            for dx in -2i32..=-1 {
                let (x, y) = ((eye[0] as i32 + dx) as usize, (eye[1] as i32 + dy) as usize);
                for c in 0..3 {
                    frame.rgb[(y * w + x) * 3 + c] = 1.6;
                }
            }
        }
    }
    let before = render(&frame, &lit(Light::default()));
    let after = render(
        &frame,
        &lit(Light {
            catchlights: 100.0,
            ..Light::default()
        }),
    );
    let moved: f32 = before
        .iter()
        .zip(after.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(
        moved < 1e-3,
        "an eye with a catchlight got another: {moved}"
    );
}

#[test]
fn auto_light_proposes_a_fill_for_a_half_lit_face_and_catchlights_for_dull_eyes() {
    let p = portrait(&PortraitSpec::default());
    let mut frame = frame_of(&p);
    let w = p.width as usize;
    let centre = (p.eyes[0][0] + p.eyes[1][0]) * 0.5;
    for y in 0..p.height as usize {
        for x in 0..w {
            let t = ((x as f32 - centre) / 20.0).clamp(0.0, 1.0);
            for c in 0..3 {
                frame.rgb[(y * w + x) * 3 + c] *= 1.0 - 0.65 * t;
            }
        }
    }
    let map = aura_render::portrait::parse(&frame, &[]).expect("a parse");
    let s = aura_render::portrait_light::suggest(&frame.rgb, frame.width, frame.height, &map);
    assert!(s.faces >= 1, "{s:?}");
    assert!(s.light.fill > 0.0, "{s:?}");
    assert!(
        s.light.key <= 0.0,
        "a harshly lit face gets no extra key: {s:?}"
    );
    assert!(s.light.catchlights > 0.0, "{s:?}");
    assert!(s.contrast.unwrap_or(0.0) > 1.0, "{s:?}");
    // An evenly lit face gets a key light instead.
    let even = frame_of(&p);
    let map = aura_render::portrait::parse(&even, &[]).expect("a parse");
    let s = aura_render::portrait_light::suggest(&even.rgb, even.width, even.height, &map);
    assert!(s.light.key > 0.0 && s.light.fill <= 0.0, "{s:?}");
}

#[test]
fn glow_alone_needs_no_face() {
    let p = portrait(&PortraitSpec::default());
    let frame = frame_of(&p);
    let before = render(&frame, &lit(Light::default()));
    let after = render(
        &frame,
        &lit(Light {
            glow: 60.0,
            ..Light::default()
        }),
    );
    assert_ne!(before, after);
}
