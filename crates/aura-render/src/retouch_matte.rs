//! Segmentation mattes at render resolution, with edges re-derived from the photograph. ADR-0082.
//!
//! A stored matte is at most 256 cells on its long side, so on a 24-megapixel export one cell
//! covers twenty pixels. Upsampling it bilinearly would put a twenty-pixel ramp across every jaw
//! line. Instead the bilinear plane is passed through a guided filter whose guide is the frame's
//! own luminance, which moves each soft edge onto the edge in the photograph - the same
//! refinement the segmenter applied at analysis resolution, repeated at whatever resolution is
//! being rendered, so preview and export agree about where the skin ends.
// Dimensions are validated by the caller; every index is inside a bounds-checked rectangle.
#![allow(clippy::indexing_slicing)]

use aura_recipe::retouch_tools::Matte;

/// A matte rendered over a pixel rectangle of the working buffer.
#[derive(Debug, Clone)]
pub(crate) struct MattePlane {
    pub bounds: [usize; 4],
    pixels: Vec<f32>,
}

impl MattePlane {
    pub(crate) fn at(&self, x: usize, y: usize) -> f32 {
        let [x0, y0, x1, y1] = self.bounds;
        if x < x0 || y < y0 || x >= x1 || y >= y1 {
            return 0.0;
        }
        self.pixels[(y - y0) * (x1 - x0) + x - x0]
    }

    /// Render `matte` over a `w x h` linear Rec.2020 buffer. `None` when it does not decode
    /// or covers no pixel.
    pub(crate) fn render(matte: &Matte, rgb: &[f32], w: usize, h: usize) -> Option<Self> {
        let alpha = matte.decode()?;
        let (mw, mh) = (matte.width as usize, matte.height as usize);
        let [l, t, r, b] = matte.bounds;
        let x0 = ((l * w as f32).floor().max(0.0) as usize).min(w);
        let y0 = ((t * h as f32).floor().max(0.0) as usize).min(h);
        let x1 = ((r * w as f32).ceil().max(0.0) as usize).min(w);
        let y1 = ((b * h as f32).ceil().max(0.0) as usize).min(h);
        if x1 <= x0 || y1 <= y0 || rgb.len() != w * h * 3 {
            return None;
        }
        let (bw, bh) = (x1 - x0, y1 - y0);
        let cell = |v: usize| f32::from(alpha[v]) / 255.0;
        // Pixel centre to matte-cell coordinates.
        let sx = mw as f32 / ((r - l) * w as f32).max(1e-6);
        let sy = mh as f32 / ((b - t) * h as f32).max(1e-6);
        let mut plane = Vec::with_capacity(bw * bh);
        for y in y0..y1 {
            let fy = (((y as f32 + 0.5) - t * h as f32) * sy - 0.5).clamp(0.0, (mh - 1) as f32);
            let gy0 = fy as usize;
            let gy1 = (gy0 + 1).min(mh - 1);
            let ty = fy - gy0 as f32;
            for x in x0..x1 {
                let fx = (((x as f32 + 0.5) - l * w as f32) * sx - 0.5).clamp(0.0, (mw - 1) as f32);
                let gx0 = fx as usize;
                let gx1 = (gx0 + 1).min(mw - 1);
                let tx = fx - gx0 as f32;
                let top = cell(gy0 * mw + gx0) * (1.0 - tx) + cell(gy0 * mw + gx1) * tx;
                let bottom = cell(gy1 * mw + gx0) * (1.0 - tx) + cell(gy1 * mw + gx1) * tx;
                plane.push(top * (1.0 - ty) + bottom * ty);
            }
        }
        // Pixels per matte cell; below about one and a half there is no edge to recover.
        let scale = (1.0 / sx).max(1.0 / sy);
        if scale >= 1.5 {
            let guide: Vec<[f32; 3]> = (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (y * w + x) * 3))
                // Perceptual, so an edge in the shadows counts as much as one in the light.
                .map(|i| [0, 1, 2].map(|c| rgb[i + c].max(0.0).powf(1.0 / 2.2).min(1.5)))
                .collect();
            let radius = ((scale * 2.0).round() as usize).max(2);
            plane = two_colour(&guide, &plane, bw, bh, radius);
            let luma: Vec<f32> = guide
                .iter()
                .map(|p| p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593)
                .collect();
            plane = guided(&luma, &plane, bw, bh, (radius / 4).max(1), 1e-3);
        }
        if plane.iter().all(|v| *v <= 0.0) {
            return None;
        }
        Some(Self {
            bounds: [x0, y0, x1, y1],
            pixels: plane,
        })
    }
}

