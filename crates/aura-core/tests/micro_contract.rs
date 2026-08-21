//! PHASE-21. Properties of the frozen micro-retouch contract that a later build must not break
//! by accident.
//!
//! Nothing here measures a photograph. These are the assertions that keep the *vocabulary*
//! honest and the *promises* structural: that every code has a sentence, that no operation can
//! express a geometry change, that a borrow cannot exist without a disclosed source, that the
//! ceilings in section 5 actually refuse what they name, and that this phase's mask port agrees
//! with phase 19's about how much a doubtful region may do.
//!
//! The last one is worth stating: ADR-0043 section 7 accepts a three-line duplication of phase
//! 19's gating ramp in exchange for not widening a frozen enum, and it is only acceptable while
//! the two agree. This file is what makes a change to one that did not move the other fail the
//! build.

use aura_core::contract::composition::Box2;
use aura_core::contract::local::{MaskField, MaskKind, FULL_MASK_CONFIDENCE, MIN_MASK_CONFIDENCE};
use aura_core::contract::micro::{
    ClothingIssue, ColourLocus, GlareMethod, MicroCode, MicroField, MicroOp, MicroOverride,
    MicroPlan, MicroReason, MicroRegion, NaturalnessGuard, NaturalnessReport, OpFamily,
    CATCHLIGHT_FLOOR, HAIR_ENERGY_FLOOR, MAX_BORROW_AREA, MAX_CLOTHING_AREA, MAX_CLOTHING_STRENGTH,
    MAX_FLYAWAY_AREA, MAX_FLYAWAY_STRENGTH, MAX_GLARE_REDUCE, MAX_IRIS_CLARITY, MAX_OPS,
    MAX_SCLERA, MAX_TEETH_LUMA_EV, MAX_TEETH_YELLOW, MIN_ALIGNMENT, MIN_OP_CONFIDENCE,
    MIN_SPECULAR_FRACTION, TEETH_EXCURSION_CEILING,
};
use aura_core::contract::scene::SceneId;
use aura_core::{IdentityId, PhotoId};

fn image() -> PhotoId {
    PhotoId::from_db("pht_00000000-0000-4000-8000-000000000021").expect("a photo id")
}

fn sibling() -> PhotoId {
    PhotoId::from_db("pht_00000000-0000-4000-8000-000000000022").expect("a photo id")
}

fn identity() -> IdentityId {
    IdentityId::from_db("idt_00000000-0000-4000-8000-000000000021").expect("an identity")
}

fn plan() -> MicroPlan {
    MicroPlan::nothing(
        image(),
        SceneId::CouplePortrait,
        MicroReason::plain(MicroCode::NoFlyawayFound, 0.0),
    )
}

/// A plan that permits every operation, for the guarantee tests.
fn open_plan() -> MicroPlan {
    let mut plan = plan();
    plan.allowed = [true; 5];
    plan
}

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

#[test]
fn every_reason_code_has_a_slug_and_a_sentence() {
    assert_eq!(MicroCode::ALL.len(), MicroCode::COUNT);
    for code in MicroCode::ALL {
        assert!(!code.as_str().is_empty(), "{code} has no slug");
        assert!(
            code.as_str().starts_with("micro_"),
            "{code} is not namespaced, and phase 13's registry is assembled from every phase"
        );
        assert!(
            code.user_text().len() > 20,
            "{code} has no sentence a photographer could read"
        );
        assert_eq!(MicroCode::parse(code.as_str()), Some(code));
    }
}

#[test]
fn no_two_reason_codes_share_a_slug() {
    let mut slugs: Vec<&str> = MicroCode::ALL.iter().map(|c| c.as_str()).collect();
    slugs.sort_unstable();
    let before = slugs.len();
    slugs.dedup();
    assert_eq!(before, slugs.len(), "two codes share a slug");
}

#[test]
fn two_thirds_of_the_codes_are_withdrawals() {
    // Twenty-two of thirty-three, and the module header says so. A build that adds a code without
    // deciding whether it is a withdrawal changes this number and has to say why - which is the
    // point of asserting it rather than the number itself.
    let doubts = MicroCode::ALL.iter().filter(|c| c.is_doubt()).count();
    assert_eq!(doubts, 22, "the withdrawal set has moved");
}

