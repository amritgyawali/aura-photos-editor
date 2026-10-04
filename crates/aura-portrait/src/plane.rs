//! A soft region: one `0..=1` weight per pixel of the analysis grid.
//!
//! Every operation here clamps at the frame edge rather than reading zero outside it. Phase
//! 18 found the alternative the hard way - a resampler that read black past the border drew a
//! one-pixel dark rim around every mask - and a region that thins toward the frame edge is a
//! retouch that stops a few pixels short of a face cut by the crop.

use rayon::prelude::*;

/// One weight per pixel, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` weights, nominally `0..=1`.
    pub values: Vec<f32>,
}

impl Plane {
    /// An empty plane.
    #[must_use]
    pub fn zeros(width: u32, height: u32) -> Self {
        Self::filled(width, height, 0.0)
    }

    /// A plane with every weight set.
    #[must_use]
    pub fn filled(width: u32, height: u32, value: f32) -> Self {
        Self {
            width,
            height,
            values: vec![value; (width as usize) * (height as usize)],
        }
    }

    /// Wrap a buffer, padding or truncating it to the size it claims.
    #[must_use]
    pub fn from_values(width: u32, height: u32, mut values: Vec<f32>) -> Self {
        values.resize((width as usize) * (height as usize), 0.0);
        Self {
            width,
            height,
            values,
        }
    }

    /// Number of pixels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True when the plane has no pixels.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The weight at a pixel, clamped into the frame.
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> f32 {
        if self.width == 0 || self.height == 0 {
            return 0.0;
        }
        let cx = x.clamp(0, i64::from(self.width) - 1) as usize;
        let cy = y.clamp(0, i64::from(self.height) - 1) as usize;
        self.values
            .get(cy * self.width as usize + cx)
            .copied()
            .unwrap_or(0.0)
    }

    /// Set the weight at a pixel. Outside the frame does nothing.
    pub fn set(&mut self, x: i64, y: i64, value: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let index = y as usize * self.width as usize + x as usize;
        if let Some(slot) = self.values.get_mut(index) {
            *slot = value;
        }
    }

    /// Bilinear sample at a continuous position, in pixel units with pixel centres at `+0.5`.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let (ix, iy) = (x0 as i64, y0 as i64);
        let top = self.at(ix, iy) * (1.0 - tx) + self.at(ix + 1, iy) * tx;
        let bottom = self.at(ix, iy + 1) * (1.0 - tx) + self.at(ix + 1, iy + 1) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    /// Bilinear resize, centre-aligned, clamped at the edges.
    #[must_use]
    pub fn resize(&self, width: u32, height: u32) -> Self {
        if width == self.width && height == self.height {
            return self.clone();
        }
        let sx = self.width as f32 / width.max(1) as f32;
        let sy = self.height as f32 / height.max(1) as f32;
        let mut out = Vec::with_capacity((width as usize) * (height as usize));
        for y in 0..height {
            let py = (y as f32 + 0.5) * sy;
            for x in 0..width {
                out.push(self.sample((x as f32 + 0.5) * sx, py));
            }
        }
        Self::from_values(width, height, out)
    }

