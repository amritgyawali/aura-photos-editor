//! The phase 25 evaluation gates. Section 10.1, as a test, so a red gate is a red build.
//!
//! Seven gates:
//!
//! 1. Within-node white-balance spread reduced by at least 60 %.
//! 2. Within-node exposure spread reduced by at least 50 %.
//! 3. Per-identity skin dE00 spread at or below 2.0 across the gallery.
//! 4. Idempotence: a second solve moves every frame by less than a small epsilon.
//! 5. Bounds: no frame exceeds the documented maximum movement on any axis.
//! 6. Intentional transitions are not flattened.
//! 7. Outliers are reported with quantified deviations.
//!
//! # What these gates do not prove
//!
//! **Every one of them is measured on a synthetic gallery whose drift was authored.** There are no
//! weddings in this repository and no labelled lighting transitions, so what is proved here is the
//! tree, the change-point detector, the anchor ranking, the robust statistics, the solver, the
//! bounds, the idempotence, the skin arithmetic and the outlier threshold - the *algorithms*. Not
//! one number here is a claim about a photograph.
//!
//! That is condition C1 of `docs/progress/PHASE-25-EXIT.md`, it is a Sev 2 trigger, and it closes
//! with phase 05's condition C10 rather than separately: the anchor ranking reads phase 15's
//! white-balance confidence, which is solved without a learned illuminant head, and the identity
//! term reads phase 06's detector, which finds no faces in a photograph.
//!
//! The skin gate has a second caveat of its own. `SKIN_FIELD_AVAILABLE` is false, so no photograph
//! in this build has an identity-scoped skin region; gate 3 runs against authored readings. It is a
//! measurement of the mechanism, on five *wanderings of a chromaticity*, not on five people. That
//! is condition C2, and `docs/skin-fairness.md` says the same thing in the product's own words.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args, clippy::assertions_on_constants)]

use std::collections::{BTreeMap, BTreeSet};

use aura_brain_gallery::policy::Consistency;
use aura_brain_gallery::skin_consistency::{SkinField, TargetBuilder};
use aura_brain_gallery::tree::{Frame, RawNode};
use aura_brain_gallery::{
    anchors, changepoint, fixtures, normalise, outlier, skin_consistency, stats, tree,
};
use aura_core::contract::gallery::{
    Bound, GalleryCode, ImageId, IDEMPOTENCE_EPSILON, MAX_D_CCT_K, MAX_D_CONTRAST,
    MAX_D_EXPOSURE_EV, MAX_D_SATURATION, MAX_D_TINT, SKIN_DE00_SPREAD_CEILING,
};
use aura_core::contract::ids::NodeId;
use aura_core::{SceneId, SegmentId};

/// Section 10.1's first gate.
const CCT_SPREAD_REDUCTION: f32 = 0.60;

/// Section 10.1's second gate.
const EV_SPREAD_REDUCTION: f32 = 0.50;

/// Solve a whole node and return the deltas, the before spread and the after spread.
///
/// The "after" is measured on the frames' own values *plus* their deltas rather than on the deltas
/// themselves, which is the only measurement that answers the question section 10.1 asks: a gallery
/// is consistent when its frames end up close to each other, not when the corrections were large.
fn solve_node(
    frames: &[Frame],
    scene: SceneId,
    policy: &Consistency,
) -> (Vec<normalise::Solved>, (f32, f32), (f32, f32)) {
    let node = RawNode {
        id: NodeId::new(),
        parent: None,
        segment: frames.first().map_or_else(SegmentId::new, |f| f.segment),
        ordinal: 0,
        siblings: 1,
        scene,
        frames: frames.to_vec(),
        reasons: Vec::new(),
    };
    let scene_policy = policy.scene(scene);
    let anchored = anchors::select(
        &node,
        scene_policy,
        &BTreeSet::new(),
        &BTreeSet::new(),
        policy.target_anchors,
    );
    let confidence = anchors::target_confidence(&anchored);
    let target = anchored.target.expect("the fixture node is anchorable");

    let before_cct: Vec<f32> = frames.iter().filter_map(|f| f.cct_k).collect();
    let before_ev: Vec<f32> = frames
        .iter()
        .filter_map(|f| f.subject_luma)
        .map(|luma| normalise::stops_between(0.45, luma))
        .collect();

    let mut solved = Vec::with_capacity(frames.len());
    let mut after_cct = Vec::with_capacity(frames.len());
    let mut after_ev = Vec::with_capacity(frames.len());
    for frame in frames {
        let one = normalise::solve(frame, node.id, &target, scene_policy, policy, confidence);
        after_cct.push(frame.cct_k.unwrap_or(0.0) + one.delta.d_cct);
        after_ev.push(
            normalise::stops_between(0.45, frame.subject_luma.unwrap_or(0.45))
                + one.delta.d_exposure,
        );
        solved.push(one);
    }

    (
        solved,
        (
            stats::mean_abs_deviation(&before_cct),
            stats::mean_abs_deviation(&before_ev),
        ),
        (
            stats::mean_abs_deviation(&after_cct),
            stats::mean_abs_deviation(&after_ev),
        ),
    )
}

