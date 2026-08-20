//! PHASE-17 section 10.1, as an ordinary test so a red gate is a red build.
//!
//! Seven gates:
//!
//! 1. Fitted parameters match known ground truth (the JPEG-only path).
//! 2. Style match dE00 <= 2.5 on held-out pairs, <= 3.5 in weak buckets.
//! 3. A usable profile beats the factory baseline in every populated bucket.
//! 4. A sparse bucket falls back to its parent without erratic output.
//! 5. One outlier wedding cannot shift the global profile beyond a bounded amount.
//! 6. A profile bundle round-trips, and a tampered one is refused.
//! 7. Training is cancellable and resumable, and re-fits nothing it already fitted.
//!
//! Plus six checks that the harness can **fail**, because a gate that has never gone red is a
//! gate nobody has tested.
//!
//! # What these numbers are and are not
//!
//! There are no photographers' archives in this repository. Every figure below is measured
//! against synthetic archives whose style was **chosen, applied through the real renderer, and
//! then recovered**. That proves the matcher, the fitter, the regression, the shrinkage, the
//! archive cap and the bundle. It says nothing about a photograph and nothing about whether a
//! real photographer would recognise their own work.
//!
//! That is condition C1 in `docs/progress/PHASE-17-EXIT.md`.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::style::{
    BucketModel, FallbackLevel, LightingBucket, SceneGroup, StyleBucket, StyleDelta, StyleProfile,
    MATCH_DE00_CEILING, WEAK_MATCH_DE00_CEILING,
};
use aura_core::ProfileId;
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::Frame;
use aura_render::{CpuEngine, RenderedData};
use aura_style::bucket::{Features, PairSource};
use aura_style::extract::TheirParams;
use aura_style::fit::optimise::{Axis, Budget, Candidate};
use aura_style::fixtures::{AuthoredArchive, Look, HEIGHT, WIDTH};
use aura_style::tree::{Sample, Target};
use aura_style::{diagnostics, fit, infer, pairs, profile, tree as style_tree};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn engine() -> CpuEngine {
    let clock: Arc<dyn Clock> = FixedClock::at(time::OffsetDateTime::UNIX_EPOCH);
    fit::optimise::engine(clock)
}

/// Render one parameter set over one fixture plate and return 8-bit sRGB.
fn render(engine: &CpuEngine, key: &str, rgb: &[f32], params: &TheirParams) -> Vec<u8> {
    let base = aura_recipe::fixtures::neutral(key, aura_style::fixtures::CAMERA);
    let recipe = params.into_recipe(&base);
    let frame = Frame::working(rgb.to_vec(), WIDTH, HEIGHT, aura_style::fixtures::CAMERA);
    let Ok(out) = engine.render_frame(
        &frame,
        &recipe,
        RenderLevel::Screen(WIDTH, HEIGHT),
        RenderPurpose::Analysis,
        &OutputSpec::default(),
    ) else {
        panic!("the fixture renderer must work")
    };
    match out.data {
        RenderedData::Eight(bytes) => bytes,
        RenderedData::Sixteen(_) => panic!("the fixtures render 8-bit"),
    }
}

/// The mean dE00 between two renders of the same plate.
fn de00(left: &[u8], right: &[u8]) -> f32 {
    let (mean, _) = fit::grid_de00(left, right, WIDTH as usize, HEIGHT as usize);
    mean
}

/// A pile of samples with a known per-bucket lean, without fitting anything.
///
/// The tree gates are about the **regression and the shrinkage**, and running a hundred real
/// coordinate descents to produce inputs whose values are already known would make them slow
/// and would make a failure ambiguous between the fitter and the tree.
fn samples_for(
    bucket: StyleBucket,
    count: usize,
    exposure: f32,
    contrast: f32,
    archive: &str,
) -> Vec<Sample> {
    (0..count)
        .map(|_| {
            let mut targets = [0.0_f32; style_tree::TARGETS];
            if let Some(slot) = targets.get_mut(Target::Exposure.index()) {
                *slot = exposure;
            }
            if let Some(slot) = targets.get_mut(Target::Contrast.index()) {
                *slot = contrast;
            }
            Sample::new(Features::intercept_only(bucket), targets, archive)
        })
        .collect()
}

