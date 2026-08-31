//! The smallest correction that makes two bodies agree, bounded on every axis and checked against
//! evidence it never saw.
//!
//! Section 8 steps 4 and 5, and section 6.2. Three things happen here in a fixed order, and the
//! order is the safety property: **fit**, then **verify**, then **blend**. A transform that is
//! blended before it is verified is a transform whose held-out check was run against a different
//! answer from the one that ships.
//!
//! ## Ten parameters are fitted and three are derived
//!
//! [`fit`] runs a bounded coordinate descent over ten numbers: temperature, tint, exposure,
//! saturation, three contrast multipliers, two skin chromaticity offsets and a skin luminance
//! offset. Every one of them is observable in [`PairReading`] - move it and the objective changes.
//!
//! The three per-channel gains are **derived from the two fingerprints** instead, by
//! [`channel_gain`], and that is a deliberate refusal rather than a simplification. A matched pair
//! carries a white point, a skin reading, a grade signature and a contrast number; none of those
//! separates a red gain from a green one. Fitting a parameter no observation constrains produces a
//! parameter that takes whatever value the optimiser's step order happens to hand it - it would
//! reduce the objective by nothing, vary between runs, and reach a photograph as a ten per cent
//! shift in every red in the frame. Deriving it from where the two bodies put a neutral is a
//! smaller claim that is actually supported.
//!
//! ## Coordinate descent rather than least squares, and why that is not a shortcut
//!
//! Section 6.2 says "bounded least squares over the small transform vector". The objective here is
//! not least squares in the parameters: `skin_de00` runs through a CIEDE2000 evaluation, the
//! temperature axis moves a chromaticity along the Planckian locus, and both are non-linear enough
//! that a normal-equations solve would be solving a different problem from the one being measured.
//! A bounded coordinate descent evaluates the **real** objective at every step, cannot leave the
//! box, and converges in a few dozen evaluations on ten parameters. ADR-0053 section 5 records the
//! substitution.
//!
//! The descent is deterministic: a fixed axis order, a fixed number of sweeps, a fixed grid per
//! sweep, no randomness anywhere. Invariant 4, and it is what makes the held-out check reproducible.
//!
//! ## Two hard constraints the objective cannot buy its way past
//!
//! **The skin locus.** Phase 15's own constraint, reused rather than re-derived: a candidate that
//! puts a known person's skin outside the region their own frames say skin lives in is rejected
//! before its score is looked at. Section 6.2, and it is the reason a metric that would happily
//! trade a magenta cast for two tenths of a dE00 cannot.
//!
//! **The bounds.** Every axis is clamped to the policy table's ceiling, which is itself at or below
//! the contract's. A descent that wants to go further stops at the edge and records which edge, so
//! `CameraCode::BoundedByPolicy` is a fact rather than an inference from the numbers.

use aura_core::contract::camera::{
    AppearanceDistance, CameraCode, CameraFingerprint, CameraReason, CameraTransform, FlashState,
    SkinCorrection, TransformBound, TransformSource, MIN_HELDOUT_IMPROVEMENT, MIN_HELDOUT_PAIRS,
    MIN_SOLVE_PAIRS, SKIN_LUMA_CAP, SKIN_UV_CAP,
};
use aura_core::contract::moment::CameraId;
use aura_core::contract::tone::SkinLocus;

use super::baseline::{self, Departure, Library};
use super::policy::Matching;
use super::transform::{self, PairReading};
use super::ANALYSIS_VER;

/// How many times the descent sweeps every axis.
///
/// Four. The first sweep moves each axis from zero to roughly the right place, the second and third
/// resolve the interactions between temperature and skin, and the fourth almost never moves
/// anything - it is there so that "the answer stopped changing" is a fact about the search rather
/// than about the budget. Section 11 allows one second per camera and a sweep is ten axes times
/// [`GRID`] evaluations of a hundred and sixty readings, which is well inside it.
pub const SWEEPS: usize = 4;

/// How many candidate values each axis is tried at, per sweep.
///
/// Nine, spanning the axis's current bracket. An odd number so the current value is always one of
/// the candidates, which is what makes a sweep unable to make the objective worse.
pub const GRID: usize = 9;

/// How much the bracket around an axis shrinks between sweeps.
///
/// A third. Four sweeps therefore resolve each axis to about one part in three hundred of its
/// bound, which is finer than any of these numbers is meaningful to.
pub const SHRINK: f32 = 1.0 / 3.0;

