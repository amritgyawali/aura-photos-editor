//! PHASE-23. Properties of the frozen geometry contract that a later build must not break by
//! accident.
//!
//! Nothing here crops a photograph. These are the assertions that keep the *vocabulary* honest
//! and the *promises* structural: that every code has a sentence, that two thirds of the
//! vocabulary describes things that were not done, that the two pieces of shared geometry
//! arithmetic agree with what a rotation actually costs, and - the one this phase exists for -
//! that **a crop which cuts a protected region is not a representable safe state**.
//!
//! Three of these tests are about things that are *absent*. Section 2.2 puts content-aware fill,
//! album layout and panoramas out of scope, and the enforcement is that there is nowhere in this
//! contract to put a fill method, a layout, an output scale or a second photograph; a test is
//! what turns "there is no field for it" from an observation into a rule.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    fit_aspect, rotation_crop, AspectRatio, CropPurpose, CropSafetyReport, CropVariant,
    GeometryCode, GeometryOutline, GeometryOverride, GeometryPlan, GeometryReason, Keystone,
    LensCorrection, LensSource, ProtectedContent, ProtectedRegion, KEYSTONE_ACT_AT, MAX_REASONS,
    MAX_STRETCH, MAX_VARIANTS, MIN_IMPROVEMENT, MIN_LONG_EDGE_FRACTION, ROTATE_ACT_AT,
    ROTATE_MAX_DEG, ROTATE_MIN_DEG, SAFETY_MARGIN,
};
use aura_core::contract::scene::SceneId;
use aura_core::PhotoId;

fn image() -> PhotoId {
    PhotoId::from_db("pht_00000000-0000-4000-8000-000000000023").expect("a photo id")
}

fn plan() -> GeometryPlan {
    GeometryPlan::untouched(image(), SceneId::CouplePortrait)
}

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

#[test]
fn every_code_has_a_distinct_slug_and_a_sentence() {
    let mut slugs: Vec<&str> = GeometryCode::ALL.iter().map(|c| c.as_str()).collect();
    slugs.sort_unstable();
    let before = slugs.len();
    slugs.dedup();
    assert_eq!(before, slugs.len(), "two codes share a slug");
    assert_eq!(GeometryCode::ALL.len(), GeometryCode::COUNT);

    for code in GeometryCode::ALL {
        assert!(
            code.as_str().starts_with("geometry_"),
            "{code} is not namespaced"
        );
        assert!(
            code.user_text().len() > 12,
            "{code} has no sentence a photographer could read"
        );
        assert_eq!(GeometryCode::parse(code.as_str()), Some(code));
    }
    assert_eq!(GeometryCode::parse("geometry_not_a_code"), None);
}

#[test]
fn most_of_the_vocabulary_describes_what_was_not_done() {
    let refusals = GeometryCode::ALL
        .iter()
        .filter(|code| code.is_refusal())
        .count();
    // Twenty of thirty. The header says so and this is what keeps it true: a phase whose section
    // 10.1 requires that seventy per cent of frames are left alone needs a vocabulary that can
    // explain being left alone in more than one way.
    assert_eq!(refusals, 23, "the refusal half of the vocabulary moved");
    assert!(
        refusals * 2 > GeometryCode::COUNT,
        "most codes must describe a refusal"
    );
}

#[test]
fn every_safety_refusal_is_a_refusal() {
    for code in GeometryCode::ALL {
        if code.is_safety_refusal() {
            assert!(
                code.is_refusal(),
                "{code} refuses a crop without being a refusal"
            );
        }
    }
    assert_eq!(
        GeometryCode::ALL
            .iter()
            .filter(|c| c.is_safety_refusal())
            .count(),
        5
    );
}

#[test]
fn a_lens_source_round_trips_and_an_unknown_one_corrects_nothing() {
    for source in LensSource::ALL {
        assert_eq!(LensSource::from_str_or_none(source.as_str()), source);
    }
    assert_eq!(LensSource::from_str_or_none("lensfun"), LensSource::None);
    assert!(!LensSource::None.is_available());
    assert!(LensSource::Embedded.is_measured());
    assert!(LensSource::Database.is_measured());
    assert!(
        !LensSource::Estimated.is_measured(),
        "an estimate from one frame is not a measurement of a lens"
    );
}