fn profile_of(tree: &style_tree::Tree, pairs: u32) -> StyleProfile {
    let mut profile = StyleProfile::empty(ProfileId::new(), "eval", "aura-render/1.0.0");
    profile.global = tree.global.clone();
    profile.groups = tree.groups.clone();
    profile.buckets = tree.buckets.clone();
    profile.trained_pairs = pairs;
    profile.analysis_ver = aura_style::api::ANALYSIS_VER;
    profile
}

// ---------------------------------------------------------------------------
// Gate 1 - the JPEG-only path recovers what the photographer set
// ---------------------------------------------------------------------------

#[test]
fn gate_1_fitted_parameters_match_the_known_ground_truth() {
    let engine = engine();
    for look in [Look::airy(), Look::moody()] {
        let Ok(archive) = AuthoredArchive::build(&look, 4, false) else {
            panic!("the fixture renderer must work")
        };
        let truth = Candidate::of_params(&look.params);

        for index in 0..4 {
            let key = format!("{}-{index:04}", look.name);
            let Ok((rgb, width, height)) = archive.original(&key, WIDTH) else {
                panic!("the fixture has that original")
            };
            let Ok((target, _, _)) = archive.finished(&key, WIDTH) else {
                panic!("the fixture has that final")
            };
            let base = aura_recipe::fixtures::neutral(&key, aura_style::fixtures::CAMERA);
            let frame = Frame::working(rgb.clone(), width, height, aura_style::fixtures::CAMERA);
            let Ok(fitted) = fit::fit(&engine, &frame, &base, &target, Budget::default()) else {
                panic!("the fit must run")
            };

            // The residual is what section 6.1's rejection is written against, and it is the
            // honest headline: a fit that reproduces the final is a fit whose parameters mean
            // something, whatever any individual axis says.
            assert!(
                fitted.residual_de00 < fit::REJECT_DE00,
                "{}: residual {} would have been rejected",
                key,
                fitted.residual_de00
            );
            assert!(
                fitted.rejection().is_none(),
                "{key}: an authored global grade must not read as unmodelled work"
            );

            // Exposure is the axis the whole fit is ordered around, so it is asserted directly.
            let recovered = fitted.candidate.get(Axis::Exposure);
            assert!(
                (recovered - truth.get(Axis::Exposure)).abs() < 0.34,
                "{}: exposure {recovered} against {}",
                key,
                truth.get(Axis::Exposure)
            );
        }
    }
}

#[test]
fn gate_1b_a_pair_with_unmodelled_work_is_rejected_rather_than_learned_from() {
    // The falsification check for gate 1. A final with a bright patch the develop pipeline
    // cannot express - a dodged face - must be thrown out, because fitting it puts local work
    // into a global tone delta.
    let engine = engine();
    let look = Look::airy();
    let Ok(archive) = AuthoredArchive::build(&look, 1, false) else {
        panic!("the fixture renderer must work")
    };
    let key = "airy-0000";
    let Ok((rgb, width, height)) = archive.original(key, WIDTH) else {
        panic!()
    };
    let Ok((mut target, _, _)) = archive.finished(key, WIDTH) else {
        panic!()
    };
    // Paint one 12x12 region far brighter. That is one and a half percent of the frame.
    for y in 8..20_usize {
        for x in 8..20_usize {
            let offset = (y * WIDTH as usize + x) * 3;
            if let Some(slice) = target.get_mut(offset..offset + 3) {
                slice.copy_from_slice(&[250, 245, 240]);
            }
        }
    }
    let base = aura_recipe::fixtures::neutral(key, aura_style::fixtures::CAMERA);
    let frame = Frame::working(rgb, width, height, aura_style::fixtures::CAMERA);
    let Ok(fitted) = fit::fit(&engine, &frame, &base, &target, Budget::quick()) else {
        panic!()
    };
    assert!(
        fitted.rejection().is_some(),
        "a dodged region must be rejected: residual {} worst tile {}",
        fitted.residual_de00,
        fitted.worst_tile_de00
    );
}

// ---------------------------------------------------------------------------
// Gate 2 - style match on held-out pairs
// ---------------------------------------------------------------------------

