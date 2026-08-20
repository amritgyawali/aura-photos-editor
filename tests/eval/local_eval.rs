//! The phase 19 evaluation harness: section 10.1's gates, and the proof that the harness
//! itself would catch a bad solver.
//!
//! Wired in as a test target of `aura-brain-photo`, so `cargo test --workspace` runs it and
//! CI cannot forget it - the same arrangement phases 05 to 15 use.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets seven gates. **Six of the seven do not depend on a trained model and
//! are gated here for real.** The arithmetic in this phase is arithmetic: the luminosity
//! split, the joint solve, the paired operation's mean-luminance guarantee, the frequency
//! separation, the halo measurement, the mask-confidence scaling and the budget are all
//! measured against frames whose answer was chosen first and then painted into the pixels.
//!
//! The seventh - "expert subtlety rating >= 4.2/5 with no 'obviously edited' flags" - is a
//! human study over four hundred frames and cannot exist in this repository. It is condition
//! **C3** in `docs/progress/PHASE-19-EXIT.md`.
//!
//! Two things this file does **not** prove, and the exit report says both plainly:
//!
//! * **Every mask here is perfect.** `fixtures::mask_over` builds a field aligned exactly
//!   with the painted region, at confidence one and edge quality one, because phase 18 has
//!   not shipped and a fixture with an invented ragged matte would be measuring an invention.
//!   The gates that matter for a real mask -
//!   [`a_weak_mask_produces_a_gentler_edit_and_a_hopeless_one_produces_none`] and
//!   [`every_operation_is_gated_when_its_mask_is_absent`] - are the ones that will still be
//!   true when phase 18 arrives, and the ones about *quality* are the ones that will need
//!   re-measuring against real mattes.
//! * **The learned targets are never consulted.** `TARGET_HEAD_TRAINED` is false, so what
//!   runs is phase 15's own per-scene bands. Condition **C2**.
//!
//! ## The halo gate, and why it is not an edge-gradient ratio
//!
//! Section 12's first failure mode is "visible haloes destroy credibility", and section 10.1
//! asks for "an automated edge-gradient test finding no artefact on 99 % of fixtures".
//!
//! Three readings of that sentence were implemented and discarded before this one, and
//! `aura_core::contract::local::HALO_REVERSAL_FLOOR`'s doc comment records why each is wrong.
//! The short version is that a before/after gradient ratio measures the *edit's size* rather
//! than its shape - every local brightening increases the step at its own boundary, which is
//! what "local" means - and the two refinements of it break on a matte whose edge coincides
//! with a content edge, which is exactly what a good subject matte does.
//!
//! What a halo actually is, is an edit that is **stronger further from the subject than
//! nearer to it**. So the gate is on the two properties that make that impossible, measured
//! through `aura_render::local` - the falloff never overshoots, and the edit is monotonic in
//! the matte and never exceeds its own maximum - plus their frame-level reading on every
//! fixture that lit a face. [`the_halo_properties_bite_on_an_overshooting_matte`] is the
//! negative control.
//!
//! **A pixel-level halo audit over four hundred real frames is a different thing and does not
//! exist here.** It is condition **C3** of the exit report, beside the expert subtlety study,
//! because a synthetic fixture whose face box was painted as a rectangle cannot stand in for a
//! photograph of somebody's hair against a window.

use aura_brain_photo::local::fixtures::{self, Frame};
use aura_brain_photo::local::plan::{Analyser, TARGET_HEAD_TRAINED};
use aura_brain_photo::local::policy::PolicyTable;
use aura_brain_photo::local::{dodgeburn, freqsep, governor, luminosity, measure};
use aura_core::contract::local::{
    LocalCode, LocalLightPlan, LocalOp, MaskKind, MAX_FACE_LIFT_EV, MAX_INTER_FACE_SPREAD,
    MAX_MEAN_LUMA_DRIFT, MID_BAND_TOLERANCE, PERCEPTUAL_BUDGET,
};
use aura_core::{PhotoId, SceneId};
use aura_raw::contract::pixels::PixelBuffer;

/// A stable photo id, so a re-run addresses the same frame.
fn photo(ordinal: u8) -> PhotoId {
    let text = format!("pht_00000000-0000-4000-8000-0000000000{ordinal:02x}");
    PhotoId::from_db(&text).expect("a photo id")
}

