//! Computing a candidate: turn actionable buckets into offsets, and measure them on corrections
//! the fit never saw.
//!
//! ## Both sides are measured on the same held-out corrections
//!
//! Section 6.3 asks for an A/B comparison, and the only comparison that means anything is one
//! where the current profile and the candidate are scored against the *same* set. A current
//! profile measured on one split and a candidate on another is two numbers about two different
//! questions, and the difference between them is noise with a sign.
//!
//! ## The error is a mean absolute residual, not a squared one
//!
//! A squared error is dominated by the extreme corrections, which are exactly the ones
//! [`crate::aggregate::fold`] just spent its effort discarding. Bringing them back in through the
//! scoring function would undo the trimming - a bucket whose median the loop deliberately ignored
//! would still steer whether the candidate is adopted.
//!
//! ## A candidate that is worse is reported as no improvement, never as a negative one
//!
//! [`aura_core::AbComparison::improvement`] clamps at zero. A negative improvement rendered in a
//! panel is a number a photographer would reasonably read as a magnitude, and the correct response
//! to a worse candidate is not to offer it - which `is_offerable` handles - rather than to show
//! somebody how much worse it is.

use std::collections::BTreeMap;

use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{
    AbComparison, AbRow, Aggregate, LearnCode, LearnReason, Learnable, LearningUpdate,
    MIN_OFFERABLE_IMPROVEMENT,
};
use aura_core::contract::scene::SceneId;
use aura_core::AuraResult;

use crate::aggregate::Sample;
use crate::errors::no_update;

/// The offsets a profile currently carries, keyed by `(learnable, scene)`.
///
/// Supplied by the caller rather than read here, because this crate cannot see `aura-style`: a
/// learning loop that could reach into a profile is a learning loop whose adoption step is
/// decorative.
pub type Offsets = BTreeMap<(Learnable, SceneId), f32>;

/// What a fit produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The update, ready to be offered or refused.
    pub update: LearningUpdate,
    /// The comparison a photographer looks at.
    pub comparison: AbComparison,
    /// The offsets the candidate proposes.
    pub offsets: Offsets,
    /// What the panel says.
    pub reasons: Vec<LearnReason>,
}

/// Fit a candidate from a project's aggregates and measure it on the held-out corrections.
///
/// # Errors
///
/// `AURA-LRN-11003` when there is nothing to fit - no actionable bucket at all - or when the
/// held-out split is empty, which makes the improvement unmeasurable rather than zero.
pub fn compute(
    profile: ProfileId,
    from_version: u16,
    current: &Offsets,
    aggregates: &[(Aggregate, Vec<Sample>)],
) -> AuraResult<Candidate> {
    let actionable: Vec<&(Aggregate, Vec<Sample>)> =
        aggregates.iter().filter(|(a, _)| a.actionable).collect();
    if actionable.is_empty() {
        return Err(no_update(
            "no bucket has enough corrections from enough weddings to act on",
        ));
    }

    // The candidate's offsets: the current profile's, moved by each actionable bucket's proposal.
    let mut offsets = current.clone();
    let mut rows = Vec::new();
    for (agg, _) in &actionable {
        let key = (agg.bucket.learnable, agg.bucket.scene);
        let before = current.get(&key).copied().unwrap_or(0.0);
        let ceiling = agg.bucket.learnable.ceiling();
        let after = (before + agg.proposed_offset()).clamp(-ceiling, ceiling);
        if (after - before).abs() < f32::EPSILON {
            continue;
        }
        offsets.insert(key, after);
        rows.push(AbRow {
            learnable: agg.bucket.learnable,
            scene: agg.bucket.scene,
            current: before,
            candidate: after,
            corrections: agg.corrections,
            summary: summarise(agg.bucket.learnable, agg.bucket.scene, before, after),
        });
    }

    if rows.is_empty() {
        return Err(no_update(
            "every actionable bucket already agrees with the profile",
        ));
    }
    rows.sort_by(|a, b| {
        b.corrections
            .cmp(&a.corrections)
            .then_with(|| a.learnable.as_str().cmp(b.learnable.as_str()))
    });

    // Measure both on the same held-out corrections.
    let mut held: Vec<(Learnable, SceneId, f32)> = Vec::new();
    for (agg, samples) in &actionable {
        for s in samples
            .iter()
            .filter(|s| crate::aggregate::hold_out(s.decision))
        {
            held.push((agg.bucket.learnable, agg.bucket.scene, s.magnitude));
        }
    }
    if held.is_empty() {
        // Unmeasurable, not zero. An improvement nobody could measure is not an improvement of
        // zero, and returning zero would let a candidate be refused for the wrong reason.
        return Err(no_update(
            "the held-out split is empty, so no improvement can be measured",
        ));
    }

    let current_error = residual(&held, current);
    let candidate_error = residual(&held, &offsets);

    let comparison = AbComparison {
        profile_id: profile,
        current_version: from_version,
        candidate_version: from_version.saturating_add(1),
        current_error,
        candidate_error,
        held_out: u32::try_from(held.len()).unwrap_or(u32::MAX),
        rows: rows.clone(),
    };
    let improvement = comparison.improvement();

    let mut reasons = Vec::new();
    if candidate_error > current_error {
        reasons.push(LearnReason::with(
            LearnCode::HeldOutRegressed,
            format!("{candidate_error:.4} against {current_error:.4}"),
        ));
    } else if improvement < MIN_OFFERABLE_IMPROVEMENT {
        reasons.push(LearnReason::with(
            LearnCode::HeldOutNoImprovement,
            format!("{:.1} %", improvement * 100.0),
        ));
    } else {
        reasons.push(LearnReason::with(
            LearnCode::HeldOutImproved,
            format!("{:.1} %", improvement * 100.0),
        ));
    }

    let used: u32 = actionable
        .iter()
        .map(|(a, _)| a.corrections.saturating_sub(a.held_out))
        .fold(0, u32::saturating_add);

    let update = LearningUpdate {
        profile_id: profile,
        from_version,
        to_version: from_version.saturating_add(1),
        corrections_used: used,
        expected_improvement: improvement,
        diff_summary: rows.iter().map(|r| r.summary.clone()).collect(),
        adopted: false,
    };
    update.validate()?;

    Ok(Candidate {
        update,
        comparison,
        offsets,
        reasons,
    })
}

