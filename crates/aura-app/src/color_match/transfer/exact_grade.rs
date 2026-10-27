//! The reference's exact colours written as a stored reference grade (ADR-0182).
//!
//! The transfer in `color_match::exact` is a function of colour; the grade the renderer applies is
//! a tone curve, a neutral axis by lightness and a palette displacement field over the colour
//! plane, applied in that order. Each is fitted to the transfer on the photograph's own colours:
//! the tone curve is the transfer's tone map read at its nodes; the axis is the colour shift each
//! output lightness takes on average, which carries a split tone; and the field is what remains,
//! spread over the plane, smoothed, and filled toward its neighbours where the photograph has no
//! colours, so an unseen colour moves the way the colours around it do.
use super::{
    distance, limit_stretch_to, measure, regularise_tone_within, transformed, AuraResult, Fit,
    GradeStats, ReferenceGrade, Sample, AXIS_NODES, MAX_CAST, MIN_SAMPLES, TONE_NODES,
};
use aura_core::progress::CancelToken;
use aura_recipe::reference_grade::palette::{EDGE, LIMIT, SPAN};
use aura_recipe::reference_grade::PaletteMap;

/// Colours the grade is fitted on.
const SAMPLES: usize = 16_384;
/// How far the exact colours may stretch nearby colours: twice a look (`MAX_STRETCH`) and a
/// little more than the strongest real looks learned from their originals (3.2), because copying a
/// reference's tone histogram onto another scene needs steeper curves; the native check still
/// refuses any band the result shows.
pub(in crate::color_match) const MAX_EXACT_STRETCH: f32 = 4.0;
/// How far the exact tone curve may depart from the identity, and how steep it may be: a high-key
/// reference can brighten a darker photograph this far; a look's curve stops at 0.35 and 4.
const EXACT_TONE_DEVIATION: f32 = 0.6;
const EXACT_TONE_SLOPE: f32 = 4.0;
/// Weight of the neighbours against a cell's own colours when the field is smoothed, in samples.
const SMOOTHING: f32 = 4.0;

fn cell(v: f32) -> f32 {
    ((v + SPAN) / (2.0 * SPAN)).clamp(0.0, 1.0) * (EDGE - 1) as f32
}

/// Spread each residual onto its four grid cells, then smooth and fill the field.
fn field(points: &[([f32; 2], [f32; 2])]) -> Vec<[f32; 2]> {
    let mut sum = vec![[0.0_f32; 2]; EDGE * EDGE];
    let mut weight = vec![0.0_f32; EDGE * EDGE];
    for (at, residual) in points {
        let (x, y) = (cell(at[0]), cell(at[1]));
        let (ix, iy) = ((x as usize).min(EDGE - 2), (y as usize).min(EDGE - 2));
        let (tx, ty) = (x - ix as f32, y - iy as f32);
        for (index, w) in [
            (iy * EDGE + ix, (1.0 - tx) * (1.0 - ty)),
            (iy * EDGE + ix + 1, tx * (1.0 - ty)),
            ((iy + 1) * EDGE + ix, (1.0 - tx) * ty),
            ((iy + 1) * EDGE + ix + 1, tx * ty),
        ] {
            if let (Some(s), Some(total)) = (sum.get_mut(index), weight.get_mut(index)) {
                s[0] += w * residual[0];
                s[1] += w * residual[1];
                *total += w;
            }
        }
    }
    let mut values: Vec<[f32; 2]> = sum
        .iter()
        .zip(&weight)
        .map(|(s, &w)| {
            if w > 0.0 {
                [s[0] / w, s[1] / w]
            } else {
                [0.0; 2]
            }
        })
        .collect();
    // Jacobi passes: a cell with colours keeps them in proportion to how many it has; an empty
    // cell takes its neighbours', so the field reaches unseen colours continuously.
    for _ in 0..96 {
        let previous = values.clone();
        for (index, value) in values.iter_mut().enumerate() {
            let (x, y) = (index % EDGE, index / EDGE);
            let mut neighbours = [0.0_f32; 2];
            let mut count = 0.0_f32;
            let around = [
                x.checked_sub(1).map(|nx| (nx, y)),
                (x + 1 < EDGE).then_some((x + 1, y)),
                y.checked_sub(1).map(|ny| (x, ny)),
                (y + 1 < EDGE).then_some((x, y + 1)),
            ];
            for (nx, ny) in around.into_iter().flatten() {
                if let Some(v) = previous.get(ny * EDGE + nx) {
                    neighbours[0] += v[0];
                    neighbours[1] += v[1];
                    count += 1.0;
                }
            }
            let own = weight.get(index).copied().unwrap_or(0.0);
            let measured = if own > 0.0 {
                sum.get(index)
                    .map_or([0.0; 2], |s| [s[0] / own, s[1] / own])
            } else {
                [0.0; 2]
            };
            let pull = SMOOTHING / count.max(1.0);
            let total = own + SMOOTHING;
            *value = [
                (own * measured[0] + pull * neighbours[0]) / total,
                (own * measured[1] + pull * neighbours[1]) / total,
            ];
        }
    }
    values
        .into_iter()
        .map(|v| v.map(|x| x.clamp(-LIMIT, LIMIT)))
        .collect()
}

