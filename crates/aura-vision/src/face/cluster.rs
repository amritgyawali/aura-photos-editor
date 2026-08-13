//! Turning ten thousand faces into sixty people, and getting the sisters right.
//!
//! Clustering is where a people feature is won or lost, and the failure that
//! matters is not "slightly wrong groups". It is the **chain merge**: guest A
//! resembles guest B, B resembles the bride's sister C, C resembles the bride, and
//! a naive algorithm walks that chain and produces one identity containing four
//! people. A photographer who opens the People panel and sees one card labelled
//! "68 photos" where there should be four cards will not use the feature again, and
//! every coverage guarantee built on top of it is nonsense.
//!
//! Four mechanisms stand between this product and that outcome, and each one is a
//! direct answer to a line in section 6.2.
//!
//! ## 1. Average linkage, computed exactly and cheaply
//!
//! Single linkage is what chains: it merges two clusters if *any* pair of members
//! is close, so one bad face bridges two people permanently. Average linkage asks
//! for the *mean* distance across every cross pair, and one bad face cannot carry
//! a merge on its own.
//!
//! Average linkage is normally expensive, because the mean is over `|A| x |B|`
//! pairs. It is not expensive here, because of an identity that holds for cosine
//! distance on unit vectors and is worth writing out:
//!
//! ```text
//! mean over i in A, j in B of (1 - a_i . b_j)
//!   = 1 - (1/(|A||B|)) * sum_i sum_j a_i . b_j
//!   = 1 - ( (sum_i a_i) . (sum_j b_j) ) / (|A||B|)
//! ```
//!
//! Cosine distance is affine in the dot product, so the average over all cross
//! pairs is **exactly** one minus the dot product of the two clusters'
//! *unnormalised* means. Keeping a running sum per cluster therefore makes exact
//! average linkage cost one 512-wide dot product per cluster pair, the same as
//! single linkage. No approximation, no sampling, no centroid-linkage substitute.
//!
//! ## 2. A skeleton of high-quality faces, capped
//!
//! Section 6.2 asks for a skeleton built from high-quality faces and a second pass
//! that assigns the rest. That is a quality argument, and it is also what makes
//! section 11's budget - 25,000 faces in 12 seconds - reachable: an exact
//! agglomerative pass is quadratic, and [`MAX_SKELETON`] caps the quadratic part at
//! a fixed size while the linear second pass absorbs everything else. A wedding has
//! tens of identities; four thousand of its best faces is an enormous sample of
//! them.
//!
//! ## 3. Mutual nearest neighbours, and the test that actually does the work
//!
//! Section 6.2 asks for "rank-order verification (mutual nearest neighbour)". The
//! mutual-nearest-neighbour half is satisfied **by construction** and is worth being
//! explicit about rather than implementing twice: this loop always merges the pair
//! with the globally smallest average-linkage distance, and if `d(A,B)` is the global
//! minimum then `B` is `A`'s nearest and `A` is `B`'s. There is no configuration in
//! which a non-mutual pair is merged.
//!
//! Which means mutual-NN, on its own, refuses nothing - and the chain merge it is
//! supposed to prevent still happens. Two sisters' clusters are mutually nearest and
//! sit below any threshold loose enough to also group one person across twelve hours
//! of changing light.
//!
//! So the guard that does the work is **relative cohesion**, and it asks a different
//! question: is the gap between these two clusters small *compared with how spread out
//! each of them already is*? Two looks of one person are separated by about as much as
//! either look is internally spread. Two similar-looking relatives are separated by
//! several times that, even when the absolute distance is small. That ratio is
//! [`ClusterConfig::max_cohesion_ratio`], and it is the single most important line in
//! the module.
//!
//! The within-cluster spread it compares against is computed with the same identity as
//! the linkage - see [`Working::spread`] - so it costs one dot product rather than a
//! quadratic pass. The deviation from the phase document's wording is recorded in
//! `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` section 5.
//!
//! ## 4. A margin on the second pass, and the willingness to say nothing
//!
//! A medium-quality face is assigned only when its best cluster is clearly better
//! than its second best. Ambiguous faces are left unassigned, they show up in the
//! panel as unassigned, and they do not vote. An identity graph that is silent
//! about 4 % of faces is far more useful than one that is confident about all of
//! them and wrong about 4 %.
//!
//! ## No clothing, and no second vector store
//!
//! Nothing here reads a colour, a histogram or a frame - only templates from
//! [`crate::face::embed`], which only ever saw a 112 px aligned crop. And nothing
//! here is persisted: the working structures live for the duration of one call.
//! The phase 05 rule that `SimilarityIndex` is the only vector store still holds,
//! and it is not violated by transient arithmetic. See ADR-0013 section 5.

