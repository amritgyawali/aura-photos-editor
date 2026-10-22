//! Independently authored photographic measurements and selection constructions.
//! These are bounded mathematical tools, not semantic or trained segmentation.
use super::{
    advanced::Options, advanced_pixels::Frame, check, invalid, plane::Plane, Action, Input,
};
use crate::AppState;
use aura_core::{progress::CancelToken, AuraResult, PhotoId};
mod advanced_selection;
mod authored;
mod coverage;
mod experimental;
mod features;
#[cfg(test)]
mod tests;

pub(super) fn handles(action: Action) -> bool {
    advanced_selection::handles(action)
        || experimental::handles(action)
        || matches!(
            action,
            Action::HueArc
                | Action::OpponentChroma
                | Action::TemperatureBalance
                | Action::ChannelClipping
                | Action::NeutralAxis
                | Action::LocalContrastPolarity
                | Action::LocalEntropy
                | Action::LaplacianDetail
                | Action::EdgeOrientation
                | Action::TensorCoherence
                | Action::HarrisCorners
                | Action::LocalRank
                | Action::ChromaticBoundary
                | Action::SpecularResidual
                | Action::ShadowChromaDeviation
                | Action::SuperellipseMarquee
                | Action::BezierRibbon
                | Action::StarMarquee
                | Action::SeedField
                | Action::BilateralSymmetry
                | Action::PeriodicSelection
                | Action::PolarSelection
                | Action::CoverageQuantiles
                | Action::HarmonicPatch
                | Action::RegionAxisStrip
        )
}
fn uses_pixels(action: Action) -> bool {
    matches!(
        action,
        Action::HueArc
            | Action::OpponentChroma
            | Action::TemperatureBalance
            | Action::ChannelClipping
            | Action::NeutralAxis
            | Action::LocalContrastPolarity
            | Action::LocalEntropy
            | Action::LaplacianDetail
            | Action::EdgeOrientation
            | Action::TensorCoherence
            | Action::HarrisCorners
            | Action::LocalRank
            | Action::ChromaticBoundary
            | Action::SpecularResidual
            | Action::ShadowChromaDeviation
    )
}
fn validate_source_radius(action: Action, options: &Options) -> AuraResult<()> {
    if (uses_pixels(action)
        || experimental::uses_pixels(action)
        || advanced_selection::uses_pixels(action))
        && options.radius > 8
    {
        return Err(invalid(
            "Photographic measurements are limited to eight selection cells.",
        ));
    }
    if action == Action::TemplatePatchCorrelation && options.radius > 4 {
        return Err(invalid(
            "Template correlation is limited to four selection cells.",
        ));
    }
    Ok(())
}
pub(super) fn edit(
    state: &AppState,
    input: &Input,
    photo: PhotoId,
    plane: &mut Plane,
    cancel: &CancelToken,
) -> AuraResult<()> {
    let options = Options::validated(input)?;
    validate_source_radius(input.action, &options)?;
    check(cancel)?;
    if advanced_selection::handles(input.action) {
        let frame = if advanced_selection::uses_pixels(input.action) {
            Some(Frame::load(
                state,
                &input.project_id,
                photo,
                plane.width,
                plane.height,
            )?)
        } else {
            None
        };
        return advanced_selection::apply(
            plane,
            input.action,
            &options,
            input.point.unwrap_or([0.5, 0.5]),
            frame.as_ref(),
            cancel,
        );
    }
    if experimental::handles(input.action) {
        let frame = if experimental::uses_pixels(input.action) {
            Some(Frame::load(
                state,
                &input.project_id,
                photo,
                plane.width,
                plane.height,
            )?)
        } else {
            None
        };
        return experimental::apply(
            plane,
            input.action,
            &options,
            input.point.unwrap_or([0.5, 0.5]),
            frame.as_ref(),
            cancel,
        );
    }
    if uses_pixels(input.action) {
        let frame = Frame::load(state, &input.project_id, photo, plane.width, plane.height)?;
        features::apply(plane, input.action, &options, &frame, cancel)
    } else {
        match input.action {
            Action::SuperellipseMarquee
            | Action::BezierRibbon
            | Action::StarMarquee
            | Action::SeedField => authored::apply(plane, input.action, &options, cancel),
            _ => coverage::apply(plane, input.action, &options, cancel),
        }
    }
}
fn band(value: f32, options: &Options) -> f32 {
    let outside = (options.range[0] - value)
        .max(value - options.range[1])
        .max(0.0);
    (1.0 - outside / options.tolerance).clamp(0.0, 1.0)
}
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Coverage is explicitly clamped.
fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
