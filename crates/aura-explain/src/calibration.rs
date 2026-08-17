//! Turning a number the deciding code believed into a number that means something.
//!
//! Section 6.1, and it is a safety mechanism rather than a nicety. The autonomy bands of
//! section 6.4 - 0.98 and above acts, 0.90 to 0.98 acts only in Zero-Touch, 0.75 to 0.90
//! goes in the review queue - are defensible only if 90 % confidence really is 90 % correct.
//! Everything in this module exists to make that sentence checkable.
//!
//! ## What is fitted, and on what
//!
//! One model per `(kind, source)` pair. Per kind because a culling decision and an export
//! decision are wrong in different proportions; per source because section 6.1 says so
//! outright - "cloud-sourced decisions are calibrated separately from local ones, because
//! their error profile differs" - and because a cloud model's failure is a schema retry
//! while a local model's failure is a placeholder head.
//!
//! The training pairs are `(raw_confidence, was it right)`. "Was it right" comes from two
//! places and neither of them is a label somebody typed: a labelled fixture with a known
//! answer, and a photographer's own override, which is the strongest outcome signal this
//! product will ever have. Phase 30 closes that loop.
//!
//! ## Isotonic regression, by pool-adjacent-violators
//!
//! [`Calibrator::fit_isotonic`] is the standard PAVA: sort by raw confidence, then merge
//! adjacent blocks while any block's mean exceeds the next one's, which yields the
//! monotone step function that minimises squared error. Monotone matters more than the
//! squared error does - a calibration that could map 0.9 below 0.8 would reorder the review
//! queue, and a photographer watching an item move *up* the queue after an upgrade has been
//! given a reason to distrust the whole panel.
//!
//! Temperature scaling is the alternative section 6.1 allows "where monotonic and
//! data-poor". It is one parameter instead of *n* steps and it cannot overfit forty
//! samples into a staircase. [`Calibrator::fit_temperature`] is a bounded scan rather than
//! a gradient descent, because the objective is one-dimensional, bounded and cheap - and a
//! scan has no learning rate for anybody to get wrong.
//!
//! ## Expected calibration error, and the honest thing about it
//!
//! [`Reliability::ece`] is the standard binned estimator: split `0..1` into ten bins by
//! predicted confidence, and take the weighted mean of |accuracy - confidence| per bin.
//! Section 6.1 fails CI above 0.05 for any decision type with 500 samples or more.
//!
//! **Everything this build measures it on is synthetic.** The estimator is real and tested;
//! the outcomes it is measured against are authored. That is condition C2 of the phase 13
//! exit report, and no number produced here is a claim about how often AURA is right.

use std::collections::BTreeMap;

use aura_core::contract::ledger::{DecisionKind, DecisionSource, LedgerDecision};

/// How many bins the expected-calibration-error estimator uses.
///
/// Ten, which is the convention the literature and every reliability diagram a reviewer has
/// seen use. Not tunable: an ECE compared across two bin counts is two different numbers
/// wearing one name, and this one is a CI gate.
pub const ECE_BINS: usize = 10;

/// The sample count above which section 6.1's ECE gate applies.
pub const ECE_GATE_SAMPLES: usize = 500;

/// The threshold section 6.1 fails CI at.
pub const ECE_GATE: f32 = 0.05;

/// One `(what the code believed, was it right)` pair.
///
/// A bool rather than a score, because calibration is about *correctness* and a continuous
/// "how right" would be a second opinion with its own calibration problem.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// What the deciding code believed at the time, `0..1`.
    pub raw: f32,
    /// Whether the decision turned out to be right.
    pub correct: bool,
}

impl Outcome {
    /// One labelled pair.
    #[must_use]
    pub fn new(raw: f32, correct: bool) -> Self {
        Self {
            raw: raw.clamp(0.0, 1.0),
            correct,
        }
    }
}

/// How a calibration was arrived at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// No fit. `calibrated == raw`, and `version` is zero.
    #[default]
    Identity,
    /// A monotone step function fitted by pool-adjacent-violators.
    Isotonic,
    /// One scalar dividing the log-odds. Section 6.1's data-poor path.
    Temperature,
}