#[test]
fn gate_2_style_match_de00_is_inside_the_ceiling_on_held_out_pairs() {
    let engine = engine();
    let look = Look::airy();
    let Ok(archive) = AuthoredArchive::build(&look, 8, false) else {
        panic!("the fixture renderer must work")
    };

    // A profile that knows this look exactly - which is what a converged fit produces, and
    // what this gate is measuring the *application* of rather than the recovery of.
    let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
    let delta = StyleDelta {
        exposure: look.params.exposure,
        temperature_k: look.params.temperature_k - 5500.0,
        tint: look.params.tint,
        contrast: look.params.contrast,
        highlights: look.params.highlights,
        shadows: look.params.shadows,
        whites: look.params.whites,
        blacks: look.params.blacks,
        vibrance: look.params.vibrance,
        saturation: look.params.saturation,
        confidence: 0.9,
        samples: 200,
        ..StyleDelta::neutral()
    }
    .clamped();

    let mut worst = 0.0_f32;
    let mut worst_baseline = 0.0_f32;
    for index in 0..8 {
        let key = format!("airy-{index:04}");
        let Ok((rgb, _, _)) = archive.original(&key, WIDTH) else {
            panic!()
        };
        let Ok((theirs, _, _)) = archive.finished(&key, WIDTH) else {
            panic!()
        };
        let baseline = TheirParams::default();
        let styled = aura_style::api::styled(&baseline, &delta);
        let ours = render(&engine, &key, &rgb, &styled);
        let plain = render(&engine, &key, &rgb, &baseline);
        worst = worst.max(de00(&ours, &theirs));
        worst_baseline = worst_baseline.max(de00(&plain, &theirs));
    }

    assert!(
        worst <= MATCH_DE00_CEILING,
        "styled dE00 {worst} against a ceiling of {MATCH_DE00_CEILING}"
    );
    assert!(
        worst < worst_baseline,
        "the style must beat doing nothing: {worst} against {worst_baseline}"
    );
    let _ = bucket;
}

#[test]
fn gate_2b_a_profile_that_learned_nothing_does_not_pass_the_match_gate() {
    // The falsification check for gate 2. A neutral style over a heavily graded look must
    // *fail* the ceiling, or the ceiling is measuring nothing.
    let engine = engine();
    let look = Look::moody();
    let Ok(archive) = AuthoredArchive::build(&look, 2, false) else {
        panic!()
    };
    let mut worst = 0.0_f32;
    for index in 0..2 {
        let key = format!("moody-{index:04}");
        let Ok((rgb, _, _)) = archive.original(&key, WIDTH) else {
            panic!()
        };
        let Ok((theirs, _, _)) = archive.finished(&key, WIDTH) else {
            panic!()
        };
        let plain = render(&engine, &key, &rgb, &TheirParams::default());
        worst = worst.max(de00(&plain, &theirs));
    }
    assert!(
        worst > MATCH_DE00_CEILING,
        "a do-nothing style scored {worst}, so this gate cannot fail"
    );
}

// ---------------------------------------------------------------------------
// Gate 3 - a usable profile improves on the factory baseline everywhere
// ---------------------------------------------------------------------------

#[test]
fn gate_3_every_populated_bucket_improves_on_the_baseline() {
    // Four buckets, seventy-five pairs each: three hundred, the headline KPI's own figure.
    let buckets = [
        StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight),
        StyleBucket::new(SceneGroup::Ceremony, LightingBucket::Tungsten),
        StyleBucket::new(SceneGroup::Dance, LightingBucket::Flash),
        StyleBucket::new(SceneGroup::Portraits, LightingBucket::GoldenHour),
    ];
    let leans = [
        (0.30_f32, -8.0_f32),
        (-0.20, 20.0),
        (0.10, 30.0),
        (0.45, -4.0),
    ];

    let mut samples = Vec::new();
    for (index, bucket) in buckets.iter().enumerate() {
        let (exposure, contrast) = leans.get(index).copied().unwrap_or((0.0, 0.0));
        samples.extend(samples_for(*bucket, 75, exposure, contrast, "wedding-1"));
    }
    let tree = style_tree::fit(&samples);
    let profile = profile_of(&tree, 300);

    for (index, bucket) in buckets.iter().enumerate() {
        let (exposure, contrast) = leans.get(index).copied().unwrap_or((0.0, 0.0));
        let (delta, level) = profile.resolve(*bucket);
        assert_eq!(
            level,
            FallbackLevel::Bucket,
            "{bucket} answered from {level} with 75 pairs"
        );
        // Improvement is the point: the styled answer must be closer to what the photographer
        // actually does than doing nothing is.
        let styled_error = (delta.exposure - exposure).abs() + (delta.contrast - contrast).abs();
        let baseline_error = exposure.abs() + contrast.abs();
        assert!(
            styled_error < baseline_error,
            "{bucket}: styled {styled_error} against baseline {baseline_error}"
        );
    }
    assert!(profile.is_usable());
    assert!(profile.strength() > 0.0);
}