use rayon::prelude::*;

use crate::face::embed::distance;

/// The clustering generation, stamped on `identities.cluster_ver`.
///
/// Bumped by any change to the linkage, the verification, the second-pass margin or
/// the sub-centroid rule. A bump does not invalidate templates - it invalidates
/// *groupings*, so the identities are rebuilt and the user's decisions are replayed
/// over the result from `identity_links`.
pub const CLUSTER_VER: u16 = 1;

/// The within-cluster spread a pair of singletons is credited with.
///
/// A cluster of one face has no internal spread to compare a gap against, so the cohesion
/// test needs a stand-in for the first merge of every identity. Without one, the first
/// merge would always be refused and the algorithm would never start.
///
/// 0.12 is what one person's *good* faces are typically spread by in cosine distance -
/// tight, because the faces that form a skeleton have passed the quality gate. It is an
/// absolute figure rather than a fraction of the merge threshold on purpose: a caller who
/// loosens the threshold to see what a wedding looks like ungrouped must not thereby
/// loosen the guard that stops two people becoming one, which is exactly what a
/// threshold-scaled floor would do.
pub const COHESION_FLOOR: f32 = 0.12;

/// The largest skeleton an exact agglomerative pass will run on.
///
/// Four thousand and ninety-six. The arithmetic behind the number: the initial
/// nearest-neighbour pass is `k^2 / 2` dot products of width 512, so 4,096 costs
/// about 4.3 GFLOP, which the eight-lane dot product from phase 05 does in well
/// under a second on four cores. Doubling it would quadruple that. See section 11's
/// twelve-second budget for 25,000 faces, of which this is the quadratic part.
pub const MAX_SKELETON: usize = 4_096;

/// One face, as the clustering sees it.
///
/// `key` exists because determinism is not optional here: two machines must produce
/// the same identities, and the tie-breaking that guarantees it has to be over
/// something stable. The caller supplies the face id's ordering as a dense index.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterFace {
    /// Stable ordering key. Usually the face id's position in a sorted list.
    pub key: u64,
    /// The L2-normalised recognition template.
    pub template: Vec<f32>,
    /// Fused usability, `0..1`. Decides skeleton membership.
    pub quality: f32,
    /// Whether the quality gate lets this face vote on identity.
    pub votes: bool,
}

/// What governs one clustering pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterConfig {
    /// Average-linkage cosine distance at or below which two clusters may merge.
    pub threshold: f32,
    /// How much better the best cluster must be than the second best before a
    /// medium-quality face is assigned to it.
    pub assign_margin: f32,
    /// Distance ceiling for assigning a medium-quality face at all.
    pub assign_ceiling: f32,
    /// The most the gap between two clusters may be, as a multiple of how spread out
    /// the more spread of the two already is. See this module's header, section 3.
    pub max_cohesion_ratio: f32,
    /// Internal variance above which an identity grows sub-centroids.
    pub sub_variance: f32,
    /// Sub-centroids per identity, at most.
    pub max_sub: usize,
    /// Clusters smaller than this are not identities; their faces are returned
    /// unassigned.
    pub min_cluster: usize,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            // The ArcFace-family verification operating point, in cosine distance:
            // published thresholds for this class of model sit at a cosine
            // similarity near 0.45 for a low false-accept rate, which is a distance
            // of 0.55. It is a starting point and not a constant of nature -
            // `tune_threshold` and `ml/models/face/tune_cluster.py` are how it is
            // set per project, exactly as section 6.2 asks.
            threshold: 0.55,
            // A tenth of the distance range. Small enough that most faces are
            // placed, large enough that the two sisters' faces are not.
            assign_margin: 0.06,
            // Stricter than the merge threshold, because a single face has no
            // averaging to protect it: section 6.2's "stricter margin".
            assign_ceiling: 0.48,
            // 2.2, and the number comes from the two cases it has to separate.
            //
            // *Two looks of one person* - a hairstyle change, a jacket off - are
            // typically separated by about 1.5 to 1.8 times the internal spread of
            // either look, because the same face under different framing moves less
            // than the face moves within one look.
            //
            // *Two similar-looking relatives* are separated by three times it or more,
            // even when the absolute distance is comfortably under the threshold: each
            // sibling's own faces cluster tightly, and the gap between the two is a
            // property of two different people rather than of one person's variation.
            //
            // 2.2 sits between them. Raising it merges relatives; lowering it shatters
            // one person into their looks, which the sub-centroids exist to hold
            // together rather than to create.
            max_cohesion_ratio: 2.2,
            // Mean member-to-centroid distance above which one identity is really
            // two looks of one person - the outfit and hairstyle change.
            sub_variance: 0.34,
            max_sub: 4,
            // A single face is not an identity. Two is: a guest photographed twice
            // is a guest.
            min_cluster: 2,
        }
    }
}

