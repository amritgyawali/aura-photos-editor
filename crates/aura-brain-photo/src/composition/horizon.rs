//! Which way is level, how sure we are, and whether the photographer meant it.
//!
//! PHASE-11 section 6.1: "estimate dominant line orientation with a weighted Hough
//! transform on long edges, cross-checked against gravity metadata when the camera
//! records it; report confidence and never straighten below 0.6 confidence."
//!
//! Section 10.1's first gate is 0.4 degrees of angle error, which is the tightest
//! numerical gate in the phase and the reason this module does not stop at a histogram.
//!
//! ## Three stages, because a histogram alone cannot reach 0.4 degrees
//!
//! 1. **Orientation accumulation.** Every pixel whose gradient is strong enough votes for
//!    the direction of the edge through it - the gradient direction turned by ninety
//!    degrees - weighted by gradient magnitude. That is the "weighted Hough" of section
//!    6.1 in its useful form: a full Hough over `(rho, theta)` would find *lines*, and
//!    what a horizon estimate needs is the dominant *direction*, which is the theta
//!    marginal of that transform computed directly.
//! 2. **Mode finding**, on quarter-degree bins. Cheap, robust, and accurate to about a
//!    quarter of a degree - which is not good enough on its own.
//! 3. **Refinement**: a magnitude-weighted circular mean of every vote within
//!    [`REFINE_WINDOW_DEG`] of the mode. Binning throws away where inside a bin a vote
//!    fell; this puts it back, and it is what takes the estimate from a quarter of a
//!    degree to under a tenth.
//!
//! ## Doubled angles, because a line has no direction
//!
//! An edge at +44 degrees and an edge at -46 degrees are the same line. Averaging them
//! naively gives -1, which is neither. Every angle in this module is doubled before it is
//! averaged and halved afterwards - the standard axial-statistics trick - so that the
//! wrap at the ends of the range is handled by arithmetic rather than by a special case
//! somebody forgets.
//!
//! ## Verticals are the same measurement, rotated
//!
//! Section 2.1 asks for "vertical-line convergence" beside the horizon. It is computed
//! from the same accumulator: near-vertical votes are collected separately, and their
//! *spread* rather than their mean is the signal. Parallel verticals in a photograph of a
//! building mean the sensor was parallel to the wall; verticals that fan out mean the
//! camera was pointed up. See [`convergence`].
//!
//! ## What "intentional" costs to get wrong
//!
//! Section 12's first failure mode is that rigid rules punish creative work, and a dutch
//! angle is the clearest example in the phase. [`intentional`] requires **all four** of
//! section 6.1's conditions, and it is written as a conjunction of four named booleans
//! rather than as a score with a threshold, so that a reader can see which one was false.

use aura_core::contract::composition::{HorizonSource, DUTCH_ANGLE_DEG};
use aura_vision::embed::descriptors::LumaPlane;

/// Half-width of the angle range a horizon is looked for in, in degrees.
///
/// Past forty-five degrees the "horizon" found is the other axis of the frame. Reporting
/// an 89 degree tilt on a portrait-orientation photograph is how a straightening tool
/// rotates a wedding onto its side, so the range is closed here rather than in the
/// caller.
pub const RANGE_DEG: f32 = 45.0;

/// Width of one accumulator bin, in degrees.
pub const BIN_DEG: f32 = 0.25;

/// How many bins cover the range.
pub const BINS: usize = 360;

/// How far either side of the mode the refinement pass reads, in degrees.
///
/// One and a half degrees. Wide enough that a mode landing near a bin edge still gathers
/// its whole peak, narrow enough that a second line family two degrees away does not pull
/// the answer towards itself.
pub const REFINE_WINDOW_DEG: f32 = 1.5;

/// Gradient magnitude below which a pixel does not vote.
///
/// On a 0..1 luminance plane. A texture-free wall produces a haze of tiny gradients whose
/// orientations are noise, and letting them vote would fill every bin evenly and make the
/// confidence figure meaningless rather than low.
pub const EDGE_FLOOR: f32 = 0.06;

/// How much of the total edge weight the peak must hold for full confidence.
///
/// Twelve per cent. A photograph with one strong horizon typically puts a quarter of its
/// edge weight within a degree of it; a portrait shot against foliage puts about three
/// per cent in the largest bin, which is what "there is no horizon here" looks like as a
/// number.
pub const PEAK_SHARE_FOR_FULL: f32 = 0.12;

