//! The PHASE-09 section 5 contract, asserted where a change to it will be noticed.
//!
//! Three kinds of claim are tested here, and they are the three a later phase would
//! be hurt by if they quietly stopped being true.
//!
//! 1. **Bit positions are a wire format.** `image_integrity.flags` is a stored `u32`,
//!    so renumbering a flag silently relabels every row in every catalog in the field.
//! 2. **Two flags are not defects.** The whole trust argument of the phase is in
//!    `INTENTIONAL_MOTION` and `EYES_CLOSED_OK`, and a `DEFECTS` mask that accidentally
//!    included one of them would turn a panned exit frame into a rejection.
//! 3. **An unknown value read from a catalog fails towards "nothing is wrong".** Every
//!    `from_str_or_*` in the contract picks the optimistic branch, because this phase's
//!    documented failure mode is false rejection.

use aura_core::contract::integrity::{
    CropRect, ExposureVerdict, EyeOpenness, IntegrityFlags, IntegrityOutline, MotionKind,
    Reason, ReasonCode,
};

#[test]
fn flag_bits_are_the_positions_section_5_names() {
    // Section 5, verbatim. A test that reads like the phase document on purpose: it is
    // the document that has to be re-read when this fails.
    assert_eq!(IntegrityFlags::SUBJECT_SOFT.bits(), 1 << 0);
    assert_eq!(IntegrityFlags::CAMERA_SHAKE.bits(), 1 << 1);
    assert_eq!(IntegrityFlags::SUBJECT_MOTION.bits(), 1 << 2);
    assert_eq!(IntegrityFlags::INTENTIONAL_MOTION.bits(), 1 << 3);
    assert_eq!(IntegrityFlags::BACK_FOCUS.bits(), 1 << 4);
    assert_eq!(IntegrityFlags::FRONT_FOCUS.bits(), 1 << 5);
    assert_eq!(IntegrityFlags::HIGHLIGHT_LOST.bits(), 1 << 6);
    assert_eq!(IntegrityFlags::SHADOW_LOST.bits(), 1 << 7);
    assert_eq!(IntegrityFlags::HEAVY_NOISE.bits(), 1 << 8);
    assert_eq!(IntegrityFlags::EYES_CLOSED.bits(), 1 << 9);
    assert_eq!(IntegrityFlags::EYES_CLOSED_OK.bits(), 1 << 10);
    assert_eq!(IntegrityFlags::SQUINT.bits(), 1 << 11);
    assert_eq!(IntegrityFlags::NO_SUBJECT_DETECTED.bits(), 1 << 12);
    assert_eq!(IntegrityFlags::MIXED_LIGHT_RISK.bits(), 1 << 13);
    assert_eq!(IntegrityFlags::ALL.len(), IntegrityFlags::COUNT);
}

#[test]
fn every_flag_has_a_slug_and_round_trips() {
    for flag in IntegrityFlags::ALL {
        let slug = flag.as_str().expect("a single flag has a slug");
        assert_eq!(IntegrityFlags::from_str_or_empty(slug), flag);
    }
    // A combination is not a single flag and must not pretend to be one.
    let both = IntegrityFlags::SUBJECT_SOFT.union(IntegrityFlags::HEAVY_NOISE);
    assert!(both.as_str().is_none());
    assert_eq!(both.count(), 2);
}

#[test]
fn the_three_flags_that_are_not_defects_are_not_defects() {
    for flag in [
        IntegrityFlags::INTENTIONAL_MOTION,
        IntegrityFlags::EYES_CLOSED_OK,
        IntegrityFlags::NO_SUBJECT_DETECTED,
    ] {
        assert!(
            !IntegrityFlags::DEFECTS.contains(flag),
            "{flag} must never count as a defect"
        );
        assert!(!flag.has_defect());
    }
    // And every other flag is one, so a fifteenth flag added without thought fails here
    // rather than in a wedding.
    for flag in IntegrityFlags::ALL {
        let exempt = matches!(
            flag,
            IntegrityFlags::INTENTIONAL_MOTION
                | IntegrityFlags::EYES_CLOSED_OK
                | IntegrityFlags::NO_SUBJECT_DETECTED
        );
        assert_eq!(flag.has_defect(), !exempt, "{flag}");
    }
}

