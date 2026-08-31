//! The phase 26 evaluation gates. Section 10.1, as a test, so a red gate is a red build.
//!
//! Seven gates:
//!
//! 1. Cross-camera skin dE00 at or below 2.0 in matched scenes after the transform.
//! 2. Grade-signature distance between cameras reduced by at least 65 %.
//! 3. Held-out verification: a transform improves evidence it never saw, or the baseline is used.
//! 4. Flash and ambient populations receive distinct transforms.
//! 5. Ordering: camera transforms always precede within-scene normalisation.
//! 6. With no matched pairs, brand baselines are used and the report says so honestly.
//! 7. No camera exceeds the documented maximum movement on any axis.
//!
//! # What these gates do not prove
//!
//! **Every one of them is measured on a synthetic wedding whose per-brand colour response was
//! authored.** There are no multi-camera weddings in this repository, no measured body and no
//! photographed target: section 9's DATA row asks for Sony+Canon, Canon+Nikon and Fujifilm fixtures
//! with matched scenes and there are none. What is proved here is the fingerprint, the pairing, the
//! background verification, the appearance metric, the solver, the bounds, the held-out check, the
//! blend, the shooter cap and the ordering - the *algorithms*. Not one number here is a claim about
//! a wedding.
//!
//! That is condition C1 of `docs/progress/PHASE-26-EXIT.md`, it is a Sev 2 trigger, and it closes
//! with phase 05's condition C10 rather than separately: the pairing pre-filter reads the
//! placeholder embedding.
//!
//! Gate 1 has a second caveat of its own. `SKIN_FIELD_AVAILABLE` is false in phase 25, so no
//! photograph in this build carries an identity-scoped skin region; the fixture authors one. It is
//! a measurement of the mechanism on a *chromaticity offset*, not on a person, and
//! `docs/skin-fairness.md` says the same thing in the product's own words.
//!
//! And every bundled brand baseline was **fabricated**. Gate 6 proves that the fallback path runs
//! and reports itself honestly; it proves nothing about whether the numbers it falls back on are
//! right. That is condition C2, and the first measured baseline reopens this phase's criteria
//! whatever phase is in flight.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args, clippy::assertions_on_constants)]

use std::collections::BTreeMap;

use aura_brain_gallery::camera::baseline::Library;
use aura_brain_gallery::camera::fixtures::{self, Body, Shape};
use aura_brain_gallery::camera::policy::Matching;
use aura_brain_gallery::camera::{fingerprint, pairs, solve, transform, CameraFrame, Field};
use aura_core::contract::camera::{
    CameraCode, CameraTransform, FlashState, TransformSource, CROSS_CAMERA_DE00_CEILING,
    MAX_CHANNEL_GAIN, MAX_CONTRAST_SHAPE, MAX_SHOOTER_EV, MAX_T_CCT_K, MAX_T_EXPOSURE_EV,
    MAX_T_SATURATION, MAX_T_TINT, SIGNATURE_REDUCTION_TARGET,
};
use aura_core::contract::moment::CameraId;

/// Section 10.1's second gate.
const SIGNATURE_REDUCTION: f32 = SIGNATURE_REDUCTION_TARGET;

/// One body solved against the reference, end to end through the real modules.
struct Solved {
    transform: CameraTransform,
    before: aura_core::contract::camera::AppearanceDistance,
    after: aura_core::contract::camera::AppearanceDistance,
    verified_pairs: u32,
    heldout_pairs: u32,
    verdict: Option<bool>,
}