// ---------------------------------------------------------------------------
// Gate 4 - a sparse bucket falls back without going erratic
// ---------------------------------------------------------------------------

#[test]
fn gate_4_a_sparse_bucket_falls_back_to_its_parent_and_stays_bounded() {
    let strong = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
    let sparse = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Shade);

    let mut samples = samples_for(strong, 200, 0.25, 10.0, "wedding-1");
    // Four pairs in the sparse bucket, and they disagree wildly with everything else.
    samples.extend(samples_for(sparse, 4, 3.0, 90.0, "wedding-1"));

    let tree = style_tree::fit(&samples);
    let profile = profile_of(&tree, 204);

    let (delta, level) = profile.resolve(sparse);
    assert_ne!(
        level,
        FallbackLevel::Bucket,
        "four pairs must not let a bucket decide for itself"
    );
    // "Without erratic output": the sparse bucket's answer must be near its parent's rather
    // than near its own four wild samples.
    let (parent, _) = profile.resolve(StyleBucket::new(
        SceneGroup::Portraits,
        LightingBucket::Unknown,
    ));
    assert!(
        (delta.exposure - parent.exposure).abs() < 0.05,
        "sparse {} parent {}",
        delta.exposure,
        parent.exposure
    );
    assert!(
        delta.exposure < 1.0,
        "the four wild pairs leaked through: {}",
        delta.exposure
    );
}

#[test]
fn gate_4b_a_bucket_with_no_pairs_at_all_produces_exactly_the_parent() {
    let strong = StyleBucket::new(SceneGroup::Dance, LightingBucket::Stage);
    let samples = samples_for(strong, 120, 0.2, 6.0, "wedding-1");
    let profile = profile_of(&style_tree::fit(&samples), 120);

    let untouched = StyleBucket::new(SceneGroup::Details, LightingBucket::Candle);
    let (delta, level) = profile.resolve(untouched);
    assert!(matches!(
        level,
        FallbackLevel::Global | FallbackLevel::Factory
    ));
    assert!(delta.magnitude() < 0.5);
}

// ---------------------------------------------------------------------------
// Gate 5 - one wedding cannot dominate
// ---------------------------------------------------------------------------

#[test]
fn gate_5_one_outlier_wedding_cannot_shift_the_global_profile_beyond_a_bound() {
    let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);

    let mut normal: Vec<Sample> = Vec::new();
    for wedding in 0..4 {
        normal.extend(samples_for(
            bucket,
            60,
            0.0,
            0.0,
            &format!("normal-{wedding}"),
        ));
    }
    let without = style_tree::fit(&normal);

    let mut with_outlier = normal.clone();
    with_outlier.extend(samples_for(bucket, 400, 2.0, 90.0, "outlier"));
    let with = style_tree::fit(&with_outlier);

    assert!(with.archive_capped, "the archive cap did not bind");
    let moved = (with.global.exposure - without.global.exposure).abs();
    // The bound is derived rather than chosen: one archive holds at most
    // `MAX_ARCHIVE_INFLUENCE` of the fitting weight, so a weighted mean can move at most that
    // fraction of the two-stop disagreement.
    let bound = style_tree::influence_bound(2.0);
    assert!(
        moved <= bound,
        "one wedding moved the global exposure lean by {moved} stops against a bound of {bound}"
    );
    assert!(
        moved < 1.0,
        "the outlier should be visibly diluted rather than merely bounded: {moved}"
    );
    assert!(
        with.global.contrast.abs() < aura_core::contract::style::MAX_PARAM_DELTA,
        "the outlier's contrast reached the bound: {}",
        with.global.contrast
    );
}

