//! PHASE-19. Properties of the frozen local-light contract that a later build must not
//! break by accident.
//!
//! Nothing here measures a photograph. These are the assertions that keep the *vocabulary*
//! honest: that every code has a sentence, that the priority order is the only priority
//! order, that a mask kind cannot be written into a recipe unless a renderer can evaluate
//! it, and that the five guarantees `broken_guarantee` checks actually refuse the frames
//! they name.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::{
    BackgroundBalanceDelta, DodgeBurnMaps, FaceLightDelta, FaceShaping, FaceZone, LocalCode,
    LocalLightPlan, LocalOp, LocalOverride, LocalReason, MaskField, MaskKind, ShapingZone,
    MAX_INTER_FACE_SPREAD, MAX_MEAN_LUMA_DRIFT, MIN_MASK_CONFIDENCE, SHAPING_SIDE,
};
use aura_core::contract::scene::SceneId;
use aura_core::PhotoId;

fn image() -> PhotoId {
    PhotoId::from_db("pht_00000000-0000-4000-8000-000000000019").expect("a photo id")
}

fn plan() -> LocalLightPlan {
    LocalLightPlan::nothing(
        image(),
        SceneId::FamilyPortrait,
        LocalReason::plain(LocalCode::FaceAlreadyInBand, 0.0),
    )
}

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

#[test]
fn every_reason_code_has_a_slug_and_a_sentence() {
    assert_eq!(LocalCode::ALL.len(), LocalCode::COUNT);
    for code in LocalCode::ALL {
        assert!(!code.as_str().is_empty(), "{code} has no slug");
        assert!(
            code.user_text().len() > 20,
            "{code} has no sentence a photographer could read"
        );
        assert_eq!(LocalCode::parse(code.as_str()), Some(code));
    }
}

#[test]
fn no_two_reason_codes_share_a_slug() {
    let mut slugs: Vec<&str> = LocalCode::ALL.iter().map(|c| c.as_str()).collect();
    slugs.sort_unstable();
    let before = slugs.len();
    slugs.dedup();
    assert_eq!(before, slugs.len(), "two codes share a slug");
}

#[test]
fn the_withdrawals_are_the_ones_that_decline_to_act() {
    // Fourteen, and the module header says so. A build that adds a code without deciding
    // whether it is a withdrawal changes this number and has to say why.
    let withdrawals = LocalCode::ALL
        .iter()
        .filter(|c| c.is_withdrawal())
        .count();
    assert_eq!(withdrawals, 14, "the withdrawal set moved");
}

#[test]
fn every_code_belongs_to_an_operation_or_to_the_plan() {
    // The panel groups reasons under a strength slider. A code that belonged to no
    // operation and was not a governance code would have nowhere to render.
    let governance = [
        LocalCode::MaskUnavailable,
        LocalCode::MaskWeak,
        LocalCode::BudgetExhausted,
        LocalCode::SceneStrengthLimited,
        LocalCode::TargetHeadUnavailable,
        LocalCode::LocalDisabled,
        LocalCode::UserOverride,
    ];
    for code in LocalCode::ALL {
        let expected_none = governance.contains(&code);
        assert_eq!(
            code.operation().is_none(),
            expected_none,
            "{code} is grouped under the wrong thing"
        );
    }
}

#[test]
fn the_priority_order_is_the_only_priority_order() {
    assert_eq!(LocalOp::PRIORITY.len(), LocalOp::COUNT);
    assert_eq!(LocalOp::FaceLight.rank(), 0, "face lighting has first claim");
    assert_eq!(
        LocalOp::DodgeBurnMid.rank(),
        LocalOp::COUNT - 1,
        "dodge and burn has last claim"
    );
    for (index, op) in LocalOp::PRIORITY.iter().enumerate() {
        assert_eq!(op.rank(), index);
        assert_eq!(LocalOp::parse(op.as_str()), Some(*op));
    }
}

#[test]
fn every_operation_needs_a_mask() {
    // An operation with no mask requirement would be a global adjustment wearing a local
    // name, and the governor would have nothing to gate it on.
    for op in LocalOp::PRIORITY {
        let kind = op.requires();
        assert!(
            MaskKind::ALL.contains(&kind),
            "{op} requires a kind that is not in the vocabulary"
        );
    }
}