#[test]
fn an_unknown_protection_reads_as_a_face_rather_than_as_nothing() {
    for kind in ProtectedContent::ALL {
        assert_eq!(ProtectedContent::from_str_or_face(kind.as_str()), kind);
    }
    assert_eq!(
        ProtectedContent::from_str_or_face("elbow"),
        ProtectedContent::Face,
        "an unreadable protection must over-protect rather than under-protect"
    );
}

// ---------------------------------------------------------------------------
// The aspect vocabulary
// ---------------------------------------------------------------------------

#[test]
fn the_original_aspect_has_no_ratio_of_its_own() {
    assert_eq!(AspectRatio::Original.ratio(), None);
    for aspect in AspectRatio::VARIANTS {
        let ratio = aspect.ratio().expect("a named aspect has a ratio");
        assert!(ratio > 0.0 && ratio.is_finite(), "{aspect} has no ratio");
    }
    assert_eq!(AspectRatio::ALL.len(), MAX_VARIANTS);
    for aspect in AspectRatio::ALL {
        assert_eq!(AspectRatio::from_str_or_original(aspect.as_str()), aspect);
    }
    assert_eq!(
        AspectRatio::from_str_or_original("3:2"),
        AspectRatio::Original
    );
}

#[test]
fn exactly_one_aspect_is_the_delivered_one() {
    let primary: Vec<AspectRatio> = AspectRatio::ALL
        .into_iter()
        .filter(|a| a.purpose() == CropPurpose::Primary)
        .collect();
    assert_eq!(
        primary,
        vec![AspectRatio::Original],
        "the frame's own aspect is the only primary one"
    );
    for purpose in CropPurpose::ALL {
        assert_eq!(CropPurpose::from_str_or_primary(purpose.as_str()), purpose);
    }
}

// ---------------------------------------------------------------------------
// The safety promise, as a shape
// ---------------------------------------------------------------------------

#[test]
fn a_region_touching_the_crop_edge_is_not_inside_it() {
    let region = ProtectedRegion::anonymous(
        ProtectedContent::PrimaryFace,
        Box2 {
            x: 0.40,
            y: 0.40,
            w: 0.20,
            h: 0.20,
        },
    );
    // Exactly containing it is not enough: the resampler reads outside its own rectangle.
    assert!(!region.inside(Box2 {
        x: 0.40,
        y: 0.40,
        w: 0.20,
        h: 0.20
    }));
    // One safety margin of slack on every side is.
    let margin = SAFETY_MARGIN + 1e-4;
    assert!(region.inside(Box2 {
        x: 0.40 - margin,
        y: 0.40 - margin,
        w: 0.20 + 2.0 * margin,
        h: 0.20 + 2.0 * margin
    }));
}

#[test]
fn a_report_over_nothing_says_so_rather_than_claiming_safety() {
    let empty = CropSafetyReport::nothing_protected(1.0);
    assert!(empty.is_safe());
    assert_eq!(
        empty.considered, 0,
        "a caller must be able to tell an empty check from a passed one"
    );
    assert_eq!(empty.at_risk, 0);
}

#[test]
fn a_plan_that_did_nothing_delivers_the_whole_frame() {
    let plan = plan();
    assert!(plan.is_identity());
    assert_eq!(plan.primary(), Box2::FULL);
    assert_eq!(plan.crops.len(), 1);
    assert_eq!(
        plan.crops[0].aspect,
        AspectRatio::Original,
        "the first variant is always the original framing"
    );
    assert!(plan.has(GeometryCode::CropKeptOriginal));
    assert!(plan.alternates().is_empty());
}

#[test]
fn a_primary_crop_index_that_points_nowhere_still_delivers_the_frame() {
    // Not reachable through any constructor here; reachable through a row somebody edited.
    let mut plan = plan();
    plan.primary_crop = 7;
    assert_eq!(
        plan.primary(),
        Box2::FULL,
        "a broken index must not deliver a rectangle nobody chose"
    );
}

