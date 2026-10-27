use super::{avg, bars, entropy, map, Diagnostic};

fn dct(y: &[f32], w: usize, h: usize) -> (Vec<f32>, usize) {
    let basis: Vec<Vec<f64>> = (0_usize..8)
        .map(|k| {
            (0_usize..8)
                .map(|x| {
                    let norm = if k == 0 { 1.0 / 8.0_f64.sqrt() } else { 0.5 };
                    norm * (std::f64::consts::PI * (2 * x + 1) as f64 * k as f64 / 16.0).cos()
                })
                .collect()
        })
        .collect();
    let mut energy = vec![0.0_f64; 8];
    let mut blocks = 0;
    for by in 0..h / 8 {
        for bx in 0..w / 8 {
            let mut horizontal = [[0.0_f64; 8]; 8];
            for row in 0..8 {
                for k in 0..8 {
                    horizontal[row][k] = (0..8)
                        .map(|x| f64::from(y[(by * 8 + row) * w + bx * 8 + x]) * basis[k][x])
                        .sum();
                }
            }
            for v in 0..8 {
                for u in 0..8 {
                    if u == 0 && v == 0 {
                        continue;
                    }
                    let value: f64 = (0..8).map(|row| horizontal[row][u] * basis[v][row]).sum();
                    let band = (((u * u + v * v) as f64).sqrt().floor() as usize).min(7);
                    energy[band] += value * value;
                }
            }
            blocks += 1;
        }
    }
    (energy.into_iter().map(|v| avg(v, blocks)).collect(), blocks)
}

