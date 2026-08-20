//! PHASE-18 section 10.1, as tests.
//!
//! Every gate below is measured against synthetic frames whose regions were **painted into the
//! pixels** and read back through the real pipeline - phase 10's construction, for the fifth
//! time. That proves the arithmetic recovers something that is genuinely in the buffer.
//!
//! **It says nothing about a wedding photograph.** Section 9's DATA task asks for 12,000
//! labelled wedding frames including veils, ethnic attire and varied skin tones; there are none
//! in this repository and there cannot be. Both shipped heads are placeholders and neither is
//! consulted. That is condition C1 of `docs/progress/PHASE-18-EXIT.md` and it is a Sev 2
//! trigger, and this paragraph is the reason the file says `authored` in every test name that
//! would otherwise read like a claim.

// The panic family is how a test asserts, and three of these assertions are *about* constants:
// that neither head is trained, and that the assignment floor is not zero. A lint that objects
// to asserting a constant is right in general and wrong here - the whole point is that the
// constant is what the gate is checking.
#![allow(clippy::assertions_on_constants, clippy::float_cmp)]

use aura_vision::contract::mask::{
    MaskKind, MaskReason, ALL_KINDS, ASSIGN_MIN_OVERLAP, FACE_SKIN_MIOU, HAIR_MIOU,
    PAYLOAD_BUDGET_BYTES, SUBJECT_MIOU,
};
use aura_vision::mask::fixtures::{self, AuthoredScene, Backdrop, SKIN_REFLECTANCES};
use aura_vision::mask::{algebra, matting, quality, segment, store, trimap};
use aura_vision::mask::{MaskPipeline, MaskSet};

/// Run the real pipeline over an authored scene.
fn run(scene: &AuthoredScene) -> MaskSet {
    MaskPipeline::new().analyse(&scene.frame, Some(&scene.people), &[])
}

/// The measured intersection-over-union for one class, against what was painted.
///
/// **Both sides are thresholded**, which is what mIoU means everywhere it is reported and is
/// what the authored truth deserves: the fixture paints a hard region, and a matte that fades
/// out over the boundary the way a matte is supposed to would be scored as *wrong* by a soft
/// comparison for being soft. How soft the boundary is is `edge_quality`, and it has tests of
/// its own; this is the class-assignment gate.
fn measured_iou(scene: &AuthoredScene, set: &MaskSet, kind: MaskKind) -> f32 {
    let truth = scene.truth_of(kind);
    let got = set.of(kind).map_or_else(
        || algebra::Plane::zeros(truth.w, truth.h),
        |p| p.plane.clone(),
    );
    algebra::iou(
        &algebra::threshold(&truth, 0.5),
        &algebra::threshold(&got, 0.5),
    )
}

// ---------------------------------------------------------------------------
// 1. The heads are placeholders and are never consulted
// ---------------------------------------------------------------------------

#[test]
fn neither_shipped_head_is_trained_or_consulted() {
    assert!(!segment::SEG_HEAD_TRAINED);
    assert!(!matting::MATTING_HEAD_TRAINED);
    let scene = fixtures::one_person(2, Backdrop::Wall);
    let features = segment::Features::measure(&scene.frame);
    assert!(segment::class_hint(&features, 10, 10).is_none());
    let map = trimap::build(&algebra::Plane::zeros(8, 8), 2);
    assert!(matting::alpha_hint(&features, &map).is_none());
}

#[test]
fn every_region_says_the_head_was_not_consulted_or_names_what_seeded_it() {
    // Invariant 2. A region with no reason is a bug, and the phase gate checks the same thing.
    let scene = fixtures::one_person(2, Backdrop::Wall);
    let set = run(&scene);
    for plane in &set.planes {
        assert!(
            !plane.reasons.is_empty(),
            "{} carried no reason",
            plane.kind
        );
    }
}

// ---------------------------------------------------------------------------
// 2. The mIoU gates, on authored pixels
// ---------------------------------------------------------------------------

