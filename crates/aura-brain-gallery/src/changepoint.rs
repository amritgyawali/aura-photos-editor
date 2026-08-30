//! Where the light genuinely changed, so a node becomes two.
//!
//! Section 2.1: "detect intentional lighting transitions (venue change, sunset, flash on/off) and
//! treat them as new normalisation groups rather than errors." Section 6.2 makes it part of the
//! solver's contract.
//!
//! ## This is the module that makes the phase safe
//!
//! Section 2.1's candle-lit vow inside a bright ceremony is the whole difficulty of gallery
//! consistency in one sentence. If the vow shares a normalisation group with the ceremony there
//! are exactly two outcomes - the vow is flattened toward the ceremony, or the ceremony is dragged
//! toward the vow - and **no damping factor avoids both**. Damping makes both happen a little,
//! which is worse than either.
//!
//! So this runs **before** [`crate::anchors::select`], not after [`crate::normalise::solve`]. The
//! alternative - solve first, then notice some frames moved a long way and un-move them - was
//! rejected for the reason phase 23 rejected nudging a failed crop back inside the safety filter:
//! a correction applied after the fact leaves the frames that did *not* trip the threshold still
//! normalised toward the wrong target. ADR-0051 section 5.
//!
//! ## The signal is three channels, each in units of its own bound
//!
//! Temperature in kelvin, tint in tint units and subject luminance in `0..1` are three numbers on
//! three completely different scales, and summing them raw is summing metres and kilograms. Each
//! is divided by its own contract bound first, so a step of one full bound on any channel
//! contributes one, and the three are then commensurable. Phase 15's lesson about `u'v'` versus
//! kelvin is the same lesson: do the arithmetic in the space the distances mean something in.
//!
//! ## A step is what the node's own trend does not already explain
//!
//! This is the thing the module got wrong first, and it is worth stating plainly because the wrong
//! version passes every unit test written against a step.
//!
//! The obvious statistic is: take the robust mean either side of a candidate boundary and divide
//! the difference by the spread *within* the sides. On a flash that works. **On a slow drift it
//! also fires**, because a chapter that warms 500 K over forty frames has a tiny frame-to-frame
//! spread and a large difference between its first half and its second - which is the definition of
//! drift, and drift is the thing this whole phase exists to *normalise* rather than to split. The
//! first implementation cut a forty-frame ceremony into six nodes, each too short to be worth
//! anchoring, and reported it as six lighting changes.
//!
//! So the divisor is the **trend**: the median absolute difference between consecutive frames, times
//! the number of frames between the two runs' midpoints. Under a pure ramp that is exactly what
//! separates the two medians, so the statistic is about one. Under a step it is the size of the step
//! over the noise, which is enormous. A 500 K drift scores about 1 and a flash scores about 40.
//!
//! Phase 22's rule, in its second half: a threshold on a measurement is a statement about the
//! instrument as well as about the world, and the instrument here had a trend in it.
//!
//! ## And a node still splits when it is simply too wide to normalise
//!
//! A sunset is not a step. It is a ramp of two or three thousand kelvin, which the statistic above
//! correctly declines to call a discontinuity - and which no target can cover, because the bound is
//! 450 K and half the frames would be clamped against it whatever the target was.
//!
//! So there is a second, separate reason to split: a node whose **robust span** exceeds
//! [`MAX_COVERABLE_SPAN`] bounds is divided at the point that most equalises the two halves, until
//! each half is a range one target can actually reach. The span is measured between the tenth and
//! ninetieth percentiles rather than between the extremes, because one stray frame in a chapter is
//! an outlier for [`crate::outlier`] to report and not a reason to cut the chapter in half.
//!
//! The three transitions section 2.1 names are covered by exactly one of the two:
//!
//! * **A flash toggled on** is a step in temperature and subject luminance together. First rule.
//! * **A venue change** is a step in all three channels at once. First rule.
//! * **A sunset** is a ramp no target can cover. Second rule, and it produces several boundaries
//!   down a long golden hour, which is correct.

use aura_core::contract::gallery::{
    GalleryCode, MAX_D_CCT_K, MAX_D_EXPOSURE_EV, MAX_D_TINT, MIN_NODE_FRAMES, MIN_RUN,
};
use aura_core::contract::ids::NodeId;

