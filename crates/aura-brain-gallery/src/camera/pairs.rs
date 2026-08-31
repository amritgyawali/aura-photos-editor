//! Which photographs prove two bodies were in the same light.
//!
//! Section 8 step 3, and section 6.1's "matched pairs are the gold standard". Everything the solver
//! is allowed to believe about this wedding comes through here, so the module's whole job is to be
//! **hard to satisfy**: a pair that should not have been formed is a wrong correction applied to
//! every frame a body shot, and there is no per-frame review that would catch it.
//!
//! ## Four filters, and the fourth is the one that matters
//!
//! 1. **The same scene node.** Phase 25's tree is this pairing's outer key. Two frames in different
//!    nodes were shot under different light *by construction* - that is what a node is - whatever
//!    their subjects look like. This filter costs nothing and removes most of the wedding.
//! 2. **The same flash state.** A flash frame and an ambient frame of the same moment differ in the
//!    way the two populations exist to separate.
//! 3. **Close in time, and similar subjects.** [`Matching::max_gap_ms`][g] and phase 05's cosine
//!    similarity. Both are cheap pre-filters and neither is evidence.
//! 4. **Backgrounds that agree.** Section 6.1: "verify by comparing background statistics rather
//!    than subjects". This is the filter that decides, and the reason it is stated so specifically
//!    in the phase document is that the obvious alternative is circular: two frames of the same
//!    bride's face from two bodies differ *in exactly the way this phase is trying to measure*, so
//!    scoring the pair on the subject scores the thing under test. A wall, a marquee ceiling and a
//!    row of chairs were lit by the same light and are not what either camera was metering for.
//!
//! [g]: super::policy::Matching::max_gap_ms
//!
//! ## The held-out split is decided here and never moves
//!
//! [`split_heldout`] takes a quarter of the verified pairs, and it takes them by a **hash of the
//! two photographs' own ids** rather than at random or by position. Random makes a transform a
//! function of a seed nobody stored, which breaks invariant 4; by position makes the split
//! correlate with time, so a solver would be checked entirely against the end of the ceremony.
//! A content-derived hash is stable across runs, uncorrelated with everything, and reproducible
//! from the row - and migration 26's `camera_pair_heldout_is_fixed` trigger makes it immutable once
//! written, because a held-out flag that could move would let a re-solve quietly promote the pairs
//! that happened to agree with it.
//!
//! ## A rejected pair is written rather than dropped
//!
//! Phase 17's rule, second application in the product, and for the same reason: here the rejection
//! *is* the evidence a photographer needs. "Both cameras shot the whole ceremony and AURA still
//! used a brand baseline" is answered by a list of forty candidate pairs whose backgrounds
//! disagreed, and by nothing else.

use std::collections::BTreeMap;

use aura_core::contract::camera::{FlashState, MatchedPair};
use aura_core::contract::ids::PairId;
use aura_core::contract::moment::CameraId;

use super::fingerprint::CameraFrame;
use super::policy::Matching;
use super::{ANALYSIS_VER, MAX_PAIRS_PER_CAMERA};

/// One in four verified pairs is held out. See [`split_heldout`].
///
/// Expressed as a divisor rather than as the contract's `HELDOUT_SHARE` float because the split is
/// a modulus over a hash, and comparing a hash against a fraction is a rounding argument nobody
/// needs to have.
pub const HELDOUT_DIVISOR: u64 = 4;

