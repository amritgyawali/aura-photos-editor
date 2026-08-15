//! Role inference and prominence.
//!
//! Two rules are asserted here that are product decisions rather than modelling ones,
//! and both would be easy to lose in a refactor:
//!
//! * **Nothing is ever inferred about a person.** The evidence identifies a *pair*;
//!   which of two people is the bride is not a photographic fact, the couple may be
//!   same-sex, and `Role::Bride` is only ever reachable through a photographer's
//!   decision.
//! * **A locked identity is an answer, not an input.** Section 12's first failure mode
//!   is a wrong couple poisoning every later decision, and the mitigation is that the
//!   human's answer is structurally unbeatable rather than merely higher-weighted.

use std::collections::BTreeMap;

use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::{FaceRef, Role, SubjectHierarchy};
use aura_core::{FaceId, IdentityId};
use aura_vision::face::align::Pose;
use aura_vision::face::prominence::{self, ProminenceInput, ProminenceWeights, SubjectFace};
use aura_vision::face::roles::{
    self, IdentityEvidence, PairEvidence, COUPLE_AMBIGUOUS_MARGIN, ROLE_VER,
    SCENELESS_CONFIDENCE_CEILING, SCENE_CEREMONY, SCENE_COUPLE_PORTRAIT, SCENE_GETTING_READY,
};

fn evidence(index: usize, frames: u32, area: f32, centrality: f32) -> IdentityEvidence {
    IdentityEvidence {
        index,
        frames,
        scene_frames: BTreeMap::new(),
        mean_area: area,
        mean_centrality: centrality,
        edge_fraction: 0.1,
        child_score: 0.0,
        first_seen_ms: Some(0),
        last_seen_ms: Some(1_000),
        user_locked: false,
        locked_role: Role::Unknown,
    }
}

fn pair(a: usize, b: usize, frames: u32) -> PairEvidence {
    PairEvidence {
        a,
        b,
        frames,
        scene_frames: BTreeMap::new(),
    }
}

fn role_of(outcome: &roles::RoleOutcome, index: usize) -> Role {
    outcome
        .verdicts
        .iter()
        .find(|verdict| verdict.index == index)
        .map_or(Role::Unknown, |verdict| verdict.role)
}

#[test]
fn the_couple_is_the_pair_photographed_together_most() {
    let identities = vec![
        evidence(0, 90, 0.12, 0.8),
        evidence(1, 88, 0.11, 0.78),
        evidence(2, 30, 0.05, 0.4),
        evidence(3, 25, 0.05, 0.35),
    ];
    let pairs = vec![pair(0, 1, 70), pair(0, 2, 12), pair(1, 3, 9), pair(2, 3, 6)];
    let outcome = roles::infer_roles(&identities, &pairs);

    assert_eq!(outcome.couple, Some((0, 1)));
    assert_eq!(role_of(&outcome, 0), Role::Couple);
    assert_eq!(role_of(&outcome, 1), Role::Couple);
}

#[test]
fn neither_half_of_the_couple_is_ever_called_the_bride() {
    // The product decision, asserted so a later refactor cannot quietly add a heuristic.
    let identities = vec![evidence(0, 90, 0.14, 0.85), evidence(1, 60, 0.06, 0.5)];
    let pairs = vec![pair(0, 1, 50)];
    let outcome = roles::infer_roles(&identities, &pairs);

    for verdict in &outcome.verdicts {
        assert_ne!(
            verdict.role,
            Role::Bride,
            "automation must never assign bride"
        );
        assert_ne!(
            verdict.role,
            Role::Groom,
            "automation must never assign groom"
        );
    }
}

#[test]
fn every_verdict_carries_reasons() {
    // Invariant 2, on the surface a photographer is most likely to disagree with.
    let identities = vec![
        evidence(0, 40, 0.10, 0.7),
        evidence(1, 38, 0.09, 0.68),
        evidence(2, 9, 0.03, 0.3),
        evidence(3, 1, 0.02, 0.2),
    ];
    let pairs = vec![pair(0, 1, 30), pair(0, 2, 5)];
    let outcome = roles::infer_roles(&identities, &pairs);

    for verdict in &outcome.verdicts {
        assert!(
            !verdict.reasons.is_empty(),
            "verdict {} has no reasons",
            verdict.index
        );
    }
    assert_eq!(outcome.role_ver, ROLE_VER);
}