/// One identity, as the clustering produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    /// Indices into the input slice, ascending.
    pub members: Vec<usize>,
    /// The L2-normalised mean template.
    pub centroid: Vec<f32>,
    /// Sub-cluster means, when the internal variance earned them. Empty otherwise.
    pub sub_centroids: Vec<Vec<f32>>,
    /// Mean cosine distance from a member to [`Cluster::centroid`].
    pub variance: f32,
}

impl Cluster {
    /// The smallest distance from `template` to this identity, counting
    /// sub-centroids.
    ///
    /// The minimum rather than the distance to the main centroid, and that is the
    /// point of sub-centroids: an identity whose members span two very different
    /// looks has a main centroid halfway between them that matches neither, and a
    /// face from either look would be refused by [`ClusterConfig::assign_ceiling`]
    /// if the main centroid were the only thing it were compared against.
    #[must_use]
    pub fn distance_to(&self, template: &[f32]) -> f32 {
        let mut best = distance(&self.centroid, template);
        for sub in &self.sub_centroids {
            best = best.min(distance(sub, template));
        }
        best
    }
}

/// What one clustering pass did, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterOutcome {
    /// The identities, largest first.
    pub clusters: Vec<Cluster>,
    /// Faces that were deliberately not placed, as indices into the input.
    pub unassigned: Vec<usize>,
    /// The threshold that was actually used.
    pub threshold: f32,
    /// Voting faces that went into the skeleton.
    pub skeleton_faces: usize,
    /// Faces placed by the second pass.
    pub assigned_second_pass: usize,
    /// Merges the threshold allowed and cohesion verification refused.
    ///
    /// The number to watch. A wedding with a large count here is a wedding with
    /// several similar-looking relatives, and it is the count that goes up when a
    /// chain merge has just been prevented.
    pub refused_merges: usize,
    /// Identities that grew sub-centroids.
    pub sub_clustered: usize,
    /// Wall-clock milliseconds, filled in by the caller that has a clock.
    pub elapsed_ms: u64,
}

/// A working cluster during the agglomerative pass.
#[derive(Debug, Clone)]
struct Working {
    members: Vec<usize>,
    /// Unnormalised sum of member templates. The whole trick: average linkage is
    /// a dot product of these, divided by the two sizes.
    sum: Vec<f32>,
    active: bool,
    /// True when [`Working::nearest`] has to be recomputed before it is trusted.
    ///
    /// A separate flag rather than `nearest: None`, and the distinction is load-bearing:
    /// "not computed yet" and "computed, and there is nobody left to merge with" are
    /// different states, and conflating them makes the main loop recompute a cluster with
    /// no candidates for ever. That is not a hypothetical - it is what happens to the last
    /// cluster standing, and to any cluster whose every neighbour has been refused.
    dirty: bool,
    /// Nearest active cluster and its average-linkage distance, or `None` when there is
    /// no eligible neighbour at all.
    nearest: Option<(usize, f32)>,
}

impl Working {
    fn singleton(index: usize, template: &[f32]) -> Self {
        Self {
            members: vec![index],
            sum: template.to_vec(),
            active: true,
            dirty: true,
            nearest: None,
        }
    }

    fn size(&self) -> f32 {
        self.members.len() as f32
    }

    /// Mean pairwise cosine distance *within* this cluster.
    ///
    /// The same identity that makes average linkage cheap, turned inwards. For unit
    /// vectors, the sum of all ordered within-cluster dot products is `|S|^2 - n`, so
    /// the mean pairwise distance is `1 - (|S|^2 - n) / (n(n-1))` - one dot product
    /// rather than a quadratic pass over the members.
    ///
    /// A singleton has no pairs, so it has no spread; the caller substitutes a floor.
    fn spread(&self) -> Option<f32> {
        let n = self.members.len();
        if n < 2 {
            return None;
        }
        let mut energy = 0.0f32;
        for value in &self.sum {
            energy += value * value;
        }
        let count = n as f32;
        let pairs = count * (count - 1.0);
        Some((1.0 - (energy - count) / pairs).clamp(0.0, 2.0))
    }
}

