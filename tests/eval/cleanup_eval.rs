//! PHASE-24 section 10.1, the safety half. Run as an ordinary test, so a red gate is a red build.
//!
//! Section 8 orders this phase literally: publish the policy, then **implement the safety engine
//! first, with tests, before any removal code exists**. This file is those tests, and it is
//! deliberately written against the engine rather than against the removals - a safety gate that
//! could only be measured once removals existed would be a gate that arrived after the thing it
//! guards.
//!
//! ## What these gates prove, and what they do not
//!
//! They prove that the five checks of section 6.2 cannot be bypassed by any path through the
//! engine, that an absent mask is treated as ignorance rather than as safety, and that the shipped
//! policy table cannot be edited into something laxer than the contract.
//!
//! They prove **nothing about a real photograph**, because there are none here and there is no
//! trained detector. Every fixture below is authored: the regions were chosen, the masks were
//! placed, and the answer is known by construction. Conditions C1 and C2 of the exit report.
//!
//! The gates that need removals - artefact-free rate, sibling preference, disclosure completeness,
//! the adversarial audit - arrive with the modules they measure and are marked below.

use aura_core::contract::cleanup::{
    CleanupCode, CleanupProposal, DistractionClass, SafetyCheck, SafetyVerdict, AREA_CAP_DEFAULT,
    DENYLIST_OVERLAP_MAX, MAX_PROPOSALS_PER_IMAGE, ZERO_TOUCH_CONFIDENCE,
};
use aura_core::contract::ids::ProposalId;
use aura_core::contract::integrity::CropRect;
use aura_core::contract::scene::ImageId;
use aura_core::contract::scene::SceneId;
use aura_generative::denylist::{Coverage, Protected};
use aura_generative::detect::{self, Frame};
use aura_generative::policy::{Policy, ScenePolicy};
use aura_generative::safety::{self, Candidate, Outcome};

type Box2 = CropRect;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Box2 {
    Box2 { x, y, w, h }
}

fn permissive() -> ScenePolicy {
    ScenePolicy {
        area_cap: AREA_CAP_DEFAULT,
        denylist_overlap_max: DENYLIST_OVERLAP_MAX,
        zero_touch_confidence: ZERO_TOUCH_CONFIDENCE,
        enabled: true,
        reason: "the most permissive row the contract allows, for an adversarial sweep".into(),
    }
}

fn clutter(region: Box2) -> Candidate {
    Candidate {
        region,
        class: DistractionClass::Bin,
        salience: 0.9,
        removability: 0.95,
        crosses_structure: false,
        touches_identity: false,
    }
}

// -------------------------------------------------------------------------------------------
// Gate 1. No proposal overlapping a face, a hand, a dress, a ring or cake is ever allowed.
// Section 10.1's first row, and the hard one.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_1_nothing_overlapping_protected_content_is_ever_allowed() {
    // An exhaustive sweep: every protected kind, at every position on a grid, against a candidate
    // that is otherwise perfect - small, confident, at the frame edge, in the most permissive
    // scene row the contract permits.
    let policy = permissive();
    let mut examined = 0usize;
    let mut blocked = 0usize;

    for kind in Protected::ALL {
        for gx in 0..10 {
            for gy in 0..10 {
                let region = rect(gx as f32 * 0.1, gy as f32 * 0.1, 0.06, 0.06);
                // The protected region sits exactly on the candidate.
                let coverage = Coverage::Known(vec![(kind, region)]);
                let outcome = safety::check(&clutter(region), &policy, &coverage);
                examined += 1;
                match outcome {
                    Outcome::Blocked { check, code, .. } => {
                        assert_eq!(check, SafetyCheck::Denylist);
                        assert_eq!(code, CleanupCode::OverlapsProtected);
                        blocked += 1;
                    }
                    Outcome::Allowed(_) => panic!(
                        "a candidate wholly inside {} at ({gx}, {gy}) was allowed",
                        kind.as_str()
                    ),
                }
            }
        }
    }

    assert_eq!(examined, 600);
    assert_eq!(blocked, examined, "every one of them must be blocked");
}