/// Every candidate pair between the reference body and one other, verified or not.
///
/// The answer is ordered by background agreement, best first, and truncated to
/// [`MAX_PAIRS_PER_CAMERA`] **verified** pairs - rejected ones are kept beyond the cap up to the
/// same number again, because a report that says "we rejected forty" needs forty rows and a solver
/// that uses a hundred and sixty does not care about the hundred and sixty-first.
///
/// `frames` must be the whole project. The pass calls this once per non-reference body.
#[must_use]
pub fn find(
    frames: &[CameraFrame],
    reference: &CameraId,
    other: &CameraId,
    policy: &Matching,
) -> Vec<MatchedPair> {
    // Index the reference body's pairable frames by node, so the inner loop is over one node's
    // worth of candidates rather than over the wedding. A 4,000-frame wedding with forty nodes
    // makes this the difference between 8 million comparisons and 200,000.
    let mut by_node: BTreeMap<(String, FlashState), Vec<usize>> = BTreeMap::new();
    for (index, frame) in frames.iter().enumerate() {
        if &frame.camera != reference || !frame.is_pairable() {
            continue;
        }
        let Some(node) = frame.node else { continue };
        by_node
            .entry((node.to_db(), frame.flash))
            .or_default()
            .push(index);
    }

    let mut out: Vec<MatchedPair> = Vec::new();
    for right in frames {
        if &right.camera != other || !right.is_pairable() {
            continue;
        }
        let Some(node) = right.node else { continue };
        let scene = policy.scene(right.scene);
        if !scene.pairable {
            // The light moves inside this scene, so two frames of it ninety seconds apart were not
            // in the same light however similar they look. Not a rejection worth storing: nothing
            // was compared, so there is nothing to report about.
            continue;
        }
        let Some(candidates) = by_node.get(&(node.to_db(), right.flash)) else {
            continue;
        };

        // The nearest reference frame in time inside the node, and only that one. A body that shot
        // three hundred frames of a ceremony beside another that shot three hundred would otherwise
        // produce ninety thousand candidate pairs of which the solver would use the same
        // information a hundred times over.
        let mut best: Option<(&CameraFrame, i64)> = None;
        for index in candidates {
            let Some(left) = frames.get(*index) else {
                continue;
            };
            let gap = (left.timeline_ms - right.timeline_ms).abs();
            if gap > policy.max_gap_ms {
                continue;
            }
            if best.is_none_or(|(_, best_gap)| gap < best_gap) {
                best = Some((left, gap));
            }
        }
        let Some((left, gap_ms)) = best else { continue };

        let similarity = cosine(
            left.embedding.as_deref().unwrap_or(&[]),
            right.embedding.as_deref().unwrap_or(&[]),
        );
        if similarity < policy.min_similarity {
            continue;
        }

        let agreement = match (left.background.as_ref(), right.background.as_ref()) {
            (Some(a), Some(b)) => a.agreement(b),
            _ => 0.0,
        };
        let verified = agreement >= scene.background_agreement;

        out.push(MatchedPair {
            id: PairId::new(),
            node,
            left: left.image,
            right: right.image,
            left_camera: left.camera.clone(),
            right_camera: right.camera.clone(),
            flash: right.flash,
            gap_ms,
            subject_similarity: similarity,
            background_agreement: agreement,
            verified,
            // Decided by `split_heldout`, after the whole set is known. A flag set here would
            // depend on what had been found so far, which is a different split for the same
            // wedding depending on the order the frames arrived in.
            held_out: false,
            analysis_ver: ANALYSIS_VER,
        });
    }

    // Best agreement first, then oldest first so the order does not depend on how the frames were
    // enumerated. Invariant 4.
    out.sort_by(|a, b| {
        b.background_agreement
            .partial_cmp(&a.background_agreement)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.gap_ms.cmp(&b.gap_ms))
            .then_with(|| a.left.to_db().cmp(&b.left.to_db()))
            .then_with(|| a.right.to_db().cmp(&b.right.to_db()))
    });
    truncate(out)
}

/// Keep the best [`MAX_PAIRS_PER_CAMERA`] verified pairs and as many rejected ones again.
fn truncate(pairs: Vec<MatchedPair>) -> Vec<MatchedPair> {
    let mut verified = 0_usize;
    let mut rejected = 0_usize;
    pairs
        .into_iter()
        .filter(|pair| {
            if pair.verified {
                verified += 1;
                verified <= MAX_PAIRS_PER_CAMERA
            } else {
                rejected += 1;
                rejected <= MAX_PAIRS_PER_CAMERA
            }
        })
        .collect()
}

/// Mark a quarter of the verified pairs as held out, deterministically.
///
/// The hash is FNV-1a over the two photographs' canonical ids, which are stable for the life of the
/// catalog. Three properties follow, and all three are load-bearing:
///
/// * **Stable across runs.** The same wedding produces the same split, so a re-solve is checked
///   against the same evidence and section 6.2's verification means the same thing twice.
/// * **Uncorrelated with time, scene and body.** A split by position would hand the solver the
///   first three quarters of a ceremony and check it against the last quarter, which is a check
///   against a different light.
/// * **Reproducible from the row.** A support case can recompute which pairs were held out from the
///   ids alone.
///
/// An unverified pair is never held out: putting a pair the verifier rejected into the set that
/// judges the solver would make the check harsher for a reason unrelated to the transform. Migration
/// 26 has the same rule as a CHECK.
pub fn split_heldout(pairs: &mut [MatchedPair]) {
    for pair in pairs.iter_mut() {
        pair.held_out = pair.verified && heldout_hash(pair) % HELDOUT_DIVISOR == 0;
    }
}