/// Rho buckets used to distinguish one long line from repeated texture.
///
/// The orientation histogram is the theta marginal of a Hough transform. It is accurate,
/// but theta alone cannot tell a horizon from a striped wall: both can have one dominant
/// direction. This small rho pass restores that missing distinction for the winning
/// angle without paying for a full two-dimensional accumulator.
pub const RHO_BINS: usize = 128;

/// Adjacent rho buckets combined into one candidate line.
///
/// Three buckets cover about three per cent of the frame diagonal, enough for an
/// anti-aliased boundary and its Sobel halo without joining separate parallel edges.
pub const RHO_WINDOW: usize = 3;

/// Share of direction-matched edge weight one line must hold for full confidence.
///
/// A real horizon concentrates most matching votes at one rho. Repeated weave distributes
/// them over many parallel rhos. Thirty per cent leaves room for foreground occlusion
/// while keeping repeated fabric and foliage below the strong-horizon threshold.
pub const LINE_SHARE_FOR_FULL: f32 = 0.30;

/// How far the gravity tag and the pixels may disagree before the pixels are distrusted.
///
/// Two degrees. Beyond that the two are measuring different things - usually because the
/// dominant line in the frame is a table edge rather than the horizon - and the estimate
/// keeps the gravity answer while dropping its confidence.
pub const GRAVITY_AGREEMENT_DEG: f32 = 2.0;

/// How near the centre a subject must be for a tilt to read as deliberate.
///
/// Section 6.1's "centred subject", as a distance from the frame centre in normalised
/// units. Twelve per cent of the frame, which is inside the middle fifth.
pub const CENTRED_WITHIN: f32 = 0.12;

/// Above this confidence there is a strong horizon, so a large tilt is a mistake.
///
/// Section 6.1's intentional-tilt rule requires "no strong horizon", and this is what
/// strong means. Deliberately *below* `HORIZON_ACT_AT`: the band between them is a frame
/// with a horizon good enough to straighten but not good enough to prove the tilt was an
/// accident, and in that band the tilt is reported and not excused.
pub const STRONG_HORIZON: f32 = 0.45;

/// What one frame's line structure says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorizonResult {
    /// Degrees off level, positive clockwise, within `-45..45`.
    pub tilt_deg: f32,
    /// How sure the estimate is, `0..1`.
    pub confidence: f32,
    /// What it was made from.
    pub source: HorizonSource,
    /// True when section 6.1's four conditions all hold.
    pub intentional: bool,
    /// How much the vertical lines fan out, `0..1`.
    pub convergence: f32,
    /// Fraction of the frame's pixels that voted, `0..1`. Carried for the panel and for
    /// the eval harness, which needs to distinguish "level" from "nothing to measure".
    pub edge_density: f32,
}

impl HorizonResult {
    /// A frame with nothing in it that says which way is up.
    #[must_use]
    pub const fn unmeasured() -> Self {
        Self {
            tilt_deg: 0.0,
            confidence: 0.0,
            source: HorizonSource::None,
            intentional: false,
            convergence: 0.0,
            edge_density: 0.0,
        }
    }

    /// True when the tilt is worth reporting at all.
    #[must_use]
    pub fn is_measured(&self) -> bool {
        self.source.is_measured()
    }
}

/// Everything outside the pixels that the horizon decision depends on.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HorizonInput {
    /// What the camera's gravity sensor said, in degrees, when it recorded one.
    ///
    /// Almost no camera writes this into EXIF today, and phase 01's schema has no column
    /// for it. The parameter exists because section 6.1 asks for the cross-check and
    /// because the shape of the code should not have to change when a body that records
    /// it arrives; it is `None` in every product path in this build, which the phase 11
    /// exit report records.
    pub gravity_deg: Option<f32>,
    /// Where the main subject is, normalised, when there is one.
    pub subject_centre: Option<(f32, f32)>,
    /// True when this scene's rule row permits a deliberate tilt.
    pub allows_dutch: bool,
}