/// Exact average-linkage cosine distance between two working clusters.
///
/// See this module's header for why this is exact rather than an approximation.
fn linkage(a: &Working, b: &Working) -> f32 {
    let scale = 1.0 / (a.size() * b.size());
    let mut dot = 0.0f32;
    for (x, y) in a.sum.iter().zip(b.sum.iter()) {
        dot += x * y;
    }
    (1.0 - dot * scale).clamp(0.0, 2.0)
}

/// Cluster a project's faces.
///
/// The two passes of section 6.2, in order. Never fails: a project with no voting
/// faces produces no identities and says so in the outcome, which is the honest
/// answer for a wedding of landscapes.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn cluster(faces: &[ClusterFace], config: &ClusterConfig) -> ClusterOutcome {
    let mut outcome = ClusterOutcome {
        clusters: Vec::new(),
        unassigned: Vec::new(),
        threshold: config.threshold,
        skeleton_faces: 0,
        assigned_second_pass: 0,
        refused_merges: 0,
        sub_clustered: 0,
        elapsed_ms: 0,
    };
    if faces.is_empty() {
        return outcome;
    }

    // --- Choose the skeleton -------------------------------------------------
    //
    // Voting faces only, best quality first, capped. The tie-break on `key` is
    // what makes the choice reproducible when a hundred faces share a quality of
    // 0.71, which quantised scores guarantee will happen.
    let mut candidates: Vec<usize> = (0..faces.len())
        .filter(|index| faces.get(*index).is_some_and(|face| face.votes))
        .collect();
    candidates.sort_by(|left, right| {
        let quality = |index: &usize| faces.get(*index).map_or(0.0, |face| face.quality);
        let key = |index: &usize| faces.get(*index).map_or(0, |face| face.key);
        quality(right)
            .partial_cmp(&quality(left))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| key(left).cmp(&key(right)))
    });
    let skeleton: Vec<usize> = candidates.iter().copied().take(MAX_SKELETON).collect();
    let overflow: Vec<usize> = candidates.iter().copied().skip(MAX_SKELETON).collect();
    outcome.skeleton_faces = skeleton.len();

    if skeleton.is_empty() {
        outcome.unassigned = (0..faces.len()).collect();
        return outcome;
    }

    // --- Pass one: exact agglomerative over the skeleton ---------------------
    let mut working: Vec<Working> = skeleton
        .iter()
        .filter_map(|index| {
            faces
                .get(*index)
                .map(|face| Working::singleton(*index, &face.template))
        })
        .collect();

    // Pairs that rank-order verification has refused, keyed by both cluster
    // indices *and* both cluster sizes.
    //
    // The sizes are what makes this terminate and stay correct. A refusal is a
    // statement about two specific clusters as they are now; when one of them grows,
    // the statement no longer applies and the pair is reconsidered. Sizes only ever
    // increase and their sum is bounded by the skeleton, so the set of possible keys
    // is finite - which is the termination argument. A refusal set keyed on indices
    // alone would either loop for ever (if refusals expired) or permanently block a
    // merge that later became correct (if they did not).
    let mut refused: std::collections::BTreeSet<(usize, usize, usize, usize)> =
        std::collections::BTreeSet::new();

    // The initial nearest-neighbour pass, in parallel over rows. This is the
    // quadratic part and the reason MAX_SKELETON exists.
    let initial: Vec<Option<(usize, f32)>> = (0..working.len())
        .into_par_iter()
        .map(|index| nearest_of(&working, index, &refused))
        .collect();
    for (slot, nearest) in working.iter_mut().zip(initial) {
        slot.nearest = nearest;
        slot.dirty = false;
    }

    loop {
        // The globally closest pair. Recomputed lazily: a cluster whose recorded
        // nearest has since been merged away is refreshed here rather than eagerly
        // on every merge, which is what keeps the loop near-quadratic instead of
        // cubic.
        let mut best: Option<(usize, usize, f32)> = None;
        let mut stale: Option<usize> = None;
        for index in 0..working.len() {
            let Some(entry) = working.get(index) else {
                continue;
            };
            if !entry.active {
                continue;
            }
            if entry.dirty {
                stale = Some(index);
                break;
            }
            match entry.nearest {
                // Computed, and there is nobody left to merge with. Not stale.
                None => {}
                Some((other, dist)) => {
                    let other_active = working.get(other).is_some_and(|slot| slot.active);
                    if !other_active {
                        stale = Some(index);
                        break;
                    }
                    if dist > config.threshold {
                        continue;
                    }
                    // Ties broken by index, so the merge order is fixed.
                    let better = match best {
                        None => true,
                        Some((best_a, best_b, best_dist)) => {
                            dist < best_dist
                                || (dist.to_bits() == best_dist.to_bits()
                                    && (index, other) < (best_a, best_b))
                        }
                    };
                    if better {
                        best = Some((index, other, dist));
                    }
                }
            }
        }

        if let Some(index) = stale {
            let nearest = nearest_of(&working, index, &refused);
            if let Some(entry) = working.get_mut(index) {
                entry.nearest = nearest;
                entry.dirty = false;
            }
            continue;
        }

        let Some((left, right, _)) = best else {
            break;
        };

        // Cohesion verification. The pair is mutually nearest by construction - it is
        // the global minimum - so the question that is left is whether the gap between
        // them is small relative to how spread out they already are. See this module's
        // header, section 3.
        if !cohesion_ok(&working, left, right, config) {
            outcome.refused_merges += 1;
            refused.insert(refusal_key(&working, left, right));
            // The refusal has to be made to stick, or the same pair is chosen again
            // on the next round for ever. Both sides are recomputed, because either
            // one may have been pointing at the other.
            for index in [left, right] {
                let nearest = nearest_of(&working, index, &refused);
                if let Some(entry) = working.get_mut(index) {
                    entry.nearest = nearest;
                    entry.dirty = false;
                }
            }
            continue;
        }

        // Merge right into left.
        let (mut members, sum) = {
            let Some(source) = working.get(right) else {
                break;
            };
            (source.members.clone(), source.sum.clone())
        };
        if let Some(target) = working.get_mut(left) {
            target.members.append(&mut members);
            target.members.sort_unstable();
            for (slot, value) in target.sum.iter_mut().zip(sum.iter()) {
                *slot += value;
            }
            target.nearest = None;
            target.dirty = true;
        }
        if let Some(source) = working.get_mut(right) {
            source.active = false;
            source.dirty = false;
            source.nearest = None;
        }
        // Anything that pointed at either side is stale now.
        for entry in &mut working {
            if !entry.active {
                continue;
            }
            if entry
                .nearest
                .is_some_and(|(other, _)| other == left || other == right)
            {
                entry.nearest = None;
                entry.dirty = true;
            }
        }
    }

    // --- Turn the survivors into identities ----------------------------------
    let mut clusters: Vec<Cluster> = Vec::new();
    let mut orphans: Vec<usize> = Vec::new();
    for entry in working.iter().filter(|entry| entry.active) {
        if entry.members.len() < config.min_cluster {
            orphans.extend(entry.members.iter().copied());
            continue;
        }
        clusters.push(finish_cluster(faces, &entry.members, config));
    }

    // --- Pass two: place the rest by nearest centroid, with a margin ---------
    let mut second_pass: Vec<usize> = Vec::new();
    second_pass.extend(overflow);
    second_pass
        .extend((0..faces.len()).filter(|index| faces.get(*index).is_some_and(|face| !face.votes)));
    second_pass.extend(orphans.iter().copied());
    second_pass.sort_unstable();
    second_pass.dedup();

    let mut additions: Vec<Vec<usize>> = vec![Vec::new(); clusters.len()];
    for index in second_pass {
        let Some(face) = faces.get(index) else {
            continue;
        };
        match best_two(&clusters, &face.template) {
            Some((best_index, best_distance, runner_up)) => {
                let clear =
                    runner_up.is_none_or(|second| second - best_distance >= config.assign_margin);
                if best_distance <= config.assign_ceiling && clear {
                    if let Some(slot) = additions.get_mut(best_index) {
                        slot.push(index);
                    }
                    outcome.assigned_second_pass += 1;
                } else {
                    outcome.unassigned.push(index);
                }
            }
            None => outcome.unassigned.push(index),
        }
    }

    // Recompute each identity from its full membership. The centroid a photographer
    // sees, and the one a later phase compares against, is the one that includes
    // every face the identity ended up with - not the skeleton's.
    for (cluster, extra) in clusters.iter_mut().zip(additions) {
        if extra.is_empty() {
            continue;
        }
        let mut members = cluster.members.clone();
        members.extend(extra);
        members.sort_unstable();
        members.dedup();
        *cluster = finish_cluster(faces, &members, config);
    }

    outcome.sub_clustered = clusters
        .iter()
        .filter(|cluster| !cluster.sub_centroids.is_empty())
        .count();

    // Largest first, ties by first member, so the People panel's order is stable.
    clusters.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then_with(|| a.members.first().cmp(&b.members.first()))
    });
    outcome.clusters = clusters;
    outcome.unassigned.sort_unstable();
    outcome.unassigned.dedup();
    outcome
}

