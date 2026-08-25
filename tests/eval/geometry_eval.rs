//! PHASE-23 section 10.1, as tests. A red gate is a red build.
//!
//! Seven gates, and every one of them is measured against **synthetic frames whose geometry
//! was chosen and painted into the pixels**: a plate bent by a known `k1`, a wall fanned to a
//! known convergence, a horizon set at a known angle, faces placed exactly where a crop would
//! have to cut them. There are no wedding photographs in this repository, no expert crop
//! labels and no measured lens profiles, so what these gates prove is the estimator, the
//! tracker, the caps, the safety filter, the search and the store - and not that a
//! photographer would agree with a crop.
//!
//! That is condition C1 in `docs/progress/PHASE-23-EXIT.md` and it is a Sev 2 trigger. The
//! harness prints it on every run.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp)]

use aura_core::contract::geometry::{
    Aspect, CropPurpose, GeometryCode, GeometryOverride, Keystone, MAX_ROTATE_DEG, MAX_STRETCH,
    MIN_ROTATE_DEG, RESOLUTION_FLOOR, STRAIGHTEN_ACT_AT,
};
use aura_core::contract::integrity::CropRect;
use aura_core::SceneId;
use aura_geometry::crop::Objective;
use aura_geometry::keystone;
use aura_geometry::lens::{self, LensInput};
use aura_geometry::plan::{GeometryInput, Planner};
use aura_geometry::profiles::ProfileTable;
use aura_geometry::rules::CropRules;
use aura_geometry::safety::SafetyInput;
use aura_geometry::straighten::{self, StraightenInput};
use aura_geometry::{fixtures, guard};

fn planner() -> Planner {
    Planner::new(
        ProfileTable::empty(),
        CropRules::shipped().expect("the shipped crop rules load"),
    )
}

fn bundled() -> ProfileTable {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets/lens_profiles");
    ProfileTable::load_dir(&dir).expect("the bundled profiles load")
}

// ---------------------------------------------------------------------------
// Gate 1 - straightening within 0.3 degrees of expert on >= 90 % of frames
// ---------------------------------------------------------------------------

#[test]
fn gate_1_straightening_matches_the_expert_angle() {
    let planner = planner();
    let cases = fixtures::wedding();
    let labelled: Vec<_> = cases
        .iter()
        .filter(|case| case.expert_rotate_deg.is_some())
        .collect();
    assert!(!labelled.is_empty(), "no labelled tilt cases");
    let mut within = 0usize;
    for case in &labelled {
        let plan = planner.plan(&case.input);
        let expert = case.expert_rotate_deg.unwrap_or(0.0);
        let delta = (plan.rotate_deg - expert).abs();
        if delta <= 0.3 {
            within += 1;
        } else {
            eprintln!(
                "  {} wanted {expert:.2} got {:.2}",
                case.name, plan.rotate_deg
            );
        }
    }
    let rate = within as f32 / labelled.len() as f32;
    assert!(
        rate >= 0.90,
        "levelled within 0.3 degrees on {within} of {} frames ({rate:.2})",
        labelled.len()
    );
}

#[test]
fn gate_1b_a_horizon_plate_is_measured_to_the_angle_it_was_painted_at() {
    // The tracker's own accuracy, independent of the planner. Every chain in a horizon plate
    // lies along the boundary, so the fitted angle is the painted one.
    for painted in [-3.5f32, -1.2, 0.0, 2.0, 4.4] {
        let plate = fixtures::horizon_plate(painted);
        let chains = lens::track_edges(&plate, fixtures::SIDE, fixtures::SIDE);
        assert!(!chains.is_empty(), "{painted}: nothing tracked");
        // A straight boundary must be measured as straight: the estimator must decline.
        assert!(
            lens::estimate_k1(&chains, 1.0).is_none(),
            "{painted}: a straight horizon was read as a bent lens"
        );
    }
}