#[test]
fn authored_skin_and_face_reach_the_face_skin_gate() {
    for reflectance in 0..SKIN_REFLECTANCES.len() {
        let scene = fixtures::one_person(reflectance, Backdrop::Wall);
        let set = run(&scene);
        let skin = measured_iou(&scene, &set, MaskKind::Skin);
        let face = measured_iou(&scene, &set, MaskKind::Face);
        assert!(
            skin >= FACE_SKIN_MIOU,
            "skin at reflectance {reflectance} scored {skin}"
        );
        assert!(
            face >= FACE_SKIN_MIOU,
            "face at reflectance {reflectance} scored {face}"
        );
    }
}

#[test]
fn authored_hair_reaches_the_hair_gate() {
    for reflectance in 0..SKIN_REFLECTANCES.len() {
        let scene = fixtures::one_person(reflectance, Backdrop::Wall);
        let set = run(&scene);
        let hair = measured_iou(&scene, &set, MaskKind::Hair);
        assert!(
            hair >= HAIR_MIOU,
            "hair at reflectance {reflectance} scored {hair}"
        );
    }
}

#[test]
fn authored_subject_reaches_the_subject_gate() {
    for reflectance in 0..SKIN_REFLECTANCES.len() {
        let scene = fixtures::one_person(reflectance, Backdrop::Wall);
        let set = run(&scene);
        let subject = measured_iou(&scene, &set, MaskKind::Subject);
        assert!(
            subject >= SUBJECT_MIOU,
            "subject at reflectance {reflectance} scored {subject}"
        );
    }
}

#[test]
fn the_gates_hold_across_every_authored_reflectance_rather_than_only_the_lightest() {
    // The fairness construction, and the caveat that goes with it: these are five
    // *reflectances*, not five people. `docs/skin-fairness.md` says so in the product's own
    // words. What this proves is that the seed is measured from the frame rather than compared
    // against a constant - a fixed skin chromaticity would score the darkest scene worst, and
    // the spread below would be wide rather than narrow.
    let scores: Vec<f32> = (0..SKIN_REFLECTANCES.len())
        .map(|r| {
            let scene = fixtures::one_person(r, Backdrop::Wall);
            measured_iou(&scene, &run(&scene), MaskKind::Skin)
        })
        .collect();
    let worst = scores.iter().copied().fold(f32::INFINITY, f32::min);
    let best = scores.iter().copied().fold(0.0_f32, f32::max);
    assert!(worst >= FACE_SKIN_MIOU, "worst reflectance scored {worst}");
    assert!(
        best - worst < 0.05,
        "the spread across reflectances was {}",
        best - worst
    );
}

#[test]
fn authored_sky_and_greenery_are_found_outdoors_and_absent_indoors() {
    let garden = fixtures::one_person(2, Backdrop::Garden);
    let garden_set = run(&garden);
    assert!(
        measured_iou(&garden, &garden_set, MaskKind::Sky) >= 0.85,
        "sky scored {}",
        measured_iou(&garden, &garden_set, MaskKind::Sky)
    );
    assert!(
        measured_iou(&garden, &garden_set, MaskKind::Greenery) >= 0.85,
        "greenery scored {}",
        measured_iou(&garden, &garden_set, MaskKind::Greenery)
    );

    let wall = fixtures::one_person(2, Backdrop::Wall);
    let wall_set = run(&wall);
    let sky = wall_set.of(MaskKind::Sky).expect("a sky entry");
    assert!(sky.plane.is_empty(), "a grey wall was called sky");
    assert_eq!(sky.confidence, 0.0);
}

// ---------------------------------------------------------------------------
// 3. Quality gating
// ---------------------------------------------------------------------------

