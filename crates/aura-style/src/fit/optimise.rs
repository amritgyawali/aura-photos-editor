//! Coordinate descent over the twelve parameters a delivered final can be reproduced with.
//!
//! One sweep visits each parameter in turn, tries it at plus and minus the current step, keeps
//! the move if the loss went down, and moves on. When a whole sweep improves nothing, the step
//! halves. When the step is below its floor, or the budget is spent, the fit stops.
//!
//! ## Why the parameters are visited in this order
//!
//! Exposure, then white balance, then the tone parameters, then the two curve anchors, then
//! vibrance and saturation. It is the order a person edits in and it is also the order of
//! decreasing influence on the loss, which matters for a fixed budget: the first sweep with a
//! large step should find the exposure, because every subsequent parameter is being fitted
//! against a frame whose overall brightness is now roughly right.
//!
//! Fitting saturation before exposure - which an alphabetical order would do - spends the first
//! and most valuable sweep compensating for a brightness error with a colour parameter, and the
//! fit then has to unwind it.
//!
//! ## The steps are per parameter and they are not a scale factor
//!
//! [`STEPS`] is in each parameter's own units: a third of a stop, four hundred kelvin, eight
//! points of contrast. A single fractional step would move exposure by 0.003 stops and
//! temperature by 16 K on the same sweep - one of them invisible and the other pointless - and
//! the sweep would spend its whole budget discovering that.
//!
//! ## Determinism
//!
//! There is no randomness here, no time, and no map iteration. The same original, the same
//! target and the same budget produce the same twelve numbers on every machine, which is
//! invariant 4 and is also what makes `tests/eval/style_eval.rs` able to assert an exact
//! recovery.

use std::sync::Arc;

use aura_core::AuraResult;
use aura_recipe::Recipe;
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::Frame;
use aura_render::{CpuEngine, RenderedData};

use crate::extract::TheirParams;

/// How many parameters the fit recovers.
///
/// Twelve. The eight HSL bands are **not** among them; `crate::extract`'s header has the
/// argument, and it is short: twenty-four more parameters recovered from one photograph is a
/// system with far more freedom than the data constrains, and what it recovers is noise wearing
/// a band name.
pub const PARAMS: usize = 12;

/// The initial step for each parameter, in its own units.
///
/// Indexed by [`Axis`]'s order. See this module's header for why these are not one number.
pub const STEPS: [f32; PARAMS] = [
    0.33,  // exposure, in stops - a third is the smallest a photographer would set
    400.0, // temperature, in kelvin
    6.0,   // tint
    8.0,   // contrast
    8.0,   // highlights
    8.0,   // shadows
    6.0,   // whites
    6.0,   // blacks
    16.0,  // curve low anchor, in 0-255 output units
    16.0,  // curve high anchor
    8.0,   // vibrance
    8.0,   // saturation
];

/// The bounds each parameter is searched inside, in its own units.
///
/// The recipe's own ranges, except for temperature, which is bounded to the range a wedding is
/// actually shot in rather than to the schema's 2,000-50,000 K. A fitter allowed to reach
/// 50,000 K will use it: a heavy magenta grade is cheaper to reproduce with an absurd white
/// balance than with the tint parameter, and the recovered "style" is then a temperature nobody
/// set.
pub const BOUNDS: [(f32, f32); PARAMS] = [
    (-5.0, 5.0),
    (2000.0, 12000.0),
    (-100.0, 100.0),
    (-100.0, 100.0),
    (-100.0, 100.0),
    (-100.0, 100.0),
    (-100.0, 100.0),
    (-100.0, 100.0),
    (-64.0, 64.0),
    (-64.0, 64.0),
    (-100.0, 100.0),
    (-100.0, 100.0),
];

/// Below this fraction of its initial value a step stops shrinking and the fit ends.
///
/// An eighth, so a step halves at most three times. A fourth halving would move exposure by a
/// fortieth of a stop, which is below what the renderer's own 8-bit output can express - the fit
/// would be optimising against quantisation.
pub const MIN_STEP_FRACTION: f32 = 0.125;

