use super::{bars, cell, map, mean, neighbours, quantile, Diagnostic};
use std::collections::VecDeque;

fn clipping_components(mask: &[bool], w: usize, h: usize) -> (Vec<f64>, usize) {
    let mut visited = vec![false; mask.len()];
    let mut sizes = Vec::new();
    for i in 0..mask.len() {
        if !mask[i] || visited[i] {
            continue;
        }
        let mut queue = VecDeque::from([i]);
        visited[i] = true;
        let mut size = 0;
        while let Some(j) = queue.pop_front() {
            size += 1;
            let x = j % w;
            let row = j / w;
            for k in [
                if x > 0 { Some(j - 1) } else { None },
                if x + 1 < w { Some(j + 1) } else { None },
                if row > 0 { Some(j - w) } else { None },
                if row + 1 < h { Some(j + w) } else { None },
            ]
            .into_iter()
            .flatten()
            {
                if mask[k] && !visited[k] {
                    visited[k] = true;
                    queue.push_back(k);
                }
            }
        }
        sizes.push(f64::from(size));
    }
    let total = sizes.iter().sum::<f64>();
    let largest = sizes.iter().copied().fold(0.0, f64::max);
    (
        vec![
            sizes.len() as f64,
            largest,
            if total > 0.0 { largest / total } else { 0.0 },
        ],
        total as usize,
    )
}

pub(super) fn measure(p: &[[f32; 3]], y: &[f64], w: usize, h: usize) -> Vec<Diagnostic> {
    let n = y.len();
    let q1 = quantile(y, 0.25);
    let q3 = quantile(y, 0.75);
    let iqr = q3 - q1;
    let mut fences = [0.0; 2];
    for value in y {
        if *value < q1 - 1.5 * iqr {
            fences[0] += 1.0;
        }
        if *value > q3 + 1.5 * iqr {
            fences[1] += 1.0;
        }
    }
    let channels: [Vec<f64>; 3] =
        std::array::from_fn(|c| p.iter().map(|v| f64::from(v[c])).collect());
    let high = channels.each_ref().map(|v| quantile(v, 0.95));
    let mut tails = [0.0; 3];
    let mut upper = [0; 3];
    for pixel in p {
        let flags = std::array::from_fn::<_, 3, _>(|c| f64::from(pixel[c]) > high[c]);
        for (slot, flag) in upper.iter_mut().zip(flags) {
            *slot += usize::from(flag);
        }
        for (k, (a, b)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
            tails[k] += f64::from(u8::from(flags[a] && flags[b]));
        }
    }
    for (k, (a, b)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
        tails[k] /= upper[a].min(upper[b]).max(1) as f64;
    }
    let clipped: Vec<_> = p
        .iter()
        .map(|v| v.iter().any(|c| *c <= 0.0 || *c >= 1.0))
        .collect();
    let (components, clipped_n) = clipping_components(&clipped, w, h);
    let mut moments = [[0.0; 2]; 64];
    let mut count = [0; 64];
    let mut edge_overlap = vec![0.0; 64];
    let mut edge_total = [0.0; 64];
    let mut pair_n = 0;
    let mut strips: [Vec<f64>; 16] = std::array::from_fn(|_| Vec::new());
    for (i, value) in y.iter().enumerate() {
        let c = cell(i, w, h);
        count[c] += 1;
        moments[c][0] += value;
        moments[c][1] += value * value;
        strips[i % w * 8 / w].push(*value);
        strips[8 + i / w * 8 / h].push(*value);
        for j in neighbours(i, w, h) {
            let edge = (value - y[j]).abs();
            edge_total[c] += edge;
            if clipped[i] || clipped[j] {
                edge_overlap[c] += edge;
            }
            pair_n += 1;
        }
    }
    let patch: Vec<_> = moments
        .iter()
        .zip(count)
        .filter(|(_, k)| *k > 1)
        .map(|(m, k)| {
            let k = f64::from(k);
            let mu = m[0] / k;
            (mu, (m[1] / k - mu * mu).max(0.0))
        })
        .collect();
    let xm = mean(&patch.iter().map(|v| v.0).collect::<Vec<_>>());
    let ym = mean(&patch.iter().map(|v| v.1).collect::<Vec<_>>());
    let xx = patch.iter().map(|v| (v.0 - xm).powi(2)).sum::<f64>();
    let xy = patch.iter().map(|v| (v.0 - xm) * (v.1 - ym)).sum::<f64>();
    let slope = if xx > 1e-20 { xy / xx } else { 0.0 };
    let intercept = ym - slope * xm;
    let residual = patch
        .iter()
        .map(|v| (v.1 - intercept - slope * v.0).powi(2))
        .sum::<f64>()
        / patch.len().max(1) as f64;
    for (value, total) in edge_overlap.iter_mut().zip(edge_total) {
        *value = if total > 1e-20 { *value / total } else { 0.0 };
    }
    let mut alias = vec![0.0; 64];
    let mut alias_n = [0; 64];
    for row in 0..h / 2 {
        for x in 0..w / 2 {
            let i = row * 2 * w + x * 2;
            let average = (y[i] + y[i + 1] + y[i + w] + y[i + w + 1]) * 0.25;
            let c = cell(i, w, h);
            alias[c] += (y[i] - average).abs();
            alias_n[c] += 1;
        }
    }
    for (value, k) in alias.iter_mut().zip(alias_n) {
        *value /= k.max(1) as f64;
    }
    vec![
        bars(("tukey-tail-support","Robust luminance outliers","Fraction belowQ1-1.5IQR or aboveQ3+1.5IQR in asinh working luminance. A descriptive tail fence, not a defect or exposure judgment."),"fraction",fences.iter().map(|v|v/n as f64).collect(),&["Lower tail","Upper tail"],n),
        bars(("channel-tail-dependence","Channel highlight dependence","Joint above95th-percentile support divided by the smaller marginal support, per channel pair. Ties at the percentile do not count as exceedances."),"ratio",tails.to_vec(),&["Red/green","Red/blue","Green/blue"],n),
        bars(("patch-mean-variance-fit","Regional mean-variance fit","Least-squares regional variance versus mean in asinh luminance; texture also contributes, so this is not sensor noise calibration."),"signal",vec![slope,intercept,residual.sqrt()],&["Slope","Intercept","Fit RMS"],patch.len()),
        bars(("clipping-component-support","Connected clipping support","Four-connected components with any working RGB channel at or outside0..1. Counts connected samples, not clipped source-sensor pixels."),"samples",components,&["Components","Largest area","Largest fraction"],clipped_n),
        map(("edge-clipping-overlap","Clipping-weighted edge fraction","Fraction of right/lower absolute asinh-luminance edge energy touching an out-of-range or endpoint working RGB sample."),"fraction",edge_overlap,pair_n),
        bars(("robust-strip-drift","Robust strip lighting profile","Median asinh luminance across eight vertical then eight horizontal strips. Median resists rare specular pixels; these are preview coordinates."),"asinh signal",strips.iter().map(|v|quantile(v,0.5)).collect(),&[],n),
        map(("thumbnail-alias-sensitivity","Thumbnail phase sensitivity","Mean absolute difference between top-left decimation and2x2 box averages in complete blocks. Predicts phase sensitivity of a simple thumbnail, not camera or JPEG quality."),"asinh signal",alias,alias_n.iter().sum()),
    ]
}
