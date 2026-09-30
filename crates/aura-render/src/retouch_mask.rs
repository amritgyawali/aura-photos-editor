//! Resolution-independent brush coverage shared by preview and delivery. ADR-0069.
use aura_recipe::retouch_tools::{BrushStroke, Edit};

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
            stroke_pixels.fill(0.0);
            paint_stroke(&mut stroke_pixels, bounds, stroke, edit.feather, w, h);
            for (value, coverage) in pixels.iter_mut().zip(&stroke_pixels) {
                let alpha = coverage * stroke.opacity;
                *value = if stroke.erase {
                    *value * (1.0 - alpha)
                } else {
                    f32::max(*value, alpha)
                };
            }
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

fn paint_stroke(
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
    for y in y0..y1 {
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
            let i = (y - by) * (ex - bx) + x - bx;
            out[i] = out[i].max(smooth(distance, feather));
        }
    }
}
