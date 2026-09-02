//! Review and adoption: the one place a profile moves forward.
//!
//! ## `adopt` is the only code path that sets `adopted`
//!
//! There is no confidence above which an update adopts itself, no setting that enables one, and no
//! autopilot stage that calls this function. Section 10.1: "no learning update is adopted without
//! explicit user action", and `learn_update_no_self_adopt` in migration 30 refuses an INSERT that
//! arrives already adopted - two locks, because a promise enforced in one layer lasts until
//! somebody writes a second caller.
//!
//! ## Adoption re-checks the candidate rather than trusting it
//!
//! [`adopt`] takes a profile and reads whatever candidate is current, and it re-runs
//! `is_offerable` before writing. A photographer who left the panel open over a weekend of
//! corrections and then clicked adopt would otherwise adopt a fit measured against a profile that
//! has since moved. ADR-0062 decision 4.

use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{
    AbComparison, LearnCode, LearnReason, LearningUpdate, MIN_OFFERABLE_IMPROVEMENT,
};
use aura_core::AuraResult;

use crate::errors::no_update;
use crate::store::LearnStore;

/// What a photographer is being asked to look at.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    /// The update.
    pub update: LearningUpdate,
    /// The comparison, both sides on the same held-out corrections.
    pub comparison: AbComparison,
    /// Whether it may be shown at all.
    pub offerable: bool,
    /// What the panel says.
    pub reasons: Vec<LearnReason>,
}

/// Assemble the offer for a profile's current candidate.
///
/// # Errors
///
/// `AURA-LRN-11003` when there is no candidate.
pub fn offer(store: &LearnStore, profile: ProfileId) -> AuraResult<Offer> {
    let Some((update, comparison)) = store.candidate(profile)? else {
        return Err(no_update(format!(
            "no candidate update for profile {}",
            profile.to_db()
        )));
    };
    let offerable = update.is_offerable();
    let mut reasons = Vec::new();
    if offerable {
        reasons.push(LearnReason::with(
            LearnCode::HeldOutImproved,
            format!("{:.1} %", update.expected_improvement * 100.0),
        ));
    } else if comparison.candidate_error > comparison.current_error {
        reasons.push(LearnReason::plain(LearnCode::HeldOutRegressed));
    } else {
        reasons.push(LearnReason::with(
            LearnCode::HeldOutNoImprovement,
            format!(
                "{:.1} %, below {:.0} %",
                update.expected_improvement * 100.0,
                MIN_OFFERABLE_IMPROVEMENT * 100.0
            ),
        ));
    }
    Ok(Offer {
        update,
        comparison,
        offerable,
        reasons,
    })
}

/// Adopt a profile's current candidate. **The only way `adopted` becomes true.**
///
/// # Errors
///
/// `AURA-LRN-11003` when there is no candidate, or when the candidate is no longer offerable -
/// which is what a photographer who left the panel open over a weekend of corrections meets.
pub fn adopt(
    store: &LearnStore,
    profile: ProfileId,
) -> AuraResult<(LearningUpdate, Vec<LearnReason>)> {
    let offer = offer(store, profile)?;
    if !offer.offerable {
        return Err(no_update(format!(
            "the candidate for profile {} is not offerable: {:.3} improvement on {} corrections",
            profile.to_db(),
            offer.update.expected_improvement,
            offer.update.corrections_used
        )));
    }
    let adopted = store.adopt(profile, offer.update.to_version)?;
    Ok((
        adopted,
        vec![
            LearnReason::plain(LearnCode::AdoptedByUser),
            LearnReason::with(
                LearnCode::HeldOutImproved,
                format!("{:.1} %", offer.update.expected_improvement * 100.0),
            ),
        ],
    ))
}