#[test]
fn a_read_only_mask_kind_can_never_be_written_into_a_recipe() {
    // Hair is an edge-quality measurement and sky is a thing this phase declines to touch.
    assert!(!MaskKind::Hair.is_writable());
    assert!(!MaskKind::Sky.is_writable());
    for kind in MaskKind::ALL {
        assert!(!kind.as_recipe_str().is_empty());
        assert_eq!(MaskKind::parse(kind.as_str()), Some(kind));
    }
}

#[test]
fn three_zones_may_only_ever_be_lifted() {
    let dodge_only: Vec<FaceZone> = FaceZone::ALL
        .into_iter()
        .filter(|z| z.is_dodge_only())
        .collect();
    assert_eq!(
        dodge_only,
        vec![FaceZone::UnderEye, FaceZone::Cheekbone, FaceZone::NoseBridge],
        "the dodge-only set moved; a shaping map can now put a shadow under somebody's eyes"
    );
    for zone in FaceZone::ALL {
        assert_eq!(FaceZone::parse(zone.as_str()), Some(zone));
    }
}

// ---------------------------------------------------------------------------
// Mask gating
// ---------------------------------------------------------------------------

fn field(confidence: f32, edge: f32) -> MaskField {
    MaskField {
        kind: MaskKind::Face,
        identity: None,
        bounds: CropRect::FULL,
        width: 4,
        height: 4,
        alpha: vec![255; 16],
        confidence,
        edge_quality: edge,
        model_ver: 1,
    }
}

#[test]
fn a_mask_below_the_floor_produces_no_edit_at_all() {
    let weak = field(MIN_MASK_CONFIDENCE - 0.01, 1.0);
    assert_eq!(weak.strength_scale(), 0.0);
    assert!(!weak.is_usable());
}

#[test]
fn a_confident_mask_with_a_bad_edge_is_still_held_back() {
    // The two numbers fail differently. A mask can be confidently the right region and have
    // a terrible boundary - hair against a bright window - and a single number would have to
    // pick which failure to hide.
    let ragged = field(1.0, 0.25);
    let clean = field(1.0, 1.0);
    assert!(ragged.strength_scale() < clean.strength_scale());
    assert!(ragged.strength_scale() > 0.0);
}

#[test]
fn an_unreadable_field_says_what_is_wrong_with_it() {
    let mut bad = field(1.0, 1.0);
    bad.alpha.pop();
    assert!(bad.problem().is_some());
    assert!(!bad.is_readable());

    let mut wide = field(1.0, 1.0);
    wide.width = MaskField::MAX_SIDE + 1;
    assert!(wide.problem().is_some());

    assert!(field(1.0, 1.0).is_readable());
}

#[test]
fn coverage_is_a_fraction_of_the_frame() {
    let mut half = field(1.0, 1.0);
    half.alpha = vec![255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0];
    assert!((half.coverage() - 0.5).abs() < 1e-6);
}

// ---------------------------------------------------------------------------
// The five guarantees
// ---------------------------------------------------------------------------

#[test]
fn a_plan_with_no_reason_is_refused() {
    let mut p = plan();
    p.reasons.clear();
    assert!(p.broken_guarantee().is_some());
    assert!(!p.is_sound());
}

#[test]
fn a_plan_that_spent_more_than_the_budget_is_refused() {
    let mut p = plan();
    p.total_budget_used = 1.4;
    assert!(p.broken_guarantee().is_some());
}

#[test]
fn a_pairing_that_moved_the_frames_mean_luminance_is_refused() {
    let mut p = plan();
    p.background = BackgroundBalanceDelta {
        exposure_ev: -0.4,
        mean_luma_before: 0.50,
        mean_luma_after: 0.50 - MAX_MEAN_LUMA_DRIFT - 0.01,
        ..BackgroundBalanceDelta::NONE
    };
    assert!(
        p.broken_guarantee().is_some(),
        "section 10.1's own acceptance criterion is not enforced"
    );
}

