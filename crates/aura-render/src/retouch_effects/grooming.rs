//! Grooming and finish: lash darken, hair fill and matched grain. ADR-0135.
use super::{area, each, hash, mix, seed_of, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(tool, Tool::LashDarken | Tool::HairFill | Tool::MatchGrain)
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::LashDarken => lashes(rgb, frame, edit, coverage),
        Tool::HairFill => fill(rgb, frame, edit, coverage),
        _ => grain(rgb, frame, edit, coverage),
    }
}

fn lashes(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).clamp(1, 8);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let lum = snapshot.luma_plane();
    let around = blur(&lum, r.w, r.h, radius);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        // A stroke is darker than what surrounds it; the darker, the more it is deepened.
        let depth = (around[i] - lum[i]) / around[i].max(1e-4);
        let stroke = smoothstep(0.05, 0.35, depth);
        (stroke > 0.0).then(|| old.map(|v| v * (1.0 - 0.6 * stroke * k)))
    });
}

fn fill(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // The hair's own colour: the mean of the darkest third of the selection.
    let mut samples: Vec<[f32; 3]> = Vec::new();
    for y in rect.y0..rect.y0 + rect.h {
        for x in rect.x0..rect.x0 + rect.w {
            if coverage.at(x, y, frame.width, frame.height) > 0.5 {
                let i = (y * frame.width + x) * 3;
                samples.push([rgb[i], rgb[i + 1], rgb[i + 2]]);
            }
        }
    }
    if samples.len() < 9 {
        return;
    }
    samples.sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
    let dark = &samples[..samples.len() / 3];
    let hair = [0, 1, 2].map(|c| dark.iter().map(|p| p[c]).sum::<f32>() / dark.len() as f32);
    let hair_l = luma(hair).max(1e-5);
    let gap_top = luma(samples[samples.len() * 2 / 3]);
    let fullness = 0.3 + edit.tone.clamp(0.0, 1.0) * 0.6;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        // A gap is lighter than the hair; the lighter, the more it is filled.
        let l = luma(old);
        let gap = smoothstep(hair_l, gap_top.max(hair_l * 1.2), l);
        (gap > 0.0).then(|| mix(old, hair, gap * fullness * k))
    });
}

fn grain(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // The photograph's own fine noise: the median absolute difference between a pixel and its
    // neighbours' mean, on a sparse grid over the whole frame, in the display encoding.
    let step = ((frame.width * frame.height / 40_000) as f32)
        .sqrt()
        .ceil()
        .max(1.0) as usize;
    let mut residuals = Vec::new();
    let enc = |x: usize, y: usize| -> f32 {
        let i = (y * frame.width + x) * 3;
        luma([rgb[i], rgb[i + 1], rgb[i + 2]])
            .max(0.0)
            .powf(1.0 / 2.2)
    };
    for y in (1..frame.height.saturating_sub(1)).step_by(step) {
        for x in (1..frame.width.saturating_sub(1)).step_by(step) {
            let mean = (enc(x - 1, y) + enc(x + 1, y) + enc(x, y - 1) + enc(x, y + 1)) * 0.25;
            residuals.push((enc(x, y) - mean).abs());
        }
    }
    if residuals.is_empty() {
        return;
    }
    residuals.sort_by(f32::total_cmp);
    // A median of absolute differences of a Gaussian is 0.674 sigma; the neighbour mean adds
    // its own small share, which the factor below leaves in.
    let sigma = (residuals[residuals.len() / 2] / 0.674).clamp(0.002, 0.05);
    let scale = sigma * (0.5 + edit.tone.clamp(0.0, 1.0));
    let seed = seed_of(edit);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        // Roughly Gaussian: the sum of three uniform draws, centred.
        let n = (hash(x as i64, y as i64, seed)
            + hash(x as i64, y as i64, seed ^ 0x5)
            + hash(x as i64, y as i64, seed ^ 0xa)
            - 1.5)
            * 2.0;
        let l = luma(old).max(1e-5);
        let e = l.powf(1.0 / 2.2);
        let target = (e + n * scale * k).clamp(0.0, 1.0).powf(2.2);
        Some(old.map(|v| v * target / l))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn lash_darken_deepens_thin_dark_lines_and_leaves_the_lid() {
        let (w, h) = (40, 20);
        let base = fill(w, h, |x, _| {
            if x % 6 == 0 {
                [0.1; 3]
            } else {
                [0.5, 0.4, 0.35]
            }
        });
        let mut e = edit(Tool::LashDarken, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.1;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 18, 10)) < luma(px(&base, w, 18, 10)) * 0.8);
        assert_eq!(px(&out, w, 21, 10), px(&base, w, 21, 10));
    }

    #[test]
    fn hair_fill_darkens_the_gaps_toward_the_hair() {
        let (w, h) = (40, 20);
        let base = fill(w, h, |x, _| {
            if x % 3 == 0 {
                [0.6, 0.45, 0.4]
            } else {
                [0.08, 0.06, 0.05]
            }
        });
        let mut e = edit(Tool::HairFill, [0.5, 0.5, 0.6, 0.6]);
        e.tone = 1.0;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 21, 10)) < luma(px(&base, w, 21, 10)) * 0.5);
        assert_eq!(px(&out, w, 20, 10), px(&base, w, 20, 10));
    }

    #[test]
    fn matched_grain_is_as_strong_as_the_photograph_s_own() {
        let (w, h) = (80, 80);
        // A noisy frame with a perfectly smooth patch in the middle - the over-smoothed skin.
        let base = fill(w, h, |x, y| {
            let n = if (30..50).contains(&x) && (30..50).contains(&y) {
                0.0
            } else {
                (super::super::hash(x as i64, y as i64, 9) - 0.5) * 0.06
            };
            [0.3 + n; 3]
        });
        let mut e = edit(Tool::MatchGrain, [0.5, 0.5, 0.12, 0.12]);
        e.tone = 0.5;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let spread = |rgb: &[f32], x0: usize| {
            let v: Vec<f32> = (x0..x0 + 6)
                .flat_map(|x| (37..43).map(move |y| (x, y)))
                .map(|(x, y)| px(rgb, w, x, y)[0])
                .collect();
            let m = v.iter().sum::<f32>() / v.len() as f32;
            (v.iter().map(|a| (a - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
        };
        let noisy = spread(&base, 5);
        let grained = spread(&out, 37);
        assert!(
            grained > noisy * 0.3 && grained < noisy * 3.0,
            "grain {grained} vs photo {noisy}"
        );
    }
}
