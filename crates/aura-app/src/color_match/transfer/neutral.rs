//! Neutral-axis estimates with explicit evidence against dominant object colours.
use super::{weighted_quantile, AXIS_NODES, MAX_CAST};

// Conservative estimation priors in OKLab units, not calibrated illuminant bounds.
const NEUTRAL_FRACTION: f32 = 0.2;
const NEUTRAL_TAIL_WIDTH: f32 = 0.015;
const CLUSTER_RADIUS: f32 = 0.035;
type WeightedChroma = ([f32; 2], f32);

fn centre(set: &[&WeightedChroma]) -> [f32; 2] {
    [0, 1].map(|component| {
        let mut pairs: Vec<_> = set
            .iter()
            .map(|(p, w)| (if component == 0 { p[0] } else { p[1] }, *w))
            .collect();
        weighted_quantile(&mut pairs, 0.5).unwrap_or(0.0)
    })
}

fn cluster(members: &[WeightedChroma], prior: [f32; 2]) -> (f32, [f32; 2]) {
    let distance = |p: [f32; 2]| (p[0] - prior[0]).hypot(p[1] - prior[1]);
    let mut radii: Vec<_> = members.iter().map(|(p, w)| (distance(*p), *w)).collect();
    let tail = weighted_quantile(&mut radii, 0.05).unwrap_or(0.0);
    // Limiting the tail width prevents ties in a large coloured object swallowing a small
    // neutral patch when the rank falls just inside that object.
    let cutoff = weighted_quantile(&mut radii, NEUTRAL_FRACTION)
        .unwrap_or(0.0)
        .min(tail + NEUTRAL_TAIL_WIDTH);
    let candidates: Vec<_> = members
        .iter()
        .filter(|(p, _)| distance(*p) <= cutoff + 1e-6)
        .collect();
    let seed = centre(&candidates);
    let near: Vec<_> = candidates
        .into_iter()
        .filter(|(p, _)| (p[0] - seed[0]).hypot(p[1] - seed[1]) <= CLUSTER_RADIUS)
        .collect();
    if near.len() < 4 {
        return (0.0, prior);
    }
    (near.iter().map(|(_, w)| *w).sum(), centre(&near))
}

/// A compact majority cluster alone is ambiguous. Prefer a well-separated low-chroma
/// cluster when it has enough samples, instead of interpreting a coloured subject as a cast.
fn interval(members: &[WeightedChroma], n: f32, ceiling: f32) -> ([f32; 2], f32) {
    // A dominant object is not a fallback illuminant. A supported minority can be a better
    // neutral anchor even when this lightness interval covers only a small part of the image.
    let (mass, c) = cluster(members, [0.0; 2]);
    if mass < 0.001 * n || c[0].hypot(c[1]) > ceiling {
        return ([0.0; 2], 0.0);
    }
    let mut radii: Vec<_> = members
        .iter()
        .filter_map(|(p, w)| {
            let radius = (p[0] - c[0]).hypot(p[1] - c[1]);
            (radius <= CLUSTER_RADIUS).then_some((radius, *w))
        })
        .collect();
    let radius = weighted_quantile(&mut radii, 0.5).unwrap_or(CLUSTER_RADIUS);
    let weight = (mass / (0.004 * n)).clamp(0.0, 1.0)
        * (1.0 - (radius - 0.005) / CLUSTER_RADIUS).clamp(0.0, 1.0);
    (c.map(|v| v.clamp(-MAX_CAST, MAX_CAST)), weight)
}

pub(super) fn neutral_axis(labs: &[[f32; 3]]) -> (Vec<[f32; 2]>, Vec<f32>) {
    if !labs.is_empty() && labs.iter().all(|l| l[1].hypot(l[2]) > 0.1) {
        return (vec![[0.0; 2]; AXIS_NODES], vec![0.0; AXIS_NODES]);
    }
    let n = labs.len().max(1) as f32;
    let mut chromas: Vec<_> = labs.iter().map(|l| (l[1].hypot(l[2]), 1.0)).collect();
    let neutral_ceiling = (weighted_quantile(&mut chromas, 0.05).unwrap_or(0.0) + 0.025).min(0.085);
    let mut raw = vec![[0.0_f32; 2]; AXIS_NODES];
    let mut evidence = vec![0.0_f32; AXIS_NODES];
    for (k, (slot, weight)) in raw.iter_mut().zip(evidence.iter_mut()).enumerate() {
        let lightness = k as f32 / (AXIS_NODES - 1) as f32;
        let members: Vec<_> = labs
            .iter()
            .filter_map(|l| {
                let w = 1.0 - (l[0] - lightness).abs() * (AXIS_NODES - 1) as f32;
                (w > 0.0).then_some(([l[1], l[2]], w))
            })
            .collect();
        if members.len() >= 8 && members.iter().map(|m| m.1).sum::<f32>() >= 1e-3 * n {
            let highlight = super::smoothstep(0.7, 0.9, lightness);
            (*slot, *weight) = interval(&members, n, neutral_ceiling + 0.04 * highlight);
        }
    }
    // A near-grey coloured wall can occupy a whole interval. Prefer evidence consistent
    // with the bright neutral anchor; allow larger shadow residuals for split toning.
    if let Some((anchor, (&white, _))) = raw
        .iter()
        .zip(&evidence)
        .enumerate()
        .rev()
        .find(|(i, (_, w))| *i >= AXIS_NODES / 2 && **w > 0.05)
    {
        for (k, (&c, w)) in raw.iter().zip(&mut evidence).enumerate() {
            let lightness = k as f32 / (AXIS_NODES - 1) as f32;
            let scale = k as f32 / anchor as f32;
            let residual = (c[0] - white[0] * scale).hypot(c[1] - white[1] * scale);
            if residual > 0.005 + 0.07 * (1.0 - lightness).powi(2) {
                *w = 0.0;
            }
        }
    }
    (fill(&raw, &evidence), evidence)
}

fn fill(raw: &[[f32; 2]], evidence: &[f32]) -> Vec<[f32; 2]> {
    let anchors: Vec<_> = raw
        .iter()
        .zip(evidence)
        .enumerate()
        .filter_map(|(i, (&c, &w))| (w > 0.05).then_some((i as f32, c)))
        .collect();
    (0..AXIS_NODES)
        .map(|k| {
            let x = k as f32;
            let left = anchors.iter().rev().find(|(i, _)| *i <= x);
            let right = anchors.iter().find(|(i, _)| *i >= x);
            match (left, right) {
                (Some(&(a, ca)), Some(&(b, cb))) if b > a => {
                    let t = (x - a) / (b - a);
                    [ca[0] + t * (cb[0] - ca[0]), ca[1] + t * (cb[1] - ca[1])]
                }
                (Some(&(_, c)), Some(_)) => c,
                (Some(&(i, c)), None) | (None, Some(&(i, c))) => {
                    let scale = if i > 0.0 { (x / i).min(2.0) } else { 1.0 };
                    c.map(|v| (v * scale).clamp(-MAX_CAST, MAX_CAST))
                }
                _ => [0.0; 2],
            }
        })
        .collect()
}