/// Run the pairing, the fit, the held-out check and the blend, exactly as the pass does.
fn solve_body(frames: &[CameraFrame], reference: &str, body: &str, flash: FlashState) -> Solved {
    let policy = Matching::default();
    let library = Library::bundled();
    let reference_id = CameraId::new(reference);
    let body_id = CameraId::new(body);

    let mut found = pairs::find(frames, &reference_id, &body_id, &policy);
    pairs::split_heldout(&mut found);

    let by_image: BTreeMap<_, _> = frames.iter().map(|frame| (frame.image, frame)).collect();
    let mut fitting = Vec::new();
    let mut heldout = Vec::new();
    for pair in found.iter().filter(|pair| pair.flash == flash) {
        let (Some(left), Some(right)) = (by_image.get(&pair.left), by_image.get(&pair.right))
        else {
            continue;
        };
        let Some(reading) = transform::PairReading::of(left, right) else {
            continue;
        };
        if pair.is_heldout() {
            heldout.push(reading);
        } else if pair.is_fitting() {
            fitting.push(reading);
        }
    }

    let verified_pairs = u32::try_from(fitting.len()).unwrap_or(0);
    let heldout_pairs = u32::try_from(heldout.len()).unwrap_or(0);

    let Some(fit) = solve::fit(&body_id, flash, &reference_id, &fitting, &[], &policy) else {
        let mut out = solve::from_baseline(
            &body_id,
            flash,
            &reference_id,
            Body::REFERENCE.brand,
            Body::SECOND.brand,
            &library,
            &policy,
        );
        out.evidence_pairs = 0;
        let before = transform::measure(&fitting, None);
        return Solved {
            transform: out,
            before,
            after: before,
            verified_pairs,
            heldout_pairs,
            verdict: None,
        };
    };

    let (_, _, verdict) = solve::verify(&fit, &heldout);
    let mut out = fit.transform.clone();
    out.evidence_pairs = verified_pairs;
    out.skin_correction = solve::skin_report(&fit.transform, fit.before, fit.after);
    out.bounded = fit.bounded;
    let weight = solve::evidence_weight(verified_pairs, &policy);
    let (departure, _) = aura_brain_gallery::camera::baseline::between(
        &library,
        Body::SECOND.brand,
        Body::REFERENCE.brand,
        flash,
    );
    solve::blend(&mut out, departure, weight);

    Solved {
        transform: out,
        before: fit.before,
        after: transform::measure(&fitting, Some(&fit.transform)),
        verified_pairs,
        heldout_pairs,
        verdict,
    }
}

/// A two-camera wedding with enough overlap to solve from.
fn overlapping_wedding() -> Vec<CameraFrame> {
    fixtures::wedding(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 4,
            per_node: 14,
            ..Shape::default()
        },
    )
}

#[test]
fn gate_1_cross_camera_skin_is_at_or_below_two_de00_in_matched_scenes() {
    let frames = overlapping_wedding();
    let solved = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    assert!(
        solved.verified_pairs >= 12,
        "the fixture must supply real evidence; {} verified pairs",
        solved.verified_pairs
    );
    assert!(
        solved.before.skin_de00 > CROSS_CAMERA_DE00_CEILING,
        "the fixture must start outside the promise, or the gate proves nothing; before {:.2}",
        solved.before.skin_de00
    );
    assert!(
        solved.after.skin_de00 <= CROSS_CAMERA_DE00_CEILING,
        "cross-camera skin is {:.2} dE00 after matching, above the {:.1} promised",
        solved.after.skin_de00,
        CROSS_CAMERA_DE00_CEILING
    );
    println!(
        "gate 1: skin {:.2} -> {:.2} dE00 on {} pairs",
        solved.before.skin_de00, solved.after.skin_de00, solved.verified_pairs
    );
}

#[test]
fn gate_1b_white_points_converge_as_well_as_skin() {
    // Section 10.1 names both halves - "cross-camera skin dE00 <= 2.0 in matched scenes after
    // transform; white points converge" - and they can come apart: a skin correction is its own
    // axis, so a solver could satisfy the first by moving skin alone.
    let frames = overlapping_wedding();
    let solved = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    let mixed_light = 1.0 - solved.after.white_point / solved.before.white_point;
    assert!(
        mixed_light >= 0.45,
        "white points converged only {:.0} %: {:.3} -> {:.3}",
        mixed_light * 100.0,
        solved.before.white_point,
        solved.after.white_point
    );

    // And the diagnostic half, which says *why* neither number is ninety-something. Two effects,
    // and both are properties of the phase rather than defects in it.
    //
    // **The temperature axis is in kelvin and the metric is in `u'v'`, and the two are not
    // proportional.** A fixed 420 K difference is a much larger chromaticity gap at 3,200 K than at
    // 6,400 K, so one number cannot close both - which is why the mixed-light figure is lower than
    // the single-room one. The residual is per-room, and per-room is exactly what phase 25
    // normalises away node by node; that is the whole reason the two phases compose in this order.
    //
    // **Skin carries twice the weight of the white point.** Section 6.2 weights them 3 and 1.5, and
    // the two are corrected through the same locus walk at different apparent temperatures - skin
    // sits off the illuminant, so a single `d_cct` moves the two by different amounts. The optimum
    // therefore trades a little white point for a little skin, deliberately and in the direction
    // the product manager chose. A gate that demanded near-perfect white-point convergence would be
    // a gate demanding the solver ignore its own objective.
    let one_room = fixtures::wedding(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 4,
            per_node: 14,
            distinct_rooms: 1,
            ..Shape::default()
        },
    );
    let single = solve_body(
        &one_room,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    let single_room = 1.0 - single.after.white_point / single.before.white_point;
    assert!(
        single_room >= 0.65,
        "a single-room wedding must converge substantially further than a mixed-light one: {:.0} %",
        single_room * 100.0
    );
    assert!(
        single_room > mixed_light,
        "one room must beat four: {:.0} % against {:.0} %",
        single_room * 100.0,
        mixed_light * 100.0
    );
    println!(
        "gate 1b: white points converged {:.0} % across four rooms, {:.0} % inside one",
        mixed_light * 100.0,
        single_room * 100.0
    );
}

