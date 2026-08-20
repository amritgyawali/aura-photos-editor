//! Resolving a profile into advice about one photograph.
//!
//! PHASE-17 section 3's last line:
//!
//! > inference: baseline (P15/P16) + style delta (bucket -> group -> global) -> recipe
//!
//! The resolution itself lives on the frozen shape - [`StyleProfile::resolve`] - because it is
//! the rule rather than an implementation of it, and a second copy of a four-level fallback is
//! two products that disagree about which bucket answered. What is here is everything around
//! it: the reasons, the confidence, the version check, and the sentence a photographer reads.
//!
//! ## Why this returns advice rather than an option
//!
//! [`advise`] always answers. A project with no profile, a bucket with no evidence and a
//! profile below the confidence floor all produce [`StyleAdvice::none`], which is a complete
//! explained answer that changes nothing.
//!
//! A `None` would have been smaller and it would have put a decision at every call site: render
//! "no style" as "a style that did nothing", or as an error, or as a missing panel. Those are
//! different sentences and only one of them is true. Invariant 2 is the other half of the same
//! argument - a decision without an explanation is a bug, and "AURA did not apply a style here"
//! is a decision.
//!
//! ## The cost, because section 11 budgets 2 ms
//!
//! Three map lookups, an addition and a clamp. There is no matrix, no dot product and no
//! allocation beyond the reason list, because `crate::tree` stores each level's **intercept**
//! rather than its slopes - the argument for which is in that module's header, and this budget
//! is one of its consequences.

use aura_core::contract::style::{
    FallbackLevel, StyleAdvice, StyleBucket, StyleCode, StyleProfile, StyleQuery, StyleReason,
    APPLY_ABOVE, USABLE_PAIRS,
};

use crate::bucket;

/// How much confidence an engine mismatch costs.
///
/// A third. The delta is still the photographer's own lean and is still worth applying; what is
/// stale is the *measured* accuracy that was reported when they adopted it. A third is enough
/// that a bucket already near the floor stops answering, which is the intended degradation, and
/// not so much that a profile becomes useless the moment the renderer moves.
pub const ENGINE_MISMATCH_PENALTY: f32 = 0.33;

/// How much confidence an under-trained profile costs.
///
/// A fifth, scaled by how far short of [`USABLE_PAIRS`] it is, so a profile at 290 pairs is
/// barely penalised and one at 40 is applied very gently. A cut-off at 300 would be the cliff
/// ADR-0035 decision 5 rejected everywhere else, and it would be worse here because it is the
/// number on the front of the panel.
pub const UNDER_TRAINED_PENALTY: f32 = 0.2;

/// What one photograph's style resolution needs from outside the profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Context<'a> {
    /// The engine this build renders with, `aura_recipe::contract::recipe::ENGINE`.
    pub engine: &'a str,
    /// The fitter version this build would produce.
    pub analysis_ver: u16,
}

