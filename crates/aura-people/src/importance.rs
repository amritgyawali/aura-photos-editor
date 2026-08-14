//! How much each person matters, and who decided.
//!
//! `identities.importance` is a single number in `0..1` that every downstream weight
//! multiplies, and it has two possible authors. The model here proposes one; the
//! photographer's slider overrides it. The distinction is not cosmetic - it is the
//! difference between a product that learns and a product that argues.
//!
//! ## The default is 0.5, and that is load-bearing
//!
//! `aura_vision::face::prominence` multiplies the role weight by `2 * importance`, so
//! an untouched slider at 0.5 is exactly a no-op and the role table alone decides.
//! That means a photographer who never touches the panel gets the product's opinion
//! unmodified, and one who drags a slider gets theirs. There is no third behaviour
//! where the two silently compound.
//!
//! ## What the model actually proposes
//!
//! Four pieces of evidence, none of which is a judgement about a person:
//!
//! * **Role.** The dominant term, from the versioned weight table.
//! * **Coverage share.** How much of the wedding this identity appears in, relative to
//!   the most-photographed identity. A person the photographer kept returning to is a
//!   person who mattered on the day.
//! * **Subject share.** How often this identity was the *dominant* subject of a frame
//!   rather than merely present. This is what separates a guest of honour from a guest
//!   who stood near the couple all evening.
//! * **Span.** How much of the day they were present for. A supplier is present all
//!   day and dominant never; a grandparent is present for two hours and dominant often.
//!   Span alone would rank them identically, which is why it is the smallest term.
//!
//! ## Why this is not a learned model
//!
//! It could be, and phase 17 is where personal style learning belongs. Learning it now
//! would need labelled importance from real photographers, which does not exist in this
//! repository - and a learned weight that nobody can explain would break invariant 2 on
//! the one surface where a photographer is most likely to disagree with the product.
//! Every number below produces a reason string.

use std::collections::BTreeMap;

use aura_core::contract::people::Role;
use aura_core::IdentityId;
use aura_vision::face::prominence::ProminenceWeights;

use crate::store::IdentityRow;
use crate::timeline::IdentityTimeline;

/// The importance model's generation, recorded with every proposal.
pub const IMPORTANCE_VER: u16 = 1;

/// The neutral importance. An untouched slider.
pub const NEUTRAL: f32 = 0.5;

/// Weight of the role term.
const ROLE_WEIGHT: f32 = 0.55;
/// Weight of the coverage share.
const COVERAGE_WEIGHT: f32 = 0.20;
/// Weight of the subject share.
const SUBJECT_WEIGHT: f32 = 0.18;
/// Weight of the span share.
const SPAN_WEIGHT: f32 = 0.07;

/// What the model thinks one identity's importance should be.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportanceProposal {
    /// Whose.
    pub identity: IdentityId,
    /// The proposed importance, `0..1`.
    pub importance: f32,
    /// The value currently stored.
    pub current: f32,
    /// True when a photographer has moved this slider and the proposal must not be
    /// applied.
    pub user_set: bool,
    /// Why.
    pub reasons: Vec<String>,
    /// Which generation proposed it.
    pub importance_ver: u16,
}

impl ImportanceProposal {
    /// True when applying this proposal would change anything.
    ///
    /// The comparison is at a hundredth, not a bit. Two importances that differ in the
    /// seventh decimal place are the same importance, and writing the row anyway would
    /// bump `updated_at` on every regroup and make the panel look like it changed.
    #[must_use]
    pub fn is_change(&self) -> bool {
        !self.user_set && (self.importance - self.current).abs() >= 0.01
    }
}

/// Everything the model needs about one identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImportanceInput {
    /// Whose.
    pub identity: IdentityId,
    /// What this person is.
    pub role: Role,
    /// Photographs they appear in.
    pub frames: u32,
    /// Photographs where they were the dominant subject.
    pub dominant_frames: u32,
    /// Minutes between their first and last appearance.
    pub span_minutes: i64,
    /// The importance currently stored.
    pub current: f32,
    /// True when a photographer has set the role, which locks the importance too.
    ///
    /// One lock, not two. A photographer who says "this is the bride" has said
    /// everything about how much she matters, and asking them to move a second control
    /// to make it stick would be a bug report.
    pub user_locked: bool,
}

/// Propose importances for a whole project.
///
/// Normalised against the project rather than against absolute numbers, because a
/// two-hour registry wedding and a three-day Nepali one have different absolute counts
/// and the same shape.
#[must_use]
pub fn propose(inputs: &[ImportanceInput], weights: &ProminenceWeights) -> Vec<ImportanceProposal> {
    let max_frames = inputs
        .iter()
        .map(|input| input.frames)
        .max()
        .unwrap_or(1)
        .max(1);
    let max_dominant = inputs
        .iter()
        .map(|input| input.dominant_frames)
        .max()
        .unwrap_or(0)
        .max(1);
    let max_span = inputs
        .iter()
        .map(|input| input.span_minutes)
        .max()
        .unwrap_or(0)
        .max(1);

    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        let role_term = weights.role_weight(input.role);
        let coverage = f64::from(input.frames) as f32 / f64::from(max_frames) as f32;
        let subject = f64::from(input.dominant_frames) as f32 / f64::from(max_dominant) as f32;
        let span = input.span_minutes as f32 / max_span as f32;

        let importance = (ROLE_WEIGHT * role_term
            + COVERAGE_WEIGHT * coverage
            + SUBJECT_WEIGHT * subject
            + SPAN_WEIGHT * span)
            .clamp(0.0, 1.0);

        let mut reasons = vec![format!(
            "role {} carries weight {role_term:.2} in weight table version {}",
            input.role, weights.version
        )];
        if input.frames > 0 {
            reasons.push(format!(
                "appears in {} photographs, {:.0}% of the most-photographed person's coverage",
                input.frames,
                coverage * 100.0
            ));
        }
        if input.dominant_frames > 0 {
            reasons.push(format!(
                "is the main subject of {} photographs",
                input.dominant_frames
            ));
        }
        if input.span_minutes > 0 {
            reasons.push(format!(
                "photographed across {} minutes of the day",
                input.span_minutes
            ));
        }
        if input.user_locked {
            reasons.push(
                "the photographer has set this person's role, so their importance is theirs to \
                 change and this proposal is not applied"
                    .to_string(),
            );
        }

        out.push(ImportanceProposal {
            identity: input.identity,
            importance,
            current: input.current,
            user_set: input.user_locked,
            reasons,
            importance_ver: IMPORTANCE_VER,
        });
    }
    out
}

/// Gather the model's inputs from stored identities and their timelines.
///
/// `dominant` counts, per identity, the frames where that identity was the dominant
/// subject - which comes from `aura_vision::face::prominence::image_subjects` and is
/// passed in rather than recomputed, because recomputing it would mean scoring every
/// frame twice.
#[must_use]
pub fn inputs_from(
    identities: &[IdentityRow],
    timelines: &[IdentityTimeline],
    dominant: &BTreeMap<IdentityId, u32>,
) -> Vec<ImportanceInput> {
    identities
        .iter()
        .map(|identity| {
            let timeline = timelines
                .iter()
                .find(|entry| entry.identity == identity.identity_id);
            ImportanceInput {
                identity: identity.identity_id,
                role: identity.role,
                frames: timeline.map_or(identity.face_count, |entry| entry.frames),
                dominant_frames: dominant.get(&identity.identity_id).copied().unwrap_or(0),
                span_minutes: timeline.map_or(0, IdentityTimeline::span_minutes),
                current: identity.importance,
                user_locked: identity.user_locked,
            }
        })
        .collect()
}