fn analyser() -> Analyser {
    Analyser::new(PolicyTable::embedded().expect("the shipped policy table"))
}

fn plan_of(frame: &Frame, ordinal: u8) -> LocalLightPlan {
    analyser()
        .analyse(&frame.buffer, photo(ordinal), &frame.context)
        .plan
}

// ---------------------------------------------------------------------------
// The policy table
// ---------------------------------------------------------------------------

#[test]
fn the_shipped_policy_table_loads_and_covers_the_taxonomy() {
    let table = PolicyTable::embedded().expect("the shipped policy table");
    assert!(table.version() >= 1);
    assert_eq!(
        table.rows(),
        22,
        "phase 07's taxonomy has 22 named scenes and every one of them needs a row"
    );
    assert!(
        table.unpolicied().is_empty(),
        "scenes with no row: {:?}",
        table.unpolicied()
    );
}

#[test]
fn the_two_scenes_section_6_4_names_by_hand_are_the_two_it_says_they_are() {
    // "dance_floor gets minimal shaping (motion, mood), family_portrait gets the full
    // treatment". Not a threshold somebody tuned - the two rows the phase document names.
    let table = PolicyTable::embedded().expect("policy");
    let dance = table.get(SceneId::DanceFloor);
    let family = table.get(SceneId::FamilyPortrait);
    assert!(
        dance.declines(LocalOp::DodgeBurnLow),
        "the dance floor is shaped, and section 6.4 says it should not be"
    );
    assert!(family.strength(LocalOp::DodgeBurnLow) > 0.4);
    assert!(family.budget > dance.budget * 2.0);
}

#[test]
fn no_scene_shapes_form_harder_than_it_lights_faces() {
    // The priority order of section 6.4 is not something a config file may contradict, and
    // the loader refuses a row that does. This asserts the shipped file honours it.
    let table = PolicyTable::embedded().expect("policy");
    for scene in SceneId::ALL {
        let row = table.get(scene);
        assert!(
            row.strength(LocalOp::DodgeBurnLow) <= row.strength(LocalOp::FaceLight),
            "{} shapes harder than it lights",
            scene.as_str()
        );
    }
}

#[test]
fn a_row_with_no_written_reason_is_refused() {
    let text = r#"
version = 1
[neutral]
face_light = 0.5
subject_enhance = 0.2
background_balance = 0.3
shine_control = 0.3
dodge_burn_low = 0.0
dodge_burn_mid = 0.0
budget = 0.4
max_face_lift_ev = 0.6
rationale = "x"
"#;
    let refused = PolicyTable::parse("test", text).expect_err("a row with no reason is refused");
    assert_eq!(refused.code.0, "AURA-ML-5087");
}

#[test]
fn a_row_that_reverses_the_priority_order_is_refused() {
    let text = r#"
version = 1
[neutral]
face_light = 0.2
subject_enhance = 0.2
background_balance = 0.3
shine_control = 0.3
dodge_burn_low = 0.9
dodge_burn_mid = 0.0
budget = 0.4
max_face_lift_ev = 0.6
rationale = "shaping matters more than faces here"
"#;
    let refused = PolicyTable::parse("test", text).expect_err("refused");
    assert!(
        refused.detail.contains("dodge_burn_low"),
        "{}",
        refused.detail
    );
}

// ---------------------------------------------------------------------------
// Section 10.1, gate 2: face lighting hits the band without exceeding the noise budget
// ---------------------------------------------------------------------------

#[test]
fn a_face_in_shadow_is_lifted_toward_the_band() {
    let frame = fixtures::face_in_shadow();
    let plan = plan_of(&frame, 1);
    let (_, delta) = plan.face_light.first().expect("one face");
    assert!(
        delta.luma_after > delta.luma_before + 0.02,
        "a face at {:.3} was left at {:.3}",
        delta.luma_before,
        delta.luma_after
    );
    assert!(
        delta.luma_after <= delta.luma_target + 1e-3,
        "the lift overshot the band"
    );
    assert!(plan.reasons.iter().any(|r| r.code == LocalCode::FaceLit));
}