/// The improvement in the objective below which the descent stops early.
///
/// A share of the distance it started at, for phase 22's reason: an absolute threshold on a
/// measurement is a statement about the instrument, and the distance between two Canon bodies and
/// the distance between a Canon and a Fujifilm differ by an order of magnitude.
pub const CONVERGED: f32 = 0.001;

/// The first sweep in which the three skin axes are allowed to move.
///
/// **This is the fix for a defect that shipped in the first implementation and that no unit test
/// would have caught.** Skin and the white point are corrected by overlapping parameters: moving
/// the illuminant moves skin, and the skin offset moves skin on its own. A coordinate descent that
/// may touch both from the first sweep discovers that the *cheapest* way to reduce the skin term -
/// which carries three times the weight of any other - is to nudge the skin offset directly, and
/// once it has, every further move of the temperature axis makes skin worse again. The descent
/// then sits in a local minimum with skin almost perfect and the white point barely corrected,
/// which is a body whose faces match and whose walls do not.
///
/// Holding the skin axes at zero for the first two sweeps makes the ordering real rather than
/// nominal: the illuminant axes converge against the whole error, and the skin offset is then
/// fitted to **what they could not reach**, which is what "a residual" means. The gate that caught
/// it is `gate_1b_white_points_converge_as_well_as_skin`, and it caught it because it measures the
/// two terms separately - a single total would have shown a converged solve.
pub const SKIN_FROM_SWEEP: usize = 2;

/// Which axis a coordinate descent is moving.
///
/// An enum rather than an index so the bound lookup, the clamp and the write-back cannot drift
/// apart - which is the failure that produces a solver that respects nine of its ten bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Cct,
    Tint,
    Exposure,
    Saturation,
    Shadow,
    Mid,
    Highlight,
    SkinU,
    SkinV,
    SkinLuma,
}

impl Axis {
    /// The ten axes, in the order the descent sweeps them.
    ///
    /// Temperature first because it is the largest term in the objective and every other axis is
    /// solved against what it leaves behind; skin last because a skin offset is a residual on top
    /// of the white-balance move rather than an alternative to it, and fitting it first would let
    /// it absorb a temperature error it cannot correct on the rest of the frame.
    const ALL: [Self; 10] = [
        Self::Cct,
        Self::Tint,
        Self::Exposure,
        Self::Saturation,
        Self::Shadow,
        Self::Mid,
        Self::Highlight,
        Self::SkinU,
        Self::SkinV,
        Self::SkinLuma,
    ];

    /// Which bound governs this axis.
    const fn bound(self) -> TransformBound {
        match self {
            Self::Cct => TransformBound::Cct,
            Self::Tint => TransformBound::Tint,
            Self::Exposure => TransformBound::Exposure,
            Self::Saturation => TransformBound::Saturation,
            Self::Shadow | Self::Mid | Self::Highlight => TransformBound::ContrastShape,
            Self::SkinU | Self::SkinV | Self::SkinLuma => TransformBound::Skin,
        }
    }

    /// True when this axis is a ratio around one rather than an offset around zero.
    const fn is_multiplicative(self) -> bool {
        matches!(self, Self::Shadow | Self::Mid | Self::Highlight)
    }

    /// True when this axis is one of the three that move skin directly.
    ///
    /// Held at zero until [`SKIN_FROM_SWEEP`]. See that constant for why being last in
    /// [`Axis::ALL`] is not enough.
    const fn is_skin(self) -> bool {
        matches!(self, Self::SkinU | Self::SkinV | Self::SkinLuma)
    }

    /// Read this axis off a transform.
    fn get(self, t: &CameraTransform) -> f32 {
        match self {
            Self::Cct => t.d_cct,
            Self::Tint => t.d_tint,
            Self::Exposure => t.d_exposure,
            Self::Saturation => t.d_saturation,
            Self::Shadow => t.contrast_shape.first().copied().unwrap_or(1.0),
            Self::Mid => t.contrast_shape.get(1).copied().unwrap_or(1.0),
            Self::Highlight => t.contrast_shape.get(2).copied().unwrap_or(1.0),
            Self::SkinU => t.skin_correction.d_uv[0],
            Self::SkinV => t.skin_correction.d_uv[1],
            Self::SkinLuma => t.skin_correction.d_luma,
        }
    }