    /// Mean weight. The share of the frame this region covers.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        if self.values.is_empty() {
            return 0.0;
        }
        let total: f64 = self
            .values
            .iter()
            .map(|v| f64::from(v.clamp(0.0, 1.0)))
            .sum();
        (total / self.values.len() as f64) as f32
    }

    /// Clamp every weight into `0..=1`.
    pub fn clamp_unit(&mut self) {
        for v in &mut self.values {
            *v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
    }

    /// Pointwise product.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        self.zip(other, |a, b| a * b)
    }

    /// Pointwise maximum: the union of two soft regions.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        self.zip(other, f32::max)
    }

    /// `self` with `other` taken out of it.
    #[must_use]
    pub fn subtract(&self, other: &Self) -> Self {
        self.zip(other, |a, b| (a * (1.0 - b.clamp(0.0, 1.0))).max(0.0))
    }

    /// One minus every weight.
    #[must_use]
    pub fn invert(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            values: self
                .values
                .iter()
                .map(|v| 1.0 - v.clamp(0.0, 1.0))
                .collect(),
        }
    }

    /// Combine two planes of the same size. A size mismatch combines against zero.
    #[must_use]
    pub fn zip(&self, other: &Self, f: impl Fn(f32, f32) -> f32) -> Self {
        let values = self
            .values
            .iter()
            .enumerate()
            .map(|(i, a)| f(*a, other.values.get(i).copied().unwrap_or(0.0)))
            .collect();
        Self {
            width: self.width,
            height: self.height,
            values,
        }
    }

    /// Map every weight.
    #[must_use]
    pub fn map(&self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            width: self.width,
            height: self.height,
            values: self.values.iter().map(|v| f(*v)).collect(),
        }
    }

    /// A box blur of the given radius, three passes - close to a Gaussian of sigma `radius`.
    ///
    /// Running sums, so the cost does not grow with the radius: a hair matte is blurred at a
    /// radius of a fortieth of the frame and a teeth matte at two pixels, through the same code.
    /// Rows run in parallel; columns are blurred as the rows of the transpose, which keeps
    /// every pass a contiguous sweep and every pass parallel.
    #[must_use]
    pub fn blur(&self, radius: u32) -> Self {
        if radius == 0 || self.is_empty() {
            return self.clone();
        }
        Self::from_values(
            self.width,
            self.height,
            blur_values(
                &self.values,
                self.width as usize,
                self.height as usize,
                radius as usize,
            ),
        )
    }

    /// Grey-level dilation with a square of the given radius.
    #[must_use]
    pub fn dilate(&self, radius: u32) -> Self {
        self.morph(radius, f32::max, f32::NEG_INFINITY)
    }

    /// Grey-level erosion with a square of the given radius.
    #[must_use]
    pub fn erode(&self, radius: u32) -> Self {
        self.morph(radius, f32::min, f32::INFINITY)
    }

    /// Closing: dilate then erode. Fills holes narrower than the radius.
    #[must_use]
    pub fn close(&self, radius: u32) -> Self {
        self.dilate(radius).erode(radius)
    }

    /// Opening: erode then dilate. Removes specks narrower than the radius.
    #[must_use]
    pub fn open(&self, radius: u32) -> Self {
        self.erode(radius).dilate(radius)
    }

    fn morph(&self, radius: u32, f: fn(f32, f32) -> f32, identity: f32) -> Self {
        if radius == 0 || self.is_empty() {
            return self.clone();
        }
        let w = self.width as usize;
        let h = self.height as usize;
        let r = radius as usize;
        let mut rows = vec![0.0_f32; self.values.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = identity;
                for sx in x.saturating_sub(r)..=(x + r).min(w - 1) {
                    acc = f(acc, self.values.get(y * w + sx).copied().unwrap_or(0.0));
                }
                if let Some(slot) = rows.get_mut(y * w + x) {
                    *slot = acc;
                }
            }
        }
        let mut out = vec![0.0_f32; self.values.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = identity;
                for sy in y.saturating_sub(r)..=(y + r).min(h - 1) {
                    acc = f(acc, rows.get(sy * w + x).copied().unwrap_or(0.0));
                }
                if let Some(slot) = out.get_mut(y * w + x) {
                    *slot = acc;
                }
            }
        }
        Self::from_values(self.width, self.height, out)
    }

    /// Keep only the connected components (above `threshold`) that a seed plane touches.
    ///
    /// Four-connected. The kept components keep their soft weights; everything else is zeroed.
    /// This is how hair that is the same colour as a door frame three metres behind somebody
    /// stays hair and the door frame stays a door frame.
    #[must_use]
    pub fn keep_seeded(&self, seed: &Self, threshold: f32) -> Self {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut keep = vec![false; self.values.len()];
        let mut stack: Vec<usize> = Vec::new();
        for (i, s) in seed.values.iter().enumerate() {
            if *s > 0.5 && self.values.get(i).copied().unwrap_or(0.0) > threshold {
                if let Some(k) = keep.get_mut(i) {
                    if !*k {
                        *k = true;
                        stack.push(i);
                    }
                }
            }
        }
        while let Some(i) = stack.pop() {
            let x = i % w.max(1);
            let y = i / w.max(1);
            let mut neighbours = [usize::MAX; 4];
            if x > 0 {
                neighbours[0] = i - 1;
            }
            if x + 1 < w {
                neighbours[1] = i + 1;
            }
            if y > 0 {
                neighbours[2] = i - w;
            }
            if y + 1 < h {
                neighbours[3] = i + w;
            }
            for n in neighbours {
                if n == usize::MAX {
                    continue;
                }
                if self.values.get(n).copied().unwrap_or(0.0) <= threshold {
                    continue;
                }
                if let Some(k) = keep.get_mut(n) {
                    if !*k {
                        *k = true;
                        stack.push(n);
                    }
                }
            }
        }
        let values = self
            .values
            .iter()
            .zip(keep.iter())
            .map(|(v, k)| if *k { *v } else { 0.0 })
            .collect();
        Self::from_values(self.width, self.height, values)
    }

    /// Paint a soft rotated ellipse into the plane, keeping the larger of old and new.
    ///
    /// `feather` is the width of the soft edge as a fraction of the radii. Coordinates are in
    /// pixels of this plane.
    pub fn paint_ellipse(&mut self, ellipse: &Ellipse, feather: f32) {
        let reach = ellipse.rx.max(ellipse.ry) * (1.0 + feather.max(0.0)) + 2.0;
        let x0 = ((ellipse.cx - reach).floor() as i64).max(0);
        let x1 = ((ellipse.cx + reach).ceil() as i64).min(i64::from(self.width) - 1);
        let y0 = ((ellipse.cy - reach).floor() as i64).max(0);
        let y1 = ((ellipse.cy + reach).ceil() as i64).min(i64::from(self.height) - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let weight = ellipse.weight(x as f32 + 0.5, y as f32 + 0.5, feather);
                if weight <= 0.0 {
                    continue;
                }
                let old = self.at(x, y);
                if weight > old {
                    self.set(x, y, weight);
                }
            }
        }
    }

    /// The weighted centroid of the plane, or `None` when it is empty.
    #[must_use]
    pub fn centroid(&self) -> Option<(f32, f32)> {
        let w = self.width as usize;
        let mut sx = 0.0_f64;
        let mut sy = 0.0_f64;
        let mut total = 0.0_f64;
        for (i, v) in self.values.iter().enumerate() {
            let v = f64::from(v.max(0.0));
            if v <= 0.0 {
                continue;
            }
            sx += v * ((i % w.max(1)) as f64 + 0.5);
            sy += v * ((i / w.max(1)) as f64 + 0.5);
            total += v;
        }
        (total > 1e-6).then(|| ((sx / total) as f32, (sy / total) as f32))
    }
}