#[test]
fn gate_2_grade_signature_distance_is_reduced_by_at_least_sixty_five_per_cent() {
    let frames = overlapping_wedding();
    let solved = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    assert!(
        solved.before.grade_signature > 0.0,
        "the fixture must author a grade difference"
    );
    let reduction = 1.0 - solved.after.grade_signature / solved.before.grade_signature;
    assert!(
        reduction >= SIGNATURE_REDUCTION,
        "grade-signature distance fell {:.1} %, below the {:.0} % promised",
        reduction * 100.0,
        SIGNATURE_REDUCTION * 100.0
    );
    println!(
        "gate 2: signature distance reduced {:.1} %",
        reduction * 100.0
    );
}

#[test]
fn gate_3_a_transform_improves_evidence_it_never_saw() {
    let frames = overlapping_wedding();
    let solved = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    assert!(
        solved.heldout_pairs >= 3,
        "the held-out split must produce something to check against; {} pairs",
        solved.heldout_pairs
    );
    assert_eq!(
        solved.verdict,
        Some(true),
        "a correct transform must improve pairs the solver never saw"
    );
}

#[test]
fn gate_3b_a_transform_that_does_not_hold_up_is_thrown_away() {
    // The other half of section 6.2, and the half that matters: a held-out check nothing ever fails
    // is a check that is not running. Fitting evidence says the body is cooler; held-out evidence
    // says the opposite, so the fitted answer must be rejected.
    let policy = Matching::default();
    let readings =
        |reference_cct: f32, body_cct: f32, count: usize| -> Vec<transform::PairReading> {
            use aura_raw::colour::illuminant::cct_to_uv;
            (0..count)
                .map(|i| {
                    let jitter = (i as f32 % 5.0 - 2.0) * 12.0;
                    transform::PairReading {
                        left_skin: None,
                        right_skin: None,
                        left_white: cct_to_uv(reference_cct + jitter),
                        right_white: cct_to_uv(body_cct + jitter),
                        left_cct: reference_cct + jitter,
                        right_cct: body_cct + jitter,
                        left_signature: [0.10; 8],
                        right_signature: [0.10; 8],
                        left_contrast: 12.0,
                        right_contrast: 12.0,
                        left_luma: 0.45,
                        right_luma: 0.45,
                    }
                })
                .collect()
        };
    let fit = solve::fit(
        &CameraId::new("cam_b"),
        FlashState::Ambient,
        &CameraId::new("cam_a"),
        &readings(5200.0, 4600.0, 30),
        &[],
        &policy,
    )
    .expect("enough readings");
    let (_, _, verdict) = solve::verify(&fit, &readings(4600.0, 5200.0, 10));
    assert_eq!(
        verdict,
        Some(false),
        "an overfitted transform must fail its held-out check"
    );
}

