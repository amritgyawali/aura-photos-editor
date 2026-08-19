//! The adaptive tone curve: control points solved to hit a subject-contrast target, under
//! three constraints, and monotone by construction.
//!
//! PHASE-16 section 6.1:
//!
//! > Fit the curve as a monotonic cubic spline (PCHIP) through control points solved so that
//! > (a) subject mid-tone contrast hits the scene intent, (b) shadow lift stays within the
//! > noise budget, (c) highlight roll-off preserves the dress texture.
//!
//! ## Why the pivot is the subject and not middle grey
//!
//! Every stock tone curve pivots at 0.5. That is right for a photograph whose subject is at
//! 0.5 and wrong for every other one: a curve pivoted at middle grey applied to a bride at
//! 0.35 darkens her while adding the contrast it was asked for, and the frame comes back
//! looking underexposed by the phase that just exposed it correctly. So the pivot here is
//! [`Constraints::subject_luma`] - phase 15's own anchor, read through the frozen service -
//! and the whole curve is built around it.
//!
//! ## Why the gain is iterated rather than computed
//!
//! The relationship between the slope at the pivot and the *achieved* interquartile spread
//! of the subject is not linear, because the curve is blended back toward the identity near
//! both endpoints and because the constraints can flatten a node. So the fit is a short
//! fixed-point loop: propose a gain, build the curve, measure what the subject's spread
//! actually became through it, and correct. [`FIT_ITERATIONS`] is eight, which is far more
//! than convergence needs and cheap enough not to argue about - the whole fit is a few dozen
//! evaluations of a five-node spline.
//!
//! It is deterministic: the same constraints give the same control points, byte for byte,
//! which is invariant 4 for this module.
//!
//! ## Monotonicity, and where the guarantee actually lives
//!
//! Nothing here checks the *rendered* curve for monotonicity, because nothing needs to. The
//! nodes are repaired to be non-decreasing before they are quantised, `ToneCurve::new`
//! refuses anything that is not, and `aura_raw::colour::curve::pchip` is monotone whenever
//! its control points are. Three links, and the weakest of them is a type. ADR-0033 decision
//! 2.

use aura_core::contract::colour::{CurvePoint, ToneCurve, CURVE_MAX_POINTS};
use aura_raw::colour::curve::{monotone_tangents, pchip};

/// How many times the gain is corrected.
pub const FIT_ITERATIONS: usize = 8;

/// The largest slope the curve will take at the pivot.
///
/// 1.8. Beyond it the curve stops being a tone adjustment and becomes a look, and the
/// shoulder it forces is visible as a hard edge on a face.
pub const MAX_GAIN: f32 = 1.8;

/// The smallest slope the curve will take at the pivot.
///
/// 0.6. Below it a frame is being flattened toward grey, which is a phase 22 restoration
/// decision rather than a phase 16 grading one.
pub const MIN_GAIN: f32 = 0.6;

/// How far from each endpoint the curve is blended back to the identity.
///
/// Fifteen percent. Without it a curve with any gain at all moves the black and white points,
/// which the `blacks` and `whites` parameters already own - and two things moving the same
/// endpoint is how a grade ends up twice as strong as either of them asked for.
pub const ENDPOINT_BLEND: f32 = 0.15;

/// The fewest levels two control points may be apart after quantisation.
///
/// Eight of 255. Closer than that and the two points describe the same place, which produces
/// a curve with a near-vertical segment - technically monotone, visibly a step.
pub const MIN_NODE_GAP: u16 = 8;

/// Where the subject's own band is measured, either side of the pivot.
///
/// Section 6.1's "mid-tone contrast" made specific: the spread between the quartiles of the
/// subject region, which for a face is the difference between its lit and shadowed halves.
pub const SUBJECT_HALF_WIDTH: f32 = 0.18;

/// The highest output level an interior control point may ever reach.
///
/// 0.985, whatever the frame contains. A node at 1.0 is a node that has flattened everything
/// above it into white - which is a posterised band and new clipping in one move, and is the
/// single failure the section 10.1 property test would find first.
pub const ABSOLUTE_CEILING: f32 = 0.985;

