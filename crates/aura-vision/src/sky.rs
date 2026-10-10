//! Sky, measured from the photograph. ADR-0102.
//!
//! No model: there is no sky segmenter in this repository whose licence and weights could ship,
//! so this is the classical method of Shen and Wang (2013), with the checks a photograph editor
//! needs on top. Going down each column, the sky ends at the first strong edge - a horizon, a
//! roof line, a tree's silhouette. The edge strength that separates sky from ground best is the
//! one whose two sides are each most uniform in colour, so it is searched for rather than
//! fixed: a cloudy sky needs a higher threshold than a clear one.
//!
//! On top of that:
//!
//! * the border is median-smoothed across columns, so one bright pole does not cut a notch;
//! * a frame where "sky" reaches the bottom of most columns - a studio backdrop, a wall - has
//!   no horizon, and so no sky;
//! * a horizon is a change of colour across the border, not only of texture, or the "border"
//!   is the noise of a plain backdrop;
//! * sky is brighter and smoother than the ground under it, or it is a ceiling or a night;
//! * the edge is then refined with a guided filter on the photograph's own luminance, and the
//!   renderer refines it again at full resolution, so it follows leaves and hair.
//!
//! What it does not do: a sky full of strong cloud edges (the border stops at the first cloud),
//! sky seen through gaps in a canopy below the tree line, and a night sky. Each comes back as
//! "no sky here" rather than a wrong selection; a learned sky model is what closes them.
// Every plane is `w * h` long from validated dimensions; indices stay inside it.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::similar_names
)]

use crate::skin::Matte;

/// Long edge of the grid the border is searched on.
const SEARCH_EDGE: usize = 384;
/// Long edge of the stored matte.
const MATTE_EDGE: usize = 256;

/// Packed sRGB reduced by box averaging to at most `edge` on the long side.
fn reduce(rgb: &[u8], w: usize, h: usize, edge: usize) -> (Vec<[f32; 3]>, usize, usize) {
    let factor = w.max(h).div_ceil(edge).max(1);
    let (ow, oh) = (w.div_ceil(factor), h.div_ceil(factor));
    let mut out = Vec::with_capacity(ow * oh);
    for y in 0..oh {
        let (y0, y1) = (y * h / oh, ((y + 1) * h / oh).max(y * h / oh + 1).min(h));
        for x in 0..ow {
            let (x0, x1) = (x * w / ow, ((x + 1) * w / ow).max(x * w / ow + 1).min(w));
            let mut sum = [0.0_f32; 3];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = (yy * w + xx) * 3;
                    for c in 0..3 {
                        sum[c] += f32::from(rgb[i + c]) / 255.0;
                    }
                }
            }
            let n = ((x1 - x0) * (y1 - y0)).max(1) as f32;
            out.push(sum.map(|v| v / n));
        }
    }
    (out, ow, oh)
}

fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
}

/// Sobel magnitude of luminance.
fn gradient(px: &[[f32; 3]], w: usize, h: usize) -> Vec<f32> {
    let l: Vec<f32> = px.iter().map(|p| luma(*p)).collect();
    let at = |x: usize, y: usize| l[y.min(h - 1) * w + x.min(w - 1)];
    let mut g = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let (xm, xp) = (x.saturating_sub(1), x + 1);
            let (ym, yp) = (y.saturating_sub(1), y + 1);
            let gx = at(xp, ym) + 2.0 * at(xp, y) + at(xp, yp)
                - at(xm, ym)
                - 2.0 * at(xm, y)
                - at(xm, yp);
            let gy = at(xm, yp) + 2.0 * at(x, yp) + at(xp, yp)
                - at(xm, ym)
                - 2.0 * at(x, ym)
                - at(xp, ym);
            g[y * w + x] = gx.hypot(gy) / 4.0;
        }
    }
    g
}

/// Where the sky ends in every column for an edge threshold.
fn border(grad: &[f32], w: usize, h: usize, threshold: f32) -> Vec<usize> {
    (0..w)
        .map(|x| (0..h).find(|y| grad[y * w + x] > threshold).unwrap_or(h))
        .collect()
}

/// Mean and covariance of a set of colours.
fn stats(colours: impl Iterator<Item = [f32; 3]>) -> Option<([f32; 3], [[f32; 3]; 3], usize)> {
    let mut n = 0_usize;
    let mut sum = [0.0_f64; 3];
    let mut sq = [[0.0_f64; 3]; 3];
    for c in colours {
        n += 1;
        for i in 0..3 {
            sum[i] += f64::from(c[i]);
            for j in 0..3 {
                sq[i][j] += f64::from(c[i]) * f64::from(c[j]);
            }
        }
    }
    if n < 16 {
        return None;
    }
    let mean = sum.map(|s| s / n as f64);
    let mut cov = [[0.0_f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            cov[i][j] = (sq[i][j] / n as f64 - mean[i] * mean[j]) as f32;
        }
    }
    Some((mean.map(|m| m as f32), cov, n))
}