/// Measure the frame's dominant line direction and its verticals.
///
/// The whole module in one call: accumulate, find the mode, refine it, cross-check
/// gravity, and decide whether a large tilt reads as a decision.
#[must_use]
pub fn analyse(plane: &LumaPlane, input: &HorizonInput) -> HorizonResult {
    let Some(votes) = accumulate(plane) else {
        return HorizonResult::unmeasured();
    };

    let mut result = match refine(&votes.horizontal) {
        Some((angle, share)) => HorizonResult {
            tilt_deg: angle,
            confidence: ((share / PEAK_SHARE_FOR_FULL)
                * (line_share(plane, angle) / LINE_SHARE_FOR_FULL))
                .clamp(0.0, 1.0),
            source: HorizonSource::Gradient,
            intentional: false,
            convergence: convergence(&votes.vertical),
            edge_density: votes.density,
        },
        None => HorizonResult {
            convergence: convergence(&votes.vertical),
            edge_density: votes.density,
            ..HorizonResult::unmeasured()
        },
    };

    // The vanishing-line fallback. When the horizontals said nothing but the verticals
    // are strongly parallel, the verticals *are* the level reference: a frame of a wall
    // with no horizon in it still says which way is up, and rotating the vertical family
    // by ninety degrees is that statement. Confidence is halved because a vertical family
    // can be a row of chair legs on a sloping lawn.
    if !result.is_measured() {
        if let Some((angle, share)) = refine(&votes.vertical) {
            if votes.vertical_parallel {
                result.tilt_deg = angle;
                result.confidence = (share / PEAK_SHARE_FOR_FULL).clamp(0.0, 1.0) * 0.5;
                result.source = HorizonSource::VanishingLines;
            }
        }
    }

    // Gravity wins where it exists, and disagreement costs confidence rather than being
    // averaged away. An average of two answers that differ by ten degrees is a third
    // answer that is wrong in a new way.
    if let Some(gravity) = input.gravity_deg {
        let gravity = gravity.clamp(-RANGE_DEG, RANGE_DEG);
        let agrees =
            !result.is_measured() || (result.tilt_deg - gravity).abs() <= GRAVITY_AGREEMENT_DEG;
        result.tilt_deg = gravity;
        result.source = HorizonSource::Gravity;
        result.confidence = if agrees {
            result.confidence.max(0.90)
        } else {
            0.55
        };
    }

    result.intentional = intentional(result.tilt_deg, result.confidence, input);
    result
}

/// Section 6.1's four conditions, as a conjunction of four named booleans.
///
/// > "a large tilt (> 6 deg) combined with a centred subject, no strong horizon and a
/// > `candid`/`dance_floor` scene is treated as style, not error."
///
/// Written out rather than scored because this is the decision section 12's first failure
/// mode turns on, and a reader debugging a wrongly flagged dutch angle needs to see which
/// of the four was false. A weighted sum would hide that behind a number.
#[must_use]
pub fn intentional(tilt_deg: f32, confidence: f32, input: &HorizonInput) -> bool {
    let large = tilt_deg.abs() > DUTCH_ANGLE_DEG;
    let centred = input.subject_centre.is_some_and(|(x, y)| {
        let dx = x - 0.5;
        let dy = y - 0.5;
        dx.hypot(dy) <= CENTRED_WITHIN
    });
    let no_strong_horizon = confidence < STRONG_HORIZON;
    let scene_permits = input.allows_dutch;
    large && centred && no_strong_horizon && scene_permits
}

/// How much the vertical family fans out, `0..1`.
///
/// Zero is a set of verticals that are all parallel - a camera held square to a wall. One
/// is a family spread across ten degrees or more, which is what pointing a wide lens up at
/// a building does.
///
/// The measure is the weighted circular standard deviation of the doubled angles, scaled
/// so that [`CONVERGENCE_FULL_DEG`] of spread reads as 1.0. Spread rather than a
/// vanishing-point solve: a vanishing point needs line segments and this module has
/// orientations, and the phase needs to *report* convergence rather than to correct it -
/// the correction is phase 23's.
#[must_use]
pub fn convergence(bins: &[f32; BINS]) -> f32 {
    let total: f32 = bins.iter().sum();
    if total <= f32::EPSILON {
        return 0.0;
    }
    let (mut sin_sum, mut cos_sum) = (0.0f32, 0.0f32);
    for (index, weight) in bins.iter().enumerate() {
        let doubled = 2.0 * bin_centre(index).to_radians();
        sin_sum += weight * doubled.sin();
        cos_sum += weight * doubled.cos();
    }
    let resultant = sin_sum.hypot(cos_sum) / total;
    // Circular standard deviation of the doubled angle, back in single-angle degrees.
    let spread_deg = (-2.0 * resultant.clamp(1e-6, 1.0).ln()).sqrt().to_degrees() / 2.0;
    (spread_deg / CONVERGENCE_FULL_DEG).clamp(0.0, 1.0)
}

