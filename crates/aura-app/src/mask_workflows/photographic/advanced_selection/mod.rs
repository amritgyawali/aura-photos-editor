//! Bounded original measurements, graph operations and coverage deformation.
#![allow(clippy::indexing_slicing)] // Action partitions and grids are checked before dispatch.
use super::{band, byte, check, invalid, Action, CancelToken, Frame, Options, Plane};
use aura_core::AuraResult;
mod geometry;
mod graphs;
mod measurements;
#[cfg(test)]
mod tests;
pub(super) const ACTIONS: [Action; 25] = [
    Action::MedianAbsoluteDeviation,
    Action::BinaryPatternTransitions,
    Action::PatchAutocorrelation,
    Action::PatchSemivariance,
    Action::LogChromaDistance,
    Action::EdgeNonmaximum,
    Action::EdgeHysteresis,
    Action::LocalMoran,
    Action::TemplatePatchCorrelation,
    Action::LocalHueDispersion,
    Action::ColourPrototypePartition,
    Action::SelectionArticulations,
    Action::SelectionKCore,
    Action::CoverageWatershed,
    Action::CoverageAnisotropicDiffusion,
    Action::CoverageCurvatureFlow,
    Action::RegionInscribedDisk,
    Action::DirectionalRunWidth,
    Action::ThinPlateCoverageWarp,
    Action::MeanValueCageWarp,
    Action::TriangulatedCageWarp,
    Action::MobiusCoverageWarp,
    Action::PathNormalUnwrap,
    Action::CoverageSeamCarve,
    Action::LissajousRibbon,
];
pub(super) fn handles(action: Action) -> bool {
    ACTIONS.contains(&action)
}
pub(super) fn uses_pixels(action: Action) -> bool {
    ACTIONS[..11].contains(&action)
}
fn validate(
    plane: &Plane,
    action: Action,
    options: &Options,
    point: [f32; 2],
    frame: Option<&Frame>,
) -> AuraResult<()> {
    let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
    if plane.width == 0
        || plane.height == 0
        || plane.width > 512
        || plane.height > 512
        || plane.width.checked_mul(plane.height) != Some(plane.alpha.len())
        || !handles(action)
    {
        return Err(invalid("Invalid advanced selection grid or operator."));
    }
    if options.radius == 0
        || options.radius > 32
        || !unit(options.tolerance)
        || options.tolerance < 0.001
        || !unit(options.corner)
        || options
            .bounds
            .iter()
            .chain(options.range.iter())
            .any(|v| !unit(*v))
        || options.bounds[0] >= options.bounds[2]
        || options.bounds[1] >= options.bounds[3]
        || options.range[0] > options.range[1]
        || point.iter().any(|v| !unit(*v))
        || !options.angle.is_finite()
        || options.angle.abs() > 360.0
        || options.vertices.len() > 128
        || options.vertices.iter().flatten().any(|v| !unit(*v))
    {
        return Err(invalid(
            "Enter finite geometry, ordered ranges and a bounded processing radius.",
        ));
    }
    if uses_pixels(action) {
        let limit = if action == Action::TemplatePatchCorrelation {
            4
        } else {
            8
        };
        if options.radius > limit {
            return Err(invalid(
                "Source measurement radius exceeds this operator's limit.",
            ));
        }
        let source = frame.ok_or_else(|| invalid("This selection requires source pixels."))?;
        if source.rgb.len() != plane.alpha.len()
            || source.luminance.len() != plane.alpha.len()
            || source
                .rgb
                .iter()
                .flatten()
                .chain(source.luminance.iter())
                .any(|v| !unit(*v))
        {
            return Err(invalid("Invalid or mismatched source selection grid."));
        }
    }
    Ok(())
}
pub(super) fn apply(
    plane: &mut Plane,
    action: Action,
    options: &Options,
    point: [f32; 2],
    frame: Option<&Frame>,
    cancel: &CancelToken,
) -> AuraResult<()> {
    check(cancel)?;
    validate(plane, action, options, point, frame)?;
    let output = if uses_pixels(action) {
        measurements::apply(
            plane,
            action,
            options,
            point,
            frame.ok_or_else(|| invalid("Missing source pixels."))?,
            cancel,
        )?
    } else if ACTIONS[11..18].contains(&action) {
        graphs::apply(plane, action, options, cancel)?
    } else {
        geometry::apply(plane, action, options, cancel)?
    };
    check(cancel)?;
    plane.alpha = output;
    Ok(())
}
fn neighbors(index: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (index % width, index / width);
    [
        x.checked_sub(1).map(|_| index - 1),
        (x + 1 < width).then_some(index + 1),
        y.checked_sub(1).map(|_| index - width),
        (y + 1 < height).then_some(index + width),
    ]
    .into_iter()
    .flatten()
}
fn seed(point: [f32; 2], plane: &Plane) -> usize {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let (x, y) = (
        ((point[0] * plane.width as f32) as usize).min(plane.width - 1),
        ((point[1] * plane.height as f32) as usize).min(plane.height - 1),
    );
    y * plane.width + x
}
#[allow(clippy::cast_precision_loss)] // All coordinates are validated <=512 selection cells.
fn sample(plane: &Plane, x: f32, y: f32) -> f32 {
    if !x.is_finite() || !y.is_finite() || !(-1.0..=2.0).contains(&x) || !(-1.0..=2.0).contains(&y)
    {
        return 0.0;
    }
    let (x, y) = (x * plane.width as f32 - 0.5, y * plane.height as f32 - 0.5);
    #[allow(clippy::cast_possible_truncation)]
    let (left, top) = (x.floor() as isize, y.floor() as isize);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let mut value = 0.0;
    for (ox, wx) in [(0, 1.0 - fx), (1, fx)] {
        for (oy, wy) in [(0, 1.0 - fy), (1, fy)] {
            if let (Ok(sx), Ok(sy)) = (usize::try_from(left + ox), usize::try_from(top + oy)) {
                if sx < plane.width && sy < plane.height {
                    value += f32::from(plane.alpha[sy * plane.width + sx]) / 255.0 * wx * wy;
                }
            }
        }
    }
    value
}