/// The canonical key for a refused pair: both indices and both current sizes.
fn refusal_key(working: &[Working], left: usize, right: usize) -> (usize, usize, usize, usize) {
    let size = |index: usize| working.get(index).map_or(0, |entry| entry.members.len());
    if left <= right {
        (left, right, size(left), size(right))
    } else {
        (right, left, size(right), size(left))
    }
}

/// The nearest active, unrefused cluster to `index`.
///
/// The refusal set is consulted here rather than at the merge site, so a refused pair
/// does not merely fail its verification each round - it stops being a candidate at all
/// until one of the two clusters grows.
fn nearest_of(
    working: &[Working],
    index: usize,
    refused: &std::collections::BTreeSet<(usize, usize, usize, usize)>,
) -> Option<(usize, f32)> {
    let source = working.get(index)?;
    if !source.active {
        return None;
    }

    let mut best: Option<(usize, f32)> = None;
    for (other, entry) in working.iter().enumerate() {
        if other == index || !entry.active {
            continue;
        }
        if refused.contains(&refusal_key(working, index, other)) {
            continue;
        }
        let dist = linkage(source, entry);
        // Ties broken by index, so the neighbour choice is a total order.
        let better = match best {
            None => true,
            Some((best_other, best_dist)) => {
                dist < best_dist || (dist.to_bits() == best_dist.to_bits() && other < best_other)
            }
        };
        if better {
            best = Some((other, dist));
        }
    }
    best
}

