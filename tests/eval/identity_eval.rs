//! The phase 06 evaluation harness: section 10.1's gates, and the proof that the
//! harness itself would catch a bad model.
//!
//! Named by section 4 of the phase document as `ml/models/face/eval_identity.py`'s Rust
//! counterpart, and wired in as a test target of `aura-people`, so
//! `cargo test --workspace` runs it and CI cannot forget it.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets six gates. Every one of them is a statement about a *trained model*,
//! and the three models this build ships are placeholders with the right architecture and
//! no training (condition **C1** in `docs/progress/PHASE-06-EXIT.md`). Measuring them
//! against the shipped weights would produce six numbers describing a random-number
//! generator, and a gate that passes for the wrong reason is worse than one that is
//! honestly deferred.
//!
//! So this file does the useful half, which is the same shape phase 05's harness took:
//!
//! 1. **It measures every metric** with the arithmetic
//!    `ml/models/face/eval_identity.py` uses, so the day a model is trained the two
//!    numbers can be compared rather than argued about.
//! 2. **It gates them on ground truth whose structure is known by construction** - the
//!    synthetic faces and templates in `aura_vision::face::fixtures` - which proves the
//!    harness would fail a model that had learned nothing. A broken metric passes
//!    silently for ever otherwise.
//! 3. **It gates, for real and today, everything that does not depend on the weights**:
//!    the anchor decode, the suppression, the bokeh rejection, the alignment, the quality
//!    gate's monotonicity, the clustering algorithm, and determinism.
//!
//! Every deferred gate is printed with the reason it is deferred, so a reader of the CI
//! log sees the same list the exit report carries.

use aura_vision::face::cluster::{self, ClusterConfig, ClusterFace};
use aura_vision::face::detect::{self, DetectConfig};
use aura_vision::face::fixtures::{self, synthetic_template, ReferenceDetector, SynthFace};

// -- section 10.1's thresholds, in one place ---------------------------------

/// Detection recall at IoU 0.5, overall.
const RECALL_GATE: f64 = 0.97;
/// Detection recall on the small-face subset.
const SMALL_FACE_RECALL_GATE: f64 = 0.90;
/// False positives on bokeh highlights, as a fraction of detections.
const BOKEH_FALSE_POSITIVE_GATE: f64 = 0.01;
/// Identity clustering F1, pairwise.
const CLUSTER_F1_GATE: f32 = 0.93;
/// Clusters containing more than one labelled person.
const IMPURITY_GATE: u32 = 0;

/// A frame with `count` faces of decreasing size, laid out on a grid.
fn crowd(count: usize, height: f32) -> Vec<SynthFace> {
    let columns = 5usize;
    (0..count)
        .map(|index| {
            let column = index % columns;
            let row = index / columns;
            SynthFace::new(
                [0.12 + column as f32 * 0.19, 0.16 + row as f32 * 0.26],
                height,
                index as u32 + 1,
            )
        })
        .collect()
}

fn detect_all(faces: &[SynthFace], width: usize, height: usize) -> Vec<detect::Detection> {
    let rgb = fixtures::render_frame(width, height, faces);
    detect::nms(
        ReferenceDetector::new().detect(&rgb, width, height),
        &DetectConfig::default(),
    )
}

#[test]
fn detection_recall_meets_the_section_ten_gate() {
    // Twenty faces at a comfortable size, on a 1600x1200 frame.
    let faces = crowd(20, 0.13);
    let found = detect_all(&faces, 1600, 1200);
    let (matched, total) = fixtures::recall_at_iou(&faces, &found, 0.5);
    let recall = matched as f64 / total.max(1) as f64;

    println!("detection recall at IoU 0.5: {recall:.4} ({matched}/{total})");
    assert!(
        recall >= RECALL_GATE,
        "recall {recall:.4} below the {RECALL_GATE} gate"
    );
}

#[test]
fn small_face_recall_meets_the_section_ten_gate() {
    // Faces at 4 % of frame height - about 48 px on a 1200 px frame, which is exactly
    // where the identity gate sits and where a detector starts to struggle.
    let faces = crowd(15, 0.04);
    let found = detect_all(&faces, 1600, 1200);
    let (matched, total) = fixtures::recall_at_iou(&faces, &found, 0.5);
    let recall = matched as f64 / total.max(1) as f64;

    println!("small-face recall at IoU 0.5: {recall:.4} ({matched}/{total})");
    assert!(
        recall >= SMALL_FACE_RECALL_GATE,
        "small-face recall {recall:.4} below the {SMALL_FACE_RECALL_GATE} gate"
    );
}

