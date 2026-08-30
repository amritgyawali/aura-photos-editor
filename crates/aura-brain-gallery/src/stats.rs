//! What a set of frames looks like, robustly.
//!
//! Section 6.1: "anchor statistics are robust (trimmed means, median chromaticity) so one anchor
//! error cannot skew a node." This module is that sentence, plus the two spread measures the
//! change-point detector and section 10.1's headline gate are both computed from.
//!
//! ## Why robust rather than plain
//!
//! A node's target is computed from **three to five** frames. A mean over three samples has no
//! resistance at all to one of them being wrong, and one of them being wrong is the failure mode
//! section 12 names second - "bad anchors propagate errors". A trimmed mean over four samples
//! drops the extreme one and averages the rest, which is the cheapest thing that survives a single
//! bad anchor and is still an average rather than a single frame's opinion.
//!
//! ## Why the chromaticity is a median and the scalars are trimmed means
//!
//! Because a chromaticity is two numbers that have to stay together. Trimming the extreme *u'* and
//! the extreme *v'* independently can produce a point that no anchor was anywhere near - the
//! horizontal outlier is dropped from one axis and kept in the other. A component-wise median over
//! an odd count is always a real coordinate on one axis and is the standard robust answer; over an
//! even count it is the mid-point of the two central values, which is the smallest departure from
//! that. Phase 22's rule about instruments: know what your estimator can and cannot produce.
//!
//! ## The spread is a mean absolute deviation, not a standard deviation
//!
//! Section 10.1 asks for "within-scene WB spread reduced >= 60 %", and a standard deviation over
//! four frames squares the contribution of the one that drifted - which reports a node whose
//! single worst frame was clamped as barely improved. A mean absolute deviation is linear in the
//! thing a person actually sees, which is how far apart two adjacent frames look. Phase 15's
//! `skin_locus` made the same choice for the same reason.

/// A trimmed mean: the extremes dropped, the rest averaged.
///
/// `trim` is how many values to drop from **each** end. With fewer than `2 * trim + 1` values
/// nothing is trimmed, because trimming a three-sample set from both ends leaves one sample and
/// calls it an average.
///
/// Returns `None` on an empty slice, which is a real answer rather than a zero: a node with no
/// anchors has no temperature, and reporting one as 0 K would put every frame in it at the bound.
#[must_use]
pub fn trimmed_mean(values: &[f32], trim: usize) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let keep = if sorted.len() > 2 * trim {
        sorted.get(trim..sorted.len() - trim).unwrap_or(&sorted)
    } else {
        &sorted[..]
    };
    if keep.is_empty() {
        return None;
    }
    let sum: f32 = keep.iter().sum();
    Some(sum / keep.len() as f32)
}

/// The median of a slice.
///
/// Over an even count it is the mid-point of the two central values. Deterministic: the sort is
/// total over finite values and non-finite values are dropped before it, so two runs of the same
/// input produce the same answer whatever order the caller assembled it in. Invariant 4.
#[must_use]
pub fn median(values: &[f32]) -> Option<f32> {
    let mut sorted: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted.get(mid).copied()
    } else {
        match (sorted.get(mid - 1), sorted.get(mid)) {
            (Some(a), Some(b)) => Some(0.5 * (a + b)),
            _ => None,
        }
    }
}

/// The component-wise median of a set of chromaticities.
///
/// See the module header for why this is not two independent trimmed means.
#[must_use]
pub fn median_uv(values: &[[f32; 2]]) -> Option<[f32; 2]> {
    if values.is_empty() {
        return None;
    }
    let us: Vec<f32> = values.iter().map(|uv| uv[0]).collect();
    let vs: Vec<f32> = values.iter().map(|uv| uv[1]).collect();
    match (median(&us), median(&vs)) {
        (Some(u), Some(v)) => Some([u, v]),
        _ => None,
    }
}