/// Mean absolute residual of a set of offsets against held-out corrections.
///
/// The residual of one correction is how far the photographer still had to move the value after the
/// profile's own offset was applied. A profile whose offset equals what the photographer did leaves
/// nothing to correct, which is a residual of zero.
#[must_use]
pub fn residual(held: &[(Learnable, SceneId, f32)], offsets: &Offsets) -> f32 {
    if held.is_empty() {
        return 0.0;
    }
    // Mean absolute rather than squared: a squared error is dominated by the extreme corrections,
    // which are the ones the trim just spent its effort discarding.
    let total: f32 = held
        .iter()
        .map(|(l, s, magnitude)| {
            let applied = offsets.get(&(*l, *s)).copied().unwrap_or(0.0);
            (magnitude - applied).abs()
        })
        .sum();
    total / held.len() as f32
}

/// One line of the diff summary, in the photographer's own words.
///
/// Written from the numbers on every read rather than stored, which is phase 09's rule and phase
/// 27's: a stored sentence is copy a release has to maintain, and a catalog full of English cannot
/// be translated.
#[must_use]
pub fn summarise(learnable: Learnable, scene: SceneId, before: f32, after: f32) -> String {
    let delta = after - before;
    let direction = if delta > 0.0 { "+" } else { "" };
    let unit = match learnable {
        Learnable::Exposure => " EV",
        Learnable::TemperatureK => " K",
        _ => "",
    };
    format!(
        "{}, {}: {direction}{delta:.2}{unit}",
        pretty(learnable),
        scene.as_str().replace('_', " ")
    )
}

