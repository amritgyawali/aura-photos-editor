//! Aggregation: buckets, the held-out split, the trimmed median, and the two bounds.
//!
//! ## The split is deterministic, and this is the four lines that matter
//!
//! [`hold_out`] hashes the correction's own decision id and takes a quarter. Not a shuffle, not a
//! timestamp cut, not "the last quarter".
//!
//! A shuffle re-draws the split on every fit. That means a fit whose measured improvement is
//! disappointing can be re-run until the line falls somewhere flattering - and nothing about that
//! would look wrong in a review, in a test, or in a panel. It is the single easiest way for this
//! feature to become a number generator, and it is the sort of thing that gets added later by
//! somebody making the tests less flaky.
//!
//! A timestamp cut is worse in a subtler way: it holds out the *most recent* corrections, which are
//! exactly the ones a photographer's current taste is in, so the candidate is measured against the
//! taste it is trying to learn.
//!
//! ## The centre is a trimmed median
//!
//! Section 6.3 asks for "robust central tendencies" and for outliers to be discarded. A mean over a
//! bucket that contains one photographer's rescue of a single badly-lit room is a mean with that
//! room in it, and the room is exactly the thing that should not generalise.
//!
//! The trim is three median absolute deviations, which is the conventional robust cut-off and is on
//! the contract as [`OUTLIER_MADS`]. What is dropped is *counted*, because a photographer should be
//! able to see that the loop ignored their four extreme fixes rather than wonder why nothing moved.
//!
//! ## The MAD's degenerate case is the one that bites
//!
//! A bucket in which every correction is identical has a MAD of zero, so every correction is
//! infinitely many deviations from the median and the trim drops the whole bucket. That is a bucket
//! that agrees with itself perfectly, which is the *best* evidence this loop can get. A zero MAD
//! keeps everything, and there is a test.

use aura_core::contract::ids::DecisionId;
use aura_core::contract::learn::{
    Aggregate, CorrectionBucket, HeldOut, LearnCode, LearnReason, HELD_OUT_SHARE, MIN_CORRECTIONS,
    MIN_PROJECTS, OUTLIER_MADS,
};

/// One correction, reduced to what aggregation reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Which decision it corrected. The split's only input.
    pub decision: DecisionId,
    /// Which wedding it came from. `MIN_PROJECTS` counts these.
    pub project: u64,
    /// How far the value moved, signed, in the value's own units.
    pub magnitude: f32,
}

/// Whether a correction is held out of the fit.
///
/// Deterministic in the correction's own id, so the same corrections produce the same split on
/// every machine and every run. See the note at the top of this module for why that matters more
/// than it looks like it should.
#[must_use]
pub fn hold_out(decision: DecisionId) -> bool {
    let digest = blake3::hash(decision.to_db().as_bytes());
    let bytes = digest.as_bytes();
    // The first four bytes as a fraction of the range, compared against the share. Four bytes
    // rather than one, because a share of 0.25 against a single byte quantises to 64/256 and the
    // realised share drifts a per cent from the documented one on a small bucket.
    let head = u32::from_be_bytes([
        *bytes.first().unwrap_or(&0),
        *bytes.get(1).unwrap_or(&0),
        *bytes.get(2).unwrap_or(&0),
        *bytes.get(3).unwrap_or(&0),
    ]);
    (f64::from(head) / f64::from(u32::MAX)) < f64::from(HELD_OUT_SHARE)
}

/// Split a bucket's samples into the fitted and the held-out.
#[must_use]
pub fn split(samples: &[Sample]) -> (Vec<Sample>, Vec<Sample>, HeldOut) {
    let mut fitted = Vec::new();
    let mut held = Vec::new();
    for s in samples {
        if hold_out(s.decision) {
            held.push(*s);
        } else {
            fitted.push(*s);
        }
    }
    let summary = HeldOut {
        deterministic: true,
        fitted: u32::try_from(fitted.len()).unwrap_or(u32::MAX),
        held: u32::try_from(held.len()).unwrap_or(u32::MAX),
    };
    (fitted, held, summary)
}

/// The median of a slice, which is `None` when it is empty.
#[must_use]
pub fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values.get(mid).copied()
    } else {
        match (values.get(mid - 1), values.get(mid)) {
            (Some(a), Some(b)) => Some((a + b) / 2.0),
            _ => None,
        }
    }
}