/// What this photographer's look says about one photograph.
///
/// `profile` of `None` is a project with nothing selected, and it produces
/// [`StyleAdvice::none`] rather than an error.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn advise(
    profile: Option<&StyleProfile>,
    query: &StyleQuery,
    context: &Context<'_>,
) -> StyleAdvice {
    let leaf = bucket::of(query.scene, query.lighting);
    let Some(profile) = profile else {
        return StyleAdvice::none(leaf);
    };

    let (delta, level) = profile.resolve(leaf);
    let mut reasons: Vec<StyleReason> = Vec::new();
    let mut confidence = delta.confidence;

    match level {
        FallbackLevel::Bucket => {
            let samples = profile.buckets.get(&leaf).map_or(0, |model| model.samples);
            reasons.push(StyleReason::detailed(
                StyleCode::BucketApplied,
                format!(
                    "this was edited the way you edit a {} in {} light, from {samples} of your \
                     own photographs",
                    leaf.group.title().to_lowercase(),
                    leaf.lighting.title().to_lowercase()
                ),
                0.0,
            ));
            if profile
                .buckets
                .get(&leaf)
                .is_some_and(aura_core::contract::style::BucketModel::is_weak)
            {
                reasons.push(StyleReason::plain(StyleCode::BucketShrunk, -0.05));
            }
        }
        FallbackLevel::Group => {
            reasons.push(StyleReason::detailed(
                StyleCode::GroupFallback,
                format!(
                    "AURA has not seen enough of your photographs in {} light yet, so it used \
                     how you edit a {} generally",
                    leaf.lighting.title().to_lowercase(),
                    leaf.group.title().to_lowercase()
                ),
                -0.10,
            ));
        }
        FallbackLevel::Global => {
            reasons.push(StyleReason::plain(StyleCode::GlobalFallback, -0.15));
        }
        FallbackLevel::Factory => {
            // Not `StyleAdvice::none`: a profile *is* selected and it had nothing to say about
            // this kind of frame, which is a different sentence from "no profile is selected"
            // and reaches the panel differently.
            return StyleAdvice {
                profile: Some(profile.id),
                bucket: leaf,
                delta: aura_core::contract::style::StyleDelta::neutral(),
                level: FallbackLevel::Factory,
                confidence: 0.0,
                reasons: vec![StyleReason::plain(StyleCode::BelowConfidenceFloor, 0.0)],
            };
        }
    }

    // The skin bands are always held to a fraction of the rest, by `StyleDelta::clamped`, which
    // has already run. Saying so is worth a reason: it is the cheap half of the skin defence and
    // a photographer looking at why their usual warmth did not fully arrive deserves the answer
    // before they go looking for a bug.
    if !delta.hsl.is_neutral() || !delta.skin_bias.is_neutral() {
        reasons.push(StyleReason::plain(StyleCode::SkinBandsHeld, 0.0));
    }

    if profile.engine_ver != context.engine || profile.analysis_ver != context.analysis_ver {
        confidence = (confidence - ENGINE_MISMATCH_PENALTY).max(0.0);
        reasons.push(StyleReason::detailed(
            StyleCode::EngineMismatch,
            format!(
                "this profile was learned with {} and this build renders with {}, so it is \
                 applied cautiously until it is re-trained",
                profile.engine_ver, context.engine
            ),
            -ENGINE_MISMATCH_PENALTY,
        ));
    }

    if !profile.is_usable() {
        let shortfall = 1.0
            - (f32::from(u16::try_from(profile.trained_pairs.min(65_535)).unwrap_or(0))
                / USABLE_PAIRS as f32)
                .clamp(0.0, 1.0);
        let penalty = UNDER_TRAINED_PENALTY * shortfall;
        confidence = (confidence - penalty).max(0.0);
        reasons.push(StyleReason::detailed(
            StyleCode::ProfileUnderTrained,
            format!(
                "this profile has {} photographs and AURA is confident from about {USABLE_PAIRS}, \
                 so it is applied gently",
                profile.trained_pairs
            ),
            -penalty,
        ));
    }

    // The floor is re-checked *after* the penalties, which is the whole point of having them: a
    // bucket that was just above the floor and whose profile is stale stops answering rather
    // than answering less. A half-applied style is a look that belongs to nobody.
    if confidence < APPLY_ABOVE {
        return StyleAdvice {
            profile: Some(profile.id),
            bucket: leaf,
            delta: aura_core::contract::style::StyleDelta::neutral(),
            level: FallbackLevel::Factory,
            confidence,
            reasons: rank(reasons, StyleCode::BelowConfidenceFloor),
        };
    }

    StyleAdvice {
        profile: Some(profile.id),
        bucket: leaf,
        delta,
        level,
        confidence: confidence.clamp(0.0, 1.0),
        reasons: rank(reasons, StyleCode::NoProfile),
    }
}

/// Rank the reasons and cap them at [`StyleAdvice::MAX_REASONS`].
///
/// Strongest doubt first, matching every phase since 09, and `also` is appended when the list
/// would otherwise be empty - which cannot happen on any path above and is here because
/// migration 17's callers and the panel both assume a non-empty list.
fn rank(mut reasons: Vec<StyleReason>, also: StyleCode) -> Vec<StyleReason> {
    if reasons.is_empty() {
        reasons.push(StyleReason::plain(also, 0.0));
    }
    reasons.sort_by(|left, right| left.weight.total_cmp(&right.weight));
    reasons.truncate(StyleAdvice::MAX_REASONS);
    reasons
}