/// Which parameter one column of the vector is.
///
/// Named rather than indexed by a bare integer, because a twelve-element array whose meaning is
/// positional is a twelve-element array somebody eventually reorders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    /// Exposure in stops.
    Exposure,
    /// Colour temperature in kelvin.
    Temperature,
    /// Green-magenta tint.
    Tint,
    /// Contrast.
    Contrast,
    /// Highlight recovery.
    Highlights,
    /// Shadow lift.
    Shadows,
    /// White point.
    Whites,
    /// Black point.
    Blacks,
    /// The curve's output offset at input 64.
    CurveLow,
    /// The curve's output offset at input 192.
    CurveHigh,
    /// Vibrance.
    Vibrance,
    /// Flat saturation.
    Saturation,
}

impl Axis {
    /// Every axis, in the order the sweep visits them.
    pub const ALL: [Self; PARAMS] = [
        Self::Exposure,
        Self::Temperature,
        Self::Tint,
        Self::Contrast,
        Self::Highlights,
        Self::Shadows,
        Self::Whites,
        Self::Blacks,
        Self::CurveLow,
        Self::CurveHigh,
        Self::Vibrance,
        Self::Saturation,
    ];

    /// Position in [`Axis::ALL`], which is the index into every array in this module.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The stable slug, for a report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exposure => "exposure",
            Self::Temperature => "temperature",
            Self::Tint => "tint",
            Self::Contrast => "contrast",
            Self::Highlights => "highlights",
            Self::Shadows => "shadows",
            Self::Whites => "whites",
            Self::Blacks => "blacks",
            Self::CurveLow => "curve_low",
            Self::CurveHigh => "curve_high",
            Self::Vibrance => "vibrance",
            Self::Saturation => "saturation",
        }
    }
}

/// One point in the search: the twelve numbers, in [`Axis::ALL`]'s order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// The twelve values.
    pub values: [f32; PARAMS],
}

impl Default for Candidate {
    fn default() -> Self {
        Self::neutral()
    }
}

impl Candidate {
    /// The candidate that changes nothing: a neutral develop at 5,500 K.
    ///
    /// It is the start point, and it is also the honest answer for a photograph the fitter
    /// could not improve on - which happens, and means the photographer delivered something
    /// very close to the camera's own rendering.
    #[must_use]
    pub const fn neutral() -> Self {
        // Written out rather than indexed, in [`Axis::ALL`]'s order: `clippy::indexing_slicing`
        // is denied in this crate and a literal is clearer than an index into a constant anyway.
        Self {
            values: [
                0.0,    // exposure
                5500.0, // temperature
                0.0,    // tint
                0.0,    // contrast
                0.0,    // highlights
                0.0,    // shadows
                0.0,    // whites
                0.0,    // blacks
                0.0,    // curve low
                0.0,    // curve high
                0.0,    // vibrance
                0.0,    // saturation
            ],
        }
    }

    /// One axis's value.
    #[must_use]
    pub fn get(&self, axis: Axis) -> f32 {
        self.values.get(axis.index()).copied().unwrap_or(0.0)
    }

    /// Set one axis's value, clamped into its bounds.
    pub fn set(&mut self, axis: Axis, value: f32) {
        let (low, high) = BOUNDS.get(axis.index()).copied().unwrap_or((-100.0, 100.0));
        if let Some(slot) = self.values.get_mut(axis.index()) {
            *slot = value.clamp(low, high);
        }
    }