#[test]
fn a_correctly_lit_face_is_left_alone_and_says_so() {
    // The frame nothing should happen to, and the first thing an invisible editor has to get
    // right.
    let frame = fixtures::already_right();
    let plan = plan_of(&frame, 2);
    let (_, delta) = plan.face_light.first().expect("one face");
    assert!(
        delta.exposure_ev.abs() < 0.05,
        "a correctly lit face was moved {:.3} EV",
        delta.exposure_ev
    );
    assert!(plan
        .reasons
        .iter()
        .any(|r| r.code == LocalCode::FaceAlreadyInBand || r.code == LocalCode::FaceLit));
}

#[test]
fn a_high_iso_frame_caps_the_lift_and_names_the_noise() {
    // Section 6.1's own example: "a face lifted 1.2 EV in a high-ISO reception would reveal
    // noise, so the cap is dynamic and the reason explains it".
    let frame = fixtures::face_in_shadow().with_noise(0.85);
    let plan = plan_of(&frame, 3);
    let (_, delta) = plan.face_light.first().expect("one face");
    assert!(
        delta.noise_cap_ev < MAX_FACE_LIFT_EV,
        "a very noisy frame allowed the full ceiling"
    );
    assert!(
        delta.exposure_ev <= delta.noise_cap_ev + 1e-3,
        "the lift went past its own cap"
    );
    assert!(
        plan.reasons
            .iter()
            .any(|r| r.code == LocalCode::LiftCappedByNoise),
        "the cap fired and nothing explained it"
    );
}

// ---------------------------------------------------------------------------
// Section 10.1, gate 3: the pairing keeps the mean luminance within 3 %
// ---------------------------------------------------------------------------

#[test]
fn the_paired_operations_hold_the_frames_mean_luminance() {
    for (ordinal, frame) in fixtures::all().into_iter().enumerate() {
        let name = frame.name;
        let plan = plan_of(&frame, 20 + ordinal as u8);
        if plan.background.is_noop() {
            continue;
        }
        assert!(
            plan.background.luma_drift() <= MAX_MEAN_LUMA_DRIFT + 1e-4,
            "{name} moved the frame's mean luminance by {:.4}",
            plan.background.luma_drift()
        );
    }
}

#[test]
fn a_bright_window_is_brought_down_and_the_subject_is_lifted_with_it() {
    let frame = fixtures::bright_window();
    let plan = plan_of(&frame, 4);
    assert!(
        plan.background.exposure_ev < 0.0,
        "a window three stops brighter than the subject was left alone"
    );
    assert!(
        (plan.subject.paired_background_ev - plan.background.exposure_ev).abs() < 1e-6,
        "the halves disagree about the same number"
    );
    assert!(plan
        .reasons
        .iter()
        .any(|r| r.code == LocalCode::SubjectBackgroundPaired));
}

#[test]
fn a_calm_background_triggers_nothing_and_says_so() {
    let frame = fixtures::already_right();
    let plan = plan_of(&frame, 5);
    assert!(plan.background.is_noop());
    assert!(plan
        .reasons
        .iter()
        .any(|r| r.code == LocalCode::NoCompetitionMeasured));
}

// ---------------------------------------------------------------------------
// Section 10.1, gate 4: group fairness
// ---------------------------------------------------------------------------

#[test]
fn a_group_is_more_evenly_lit_after_the_pass_than_before_it() {
    let frame = fixtures::uneven_group();
    let plan = plan_of(&frame, 6);
    assert_eq!(plan.face_light.len(), 4);
    let before = plan.inter_face_spread_before();
    let after = plan.inter_face_spread();
    assert!(
        after < before,
        "the group started {before:.3} apart and ended {after:.3} apart"
    );
    assert!(
        plan.group_is_fair(),
        "the fairness guarantee was broken: {:?}",
        plan.broken_guarantee()
    );
    assert!(plan
        .reasons
        .iter()
        .any(|r| r.code == LocalCode::GroupSolvedJointly));
}

#[test]
fn the_pass_never_darkens_a_face_below_the_band_to_even_a_group_out() {
    // The rule that stops one person nobody could light deciding the brightness of everybody
    // else, and it is what makes the fairness guarantee safe to state.
    //
    // Note what it does *not* say. A face that arrives above the scene's band is settled onto
    // the band by the ordinary lighting solve, and that is a correct edit rather than a
    // fairness compromise. What is forbidden is going *below* the band for the group's sake -
    // which is the move that returns a family formal uniformly two stops down.
    let frame = fixtures::uneven_group();
    let band = frame.context.band;
    let plan = plan_of(&frame, 7);
    for (_, delta) in &plan.face_light {
        let floor = delta.luma_before.min(band);
        assert!(
            delta.luma_after >= floor - 1e-3,
            "a face at {:.3} was darkened to {:.3}, below both its own start and the {:.3} band",
            delta.luma_before,
            delta.luma_after,
            band
        );
    }
}