#[test]
fn bokeh_false_positives_meet_the_section_ten_gate() {
    // Ten real faces and thirty face-coloured, structureless highlights - a string of
    // fairy lights across the back wall, which is the canonical wedding false positive.
    let mut faces = crowd(10, 0.11);
    for index in 0..30 {
        let column = index % 6;
        let row = index / 6;
        faces.push(SynthFace::bokeh(
            [0.08 + column as f32 * 0.16, 0.80 - row as f32 * 0.06],
            0.03 + (index % 3) as f32 * 0.01,
        ));
    }

    let found = detect_all(&faces, 1600, 1200);
    let false_positives = fixtures::false_positives(&faces, &found, 0.5);
    let rate = false_positives as f64 / found.len().max(1) as f64;

    println!(
        "bokeh false positives: {false_positives} of {} detections ({rate:.4})",
        found.len()
    );
    assert!(
        rate <= BOKEH_FALSE_POSITIVE_GATE,
        "false-positive rate {rate:.4} above the {BOKEH_FALSE_POSITIVE_GATE} gate"
    );
}

#[test]
fn clustering_meets_the_section_ten_gates() {
    // A wedding: fourteen people, an uneven number of faces each, at the intra-identity
    // spread an ArcFace-class recogniser produces.
    let mut key = 0u64;
    let mut faces = Vec::new();
    let mut truth = Vec::new();
    for identity in 0..14u32 {
        let count = 4 + (identity % 6) * 2;
        for variation in 0..count {
            faces.push(ClusterFace {
                key,
                template: synthetic_template(identity, variation, 0.40, None),
                quality: 0.80,
                votes: true,
            });
            truth.push(Some(identity));
            key += 1;
        }
    }

    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    let predicted = cluster::assignment_labels(faces.len(), &outcome);
    let metrics = cluster::pairwise_metrics(&truth, &predicted);

    println!(
        "clustering: {} identities, F1 {:.4}, precision {:.4}, recall {:.4}, impure {}, \
         unassigned {}, refused merges {}",
        outcome.clusters.len(),
        metrics.f1,
        metrics.precision,
        metrics.recall,
        metrics.impure_clusters,
        outcome.unassigned.len(),
        outcome.refused_merges
    );

    assert!(
        metrics.f1 >= CLUSTER_F1_GATE,
        "F1 {:.4} below the {CLUSTER_F1_GATE} gate",
        metrics.f1
    );
    assert_eq!(
        metrics.impure_clusters, IMPURITY_GATE,
        "section 10.1 states impurity as an absolute, not as an average"
    );
}

#[test]
fn the_clustering_metric_rejects_a_useless_recogniser() {
    // The proof that the gate above would fail a model that had learned nothing. Templates
    // drawn independently of identity: an honest metric must score near zero on them, and
    // a metric that scored well here would pass a broken model for ever.
    let mut key = 0u64;
    let mut faces = Vec::new();
    let mut truth = Vec::new();
    for identity in 0..8u32 {
        for variation in 0..6u32 {
            faces.push(ClusterFace {
                key,
                // The identity is ignored: every template is its own random direction.
                template: synthetic_template(1_000 + key as u32, variation, 0.9, None),
                quality: 0.8,
                votes: true,
            });
            truth.push(Some(identity));
            key += 1;
        }
    }

    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    let predicted = cluster::assignment_labels(faces.len(), &outcome);
    let metrics = cluster::pairwise_metrics(&truth, &predicted);

    println!(
        "useless recogniser: F1 {:.4} over {} identities",
        metrics.f1,
        outcome.clusters.len()
    );
    assert!(
        metrics.f1 < CLUSTER_F1_GATE,
        "a recogniser that learned nothing must not pass the gate, scored {:.4}",
        metrics.f1
    );
}