    /// The photographer's parameters this candidate represents.
    ///
    /// The two curve anchors become a four-point curve through
    /// [`aura_core::contract::colour::ToneCurve::new`], which **refuses a non-monotone set** -
    /// so a candidate whose low anchor has been pushed above its high anchor produces the
    /// identity curve rather than a posterising one. The search then sees no improvement and
    /// backs out of that direction on its own, which is exactly the behaviour wanted: the
    /// constraint is enforced by the type and the optimiser does not need to know about it.
    #[must_use]
    pub fn to_params(self) -> TheirParams {
        use aura_core::contract::colour::{CurvePoint, ToneCurve};

        let low = (64.0 + self.get(Axis::CurveLow)).clamp(0.0, 255.0) as u16;
        let high = (192.0 + self.get(Axis::CurveHigh)).clamp(0.0, 255.0) as u16;
        let curve = ToneCurve::new(vec![
            CurvePoint::new(0, 0),
            CurvePoint::new(64, low),
            CurvePoint::new(192, high),
            CurvePoint::new(255, 255),
        ])
        .unwrap_or_else(|_| ToneCurve::identity());

        TheirParams {
            exposure: self.get(Axis::Exposure),
            temperature_k: self.get(Axis::Temperature),
            tint: self.get(Axis::Tint),
            contrast: self.get(Axis::Contrast),
            highlights: self.get(Axis::Highlights),
            shadows: self.get(Axis::Shadows),
            whites: self.get(Axis::Whites),
            blacks: self.get(Axis::Blacks),
            vibrance: self.get(Axis::Vibrance),
            saturation: self.get(Axis::Saturation),
            curve,
            ..TheirParams::default()
        }
    }

    /// The candidate that reproduces one set of parameters, for the fixtures and the gate.
    #[must_use]
    pub fn of_params(params: &TheirParams) -> Self {
        let mut out = Self::neutral();
        out.set(Axis::Exposure, params.exposure);
        out.set(Axis::Temperature, params.temperature_k);
        out.set(Axis::Tint, params.tint);
        out.set(Axis::Contrast, params.contrast);
        out.set(Axis::Highlights, params.highlights);
        out.set(Axis::Shadows, params.shadows);
        out.set(Axis::Whites, params.whites);
        out.set(Axis::Blacks, params.blacks);
        out.set(Axis::Vibrance, params.vibrance);
        out.set(Axis::Saturation, params.saturation);
        let points = params.curve.points();
        for point in points {
            if point.x == 64 {
                out.set(Axis::CurveLow, f32::from(point.y) - 64.0);
            } else if point.x == 192 {
                out.set(Axis::CurveHigh, f32::from(point.y) - 192.0);
            }
        }
        out
    }
}

/// How much work one fit may do.
///
/// Section 6.1's "typically 60-120 render iterations at 512 px", as a ceiling rather than a
/// target: the fit stops when a sweep improves nothing, which on a lightly graded frame happens
/// after about thirty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    /// The most renders one fit may spend.
    pub max_renders: u32,
    /// The most sweeps it may take.
    pub max_sweeps: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_renders: 160,
            max_sweeps: 8,
        }
    }
}

impl Budget {
    /// A budget for the phase gate and the fixtures: enough to converge, small enough to run in
    /// a test.
    #[must_use]
    pub const fn quick() -> Self {
        Self {
            max_renders: 96,
            max_sweeps: 5,
        }
    }
}