#[test]
fn gate_4_flash_and_ambient_populations_receive_distinct_transforms() {
    // One body, two flash states, two genuinely different colour behaviours. Section 6.1: brand
    // differences are amplified under flash, and a single transform fitted across both is wrong for
    // both.
    let ambient = fixtures::wedding(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 3,
            per_node: 14,
            flash: FlashState::Ambient,
            ..Shape::default()
        },
    );
    let mut flash_body = Body::SECOND;
    // Under a strobe the body's temperature difference nearly closes, which is what the bundled
    // baselines say and what the fixture authors here.
    flash_body.d_cct = -80.0;
    let flash = fixtures::wedding(
        &[Body::REFERENCE, flash_body],
        Shape {
            nodes: 3,
            per_node: 14,
            flash: FlashState::Flash,
            ..Shape::default()
        },
    );
    let mut frames = ambient;
    frames.extend(flash);

    let a = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    let f = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Flash,
    );
    assert!(
        a.verified_pairs > 0 && f.verified_pairs > 0,
        "both populations must pair"
    );
    assert!(
        (a.transform.d_cct - f.transform.d_cct).abs() > 100.0,
        "the two populations got the same answer: ambient {:.0} K, flash {:.0} K",
        a.transform.d_cct,
        f.transform.d_cct
    );
    println!(
        "gate 4: ambient {:.0} K, flash {:.0} K",
        a.transform.d_cct, f.transform.d_cct
    );
}

#[test]
fn gate_4b_a_pair_is_never_formed_across_the_flash_boundary() {
    let mut frames = fixtures::wedding(&[Body::REFERENCE, Body::SECOND], Shape::default());
    for frame in &mut frames {
        if frame.camera.as_str() == Body::SECOND.id {
            frame.flash = FlashState::Flash;
        }
    }
    let found = pairs::find(
        &frames,
        &CameraId::new(Body::REFERENCE.id),
        &CameraId::new(Body::SECOND.id),
        &Matching::default(),
    );
    assert!(found.is_empty());
}

#[test]
fn gate_5_camera_transforms_precede_within_scene_normalisation() {
    // Section 6.4's ordering, as a property of the data rather than of a comment. A gallery frame
    // that has been through the field carries the *corrected* temperature, which is what phase 25's
    // tree, change points, anchors and targets are then computed over.
    let frames = overlapping_wedding();
    let solved = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    let second: Vec<&CameraFrame> = frames
        .iter()
        .filter(|frame| frame.camera.as_str() == Body::SECOND.id)
        .collect();
    let image = second[0].image;
    let raw_cct = second[0].cct_k.expect("a temperature");

    let field = Field::from_transforms(
        std::slice::from_ref(&solved.transform),
        &[(image, CameraId::new(Body::SECOND.id), FlashState::Ambient)],
    );
    let mut gallery = vec![aura_brain_gallery::camera::fixtures::plain_gallery_frame(
        image,
    )];
    gallery[0].cct_k = Some(raw_cct);
    let moved = field.apply_to_gallery_frames(&mut gallery);

    assert_eq!(moved, 1, "the field must reach the frame");
    let corrected = gallery[0].cct_k.expect("a temperature");
    assert!(
        (corrected - raw_cct - solved.transform.d_cct).abs() < 1e-3,
        "the gallery frame carries {corrected:.0} K, not the corrected {:.0} K",
        raw_cct + solved.transform.d_cct
    );
    assert!(
        (corrected - raw_cct).abs() > 100.0,
        "the correction must actually change what phase 25 sees"
    );
}

#[test]
fn gate_5b_a_disabled_camera_reaches_phase_25_uncorrected_and_absent() {
    // "Matching runs before gallery normalisation and can be disabled per camera" - section 13's
    // fourth acceptance criterion. A disabled body must be **absent** from the field rather than
    // present as an identity, so a caller cannot confuse "matching is off here" with "matching
    // found nothing to do".
    let frames = overlapping_wedding();
    let mut solved = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    solved.transform.enabled = false;
    let image = frames[0].image;
    let field = Field::from_transforms(
        &[solved.transform],
        &[(image, CameraId::new(Body::SECOND.id), FlashState::Ambient)],
    );
    assert!(field.is_empty());
    let mut gallery = vec![aura_brain_gallery::camera::fixtures::plain_gallery_frame(
        image,
    )];
    let before = gallery[0].cct_k;
    assert_eq!(field.apply_to_gallery_frames(&mut gallery), 0);
    assert_eq!(gallery[0].cct_k, before);
}