use crate::stats;
use crate::tree::RawNode;

/// The smallest within-run spread the statistic is divided by.
///
/// A floor rather than a fitted minimum, and it is what stops a node whose frames are *identical*
/// from splitting on floating-point noise: with a spread of zero the ratio is infinite and every
/// boundary looks significant. Expressed in the same normalised units as the signal, so 0.02 is
/// two per cent of a bound - about the difference nothing in this product can resolve anyway.
pub const MIN_SPREAD: f32 = 0.02;

/// The widest range of one channel a single target can cover, in bounds.
///
/// Two: a target sits in the middle and the bound reaches one full bound either side of it. A node
/// wider than this has frames the bound cannot reach whatever the target is, so it is two nodes -
/// and this is the rule that splits a sunset, which is a ramp rather than a step.
pub const MAX_COVERABLE_SPAN: f32 = 2.0;

/// The percentiles the span is measured between.
///
/// Not the extremes. One stray frame in a chapter is something for `crate::outlier` to report, and
/// cutting a coherent chapter in half because of it would turn one bad frame into two unanchorable
/// nodes.
pub const SPAN_PERCENTILES: (f32, f32) = (0.10, 0.90);

/// The largest number of boundaries one node may be split at.
///
/// Six, so a node becomes at most seven. Past that the node was not a lighting group with a
/// transition in it; it was a segment that should have been sub-clustered, and continuing to split
/// produces runs too short to anchor. The cap is a bug detector as much as a limit.
pub const MAX_SPLITS: usize = 6;

/// Why a boundary was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// A discontinuity the node's own trend does not explain. A flash, a venue change.
    Step,
    /// A range one target cannot cover. A sunset.
    Span,
}

/// One boundary: the index the later run starts at, why, and how strong the evidence was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Boundary {
    /// The index into the node's frames where the later run begins.
    pub at: usize,
    /// Which of the two rules declared it.
    pub why: Why,
    /// The statistic that put it there. A step boundary carries the step in trend-widths; a span
    /// boundary carries the parent's span in bounds.
    pub strength: f32,
}

/// Split one node wherever the light genuinely changed.
///
/// Returns the node unchanged when there is nothing to split, and a list of children in capture
/// order when there is. Each child carries `GalleryCode::NodeSplitByChangePoint` and points at the
/// original as its parent, so a panel can say what happened and a re-analysis can tell a split node
/// from a sub-clustered one.
///
/// A boundary that would leave fewer than [`MIN_NODE_FRAMES`] on either side is declined and
/// recorded as `GalleryCode::SplitTooSmall` on the node that was not split. That is a real answer:
/// three frames of candlelight in the middle of a ceremony genuinely cannot be anchored, and
/// splitting them out converts a mild inconsistency into an unanchored gap that normalises nothing.
#[must_use]
pub fn split(node: &RawNode, sigma: f32) -> Vec<RawNode> {
    let signal = signal_of(node);
    if signal.len() < 2 * MIN_RUN {
        return vec![node.clone()];
    }

    let mut declined = false;
    let mut boundaries = find(&signal, sigma, &mut declined);
    boundaries.sort_by_key(|b| b.at);
    boundaries.truncate(MAX_SPLITS);

    if boundaries.is_empty() {
        let mut only = node.clone();
        if declined && !only.reasons.contains(&GalleryCode::SplitTooSmall) {
            only.reasons.push(GalleryCode::SplitTooSmall);
        }
        return vec![only];
    }

    let mut children = Vec::new();
    let mut start = 0usize;
    let mut cuts: Vec<usize> = boundaries.iter().map(|b| b.at).collect();
    cuts.push(node.frames.len());
    let siblings = cuts.len();
    for (ordinal, end) in cuts.into_iter().enumerate() {
        let Some(frames) = node.frames.get(start..end) else {
            continue;
        };
        if frames.is_empty() {
            continue;
        }
        let mut reasons = node.reasons.clone();
        reasons.push(GalleryCode::NodeSplitByChangePoint);
        if declined {
            reasons.push(GalleryCode::SplitTooSmall);
        }
        children.push(RawNode {
            id: NodeId::new(),
            parent: Some(node.id),
            segment: node.segment,
            ordinal,
            siblings,
            scene: crate::tree::dominant_scene(frames),
            frames: frames.to_vec(),
            reasons,
        });
        start = end;
    }
    if children.is_empty() {
        vec![node.clone()]
    } else {
        children
    }
}