/// Vertical spread, in degrees, that reads as fully converging.
///
/// Ten degrees. A 24 mm lens tilted fifteen degrees up at a three-storey facade spreads
/// its verticals by roughly that much, and it is the frame everybody recognises as
/// "keystoned".
pub const CONVERGENCE_FULL_DEG: f32 = 10.0;

/// Above this the verticals are parallel enough to be a level reference.
pub const VERTICAL_PARALLEL_BELOW: f32 = 0.35;

/// The two orientation histograms and how much of the frame voted.
#[derive(Debug, Clone)]
struct Votes {
    /// Near-horizontal edge directions, in `-45..45` degrees.
    horizontal: [f32; BINS],
    /// Near-vertical edge directions, rotated into `-45..45` degrees.
    vertical: [f32; BINS],
    /// Fraction of sampled pixels that voted.
    density: f32,
    /// True when the vertical family is tight enough to use as a level reference.
    vertical_parallel: bool,
}

/// Vote every strong-gradient pixel into one of the two histograms.
///
/// Returns `None` when too little of the frame has any structure at all, which is a real
/// answer rather than an error: a photograph of fog has no horizon and pretending
/// otherwise is how a straightening tool rotates one.
fn accumulate(plane: &LumaPlane) -> Option<Votes> {
    if plane.width < 8 || plane.height < 8 {
        return None;
    }
    let mut horizontal = [0.0f32; BINS];
    let mut vertical = [0.0f32; BINS];
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };

    let mut sampled = 0u32;
    let mut voted = 0u32;
    for y in 1..plane.height - 1 {
        for x in 1..plane.width - 1 {
            sampled += 1;
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            let magnitude = gx.hypot(gy);
            if magnitude < EDGE_FLOOR {
                continue;
            }
            voted += 1;
            // The edge runs perpendicular to its gradient. `atan2(gy, gx)` is the gradient
            // direction; the edge direction is that turned by ninety degrees, which after
            // wrapping into `-90..90` is what the two branches below sort out.
            let gradient_deg = gy.atan2(gx).to_degrees();
            let edge_deg = wrap_signed(gradient_deg + 90.0);
            if edge_deg.abs() <= RANGE_DEG {
                add(&mut horizontal, edge_deg, magnitude);
            } else {
                // Near-vertical: rotate by ninety degrees into the same range so one bin
                // layout serves both families.
                add(&mut vertical, wrap_signed(edge_deg + 90.0), magnitude);
            }
        }
    }

    if sampled == 0 {
        return None;
    }
    let density = f64::from(voted) as f32 / f64::from(sampled) as f32;
    // Below one pixel in five hundred there is not enough structure for any of this to
    // mean anything. Deliberately low: the alternative failure - refusing to measure a
    // frame that does have a horizon - is worse, because it silently withdraws the phase's
    // most reliable measurement.
    if density < 0.002 {
        return None;
    }
    let vertical_parallel = convergence(&vertical) < VERTICAL_PARALLEL_BELOW;
    Some(Votes {
        horizontal,
        vertical,
        density,
        vertical_parallel,
    })
}