/// The mean absolute deviation about the set's own mean.
///
/// The spread section 10.1's gates are measured in. Zero on a set of one, which is correct: a node
/// with one frame in it has no internal inconsistency, and reporting a spread for it would put a
/// number in the denominator of a reduction that means nothing.
#[must_use]
pub fn mean_abs_deviation(values: &[f32]) -> f32 {
    let finite: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.len() < 2 {
        return 0.0;
    }
    let mean = finite.iter().sum::<f32>() / finite.len() as f32;
    finite.iter().map(|v| (v - mean).abs()).sum::<f32>() / finite.len() as f32
}

/// How tightly a set of values agrees, `0..1`, against a scale that says what "tight" means.
///
/// One when every value is identical; falling toward zero as the mean absolute deviation
/// approaches `scale`. The scale is the node's own tolerance rather than a constant, which is
/// invariant 7 applied to an agreement measure: a dance floor whose frames sit 300 K apart is
/// cohesive and a family portrait session whose frames sit 300 K apart is not.
#[must_use]
pub fn cohesion(values: &[f32], scale: f32) -> f32 {
    if values.len() < 2 {
        // A set of one agrees with itself, and saying otherwise would make every node with a
        // single anchor look like a disagreement rather than like a node with a single anchor.
        return 1.0;
    }
    if !scale.is_finite() || scale <= 0.0 {
        return 0.0;
    }
    (1.0 - mean_abs_deviation(values) / scale).clamp(0.0, 1.0)
}

/// The eight-number colour-character descriptor a node's target carries.
///
/// Section 5's `grade_signature: [f32; 8]`, in a fixed order that is part of the contract with
/// every stored node:
///
/// | Slot | What it is |
/// |---|---|
/// | 0 | shadow hue, as a turn `0..1` |
/// | 1 | shadow hue spread, `0..1` |
/// | 2 | highlight hue, as a turn `0..1` |
/// | 3 | highlight hue spread, `0..1` |
/// | 4 | shadow chroma, `0..1` |
/// | 5 | highlight chroma, `0..1` |
/// | 6 | mid-tone slope, normalised `0..1` about a neutral 0.5 |
/// | 7 | black point, `0..1` |
///
/// It is **compared, never applied**. Nothing in this crate or in the contract turns a signature
/// back into parameters: phase 16 owns the grade and this phase owns the distance between two of
/// them. A signature that could be inverted would be a second grader.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradeSignature {
    /// The eight numbers.
    pub values: [f32; 8],
}

impl GradeSignature {
    /// Assemble one from the readings phase 16 already stored.
    ///
    /// Hues arrive in degrees and are stored as turns, because a hue is circular and a distance in
    /// degrees between 359 and 1 is 358 rather than 2. The turn form is what
    /// [`GradeSignature::hue_distance`] wraps.
    // Eight arguments, one per slot, and the slots are a frozen order every stored node carries.
    // A struct of eight `f32`s passed by value would be the same eight numbers with a second name
    // for each of them, and a builder would let a caller construct a signature with a slot missing.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        shadow_hue_deg: f32,
        shadow_hue_spread_deg: f32,
        highlight_hue_deg: f32,
        highlight_hue_spread_deg: f32,
        shadow_chroma: f32,
        highlight_chroma: f32,
        mid_slope: f32,
        black_point: f32,
    ) -> Self {
        Self {
            values: [
                wrap_turn(shadow_hue_deg / 360.0),
                (shadow_hue_spread_deg / 360.0).clamp(0.0, 1.0),
                wrap_turn(highlight_hue_deg / 360.0),
                (highlight_hue_spread_deg / 360.0).clamp(0.0, 1.0),
                shadow_chroma.clamp(0.0, 1.0),
                highlight_chroma.clamp(0.0, 1.0),
                ((mid_slope + 1.0) * 0.5).clamp(0.0, 1.0),
                black_point.clamp(0.0, 1.0),
            ],
        }
    }

    /// The robust central signature of a set.
    ///
    /// Component-wise medians, and the two hue components go through a circular median rather than
    /// a linear one: the linear median of 350 and 10 degrees is 180, which is the opposite colour.
    #[must_use]
    pub fn central(set: &[Self]) -> Option<Self> {
        if set.is_empty() {
            return None;
        }
        let mut values = [0.0_f32; 8];
        for slot in 0..8 {
            let column: Vec<f32> = set
                .iter()
                .filter_map(|sig| sig.values.get(slot).copied())
                .collect();
            let value = if slot == 0 || slot == 2 {
                circular_median(&column)
            } else {
                median(&column)
            };
            if let (Some(target), Some(value)) = (values.get_mut(slot), value) {
                *target = value;
            }
        }
        Some(Self { values })
    }

    /// The distance to another signature, `0..1`.
    ///
    /// Euclidean over the eight components, with the two hue components measured as the shorter
    /// way round a circle. Scaled by the length so the answer does not depend on how many
    /// components there are.
    #[must_use]
    pub fn distance(&self, other: &Self) -> f32 {
        let mut sum = 0.0_f32;
        for slot in 0..8 {
            let (Some(a), Some(b)) = (self.values.get(slot), other.values.get(slot)) else {
                continue;
            };
            let d = if slot == 0 || slot == 2 {
                hue_distance(*a, *b)
            } else {
                a - b
            };
            sum += d * d;
        }
        (sum / 8.0).sqrt().clamp(0.0, 1.0)
    }
}