/// The median absolute deviation, which is how sure a bucket is of itself.
#[must_use]
pub fn mad(values: &[f32], centre: f32) -> f32 {
    let mut deviations: Vec<f32> = values.iter().map(|v| (v - centre).abs()).collect();
    median(&mut deviations).unwrap_or(0.0)
}

/// The scale the outlier trim is measured in.
///
/// **The MAD, with a fallback, and the fallback is the whole point of this function.**
///
/// A median absolute deviation is zero whenever more than half a bucket's samples equal the median
/// exactly - which on this loop's data is the *common* case, because a photographer correcting the
/// same thing the same way forty times produces forty identical magnitudes. A zero scale disables
/// the trim, and the bucket a zero scale appears in is exactly the bucket in which four extreme
/// rescues would otherwise hide: sixty identical corrections and four at twenty times the size
/// still has a MAD of zero.
///
/// So a zero MAD falls back on the *mean* absolute deviation, which is not robust and does not need
/// to be - it is only ever consulted when the median is degenerate, and in that situation any
/// non-zero scale is better than none. Only a bucket in which every sample is identical returns
/// zero, and that is a bucket with nothing to trim.
///
/// This was got wrong first, and the shape is the family phases 19, 21, 22, 25 and 29 wrote down:
/// **a threshold that a correct implementation cannot reach and a threshold that everything
/// necessarily passes are the same bug.** Here the guard that protected the perfect-agreement case
/// also protected the case it existed to catch.
#[must_use]
pub fn scale(values: &[f32], centre: f32) -> f32 {
    let robust = mad(values, centre);
    if robust > f32::EPSILON {
        return robust;
    }
    if values.is_empty() {
        return 0.0;
    }
    let total: f32 = values.iter().map(|v| (v - centre).abs()).sum();
    total / values.len() as f32
}