/// The lowest output level an interior control point may ever reach.
///
/// The mirror of [`ABSOLUTE_CEILING`], and it matters for the same reason: a steep curve
/// pivoted at 0.45 drives its shadow nodes below zero long before its shoulder reaches one,
/// and a node clamped at zero is a crushed band.
pub const ABSOLUTE_FLOOR: f32 = 0.004;

/// What the fit has to satisfy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraints {
    /// The subject's median luminance, `0..1`. The pivot.
    pub subject_luma: f32,
    /// The subject's measured interquartile spread, `0..1`.
    pub measured_spread: f32,
    /// The spread this scene wants, `0..1`.
    pub target_spread: f32,
    /// The most the curve may lift a shadow node, in luminance units.
    ///
    /// Constraint (b). Derived from phase 09's shadow headroom by the caller, so this module
    /// never forms its own opinion about what a lift costs.
    pub shadow_lift_allow: f32,
    /// The highest output level the shoulder may reach, `0..1`.
    ///
    /// Constraint (c). Below 1.0 whenever the frame has a bright near-neutral region - a
    /// dress, a cake, a tablecloth - because a shoulder that reaches white is a shoulder that
    /// has flattened the texture out of it.
    pub highlight_ceiling: f32,
}

impl Default for Constraints {
    fn default() -> Self {
        Self {
            subject_luma: 0.5,
            measured_spread: 0.25,
            target_spread: 0.25,
            shadow_lift_allow: 0.10,
            highlight_ceiling: 1.0,
        }
    }
}

/// What the fit produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Fitted {
    /// The curve. Monotone by construction.
    pub curve: ToneCurve,
    /// The subject spread the curve actually achieves.
    pub achieved_spread: f32,
    /// The slope it settled on at the pivot.
    pub gain: f32,
    /// True when a constraint stopped the curve reaching its target.
    ///
    /// What `ColourCode::CurveFlattenedForClipping` is emitted for, and the honest half of
    /// the fit: a curve that hit its highlight ceiling has not done what the scene asked and
    /// saying so is cheaper than a photographer wondering why the frame is flat.
    pub constrained: bool,
}

impl Fitted {
    /// The identity, for a frame that needs no curve.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            curve: ToneCurve::identity(),
            achieved_spread: 0.0,
            gain: 1.0,
            constrained: false,
        }
    }
}

/// Fit a curve to the constraints.
///
/// Returns the identity when the subject already has the separation the scene wants, which
/// is the common case on a well-exposed frame and is a real answer rather than a failure to
/// find one.
#[must_use]
pub fn fit(constraints: &Constraints) -> Fitted {
    let measured = constraints.measured_spread.clamp(0.01, 1.0);
    let target = constraints.target_spread.clamp(0.01, 1.0);

    // Within a twentieth of the target is within the noise of the measurement itself. A curve
    // fitted to close a gap that small is a curve nobody asked for.
    if (target - measured).abs() < 0.02 {
        return Fitted::identity();
    }

    let mut gain = (target / measured).clamp(MIN_GAIN, MAX_GAIN);
    let mut best = Fitted::identity();

    for _ in 0..FIT_ITERATIONS {
        let (nodes, constrained, bounded) = nodes_for(constraints, gain);
        let (xs, ys): (Vec<f32>, Vec<f32>) = nodes.iter().copied().unzip();
        let tangents = monotone_tangents(&xs, &ys);
        let achieved = achieved_spread(constraints, &xs, &ys, &tangents);

        best = Fitted {
            curve: quantise(&nodes),
            achieved_spread: achieved,
            // The gain the nodes were actually built with, which is what a reader of this
            // struct wants: a fit that asked for 1.8 and was held to 1.38 by the shadow
            // floor did the second thing, and reporting the first would make `constrained`
            // the only clue.
            gain: bounded,
            constrained,
        };

        if (achieved - target).abs() < 0.005 {
            break;
        }
        // The correction. Multiplicative rather than additive because the relationship is a
        // ratio, and damped by a half so a constrained curve - one whose achieved spread
        // cannot reach the target at any gain - converges on its ceiling rather than
        // oscillating against it.
        let ratio = (target / achieved.max(0.01)).clamp(0.5, 2.0);
        let corrected = gain * (1.0 + (ratio - 1.0) * 0.5);
        let next = corrected.clamp(MIN_GAIN, MAX_GAIN);
        if (next - gain).abs() < 1e-4 {
            break;
        }
        gain = next;
    }

    // A fit that ended up on the identity is the identity, said structurally rather than
    // numerically: `is_identity` is a question about the points and this is where the points
    // are decided.
    if best.curve.is_identity() {
        return Fitted {
            achieved_spread: measured,
            ..Fitted::identity()
        };
    }
    best
}