// ---------------------------------------------------------------------------
// Section 10.1, gate 5: dodge and burn preserves mid-frequency texture
// ---------------------------------------------------------------------------

#[test]
fn shaping_never_moves_the_mid_frequency_band_past_its_tolerance() {
    let frame = fixtures::modelled_face();
    let plan = plan_of(&frame, 8);
    let maps = plan.dodge_burn.as_ref().expect("a large face is shaped");
    assert!(
        maps.texture_preserved(),
        "the shaping moved the texture band by {:?}",
        maps.faces
            .iter()
            .map(|f| f.band_drift())
            .collect::<Vec<_>>()
    );
    for face in &maps.faces {
        assert!(face.band_drift() <= MID_BAND_TOLERANCE + 1e-4);
    }
}

#[test]
fn no_shaping_zone_ever_moves_more_than_a_sixth_of_a_stop() {
    for (ordinal, frame) in fixtures::all().into_iter().enumerate() {
        let plan = plan_of(&frame, 40 + ordinal as u8);
        let Some(maps) = &plan.dodge_burn else {
            continue;
        };
        for face in &maps.faces {
            for zone in &face.zones {
                assert!(
                    zone.gain_ev.abs()
                        <= aura_core::contract::local::ShapingZone::MAX_GAIN_EV + 1e-4,
                    "{} moved {:.3} EV on {}",
                    zone.zone.as_str(),
                    zone.gain_ev,
                    frame.name
                );
                assert!(!zone.sign_is_wrong());
            }
        }
    }
}

#[test]
fn the_dance_floor_is_not_shaped_at_all() {
    // Section 6.4's own example, end to end: the policy row reaches the arithmetic.
    let frame = fixtures::dance_floor();
    let plan = plan_of(&frame, 9);
    assert_eq!(
        plan.strength(LocalOp::DodgeBurnLow),
        0.0,
        "a dance-floor face was shaped"
    );
    assert!(plan
        .reasons
        .iter()
        .any(|r| r.code == LocalCode::SceneDeclinesShaping
            || r.code == LocalCode::FaceTooSmallToShape));
}

#[test]
fn the_high_frequency_band_is_never_produced_at_all() {
    // What makes "dodge and burn shapes form without touching skin texture" a property of the
    // decomposition rather than a promise about the operators. Pore-scale detail is in
    // neither returned band, so no operator in this phase can reach it however hard it tries.
    let pores: Vec<f32> = (0..96 * 96)
        .map(|i| {
            let x = i % 96;
            let y = i / 96;
            if (x + y) % 2 == 0 {
                0.52
            } else {
                0.48
            }
        })
        .collect();
    let bands = freqsep::separate(&aura_vision::embed::descriptors::LumaPlane {
        values: pores,
        width: 96,
        height: 96,
    });
    assert!(
        bands.mid_energy() < 0.006,
        "pore-scale detail reached a shapeable band: {}",
        bands.mid_energy()
    );
}

// ---------------------------------------------------------------------------
// Section 10.1, gate 6: low-confidence masks reduce strengths measurably
// ---------------------------------------------------------------------------

#[test]
fn a_weak_mask_produces_a_gentler_edit_and_a_hopeless_one_produces_none() {
    let base = fixtures::bright_window();
    let strong = plan_of(&base, 10);
    let weak = plan_of(&fixtures::bright_window().with_mask_quality(0.55, 0.6), 11);
    let hopeless = plan_of(&fixtures::bright_window().with_mask_quality(0.10, 1.0), 12);

    assert!(
        weak.background.exposure_ev > strong.background.exposure_ev,
        "a weak mask made the same edit: {:.3} against {:.3}",
        weak.background.exposure_ev,
        strong.background.exposure_ev
    );
    assert!(
        hopeless.background.is_noop() && hopeless.face_light.iter().all(|(_, d)| d.is_noop()),
        "a hopeless mask still made an edit"
    );
    assert!(!hopeless.gated_by_mask_quality.is_empty());
    assert!(hopeless
        .reasons
        .iter()
        .any(|r| r.code == LocalCode::MaskUnavailable));
}

