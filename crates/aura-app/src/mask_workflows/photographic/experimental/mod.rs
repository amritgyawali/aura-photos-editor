//! Original bounded selection operators. No semantic model or vendor implementation is used.
#![allow(clippy::indexing_slicing)] // Static action partitions have fixed, declared lengths.
use super::{band, byte, check, invalid, Action, CancelToken, Frame, Options, Plane};
use aura_core::AuraResult;
mod pixels;
#[cfg(test)]
mod tests;
mod topology;
mod warp;

pub(super) const ACTIONS: [Action; 25] = [
    Action::LabSampleDistance,
    Action::MahalanobisColour,
    Action::GeodesicColour,
    Action::LocalOtsu,
    Action::SauvolaThreshold,
    Action::GaborResponse,
    Action::DirectionalRidge,
    Action::LocalSkewness,
    Action::LocalKurtosis,
    Action::ColourLineResidual,
    Action::CoverageOpening,
    Action::CoverageClosing,
    Action::MorphologicalReconstruction,
    Action::GrayscaleAreaOpening,
    Action::BorderConnected,
    Action::GeodesicRings,
    Action::HoleBearingRegions,
    Action::RegionalMaxima,
    Action::SpiralMarquee,
    Action::BarycentricTriangle,
    Action::CoverageShear,
    Action::CoverageTwirl,
    Action::CoverageSpherize,
    Action::CoverageDisplace,
    Action::SignedDistanceSmooth,
];
pub(super) fn handles(action: Action) -> bool {
    ACTIONS.contains(&action)
}
pub(super) fn uses_pixels(action: Action) -> bool {
    ACTIONS[..10].contains(&action) || action == Action::CoverageDisplace
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
    // Validate even direct/internal calls, preserving the input until a complete output exists.
    if plane.width == 0
        || plane.height == 0
        || plane.width > 512
        || plane.height > 512
        || plane.width.checked_mul(plane.height) != Some(plane.alpha.len())
    {
        return Err(invalid("Invalid selection grid."));
    }
    if !handles(action) {
        return Err(invalid("Unsupported selection operator."));
    }
    let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
    if options.radius == 0
        || options.radius > 32
        || !unit(options.tolerance)
        || options.tolerance < 0.001
        || !unit(options.corner)
        || options.bounds.iter().any(|v| !unit(*v))
        || options.bounds[0] >= options.bounds[2]
        || options.bounds[1] >= options.bounds[3]
        || options.range.iter().any(|v| !unit(*v))
        || options.range[0] > options.range[1]
        || !options.angle.is_finite()
        || options.angle.abs() > 360.0
        || !options.sweep.is_finite()
        || !(0.1..=360.0).contains(&options.sweep)
        || options.vertices.len() > 128
        || options.vertices.iter().flatten().any(|v| !unit(*v))
        || point.iter().any(|v| !unit(*v))
        || options.max_area > 262_144
        || options.min_area > options.max_area
    {
        return Err(invalid(
            "Enter finite coordinates, ordered ranges and valid processing limits.",
        ));
    }
    if uses_pixels(action) && options.radius > 8 {
        return Err(invalid(
            "Source analysis is limited to eight selection cells.",
        ));
    }
    let frame = if uses_pixels(action) {
        let f = frame.ok_or_else(|| invalid("This selection requires a source preview."))?;
        if f.rgb.len() != plane.alpha.len()
            || f.luminance.len() != plane.alpha.len()
            || f.rgb
                .iter()
                .flatten()
                .chain(f.luminance.iter())
                .any(|v| !unit(*v))
        {
            return Err(invalid("Invalid source frame or mismatched grid."));
        }
        Some(f)
    } else {
        frame
    };
    let output = if ACTIONS[..10].contains(&action) {
        pixels::apply(
            plane,
            action,
            options,
            point,
            frame.ok_or_else(|| invalid("Missing source."))?,
            cancel,
        )?
    } else if ACTIONS[10..18].contains(&action) {
        topology::apply(plane, action, options, point, cancel)?
    } else {
        warp::apply(plane, action, options, frame, cancel)?
    };
    check(cancel)?;
    plane.alpha = output;
    Ok(())
}

#[allow(clippy::indexing_slicing)] // Each neighbor is admitted only by a checked grid boundary.
fn neighbors(index: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (index % width, index / width);
    [
        if x > 0 { Some(index - 1) } else { None },
        if x + 1 < width { Some(index + 1) } else { None },
        if y > 0 { Some(index - width) } else { None },
        if y + 1 < height {
            Some(index + width)
        } else {
            None
        },
    ]
    .into_iter()
    .flatten()
}