impl Method {
    /// The stable slug, as migration 13 stores it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Isotonic => "isotonic",
            Self::Temperature => "temperature",
        }
    }

    /// Parse the stored slug. Unknown text is [`Method::Identity`].
    ///
    /// The direction that claims least: an unreadable method read as a fitted one would let
    /// an unfitted map be reported as calibrated.
    #[must_use]
    pub fn from_str_or_identity(text: &str) -> Self {
        match text {
            "isotonic" => Self::Isotonic,
            "temperature" => Self::Temperature,
            _ => Self::Identity,
        }
    }
}

/// How well a set of predictions was calibrated.
///
/// Carried beside the model rather than computed on demand, because the numbers a CI gate
/// asserts and the numbers a reliability diagram is drawn from must be the same numbers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reliability {
    /// Expected calibration error, `0..1`. Lower is better; section 6.1 gates at 0.05.
    pub ece: f32,
    /// Brier score, `0..1`. The mean squared error of the probability itself.
    ///
    /// Reported beside ECE because they fail differently: a model that always says 0.5 has a
    /// poor Brier score and can have an excellent ECE, and a product that gated only on ECE
    /// would ship it.
    pub brier: f32,
    /// How many outcomes it was measured on.
    pub samples: usize,
    /// Per bin: `(mean predicted confidence, observed accuracy, count)`.
    ///
    /// What `ml/eval/calibration_report.py` draws the reliability diagram from, and what a
    /// human reads to see *where* a model is overconfident rather than only that it is.
    pub bins: Vec<(f32, f32, usize)>,
}

impl Reliability {
    /// Measure a set of outcomes.
    #[must_use]
    pub fn measure(outcomes: &[Outcome]) -> Self {
        if outcomes.is_empty() {
            return Self::default();
        }
        let mut sums = vec![(0.0f64, 0usize, 0usize); ECE_BINS];
        let mut brier = 0.0f64;
        for outcome in outcomes {
            let actual = f64::from(u8::from(outcome.correct));
            let predicted = f64::from(outcome.raw);
            brier += (predicted - actual) * (predicted - actual);
            let bin = bin_of(outcome.raw);
            if let Some(slot) = sums.get_mut(bin) {
                slot.0 += predicted;
                slot.1 += usize::from(outcome.correct);
                slot.2 += 1;
            }
        }

        let total = outcomes.len();
        let mut ece = 0.0f64;
        let mut bins = Vec::with_capacity(ECE_BINS);
        for (sum_conf, hits, count) in sums {
            if count == 0 {
                continue;
            }
            let mean_conf = sum_conf / count as f64;
            let accuracy = hits as f64 / count as f64;
            ece += (count as f64 / total as f64) * (accuracy - mean_conf).abs();
            bins.push((mean_conf as f32, accuracy as f32, count));
        }

        Self {
            ece: ece as f32,
            brier: (brier / total as f64) as f32,
            samples: total,
            bins,
        }
    }

    /// True when this passes section 6.1's gate.
    ///
    /// A model with fewer than [`ECE_GATE_SAMPLES`] outcomes passes trivially, and section
    /// 6.1 says so: an ECE over forty samples is an estimate with a confidence interval
    /// wider than the threshold, and gating on it would fail builds at random.
    #[must_use]
    pub fn passes_gate(&self) -> bool {
        self.samples < ECE_GATE_SAMPLES || self.ece <= ECE_GATE
    }
}

/// Which bin a confidence falls in.
fn bin_of(confidence: f32) -> usize {
    let scaled = (confidence.clamp(0.0, 1.0) * ECE_BINS as f32) as usize;
    scaled.min(ECE_BINS.saturating_sub(1))
}

/// One fitted mapping from a raw confidence to a calibrated one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Calibrator {
    /// How it was fitted.
    pub method: Method,
    /// The version. `0` is the identity map and nothing else may be zero.
    pub version: u16,
    /// The step function, ascending in `raw`. Empty for the identity map and for
    /// temperature scaling.
    pub knots: Vec<(f32, f32)>,
    /// The scalar, for temperature scaling. `1.0` elsewhere.
    pub temperature: f32,
    /// How the fit came out.
    pub reliability: Reliability,
}