#[test]
fn a_group_left_inconsistently_lit_is_refused() {
    let mut p = plan();
    let mut a = FaceLightDelta::none(0.50);
    a.luma_after = 0.50;
    let mut b = FaceLightDelta::none(0.50);
    b.luma_after = 0.50 + MAX_INTER_FACE_SPREAD + 0.01;
    p.face_light = vec![(None, a), (None, b)];
    assert!(!p.group_is_fair());
    assert!(p.broken_guarantee().is_some());
}

#[test]
fn one_face_alone_has_no_spread() {
    let mut p = plan();
    p.face_light = vec![(None, FaceLightDelta::none(0.5))];
    assert_eq!(p.inter_face_spread(), 0.0);
    assert!(p.is_sound());
}

#[test]
fn a_zone_shaped_in_the_wrong_direction_is_refused() {
    let mut p = plan();
    p.dodge_burn = Some(DodgeBurnMaps {
        faces: vec![FaceShaping {
            identity: None,
            region: CropRect::FULL,
            side: SHAPING_SIDE,
            low_freq: Vec::new(),
            mid_freq: Vec::new(),
            zones: vec![ShapingZone {
                zone: FaceZone::UnderEye,
                centre: [0.5, 0.5],
                radius: 0.1,
                // A burn under the eyes. The one thing a dodge-only zone may never do.
                gain_ev: -0.05,
            }],
            evening: 0.0,
            band_energy_before: 1.0,
            band_energy_after: 1.0,
        }],
        shaping_ver: 1,
    });
    assert!(p.broken_guarantee().is_some());
}

#[test]
fn a_zone_that_moved_too_far_is_refused() {
    let mut p = plan();
    p.dodge_burn = Some(DodgeBurnMaps {
        faces: vec![FaceShaping {
            identity: None,
            region: CropRect::FULL,
            side: SHAPING_SIDE,
            low_freq: Vec::new(),
            mid_freq: Vec::new(),
            zones: vec![ShapingZone {
                zone: FaceZone::Jaw,
                centre: [0.5, 0.5],
                radius: 0.1,
                gain_ev: ShapingZone::MAX_GAIN_EV * 2.0,
            }],
            evening: 0.0,
            band_energy_before: 1.0,
            band_energy_after: 1.0,
        }],
        shaping_ver: 1,
    });
    assert!(p.broken_guarantee().is_some());
}

#[test]
fn texture_is_preserved_within_tolerance_and_not_beyond_it() {
    let mut face = FaceShaping {
        identity: None,
        region: CropRect::FULL,
        side: SHAPING_SIDE,
        low_freq: Vec::new(),
        mid_freq: Vec::new(),
        zones: Vec::new(),
        evening: 0.5,
        band_energy_before: 1.0,
        band_energy_after: 0.97,
    };
    assert!(face.texture_preserved());
    face.band_energy_after = 0.80;
    assert!(!face.texture_preserved());
    // A flat crop reports zero drift rather than dividing by zero.
    face.band_energy_before = 0.0;
    assert_eq!(face.band_drift(), 0.0);
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

#[test]
fn an_empty_override_is_refused_so_it_cannot_lock_a_frame_nobody_edited() {
    assert!(LocalOverride::default().problem().is_some());
}

#[test]
fn an_override_sets_exactly_the_operation_it_names() {
    let one = LocalOverride::one(LocalOp::DodgeBurnLow, 0.0);
    assert!(one.problem().is_none());
    assert_eq!(one.strengths[LocalOp::DodgeBurnLow.rank()], Some(0.0));
    assert_eq!(one.strengths[LocalOp::FaceLight.rank()], None);
}

#[test]
fn an_override_outside_the_range_names_the_operation() {
    let bad = LocalOverride::one(LocalOp::FaceLight, 1.5);
    let problem = bad.problem().expect("refused");
    assert!(problem.contains("face_light"), "{problem}");
}

// ---------------------------------------------------------------------------
// The empty plan
// ---------------------------------------------------------------------------

#[test]
fn a_plan_that_does_nothing_still_carries_a_reason() {
    let p = plan();
    assert!(p.is_noop());
    assert!(!p.reasons.is_empty(), "invariant 2");
    assert!(p.is_sound());
    assert_eq!(p.active_operations(), 0);
    assert!(!p.was_gated(LocalOp::FaceLight));
}