#[test]
fn a_locked_identity_wins_every_contest() {
    // The photographer said this is the groom. Automation may not disagree, and the
    // couple contest may not include somebody who has been locked to something else.
    let mut identities = vec![
        evidence(0, 90, 0.12, 0.8),
        evidence(1, 88, 0.11, 0.78),
        evidence(2, 20, 0.04, 0.3),
    ];
    identities[1].user_locked = true;
    identities[1].locked_role = Role::Vendor;

    let pairs = vec![pair(0, 1, 70), pair(0, 2, 30)];
    let outcome = roles::infer_roles(&identities, &pairs);

    assert_eq!(role_of(&outcome, 1), Role::Vendor, "a locked role is final");
    assert!(
        outcome.couple != Some((0, 1)),
        "an identity locked to something else may not be half of the couple"
    );
    let locked = outcome
        .verdicts
        .iter()
        .find(|verdict| verdict.index == 1)
        .expect("a verdict for the locked identity");
    assert!(locked.locked);
    assert!((locked.confidence - 1.0).abs() < f32::EPSILON);
}

#[test]
fn a_locked_couple_overrides_the_evidence() {
    let mut identities = vec![
        evidence(0, 90, 0.12, 0.8),
        evidence(1, 88, 0.11, 0.78),
        evidence(2, 20, 0.04, 0.3),
    ];
    identities[0].user_locked = true;
    identities[0].locked_role = Role::Bride;
    identities[2].user_locked = true;
    identities[2].locked_role = Role::Groom;

    let pairs = vec![pair(0, 1, 70), pair(0, 2, 12)];
    let outcome = roles::infer_roles(&identities, &pairs);

    assert_eq!(outcome.couple, Some((0, 2)), "the photographer's pair wins");
    assert!(!outcome.ambiguous);
    assert!((outcome.couple_score - 1.0).abs() < f32::EPSILON);
}

#[test]
fn a_close_second_pair_is_reported_as_ambiguous() {
    // Section 7's trigger and section 12's first mitigation are the same question.
    let identities = vec![
        evidence(0, 60, 0.10, 0.7),
        evidence(1, 60, 0.10, 0.7),
        evidence(2, 60, 0.10, 0.7),
    ];
    let pairs = vec![pair(0, 1, 40), pair(0, 2, 40), pair(1, 2, 39)];
    let outcome = roles::infer_roles(&identities, &pairs);

    assert!(
        outcome.ambiguous,
        "two pairs {:.3} apart should be ambiguous",
        outcome.couple_score - outcome.runner_up_score
    );
    assert!(outcome.couple_score - outcome.runner_up_score < COUPLE_AMBIGUOUS_MARGIN);

    let couple_verdict = outcome
        .verdicts
        .iter()
        .find(|verdict| verdict.role.is_couple())
        .expect("a couple verdict");
    assert!(
        couple_verdict
            .reasons
            .iter()
            .any(|reason| reason.contains("confirm")),
        "{:?}",
        couple_verdict.reasons
    );
}

#[test]
fn a_scene_starved_decision_is_capped_and_says_so() {
    // Until phase 07 runs, a bride and her mother are as co-occurrent as a bride and a
    // groom. The confidence has to reflect that rather than sounding earned.
    let identities = vec![evidence(0, 90, 0.14, 0.85), evidence(1, 85, 0.13, 0.83)];
    let pairs = vec![pair(0, 1, 80)];
    let outcome = roles::infer_roles(&identities, &pairs);

    assert!(outcome.scene_starved);
    let verdict = outcome
        .verdicts
        .iter()
        .find(|verdict| verdict.role.is_couple())
        .expect("a couple verdict");
    assert!(
        verdict.confidence <= SCENELESS_CONFIDENCE_CEILING,
        "confidence {} exceeds the sceneless ceiling",
        verdict.confidence
    );
    assert!(
        verdict
            .reasons
            .iter()
            .any(|reason| reason.contains("no scene labels")),
        "{:?}",
        verdict.reasons
    );
}

#[test]
fn scene_evidence_lifts_the_ceiling() {
    // The same code path with real scene counts, which is what phase 07 will supply.
    let mut identities = vec![evidence(0, 90, 0.14, 0.85), evidence(1, 85, 0.13, 0.83)];
    for identity in &mut identities {
        identity
            .scene_frames
            .insert(SCENE_GETTING_READY.to_string(), 20);
        identity.scene_frames.insert(SCENE_CEREMONY.to_string(), 30);
        identity
            .scene_frames
            .insert(SCENE_COUPLE_PORTRAIT.to_string(), 25);
    }
    let mut portraits = pair(0, 1, 80);
    portraits
        .scene_frames
        .insert(SCENE_COUPLE_PORTRAIT.to_string(), 24);

    let outcome = roles::infer_roles(&identities, &[portraits]);
    assert!(!outcome.scene_starved);
    let verdict = outcome
        .verdicts
        .iter()
        .find(|verdict| verdict.role.is_couple())
        .expect("a couple verdict");
    assert!(
        verdict.confidence > SCENELESS_CONFIDENCE_CEILING,
        "with scenes the ceiling should not apply, got {}",
        verdict.confidence
    );
}