/// Fold one bucket's samples into an aggregate.
///
/// Returns the aggregate and the reasons a photographer reads. A bucket that does not meet
/// [`MIN_CORRECTIONS`] and [`MIN_PROJECTS`] is still returned - with `actionable` false and the
/// reason that says which floor it missed - because "you have not corrected enough yet" and "your
/// corrections disagree with each other" are different sentences and a panel that returned nothing
/// could say neither.
#[must_use]
pub fn fold(bucket: CorrectionBucket, samples: &[Sample]) -> (Aggregate, Vec<LearnReason>) {
    let mut reasons = Vec::new();
    let (fitted, held, _) = split(samples);

    let mut projects: Vec<u64> = samples.iter().map(|s| s.project).collect();
    projects.sort_unstable();
    projects.dedup();
    let project_count = u32::try_from(projects.len()).unwrap_or(u32::MAX);
    let total = u32::try_from(samples.len()).unwrap_or(u32::MAX);

    let mut magnitudes: Vec<f32> = fitted.iter().map(|s| s.magnitude).collect();
    let Some(centre) = median(&mut magnitudes) else {
        reasons.push(LearnReason::plain(LearnCode::TooFewCorrections));
        return (
            Aggregate {
                bucket,
                corrections: total,
                projects: project_count,
                outliers_dropped: 0,
                central: 0.0,
                dispersion: 0.0,
                held_out: u32::try_from(held.len()).unwrap_or(0),
                actionable: false,
            },
            reasons,
        );
    };

    let spread = scale(&magnitudes, centre);
    // A bucket in which *every* sample is identical has no scale at all, and there is nothing to
    // trim: it agrees with itself perfectly, which is the best evidence this loop can get.
    let kept: Vec<f32> = if spread <= f32::EPSILON {
        magnitudes.clone()
    } else {
        magnitudes
            .iter()
            .copied()
            .filter(|v| (v - centre).abs() <= OUTLIER_MADS * spread)
            .collect()
    };
    let dropped = u32::try_from(magnitudes.len().saturating_sub(kept.len())).unwrap_or(0);
    if dropped > 0 {
        reasons.push(LearnReason::with(
            LearnCode::OutlierDropped,
            format!("{dropped} of {}", magnitudes.len()),
        ));
    }

    let mut kept_sorted = kept.clone();
    let central = median(&mut kept_sorted).unwrap_or(centre);
    let dispersion = scale(&kept, central);

    let enough_corrections = total >= MIN_CORRECTIONS;
    let enough_projects = project_count >= MIN_PROJECTS;
    if !enough_corrections {
        reasons.push(LearnReason::with(
            LearnCode::TooFewCorrections,
            format!("{total} of {MIN_CORRECTIONS}"),
        ));
    }
    if !enough_projects {
        reasons.push(LearnReason::with(
            LearnCode::TooFewWeddings,
            format!("{project_count} of {MIN_PROJECTS}"),
        ));
    }
    if enough_corrections && enough_projects {
        // "Consistent" is dispersion below a fifth of the centre's own size: a bucket whose
        // corrections vary less than the move they are proposing.
        if dispersion <= central.abs() * 0.2 {
            reasons.push(LearnReason::plain(LearnCode::BucketConsistent));
        } else {
            reasons.push(LearnReason::with(
                LearnCode::BucketDispersed,
                format!("spread {dispersion:.3} against a centre of {central:.3}"),
            ));
        }
    }

    let aggregate = Aggregate {
        bucket,
        corrections: total,
        projects: project_count,
        outliers_dropped: dropped,
        central,
        dispersion,
        held_out: u32::try_from(held.len()).unwrap_or(0),
        actionable: enough_corrections && enough_projects,
    };

    // The two bounds. The share bound alone would oscillate, because the next wedding's
    // corrections are measured against a baseline that has already moved; the ceiling alone would
    // let one wedding move a profile a long way. Both, always.
    let offset = aggregate.proposed_offset();
    if enough_corrections && enough_projects {
        let ceiling = bucket.learnable.ceiling();
        if (offset.abs() - ceiling).abs() < 1e-6 {
            reasons.push(LearnReason::with(
                LearnCode::CeilingBinding,
                format!("{ceiling:.3}"),
            ));
        } else if offset.abs() < central.abs() {
            reasons.push(LearnReason::with(
                LearnCode::StepBounded,
                format!("{offset:.3} of a measured {central:.3}"),
            ));
        }
    }

    (aggregate, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::learn::Learnable;
    use aura_core::contract::ledger::DecisionKind;
    use aura_core::contract::scene::SceneId;

    fn bucket() -> CorrectionBucket {
        CorrectionBucket {
            kind: DecisionKind::Edit,
            scene: SceneId::Unknown,
            learnable: Learnable::Exposure,
            subject_close: false,
        }
    }

    fn samples(magnitudes: &[f32], projects: usize) -> Vec<Sample> {
        magnitudes
            .iter()
            .enumerate()
            .map(|(i, m)| Sample {
                decision: DecisionId::new(),
                project: (i % projects) as u64,
                magnitude: *m,
            })
            .collect()
    }

    #[test]
    fn the_split_is_reproducible_from_the_corrections_alone() {
        // The property the whole module hangs off. A shuffle would pass every other test in this
        // file and would let a disappointing fit be re-run until the line fell somewhere
        // flattering.
        let s = samples(&[0.1; 200], 4);
        let (a, b, summary) = split(&s);
        let (a2, b2, summary2) = split(&s);
        assert_eq!(a.len(), a2.len());
        assert_eq!(b.len(), b2.len());
        assert_eq!(summary, summary2);
        assert!(summary.deterministic);
        // ...and one id always lands on the same side.
        let id = DecisionId::new();
        assert_eq!(hold_out(id), hold_out(id));
    }

    #[test]
    fn the_split_is_about_a_quarter() {
        let s = samples(&[0.1; 2000], 4);
        let (_, held, summary) = split(&s);
        let share = held.len() as f32 / 2000.0;
        assert!(
            (share - HELD_OUT_SHARE).abs() < 0.04,
            "held out {share}, wanted about {HELD_OUT_SHARE}"
        );
        assert_eq!(summary.fitted + summary.held, 2000);
    }

    #[test]
    fn a_bucket_that_agrees_with_itself_perfectly_is_not_trimmed_away() {
        // The degenerate case that bites: a MAD of zero makes every sample infinitely many
        // deviations from the median, so a naive trim drops the *best* evidence this loop can get.
        let s = samples(&[0.20; 60], 4);
        let (agg, reasons) = fold(bucket(), &s);
        assert_eq!(agg.outliers_dropped, 0);
        assert!((agg.central - 0.20).abs() < 1e-5);
        assert!(agg.actionable);
        assert!(reasons
            .iter()
            .any(|r| r.code == LearnCode::BucketConsistent));
    }

    #[test]
    fn one_rescue_of_a_badly_lit_room_does_not_move_the_centre() {
        // A mean over this bucket is 0.31; a trimmed median is 0.20, which is what generalises.
        let mut magnitudes = vec![0.20_f32; 40];
        magnitudes.extend([4.0_f32; 4]);
        let s = samples(&magnitudes, 4);
        let (agg, reasons) = fold(bucket(), &s);
        assert!(
            (agg.central - 0.20).abs() < 0.02,
            "centre moved to {}",
            agg.central
        );
        assert!(agg.outliers_dropped > 0);
        assert!(reasons.iter().any(|r| r.code == LearnCode::OutlierDropped));
    }

    #[test]
    fn a_bucket_below_the_floors_is_returned_rather_than_dropped() {
        // "You have not corrected enough yet" and "your corrections disagree" are different
        // sentences, and a panel that got nothing back could say neither.
        let s = samples(&[0.2; 4], 2);
        let (agg, reasons) = fold(bucket(), &s);
        assert!(!agg.actionable);
        assert_eq!(agg.proposed_offset(), 0.0);
        assert!(reasons
            .iter()
            .any(|r| r.code == LearnCode::TooFewCorrections));

        // Enough corrections, one wedding: a different sentence.
        let s = samples(&[0.2; 40], 1);
        let (agg, reasons) = fold(bucket(), &s);
        assert!(!agg.actionable);
        assert!(reasons.iter().any(|r| r.code == LearnCode::TooFewWeddings));
    }

    #[test]
    fn an_offset_is_bounded_by_the_share_and_then_by_the_ceiling() {
        let s = samples(&[0.30; 60], 4);
        let (agg, reasons) = fold(bucket(), &s);
        assert!((agg.proposed_offset() - 0.15).abs() < 1e-3);
        assert!(reasons.iter().any(|r| r.code == LearnCode::StepBounded));

        // Four stops measured; the ceiling is 0.60 and half of four is not it.
        let s = samples(&[4.0; 60], 4);
        let (agg, reasons) = fold(bucket(), &s);
        assert!((agg.proposed_offset() - Learnable::Exposure.ceiling()).abs() < 1e-5);
        assert!(reasons.iter().any(|r| r.code == LearnCode::CeilingBinding));
    }

    #[test]
    fn a_dispersed_bucket_says_so_rather_than_moving_confidently() {
        let magnitudes: Vec<f32> = (0..60)
            .map(|i| if i % 2 == 0 { -0.3 } else { 0.35 })
            .collect();
        let s = samples(&magnitudes, 4);
        let (_, reasons) = fold(bucket(), &s);
        assert!(reasons.iter().any(|r| r.code == LearnCode::BucketDispersed));
    }

    #[test]
    fn a_mostly_identical_bucket_still_has_its_extremes_trimmed() {
        // The defect this module got wrong first. Sixty identical corrections and four at twenty
        // times the size have a median absolute deviation of **zero**, because more than half the
        // samples equal the median exactly - so a trim measured in MADs disables itself in exactly
        // the bucket the extremes are hiding in.
        let mut magnitudes = vec![0.20_f32; 60];
        magnitudes.extend([3.5_f32; 4]);
        assert_eq!(mad(&magnitudes, 0.20), 0.0, "the MAD really is zero here");
        assert!(scale(&magnitudes, 0.20) > 0.0, "the fallback is not");

        let s = samples(&magnitudes, 4);
        let (agg, reasons) = fold(bucket(), &s);
        assert!(agg.outliers_dropped > 0, "the extremes survived the trim");
        assert!((agg.central - 0.20).abs() < 0.01);
        assert!(reasons.iter().any(|r| r.code == LearnCode::OutlierDropped));
    }

    #[test]
    fn the_median_handles_both_parities() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&mut [4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(&mut []), None);
    }
}