/// The hash a held-out decision is taken from. Public so the phase gate can recompute it.
#[must_use]
pub fn heldout_hash(pair: &MatchedPair) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in pair
        .left
        .to_db()
        .as_bytes()
        .iter()
        .chain(pair.right.to_db().as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The cosine similarity of two embeddings, or zero when either is missing or degenerate.
#[must_use]
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return 0.0;
    }
    (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0)
}

/// How many of a set are verified, and how many of those are held out.
#[must_use]
pub fn counts(pairs: &[MatchedPair]) -> (u32, u32, u32) {
    let verified = pairs.iter().filter(|p| p.verified).count();
    let held = pairs.iter().filter(|p| p.is_heldout()).count();
    let rejected = pairs.iter().filter(|p| !p.verified).count();
    (
        u32::try_from(verified).unwrap_or(u32::MAX),
        u32::try_from(held).unwrap_or(u32::MAX),
        u32::try_from(rejected).unwrap_or(u32::MAX),
    )
}

#[cfg(test)]
mod tests {
    use aura_core::contract::camera::Brand;
    use aura_core::contract::gallery::ImageId;
    use aura_core::contract::ids::NodeId;
    use aura_core::SceneId;

    use super::super::fingerprint::BackgroundStats;
    use super::*;

    fn frame(
        camera: &str,
        node: NodeId,
        ms: i64,
        scene: SceneId,
        hist: [u8; 512],
        luma: [f32; 4],
    ) -> CameraFrame {
        CameraFrame {
            image: ImageId::new(),
            camera: CameraId::new(camera),
            brand: Brand::Canon,
            shooter: "primary".to_string(),
            flash: FlashState::Ambient,
            node: Some(node),
            scene,
            timeline_ms: ms,
            cct_k: Some(5200.0),
            tint: Some(0.0),
            exposure_ev: Some(0.0),
            subject_luma: Some(0.45),
            wb_conf: 0.8,
            white_uv: Some([0.20, 0.47]),
            skin_uv: Some([0.24, 0.50]),
            skin_luma: Some(0.5),
            contrast: Some(8.0),
            saturation: Some(4.0),
            signature: Some([0.1; 8]),
            embedding: Some(vec![1.0, 0.2, 0.1, 0.0]),
            background: Some(BackgroundStats::from_descriptors(&hist, luma, 0.2)),
        }
    }

    fn same_room() -> ([u8; 512], [f32; 4]) {
        ([4; 512], [0.40, 0.05, 0.38, 0.92])
    }

    fn other_room() -> ([u8; 512], [f32; 4]) {
        let mut hist = [0_u8; 512];
        for (index, slot) in hist.iter_mut().enumerate() {
            *slot = if index < 64 { 200 } else { 0 };
        }
        (hist, [0.10, 0.01, 0.08, 0.35])
    }