    /// Write this axis onto a transform.
    fn set(self, t: &mut CameraTransform, value: f32) {
        match self {
            Self::Cct => t.d_cct = value,
            Self::Tint => t.d_tint = value,
            Self::Exposure => t.d_exposure = value,
            Self::Saturation => t.d_saturation = value,
            Self::Shadow => t.contrast_shape[0] = value,
            Self::Mid => t.contrast_shape[1] = value,
            Self::Highlight => t.contrast_shape[2] = value,
            Self::SkinU => t.skin_correction.d_uv[0] = value,
            Self::SkinV => t.skin_correction.d_uv[1] = value,
            Self::SkinLuma => t.skin_correction.d_luma = value,
        }
    }

    /// The half-width of the box this axis may move inside, under a given policy.
    fn half_width(self, policy: &Matching) -> f32 {
        match self {
            Self::SkinLuma => SKIN_LUMA_CAP.min(policy.bound(TransformBound::Skin) * 4.0),
            // The two skin chromaticity axes share one budget, so each gets the whole cap and the
            // pair is renormalised afterwards - a box rather than a disc during the search, and a
            // disc when the answer is written. Searching inside a disc directly would make each
            // axis's bracket depend on the other's current value, which is a coordinate descent
            // whose steps are not independent.
            Self::SkinU | Self::SkinV => policy.bound(TransformBound::Skin),
            other => policy.bound(other.bound()),
        }
    }
}

/// What a fit produced, before it was verified or blended.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    /// The solved transform.
    pub transform: CameraTransform,
    /// The distance on the fitting pairs before it.
    pub before: AppearanceDistance,
    /// The distance on the fitting pairs after it.
    pub after: AppearanceDistance,
    /// Which bound the descent stopped against, when one did.
    pub bounded: Option<TransformBound>,
    /// How many candidate steps the skin locus rejected.
    ///
    /// Nonzero means the metric wanted to go somewhere the constraint would not allow, which is
    /// [`CameraCode::SkinLocusRefused`] and is the constraint doing its job rather than a failure.
    pub locus_refusals: u32,
}

/// Fit a transform on a set of readings, bounded and locus-constrained.
///
/// `None` when there are fewer than [`MIN_SOLVE_PAIRS`] readings: below that there is nothing to
/// fit, and the caller falls back on the brand baseline. A fit on three pairs of a ten-parameter
/// model is not a weak answer, it is an arbitrary one.
#[must_use]
pub fn fit(
    camera: &CameraId,
    flash: FlashState,
    reference: &CameraId,
    readings: &[PairReading],
    loci: &[SkinLocus],
    policy: &Matching,
) -> Option<Fit> {
    if readings.len() < MIN_SOLVE_PAIRS as usize {
        return None;
    }

    let mut current = CameraTransform::identity(
        camera.clone(),
        flash,
        reference.clone(),
        ANALYSIS_VER,
        policy.version,
    );
    let before = transform::measure(readings, None);
    let mut best_score = before.total();
    let mut bounded: Option<TransformBound> = None;
    let mut locus_refusals = 0_u32;

    for sweep in 0..SWEEPS {
        let scale = SHRINK.powi(i32::try_from(sweep).unwrap_or(0));
        let mut improved = 0.0_f32;

        for axis in Axis::ALL {
            if axis.is_skin() && sweep < SKIN_FROM_SWEEP {
                continue;
            }
            let half = axis.half_width(policy);
            let centre = axis.get(&current);
            let identity = if axis.is_multiplicative() { 1.0 } else { 0.0 };
            let reach = half * scale;

            let mut best_value = centre;
            for step in 0..GRID {
                #[allow(clippy::cast_precision_loss)]
                let t = (step as f32) / ((GRID - 1) as f32) * 2.0 - 1.0;
                let raw = centre + t * reach;
                // The clamp is against the *identity*, so a multiplicative axis is bounded to
                // `1 ± half` and an additive one to `0 ± half`. Doing this arithmetic in one place
                // is what stops a solver respecting nine of its ten bounds.
                let value = raw.clamp(identity - half, identity + half);

                let mut candidate = current.clone();
                axis.set(&mut candidate, value);
                normalise_skin(&mut candidate, policy);

                if !locus_admits(&candidate, readings, loci) {
                    locus_refusals += 1;
                    continue;
                }

                let score = transform::measure(readings, Some(&candidate)).total();
                if score + f32::EPSILON < best_score {
                    best_score = score;
                    best_value = value;
                }
            }

            let moved = (best_value - centre).abs();
            if moved > 0.0 {
                improved += moved / half.max(f32::EPSILON);
            }
            axis.set(&mut current, best_value);
            normalise_skin(&mut current, policy);

            // "At the edge" rather than "past it": the clamp already stopped the value going
            // further, so the only way to know the descent wanted more is that it came to rest on
            // the boundary. Recorded once, for the first axis it happens on, because a report that
            // named six bounds would be a report nobody reads.
            let at_edge = (best_value - identity).abs() >= half - half * 1e-3;
            if at_edge && bounded.is_none() && half > 0.0 {
                bounded = Some(axis.bound());
            }
        }

        // The early stop may not fire before the skin axes have had a sweep of their own: a solve
        // that converged on the illuminant in two sweeps would otherwise never fit skin at all.
        if improved < CONVERGED && sweep + 1 >= SKIN_FROM_SWEEP {
            break;
        }
    }

    let after = transform::measure(readings, Some(&current));
    current.skin_correction.capped = skin_is_capped(&current, policy);
    current.skin_correction.locus_valid =
        locus_refusals == 0 || locus_admits(&current, readings, loci);

    Some(Fit {
        transform: current,
        before,
        after,
        bounded,
        locus_refusals,
    })
}