#[test]
fn every_region_maps_onto_exactly_one_phase_18_class() {
    assert_eq!(MicroRegion::ALL.len(), MicroRegion::COUNT);
    let mut spellings: Vec<&str> = MicroRegion::ALL.iter().map(|r| r.as_mask_str()).collect();
    spellings.sort_unstable();
    let before = spellings.len();
    spellings.dedup();
    assert_eq!(
        before,
        spellings.len(),
        "two micro regions name the same phase 18 class, which would make the mapping a merge"
    );
    for region in MicroRegion::ALL {
        assert_eq!(MicroRegion::parse(region.as_str()), Some(region));
    }
}

#[test]
fn the_read_only_regions_are_the_ones_another_phase_owns() {
    // Skin is phase 20's, and eyes, face and background are bounds rather than targets.
    for region in [
        MicroRegion::Skin,
        MicroRegion::Eyes,
        MicroRegion::Face,
        MicroRegion::Background,
    ] {
        assert!(
            !region.is_writable(),
            "{region} became writable, and phase 20 owns skin"
        );
    }
    for region in [
        MicroRegion::Hair,
        MicroRegion::Teeth,
        MicroRegion::Sclera,
        MicroRegion::Iris,
        MicroRegion::Clothing,
        MicroRegion::Dress,
    ] {
        assert!(region.is_writable(), "{region} cannot be acted on");
    }
}

#[test]
fn the_two_opt_in_clothing_issues_are_named_in_the_type() {
    assert_eq!(ClothingIssue::ALL.len(), ClothingIssue::COUNT);
    assert!(ClothingIssue::Strap.is_opt_in_only());
    assert!(ClothingIssue::Crease.is_opt_in_only());
    for issue in [
        ClothingIssue::Lint,
        ClothingIssue::Thread,
        ClothingIssue::Stain,
    ] {
        assert!(!issue.is_opt_in_only(), "{issue} became opt-in only");
        assert_eq!(ClothingIssue::parse(issue.as_str()), Some(issue));
    }
}

// ---------------------------------------------------------------------------
// What the contract cannot express - the structural half of the ethics policy
// ---------------------------------------------------------------------------

#[test]
fn no_operation_can_express_a_geometry_change() {
    // Section 11 of docs/plan/CLAUDE.md forbids body reshaping, face swapping and eye
    // replacement permanently. The defence that survives a hurried change is that there is
    // nowhere to put one, so this checks the serialised shape rather than the source.
    let ops = [
        MicroOp::Flyaway {
            region: Box2 {
                x: 0.1,
                y: 0.1,
                w: 0.01,
                h: 0.01,
            },
            strength: 0.5,
        },
        MicroOp::Teeth {
            identity: identity(),
            luma: 0.1,
            yellow_reduce: 0.2,
        },
        MicroOp::Eyes {
            identity: identity(),
            sclera: 0.2,
            iris_clarity: 0.1,
        },
        MicroOp::Clothing {
            region: Box2 {
                x: 0.5,
                y: 0.5,
                w: 0.005,
                h: 0.005,
            },
            kind: ClothingIssue::Lint,
            strength: 0.5,
        },
        MicroOp::Glare {
            region: Box2 {
                x: 0.3,
                y: 0.3,
                w: 0.02,
                h: 0.01,
            },
            method: GlareMethod::Reduce { strength: 0.4 },
        },
    ];
    const FORBIDDEN: [&str; 10] = [
        "dx", "dy", "scale", "warp", "landmark", "vertex", "mesh", "shift", "displace", "swap",
    ];
    for op in ops {
        let json = serde_json::to_string(&op).expect("an operation serialises");
        for word in FORBIDDEN {
            assert!(
                !json.contains(word),
                "`{}` carries a `{word}` field, which could express a reshaping",
                op.as_str()
            );
        }
    }
}

