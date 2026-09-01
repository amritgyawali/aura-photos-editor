//! How long is left, and whether the number came from this machine.
//!
//! Section 6.4: "ETA is computed from measured throughput of completed units per stage plus
//! per-stage estimates for remaining stages, updated continuously; it must be within 20 % after
//! 10 % of the run."
//!
//! ## The two halves, and why they are never blended
//!
//! The current stage's remaining time comes from *measured* throughput once there is any, and
//! from its declared estimate before that. Every stage that has not started comes from its
//! declared estimate, always - there is no measurement of a stage nobody has run, and inventing
//! one by scaling another stage's throughput would be assuming the retouch runs as fast as the
//! ingest.
//!
//! [`Eta::measured`] is true exactly when the current stage has passed its warm-up share, which is
//! what a panel renders as a time rather than as an estimate. A build that reported one number
//! with no provenance would be a build promising two hours on a machine doing four, and the
//! photographer would find out at hour three.
//!
//! ## Why the warm-up is a share of the stage rather than a count of units
//!
//! Because a stage with four units and a stage with four thousand need different amounts of
//! evidence for the same confidence, and the run's own accuracy gate is stated as a share.
//! [`ETA_WARMUP_SHARE`] of a four-unit stage is one unit, which is thin - and it is thin for a
//! stage that will be over in seconds, where a wrong ETA costs nothing.

use std::time::Duration;

use crate::contract::autopilot::{Eta, StageId, ETA_WARMUP_SHARE};
use crate::stages;

/// One stage's contribution to the remaining time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Remaining {
    /// Which stage.
    pub stage: StageId,
    /// Units it still has to do.
    pub units: u32,
}

/// What the current stage has actually done.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    /// Which stage.
    pub stage: StageId,
    /// Units finished in it.
    pub done: u32,
    /// Units it has in total.
    pub total: u32,
    /// Wall clock spent in it.
    pub elapsed: Duration,
}

impl Measured {
    /// Units per second, or `None` before anything finished.
    #[must_use]
    pub fn throughput_per_s(&self) -> Option<f32> {
        let seconds = self.elapsed.as_secs_f32();
        if self.done == 0 || seconds <= 0.0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        Some(self.done as f32 / seconds)
    }

    /// Whether enough of this stage has run for its throughput to be worth trusting.
    #[must_use]
    pub fn past_warmup(&self) -> bool {
        if self.total == 0 {
            return false;
        }
        #[allow(clippy::cast_precision_loss)]
        let share = self.done as f32 / self.total as f32;
        self.done > 0 && share >= ETA_WARMUP_SHARE
    }
}

