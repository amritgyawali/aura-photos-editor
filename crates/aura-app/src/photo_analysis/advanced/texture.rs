use super::{bars, bins, cell, entropy, map, mean, neighbours, quantile, Diagnostic};

fn permutation(y: &[f64], w: usize, h: usize) -> (Vec<f64>, usize) {
    let mut hist = [0_usize; 6];
    let mut ties = 0;
    for row in 0..h {
        for x in 0..w.saturating_sub(2) {
            let a = &y[row * w + x..row * w + x + 3];
            if a[0].total_cmp(&a[1]).is_eq()
                || a[1].total_cmp(&a[2]).is_eq()
                || a[0].total_cmp(&a[2]).is_eq()
            {
                ties += 1;
                continue;
            }
            let mut order = [0, 1, 2];
            order.sort_by(|aidx, bidx| a[*aidx].total_cmp(&a[*bidx]));
            let k = match order {
                [0, 1, 2] => 0,
                [0, 2, 1] => 1,
                [1, 0, 2] => 2,
                [1, 2, 0] => 3,
                [2, 0, 1] => 4,
                _ => 5,
            };
            hist[k] += 1;
        }
    }
    let n = hist.iter().sum::<usize>();
    (
        vec![
            entropy(&hist) / 6.0_f64.ln(),
            ties as f64 / (n + ties).max(1) as f64,
        ],
        n + ties,
    )
}
fn sample_entropy(y: &[f64]) -> (Vec<f64>, usize) {
    // A fixed evenly spaced sequence bounds O(N²) work to 384 samples.
    let size = y.len().min(384);
    let v: Vec<_> = (0..size).map(|i| y[i * y.len() / size]).collect();
    let mu = mean(&v);
    let sd = (v.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / size.max(1) as f64).sqrt();
    let tolerance = (0.2 * sd).max(1e-10);
    let mut matches = [0_usize; 2];
    let mut pairs = 0;
    for i in 0..size.saturating_sub(2) {
        for j in i + 1..size.saturating_sub(2) {
            pairs += 1;
            if (v[i] - v[j]).abs() <= tolerance && (v[i + 1] - v[j + 1]).abs() <= tolerance {
                matches[0] += 1;
                if (v[i + 2] - v[j + 2]).abs() <= tolerance {
                    matches[1] += 1;
                }
            }
        }
    }
    // Half-count smoothing makes unsupported rare-match estimates finite and explicit.
    (
        vec![
            -((matches[1] as f64 + 0.5) / (matches[0] as f64 + 0.5)).ln(),
            matches[0] as f64 / pairs.max(1) as f64,
        ],
        pairs,
    )
}
fn spectrum(y: &[f64], w: usize, h: usize) -> (Vec<f64>, usize) {
    let mut values = Vec::new();
    for vertical in [false, true] {
        let length = if vertical { h } else { w };
        let cross = if vertical { w } else { h };
        let size = length.min(64);
        let lines = cross.min(8);
        let mut flatness = 0.0;
        let mut count = 0;
        for line in 0..lines {
            let coordinate = line * cross / lines;
            let v: Vec<_> = (0..size)
                .map(|j| {
                    let along = j * length / size;
                    y[if vertical {
                        along * w + coordinate
                    } else {
                        coordinate * w + along
                    }]
                })
                .collect();
            let mu = mean(&v);
            let mut energy = Vec::new();
            for k in 1..=size / 2 {
                let (mut re, mut im) = (0.0, 0.0);
                for (j, sample) in v.iter().enumerate() {
                    let theta = std::f64::consts::TAU * k as f64 * j as f64 / size as f64;
                    re += (sample - mu) * theta.cos();
                    im += (sample - mu) * theta.sin();
                }
                energy.push(re * re + im * im);
            }
            let arithmetic = mean(&energy);
            if arithmetic > 1e-18 {
                flatness += ((energy.iter().map(|e| (e + 1e-24).ln()).sum::<f64>()
                    / energy.len() as f64)
                    .exp()
                    / arithmetic)
                    .clamp(0.0, 1.0);
            }
            count += 1;
        }
        values.push(flatness / f64::from(count.max(1)));
    }
    (values, w.min(8) * h.min(64) + h.min(8) * w.min(64))
}