fn det(m: &[[f32; 3]; 3]) -> f32 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// The largest eigenvalue of a symmetric 3 x 3 matrix, by power iteration.
fn largest_eigen(m: &[[f32; 3]; 3]) -> f32 {
    let mut v = [1.0_f32, 1.0, 1.0];
    let mut lambda = 0.0;
    for _ in 0..24 {
        let next = [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2]);
        let norm = (next[0] * next[0] + next[1] * next[1] + next[2] * next[2]).sqrt();
        if norm < 1e-12 {
            return 0.0;
        }
        lambda = norm;
        v = next.map(|c| c / norm);
    }
    lambda
}

/// How much more the sky's own colour spread counts than the ground's, after Shen and Wang.
const GAMMA: f32 = 2.0;

/// Shen and Wang's energy: high when sky and ground are each uniform in colour.
fn energy(px: &[[f32; 3]], w: usize, h: usize, b: &[usize]) -> Option<f32> {
    let sky = stats((0..w).flat_map(|x| (0..b[x]).map(move |y| px[y * w + x])))?;
    let ground = stats((0..w).flat_map(|x| (b[x]..h).map(move |y| px[y * w + x])))?;
    let denominator = GAMMA * det(&sky.1).abs()
        + det(&ground.1).abs()
        + GAMMA * largest_eigen(&sky.1)
        + largest_eigen(&ground.1);
    Some(1.0 / denominator.max(1e-12))
}

/// A median over a sliding window of columns, so a pole or a lamp does not notch the sky.
fn smooth_border(b: &[usize], radius: usize) -> Vec<usize> {
    (0..b.len())
        .map(|x| {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius + 1).min(b.len());
            let mut window: Vec<usize> = b[lo..hi].to_vec();
            window.sort_unstable();
            window[window.len() / 2]
        })
        .collect()
}

/// Mean over a (2r+1)^2 window, clipped at the borders.
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

/// Guided filter: `src` smoothed so its edges follow `guide`'s.
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