#[test]
fn gate_1b_a_partial_overlap_above_the_threshold_is_also_blocked() {
    let policy = permissive();
    // Slide a hand *onto* a fixed candidate, from just clear of it to wholly covering it, so the
    // overlap rises monotonically from zero to one. The answer must flip from allowed to blocked
    // exactly once and never flip back.
    //
    // The range deliberately stops where the hand fully covers the candidate rather than
    // continuing across it: a hand that has slid off the far side is genuinely clear again, and a
    // test that swept the whole way would be asserting that overlap is monotone in x, which it is
    // not - it rises and falls.
    let candidate = clutter(rect(0.40, 0.40, 0.10, 0.10));
    let mut first_block: Option<usize> = None;
    for step in 0..=100 {
        let x = 0.30 + (step as f32) * 0.001; // 0.30 .. 0.40
        let coverage = Coverage::Known(vec![(Protected::Hands, rect(x, 0.40, 0.10, 0.10))]);
        let allowed = safety::check(&candidate, &policy, &coverage).is_allowed();
        match (first_block, allowed) {
            (None, false) => first_block = Some(step),
            (Some(at), true) => {
                panic!("the answer flipped back to allowed at step {step} after blocking at {at}")
            }
            _ => {}
        }
    }
    let at = first_block.expect("sliding a hand onto the candidate must block it");
    assert!(
        at > 0,
        "a hand that is clear of the candidate must not block it"
    );
}

// -------------------------------------------------------------------------------------------
// Gate 2. The size cap and the structure check cannot be bypassed by any code path.
// Section 10.1's second row, as property tests.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_2_no_region_above_the_cap_is_ever_allowed_in_any_scene() {
    let table = Policy::shipped().expect("the shipped policy table must load");
    for scene in SceneId::ALL {
        let row = table
            .scene(scene)
            .unwrap_or_else(|| panic!("{} has no policy row", scene.as_str()));
        // A region one part in a thousand above this scene's own cap.
        let side = (row.area_cap + 0.001).sqrt().min(1.0);
        let candidate = clutter(rect(0.0, 0.0, side, side));
        let outcome = safety::check(&candidate, row, &Coverage::known_empty());
        assert_eq!(
            outcome.blocked_by(),
            Some(SafetyCheck::SizeCap),
            "{} allowed a region above its own cap",
            scene.as_str()
        );
    }
}

#[test]
fn gate_2b_a_region_crossing_structure_is_never_allowed() {
    let policy = permissive();
    for step in 0..50 {
        let y = (step as f32) * 0.02;
        let mut candidate = clutter(rect(0.02, y, 0.04, 0.04));
        candidate.crosses_structure = true;
        assert_eq!(
            safety::check(&candidate, &policy, &Coverage::known_empty()).blocked_by(),
            Some(SafetyCheck::StructureSpan),
            "a region crossing structure at y={y} was not blocked"
        );
    }
}

// -------------------------------------------------------------------------------------------
// Gate 3. A person is never removed automatically, and an unknown object is never removed.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_3_a_person_is_never_proposed_at_any_size_confidence_or_scene() {
    let table = Policy::shipped().expect("loads");
    for scene in SceneId::ALL {
        let row = table.scene(scene).expect("row");
        for size in [0.001_f32, 0.005, 0.01, 0.02] {
            let mut candidate = clutter(rect(0.01, 0.01, size.sqrt(), size.sqrt()));
            candidate.class = DistractionClass::BackgroundPerson;
            candidate.removability = 1.0;
            candidate.salience = 1.0;
            let outcome = safety::check(&candidate, row, &Coverage::known_empty());
            assert!(
                !outcome.is_allowed(),
                "{} allowed a person at area {size}",
                scene.as_str()
            );
        }
    }
}

#[test]
fn gate_3b_an_unclassified_object_is_never_proposed() {
    let policy = permissive();
    let mut candidate = clutter(rect(0.01, 0.90, 0.03, 0.03));
    candidate.class = DistractionClass::Unclassified;
    candidate.removability = 1.0;
    match safety::check(&candidate, &policy, &Coverage::known_empty()) {
        Outcome::Blocked { code, .. } => assert_eq!(code, CleanupCode::ClassUnknown),
        Outcome::Allowed(_) => panic!("an unknown class cannot be shown to be extraneous"),
    }
}