#[test]
fn gate_1_within_scene_wb_spread_is_reduced_by_at_least_sixty_per_cent() {
    let policy = Consistency::default();
    let mut worst = 1.0_f32;
    // Three chapters at three damping factors, so the gate is not met by one lucky scene.
    for (scene, spread) in [
        (SceneId::Ceremony, 600.0_f32),
        (SceneId::FamilyPortrait, 400.0),
        (SceneId::GettingReadyBride, 500.0),
    ] {
        let frames = fixtures::drifting_chapter(SegmentId::new(), scene, 60, spread);
        let (_, before, after) = solve_node(&frames, scene, &policy);
        let reduction = 1.0 - after.0 / before.0;
        println!(
            "gate 1  {scene:?}: {:.1} K -> {:.1} K, {:.1} % reduced",
            before.0,
            after.0,
            reduction * 100.0
        );
        worst = worst.min(reduction);
    }
    assert!(
        worst >= CCT_SPREAD_REDUCTION,
        "worst reduction {:.1} % is below the {:.0} % gate",
        worst * 100.0,
        CCT_SPREAD_REDUCTION * 100.0
    );
}

#[test]
fn gate_2_within_scene_exposure_spread_is_reduced_by_at_least_fifty_per_cent() {
    let policy = Consistency::default();
    let segment = SegmentId::new();
    // A third of a stop peak to peak, which is what a photographer working one room produces: the
    // subject turns, the background changes, the meter follows.
    //
    // **The size of this drift is part of the gate rather than an arbitrary fixture choice.** A
    // reduction gate can only be met while the drift is inside the bound the product is allowed to
    // move a frame by, and the bound is 0.35 EV. A within-node drift of a full stop is not drift at
    // all - it is a lighting change, the change-point detector splits it, and the frames that
    // survive the split become outliers rather than under-corrected. The second half of this test
    // is that claim, measured.
    let frames: Vec<Frame> = (0..60)
        .map(|i| {
            let t = i as f32 / 59.0;
            let mut frame = fixtures::frame_at(segment, i as i64 * 3_000, SceneId::Ceremony);
            frame.subject_luma = Some(0.36 + t * 0.10 + ((i % 5) as f32 - 2.0) * 0.003);
            frame
        })
        .collect();
    let (solved, before, after) = solve_node(&frames, SceneId::Ceremony, &policy);
    let reduction = 1.0 - after.1 / before.1;
    let clamped = solved
        .iter()
        .filter(|one| one.delta.bounded_by == Some(Bound::Exposure))
        .count();
    println!(
        "gate 2  {:.3} EV -> {:.3} EV, {:.1} % reduced, {clamped} clamped",
        before.1,
        after.1,
        reduction * 100.0
    );
    assert_eq!(
        clamped, 0,
        "an in-bound drift should not clamp; the gate would be measuring the bound instead"
    );
    assert!(
        reduction >= EV_SPREAD_REDUCTION,
        "reduction {:.1} % is below the {:.0} % gate",
        reduction * 100.0,
        EV_SPREAD_REDUCTION * 100.0
    );
}