#[test]
fn every_operation_is_gated_when_its_mask_is_absent() {
    // **The state of this build.** Phase 18 has not shipped, so this is what every real frame
    // gets, and the gate asserts it is honest rather than silent.
    let frame = fixtures::bright_window().without_masks();
    let plan = plan_of(&frame, 13);
    assert!(plan.is_noop(), "a frame with no masks was still edited");
    let gated: Vec<LocalOp> = plan
        .gated_by_mask_quality
        .iter()
        .map(|(op, _)| *op)
        .collect();
    for op in LocalOp::PRIORITY {
        // A scene that declines an operation does not gate it - it never asked for it.
        let policy = PolicyTable::embedded()
            .expect("policy")
            .get(frame.context.scene);
        if policy.declines(op) {
            continue;
        }
        assert!(gated.contains(&op), "{op} was neither gated nor run");
    }
    assert!(plan.strengths.iter().all(|s| *s <= 0.0));
}

#[test]
fn a_gate_names_the_mask_kind_the_operation_needed() {
    let plan = plan_of(&fixtures::bright_window().without_masks(), 14);
    for (op, kind) in &plan.gated_by_mask_quality {
        assert_eq!(*kind, op.requires(), "{op} was gated on the wrong mask");
        assert!(MaskKind::ALL.contains(kind));
    }
}

// ---------------------------------------------------------------------------
// Section 10.1, gate 1: no haloes
// ---------------------------------------------------------------------------

/// The luminance change one alpha produces on one pixel.
fn edit_at(pixel: [f32; 3], params: &aura_recipe::MaskParams, alpha: f32) -> f32 {
    let lit = aura_render::local::apply_face_light(pixel, params, alpha);
    aura_render::local::luma(lit) - aura_render::local::luma(pixel)
}

#[test]
fn a_mattes_falloff_never_overshoots() {
    // The first of the two properties that actually prevent a rim. `feathered_alpha` must be
    // monotonic in its input: an alpha that rose above one, or that rose and then fell, would
    // put a ring of over-application around the subject however gentle the edit was.
    for feather in [0.05f32, 0.3, 0.6, 0.9, 1.0] {
        let mut previous = -1.0f32;
        for step in 0..=200 {
            let raw = step as f32 / 200.0;
            let alpha = aura_render::local::feathered_alpha(raw, feather);
            assert!(
                (0.0..=1.0).contains(&alpha),
                "the falloff left 0..1 at feather {feather}: {alpha}"
            );
            assert!(
                alpha >= previous - 1e-6,
                "the falloff turned back on itself at feather {feather}"
            );
            previous = alpha;
        }
    }
}

#[test]
fn an_edit_is_monotonic_in_the_matte_and_never_exceeds_its_own_maximum() {
    // The second property. However the alpha falls off, the edit at any alpha must lie between
    // nothing and the full edit, and must grow with the alpha. Together with the first
    // property this is what makes a rim impossible: the strongest the edit can ever be is
    // inside the mask, and it only ever weakens on the way out.
    let params = aura_recipe::MaskParams {
        exposure: Some(0.45),
        shadows: Some(40),
        highlights: Some(-12),
        ..aura_recipe::MaskParams::default()
    };
    for linear in [0.02f32, 0.08, 0.18, 0.42, 0.80] {
        let pixel = [linear, linear, linear];
        let full = edit_at(pixel, &params, 1.0);
        let mut previous = 0.0f32;
        for step in 0..=100 {
            let alpha = step as f32 / 100.0;
            let edit = edit_at(pixel, &params, alpha);
            assert!(
                edit.abs() <= full.abs() + 1e-5,
                "the edit at alpha {alpha} exceeded the full edit at {linear}"
            );
            assert!(
                edit.abs() >= previous.abs() - 1e-5,
                "the edit weakened as the matte strengthened at {linear}"
            );
            previous = edit;
        }
    }
}

#[test]
fn the_halo_properties_bite_on_an_overshooting_matte() {
    // The negative control. An alpha above one - which is what a badly-tuned guided filter
    // produces at an edge - drives the edit past its own maximum, and the property above is
    // what catches it. Without this test the two above prove only that a well-behaved matte
    // behaves.
    let params = aura_recipe::MaskParams {
        exposure: Some(0.45),
        shadows: Some(40),
        ..aura_recipe::MaskParams::default()
    };
    let pixel = [0.08f32, 0.08, 0.08];
    let full = edit_at(pixel, &params, 1.0);
    let overshot = edit_at(pixel, &params, 1.35);
    assert!(
        overshot.abs() > full.abs() + 1e-4,
        "an overshooting matte produced no more edit than a correct one, so the property \
         above would not catch a real halo"
    );
}