#[test]
fn a_borrow_cannot_exist_without_a_disclosed_source() {
    let borrow = GlareMethod::BorrowFrom {
        source: sibling(),
        alignment: 0.9,
    };
    assert!(borrow.is_composite());
    assert_eq!(borrow.source(), Some(sibling()));

    let reduce = GlareMethod::Reduce { strength: 0.4 };
    assert!(!reduce.is_composite());
    assert_eq!(reduce.source(), None);

    // The property that matters: every composite method names its source, so a plan's
    // disclosure list cannot be shorter than its composite count.
    let mut plan = open_plan();
    plan.ops = vec![
        MicroOp::Glare {
            region: Box2 {
                x: 0.3,
                y: 0.3,
                w: 0.02,
                h: 0.01,
            },
            method: borrow,
        },
        MicroOp::Glare {
            region: Box2 {
                x: 0.6,
                y: 0.3,
                w: 0.02,
                h: 0.01,
            },
            method: reduce,
        },
    ];
    assert!(plan.is_composite());
    assert_eq!(plan.borrowed_from(), vec![sibling()]);
    assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
}

// ---------------------------------------------------------------------------
// The ceilings
// ---------------------------------------------------------------------------

#[test]
fn every_ceiling_refuses_the_value_above_it() {
    let over = 1e-3;
    let cases: Vec<(&str, MicroOp)> = vec![
        (
            "flyaway strength",
            MicroOp::Flyaway {
                region: Box2 {
                    x: 0.1,
                    y: 0.1,
                    w: 0.01,
                    h: 0.01,
                },
                strength: MAX_FLYAWAY_STRENGTH + over,
            },
        ),
        (
            "flyaway area",
            MicroOp::Flyaway {
                region: Box2 {
                    x: 0.1,
                    y: 0.1,
                    w: 0.5,
                    h: 0.5,
                },
                strength: 0.2,
            },
        ),
        (
            "teeth luma",
            MicroOp::Teeth {
                identity: identity(),
                luma: MAX_TEETH_LUMA_EV + over,
                yellow_reduce: 0.1,
            },
        ),
        (
            "teeth yellow",
            MicroOp::Teeth {
                identity: identity(),
                luma: 0.1,
                yellow_reduce: MAX_TEETH_YELLOW + over,
            },
        ),
        (
            "sclera",
            MicroOp::Eyes {
                identity: identity(),
                sclera: MAX_SCLERA + over,
                iris_clarity: 0.1,
            },
        ),
        (
            "iris clarity",
            MicroOp::Eyes {
                identity: identity(),
                sclera: 0.1,
                iris_clarity: MAX_IRIS_CLARITY + over,
            },
        ),
        (
            "clothing strength",
            MicroOp::Clothing {
                region: Box2 {
                    x: 0.5,
                    y: 0.5,
                    w: 0.005,
                    h: 0.005,
                },
                kind: ClothingIssue::Lint,
                strength: MAX_CLOTHING_STRENGTH + over,
            },
        ),
        (
            "clothing area",
            MicroOp::Clothing {
                region: Box2 {
                    x: 0.5,
                    y: 0.5,
                    w: 0.2,
                    h: 0.2,
                },
                kind: ClothingIssue::Lint,
                strength: 0.4,
            },
        ),
        (
            "glare reduction",
            MicroOp::Glare {
                region: Box2 {
                    x: 0.3,
                    y: 0.3,
                    w: 0.02,
                    h: 0.01,
                },
                method: GlareMethod::Reduce {
                    strength: MAX_GLARE_REDUCE + over,
                },
            },
        ),
        (
            "borrow area",
            MicroOp::Glare {
                region: Box2 {
                    x: 0.3,
                    y: 0.3,
                    w: 0.2,
                    h: 0.2,
                },
                method: GlareMethod::BorrowFrom {
                    source: sibling(),
                    alignment: 0.95,
                },
            },
        ),
        (
            "borrow alignment",
            MicroOp::Glare {
                region: Box2 {
                    x: 0.3,
                    y: 0.3,
                    w: 0.02,
                    h: 0.01,
                },
                method: GlareMethod::BorrowFrom {
                    source: sibling(),
                    alignment: MIN_ALIGNMENT - 0.05,
                },
            },
        ),
    ];
    for (name, op) in cases {
        assert!(
            op.problem().is_some(),
            "{name} above its ceiling was accepted"
        );
    }
}

