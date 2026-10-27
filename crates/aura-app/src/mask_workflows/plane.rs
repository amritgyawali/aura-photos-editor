//! Bounded operations on stored full-frame coverage; no image colours are synthesized.
// Grid indices derive from checked dimensions. Byte conversions clamp first.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
use super::{check, invalid, Action};
// The recipe matte decoder's 512²-cell budget; keep aspect ratio within it.
const MAX_GRID_EDGE: usize = 512;
const HALF_COVERAGE: u8 = 128;
use aura_core::{progress::CancelToken, AuraResult};
use aura_recipe::retouch_tools::Matte;
use std::collections::VecDeque;

pub(super) struct Plane {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) alpha: Vec<u8>,
}
impl Plane {
    pub(super) fn from_rgb(rgb: &[u8], width: u32, height: u32) -> AuraResult<Self> {
        let (source_width, source_height) = (width as usize, height as usize);
        if source_width == 0
            || source_height == 0
            || source_width
                .checked_mul(source_height)
                .and_then(|n| n.checked_mul(3))
                != Some(rgb.len())
        {
            return Err(invalid("Invalid selection coverage."));
        }
        let scale =
            MAX_GRID_EDGE as f64 / source_width.max(source_height).max(MAX_GRID_EDGE) as f64;
        let width = ((source_width as f64 * scale).round() as usize).max(1);
        let height = ((source_height as f64 * scale).round() as usize).max(1);
        let mut alpha = vec![0; width * height];
        // Average every covered source sample, retaining small islands when reducing the grid.
        for y in 0..height {
            for x in 0..width {
                let (left, right) = (
                    x * source_width / width,
                    ((x + 1) * source_width / width).max(x * source_width / width + 1),
                );
                let (top, bottom) = (
                    y * source_height / height,
                    ((y + 1) * source_height / height).max(y * source_height / height + 1),
                );
                let mut sum = 0_u64;
                let mut count = 0_u64;
                for sy in top..bottom {
                    for sx in left..right {
                        sum += u64::from(rgb[(sy * source_width + sx) * 3]);
                        count += 1;
                    }
                }
                alpha[y * width + x] = ((sum + count / 2) / count) as u8;
            }
        }
        Ok(Self {
            width,
            height,
            alpha,
        })
    }
    pub(super) fn matte(&self) -> Matte {
        let mut matte = Matte::encode(
            [0.0, 0.0, 1.0, 1.0],
            self.width as u32,
            self.height as u32,
            &self.alpha,
        );
        // Re-running edge detection would change authored morphology or reopen filled holes.
        matte.refine_edges = false;
        matte.refine_contour = false;
        matte
    }
    pub(super) fn edit(
        &mut self,
        action: Action,
        value: u32,
        other: Option<&Self>,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        check(cancel)?;
        match action {
            Action::Invert => {
                for alpha in &mut self.alpha {
                    *alpha = 255 - *alpha;
                }
            }
            Action::Union | Action::Intersection | Action::Subtraction | Action::ExclusiveUnion => {
                self.combine(
                    action,
                    other.ok_or_else(|| invalid("Choose a second selection."))?,
                )?;
            }
            Action::Feather | Action::Expand | Action::Contract => {
                self.spatial(action, value as usize, cancel)?;
            }
            Action::EdgeContrast => {
                for a in &mut self.alpha {
                    let contrast = 1.0 + value as f32 * 0.04;
                    *a = ((f32::from(*a) - 127.5).mul_add(contrast, 127.5))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
            Action::RemoveIslands | Action::FillHoles => {
                self.components(action, value as usize, cancel)?;
            }
            Action::BorderSelection => {
                let original = self.alpha.clone();
                self.spatial(Action::Expand, value as usize, cancel)?;
                let outer = self.alpha.clone();
                self.alpha = original;
                self.spatial(Action::Contract, value as usize, cancel)?;
                for (a, b) in self.alpha.iter_mut().zip(outer) {
                    *a = b.saturating_sub(*a);
                }
            }
            Action::MedianCleanup => self.median(value as usize, cancel)?,
            Action::KeepLargest => self.keep_largest(cancel)?,
            Action::ConvexHull => self.convex_hull(cancel)?,
            _ => {}
        }
        check(cancel)
    }
    fn combine(&mut self, action: Action, other: &Self) -> AuraResult<()> {
        if self.width != other.width || self.height != other.height {
            return Err(invalid("Selection coverage dimensions differ."));
        }
        for (a, b) in self.alpha.iter_mut().zip(&other.alpha) {
            *a = match action {
                Action::Union => (*a).max(*b),
                Action::Intersection => ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8,
                Action::Subtraction => ((u16::from(*a) * (255 - u16::from(*b)) + 127) / 255) as u8,
                Action::ExclusiveUnion => a.abs_diff(*b),
                _ => *a,
            };
        }
        Ok(())
    }
    fn spatial(&mut self, action: Action, radius: usize, cancel: &CancelToken) -> AuraResult<()> {
        if radius == 0 {
            return Ok(());
        }
        let mut horizontal = vec![0; self.alpha.len()];
        for vertical in [false, true] {
            let source = if vertical { &horizontal } else { &self.alpha };
            let mut output = vec![0; source.len()];
            for y in 0..self.height {
                check(cancel)?;
                for x in 0..self.width {
                    let coordinate = if vertical { y } else { x };
                    let limit = if vertical { self.height } else { self.width };
                    let left = coordinate.saturating_sub(radius);
                    let right = (coordinate + radius + 1).min(limit);
                    let samples = (left..right).map(|c| {
                        source[if vertical {
                            c * self.width + x
                        } else {
                            y * self.width + c
                        }]
                    });
                    output[y * self.width + x] = match action {
                        Action::Expand => samples.max().unwrap_or(0),
                        Action::Contract => samples.min().unwrap_or(0),
                        _ => {
                            let count = (right - left) as u32;
                            ((samples.map(u32::from).sum::<u32>() + count / 2) / count) as u8
                        }
                    };
                }
            }
            if vertical {
                self.alpha = output;
            } else {
                horizontal = output;
            }
        }
        Ok(())
    }
    fn components(
        &mut self,
        action: Action,
        maximum: usize,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        if maximum == 0 {
            return Ok(());
        }
        let fill = action == Action::FillHoles;
        let mut visited = vec![false; self.alpha.len()];
        for start in 0..self.alpha.len() {
            if start % self.width == 0 {
                check(cancel)?;
            }
            if visited[start] || (self.alpha[start] < HALF_COVERAGE) != fill {
                continue;
            }
            visited[start] = true;
            let mut queue = VecDeque::from([start]);
            let mut group = Vec::new();
            let mut boundary = false;
            while let Some(index) = queue.pop_front() {
                if group.len() % 1024 == 0 {
                    check(cancel)?;
                }
                group.push(index);
                let (x, y) = (index % self.width, index / self.width);
                boundary |= x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height;
                let neighbors = [
                    x.checked_sub(1).map(|n| y * self.width + n),
                    (x + 1 < self.width).then_some(index + 1),
                    y.checked_sub(1).map(|n| n * self.width + x),
                    (y + 1 < self.height).then_some(index + self.width),
                ];
                for next in neighbors.into_iter().flatten() {
                    if !visited[next] && (self.alpha[next] < HALF_COVERAGE) == fill {
                        visited[next] = true;
                        queue.push_back(next);
                    }
                }
            }
            if group.len() <= maximum && (!fill || !boundary) {
                for index in group {
                    self.alpha[index] = if fill { 255 } else { 0 };
                }
            }
        }
        Ok(())
    }
}