#[test]
fn a_drift_wider_than_the_bound_is_reported_rather_than_half_corrected() {
    // The other half of gate 2, and the reason gate 2's fixture is the size it is. A node whose
    // frames are a full stop apart cannot have its exposure spread halved: the bound is 0.35 EV and
    // the arithmetic does not care what the gate says. What the product must do instead is move
    // every frame as far as it is allowed to and then **say which frames it could not reach**.
    //
    // The same shape as phase 19's edge-gradient halo test, phase 21's chance-corrected margin and
    // phase 22's sharpening kernel floor: a threshold a correct implementation cannot meet is a bug
    // in the threshold, not a finding. Here the threshold is fine and the *fixture* would have been
    // the bug.
    let policy = Consistency::default();
    let segment = SegmentId::new();
    let frames: Vec<Frame> = (0..60)
        .map(|i| {
            let t = i as f32 / 59.0;
            let mut frame = fixtures::frame_at(segment, i as i64 * 3_000, SceneId::Ceremony);
            frame.subject_luma = Some(0.24 + t * 0.42);
            frame
        })
        .collect();
    let (solved, before, after) = solve_node(&frames, SceneId::Ceremony, &policy);
    let node = NodeId::new();
    let clamped = solved
        .iter()
        .filter(|one| one.delta.bounded_by.is_some())
        .count();
    let reported = solved
        .iter()
        .filter_map(|one| outlier::detect(one, node, &policy, false))
        .count();
    println!(
        "        a 1.5-stop node: {:.3} EV -> {:.3} EV, {clamped} clamped, {reported} reported",
        before.1, after.1
    );
    assert!(clamped > 0, "a 1.5-stop drift must hit the bound");
    assert!(
        reported > 0,
        "a frame the bound could not reach must reach the QC queue rather than being silently          half corrected"
    );
    for one in &solved {
        assert!(one.delta.within_bounds());
    }
}

#[test]
fn gate_3_per_identity_skin_spread_is_at_or_below_two_de00_across_the_gallery() {
    let frames = fixtures::wedding();
    // A wander of 0.03 in u'v' is a visible cast: roughly the difference between skin under a warm
    // tungsten room and the same skin under an overcast sky.
    let field = fixtures::AuthoredSkin::new(&frames, [0.240, 0.500], 0.45, 0.030);
    let identity = fixtures::AuthoredSkin::identity();

    let mut builder = TargetBuilder::new();
    for frame in &frames {
        for reading in field.readings(frame.image) {
            builder.add(reading);
        }
    }
    let targets = builder.finish(aura_brain_gallery::ANALYSIS_VER);
    let mut target = *targets
        .get(&identity)
        .expect("the authored identity has enough frames");

    let readings: Vec<_> = frames
        .iter()
        .flat_map(|frame| field.readings(frame.image))
        .collect();
    let corrections: BTreeMap<ImageId, _> = readings
        .iter()
        .filter_map(|reading| {
            skin_consistency::correct(reading, &target).map(|c| (reading.image, c))
        })
        .collect();
    skin_consistency::measure_after(&mut target, &readings, &corrections);

    println!(
        "gate 3  spread {:.2} dE00 -> {:.2} dE00 over {} frames",
        target.spread_before, target.spread_after, target.frames
    );
    assert!(
        target.spread_after <= SKIN_DE00_SPREAD_CEILING,
        "spread after correction is {:.2} dE00, above the {SKIN_DE00_SPREAD_CEILING} gate",
        target.spread_after
    );
    assert!(
        target.spread_after < target.spread_before,
        "the correction did not reduce the spread at all"
    );
}

#[test]
fn gate_4_a_second_solve_moves_every_frame_by_less_than_the_epsilon() {
    let policy = Consistency::default();
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 50, 700.0);
    let (first, ..) = solve_node(&frames, SceneId::Ceremony, &policy);

    // The second pass sees exactly what the first did: the solver reads phase 15's and phase 16's
    // stored answer, which its own output does not touch. That is the mechanism; this is the guard.
    let (second, ..) = solve_node(&frames, SceneId::Ceremony, &policy);

    assert_eq!(first.len(), second.len());
    let mut worst = 0.0_f32;
    for (a, b) in first.iter().zip(second.iter()) {
        assert!(
            a.delta.agrees_with(&b.delta),
            "a frame moved again on the second run"
        );
        worst = worst.max((a.delta.d_cct - b.delta.d_cct).abs() / MAX_D_CCT_K);
    }
    println!("gate 4  worst second-run movement {worst:.6} of a bound");
    assert!(worst <= IDEMPOTENCE_EPSILON);
}