impl Calibrator {
    /// The map that changes nothing, which is what ships.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            method: Method::Identity,
            version: LedgerDecision::UNCALIBRATED,
            knots: Vec::new(),
            temperature: 1.0,
            reliability: Reliability::default(),
        }
    }

    /// True when this is a fitted map rather than the identity.
    #[must_use]
    pub const fn is_fitted(&self) -> bool {
        self.version != LedgerDecision::UNCALIBRATED
    }

    /// What a raw confidence is worth.
    ///
    /// Linear interpolation between the two surrounding knots rather than a step, because a
    /// step function makes the panel's confidence jump when a score moves by a thousandth,
    /// and a photographer watching a badge flip between two values while nothing changes has
    /// been shown a bug rather than a calibration.
    #[must_use]
    pub fn apply(&self, raw: f32) -> f32 {
        let raw = raw.clamp(0.0, 1.0);
        match self.method {
            Method::Identity => raw,
            Method::Temperature => {
                let temperature = if self.temperature.abs() < f32::EPSILON {
                    1.0
                } else {
                    self.temperature
                };
                sigmoid(logit(raw) / temperature)
            }
            Method::Isotonic => self.interpolate(raw),
        }
    }

    fn interpolate(&self, raw: f32) -> f32 {
        let Some(first) = self.knots.first() else {
            return raw;
        };
        if raw <= first.0 {
            return first.1.clamp(0.0, 1.0);
        }
        let Some(last) = self.knots.last() else {
            return raw;
        };
        if raw >= last.0 {
            return last.1.clamp(0.0, 1.0);
        }
        for pair in self.knots.windows(2) {
            let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if raw >= left.0 && raw <= right.0 {
                let span = right.0 - left.0;
                if span.abs() < f32::EPSILON {
                    return right.1.clamp(0.0, 1.0);
                }
                let t = (raw - left.0) / span;
                return (left.1 + t * (right.1 - left.1)).clamp(0.0, 1.0);
            }
        }
        raw
    }

    /// Fit a monotone step function by pool-adjacent-violators.
    ///
    /// Returns the identity map when there is nothing to fit: fewer than two outcomes, or
    /// outcomes that are all the same. Both cases would produce a "calibration" that is a
    /// constant, and a constant confidence is worse than an uncalibrated one because it
    /// destroys the ordering the review queue is sorted by.
    #[must_use]
    pub fn fit_isotonic(outcomes: &[Outcome], version: u16) -> Self {
        let reliability = Reliability::measure(outcomes);
        if outcomes.len() < 2 || version == LedgerDecision::UNCALIBRATED {
            return Self::identity();
        }
        let mut sorted: Vec<Outcome> = outcomes.to_vec();
        sorted.sort_by(|left, right| left.raw.total_cmp(&right.raw));

        // Each block is (sum of outcomes, count, mean raw confidence * count).
        let mut blocks: Vec<(f64, usize, f64)> = Vec::with_capacity(sorted.len());
        for outcome in &sorted {
            blocks.push((
                f64::from(u8::from(outcome.correct)),
                1,
                f64::from(outcome.raw),
            ));
            // Pool while the previous block's mean exceeds this one's - the violation PAVA
            // is named for.
            while blocks.len() >= 2 {
                let (Some(right), Some(left)) = (
                    blocks.last().copied(),
                    blocks.len().checked_sub(2).and_then(|ix| blocks.get(ix)),
                ) else {
                    break;
                };
                let left_mean = left.0 / left.1 as f64;
                let right_mean = right.0 / right.1 as f64;
                if left_mean <= right_mean {
                    break;
                }
                let merged = (left.0 + right.0, left.1 + right.1, left.2 + right.2);
                blocks.pop();
                blocks.pop();
                blocks.push(merged);
            }
        }

        let knots: Vec<(f32, f32)> = blocks
            .iter()
            .map(|(sum, count, raw_sum)| {
                let denominator = *count as f64;
                (
                    (raw_sum / denominator) as f32,
                    (sum / denominator).clamp(0.0, 1.0) as f32,
                )
            })
            .collect();

        if knots.len() < 2 {
            return Self::identity();
        }
        Self {
            method: Method::Isotonic,
            version,
            knots,
            temperature: 1.0,
            reliability,
        }
    }

    /// Fit one temperature by a bounded scan over the log-loss.
    ///
    /// Section 6.1's data-poor path. The scan runs from 0.5 to 5.0 in a hundred steps,
    /// which is finer than the difference any panel can show and coarse enough to be
    /// instant; a bounded scan has no learning rate, no initialisation and no way to
    /// diverge, and the objective is one-dimensional and convex in practice.
    #[must_use]
    pub fn fit_temperature(outcomes: &[Outcome], version: u16) -> Self {
        let reliability = Reliability::measure(outcomes);
        if outcomes.is_empty() || version == LedgerDecision::UNCALIBRATED {
            return Self::identity();
        }
        let mut best = (1.0f32, f64::MAX);
        for step in 0..=100u32 {
            let temperature = 0.5 + f32::from(u16::try_from(step).unwrap_or(0)) * 0.045;
            let mut loss = 0.0f64;
            for outcome in outcomes {
                let mapped = sigmoid(logit(outcome.raw) / temperature).clamp(1e-6, 1.0 - 1e-6);
                let probability = if outcome.correct {
                    f64::from(mapped)
                } else {
                    1.0 - f64::from(mapped)
                };
                loss -= probability.ln();
            }
            if loss < best.1 {
                best = (temperature, loss);
            }
        }
        Self {
            method: Method::Temperature,
            version,
            knots: Vec::new(),
            temperature: best.0,
            reliability,
        }
    }
}

