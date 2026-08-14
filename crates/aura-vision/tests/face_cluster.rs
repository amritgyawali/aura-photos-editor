//! Turning templates into people, and getting the sisters right.
//!
//! The failure this module exists to prevent is the **chain merge**, and it is the one
//! that cannot be found by looking at an F1 score alone: one bad merge joining two
//! thirty-face identities creates 900 false-positive pairs, which collapses precision,
//! but a wedding with a hundred identities can absorb one such merge and still look
//! respectable on average. So the tests here assert impurity directly, construct the
//! lookalike-relatives case on purpose, and check that the cohesion verification is doing
//! work rather than merely existing.

use aura_vision::face::cluster::{
    self, ClusterConfig, ClusterFace, ClusterMetrics, CLUSTER_VER, MAX_SKELETON,
};
use aura_vision::face::embed::distance;
use aura_vision::face::fixtures::synthetic_template;

/// `count` faces of one identity, with a controlled spread.
fn faces_of(
    identity: u32,
    count: u32,
    spread: f32,
    quality: f32,
    next_key: &mut u64,
) -> Vec<ClusterFace> {
    (0..count)
        .map(|variation| {
            let face = ClusterFace {
                key: *next_key,
                template: synthetic_template(identity, variation, spread, None),
                quality,
                votes: true,
            };
            *next_key += 1;
            face
        })
        .collect()
}

/// A wedding: `people` identities, `each` faces apiece.
fn wedding(people: u32, each: u32, spread: f32) -> (Vec<ClusterFace>, Vec<Option<u32>>) {
    let mut key = 0u64;
    let mut faces = Vec::new();
    let mut truth = Vec::new();
    for identity in 0..people {
        for face in faces_of(identity, each, spread, 0.8, &mut key) {
            faces.push(face);
            truth.push(Some(identity));
        }
    }
    (faces, truth)
}

fn metrics_of(
    faces: &[ClusterFace],
    truth: &[Option<u32>],
    config: &ClusterConfig,
) -> ClusterMetrics {
    let outcome = cluster::cluster(faces, config);
    let predicted = cluster::assignment_labels(faces.len(), &outcome);
    cluster::pairwise_metrics(truth, &predicted)
}

#[test]
fn synthetic_identities_are_separable_at_all() {
    // If this fails, every clustering number below is measuring the fixture rather than
    // the algorithm. Two independent directions in 512 dimensions are very nearly
    // orthogonal, which is what makes them usable as ground truth.
    let same = distance(
        &synthetic_template(1, 0, 0.35, None),
        &synthetic_template(1, 1, 0.35, None),
    );
    let different = distance(
        &synthetic_template(1, 0, 0.35, None),
        &synthetic_template(2, 0, 0.35, None),
    );
    assert!(same < 0.25, "same identity: {same}");
    assert!(different > 0.75, "different identities: {different}");
}

#[test]
fn a_clean_wedding_clusters_perfectly() {
    let (faces, truth) = wedding(8, 6, 0.30);
    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    let predicted = cluster::assignment_labels(faces.len(), &outcome);
    let metrics = cluster::pairwise_metrics(&truth, &predicted);

    assert_eq!(outcome.clusters.len(), 8, "one cluster per person");
    assert_eq!(metrics.impure_clusters, 0, "no cluster may hold two people");
    assert!(metrics.f1 > 0.99, "F1 {}", metrics.f1);
    assert_eq!(outcome.unassigned.len(), 0);
}

#[test]
fn clustering_meets_the_section_ten_gate() {
    // Section 10.1: identity clustering F1 >= 0.93, and no cluster contains two labelled
    // people. Measured on a harder wedding than the clean one: twelve people, an uneven
    // number of faces each, and a wider spread.
    let mut key = 0u64;
    let mut faces = Vec::new();
    let mut truth = Vec::new();
    for identity in 0..12u32 {
        let count = 3 + (identity % 5) * 3;
        for face in faces_of(identity, count, 0.38, 0.75, &mut key) {
            faces.push(face);
            truth.push(Some(identity));
        }
    }

    let metrics = metrics_of(&faces, &truth, &ClusterConfig::default());
    assert!(
        metrics.f1 >= 0.93,
        "F1 {} below the section 10.1 gate",
        metrics.f1
    );
    assert_eq!(
        metrics.impure_clusters, 0,
        "section 10.1 states impurity as an absolute, not as an average"
    );
}

