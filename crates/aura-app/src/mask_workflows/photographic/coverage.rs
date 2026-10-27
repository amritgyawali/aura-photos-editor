//! Deterministic resampling and statistical operations on authored coverage only.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{byte, check, invalid, Action, CancelToken, Options, Plane};
use aura_core::AuraResult;
use std::f32::consts::TAU;

fn sample(plane: &Plane, x: f32, y: f32) -> f32 {
    // Centre aligned bilinear taps; coverage outside the frame is transparent.
    let (x, y) = (x * plane.width as f32 - 0.5, y * plane.height as f32 - 0.5);
    let (left, top) = (x.floor() as isize, y.floor() as isize);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let mut result = 0.0;
    for (oy, wy) in [(0, 1.0 - fy), (1, fy)] {
        for (ox, wx) in [(0, 1.0 - fx), (1, fx)] {
            let (sx, sy) = (left + ox, top + oy);
            if let (Ok(sx), Ok(sy)) = (usize::try_from(sx), usize::try_from(sy)) {
                if sx < plane.width && sy < plane.height {
                    result += f32::from(plane.alpha[sy * plane.width + sx]) / 255.0 * wx * wy;
                }
            }
        }
    }
    result
}
fn quantiles(plane: &Plane, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let mut histogram = [0_usize; 256];
    for value in &plane.alpha {
        histogram[*value as usize] += 1;
    }
    let mut lut = [0; 256];
    let mut count = 0;
    // Keep background transparent; remap nonzero coverage with tie midranks.
    let total = plane.alpha.len() - histogram[0];
    for value in 1..256 {
        check(cancel)?;
        lut[value] = if total == 0 {
            0
        } else {
            byte((count as f32 + histogram[value] as f32 * 0.5) / total as f32)
        };
        count += histogram[value];
    }
    Ok(plane.alpha.iter().map(|v| lut[*v as usize]).collect())
}
fn harmonic(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let mut values: Vec<f32> = plane.alpha.iter().map(|v| f32::from(*v) / 255.0).collect();
    let [left, top, right, bottom] = options.bounds;
    let mut active = vec![false; values.len()];
    for y in 1..plane.height.saturating_sub(1) {
        for x in 1..plane.width.saturating_sub(1) {
            let nx = (x as f32 + 0.5) / plane.width as f32;
            let ny = (y as f32 + 0.5) / plane.height as f32;
            active[y * plane.width + x] = nx > left && nx < right && ny > top && ny < bottom;
        }
    }
    // A fixed, explicit iteration budget makes results independent of timing.
    for _ in 0..options.radius * 8 {
        let mut next = values.clone();
        for y in 1..plane.height.saturating_sub(1) {
            check(cancel)?;
            for x in 1..plane.width.saturating_sub(1) {
                let i = y * plane.width + x;
                if active[i] {
                    next[i] = (values[i - 1]
                        + values[i + 1]
                        + values[i - plane.width]
                        + values[i + plane.width])
                        * 0.25;
                }
            }
        }
        values = next;
    }
    Ok(values.into_iter().map(byte).collect())
}
fn axis_strip(plane: &Plane, options: &Options, cancel: &CancelToken) -> AuraResult<Vec<u8>> {
    let mut output = vec![0; plane.alpha.len()];
    for group in plane.region_groups(cancel)? {
        check(cancel)?;
        let count = group.len() as f32;
        let (mut cx, mut cy) = (0.0, 0.0);
        for i in &group {
            cx += (*i % plane.width) as f32;
            cy += (*i / plane.width) as f32;
        }
        cx /= count;
        cy /= count;
        let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
        for i in &group {
            let dx = (*i % plane.width) as f32 - cx;
            let dy = (*i / plane.width) as f32 - cy;
            xx += dx * dx;
            xy += dx * dy;
            yy += dy * dy;
        }
        let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
        let (sn, cs) = angle.sin_cos();
        let max_distance = group
            .iter()
            .map(|i| {
                let dx = (*i % plane.width) as f32 - cx;
                let dy = (*i / plane.width) as f32 - cy;
                (-sn * dx + cs * dy).abs()
            })
            .fold(0.5_f32, f32::max);
        for i in group {
            let dx = (i % plane.width) as f32 - cx;
            let dy = (i / plane.width) as f32 - cy;
            let distance = (-sn * dx + cs * dy).abs() / max_distance;
            if distance <= options.tolerance {
                output[i] = plane.alpha[i];
            }
        }
    }
    Ok(output)
}
pub(super) fn apply(
    plane: &mut Plane,
    action: Action,
    options: &Options,
    cancel: &CancelToken,
) -> AuraResult<()> {
    check(cancel)?;
    let mut output = match action {
        Action::CoverageQuantiles => quantiles(plane, cancel)?,
        Action::HarmonicPatch => harmonic(plane, options, cancel)?,
        Action::RegionAxisStrip => axis_strip(plane, options, cancel)?,
        Action::BilateralSymmetry | Action::PeriodicSelection | Action::PolarSelection => {
            vec![0; plane.alpha.len()]
        }
        _ => return Err(invalid("Unknown photographic selection operation.")),
    };
    if matches!(
        action,
        Action::BilateralSymmetry | Action::PeriodicSelection | Action::PolarSelection
    ) {
        for y in 0..plane.height {
            check(cancel)?;
            for x in 0..plane.width {
                let (nx, ny) = (
                    (x as f32 + 0.5) / plane.width as f32,
                    (y as f32 + 0.5) / plane.height as f32,
                );
                let value = match action {
                    Action::BilateralSymmetry => {
                        let (sn, cs) = options.angle.to_radians().sin_cos();
                        let dx = nx - 0.5;
                        let dy = ny - 0.5;
                        let normal = -sn * dx + cs * dy;
                        sample(plane, nx + 2.0 * sn * normal, ny - 2.0 * cs * normal)
                            .min(f32::from(plane.alpha[y * plane.width + x]) / 255.0)
                    }
                    Action::PeriodicSelection => sample(
                        plane,
                        (nx * options.radius as f32).rem_euclid(1.0),
                        (ny * options.radius as f32).rem_euclid(1.0),
                    ),
                    Action::PolarSelection => {
                        let angle = nx * TAU + options.angle.to_radians();
                        let radius = ny * 0.5;
                        sample(
                            plane,
                            0.5 + radius * angle.cos(),
                            0.5 + radius * angle.sin(),
                        )
                    }
                    _ => 0.0,
                };
                output[y * plane.width + x] = byte(value);
            }
        }
    }
    check(cancel)?;
    plane.alpha = output;
    Ok(())
}
