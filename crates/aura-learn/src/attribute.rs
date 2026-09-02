//! Attribution: which decision a change is a correction *of*.
//!
//! ## `decision_id` is not an `Option`, and this module is why
//!
//! A photographer moves the exposure slider on a photograph. That is a correction if, and only if,
//! AURA had made an exposure decision about that photograph. If it had not, the slider is a
//! preference with no baseline, and a residual measured from no baseline is an absolute edit
//! wearing a residual's shape.
//!
//! Phase 17 found that from the other side: its condition C4 is that a learned delta computed
//! against a neutral baseline is an absolute edit. This is the phase that would carry it into
//! *every future wedding*, so the check is here rather than in a validator, and
//! `AURA-LRN-11004` is what a caller meets.
//!
//! ## Consent is checked at the same place, and for the same reason
//!
//! Both refusals leave the photograph exactly as the photographer left it and both decline to
//! learn. Splitting them across two call sites is how one of them comes to be forgotten - which
//! phase 27 wrote down after `TicketStatus::is_open()` answered two questions.

use aura_core::contract::ids::DecisionId;
use aura_core::contract::learn::{
    Consent, Correction, CorrectionBucket, CorrectionContext, LearnCode, LearnReason,
};
use aura_core::contract::ledger::{DecisionKind, ExplainService};
use aura_core::AuraResult;

use crate::errors::not_attributed;

/// What attribution decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Attributed {
    /// The bucket this correction counts in.
    pub bucket: CorrectionBucket,
    /// Whether the correction is held out of the fit.
    pub held_out: bool,
    /// What the panel says.
    pub reasons: Vec<LearnReason>,
}

/// Check a correction against the ledger and the project's consent.
///
/// # Errors
///
/// `AURA-LRN-11004` when there is no decision behind the change, when the decision names a
/// different kind from the correction, or when the project has not consented to local learning.
pub fn attribute(
    explain: &dyn ExplainService,
    correction: &Correction,
    context: &CorrectionContext,
    consent: &Consent,
) -> AuraResult<Attributed> {
    if !consent.local_learning {
        return Err(not_attributed(format!(
            "project {} has not consented to local learning",
            consent.project.to_db()
        )));
    }

    // The decision has to exist. This is the check the whole module is for.
    let Some(decision) = explain.decision(correction.decision_id)? else {
        return Err(not_attributed(format!(
            "no ledger decision {} for this change",
            correction.decision_id.to_db()
        )));
    };

    // ...and it has to be a decision of the right kind. A photographer who moved an exposure
    // slider on a frame whose only decision was a cull decision has still corrected nothing about
    // exposure, and counting it would put a cull's baseline under an exposure offset.
    if decision.kind != correction.kind || decision.kind != context.learnable.decision_kind() {
        return Err(not_attributed(format!(
            "decision {} is a {} decision; the correction is a {} correction of {}",
            correction.decision_id.to_db(),
            decision.kind.as_str(),
            correction.kind.as_str(),
            context.learnable.as_str()
        )));
    }

    if !correction.is_material(context.learnable) {
        // Not an error: a photographer opening a panel and closing it is not a failure. But it is
        // not recorded either, because a thousand rows of nothing drag every median toward zero.
        return Ok(Attributed {
            bucket: bucket_of(correction, context, false),
            held_out: false,
            reasons: vec![LearnReason::plain(LearnCode::CorrectionImmaterial)],
        });
    }

    let subject_close = correction.identity.is_some();
    Ok(Attributed {
        bucket: bucket_of(correction, context, subject_close),
        held_out: crate::aggregate::hold_out(correction.decision_id),
        reasons: vec![LearnReason::plain(LearnCode::CorrectionCaptured)],
    })
}

/// The bucket a correction counts in.
///
/// Section 6.3: "(decision kind, scene bucket, identity role)". The identity half is a **role** and
/// not a person - a profile that learned "brighten this specific bride" is a profile that is wrong
/// on every subsequent wedding, and `subject_close` is the only thing about the person that
/// reaches the key.
#[must_use]
pub fn bucket_of(
    correction: &Correction,
    context: &CorrectionContext,
    subject_close: bool,
) -> CorrectionBucket {
    CorrectionBucket {
        kind: correction.kind,
        scene: correction.scene,
        learnable: context.learnable,
        subject_close,
    }
}

/// Whether a decision kind is one this loop reads at all.
///
/// Three of the six. `Retouch` is absent because a retouch strength is a guarantee's neighbour and
/// the ceiling is not learnable; `Qc` because a QC dismissal is a statement about a finding rather
/// than about a preference; `Export` because an export setting is a job, not a taste.
#[must_use]
pub const fn is_learnable_kind(kind: DecisionKind) -> bool {
    matches!(
        kind,
        DecisionKind::Edit | DecisionKind::Cull | DecisionKind::Curate
    )
}

/// Which id a correction points at, for a caller that has only the row.
#[must_use]
pub const fn decision_of(correction: &Correction) -> DecisionId {
    correction.decision_id
}