#[test]
fn gate_6_with_no_matched_pairs_a_brand_baseline_is_used_and_the_report_says_so() {
    let frames = fixtures::wedding_with_no_overlap(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 4,
            per_node: 14,
            ..Shape::default()
        },
    );
    let found = pairs::find(
        &frames,
        &CameraId::new(Body::REFERENCE.id),
        &CameraId::new(Body::SECOND.id),
        &Matching::default(),
    );
    assert!(
        found.is_empty(),
        "the fixture must supply no overlap at all"
    );

    let solved = solve_body(
        &frames,
        Body::REFERENCE.id,
        Body::SECOND.id,
        FlashState::Ambient,
    );
    assert_eq!(solved.transform.source, TransformSource::BrandBaseline);
    assert_eq!(solved.transform.evidence_pairs, 0);
    assert!(
        solved
            .transform
            .reasons
            .iter()
            .any(|reason| reason.code == CameraCode::BaselineOnly),
        "the report must say the correction came from the brand"
    );

    // And the sentence a photographer reads has to say it too, not only the code.
    let report = aura_brain_gallery::camera::report::of_camera(
        &solved.transform,
        None,
        &[],
        None,
        Some("second"),
    );
    assert!(
        report.evidence.contains("knows about the brand"),
        "the report reads: {}",
        report.evidence
    );
}

#[test]
fn gate_6b_an_unknown_manufacturer_changes_nothing_rather_than_guessing() {
    let library = Library::bundled();
    let policy = Matching::default();
    let transform = solve::from_baseline(
        &CameraId::new("cam_x"),
        FlashState::Ambient,
        &CameraId::new("cam_a"),
        aura_core::contract::camera::Brand::Canon,
        aura_core::contract::camera::Brand::Other,
        &library,
        &policy,
    );
    assert!(transform.is_identity());
    assert_eq!(transform.confidence, 0.0);
    assert!(transform
        .reasons
        .iter()
        .any(|reason| reason.code == CameraCode::BaselineUnknownBrand));
}

#[test]
fn gate_7_no_camera_exceeds_the_documented_maximum_movement() {
    // Two bodies further apart than any bound allows, on every axis at once. The answer must be at
    // the ceilings and must never be past them - and migration 26's CHECK constraints are the
    // second layer under this.
    let extreme = Body {
        d_cct: -3000.0,
        d_tint: -60.0,
        luma_scale: 0.35,
        d_saturation: -40.0,
        ..Body::SECOND
    };
    let frames = fixtures::wedding(
        &[Body::REFERENCE, extreme],
        Shape {
            nodes: 4,
            per_node: 14,
            ..Shape::default()
        },
    );
    let solved = solve_body(&frames, Body::REFERENCE.id, extreme.id, FlashState::Ambient);
    let t = &solved.transform;
    assert!(t.within_bounds(), "{t:?}");
    assert!(t.d_cct.abs() <= MAX_T_CCT_K + 1e-3);
    assert!(t.d_tint.abs() <= MAX_T_TINT + 1e-3);
    assert!(t.d_exposure.abs() <= MAX_T_EXPOSURE_EV + 1e-3);
    assert!(t.d_saturation.abs() <= MAX_T_SATURATION + 1e-3);
    assert!(t
        .channel_gain
        .iter()
        .all(|g| (g - 1.0).abs() <= MAX_CHANNEL_GAIN + 1e-4));
    assert!(t
        .contrast_shape
        .iter()
        .all(|c| (c - 1.0).abs() <= MAX_CONTRAST_SHAPE + 1e-4));
    assert!(t.skin_correction.within_caps());
    assert!(
        t.bounded.is_some(),
        "a clamped transform must name the axis"
    );
}

#[test]
fn a_shooter_is_harmonised_and_never_erased() {
    // Section 6.3's cap, on the fixture's own second shooter - who works a third of a stop darker
    // than the lead by construction.
    let frames = fixtures::wedding(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 4,
            per_node: 20,
            ..Shape::default()
        },
    );
    let rows = aura_brain_gallery::camera::shooter::measure(
        &frames,
        &CameraId::new(Body::REFERENCE.id),
        &Matching::default(),
    );
    let usable: Vec<_> = rows.iter().filter(|row| row.is_usable()).collect();
    assert!(
        !usable.is_empty(),
        "the fixture must supply enough frames to measure"
    );
    for row in usable {
        assert!(
            row.measured_ev < -0.2,
            "the fixture's second shooter is darker"
        );
        assert!(row.applied_ev > 0.0, "the correction brightens them");
        assert!(
            row.applied_ev.abs() < row.measured_ev.abs(),
            "a habit is harmonised, not erased: measured {:.2}, applied {:.2}",
            row.measured_ev,
            row.applied_ev
        );
        assert!(row.applied_ev.abs() <= MAX_SHOOTER_EV + f32::EPSILON);
    }
}

