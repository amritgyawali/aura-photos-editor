//! The truth the graph is measured against, and the four numbers section 6.4
//! turns into gates.
//!
//! An approximate index is only worth having if somebody has measured how
//! approximate it is. [`exact_knn`] is the brute-force answer - 4,000 dot
//! products, a millisecond, and no graph - and [`recall_at_k`] is the fraction of
//! it the graph found. Everything else here is retrieval and clustering
//! arithmetic that the eval harness and `ml/models/embed/eval_retrieval.py`
//! compute identically, so the day a real head is trained the two numbers can be
//! compared rather than argued about.
//!
//! Nothing in this module is used by the product at runtime. It is the
//! measurement apparatus, and it lives in the crate rather than in a test file
//! because the phase gate, the perf suite and the eval harness all need it.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::index::{cosine_distance, ImageEmbedding, ImageId};

/// Exhaustive nearest neighbours: the answer the graph is approximating.
///
/// Ties are broken by id so two runs agree, and the query frame is never in its
/// own result, exactly as [`crate::contract::index::SimilarityIndex::knn`]
/// promises.
#[must_use]
pub fn exact_knn(
    query: &ImageEmbedding,
    corpus: &[ImageEmbedding],
    k: usize,
) -> Vec<(ImageId, f32)> {
    let vector = query.to_f32();
    let mut scored: Vec<(ImageId, f32)> = corpus
        .iter()
        .filter(|candidate| candidate.image_id != query.image_id)
        .map(|candidate| {
            (
                candidate.image_id,
                cosine_distance(&vector, &candidate.to_f32()),
            )
        })
        .collect();
    scored.sort_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(&right.0)));
    scored.truncate(k);
    scored
}

/// Fraction of the exact neighbours the approximate search also found.
///
/// The one number that says whether the graph is worth its memory. Recall of 1.0
/// means the graph and the flat scan agree completely.
#[must_use]
pub fn recall_at_k(approximate: &[(ImageId, f32)], exact: &[(ImageId, f32)]) -> f64 {
    if exact.is_empty() {
        return 1.0;
    }
    let found: BTreeSet<ImageId> = approximate.iter().map(|(id, _)| *id).collect();
    let hits = exact.iter().filter(|(id, _)| found.contains(id)).count();
    hits as f64 / exact.len() as f64
}

/// Mean average precision at `k` over a set of queries.
///
/// `relevant` answers "is this neighbour a correct answer for this query" -
/// in the eval harness, "are these two frames from the same labelled scene".
/// Section 6.4 gates the head on beating the raw backbone by eight points of
/// this.
#[must_use]
pub fn mean_average_precision(
    results: &[(ImageId, Vec<ImageId>)],
    relevant: &dyn Fn(ImageId, ImageId) -> bool,
) -> f64 {
    if results.is_empty() {
        return 0.0;
    }
    let mut total = 0.0f64;
    for (query, neighbours) in results {
        let mut hits = 0usize;
        let mut precision_sum = 0.0f64;
        for (rank, neighbour) in neighbours.iter().enumerate() {
            if relevant(*query, *neighbour) {
                hits += 1;
                precision_sum += hits as f64 / (rank + 1) as f64;
            }
        }
        if hits > 0 {
            total += precision_sum / hits as f64;
        }
    }
    total / results.len() as f64
}

/// One point on a precision/recall curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrPoint {
    /// The threshold this point was measured at.
    pub threshold: f64,
    /// True positives over predicted positives.
    pub precision: f64,
    /// True positives over actual positives.
    pub recall: f64,
}

/// Precision and recall of a duplicate detector, swept over its threshold.
///
/// `scored` is `(is_a_true_duplicate, score)` where a *lower* score means more
/// alike - a cosine distance or a Hamming distance. The sweep is over the scores
/// that actually occur, so the curve has a point wherever the answer changes and
/// nowhere else.
#[must_use]
pub fn duplicate_pr_curve(scored: &[(bool, f64)]) -> Vec<PrPoint> {
    let positives = scored.iter().filter(|(truth, _)| *truth).count();
    if positives == 0 {
        return Vec::new();
    }
    let mut thresholds: Vec<f64> = scored.iter().map(|(_, score)| *score).collect();
    thresholds.sort_by(f64::total_cmp);
    thresholds.dedup_by(|left, right| (*left - *right).abs() < f64::EPSILON);

    thresholds
        .into_iter()
        .map(|threshold| {
            let predicted = scored.iter().filter(|(_, score)| *score <= threshold);
            let mut true_positives = 0usize;
            let mut predicted_count = 0usize;
            for (truth, _) in predicted {
                predicted_count += 1;
                if *truth {
                    true_positives += 1;
                }
            }
            PrPoint {
                threshold,
                precision: if predicted_count == 0 {
                    1.0
                } else {
                    true_positives as f64 / predicted_count as f64
                },
                recall: true_positives as f64 / positives as f64,
            }
        })
        .collect()
}

