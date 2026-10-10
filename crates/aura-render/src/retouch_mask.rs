//! Resolution-independent brush coverage shared by preview and delivery. ADR-0074.
// Buffers come from dimension-checked input; all coordinates are bounded before indexing.
#![allow(clippy::indexing_slicing)]
use aura_recipe::retouch_tools::{BrushStroke, Edit};
use rayon::prelude::*;

#[derive(Clone)]
pub(crate) struct Coverage {
    pub bounds: [usize; 4],
    pixels: Option<Vec<f32>>,
    region: [f32; 4],
    feather: f32,
}

fn smooth(distance: f32, feather: f32) -> f32 {
    if distance >= 1.0 {
        return 0.0;
    }
    if feather < 0.001 {
        return 1.0;
    }
    let t = ((1.0 - distance) / feather).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn stroke_bounds(stroke: &BrushStroke, w: usize, h: usize) -> [usize; 4] {
    let radius = stroke.radius * w.min(h) as f32;
    let mut bounds = [w, h, 0, 0];
    for p in &stroke.points {
        let x = p[0] * w as f32;
        let y = p[1] * h as f32;
        bounds[0] = bounds[0].min((x - radius).max(0.0) as usize);
        bounds[1] = bounds[1].min((y - radius).max(0.0) as usize);
        bounds[2] = bounds[2].max((x + radius).ceil() as usize).min(w);
        bounds[3] = bounds[3].max((y + radius).ceil() as usize).min(h);
    }
    bounds
}

impl Coverage {
    /// This coverage multiplied by a segmentation matte, over the rectangle both cover.
    pub(crate) fn with_matte(
        self,
        matte: &crate::retouch_matte::MattePlane,
        w: usize,
        h: usize,
    ) -> Self {
        let [ax0, ay0, ax1, ay1] = self.bounds;
        let [bx0, by0, bx1, by1] = matte.bounds;
        let bounds = [ax0.max(bx0), ay0.max(by0), ax1.min(bx1), ay1.min(by1)];
        if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
            return Self {
                bounds: [0; 4],
                pixels: Some(Vec::new()),
                ..self
            };
        }
        // Rows in parallel, joined in order: the same coverage on every machine.
        let pixels: Vec<f32> = (bounds[1]..bounds[3])
            .into_par_iter()
            .flat_map_iter(|y| (bounds[0]..bounds[2]).map(move |x| (x, y)))
            .map(|(x, y)| self.at(x, y, w, h) * matte.at(x, y))
            .collect();
        Self {
            bounds,
            pixels: Some(pixels),
            ..self
        }
    }

    pub(crate) fn for_edit(edit: &Edit, w: usize, h: usize, rgb: &[f32]) -> Self {
        let base = Self::new(edit, w, h);
        let Some(selection) = &edit.selection else {
            return base;
        };
        if !selection.inverted && selection.gradient.is_none() && selection.luminance.is_none() {
            return base;
        }
        let bounds = if selection.inverted || selection.gradient.is_some() {
            [0, 0, w, h]
        } else {
            base.bounds
        };
        let [x0, y0, x1, y1] = bounds;
        let pixels: Vec<f32> = (y0..y1)
            .into_par_iter()
            .flat_map_iter(|y| (x0..x1).map(move |x| (x, y)))
            .map(|(x, y)| {
                let mut weight = selection.gradient.as_ref().map_or_else(
                    || base.at(x, y, w, h),
                    |g| {
                        let dx = (g.end[0] - g.start[0]) * w as f32;
                        let dy = (g.end[1] - g.start[1]) * h as f32;
                        let t = (((x as f32 + 0.5 - g.start[0] * w as f32) * dx
                            + (y as f32 + 0.5 - g.start[1] * h as f32) * dy)
                            / (dx * dx + dy * dy).max(1e-12))
                        .clamp(0.0, 1.0);
                        t * t * (3.0 - 2.0 * t)
                    },
                );
                if selection.inverted {
                    weight = 1.0 - weight;
                }
                if let Some(range) = &selection.luminance {
                    let i = (y * w + x) * 3;
                    // The caller checks RGB length; x/y are bounded by image dimensions.
                    #[allow(clippy::indexing_slicing)]
                    let luma = rgb[i] * 0.2627 + rgb[i + 1] * 0.678 + rgb[i + 2] * 0.0593;
                    // Clamp black to the lower endpoint, and HDR values to the upper endpoint.
                    let ev = (luma.max(0.18 * 2.0_f32.powi(-16)) / 0.18)
                        .log2()
                        .clamp(-16.0, 16.0);
                    let distance = (range.low - ev).max(ev - range.high).max(0.0);
                    let t = if range.softness <= 0.0 {
                        if distance <= 0.0 {
                            1.0
                        } else {
                            0.0
                        }
                    } else {
                        (1.0 - distance / range.softness).clamp(0.0, 1.0)
                    };
                    weight *= t * t * (3.0 - 2.0 * t);
                }
                weight
            })
            .collect();
        Self {
            bounds,
            pixels: Some(pixels),
            ..base
        }
    }

    pub(crate) fn new(edit: &Edit, w: usize, h: usize) -> Self {
        let [cx, cy, rx, ry] = edit.region;
        let mut result = Self {
            bounds: [
                ((cx - rx).max(0.0) * w as f32) as usize,
                ((cy - ry).max(0.0) * h as f32) as usize,
                (((cx + rx).min(1.0) * w as f32).ceil() as usize).min(w),
                (((cy + ry).min(1.0) * h as f32).ceil() as usize).min(h),
            ],
            pixels: None,
            region: edit.region,
            feather: edit.feather,
        };
        let Some(mask) = &edit.mask else {
            return result;
        };
        let mut bounds = [w, h, 0, 0];
        for stroke in mask.strokes.iter().filter(|s| !s.erase && s.opacity > 0.0) {
            let b = stroke_bounds(stroke, w, h);
            bounds = [
                bounds[0].min(b[0]),
                bounds[1].min(b[1]),
                bounds[2].max(b[2]),
                bounds[3].max(b[3]),
            ];
        }
        if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
            bounds = [0; 4];
        }
        result.bounds = bounds;
        let len = (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]);
        let mut pixels = vec![0.0; len];
        let mut stroke_pixels = vec![0.0; len];
        for stroke in &mask.strokes {
            stroke_pixels.par_iter_mut().for_each(|v| *v = 0.0);
            paint_stroke(&mut stroke_pixels, bounds, stroke, edit.feather, w, h);
            pixels
                .par_iter_mut()
                .zip(stroke_pixels.par_iter())
                .for_each(|(value, coverage)| {
                    let alpha = coverage * stroke.opacity;
                    *value = if stroke.erase {
                        *value * (1.0 - alpha)
                    } else {
                        f32::max(*value, alpha)
                    };
                });
        }
        result.pixels = Some(pixels);
        result
    }

    pub(crate) fn at(&self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        let [x0, y0, x1, y1] = self.bounds;
        if x < x0 || y < y0 || x >= x1 || y >= y1 {
            return 0.0;
        }
        if let Some(pixels) = &self.pixels {
            return pixels[(y - y0) * (x1 - x0) + x - x0];
        }
        let [cx, cy, rx, ry] = self.region;
        let dx = ((x as f32 + 0.5) / w as f32 - cx) / rx;
        let dy = ((y as f32 + 0.5) / h as f32 - cy) / ry;
        smooth(dx.hypot(dy), self.feather)
    }
}