/// Cohesion verification: is the gap between two clusters small relative to how spread
/// out they already are?
///
/// The chain-merge guard. See this module's header, section 3, for why this and not a
/// rank test - and for the two cases the ratio has to separate.
///
/// A pair of singletons has no internal spread to compare against, so [`COHESION_FLOOR`]
/// stands in for it.
fn cohesion_ok(working: &[Working], left: usize, right: usize, config: &ClusterConfig) -> bool {
    let (Some(a), Some(b)) = (working.get(left), working.get(right)) else {
        return false;
    };
    let floor = COHESION_FLOOR;
    let spread = a
        .spread()
        .unwrap_or(floor)
        .max(b.spread().unwrap_or(floor))
        .max(floor);
    linkage(a, b) <= spread * config.max_cohesion_ratio
}

/// Build a finished cluster: centroid, variance and, when earned, sub-centroids.
fn finish_cluster(faces: &[ClusterFace], members: &[usize], config: &ClusterConfig) -> Cluster {
    let centroid = mean_template(faces, members);
    let variance = mean_distance(faces, members, &centroid);

    let sub_centroids = if variance > config.sub_variance && members.len() >= 4 {
        sub_cluster(faces, members, config)
    } else {
        Vec::new()
    };

    Cluster {
        members: members.to_vec(),
        centroid,
        sub_centroids,
        variance,
    }
}

/// The L2-normalised mean of a set of templates.
fn mean_template(faces: &[ClusterFace], members: &[usize]) -> Vec<f32> {
    let width = faces
        .first()
        .map_or(crate::face::embed::TEMPLATE_DIM, |face| face.template.len());
    let mut sum = vec![0.0f32; width];
    let mut count = 0usize;
    for index in members {
        let Some(face) = faces.get(*index) else {
            continue;
        };
        for (slot, value) in sum.iter_mut().zip(face.template.iter()) {
            *slot += value;
        }
        count += 1;
    }
    if count > 0 {
        let scale = 1.0 / count as f32;
        for slot in &mut sum {
            *slot *= scale;
        }
    }
    aura_index::query::normalise(&mut sum);
    sum
}

/// Mean cosine distance from members to a reference vector.
fn mean_distance(faces: &[ClusterFace], members: &[usize], reference: &[f32]) -> f32 {
    let mut total = 0.0f32;
    let mut count = 0usize;
    for index in members {
        let Some(face) = faces.get(*index) else {
            continue;
        };
        total += distance(reference, &face.template);
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        total / count as f32
    }
}