/// The control nodes for one gain, in `0..1`, with the three constraints applied.
///
/// Seven candidate positions: the two endpoints, a toe, the subject's two quartile points, a
/// shoulder, and the pivot itself. Positions that collide after quantisation are dropped by
/// [`quantise`], so a subject at 0.9 produces a four-node curve rather than a broken one.
///
/// **The constraints bound the gain rather than clamping the nodes**, and that is the
/// difference between a curve and a bug. Clamping a node that wanted to sit at 1.08 down to
/// 1.0 produces a flat top - every input above that node maps to white - which is
/// simultaneously a posterised band, new clipping and a violation of the phase's own property
/// test. Solving for the interval of gains that keeps every node inside its bounds produces a
/// gentler curve that is still a curve.
///
/// Each node gives one linear inequality, because `y(g) = x + lever * (g - 1)` where
/// `lever = (x - pivot) * blend(x)`. Three of them per node - the absolute ceiling, the
/// absolute floor and, below the pivot, phase 09's shadow-lift allowance - and the feasible
/// set is their intersection. `g = 1` always satisfies every one of them, so the interval is
/// never empty.
fn nodes_for(constraints: &Constraints, gain: f32) -> (Vec<(f32, f32)>, bool, f32) {
    let pivot = constraints.subject_luma.clamp(0.05, 0.95);
    let ceiling = constraints.highlight_ceiling.clamp(0.4, ABSOLUTE_CEILING);
    let floor = ABSOLUTE_FLOOR;
    let allow = constraints.shadow_lift_allow.max(0.0);

    let positions = [
        0.0,
        (pivot - 0.34).clamp(0.05, 0.45),
        (pivot - SUBJECT_HALF_WIDTH).clamp(0.08, 0.48),
        pivot,
        (pivot + SUBJECT_HALF_WIDTH).clamp(0.52, 0.92),
        (pivot + 0.34).clamp(0.55, 0.95),
        1.0,
    ];

    let mut lowest = MIN_GAIN;
    let mut highest = MAX_GAIN;
    for x in positions {
        if x <= 0.0 || x >= 1.0 {
            continue;
        }
        let lever = (x - pivot) * blend_weight(x);
        if lever.abs() < 1e-6 {
            continue;
        }
        // `lever * (g - 1) <= limit - x` and `>= floor - x`, with the inequality flipping
        // when the lever is negative.
        let at = |limit: f32| (limit - x) / lever + 1.0;
        if lever > 0.0 {
            highest = highest.min(at(ceiling));
            lowest = lowest.max(at(floor));
        } else {
            highest = highest.min(at(floor));
            lowest = lowest.max(at(ceiling));
        }
        if x < pivot {
            // Constraint (b). Below the pivot a gain under one *raises* the node, and this is
            // the only place a curve can lift a shadow at all. `lever` is negative there, so
            // the allowance is a lower bound on the gain.
            lowest = lowest.max(allow / lever + 1.0);
        }
    }
    // `g = 1` is feasible for every inequality above, so the interval always contains it -
    // but rounding can invert the two by a hair on a degenerate pivot, and a clamp with an
    // inverted range is a silent wrong answer rather than a loud one.
    if lowest > highest {
        lowest = 1.0;
        highest = 1.0;
    }
    let bounded = gain.clamp(lowest, highest).clamp(MIN_GAIN, MAX_GAIN);
    let constrained = (bounded - gain).abs() > 1e-4;

    let mut nodes: Vec<(f32, f32)> = Vec::with_capacity(positions.len());
    for x in positions {
        let raw = pivot + (x - pivot) * bounded;
        let weight = blend_weight(x);
        let y = x + (raw - x) * weight;
        nodes.push((x, y.clamp(0.0, 1.0)));
    }

    // The endpoints are anchors, always. Black stays black and white stays white.
    if let Some(first) = nodes.first_mut() {
        first.1 = 0.0;
    }
    if let Some(last) = nodes.last_mut() {
        last.1 = 1.0;
    }

    // Monotone repair, before quantisation. A running maximum is the cheapest repair that is
    // also the right one: it never moves a node down. With the gain bounded above it has
    // almost nothing left to do, which is the point.
    let mut running = 0.0_f32;
    for node in &mut nodes {
        node.1 = node.1.max(running);
        running = node.1;
    }
    (nodes, constrained, bounded)
}