/// The median colour step across the border, over the columns where the sky ends above the
/// bottom: a horizon is a change of colour, not just a change of texture.
fn horizon_contrast(px: &[[f32; 3]], w: usize, h: usize, b: &[usize]) -> f32 {
    let reach = (h / 40).max(2);
    let mut steps: Vec<f32> = (0..w)
        .filter(|x| b[*x] > reach && b[*x] + reach < h)
        .map(|x| {
            let mean = |ys: std::ops::Range<usize>| {
                let n = ys.len().max(1) as f32;
                ys.fold([0.0_f32; 3], |acc, y| {
                    let p = px[y * w + x];
                    [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
                })
                .map(|v| v / n)
            };
            let above = mean(b[x] - reach..b[x]);
            let below = mean(b[x]..b[x] + reach);
            (0..3).map(|c| (above[c] - below[c]).abs()).sum::<f32>()
        })
        .collect();
    if steps.is_empty() {
        return 0.0;
    }
    let middle = steps.len() / 2;
    *steps.select_nth_unstable_by(middle, f32::total_cmp).1
}

/// Why a frame was found to have no sky, or how much it has.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// The sky's matte, over the whole frame.
    Sky(Matte),
    /// No sky, and the reason in a few words for the panel.
    None(&'static str),
}

/// Find the sky in a packed, oriented sRGB photograph.
#[must_use]
pub fn find(rgb: &[u8], width: u32, height: u32) -> Finding {
    let (w0, h0) = (width as usize, height as usize);
    if w0 < 16 || h0 < 16 || rgb.len() != w0 * h0 * 3 {
        return Finding::None("the photograph is too small");
    }
    let (px, w, h) = reduce(rgb, w0, h0, SEARCH_EDGE);
    let grad = gradient(&px, w, h);
    // Search the threshold that best separates two uniform regions.
    let mut best: Option<(f32, Vec<usize>)> = None;
    for k in 1..=40 {
        let threshold = 0.004 * k as f32;
        let b = smooth_border(&border(&grad, w, h, threshold), (w / 80).max(1));
        let Some(j) = energy(&px, w, h, &b) else {
            continue;
        };
        if best.as_ref().is_none_or(|(e, _)| j > *e) {
            best = Some((j, b));
        }
    }
    let Some((_, b)) = best else {
        return Finding::None("no horizon was found");
    };
    let sky_cells: usize = b.iter().sum();
    let share = sky_cells as f32 / (w * h) as f32;
    if share < 0.02 {
        return Finding::None("no open sky at the top of the frame");
    }
    // A backdrop or a wall reaches the bottom of the frame; a sky has ground under it.
    let to_bottom = b.iter().filter(|y| **y >= h * 95 / 100).count() as f32 / w as f32;
    if to_bottom > 0.25 || share > 0.85 {
        return Finding::None("nothing under it reads as ground: a backdrop or a wall, not sky");
    }
    let mean = |inside: bool, f: &dyn Fn(usize, usize) -> f32| {
        let (mut sum, mut n) = (0.0_f32, 0_usize);
        for x in 0..w {
            for y in 0..h {
                if (y < b[x]) == inside {
                    sum += f(x, y);
                    n += 1;
                }
            }
        }
        sum / n.max(1) as f32
    };
    let bright = |x: usize, y: usize| luma(px[y * w + x]);
    let rough = |x: usize, y: usize| grad[y * w + x];
    let (sky_light, ground_light) = (mean(true, &bright), mean(false, &bright));
    let (sky_rough, ground_rough) = (mean(true, &rough), mean(false, &rough));
    let blue = {
        let c = (0..w).flat_map(|x| (0..b[x]).map(move |y| (x, y))).fold(
            [0.0_f32; 3],
            |acc, (x, y)| {
                let p = px[y * w + x];
                [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
            },
        );
        c[2] > c[0] * 1.05
    };
    // A horizon is a change of colour as well as of texture. Without one, the "border" is the
    // noise of a plain backdrop or a veil's edge.
    if horizon_contrast(&px, w, h, &b) < 0.06 {
        return Finding::None("no horizon: the top of the frame runs on into what is below it");
    }
    if sky_light < 0.25 || (sky_light < ground_light * 1.05 && !blue) {
        return Finding::None(
            "the top of the frame is darker than the ground: a ceiling or a night",
        );
    }
    if sky_rough > ground_rough * 0.8 {
        return Finding::None("the top of the frame is as detailed as the rest: not open sky");
    }
    // A soft step at the border, then the photograph's own edge.
    let hard: Vec<f32> = (0..w * h)
        .map(|i| if i / w < b[i % w] { 1.0 } else { 0.0 })
        .collect();
    let guide: Vec<f32> = px.iter().map(|p| luma(*p)).collect();
    let soft = guided(&guide, &hard, w, h, (w.max(h) / 96).max(2), 1e-3);
    // Stored at most MATTE_EDGE on its long side.
    let (mw, mh) = if w >= h {
        (
            MATTE_EDGE.min(w),
            (MATTE_EDGE.min(w) * h).div_ceil(w).max(1),
        )
    } else {
        (
            (MATTE_EDGE.min(h) * w).div_ceil(h).max(1),
            MATTE_EDGE.min(h),
        )
    };
    let mut alpha = Vec::with_capacity(mw * mh);
    for y in 0..mh {
        for x in 0..mw {
            let sx = ((x as f32 + 0.5) * w as f32 / mw as f32) as usize;
            let sy = ((y as f32 + 0.5) * h as f32 / mh as f32) as usize;
            alpha.push((soft[sy.min(h - 1) * w + sx.min(w - 1)] * 255.0).round() as u8);
        }
    }
    Finding::Sky(Matte {
        bounds: [0.0, 0.0, 1.0, 1.0],
        width: mw,
        height: mh,
        alpha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blue sky over textured green ground, the horizon at `horizon` of the height.
    fn landscape(w: usize, h: usize, horizon: f32) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                if (y as f32) < horizon * h as f32 {
                    let t = y as f32 / h as f32;
                    rgb.extend([(110.0 + 60.0 * t) as u8, (160.0 + 40.0 * t) as u8, 235]);
                } else {
                    let n = ((x * 7919 + y * 104_729) % 61) as u8;
                    rgb.extend([40 + n, 90 + n, 30 + n / 2]);
                }
            }
        }
        rgb
    }

    #[test]
    fn a_landscape_has_its_sky_above_the_horizon() {
        let (w, h) = (300, 200);
        let Finding::Sky(m) = find(&landscape(w, h, 0.4), w as u32, h as u32) else {
            panic!("no sky found");
        };
        assert!(m.at(0.5, 0.1) > 0.9, "{}", m.at(0.5, 0.1));
        assert!(m.at(0.5, 0.8) < 0.1, "{}", m.at(0.5, 0.8));
    }

    #[test]
    fn a_studio_backdrop_and_a_dark_ceiling_are_not_sky() {
        let (w, h) = (300, 200);
        // A plain pink backdrop with a person-shaped block in the middle.
        let mut studio = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                if (120..180).contains(&x) && y > 40 {
                    studio.extend([180, 140, 120]);
                } else {
                    studio.extend([240, 170, 180]);
                }
            }
        }
        assert!(matches!(
            find(&studio, w as u32, h as u32),
            Finding::None(_)
        ));
        // A dark, smooth ceiling over a bright, busy room.
        let mut room = landscape(w, h, 0.3);
        for p in room.chunks_exact_mut(3).take(w * h * 3 / 10) {
            p.copy_from_slice(&[30, 28, 26]);
        }
        for (i, p) in room.chunks_exact_mut(3).enumerate().skip(w * h * 3 / 10) {
            let n = (i % 13) as u8 * 7;
            p.copy_from_slice(&[150 + n, 150 + n, 150 + n]);
        }
        assert!(matches!(find(&room, w as u32, h as u32), Finding::None(_)));
    }
}