/// The three-channel signal, each channel already divided by its own bound.
///
/// A frame with no tone estimate contributes the previous frame's values rather than a zero,
/// because a gap read as zero is a step of one full bound in both directions and would split the
/// node twice around every unanalysed frame. The first frames of a node with no estimate at all
/// contribute nothing and the node is not split.
#[must_use]
pub fn signal_of(node: &RawNode) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity(node.frames.len());
    let mut last: Option<[f32; 3]> = None;
    for frame in &node.frames {
        let point = match (frame.cct_k, frame.tint, frame.subject_luma) {
            (Some(cct), Some(tint), Some(luma)) => Some([
                cct / MAX_D_CCT_K,
                tint / MAX_D_TINT,
                // Subject luminance is `0..1` and the exposure bound is in stops, so the natural
                // scale for a luminance step is the luminance change one bound of exposure makes.
                // A third of a stop is about a quarter of the way up a mid-tone, which is what
                // this divisor is: it puts a full-bound exposure step at one, like the other two.
                luma / (MAX_D_EXPOSURE_EV * 0.7),
            ]),
            _ => last,
        };
        if let Some(point) = point {
            out.push(point);
            last = Some(point);
        }
    }
    out
}

/// Every boundary in a signal, strongest first.
///
/// A greedy binary segmentation: find the single strongest boundary, then recurse into each side.
/// Greedy rather than the dynamic program phase 07's PELT uses, and that is a deliberate
/// difference: here the number of change points is small and bounded by [`MAX_SPLITS`], the runs
/// are required to be long, and a greedy search over a bounded depth is exhaustive in practice
/// while being far easier to show terminates.
fn find(signal: &[[f32; 3]], sigma: f32, declined: &mut bool) -> Vec<Boundary> {
    let mut found = Vec::new();
    let mut stack = vec![(0usize, signal.len())];
    while let Some((from, to)) = stack.pop() {
        if found.len() >= MAX_SPLITS {
            break;
        }
        let Some(window) = signal.get(from..to) else {
            continue;
        };
        let Some(boundary) = strongest(window, sigma, declined) else {
            continue;
        };
        let at = from + boundary.at;
        found.push(Boundary {
            at,
            why: boundary.why,
            strength: boundary.strength,
        });
        stack.push((from, at));
        stack.push((at, to));
    }
    found.sort_by(|a, b| {
        b.strength
            .partial_cmp(&a.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.at.cmp(&b.at))
    });
    found
}

/// The strongest boundary in one window, or `None` when neither rule fires.
///
/// Two rules, tried in order. A step is a claim about the room and takes precedence; a span is a
/// claim about arithmetic - that no single target can reach these frames - and applies when there
/// is no discontinuity to find.
///
/// Sets `declined` when a rule *did* fire but one of its sides would have been too short. That is
/// the difference between "the light did not change" and "the light changed for four frames", and
/// only the first is a statement about the room.
fn strongest(window: &[[f32; 3]], sigma: f32, declined: &mut bool) -> Option<Boundary> {
    if window.len() < 2 * MIN_RUN {
        return None;
    }

    // Rule one: a discontinuity the trend does not explain.
    let mut best: Option<Boundary> = None;
    for at in MIN_RUN..=window.len().saturating_sub(MIN_RUN) {
        let (Some(left), Some(right)) = (window.get(..at), window.get(at..)) else {
            continue;
        };
        let statistic = step(left, right);
        if statistic < sigma {
            continue;
        }
        if left.len() < MIN_NODE_FRAMES || right.len() < MIN_NODE_FRAMES {
            *declined = true;
            continue;
        }
        let better = best.is_none_or(|current| statistic > current.strength + 1e-6);
        if better {
            best = Some(Boundary {
                at,
                why: Why::Step,
                strength: statistic,
            });
        }
    }
    if best.is_some() {
        return best;
    }

    // Rule two: a range no single target can cover. The boundary chosen is the one that most
    // equalises the halves, because the point of the split is that each side becomes reachable -
    // and cutting a sunset near one end leaves the long side exactly as unreachable as it was.
    let parent = span(window);
    if parent <= MAX_COVERABLE_SPAN {
        return None;
    }
    let mut widest: Option<(usize, f32)> = None;
    for at in MIN_RUN..=window.len().saturating_sub(MIN_RUN) {
        let (Some(left), Some(right)) = (window.get(..at), window.get(at..)) else {
            continue;
        };
        if left.len() < MIN_NODE_FRAMES || right.len() < MIN_NODE_FRAMES {
            *declined = true;
            continue;
        }
        let worst = span(left).max(span(right));
        let better = widest.is_none_or(|(_, current)| worst < current - 1e-6);
        if better {
            widest = Some((at, worst));
        }
    }
    widest.map(|(at, _)| Boundary {
        at,
        why: Why::Span,
        strength: parent,
    })
}