/// How much of the linear response applies at `x`.
///
/// One in the middle, falling to zero at both endpoints over [`ENDPOINT_BLEND`]. Smooth
/// rather than linear so the curve has no slope discontinuity where the blend ends - a
/// discontinuity there shows as a faint band in a sky.
fn blend_weight(x: f32) -> f32 {
    let from_low = (x / ENDPOINT_BLEND).clamp(0.0, 1.0);
    let from_high = ((1.0 - x) / ENDPOINT_BLEND).clamp(0.0, 1.0);
    let raw = from_low.min(from_high);
    raw * raw * (3.0 - 2.0 * raw)
}

/// What the subject's spread becomes through a candidate curve.
fn achieved_spread(constraints: &Constraints, xs: &[f32], ys: &[f32], tangents: &[f32]) -> f32 {
    let half = constraints.measured_spread.clamp(0.01, 1.0) / 2.0;
    let low = (constraints.subject_luma - half).clamp(0.0, 1.0);
    let high = (constraints.subject_luma + half).clamp(0.0, 1.0);
    (pchip(xs, ys, tangents, high) - pchip(xs, ys, tangents, low)).max(0.0)
}

/// Quantise the nodes to the recipe's 0-255 levels and build the curve.
///
/// Nodes closer than [`MIN_NODE_GAP`] are dropped rather than merged, keeping the endpoints,
/// which is what makes a subject at 0.92 produce a short curve instead of a refused one. A
/// set that still will not pass `ToneCurve::new` produces the identity - the refusal is
/// `AURA-ML-5066` and the caller records the withdrawal.
fn quantise(nodes: &[(f32, f32)]) -> ToneCurve {
    let mut points: Vec<CurvePoint> = Vec::with_capacity(nodes.len());
    for (x, y) in nodes {
        let level = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u16;
        let candidate = CurvePoint::new(level(*x), level(*y));
        match points.last() {
            Some(previous) if candidate.x < previous.x.saturating_add(MIN_NODE_GAP) => {}
            _ => points.push(candidate),
        }
    }
    // The last node is the white point and it is not optional. If the gap rule dropped it,
    // it replaces whatever was kept in its place.
    match points.last() {
        Some(last) if last.x == ToneCurve::MAX_LEVEL => {}
        Some(last) if last.x + MIN_NODE_GAP > ToneCurve::MAX_LEVEL => {
            points.pop();
            points.push(CurvePoint::new(ToneCurve::MAX_LEVEL, ToneCurve::MAX_LEVEL));
        }
        _ => points.push(CurvePoint::new(ToneCurve::MAX_LEVEL, ToneCurve::MAX_LEVEL)),
    }
    // Non-decreasing in y, after rounding. Rounding two adjacent nodes can invert them by one
    // level, which is enough for `ToneCurve::new` to refuse the whole curve.
    let mut ceiling = 0u16;
    for point in &mut points {
        point.y = point.y.max(ceiling);
        ceiling = point.y;
    }
    points.truncate(CURVE_MAX_POINTS);
    if let Some(last) = points.last_mut() {
        last.x = ToneCurve::MAX_LEVEL;
        last.y = ToneCurve::MAX_LEVEL;
    }
    ToneCurve::new(points).unwrap_or_else(|_| ToneCurve::identity())
}