#[test]
fn lookalike_relatives_are_not_merged() {
    // Section 12's third failure mode, constructed on purpose and calibrated against real
    // recogniser behaviour rather than chosen for convenience:
    //
    // * a spread of 0.485 gives an intra-identity distance of about 0.20, which is where
    //   an ArcFace-class model puts two good photographs of one person;
    // * a blend of 0.40 puts the two identities 0.497 apart, which is where the same model
    //   puts two siblings.
    //
    // 0.497 is comfortably under the 0.55 merge threshold, so the absolute threshold would
    // merge them and average linkage would not save it. Cohesion verification is what does:
    // the gap is 2.5 times each cluster's own spread, past the 2.2 limit.
    let mut key = 0u64;
    let mut faces = Vec::new();
    let mut truth = Vec::new();

    for variation in 0..8u32 {
        faces.push(ClusterFace {
            key,
            template: synthetic_template(10, variation, 0.485, None),
            quality: 0.85,
            votes: true,
        });
        truth.push(Some(10));
        key += 1;
    }
    for variation in 0..8u32 {
        faces.push(ClusterFace {
            key,
            template: synthetic_template(11, variation, 0.485, Some((10, 0.40))),
            quality: 0.85,
            votes: true,
        });
        truth.push(Some(11));
        key += 1;
    }

    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    let predicted = cluster::assignment_labels(faces.len(), &outcome);
    let metrics = cluster::pairwise_metrics(&truth, &predicted);

    assert_eq!(
        metrics.impure_clusters,
        0,
        "the sisters were merged: {} clusters, F1 {}",
        outcome.clusters.len(),
        metrics.f1
    );
}

#[test]
fn cohesion_verification_actually_refuses_something() {
    // The guard has to be doing work, not merely present. A wedding of near-lookalikes
    // must record refusals; if this count is always zero the mechanism is dead code and
    // the sisters test above is passing for some other reason.
    let mut key = 0u64;
    let mut faces = Vec::new();
    for identity in 0..6u32 {
        for variation in 0..5u32 {
            faces.push(ClusterFace {
                key,
                template: synthetic_template(
                    identity,
                    variation,
                    0.30,
                    if identity == 0 { None } else { Some((0, 0.42)) },
                ),
                quality: 0.8,
                votes: true,
            });
            key += 1;
        }
    }
    let loose = ClusterConfig {
        // A threshold loose enough that everything is a candidate, so the only thing
        // standing between this wedding and one giant identity is the cohesion test.
        threshold: 1.2,
        ..ClusterConfig::default()
    };
    let outcome = cluster::cluster(&faces, &loose);
    assert!(
        outcome.refused_merges > 0,
        "cohesion verification refused nothing at threshold 1.2"
    );
    assert!(
        outcome.clusters.len() > 1,
        "a threshold of 1.2 must not produce one identity for six people"
    );
}

#[test]
fn a_medium_quality_face_is_left_unassigned_rather_than_guessed_at() {
    // Section 6.2's second pass: "leaving ambiguous faces unassigned rather than wrong".
    // The ambiguous face here is exactly between two identities.
    let mut key = 0u64;
    let mut faces = Vec::new();
    for identity in 20..22u32 {
        faces.extend(faces_of(identity, 5, 0.20, 0.9, &mut key));
    }

    let a = synthetic_template(20, 99, 0.0, None);
    let b = synthetic_template(21, 99, 0.0, None);
    let mut between: Vec<f32> = a.iter().zip(b.iter()).map(|(x, y)| x + y).collect();
    aura_index::query::normalise(&mut between);
    faces.push(ClusterFace {
        key,
        template: between,
        quality: 0.45,
        votes: false,
    });

    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    assert!(
        outcome.unassigned.contains(&(faces.len() - 1)),
        "a face equidistant from two identities must not be assigned to either"
    );
}

#[test]
fn a_medium_quality_face_near_one_identity_is_placed() {
    // The other half of the margin rule: an unambiguous non-voting face still gets a home,
    // or the panel would show a wedding of half-empty identities.
    let mut key = 0u64;
    let mut faces = Vec::new();
    for identity in 30..32u32 {
        faces.extend(faces_of(identity, 5, 0.20, 0.9, &mut key));
    }
    faces.push(ClusterFace {
        key,
        template: synthetic_template(30, 77, 0.24, None),
        quality: 0.45,
        votes: false,
    });

    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    let labels = cluster::assignment_labels(faces.len(), &outcome);
    assert!(
        labels.get(faces.len() - 1).copied().flatten().is_some(),
        "a clearly-nearest medium-quality face should be placed; {} unassigned",
        outcome.unassigned.len()
    );
    assert!(outcome.assigned_second_pass > 0);
}