/// Split one high-variance identity into sub-clusters, and return their means.
///
/// Section 6.2's outfit-and-hairstyle case. The same agglomerative machinery at a
/// tighter threshold, run on one identity's members, which is a handful of faces
/// rather than thousands - so the cost is irrelevant and the code path is shared.
///
/// The sub-centroids are *additional* match targets, never a split of the identity:
/// the photographer sees one person, and phases that ask "is this the bride" get a
/// yes from either look.
fn sub_cluster(faces: &[ClusterFace], members: &[usize], config: &ClusterConfig) -> Vec<Vec<f32>> {
    let subset: Vec<ClusterFace> = members
        .iter()
        .filter_map(|index| faces.get(*index).cloned())
        .collect();
    let tighter = ClusterConfig {
        // Two-thirds of the merge threshold: tight enough to separate two distinct
        // looks, loose enough not to shatter one look into singletons.
        threshold: config.threshold * 0.66,
        // Cohesion is relaxed inside one identity. The point of this pass is to *find*
        // the two looks, and the cohesion test is exactly what would refuse to separate
        // them - it is a guard against merging two people, and there is only one person
        // here by construction.
        max_cohesion_ratio: f32::INFINITY,
        min_cluster: 2,
        // No recursion. One level of sub-clustering is a description of a person's
        // two looks; two levels is a description of the lighting.
        sub_variance: f32::INFINITY,
        ..*config
    };
    let inner = cluster(&subset, &tighter);
    let mut out: Vec<Vec<f32>> = inner
        .clusters
        .into_iter()
        .map(|found| found.centroid)
        .collect();
    out.truncate(config.max_sub);
    out
}

/// The best and second-best identity for one template.
///
/// Returns `(index, best distance, second-best distance)`. The second is `None`
/// when there is only one identity, in which case the margin test cannot fail -
/// which is right: with one candidate there is no ambiguity to protect against.
fn best_two(clusters: &[Cluster], template: &[f32]) -> Option<(usize, f32, Option<f32>)> {
    let mut best: Option<(usize, f32)> = None;
    let mut second: Option<f32> = None;
    for (index, cluster) in clusters.iter().enumerate() {
        let dist = cluster.distance_to(template);
        match best {
            None => best = Some((index, dist)),
            Some((_, best_distance)) if dist < best_distance => {
                second = Some(best_distance);
                best = Some((index, dist));
            }
            Some(_) => {
                if second.is_none_or(|current| dist < current) {
                    second = Some(dist);
                }
            }
        }
    }
    best.map(|(index, dist)| (index, dist, second))
}

/// Pairwise precision, recall and F1 against labelled ground truth.
///
/// The metric section 10.1 states the gate in. Pairwise rather than cluster
/// accuracy, because pairwise is the metric that punishes the failure that matters:
/// one chain merge joining two thirty-face identities creates 900 false positive
/// pairs and collapses precision, where a cluster-counting metric would report two
/// clusters instead of three and look almost right.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClusterMetrics {
    /// Pairs the clustering joined that ground truth also joins.
    pub true_positive: u64,
    /// Pairs the clustering joined that ground truth separates.
    pub false_positive: u64,
    /// Pairs ground truth joins that the clustering separated.
    pub false_negative: u64,
    /// True positives over everything joined.
    pub precision: f32,
    /// True positives over everything that should have been joined.
    pub recall: f32,
    /// Harmonic mean.
    pub f1: f32,
    /// Predicted clusters that contain faces of more than one labelled person.
    ///
    /// Section 10.1 states this separately from F1 and as an absolute: "no cluster
    /// contains two labelled people". It has to be separate, because a single
    /// impure cluster in a large wedding barely moves F1 and is still the failure
    /// the photographer will see.
    pub impure_clusters: u32,
}

