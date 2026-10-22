use super::{avg, bars, map, Diagnostic};

fn gradient(luma: &[f32], w: usize, h: usize, x: usize, y: usize) -> f32 {
    let dx = luma[y * w + (x + 1).min(w - 1)] - luma[y * w + x.saturating_sub(1)];
    let dy = luma[(y + 1).min(h - 1) * w + x] - luma[y.saturating_sub(1) * w + x];
    dx.hypot(dy) * 0.5
}

fn autocorrelation(luma: &[f32], w: usize, h: usize) -> Vec<f32> {
    (1..=8)
        .map(|lag| {
            let count = w.saturating_sub(lag) * h;
            if count == 0 {
                return 0.0;
            }
            // Each lag has different valid supports. Centre each side on its own
            // paired samples so even a cropped affine ramp has correlation one.
            let (mut left, mut right) = (0.0_f64, 0.0_f64);
            for y in 0..h {
                for x in 0..w.saturating_sub(lag) {
                    left += f64::from(luma[y * w + x]);
                    right += f64::from(luma[y * w + x + lag]);
                }
            }
            let (left, right) = (left / count as f64, right / count as f64);
            let (mut cross, mut aa, mut bb) = (0.0_f64, 0.0_f64, 0.0_f64);
            for y in 0..h {
                for x in 0..w.saturating_sub(lag) {
                    let a = f64::from(luma[y * w + x]) - left;
                    let b = f64::from(luma[y * w + x + lag]) - right;
                    cross += a * b;
                    aa += a * a;
                    bb += b * b;
                }
            }
            if aa * bb <= 1e-24 {
                0.0
            } else {
                (cross / (aa * bb).sqrt()).clamp(-1.0, 1.0) as f32
            }
        })
        .collect()
}

fn haar(luma: &[f32], w: usize, h: usize) -> Vec<f32> {
    [1, 2, 4]
        .into_iter()
        .map(|r| {
            let mut energy = 0.0_f64;
            let mut count = 0;
            for block_y in 0..h / (2 * r) {
                for block_x in 0..w / (2 * r) {
                    let x = block_x * 2 * r;
                    let y = block_y * 2 * r;
                    let p = [(0, 0), (r, 0), (0, r), (r, r)].map(|(dx, dy)| {
                        let mut sum = 0.0_f64;
                        for row in 0..r {
                            for col in 0..r {
                                sum += f64::from(luma[(y + dy + row) * w + x + dx + col]);
                            }
                        }
                        (sum / (r * r) as f64) as f32
                    });
                    let d = [
                        p[0] + p[2] - p[1] - p[3],
                        p[0] + p[1] - p[2] - p[3],
                        p[0] + p[3] - p[1] - p[2],
                    ];
                    energy += d.iter().map(|v| f64::from(*v).powi(2) * 0.25).sum::<f64>();
                    count += 1;
                }
            }
            avg(energy, count)
        })
        .collect()
}

