//! `ExplainService`, and the three things that happen between a decision being made and a
//! decision being recorded.
//!
//! In order, always, and the order is the point:
//!
//! 1. **Validate.** Every reason's code must be in the registry and every decision must
//!    carry at least one. A decision that fails this is refused - `AURA-ML-5054` - and not
//!    written, because a ledger row that cannot say why is the row a support case finds
//!    when it is looking for the reason.
//! 2. **Calibrate.** The raw confidence goes through the model for this
//!    `(kind, source)` pair. Both numbers are kept.
//! 3. **Band.** The calibrated number plus the risks produce an [`Autonomy`], which is
//!    stored rather than recomputed on read.
//!
//! ## Why steps 2 and 3 are done here rather than by the caller
//!
//! Because the caller is the deciding phase, and a deciding phase that could set its own
//! autonomy band would be a deciding phase that could grant itself permission to act. The
//! `calibrated_confidence` and `autonomy` fields of the argument are **overwritten**, not
//! trusted, and a test asserts it by handing `record` a decision that claims
//! [`Autonomy::Auto`] at confidence 0.1 and checking what comes back out.
//!
//! ## The uncalibrated caveat is a reason, not a footnote
//!
//! When the model for a pair is the identity map - which is every pair in this build - the
//! recorded decision gains an `uncalibrated_confidence` reason and the band is raised one
//! step. The photographer therefore reads *why* AURA asked, instead of seeing a 0.99 beside
//! a review request and concluding the product is broken.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{
    Autonomy, DecisionKind, DecisionSubject, ExplainService, LedgerDecision, LedgerOutline,
    LedgerReason,
};
use aura_core::{AuraResult, ProjectId};

use crate::calibration::CalibrationSet;
use crate::decision::{rank, DecisionBuilder};
use crate::errors;
use crate::ledger::{Compaction, Ledger};
use crate::policy::{AutonomyPolicy, Risk};
use crate::reason::Catalog;

/// The one implementation of [`ExplainService`].
#[derive(Debug)]
pub struct Explain {
    ledger: Arc<Ledger>,
    policy: Arc<AutonomyPolicy>,
    calibration: Arc<CalibrationSet>,
    clock: Arc<dyn Clock>,
    registry: Catalog,
}

impl Explain {
    /// An explainer over one catalog, with the shipped band table and calibration set.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5055` when the embedded band table will not load. Halting, because a product
    /// that cannot say what it is allowed to do must not do anything.
    pub fn new(ledger: Arc<Ledger>, clock: Arc<dyn Clock>) -> AuraResult<Self> {
        Ok(Self {
            ledger,
            policy: Arc::new(AutonomyPolicy::embedded()?),
            calibration: Arc::new(CalibrationSet::shipped()),
            clock,
            registry: Catalog::shipped(),
        })
    }

    /// The same explainer with a band table and calibration set already loaded.
    #[must_use]
    pub fn with_tables(
        ledger: Arc<Ledger>,
        clock: Arc<dyn Clock>,
        policy: Arc<AutonomyPolicy>,
        calibration: Arc<CalibrationSet>,
    ) -> Self {
        Self {
            ledger,
            policy,
            calibration,
            clock,
            registry: Catalog::shipped(),
        }
    }

    /// The band table underneath this explainer.
    #[must_use]
    pub fn policy(&self) -> &AutonomyPolicy {
        &self.policy
    }

    /// The calibration set underneath it.
    #[must_use]
    pub fn calibration(&self) -> &CalibrationSet {
        &self.calibration
    }

    /// The reason registry it validates against.
    #[must_use]
    pub const fn registry(&self) -> &Catalog {
        &self.registry
    }

    /// The ledger it writes to.
    #[must_use]
    pub fn ledger(&self) -> &Arc<Ledger> {
        &self.ledger
    }

    /// Build, stamp and record one decision, returning what was actually stored.
    ///
    /// The convenience every deciding phase uses. It assigns the id and the timestamp -
    /// which is why it is here rather than on [`DecisionBuilder`], since a builder that
    /// read a clock would be a builder no test could pin - and then runs the three steps
    /// this module's header describes.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5054` when the decision cannot explain itself, and `AURA-DB-3006` when the
    /// row cannot be written.
    pub fn record_built(&self, builder: DecisionBuilder, risk: Risk) -> AuraResult<LedgerDecision> {
        let created_at = (self.clock.now_utc().unix_timestamp_nanos() / 1_000_000) as i64;
        let decision = builder.build(DecisionId::new(), created_at);
        self.record_with_risk(&decision, risk)
    }