/// Compare a predicted assignment with ground truth.
///
/// Both slices are per-face labels of the same length. A negative or absent label
/// means "not assigned", and unassigned faces are excluded from the pair counts on
/// both sides - a face the product deliberately declined to place is not a wrong
/// answer, it is a refusal, and [`ClusterOutcome::unassigned`] is where its cost is
/// accounted for.
#[must_use]
pub fn pairwise_metrics(truth: &[Option<u32>], predicted: &[Option<u32>]) -> ClusterMetrics {
    let mut metrics = ClusterMetrics::default();
    let count = truth.len().min(predicted.len());

    for left in 0..count {
        for right in (left + 1)..count {
            let (Some(Some(truth_left)), Some(Some(truth_right))) =
                (truth.get(left), truth.get(right))
            else {
                continue;
            };
            let (Some(predicted_left), Some(predicted_right)) =
                (predicted.get(left), predicted.get(right))
            else {
                continue;
            };
            let same_truth = truth_left == truth_right;
            let same_predicted = match (predicted_left, predicted_right) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            match (same_truth, same_predicted) {
                (true, true) => metrics.true_positive += 1,
                (false, true) => metrics.false_positive += 1,
                (true, false) => metrics.false_negative += 1,
                (false, false) => {}
            }
        }
    }

    let joined = metrics.true_positive + metrics.false_positive;
    let should = metrics.true_positive + metrics.false_negative;
    metrics.precision = if joined == 0 {
        1.0
    } else {
        metrics.true_positive as f32 / joined as f32
    };
    metrics.recall = if should == 0 {
        1.0
    } else {
        metrics.true_positive as f32 / should as f32
    };
    metrics.f1 = if metrics.precision + metrics.recall <= f32::EPSILON {
        0.0
    } else {
        2.0 * metrics.precision * metrics.recall / (metrics.precision + metrics.recall)
    };

    // Impurity: for every predicted cluster, how many distinct truth labels it
    // holds. A BTreeMap rather than a HashMap because `check-banned.sh` refuses the
    // latter, for the reason it exists: iteration order must not vary.
    let mut per_cluster: std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    for index in 0..count {
        let (Some(Some(assigned)), Some(Some(label))) = (predicted.get(index), truth.get(index))
        else {
            continue;
        };
        per_cluster.entry(*assigned).or_default().insert(*label);
    }
    metrics.impure_clusters = per_cluster
        .values()
        .filter(|labels| labels.len() > 1)
        .count() as u32;

    metrics
}

/// One point of a threshold sweep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepPoint {
    /// The threshold that was tried.
    pub threshold: f32,
    /// What it scored.
    pub metrics: ClusterMetrics,
    /// Identities it produced.
    pub clusters: usize,
}

/// The per-project validation sweep of section 6.2.
///
/// "Threshold tuned per project by a small validation sweep using high-quality
/// faces only" - so the sweep runs on the faces that pass the gate, and the choice
/// is by F1 with impurity as a hard veto. A threshold that scores 0.94 F1 with one
/// impure cluster loses to one that scores 0.93 with none, because an impure
/// cluster is the failure a photographer notices.
///
/// Returns the sweep and the chosen threshold, so QA sees the shape of the curve
/// rather than only its argmax - a flat curve and a sharp peak call for different
/// amounts of trust.
#[must_use]
pub fn tune_threshold(
    faces: &[ClusterFace],
    truth: &[Option<u32>],
    candidates: &[f32],
    base: &ClusterConfig,
) -> (Vec<SweepPoint>, f32) {
    let mut sweep = Vec::with_capacity(candidates.len());
    for threshold in candidates {
        let config = ClusterConfig {
            threshold: *threshold,
            ..*base
        };
        let outcome = cluster(faces, &config);
        let predicted = assignment_labels(faces.len(), &outcome);
        sweep.push(SweepPoint {
            threshold: *threshold,
            metrics: pairwise_metrics(truth, &predicted),
            clusters: outcome.clusters.len(),
        });
    }

    let chosen = sweep
        .iter()
        .min_by(|a, b| {
            // Impurity first, then F1 descending, then the lower threshold - which
            // is the conservative direction: a lower threshold splits rather than
            // merges, and a split identity is a merge dialog away from correct
            // while a merged one has to be found first.
            a.metrics
                .impure_clusters
                .cmp(&b.metrics.impure_clusters)
                .then_with(|| {
                    b.metrics
                        .f1
                        .partial_cmp(&a.metrics.f1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    a.threshold
                        .partial_cmp(&b.threshold)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .map_or(base.threshold, |point| point.threshold);

    (sweep, chosen)
}

/// Per-face cluster labels from an outcome, for the metrics.
#[must_use]
pub fn assignment_labels(face_count: usize, outcome: &ClusterOutcome) -> Vec<Option<u32>> {
    let mut labels = vec![None; face_count];
    for (cluster_index, cluster) in outcome.clusters.iter().enumerate() {
        for member in &cluster.members {
            if let Some(slot) = labels.get_mut(*member) {
                *slot = u32::try_from(cluster_index).ok();
            }
        }
    }
    labels
}