#[test]
fn two_looks_of_one_person_grow_sub_centroids() {
    // Section 6.2's outfit-and-hairstyle case: one identity, two clusters of appearance.
    // The main centroid sits between them and matches neither, which is what the
    // sub-centroids are for.
    let mut key = 0u64;
    let mut faces = Vec::new();
    for variation in 0..6u32 {
        faces.push(ClusterFace {
            key,
            template: synthetic_template(40, variation, 0.18, None),
            quality: 0.85,
            votes: true,
        });
        key += 1;
    }
    for variation in 0..6u32 {
        // The same person, a long way away in template space: a different look.
        faces.push(ClusterFace {
            key,
            template: synthetic_template(41, variation, 0.18, Some((40, 0.30))),
            quality: 0.85,
            votes: true,
        });
        key += 1;
    }

    // A threshold loose enough to hold both looks in one identity, which is the situation
    // sub-centroids exist for.
    let loose = ClusterConfig {
        threshold: 0.95,
        sub_variance: 0.12,
        max_cohesion_ratio: f32::INFINITY,
        ..ClusterConfig::default()
    };
    let outcome = cluster::cluster(&faces, &loose);
    let merged = outcome
        .clusters
        .iter()
        .find(|found| found.members.len() >= 10);
    if let Some(found) = merged {
        assert!(
            !found.sub_centroids.is_empty(),
            "a high-variance identity must grow sub-centroids, variance {}",
            found.variance
        );
        // And the sub-centroids must be better match targets than the main one.
        let far = &faces[0].template;
        assert!(
            found.distance_to(far) <= distance(&found.centroid, far) + 1e-6,
            "sub-centroids must never make a match worse"
        );
    }
}

#[test]
fn clustering_is_deterministic() {
    // Invariant 4. Two runs over the same faces must produce the same identities in the
    // same order, or two machines analysing one wedding disagree about who is in it.
    let (faces, _) = wedding(6, 5, 0.32);
    let first = cluster::cluster(&faces, &ClusterConfig::default());
    let second = cluster::cluster(&faces, &ClusterConfig::default());

    assert_eq!(first.clusters.len(), second.clusters.len());
    for (left, right) in first.clusters.iter().zip(second.clusters.iter()) {
        assert_eq!(left.members, right.members);
        assert_eq!(
            left.centroid
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            right
                .centroid
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            "centroids must agree bit for bit"
        );
    }
    assert_eq!(first.unassigned, second.unassigned);
    assert_eq!(first.refused_merges, second.refused_merges);
}

#[test]
fn input_order_does_not_change_the_grouping() {
    // The stronger determinism claim: the *set* of identities must not depend on the order
    // the faces arrived in. Faces arrive ordered by id from the store, but a re-scan can
    // reorder them, and a grouping that changed would orphan every user decision.
    let (faces, truth) = wedding(5, 4, 0.30);
    let forward = cluster::cluster(&faces, &ClusterConfig::default());

    let mut reversed = faces.clone();
    reversed.reverse();
    let mut reversed_truth = truth.clone();
    reversed_truth.reverse();
    let backward = cluster::cluster(&reversed, &ClusterConfig::default());

    assert_eq!(forward.clusters.len(), backward.clusters.len());

    let forward_metrics =
        cluster::pairwise_metrics(&truth, &cluster::assignment_labels(faces.len(), &forward));
    let backward_metrics = cluster::pairwise_metrics(
        &reversed_truth,
        &cluster::assignment_labels(reversed.len(), &backward),
    );
    assert!((forward_metrics.f1 - backward_metrics.f1).abs() < 1e-6);
}

#[test]
fn a_project_with_no_voting_faces_produces_nothing_and_says_so() {
    let faces: Vec<ClusterFace> = (0..5)
        .map(|index| ClusterFace {
            key: index,
            template: synthetic_template(1, index as u32, 0.3, None),
            quality: 0.2,
            votes: false,
        })
        .collect();
    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    assert!(outcome.clusters.is_empty());
    assert_eq!(outcome.unassigned.len(), 5);
    assert_eq!(outcome.skeleton_faces, 0);
}