#[test]
fn an_alternate_is_never_the_delivered_crop_and_is_never_unsafe() {
    let mut plan = plan();
    plan.crops.push(CropVariant {
        aspect: AspectRatio::Square,
        rect: Box2 {
            x: 0.1,
            y: 0.0,
            w: 0.66,
            h: 1.0,
        },
        purpose: CropPurpose::Social,
        score: 0.7,
        safe: true,
    });
    plan.crops.push(CropVariant {
        aspect: AspectRatio::SixteenNine,
        rect: Box2 {
            x: 0.0,
            y: 0.2,
            w: 1.0,
            h: 0.5,
        },
        purpose: CropPurpose::Album,
        score: 0.9,
        safe: false,
    });
    let alternates = plan.alternates();
    assert_eq!(alternates.len(), 1);
    assert_eq!(alternates[0].aspect, AspectRatio::Square);
}

// ---------------------------------------------------------------------------
// The lens correction
// ---------------------------------------------------------------------------

#[test]
fn a_correction_from_nowhere_corrects_nothing_however_it_was_built() {
    let corrupt = LensCorrection {
        distortion: true,
        vignette: 200,
        ca: true,
        profile_id: Some("somebody_elses_lens".into()),
        source: LensSource::None,
    };
    let clamped = corrupt.clamped();
    assert!(
        clamped.is_identity(),
        "a correction with no source must not move a pixel"
    );
    assert_eq!(clamped.profile_id, None);
}

#[test]
fn a_vignette_over_a_hundred_is_clamped_rather_than_refused() {
    let over = LensCorrection {
        vignette: 250,
        source: LensSource::Database,
        ..LensCorrection::none()
    };
    assert_eq!(over.clamped().vignette, 100);
}

// ---------------------------------------------------------------------------
// The keystone cap
// ---------------------------------------------------------------------------

#[test]
fn a_keystone_past_the_stretch_cap_is_outside_it() {
    let inside = Keystone {
        vertical: 30.0,
        horizontal: 0.0,
        stretch: MAX_STRETCH - 0.01,
        convergence: 0.5,
    };
    let outside = Keystone {
        stretch: MAX_STRETCH + 0.01,
        ..inside
    };
    assert!(inside.within_cap());
    assert!(!outside.within_cap());
    assert!(Keystone::identity().within_cap());
    assert!(Keystone::identity().is_identity());
}

#[test]
fn a_keystone_with_a_broken_stretch_is_clamped_to_doing_nothing_measurable() {
    let broken = Keystone {
        vertical: 1e9,
        horizontal: -1e9,
        stretch: f32::NAN,
        convergence: 4.0,
    }
    .clamped();
    assert_eq!(broken.vertical, 100.0);
    assert_eq!(broken.horizontal, -100.0);
    assert_eq!(broken.stretch, 1.0);
    assert_eq!(broken.convergence, 1.0);
}

// ---------------------------------------------------------------------------
// The shared arithmetic
// ---------------------------------------------------------------------------

#[test]
fn a_rotation_of_zero_costs_nothing() {
    assert_eq!(rotation_crop(6000, 4000, 0.0), Box2::FULL);
    assert_eq!(rotation_crop(6000, 4000, f32::NAN), Box2::FULL);
    assert_eq!(rotation_crop(0, 4000, 3.0), Box2::FULL);
}

#[test]
fn a_larger_rotation_always_costs_more_of_the_frame() {
    let mut previous = 1.0f32;
    for tenths in 0..=80u32 {
        let degrees = tenths as f32 / 10.0;
        let crop = rotation_crop(6000, 4000, degrees);
        let area = crop.w * crop.h;
        assert!(
            area <= previous + 1e-5,
            "rotating by {degrees} kept more of the frame than a smaller angle"
        );
        assert!(area > 0.0, "rotating by {degrees} kept nothing");
        previous = area;
    }
}

#[test]
fn a_rotated_crop_is_centred_and_keeps_the_frames_aspect() {
    let crop = rotation_crop(6000, 4000, 5.0);
    assert!((crop.x * 2.0 + crop.w - 1.0).abs() < 1e-5, "not centred in x");
    assert!((crop.y * 2.0 + crop.h - 1.0).abs() < 1e-5, "not centred in y");
    // The inscribed rectangle has the frame's aspect, so both fractions are equal.
    assert!(
        (crop.w - crop.h).abs() < 1e-4,
        "the inscribed rectangle changed shape: {crop:?}"
    );
}