/// The widest robust range of any channel in a run, in bounds.
///
/// Measured between [`SPAN_PERCENTILES`] rather than between the extremes; see the constant.
#[must_use]
pub fn span(run: &[[f32; 3]]) -> f32 {
    let mut worst = 0.0_f32;
    for channel in 0..3 {
        let mut values: Vec<f32> = run
            .iter()
            .filter_map(|point| point.get(channel).copied())
            .filter(|value| value.is_finite())
            .collect();
        if values.len() < 2 {
            continue;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let last = values.len() - 1;
        let lo = ((last as f32) * SPAN_PERCENTILES.0).round() as usize;
        let hi = ((last as f32) * SPAN_PERCENTILES.1).round() as usize;
        let (Some(low), Some(high)) = (values.get(lo.min(last)), values.get(hi.min(last))) else {
            continue;
        };
        let range = (high - low).abs();
        if range > worst {
            worst = range;
        }
    }
    worst
}

/// How big the step between two runs is, in **trend-widths**.
///
/// The largest of the three channels rather than their sum: a venue change moves all three and a
/// flash moves two, but a sunset moves *one*, and a sum divides a real single-channel step by three.
/// Phase 08's rule about conjunctions read in the other direction - this is a disjunction, because
/// any one of the three changing is a lighting change.
///
/// The divisor is the module header's whole point: the trend times the distance between the two
/// runs' medians is what a pure ramp puts between them, so a ramp scores about one and only a
/// genuine discontinuity scores more.
#[must_use]
pub fn step(left: &[[f32; 3]], right: &[[f32; 3]]) -> f32 {
    let mut worst = 0.0_f32;
    // How far apart the two runs' *medians* are, in frames, under a pure ramp: half of one run plus
    // half of the other. Not the shorter run's length, which was the first version and is wrong at
    // every off-centre boundary - a six-frame head against a thirty-four-frame tail has medians
    // twenty frames apart and would be divided by three, scoring a smooth ramp at six.
    let width = ((left.len() + right.len()) as f32) * 0.5;
    for channel in 0..3 {
        let l: Vec<f32> = left
            .iter()
            .filter_map(|p| p.get(channel).copied())
            .collect();
        let r: Vec<f32> = right
            .iter()
            .filter_map(|p| p.get(channel).copied())
            .collect();
        let (Some(lm), Some(rm)) = (stats::median(&l), stats::median(&r)) else {
            continue;
        };
        // Two things a run can look like without there being a step in it: a slope, and noise. The
        // divisor has to cover both, so it is the larger of what the trend predicts and what the
        // within-run scatter predicts.
        let trend = trend_of(&l).max(trend_of(&r));
        let scatter = 0.5 * (stats::mean_abs_deviation(&l) + stats::mean_abs_deviation(&r));
        let expected = (trend * width).max(scatter).max(MIN_SPREAD);
        let statistic = (lm - rm).abs() / expected;
        if statistic > worst {
            worst = statistic;
        }
    }
    worst
}

/// The typical absolute difference between consecutive frames of a run.
///
/// A median rather than a mean, because one abrupt frame inside a run - a single mis-metered
/// exposure - would otherwise inflate the trend and hide the step beside it.
#[must_use]
pub fn trend_of(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let steps: Vec<f32> = values
        .windows(2)
        .filter_map(|pair| match pair {
            [a, b] => Some((b - a).abs()),
            _ => None,
        })
        .collect();
    stats::median(&steps).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use aura_core::{SceneId, SegmentId};

    fn node_from(values: &[(f32, f32, f32)]) -> RawNode {
        let segment = SegmentId::new();
        let frames = values
            .iter()
            .enumerate()
            .map(|(i, (cct, tint, luma))| {
                let mut frame = fixtures::frame_at(segment, i as i64 * 2_000, SceneId::Ceremony);
                frame.cct_k = Some(*cct);
                frame.tint = Some(*tint);
                frame.subject_luma = Some(*luma);
                frame
            })
            .collect();
        RawNode {
            id: NodeId::new(),
            parent: None,
            segment,
            ordinal: 0,
            siblings: 1,
            scene: SceneId::Ceremony,
            frames,
            reasons: Vec::new(),
        }
    }

    #[test]
    fn a_steady_room_is_not_split() {
        let values: Vec<(f32, f32, f32)> = (0..40)
            .map(|i| (5000.0 + (i % 5) as f32 * 8.0, 2.0, 0.45))
            .collect();
        let node = node_from(&values);
        let parts = split(&node, 3.0);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].parent.is_none());
    }

    #[test]
    fn a_slow_drift_is_normalised_rather_than_split() {
        // The regression this module was rewritten for. A chapter that warms 500 K over forty
        // frames is *drift*, which is the thing this phase exists to remove; the first
        // implementation cut it into six nodes and reported six lighting changes. A trend-aware
        // statistic scores it at about one.
        let values: Vec<(f32, f32, f32)> = (0..40)
            .map(|i| {
                let t = i as f32 / 39.0;
                (
                    4750.0 + t * 500.0 + ((i % 7) as f32 - 3.0) * 8.0,
                    t * 2.0,
                    0.42 + t * 0.06,
                )
            })
            .collect();
        let node = node_from(&values);
        let parts = split(&node, 3.0);
        assert_eq!(
            parts.len(),
            1,
            "a 500 K drift over forty frames was cut into {} nodes",
            parts.len()
        );
    }

    #[test]
    fn a_ramp_scores_about_one_and_a_step_of_the_same_size_scores_far_more() {
        let ramp: Vec<[f32; 3]> = (0..24).map(|i| [i as f32 * 0.05, 0.0, 0.0]).collect();
        let (a, b) = ramp.split_at(12);
        let ramp_stat = step(a, b);

        let mut stepped: Vec<[f32; 3]> = (0..24).map(|_| [0.0_f32, 0.0, 0.0]).collect();
        for point in stepped.iter_mut().skip(12) {
            point[0] = 0.6;
        }
        let (c, d) = stepped.split_at(12);
        let step_stat = step(c, d);

        assert!(ramp_stat < 3.0, "a pure ramp scored {ramp_stat}");
        assert!(
            step_stat > 10.0 * ramp_stat,
            "a step of the same total size scored {step_stat} against a ramp's {ramp_stat}"
        );
    }

    #[test]
    fn a_span_no_target_can_cover_splits_even_with_no_discontinuity_in_it() {
        // A sunset. 2,700 K over sixty frames is smooth - no step anywhere in it - and no single
        // target can reach both ends, because the bound is 450 K and the span is six of them.
        let values: Vec<(f32, f32, f32)> = (0..60)
            .map(|i| (6500.0 - i as f32 * 45.0, 0.0, 0.5 - i as f32 * 0.002))
            .collect();
        let node = node_from(&values);
        let parts = split(&node, 3.0);
        assert!(parts.len() >= 2, "a six-bound span is not one look");
        for part in &parts {
            let signal = signal_of(part);
            assert!(
                span(&signal) <= MAX_COVERABLE_SPAN + 0.5,
                "a child still spans {:.2} bounds",
                span(&signal)
            );
        }
    }

    #[test]
    fn one_stray_frame_does_not_split_a_coherent_chapter() {
        // The span is measured between percentiles rather than between the extremes, so a single
        // frame 2,600 K away is an outlier for `crate::outlier` and not a reason to cut a chapter
        // in half.
        let mut values: Vec<(f32, f32, f32)> = (0..40)
            .map(|i| (5000.0 + ((i % 5) as f32 - 2.0) * 15.0, 0.0, 0.45))
            .collect();
        values[39] = (7600.0, 0.0, 0.22);
        let node = node_from(&values);
        assert_eq!(split(&node, 3.0).len(), 1);
    }

    #[test]
    fn a_flash_going_on_splits_the_node_and_says_why() {
        let mut values: Vec<(f32, f32, f32)> = (0..20)
            .map(|i| (3100.0 + (i % 3) as f32 * 10.0, 4.0, 0.28))
            .collect();
        values.extend((0..20).map(|i| (5400.0 + (i % 3) as f32 * 10.0, 1.0, 0.52)));
        let node = node_from(&values);
        let parts = split(&node, 3.0);
        assert_eq!(parts.len(), 2, "a flash is a lighting change");
        assert!(parts
            .iter()
            .all(|p| p.reasons.contains(&GalleryCode::NodeSplitByChangePoint)));
        assert_eq!(parts[0].parent, Some(node.id));
        assert!((parts[0].frames.len() as i32 - 20).abs() <= 2);
    }

    #[test]
    fn a_sunset_produces_boundaries_down_the_ramp_rather_than_none() {
        let values: Vec<(f32, f32, f32)> = (0..60)
            .map(|i| (6500.0 - i as f32 * 45.0, 0.0, 0.5 - i as f32 * 0.003))
            .collect();
        let node = node_from(&values);
        let parts = split(&node, 3.0);
        assert!(parts.len() >= 2, "a ramp of 2,700 K is not one look");
    }

    #[test]
    fn a_four_frame_flicker_is_declined_rather_than_split_out() {
        let mut values: Vec<(f32, f32, f32)> = (0..20).map(|_| (5000.0, 0.0, 0.45)).collect();
        values.extend((0..4).map(|_| (2100.0, 0.0, 0.15)));
        values.extend((0..20).map(|_| (5000.0, 0.0, 0.45)));
        let node = node_from(&values);
        let parts = split(&node, 3.0);
        // The two long runs either side of the flicker are the same light, so either the node is
        // whole and declines, or it splits at the flicker's edges - and the flicker itself is never
        // a node of its own, because four frames cannot be anchored.
        assert!(parts.iter().all(|p| p.frames.len() >= MIN_NODE_FRAMES));
    }

    #[test]
    fn a_node_with_no_tone_estimates_at_all_is_not_split() {
        let segment = SegmentId::new();
        let frames = (0..30)
            .map(|i| {
                let mut frame = fixtures::frame_at(segment, i * 1_000, SceneId::Venue);
                frame.cct_k = None;
                frame.tint = None;
                frame.subject_luma = None;
                frame
            })
            .collect();
        let node = RawNode {
            id: NodeId::new(),
            parent: None,
            segment,
            ordinal: 0,
            siblings: 1,
            scene: SceneId::Venue,
            frames,
            reasons: Vec::new(),
        };
        assert_eq!(split(&node, 3.0).len(), 1);
    }

    #[test]
    fn identical_frames_do_not_split_on_floating_point_noise() {
        let values: Vec<(f32, f32, f32)> = (0..40).map(|_| (5000.0, 0.0, 0.45)).collect();
        let node = node_from(&values);
        assert_eq!(split(&node, 3.0).len(), 1, "MIN_SPREAD is the floor");
    }

    #[test]
    fn a_step_is_the_worst_channel_rather_than_the_sum() {
        let left = [[1.0_f32, 0.0, 0.0]; 8];
        let mut right = [[1.0_f32, 0.0, 0.0]; 8];
        for point in &mut right {
            point[0] = 2.0;
        }
        let one_channel = step(&left, &right);
        assert!(
            one_channel > 3.0,
            "a single-channel step is not divided by three"
        );
    }

    #[test]
    fn splitting_is_deterministic() {
        let mut values: Vec<(f32, f32, f32)> = (0..25).map(|_| (3000.0, 5.0, 0.3)).collect();
        values.extend((0..25).map(|_| (5600.0, 0.0, 0.55)));
        let node = node_from(&values);
        let a: Vec<usize> = split(&node, 3.0).iter().map(|n| n.frames.len()).collect();
        let b: Vec<usize> = split(&node, 3.0).iter().map(|n| n.frames.len()).collect();
        assert_eq!(a, b);
    }
}