/// Every calibration model this build has, keyed by `(kind, source)`.
///
/// The set is what [`crate::api::Explain`] consults on every recorded decision, so the
/// lookup is a `BTreeMap` and the miss is the identity map rather than an error: a decision
/// kind nobody has fitted yet is uncalibrated, which is a fact reported by
/// `AURA-ML-5058`, not a failure.
#[derive(Debug, Clone, Default)]
pub struct CalibrationSet {
    models: BTreeMap<(DecisionKind, DecisionSource), Calibrator>,
}

impl CalibrationSet {
    /// The set this build ships: the identity map for every pair.
    ///
    /// Every one of them, at version 0, and that is the honest state of this phase. Fitting
    /// needs labelled outcomes from real weddings; the fitter above is real, tested and
    /// measured, and it has nothing to fit on.
    #[must_use]
    pub fn shipped() -> Self {
        Self::default()
    }

    /// Add or replace one model.
    pub fn insert(
        &mut self,
        kind: DecisionKind,
        source: DecisionSource,
        calibrator: Calibrator,
    ) -> &mut Self {
        self.models.insert((kind, source), calibrator);
        self
    }

    /// The model for one pair, or the identity map.
    #[must_use]
    pub fn get(&self, kind: DecisionKind, source: DecisionSource) -> Calibrator {
        self.models
            .get(&(kind, source))
            .cloned()
            .unwrap_or_else(Calibrator::identity)
    }

    /// What a raw confidence is worth, and which version said so.
    #[must_use]
    pub fn apply(&self, kind: DecisionKind, source: DecisionSource, raw: f32) -> (f32, u16) {
        let model = self.get(kind, source);
        (model.apply(raw), model.version)
    }

    /// Every pair that has no fitted model, as slugs, for `AURA-ML-5058`.
    ///
    /// Only the calibratable sources: a photographer's own decision is right by definition
    /// and reporting it as uncalibrated would be reporting a category error as a gap.
    #[must_use]
    pub fn uncalibrated(&self) -> Vec<String> {
        let mut gaps = Vec::new();
        for kind in DecisionKind::ALL {
            for source in DecisionSource::ALL {
                if !source.is_calibratable() {
                    continue;
                }
                if !self.get(kind, source).is_fitted() {
                    gaps.push(format!("{}/{}", kind.as_str(), source.as_str()));
                }
            }
        }
        gaps
    }

    /// Every fitted model, for the store and the report.
    #[must_use]
    pub fn models(&self) -> Vec<(DecisionKind, DecisionSource, &Calibrator)> {
        self.models
            .iter()
            .map(|((kind, source), model)| (*kind, *source, model))
            .collect()
    }

    /// True when at least one pair is fitted.
    #[must_use]
    pub fn is_fitted(&self) -> bool {
        self.models.values().any(Calibrator::is_fitted)
    }

    /// The highest version in the set. Zero when nothing is fitted.
    #[must_use]
    pub fn version(&self) -> u16 {
        self.models
            .values()
            .map(|model| model.version)
            .max()
            .unwrap_or(LedgerDecision::UNCALIBRATED)
    }
}

/// The log-odds of a probability, clamped away from the asymptotes.
fn logit(probability: f32) -> f32 {
    let clamped = probability.clamp(1e-6, 1.0 - 1e-6);
    (clamped / (1.0 - clamped)).ln()
}

/// The inverse of [`logit`].
fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}