/// Re-derive the uncertain band of a soft matte from the photograph's colours.
///
/// Near each pixel, the mean colour of what the matte is sure is *inside* and of what it is
/// sure is *outside* is measured; a pixel in between is then as selected as its own colour is
/// close to the inside colour along the line between the two. Where the two sides look alike
/// (skin against a skin-toned wall) there is nothing to snap to and the matte is kept.
fn two_colour(guide: &[[f32; 3]], plane: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let channel = |c: usize, inside: bool| -> Vec<f32> {
        guide
            .iter()
            .zip(plane)
            .map(|(p, a)| {
                let sure = if inside { *a > 0.9 } else { *a < 0.1 };
                if sure {
                    p[c]
                } else {
                    0.0
                }
            })
            .collect()
    };
    let count = |inside: bool| -> Vec<f32> {
        plane
            .iter()
            .map(|a| {
                let sure = if inside { *a > 0.9 } else { *a < 0.1 };
                f32::from(u8::from(sure))
            })
            .collect()
    };
    let n_in = box_mean(&count(true), w, h, r);
    let n_out = box_mean(&count(false), w, h, r);
    let m_in: Vec<Vec<f32>> = (0..3)
        .map(|c| box_mean(&channel(c, true), w, h, r))
        .collect();
    let m_out: Vec<Vec<f32>> = (0..3)
        .map(|c| box_mean(&channel(c, false), w, h, r))
        .collect();
    (0..w * h)
        .map(|i| {
            let p = plane[i];
            if p <= 0.0 || p >= 1.0 || n_in[i] < 0.02 || n_out[i] < 0.02 {
                return p;
            }
            let inside = [0, 1, 2].map(|c| m_in[c][i] / n_in[i]);
            let outside = [0, 1, 2].map(|c| m_out[c][i] / n_out[i]);
            let axis = [0, 1, 2].map(|c| inside[c] - outside[c]);
            let length2 = axis.iter().map(|v| v * v).sum::<f32>();
            let along = (0..3)
                .map(|c| (guide[i][c] - outside[c]) * axis[c])
                .sum::<f32>()
                / length2.max(1e-9);
            // Full trust once the two sides differ by about a tenth of the range.
            let t = (length2.sqrt() / 0.1).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t);
            p * (1.0 - t) + along.clamp(0.0, 1.0) * t
        })
        .collect()
}

/// Mean over a (2r+1)^2 window clipped at the borders, via an integral image.
fn box_mean(values: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let stride = w + 1;
    let mut sum = vec![0.0_f64; stride * (h + 1)];
    for y in 0..h {
        let mut row = 0.0_f64;
        for x in 0..w {
            row += f64::from(values[y * w + x]);
            sum[(y + 1) * stride + x + 1] = sum[y * stride + x + 1] + row;
        }
    }
    let mut out = vec![0.0_f32; w * h];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
            let total = sum[y1 * stride + x1] - sum[y0 * stride + x1] - sum[y1 * stride + x0]
                + sum[y0 * stride + x0];
            out[y * w + x] = (total / ((y1 - y0) * (x1 - x0)) as f64) as f32;
        }
    }
    out
}

/// Guided filter (He, Sun and Tang), clamped to 0..1.
fn guided(guide: &[f32], src: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    let mean_i = box_mean(guide, w, h, r);
    let mean_p = box_mean(src, w, h, r);
    let ip: Vec<f32> = guide.iter().zip(src).map(|(a, b)| a * b).collect();
    let ii: Vec<f32> = guide.iter().map(|a| a * a).collect();
    let corr_ip = box_mean(&ip, w, h, r);
    let corr_ii = box_mean(&ii, w, h, r);
    let mut a = vec![0.0_f32; w * h];
    let mut b = vec![0.0_f32; w * h];
    for i in 0..w * h {
        let var = (corr_ii[i] - mean_i[i] * mean_i[i]).max(0.0);
        let cov = corr_ip[i] - mean_i[i] * mean_p[i];
        a[i] = cov / (var + eps);
        b[i] = mean_p[i] - a[i] * mean_i[i];
    }
    let mean_a = box_mean(&a, w, h, r);
    let mean_b = box_mean(&b, w, h, r);
    (0..w * h)
        .map(|i| (mean_a[i] * guide[i] + mean_b[i]).clamp(0.0, 1.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coarse_matte_snaps_to_the_photographs_edge_when_upsampled() {
        // A frame that is dark left of x = 50 and bright right of it, and a 10-cell matte that
        // selects the bright side with its edge smeared over two cells.
        let (w, h) = (100, 20);
        let rgb: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let v = if i % w < 50 { 0.02 } else { 0.5 };
                [v, v, v]
            })
            .collect();
        let alpha: Vec<u8> = (0..10 * 2)
            .map(|i| match i % 10 {
                0..=3 => 0,
                4 => 80,
                5 => 175,
                _ => 255,
            })
            .collect();
        let matte = Matte::encode([0.0, 0.0, 1.0, 1.0], 10, 2, &alpha);
        let plane = MattePlane::render(&matte, &rgb, w, h).unwrap();
        // Distance from the ideal step, against plain bilinear upsampling of the same cells.
        let bilinear = |x: usize| {
            let f = ((x as f32 + 0.5) / 10.0 - 0.5).clamp(0.0, 9.0);
            let (i, t) = (f as usize, f - f.floor());
            let at = |k: usize| f32::from(alpha[k.min(9)]) / 255.0;
            at(i) * (1.0 - t) + at(i + 1) * t
        };
        let ideal = |x: usize| if x < 50 { 0.0 } else { 1.0 };
        let error = |f: &dyn Fn(usize) -> f32| -> f32 {
            (35..65).map(|x| (f(x) - ideal(x)).abs()).sum::<f32>()
        };
        let guided_error = error(&|x| plane.at(x, 10));
        let bilinear_error = error(&bilinear);
        assert!(
            guided_error < bilinear_error * 0.4,
            "guided {guided_error} against bilinear {bilinear_error}"
        );
        assert!(plane.at(5, 10) < 0.01 && plane.at(95, 10) > 0.99);
        assert!(plane.at(5, 30).abs() < f32::EPSILON);
    }

    #[test]
    fn an_empty_or_undecodable_matte_renders_nothing() {
        let rgb = vec![0.2; 30 * 30 * 3];
        let empty = Matte::encode([0.2, 0.2, 0.8, 0.8], 4, 4, &[0; 16]);
        assert!(MattePlane::render(&empty, &rgb, 30, 30).is_none());
        let mut broken = Matte::encode([0.2, 0.2, 0.8, 0.8], 4, 4, &[255; 16]);
        broken.height = 5;
        assert!(MattePlane::render(&broken, &rgb, 30, 30).is_none());
    }
}
