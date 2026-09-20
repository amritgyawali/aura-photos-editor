//! The whole chain, through the real renderer.
//!
//! `look_eval.rs` measures the solver against an analytic probe, which answers "does the search
//! find the minimum it is pointed at". This answers the different and harder question: **when
//! the delta is handed to `RenderService` and the result is measured, does the gallery actually
//! move toward the reference.**
//!
//! It is a separate file because the two fail for different reasons and a single file would
//! make a renderer regression look like a solver regression.

use std::sync::Arc;

use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::look::MATCH_DE00_CEILING;
use aura_core::contract::style::{LightingBucket, StyleDelta};
use aura_look::fixtures::{self, SyntheticLook};
use aura_look::verify::{FrameProbe, OwnFrame, Renderer};
use aura_look::{aggregate, solve};
use aura_render::contract::render::OutputSpec;
use aura_render::cpu::Frame;
use time::OffsetDateTime;

fn clock() -> Arc<dyn Clock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000))
}

/// The photographer's own frames: authored plates at the neutral look, carrying a neutral
/// baseline recipe. These stand in for a wedding phases 15 and 16 have already decided.
fn own_frames(count: u32) -> Vec<OwnFrame> {
    (0..count)
        .map(|seed| {
            let image = fixtures::plate(192, 128, seed + 500);
            let rgb: Vec<f32> = image
                .data
                .iter()
                .map(|sample| {
                    let value = f32::from(*sample) / 255.0;
                    if value <= 0.040_45 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
                })
                .collect();
            OwnFrame {
                key: format!("own-{seed:04}"),
                frame: Frame::working(rgb, image.width, image.height, "reference"),
                width: image.width,
                height: image.height,
                baseline: aura_recipe::fixtures::neutral(&format!("own-{seed:04}"), "reference"),
                lighting: LightingBucket::Daylight,
                user_edited: false,
            }
        })
        .collect()
}

#[test]
fn a_look_moves_a_gallery_toward_the_reference_when_it_is_actually_rendered() {
    let renderer = Renderer::new(clock(), OutputSpec::default());
    let frames = own_frames(6);

    let reference = fixtures::aggregate(24, SyntheticLook::light_and_airy());

    // Where the gallery sits before anything is applied, measured through the renderer.
    let neutral = StyleDelta::neutral();
    let before_readings: Vec<_> = frames
        .iter()
        .map(|frame| {
            renderer
                .read_with(frame, &neutral)
                .expect("the renderer must produce a frame")
        })
        .collect();
    let baseline = aggregate::fold(&aggregate::all(&before_readings));
    let before = solve::distance(&baseline, &reference);

    // Solve, refine against the real renderer, and measure what actually came out.
    let initial = solve::initial(&reference, &baseline);
    let probe = FrameProbe::new(&renderer, &frames);
    let (delta, _) = solve::refine(&initial, &reference, &probe);

    let after_readings: Vec<_> = frames
        .iter()
        .map(|frame| renderer.read_with(frame, &delta).expect("render"))
        .collect();
    let after = solve::distance(
        &aggregate::fold(&aggregate::all(&after_readings)),
        &reference,
    );

    assert!(
        after < before,
        "applying the look through the real renderer did not move the gallery toward the \
         reference: {before:.3} -> {after:.3} dE00"
    );
    // Phase 27's rule: measured against what the gap was, not against the ceiling.
    let closed = (before - after) / before.max(1e-6);
    assert!(
        closed > 0.4,
        "the look closed only {:.0}% of a {before:.2} dE00 gap when rendered",
        closed * 100.0
    );
}

#[test]
fn a_reference_the_gallery_already_matches_is_left_alone() {
    let renderer = Renderer::new(clock(), OutputSpec::default());
    let frames = own_frames(6);

    let neutral = StyleDelta::neutral();
    let readings: Vec<_> = frames
        .iter()
        .map(|frame| renderer.read_with(frame, &neutral).expect("render"))
        .collect();
    let baseline = aggregate::fold(&aggregate::all(&readings));

    // The reference IS the rendered baseline, so there is nothing to ask for.
    let delta = solve::initial(&baseline, &baseline);

    assert!(
        delta.is_neutral(),
        "a gallery that already matches its reference was still edited: {delta:?}"
    );
    let distance = solve::distance(&baseline, &baseline);
    assert!(
        distance <= MATCH_DE00_CEILING,
        "an aggregate is not zero distance from itself: {distance}"
    );
}

#[test]
fn a_frame_the_photographer_edited_by_hand_is_measured_and_never_moved() {
    let renderer = Renderer::new(clock(), OutputSpec::default());
    let mut frames = own_frames(2);
    if let Some(frame) = frames.get_mut(0) {
        frame.user_edited = true;
    }

    // A delta large enough that applying it would be obvious.
    let delta = StyleDelta {
        exposure: 0.5,
        ..StyleDelta::neutral()
    }
    .clamped();

    let edited = renderer
        .read_with(frames.first().expect("a frame"), &delta)
        .expect("render");
    let edited_neutral = renderer
        .read_with(frames.first().expect("a frame"), &StyleDelta::neutral())
        .expect("render");
    let untouched = renderer
        .read_with(frames.get(1).expect("a frame"), &delta)
        .expect("render");
    let untouched_neutral = renderer
        .read_with(frames.get(1).expect("a frame"), &StyleDelta::neutral())
        .expect("render");

    // Phase 14's rule: a parameter a person set is never overwritten.
    assert!(
        (edited.tone.p50 - edited_neutral.tone.p50).abs() < 1e-5,
        "a hand-edited frame was moved by the look"
    );
    // And the control: the same delta does move a frame nobody edited, so the assertion above
    // is about the guard rather than about a delta that happened to do nothing.
    assert!(
        (untouched.tone.p50 - untouched_neutral.tone.p50).abs() > 0.01,
        "the delta did not move an ordinary frame either, so the test above proves nothing"
    );
}