// -------------------------------------------------------------------------------------------
// Gate 4. An absent mask is ignorance, not safety. ADR-0049 section 3.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_4_an_absent_mask_blocks_everywhere_and_says_so_distinctly() {
    let policy = permissive();
    for gx in 0..10 {
        for gy in 0..10 {
            let region = rect(gx as f32 * 0.1, gy as f32 * 0.1, 0.05, 0.05);
            match safety::check(&clutter(region), &policy, &Coverage::Absent) {
                Outcome::Blocked { check, code, .. } => {
                    assert_eq!(check, SafetyCheck::Denylist);
                    // Distinct from OverlapsProtected: one is a claim about the photograph, the
                    // other is the absence of one.
                    assert_eq!(code, CleanupCode::ProtectionUnknown);
                }
                Outcome::Allowed(_) => {
                    panic!("an absent mask allowed a removal at ({gx}, {gy})")
                }
            }
        }
    }
}

#[test]
fn gate_4b_this_build_proposes_nothing_on_a_frame_with_no_masks() {
    // The consequence of gate 4 stated as the product behaviour it produces, so that a future
    // change which starts proposing removals without masks fails a test that says what it cost.
    let table = Policy::shipped().expect("loads");
    let frame = Frame {
        salient: vec![
            (rect(0.02, 0.90, 0.04, 0.04), 0.9),
            (rect(0.94, 0.05, 0.04, 0.04), 0.8),
        ],
        subjects: vec![rect(0.40, 0.30, 0.20, 0.40)],
        ..Frame::default()
    };
    let row = table.scene(SceneId::CouplePortrait).expect("row");
    let candidates = detect::candidates(&frame);
    assert!(
        !candidates.is_empty(),
        "the fixture must produce candidates"
    );
    for candidate in candidates {
        assert!(
            !safety::check(&candidate, row, &Coverage::Absent).is_allowed(),
            "this build must propose nothing while phase 18's masks are absent"
        );
    }
}

// -------------------------------------------------------------------------------------------
// Gate 5. The policy file may only make the product stricter.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_5_the_shipped_table_covers_every_scene_and_stays_inside_the_contract() {
    let table = Policy::shipped().expect("the shipped policy table must load");
    assert_eq!(table.len(), SceneId::ALL.len());
    for scene in SceneId::ALL {
        let row = table
            .scene(scene)
            .unwrap_or_else(|| panic!("{} has no policy row", scene.as_str()));
        assert!(row.area_cap <= AREA_CAP_DEFAULT);
        assert!(row.denylist_overlap_max <= DENYLIST_OVERLAP_MAX);
        assert!(row.zero_touch_confidence >= ZERO_TOUCH_CONFIDENCE);
        assert!(
            !row.reason.trim().is_empty(),
            "{} has no written reason",
            scene.as_str()
        );
    }
}

#[test]
fn gate_5b_the_scenes_that_are_the_wedding_have_cleanup_switched_off() {
    // The argument of `cleanup_policy.toml` as a test: in the scenes where nearly everything in
    // frame is part of the story, nothing is removed automatically. A future edit that switches
    // one of these on has to change this list and say why.
    let table = Policy::shipped().expect("loads");
    for scene in [
        SceneId::Ceremony,
        SceneId::Ritual,
        SceneId::Vows,
        SceneId::Rings,
        SceneId::Kiss,
        SceneId::Cake,
        SceneId::Details,
        SceneId::Unknown,
    ] {
        let row = table.scene(scene).expect("row");
        assert!(
            !row.enabled && row.area_cap == 0.0,
            "{} must not be cleaned up automatically",
            scene.as_str()
        );
    }
}

// -------------------------------------------------------------------------------------------
// Gate 6. A proposal cannot exist without a passing verdict. The ordering as a type property.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_6_a_blocked_candidate_cannot_become_a_proposal() {
    let blocked = SafetyVerdict::block(SafetyCheck::Denylist, "her hand is in it");
    let err = CleanupProposal::new(
        ProposalId::new(),
        ImageId::new(),
        rect(0.1, 0.1, 0.05, 0.05),
        DistractionClass::Bin,
        aura_core::contract::cleanup::CleanupMethod::ClassicalFill,
        blocked,
        vec![aura_core::contract::cleanup::CleanupReason::plain(
            CleanupCode::OverlapsProtected,
            -1.0,
        )],
    )
    .expect_err("a blocked candidate must never become a proposal");
    assert_eq!(err.code.0, "AURA-ML-5116");
}