/// The best recall available at or above a precision floor.
///
/// Section 6.4's duplicate gate reads "recall >= 0.98 at precision >= 0.95",
/// which is this function with `floor = 0.95`.
#[must_use]
pub fn recall_at_precision(curve: &[PrPoint], floor: f64) -> f64 {
    curve
        .iter()
        .filter(|point| point.precision >= floor)
        .map(|point| point.recall)
        .fold(0.0, f64::max)
}

/// Cluster purity against human labels.
///
/// Each cluster is credited with the size of its most common true label; the
/// total over all clusters, divided by the number of items. One cluster per item
/// scores 1.0, which is why purity is never quoted without NMI beside it.
#[must_use]
pub fn cluster_purity(assignments: &[(ImageId, u32)], truth: &BTreeMap<ImageId, u32>) -> f64 {
    if assignments.is_empty() {
        return 0.0;
    }
    let mut per_cluster: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut counted = 0usize;
    for (id, cluster) in assignments {
        let Some(label) = truth.get(id) else {
            continue;
        };
        *per_cluster
            .entry(*cluster)
            .or_default()
            .entry(*label)
            .or_default() += 1;
        counted += 1;
    }
    if counted == 0 {
        return 0.0;
    }
    let dominant: usize = per_cluster
        .values()
        .map(|labels| labels.values().copied().max().unwrap_or(0))
        .sum();
    dominant as f64 / counted as f64
}

/// Normalised mutual information between an assignment and the truth.
///
/// The companion to purity: it punishes both splitting one scene across many
/// clusters and merging many scenes into one, which purity alone does not.
/// Normalised by the arithmetic mean of the two entropies, which is the
/// convention `sklearn.metrics.normalized_mutual_info_score` uses by default and
/// therefore the one `eval_retrieval.py` produces.
#[must_use]
pub fn normalised_mutual_information(
    assignments: &[(ImageId, u32)],
    truth: &BTreeMap<ImageId, u32>,
) -> f64 {
    let pairs: Vec<(u32, u32)> = assignments
        .iter()
        .filter_map(|(id, cluster)| truth.get(id).map(|label| (*cluster, *label)))
        .collect();
    let total = pairs.len();
    if total == 0 {
        return 0.0;
    }
    let n = total as f64;

    let mut joint: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut left_counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut right_counts: BTreeMap<u32, usize> = BTreeMap::new();
    for (cluster, label) in pairs {
        *joint.entry((cluster, label)).or_default() += 1;
        *left_counts.entry(cluster).or_default() += 1;
        *right_counts.entry(label).or_default() += 1;
    }

    let entropy = |counts: &BTreeMap<u32, usize>| -> f64 {
        -counts
            .values()
            .map(|count| {
                let p = *count as f64 / n;
                p * p.ln()
            })
            .sum::<f64>()
    };
    let h_left = entropy(&left_counts);
    let h_right = entropy(&right_counts);
    if h_left <= f64::EPSILON && h_right <= f64::EPSILON {
        return 1.0;
    }

    let mut information = 0.0f64;
    for ((cluster, label), count) in joint {
        let p_joint = count as f64 / n;
        let p_left = left_counts.get(&cluster).copied().unwrap_or(0) as f64 / n;
        let p_right = right_counts.get(&label).copied().unwrap_or(0) as f64 / n;
        if p_joint > 0.0 && p_left > 0.0 && p_right > 0.0 {
            information += p_joint * (p_joint / (p_left * p_right)).ln();
        }
    }
    let denominator = f64::midpoint(h_left, h_right);
    if denominator <= f64::EPSILON {
        return 0.0;
    }
    (information / denominator).clamp(0.0, 1.0)
}