/// Keep a skin correction inside its disc, preserving direction.
///
/// The two chromaticity axes are searched independently inside a box and then renormalised onto
/// the disc the contract actually promises. Scaling both components by one factor keeps the
/// direction the descent chose, which a per-axis clamp would not: clamping `u` alone rotates the
/// correction toward `v`, and the direction is the part of a skin correction that says *which way*
/// a body's skin is wrong.
fn normalise_skin(t: &mut CameraTransform, policy: &Matching) {
    let cap = policy.bound(TransformBound::Skin).min(SKIN_UV_CAP);
    let uv = t.skin_correction.d_uv;
    let length = (uv[0] * uv[0] + uv[1] * uv[1]).sqrt();
    if length > cap && length > 1e-9 {
        let scale = cap / length;
        t.skin_correction.d_uv = [uv[0] * scale, uv[1] * scale];
    }
    t.skin_correction.d_luma = t
        .skin_correction
        .d_luma
        .clamp(-SKIN_LUMA_CAP, SKIN_LUMA_CAP);
}

/// True when a skin correction has been reduced by a cap.
fn skin_is_capped(t: &CameraTransform, policy: &Matching) -> bool {
    let cap = policy.bound(TransformBound::Skin).min(SKIN_UV_CAP);
    let uv = t.skin_correction.d_uv;
    let length = (uv[0] * uv[0] + uv[1] * uv[1]).sqrt();
    length >= cap - cap * 1e-3 || t.skin_correction.d_luma.abs() >= SKIN_LUMA_CAP * 0.999
}

/// True when every corrected skin reading stays inside somebody's plausible locus.
///
/// Phase 15's hard constraint, and the reuse is the point: this phase has no idea what skin looks
/// like and must not acquire one. An empty locus set admits everything, which is this build on a
/// real photograph - and that absence is `CameraCode::FingerprintThin` rather than a silent pass,
/// because a constraint that never fires and a constraint that never ran are different facts.
fn locus_admits(t: &CameraTransform, readings: &[PairReading], loci: &[SkinLocus]) -> bool {
    let usable: Vec<&SkinLocus> = loci.iter().filter(|l| l.is_usable()).collect();
    if usable.is_empty() {
        return true;
    }
    for reading in readings {
        let Some((uv, _)) = reading.right_skin else {
            continue;
        };
        let moved = transform::shift_uv(uv, t.d_cct, t.d_tint);
        let corrected = [
            moved.first().copied().unwrap_or(0.0) + t.skin_correction.d_uv[0],
            moved.get(1).copied().unwrap_or(0.0) + t.skin_correction.d_uv[1],
        ];
        // Inside *any* person's locus is enough. The alternative - inside the locus of the person
        // actually in the frame - needs a per-pair identity this reading does not carry, and the
        // strict reading would reject a correct correction on a frame of a guest whose own locus
        // was never measured.
        if !usable.iter().any(|locus| locus.contains(corrected)) {
            return false;
        }
    }
    true
}