/// Fit the twelve parameters that best reproduce `target` from `frame`.
///
/// `base` is the neutral recipe for this photograph - the right content hash and camera - and
/// every candidate is that recipe with the twelve values written into it. **The candidate is
/// rendered by the real engine**, so a parameter this fit recovers is a parameter
/// `RenderService` will honour; a reimplementation of the pipeline here would recover
/// parameters for a renderer that does not exist.
///
/// # Errors
///
/// Whatever the engine raised. A render that fails ends the fit at whatever it had, which is
/// reported as a residual rather than swallowed.
pub fn fit(
    engine: &CpuEngine,
    frame: &Frame,
    base: &Recipe,
    target: &[u8],
    budget: Budget,
) -> AuraResult<super::Fitted> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let output = OutputSpec::default();

    let mut best = Candidate::neutral();
    let mut renders = 0_u32;
    let (mut best_loss, mut best_mean, mut best_worst) = measure(
        engine,
        frame,
        base,
        &best,
        target,
        width,
        height,
        &output,
        &mut renders,
    )?;

    let mut scale = 1.0_f32;
    let mut sweep = 0_u32;
    while sweep < budget.max_sweeps && renders < budget.max_renders {
        let mut improved = false;
        for axis in Axis::ALL {
            let step = STEPS.get(axis.index()).copied().unwrap_or(1.0) * scale;
            if step <= f32::EPSILON {
                continue;
            }
            for direction in [1.0_f32, -1.0] {
                if renders >= budget.max_renders {
                    break;
                }
                let mut trial = best;
                trial.set(axis, best.get(axis) + step * direction);
                // `set` clamps, so a trial at a bound is the same point as `best` and rendering
                // it would spend a render learning nothing.
                if (trial.get(axis) - best.get(axis)).abs() <= f32::EPSILON {
                    continue;
                }
                let (loss, mean, worst) = measure(
                    engine,
                    frame,
                    base,
                    &trial,
                    target,
                    width,
                    height,
                    &output,
                    &mut renders,
                )?;
                if loss < best_loss {
                    best = trial;
                    best_loss = loss;
                    best_mean = mean;
                    best_worst = worst;
                    improved = true;
                    // Keep going in the same direction next sweep rather than immediately: a
                    // greedy line search along one axis lets exposure run away before white
                    // balance has been looked at once, which is the failure the visiting order
                    // above exists to avoid.
                    break;
                }
            }
        }
        if !improved {
            scale *= 0.5;
            if scale < MIN_STEP_FRACTION {
                break;
            }
        }
        sweep += 1;
    }

    Ok(super::Fitted {
        candidate: best,
        residual_de00: best_mean,
        worst_tile_de00: best_worst,
        iterations: renders,
    })
}

/// A frame source that is never asked for anything.
///
/// `CpuEngine::render_frame` renders pixels that are already in hand and never touches the
/// source, but `CpuEngine::new` takes one. Returning an error rather than a blank frame is
/// deliberate: if a future change ever routes the fit through `RenderService::render`, this
/// fails loudly instead of silently fitting a black rectangle.
#[derive(Debug, Default)]
pub struct NoSource;

impl aura_render::cpu::FrameSource for NoSource {
    fn frame(&self, image: &aura_core::PhotoId, _level: RenderLevel) -> AuraResult<Frame> {
        Err(aura_core::errors::io::not_found(std::path::Path::new(
            &image.to_db(),
        )))
    }
}

/// An engine for fitting: the real one, over a source that is never used.
#[must_use]
pub fn engine(clock: Arc<dyn aura_core::clock::Clock>) -> CpuEngine {
    CpuEngine::new(Arc::new(NoSource), clock)
}