pub(super) fn measure(y: &[f64], w: usize, h: usize) -> Vec<Diagnostic> {
    let n = y.len();
    let q = bins(y, 16);
    let (permutation, pn) = permutation(y, w, h);
    let (sample, sn) = sample_entropy(y);
    let mut runs = [0_usize; 8];
    let mut glcm = [0_usize; 256];
    let mut pairs = 0;
    for row in 0..h {
        let mut run = 1;
        for x in 1..w {
            let i = row * w + x;
            if q[i] == q[i - 1] {
                run += 1;
            } else {
                runs[(run - 1).min(7)] += 1;
                run = 1;
            }
        }
        runs[(run - 1).min(7)] += 1;
    }
    for i in 0..n {
        for j in neighbours(i, w, h) {
            glcm[q[i] * 16 + q[j]] += 1;
            pairs += 1;
        }
    }
    let mut co = [0.0; 4];
    for (i, count) in glcm.iter().enumerate() {
        let p = *count as f64 / pairs.max(1) as f64;
        let delta = (i / 16) as f64 - (i % 16) as f64;
        co[0] += p * delta * delta;
        co[1] += p / (1.0 + delta * delta);
        co[2] += p * p;
        if p > 0.0 {
            co[3] -= p * p.ln();
        }
    }
    let threshold = quantile(y, 0.5);
    let mut lacunarity = Vec::new();
    for side in [2, 4, 8, 16] {
        let mut mass = Vec::new();
        for row in (0..h).step_by(side) {
            for x in (0..w).step_by(side) {
                if row + side > h || x + side > w {
                    continue;
                }
                let mut total = 0;
                for dy in 0..side {
                    for dx in 0..side {
                        total += usize::from(y[(row + dy) * w + x + dx] > threshold);
                    }
                }
                mass.push(total as f64);
            }
        }
        let mu = mean(&mass);
        lacunarity.push(if mu > 0.0 {
            mass.iter().map(|v| v * v).sum::<f64>() / mass.len() as f64 / (mu * mu)
        } else {
            0.0
        });
    }
    let mut tensor = [[0.0_f64; 3]; 64];
    let mut moments = [[0.0_f64; 8]; 64];
    let mut counts = [0_usize; 64];
    let mut rough = vec![Vec::new(); 64];
    let mu = mean(y);
    let denominator = y.iter().map(|v| (v - mu).powi(2)).sum::<f64>();
    let mut moran = 0.0;
    for i in 0..n {
        let x = i % w;
        let row = i / w;
        let c = cell(i, w, h);
        counts[c] += 1;
        let dx = (y[row * w + (x + 1).min(w - 1)] - y[row * w + x.saturating_sub(1)]) * 0.5;
        let dy = (y[(row + 1).min(h - 1) * w + x] - y[row.saturating_sub(1) * w + x]) * 0.5;
        tensor[c][0] += dx * dx;
        tensor[c][1] += dx * dy;
        tensor[c][2] += dy * dy;
        // Centre coordinates within the actual grid cell, including incomplete cells.
        let px = (x as f64 + 0.5) / w as f64;
        let py = (row as f64 + 0.5) / h as f64;
        for (slot, value) in moments[c].iter_mut().zip([
            px,
            py,
            px * px,
            py * py,
            px * py,
            y[i],
            px * y[i],
            py * y[i],
        ]) {
            *slot += value;
        }
        for j in neighbours(i, w, h) {
            rough[c].push((y[i] - y[j]).abs());
            moran += (y[i] - mu) * (y[j] - mu);
        }
    }
    let mut corner = vec![0.0; 64];
    let mut plane = vec![0.0; 64];
    let mut coefficients = [[0.0; 3]; 64];
    for c in 0..64 {
        if counts[c] == 0 {
            continue;
        }
        let k = counts[c] as f64;
        let [xx, xy, yy] = tensor[c].map(|v| v / k);
        corner[c] = xx * yy - xy * xy - 0.04 * (xx + yy).powi(2);
        let m = moments[c].map(|v| v / k);
        let vx = (m[2] - m[0] * m[0]).max(0.0);
        let vy = (m[3] - m[1] * m[1]).max(0.0);
        let cov = m[4] - m[0] * m[1];
        let det = vx * vy - cov * cov;
        let cx = m[6] - m[0] * m[5];
        let cy = m[7] - m[1] * m[5];
        let (ax, ay) = if det > 1e-20 {
            ((cx * vy - cy * cov) / det, (cy * vx - cx * cov) / det)
        } else {
            (
                if vx > 1e-20 { cx / vx } else { 0.0 },
                if vy > 1e-20 { cy / vy } else { 0.0 },
            )
        };
        coefficients[c] = [m[5] - ax * m[0] - ay * m[1], ax, ay];
    }
    // One second pass replaces scanning the whole image once per regional fit.
    for (i, value) in y.iter().enumerate() {
        let c = cell(i, w, h);
        let [offset, ax, ay] = coefficients[c];
        let x = ((i % w) as f64 + 0.5) / w as f64;
        let row = ((i / w) as f64 + 0.5) / h as f64;
        plane[c] += (value - (offset + ax * x + ay * row)).powi(2) / counts[c] as f64;
    }
    for value in &mut plane {
        *value = value.sqrt();
    }
    let (spectral, spectral_n) = spectrum(y, w, h);
    let total_runs = runs.iter().sum::<usize>();
    vec![
        bars(("ordinal-texture-entropy","Ordinal texture entropy","Normalized six-permutation entropy of horizontal triples; exact ties are reported separately. Asinh working luminance."),"ratio",permutation,&["Entropy","Tied triples"],pn),
        bars(("sample-texture-entropy","Sequence sample entropy","Half-count-smoothed length-2 to length-3 template continuation on at most 384 evenly spaced samples; scene complexity, not a noise verdict."),"signal",sample,&["Sample entropy","Length-2 match fraction"],sn),
        bars(("quantized-run-lengths","Quantized texture runs","Horizontal equal-bin run distribution after sixteen-level frame-relative luminance quantization; last bin includes runs of eight or more."),"fraction",runs.iter().map(|v|*v as f64/total_runs.max(1) as f64).collect(),&["1","2","3","4","5","6","7","8+"],total_runs),
        bars(("cooccurrence-texture","Neighbour co-occurrence texture","Sixteen-level right/lower co-occurrence contrast, homogeneity, energy and natural-log entropy. Frame-relative asinh luminance."),"signal",co.to_vec(),&["Contrast","Homogeneity","Energy","Entropy"],pairs),
        bars(("bright-mass-lacunarity","Bright mass lacunarity","Second moment divided by squared mean of above-median mass in complete 2,4,8,16-pixel boxes. Zero denotes no bright support."),"ratio",lacunarity,&["2 px","4 px","8 px","16 px"],n),
        map(("harris-corner-response","Regional corner response","Signed Harris determinant minus 0.04 times squared trace of the regional gradient tensor; positive corners and negative straight edges."),"signal4",corner,n),
        map(("local-plane-residual","Local illumination fit residual","RMS residual after a least-squares affine plane fit within each grid cell. Texture and nonplanar illumination both contribute."),"asinh signal",plane,n),
        bars(("line-spectral-flatness","Line spectral flatness","Geometric/arithmetic non-DC Fourier-power ratio on up to eight 64-sample lines per direction. Uniform lines report zero."),"ratio",spectral,&["Horizontal","Vertical"],spectral_n),
        bars(("neighbour-moran-index","Neighbour Moran index","Population-centred luminance autocorrelation over right/lower grid adjacency, normalized by total frame variance; constant fields report zero."),"ratio",vec![if denominator>1e-20 && pairs>0 {n as f64/pairs as f64*moran/denominator} else {0.0}],&["Moran index"],pairs),
        map(("robust-neighbour-roughness","Robust local roughness","Median absolute asinh-luminance difference to right/lower neighbours in each grid cell; isolates typical variation from rare large edges."),"asinh signal",rough.iter().map(|v|quantile(v,0.5)).collect(),pairs),
    ]
}
