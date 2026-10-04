//! A look, written into phase 17's frozen profile shape.
//!
//! ## Why a look becomes a `StyleProfile` rather than staying its own thing
//!
//! Phase 17's rule: **`StyleService` is the only way to ask what a photographer's own look is**,
//! and every consumer - phases 15 and 16 applying it, 25 normalising a gallery that carries it,
//! 26 matching a second camera through it, 27 explaining it, 28 running it unattended - reads a
//! `StyleProfile`. A look that kept its own shape would need every one of those phases to learn
//! a second one, and the first time two of them disagreed about which to read, a gallery would
//! stop matching its album.
//!
//! So a [`LookProfile`] is the *evidence* and a `StyleProfile` is what the rest of the product
//! consumes. [`LookService`](aura_core::contract::look::LookService) still exists and is still
//! the only way to ask what a reference look is, because the two questions are different: "what
//! does that page do" is answerable from a folder of JPEGs, and "what do *you* do" needs an
//! archive of pairs. Collapsing them would report a look measured off twenty-four JPEGs with the
//! confidence of a profile fitted from three hundred matched pairs.
//!
//! ## The replication, and the reason it is honest
//!
//! A look has one axis: light. A `StyleProfile` has two: scene group and light. So the same
//! lighting-conditioned delta is written into **every** scene group, and
//! [`LookCode::SceneAxisNotLearned`] is on the profile's diagnostics and in the panel.
//!
//! The alternative - writing only the global lean and leaving every leaf empty - is worse and
//! not more honest. Phase 17's resolution walks bucket, then group, then global, so an empty
//! leaf means a portrait made in candlelight would resolve past the candlelight answer to the
//! page's overall lean, and the one axis that *was* measured would be thrown away at the moment
//! it applied. Replicating says "this is what the page does in this light, whatever the
//! photograph is of", which is exactly what was measured.

use std::collections::BTreeMap;

use aura_core::contract::look::{LookCode, LookMatchReport, LookProfile, APPLY_ABOVE};
use aura_core::contract::style::{
    BucketDiagnostic, BucketModel, FallbackLevel, LightingBucket, ProfileDiagnostics,
    ProfileStatus, SceneGroup, StyleBucket, StyleDelta, StyleProfile,
};

/// Phase 17's profile, filled from a look that has been measured.
///
/// The `id`, `name` and `engine_ver` carry across unchanged, so a photographer looking at the
/// style panel and at the look panel sees one thing with one name in both.
///
/// **The measured report is required, not optional**, and that is this function's one rule.
/// `ProfileDiagnostics::overall_de00` is an `f32` rather than an `Option<f32>` - phase 17 could
/// always measure it, so it never needed to express "not measured" - and a look materialised
/// before [`crate::verify::measure`] has run would put a `0.0` in the field every panel in the
/// product renders as a perfect match. Phase 22's rule, in the place it would have been easiest
/// to miss: a result that cannot be measured is not produced.
#[must_use]
pub fn into_style(look: &LookProfile, matched: &LookMatchReport) -> StyleProfile {
    let mut profile = StyleProfile::empty(look.id, look.name.clone(), look.engine_ver.clone());
    profile.status = ProfileStatus::Candidate;
    profile.global = look.global.clamped();
    profile.trained_pairs = look.references;
    profile.trained_at = look.measured_at;
    profile.analysis_ver = look.analysis_ver;

    // Every group gets the global lean, because that is what "applies to every photograph before
    // any conditioning" means and phase 17's resolution adds a group's delta on top of it. A
    // group left absent would resolve to the global anyway; writing it makes the matrix in the
    // panel show what was learned rather than a column of blanks.
    let mut groups: BTreeMap<SceneGroup, StyleDelta> = BTreeMap::new();
    for group in SceneGroup::ALL {
        groups.insert(group, StyleDelta::neutral());
    }
    profile.groups = groups;

    let mut buckets: BTreeMap<StyleBucket, BucketModel> = BTreeMap::new();
    for (lighting, bucket) in &look.buckets {
        if bucket.confidence < APPLY_ABOVE {
            continue;
        }
        for group in SceneGroup::ALL {
            buckets.insert(
                StyleBucket::new(group, *lighting),
                BucketModel {
                    bucket: StyleBucket::new(group, *lighting),
                    delta: bucket.delta.clamped(),
                    samples: bucket.reference.samples,
                    // Nothing was held out, because there is nothing to hold out: a look has no
                    // pairs and no second version of any photograph. `None` rather than zero,
                    // for the reason phase 17's own comment gives about this exact field.
                    held_out: 0,
                    match_de00: residual_for(matched, *lighting),
                    // How far this bucket's own answer survived the pull toward the global
                    // lean, which is what `solve::shrink` returned.
                    shrink: shrink_of(bucket.reference.samples),
                },
            );
        }
    }
    profile.buckets = buckets;

    profile.diagnostics = diagnostics_of(look, matched);
    profile
}