#[allow(clippy::too_many_arguments)]
fn measure(
    engine: &CpuEngine,
    frame: &Frame,
    base: &Recipe,
    candidate: &Candidate,
    target: &[u8],
    width: usize,
    height: usize,
    output: &OutputSpec,
    renders: &mut u32,
) -> AuraResult<(f32, f32, f32)> {
    let recipe = candidate.to_params().into_recipe(base);
    let rendered = engine.render_frame(
        frame,
        &recipe,
        RenderLevel::Screen(width as u32, height as u32),
        // Analysis, not Interactive: nothing may be skipped. A fit against a render that
        // skipped a stage recovers parameters that compensate for the missing stage, and the
        // profile then carries that compensation into every export.
        RenderPurpose::Analysis,
        output,
    )?;
    *renders = renders.saturating_add(1);
    let RenderedData::Eight(bytes) = &rendered.data else {
        return Err(aura_core::errors::ml::shape_mismatch(
            "8-bit sRGB",
            "16-bit output",
        ));
    };
    let (mean, worst) = super::grid_de00(bytes, target, width, height);
    let histogram = super::histogram_distance(bytes, target);
    Ok((mean + super::HISTOGRAM_WEIGHT * histogram, mean, worst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_axis_has_a_step_and_a_bound() {
        assert_eq!(Axis::ALL.len(), PARAMS);
        assert_eq!(STEPS.len(), PARAMS);
        assert_eq!(BOUNDS.len(), PARAMS);
        for axis in Axis::ALL {
            assert!(STEPS.get(axis.index()).copied().unwrap_or(0.0) > 0.0);
        }
    }

    #[test]
    fn the_axes_are_in_the_order_a_person_edits_in() {
        // Exposure before white balance before tone before colour. Asserted rather than
        // commented, because the order is load-bearing for a fixed budget.
        let order: Vec<&str> = Axis::ALL.iter().map(|axis| axis.as_str()).collect();
        assert_eq!(
            order.first().copied(),
            Some("exposure"),
            "exposure must be fitted first"
        );
        assert_eq!(order.get(1).copied(), Some("temperature"));
        assert_eq!(order.last().copied(), Some("saturation"));
    }

    #[test]
    fn temperature_is_bounded_to_what_a_wedding_is_shot_in() {
        let (low, high) = BOUNDS
            .get(Axis::Temperature.index())
            .copied()
            .unwrap_or((0.0, 0.0));
        assert!(low >= 2000.0 && high <= 12_000.0);
    }

    #[test]
    fn a_candidate_round_trips_through_its_parameters() {
        let mut original = Candidate::neutral();
        original.set(Axis::Exposure, 0.4);
        original.set(Axis::Temperature, 4300.0);
        original.set(Axis::Contrast, 22.0);
        original.set(Axis::CurveLow, -12.0);
        original.set(Axis::CurveHigh, 9.0);
        let back = Candidate::of_params(&original.to_params());
        for axis in Axis::ALL {
            assert!(
                (back.get(axis) - original.get(axis)).abs() < 1.0,
                "{} moved: {} -> {}",
                axis.as_str(),
                original.get(axis),
                back.get(axis)
            );
        }
    }

    #[test]
    fn the_anchor_bounds_make_a_decreasing_curve_unreachable() {
        // The most extreme pair the search can reach: the low anchor pushed as high as it goes
        // and the high anchor pushed as low as it goes. `BOUNDS` is +-64 on both, so the best
        // they can do is meet at 128 - and a curve whose control points are equal in `y` is
        // flat there, which is monotone and legal. There is no setting of the twelve values
        // that produces a curve `ToneCurve::new` refuses.
        let mut crossed = Candidate::neutral();
        crossed.set(Axis::CurveLow, 999.0);
        crossed.set(Axis::CurveHigh, -999.0);
        let params = crossed.to_params();
        assert!(
            !params.curve.is_identity(),
            "the extreme pair is still a curve"
        );
        let points = params.curve.points();
        for pair in points.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            assert!(b.y >= a.y, "the curve decreased: {points:?}");
        }
    }

    #[test]
    fn a_curve_that_did_go_backwards_would_be_refused_by_the_constructor() {
        // The guarantee the test above relies on, asserted from the other side: it is
        // `ToneCurve::new` that refuses a decreasing set, and the bounds above are what keep
        // the optimiser from ever handing it one.
        use aura_core::contract::colour::{CurvePoint, ToneCurve};
        assert!(ToneCurve::new(vec![
            CurvePoint::new(0, 0),
            CurvePoint::new(64, 200),
            CurvePoint::new(192, 100),
            CurvePoint::new(255, 255),
        ])
        .is_err());
    }

    #[test]
    fn set_clamps_into_the_bound_rather_than_wrapping() {
        let mut candidate = Candidate::neutral();
        candidate.set(Axis::Exposure, 99.0);
        assert!((candidate.get(Axis::Exposure) - 5.0).abs() < f32::EPSILON);
        candidate.set(Axis::Temperature, 100.0);
        assert!((candidate.get(Axis::Temperature) - 2000.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_neutral_candidate_is_a_neutral_develop() {
        let params = Candidate::neutral().to_params();
        assert!(params.exposure.abs() < f32::EPSILON);
        assert!((params.temperature_k - 5500.0).abs() < f32::EPSILON);
        assert!(params.curve.is_identity());
    }
}