#[test]
fn gate_5_no_frame_exceeds_the_documented_maximum_movement() {
    let policy = Consistency::default();
    // A chapter with an enormous authored spread, so the bounds are the thing being measured rather
    // than the damping. Every frame here wants to move much further than it is allowed to.
    let segment = SegmentId::new();
    let frames: Vec<Frame> = (0..60)
        .map(|i| {
            let t = i as f32 / 59.0;
            let mut frame = fixtures::frame_at(segment, i as i64 * 2_000, SceneId::Ceremony);
            frame.cct_k = Some(2600.0 + t * 5_600.0);
            frame.tint = Some(-40.0 + t * 80.0);
            frame.subject_luma = Some(0.08 + t * 0.60);
            frame.contrast = Some(-60.0 + t * 120.0);
            frame.saturation = Some(-40.0 + t * 80.0);
            frame
        })
        .collect();
    let (solved, ..) = solve_node(&frames, SceneId::Ceremony, &policy);

    let mut clamped = 0usize;
    for one in &solved {
        assert!(
            one.delta.within_bounds(),
            "a delta escaped its bounds: {:?}",
            one.delta
        );
        assert!(one.delta.d_cct.abs() <= MAX_D_CCT_K + f32::EPSILON);
        assert!(one.delta.d_tint.abs() <= MAX_D_TINT + f32::EPSILON);
        assert!(one.delta.d_exposure.abs() <= MAX_D_EXPOSURE_EV + f32::EPSILON);
        assert!(one.delta.d_contrast.abs() <= MAX_D_CONTRAST + f32::EPSILON);
        assert!(one.delta.d_saturation.abs() <= MAX_D_SATURATION + f32::EPSILON);
        if one.delta.bounded_by.is_some() {
            clamped += 1;
        }
    }
    println!("gate 5  {clamped} of {} frames were clamped", solved.len());
    assert!(
        clamped > 0,
        "a bounds gate on a fixture nothing clamps is a gate that proves nothing"
    );
}

#[test]
fn gate_6_an_intentional_lighting_transition_is_not_flattened() {
    let policy = Consistency::default();
    let segment = SegmentId::new();
    let frames = fixtures::transitioning_chapter(segment, SceneId::Ceremony, 24);

    let raw = tree::build(&frames);
    assert_eq!(raw.len(), 1, "the fixture is one segment");
    let split = changepoint::split(&raw[0], policy.split_sigma);
    assert!(
        split.len() >= 2,
        "the change point was missed, so the two halves would share one target"
    );
    assert!(split
        .iter()
        .all(|node| node.reasons.contains(&GalleryCode::NodeSplitByChangePoint)));

    // Each half keeps its own character: the warm half stays warm and the flash half stays cool.
    let mut medians = Vec::new();
    for node in &split {
        let (solved, ..) = solve_node(&node.frames, SceneId::Ceremony, &policy);
        let after: Vec<f32> = node
            .frames
            .iter()
            .zip(solved.iter())
            .map(|(frame, one)| frame.cct_k.unwrap_or(0.0) + one.delta.d_cct)
            .collect();
        medians.push(stats::median(&after).unwrap_or(0.0));
    }
    let gap = medians.iter().copied().fold(f32::MIN, f32::max)
        - medians.iter().copied().fold(f32::MAX, f32::min);
    println!("gate 6  the two halves stay {gap:.0} K apart after normalising");
    assert!(
        gap > 1_800.0,
        "the transition was flattened to {gap:.0} K, so the candle-lit vow was dragged toward the \
         ceremony"
    );
}

