//! Capture: writing one correction down.
//!
//! The thinnest module in the crate, and it is thin on purpose. Everything that decides whether a
//! correction counts is in [`crate::attribute`]; this is what happens after that decision.
//!
//! ## There is no `learn_capture` command on the IPC surface
//!
//! Corrections are captured by the panels that already own the override - the develop panel, the
//! cull panel, the curation panel - inside the command that records the override. A capture command
//! on the wire would be a second route into the correction table with no decision behind it, which
//! is exactly what `AURA-LRN-11004` refuses. ADR-0062 decision 6.

use aura_core::contract::learn::{Consent, Correction, CorrectionContext, LearnCode, LearnReason};
use aura_core::contract::ledger::ExplainService;
use aura_core::AuraResult;

use crate::attribute::attribute;
use crate::store::LearnStore;

/// Record one correction, if it is one.
///
/// Returns the reasons a photographer reads. A correction that is refused returns an error rather
/// than an empty list, because a caller that ignored the difference would silently stop learning.
///
/// # Errors
///
/// `AURA-LRN-11004` when there is no decision behind the change or the project has not consented.
pub fn capture(
    store: &LearnStore,
    explain: &dyn ExplainService,
    correction: &Correction,
    context: &CorrectionContext,
    consent: &Consent,
) -> AuraResult<Vec<LearnReason>> {
    let attributed = attribute(explain, correction, context, consent)?;

    // An immaterial change is not written. A thousand rows of nothing would drag every median
    // toward zero, which is the same defect as counting an unopened panel as agreement.
    if attributed
        .reasons
        .iter()
        .any(|r| r.code == LearnCode::CorrectionImmaterial)
    {
        return Ok(attributed.reasons);
    }

    let mut context = context.clone();
    context.held_out = attributed.held_out;
    store.write_correction(correction, &context, attributed.bucket.subject_close)?;
    Ok(attributed.reasons)
}

/// Whether anything about this project may be learned from at all.
///
/// One predicate, checked once, rather than three checks at three call sites - which is how one of
/// them comes to be forgotten. Phase 27 wrote this down after `TicketStatus::is_open()` answered
/// two different questions.
#[must_use]
pub const fn may_learn(consent: &Consent) -> bool {
    consent.local_learning
}

/// Whether anything about this project may leave the machine.
///
/// Deliberately separate from [`may_learn`]. "May this machine learn from this wedding" and "may
/// anonymised evidence leave it" are different questions, and collapsing them is how the second
/// one happens by accident.
#[must_use]
pub const fn may_contribute(consent: &Consent) -> bool {
    consent.dataset_contribution
}
