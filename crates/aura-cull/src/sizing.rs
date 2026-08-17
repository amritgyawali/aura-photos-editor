//! How big the gallery should be, and how it gets there.
//!
//! PHASE-12 section 6.4: "predict the deliverable count with a small regression trained on
//! real delivered galleries (features: total frames, moments, chapters, hours, keeper-score
//! distribution); typical output 22-38 % of shot volume."
//!
//! ## What ships, said plainly
//!
//! **The regression is authored, not trained.** Fitting it needs sixty real delivered
//! galleries and this repository has none. What ships is the same feature vector with
//! coefficients chosen by argument, an output clamped into section 6.4's own 22-38 % band,
//! and a reason written beside every coefficient for the sign it has.
//!
//! That is weaker than the phase asks for and it is honest about being weaker. It is
//! condition C3 in the exit report, and it is the same situation phase 10's Bradley-Terry
//! ranker is in: the shape is right, the pipeline around it is real, and the numbers are
//! arguments rather than measurements.
//!
//! It is also, deliberately, the part of this phase that matters least. The slider exists
//! precisely so that a wrong prediction costs a photographer one drag rather than a
//! re-analysis, and section 11 budgets two seconds for that drag.
//!
//! ## Why the model predicts a *rate* rather than a count
//!
//! Because the thing that generalises across weddings is the fraction, not the number. A
//! four-hundred-frame elopement and a six-thousand-frame three-day wedding have nothing in
//! common in absolute terms and are both delivered at roughly a quarter to a third. A model
//! that predicted counts would need the volume as its dominant feature and would spend all
//! its capacity re-learning multiplication.
//!
//! ## Why the reconciliation adds runner-ups rather than lowering the floor
//!
//! When a gallery comes out short of its target, the frames to add are the best ones that
//! were rejected - which are, by construction, the runner-ups of moments that already
//! contributed. Lowering the floor instead would add frames from *everywhere*, including
//! the parts of the day that were shot badly, which is how a gallery gets padded with the
//! photographs the photographer would be embarrassed by.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::ImageId;
use aura_core::KeepScore;

/// The features section 6.4 names.
///
/// Per-wedding rather than per-frame, and all six are cheap: they come from counting the
/// input, which the engine has already assembled.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SizeFeatures {
    /// Photographs that carried a phase 09 verdict. The volume the rate applies to.
    pub eligible: u32,
    /// How many moments phase 08 found.
    pub moments: u32,
    /// How many chapters phase 07 found frames in.
    pub chapters: u32,
    /// How long the wedding ran, in hours.
    pub hours: f32,
    /// Mean fused score over eligible frames.
    pub mean_score: f32,
    /// The fused score at the seventy-fifth percentile.
    ///
    /// The distribution feature section 6.4 asks for, as one number. The upper quartile
    /// rather than the standard deviation, because what decides how many frames a wedding
    /// deserves is how many *good* ones it has, and a wedding can have a wide spread
    /// because it is varied or because half of it is unusable.
    pub p75_score: f32,
}

/// The rate a wedding with entirely average features is delivered at.
///
/// Thirty per cent, the middle of section 6.4's 22-38 % band. Every coefficient below is a
/// deviation from this.
pub const BASE_RATE: f32 = 0.30;

/// The lowest rate the model may predict. Section 6.4's band.
pub const MIN_RATE: f32 = 0.22;

/// The highest. Section 6.4's band.
pub const MAX_RATE: f32 = 0.38;

/// How much a wedding of consistently strong frames raises the rate.
///
/// **Positive**, and this is the coefficient most likely to be argued with. A wedding whose
/// frames score well is a wedding where more frames are deliverable, so more of them should
/// be delivered - a fixed rate would punish a photographer for having a good day. The
/// magnitude is small because the alternative reading is also true: a high mean can mean an
/// easy wedding rather than a good one.
pub const QUALITY_COEFF: f32 = 0.18;

/// How much a wedding with a strong upper quartile raises it.
///
/// **Positive**, for the reason the field exists: what decides how many frames a wedding
/// deserves is how many good ones it has. Smaller than the mean's coefficient so the two
/// cannot compound into an extreme.
pub const UPPER_QUARTILE_COEFF: f32 = 0.10;

/// How much a long day lowers the rate.
///
/// **Negative.** A twelve-hour wedding is not delivered at the same fraction as a four-hour
/// one: the extra hours are the reception and the dance floor, which are the most heavily
/// shot and most heavily culled parts of the day. Expressed against an eight-hour
/// reference, which is what a full-day booking is nearly everywhere.
pub const HOURS_COEFF: f32 = -0.05;

/// How much a wedding shot in many short bursts lowers the rate.
///
/// **Negative.** Frames per moment is a measure of how repetitively the photographer shot;
/// a wedding averaging nine frames per moment has more redundancy to remove than one
/// averaging three. Expressed against a reference of four frames per moment, which is the
/// middle of the band phase 08's own exit report reports.
pub const BURSTINESS_COEFF: f32 = -0.06;