#[test]
fn gate_7_outliers_are_reported_with_quantified_deviations() {
    let policy = Consistency::default();
    let frames = fixtures::chapter_with_a_stray(SegmentId::new(), SceneId::Speeches, 40);
    let (solved, ..) = solve_node(&frames, SceneId::Speeches, &policy);
    let node = NodeId::new();

    let mut found: Vec<_> = solved
        .iter()
        .filter_map(|one| outlier::detect(one, node, &policy, false))
        .collect();
    outlier::rank(&mut found);

    println!("gate 7  {} outlier(s)", found.len());
    assert_eq!(found.len(), 1, "one stray was authored");
    let worst = &found[0];
    println!("gate 7  worst: {}", worst.describe());
    assert!(
        worst.residual_cct.abs() > 100.0,
        "the residual was not quantified: {}",
        worst.residual_cct
    );
    let text = worst.describe();
    assert!(text.contains("K"), "{text}");
    assert!(!text.contains("within tolerance"), "{text}");
    assert!(worst
        .reasons
        .iter()
        .any(|r| r.code == GalleryCode::OutlierAfterNormalisation));

    // And the frames that were corrected are not in the queue. A QC queue full of frames the
    // product already fixed is a queue a photographer stops opening.
    assert!(
        found.len() < frames.len() / 4,
        "too many frames reached the queue"
    );
}

#[test]
fn every_reason_code_is_documented_in_the_product_voice() {
    // Not a section 10.1 gate. It is what makes the contract's claim true: `GalleryCode::ALL` says
    // `docs/gallery-consistency.md` is written against it and a test asserts every variant appears
    // there. Section 9 gives DOC "explain gallery consistency, anchors and how to pin them", which
    // is only a finishable job if the codes are enumerable - and only a *finished* one if somebody
    // checks that the enumeration and the document agree.
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gallery-consistency.md"),
    )
    .expect("docs/gallery-consistency.md is readable");

    let mut missing_code = Vec::new();
    let mut missing_sentence = Vec::new();
    for code in GalleryCode::ALL {
        if !doc.contains(code.as_str()) {
            missing_code.push(code.as_str());
        }
        // The sentence too, not only the slug: a table of slugs with the wrong sentences beside
        // them is a document that reads as complete and tells a photographer the wrong thing.
        if !doc.contains(code.user_text()) {
            missing_sentence.push(code.as_str());
        }
    }
    assert!(
        missing_code.is_empty(),
        "codes missing from docs/gallery-consistency.md: {missing_code:?}"
    );
    assert!(
        missing_sentence.is_empty(),
        "codes whose sentence has drifted from the document: {missing_sentence:?}"
    );
    println!(
        "docs: all {} reason codes and their sentences appear in gallery-consistency.md",
        GalleryCode::COUNT
    );
}

#[test]
fn the_gates_print_what_they_do_not_prove() {
    // Not a gate. A test, so the caveat is printed on every run of the harness rather than living
    // only in a document nobody opens with a red build in front of them. Phase 24's `cleanup_eval`
    // does the same.
    println!(
        "\nWhat phase 25's gates do NOT prove\n\
         ----------------------------------\n\
         * Every gallery above is synthetic. There are no weddings in this repository, so the\n\
           drift, the transitions and the skin wander were all authored and read back. These are\n\
           measurements of algorithms, not of photographs. Exit report condition C1, Sev 2.\n\
         * SKIN_FIELD_AVAILABLE is {}. Phase 18's segmentation head is untrained, so no photograph\n\
           in this build has an identity-scoped skin region and gate 3 ran on authored readings.\n\
           It is a measurement of the mechanism on five wanderings of a chromaticity, not on five\n\
           people. Exit report condition C2.\n\
         * The anchor ranking reads phase 15's white-balance confidence and phase 06's face\n\
           detector. Both are placeholder-backed, so 'the best-judged frames' is a claim about the\n\
           ranking rather than about which photographs are best. Closes with phase 05's C10.\n\
         * No photographer has looked at a before-and-after gallery from this build. Section 9's\n\
           QAIQ audit of five weddings did not happen, so the phase's own headline - that a wedding\n\
           reads as one coherent body of work - is unmeasured. Exit report condition C3.\n",
        aura_brain_gallery::SKIN_FIELD_AVAILABLE
    );
}
