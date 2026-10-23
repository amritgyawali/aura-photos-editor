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
fn interval(members: &[WeightedChroma], n: f32) -> ([f32; 2], f32) {
    let all: Vec<_> = members.iter().collect();
    let mut c = centre(&all);
    let mut radius = 0.0;
    for _ in 0..2 {
        let mut radii: Vec<_> = members
            .iter()
            .map(|(p, w)| ((p[0] - c[0]).hypot(p[1] - c[1]), *w))
            .collect();
        radius = weighted_quantile(&mut radii, 0.3).unwrap_or(0.0);
        let near: Vec<_> = members
            .iter()
            .filter(|(p, _)| (p[0] - c[0]).hypot(p[1] - c[1]) <= radius + 1e-6)
            .collect();
        if near.len() >= 4 {
            c = centre(&near);
        }
    }
    let (neutral_mass, neutral) = cluster(members, [0.0; 2]);
    let chroma = |p: [f32; 2]| p[0].hypot(p[1]);
    let mass: f32 = members.iter().map(|m| m.1).sum();
    let adjust = mass >= 0.6 * n
        && neutral_mass >= 0.004 * n
        && chroma(neutral) < 0.035
        && chroma(neutral) + 0.025 < 0.65 * chroma(c);
    if adjust {
        c = neutral;
        radius = 0.0;
    }
    let weight =
        (mass / (0.02 * n)).clamp(0.0, 1.0) * (1.0 - (radius - 0.015) / 0.05).clamp(0.15, 1.0);
    (c.map(|v| v.clamp(-MAX_CAST, MAX_CAST)), weight)
}

pub(super) fn neutral_axis(labs: &[[f32; 3]]) -> (Vec<[f32; 2]>, Vec<f32>) {
    if !labs.is_empty() && labs.iter().all(|l| l[1].hypot(l[2]) > 0.1) {
        return (vec![[0.0; 2]; AXIS_NODES], vec![0.0; AXIS_NODES]);
    }
    let n = labs.len().max(1) as f32;
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
            (*slot, *weight) = interval(&members, n);
        }
    }
    (fill(&raw, &evidence), evidence)
}

fn fill(raw: &[[f32; 2]], evidence: &[f32]) -> Vec<[f32; 2]> {
    let total: f32 = evidence.iter().sum();
    let mut global = [0.0; 2];
    if total > 1e-6 {
        for (c, w) in raw.iter().zip(evidence) {
            global[0] += c[0] * w / total;
            global[1] += c[1] * w / total;
        }
    }
    let mut filled: Vec<_> = raw
        .iter()
        .zip(evidence)
        .map(|(c, w)| {
            [
                c[0] * w + global[0] * (1.0 - w),
                c[1] * w + global[1] * (1.0 - w),
            ]
        })
        .collect();
    for _ in 0..3 {
        let previous = filled.clone();
        for (k, slot) in filled.iter_mut().enumerate() {
            let neighbours: Vec<_> = [k.checked_sub(1), Some(k + 1)]
                .into_iter()
                .flatten()
                .filter_map(|i| previous.get(i).copied())
                .collect();
            let nb = neighbours.iter().fold([0.0; 2], |s, v| {
                [
                    s[0] + v[0] / neighbours.len() as f32,
                    s[1] + v[1] / neighbours.len() as f32,
                ]
            });
            let w = evidence.get(k).copied().unwrap_or(0.0);
            let c = raw.get(k).copied().unwrap_or([0.0; 2]);
            *slot = [c[0] * w + nb[0] * (1.0 - w), c[1] * w + nb[1] * (1.0 - w)];
        }
    }
    filled
}
