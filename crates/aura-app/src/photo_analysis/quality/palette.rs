use super::{avg, bars, bin, Diagnostic};

fn entropy(hist: &[usize], n: usize) -> f64 {
    hist.iter()
        .filter(|c| **c > 0)
        .map(|c| {
            let p = *c as f64 / n.max(1) as f64;
            -p * p.log2()
        })
        .sum()
}

fn mutual_information(pixels: &[[f32; 3]], a: usize, b: usize) -> f32 {
    let mut joint = [0; 256];
    let mut left = [0; 16];
    let mut right = [0; 16];
    for p in pixels {
        let x = bin(p[a], 16);
        let y = bin(p[b], 16);
        left[x] += 1;
        right[y] += 1;
        joint[y * 16 + x] += 1;
    }
    (entropy(&left, pixels.len()) + entropy(&right, pixels.len()) - entropy(&joint, pixels.len()))
        .max(0.0) as f32
}

pub(super) fn measure(pixels: &[[f32; 3]], luma: &[f32]) -> Vec<Diagnostic> {
    let n = pixels.len();
    let mut cube = vec![0_usize; 64];
    let mut hues = vec![0.0; 24];
    let mut joint = vec![0.0; 16 * 16];
    let mut level_count = [0; 256];
    let mut neutral = [[0.0_f64; 3]; 8];
    let mut neutral_n = [0; 8];
    let mut chroma = [0.0_f64; 5];
    let mut colored = 0;
    let mut hue_n = 0;
    let mean = luma.iter().map(|v| f64::from(*v)).sum::<f64>() / n as f64;
    let mut moments = [0.0_f64; 3];
    for (p, y) in pixels.iter().zip(luma) {
        let high = p.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let low = p.iter().copied().fold(f32::INFINITY, f32::min);
        let range = (high - low).max(0.0);
        let saturation = range / high.max(1e-6);
        cube[bin(p[0], 4) * 16 + bin(p[1], 4) * 4 + bin(p[2], 4)] += 1;
        joint[bin(saturation, 16) * 16 + bin(*y, 16)] += 1.0;
        level_count[bin(*y, 256)] += 1;
        if range > 1e-6 {
            let angle = (3.0_f32.sqrt() * (p[1] - p[2])).atan2(2.0 * p[0] - p[1] - p[2]);
            let hue = (angle / std::f32::consts::TAU).rem_euclid(1.0);
            hues[((hue * 24.0) as usize).min(23)] += range;
            hue_n += 1;
        }
        if saturation < 0.15 && *y > 0.01 {
            let index = bin(*y, 8);
            for c in 0..3 {
                neutral[index][c] += f64::from(p[c] / y.max(1e-6) - 1.0);
            }
            neutral_n[index] += 1;
        }
        if *y > 0.001 {
            let u = f64::from((p[0] / y - 1.0).clamp(-8.0, 8.0));
            let v = f64::from((p[2] / y - 1.0).clamp(-8.0, 8.0));
            for (sum, value) in chroma.iter_mut().zip([u, v, u * u, u * v, v * v]) {
                *sum += value;
            }
            colored += 1;
        }
        let delta = f64::from(*y) - mean;
        moments[0] += delta.powi(2);
        moments[1] += delta.powi(3);
        moments[2] += delta.powi(4);
    }
    let mut gaps = vec![0.0; 16];
    let first = level_count.iter().position(|v| *v > 0).unwrap_or(0);
    let last = level_count.iter().rposition(|v| *v > 0).unwrap_or(0);
    let mut run = 0_usize;
    for count in &level_count[first..=last] {
        if *count == 0 {
            run += 1;
        } else if run > 0 {
            gaps[run.min(16) - 1] += 1.0;
            run = 0;
        }
    }
    let curve: Vec<_> = (0..3)
        .flat_map(|c| (0..8).map(move |i| avg(neutral[i][c], neutral_n[i])))
        .collect();
    let covariance = if colored == 0 {
        vec![0.0; 3]
    } else {
        let t = colored as f64;
        vec![
            (chroma[2] / t - (chroma[0] / t).powi(2)).max(0.0) as f32,
            (chroma[3] / t - chroma[0] * chroma[1] / (t * t)) as f32,
            (chroma[4] / t - (chroma[1] / t).powi(2)).max(0.0) as f32,
        ]
    };
    let variance = moments[0] / n as f64;
    let standardized = if variance > 1e-18 {
        vec![
            (moments[1] / n as f64 / variance.powf(1.5)) as f32,
            (moments[2] / n as f64 / variance.powi(2) - 3.0) as f32,
        ]
    } else {
        vec![0.0; 2]
    };
    vec![
        bars("channel-information", "Channel information dependence", "Mutual information of 16-bin linear channel pairs. Zero means independent bins, including constant signals.", "bits", vec![mutual_information(pixels,0,1),mutual_information(pixels,0,2),mutual_information(pixels,1,2)], &["R/G","R/B","G/B"], n),
        bars("palette-cube", "Working RGB palette cube", "Photo area in 64 linear RGB cube cells ordered red, green, blue. Values outside nominal range accumulate at its boundary.", "%", cube.into_iter().map(|count| (count as f64 / n as f64 * 100.0) as f32).collect(), &[], n),
        bars("hue-energy", "Circular hue energy", "RGB opponent angle weighted by absolute channel range in 24 angular bins; neutral pixels contribute no hue.", "signal", hues, &[], hue_n),
        super::super::diagnostic(("chroma-light-joint", "Chroma and light distribution", "Linear luminance horizontally against relative RGB saturation vertically; a joint population plot."), "density", "samples", (16,16), joint, &[], n),
        bars("neutral-cast-trend", "Neutral cast by luminance", "Mean R/Y−1, G/Y−1 and B/Y−1 in eight brightness bins for low-chroma samples. Empty bins are zero; scene colours can bias this heuristic.", "ratio", curve, &[], neutral_n.iter().sum()),
        bars("chroma-covariance", "Chroma covariance", "Population covariance of R/Y−1 and B/Y−1; reveals chromatic spread and its signed association.", "ratio²", covariance, &["Red variance","Cross covariance","Blue variance"], colored),
        bars("tonal-moments", "Tonal asymmetry and tails", "Standardized luminance skewness and excess kurtosis. Constants report zero; these describe the scene, not a quality verdict.", "ratio", standardized, &["Skewness","Excess kurtosis"], n),
        bars("level-gap-runs", "Missing level runs", "Runs of unoccupied levels inside the measured 256-bin linear luminance range; sixteen includes longer gaps. Scene content can create gaps.", "runs", gaps, &[], n),
    ]
}
