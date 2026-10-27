//! Segmentation mattes at render resolution, with edges re-derived from the photograph. ADR-0082.
//!
//! Stored mattes have up to 512 cells per side. At full resolution each cell can still span
//! several pixels. Colour-guided refinement tightens these uncertain boundaries against the
//! actual image. Protected portrait mattes use exterior-only contraction: enclosed feature
//! holes cannot become background references, and no additional pixel can become selected.
// Dimensions are validated by the caller; every index is inside a bounds-checked rectangle.
#![allow(clippy::indexing_slicing)]

use aura_recipe::retouch_tools::Matte;
use rayon::prelude::*;

pub(crate) const VERSION: &str = "bounded-contour-v2-zero-cell-support";

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
        // Rows are independent: upsampled in parallel and joined in order.
        let mut plane: Vec<f32> = (y0..y1)
            .into_par_iter()
            .flat_map_iter(|y| {
                let fy = (((y as f32 + 0.5) - t * h as f32) * sy - 0.5).clamp(0.0, (mh - 1) as f32);
                let gy0 = fy as usize;
                let gy1 = (gy0 + 1).min(mh - 1);
                let ty = fy - gy0 as f32;
                (x0..x1).map(move |x| {
                    let fx =
                        (((x as f32 + 0.5) - l * w as f32) * sx - 0.5).clamp(0.0, (mw - 1) as f32);
                    let gx0 = fx as usize;
                    let gx1 = (gx0 + 1).min(mw - 1);
                    let tx = fx - gx0 as f32;
                    // A zero cell in an authored protection matte is a hard exclusion.
                    // Bilinear interpolation alone gives its interior partial coverage
                    // (even its centre at even upsampling ratios). Preserve its footprint.
                    if matte.refine_contour {
                        let nx = ((fx + 0.5).floor() as usize).min(mw - 1);
                        let ny = ((fy + 0.5).floor() as usize).min(mh - 1);
                        if cell(ny * mw + nx) == 0.0 {
                            return 0.0;
                        }
                    }
                    let top = cell(gy0 * mw + gx0) * (1.0 - tx) + cell(gy0 * mw + gx1) * tx;
                    let bottom = cell(gy1 * mw + gx0) * (1.0 - tx) + cell(gy1 * mw + gx1) * tx;
                    top * (1.0 - ty) + bottom * ty
                })
            })
            .collect();
        // Pixels per matte cell; below about one and a half there is no edge to recover.
        let scale = (1.0 / sx).max(1.0 / sy);
        if scale >= 1.5 && (matte.refine_edges || matte.refine_contour) {
            let guide: Vec<[f32; 3]> = (y0..y1)
                .into_par_iter()
                .flat_map_iter(|y| (x0..x1).map(move |x| (y * w + x) * 3))
                // Perceptual, so an edge in the shadows counts as much as one in the light.
                .map(|i| [0, 1, 2].map(|c| rgb[i + c].max(0.0).powf(1.0 / 2.2).min(1.5)))
                .collect();
            let radius = ((scale * 2.0).round() as usize).max(2);
            let authored = plane;
            let exterior = matte.refine_contour.then(|| exterior(&authored, bw, bh));
            plane = two_colour(&guide, &authored, bw, bh, radius, exterior.as_deref());
            let luma: Vec<f32> = guide
                .par_iter()
                .map(|p| p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593)
                .collect();
            if !matte.refine_contour {
                plane = guided(&luma, &plane, bw, bh, (radius / 4).max(1), 1e-3);
            }
            for (value, original) in plane.iter_mut().zip(authored) {
                // A filter must not reopen a protected hole or grow beyond support.
                if matte.refine_contour {
                    *value = value.min(original);
                } else if original == 0.0 {
                    *value = 0.0;
                }
            }
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
fn two_colour(
    guide: &[[f32; 3]],
    plane: &[f32],
    w: usize,
    h: usize,
    r: usize,
    exterior: Option<&[bool]>,
) -> Vec<f32> {
    let channel = |c: usize, inside: bool| -> Vec<f32> {
        guide
            .par_iter()
            .zip(plane.par_iter())
            .enumerate()
            .map(|(i, (p, a))| {
                let sure = if inside {
                    *a > 0.9
                } else {
                    *a < 0.1 && exterior.is_none_or(|e| e[i])
                };
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
            .enumerate()
            .map(|(i, a)| {
                let sure = if inside {
                    *a > 0.9
                } else {
                    *a < 0.1 && exterior.is_none_or(|e| e[i])
                };
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
        .into_par_iter()
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

/// Background connected to the crop border. Internal eye/bindi/blemish exclusions
/// must never become outside-colour references for contour contraction.
fn exterior(plane: &[f32], w: usize, h: usize) -> Vec<bool> {
    let mut seen = vec![false; w * h];
    let mut stack = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if (x == 0 || y == 0 || x + 1 == w || y + 1 == h) && plane[i] < 0.1 {
                seen[i] = true;
                stack.push(i);
            }
        }
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        for (valid, next) in [
            (x > 0, i.wrapping_sub(1)),
            (x + 1 < w, i + 1),
            (y > 0, i.wrapping_sub(w)),
            (y + 1 < h, i + w),
        ] {
            if valid && !seen[next] && plane[next] < 0.1 {
                seen[next] = true;
                stack.push(next);
            }
        }
    }
    seen
}

/// Mean over a (2r+1)^2 window clipped at the borders.
///
/// The renderer's one box mean, [`crate::bands::box_mean`]: a running sum per row and per
/// column, every core in use, the same result on every machine. An integral image built on one
/// core did the same job two and a half times slower, and agreed with it to about 3e-8.
fn box_mean(values: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    crate::bands::box_mean(values, w, h, r)
}

/// Guided filter (He, Sun and Tang), clamped to 0..1.
fn guided(guide: &[f32], src: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    let mean_i = box_mean(guide, w, h, r);
    let mean_p = box_mean(src, w, h, r);
    let ip: Vec<f32> = guide.iter().zip(src).map(|(a, b)| a * b).collect();
    let ii: Vec<f32> = guide.iter().map(|a| a * a).collect();
    let corr_ip = box_mean(&ip, w, h, r);
    let corr_ii = box_mean(&ii, w, h, r);
    let (a, b): (Vec<f32>, Vec<f32>) = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let var = (corr_ii[i] - mean_i[i] * mean_i[i]).max(0.0);
            let cov = corr_ip[i] - mean_i[i] * mean_p[i];
            let a = cov / (var + eps);
            (a, mean_p[i] - a * mean_i[i])
        })
        .unzip();
    let mean_a = box_mean(&a, w, h, r);
    let mean_b = box_mean(&b, w, h, r);
    (0..w * h)
        .into_par_iter()
        .map(|i| (mean_a[i] * guide[i] + mean_b[i]).clamp(0.0, 1.0))
        .collect()
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Exact exclusion and identity are part of the mask contract.
mod tests {
    use super::*;

    #[test]
    fn contour_contraction_removes_spill_without_reopening_feature_holes() {
        let (w, h) = (100, 100);
        let rgb: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                if i % w < 50 {
                    [0.02; 3]
                } else {
                    [0.4, 0.25, 0.18]
                }
            })
            .collect();
        let alpha: Vec<u8> = (0..25 * 25)
            .map(|i| {
                let (x, y) = (i % 25, i / 25);
                if x == 20 && y == 12 {
                    return 0;
                }
                match x {
                    0..=10 => 0,
                    11 => 80,
                    12 => 175,
                    _ => 255,
                }
            })
            .collect();
        let mut matte = Matte::encode([0.0, 0.0, 1.0, 1.0], 25, 25, &alpha);
        matte.refine_edges = false;
        let original = MattePlane::render(&matte, &rgb, w, h).unwrap();
        matte.refine_contour = true;
        let contracted = MattePlane::render(&matte, &rgb, w, h).unwrap();
        let outside = |p: &MattePlane| (40..50).map(|x| p.at(x, 40)).sum::<f32>();
        assert!(outside(&contracted) < outside(&original) * 0.3);
        assert!(contracted.at(60, 40) > 0.99);
        for y in 0..h {
            for x in 0..w {
                assert!(contracted.at(x, y) <= original.at(x, y) + 1e-6);
                // The protected cell's entire footprint stays excluded at even scales.
                if (80..84).contains(&x) && (48..52).contains(&y) {
                    assert_eq!(contracted.at(x, y), 0.0);
                }
            }
        }
    }

    #[test]
    fn protected_cells_survive_even_odd_and_noninteger_resampling() {
        let mut alpha = vec![255; 10 * 10];
        alpha[55] = 0;
        for (w, h) in [(80, 60), (90, 90), (73, 117)] {
            let mut matte = Matte::encode([0.0, 0.0, 1.0, 1.0], 10, 10, &alpha);
            matte.refine_contour = true;
            let plane = MattePlane::render(&matte, &vec![0.3; w * h * 3], w, h).unwrap();
            for y in 0..h {
                for x in 0..w {
                    let mx = ((x as f32 + 0.5) * 10.0 / w as f32) as usize;
                    let my = ((y as f32 + 0.5) * 10.0 / h as f32) as usize;
                    if mx == 5 && my == 5 {
                        assert_eq!(plane.at(x, y), 0.0, "{w}x{h} at {x},{y}");
                    }
                }
            }
            assert!(plane.at(w / 4, h / 4) > 0.99);
        }
    }

    #[test]
    fn guided_filter_never_reopens_authored_zero_coverage() {
        let mut alpha = vec![255; 10 * 10];
        alpha[55] = 0;
        let matte = Matte::encode([0.0, 0.0, 1.0, 1.0], 10, 10, &alpha);
        let plane = MattePlane::render(&matte, &vec![0.3; 90 * 90 * 3], 90, 90).unwrap();
        // The centre of the zero cell is also a pixel centre at this scale.
        assert_eq!(plane.at(49, 49), 0.0);
        assert!(plane.at(20, 20) > 0.99);
    }

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