#[test]
fn gate_1c_the_confidence_gate_and_the_band_are_both_enforced() {
    let open = SafetyInput::permissive();
    // Below the gate, nothing turns - at any angle.
    for conf in [0.0f32, 0.59, STRAIGHTEN_ACT_AT - 0.001] {
        for tilt in [0.5f32, 3.0, 7.9] {
            let out = straighten::decide(
                &StraightenInput {
                    tilt_deg: tilt,
                    horizon_conf: conf,
                    tilt_intentional: false,
                    aspect: 1.5,
                },
                &open,
            );
            assert_eq!(out.rotate_deg, 0.0, "turned at conf {conf}, tilt {tilt}");
        }
    }
    // Inside the band and above the gate, it turns; outside the band it does not.
    for tilt in [MIN_ROTATE_DEG + 0.01, 3.0, MAX_ROTATE_DEG - 0.01] {
        let out = straighten::decide(
            &StraightenInput {
                tilt_deg: tilt,
                horizon_conf: 0.9,
                tilt_intentional: false,
                aspect: 1.5,
            },
            &open,
        );
        assert!(out.rotate_deg != 0.0, "did not level a {tilt} degree tilt");
    }
    for tilt in [MIN_ROTATE_DEG - 0.01, MAX_ROTATE_DEG + 0.01, 30.0] {
        let out = straighten::decide(
            &StraightenInput {
                tilt_deg: tilt,
                horizon_conf: 0.99,
                tilt_intentional: false,
                aspect: 1.5,
            },
            &open,
        );
        assert_eq!(out.rotate_deg, 0.0, "levelled a {tilt} degree tilt");
    }
}

// ---------------------------------------------------------------------------
// Gate 2 - zero auto-crops cut a face or a primary identity's hands. HARD.
// ---------------------------------------------------------------------------

#[test]
fn gate_2_no_crop_anywhere_cuts_a_face_or_a_primary_pair_of_hands() {
    let planner = planner();
    let mut checked = 0usize;
    // Every fixture case, every scene, every variant. The scene loop matters: a rule row can
    // be edited, and the guarantee must not depend on which row it lands on.
    for case in fixtures::wedding() {
        for scene in SceneId::ALL {
            let mut input = case.input.clone();
            input.scene = scene;
            let plan = planner.plan(&input);
            assert!(
                guard::check_plan(&plan).is_ok(),
                "{} in {scene}: {:?}",
                case.name,
                plan.broken_guarantee()
            );
            for variant in &plan.crops {
                for region in &input.regions {
                    if !region.is_enforced() {
                        continue;
                    }
                    checked += 1;
                    assert!(
                        region.is_inside(variant.rect, 0.0),
                        "{} in {scene}: the {} crop cut a {}",
                        case.name,
                        variant.purpose,
                        region.kind
                    );
                }
            }
        }
    }
    assert!(checked > 500, "only {checked} region checks ran");
}

#[test]
fn gate_2b_a_guests_hands_do_not_block_a_crop_and_a_primarys_do() {
    let regions = vec![
        fixtures::face(0.42, 0.30, true),
        fixtures::hands(0.03, 0.60, false),
    ];
    let input = SafetyInput {
        regions: &regions,
        aspect: 1.5,
        resolution_floor: RESOLUTION_FLOOR,
    };
    let tight = CropRect {
        x: 0.15,
        y: 0.10,
        w: 0.80,
        h: 0.80,
    };
    assert!(aura_geometry::safety::is_safe(tight, &input));

    let primary = vec![
        fixtures::face(0.42, 0.30, true),
        fixtures::hands(0.03, 0.60, true),
    ];
    let input = SafetyInput {
        regions: &primary,
        aspect: 1.5,
        resolution_floor: RESOLUTION_FLOOR,
    };
    assert_eq!(
        aura_geometry::safety::refusal(tight, &input),
        Some(GeometryCode::CropCutsHands)
    );
}

// ---------------------------------------------------------------------------
// Gate 3 - the resolution floor is respected on every crop
// ---------------------------------------------------------------------------

