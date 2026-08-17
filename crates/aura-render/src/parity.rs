//! The GPU/CPU parity harness.
//!
//! Section 6.2: *"CI compares GPU vs CPU within a per-stage tolerance (typically <= 1/1024
//! in linear light) and fails on drift."* This module is that comparison, and
//! `AURA-RENDER-8010` is what it raises.
//!
//! # It cannot fire in this build, and it is tested anyway
//!
//! There is no accelerator (ADR-0029 section 4), so there is nothing to disagree with the
//! reference. `crates/aura-render/tests/parity.rs` therefore runs the harness against a
//! deliberately perturbed copy of the reference and asserts that it *catches* the
//! perturbation - which proves the harness detects drift and says nothing whatever about any
//! device.
//!
//! That distinction is the phase's own honesty rule applied to a test: a green parity gate
//! in this build means "the harness works", not "the accelerator agrees".

use aura_core::errors::render::parity_drift;
use aura_core::AuraResult;

use crate::graph::Stage;

/// The per-stage tolerance, in linear light.
///
/// 1/1024. Stated in **linear** light rather than in output codes, because that is where the
/// arithmetic happens and because an output-code tolerance would be four times looser in the
/// shadows than in the highlights - which is exactly backwards from where an error is
/// visible.
pub const TOLERANCE: f32 = 1.0 / 1024.0;

/// What a comparison found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    /// The stage compared.
    pub stage: Stage,
    /// The largest absolute difference found.
    pub max_delta: f32,
    /// The mean absolute difference.
    pub mean_delta: f32,
    /// Samples compared.
    pub samples: usize,
    /// True when `max_delta <= TOLERANCE`.
    pub within_tolerance: bool,
}

impl Comparison {
    /// The error this comparison raises, or `None` when it passed.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8010` at `RunBlocking`. Section 6.2: *"failure blocks the build"*.
    pub fn check(&self) -> AuraResult<()> {
        if self.within_tolerance {
            Ok(())
        } else {
            Err(parity_drift(self.stage.as_str(), self.max_delta, TOLERANCE))
        }
    }
}

/// Compare two buffers for one stage.
///
/// `reference` is the processor path's output and `candidate` is the accelerator's. The
/// asymmetry is deliberate and is stated in the argument names: when the two disagree, the
/// reference is right until an ADR says otherwise.
#[must_use]
pub fn compare(stage: Stage, reference: &[f32], candidate: &[f32]) -> Comparison {
    let samples = reference.len().min(candidate.len());
    let mut max_delta = 0.0f32;
    let mut total = 0.0f64;

    for index in 0..samples {
        let a = reference.get(index).copied().unwrap_or(0.0);
        let b = candidate.get(index).copied().unwrap_or(0.0);
        let delta = (a - b).abs();
        if delta > max_delta {
            max_delta = delta;
        }
        total += f64::from(delta);
    }

    // A length mismatch is a failure rather than a shorter comparison: two buffers of
    // different sizes are two different renders, and comparing the overlap would pass.
    if reference.len() != candidate.len() {
        max_delta = f32::INFINITY;
    }

    Comparison {
        stage,
        max_delta,
        mean_delta: if samples == 0 {
            0.0
        } else {
            (total / samples as f64) as f32
        },
        samples,
        within_tolerance: max_delta <= TOLERANCE,
    }
}

/// Compare every stage in a list, returning the first failure.
///
/// # Errors
///
/// `AURA-RENDER-8010` for the first stage that drifts.
pub fn check_all(comparisons: &[Comparison]) -> AuraResult<()> {
    for comparison in comparisons {
        comparison.check()?;
    }
    Ok(())
}

/// Perturb a buffer by a fixed amount, for testing the harness itself.
///
/// Deterministic: every sample moves by exactly `delta`. A pseudo-random perturbation would
/// make the harness's own test flaky, which is the one thing a drift detector must not be.
#[must_use]
pub fn perturb(buffer: &[f32], delta: f32) -> Vec<f32> {
    buffer.iter().map(|value| value + delta).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers_pass() {
        let buffer = vec![0.1f32, 0.5, 0.9, 1.4];
        let comparison = compare(Stage::Exposure, &buffer, &buffer);
        assert!(comparison.within_tolerance);
        assert!(comparison.max_delta.abs() < f32::EPSILON);
        comparison.check().expect("identical buffers agree");
    }

    #[test]
    fn a_drift_just_inside_the_tolerance_passes() {
        let buffer = vec![0.1f32; 32];
        let candidate = perturb(&buffer, TOLERANCE * 0.9);
        assert!(compare(Stage::Tone, &buffer, &candidate).within_tolerance);
    }

    #[test]
    fn a_drift_just_outside_the_tolerance_fails_and_names_the_stage() {
        let buffer = vec![0.1f32; 32];
        let candidate = perturb(&buffer, TOLERANCE * 1.1);
        let comparison = compare(Stage::Curve, &buffer, &candidate);
        assert!(!comparison.within_tolerance);
        let err = comparison.check().expect_err("must fail");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8010");
        assert!(err
            .context
            .iter()
            .any(|(k, v)| k == "stage" && v == "curve"));
    }

    #[test]
    fn a_parity_failure_blocks_the_build() {
        let comparison = compare(Stage::Hsl, &[0.0], &[1.0]);
        let err = comparison.check().expect_err("must fail");
        assert_eq!(err.severity, aura_core::Severity::RunBlocking);
        assert_eq!(err.recovery, aura_core::Recovery::Halt);
    }

    #[test]
    fn buffers_of_different_lengths_never_pass() {
        let comparison = compare(Stage::Sharpen, &[0.1, 0.1, 0.1], &[0.1, 0.1]);
        assert!(!comparison.within_tolerance);
    }

    #[test]
    fn check_all_reports_the_first_failure() {
        let good = compare(Stage::Exposure, &[0.5], &[0.5]);
        let bad = compare(Stage::Vibrance, &[0.5], &[0.9]);
        let err = check_all(&[good, bad]).expect_err("must fail");
        assert!(err
            .context
            .iter()
            .any(|(k, v)| k == "stage" && v == "vibrance"));
    }

    #[test]
    fn the_tolerance_is_one_part_in_1024() {
        // Spelled out so that a change to it is a change to a number somebody has to defend
        // in a review rather than a constant that quietly widened.
        assert!((TOLERANCE - 0.000_976_562_5).abs() < f32::EPSILON);
    }
}