#[test]
fn the_rotated_crop_actually_fits_inside_the_rotated_frame() {
    // The property the whole function exists for, checked against the definition rather than
    // against the formula: every corner of the returned rectangle, rotated back, must land
    // inside the original frame.
    for degrees in [0.5f32, 1.0, 2.5, 5.0, 8.0, 30.0, 44.0, 60.0] {
        let (w, h) = (6000.0f32, 4000.0f32);
        let crop = rotation_crop(6000, 4000, degrees);
        let a = degrees.to_radians();
        let (sin_a, cos_a) = (a.sin(), a.cos());
        let corners = [
            (crop.x, crop.y),
            (crop.x + crop.w, crop.y),
            (crop.x, crop.y + crop.h),
            (crop.x + crop.w, crop.y + crop.h),
        ];
        for (nx, ny) in corners {
            // Into pixels about the centre, then rotated by the angle the render applies.
            let px = (nx - 0.5) * w;
            let py = (ny - 0.5) * h;
            let rx = px * cos_a - py * sin_a;
            let ry = px * sin_a + py * cos_a;
            assert!(
                rx.abs() <= w / 2.0 + 1.0 && ry.abs() <= h / 2.0 + 1.0,
                "at {degrees} deg a corner landed outside the frame: ({rx}, {ry})"
            );
        }
    }
}

#[test]
fn a_fitted_aspect_is_the_aspect_it_was_asked_for() {
    let frame_aspect = 1.5f32; // a 3:2 frame
    for aspect in AspectRatio::VARIANTS {
        let ratio = aspect.ratio().expect("a named aspect has a ratio");
        let rect = fit_aspect(Box2::FULL, frame_aspect, ratio, (0.5, 0.5));
        let got = (rect.w * frame_aspect) / rect.h;
        assert!(
            (got - ratio).abs() < 1e-3,
            "{aspect} came out at {got} rather than {ratio}"
        );
        assert!(rect.x >= -1e-6 && rect.y >= -1e-6);
        assert!(rect.x + rect.w <= 1.0 + 1e-6);
        assert!(rect.y + rect.h <= 1.0 + 1e-6);
    }
}

#[test]
fn a_centre_outside_the_bounds_slides_the_rectangle_rather_than_shrinking_it() {
    let full = fit_aspect(Box2::FULL, 1.5, 1.0, (0.5, 0.5));
    let edge = fit_aspect(Box2::FULL, 1.5, 1.0, (0.02, 0.5));
    assert!(
        (full.w - edge.w).abs() < 1e-6 && (full.h - edge.h).abs() < 1e-6,
        "honouring a centre must never cost resolution"
    );
    assert!(edge.x >= -1e-6, "the rectangle left the frame");
}

#[test]
fn a_square_crop_of_a_three_by_two_frame_keeps_two_thirds_of_the_long_edge() {
    // The number MIN_LONG_EDGE_FRACTION has to be feasible against, worked by hand: a 6000x4000
    // frame cropped square is 4000 px on its long edge, which is 0.667 of 6000. A floor above
    // that would forbid every square variant in the product.
    let square = CropVariant {
        aspect: AspectRatio::Square,
        rect: fit_aspect(Box2::FULL, 1.5, 1.0, (0.5, 0.5)),
        purpose: CropPurpose::Social,
        score: 0.5,
        safe: true,
    };
    let fraction = square.long_edge_fraction(1.5);
    assert!(
        (fraction - 2.0 / 3.0).abs() < 1e-3,
        "a square crop of a 3:2 frame keeps {fraction} of the long edge"
    );
    assert!(
        fraction > MIN_LONG_EDGE_FRACTION,
        "the resolution floor forbids an aspect section 2.1 requires"
    );
}

