use super::{bars, cell, entropy, map, mean, neighbours, quantile, Diagnostic};

pub(super) fn measure(p: &[[f32; 3]], y: &[f64], w: usize, h: usize) -> Vec<Diagnostic> {
    let n = p.len();
    let bounded: Vec<_> = p.iter().map(|v| v.map(|c| f64::from(c).asinh())).collect();
    let mut ratios = [Vec::with_capacity(n), Vec::with_capacity(n)];
    let mut channels = [
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    ];
    let mut rg = Vec::with_capacity(n);
    let mut yb = Vec::with_capacity(n);
    let mut hue = vec![None; n];
    let mut hue_bins = [0_usize; 36];
    let mut chroma = Vec::with_capacity(n);
    let mut moments = [[0.0_f64; 5]; 64];
    let mut count = [0_usize; 64];
    for (i, pixel) in p.iter().enumerate() {
        let v = bounded[i];
        let a = v[0] - v[1];
        let b = (v[0] + v[1]) * 0.5 - v[2];
        let radius = a.hypot(b);
        rg.push(a);
        yb.push(b);
        chroma.push(radius);
        ratios[0].push((f64::from(pixel[0]).max(1e-6) / f64::from(pixel[1]).max(1e-6)).ln());
        ratios[1].push((f64::from(pixel[2]).max(1e-6) / f64::from(pixel[1]).max(1e-6)).ln());
        for (values, channel) in channels.iter_mut().zip(pixel) {
            values.push(f64::from(*channel).max(0.0));
        }
        if radius > 1e-6 {
            let angle = b.atan2(a).rem_euclid(std::f64::consts::TAU);
            hue[i] = Some(angle);
            hue_bins[((angle / std::f64::consts::TAU * 36.0).floor() as usize).min(35)] += 1;
        }
        let c = cell(i, w, h);
        count[c] += 1;
        for (sum, value) in moments[c].iter_mut().zip([a, b, a * a, a * b, b * b]) {
            *sum += value;
        }
    }
    let mut gray_edge = [0.0_f64; 3];
    let mut pair_n = 0;
    let mut winding = vec![0.0; 64];
    let mut winding_n = [0; 64];
    for i in 0..n {
        for j in neighbours(i, w, h) {
            pair_n += 1;
            for (channel, sum) in gray_edge.iter_mut().enumerate() {
                *sum += (bounded[i][channel] - bounded[j][channel]).abs().powi(6);
            }
            if let (Some(a), Some(b)) = (hue[i], hue[j]) {
                let d = (a - b + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI;
                let c = cell(i, w, h);
                winding[c] += d.abs() / std::f64::consts::PI;
                winding_n[c] += 1;
            }
        }
    }
    let max_rgb = channels.map(|values| quantile(&values, 0.95));
    let norm = max_rgb.iter().sum::<f64>();
    let max_rgb = max_rgb.map(|v| if norm > 1e-20 { v / norm } else { 0.0 });
    let edge = gray_edge.map(|v| (v / pair_n.max(1) as f64).powf(1.0 / 6.0));
    let norm = edge.iter().sum::<f64>();
    let edge = edge.map(|v| if norm > 1e-20 { v / norm } else { 0.0 });
    let means = [mean(&rg), mean(&yb)];
    let variance = rg
        .iter()
        .zip(&yb)
        .map(|(a, b)| (a - means[0]).powi(2) + (b - means[1]).powi(2))
        .sum::<f64>()
        / n as f64;
    let colourfulness = variance.sqrt() + 0.3 * means[0].hypot(means[1]);
    let occupied = hue_bins.iter().filter(|v| **v > 0).count();
    let mut gap = 0;
    let mut run = 0;
    for i in 0..72 {
        if hue_bins[i % 36] == 0 {
            run += 1;
            gap = gap.max(run.min(36));
        } else {
            run = 0;
        }
    }
    let mut joint = [0_usize; 256];
    let mut lm = [0_usize; 16];
    let mut cm = [0_usize; 16];
    let lo = y.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let max_chroma = chroma.iter().copied().fold(0.0, f64::max);
    for i in 0..n {
        let l = (((y[i] - lo) / (hi - lo).max(1e-12) * 16.0) as usize).min(15);
        let c = ((chroma[i] / max_chroma.max(1e-12) * 16.0) as usize).min(15);
        joint[l * 16 + c] += 1;
        lm[l] += 1;
        cm[c] += 1;
    }
    let information = (entropy(&lm) + entropy(&cm) - entropy(&joint)).max(0.0) / 2.0_f64.ln();
    let dispersion: Vec<_> = moments
        .iter()
        .zip(count)
        .map(|(m, k)| {
            let k = k.max(1) as f64;
            ((m[2] + m[4]) / k - (m[0] / k).powi(2) - (m[1] / k).powi(2))
                .max(0.0)
                .sqrt()
        })
        .collect();
    for (value, k) in winding.iter_mut().zip(winding_n) {
        *value /= k.max(1) as f64;
    }
    vec![
        bars(("log-ratio-neutrality","Log channel-ratio distribution","10th,50th,90th percentiles of log red/green and blue/green ratios. Nonpositive channels use a 1e-6 floor; this is not a semantic skin estimate."),"log ratio",ratios.iter().flat_map(|v|[0.1,0.5,0.9].map(|q|quantile(v,q))).collect(),&["R/G p10","R/G p50","R/G p90","B/G p10","B/G p50","B/G p90"],n),
        bars(("white-patch-illuminant","White-patch chromaticity","Normalized per-channel95th-percentile nonnegative working RGB; a scene-dependent illuminant candidate, not a white-balance verdict."),"fraction",max_rgb.to_vec(),&["Red","Green","Blue"],n),
        bars(("gray-edge-illuminant","Gray-edge chromaticity","Normalized sixth-power mean of asinh channel derivatives over right/lower adjacency. Scene edges bias this independent illuminant candidate."),"fraction",edge.to_vec(),&["Red","Green","Blue"],pair_n),
        bars(("opponent-colourfulness","Opponent colourfulness proxy","Opponent-plane spread plus0.3 times mean magnitude in signed asinh working RGB. Descriptive proxy, not calibrated display perception."),"asinh signal",vec![colourfulness],&["Colourfulness"],n),
        bars(("hue-gap-coverage","Hue occupancy and largest gap","Fraction of36 opponent-hue bins occupied and longest circular empty sector. Near-neutral samples are excluded; no chromatic support reports zero coverage and full gap."),"fraction",vec![occupied as f64/36.0,f64::from(gap)/36.0],&["Occupied hue bins","Largest empty arc"],hue_bins.iter().sum()),
        map(("local-hue-winding","Local hue discontinuity","Mean shortest angular change to valid right/lower neighbours divided by pi. Undefined neutral hues are excluded, not assigned an angle."),"fraction",winding,winding_n.iter().sum()),
        bars(("chroma-light-information","Chroma-light mutual information","Discrete mutual information in bits between sixteen frame-relative asinh-luminance and opponent-chroma bins. Zero means independent bin populations."),"bits",vec![information],&["Mutual information"],n),
        map(("patch-chroma-dispersion","Regional colour dispersion","Square root of the sum of opponent covariance diagonal entries in each grid cell. Measures local colour spread without classifying skin."),"asinh signal",dispersion,n),
    ]
}