#[test]
fn spare_bits_are_dropped_rather_than_believed() {
    // A catalog written by a build that knows a fifteenth flag must still open here,
    // and the flag must read as nothing rather than as some other flag.
    let future = IntegrityFlags::from_bits(0xFFFF_FFFF);
    assert_eq!(future.count(), u32::try_from(IntegrityFlags::COUNT).unwrap_or(0));
    assert!(future.contains(IntegrityFlags::MIXED_LIGHT_RISK));
}

#[test]
fn dismissing_a_flag_clears_only_that_flag() {
    let flags = IntegrityFlags::SUBJECT_SOFT
        .union(IntegrityFlags::HEAVY_NOISE)
        .union(IntegrityFlags::EYES_CLOSED);
    let after = flags.without(IntegrityFlags::HEAVY_NOISE);
    assert!(!after.contains(IntegrityFlags::HEAVY_NOISE));
    assert!(after.contains(IntegrityFlags::SUBJECT_SOFT));
    assert!(after.contains(IntegrityFlags::EYES_CLOSED));
}

#[test]
fn unknown_stored_text_fails_towards_nothing_being_wrong() {
    assert_eq!(MotionKind::from_str_or_none("swirl"), MotionKind::None);
    assert_eq!(
        ExposureVerdict::from_str_or_good("catastrophic"),
        ExposureVerdict::Good
    );
    assert_eq!(EyeOpenness::from_str_or_open("winking"), EyeOpenness::Open);
    assert_eq!(ReasonCode::from_str_or_clean("cursed"), ReasonCode::Clean);
}

#[test]
fn motion_kinds_and_flags_agree_about_what_is_a_defect() {
    for kind in MotionKind::ALL {
        assert_eq!(kind.flag().has_defect(), kind.is_defect(), "{kind}");
        assert_eq!(MotionKind::from_str_or_none(kind.as_str()), kind);
    }
    assert!(!MotionKind::Intentional.is_defect());
}

#[test]
fn every_reason_code_round_trips_and_exonerations_are_the_documented_nine() {
    for code in ReasonCode::ALL {
        assert_eq!(ReasonCode::from_str_or_clean(code.as_str()), code);
    }
    let exonerations: Vec<&str> = ReasonCode::ALL
        .into_iter()
        .filter(|code| code.is_exoneration())
        .map(ReasonCode::as_str)
        .collect();
    assert_eq!(
        exonerations,
        vec![
            "clean",
            "shallow_depth_of_field",
            "intentional_motion",
            "highlight_recoverable",
            "specular_highlight",
            "noise_within_scene",
            "eyes_closed_ok",
            "no_subject",
            "uncalibrated",
        ]
    );
}

#[test]
fn a_penalty_is_a_negative_weight_and_nothing_else() {
    let penalty = Reason::frame(ReasonCode::SubjectSoft, "her face is soft", -0.35);
    let exoneration = Reason::frame(
        ReasonCode::ShallowDepthOfField,
        "the background is soft on purpose",
        0.0,
    );
    assert!(penalty.is_penalty());
    assert!(!exoneration.is_penalty());
}

#[test]
fn an_evidence_crop_stays_inside_the_frame_however_far_it_is_grown() {
    let face = CropRect {
        x: 0.90,
        y: 0.02,
        w: 0.08,
        h: 0.10,
    };
    let shown = face.expanded(4.0);
    assert!(shown.x >= 0.0 && shown.y >= 0.0);
    assert!(shown.x + shown.w <= 1.0 + f32::EPSILON);
    assert!(shown.y + shown.h <= 1.0 + f32::EPSILON);
    assert!(!shown.is_empty());
    assert_eq!(CropRect::FULL.expanded(2.0), CropRect::FULL);
}

#[test]
fn the_outline_counts_flags_by_position() {
    let mut outline = IntegrityOutline {
        scored: 10,
        photos: 10,
        coverage: 1.0,
        ..IntegrityOutline::default()
    };
    // Position 0 is SUBJECT_SOFT, position 10 is EYES_CLOSED_OK.
    if let Some(slot) = outline.flag_histogram.get_mut(0) {
        *slot = 3;
    }
    if let Some(slot) = outline.flag_histogram.get_mut(10) {
        *slot = 7;
    }
    assert_eq!(outline.count(IntegrityFlags::SUBJECT_SOFT), 3);
    assert_eq!(outline.count(IntegrityFlags::EYES_CLOSED_OK), 7);
    // The exoneration is not counted towards the review queue's header.
    assert_eq!(outline.defective_at_most(), 3);
}