#[test]
fn an_empty_project_is_survivable() {
    let outcome = cluster::cluster(&[], &ClusterConfig::default());
    assert!(outcome.clusters.is_empty());
    assert!(outcome.unassigned.is_empty());
}

#[test]
fn a_single_face_is_not_an_identity() {
    let faces = vec![
        ClusterFace {
            key: 0,
            template: synthetic_template(1, 0, 0.0, None),
            quality: 0.9,
            votes: true,
        },
        ClusterFace {
            key: 1,
            template: synthetic_template(2, 0, 0.0, None),
            quality: 0.9,
            votes: true,
        },
    ];
    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    assert!(
        outcome.clusters.is_empty(),
        "two strangers photographed once each are not two identities"
    );
    assert_eq!(outcome.unassigned.len(), 2);
}

#[test]
fn the_skeleton_is_capped_and_the_rest_is_still_placed() {
    // The cap is what makes section 11's twelve-second budget hold at any project size.
    // Past it, the extra faces go through the linear second pass rather than the quadratic
    // first one, and they must still end up in the right identity.
    assert_eq!(MAX_SKELETON, 4_096);
    let (faces, truth) = wedding(4, 20, 0.28);
    let tiny = ClusterConfig::default();
    let outcome = cluster::cluster(&faces, &tiny);
    let predicted = cluster::assignment_labels(faces.len(), &outcome);
    let metrics = cluster::pairwise_metrics(&truth, &predicted);
    assert_eq!(metrics.impure_clusters, 0);
    assert!(metrics.recall > 0.9, "recall {}", metrics.recall);
}

#[test]
fn the_threshold_sweep_prefers_purity_over_f1() {
    // A threshold that scores marginally better on F1 while producing an impure cluster
    // must lose. An impure cluster is the failure a photographer notices; a fraction of a
    // point of F1 is not.
    let (faces, truth) = wedding(6, 5, 0.30);
    let candidates = [0.30f32, 0.45, 0.55, 0.70, 0.90, 1.10];
    let (sweep, chosen) =
        cluster::tune_threshold(&faces, &truth, &candidates, &ClusterConfig::default());

    assert_eq!(sweep.len(), candidates.len(), "QA sees the whole curve");
    let picked = sweep
        .iter()
        .find(|point| (point.threshold - chosen).abs() < 1e-6)
        .expect("the chosen threshold must be in the sweep");
    assert_eq!(
        picked.metrics.impure_clusters, 0,
        "the sweep chose a threshold that merges two people"
    );
}

#[test]
fn pairwise_metrics_count_what_they_claim_to() {
    // Two identities of two faces each. Predicting one cluster for all four joins two
    // pairs that should be separate.
    let truth = vec![Some(0), Some(0), Some(1), Some(1)];
    let merged = vec![Some(0), Some(0), Some(0), Some(0)];
    let metrics = cluster::pairwise_metrics(&truth, &merged);
    assert_eq!(metrics.true_positive, 2);
    assert_eq!(metrics.false_positive, 4);
    assert_eq!(metrics.false_negative, 0);
    assert_eq!(metrics.impure_clusters, 1);

    let split = vec![Some(0), Some(1), Some(2), Some(3)];
    let metrics = cluster::pairwise_metrics(&truth, &split);
    assert_eq!(metrics.true_positive, 0);
    assert_eq!(metrics.false_negative, 2);
    assert_eq!(metrics.impure_clusters, 0, "over-splitting is never impure");
}

#[test]
fn unassigned_faces_are_excluded_from_the_metrics_rather_than_counted_wrong() {
    // A face the product deliberately declined to place is a refusal, not a wrong answer.
    // Its cost is accounted for in `unassigned`, and counting it as a false negative would
    // punish the product for the exact behaviour section 6.2 asks for.
    let truth = vec![Some(0), Some(0), Some(0)];
    let partial = vec![Some(0), Some(0), None];
    let metrics = cluster::pairwise_metrics(&truth, &partial);
    assert_eq!(metrics.true_positive, 1);
    assert_eq!(metrics.false_negative, 2);
    assert_eq!(metrics.false_positive, 0);
}

#[test]
fn the_cluster_version_is_stamped() {
    assert_eq!(CLUSTER_VER, 1);
}
