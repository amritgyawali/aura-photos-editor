//! The two axes a photograph is measured against at each lightness (ADR-0160).
//!
//! A grade's white balance and split toning tint the things that were grey - a wall, a shirt, a
//! shadow - and they do it smoothly from shadows to highlights. What a photograph's content does
//! is different on both counts: foliage, a sky or a beige wall is a cluster of one colour at one
//! lightness, with nothing less coloured beside it. So each lightness interval offers its least
//! coloured cluster as evidence only when that cluster is plausibly a tinted grey and clearly
//! less coloured than the rest of the interval, and the axis is then the smoothest curve through
//! the evidence, with an interval that disagrees with its neighbours heard less.
use super::{smoothstep, weighted_quantile, AXIS_NODES, MAX_CAST};

/// The least coloured fifth of an interval are the neutral candidates, but never a band wider
/// than this above the very least coloured pixels, so a large coloured object whose rank falls
/// just inside the fifth cannot swallow a small grey patch.
const NEUTRAL_FRACTION: f32 = 0.2;
const NEUTRAL_TAIL_WIDTH: f32 = 0.015;
/// Candidates within this `OKLab` distance of their median form the cluster.
const CLUSTER_RADIUS: f32 = 0.035;
/// A cluster this weakly coloured is a tinted grey whatever surrounds it: a strong tungsten
/// cast left in measures about 0.05 of `OKLab` chroma.
const GREY: (f32, f32) = (0.035, 0.08);
/// A grade rarely tints greys beyond about 0.08; a cluster at 0.13 is an object.
const PLAUSIBLE: (f32, f32) = (0.08, 0.13);
/// Curvature and slope penalties of the smoother, relative to one interval of full evidence.
/// The evidence curve is stiff; the measuring axis follows its data closely.
const CURVATURE: f64 = 1.0;
const FOLLOW: f64 = 0.02;
const SLOPE: f64 = 0.05;
/// An interval farther than this from the smoothed axis keeps a third of its evidence.
const OUTLIER: f32 = 0.035;

type WeightedChroma = ([f32; 2], f32);

fn chroma(p: [f32; 2]) -> f32 {
    p[0].hypot(p[1])
}

fn median(set: &[&WeightedChroma]) -> [f32; 2] {
    [0, 1].map(|component| {
        let mut pairs: Vec<_> = set
            .iter()
            .map(|(p, w)| (p.get(component).copied().unwrap_or(0.0), *w))
            .collect();
        weighted_quantile(&mut pairs, 0.5).unwrap_or(0.0)
    })
}

/// The least coloured cluster in one lightness interval and the weight of data in it.
fn grey_cluster(members: &[WeightedChroma]) -> ([f32; 2], f32) {
    let mut radii: Vec<_> = members.iter().map(|(p, w)| (chroma(*p), *w)).collect();
    let tail = weighted_quantile(&mut radii, 0.05).unwrap_or(0.0);
    let cutoff = weighted_quantile(&mut radii, NEUTRAL_FRACTION)
        .unwrap_or(0.0)
        .min(tail + NEUTRAL_TAIL_WIDTH);
    let candidates: Vec<_> = members
        .iter()
        .filter(|(p, _)| chroma(*p) <= cutoff + 1e-6)
        .collect();
    if candidates.len() < 4 {
        return ([0.0; 2], 0.0);
    }
    let seed = median(&candidates);
    let near: Vec<_> = candidates
        .into_iter()
        .filter(|(p, _)| (p[0] - seed[0]).hypot(p[1] - seed[1]) <= CLUSTER_RADIUS)
        .collect();
    if near.len() < 4 {
        return ([0.0; 2], 0.0);
    }
    (median(&near), near.iter().map(|(_, w)| *w).sum())
}

/// The least coloured cluster in one lightness interval, how much data it rests on, and how
/// much it is evidence of a cast rather than of an object.
fn interval(members: &[WeightedChroma], n: f32) -> ([f32; 2], f32, f32) {
    let mut radii: Vec<_> = members.iter().map(|(p, w)| (chroma(*p), *w)).collect();
    let typical = weighted_quantile(&mut radii, 0.5).unwrap_or(0.0);
    let (centre, mass) = grey_cluster(members);
    if mass <= 0.0 {
        return ([0.0; 2], 0.0, 0.0);
    }
    let c = chroma(centre);
    // Grey on its own terms, or clearly less coloured than what surrounds it.
    let grey = 1.0 - smoothstep(GREY.0, GREY.1, c);
    let distinct = (2.0 * (typical - c) / typical.max(0.02)).clamp(0.0, 1.0);
    let plausible = 1.0 - smoothstep(PLAUSIBLE.0, PLAUSIBLE.1, c);
    let support = (mass / (0.01 * n)).clamp(0.0, 1.0);
    let evidence = support * grey.max(distinct) * plausible;
    (
        centre.map(|v| v.clamp(-MAX_CAST, MAX_CAST)),
        support,
        evidence,
    )
}