/// Evaluate a fitted curve at one input level, `0..1`.
///
/// For the guards and the eval harness, which need to know what a candidate curve does to a
/// highlight before it is committed. It calls the same `pchip` the renderer does, from the
/// same crate, so there is no second answer to what a curve does.
#[must_use]
pub fn evaluate(curve: &ToneCurve, x: f32) -> f32 {
    let xs: Vec<f32> = curve
        .points()
        .iter()
        .map(|point| f32::from(point.x) / 255.0)
        .collect();
    let ys: Vec<f32> = curve
        .points()
        .iter()
        .map(|point| f32::from(point.y) / 255.0)
        .collect();
    if xs.len() < 2 {
        return x;
    }
    let tangents = monotone_tangents(&xs, &ys);
    pchip(&xs, &ys, &tangents, x.clamp(0.0, 1.0))
}

/// A curve scaled toward the identity, for the variants and the subtlety cap.
///
/// Every node is moved a fraction of the way back to `y = x`. Monotonicity survives it,
/// because a convex combination of two non-decreasing sequences is non-decreasing - which is
/// why scaling is the operation the subtlety cap uses rather than re-fitting at a lower gain.
#[must_use]
pub fn scaled(curve: &ToneCurve, factor: f32) -> ToneCurve {
    let factor = factor.clamp(0.0, 1.0);
    let points: Vec<CurvePoint> = curve
        .points()
        .iter()
        .map(|point| {
            let x = f32::from(point.x);
            let y = f32::from(point.y);
            CurvePoint::new(
                point.x,
                (x + (y - x) * factor).round().clamp(0.0, 255.0) as u16,
            )
        })
        .collect();
    ToneCurve::new(points).unwrap_or_else(|_| ToneCurve::identity())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_subject() -> Constraints {
        Constraints {
            subject_luma: 0.45,
            measured_spread: 0.16,
            target_spread: 0.30,
            shadow_lift_allow: 0.12,
            highlight_ceiling: 1.0,
        }
    }

    #[test]
    fn a_fitted_curve_moves_the_subject_toward_its_target() {
        // Toward, not necessarily all the way to. A subject at a 16 % spread cannot be taken
        // to 30 % without a gain past `MAX_GAIN` or a node past the absolute floor, and the
        // fit is required to get most of the way and to say that it stopped.
        let fitted = fit(&flat_subject());
        assert!(!fitted.curve.is_identity());
        assert!(
            fitted.achieved_spread > 0.16,
            "the curve did nothing: {fitted:?}"
        );
        let closed = (fitted.achieved_spread - 0.16) / (0.30 - 0.16);
        assert!(
            closed >= 0.40,
            "the fit closed only {closed:.2} of the gap: {fitted:?}"
        );
        assert!(
            fitted.achieved_spread <= 0.30 + 0.06,
            "the fit overshot: {fitted:?}"
        );
    }

    #[test]
    fn a_subject_already_at_its_target_gets_no_curve() {
        let fitted = fit(&Constraints {
            measured_spread: 0.28,
            target_spread: 0.28,
            ..flat_subject()
        });
        assert!(fitted.curve.is_identity());
    }

    #[test]
    fn every_fitted_curve_is_monotone_over_a_four_thousand_step_ramp() {
        // Section 10.1's property test, run over the whole parameter space this module can
        // reach rather than over one example. A single step backwards anywhere is a
        // posterised band in a delivered photograph.
        for luma in [0.12_f32, 0.30, 0.45, 0.62, 0.88] {
            for measured in [0.05_f32, 0.15, 0.30, 0.50] {
                for target in [0.08_f32, 0.20, 0.35, 0.55] {
                    for allow in [0.0_f32, 0.06, 0.20] {
                        for ceiling in [0.80_f32, 0.94, 1.0] {
                            let fitted = fit(&Constraints {
                                subject_luma: luma,
                                measured_spread: measured,
                                target_spread: target,
                                shadow_lift_allow: allow,
                                highlight_ceiling: ceiling,
                            });
                            let mut previous = f32::NEG_INFINITY;
                            for step in 0..=4096 {
                                let x = step as f32 / 4096.0;
                                let y = evaluate(&fitted.curve, x);
                                assert!(
                                    y >= previous - 1e-5,
                                    "curve went backwards at x = {x} for {fitted:?}"
                                );
                                previous = y;
                            }
                            for pair in fitted.curve.points().windows(2) {
                                let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
                                    continue;
                                };
                                assert!(b.x > a.x && b.y >= a.y, "{fitted:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_noise_budget_stops_the_curve_lifting_shadows() {
        // Constraint (b). With no headroom at all the curve may not raise a shadow node above
        // the identity, whatever the target asks for.
        let fitted = fit(&Constraints {
            subject_luma: 0.30,
            measured_spread: 0.40,
            target_spread: 0.12,
            shadow_lift_allow: 0.0,
            ..flat_subject()
        });
        for point in fitted.curve.points() {
            if f32::from(point.x) / 255.0 < 0.30 {
                assert!(
                    point.y <= point.x,
                    "a shadow node was lifted with no headroom: {fitted:?}"
                );
            }
        }
        assert!(fitted.constrained || fitted.curve.is_identity());
    }

    #[test]
    fn the_highlight_ceiling_stops_the_shoulder_reaching_white() {
        // Constraint (c). A dress in the frame lowers the ceiling; no interior node may pass
        // it, which is what keeps the texture.
        let fitted = fit(&Constraints {
            subject_luma: 0.55,
            measured_spread: 0.10,
            target_spread: 0.40,
            highlight_ceiling: 0.86,
            ..flat_subject()
        });
        for point in fitted.curve.points() {
            if point.x < 255 {
                assert!(
                    f32::from(point.y) / 255.0 <= 0.861,
                    "a node passed the ceiling: {fitted:?}"
                );
            }
        }
    }

    #[test]
    fn the_endpoints_are_never_moved() {
        // The blacks and whites parameters own the endpoints. A curve that moved them would
        // double every endpoint decision.
        let fitted = fit(&flat_subject());
        assert_eq!(
            fitted.curve.points().first().map(|p| (p.x, p.y)),
            Some((0, 0))
        );
        assert_eq!(
            fitted.curve.points().last().map(|p| (p.x, p.y)),
            Some((255, 255))
        );
    }

    #[test]
    fn the_fit_is_deterministic() {
        let a = fit(&flat_subject());
        let b = fit(&flat_subject());
        assert_eq!(a, b);
    }

    #[test]
    fn a_subject_at_the_top_of_the_range_produces_a_short_curve_not_a_broken_one() {
        let fitted = fit(&Constraints {
            subject_luma: 0.93,
            measured_spread: 0.06,
            target_spread: 0.30,
            ..flat_subject()
        });
        assert!(fitted.curve.points().len() >= 2);
        assert!(fitted.curve.points().len() <= CURVE_MAX_POINTS);
    }

    #[test]
    fn scaling_a_curve_keeps_it_monotone() {
        let fitted = fit(&flat_subject());
        for factor in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let scaled = scaled(&fitted.curve, factor);
            let mut previous = f32::NEG_INFINITY;
            for step in 0..=1024 {
                let x = step as f32 / 1024.0;
                let y = evaluate(&scaled, x);
                assert!(y >= previous - 1e-5, "scaled curve went backwards at {x}");
                previous = y;
            }
        }
    }
}