#[test]
fn a_low_contrast_boundary_lowers_edge_quality_rather_than_producing_a_wrong_edge() {
    // Section 10.1's "dark-suit boundaries". The mask still exists; what changes is what it is
    // allowed to carry.
    let easy = fixtures::one_person(2, Backdrop::Wall);
    let hard = fixtures::one_person(2, Backdrop::LowContrast);
    let easy_edge = run(&easy)
        .of(MaskKind::Subject)
        .map_or(0.0, |p| p.edge_quality);
    let hard_edge = run(&hard)
        .of(MaskKind::Subject)
        .map_or(0.0, |p| p.edge_quality);
    assert!(
        hard_edge < easy_edge,
        "low contrast scored {hard_edge}, wall scored {easy_edge}"
    );
}

#[test]
fn a_low_quality_mask_blocks_an_aggressive_operation_and_still_carries_a_local_tone_move() {
    // Section 10.1: "low-confidence masks demonstrably block aggressive downstream operations".
    // This is the phase 20 stub the phase document asks for: `quality::Operation` is the whole
    // interface, and there is nothing else for a later phase to consult.
    let scene = fixtures::one_person(2, Backdrop::Wall);
    let set = run(&scene);
    let mut plane = set.of(MaskKind::Skin).cloned().expect("a skin plane");
    plane.edge_quality = 0.05;
    quality::settle(&mut plane);
    let (mask, _) = aura_vision::mask::to_mask(scene.image_id, &plane, 0.0);

    let smooth = quality::allowance(&mask, quality::Operation::SkinSmooth);
    assert!(!smooth.permitted, "a bad mask was allowed to smooth skin");
    assert_eq!(smooth.ceiling, 0.0);
    assert!(smooth.note.is_some(), "no reason was recorded");

    let tone = quality::allowance(&mask, quality::Operation::LocalTone);
    assert!(tone.permitted, "a bad mask blocked a local tone move");
    assert!(tone.ceiling > 0.0 && tone.ceiling < 1.0);
}

#[test]
fn a_frame_with_nobody_in_it_produces_no_person_regions_rather_than_guesses() {
    let scene = fixtures::no_people();
    let set = run(&scene);
    for kind in [MaskKind::Skin, MaskKind::Face, MaskKind::Hair] {
        let plane = set.of(kind).expect("an entry for every class");
        assert!(plane.plane.is_empty(), "{kind} was invented with no face");
        assert_eq!(plane.confidence, 0.0);
        assert!(plane.reasons.contains(&MaskReason::NoFaces));
    }
    // The subject falls back rather than vanishing, because the details and the flat-lays are a
    // fifth of a wedding - and the fallback cannot carry an aggressive operation.
    let subject = set.of(MaskKind::Subject).expect("a subject");
    assert!(subject.reasons.contains(&MaskReason::NoFaces));
    assert!(subject.allowance() < aura_vision::contract::mask::AGGRESSIVE_FLOOR);
}

// ---------------------------------------------------------------------------
// 4. Instance scoping
// ---------------------------------------------------------------------------

#[test]
fn per_identity_skin_does_not_bleed_between_adjacent_people() {
    // Section 10.1, and the failure ADR-0037 decision 9 exists to prevent.
    let scene = fixtures::two_people();
    let bride = aura_core::IdentityId::new();
    let guest = aura_core::IdentityId::new();
    let set =
        MaskPipeline::new().analyse(&scene.frame, Some(&scene.people), &[(0, bride), (1, guest)]);

    let hers = set
        .planes
        .iter()
        .find(|p| p.kind == MaskKind::Skin && p.identity == Some(bride))
        .expect("the first person's skin");
    let theirs = set
        .planes
        .iter()
        .find(|p| p.kind == MaskKind::Skin && p.identity == Some(guest))
        .expect("the second person's skin");

    let overlap = algebra::iou(&hers.plane, &theirs.plane);
    assert!(
        overlap < 0.01,
        "the two identity-scoped skin masks overlapped at {overlap}"
    );

    // And each of them sits inside its own person's box.
    let first_box = scene.people.persons.first().expect("a first body").bbox;
    let mut inside = 0.0_f64;
    let mut total = 0.0_f64;
    for y in 0..hers.plane.h {
        for x in 0..hers.plane.w {
            let a = hers.plane.at(i64::from(x), i64::from(y));
            if a <= 0.0 {
                continue;
            }
            total += f64::from(a);
            let nx = f32::from(x as u16) / hers.plane.w as f32;
            if nx >= first_box.x && nx <= first_box.x + first_box.w {
                inside += f64::from(a);
            }
        }
    }
    assert!(total > 0.0, "the first person had no scoped skin at all");
    assert!(
        inside / total > 0.95,
        "only {:.2} of her skin was inside her own box",
        inside / total
    );
}