#[test]
fn a_vendor_is_present_all_day_and_never_the_subject() {
    let identities = vec![
        evidence(0, 60, 0.14, 0.8),
        evidence(1, 58, 0.13, 0.78),
        // Present more than anybody, tiny in frame, always at the edge.
        IdentityEvidence {
            edge_fraction: 0.85,
            ..evidence(2, 95, 0.02, 0.15)
        },
    ];
    let pairs = vec![pair(0, 1, 50), pair(0, 2, 6), pair(1, 2, 5)];
    let outcome = roles::infer_roles(&identities, &pairs);

    assert_eq!(role_of(&outcome, 2), Role::Vendor);
}

#[test]
fn close_family_is_recognised_by_co_occurrence_with_the_couple() {
    let identities = vec![
        evidence(0, 80, 0.13, 0.8),
        evidence(1, 78, 0.12, 0.78),
        evidence(2, 40, 0.08, 0.6),
        evidence(3, 6, 0.04, 0.4),
    ];
    let pairs = vec![
        pair(0, 1, 60),
        pair(0, 2, 34),
        pair(1, 2, 30),
        pair(0, 3, 2),
    ];
    let outcome = roles::infer_roles(&identities, &pairs);

    assert_eq!(role_of(&outcome, 2), Role::FamilyClose);
    assert_eq!(role_of(&outcome, 3), Role::Guest);
}

#[test]
fn a_child_is_recognised_by_proportion_and_not_by_anything_else() {
    let identities = vec![
        evidence(0, 60, 0.12, 0.8),
        evidence(1, 58, 0.11, 0.78),
        IdentityEvidence {
            child_score: 0.8,
            ..evidence(2, 20, 0.06, 0.5)
        },
    ];
    let pairs = vec![pair(0, 1, 40), pair(0, 2, 4)];
    let outcome = roles::infer_roles(&identities, &pairs);
    assert_eq!(role_of(&outcome, 2), Role::Child);
}

#[test]
fn somebody_seen_once_gets_no_role_at_all() {
    let identities = vec![
        evidence(0, 40, 0.10, 0.7),
        evidence(1, 38, 0.10, 0.7),
        evidence(2, 1, 0.05, 0.5),
    ];
    let outcome = roles::infer_roles(&identities, &[pair(0, 1, 30)]);
    assert_eq!(role_of(&outcome, 2), Role::Unknown);
    let verdict = outcome
        .verdicts
        .iter()
        .find(|verdict| verdict.index == 2)
        .expect("a verdict");
    assert_eq!(verdict.confidence, 0.0);
}

#[test]
fn an_empty_project_produces_no_couple() {
    let outcome = roles::infer_roles(&[], &[]);
    assert!(outcome.couple.is_none());
    assert!(outcome.verdicts.is_empty());
}