/// How long the run has left.
///
/// `current` is the stage in flight; `pending` is every stage after it that will run, in order,
/// with the units each will have. A stage the run will skip must not appear in `pending` - a
/// skipped stage takes no time, and counting it is the difference between an ETA that is honest
/// and one that is padded by three stages this build cannot run.
#[must_use]
pub fn estimate(current: Measured, pending: &[Remaining]) -> Eta {
    let declared = u64::from(stages::decl(current.stage).est_ms_per_item);
    let outstanding = u64::from(current.total.saturating_sub(current.done));

    let measured_rate = current.throughput_per_s();
    let past_warmup = current.past_warmup();

    let current_ms = match measured_rate {
        Some(rate) if past_warmup && rate > 0.0 => {
            #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
            #[allow(clippy::cast_possible_truncation)]
            {
                ((outstanding as f32 / rate) * 1_000.0) as u64
            }
        }
        _ => outstanding.saturating_mul(declared),
    };

    let pending_ms: u64 = pending
        .iter()
        .map(|row| {
            u64::from(stages::decl(row.stage).est_ms_per_item).saturating_mul(u64::from(row.units))
        })
        .sum();

    Eta {
        remaining_ms: current_ms.saturating_add(pending_ms),
        throughput_per_s: measured_rate.unwrap_or(0.0),
        measured: past_warmup,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn before_the_warm_up_the_estimate_is_declared_rather_than_measured() {
        let current = Measured {
            stage: StageId::Previews,
            done: 1,
            total: 1_000,
            elapsed: Duration::from_millis(50),
        };
        let eta = estimate(current, &[]);
        assert!(!eta.measured);
        // 999 outstanding at the declared 380 ms.
        assert_eq!(eta.remaining_ms, 999 * 380);
    }

    #[test]
    fn past_the_warm_up_the_estimate_is_this_machines_own_throughput() {
        // 200 of 1,000 done in 100 s is 2 per second, so 800 left is 400 s. The declared estimate
        // would have said 800 * 380 ms = 304 s, and the measured number is the one that ships.
        let current = Measured {
            stage: StageId::Previews,
            done: 200,
            total: 1_000,
            elapsed: Duration::from_secs(100),
        };
        let eta = estimate(current, &[]);
        assert!(eta.measured);
        assert_eq!(eta.remaining_s(), 400);
        assert!((eta.throughput_per_s - 2.0).abs() < 0.001);
    }

    #[test]
    fn a_slow_machine_is_reported_as_slow_rather_than_as_the_reference_laptop() {
        // The failure this whole module exists to prevent: a machine running at a quarter of the
        // reference speed must not still be promising the reference time.
        let slow = Measured {
            stage: StageId::Previews,
            done: 500,
            total: 1_000,
            elapsed: Duration::from_secs(760),
        };
        let eta = estimate(slow, &[]);
        assert!(eta.measured);
        let declared = 500 * 380;
        assert!(
            eta.remaining_ms > declared * 3,
            "a machine at a quarter speed reported {} ms against a declared {declared} ms",
            eta.remaining_ms
        );
    }

    #[test]
    fn pending_stages_use_their_declared_estimates() {
        let current = Measured {
            stage: StageId::Ingest,
            done: 100,
            total: 100,
            elapsed: Duration::from_secs(4),
        };
        let eta = estimate(
            current,
            &[Remaining {
                stage: StageId::Previews,
                units: 10,
            }],
        );
        assert_eq!(eta.remaining_ms, 10 * 380);
    }

    #[test]
    fn a_skipped_stage_contributes_nothing_because_it_is_not_pending() {
        let current = Measured {
            stage: StageId::Ingest,
            done: 100,
            total: 100,
            elapsed: Duration::from_secs(4),
        };
        assert_eq!(estimate(current, &[]).remaining_ms, 0);
    }

    #[test]
    fn a_stage_with_no_units_is_never_past_its_warm_up() {
        let current = Measured {
            stage: StageId::Cull,
            done: 0,
            total: 0,
            elapsed: Duration::from_secs(1),
        };
        assert!(!current.past_warmup());
        assert_eq!(current.throughput_per_s(), None);
    }

    #[test]
    fn the_accuracy_gate_is_met_on_a_machine_running_at_a_constant_speed() {
        // Section 10.1: within 20 % after 10 % of the run. A machine whose throughput is constant
        // is the case the gate is actually about - the estimator must not be wrong on the easy
        // one - and this walks a 1,000-unit stage from 10 % to 90 % checking every step.
        let real_ms_per_unit = 500u64;
        let total = 1_000u32;
        for done in (100..=900).step_by(50) {
            let elapsed = Duration::from_millis(u64::from(done) * real_ms_per_unit);
            let eta = estimate(
                Measured {
                    stage: StageId::Previews,
                    done,
                    total,
                    elapsed,
                },
                &[],
            );
            let truth = u64::from(total - done) * real_ms_per_unit;
            #[allow(clippy::cast_precision_loss)]
            let error = (eta.remaining_ms as f32 - truth as f32).abs() / truth as f32;
            assert!(
                error <= crate::contract::autopilot::ETA_TOLERANCE,
                "at {done} of {total} the estimate was off by {:.1} %",
                error * 100.0
            );
        }
    }
}
