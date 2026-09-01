//! What happens when a stage fails, and what a failure means for the rest of the wedding.
//!
//! Section 6.3. Two decisions, and they are separate on purpose.
//!
//! **How many more times to try** is about the failure: a stage that has attempts left is retried
//! with backoff, and one that does not is finished with. **What that means for the run** is about
//! the stage: an optional stage that ran out of attempts degrades the run and a mandatory one ends
//! it.
//!
//! Collapsing them is the mistake worth naming, and phase 27 made a version of it: a predicate
//! named for one question reused for a second it answers wrongly. "Has this stage failed" and
//! "may the run continue without it" are not the same question, and a single `should_stop` would
//! answer the first while being read as the second.

use crate::contract::autopilot::{StageOutcome, MAX_STAGE_ATTEMPTS, RETRY_BACKOFF_MS};

/// What to do about a stage that just failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Try again after this many milliseconds.
    Retry {
        /// How long to wait first.
        backoff_ms: u64,
        /// Which attempt the next one will be, from one.
        attempt: u8,
    },
    /// Out of attempts, and the run carries on without this stage.
    Isolate,
    /// Out of attempts, and the run cannot continue.
    FailRun,
}

/// What to do about a failure.
///
/// `attempts` is how many have already happened, including the one that just failed.
#[must_use]
pub const fn disposition(attempts: u8, optional: bool) -> Disposition {
    if attempts < MAX_STAGE_ATTEMPTS {
        return Disposition::Retry {
            backoff_ms: backoff_ms(attempts),
            attempt: attempts + 1,
        };
    }
    if optional {
        Disposition::Isolate
    } else {
        Disposition::FailRun
    }
}

/// How long to wait before attempt `attempts + 1`.
///
/// Doubling from [`RETRY_BACKOFF_MS`], and it saturates rather than shifting past the width of the
/// type. With `MAX_STAGE_ATTEMPTS` at three the largest wait is four seconds, so the saturation is
/// unreachable today and is here because a bound that is unreachable by arithmetic is one somebody
/// can raise without discovering an overflow.
#[must_use]
pub const fn backoff_ms(attempts: u8) -> u64 {
    let doublings = if attempts > 16 { 16 } else { attempts };
    RETRY_BACKOFF_MS.saturating_mul(1u64 << doublings)
}

/// Whether an outcome ends the run.
///
/// Separate from [`disposition`] and deliberately: this asks about the *run*, and it is true
/// exactly when a mandatory stage failed. An optional stage that failed, a stage that was skipped
/// and a stage that finished partially all leave a run that can still be delivered.
#[must_use]
pub const fn ends_run(outcome: &StageOutcome, optional: bool) -> bool {
    matches!(outcome, StageOutcome::Failed { .. }) && !optional
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_two_failures_are_retried_and_the_third_is_not() {
        assert_eq!(
            disposition(1, true),
            Disposition::Retry {
                backoff_ms: 4_000,
                attempt: 2
            }
        );
        assert_eq!(
            disposition(2, true),
            Disposition::Retry {
                backoff_ms: 8_000,
                attempt: 3
            }
        );
        assert_eq!(disposition(3, true), Disposition::Isolate);
    }

    #[test]
    fn a_mandatory_stage_out_of_attempts_ends_the_run_and_an_optional_one_does_not() {
        assert_eq!(disposition(MAX_STAGE_ATTEMPTS, false), Disposition::FailRun);
        assert_eq!(disposition(MAX_STAGE_ATTEMPTS, true), Disposition::Isolate);
    }

    #[test]
    fn the_backoff_doubles() {
        assert_eq!(backoff_ms(0), RETRY_BACKOFF_MS);
        assert_eq!(backoff_ms(1), RETRY_BACKOFF_MS * 2);
        assert_eq!(backoff_ms(2), RETRY_BACKOFF_MS * 4);
    }

    #[test]
    fn the_backoff_saturates_rather_than_overflowing() {
        assert!(backoff_ms(200) > 0);
        assert!(backoff_ms(200) >= backoff_ms(16));
    }

    #[test]
    fn only_a_failed_mandatory_stage_ends_a_run() {
        let failed = StageOutcome::Failed {
            code: "AURA-JOB-7005".into(),
            detail: String::new(),
        };
        assert!(ends_run(&failed, false));
        assert!(!ends_run(&failed, true));
        assert!(!ends_run(&StageOutcome::Completed { items: 4 }, false));
        assert!(!ends_run(
            &StageOutcome::Partial {
                items: 3,
                failed: 1,
                detail: String::new()
            },
            false
        ));
    }
}