/// How much a wedding whose story covers the whole day raises the rate.
///
/// **Positive**, and the smallest coefficient here. A wedding with frames in all nine
/// chapters is a full day that needs representing across all of it; one with frames in
/// three is a shorter event. Small because chapter count saturates immediately - almost
/// every full wedding reaches seven or eight.
pub const BREADTH_COEFF: f32 = 0.04;

/// Predict how many frames to deliver.
///
/// `scale` is the mode's multiplier from `cull_weights.toml`. It is applied to the *count*
/// rather than to the rate so that the rate stays inside its documented band and the mode's
/// effect stays legible in telemetry: a `Conservative` gallery is 1.2 times a `Balanced`
/// one, exactly, whatever the wedding.
#[must_use]
pub fn predict(features: SizeFeatures, scale: f32) -> u32 {
    if features.eligible == 0 {
        return 0;
    }
    let frames_per_moment = if features.moments == 0 {
        4.0
    } else {
        f32::from(u16::try_from(features.eligible.min(65_535)).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(features.moments.min(65_535)).unwrap_or(u16::MAX))
    };

    let rate = BASE_RATE
        + QUALITY_COEFF * (features.mean_score - 0.5)
        + UPPER_QUARTILE_COEFF * (features.p75_score - 0.6)
        + HOURS_COEFF * ((features.hours - 8.0) / 8.0).clamp(-1.0, 1.0)
        + BURSTINESS_COEFF * ((frames_per_moment - 4.0) / 4.0).clamp(-1.0, 2.0)
        + BREADTH_COEFF
            * ((f32::from(u16::try_from(features.chapters).unwrap_or(0)) - 5.0) / 4.0)
                .clamp(-1.0, 1.0);

    let clamped = rate.clamp(MIN_RATE, MAX_RATE);
    let eligible = f32::from(u16::try_from(features.eligible.min(65_535)).unwrap_or(u16::MAX));
    let count = eligible * clamped * scale.clamp(0.5, 2.0);
    (count + 0.5) as u32
}

/// What the reconciliation changed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SizeOutcome {
    /// Frames added to reach the target, best first.
    pub added: Vec<ImageId>,
    /// Frames removed to reach it, weakest first.
    pub trimmed: Vec<ImageId>,
}

/// Move a gallery toward a target size.
///
/// Adds the strongest rejected frames when it is short and removes the weakest unprotected
/// ones when it is long. It never touches a protected frame in either direction, and the
/// coverage guard runs again afterwards so that a trim which would have broken a guarantee
/// is repaired before anything is stored.
///
/// `pool` is every frame the engine considered and did not keep, in no particular order;
/// this function sorts it.
#[must_use]
pub fn reconcile(
    target: u32,
    pool: &[ImageId],
    scores: &BTreeMap<ImageId, KeepScore>,
    floors: &BTreeMap<ImageId, f32>,
    protected: &BTreeSet<ImageId>,
    kept: &mut BTreeSet<ImageId>,
) -> SizeOutcome {
    let mut outcome = SizeOutcome::default();
    let target = target as usize;

    if kept.len() < target {
        let mut available: Vec<ImageId> = pool
            .iter()
            .copied()
            .filter(|image| !kept.contains(image))
            // Only frames that clear their own scene's floor. Padding a gallery with
            // frames nobody would have delivered is how "we hit the number" becomes "the
            // last hundred photographs are filler".
            .filter(|image| {
                let score = scores.get(image).map_or(0.0, |keep| keep.scene_weighted);
                score >= floors.get(image).copied().unwrap_or(0.0) && score > 0.0
            })
            .collect();
        available.sort_by(|left, right| {
            let left_score = scores.get(left).map_or(0.0, |keep| keep.scene_weighted);
            let right_score = scores.get(right).map_or(0.0, |keep| keep.scene_weighted);
            right_score
                .total_cmp(&left_score)
                .then_with(|| left.cmp(right))
        });
        for image in available {
            if kept.len() >= target {
                break;
            }
            kept.insert(image);
            outcome.added.push(image);
        }
        return outcome;
    }

    if kept.len() > target {
        let mut removable: Vec<ImageId> = kept
            .iter()
            .copied()
            .filter(|image| !protected.contains(image))
            .collect();
        removable.sort_by(|left, right| {
            let left_score = scores.get(left).map_or(0.0, |keep| keep.scene_weighted);
            let right_score = scores.get(right).map_or(0.0, |keep| keep.scene_weighted);
            left_score
                .total_cmp(&right_score)
                .then_with(|| left.cmp(right))
        });
        for image in removable {
            if kept.len() <= target {
                break;
            }
            kept.remove(&image);
            outcome.trimmed.push(image);
        }
    }
    outcome
}

/// The seventy-fifth percentile of a set of scores.
///
/// Nearest-rank rather than interpolated, because the input is already a set of estimates
/// and interpolating between two of them adds precision that is not there.
#[must_use]
pub fn upper_quartile(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    let index = (values.len() * 3) / 4;
    values
        .get(index.min(values.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0.0)
}
