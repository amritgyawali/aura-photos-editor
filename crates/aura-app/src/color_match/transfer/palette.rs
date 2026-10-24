//! Deterministic sliced chromatic distribution transport, baked into a continuous 2D LUT.
//! Unlike shared hue sectors, signed projections detect colours absent from either image.
use super::{Distance, GradeStats, Sample, Scales};
use aura_core::AuraResult;
use aura_recipe::reference_grade::{
    palette::{EDGE, LIMIT},
    PaletteMap, ReferenceGrade,
};

pub(super) fn compact(labs: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let count = labs.len().min(1024);
    (0..count)
        .filter_map(|i| {
            labs.get(((2 * i + 1) * labs.len() / (2 * count)).min(labs.len() - 1))
                .copied()
        })
        .collect()
}

fn direction(step: usize) -> [f32; 2] {
    let angle = step as f32 * 2.399_963_1;
    [angle.cos(), angle.sin()]
}

fn projection(labs: &[[f32; 3]], dir: [f32; 2]) -> Vec<f32> {
    super::quantiles(
        labs.iter()
            .map(|p| p[1] * dir[0] + p[2] * dir[1] + 0.6)
            .collect(),
        65,
        0.998,
        0.002,
    )
}

pub(super) fn distance(reference: &[[f32; 3]], actual: &[[f32; 3]]) -> f32 {
    if reference.is_empty() || actual.is_empty() {
        return 0.0;
    }
    (0..8)
        .map(|i| {
            let (r, a) = (
                projection(reference, direction(i)),
                projection(actual, direction(i)),
            );
            r.iter().zip(a).map(|(r, a)| (r - a).abs()).sum::<f32>() / r.len() as f32
        })
        .sum::<f32>()
        * 100.0
        / 8.0
}

struct Slice {
    dir: [f32; 2],
    map: super::QuantileMap,
}

impl Slice {
    fn apply(&self, p: &mut [f32; 2]) {
        let x = p[0] * self.dir[0] + p[1] * self.dir[1] + 0.6;
        let shift = 0.75 * (self.map.apply(x) - x).clamp(-0.15, 0.15);
        p[0] += shift * self.dir[0];
        p[1] += shift * self.dir[1];
    }
}

fn map(grid: &[[f32; 2]], offset: [f32; 2]) -> PaletteMap {
    PaletteMap {
        offset,
        deltas: grid
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let at = PaletteMap::coordinate(i);
                [p[0] - at[0] - offset[0], p[1] - at[1] - offset[1]].map(|v| v.clamp(-LIMIT, LIMIT))
            })
            .collect(),
    }
}

pub(super) fn refine(
    reference: &GradeStats,
    target: &[Sample],
    mut best: (Distance, ReferenceGrade),
    quality: super::super::MatchQuality,
    checkpoint: &mut impl FnMut() -> AuraResult<()>,
) -> AuraResult<(Distance, ReferenceGrade)> {
    let base = best.1.clone();
    let mut actual = super::transformed(&base, target);
    let mean = |points: &[[f32; 3]]| {
        points
            .iter()
            .fold([0.0; 2], |sum, p| [sum[0] + p[1], sum[1] + p[2]])
            .map(|v| v / points.len().max(1) as f32)
    };
    let (r, a) = (mean(&reference.palette_samples), mean(&actual));
    let offset = [r[0] - a[0], r[1] - a[1]].map(|v| v.clamp(-LIMIT, LIMIT));
    let mut grid: Vec<_> = (0..EDGE * EDGE).map(PaletteMap::coordinate).collect();
    for step in 0..quality.iterations() * 8 {
        checkpoint()?;
        let dir = direction(step);
        let slice = Slice {
            dir,
            map: super::QuantileMap::bounded(
                &projection(&actual, dir),
                &projection(&reference.palette_samples, dir),
                1.2,
                (0.25, 2.0),
            ),
        };
        for p in &mut actual {
            let mut ab = [p[1], p[2]];
            slice.apply(&mut ab);
            p[1] = ab[0];
            p[2] = ab[1];
        }
        for p in &mut grid {
            slice.apply(p);
        }
        if step % 8 == 7 {
            let mut candidate = base.clone();
            candidate.palette = Some(map(&grid, offset));
            let score = super::distance(
                reference,
                &super::measure(&super::transformed(&candidate, target)),
            );
            if score.total.is_finite() && score.total + 1e-4 < best.0.total {
                best = (score, candidate);
            }
        }
    }
    Ok(best)
}

pub(super) fn mix(
    from: Option<&PaletteMap>,
    to: Option<&PaletteMap>,
    scales: &Scales,
) -> Option<PaletteMap> {
    if from.is_none() && to.is_none() {
        return None;
    }
    let colour = scales.chroma.min(scales.saturation).min(scales.hue);
    if colour.abs() < f32::EPSILON
        && scales.temperature.abs() < f32::EPSILON
        && scales.tint.abs() < f32::EPSILON
    {
        return from.cloned();
    }
    if (colour - 1.0).abs() < f32::EPSILON
        && (scales.temperature - 1.0).abs() < f32::EPSILON
        && (scales.tint - 1.0).abs() < f32::EPSILON
    {
        return to.cloned();
    }
    let zero = [0.0; 2];
    let offset = |map: Option<&PaletteMap>| map.map_or(zero, |m| m.offset);
    let (a, b) = (offset(from), offset(to));
    let at =
        |map: Option<&PaletteMap>, i| map.and_then(|m| m.deltas.get(i)).copied().unwrap_or(zero);
    Some(PaletteMap {
        offset: [
            super::lerp(a[0], b[0], scales.tint),
            super::lerp(a[1], b[1], scales.temperature),
        ],
        deltas: (0..EDGE * EDGE)
            .map(|i| {
                let (a, b) = (at(from, i), at(to, i));
                [
                    super::lerp(a[0], b[0], colour),
                    super::lerp(a[1], b[1], colour),
                ]
            })
            .collect(),
    })
}