#[test]
fn the_unscoped_region_survives_beside_the_scoped_ones() {
    // Phase 16's skin guard wants "all the skin in this frame" and phase 25 wants "hers". Both
    // are rows; neither is a query over the other.
    let scene = fixtures::two_people();
    let bride = aura_core::IdentityId::new();
    let set = MaskPipeline::new().analyse(&scene.frame, Some(&scene.people), &[(0, bride)]);
    assert!(
        set.of(MaskKind::Skin).is_some(),
        "the unscoped skin vanished"
    );
    assert!(
        set.all_of(MaskKind::Skin).len() >= 2,
        "no scoped skin was produced"
    );
}

#[test]
fn the_assignment_floor_is_a_containment_test_rather_than_a_nearest_box() {
    assert!(
        ASSIGN_MIN_OVERLAP > 0.0,
        "an overlap floor of zero is a nearest-box assignment"
    );
}

// ---------------------------------------------------------------------------
// 5. Storage
// ---------------------------------------------------------------------------

#[test]
fn every_class_of_one_authored_frame_fits_the_payload_budget() {
    // Section 11's third row, and section 12's third failure mode as a test.
    let scene = fixtures::one_person(2, Backdrop::Garden);
    let set = run(&scene);
    let mut total = 0_usize;
    for plane in &set.planes {
        let (payload, _) = store::encode(plane.kind, &plane.plane);
        total += payload.byte_len();
    }
    assert!(
        total <= PAYLOAD_BUDGET_BYTES,
        "{total} bytes against a budget of {PAYLOAD_BUDGET_BYTES}"
    );
}

#[test]
fn a_thousand_authored_frames_stay_inside_the_gallery_budget() {
    // The same figure scaled to section 10.1's "all masks for a 1,000-image gallery stay within
    // budget". Measured once and multiplied rather than looped a thousand times, because the
    // per-frame figure is what the budget is written against and a thousand identical frames
    // would measure the same number a thousand times.
    let scene = fixtures::one_person(2, Backdrop::Garden);
    let set = run(&scene);
    let per_frame: usize = set
        .planes
        .iter()
        .map(|p| store::encode(p.kind, &p.plane).0.byte_len())
        .sum();
    let gallery = per_frame * 1_000;
    assert!(
        gallery <= PAYLOAD_BUDGET_BYTES * 1_000,
        "{gallery} bytes for a thousand frames"
    );
}