/// Find the mode, then take a weighted circular mean around it.
///
/// Returns the refined angle and the share of the total weight the peak's window holds -
/// which is what becomes the confidence. `None` when there is no weight at all.
fn refine(bins: &[f32; BINS]) -> Option<(f32, f32)> {
    let total: f32 = bins.iter().sum();
    if total <= f32::EPSILON {
        return None;
    }
    let (mode_index, _) =
        bins.iter()
            .enumerate()
            .fold((0usize, f32::MIN), |(best_index, best), (index, weight)| {
                if *weight > best {
                    (index, *weight)
                } else {
                    (best_index, best)
                }
            });
    let mode_deg = bin_centre(mode_index);

    // Doubled angles again, so a peak sitting on the wrap at +-45 degrees averages
    // correctly instead of landing at zero.
    let (mut sin_sum, mut cos_sum, mut window) = (0.0f32, 0.0f32, 0.0f32);
    for (index, weight) in bins.iter().enumerate() {
        let centre = bin_centre(index);
        if wrap_signed(centre - mode_deg).abs() > REFINE_WINDOW_DEG {
            continue;
        }
        let doubled = 2.0 * centre.to_radians();
        sin_sum += weight * doubled.sin();
        cos_sum += weight * doubled.cos();
        window += weight;
    }
    if window <= f32::EPSILON {
        return Some((mode_deg, 0.0));
    }
    let refined = sin_sum.atan2(cos_sum).to_degrees() / 2.0;
    Some((refined.clamp(-RANGE_DEG, RANGE_DEG), window / total))
}

/// Share of matching edge weight carried by one long line at `angle_deg`.
///
/// This is the rho half of the weighted Hough transform, evaluated only for the theta
/// mode selected above. Coordinates are centred and normalised, so rho always lies in
/// `-sqrt(1/2)..sqrt(1/2)` regardless of the proxy dimensions.
fn line_share(plane: &LumaPlane, angle_deg: f32) -> f32 {
    if plane.width < 8 || plane.height < 8 {
        return 0.0;
    }
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };
    let angle = angle_deg.to_radians();
    let normal = (-angle.sin(), angle.cos());
    let rho_radius = std::f32::consts::FRAC_1_SQRT_2;
    let mut bins = [0.0f32; RHO_BINS];

    for y in 1..plane.height - 1 {
        for x in 1..plane.width - 1 {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            let magnitude = gx.hypot(gy);
            if magnitude < EDGE_FLOOR {
                continue;
            }
            let edge_deg = wrap_signed(gy.atan2(gx).to_degrees() + 90.0);
            if wrap_signed(edge_deg - angle_deg).abs() > REFINE_WINDOW_DEG {
                continue;
            }

            let nx = x as f32 / (plane.width - 1) as f32 - 0.5;
            let ny = y as f32 / (plane.height - 1) as f32 - 0.5;
            let rho = nx.mul_add(normal.0, ny * normal.1);
            let position = ((rho + rho_radius) / (2.0 * rho_radius) * RHO_BINS as f32)
                .floor()
                .clamp(0.0, (RHO_BINS - 1) as f32) as usize;
            if let Some(slot) = bins.get_mut(position) {
                *slot += magnitude;
            }
        }
    }

    let total: f32 = bins.iter().sum();
    if total <= f32::EPSILON {
        return 0.0;
    }
    let strongest = bins
        .windows(RHO_WINDOW)
        .map(|window| window.iter().sum::<f32>())
        .fold(0.0f32, f32::max);
    (strongest / total).clamp(0.0, 1.0)
}

/// Add one weighted vote, splitting it between the two nearest bins.
///
/// Linear splitting rather than nearest-bin, because nearest-bin quantises every vote to
/// a quarter of a degree before the refinement pass ever sees it - and the refinement pass
/// is the only reason the 0.4 degree gate is reachable.
fn add(bins: &mut [f32; BINS], degrees: f32, weight: f32) {
    let position = (degrees + RANGE_DEG) / BIN_DEG - 0.5;
    let lower = position.floor();
    let fraction = position - lower;
    let low_index = lower as i32;
    for (index, share) in [(low_index, 1.0 - fraction), (low_index + 1, fraction)] {
        let wrapped =
            usize::try_from(index.rem_euclid(i32::try_from(BINS).unwrap_or(i32::MAX))).unwrap_or(0);
        if let Some(slot) = bins.get_mut(wrapped) {
            *slot += weight * share;
        }
    }
}

/// The angle at the centre of one bin.
fn bin_centre(index: usize) -> f32 {
    (index as f32 + 0.5) * BIN_DEG - RANGE_DEG
}

/// Wrap an angle into `-90..90`, which is the range a line direction lives in.
fn wrap_signed(degrees: f32) -> f32 {
    let mut value = degrees % 180.0;
    if value > 90.0 {
        value -= 180.0;
    } else if value < -90.0 {
        value += 180.0;
    }
    value
}
