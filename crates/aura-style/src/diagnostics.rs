//! The honest report: what was learned, where it is weak, and what to add next.
//!
//! PHASE-17 section 6.3:
//!
//! > After training, re-render a held-out set of the photographer's own pairs and report dE00
//! > per bucket; **this is the number shown in the UI, not a vague 'profile ready'**.
//!
//! That sentence is the whole module. Everything here exists so that a photographer is told a
//! measurement rather than a status, and so that the measurement's own gaps are visible: a
//! bucket with no held-out pairs reports `None` and renders as "not measured yet", never as
//! zero. Zero would read as a perfect match, and the one thing a report about accuracy must not
//! do is look best where it knows least. ADR-0036 decision 2.
//!
//! ## Why the recommendation is generated here rather than in the panel
//!
//! Section 6.3 asks for a concrete sentence - "add one indoor flash reception to improve
//! dance-floor accuracy" - and the same sentence has to appear in the panel, in the CLI report
//! and in the phase exit report. Generating it once, from the actual gap, is what makes those
//! three agree. It is assembled from the bucket's own name and numbers rather than written as
//! free text, so it can be translated later without becoming three sentences again.
//!
//! ## What "weak" means, and what it does not
//!
//! A weak bucket is one at or below [`WEAK_BUCKET_SAMPLES`] pairs. It is **not** a bucket that
//! scored badly: a bucket with four hundred pairs and 3.1 dE00 is a bucket where this
//! photographer is inconsistent, which is a different fact and is reported differently. Mixing
//! the two would send a photographer to shoot more weddings to fix a problem more weddings
//! cannot fix.

use aura_core::contract::style::{
    BucketDiagnostic, BucketModel, FallbackLevel, ProfileDiagnostics, StyleBucket, StyleProfile,
    MATCH_DE00_CEILING, USABLE_PAIRS, WEAK_BUCKET_SAMPLES, WEAK_MATCH_DE00_CEILING,
};

/// How many weak buckets the recommendation names.
///
/// One. A recommendation that lists six things to shoot is a recommendation nobody acts on, and
/// the second-worst bucket usually improves on its own when the wedding that fixes the worst is
/// added - a reception is an evening of flash, tungsten and stage light in one folder.
pub const RECOMMEND_BUCKETS: usize = 1;

/// One held-out pair's measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldOut {
    /// Which leaf it belongs to.
    pub bucket: StyleBucket,
    /// How far the styled render sat from the photographer's own, in dE00.
    pub de00: f32,
    /// How far the **unstyled** baseline sat from it, in the same units.
    ///
    /// The second number is what makes the first mean anything. Section 13's criterion is that
    /// edits made with the profile are *measurably closer* to the photographer's own than the
    /// factory baseline, and a report with only the styled figure cannot say whether the profile
    /// helped, did nothing, or made things worse.
    pub baseline_de00: f32,
}

impl HeldOut {
    /// How much the profile improved on the baseline, in dE00. Negative means it made it worse.
    #[must_use]
    pub fn improvement(&self) -> f32 {
        self.baseline_de00 - self.de00
    }
}

/// Everything one training run measured.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    /// The frozen diagnostics that go on the profile.
    pub diagnostics: ProfileDiagnostics,
    /// The mean dE00 of the *unstyled* baseline over the same held-out pairs.
    pub baseline_de00: f32,
    /// How many buckets improved on the baseline, and how many did not.
    pub improved_buckets: u32,
    /// Buckets where the profile was measurably worse than doing nothing.
    ///
    /// **Reported rather than hidden.** A bucket where a learned style is worse than the factory
    /// baseline is a bucket whose pairs disagree with each other, and a photographer is entitled
    /// to know which one before they adopt.
    pub regressed_buckets: Vec<StyleBucket>,
}

impl Report {
    /// True when the profile beat the baseline overall.
    #[must_use]
    pub fn beat_baseline(&self) -> bool {
        self.diagnostics.overall_de00 < self.baseline_de00
    }

    /// How much better than the baseline, in dE00.
    #[must_use]
    pub fn improvement(&self) -> f32 {
        self.baseline_de00 - self.diagnostics.overall_de00
    }
}

