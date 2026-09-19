//! What a look actually did, measured through the real renderer.
//!
//! ## Why this module exists at all
//!
//! "Your photographs now look like that page" is a claim, and [`solve`](crate::solve) can only
//! make it about parameters. A solver that moved the exposure by a third of a stop has moved the
//! exposure by a third of a stop; whether the *result* sits where the reference sits is a
//! different question, and the only instrument that answers it is the renderer.
//!
//! So this module renders the photographer's own frames with the look applied, measures them
//! with the same [`crate::measure`] the reference was measured with, and reports the distance.
//! Phase 16's rule - a guarantee is measured, not asserted - and its second half, which phase 25
//! wrote: the measurement is stored, so "the match reached three dE00" is a `SELECT` rather than
//! a sentence.
//!
//! ## The two things this measurement is careful about
//!
//! **Both sides are measured the same way.** The reference readings and the rendered readings go
//! through one function at one scale. Phase 22's lesson - a threshold on a measurement is a
//! statement about the instrument as well as about the world - has a corollary for a comparison:
//! two measurements taken with different instruments are not a difference, they are two numbers.
//!
//! **The improvement is measured against what the gap was**, never against the ceiling.
//! [`aura_core::contract::look::BucketResidual::realised_share`] is what the report leads with,
//! and phase 27 wrote the argument: a match that closed ninety per cent of a large gap and landed
//! just outside is a match that worked, and one that landed inside because there was nothing to
//! close is not a result.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::error::AuraResult;
use aura_core::contract::look::{
    BucketResidual, LookAggregate, LookCode, LookProfile, LookReason, MATCH_DE00_CEILING,
};
use aura_core::contract::style::{LightingBucket, StyleDelta};
use aura_raw::codec::Rgb8;
use aura_recipe::Recipe;
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::Frame;
use aura_render::{CpuEngine, RenderedData};
use aura_style::extract::TheirParams;

use crate::solve::{self, Probe};

/// One of the photographer's own photographs, ready to be rendered with a look on it.
#[derive(Debug)]
pub struct OwnFrame {
    /// A stable key for the reading. The photograph's id, as text.
    pub key: String,
    /// The pixels, already decoded.
    pub frame: Frame,
    /// Its width in pixels, at the level this is rendered at.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// What phases 15 and 16 decided about it: the baseline the look is a residual from.
    pub baseline: Recipe,
    /// Which light phase 15 found on the subject.
    pub lighting: LightingBucket,
    /// True when the photographer has edited this frame by hand.
    ///
    /// Such a frame is **measured and never moved**. Phase 14's rule - a parameter a person set
    /// is never overwritten - and the reason it is a field here rather than a filter applied by
    /// the caller: a match report that silently excluded hand-edited frames would report a
    /// better number than the gallery a photographer is going to look at.
    pub user_edited: bool,
}

/// An engine and an output specification, built once and reused for every render.
#[derive(Debug)]
pub struct Renderer {
    engine: CpuEngine,
    output: OutputSpec,
}

/// A frame source that is never asked for anything.
///
/// `CpuEngine::render_frame` renders pixels that are already in hand and never touches the
/// source, but `CpuEngine::new` takes one. Erroring rather than returning a blank frame is
/// phase 17's decision copied deliberately: if a future change ever routes this through
/// `RenderService::render`, it fails loudly instead of quietly measuring a black rectangle.
#[derive(Debug, Default)]
struct NoSource;

impl aura_render::cpu::FrameSource for NoSource {
    fn frame(&self, image: &aura_core::PhotoId, _level: RenderLevel) -> AuraResult<Frame> {
        Err(aura_core::errors::io::not_found(std::path::Path::new(
            &image.to_db(),
        )))
    }
}

