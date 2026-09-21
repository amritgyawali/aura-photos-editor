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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use aura_core::{KeepScore, SceneId};
    use uuid::Uuid;

    use super::{predict, reconcile, upper_quartile, SizeFeatures, BASE_RATE, MAX_RATE, MIN_RATE};

    fn image(n: u128) -> aura_core::contract::cull::ImageId {
        aura_core::PhotoId::from_uuid(Uuid::from_u128(n))
    }

    fn score(image_id: aura_core::contract::cull::ImageId, value: f32) -> KeepScore {
        KeepScore {
            image_id,
            technical: value,
            emotion: value,
            composition: value,
            prominence: value,
            scene_weighted: value,
            scene: SceneId::Unknown,
            calibration_ver: KeepScore::UNCALIBRATED,
        }
    }

    /// A wedding whose every feature sits at the model's own centre.
    fn average() -> SizeFeatures {
        SizeFeatures {
            eligible: 4_000,
            moments: 1_000,
            chapters: 5,
            hours: 8.0,
            mean_score: 0.5,
            p75_score: 0.6,
        }
    }

    #[test]
    fn a_wedding_with_average_features_is_delivered_at_the_base_rate() {
        // Every coefficient in the model is a deviation from `BASE_RATE`, so a wedding sitting
        // on the centre of all six has to come out at exactly it. If this drifts, one of the
        // six centring constants has moved and the whole model is quietly re-based.
        let predicted = predict(average(), 1.0);
        let expected = (4_000.0 * BASE_RATE + 0.5) as u32;
        assert_eq!(predicted, expected);
    }

    #[test]
    fn the_predicted_rate_never_leaves_the_documented_band() {
        // Section 6.4's 22-38 %. The clamp is what makes the band a guarantee rather than a
        // description of what the coefficients happen to produce, and the two extremes below
        // are well outside anything the coefficients could reach on their own.
        let best = SizeFeatures {
            mean_score: 1.0,
            p75_score: 1.0,
            hours: 0.0,
            chapters: 9,
            moments: 4_000,
            ..average()
        };
        let worst = SizeFeatures {
            mean_score: 0.0,
            p75_score: 0.0,
            hours: 20.0,
            chapters: 1,
            moments: 100,
            ..average()
        };
        let high = f64::from(predict(best, 1.0)) / 4_000.0;
        let low = f64::from(predict(worst, 1.0)) / 4_000.0;
        assert!(high <= f64::from(MAX_RATE) + 0.001, "{high}");
        assert!(low >= f64::from(MIN_RATE) - 0.001, "{low}");
    }

    #[test]
    fn a_wedding_with_no_eligible_frames_is_delivered_as_nothing_rather_than_as_a_rate() {
        assert_eq!(predict(SizeFeatures::default(), 1.0), 0);
    }

    #[test]
    fn the_mode_scales_the_count_exactly_and_leaves_the_rate_inside_its_band() {
        // "A `Conservative` gallery is 1.2 times a `Balanced` one, exactly, whatever the
        // wedding" - which is only true because the scale multiplies the count rather than
        // the rate. Scaling the rate would push it out of the documented band and then clamp,
        // and the mode's effect would silently disappear on a strong wedding.
        let balanced = predict(average(), 1.0);
        let conservative = predict(average(), 1.2);
        let ratio = f64::from(conservative) / f64::from(balanced);
        assert!((ratio - 1.2).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn a_short_gallery_is_padded_only_with_frames_that_clear_their_own_floor() {
        // "Padding a gallery with frames nobody would have delivered is how *we hit the
        // number* becomes *the last hundred photographs are filler*."
        let strong = image(1);
        let weak = image(2);
        let mut scores = BTreeMap::new();
        scores.insert(strong, score(strong, 0.80));
        scores.insert(weak, score(weak, 0.10));
        let mut floors = BTreeMap::new();
        floors.insert(strong, 0.35);
        floors.insert(weak, 0.35);

        let mut kept = BTreeSet::new();
        let outcome = reconcile(
            5,
            &[strong, weak],
            &scores,
            &floors,
            &BTreeSet::new(),
            &mut kept,
        );
        assert_eq!(outcome.added, vec![strong]);
        assert!(
            !kept.contains(&weak),
            "a frame under its floor was used as filler"
        );
        assert!(kept.len() < 5, "the target was met with filler");
    }

    #[test]
    fn a_long_gallery_is_trimmed_weakest_first_and_never_touches_a_protected_frame() {
        let ids: Vec<_> = (1..=4).map(image).collect();
        let mut scores = BTreeMap::new();
        for (index, id) in ids.iter().enumerate() {
            scores.insert(*id, score(*id, 0.2 + 0.1 * index as f32));
        }
        // The weakest frame is also the one a coverage guarantee is holding.
        let mut protected = BTreeSet::new();
        protected.insert(ids[0]);

        let mut kept: BTreeSet<_> = ids.iter().copied().collect();
        let outcome = reconcile(2, &[], &scores, &BTreeMap::new(), &protected, &mut kept);
        assert!(
            kept.contains(&ids[0]),
            "a guarantee was trimmed to hit a number"
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(outcome.trimmed, vec![ids[1], ids[2]]);
    }

    #[test]
    fn reconciling_a_gallery_that_is_already_the_right_size_changes_nothing() {
        let ids: Vec<_> = (1..=3).map(image).collect();
        let mut kept: BTreeSet<_> = ids.iter().copied().collect();
        let before = kept.clone();
        let outcome = reconcile(
            3,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeSet::new(),
            &mut kept,
        );
        assert_eq!(kept, before);
        assert_eq!(outcome, super::SizeOutcome::default());
    }

    #[test]
    fn the_upper_quartile_is_nearest_rank_rather_than_interpolated() {
        // The input is already a set of estimates, and interpolating between two of them adds
        // precision that is not there.
        assert!((upper_quartile(vec![0.1, 0.2, 0.3, 0.4]) - 0.4).abs() < 1e-6);
        assert_eq!(upper_quartile(Vec::new()), 0.0);
        assert!((upper_quartile(vec![0.5]) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_bursty_wedding_is_delivered_at_a_lower_rate_than_a_deliberate_one() {
        // Twelve frames a moment is a photographer who sprays; three is one who waits. The
        // second earns a higher keep rate from the same number of frames.
        let bursty = SizeFeatures {
            moments: 333,
            ..average()
        };
        let deliberate = SizeFeatures {
            moments: 1_333,
            ..average()
        };
        assert!(predict(bursty, 1.0) < predict(deliberate, 1.0));
    }
}