/// Phase 17's diagnostics, filled from a look's own and from what was measured.
///
/// Three fields carry across honestly and three are filled with what this phase can actually
/// say. `overall_de00` is the **measured** frame-weighted distance from
/// [`crate::verify::measure`] - not a parameter distance, not a held-out pair figure, and not
/// zero - because a reader comparing it against phase 17's 2.5 dE00 ceiling deserves a number
/// that came from the same kind of instrument even though it answers a different question.
///
/// `accepted_pairs` is the reference count and `rejected_pairs` is what the walk refused, which
/// is the nearest true statement: a reference photograph is what this phase learns from, and a
/// file it could not read is what it did not.
fn diagnostics_of(look: &LookProfile, matched: &LookMatchReport) -> ProfileDiagnostics {
    let per_bucket: Vec<BucketDiagnostic> = look
        .buckets
        .iter()
        .filter(|(_, bucket)| bucket.confidence >= APPLY_ABOVE)
        .map(|(lighting, bucket)| BucketDiagnostic {
            // The matrix has eight rows and a look fills all of them identically, so the
            // diagnostic names the group a photographer is most likely to be looking at when
            // they open it. `Other` would be accurate and would render as a blank row.
            bucket: StyleBucket::new(SceneGroup::Portraits, *lighting),
            samples: bucket.reference.samples,
            match_de00: residual_for(matched, *lighting),
            level: FallbackLevel::Bucket,
        })
        .collect();

    let weak_buckets: Vec<StyleBucket> = look
        .buckets
        .iter()
        .filter(|(_, bucket)| bucket.reference.is_weak())
        .map(|(lighting, _)| StyleBucket::new(SceneGroup::Portraits, *lighting))
        .collect();

    ProfileDiagnostics {
        per_bucket,
        weak_buckets,
        overall_de00: matched.after_de00,
        accepted_pairs: look.diagnostics.measured,
        rejected_pairs: look.diagnostics.refused,
        recommendation: look.diagnostics.summary.clone(),
    }
}

/// What the measured report says about one lighting bucket, when it says anything.
fn residual_for(matched: &LookMatchReport, lighting: LightingBucket) -> Option<f32> {
    matched
        .buckets
        .iter()
        .find(|row| row.lighting == lighting)
        .map(|row| row.after_de00)
}

/// The shrinkage a bucket with this many reference photographs kept, `0..1`.
///
/// The same `n / (n + k)` [`crate::solve::shrink`] applied, recomputed here rather than threaded
/// through the profile: it is a function of the sample count alone, and storing it would be a
/// second copy of a number that can disagree with the first.
fn shrink_of(samples: u32) -> f32 {
    samples as f32 / (samples as f32 + crate::solve::PRIOR_STRENGTH)
}

/// Which lighting buckets a look actually answers for, in the order the matrix renders them.
#[must_use]
pub fn answered(look: &LookProfile) -> Vec<LightingBucket> {
    LightingBucket::ALL
        .into_iter()
        .filter(|lighting| {
            look.buckets
                .get(lighting)
                .is_some_and(|bucket| bucket.confidence >= APPLY_ABOVE)
        })
        .collect()
}

/// The one reason every materialised look carries, whatever else is true of it.
#[must_use]
pub const fn scene_axis_reason() -> LookCode {
    LookCode::SceneAxisNotLearned
}