#[test]
fn the_full_frame_keeps_all_of_its_long_edge() {
    let original = CropVariant::original(0.5);
    assert!((original.long_edge_fraction(1.5) - 1.0).abs() < 1e-6);
    assert!((original.long_edge_fraction(0.667) - 1.0).abs() < 1e-6);
    assert!(original.is_full_frame());
}

// ---------------------------------------------------------------------------
// The bands themselves
// ---------------------------------------------------------------------------

#[test]
fn the_rotation_band_is_a_band_and_not_a_ceiling() {
    assert!(ROTATE_MIN_DEG > 0.0);
    assert!(ROTATE_MAX_DEG > ROTATE_MIN_DEG);
    assert!(
        ROTATE_MIN_DEG < 0.4,
        "the floor must be below phase 11's own angle error, or nothing is ever level enough"
    );
    assert!(
        ROTATE_MAX_DEG < 15.0,
        "past this a tilt is a decision rather than a mistake"
    );
}

#[test]
fn this_phase_acts_on_less_than_phase_eleven_reports() {
    use aura_core::contract::composition::HORIZON_ACT_AT;
    assert!(
        ROTATE_ACT_AT > HORIZON_ACT_AT,
        "acting on a horizon must need more confidence than reporting one"
    );
}

#[test]
fn every_band_is_inside_its_own_unit_range() {
    for (name, value) in [
        ("MIN_IMPROVEMENT", MIN_IMPROVEMENT),
        ("MIN_LONG_EDGE_FRACTION", MIN_LONG_EDGE_FRACTION),
        ("ROTATE_ACT_AT", ROTATE_ACT_AT),
        ("KEYSTONE_ACT_AT", KEYSTONE_ACT_AT),
        ("SAFETY_MARGIN", SAFETY_MARGIN),
    ] {
        assert!(
            value > 0.0 && value < 1.0,
            "{name} is outside the range it is measured in"
        );
    }
    assert!(MAX_STRETCH > 1.0 && MAX_STRETCH < 1.5);
    assert!(MAX_REASONS >= 8);
}

// ---------------------------------------------------------------------------
// The override
// ---------------------------------------------------------------------------

#[test]
fn an_empty_override_asks_for_nothing_and_a_revert_asks_for_everything() {
    assert!(GeometryOverride::default().is_empty());
    let revert = GeometryOverride::reverted();
    assert!(!revert.is_empty());
    assert!(revert.revert);
    assert_eq!(
        revert.crop, None,
        "a revert is not a hand-set full-frame crop"
    );
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

#[test]
fn conservatism_over_an_empty_project_is_not_a_failure() {
    let empty = GeometryOutline::default();
    assert!((empty.conservatism() - 1.0).abs() < f32::EPSILON);
    let half = GeometryOutline {
        planned: 100,
        kept_original: 71,
        ..GeometryOutline::default()
    };
    assert!(half.conservatism() > 0.70);
}

// ---------------------------------------------------------------------------
// What is absent, and must stay absent
// ---------------------------------------------------------------------------

#[test]
fn nothing_in_this_contract_can_name_a_fill_a_scale_or_a_second_photograph() {
    // Section 2.2 puts content-aware fill in phase 24, album layout in phase 29, and panoramas
    // nowhere. The enforcement is that there is no field for any of them, and this is the grep
    // that turns that from an observation into a rule.
    let source = include_str!("../src/contract/geometry.rs");
    for forbidden in [
        "pub fill",
        "pub scale",
        "pub upscale",
        "pub source_image",
        "pub donor",
        "pub layout",
        "pub output_width",
        "pub output_height",
    ] {
        assert!(
            !source.contains(forbidden),
            "the geometry contract grew a `{forbidden}` field"
        );
    }
}

#[test]
fn a_reason_carries_a_code_rather_than_a_sentence() {
    let reason = GeometryReason::at(
        GeometryCode::CropCutsHands,
        -0.8,
        Box2 {
            x: 0.0,
            y: 0.8,
            w: 0.3,
            h: 0.2,
        },
    );
    assert!(reason.is_penalty());
    assert_eq!(reason.code, GeometryCode::CropCutsHands);
    assert!(reason.evidence.is_some());
    assert!(!GeometryReason::plain(GeometryCode::Straightened, 0.4).is_penalty());
}