    /// Record a decision that is already built, with an explicit risk.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5054` when the decision cannot explain itself, and `AURA-DB-3006` when the
    /// row cannot be written.
    pub fn record_with_risk(
        &self,
        decision: &LedgerDecision,
        risk: Risk,
    ) -> AuraResult<LedgerDecision> {
        self.validate(decision)?;

        let (calibrated, calibration_ver) =
            self.calibration
                .apply(decision.kind, decision.source, decision.raw_confidence);
        let uncalibrated = calibration_ver == LedgerDecision::UNCALIBRATED;
        let risk = risk.for_kind(decision.kind).with_uncalibrated(uncalibrated);
        let autonomy = self.policy.decide(decision.kind, calibrated, risk);

        let mut stored = decision.clone();
        stored.calibrated_confidence = calibrated;
        stored.calibration_ver = calibration_ver;
        stored.autonomy = autonomy;
        stored
            .config_versions
            .push(("autonomy_bands".to_string(), self.policy.version()));
        stored.config_versions.sort();
        stored.config_versions.dedup();

        // Why the band was raised, in the photographer's own panel rather than in a log.
        let raised = risk.reason_codes(&self.policy.multipliers());
        if !raised.is_empty() {
            for code in raised {
                stored.reasons.push(LedgerReason::plain(
                    code,
                    self.registry.template(code),
                    // Zero weight: a raise did not argue for or against the decision, it
                    // changed what the product was allowed to do about it. A non-zero weight
                    // would sort it among the reasons that moved the decision itself.
                    0.0,
                ));
            }
            stored.reasons = rank(stored.reasons, LedgerDecision::MAX_REASONS);
        }

        self.ledger.append(&stored)?;
        Ok(stored)
    }

    /// Invariant 2, checked before anything is written.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5054`, in both of its forms.
    pub fn validate(&self, decision: &LedgerDecision) -> AuraResult<()> {
        if !decision.is_explained() {
            return Err(errors::decision_unexplained(
                decision.kind.as_str(),
                &decision.subject.to_string(),
            ));
        }
        for reason in &decision.reasons {
            if !self.registry.contains(&reason.code) {
                return Err(errors::unknown_reason_code(
                    &reason.code,
                    decision.kind.as_str(),
                ));
            }
        }
        Ok(())
    }

    /// The pairs that have no fitted calibration, as one warning or nothing.
    ///
    /// Raised once per run by whatever drives a pass, never per decision: an unfitted map is
    /// a property of the build. `AURA-ML-5058`.
    #[must_use]
    pub fn calibration_warning(&self) -> Option<aura_core::AuraError> {
        let gaps = self.calibration.uncalibrated();
        if gaps.is_empty() {
            return None;
        }
        let slugs: Vec<&str> = gaps.iter().map(String::as_str).collect();
        Some(errors::uncalibrated_confidence(&slugs))
    }

    /// Compact this project's ledger with the default retention window.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the compaction cannot run.
    pub fn compact_now(&self, project: ProjectId) -> AuraResult<u32> {
        let now = (self.clock.now_utc().unix_timestamp_nanos() / 1_000_000) as i64;
        self.ledger.compact(project, Compaction::at(now))
    }
}

impl ExplainService for Explain {
    fn record(&self, decision: &LedgerDecision) -> AuraResult<DecisionId> {
        // The risk of a decision recorded through the frozen trait is whatever the kind
        // itself implies. A caller that knows more - that this frame is the only photograph
        // of the rings - uses `record_with_risk`, because a trait method that took a risk
        // argument would be a trait every later phase had to understand phase 12's coverage
        // rules to call.
        let stored = self.record_with_risk(decision, Risk::NONE)?;
        Ok(stored.id)
    }

    fn decision(&self, id: DecisionId) -> AuraResult<Option<LedgerDecision>> {
        self.ledger.get(id)
    }

    fn history(&self, subject: DecisionSubject) -> AuraResult<Vec<LedgerDecision>> {
        self.ledger.history(subject)
    }

    fn latest(
        &self,
        subject: DecisionSubject,
        kind: DecisionKind,
    ) -> AuraResult<Option<LedgerDecision>> {
        Ok(self
            .ledger
            .history(subject)?
            .into_iter()
            .find(|decision| decision.kind == kind))
    }

    fn outline(&self, project: ProjectId) -> AuraResult<LedgerOutline> {
        self.ledger.outline(project)
    }

    fn compact(&self, project: ProjectId) -> AuraResult<u32> {
        self.compact_now(project)
    }
}

/// The band a confidence would get, without recording anything.
///
/// What the grid's confidence badges call: four thousand thumbnails must not each write a
/// ledger row to find out what colour their badge is.
#[must_use]
pub fn preview_band(
    policy: &AutonomyPolicy,
    kind: DecisionKind,
    calibrated: f32,
    risk: Risk,
) -> Autonomy {
    policy.decide(kind, calibrated, risk.for_kind(kind))
}