/// The neutral axis at `lightness`, interpolated as the renderer interpolates it.
fn axis_value(nodes: &[[f32; 2]], lightness: f32) -> [f32; 2] {
    let position = lightness.clamp(0.0, 1.0) * (AXIS_NODES - 1) as f32;
    let node = (position as usize).min(AXIS_NODES - 2);
    let along = position - node as f32;
    let low = nodes.get(node).copied().unwrap_or([0.0; 2]);
    let high = nodes.get(node + 1).copied().unwrap_or(low);
    [
        low[0] + (high[0] - low[0]) * along,
        low[1] + (high[1] - low[1]) * along,
    ]
}

/// The colour shift each output lightness takes on average; nodes the photograph does not reach
/// take their nearest neighbours'.
fn axis(pairs: &[([f32; 3], [f32; 3])]) -> Vec<[f32; 2]> {
    let mut sum = [[0.0_f32; 2]; AXIS_NODES];
    let mut weight = [0.0_f32; AXIS_NODES];
    for (from, to) in pairs {
        let x = to[0].clamp(0.0, 1.0) * (AXIS_NODES - 1) as f32;
        let i = (x as usize).min(AXIS_NODES - 2);
        let t = x - i as f32;
        for (k, w) in [(i, 1.0 - t), (i + 1, t)] {
            if let (Some(s), Some(total)) = (sum.get_mut(k), weight.get_mut(k)) {
                s[0] += w * (to[1] - from[1]);
                s[1] += w * (to[2] - from[2]);
                *total += w;
            }
        }
    }
    let floor = 1e-3 * pairs.len() as f32;
    let known: Vec<(usize, [f32; 2])> = sum
        .iter()
        .zip(&weight)
        .enumerate()
        .filter(|(_, (_, &w))| w > floor.max(1e-6))
        .map(|(k, (s, &w))| (k, [s[0] / w, s[1] / w]))
        .collect();
    (0..AXIS_NODES)
        .map(|k| {
            let left = known.iter().rev().find(|(i, _)| *i <= k);
            let right = known.iter().find(|(i, _)| *i >= k);
            let value = match (left, right) {
                (Some(&(a, va)), Some(&(b, vb))) if b > a => {
                    let t = (k - a) as f32 / (b - a) as f32;
                    [va[0] + t * (vb[0] - va[0]), va[1] + t * (vb[1] - va[1])]
                }
                (Some(&(_, v)), _) | (None, Some(&(_, v))) => v,
                (None, None) => [0.0; 2],
            };
            value.map(|v| v.clamp(-MAX_CAST, MAX_CAST))
        })
        .collect()
}

/// The reference's exact colours on this photograph, as a fit the native checks then review.
pub(in crate::color_match) fn fit_exact(
    reference: &GradeStats,
    target: &[Sample],
    colours: &[[f32; 3]],
    skin: Option<super::super::exact::Skin<'_>>,
    cancel: &CancelToken,
) -> AuraResult<Fit> {
    let labs: Vec<[f32; 3]> = target.iter().map(|s| s.lab).collect();
    let before = distance(reference, &measure(&labs));
    let stride = (labs.len() / SAMPLES).max(1);
    let learned: Vec<[f32; 3]> = labs.iter().step_by(stride).copied().collect();
    if learned.len() < MIN_SAMPLES || colours.len() < MIN_SAMPLES {
        return Err(super::super::invalid(
            "This photograph or reference has too few pixels to copy its colours. No edit was saved.",
        ));
    }
    let transport = super::super::exact::Transport::learn(&learned, colours, skin, cancel)?;
    check_cancel_with(cancel)?;
    let pairs: Vec<([f32; 3], [f32; 3])> =
        learned.iter().map(|&x| (x, transport.apply(x))).collect();
    let mut grade = ReferenceGrade::identity();
    // Skin is carried by the transfer itself: to the reference's skin, or kept.
    grade.skin = 0.0;
    grade.tone = regularise_tone_within(
        (0..TONE_NODES)
            .map(|i| transport.tone_at(i as f32 / (TONE_NODES - 1) as f32))
            .collect(),
        EXACT_TONE_DEVIATION,
        EXACT_TONE_SLOPE,
    );
    grade.cast_out = axis(&pairs);
    let residuals: Vec<([f32; 2], [f32; 2])> = pairs
        .iter()
        .map(|(from, to)| {
            let shift = axis_value(&grade.cast_out, to[0]);
            let at = [from[1] + shift[0], from[2] + shift[1]];
            (at, [to[1] - at[0], to[2] - at[1]])
        })
        .collect();
    grade.palette = Some(PaletteMap {
        offset: [0.0; 2],
        deltas: field(&residuals),
    });
    let grade = grade.bounded();
    check_cancel_with(cancel)?;
    let (grade, stretch_limited_from) = limit_stretch_to(grade, target, MAX_EXACT_STRETCH);
    let after = distance(reference, &measure(&transformed(&grade, target)));
    Ok(Fit {
        grade,
        before,
        after,
        iterations: 1,
        content: 0.0,
        tone: None,
        stretch_limited_from,
    })
}

fn check_cancel_with(cancel: &CancelToken) -> AuraResult<()> {
    super::super::check_cancel(cancel)
}