/// The shorter way round the circle between two hues expressed as turns, `0..0.5`.
#[must_use]
pub fn hue_distance(a: f32, b: f32) -> f32 {
    let raw = (a - b).abs() % 1.0;
    if raw > 0.5 {
        1.0 - raw
    } else {
        raw
    }
}

fn wrap_turn(turn: f32) -> f32 {
    if !turn.is_finite() {
        return 0.0;
    }
    let wrapped = turn % 1.0;
    if wrapped < 0.0 {
        wrapped + 1.0
    } else {
        wrapped
    }
}

/// The median of a set of hues expressed as turns.
///
/// The candidate that minimises the summed circular distance to the others, which is the circular
/// analogue of a median and, unlike a mean of angles, is always one of the observations. Exhaustive
/// over the set because a node has at most five anchors and an exhaustive search over five is
/// twenty-five comparisons - phase 24 made the same call for its homography.
fn circular_median(values: &[f32]) -> Option<f32> {
    let finite: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    let mut best = None::<(f32, f32)>;
    for candidate in &finite {
        let cost: f32 = finite
            .iter()
            .map(|other| hue_distance(*candidate, *other))
            .sum();
        let better = match best {
            None => true,
            // Ties break toward the smaller hue so two runs of the same set agree. Invariant 4.
            Some((_, best_cost)) => cost < best_cost - 1e-9,
        };
        if better {
            best = Some((*candidate, cost));
        }
    }
    best.map(|(value, _)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trimmed_mean_drops_one_bad_anchor() {
        let good = [5000.0, 5050.0, 4980.0, 5020.0];
        let with_bad = [5000.0, 5050.0, 4980.0, 5020.0, 9000.0];
        let plain: f32 = with_bad.iter().sum::<f32>() / with_bad.len() as f32;
        let trimmed = trimmed_mean(&with_bad, 1).unwrap();
        let clean = trimmed_mean(&good, 1).unwrap();
        assert!(
            (trimmed - clean).abs() < 60.0,
            "the trimmed mean {trimmed} should be near the clean {clean}"
        );
        assert!(plain > 5500.0, "the plain mean is dragged to {plain}");
    }

    #[test]
    fn trimming_never_leaves_a_set_of_one_calling_itself_an_average() {
        let three = [1.0_f32, 2.0, 30.0];
        // 2 * 1 + 1 == 3, so one from each end is exactly allowed and leaves the middle.
        assert_eq!(trimmed_mean(&three, 1), Some(2.0));
        // Two from each end would leave nothing, so nothing is trimmed.
        let mean = trimmed_mean(&three, 2).unwrap();
        assert!((mean - 11.0).abs() < 1e-4);
    }

    #[test]
    fn an_empty_set_has_no_mean_rather_than_a_zero_one() {
        assert_eq!(trimmed_mean(&[], 1), None);
        assert_eq!(median(&[]), None);
        assert_eq!(median_uv(&[]), None);
    }

    #[test]
    fn a_median_uv_is_a_plausible_point_when_one_anchor_is_wrong_on_one_axis() {
        let set = [[0.20, 0.47], [0.21, 0.48], [0.205, 0.475], [0.40, 0.478]];
        let uv = median_uv(&set).unwrap();
        assert!(
            uv[0] < 0.25,
            "the horizontal outlier did not move the median: {uv:?}"
        );
        assert!(uv[1] > 0.46 && uv[1] < 0.49);
    }

    #[test]
    fn the_spread_is_linear_in_what_a_person_sees() {
        let tight = [5000.0, 5010.0, 4990.0, 5000.0];
        let loose = [5000.0, 5300.0, 4700.0, 5000.0];
        assert!(mean_abs_deviation(&tight) < mean_abs_deviation(&loose));
        assert_eq!(mean_abs_deviation(&[5000.0]), 0.0);
    }

    #[test]
    fn cohesion_is_scaled_by_what_the_scene_tolerates() {
        let frames = [5000.0, 5150.0, 4850.0, 5000.0];
        let strict = cohesion(&frames, 120.0);
        let loose = cohesion(&frames, 450.0);
        assert!(
            loose > strict,
            "the same frames agree more on a dance floor than in a family portrait"
        );
        assert_eq!(cohesion(&[5000.0], 120.0), 1.0);
    }

    #[test]
    fn a_hue_distance_goes_the_short_way_round() {
        // 350 degrees and 10 degrees are 20 degrees apart, which is 0.0556 of a turn.
        let d = hue_distance(350.0 / 360.0, 10.0 / 360.0);
        assert!((d - 20.0 / 360.0).abs() < 1e-5, "{d}");
    }

    #[test]
    fn a_circular_median_does_not_land_on_the_opposite_colour() {
        let hues = [350.0 / 360.0, 10.0 / 360.0, 0.0];
        let m = circular_median(&hues).unwrap();
        let d = hue_distance(m, 0.0);
        assert!(d < 0.05, "circular median landed at {m}");
    }

    #[test]
    fn two_signatures_of_the_same_look_are_close_and_of_opposite_looks_are_not() {
        let warm = GradeSignature::new(30.0, 8.0, 40.0, 6.0, 0.10, 0.05, 0.1, 0.02);
        let same = GradeSignature::new(32.0, 9.0, 41.0, 6.0, 0.11, 0.05, 0.1, 0.02);
        let cool = GradeSignature::new(210.0, 8.0, 40.0, 6.0, 0.30, 0.25, -0.4, 0.20);
        assert!(warm.distance(&same) < 0.05, "{}", warm.distance(&same));
        assert!(warm.distance(&cool) > 0.15, "{}", warm.distance(&cool));
        assert_eq!(warm.distance(&warm), 0.0);
    }

    #[test]
    fn a_central_signature_survives_one_disagreeing_anchor() {
        let a = GradeSignature::new(30.0, 8.0, 40.0, 6.0, 0.10, 0.05, 0.1, 0.02);
        let b = GradeSignature::new(31.0, 8.0, 41.0, 6.0, 0.11, 0.05, 0.1, 0.02);
        let c = GradeSignature::new(29.0, 8.0, 39.0, 6.0, 0.09, 0.05, 0.1, 0.02);
        let bad = GradeSignature::new(210.0, 40.0, 200.0, 40.0, 0.9, 0.9, -0.9, 0.9);
        let central = GradeSignature::central(&[a, b, c, bad]).unwrap();
        assert!(
            central.distance(&a) < 0.08,
            "one bad anchor moved the centre to {:?}",
            central.values
        );
    }
}