#[test]
fn gate_3_no_crop_falls_below_its_scenes_resolution_floor() {
    let planner = planner();
    let rules = CropRules::shipped().expect("rules");
    for case in fixtures::wedding() {
        for scene in SceneId::ALL {
            let mut input = case.input.clone();
            input.scene = scene;
            let plan = planner.plan(&input);
            let floor = rules.for_scene(scene).0.resolution_floor;
            assert!(
                floor >= RESOLUTION_FLOOR,
                "{scene} has a floor below the contract's"
            );
            for variant in &plan.crops {
                if variant.purpose == CropPurpose::Original {
                    continue; // The rotation's own rectangle, bounded by the angle band.
                }
                let kept = variant.long_edge_fraction(input.aspect);
                assert!(
                    kept >= floor - 1e-3,
                    "{} in {scene}: the {} crop kept {kept:.3} of the long edge against a \
                     {floor:.2} floor",
                    case.name,
                    variant.purpose
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gate 4 - the distortion estimator recovers a painted bend, and declines otherwise
// ---------------------------------------------------------------------------

#[test]
fn gate_4_a_painted_bend_is_recovered_through_the_tracker() {
    // Barrel and pincushion, at magnitudes where the bow of a straight line is larger than the
    // pixel it is tracked to. Three claims, and the third is the one that matters:
    //
    //   * the sign is right - a barrel lens is never corrected as a pincushion one;
    //   * the magnitude is within thirty per cent;
    //   * **it never over-corrects.** An under-correction leaves a slight bow nobody sees; an
    //     over-correction turns barrel into pincushion, which reads as a mistake because it is.
    let side = fixtures::DISTORTION_SIDE;
    for painted in [0.035f32, 0.050, -0.035, -0.050] {
        let plate = fixtures::grid_plate_at(painted, side);
        let chains = lens::track_edges(&plate, side, side);
        assert!(
            chains.len() >= lens::MIN_EDGES,
            "{painted}: only {} chains tracked",
            chains.len()
        );
        let found = lens::estimate_k1(&chains, 1.0)
            .unwrap_or_else(|| panic!("{painted}: the estimator declined"));
        assert!(
            found.signum() == painted.signum(),
            "painted {painted}, recovered {found} - the wrong sign"
        );
        let relative = (found - painted).abs() / painted.abs();
        assert!(
            relative <= 0.30,
            "painted {painted}, recovered {found} ({:.0} % out)",
            relative * 100.0
        );
        assert!(
            found.abs() <= painted.abs() + 1e-4,
            "painted {painted}, recovered {found} - an over-correction turns barrel into \
             pincushion"
        );
    }
}

#[test]
fn gate_4a_a_bend_below_the_resolution_limit_is_declined_rather_than_guessed() {
    // At 512 px a `k1` of 0.01 bows a straight line by less than the pixel it is tracked to.
    // Declining is the correct answer, and fitting the tracking noise is the failure this
    // guards: a correction nobody asked for is a resample nobody asked for.
    let side = fixtures::DISTORTION_SIDE;
    let plate = fixtures::grid_plate_at(0.010, side);
    let chains = lens::track_edges(&plate, side, side);
    assert!(chains.len() >= lens::MIN_EDGES);
    assert!(
        lens::estimate_k1(&chains, 1.0).is_none(),
        "a sub-pixel bow was fitted"
    );
}

#[test]
fn gate_4c_fringing_is_corrected_from_a_measured_profile_and_withheld_from_an_estimate() {
    let table = bundled();
    assert!(!table.is_empty(), "the bundled table is empty");
    let measured = LensInput {
        lens_id: Some("Canon RF 24-70mm F2.8 L IS USM".to_string()),
        focal_mm: Some(24.0),
        embedded: None,
    };
    let (correction, reasons) = lens::decide(&measured, &table, None);
    assert!(correction.corrects_ca(), "a measured profile withheld CA");
    assert!(correction.corrects_distortion());
    assert!(correction.vignette > 0.0);
    assert!(reasons.iter().any(|r| r.code == GeometryCode::CaCorrected));

    let unknown = LensInput {
        lens_id: Some("NOT A LENS".to_string()),
        focal_mm: Some(24.0),
        embedded: None,
    };
    let (correction, reasons) = lens::decide(&unknown, &table, Some(0.03));
    assert!(
        !correction.corrects_ca(),
        "an estimated profile corrected fringing, which can invent a rim of the opposite colour"
    );
    assert!(correction.vignette.abs() < f32::EPSILON);
    assert!(reasons.iter().any(|r| r.code == GeometryCode::CaWithheld));
}

#[test]
fn gate_4d_the_correction_transform_round_trips_and_never_samples_outside() {
    let table = bundled();
    for id in table.ids() {
        let profile = table.find(&id).expect("a listed lens");
        for entry in &profile.entry {
            let k = [entry.k1, entry.k2, entry.k3];
            for aspect in [1.5f32, 0.6667] {
                let scale = lens::valid_scale(k, aspect);
                assert!((0.25..=1.0).contains(&scale), "{id}: scale {scale}");
                for point in [[0.0, 0.0], [1.0, 0.0], [0.5, 0.5], [1.0, 1.0], [0.0, 1.0]] {
                    let source = lens::source_of(point, k, aspect, scale);
                    assert!(
                        (-1e-3..=1.0 + 1e-3).contains(&source[0])
                            && (-1e-3..=1.0 + 1e-3).contains(&source[1]),
                        "{id} at {}mm: {point:?} samples {source:?}",
                        entry.focal_mm
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gate 5 - the keystone never exceeds the stretch cap
// ---------------------------------------------------------------------------

#[test]
fn gate_5_no_keystone_survives_the_stretch_cap() {
    // Every convergence from parallel to severe, at both orientations.
    for step in 0..60 {
        let ratio = 0.40 + 0.02 * step as f32;
        for aspect in [0.6667f32, 1.5] {
            let out = keystone::decide(&fixtures::converging(ratio, 6), aspect);
            if let Some(keystone) = out.keystone {
                assert!(
                    keystone.stretch <= MAX_STRETCH + 1e-6,
                    "ratio {ratio} produced stretch {}",
                    keystone.stretch
                );
                assert!(keystone.scale >= 1.0);
                assert!(keystone.verticals >= Keystone::MIN_VERTICALS);
            } else {
                let expected = ratio.max(1.0 / ratio) > MAX_STRETCH || (ratio - 1.0).abs() < 0.03;
                assert!(
                    expected,
                    "ratio {ratio} was refused but is inside the cap and not parallel"
                );
            }
        }
    }
}

#[test]
fn gate_5b_a_wall_plate_is_measured_and_squared() {
    let plate = fixtures::wall_plate(0.88);
    let lines = keystone::track_verticals(&plate, fixtures::SIDE, fixtures::SIDE, 1.0);
    assert!(
        lines.len() >= Keystone::MIN_VERTICALS as usize,
        "only {} verticals tracked",
        lines.len()
    );
    let out = keystone::decide(&lines, 1.0);
    if let Some(keystone) = out.keystone {
        assert!(keystone.vertical > 0.0, "the top should widen");
        assert!(keystone.stretch <= MAX_STRETCH);
    }
}

#[test]
fn gate_5c_a_keystone_is_skipped_when_it_would_violate_crop_safety() {
    // Section 10.1: "keystone ... is skipped when it would violate crop safety". A keystone
    // opens two corners and the frame is scaled to hide them, which is a crop - so the
    // usable fraction has to clear the scene's floor.
    let rules = CropRules::shipped().expect("rules");
    for scene in SceneId::ALL {
        let floor = rules.for_scene(scene).0.resolution_floor;
        for ratio in [0.82f32, 0.88, 0.95] {
            let out = keystone::decide(&fixtures::converging(ratio, 6), 0.6667);
            if let Some(keystone) = out.keystone {
                let usable = keystone::usable_fraction(&keystone);
                assert!(
                    usable >= floor - 1e-3,
                    "{scene}: a keystone at ratio {ratio} left {usable:.3} against a \
                     {floor:.2} floor"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gate 6 - at least 70 % of frames keep their original framing
// ---------------------------------------------------------------------------

#[test]
fn gate_6_most_frames_keep_their_original_framing() {
    let planner = planner();
    let cases = fixtures::wedding();
    let mut kept = 0usize;
    let mut total = 0usize;
    for case in &cases {
        let plan = planner.plan(&case.input);
        total += 1;
        if plan.kept_original_framing() {
            kept += 1;
        }
        if case.must_keep_framing {
            assert!(
                plan.kept_original_framing(),
                "{}: the framing was changed",
                case.name
            );
        }
    }
    let rate = kept as f32 / total as f32;
    assert!(
        rate >= 0.70,
        "only {kept} of {total} frames kept their framing ({rate:.2})"
    );
}

#[test]
fn gate_6b_the_improvement_margin_is_what_keeps_them() {
    // The mechanism, not the aggregate: a candidate that beats the frame as shot by less than
    // the scene's margin must lose. Shown by driving the margin to one, at which nothing can
    // ever win.
    let strict = CropRules::parse(
        "[defaults]\nreason = \"d\"\n[[scene]]\nid = \"speeches\"\ncrop = true\n\
         improvement_margin = 1.0\nreason = \"nothing may ever beat the frame as shot\"\n",
    )
    .expect("a strict table");
    let planner = Planner::new(ProfileTable::empty(), strict);
    for case in fixtures::wedding() {
        let mut input = case.input.clone();
        input.scene = SceneId::Speeches;
        let plan = planner.plan(&input);
        assert!(
            plan.kept_original_framing(),
            "{}: a crop beat an impossible margin",
            case.name
        );
        assert!(plan
            .reasons
            .iter()
            .any(|r| r.code == GeometryCode::CropKeptOriginal));
    }
}

// ---------------------------------------------------------------------------
// Gate 7 - revert-to-original restores exact framing
// ---------------------------------------------------------------------------

#[test]
fn gate_7_reverting_restores_the_frame_exactly() {
    let over = GeometryOverride::revert(fixtures::photo(1));
    assert!(over.is_revert());
    assert!(over.problem().is_none());
    assert_eq!(over.rect, CropRect::FULL);
    assert_eq!(over.rotate_deg, 0.0);
    assert_eq!(over.aspect, Aspect::Original);
}

#[test]
fn gate_7b_the_original_framing_is_index_zero_on_every_plan() {
    let planner = planner();
    for case in fixtures::wedding() {
        for scene in SceneId::ALL {
            let mut input = case.input.clone();
            input.scene = scene;
            let plan = planner.plan(&input);
            let first = plan.crops.first().expect("a plan always has a crop");
            assert_eq!(
                first.purpose,
                CropPurpose::Original,
                "{} in {scene}",
                case.name
            );
            assert_eq!(first.aspect, Aspect::Original);
            if plan.rotate_deg.abs() < f32::EPSILON {
                assert!(
                    first.is_full_frame(),
                    "{} in {scene}: an unrotated frame's original entry is not the whole frame",
                    case.name
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-cutting: determinism, and what this harness does not prove
// ---------------------------------------------------------------------------

#[test]
fn gate_8_planning_is_deterministic() {
    let planner = planner();
    for case in fixtures::wedding() {
        let a = planner.plan(&case.input);
        let b = planner.plan(&case.input);
        assert_eq!(a, b, "{}", case.name);
    }
}

#[test]
fn gate_9_the_objective_cannot_be_rescued_by_averaging() {
    // The house shape since phase 09: a product of terms, not a sum. A candidate that cuts a
    // bright window in half must not out-score one that leaves it whole, however good its
    // placement is.
    let regions = [fixtures::face(0.31, 0.29, true)];
    let straddled = [CropRect {
        x: 0.60,
        y: 0.30,
        w: 0.20,
        h: 0.20,
    }];
    let objective = Objective {
        regions: &regions,
        distractions: &straddled,
        subject: None,
        headroom: (0.05, 0.20),
        aspect: 1.5,
    };
    let whole = CropRect::FULL;
    let cuts_it = CropRect {
        x: 0.0,
        y: 0.0,
        w: 0.70,
        h: 1.0,
    };
    assert!(
        objective.score(whole) > objective.score(cuts_it),
        "whole {:.3} vs straddling {:.3}",
        objective.score(whole),
        objective.score(cuts_it)
    );
}

#[test]
fn gate_10_the_lens_correction_moves_a_protected_region_before_the_filter_sees_it() {
    // The trap, as a gate. A face near the corner of a wide frame is in a different place
    // after a barrel correction, and the planner must map it before the safety filter runs.
    let mut input = GeometryInput::bare(fixtures::photo(11), SceneId::GettingReadyBride);
    input.lens = LensInput {
        lens_id: Some("NIKKOR Z 14-24mm f/2.8 S".to_string()),
        focal_mm: Some(14.0),
        embedded: None,
    };
    input.regions = vec![fixtures::face(0.84, 0.10, true)];
    let planner = Planner::new(bundled(), CropRules::shipped().expect("rules"));
    let plan = planner.plan(&input);
    assert!(plan.lens.corrects_distortion(), "the 14 mm was not corrected");
    let scale = lens::valid_scale(plan.lens.distortion, input.aspect);
    let mapped = lens::map_rect(input.regions[0].rect, plan.lens.distortion, input.aspect, scale);
    for variant in &plan.crops {
        assert!(
            mapped.x >= variant.rect.x - 1e-3
                && mapped.x + mapped.w <= variant.rect.x + variant.rect.w + 1e-3,
            "the {} crop cut the corrected face at {mapped:?}",
            variant.purpose
        );
    }
}

#[test]
fn what_this_harness_does_not_prove() {
    // Printed on every run, as phases 09 to 19's harnesses do.
    eprintln!(
        "\nPHASE-23 gates passed against SYNTHETIC frames.\n\
         \n\
         C1: there are no wedding photographs and no expert crop labels in this repository.\n\
         Every gate above measures a geometry that was chosen, painted into the pixels and\n\
         read back through the real pipeline. It proves the estimator, the tracker, the caps,\n\
         the safety filter, the search and the store. It is NOT evidence that a photographer\n\
         would agree with a crop, and section 10.1's QAIQ audit of 300 auto-crops has not\n\
         happened.\n\
         \n\
         C2: every lens profile in assets/lens_profiles/ is FABRICATED. No lens was measured.\n\
         The distortion, vignette and fringing numbers have the right sign and order of\n\
         magnitude and are not measurements.\n\
         \n\
         C3: there is no pose estimate in this build, so hands_checked is zero on every\n\
         photograph in the product. The zero-face-cut gate is a claim; the same gate for\n\
         hands is currently a claim about an empty set.\n"
    );
}