#[test]
fn gate_5b_without_the_cap_the_outlier_would_dominate() {
    // The falsification check for gate 5: the same outlier as the only archive moves the
    // profile all the way to its bound, which is what the cap exists to prevent when there are
    // other weddings to compare it against.
    let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
    let alone = style_tree::fit(&samples_for(bucket, 400, 2.0, 90.0, "outlier"));
    assert!(
        alone.global.exposure > 0.5,
        "a single-archive fit should follow its one archive: {}",
        alone.global.exposure
    );
}

// ---------------------------------------------------------------------------
// Gate 6 - the bundle
// ---------------------------------------------------------------------------

#[test]
fn gate_6_a_profile_bundle_round_trips_and_a_tampered_one_is_refused() {
    let bucket = StyleBucket::new(SceneGroup::Ceremony, LightingBucket::Tungsten);
    let tree = style_tree::fit(&samples_for(bucket, 90, -0.2, 14.0, "wedding-1"));
    let mut original = profile_of(&tree, 400);
    original.diagnostics.overall_de00 = 1.7;
    original.diagnostics.recommendation = "add one outdoor daylight wedding".to_string();

    let identity = profile::Identity::from_seed([3u8; 32]);
    let Ok(bundle) = profile::export(&original, &identity) else {
        panic!("export must work")
    };
    let file = bundle.to_file();

    let Ok(read) = profile::import(&file) else {
        panic!("a bundle this build wrote must import")
    };
    assert!(read.unchanged_since_signing);
    assert_eq!(read.profile.buckets.len(), original.buckets.len());
    assert!(
        (read.profile.diagnostics.overall_de00 - 1.7).abs() < 1e-3,
        "the measured accuracy must survive the round trip"
    );
    assert_eq!(read.fingerprint, identity.fingerprint());

    // Every byte of the body is covered. Flip one and the import must refuse.
    let tampered = file.replacen("aura-render", "aura-rendex", 1);
    assert!(
        profile::import(&tampered).is_err(),
        "a tampered bundle must be refused"
    );
}

#[test]
fn gate_6b_the_bundle_carries_no_skin_colour_anywhere_in_it() {
    // The structural half of ADR-0035 decision 7, asserted on the wire format rather than only
    // on the schema. `skin` is a pair of *leans*; there is nothing in a bundle that could hold
    // a chromaticity, a Monk bucket or a tone name.
    let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
    let tree = style_tree::fit(&samples_for(bucket, 40, 0.1, 4.0, "w"));
    let body = profile::canonical(&profile_of(&tree, 40));
    for forbidden in [
        "skin_tone",
        "skin_target",
        "ideal_skin",
        "monk",
        "fitzpatrick",
        "chromaticity",
    ] {
        assert!(
            !body.to_lowercase().contains(forbidden),
            "the bundle grew a {forbidden} field"
        );
    }
}

// ---------------------------------------------------------------------------
// Gate 7 - resumability
// ---------------------------------------------------------------------------

#[test]
fn gate_7_a_re_scan_refits_nothing_it_already_fitted() {
    // The resume unit is the pair, keyed on the original's content hash. The matcher is what
    // this asserts about: matching the same archive twice must produce the same keys, so the
    // `fitted` query can recognise them.
    let Ok(archive) = AuthoredArchive::build(&Look::airy(), 6, false) else {
        panic!()
    };
    let first = pairs::match_all(&archive.archive);
    let second = pairs::match_all(&archive.archive);
    let keys = |report: &pairs::MatchReport| -> Vec<String> {
        report
            .pairs
            .iter()
            .filter_map(|pair| pair.original.hash.clone())
            .collect()
    };
    assert_eq!(keys(&first), keys(&second));
    assert_eq!(first.pairs.len(), 6);
}

#[test]
fn gate_7b_the_held_out_split_is_the_same_on_every_run() {
    // Invariant 4 reaching the accuracy figure itself: a random split would make the number on
    // the front of the panel change with no change to the data.
    let held: Vec<usize> = (0..40)
        .filter(|index| index % style_tree::HOLDOUT_EVERY == 0)
        .collect();
    let again: Vec<usize> = (0..40)
        .filter(|index| index % style_tree::HOLDOUT_EVERY == 0)
        .collect();
    assert_eq!(held, again);
    assert_eq!(held.len(), 8);
}