/// Which leaf a query lands in, without resolving anything.
///
/// Used by the training pass to bucket a pair and by the panel to highlight a cell.
#[must_use]
pub fn bucket_of(query: &StyleQuery) -> StyleBucket {
    bucket::of(query.scene, query.lighting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::style::{
        BucketModel, LightingBucket, SceneGroup, StyleDelta, StyleProfile,
    };
    use aura_core::{ProfileId, SceneId};

    const ENGINE: &str = "aura-render/1.0.0";

    fn context() -> Context<'static> {
        Context {
            engine: ENGINE,
            analysis_ver: 1,
        }
    }

    fn query(scene: SceneId, lighting: LightingBucket) -> StyleQuery {
        StyleQuery {
            scene,
            lighting,
            ..StyleQuery::default()
        }
    }

    fn profile_with(bucket: StyleBucket, exposure: f32, confidence: f32) -> StyleProfile {
        let mut profile = StyleProfile::empty(ProfileId::new(), "personal", ENGINE);
        profile.analysis_ver = 1;
        profile.trained_pairs = 600;
        profile.buckets.insert(
            bucket,
            BucketModel {
                bucket,
                delta: StyleDelta {
                    exposure,
                    confidence,
                    samples: 40,
                    ..StyleDelta::neutral()
                },
                samples: 40,
                held_out: 8,
                match_de00: Some(1.2),
                shrink: confidence,
            },
        );
        profile
    }

    #[test]
    fn no_profile_is_a_complete_explained_answer_rather_than_an_absence() {
        let advice = advise(
            None,
            &query(SceneId::Ceremony, LightingBucket::Tungsten),
            &context(),
        );
        assert!(advice.is_neutral());
        assert_eq!(advice.level, FallbackLevel::Factory);
        assert_eq!(advice.reasons.len(), 1);
        assert_eq!(
            advice.reasons.first().map(|reason| reason.code),
            Some(StyleCode::NoProfile)
        );
    }

    #[test]
    fn a_taught_bucket_answers_and_says_so() {
        let bucket = StyleBucket::new(SceneGroup::Ceremony, LightingBucket::Tungsten);
        let profile = profile_with(bucket, 0.25, 0.8);
        let advice = advise(
            Some(&profile),
            &query(SceneId::Ceremony, LightingBucket::Tungsten),
            &context(),
        );
        assert_eq!(advice.level, FallbackLevel::Bucket);
        assert!((advice.delta.exposure - 0.25).abs() < 1e-4);
        assert!(advice
            .reasons
            .iter()
            .any(|reason| reason.code == StyleCode::BucketApplied));
    }

    #[test]
    fn an_untaught_bucket_falls_back_and_the_fallback_is_a_withdrawal() {
        let bucket = StyleBucket::new(SceneGroup::Ceremony, LightingBucket::Tungsten);
        let mut profile = profile_with(bucket, 0.25, 0.8);
        profile.groups.insert(
            SceneGroup::Dance,
            StyleDelta {
                contrast: 6.0,
                confidence: 0.7,
                samples: 30,
                ..StyleDelta::neutral()
            },
        );
        let advice = advise(
            Some(&profile),
            &query(SceneId::DanceFloor, LightingBucket::Stage),
            &context(),
        );
        assert_eq!(advice.level, FallbackLevel::Group);
        let Some(reason) = advice.reasons.first() else {
            unreachable!("at least one reason")
        };
        assert!(reason.code.is_withdrawal());
    }

    #[test]
    fn a_bucket_below_the_floor_produces_the_baseline_rather_than_a_hedged_version_of_itself() {
        let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
        let profile = profile_with(bucket, 0.40, 0.20);
        let advice = advise(
            Some(&profile),
            &query(SceneId::CouplePortrait, LightingBucket::Daylight),
            &context(),
        );
        assert!(advice.is_neutral(), "a hedged style is a look nobody chose");
        assert_eq!(advice.level, FallbackLevel::Factory);
    }

    #[test]
    fn an_engine_mismatch_costs_confidence_and_says_which_engines() {
        let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
        let mut profile = profile_with(bucket, 0.30, 0.9);
        profile.engine_ver = "aura-render/0.9.0".to_string();
        let advice = advise(
            Some(&profile),
            &query(SceneId::CouplePortrait, LightingBucket::Daylight),
            &context(),
        );
        assert!(advice.confidence < 0.9);
        let Some(reason) = advice
            .reasons
            .iter()
            .find(|reason| reason.code == StyleCode::EngineMismatch)
        else {
            unreachable!("the mismatch must be explained")
        };
        assert!(reason.text.contains("0.9.0"));
    }

    #[test]
    fn an_under_trained_profile_is_applied_gently_rather_than_refused() {
        let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
        let mut profile = profile_with(bucket, 0.30, 0.9);
        profile.trained_pairs = 60;
        let advice = advise(
            Some(&profile),
            &query(SceneId::CouplePortrait, LightingBucket::Daylight),
            &context(),
        );
        assert!(!advice.is_neutral(), "it should still apply");
        assert!(advice.confidence < 0.9);
        assert!(advice
            .reasons
            .iter()
            .any(|reason| reason.code == StyleCode::ProfileUnderTrained));
    }

    #[test]
    fn a_profile_that_knows_nothing_about_this_frame_says_so_differently_from_having_no_profile() {
        let empty = StyleProfile::empty(ProfileId::new(), "personal", ENGINE);
        let advice = advise(
            Some(&empty),
            &query(SceneId::Cake, LightingBucket::Flash),
            &context(),
        );
        assert_eq!(advice.level, FallbackLevel::Factory);
        assert_eq!(advice.profile, Some(empty.id));
        assert_eq!(
            advice.reasons.first().map(|reason| reason.code),
            Some(StyleCode::BelowConfidenceFloor)
        );
    }

    #[test]
    fn the_reasons_are_capped_and_the_strongest_doubt_leads() {
        let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
        let mut profile = profile_with(bucket, 0.30, 0.95);
        profile.engine_ver = "aura-render/0.9.0".to_string();
        profile.trained_pairs = 50;
        let advice = advise(
            Some(&profile),
            &query(SceneId::CouplePortrait, LightingBucket::Daylight),
            &context(),
        );
        assert!(advice.reasons.len() <= StyleAdvice::MAX_REASONS);
        let weights: Vec<f32> = advice.reasons.iter().map(|reason| reason.weight).collect();
        for pair in weights.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            assert!(a <= b, "reasons are not ordered by doubt: {weights:?}");
        }
    }
}
