//! This phase's own errors. PHASE-27.
//!
//! Six codes, `AURA-ML-5136` to `AURA-ML-5141`. Three are the shapes every phase since 15 has
//! needed - the pass that could not run, the policy table that was refused, and the version drift
//! that stops a stale comparison happening silently - and three are raised by migration 27's own
//! triggers rather than by this module.
//!
//! `qc_decision_refused` lives in `aura_core::errors::ml` rather than here, because the frozen
//! `QcService` documents it on `decide` and a contract cannot depend on the crate that implements
//! it. The same split phases 16, 22, 23, 24, 25 and 26 made.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// One project's quality-control pass could not run.
pub const ML_QC_PASS_FAILED: ErrorCode = ErrorCode("AURA-ML-5136");

/// Something tried to edit or remove a recorded remediation round.
///
/// Raised by migration 27's `qc_round_no_update` and `qc_round_no_direct_delete` triggers. Named
/// here so a caller can match on it; this module never constructs it, because a correctly built
/// product never reaches it.
pub const ML_QC_ROUND_IMMUTABLE: ErrorCode = ErrorCode("AURA-ML-5138");

/// Something tried to edit a recorded frame replacement.
///
/// Raised by migration 27's `qc_replacement_is_immutable` trigger.
pub const ML_QC_REPLACEMENT_IMMUTABLE: ErrorCode = ErrorCode("AURA-ML-5139");

/// The quality-control thresholds table was refused.
pub const ML_QC_POLICY_REFUSED: ErrorCode = ErrorCode("AURA-ML-5140");

/// Stored findings came from different arithmetic or different thresholds.
pub const ML_QC_VERSION_DRIFT: ErrorCode = ErrorCode("AURA-ML-5141");

/// One project's quality-control pass could not run.
///
/// The pass is resumable and idempotent, so this is always safe to retry: a second attempt
/// continues from wherever the first stopped rather than re-inspecting what it already did.
///
/// It never means a photograph was damaged. Nothing in this crate opens a file, writes a recipe or
/// reaches a pixel, so a failed pass is a wedding that has not been checked - which is a different
/// and much smaller problem than a wedding that has been checked wrongly.
#[must_use]
pub fn pass_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_QC_PASS_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "AURA could not check this wedding's finished photographs. Every edit is exactly as it \
         was and nothing has been changed or lost.",
    )
}

/// The thresholds table asked for something the contract does not permit.
///
/// The pass **halts** rather than falling back on defaults - the same choice phases 24, 25 and 26
/// made for their own policy tables, and for the same reason: a QC pass running on thresholds
/// nobody chose would produce a report a photographer trusts and a set of remedies applied against
/// numbers from a file that failed to load.
///
/// The file may tighten a bound and may never widen one. That direction is the whole point: a
/// ceiling a studio can lower and nobody can raise is what makes `docs/how-qc-works.md` a promise
/// about the product rather than a description of its defaults.
#[must_use]
pub fn policy_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_QC_POLICY_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail,
        "AURA could not load the settings that decide what counts as a problem in a finished \
         photograph, so it has not checked anything. Restore the file or reinstall.",
    )
}

/// Stored findings were produced by different arithmetic or against different thresholds.
///
/// Two versions, because they invalidate two different things. `analysis_ver` invalidates every
/// deviation - the number came from arithmetic this build no longer runs. `thresholds_ver`
/// invalidates every threshold and therefore every severity ordering, without invalidating the
/// measurements.
///
/// Degraded rather than failing, because the answer is a background re-analysis and the
/// photographer's own accepted and dismissed verdicts survive it.
#[must_use]
pub fn version_drift(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_QC_VERSION_DRIFT,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "AURA has improved how it checks finished photographs, so it is re-checking this wedding \
         in the background. Anything you accepted or dismissed yourself is kept.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_is_in_the_registered_range() {
        for code in [
            ML_QC_PASS_FAILED,
            ML_QC_ROUND_IMMUTABLE,
            ML_QC_REPLACEMENT_IMMUTABLE,
            ML_QC_POLICY_REFUSED,
            ML_QC_VERSION_DRIFT,
        ] {
            assert!(code.0.starts_with("AURA-ML-513") || code.0.starts_with("AURA-ML-514"));
        }
    }

    #[test]
    fn a_refused_policy_halts_and_a_failed_pass_retries() {
        // The distinction is the whole of the two: a policy that will not load is a build problem
        // and running on defaults would be worse than not running; a pass that failed is a
        // transient and the pass is resumable.
        assert_eq!(policy_refused("x").severity, Severity::RunBlocking);
        assert_eq!(policy_refused("x").recovery, Recovery::Halt);
        assert_eq!(pass_failed("x").recovery, Recovery::Retry);
    }

    #[test]
    fn drift_is_degraded_rather_than_failing() {
        assert_eq!(version_drift("x").severity, Severity::Degraded);
    }
}
