//! Native-resolution finite-footprint processing with immutable source stripes.
//! Only independent local methods qualify: normalized-coordinate optics, donor
//! methods, frame statistics and iterative solvers keep their established path.
use super::{evaluate, Grid, Operation, Tool};

const HALO: usize = 4;
const TILE: usize = 192;
const SCRATCH_CELLS: usize = 2_097_152;

pub(super) fn supports(tool: Tool) -> bool {
    super::analytical::supports(tool)
        || matches!(
            tool,
            Tool::AdaptiveMedianImpulse
                | Tool::SigmaRangeMean
                | Tool::SpatialWeightedMedian
                | Tool::LuminanceWinsor
                | Tool::ChromaBilateral
                | Tool::ChromaCovariance
                | Tool::RgbCovariance
                | Tool::EntropyDetail
                | Tool::RmsContrast
                | Tool::LogRetinex
                | Tool::PeriodicNotch
                | Tool::CrossMedian
                | Tool::HessianRidge
                | Tool::CoherenceDetail
                | Tool::DirectionalMedian
                | Tool::QuadraticResidual
                | Tool::BilateralToneCompress
                | Tool::ConnectedImpulse
                | Tool::ChromaMedian
        )
}

// Explicit stripe/tile coordinates keep source ownership and write boundaries
// visible. A stripe is snapshotted before writes and retains its original halo.
#[allow(clippy::too_many_lines)]
pub(super) fn process(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    weights: &[f32],
    op: &Operation,
) -> bool {
    if width == 0
        || height == 0
        || weights.len() != width * height
        || rgb.len() != weights.len() * 3
    {
        return false;
    }
    let mut bounds = [width, height, 0, 0];
    let mut found = false;
    for (i, w) in weights.iter().enumerate() {
        if *w > 0.0 {
            found = true;
            bounds[0] = bounds[0].min(i % width);
            bounds[1] = bounds[1].min(i / width);
            bounds[2] = bounds[2].max(i % width + 1);
            bounds[3] = bounds[3].max(i / width + 1);
        }
    }
    if !found {
        return true;
    }
    let left = bounds[0].saturating_sub(HALO);
    let right = (bounds[2] + HALO).min(width);
    let stride = right - left;
    let stripe_height = (SCRATCH_CELLS / stride).saturating_sub(2 * HALO).min(TILE);
    // Unusually wide images fall back rather than exceed the scratch budget.
    if stripe_height == 0 {
        return false;
    }
    let mut previous = Vec::<[f32; 3]>::new();
    let mut previous_top = 0;
    let mut first_y = bounds[1];
    while first_y < bounds[3] {
        let last_y = (first_y + stripe_height).min(bounds[3]);
        let top = first_y.saturating_sub(HALO);
        let bottom = (last_y + HALO).min(height);
        let mut original = Vec::with_capacity(stride * (bottom - top));
        for y in top..bottom {
            for x in left..right {
                original.push(if y < first_y && !previous.is_empty() {
                    previous[(y - previous_top) * stride + x - left]
                } else {
                    let i = (y * width + x) * 3;
                    [rgb[i], rgb[i + 1], rgb[i + 2]]
                });
            }
        }
        for first_x in (bounds[0]..bounds[2]).step_by(TILE) {
            let last_x = (first_x + TILE).min(bounds[2]);
            let tile_left = first_x.saturating_sub(HALO);
            let tile_right = (last_x + HALO).min(width);
            let w = tile_right - tile_left;
            let h = bottom - top;
            let mut grid = Grid {
                pixels: Vec::with_capacity(w * h),
                weight: Vec::with_capacity(w * h),
                width: w,
                height: h,
                centre: [0.5; 2],
            };
            for y in top..bottom {
                for x in tile_left..tile_right {
                    grid.pixels.push(original[(y - top) * stride + x - left]);
                    grid.weight.push(weights[y * width + x]);
                }
            }
            if !grid.weight.iter().any(|w| *w > 0.0) {
                continue;
            }
            let output = evaluate(&grid, op);
            for y in first_y..last_y {
                for x in first_x..last_x {
                    let index = y * width + x;
                    let alpha = weights[index] * op.amount;
                    if alpha <= 0.0 {
                        continue;
                    }
                    let cell = (y - top) * w + x - tile_left;
                    for c in 0..3 {
                        let value =
                            grid.pixels[cell][c] + (output[cell][c] - grid.pixels[cell][c]) * alpha;
                        if value.is_finite() {
                            rgb[index * 3 + c] = value.max(0.0);
                        }
                    }
                }
            }
        }
        previous_top = last_y.saturating_sub(HALO);
        previous.clear();
        previous
            .extend_from_slice(&original[(previous_top - top) * stride..(last_y - top) * stride]);
        first_y = last_y;
    }
    true
}
