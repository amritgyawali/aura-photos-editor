//! Explicit sRGB diagnostic measurements. No colour correction is performed here.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{band, byte, check, invalid, Action, CancelToken, Frame, Options, Plane};
use aura_core::AuraResult;

struct Samples<'a> {
    frame: &'a Frame,
    width: usize,
    height: usize,
}
impl Samples<'_> {
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }
    fn gradient(&self, x: usize, y: usize, channel: Option<usize>) -> [f32; 2] {
        let at = |x, y| {
            let i = self.index(x, y);
            channel.map_or(self.frame.luminance[i], |c| self.frame.rgb[i][c])
        };
        [
            (at((x + 1).min(self.width - 1), y) - at(x.saturating_sub(1), y)) * 0.5,
            (at(x, (y + 1).min(self.height - 1)) - at(x, y.saturating_sub(1))) * 0.5,
        ]
    }
    fn patch(&self, x: usize, y: usize, radius: usize) -> impl Iterator<Item = (usize, usize)> {
        let (left, right) = (x.saturating_sub(radius), (x + radius + 1).min(self.width));
        let (top, bottom) = (y.saturating_sub(radius), (y + radius + 1).min(self.height));
        (top..bottom).flat_map(move |sy| (left..right).map(move |sx| (sx, sy)))
    }
    fn local(&self, x: usize, y: usize, radius: usize, action: Action) -> f32 {
        let index = self.index(x, y);
        let centre = self.frame.luminance[index];
        let mut bins = [0.0_f32; 16];
        let (mut count, mut mean, mut rank, mut ties) = (0.0, 0.0, 0.0, 0.0);
        let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
        let mut chroma_mean = [0.0; 2];
        for (sx, sy) in self.patch(x, y, radius) {
            let i = self.index(sx, sy);
            let value = self.frame.luminance[i];
            count += 1.0;
            mean += value;
            rank += if value < centre { 1.0 } else { 0.0 };
            ties += if value.to_bits() == centre.to_bits() {
                1.0
            } else {
                0.0
            };
            bins[(value * 15.0).round() as usize] += 1.0;
            let pixel = self.frame.rgb[i];
            chroma_mean[0] += pixel[0] - pixel[1];
            chroma_mean[1] += pixel[2] - pixel[1];
            if matches!(action, Action::TensorCoherence | Action::HarrisCorners) {
                let [gx, gy] = self.gradient(sx, sy, None);
                xx += gx * gx;
                xy += gx * gy;
                yy += gy * gy;
            }
        }
        mean /= count;
        match action {
            Action::LocalContrastPolarity => (0.5 + (centre - mean) * 2.0).clamp(0.0, 1.0),
            Action::LocalEntropy => {
                -bins
                    .iter()
                    .filter(|n| **n > 0.0)
                    .map(|n| {
                        let p = n / count;
                        p * p.log2()
                    })
                    .sum::<f32>()
                    / 4.0
            }
            Action::LocalRank => (rank + 0.5 * ties) / count,
            Action::TensorCoherence => {
                ((xx - yy).hypot(2.0 * xy) / (xx + yy).max(1e-12)).clamp(0.0, 1.0)
            }
            Action::HarrisCorners => {
                xx /= count;
                xy /= count;
                yy /= count;
                ((xx * yy - xy * xy - 0.04 * (xx + yy).powi(2)).max(0.0) * 256.0)
                    .sqrt()
                    .min(1.0)
            }
            Action::SpecularResidual => {
                let p = self.frame.rgb[index];
                let max = p.into_iter().fold(0.0_f32, f32::max);
                let min = p.into_iter().fold(1.0_f32, f32::min);
                ((centre - mean).max(0.0) * 4.0 * (1.0 - max + min)).min(1.0)
            }
            Action::ShadowChromaDeviation => {
                let p = self.frame.rgb[index];
                ((p[0] - p[1] - chroma_mean[0] / count).hypot(p[2] - p[1] - chroma_mean[1] / count)
                    * (1.0 - centre)
                    * 2.0)
                    .min(1.0)
            }
            _ => 0.0,
        }
    }
    fn value(&self, x: usize, y: usize, action: Action, options: &Options) -> f32 {
        let i = self.index(x, y);
        let p = self.frame.rgb[i];
        let max = p.into_iter().fold(0.0_f32, f32::max);
        let min = p.into_iter().fold(1.0_f32, f32::min);
        match action {
            Action::HueArc => {
                if max - min < 1e-6 {
                    return -2.0;
                }
                let angle = (3.0_f32.sqrt() * (p[1] - p[2]))
                    .atan2(2.0 * p[0] - p[1] - p[2])
                    .to_degrees();
                let distance = (angle - options.angle + 180.0).rem_euclid(360.0) - 180.0;
                (1.0 - (distance.abs() - options.sweep * 0.5).max(0.0)
                    / (options.tolerance * 180.0))
                    .clamp(0.0, 1.0)
            }
            Action::OpponentChroma => ((p[0] - p[1]).powi(2)
                + ((p[0] + p[1]) * 0.5 - p[2]).powi(2))
            .sqrt()
            .min(1.0),
            Action::TemperatureBalance => (0.5 + (p[0] - p[2]) * 0.5).clamp(0.0, 1.0),
            Action::ChannelClipping => {
                p.into_iter()
                    .filter(|v| *v <= options.tolerance || *v >= 1.0 - options.tolerance)
                    .count() as f32
                    / 3.0
            }
            Action::NeutralAxis => {
                let mean = p.into_iter().sum::<f32>() / 3.0;
                (1.0 - p
                    .into_iter()
                    .map(|v| (v - mean).powi(2))
                    .sum::<f32>()
                    .sqrt()
                    / 0.816_496_6)
                    .clamp(0.0, 1.0)
            }
            Action::LaplacianDetail => {
                let at = |x, y| self.frame.luminance[self.index(x, y)];
                (at(x.saturating_sub(1), y)
                    + at((x + 1).min(self.width - 1), y)
                    + at(x, y.saturating_sub(1))
                    + at(x, (y + 1).min(self.height - 1))
                    - 4.0 * at(x, y))
                .abs()
                .min(1.0)
            }
            Action::EdgeOrientation => {
                let [gx, gy] = self.gradient(x, y, None);
                let theta = options.angle.to_radians();
                ((gx * theta.cos() + gy * theta.sin()).abs() * 2.0).min(1.0)
            }
            Action::ChromaticBoundary => {
                let mut sum = 0.0;
                for channel in 0..3 {
                    let [gx, gy] = self.gradient(x, y, Some(channel));
                    let [lx, ly] = self.gradient(x, y, None);
                    sum += (gx - lx).powi(2) + (gy - ly).powi(2);
                }
                (sum / 3.0).sqrt().min(1.0)
            }
            _ => self.local(x, y, options.radius as usize, action),
        }
    }
}
pub(super) fn apply(
    plane: &mut Plane,
    action: Action,
    options: &Options,
    frame: &Frame,
    cancel: &CancelToken,
) -> AuraResult<()> {
    if frame.rgb.len() != plane.alpha.len() || options.radius > 8 {
        return Err(invalid("Invalid photographic measurement grid or radius."));
    }
    let samples = Samples {
        frame,
        width: plane.width,
        height: plane.height,
    };
    let mut output = vec![0; plane.alpha.len()];
    for y in 0..plane.height {
        check(cancel)?;
        for x in 0..plane.width {
            let value = samples.value(x, y, action, options);
            output[y * plane.width + x] = byte(if action == Action::HueArc {
                value
            } else {
                band(value, options)
            });
        }
    }
    plane.alpha = output;
    Ok(())
}