#[test]
fn teeth_can_never_be_darkened() {
    // Not a ceiling but a direction. A negative lift arriving here is a solver sign error, and
    // the failure it would produce - somebody's teeth darkened by an automated retoucher - is
    // one nobody would look for.
    let op = MicroOp::Teeth {
        identity: identity(),
        luma: -0.05,
        yellow_reduce: 0.1,
    };
    assert!(op.problem().is_some());
}

#[test]
fn the_guard_refuses_a_table_that_raises_a_ceiling() {
    assert!(NaturalnessGuard::CEILING.is_sound());
    let cases: [(&str, NaturalnessGuard); 5] = [
        (
            "teeth luma",
            NaturalnessGuard {
                teeth_max_luma: MAX_TEETH_LUMA_EV + 0.01,
                ..NaturalnessGuard::CEILING
            },
        ),
        (
            "sclera",
            NaturalnessGuard {
                sclera_max: MAX_SCLERA + 0.01,
                ..NaturalnessGuard::CEILING
            },
        ),
        (
            "iris",
            NaturalnessGuard {
                iris_max: MAX_IRIS_CLARITY + 0.01,
                ..NaturalnessGuard::CEILING
            },
        ),
        (
            "flyaway area",
            NaturalnessGuard {
                flyaway_max_area_frac: MAX_FLYAWAY_AREA + 0.01,
                ..NaturalnessGuard::CEILING
            },
        ),
        (
            "confidence floor",
            NaturalnessGuard {
                require_confidence: MIN_OP_CONFIDENCE - 0.01,
                ..NaturalnessGuard::CEILING
            },
        ),
    ];
    for (name, guard) in cases {
        assert!(
            guard.problem().is_some(),
            "a table raising the {name} bound was accepted"
        );
    }
    // Lowering a ceiling is always permitted: a studio may be more conservative than AURA.
    let cautious = NaturalnessGuard {
        teeth_max_luma: 0.05,
        sclera_max: 0.1,
        iris_max: 0.05,
        flyaway_max_area_frac: 0.001,
        require_confidence: 0.9,
        ..NaturalnessGuard::CEILING
    };
    assert!(cautious.is_sound());
}

// ---------------------------------------------------------------------------
// The locus, which is relative and has no centre-seeking method
// ---------------------------------------------------------------------------

#[test]
fn a_chromaticity_inside_the_locus_is_never_moved() {
    let locus = ColourLocus {
        du: 0.004,
        dv: 0.006,
        radius: 0.02,
    };
    assert!(locus.contains(0.004, 0.006));
    assert_eq!(locus.excess(0.004, 0.006), 0.0);
    assert!(locus.contains(0.010, 0.010));
    // Just outside, and the excess is the distance past the boundary rather than the distance
    // to the centre - which is what makes the operator a reduction rather than a target.
    let far = locus.excess(0.004, 0.040);
    assert!((far - (0.034 - 0.02)).abs() < 1e-5, "excess was {far}");
}

#[test]
fn a_locus_with_an_absolute_looking_centre_is_refused() {
    // A `u'v'` offset of half a unit is off the spectral locus entirely: a file that asks for one
    // has a decimal point in the wrong place, or has stopped expressing an offset and started
    // expressing an absolute chromaticity. Either way it is refused.
    let absurd = ColourLocus {
        du: 0.62,
        dv: 0.35,
        radius: 0.02,
    };
    assert!(absurd.problem().is_some());
    assert!(ColourLocus {
        du: 0.0,
        dv: 0.0,
        radius: 0.0
    }
    .problem()
    .is_some());
    assert!(ColourLocus::OPEN.problem().is_none());
}

// ---------------------------------------------------------------------------
// The plan's own guarantees
// ---------------------------------------------------------------------------

#[test]
fn a_plan_with_no_reason_is_refused() {
    let mut plan = plan();
    plan.reasons.clear();
    assert!(plan.broken_guarantee().is_some());
}