/// Whether a fit improved evidence it never saw, and by how much.
///
/// Section 6.2's held-out verification. Returns the two distances and the verdict, where the
/// verdict is `None` when there were fewer than [`MIN_HELDOUT_PAIRS`] to check against - "we could
/// not check" is a third state and collapsing it into either of the other two is how a product
/// claims verification it did not do.
#[must_use]
pub fn verify(
    fit: &Fit,
    heldout: &[PairReading],
) -> (AppearanceDistance, AppearanceDistance, Option<bool>) {
    let before = transform::measure(heldout, None);
    if heldout.len() < MIN_HELDOUT_PAIRS as usize {
        return (before, before, None);
    }
    let after = transform::measure(heldout, Some(&fit.transform));
    let improved = before.reduction_to(&after) >= MIN_HELDOUT_IMPROVEMENT;
    (before, after, Some(improved))
}

/// How much of a solved answer survives, given how much evidence there was.
///
/// Section 6.1: "below that, blend with the brand baseline proportionally to evidence." Linear from
/// [`MIN_SOLVE_PAIRS`] to the policy's own `min_pairs`, and one at or above it. Zero below
/// [`MIN_SOLVE_PAIRS`], which is the case where no fit was attempted at all.
///
/// The blend is over the **parameters** rather than over the two candidate outputs, and the two are
/// the same thing only because every axis here is either an offset or a ratio and both interpolate
/// sensibly. That is worth stating because it stops being true the moment somebody adds a
/// non-linear axis - a curve, a matrix - and the blend would then have to move to the output.
#[must_use]
pub fn evidence_weight(pairs: u32, policy: &Matching) -> f32 {
    if pairs < MIN_SOLVE_PAIRS {
        return 0.0;
    }
    if pairs >= policy.min_pairs {
        return 1.0;
    }
    let span = f32::from(u16::try_from(policy.min_pairs - MIN_SOLVE_PAIRS).unwrap_or(1)).max(1.0);
    let over = f32::from(u16::try_from(pairs - MIN_SOLVE_PAIRS).unwrap_or(0));
    (over / span).clamp(0.0, 1.0)
}

/// Blend a solved transform toward a brand baseline, in place.
///
/// `weight` is [`evidence_weight`]: one keeps the solved answer whole, zero replaces it with the
/// baseline. The additive axes interpolate and the multiplicative ones interpolate in the ratio -
/// `a^(1-w) * b^w` would be the strictly correct interpolation of two ratios, and a linear one is
/// within a thousandth of it over the ten and fifteen per cent ranges these axes are bounded to,
/// which is far below anything visible. The linear form is used because it is the one a
/// photographer reading `blend = 0.4` in a report can check with a calculator.
pub fn blend(transform: &mut CameraTransform, toward: Departure, weight: f32) {
    let w = weight.clamp(0.0, 1.0);
    let mix = |solved: f32, base: f32| solved * w + base * (1.0 - w);
    transform.d_cct = mix(transform.d_cct, toward.d_cct);
    transform.d_tint = mix(transform.d_tint, toward.d_tint);
    transform.d_exposure = mix(transform.d_exposure, toward.d_exposure);
    transform.d_saturation = mix(transform.d_saturation, toward.d_saturation);
    // Iterate the arrays rather than index them, so `clippy::indexing_slicing` stays denied in this
    // crate. Three channels, both arrays, in the same pass.
    for (index, gain) in transform.channel_gain.iter_mut().enumerate() {
        *gain = mix(
            *gain,
            toward.channel_gain.get(index).copied().unwrap_or(1.0),
        );
    }
    for (index, shape) in transform.contrast_shape.iter_mut().enumerate() {
        *shape = mix(
            *shape,
            toward.contrast_shape.get(index).copied().unwrap_or(1.0),
        );
    }
    transform.skin_correction.d_uv = [
        mix(transform.skin_correction.d_uv[0], toward.skin_uv[0]),
        mix(transform.skin_correction.d_uv[1], toward.skin_uv[1]),
    ];
    transform.skin_correction.d_luma = mix(transform.skin_correction.d_luma, toward.skin_luma);
    transform.blend = w;
    transform.source = if w >= 1.0 {
        TransformSource::MatchedPairs
    } else if w <= 0.0 {
        TransformSource::BrandBaseline
    } else {
        TransformSource::Blended
    };
}