#[test]
fn gate_6b_a_malformed_verdict_cannot_become_a_proposal() {
    // A verdict that claims `allowed` while carrying a failed check is the one row that would
    // make the whole audit meaningless.
    let mut lying = SafetyVerdict::allow();
    lying.checks = vec![(SafetyCheck::SizeCap, false)];
    let err = CleanupProposal::new(
        ProposalId::new(),
        ImageId::new(),
        rect(0.1, 0.1, 0.05, 0.05),
        DistractionClass::Bin,
        aura_core::contract::cleanup::CleanupMethod::ClassicalFill,
        lying,
        vec![aura_core::contract::cleanup::CleanupReason::plain(
            CleanupCode::UnexplainedSalience,
            1.0,
        )],
    )
    .expect_err("a self-inconsistent verdict must be refused");
    assert_eq!(err.code.0, "AURA-ML-5116");
}

#[test]
fn gate_6c_a_proposal_with_no_reasons_is_refused() {
    let err = CleanupProposal::new(
        ProposalId::new(),
        ImageId::new(),
        rect(0.1, 0.1, 0.05, 0.05),
        DistractionClass::Bin,
        aura_core::contract::cleanup::CleanupMethod::ClassicalFill,
        SafetyVerdict::allow(),
        vec![],
    )
    .expect_err("invariant 2");
    assert_eq!(err.code.0, "AURA-ML-5116");
}

// -------------------------------------------------------------------------------------------
// Gate 7. Nothing in this build applies unattended.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_7_nothing_this_build_produces_may_apply_unattended() {
    let proposal = CleanupProposal::new(
        ProposalId::new(),
        ImageId::new(),
        rect(0.02, 0.90, 0.03, 0.03),
        DistractionClass::Bin,
        aura_core::contract::cleanup::CleanupMethod::ClassicalFill,
        SafetyVerdict::allow(),
        vec![aura_core::contract::cleanup::CleanupReason::plain(
            CleanupCode::UnexplainedSalience,
            1.0,
        )],
    )
    .expect("a sound proposal");
    // A freshly constructed proposal is `RequireReview` and carries no confidence, so all three
    // conditions fail. Phase 13's `uncalibrated_raises` keeps it that way even once a caller
    // fills the band in, because nothing in this build is calibrated.
    assert!(!proposal.may_apply_unattended());
    assert!(proposal.broken_guarantee().is_none());
}

// -------------------------------------------------------------------------------------------
// Gate 8. The detector stays a light touch and names nothing.
// -------------------------------------------------------------------------------------------

#[test]
fn gate_8_the_detector_is_capped_and_names_nothing() {
    let salient = (0..20)
        .map(|i| (rect(0.01, 0.02 + (i as f32) * 0.045, 0.03, 0.03), 0.9))
        .collect();
    let frame = Frame {
        salient,
        subjects: vec![rect(0.45, 0.40, 0.15, 0.25)],
        ..Frame::default()
    };
    let found = detect::candidates(&frame);
    assert!(found.len() <= MAX_PROPOSALS_PER_IMAGE);
    for candidate in &found {
        assert_eq!(candidate.class, DistractionClass::Unclassified);
    }
}

#[test]
fn gate_8b_detection_is_deterministic() {
    // Invariant 4. Two runs over the same frame produce the same candidates in the same order.
    let frame = Frame {
        salient: vec![
            (rect(0.30, 0.02, 0.04, 0.04), 0.6),
            (rect(0.10, 0.02, 0.04, 0.04), 0.6),
            (rect(0.02, 0.90, 0.04, 0.04), 0.6),
        ],
        subjects: vec![rect(0.45, 0.45, 0.10, 0.10)],
        ..Frame::default()
    };
    assert_eq!(detect::candidates(&frame), detect::candidates(&frame));
}

// -------------------------------------------------------------------------------------------
// What is not measured here yet, named so a missing gate cannot look like a passing one.
// -------------------------------------------------------------------------------------------

#[test]
fn the_gates_that_need_removals_are_named_rather_than_silently_absent() {
    // Section 10.1 has seven rows. Four of them are measured above. These three arrive with the
    // modules they measure, and this test exists so that reading the harness tells you which.
    let pending = [
        "artefact-free rate >= 98 % on approved removals (needs selfcheck + the renderer)",
        "sibling borrowing is preferred whenever available (needs borrow + fill)",
        "every applied cleanup appears in the recipe, the ledger and the delivery report \
         (needs the store and migration 24)",
    ];
    assert_eq!(pending.len(), 3);
    for row in pending {
        assert!(!row.is_empty());
    }
}