#[test]
fn role_inference_is_deterministic() {
    let identities = vec![
        evidence(0, 50, 0.10, 0.7),
        evidence(1, 50, 0.10, 0.7),
        evidence(2, 50, 0.10, 0.7),
        evidence(3, 50, 0.10, 0.7),
    ];
    // Every pair identical: the tie-break is the only thing deciding, and it must decide
    // the same way twice.
    let pairs = vec![pair(0, 1, 20), pair(2, 3, 20), pair(0, 2, 20)];
    let first = roles::infer_roles(&identities, &pairs);
    let second = roles::infer_roles(&identities, &pairs);
    assert_eq!(first.couple, second.couple);
    assert_eq!(
        first
            .verdicts
            .iter()
            .map(|v| (v.index, v.role))
            .collect::<Vec<_>>(),
        second
            .verdicts
            .iter()
            .map(|v| (v.index, v.role))
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Prominence
// ---------------------------------------------------------------------------

fn face(identity: Option<IdentityId>, area: f32, centrality: f32, sharpness: f32) -> SubjectFace {
    SubjectFace {
        reference: FaceRef {
            face_id: FaceId::new(),
            identity_id: identity,
            // Prominence does not read the geometry; these are here because the
            // PHASE-09 amendment made them fields rather than because this test needs
            // them. A centred box of the right area is the honest filler.
            bbox: CropRect {
                x: 0.5 - area.sqrt() / 2.0,
                y: 0.5 - area.sqrt() / 2.0,
                w: area.sqrt(),
                h: area.sqrt(),
            },
            eyes: [[0.45, 0.45], [0.55, 0.45]],
            area_frac: area,
            centrality,
            sharpness,
            quality: 0.8,
            votes: true,
        },
        input: ProminenceInput {
            area_frac: area,
            centrality,
            sharpness,
            gaze: 1.0,
            role: Role::Guest,
            importance: 0.5,
        },
    }
}

#[test]
fn the_embedded_weight_table_parses_and_carries_a_version() {
    let weights = ProminenceWeights::embedded();
    assert!(
        weights.version > 0,
        "a weight table with version 0 cannot be recorded against a score"
    );
    assert!(weights.base.total() > 0.0);
    assert!(
        weights.scenes.contains_key("dancing"),
        "the scene tables must be present before phase 07 turns them on"
    );
    assert!(weights.role_weight(Role::Bride) > weights.role_weight(Role::Guest));
    assert!(weights.role_weight(Role::Guest) > weights.role_weight(Role::Vendor));
}

#[test]
fn an_untouched_importance_slider_is_exactly_a_no_op() {
    // 0.5 must be neutral, or a photographer who never opens the panel gets a different
    // answer from the product's own opinion.
    let weights = ProminenceWeights::embedded();
    let neutral = ProminenceInput {
        area_frac: 0.10,
        centrality: 0.7,
        sharpness: 0.8,
        gaze: 0.9,
        role: Role::FamilyClose,
        importance: 0.5,
    };
    let scored = prominence::prominence(&neutral, &weights, None);

    // The same score, computed with the role weight applied directly.
    let set = weights.for_scene(None);
    let expected = (set.area * neutral.area_frac.sqrt()
        + set.centrality * neutral.centrality
        + set.sharpness * neutral.sharpness
        + set.gaze * neutral.gaze
        + set.role * weights.role_weight(Role::FamilyClose))
        / set.total();
    assert!((scored - expected).abs() < 1e-5, "{scored} vs {expected}");
}

#[test]
fn the_importance_slider_moves_the_role_term_only() {
    let weights = ProminenceWeights::embedded();
    let base = ProminenceInput {
        area_frac: 0.02,
        centrality: 0.2,
        sharpness: 0.95,
        gaze: 0.2,
        role: Role::Guest,
        importance: 0.5,
    };
    let raised = ProminenceInput {
        importance: 1.0,
        ..base
    };
    let lowered = ProminenceInput {
        importance: 0.0,
        ..base
    };
    let middle = prominence::prominence(&base, &weights, None);
    assert!(prominence::prominence(&raised, &weights, None) > middle);
    assert!(prominence::prominence(&lowered, &weights, None) < middle);
}

#[test]
fn scene_conditioning_changes_what_matters() {
    // Invariant 7. In `dancing`, area matters less; in `family_formals`, centrality
    // matters more. The tables are written now so phase 07 turns them on by supplying a
    // label, with no change to this code.
    // The claim is about *sensitivity*, not about absolute scores. Comparing one face's
    // score under two tables tells you nothing useful - the tables differ on five terms at
    // once, so a face that happens to be sharp scores higher under `dancing` for a reason
    // that has nothing to do with area. What has to hold is that changing the area moves
    // the score less under `dancing` than it does under the baseline.
    let weights = ProminenceWeights::embedded();
    let sensitivity = |scene: Option<&str>, small: ProminenceInput, large: ProminenceInput| {
        prominence::prominence(&large, &weights, scene)
            - prominence::prominence(&small, &weights, scene)
    };

    let base_face = ProminenceInput {
        area_frac: 0.01,
        centrality: 0.5,
        sharpness: 0.7,
        gaze: 0.5,
        role: Role::Guest,
        importance: 0.5,
    };
    let large = ProminenceInput {
        area_frac: 0.25,
        ..base_face
    };
    assert!(
        sensitivity(Some("dancing"), base_face, large) < sensitivity(None, base_face, large),
        "area must matter less on a dance floor"
    );

    let off_centre = ProminenceInput {
        centrality: 0.05,
        ..base_face
    };
    let central = ProminenceInput {
        centrality: 0.98,
        ..base_face
    };
    assert!(
        sensitivity(Some("family_formals"), off_centre, central)
            > sensitivity(None, off_centre, central),
        "centrality must matter more in a posed group"
    );
}

#[test]
fn an_unknown_scene_falls_back_to_the_baseline() {
    let weights = ProminenceWeights::embedded();
    assert_eq!(weights.for_scene(Some("no_such_scene")), weights.base);
    assert_eq!(weights.for_scene(None), weights.base);
}

#[test]
fn subject_focus_is_weighted_by_who_is_in_the_frame() {
    // The number phases 09 and 12 use instead of global sharpness. A frame that is tack
    // sharp on a stranger and soft on the bride must score low.
    let weights = ProminenceWeights::embedded();
    let bride = IdentityId::new();
    let stranger = IdentityId::new();

    let mut hierarchy = SubjectHierarchy::default();
    hierarchy.primary.push(bride);
    hierarchy.weights.insert(bride, 1.0);
    hierarchy.weights.insert(stranger, 0.3);

    let mut soft_bride = face(Some(bride), 0.14, 0.8, 0.2);
    soft_bride.input.role = Role::Bride;
    let mut sharp_stranger = face(Some(stranger), 0.04, 0.2, 0.98);
    sharp_stranger.input.role = Role::Guest;

    let subjects =
        prominence::image_subjects(&[soft_bride, sharp_stranger], &hierarchy, None, 2, &weights);

    assert_eq!(subjects.dominant, Some(bride));
    assert!(
        subjects.subject_focus_score < 0.6,
        "a frame soft on the bride must not score well: {}",
        subjects.subject_focus_score
    );
    assert_eq!(subjects.weights_ver, weights.version);
    assert_eq!(subjects.people_count, 2);
}

#[test]
fn one_identity_twice_in_a_frame_takes_its_best_appearance() {
    // A mirror, a screen, a portrait on a wall. Not twice as much about her, and the
    // poorer reflection must not drag the direct look down.
    let weights = ProminenceWeights::embedded();
    let bride = IdentityId::new();
    let mut hierarchy = SubjectHierarchy::default();
    hierarchy.weights.insert(bride, 1.0);

    let direct = face(Some(bride), 0.18, 0.85, 0.95);
    let reflection = face(Some(bride), 0.02, 0.1, 0.3);
    let subjects = prominence::image_subjects(&[direct, reflection], &hierarchy, None, 1, &weights);

    let only = prominence::image_subjects(&[direct], &hierarchy, None, 1, &weights);
    assert!(
        (subjects.prominence.get(&bride).copied().unwrap_or(0.0)
            - only.prominence.get(&bride).copied().unwrap_or(0.0))
        .abs()
            < 1e-6,
        "the maximum, not the mean"
    );
}

#[test]
fn a_frame_with_no_faces_has_no_subject_focus() {
    let weights = ProminenceWeights::embedded();
    let subjects = prominence::image_subjects(&[], &SubjectHierarchy::default(), None, 0, &weights);
    assert_eq!(subjects.subject_focus_score, 0.0);
    assert!(subjects.dominant.is_none());
    assert!(subjects.faces.is_empty());
}

#[test]
fn the_hierarchy_is_total_over_the_projects_identities() {
    // A sparse map forces every caller to invent a default, and nine callers will invent
    // four defaults.
    let weights = ProminenceWeights::embedded();
    let people: Vec<(IdentityId, Role, f32)> = vec![
        (IdentityId::new(), Role::Bride, 0.5),
        (IdentityId::new(), Role::Guest, 0.5),
        (IdentityId::new(), Role::Vendor, 0.5),
    ];
    let hierarchy = prominence::subject_hierarchy(&people, 0.8, true, &weights);

    assert_eq!(hierarchy.weights.len(), people.len());
    for (identity, _, _) in &people {
        assert!(hierarchy.weights.contains_key(identity));
    }
    assert_eq!(hierarchy.primary.len(), 1);
    assert!(hierarchy.couple_unconfirmed);
    assert!((hierarchy.coverage - 0.8).abs() < 1e-6);
}

#[test]
fn gaze_follows_the_pose_estimate() {
    let frontal = prominence::gaze_from_pose(Pose {
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
    });
    let turned = prominence::gaze_from_pose(Pose {
        yaw: 70.0,
        pitch: 0.0,
        roll: 0.0,
    });
    assert!(frontal > 0.95);
    assert!(turned < frontal);
}