    #[test]
    fn two_bodies_in_one_node_close_in_time_make_a_verified_pair() {
        let node = NodeId::new();
        let (hist, luma) = same_room();
        let frames = vec![
            frame("cam_a", node, 0, SceneId::Ceremony, hist, luma),
            frame("cam_b", node, 20_000, SceneId::Ceremony, hist, luma),
        ];
        let pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].verified);
        assert_eq!(pairs[0].left_camera, CameraId::new("cam_a"));
        assert_eq!(pairs[0].gap_ms, 20_000);
    }

    #[test]
    fn similar_subjects_in_different_light_are_rejected_and_the_rejection_is_kept() {
        // The mechanism section 6.1 asks for. The two frames embed identically - the same
        // embedding vector - and their surroundings say they were in two different rooms.
        let node = NodeId::new();
        let (hist_a, luma_a) = same_room();
        let (hist_b, luma_b) = other_room();
        let frames = vec![
            frame("cam_a", node, 0, SceneId::Ceremony, hist_a, luma_a),
            frame("cam_b", node, 10_000, SceneId::Ceremony, hist_b, luma_b),
        ];
        let pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        assert_eq!(
            pairs.len(),
            1,
            "a rejected pair is written rather than dropped"
        );
        assert!(!pairs[0].verified);
        assert!(pairs[0].subject_similarity > 0.99, "the subjects do agree");
    }

    #[test]
    fn a_pair_across_a_node_boundary_is_never_formed() {
        let (hist, luma) = same_room();
        let frames = vec![
            frame("cam_a", NodeId::new(), 0, SceneId::Ceremony, hist, luma),
            frame("cam_b", NodeId::new(), 5_000, SceneId::Ceremony, hist, luma),
        ];
        let pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        assert!(pairs.is_empty());
    }

    #[test]
    fn a_pair_across_the_flash_boundary_is_never_formed() {
        let node = NodeId::new();
        let (hist, luma) = same_room();
        let mut frames = vec![
            frame("cam_a", node, 0, SceneId::Ceremony, hist, luma),
            frame("cam_b", node, 5_000, SceneId::Ceremony, hist, luma),
        ];
        frames[1].flash = FlashState::Flash;
        let pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        assert!(
            pairs.is_empty(),
            "brand differences are amplified under flash"
        );
    }

    #[test]
    fn an_unpairable_scene_supplies_no_evidence_at_all() {
        let node = NodeId::new();
        let (hist, luma) = same_room();
        let frames = vec![
            frame("cam_a", node, 0, SceneId::DanceFloor, hist, luma),
            frame("cam_b", node, 5_000, SceneId::DanceFloor, hist, luma),
        ];
        let pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        assert!(
            pairs.is_empty(),
            "a dance floor's light moves between two frames"
        );
    }

    #[test]
    fn a_gap_wider_than_the_policy_is_never_paired() {
        let node = NodeId::new();
        let (hist, luma) = same_room();
        let policy = Matching::default();
        let frames = vec![
            frame("cam_a", node, 0, SceneId::Ceremony, hist, luma),
            frame(
                "cam_b",
                node,
                policy.max_gap_ms + 1,
                SceneId::Ceremony,
                hist,
                luma,
            ),
        ];
        assert!(find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &policy
        )
        .is_empty());
    }

    #[test]
    fn the_held_out_split_is_a_quarter_and_is_stable_across_runs() {
        let node = NodeId::new();
        let (hist, luma) = same_room();
        let mut frames = Vec::new();
        for i in 0..80_i64 {
            frames.push(frame(
                "cam_a",
                node,
                i * 1_000,
                SceneId::Ceremony,
                hist,
                luma,
            ));
            frames.push(frame(
                "cam_b",
                node,
                i * 1_000 + 200,
                SceneId::Ceremony,
                hist,
                luma,
            ));
        }
        let mut pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        assert_eq!(pairs.len(), 80);
        split_heldout(&mut pairs);
        let (verified, held, _) = counts(&pairs);
        assert_eq!(verified, 80);
        // A hash-derived split is a quarter in expectation rather than exactly; anything inside a
        // factor of two of twenty is the split doing its job.
        assert!((10..=32).contains(&held), "held {held} of {verified}");

        // Stability: running the split again is a no-op, and the same rows come back.
        let before: Vec<bool> = pairs.iter().map(|p| p.held_out).collect();
        split_heldout(&mut pairs);
        let after: Vec<bool> = pairs.iter().map(|p| p.held_out).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn an_unverified_pair_is_never_held_out() {
        let node = NodeId::new();
        let (hist_a, luma_a) = same_room();
        let (hist_b, luma_b) = other_room();
        let mut frames = Vec::new();
        for i in 0..40_i64 {
            frames.push(frame(
                "cam_a",
                node,
                i * 1_000,
                SceneId::Ceremony,
                hist_a,
                luma_a,
            ));
            frames.push(frame(
                "cam_b",
                node,
                i * 1_000 + 200,
                SceneId::Ceremony,
                hist_b,
                luma_b,
            ));
        }
        let mut pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        split_heldout(&mut pairs);
        assert!(pairs.iter().all(|p| !p.held_out));
    }

    #[test]
    fn one_reference_frame_is_used_per_candidate_rather_than_all_of_them() {
        // Three reference frames inside the window and one other-body frame: one pair, not three.
        let node = NodeId::new();
        let (hist, luma) = same_room();
        let frames = vec![
            frame("cam_a", node, 0, SceneId::Ceremony, hist, luma),
            frame("cam_a", node, 1_000, SceneId::Ceremony, hist, luma),
            frame("cam_a", node, 2_000, SceneId::Ceremony, hist, luma),
            frame("cam_b", node, 1_100, SceneId::Ceremony, hist, luma),
        ];
        let pairs = find(
            &frames,
            &CameraId::new("cam_a"),
            &CameraId::new("cam_b"),
            &Matching::default(),
        );
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].gap_ms, 100, "the nearest reference frame in time");
    }

    #[test]
    fn cosine_is_zero_on_a_missing_or_mismatched_embedding() {
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[1.0, 0.0], &[1.0]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
    }
}