#[test]
fn an_operation_the_matrix_forbade_is_refused() {
    let mut plan = plan();
    // `allowed` is all false on a `nothing` plan, which is the honest default: a plan that did
    // nothing permitted nothing.
    plan.ops = vec![MicroOp::Teeth {
        identity: identity(),
        luma: 0.1,
        yellow_reduce: 0.1,
    }];
    let problem = plan.broken_guarantee().expect("refused");
    assert!(problem.contains("teeth"), "{problem}");

    plan.allowed[1] = true;
    assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
}

#[test]
fn a_withdrawn_family_may_not_still_carry_operations() {
    let mut plan = open_plan();
    plan.ops = vec![MicroOp::Teeth {
        identity: identity(),
        luma: 0.1,
        yellow_reduce: 0.1,
    }];
    assert!(plan.is_sound());

    plan.naturalness.withdrawn[1] = true;
    let problem = plan.broken_guarantee().expect("refused");
    assert!(problem.contains("teeth"), "{problem}");

    // And the point of per-family withdrawal: a withdrawn teeth family leaves clothing alone.
    plan.ops = vec![MicroOp::Clothing {
        region: Box2 {
            x: 0.5,
            y: 0.5,
            w: 0.005,
            h: 0.005,
        },
        kind: ClothingIssue::Lint,
        strength: 0.4,
    }];
    assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
}

#[test]
fn a_plan_above_the_operation_cap_is_refused() {
    let mut plan = open_plan();
    plan.ops = (0..=MAX_OPS)
        .map(|_| MicroOp::Clothing {
            region: Box2 {
                x: 0.5,
                y: 0.5,
                w: 0.005,
                h: 0.005,
            },
            kind: ClothingIssue::Lint,
            strength: 0.4,
        })
        .collect();
    assert!(plan.broken_guarantee().is_some());
}

#[test]
fn each_operation_belongs_to_the_family_that_measures_it() {
    let flyaway = MicroOp::Flyaway {
        region: Box2 {
            x: 0.1,
            y: 0.1,
            w: 0.01,
            h: 0.01,
        },
        strength: 0.2,
    };
    assert_eq!(flyaway.family(), Some(OpFamily::Hair));
    assert_eq!(
        MicroOp::Teeth {
            identity: identity(),
            luma: 0.1,
            yellow_reduce: 0.1
        }
        .family(),
        Some(OpFamily::Teeth)
    );
    assert_eq!(
        MicroOp::Eyes {
            identity: identity(),
            sclera: 0.1,
            iris_clarity: 0.1
        }
        .family(),
        Some(OpFamily::Eyes)
    );
    // Glare shares the eye family, because the guarantee it can break is the catchlight.
    assert_eq!(
        MicroOp::Glare {
            region: Box2 {
                x: 0.3,
                y: 0.3,
                w: 0.02,
                h: 0.01
            },
            method: GlareMethod::Reduce { strength: 0.4 }
        }
        .family(),
        Some(OpFamily::Eyes)
    );
    // Clothing has no naturalness floor: its guarantee is the area cap and the fabric-texture
    // test, both of which are checks on the operation rather than on the rendered region.
    assert_eq!(
        MicroOp::Clothing {
            region: Box2 {
                x: 0.5,
                y: 0.5,
                w: 0.005,
                h: 0.005
            },
            kind: ClothingIssue::Lint,
            strength: 0.4
        }
        .family(),
        None
    );
}

#[test]
fn the_naturalness_report_knows_which_family_it_withdrew() {
    let mut report = NaturalnessReport::UNTOUCHED;
    assert!(report.passed());
    assert!(!report.any_withdrawn());
    report.withdrawn[0] = true;
    assert!(report.is_withdrawn(OpFamily::Hair));
    assert!(!report.is_withdrawn(OpFamily::Teeth));
    assert!(report.any_withdrawn());

    let missed = NaturalnessReport {
        catchlight_ratio: CATCHLIGHT_FLOOR - 0.01,
        ..NaturalnessReport::UNTOUCHED
    };
    assert!(!missed.passed());
    let missed = NaturalnessReport {
        hair_energy_ratio: HAIR_ENERGY_FLOOR - 0.01,
        ..NaturalnessReport::UNTOUCHED
    };
    assert!(!missed.passed());
    let missed = NaturalnessReport {
        teeth_excursion: TEETH_EXCURSION_CEILING * 2.0,
        ..NaturalnessReport::UNTOUCHED
    };
    assert!(!missed.passed());
}