/// Three box passes of radius `r` over a row-major `w x h` buffer, clamped at the edges.
#[must_use]
pub fn blur_values(values: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return values.to_vec();
    }
    let mut current = values.to_vec();
    current.resize(w * h, 0.0);
    for _ in 0..3 {
        current = box_rows(&current, w, r);
        let turned = transpose(&current, w, h);
        let turned = box_rows(&turned, h, r);
        current = transpose(&turned, h, w);
    }
    current
}

fn box_rows(src: &[f32], w: usize, r: usize) -> Vec<f32> {
    let norm = 1.0 / (2 * r + 1) as f32;
    let mut out = vec![0.0_f32; src.len()];
    out.par_chunks_mut(w.max(1))
        .zip(src.par_chunks(w.max(1)))
        .for_each(|(dst, row)| {
            let last = row.len() as i64 - 1;
            let at = |x: i64| -> f32 {
                row.get(x.clamp(0, last.max(0)) as usize)
                    .copied()
                    .unwrap_or(0.0)
            };
            let mut sum = 0.0_f32;
            for x in -(r as i64)..=(r as i64) {
                sum += at(x);
            }
            for (x, slot) in dst.iter_mut().enumerate() {
                *slot = sum * norm;
                sum += at(x as i64 + r as i64 + 1) - at(x as i64 - r as i64);
            }
        });
    out
}