pub(super) fn measure(luma: &[f32], w: usize, h: usize) -> Vec<Diagnostic> {
    let n = luma.len();
    let mut lap = vec![0.0; n];
    let mut residual = vec![0.0; n];
    let mut edges = vec![0.0; n];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let neighbors = luma[y * w + x.saturating_sub(1)]
                + luma[y * w + (x + 1).min(w - 1)]
                + luma[y.saturating_sub(1) * w + x]
                + luma[(y + 1).min(h - 1) * w + x];
            lap[i] = neighbors - 4.0 * luma[i];
            residual[i] = luma[i] - neighbors * 0.25;
            edges[i] = gradient(luma, w, h, x, y);
        }
    }
    let mut crossings = vec![0.0; 64];
    let mut overshoot = vec![0.0; 64];
    let mut count = [0; 64];
    let mut widths = vec![0.0; 9];
    let mut width_n = 0;
    let mut parity = [0.0_f64; 4];
    let mut parity_n = [0; 4];
    let mut boundary = [0.0_f64; 4];
    let mut boundary_n = [0; 4];
    let mut spatial = [0.0_f64; 6];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let cell = (y * 8 / h) * 8 + x * 8 / w;
            count[cell] += 1;
            let opposite = |j: usize| lap[i] * lap[j] < 0.0 && (lap[i] - lap[j]).abs() > 0.005;
            if (x + 1 < w && opposite(i + 1)) || (y + 1 < h && opposite(i + w)) {
                crossings[cell] += 100.0;
            }
            if x >= 2 && x + 2 < w {
                let a = luma[i - 2];
                let b = luma[i + 2];
                if (b - a).abs() > 0.02 {
                    overshoot[cell] += (luma[i] - a.max(b)).max(a.min(b) - luma[i]).max(0.0);
                }
            }
            if x >= 4 && x + 4 < w && edges[i] > 0.01 {
                let mut sum = 0.0;
                let mut square = 0.0;
                for offset in 0..8 {
                    let d = (luma[i + offset - 3] - luma[i + offset - 4]).abs();
                    sum += d;
                    square += d * d;
                }
                if square > 1e-12 {
                    let width = (sum * sum / square).round().clamp(1.0, 9.0) as usize;
                    widths[width - 1] += 1.0;
                    width_n += 1;
                }
            }
            if x > 0 && y > 0 {
                let diagonal = (luma[i] - luma[i - w - 1]).abs();
                if diagonal > 0.02 {
                    let p = (y % 2) * 2 + x % 2;
                    parity[p] +=
                        f64::from((luma[i] + luma[i - w - 1] - luma[i - 1] - luma[i - w]).abs());
                    parity_n[p] += 1;
                }
            }
            for (side, applies) in [x == 0, x + 1 == w, y == 0, y + 1 == h]
                .into_iter()
                .enumerate()
            {
                if applies {
                    boundary[side] += f64::from(edges[i]);
                    boundary_n[side] += 1;
                }
            }
            let weight = f64::from(edges[i]);
            let px = (x as f64 + 0.5) / w as f64;
            let py = (y as f64 + 0.5) / h as f64;
            for (sum, v) in spatial
                .iter_mut()
                .zip([1.0, px, py, px * px, px * py, py * py])
            {
                *sum += weight * v;
            }
        }
    }
    let cross: Vec<_> = [(1, 0), (0, 1), (1, 1), (1, 2)]
        .into_iter()
        .map(|(dx, dy)| {
            let (mut ab, mut aa, mut bb) = (0.0_f64, 0.0_f64, 0.0_f64);
            let mut count = 0;
            let mut ma = 0.0_f64;
            let mut mb = 0.0_f64;
            for y in 0..h.saturating_sub(dy) {
                for x in 0..w.saturating_sub(dx) {
                    ma += f64::from(residual[y * w + x]);
                    mb += f64::from(residual[(y + dy) * w + x + dx]);
                    count += 1;
                }
            }
            if count == 0 {
                return 0.0;
            }
            ma /= f64::from(count);
            mb /= f64::from(count);
            for y in 0..h.saturating_sub(dy) {
                for x in 0..w.saturating_sub(dx) {
                    let a = f64::from(residual[y * w + x]) - ma;
                    let b = f64::from(residual[(y + dy) * w + x + dx]) - mb;
                    ab += a * b;
                    aa += a * a;
                    bb += b * b;
                }
            }
            if aa * bb <= 1e-24 {
                0.0
            } else {
                (ab / (aa * bb).sqrt()).clamp(-1.0, 1.0) as f32
            }
        })
        .collect();
    let covariance = if spatial[0] > 1e-12 {
        let s = spatial.map(|v| v / spatial[0]);
        vec![
            (s[3] - s[1] * s[1]).max(0.0) as f32,
            (s[4] - s[1] * s[2]) as f32,
            (s[5] - s[2] * s[2]).max(0.0) as f32,
        ]
    } else {
        vec![0.0; 3]
    };
    for i in 0..64 {
        if count[i] > 0 {
            crossings[i] /= count[i] as f32;
            overshoot[i] /= count[i] as f32;
        }
    }
    vec![
        map("laplacian-crossings", "Curvature sign transitions", "Regional fraction with a significant Laplacian sign transition to a right or lower neighbor. Texture and ringing can both contribute.", "%", crossings, n),
        bars("edge-effective-width", "Horizontal edge support", "Effective support (sum of gradient magnitudes squared divided by their squared sum) across eight horizontal differences near edges. Preview scale only.", "samples", widths, &[], width_n),
        map("edge-overshoot", "Edge overshoot residual", "Luminance beyond the endpoint interval of a four-pixel horizontal edge span; a ringing review aid, not a defect verdict.", "signal", overshoot, n),
        bars("luma-autocorrelation", "Horizontal signal autocorrelation", "Centered luminance correlation at one through eight preview-pixel lags. Unsupported or constant pairs report zero.", "ratio", autocorrelation(luma,w,h), &["1 px","2 px","3 px","4 px","5 px","6 px","7 px","8 px"], w.saturating_sub(1)*h),
        bars("diagonal-phase", "Diagonal parity residual", "Mixed diagonal second differences grouped by row/column parity on diagonal transitions. Uneven phases can motivate demosaic artifact review.", "signal", (0..4).map(|i|avg(parity[i],parity_n[i])).collect(), &["Even/even","Even/odd","Odd/even","Odd/odd"], parity_n.iter().sum()),
        bars("residual-correlation", "Directional residual correlation", "Centered correlation of four-neighbor residuals at horizontal, vertical, diagonal and (1,2) lags. Unsupported or constant pairs report zero; scene texture also contributes.", "ratio", cross, &["Horizontal","Vertical","Diagonal","Steep diagonal"], if w>1 || h>1 {n} else {0}),
        bars("haar-energy", "Decimated Haar energy", "Mean sum of squared horizontal, vertical and diagonal Haar coefficients after quadrant averaging at three dyadic supports; incomplete border blocks are omitted.", "signal²", haar(luma,w,h), &["2 px","4 px","8 px"], (w/2)*(h/2)),
        bars("edge-spread", "Edge position covariance", "Gradient-weighted covariance of normalized edge locations. Describes spatial spread and diagonal arrangement without identifying a subject.", "frame²", covariance, &["Horizontal spread","Cross covariance","Vertical spread"], edges.iter().filter(|v| **v>0.0).count()),
        bars("border-edge-arrivals", "Boundary edge activity", "Mean gradient magnitude on each outer image boundary. Large values can flag cropped-through detail for human inspection.", "signal", (0..4).map(|i|avg(boundary[i],boundary_n[i])).collect(), &["Left","Right","Top","Bottom"], boundary_n.iter().sum()),
    ]
}