impl Renderer {
    /// The real engine, over a source that is never used.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, output: OutputSpec) -> Self {
        Self {
            engine: CpuEngine::new(Arc::new(NoSource), clock),
            output,
        }
    }

    /// Render one frame with a look applied and read the result.
    ///
    /// # Errors
    ///
    /// Whatever the engine raised, or `AURA-ML-5008` when the engine produced sixteen-bit output
    /// where this measurement expects eight.
    pub fn read_with(
        &self,
        frame: &OwnFrame,
        delta: &StyleDelta,
    ) -> AuraResult<aura_core::contract::look::ReferenceReading> {
        // A hand-edited frame is rendered as the photographer left it. The look is not applied
        // and the reading is still taken, so the report counts it in the gallery it belongs to.
        let recipe = if frame.user_edited {
            frame.baseline.clone()
        } else {
            shift(&frame.baseline, delta)
        };

        let rendered = self.engine.render_frame(
            &frame.frame,
            &recipe,
            RenderLevel::Screen(frame.width, frame.height),
            // Analysis rather than Interactive: nothing may be skipped. A match measured against
            // a render that skipped a stage is a number about a pipeline that will not be the
            // one delivering the photograph. Phase 17's fitter makes the same choice.
            RenderPurpose::Analysis,
            &self.output,
        )?;
        let RenderedData::Eight(bytes) = &rendered.data else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "8-bit sRGB",
                "16-bit output",
            ));
        };

        Ok(crate::measure::read(
            frame.key.clone(),
            &Rgb8 {
                width: frame.width,
                height: frame.height,
                data: bytes.clone(),
            },
        ))
    }
}

/// One recipe with a look applied to it.
///
/// **Phase 17's `styled` and nothing else.** A look is a `StyleDelta`, adding one to a set of
/// develop parameters is a thing phase 17 already knows how to do, and a second implementation
/// here would be a second answer to what a delta means - which would show up as an album that
/// does not match the gallery, exactly as `RenderService` being the only renderer exists to
/// prevent.
#[must_use]
pub fn shift(baseline: &Recipe, delta: &StyleDelta) -> Recipe {
    let params = TheirParams::of_recipe(baseline);
    aura_style::api::styled(&params, delta).into_recipe(baseline)
}

/// The most frames one refinement probe renders.
///
/// Eight. The refinement calls the probe `REFINE_SWEEPS * 11 * REFINE_STEPS.len()` times - a
/// hundred and thirty-two - so the frame count is a multiplier on every render in the phase, and
/// an uncapped probe makes learning a look cost proportional to the size of the wedding. A
/// thousand-frame project would spend a hundred and thirty thousand renders to refine a number
/// that eight frames already pin down: the probe's job is to fold a *median*, and a median over
/// eight frames and a median over a thousand differ by far less than the step the search is
/// deciding between.
///
/// It is a bound on the **refinement** and not on the measurement.
/// [`measure`] reads every frame it is given, because that is the number a photographer is
/// shown and it has to be about their gallery rather than about a sample of it.
pub const MAX_REFINE_FRAMES: usize = 8;

/// Every `n`th frame, up to [`MAX_REFINE_FRAMES`].
///
/// A stride rather than the first eight, so a wedding that starts in one room and ends in
/// another is sampled across both. Deterministic, so two refinements of one project walk the
/// same frames - which is what makes `the_same_reference_produces_the_same_look` true of the
/// refined answer and not only of the initial one.
fn sample(frames: &[OwnFrame]) -> Vec<&OwnFrame> {
    if frames.len() <= MAX_REFINE_FRAMES {
        return frames.iter().collect();
    }
    let stride = frames.len().div_ceil(MAX_REFINE_FRAMES).max(1);
    frames.iter().step_by(stride).take(MAX_REFINE_FRAMES).collect()
}

/// The probe [`crate::solve::refine`] walks against: the photographer's own frames, rendered.
#[derive(Debug)]
pub struct FrameProbe<'a> {
    renderer: &'a Renderer,
    frames: Vec<&'a OwnFrame>,
}

impl<'a> FrameProbe<'a> {
    /// Build a probe over a set of frames, sampled down to [`MAX_REFINE_FRAMES`].
    #[must_use]
    pub fn new(renderer: &'a Renderer, frames: &'a [OwnFrame]) -> Self {
        Self {
            renderer,
            frames: sample(frames),
        }
    }