fn cell(a: &[Vec<f64>], row: usize, col: usize) -> f64 {
    a.get(row).and_then(|r| r.get(col)).copied().unwrap_or(0.0)
}

/// Solve the evidence-weighted smoothing problem exactly for one channel: minimise
/// `sum w (y - x)^2 + CURVATURE * sum (second difference)^2 + SLOPE * sum (first difference)^2`.
/// An interval without evidence takes its neighbours' level rather than extending a trend.
fn smooth(values: &[f32], weights: &[f32], curvature: f64) -> Vec<f32> {
    let n = values.len();
    let mut a = vec![vec![0.0_f64; n]; n];
    let mut b = vec![0.0_f64; n];
    let add = |a: &mut Vec<Vec<f64>>, terms: &[(usize, f64)], scale: f64| {
        for &(i, ci) in terms {
            for &(j, cj) in terms {
                if let Some(entry) = a.get_mut(i).and_then(|row| row.get_mut(j)) {
                    *entry += scale * ci * cj;
                }
            }
        }
    };
    for k in 0..n {
        let w = f64::from(weights.get(k).copied().unwrap_or(0.0));
        add(&mut a, &[(k, 1.0)], w + 1e-9);
        if let Some(slot) = b.get_mut(k) {
            *slot = w * f64::from(values.get(k).copied().unwrap_or(0.0));
        }
        if k + 1 < n {
            add(&mut a, &[(k, -1.0), (k + 1, 1.0)], SLOPE);
        }
        if k + 2 < n {
            add(&mut a, &[(k, 1.0), (k + 1, -2.0), (k + 2, 1.0)], curvature);
        }
    }
    // Gaussian elimination with partial pivoting on a nine by nine symmetric system.
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&i, &j| cell(&a, i, col).abs().total_cmp(&cell(&a, j, col).abs()))
            .unwrap_or(col);
        a.swap(col, pivot);
        b.swap(col, pivot);
        let diagonal = cell(&a, col, col);
        if diagonal.abs() < 1e-15 {
            continue;
        }
        let source: Vec<f64> = a.get(col).cloned().unwrap_or_default();
        let bc = b.get(col).copied().unwrap_or(0.0);
        for row in col + 1..n {
            let factor = cell(&a, row, col) / diagonal;
            if let Some(target) = a.get_mut(row) {
                for (t, s) in target.iter_mut().zip(&source) {
                    *t -= factor * s;
                }
            }
            if let Some(slot) = b.get_mut(row) {
                *slot -= factor * bc;
            }
        }
    }
    let mut y = vec![0.0_f64; n];
    for row in (0..n).rev() {
        let sum: f64 = (row + 1..n)
            .map(|j| cell(&a, row, j) * y.get(j).copied().unwrap_or(0.0))
            .sum();
        let diagonal = cell(&a, row, row);
        if let Some(slot) = y.get_mut(row) {
            *slot = if diagonal.abs() < 1e-15 {
                0.0
            } else {
                (b.get(row).copied().unwrap_or(0.0) - sum) / diagonal
            };
        }
    }
    y.into_iter().map(|v| v as f32).collect()
}

fn smooth_axis(raw: &[[f32; 2]], evidence: &[f32], curvature: f64) -> Vec<[f32; 2]> {
    let a: Vec<f32> = raw.iter().map(|c| c[0]).collect();
    let b: Vec<f32> = raw.iter().map(|c| c[1]).collect();
    smooth(&a, evidence, curvature)
        .into_iter()
        .zip(smooth(&b, evidence, curvature))
        .map(|(x, y)| [x.clamp(-MAX_CAST, MAX_CAST), y.clamp(-MAX_CAST, MAX_CAST)])
        .collect()
}

fn members_at(labs: &[[f32; 3]], k: usize) -> Vec<WeightedChroma> {
    let lightness = k as f32 / (AXIS_NODES - 1) as f32;
    labs.iter()
        .filter_map(|l| {
            let w = 1.0 - (l[0] - lightness).abs() * (AXIS_NODES - 1) as f32;
            (w > 0.0).then_some(([l[1], l[2]], w))
        })
        .collect()
}

fn enough(members: &[WeightedChroma], n: f32) -> bool {
    members.len() >= 8 && members.iter().map(|m| m.1).sum::<f32>() >= 1e-3 * n
}