pub(super) fn measure(p: &[[f32; 3]], light: &[f32], w: usize, h: usize) -> Vec<Diagnostic> {
    let n = light.len();
    // Bounded asinh domain retains sign and HDR while keeping derivative products stable.
    let y: Vec<_> = light.iter().map(|v| v.asinh()).collect();
    let mut gradient = vec![0.0; n];
    let mut residual = vec![0.0; n];
    let mut tensor = [[0.0_f64; 3]; 64];
    let mut count = [0_usize; 64];
    let mut hessian = [0_usize; 4];
    let mut polarity = [0_usize; 4];
    let mut align = 0.0_f64;
    let mut align_n = 0;
    let mut variation = [0.0_f64; 2];
    let mut pair_n = [0; 2];
    let mut hist = [[0_usize; 16]; 64];
    let mut projected = [0.0_f64; 5];
    let mut squared = [0.0_f64; 5];
    let mx2 = (0..w)
        .map(|x| ((x as f64 + 0.5) / w as f64 - 0.5).powi(2))
        .sum::<f64>()
        / w as f64;
    let my2 = (0..h)
        .map(|y| ((y as f64 + 0.5) / h as f64 - 0.5).powi(2))
        .sum::<f64>()
        / h as f64;
    let mut interior = 0;
    for row in 0..h {
        for x in 0..w {
            let i = row * w + x;
            let cell = row * 8 / h * 8 + x * 8 / w;
            count[cell] += 1;
            // Fixed bounded preview luminance bins. HDR accumulates at the top bin.
            hist[cell][(light[i].clamp(0.0, 1.0) * 15.0).floor() as usize] += 1;
            let nx = (x as f64 + 0.5) / w as f64 - 0.5;
            let ny = (row as f64 + 0.5) / h as f64 - 0.5;
            let basis = [nx, ny, nx * nx - mx2, nx * ny, ny * ny - my2];
            for k in 0..5 {
                projected[k] += basis[k] * f64::from(y[i]);
                squared[k] += basis[k] * basis[k];
            }
            if x + 1 < w {
                variation[0] += f64::from((y[i + 1] - y[i]).abs());
                pair_n[0] += 1;
            }
            if row + 1 < h {
                variation[1] += f64::from((y[i + w] - y[i]).abs());
                pair_n[1] += 1;
            }
            if x == 0 || row == 0 || x + 1 == w || row + 1 == h {
                continue;
            }
            interior += 1;
            let dx = (y[i + 1] - y[i - 1]) * 0.5;
            let dy = (y[i + w] - y[i - w]) * 0.5;
            gradient[i] = dx.hypot(dy);
            tensor[cell][0] += f64::from(dx * dx);
            tensor[cell][1] += f64::from(dx * dy);
            tensor[cell][2] += f64::from(dy * dy);
            let xx = y[i + 1] + y[i - 1] - 2.0 * y[i];
            let yy = y[i + w] + y[i - w] - 2.0 * y[i];
            let xy = (y[i + w + 1] - y[i + w - 1] - y[i - w + 1] + y[i - w - 1]) * 0.25;
            let det = xx * yy - xy * xy;
            let index = if xx.abs() + yy.abs() + xy.abs() < 1e-6 {
                3
            } else if det < 0.0 {
                2
            } else {
                usize::from(xx + yy < 0.0)
            };
            hessian[index] += 1;
            for (k, value) in [dx, dy].into_iter().enumerate() {
                if value > 1e-6 {
                    polarity[k * 2] += 1;
                } else if value < -1e-6 {
                    polarity[k * 2 + 1] += 1;
                }
            }
            residual[i] = y[i] - (y[i - 1] + y[i + 1] + y[i - w] + y[i + w]) * 0.25;
            let cx = ((p[i + 1][0] - p[i + 1][1]) - (p[i - 1][0] - p[i - 1][1])).asinh() * 0.5;
            let cy = ((p[i + w][0] - p[i + w][1]) - (p[i - w][0] - p[i - w][1])).asinh() * 0.5;
            let norm = gradient[i] * cx.hypot(cy);
            if norm > 1e-10 {
                align += f64::from((dx * cx + dy * cy).abs() / norm);
                align_n += 1;
            }
        }
    }
    let coherence = tensor
        .iter()
        .map(|t| {
            if t[0] + t[2] > 1e-20 {
                ((t[0] - t[2]).hypot(2.0 * t[1]) / (t[0] + t[2])).clamp(0.0, 1.0) as f32
            } else {
                0.0
            }
        })
        .collect();
    let kurtosis = {
        let mean = gradient.iter().map(|v| f64::from(*v)).sum::<f64>() / n as f64;
        let second = gradient
            .iter()
            .map(|v| (f64::from(*v) - mean).powi(2))
            .sum::<f64>()
            / n as f64;
        if second > 1e-20 {
            (gradient
                .iter()
                .map(|v| (f64::from(*v) - mean).powi(4))
                .sum::<f64>()
                / n as f64
                / second.powi(2)
                - 3.0) as f32
        } else {
            0.0
        }
    };
    let mut residual_sign = [0.0_f64; 2];
    for value in &residual {
        residual_sign[usize::from(*value < 0.0)] += f64::from(value.abs());
    }
    let residual_balance = if residual_sign[0] + residual_sign[1] > 1e-20 {
        ((residual_sign[0] - residual_sign[1]) / (residual_sign[0] + residual_sign[1])) as f32
    } else {
        0.0
    };
    let mean_light = light
        .iter()
        .map(|v| f64::from(v.abs().asinh()))
        .sum::<f64>()
        / n as f64;
    let total_variation = variation
        .iter()
        .zip(pair_n)
        .map(|(v, c)| {
            if mean_light > 1e-12 {
                avg(*v, c) / mean_light as f32
            } else {
                0.0
            }
        })
        .collect();
    let coefficients: Vec<_> = projected
        .iter()
        .zip(squared)
        .map(|(v, s)| if s > 1e-20 { (v / s) as f32 } else { 0.0 })
        .collect();
    let threshold = gradient.iter().sum::<f32>() / n as f32;
    let edge: Vec<_> = gradient
        .iter()
        .map(|v| *v > threshold && *v > 1e-6)
        .collect();
    let edge_n = edge.iter().filter(|v| **v).count();
    let (spectrum, blocks) = dct(&y, w, h);
    let mut vertices = 0;
    let mut edges = 0;
    let occupied = |x: usize, row: usize| x < w && row < h && edge[row * w + x];
    for row in 0..=h {
        for x in 0..=w {
            if occupied(x, row)
                || (x > 0 && occupied(x - 1, row))
                || (row > 0 && occupied(x, row - 1))
                || (x > 0 && row > 0 && occupied(x - 1, row - 1))
            {
                vertices += 1;
            }
            if x < w && (occupied(x, row) || (row > 0 && occupied(x, row - 1))) {
                edges += 1;
            }
            if row < h && (occupied(x, row) || (x > 0 && occupied(x - 1, row))) {
                edges += 1;
            }
        }
    }
    let euler = vertices as f32 - edges as f32 + edge_n as f32;
    let boxes: Vec<_> = [1, 2, 4, 8]
        .into_iter()
        .map(|size| {
            let mut occupied_boxes = 0;
            for by in (0..h).step_by(size) {
                for bx in (0..w).step_by(size) {
                    if (by..(by + size).min(h))
                        .any(|row| (bx..(bx + size).min(w)).any(|x| edge[row * w + x]))
                    {
                        occupied_boxes += 1;
                    }
                }
            }
            occupied_boxes
        })
        .collect();
    let dimensions = boxes
        .windows(2)
        .map(|b| {
            if b[0] > 0 && b[1] > 0 {
                ((b[0] as f32 / b[1] as f32).log2()).clamp(0.0, 2.0)
            } else {
                0.0
            }
        })
        .collect();
    vec![
        map(("structure-coherence", "Regional structure coherence", "Anisotropy of the 2 by 2 luminance gradient structure tensor. Zero includes flat regions; one indicates a dominant direction."), "ratio", coherence, interior),
        bars(("hessian-signature", "Surface curvature classes", "Fractions of positive curvature, negative curvature, saddle and flat Hessians on interior preview pixels. This is mathematical structure, not skin classification."), "%", hessian.map(|v|if interior>0 {100.0*v as f32/interior as f32}else{0.0}).to_vec(), &["Bowl","Dome","Saddle","Flat"], interior),
        bars(("gradient-tailedness", "Gradient tail weight", "Excess kurtosis of bounded-domain gradient magnitude. Sparse strong boundaries produce heavier tails."), "ratio", vec![kurtosis], &["Excess kurtosis"], interior),
        bars(("edge-polarity", "Signed edge transitions", "Counts of positive and negative horizontal and vertical interior derivatives above a small numerical floor."), "samples", polarity.map(|v|v as f32).to_vec(), &["Right rising","Right falling","Down rising","Down falling"], interior),
        bars(("normalized-total-variation", "Relative total variation", "Mean absolute adjacent asinh-luminance change divided by mean absolute asinh luminance, independently across axes."), "ratio", total_variation, &["Horizontal","Vertical"], pair_n.iter().sum()),
        bars(("dct-energy", "Cosine frequency energy", "Orthonormal 8 by 8 block DCT energy in radial AC frequency bands of asinh luminance. Incomplete border blocks are excluded."), "signal squared / block", spectrum, &[], blocks),
        bars(("chroma-edge-alignment", "Chroma and light edge alignment", "Mean absolute angular cosine between red-green opponent gradients and luminance gradients with nonzero support."), "ratio", vec![avg(align,align_n)], &["Alignment"], align_n),
        bars(("residual-asymmetry", "Fine residual polarity", "Positive and negative four-neighbor residual mass and their normalized signed balance in asinh luminance."), "mixed: signal / ratio", vec![avg(residual_sign[0],interior),avg(residual_sign[1],interior),residual_balance], &["Positive mass","Negative mass","Balance"], interior),
        map(("local-bit-entropy", "Regional tone entropy", "Shannon entropy in fixed 16-bin nominal luminance histograms per region. HDR and negative samples accumulate in boundary bins."), "bits", hist.iter().map(|v|entropy(v)).collect(), n),
        bars(("illumination-plane", "Linear lighting trend", "Least-squares horizontal and vertical slopes in centred normalized coordinates of asinh luminance. Scene reflectance contributes."), "signal / frame", coefficients[..2].to_vec(), &["Horizontal","Vertical"], n),
        bars(("illumination-curvature", "Quadratic lighting trend", "Orthogonal centred quadratic and cross-coordinate coefficients of asinh luminance; measures broad shape without claiming lens calibration."), "signal / frame squared", coefficients[2..].to_vec(), &["Horizontal curve","Cross curve","Vertical curve"], n),
        bars(("edge-euler", "Edge topology", "Cubical Euler characteristic V minus E plus F of above-mean significant gradient pixels. Closed pixel squares include corner contacts."), "characteristic", vec![euler], &["Euler"], edge_n),
        bars(("edge-box-dimension", "Edge occupancy scaling", "Finite-scale box-count slopes from significant edges at one, two, four and eight preview pixels. Not an asymptotic fractal claim."), "dimension", dimensions, &["1 to 2 px","2 to 4 px","4 to 8 px"], edge_n),
    ]
}