/// Measure a trained profile against the pairs it was not fitted on.
///
/// `measurements` is one entry per held-out pair, already rendered and compared by the caller -
/// this module does no I/O and opens nothing, so it can be tested against measurements whose
/// answer is known by construction.
#[must_use]
pub fn evaluate(
    profile: &StyleProfile,
    measurements: &[HeldOut],
    accepted: u32,
    rejected: u32,
) -> Report {
    let mut per_bucket: Vec<BucketDiagnostic> = Vec::new();
    let mut regressed = Vec::new();
    let mut improved = 0_u32;
    let mut total = 0.0_f64;
    let mut baseline_total = 0.0_f64;
    let mut count = 0_u32;

    for bucket in ordered_buckets(profile) {
        let mine: Vec<&HeldOut> = measurements
            .iter()
            .filter(|entry| entry.bucket == bucket)
            .collect();
        let samples = profile
            .buckets
            .get(&bucket)
            .map_or(0, |model| model.samples);
        let (_, level) = profile.resolve(bucket);
        let measured = if mine.is_empty() {
            None
        } else {
            let sum: f32 = mine.iter().map(|entry| entry.de00).sum();
            let base: f32 = mine.iter().map(|entry| entry.baseline_de00).sum();
            let n = mine.len() as f32;
            if base / n > sum / n {
                improved = improved.saturating_add(1);
            } else {
                regressed.push(bucket);
            }
            Some(sum / n)
        };
        per_bucket.push(BucketDiagnostic {
            bucket,
            samples,
            match_de00: measured,
            level,
        });
    }

    for entry in measurements {
        total += f64::from(entry.de00);
        baseline_total += f64::from(entry.baseline_de00);
        count += 1;
    }
    let overall = if count == 0 {
        0.0
    } else {
        (total / f64::from(count)) as f32
    };
    let baseline = if count == 0 {
        0.0
    } else {
        (baseline_total / f64::from(count)) as f32
    };

    let weak = weak_buckets(profile);
    let recommendation = recommend(profile, &weak, accepted, overall);

    Report {
        diagnostics: ProfileDiagnostics {
            per_bucket,
            weak_buckets: weak,
            overall_de00: overall,
            accepted_pairs: accepted,
            rejected_pairs: rejected,
            recommendation,
        },
        baseline_de00: baseline,
        improved_buckets: improved,
        regressed_buckets: regressed,
    }
}

/// The buckets that need more weddings, worst first.
///
/// Ordered by how few pairs they have and then by name, so the order is deterministic and the
/// same profile always recommends the same thing.
#[must_use]
pub fn weak_buckets(profile: &StyleProfile) -> Vec<StyleBucket> {
    let mut weak: Vec<(&BucketModel, StyleBucket)> = profile
        .buckets
        .values()
        .filter(|model| model.samples > 0 && model.samples <= WEAK_BUCKET_SAMPLES)
        .map(|model| (model, model.bucket))
        .collect();
    weak.sort_by(|left, right| {
        left.0
            .samples
            .cmp(&right.0.samples)
            .then_with(|| left.1.as_key().cmp(&right.1.as_key()))
    });
    weak.into_iter().map(|(_, bucket)| bucket).collect()
}

/// The sentence a photographer reads.
///
/// Four cases, in the order they matter. Under-trained first, because a profile with sixty pairs
/// has one problem and it is not which bucket is weak; then a weak bucket; then an inaccurate
/// profile; then the case where there is nothing to say, which gets a sentence too - a blank
/// recommendation reads as a bug.
#[must_use]
pub fn recommend(
    profile: &StyleProfile,
    weak: &[StyleBucket],
    accepted: u32,
    overall_de00: f32,
) -> String {
    if accepted < USABLE_PAIRS {
        let short = USABLE_PAIRS.saturating_sub(accepted);
        return format!(
            "add about {short} more edited photographs - AURA is confident from around \
             {USABLE_PAIRS} and this profile has {accepted}"
        );
    }
    if let Some(bucket) = weak.iter().take(RECOMMEND_BUCKETS).next() {
        let samples = profile.buckets.get(bucket).map_or(0, |model| model.samples);
        return format!(
            "add one wedding with {} in {} light - this profile has only {samples} of them, so \
             those frames are edited from your overall look rather than from how you edit them",
            bucket.group.title().to_lowercase(),
            bucket.lighting.title().to_lowercase()
        );
    }
    if overall_de00 > MATCH_DE00_CEILING {
        return format!(
            "your edits vary more than AURA can predict yet ({overall_de00:.1} against a target \
             of {MATCH_DE00_CEILING:.1}) - adding another wedding of the same kind usually \
             closes it"
        );
    }
    format!(
        "this profile matches your own edits to {overall_de00:.1} across every kind of \
         photograph AURA has seen from you - nothing needs adding"
    )
}

/// Which ceiling one bucket is held to.
///
/// A weak bucket is judged against [`WEAK_MATCH_DE00_CEILING`] and a populated one against
/// [`MATCH_DE00_CEILING`]. Section 10.1 sets both and the difference is deliberate: a bucket
/// answering out of its parent group is *allowed* to be worse, and it is not allowed to be
/// arbitrary.
#[must_use]
pub fn ceiling_for(samples: u32) -> f32 {
    if samples <= WEAK_BUCKET_SAMPLES {
        WEAK_MATCH_DE00_CEILING
    } else {
        MATCH_DE00_CEILING
    }
}

/// Every bucket that has a model or a measurement, in matrix order.
fn ordered_buckets(profile: &StyleProfile) -> Vec<StyleBucket> {
    let mut out: Vec<StyleBucket> = StyleBucket::all()
        .into_iter()
        .filter(|bucket| profile.buckets.contains_key(bucket))
        .collect();
    out.dedup();
    out
}