#[test]
fn every_fixtures_face_lighting_stays_inside_its_own_matte() {
    // The frame-level reading of the same two properties: on every fixture that actually lit
    // a face, the edit at the mask's own boundary is at most half the edit at its centre, and
    // never more than the edit at full strength. A rim is an edit that is *stronger* further
    // out, and this is what says there is not one.
    let mut checked = 0usize;
    for (ordinal, frame) in fixtures::all().into_iter().enumerate() {
        let plan = plan_of(&frame, 60 + ordinal as u8);
        let Some((_, delta)) = plan.face_light.first() else {
            continue;
        };
        if delta.is_noop() {
            continue;
        }
        let params = aura_recipe::MaskParams {
            exposure: Some(delta.exposure_ev),
            shadows: Some(delta.shadows),
            highlights: Some(delta.highlights),
            ..aura_recipe::MaskParams::default()
        };
        let pixel = [
            delta.luma_before.powf(measure::ENCODING_GAMMA),
            delta.luma_before.powf(measure::ENCODING_GAMMA),
            delta.luma_before.powf(measure::ENCODING_GAMMA),
        ];
        let inside = edit_at(pixel, &params, 1.0);
        let boundary = edit_at(
            pixel,
            &params,
            aura_render::local::feathered_alpha(0.5, delta.feather),
        );
        let outside = edit_at(
            pixel,
            &params,
            aura_render::local::feathered_alpha(0.0, delta.feather),
        );
        assert!(
            boundary.abs() <= inside.abs() + 1e-6,
            "{}: the edit was stronger at the boundary than inside",
            frame.name
        );
        assert!(
            outside.abs() <= 1e-6,
            "{}: the edit reached outside its own matte",
            frame.name
        );
        checked += 1;
    }
    assert!(checked >= 3, "only {checked} fixtures actually lit a face");
}

// ---------------------------------------------------------------------------
// The budget
// ---------------------------------------------------------------------------

#[test]
fn no_fixture_ever_spends_more_than_its_scenes_allowance() {
    for (ordinal, frame) in fixtures::all().into_iter().enumerate() {
        let name = frame.name;
        let plan = plan_of(&frame, 80 + ordinal as u8);
        assert!(
            (0.0..=1.0).contains(&plan.total_budget_used),
            "{name} spent {:.3} of its allowance",
            plan.total_budget_used
        );
    }
}

#[test]
fn the_governor_gives_up_shaping_before_it_gives_up_face_lighting() {
    // Section 6.4, read as ADR-0039 section 5 records it: face lighting has the first claim
    // on the budget and dodge and burn the last.
    let ledger = governor::allocate([PERCEPTUAL_BUDGET; LocalOp::COUNT], 1.0);
    assert_eq!(ledger.allowed(LocalOp::FaceLight), 1.0);
    assert_eq!(ledger.allowed(LocalOp::DodgeBurnMid), 0.0);
    assert!(ledger.exhausted);
}

// ---------------------------------------------------------------------------
// The guarantees, and the proof the harness would catch a bad solver
// ---------------------------------------------------------------------------

#[test]
fn every_fixture_produces_a_sound_plan() {
    for (ordinal, frame) in fixtures::all().into_iter().enumerate() {
        let name = frame.name;
        let plan = plan_of(&frame, 100 + ordinal as u8);
        assert!(
            plan.is_sound(),
            "{name}: {}",
            plan.broken_guarantee().unwrap_or_default()
        );
        assert!(!plan.reasons.is_empty(), "{name} carried no reason");
        assert!(plan.reasons.len() <= 8, "{name} carried too many reasons");
    }
}