#[test]
fn two_bodies_of_the_same_make_are_never_corrected_toward_each_other_by_a_baseline() {
    // The property that makes composing baselines through a neutral reference the right shape, and
    // the one a per-pair table could not guarantee without seven more rows nobody would check.
    let library = Library::bundled();
    for brand in aura_core::contract::camera::Brand::ALL {
        for flash in FlashState::ALL {
            let (departure, bound) =
                aura_brain_gallery::camera::baseline::between(&library, brand, brand, flash);
            assert!(departure.is_neutral(), "{brand} under {flash}");
            assert_eq!(bound, None);
        }
    }
}

#[test]
fn a_pair_whose_backgrounds_disagree_is_rejected_and_the_rejection_is_kept() {
    // Section 6.1's verification, on a fixture built to catch the circular alternative: the two
    // frames embed identically and their surroundings say they were in different rooms.
    let frames = fixtures::wedding_with_no_overlap(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 2,
            per_node: 12,
            distinct_rooms: 2,
            ..Shape::default()
        },
    );
    // Put both bodies back in one node so a pair can be *proposed*, while the rooms still differ.
    let node = frames
        .iter()
        .find(|frame| frame.camera.as_str() == Body::REFERENCE.id)
        .and_then(|frame| frame.node);
    let mut mixed = frames.clone();
    let mut room_flip = false;
    for frame in &mut mixed {
        frame.node = node;
        if frame.camera.as_str() == Body::SECOND.id {
            room_flip = true;
            // A different room: much darker, different dominant hue.
            frame.background = Some(fingerprint::BackgroundStats::from_descriptors(
                &{
                    let mut hist = [0_u8; 512];
                    for (index, slot) in hist.iter_mut().enumerate() {
                        *slot = if (192..256).contains(&index) { 200 } else { 0 };
                    }
                    hist
                },
                [0.08, 0.00, 0.06, 0.30],
                0.70,
            ));
        }
    }
    assert!(room_flip);
    let found = pairs::find(
        &mixed,
        &CameraId::new(Body::REFERENCE.id),
        &CameraId::new(Body::SECOND.id),
        &Matching::default(),
    );
    assert!(
        !found.is_empty(),
        "a candidate must be proposed to be rejected"
    );
    assert!(
        found.iter().all(|pair| !pair.verified),
        "two different rooms must not verify"
    );
    assert!(
        found.iter().all(|pair| pair.subject_similarity > 0.9),
        "and the subjects must agree, or the gate proves nothing"
    );
}

#[test]
fn every_reason_code_is_documented_in_the_product_voice() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/camera-matching.md"),
    )
    .expect("docs/camera-matching.md");
    for code in CameraCode::ALL {
        assert!(
            doc.contains(code.as_str()),
            "`{}` is not documented in docs/camera-matching.md",
            code.as_str()
        );
    }
}

#[test]
fn the_gates_print_what_they_do_not_prove() {
    // Phases 09 to 25 all end this way, and the reason is the same every time: a green suite is
    // read as a claim about the product unless somebody says otherwise on every run.
    let library = Library::bundled();
    println!("\n--- what the phase 26 gates do not prove ---");
    println!(
        "C1  Every gate above ran on a synthetic wedding whose per-brand colour response was\n\
         \x20   authored. There are no multi-camera weddings in this repository. What is proved is\n\
         \x20   the fingerprint, the pairing, the background verification, the metric, the solver,\n\
         \x20   the bounds, the held-out check, the blend and the ordering - not accuracy on a\n\
         \x20   photograph."
    );
    println!(
        "C2  All {} bundled brand baselines were fabricated; measured = {}. The fallback path is\n\
         \x20   proved to run and to report itself honestly, and nothing is proved about the\n\
         \x20   numbers it falls back on. The first measured baseline reopens these criteria.",
        library.len(),
        library.any_measured()
    );
    println!(
        "C3  The skin term ran against an authored chromaticity offset. Phase 25's\n\
         \x20   SKIN_FIELD_AVAILABLE is false, so no photograph in this build carries an\n\
         \x20   identity-scoped skin region. Gate 1 measures a mechanism, not a person."
    );
    println!(
        "C4  Section 9's blind study - can a photographer pick out the second camera after\n\
         \x20   matching - did not happen. The phase's own headline acceptance criterion is\n\
         \x20   unmeasured and no claim about it may be made from this build.\n"
    );
    assert!(!library.any_measured(), "see C2");
}