// ---------------------------------------------------------------------------
// The port agrees with phase 19's about how much a doubtful region may do
// ---------------------------------------------------------------------------

fn micro_field(confidence: f32, edge: f32) -> MicroField {
    MicroField {
        region: MicroRegion::Teeth,
        identity: None,
        bounds: Box2 {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        width: 4,
        height: 4,
        alpha: vec![255; 16],
        confidence,
        edge_quality: edge,
        model_ver: 1,
    }
}

fn mask_field(confidence: f32, edge: f32) -> MaskField {
    MaskField {
        kind: MaskKind::Skin,
        identity: None,
        bounds: aura_core::contract::integrity::CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        width: 4,
        height: 4,
        alpha: vec![255; 16],
        confidence,
        edge_quality: edge,
        model_ver: 1,
    }
}

#[test]
fn the_micro_port_gates_exactly_as_phase_19s_does() {
    // ADR-0043 section 7 accepts a three-line duplication in exchange for not widening a frozen
    // enum, and it is only acceptable while the two agree. This is the assertion that makes a
    // change to one that did not move the other fail the build.
    for confidence in [
        0.0,
        MIN_MASK_CONFIDENCE - 0.01,
        MIN_MASK_CONFIDENCE,
        (MIN_MASK_CONFIDENCE + FULL_MASK_CONFIDENCE) / 2.0,
        FULL_MASK_CONFIDENCE,
        1.0,
    ] {
        for edge in [0.0, 0.37, 0.8, 1.0] {
            let micro = micro_field(confidence, edge).strength_scale();
            let local = mask_field(confidence, edge).strength_scale();
            assert!(
                (micro - local).abs() < 1e-6,
                "at confidence {confidence} and edge {edge} the micro port said {micro} and \
                 phase 19's said {local}"
            );
        }
    }
}

#[test]
fn an_unreadable_field_is_refused_rather_than_read_as_empty() {
    let mut field = micro_field(0.9, 0.9);
    field.alpha.pop();
    assert!(field.problem().is_some());
    assert!(!field.is_readable());

    let mut field = micro_field(0.9, 0.9);
    field.width = MicroField::MAX_SIDE + 1;
    assert!(field.problem().is_some());
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

#[test]
fn an_empty_override_is_refused_and_borrowing_is_separate_from_glare() {
    assert!(MicroOverride::default().problem().is_some());

    // A studio can want glare reduced and want no composites delivered. Collapsing the two
    // would force them to choose, which is the failure this field exists to prevent.
    let no_composites = MicroOverride {
        borrowing: Some(false),
        ..MicroOverride::default()
    };
    assert!(no_composites.problem().is_none());
    assert!(!no_composites.is_empty());
    assert!(no_composites.allowed.is_none());
}

#[test]
fn the_specular_floor_is_what_separates_a_repair_from_a_composite() {
    // Not arithmetic - a statement about the numbers. ADR-0043 section 4: you may only borrow
    // pixels that carry no information, so more than half the region has to be blown.
    //
    // Read through bindings so the compiler compares values rather than folding three constant
    // assertions away. Phase 20's `every_plan_says_the_heads_are_untrained` uses the same shape
    // and for the same reason: a test the optimiser deletes is a test that cannot fail.
    let specular = MIN_SPECULAR_FRACTION;
    let borrow_area = MAX_BORROW_AREA;
    let flyaway_area = MAX_FLYAWAY_AREA;
    let clothing_area = MAX_CLOTHING_AREA;
    assert!(
        specular > 0.5,
        "a borrow region that is less than half destroyed still carries the record"
    );
    assert!(borrow_area <= flyaway_area * 2.0);
    assert!(clothing_area < borrow_area);
}