/// A row-major `w x h` buffer as a row-major `h x w` one.
fn transpose(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; w * h];
    out.par_chunks_mut(h.max(1))
        .enumerate()
        .for_each(|(x, column)| {
            for (y, slot) in column.iter_mut().enumerate() {
                *slot = src.get(y * w + x).copied().unwrap_or(0.0);
            }
        });
    out
}

/// A rotated ellipse in pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipse {
    /// Centre, x.
    pub cx: f32,
    /// Centre, y.
    pub cy: f32,
    /// Radius along the rotated x axis.
    pub rx: f32,
    /// Radius along the rotated y axis.
    pub ry: f32,
    /// Rotation in radians, clockwise in image coordinates.
    pub angle: f32,
}

impl Ellipse {
    /// The normalised radial distance of a point: `1.0` on the boundary.
    #[must_use]
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        let (s, c) = self.angle.sin_cos();
        let dx = x - self.cx;
        let dy = y - self.cy;
        let u = dx * c + dy * s;
        let v = -dx * s + dy * c;
        let a = u / self.rx.max(1e-3);
        let b = v / self.ry.max(1e-3);
        (a * a + b * b).sqrt()
    }

    /// A soft inside weight: one inside, falling to zero across `feather` of the radius.
    #[must_use]
    pub fn weight(&self, x: f32, y: f32, feather: f32) -> f32 {
        let d = self.distance(x, y);
        if feather <= 1e-4 {
            return if d <= 1.0 { 1.0 } else { 0.0 };
        }
        let t = ((d - (1.0 - feather * 0.5)) / feather).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_preserves_the_mean_and_the_constant() {
        let plane = Plane::filled(17, 9, 0.4);
        let blurred = plane.blur(5);
        for v in &blurred.values {
            assert!((v - 0.4).abs() < 1e-5);
        }
    }

    #[test]
    fn a_blur_spreads_a_point_symmetrically() {
        let mut plane = Plane::zeros(21, 21);
        plane.set(10, 10, 1.0);
        let blurred = plane.blur(2);
        assert!((blurred.at(8, 10) - blurred.at(12, 10)).abs() < 1e-6);
        assert!(blurred.at(10, 10) > blurred.at(12, 10));
    }

    #[test]
    fn resizing_never_darkens_the_edge() {
        let plane = Plane::filled(8, 8, 1.0);
        let big = plane.resize(31, 23);
        assert!(big.values.iter().all(|v| (v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn morphology_closes_a_hole() {
        let mut plane = Plane::filled(9, 9, 1.0);
        plane.set(4, 4, 0.0);
        assert!((plane.close(1).at(4, 4) - 1.0).abs() < 1e-6);
        let mut speck = Plane::zeros(9, 9);
        speck.set(4, 4, 1.0);
        assert!(speck.open(1).at(4, 4).abs() < 1e-6);
    }

    #[test]
    fn only_seeded_components_survive() {
        let mut plane = Plane::zeros(10, 3);
        for x in 0..3 {
            plane.set(x, 1, 1.0);
        }
        for x in 6..9 {
            plane.set(x, 1, 1.0);
        }
        let mut seed = Plane::zeros(10, 3);
        seed.set(1, 1, 1.0);
        let kept = plane.keep_seeded(&seed, 0.5);
        assert!((kept.at(2, 1) - 1.0).abs() < 1e-6);
        assert!(kept.at(7, 1).abs() < 1e-6);
    }

    #[test]
    fn an_ellipse_is_one_inside_and_zero_outside() {
        let mut plane = Plane::zeros(40, 40);
        let e = Ellipse {
            cx: 20.0,
            cy: 20.0,
            rx: 10.0,
            ry: 5.0,
            angle: 0.0,
        };
        plane.paint_ellipse(&e, 0.2);
        assert!((plane.at(20, 20) - 1.0).abs() < 1e-6);
        assert!(plane.at(20, 30).abs() < 1e-6);
        assert!(plane.at(28, 20) > 0.5);
        let (cx, cy) = plane.centroid().unwrap();
        assert!((cx - 20.0).abs() < 0.6 && (cy - 20.0).abs() < 0.6);
    }
}