/// How many pairs each level of the tree answered for, for the outline's `level_counts`.
#[must_use]
pub fn level_counts(profile: &StyleProfile, buckets: &[StyleBucket]) -> Vec<(FallbackLevel, u32)> {
    let mut counts = [(FallbackLevel::Bucket, 0_u32); 4];
    for (index, level) in FallbackLevel::ALL.into_iter().enumerate() {
        if let Some(slot) = counts.get_mut(index) {
            *slot = (level, 0);
        }
    }
    for bucket in buckets {
        let (_, level) = profile.resolve(*bucket);
        for slot in &mut counts {
            if slot.0 == level {
                slot.1 = slot.1.saturating_add(1);
            }
        }
    }
    counts.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::style::{BucketModel, LightingBucket, SceneGroup, StyleDelta};
    use aura_core::ProfileId;

    fn profile_with(samples: u32) -> (StyleProfile, StyleBucket) {
        let mut profile = StyleProfile::empty(ProfileId::new(), "personal", "aura-render/1.0.0");
        profile.trained_pairs = 400;
        let bucket = StyleBucket::new(SceneGroup::Dance, LightingBucket::Flash);
        profile.buckets.insert(
            bucket,
            BucketModel {
                bucket,
                delta: StyleDelta {
                    contrast: 4.0,
                    confidence: 0.6,
                    samples,
                    ..StyleDelta::neutral()
                },
                samples,
                held_out: 0,
                match_de00: None,
                shrink: 0.6,
            },
        );
        (profile, bucket)
    }

    #[test]
    fn a_bucket_with_no_held_out_pairs_reports_none_rather_than_zero() {
        let (profile, _) = profile_with(40);
        let report = evaluate(&profile, &[], 400, 20);
        let Some(row) = report.diagnostics.per_bucket.first() else {
            unreachable!("one bucket")
        };
        assert_eq!(
            row.match_de00, None,
            "an unmeasured bucket must not be zero"
        );
    }

    #[test]
    fn the_report_says_whether_the_profile_beat_doing_nothing() {
        let (profile, bucket) = profile_with(40);
        let measurements = vec![
            HeldOut {
                bucket,
                de00: 1.2,
                baseline_de00: 3.4,
            },
            HeldOut {
                bucket,
                de00: 1.4,
                baseline_de00: 3.0,
            },
        ];
        let report = evaluate(&profile, &measurements, 400, 10);
        assert!(report.beat_baseline());
        assert!(report.improvement() > 1.0);
        assert_eq!(report.improved_buckets, 1);
        assert!(report.regressed_buckets.is_empty());
    }

    #[test]
    fn a_bucket_the_profile_made_worse_is_named_rather_than_hidden() {
        let (profile, bucket) = profile_with(40);
        let measurements = vec![HeldOut {
            bucket,
            de00: 4.0,
            baseline_de00: 2.0,
        }];
        let report = evaluate(&profile, &measurements, 400, 0);
        assert_eq!(report.regressed_buckets, vec![bucket]);
        assert!(!report.beat_baseline());
    }

    #[test]
    fn a_sparse_bucket_is_weak_and_a_populated_inaccurate_one_is_not() {
        let (sparse, bucket) = profile_with(6);
        assert_eq!(weak_buckets(&sparse), vec![bucket]);

        let (populated, _) = profile_with(400);
        assert!(
            weak_buckets(&populated).is_empty(),
            "an inaccurate bucket is not a weak one and more weddings will not fix it"
        );
    }

    #[test]
    fn an_under_trained_profile_is_told_to_add_photographs_before_anything_else() {
        let (profile, _) = profile_with(6);
        let sentence = recommend(&profile, &weak_buckets(&profile), 90, 1.0);
        assert!(sentence.contains("210"), "{sentence}");
    }

    #[test]
    fn a_weak_bucket_recommendation_names_the_scene_and_the_light() {
        let (profile, _) = profile_with(6);
        let sentence = recommend(&profile, &weak_buckets(&profile), 400, 1.0);
        assert!(sentence.contains("dance"), "{sentence}");
        assert!(sentence.contains("flash"), "{sentence}");
    }

    #[test]
    fn a_good_profile_still_gets_a_sentence() {
        let (profile, _) = profile_with(400);
        let sentence = recommend(&profile, &[], 900, 1.4);
        assert!(!sentence.is_empty());
        assert!(sentence.contains("1.4"), "{sentence}");
    }

    #[test]
    fn an_inaccurate_but_well_populated_profile_says_so() {
        let (profile, _) = profile_with(400);
        let sentence = recommend(&profile, &[], 900, 4.2);
        assert!(sentence.contains("vary more"), "{sentence}");
    }

    #[test]
    fn the_two_ceilings_apply_to_the_two_kinds_of_bucket() {
        assert!((ceiling_for(4) - WEAK_MATCH_DE00_CEILING).abs() < f32::EPSILON);
        assert!((ceiling_for(400) - MATCH_DE00_CEILING).abs() < f32::EPSILON);
    }

    #[test]
    fn the_acceptance_fraction_is_reported_and_survives_an_empty_run() {
        let (profile, _) = profile_with(40);
        let report = evaluate(&profile, &[], 900, 100);
        assert!((report.diagnostics.acceptance() - 0.9).abs() < 1e-3);
        assert!((ProfileDiagnostics::default().acceptance() - 0.0).abs() < f32::EPSILON);
    }
}
