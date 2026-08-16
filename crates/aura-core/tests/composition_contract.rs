//! The PHASE-11 frozen composition contract, asserted at its compatibility boundary.

use aura_core::contract::composition::{
    Box2, CompositionCode, CompositionFlags, CompositionOutline, CropHint, FrameEdge,
    HorizonSource, Joint,
};

#[test]
fn flag_bits_are_the_stored_positions() {
    let expected = [
        CompositionFlags::HORIZON_TILTED,
        CompositionFlags::INTENTIONAL_TILT,
        CompositionFlags::VERTICALS_CONVERGING,
        CompositionFlags::HEADROOM_TIGHT,
        CompositionFlags::HEADROOM_EXCESSIVE,
        CompositionFlags::HEAD_CROPPED,
        CompositionFlags::JOINT_CUT,
        CompositionFlags::LIMB_CUT,
        CompositionFlags::EDGE_INTRUSION,
        CompositionFlags::OFF_BALANCE,
        CompositionFlags::CENTRED,
        CompositionFlags::CLUTTERED_BACKGROUND,
        CompositionFlags::BRIGHT_BLOB,
        CompositionFlags::HEAD_MERGE,
        CompositionFlags::COLOUR_COMPETITION,
        CompositionFlags::NO_GEOMETRY,
    ];
    assert_eq!(CompositionFlags::ALL, expected);
    for (position, flag) in expected.into_iter().enumerate() {
        assert_eq!(flag.bits(), 1_u32 << position);
        let slug = flag.as_str().expect("a single flag has a slug");
        assert_eq!(CompositionFlags::from_str_or_empty(slug), flag);
    }
    assert_eq!(CompositionFlags::COUNT, 16);
}

#[test]
fn only_the_documented_three_flags_are_not_violations() {
    for flag in CompositionFlags::ALL {
        let factual = matches!(
            flag,
            CompositionFlags::INTENTIONAL_TILT
                | CompositionFlags::CENTRED
                | CompositionFlags::NO_GEOMETRY
        );
        assert_eq!(CompositionFlags::DEFECTS.contains(flag), !factual, "{flag}");
    }
}

#[test]
fn unknown_and_future_flags_fail_towards_no_new_claim() {
    assert_eq!(
        CompositionFlags::from_str_or_empty("taste_violation"),
        CompositionFlags::EMPTY
    );
    let future = CompositionFlags::from_bits(u32::MAX);
    assert_eq!(future.count(), CompositionFlags::COUNT as u32);
}

#[test]
fn every_reason_round_trips_and_the_exonerations_are_frozen() {
    for code in CompositionCode::ALL {
        assert_eq!(CompositionCode::from_str_or_clean(code.as_str()), code);
    }
    let exonerations: Vec<&str> = CompositionCode::ALL
        .into_iter()
        .filter(|code| code.is_exoneration())
        .map(CompositionCode::as_str)
        .collect();
    assert_eq!(
        exonerations,
        vec![
            "clean",
            "horizon_uncertain",
            "intentional_tilt",
            "deliberate_crop",
            "thirds_aligned",
            "centred",
            "balanced",
            "clean_background",
            "no_geometry",
            "keypoints_unavailable",
            "no_rule",
            "aesthetic_unavailable",
        ]
    );
}

#[test]
fn every_reason_is_publicly_documented_and_written_for_a_photographer() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/composition-and-framing.md"),
    )
    .expect("docs/composition-and-framing.md");
    for code in CompositionCode::ALL {
        assert!(
            page.contains(&format!("`{}`", code.as_str())),
            "{code} is absent from the public reference"
        );
        let text = code.user_text();
        assert!(text.len() > 20, "{code}: user text is too short");
        for banned in ["f32", "0..1", "bit", "flag", "sigma", "model output"] {
            assert!(
                !text.contains(banned),
                "{code} leaks implementation vocabulary: {banned}"
            );
        }
    }
}

#[test]
fn supporting_enum_slugs_round_trip() {
    for joint in Joint::ALL {
        assert_eq!(Joint::from_str_or_neck(joint.as_str()), joint);
    }
    for edge in FrameEdge::ALL {
        assert_eq!(FrameEdge::from_str_or_bottom(edge.as_str()), edge);
    }
    for source in HorizonSource::ALL {
        assert_eq!(HorizonSource::from_str_or_none(source.as_str()), source);
    }
}

#[test]
fn crop_objective_and_actionability_are_separate() {
    let objective = CropHint {
        region: Box2 {
            x: 0.2,
            y: 0.1,
            w: 0.6,
            h: 0.8,
        },
        safe_margin: 0.0,
        straighten_deg: None,
        confidence: 0.9,
    };
    assert!(!objective.region.is_empty());
    assert!(!objective.is_actionable());
    assert!(CropHint {
        safe_margin: CropHint::MIN_MARGIN,
        ..objective
    }
    .is_actionable());
}

#[test]
fn outline_violation_count_excludes_factual_flags() {
    let mut outline = CompositionOutline::default();
    for (index, slot) in outline.flag_histogram.iter_mut().enumerate() {
        *slot = (index + 1) as u32;
    }
    let expected: u32 = CompositionFlags::ALL
        .iter()
        .enumerate()
        .filter(|(_, flag)| CompositionFlags::DEFECTS.contains(**flag))
        .map(|(index, _)| (index + 1) as u32)
        .sum();
    assert_eq!(outline.violating_at_most(), expected);
    assert_eq!(outline.count(CompositionFlags::HEAD_MERGE), 14);
}