/// The per-channel gain that maps one body's neutral onto another's.
///
/// **Derived rather than fitted.** See the module header: a matched pair carries no per-channel
/// observation, so a fitted gain would be a parameter nothing constrains, and a parameter nothing
/// constrains reaches a photograph as a ten per cent shift in every red in the frame.
///
/// What is derived is small and defensible: the two bodies' white points are converted to linear
/// RGB, the ratio is taken channel by channel, the result is normalised so the green channel is one
/// - a gain triple is only meaningful up to a scale, and letting all three drift together would be
///   an exposure change wearing a colour change's clothes - and then bounded.
#[must_use]
pub fn channel_gain(
    reference: &CameraFingerprint,
    body: &CameraFingerprint,
    policy: &Matching,
) -> [f32; 3] {
    use aura_raw::colour::illuminant::uv_to_linear_srgb;
    let want = uv_to_linear_srgb(reference.white_point);
    let have = uv_to_linear_srgb(body.white_point);
    let cap = policy.bound(TransformBound::ChannelGain);
    let mut gain = [1.0_f32; 3];
    let green = {
        let h = have.get(1).copied().unwrap_or(1.0);
        let w = want.get(1).copied().unwrap_or(1.0);
        if h.abs() < 1e-6 || w.abs() < 1e-6 {
            1.0
        } else {
            w / h
        }
    };
    for index in 0..3 {
        let h = have.get(index).copied().unwrap_or(1.0);
        let w = want.get(index).copied().unwrap_or(1.0);
        let raw = if h.abs() < 1e-6 { 1.0 } else { w / h };
        let normalised = if green.abs() < 1e-6 { 1.0 } else { raw / green };
        if let Some(slot) = gain.get_mut(index) {
            *slot = normalised.clamp(1.0 - cap, 1.0 + cap);
        }
    }
    gain
}

/// The transform a body gets when there is nothing in the wedding to solve from.
///
/// Section 6.1's fallback path. The composed baseline between the two brands, with
/// [`CameraCode::BaselineOnly`] or [`CameraCode::BaselineUnknownBrand`] beside it - the second when
/// this build has no measurements for the manufacturer at all, in which case the answer is the
/// identity and the report says AURA changed nothing rather than guessing.
#[must_use]
pub fn from_baseline(
    camera: &CameraId,
    flash: FlashState,
    reference: &CameraId,
    reference_brand: aura_core::contract::camera::Brand,
    body_brand: aura_core::contract::camera::Brand,
    library: &Library,
    policy: &Matching,
) -> CameraTransform {
    let mut out = CameraTransform::identity(
        camera.clone(),
        flash,
        reference.clone(),
        ANALYSIS_VER,
        policy.version,
    );
    // A composition is only meaningful when **both** ends are measured. Composing an unknown body
    // with a known reference would apply the reference brand's whole departure to it, which is the
    // assumption that the unknown body renders exactly on the neutral - a guess, and precisely the
    // guess `Brand::Other` exists to refuse. So an unknown brand on either side is the identity.
    let known = library.knows(body_brand) && library.knows(reference_brand);
    let (departure, bound) = if known {
        baseline::between(library, body_brand, reference_brand, flash)
    } else {
        (Departure::NEUTRAL, None)
    };
    departure.write_into(&mut out);
    out.bounded = bound;
    out.source = TransformSource::BrandBaseline;
    out.blend = 0.0;
    out.evidence_pairs = 0;
    out.skin_correction.locus_valid = true;
    let mut reasons = Vec::new();
    if known {
        reasons.push(CameraReason::of(CameraCode::BaselineOnly));
    } else {
        reasons.push(CameraReason::of(CameraCode::BaselineUnknownBrand));
    }
    if bound.is_some() {
        reasons.push(CameraReason::of(CameraCode::BoundedByPolicy));
    }
    out.reasons = reasons;
    // A baseline is a general statement about a manufacturer and never a measurement of this
    // wedding, so its confidence is capped well below anything a solve can reach. The number is
    // low rather than zero because the correction is still better than nothing when the brands
    // genuinely differ - and it is zero when the brand is unknown, because then it *is* nothing.
    out.confidence = if known { 0.35 } else { 0.0 };
    out
}