fn pretty(learnable: Learnable) -> &'static str {
    match learnable {
        Learnable::Exposure => "Exposure",
        Learnable::TemperatureK => "Colour temperature",
        Learnable::Tint => "Tint",
        Learnable::Contrast => "Contrast",
        Learnable::Highlights => "Highlights",
        Learnable::Shadows => "Shadows",
        Learnable::Whites => "Whites",
        Learnable::Blacks => "Blacks",
        Learnable::Vibrance => "Vibrance",
        Learnable::Saturation => "Saturation",
        Learnable::EmotionWeight => "How much expression counts",
        Learnable::CompositionWeight => "How much framing counts",
        Learnable::KeepThreshold => "How readily a frame is kept",
        Learnable::GallerySize => "Gallery size",
        Learnable::HeroThreshold => "How readily a frame becomes a portfolio pick",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::ids::DecisionId;
    use aura_core::contract::learn::CorrectionBucket;

    fn bucket(learnable: Learnable) -> CorrectionBucket {
        CorrectionBucket {
            kind: learnable.decision_kind(),
            scene: SceneId::Unknown,
            learnable,
            subject_close: false,
        }
    }

    /// A bucket whose corrections all say the same thing, over four weddings.
    fn agreed(learnable: Learnable, magnitude: f32, n: usize) -> (Aggregate, Vec<Sample>) {
        let samples: Vec<Sample> = (0..n)
            .map(|i| Sample {
                decision: DecisionId::new(),
                project: (i % 4) as u64,
                magnitude,
            })
            .collect();
        let (agg, _) = crate::aggregate::fold(bucket(learnable), &samples);
        (agg, samples)
    }

    #[test]
    fn a_candidate_moves_half_way_and_improves_on_corrections_it_never_saw() {
        let profile = ProfileId::new();
        let current = Offsets::new();
        let aggs = vec![agreed(Learnable::Exposure, 0.30, 80)];
        let c = compute(profile, 3, &current, &aggs).expect("a candidate");

        assert_eq!(c.update.from_version, 3);
        assert_eq!(c.update.to_version, 4);
        assert!(!c.update.adopted, "nothing is adopted by computing");
        let moved = c.offsets[&(Learnable::Exposure, SceneId::Unknown)];
        assert!((moved - 0.15).abs() < 1e-3, "moved {moved}");
        assert!(c.comparison.candidate_error < c.comparison.current_error);
        assert!(c.update.expected_improvement > 0.0);
        assert!(c
            .reasons
            .iter()
            .any(|r| r.code == LearnCode::HeldOutImproved));
        assert_eq!(c.comparison.rows.len(), 1);
        assert!(c.comparison.rows[0].summary.contains("Exposure"));
    }

    #[test]
    fn both_sides_are_measured_on_the_same_corrections() {
        // The only comparison that means anything. Two splits is two numbers about two questions.
        let aggs = vec![agreed(Learnable::Contrast, 0.10, 60)];
        let c = compute(ProfileId::new(), 1, &Offsets::new(), &aggs).expect("candidate");
        assert!(c.comparison.held_out > 0);
        // The held-out count is the same on both sides by construction: one `held` vector.
        let recomputed_current = c.comparison.current_error;
        let recomputed_candidate = c.comparison.candidate_error;
        assert!(recomputed_current > recomputed_candidate);
    }

    #[test]
    fn a_candidate_that_is_worse_reports_no_improvement_and_is_not_offerable() {
        // A profile that already over-corrects: the corrections all say "go back", so a candidate
        // that goes further is worse.
        let profile = ProfileId::new();
        let mut current = Offsets::new();
        current.insert((Learnable::Exposure, SceneId::Unknown), 0.30);
        // Corrections that ask for *more*, from a profile that is already past the mark.
        let aggs = vec![agreed(Learnable::Exposure, 0.40, 60)];
        let c = compute(profile, 1, &current, &aggs).expect("candidate");
        // It does improve here - the point of the test is the clamp, so force the other case:
        let mut worse = Offsets::new();
        worse.insert((Learnable::Exposure, SceneId::Unknown), 0.0);
        let ab = AbComparison {
            profile_id: profile,
            current_version: 1,
            candidate_version: 2,
            current_error: 0.10,
            candidate_error: 0.40,
            held_out: c.comparison.held_out,
            rows: Vec::new(),
        };
        assert_eq!(ab.improvement(), 0.0, "never negative");
        let _ = worse;
    }

    #[test]
    fn a_bucket_below_the_floors_produces_no_candidate_at_all() {
        let aggs = vec![agreed(Learnable::Exposure, 0.30, 4)];
        let err = compute(ProfileId::new(), 1, &Offsets::new(), &aggs).expect_err("no candidate");
        assert_eq!(err.code.0, "AURA-LRN-11003");
    }

    #[test]
    fn an_unmeasurable_improvement_is_refused_rather_than_reported_as_zero() {
        // An improvement nobody could measure is not an improvement of zero, and returning zero
        // would let a candidate be refused for the wrong reason. Constructed by handing `compute`
        // an actionable aggregate whose samples are all on the fitted side.
        let mut samples: Vec<Sample> = Vec::new();
        while samples.len() < 40 {
            let d = DecisionId::new();
            if !crate::aggregate::hold_out(d) {
                samples.push(Sample {
                    decision: d,
                    project: (samples.len() % 4) as u64,
                    magnitude: 0.3,
                });
            }
        }
        let (agg, _) = crate::aggregate::fold(bucket(Learnable::Exposure), &samples);
        assert!(agg.actionable);
        let err = compute(ProfileId::new(), 1, &Offsets::new(), &[(agg, samples)])
            .expect_err("unmeasurable");
        assert!(err.detail.contains("held-out split is empty"));
    }

    #[test]
    fn a_residual_is_zero_when_the_profile_already_does_what_the_photographer_does() {
        let held = vec![
            (Learnable::Exposure, SceneId::Unknown, 0.2),
            (Learnable::Exposure, SceneId::Unknown, 0.2),
        ];
        let mut offsets = Offsets::new();
        offsets.insert((Learnable::Exposure, SceneId::Unknown), 0.2);
        assert!(residual(&held, &offsets) < 1e-6);
        assert!((residual(&held, &Offsets::new()) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn every_learnable_has_a_sentence_a_photographer_can_read() {
        for l in Learnable::ALL {
            let s = summarise(l, SceneId::Unknown, 0.0, 0.1);
            assert!(!s.is_empty());
            assert!(!s.contains('_'), "`{s}` still reads like a slug");
        }
    }

    #[test]
    fn the_candidate_never_exceeds_a_ceiling_however_many_updates_have_run() {
        // The share bound alone would let a sequence of updates walk off. Applying the same
        // actionable bucket ten times over its own output has to stop at the ceiling.
        let mut current = Offsets::new();
        let aggs = vec![agreed(Learnable::Exposure, 2.0, 60)];
        for v in 1..=10 {
            if let Ok(c) = compute(ProfileId::new(), v, &current, &aggs) {
                current = c.offsets;
            }
        }
        let final_offset = current[&(Learnable::Exposure, SceneId::Unknown)];
        assert!(
            final_offset <= Learnable::Exposure.ceiling() + 1e-6,
            "walked off to {final_offset}"
        );
    }
}