    /// Build a probe over an already-chosen subset, sampled down the same way.
    #[must_use]
    pub fn over(renderer: &'a Renderer, frames: &[&'a OwnFrame]) -> Self {
        let chosen: Vec<&'a OwnFrame> = if frames.len() <= MAX_REFINE_FRAMES {
            frames.to_vec()
        } else {
            let stride = frames.len().div_ceil(MAX_REFINE_FRAMES).max(1);
            frames
                .iter()
                .copied()
                .step_by(stride)
                .take(MAX_REFINE_FRAMES)
                .collect()
        };
        Self {
            renderer,
            frames: chosen,
        }
    }
}

impl Probe for FrameProbe<'_> {
    fn aggregate_with(&self, delta: &StyleDelta) -> LookAggregate {
        let readings: Vec<_> = self
            .frames
            .iter()
            .filter_map(|frame| self.renderer.read_with(frame, delta).ok())
            .collect();
        crate::aggregate::fold(&crate::aggregate::all(&readings))
    }
}

/// What one look did to a project's photographs, measured.
///
/// Renders every frame twice - once at the baseline and once with the look - and reports the
/// distance to the reference on both sides. Two renders per frame rather than one is the cost of
/// being able to say what the gap *was*, and without that number the report can only say where
/// the gallery ended up, which is the measurement phase 27's rule calls useless.
#[must_use]
pub fn measure(
    profile: &LookProfile,
    renderer: &Renderer,
    frames: &[OwnFrame],
) -> Vec<BucketResidual> {
    let mut out = Vec::new();

    for (lighting, bucket) in &profile.buckets {
        let in_bucket: Vec<&OwnFrame> = frames
            .iter()
            .filter(|frame| frame.lighting == *lighting)
            .collect();
        if in_bucket.is_empty() || !bucket.reference.is_usable() {
            continue;
        }

        let (delta, _) = profile.resolve(*lighting);
        let neutral = StyleDelta::neutral();

        let before_readings: Vec<_> = in_bucket
            .iter()
            .filter_map(|frame| renderer.read_with(frame, &neutral).ok())
            .collect();
        let after_readings: Vec<_> = in_bucket
            .iter()
            .filter_map(|frame| renderer.read_with(frame, &delta).ok())
            .collect();

        let before = crate::aggregate::fold(&crate::aggregate::all(&before_readings));
        let after = crate::aggregate::fold(&crate::aggregate::all(&after_readings));

        out.push(BucketResidual {
            lighting: *lighting,
            before_de00: solve::distance(&before, &bucket.reference),
            after_de00: solve::distance(&after, &bucket.reference),
            frames: u32::try_from(in_bucket.len()).unwrap_or(u32::MAX),
        });
    }

    out
}

/// The frame-weighted distance over a set of per-bucket residuals.
///
/// Weighted by how many frames each bucket held rather than averaged flat, because a bucket with
/// four frames in it and a bucket with four hundred are not two equal opinions about whether a
/// gallery matches - and a flat mean lets the small one halve the reported number.
#[must_use]
pub fn weighted(residuals: &[BucketResidual], pick: fn(&BucketResidual) -> f32) -> f32 {
    let total: u32 = residuals.iter().map(|row| row.frames).sum();
    if total == 0 {
        return 0.0;
    }
    residuals
        .iter()
        .map(|row| pick(row) * row.frames as f32)
        .sum::<f32>()
        / total as f32
}

/// The reasons a set of residuals raises.
#[must_use]
pub fn reasons_for(residuals: &[BucketResidual], user_edited: u32) -> Vec<LookReason> {
    let mut reasons = Vec::new();
    let after = weighted(residuals, |row| row.after_de00);

    if !residuals.is_empty() {
        if after <= MATCH_DE00_CEILING {
            reasons.push(LookReason::measured(
                LookCode::MatchReached,
                after,
                MATCH_DE00_CEILING,
            ));
        } else {
            reasons.push(LookReason::measured(
                LookCode::MatchShort,
                after,
                MATCH_DE00_CEILING,
            ));
        }
    }
    if user_edited > 0 {
        reasons.push(LookReason::counted(
            LookCode::UserEditPreserved,
            user_edited as f32,
        ));
    }
    reasons
}