/// The skin correction a fit produced, with the two dE00 measurements section 10.1 gates on.
#[must_use]
pub fn skin_report(
    fit: &CameraTransform,
    before: AppearanceDistance,
    after: AppearanceDistance,
) -> SkinCorrection {
    SkinCorrection {
        d_uv: fit.skin_correction.d_uv,
        d_luma: fit.skin_correction.d_luma,
        de00_before: before.skin_de00,
        de00_after: after.skin_de00,
        locus_valid: fit.skin_correction.locus_valid,
        capped: fit.skin_correction.capped,
    }
}

#[cfg(test)]
mod tests {
    use aura_raw::colour::illuminant::cct_to_uv;

    use super::*;

    fn readings(reference_cct: f32, body_cct: f32, count: usize) -> Vec<PairReading> {
        (0..count)
            .map(|i| {
                // A little honest variation, so the fit is not solving a single repeated equation.
                #[allow(clippy::cast_precision_loss)]
                let jitter = (i as f32 % 5.0 - 2.0) * 12.0;
                PairReading {
                    left_skin: None,
                    right_skin: None,
                    left_white: cct_to_uv(reference_cct + jitter),
                    right_white: cct_to_uv(body_cct + jitter),
                    left_cct: reference_cct + jitter,
                    right_cct: body_cct + jitter,
                    left_signature: [0.10; 8],
                    right_signature: [0.10; 8],
                    left_contrast: 12.0,
                    right_contrast: 12.0,
                    left_luma: 0.45,
                    right_luma: 0.45,
                }
            })
            .collect()
    }

    fn solved(reference_cct: f32, body_cct: f32, count: usize) -> Fit {
        fit(
            &CameraId::new("cam_b"),
            FlashState::Ambient,
            &CameraId::new("cam_a"),
            &readings(reference_cct, body_cct, count),
            &[],
            &Matching::default(),
        )
        .expect("enough readings")
    }

    #[test]
    fn a_fit_recovers_a_known_temperature_difference() {
        let fit = solved(5200.0, 4700.0, 30);
        assert!(fit.after.total() < fit.before.total());
        assert!(
            fit.transform.d_cct > 250.0,
            "the body is cooler than the reference; d_cct {}",
            fit.transform.d_cct
        );
        assert!(fit.transform.within_bounds());
    }

    #[test]
    fn a_fit_is_deterministic() {
        let a = solved(5200.0, 4700.0, 30);
        let b = solved(5200.0, 4700.0, 30);
        assert_eq!(a.transform.d_cct, b.transform.d_cct);
        assert_eq!(a.transform.d_tint, b.transform.d_tint);
        assert_eq!(a.after, b.after);
    }

    #[test]
    fn a_fit_on_too_few_readings_is_refused_rather_than_weak() {
        assert!(fit(
            &CameraId::new("cam_b"),
            FlashState::Ambient,
            &CameraId::new("cam_a"),
            &readings(5200.0, 4700.0, MIN_SOLVE_PAIRS as usize - 1),
            &[],
            &Matching::default(),
        )
        .is_none());
    }

    #[test]
    fn a_fit_never_leaves_its_box_and_says_which_edge_it_stopped_at() {
        // Two bodies 3,000 K apart, which is more than any bound allows. The answer must be at the
        // ceiling and must name it, rather than being at the ceiling and looking like a choice.
        let fit = solved(7000.0, 4000.0, 30);
        assert!(fit.transform.within_bounds());
        let policy = Matching::default();
        assert!(
            (fit.transform.d_cct - policy.bound(TransformBound::Cct)).abs() < 1.0,
            "d_cct {}",
            fit.transform.d_cct
        );
        assert_eq!(fit.bounded, Some(TransformBound::Cct));
    }

    #[test]
    fn two_identical_bodies_are_left_alone() {
        let fit = solved(5200.0, 5200.0, 30);
        assert!(
            fit.transform.magnitude() < 0.05,
            "magnitude {}",
            fit.transform.magnitude()
        );
    }