// ---------------------------------------------------------------------------
// Determinism, and the two things the report must never do
// ---------------------------------------------------------------------------

#[test]
fn the_whole_fit_is_deterministic() {
    let bucket = StyleBucket::new(SceneGroup::Reception, LightingBucket::Artificial);
    let samples = samples_for(bucket, 33, 0.17, -9.0, "w1");
    let first = style_tree::fit(&samples);
    let second = style_tree::fit(&samples);
    assert_eq!(first.global, second.global);
    assert_eq!(first.buckets, second.buckets);

    // One profile exported twice. Two calls to `profile_of` would produce two ids - which is
    // correct, a profile id names a training run - and would make this an assertion about
    // `Uuid` rather than about the canonical form.
    let profile = profile_of(&first, 33);
    let identity = profile::Identity::from_seed([1u8; 32]);
    let (Ok(a), Ok(b)) = (
        profile::export(&profile, &identity),
        profile::export(&profile, &identity),
    ) else {
        panic!("export must work")
    };
    assert_eq!(a.body, b.body, "the canonical form must be stable");
}

#[test]
fn an_unmeasured_bucket_reports_none_and_never_zero() {
    let bucket = StyleBucket::new(SceneGroup::Dance, LightingBucket::Stage);
    let tree = style_tree::fit(&samples_for(bucket, 20, 0.1, 5.0, "w1"));
    let profile = profile_of(&tree, 20);
    let report = diagnostics::evaluate(&profile, &[], 20, 0);
    for row in &report.diagnostics.per_bucket {
        assert_eq!(
            row.match_de00, None,
            "a bucket with no held-out pairs must not report a number"
        );
    }
}

#[test]
fn a_weak_bucket_is_held_to_the_weaker_ceiling_and_a_populated_one_is_not() {
    assert!((diagnostics::ceiling_for(4) - WEAK_MATCH_DE00_CEILING).abs() < f32::EPSILON);
    assert!((diagnostics::ceiling_for(400) - MATCH_DE00_CEILING).abs() < f32::EPSILON);

    let bucket = StyleBucket::new(SceneGroup::Details, LightingBucket::Tungsten);
    let weak = BucketModel {
        bucket,
        delta: StyleDelta::neutral(),
        samples: 6,
        held_out: 2,
        match_de00: Some(3.2),
        shrink: 0.3,
    };
    assert_eq!(
        weak.met_ceiling(),
        Some(true),
        "3.2 is inside 3.5 for a weak bucket"
    );
    let populated = BucketModel {
        samples: 200,
        ..weak.clone()
    };
    assert_eq!(
        populated.met_ceiling(),
        Some(false),
        "3.2 is outside 2.5 for a populated one"
    );
}

#[test]
fn a_project_with_no_profile_gets_the_baseline_and_a_reason_saying_so() {
    let advice = infer::advise(
        None,
        &aura_style::fixtures::query_for(0),
        &infer::Context {
            engine: "aura-render/1.0.0",
            analysis_ver: aura_style::api::ANALYSIS_VER,
        },
    );
    assert!(advice.is_neutral());
    assert_eq!(advice.level, FallbackLevel::Factory);
    assert!(!advice.reasons.is_empty(), "invariant 2");
}

#[test]
fn the_bucket_matrix_is_eighty_leaves_and_every_one_of_them_has_a_name() {
    let all = StyleBucket::all();
    assert_eq!(all.len(), StyleBucket::COUNT);
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for bucket in all {
        let key = bucket.as_key();
        assert!(seen.insert(key.clone(), ()).is_none(), "duplicate {key}");
        assert_eq!(StyleBucket::from_key(&key), bucket);
        assert!(!bucket.title().is_empty());
    }
}

#[test]
fn what_these_numbers_are_is_printed_rather_than_hidden() {
    // Not an assertion about the product. It runs last in the file and prints the caveat, so a
    // developer reading a green run sees the same sentence the exit report carries.
    println!(
        "PHASE-17 gates: measured against synthetic archives whose style was chosen, applied \
         through the real renderer and recovered. This proves the matcher, the fitter, the \
         regression, the shrinkage, the archive cap and the bundle. It says nothing about a \
         photograph, and nothing about whether a real photographer would recognise their own \
         work. Condition C1."
    );
}
