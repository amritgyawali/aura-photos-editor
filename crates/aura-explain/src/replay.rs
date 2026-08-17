//! Asking the same question again, and finding out whether the answer moved.
//!
//! Section 2.1: "`aura replay <decision_id>` re-runs a decision from the ledger and asserts
//! an identical outcome." Section 6.3 says what makes that possible: `inputs_hash` covers
//! the analysis, the config and the model versions, "so replay can assert determinism and
//! detect drift after an upgrade".
//!
//! ## The two failures a replay can find, and why they are opposite
//!
//! * **Same inputs, different outputs.** A determinism bug. The question was identical and
//!   the product answered it differently, which means something in the pipeline is reading
//!   a clock, iterating a hash map or depending on a thread count. This is a defect.
//! * **Different inputs, different outputs.** An upgrade. A model was retrained, a
//!   threshold moved, a photographer re-ran an analysis pass. The product is *supposed* to
//!   decide differently, and a tool that reported this as a failure would be a tool nobody
//!   ran twice.
//!
//! [`ReplayOutcome`] distinguishes them, and `AURA-ML-5057`'s message says which one it is
//! in words rather than making a support engineer compare two hex strings.
//!
//! ## Why this crate does not run the engine
//!
//! Because it must not depend on the phases that decide. [`ReplaySource`] is a port: it
//! takes a stored decision and returns what today's code would produce for the same
//! subject. `aura-cli` implements it over phase 12's engine, and phase 27 will implement it
//! over its own. The comparison, the verdict and the error live here, so the six phases that
//! will eventually be replayable share one definition of what "identical" means.

use aura_core::contract::ledger::LedgerDecision;
use aura_core::{AuraError, AuraResult};

use crate::errors;

/// What today's code would decide about the same subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recomputed {
    /// The hash of the question, computed the same way the original was.
    pub inputs_hash: u64,
    /// What was decided, in the same canonical encoding.
    pub outputs_json: String,
}

/// Something that can re-run a decision.
///
/// One method, and it takes the stored decision rather than a subject: a replay needs to
/// know *which* decision it is reproducing, because a subject can have several - a cull, an
/// edit and a QC ticket about one photograph.
pub trait ReplaySource: std::fmt::Debug {
    /// Re-run this decision against the catalog as it stands now.
    ///
    /// # Errors
    ///
    /// Whatever the deciding phase raised. A subject that no longer exists, an analysis that
    /// has been cleared and a project that has been deleted are all legitimate reasons for a
    /// replay to be impossible, and all three are `AURA-ML-5056`.
    fn recompute(&self, decision: &LedgerDecision) -> AuraResult<Recomputed>;
}

/// How a replay came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// Identical inputs, identical outputs. What the gate asserts.
    Identical,
    /// Identical inputs, different outputs. A determinism defect.
    Nondeterministic {
        /// What was stored.
        stored: String,
        /// What came back.
        fresh: String,
    },
    /// Different inputs. The pipeline moved underneath the decision.
    Drifted {
        /// The stored question.
        stored_hash: u64,
        /// Today's question.
        fresh_hash: u64,
        /// True when the answer changed as well as the question.
        outputs_differ: bool,
    },
}

impl ReplayOutcome {
    /// True when nothing moved.
    #[must_use]
    pub const fn is_identical(&self) -> bool {
        matches!(self, Self::Identical)
    }

    /// The stable slug, for a report and for the CLI's exit line.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Identical => "identical",
            Self::Nondeterministic { .. } => "nondeterministic",
            Self::Drifted { .. } => "drifted",
        }
    }
}

/// Compares a stored decision with a fresh one.
#[derive(Debug)]
pub struct Replayer<'a> {
    source: &'a dyn ReplaySource,
}

impl<'a> Replayer<'a> {
    /// A replayer over one deciding phase.
    #[must_use]
    pub fn new(source: &'a dyn ReplaySource) -> Self {
        Self { source }
    }

    /// Re-run one decision and say what happened.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5056` when the decision cannot be re-run at all.
    pub fn replay(&self, decision: &LedgerDecision) -> AuraResult<ReplayOutcome> {
        let fresh = self
            .source
            .recompute(decision)
            .map_err(|err| errors::replay_failed(&decision.id.to_db(), err.detail.clone()))?;
        Ok(compare(decision, &fresh))
    }

    /// Re-run one decision and turn anything but an exact reproduction into an error.
    ///
    /// What `aura replay` uses when it is asked to be a gate rather than a report.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5056` when the decision cannot be re-run, `AURA-ML-5057` when it comes out
    /// differently.
    pub fn assert_identical(&self, decision: &LedgerDecision) -> Result<(), AuraError> {
        match self.replay(decision)? {
            ReplayOutcome::Identical => Ok(()),
            ReplayOutcome::Nondeterministic { .. } => Err(errors::replay_drift(
                &decision.id.to_db(),
                decision.inputs_hash,
                decision.inputs_hash,
                "the decision itself",
            )),
            ReplayOutcome::Drifted {
                stored_hash,
                fresh_hash,
                outputs_differ,
            } => Err(errors::replay_drift(
                &decision.id.to_db(),
                stored_hash,
                fresh_hash,
                if outputs_differ {
                    "its inputs and its decision"
                } else {
                    "its inputs, though it still decided the same thing"
                },
            )),
        }
    }
}

/// The comparison itself, as a free function so a test can reach it without a source.
#[must_use]
pub fn compare(stored: &LedgerDecision, fresh: &Recomputed) -> ReplayOutcome {
    let outputs_differ = stored.outputs_json != fresh.outputs_json;
    if stored.inputs_hash != fresh.inputs_hash {
        return ReplayOutcome::Drifted {
            stored_hash: stored.inputs_hash,
            fresh_hash: fresh.inputs_hash,
            outputs_differ,
        };
    }
    if outputs_differ {
        return ReplayOutcome::Nondeterministic {
            stored: stored.outputs_json.clone(),
            fresh: fresh.outputs_json.clone(),
        };
    }
    ReplayOutcome::Identical
}