pub(crate) fn paint_stroke(
    out: &mut [f32],
    bounds: [usize; 4],
    stroke: &BrushStroke,
    feather: f32,
    w: usize,
    h: usize,
) {
    let radius = stroke.radius * w.min(h) as f32;
    let point = |p: [f32; 3]| [p[0] * w as f32, p[1] * h as f32, radius * p[2].max(0.1)];
    for (i, p) in stroke.points.iter().enumerate() {
        let a = point(*p);
        let b = point(*stroke.points.get(i + 1).unwrap_or(p));
        // Split long pointer jumps so diagonal strokes don't scan their entire bounding rectangle.
        let parts = ((b[0] - a[0]).hypot(b[1] - a[1]) / (radius * 2.0).max(1.0))
            .ceil()
            .max(1.0) as usize;
        for part in 0..parts {
            let lerp = |t: f32| std::array::from_fn(|c| a[c] + (b[c] - a[c]) * t);
            paint_segment(
                out,
                bounds,
                lerp(part as f32 / parts as f32),
                lerp((part + 1) as f32 / parts as f32),
                feather,
            );
        }
    }
}

fn paint_segment(out: &mut [f32], bounds: [usize; 4], a: [f32; 3], b: [f32; 3], feather: f32) {
    let [bx, by, ex, ey] = bounds;
    let radius = a[2].max(b[2]);
    let x0 = ((a[0].min(b[0]) - radius).max(0.0) as usize).max(bx);
    let y0 = ((a[1].min(b[1]) - radius).max(0.0) as usize).max(by);
    let x1 = ((a[0].max(b[0]) + radius).ceil() as usize).min(ex);
    let y1 = ((a[1].max(b[1]) + radius).ceil() as usize).min(ey);
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length2 = dx * dx + dy * dy;
    if y1 <= y0 || x1 <= x0 {
        return;
    }
    // Each row of the segment's box is painted by one task; rows never overlap.
    let stride = ex - bx;
    out.par_chunks_mut(stride)
        .enumerate()
        .skip(y0 - by)
        .take(y1 - y0)
        .for_each(|(row, line)| {
            let y = by + row;
            for x in x0..x1 {
                let px = x as f32 + 0.5 - a[0];
                let py = y as f32 + 0.5 - a[1];
                let t = if length2 > 0.00001 {
                    ((px * dx + py * dy) / length2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let distance =
                    (px - t * dx).hypot(py - t * dy) / (a[2] + t * (b[2] - a[2])).max(0.00001);
                let i = x - bx;
                line[i] = line[i].max(smooth(distance, feather));
            }
        });
}