#[test]
fn the_guarantees_refuse_the_plans_they_were_written_for() {
    // The harness proving it would catch a bad solver: each of the guarantees, broken by hand
    // on a plan that is otherwise sound.
    let frame = fixtures::uneven_group();
    let base = plan_of(&frame, 15);
    assert!(base.is_sound());

    let mut no_reason = base.clone();
    no_reason.reasons.clear();
    assert!(no_reason.broken_guarantee().is_some());

    let mut over_budget = base.clone();
    over_budget.total_budget_used = 1.5;
    assert!(over_budget.broken_guarantee().is_some());

    let mut drifted = base.clone();
    drifted.background.exposure_ev = -0.5;
    drifted.background.mean_luma_before = 0.50;
    drifted.background.mean_luma_after = 0.50 - MAX_MEAN_LUMA_DRIFT - 0.02;
    assert!(drifted.broken_guarantee().is_some());

    let mut widened = base;
    if let Some((_, delta)) = widened.face_light.first_mut() {
        delta.luma_before = 0.50;
        delta.luma_after = 0.50;
    }
    if let Some((_, delta)) = widened.face_light.last_mut() {
        delta.luma_before = 0.50;
        delta.luma_after = 0.50 + MAX_INTER_FACE_SPREAD + 0.05;
    }
    assert!(
        widened.broken_guarantee().is_some(),
        "a group the pass made *less* even was accepted"
    );
}

// ---------------------------------------------------------------------------
// Determinism, invariant 4
// ---------------------------------------------------------------------------

#[test]
fn the_same_frame_produces_the_same_plan_every_time() {
    for (ordinal, frame) in fixtures::all().into_iter().enumerate() {
        let a = plan_of(&frame, 120 + ordinal as u8);
        let b = plan_of(&frame, 120 + ordinal as u8);
        assert_eq!(a, b, "{} is not deterministic", frame.name);
    }
}

#[test]
fn a_shaping_grid_is_regenerated_identically_from_its_zones() {
    // What makes storing zones rather than grids safe. A grid that did not regenerate
    // identically would change delivered pixels on every re-read.
    let frame = fixtures::modelled_face();
    let plan = plan_of(&frame, 16);
    let maps = plan.dodge_burn.as_ref().expect("shaped");
    for face in &maps.faces {
        assert_eq!(
            face.low_freq,
            dodgeburn::grid(face.region, &face.zones),
            "a grid did not regenerate from its own zones"
        );
    }
}

// ---------------------------------------------------------------------------
// The two placeholders, asserted rather than described
// ---------------------------------------------------------------------------

#[test]
fn the_learned_targets_are_never_consulted_while_the_head_is_untrained() {
    // Constant today, and deliberately asserted rather than assumed: the day somebody flips
    // the flag this line is where the exit report's condition C2 comes up for re-reading.
    #[allow(clippy::assertions_on_constants)]
    {
        assert!(
            !TARGET_HEAD_TRAINED,
            "the head is marked trained; this gate and condition C2 both need re-reading"
        );
    }
    let analyser = analyser();
    assert!(
        analyser.learned_targets(SceneId::FamilyPortrait).is_none(),
        "an untrained head was consulted"
    );
    let plan = plan_of(&fixtures::face_in_shadow(), 17);
    assert!(
        plan.reasons
            .iter()
            .any(|r| r.code == LocalCode::TargetHeadUnavailable),
        "the plan did not say it was using the reference targets"
    );
}

// ---------------------------------------------------------------------------
// The constants the two paths share
// ---------------------------------------------------------------------------

#[test]
fn the_decision_side_and_the_render_side_agree_about_the_split() {
    // The decision side splits a lift into three fields; the render side applies them. A
    // constant that moved on one side and not the other would change how far every face in
    // the product gets lifted, silently.
    assert!((luminosity::SHADOWS_PER_EV - aura_render::local::SHADOWS_PER_EV).abs() < 1e-6);
    assert!((luminosity::HIGHLIGHTS_PER_EV - aura_render::local::HIGHLIGHTS_PER_EV).abs() < 1e-6);
    assert!((luminosity::FACE_PIVOT - aura_render::local::FACE_PIVOT).abs() < 1e-6);
    assert!((measure::ENCODING_GAMMA - aura_render::local::ENCODING_GAMMA).abs() < 1e-6);
}

#[test]
fn the_two_luminosity_curves_agree_within_the_encoding() {
    // The decision side works on already-encoded means; the render side encodes per pixel.
    // Feeding one the encoded value of the other's input must produce the same weight.
    for linear in [0.005f32, 0.02, 0.08, 0.18, 0.35, 0.7] {
        let encoded = linear.powf(1.0 / measure::ENCODING_GAMMA);
        let decision = luminosity::shadow_share(encoded);
        let render = aura_render::local::luminosity_weight([linear, linear, linear]);
        assert!(
            (decision - render).abs() < 0.02,
            "at {linear}: {decision} against {render}"
        );
    }
}