#[test]
fn siblings_are_not_merged() {
    // Section 10.1 states this separately from F1, and it has to be separate: a single
    // sibling merge in a large wedding barely moves F1 and is still the failure the
    // photographer sees.
    let mut key = 0u64;
    let mut faces = Vec::new();
    let mut truth = Vec::new();
    for (identity, blend) in [(0u32, None), (1, Some((0u32, 0.40f32)))] {
        for variation in 0..10u32 {
            faces.push(ClusterFace {
                key,
                template: synthetic_template(
                    identity + 50,
                    variation,
                    0.485,
                    blend.map(|(a, b)| (a + 50, b)),
                ),
                quality: 0.85,
                votes: true,
            });
            truth.push(Some(identity));
            key += 1;
        }
    }

    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    let predicted = cluster::assignment_labels(faces.len(), &outcome);
    let metrics = cluster::pairwise_metrics(&truth, &predicted);

    println!(
        "siblings: {} identities, impure {}, refused merges {}",
        outcome.clusters.len(),
        metrics.impure_clusters,
        outcome.refused_merges
    );
    assert_eq!(metrics.impure_clusters, 0, "two siblings were merged");
}

#[test]
fn the_whole_pass_is_deterministic() {
    // Invariant 4, at the level a phase gate can assert: the same pixels through the same
    // detector produce the same boxes, the same landmarks and the same face ids.
    let faces = crowd(8, 0.12);
    let first = detect_all(&faces, 1200, 900);
    let second = detect_all(&faces, 1200, 900);

    assert_eq!(first.len(), second.len());
    for (left, right) in first.iter().zip(second.iter()) {
        assert_eq!(left.face.x.to_bits(), right.face.x.to_bits());
        assert_eq!(left.face.y.to_bits(), right.face.y.to_bits());
        assert_eq!(left.landmarks, right.landmarks);
    }

    let photo = aura_core::PhotoId::new();
    for detection in &first {
        assert_eq!(
            aura_vision::face::derive_face_id(photo, &detection.face, detect::MODEL_VER),
            aura_vision::face::derive_face_id(photo, &detection.face, detect::MODEL_VER),
            "a face id is derived, so it must be stable"
        );
    }
}

#[test]
fn a_face_id_moves_when_the_detector_version_moves() {
    // The other half of the derivation: a `model_ver` bump must produce different ids, or
    // a re-scan after a model change would overwrite the old rows in place and the two
    // versions would be indistinguishable.
    let box_norm = detect::NormBox::from_corners(0.2, 0.2, 0.3, 0.34);
    let photo = aura_core::PhotoId::new();
    assert_ne!(
        aura_vision::face::derive_face_id(photo, &box_norm, 100),
        aura_vision::face::derive_face_id(photo, &box_norm, 110)
    );
}

#[test]
fn the_deferred_gates_are_reported_with_the_reason_they_are_deferred() {
    // The list a reader of the CI log should see, and the same list the exit report
    // carries. Printing it is the point: a deferred gate that nobody is reminded of
    // becomes a gate nobody ever closes.
    let deferred: &[(&str, &str)] = &[
        (
            "detection recall on photographs",
            "C1: `face_detect` 1.0.0 is a placeholder and finds no faces in a photograph; \
             recall here is measured against synthetic ground truth",
        ),
        (
            "identity clustering F1 on photographs",
            "C1: `face_embed` 1.0.0 is a placeholder and its templates carry no identity \
             information; F1 here is measured against synthetic templates with known \
             separation",
        ),
        (
            "the trained quality head's contribution",
            "C2: `QUALITY_MODEL_WEIGHT` is 0.0 because the shipped head has no training \
             behind it; the gate is currently four measured factors",
        ),
        (
            "couple identification on 20 audit weddings",
            "C3: needs real weddings and a human audit; the local scorer is measured on \
             constructed evidence in crates/aura-vision/tests/face_roles.rs",
        ),
        (
            "per-group recall, vote rate and clustering F1",
            "C5: needs a balanced evaluation set with per-group consent records; the \
             synthetic fixtures use one skin tone and a number computed from them would \
             describe a renderer",
        ),
    ];

    println!("deferred gates:");
    for (gate, reason) in deferred {
        println!("  - {gate}: {reason}");
    }
    assert_eq!(
        deferred.len(),
        5,
        "the exit report lists five deferred gates"
    );
}