/// The axis chroma is measured from, at each node, and how much data it rests on there.
///
/// Two properties pull against each other and both were measured. A photograph and its own edit
/// must measure each other consistently, which wants the centre of the interval's densest third:
/// an edit that shifts every colour shifts that centre by exactly as much, where "the least
/// coloured pixels" are different pixels once a warm grade has made the bluish ones least
/// coloured. And an edit learned from one frame must carry to the others, which wants the axis
/// on the greys, because what an edit does to greys is what every frame shares - anchored on
/// the dominant colour instead, a learned edit describes what happened to a red sari and applies
/// it to a blue sky. So the densest third is the axis, unless the interval holds a clearly
/// separate, plainly grey cluster, which is then the axis instead; on one real wedding shoot that
/// choice left a learned edit 1.9 from the right answer where the densest third alone left 2.6.
pub(super) fn measuring_axis(labs: &[[f32; 3]]) -> (Vec<[f32; 2]>, Vec<f32>) {
    let n = labs.len().max(1) as f32;
    let mut raw = vec![[0.0_f32; 2]; AXIS_NODES];
    let mut support = vec![0.0_f32; AXIS_NODES];
    for (k, (slot, held)) in raw.iter_mut().zip(support.iter_mut()).enumerate() {
        let members = members_at(labs, k);
        if !enough(&members, n) {
            continue;
        }
        let all: Vec<&WeightedChroma> = members.iter().collect();
        let mut c = median(&all);
        for _ in 0..2 {
            let mut radii: Vec<_> = members
                .iter()
                .map(|(p, w)| ((p[0] - c[0]).hypot(p[1] - c[1]), *w))
                .collect();
            let radius = weighted_quantile(&mut radii, 0.3).unwrap_or(0.0);
            let near: Vec<&WeightedChroma> = members
                .iter()
                .filter(|(p, _)| (p[0] - c[0]).hypot(p[1] - c[1]) <= radius + 1e-6)
                .collect();
            if near.len() >= 4 {
                c = median(&near);
            }
        }
        let (grey, grey_mass) = grey_cluster(&members);
        if grey_mass >= 0.004 * n && chroma(grey) < 0.035 && chroma(grey) + 0.025 < 0.65 * chroma(c)
        {
            c = grey;
        }
        // A cluster as coloured as an object is followed less, and an impossible one not at all.
        let plausible = 1.0 - smoothstep(0.12, 0.18, chroma(c));
        *slot = c.map(|v| v.clamp(-MAX_CAST, MAX_CAST));
        *held = (members.iter().map(|m| m.1).sum::<f32>() / (0.01 * n)).clamp(0.0, 1.0) * plausible;
    }
    if support.iter().sum::<f32>() <= 1e-6 {
        return (vec![[0.0; 2]; AXIS_NODES], vec![0.0; AXIS_NODES]);
    }
    (smooth_axis(&raw, &support, FOLLOW), support)
}

/// Where the photograph's greys sit at each node, and the evidence that they are greys.
///
/// The least coloured cluster of each interval, heard only when it is plausibly a tinted grey
/// and clearly less coloured than its surroundings, then the smoothest curve through that
/// evidence, with an interval that disagrees with its neighbours heard less. Between two
/// different scenes, the difference of two of these is the only cast one may take from the other.
pub(super) fn neutral_axis(labs: &[[f32; 3]]) -> (Vec<[f32; 2]>, Vec<f32>) {
    let n = labs.len().max(1) as f32;
    let mut raw = vec![[0.0_f32; 2]; AXIS_NODES];
    let mut evidence = vec![0.0_f32; AXIS_NODES];
    for (k, (slot, weight)) in raw.iter_mut().zip(evidence.iter_mut()).enumerate() {
        let members = members_at(labs, k);
        if enough(&members, n) {
            let (centre, _, heard) = interval(&members, n);
            (*slot, *weight) = (centre, heard);
        }
    }
    if evidence.iter().sum::<f32>() <= 1e-6 {
        return (vec![[0.0; 2]; AXIS_NODES], vec![0.0; AXIS_NODES]);
    }
    let mut heard = evidence.clone();
    let mut curve = smooth_axis(&raw, &heard, CURVATURE);
    for _ in 0..2 {
        for ((w, e), (r, s)) in heard.iter_mut().zip(&evidence).zip(raw.iter().zip(&curve)) {
            let off = (r[0] - s[0]).hypot(r[1] - s[1]);
            *w = if off > OUTLIER { e / 3.0 } else { *e };
        }
        curve = smooth_axis(&raw, &heard, CURVATURE);
    }
    (curve, heard)
}