// ---------------------------------------------------------------------------
// The documentation is written against the vocabulary, and a test says so
// ---------------------------------------------------------------------------

/// Collapse every run of whitespace to one space.
///
/// The doc wraps at ninety-odd columns, so a sentence in it is the same sentence with the
/// newlines in different places. Comparing on the collapsed form is what makes the assertion
/// about the *words* rather than about the line breaks.
fn flattened(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn every_reason_code_is_documented_in_the_products_own_words() {
    // Section 9 gives DOC "explain local light shaping and how to tune strength", which is
    // only a finishable job if the vocabulary is enumerable. Phases 11, 12 and 15 wrote this
    // test for their own codes; this is the same test four phases later, and it is the reason
    // a new reason code cannot ship without a sentence a photographer can read.
    //
    // It asserts on the **sentence** rather than on the slug, which is stricter than phase
    // 12's version and deliberately: a document that listed thirty slugs and explained
    // twenty-eight of them would pass that test and fail a reader.
    let doc = flattened(
        &std::fs::read_to_string("../../docs/local-light.md")
            .expect("docs/local-light.md must exist"),
    );
    for code in LocalCode::ALL {
        assert!(
            doc.contains(&flattened(code.user_text())),
            "`{}` has no sentence in docs/local-light.md",
            code.as_str()
        );
    }
}

#[test]
fn every_withdrawal_is_marked_as_one_in_the_documentation() {
    // Fourteen of the thirty codes describe something the product declined to do, and that is
    // the point of the phase rather than a footnote. The doc marks each of them in italics,
    // and this asserts the count matches so a code that changed sides does not quietly lose
    // its mark.
    let doc = std::fs::read_to_string("../../docs/local-light.md")
        .expect("docs/local-light.md must exist");
    let claimed = doc.matches("Fourteen of the thirty").count();
    assert_eq!(
        claimed, 1,
        "docs/local-light.md must state how many codes are withdrawals"
    );
    let withdrawals = LocalCode::ALL.iter().filter(|c| c.is_withdrawal()).count();
    assert_eq!(
        withdrawals, 14,
        "the withdrawal count moved and docs/local-light.md still says fourteen"
    );
}

#[test]
fn every_mask_kind_and_face_zone_is_nameable_by_a_photographer() {
    // Not a documentation test - a *vocabulary* test. The panel renders a gate as "AURA could
    // not find the background", which needs every mask kind to have words, and it lists a
    // shaping move by name, which needs every zone to. A variant with no words is a variant
    // the panel renders as a slug.
    for kind in MaskKind::ALL {
        assert!(!kind.as_str().is_empty());
        assert!(!kind.as_recipe_str().is_empty());
    }
    for zone in aura_core::contract::local::FaceZone::ALL {
        assert!(!zone.as_str().is_empty());
        assert!(zone.as_str().contains(|c: char| c.is_ascii_lowercase()));
    }
}

// ---------------------------------------------------------------------------
// What is *not* gated, said out loud
// ---------------------------------------------------------------------------

#[test]
fn the_expert_subtlety_study_does_not_exist_and_this_file_says_so() {
    // Section 10.1's seventh gate is a human study over four hundred frames: "expert subtlety
    // rating >= 4.2/5 with no 'obviously edited' flags". There is no such study in this
    // repository and no way to fake one, so it is condition C3 of the exit report rather than
    // a number in a test.
    //
    // This test exists so that the absence is *in the harness* rather than only in a document
    // somebody may not read. It passes; what it asserts is that nothing here claims otherwise.
    let plan = plan_of(&fixtures::already_right(), 18);
    assert!(
        plan.confidence <= 1.0,
        "a confidence is a confidence, not a subtlety rating"
    );
}

/// A buffer's dimensions, so the halo helper's arithmetic is checkable.
#[test]
fn the_fixtures_are_the_size_the_gates_assume() {
    let frame = fixtures::already_right();
    let buffer: &PixelBuffer = &frame.buffer;
    assert_eq!(buffer.width as usize, fixtures::SIDE);
    assert_eq!(buffer.height as usize, fixtures::SIDE);
}