#[test]
fn every_stored_payload_round_trips_through_the_codec() {
    let scene = fixtures::one_person(3, Backdrop::Garden);
    let set = run(&scene);
    for plane in &set.planes {
        let (payload, _) = store::encode(plane.kind, &plane.plane);
        let back = store::decode(&payload);
        if plane.plane.is_empty() {
            assert!(back.is_empty(), "{} came back non-empty", plane.kind);
            continue;
        }
        if matches!(
            plane.kind.stored_as(),
            aura_vision::contract::mask::Storage::Rle
        ) {
            // A run length stores the *thresholded* region, so the round trip is exact against
            // that and lossy against the soft plane. Asserting the first is the accurate
            // statement; asserting the second at a tolerance would be measuring how soft the
            // fixture happens to be.
            let hard = algebra::threshold(&plane.plane, 0.5);
            assert_eq!(back, hard, "{} did not round-trip exactly", plane.kind);
            // And the cost of the hard form is bounded, which is the number that decides
            // whether a class deserves alpha. Everything above `SOFTNESS_FLOOR` keeps its
            // boundary; the four that do not are the four stored as alpha.
            let softness = algebra::iou(&plane.plane, &back);
            assert!(
                softness >= 0.9,
                "{} lost {:.3} of itself to the hard form",
                plane.kind,
                1.0 - softness
            );
        } else {
            let score = algebra::iou(&plane.plane, &back);
            assert!(score >= 0.85, "{} round-tripped at {score}", plane.kind);
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Determinism and the vocabulary
// ---------------------------------------------------------------------------

#[test]
fn two_runs_over_one_frame_produce_identical_regions() {
    // Invariant 4. Nothing in this phase reads a clock, a map iteration order or a random seed,
    // and this is what says so.
    let scene = fixtures::one_person(2, Backdrop::Garden);
    let first = run(&scene);
    let second = run(&scene);
    assert_eq!(first.planes.len(), second.planes.len());
    for (a, b) in first.planes.iter().zip(second.planes.iter()) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.plane, b.plane, "{} differed between runs", a.kind);
        assert_eq!(a.confidence, b.confidence);
        assert_eq!(a.edge_quality, b.edge_quality);
    }
}

#[test]
fn every_class_in_the_frozen_vocabulary_gets_a_row() {
    // A class that is absent from the output is a class a later phase would read as "not in
    // this photograph" without ever finding out that nobody looked.
    let scene = fixtures::one_person(2, Backdrop::Wall);
    let set = run(&scene);
    for kind in ALL_KINDS {
        assert!(set.of(kind).is_some(), "{kind} produced no region at all");
    }
}

#[test]
fn the_iteration_order_is_the_frozen_one_rather_than_the_alphabet() {
    let scene = fixtures::one_person(2, Backdrop::Wall);
    let set = run(&scene);
    let order: Vec<MaskKind> = set
        .planes
        .iter()
        .filter(|p| p.identity.is_none())
        .map(|p| p.kind)
        .collect();
    let mut expected: Vec<MaskKind> = ALL_KINDS.to_vec();
    expected.retain(|k| order.contains(k));
    assert_eq!(order, expected);
}

#[test]
fn the_alpha_classes_are_the_four_whose_boundary_is_the_point() {
    use aura_vision::contract::mask::Storage;
    let alpha: Vec<MaskKind> = ALL_KINDS
        .into_iter()
        .filter(|k| matches!(k.stored_as(), Storage::Alpha))
        .collect();
    assert_eq!(
        alpha,
        vec![
            MaskKind::Skin,
            MaskKind::Face,
            MaskKind::Hair,
            MaskKind::Subject
        ]
    );
}

// ---------------------------------------------------------------------------
// 7. The algebra later phases are written against
// ---------------------------------------------------------------------------

#[test]
fn mask_minus_skin_removes_exactly_the_skin() {
    let scene = fixtures::one_person(2, Backdrop::Wall);
    let set = run(&scene);
    let subject = set.of(MaskKind::Subject).expect("a subject").plane.clone();
    let skin = set.of(MaskKind::Skin).expect("skin").plane.clone();
    let without = algebra::subtract(&subject, &skin);
    assert!(
        algebra::iou(&without, &skin) < 0.02,
        "skin survived the subtraction"
    );
    assert!(
        without.coverage() > 0.0,
        "the subtraction removed everything"
    );
}

#[test]
fn the_skin_safe_zone_contains_the_skin_it_protects() {
    // Phase 16's guard is the caller. A zone that did not strictly contain the skin would let
    // clarity move the pixel next to somebody's cheek and call the guarantee kept.
    let scene = fixtures::one_person(2, Backdrop::Wall);
    let set = run(&scene);
    let skin = set.of(MaskKind::Skin).expect("skin").plane.clone();
    let safe = set
        .of(MaskKind::SkinSafe)
        .expect("a safe zone")
        .plane
        .clone();
    let outside = algebra::subtract(&skin, &safe);
    assert!(
        outside.coverage() < 0.001,
        "{} of the skin sat outside the safe zone",
        outside.coverage()
    );
    assert!(
        safe.coverage() > skin.coverage(),
        "the safe zone was not grown at all"
    );
}
