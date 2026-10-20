//! Coverage-premultiplied linear pixels. Filtering excluded pixels cannot leak colour.
// All indices are derived from bounded rectangles or checked before access.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::Rect;

struct Level {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 4]>,
}

pub(super) struct Pyramid {
    rect: Rect,
    levels: Vec<Level>,
}

impl Pyramid {
    pub(super) fn capture(
        rgb: &[f32],
        width: usize,
        height: usize,
        rect: Rect,
        coverage: &Coverage,
        spacing: f32,
    ) -> Self {
        let pixels = rect
            .cells()
            .map(|(x, y)| {
                let weight = coverage.at(x, y, width, height);
                let i = (y * width + x) * 3;
                [
                    rgb[i] * weight,
                    rgb[i + 1] * weight,
                    rgb[i + 2] * weight,
                    weight,
                ]
            })
            .collect();
        let mut levels = vec![Level {
            width: rect.w,
            height: rect.h,
            pixels,
        }];
        let depth = spacing.max(1.0).log2().ceil() as usize;
        for index in 0..depth {
            let previous = &levels[index];
            if previous.width == 1 && previous.height == 1 {
                break;
            }
            let (w, h) = (previous.width.div_ceil(2), previous.height.div_ceil(2));
            let mut pixels = Vec::with_capacity(w * h);
            for y in 0..h {
                for x in 0..w {
                    let mut value = [0.0; 4];
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let (sx, sy) = (x * 2 + dx, y * 2 + dy);
                            if sx < previous.width && sy < previous.height {
                                for (c, channel) in value.iter_mut().enumerate() {
                                    // Missing cells are transparent, not duplicated edge pixels.
                                    *channel += previous.pixels[sy * previous.width + sx][c] * 0.25;
                                }
                            }
                        }
                    }
                    pixels.push(value);
                }
            }
            levels.push(Level {
                width: w,
                height: h,
                pixels,
            });
        }
        Self { rect, levels }
    }

    pub(super) fn sample(&self, point: [f32; 2], level: f32) -> [f32; 4] {
        let level = level.clamp(0.0, (self.levels.len() - 1) as f32);
        let lower = level.floor() as usize;
        let upper = (lower + 1).min(self.levels.len() - 1);
        let local = [
            point[0] - self.rect.x0 as f32,
            point[1] - self.rect.y0 as f32,
        ];
        let first = self.at_level(local, lower);
        if upper == lower {
            return first;
        }
        let second = self.at_level(local, upper);
        let fraction = level - lower as f32;
        std::array::from_fn(|c| first[c] + fraction * (second[c] - first[c]))
    }

    fn at_level(&self, point: [f32; 2], index: usize) -> [f32; 4] {
        let level = &self.levels[index];
        let scale = (1_usize << index) as f32;
        let x = (point[0] + 0.5) / scale - 0.5;
        let y = (point[1] + 0.5) / scale - 0.5;
        let (left, top) = (x.floor() as isize, y.floor() as isize);
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let mut value = [0.0; 4];
        for dy in 0..2 {
            for dx in 0..2 {
                let (sx, sy) = (left + dx, top + dy);
                if sx < 0 || sy < 0 || sx as usize >= level.width || sy as usize >= level.height {
                    continue;
                }
                let weight =
                    (if dx == 0 { 1.0 - fx } else { fx }) * (if dy == 0 { 1.0 - fy } else { fy });
                for (c, channel) in value.iter_mut().enumerate() {
                    *channel += level.pixels[sy as usize * level.width + sx as usize][c] * weight;
                }
            }
        }
        value
    }
}