    #[test]
    fn a_locus_that_refuses_everything_stops_the_skin_axes_moving() {
        use aura_core::IdentityId;
        // A locus centred somewhere no correction can reach. Every candidate that moves skin is
        // refused, and the fit still returns - the constraint reduces what is done rather than
        // failing the body.
        let locus = SkinLocus {
            identity: IdentityId::new(),
            uv: [0.9, 0.9],
            radius: SkinLocus::MIN_RADIUS,
            luma: 0.5,
            samples: 20,
            cohesion: 0.9,
            analysis_ver: 1,
        };
        let mut with_skin = readings(5200.0, 4700.0, 30);
        for reading in &mut with_skin {
            reading.left_skin = Some(([0.24, 0.50], 0.5));
            reading.right_skin = Some(([0.23, 0.49], 0.5));
        }
        let fit = fit(
            &CameraId::new("cam_b"),
            FlashState::Ambient,
            &CameraId::new("cam_a"),
            &with_skin,
            &[locus],
            &Matching::default(),
        )
        .expect("enough readings");
        assert!(fit.locus_refusals > 0, "the constraint never fired");
        assert!(
            fit.transform.is_identity(),
            "nothing may move past the locus"
        );
    }

    #[test]
    fn held_out_verification_has_three_states_and_not_two() {
        let fit = solved(5200.0, 4700.0, 30);
        let (_, _, none) = verify(&fit, &readings(5200.0, 4700.0, 1));
        assert_eq!(
            none, None,
            "a check that did not run is not a check that passed"
        );

        let (_, _, pass) = verify(&fit, &readings(5200.0, 4700.0, 10));
        assert_eq!(pass, Some(true));

        // Held-out evidence that says the opposite of the fitting evidence: the body is *warmer*
        // than the reference here, so a transform that warms it further makes things worse.
        let (_, _, fail) = verify(&fit, &readings(4700.0, 5200.0, 10));
        assert_eq!(fail, Some(false));
    }

    #[test]
    fn the_evidence_weight_is_zero_below_the_floor_and_one_at_the_threshold() {
        let policy = Matching::default();
        assert_eq!(evidence_weight(MIN_SOLVE_PAIRS - 1, &policy), 0.0);
        assert_eq!(evidence_weight(policy.min_pairs, &policy), 1.0);
        assert_eq!(evidence_weight(policy.min_pairs + 100, &policy), 1.0);
        let mid = evidence_weight(u32::midpoint(MIN_SOLVE_PAIRS, policy.min_pairs), &policy);
        assert!(mid > 0.2 && mid < 0.8, "{mid}");
    }

    #[test]
    fn a_blend_names_itself_honestly_at_both_ends_and_in_the_middle() {
        let mut t = solved(5200.0, 4700.0, 30).transform;
        let full = t.d_cct;
        blend(&mut t, Departure::NEUTRAL, 1.0);
        assert_eq!(t.source, TransformSource::MatchedPairs);
        assert!((t.d_cct - full).abs() < 1e-4);

        blend(&mut t, Departure::NEUTRAL, 0.5);
        assert_eq!(t.source, TransformSource::Blended);
        assert!((t.d_cct - full * 0.5).abs() < 1e-3);

        blend(&mut t, Departure::NEUTRAL, 0.0);
        assert_eq!(t.source, TransformSource::BrandBaseline);
        assert!(t.d_cct.abs() < 1e-4);
    }

    #[test]
    fn a_channel_gain_is_normalised_on_green_and_bounded() {
        let policy = Matching::default();
        let make = |uv: [f32; 2]| CameraFingerprint {
            camera_id: CameraId::new("x"),
            flash: FlashState::Ambient,
            skin_chroma: uv,
            white_point: uv,
            sat_response: [1.0; 4],
            contrast_response: [1.0; 4],
            highlight_rolloff: 0.4,
            samples: 50,
            confidence: 0.8,
            brand: aura_core::contract::camera::Brand::Canon,
            grade_signature: [0.1; 8],
            subject_luma: 0.45,
            reasons: Vec::new(),
            analysis_ver: 1,
        };
        let gain = channel_gain(&make(cct_to_uv(5200.0)), &make(cct_to_uv(3000.0)), &policy);
        assert!(
            (gain[1] - 1.0).abs() < 1e-4,
            "green is the anchor: {gain:?}"
        );
        assert!(gain
            .iter()
            .all(|g| (g - 1.0).abs() <= policy.bound(TransformBound::ChannelGain) + 1e-5));
        // A body identical to the reference needs no gain at all.
        let same = channel_gain(&make(cct_to_uv(5200.0)), &make(cct_to_uv(5200.0)), &policy);
        assert!(same.iter().all(|g| (g - 1.0).abs() < 1e-4));
    }
}
