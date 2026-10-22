//! Practical repairs: dust and scratches, JPEG artefacts, banding, auto levels, colour cast,
//! uneven light, HDR structure and bokeh highlights. ADR-0132.
use super::{area, each, hash, mix, seed_of, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::DustScratches
            | Tool::JpegCleanup
            | Tool::Deband
            | Tool::AutoLevels
            | Tool::AutoNeutral
            | Tool::FlattenLight
            | Tool::HdrDetail
            | Tool::BokehHighlights
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::DustScratches => dust(rgb, frame, edit, coverage),
        Tool::JpegCleanup => deblock(rgb, frame, edit, coverage),
        Tool::Deband => deband(rgb, frame, edit, coverage),
        Tool::AutoLevels => levels(rgb, frame, edit, coverage),
        Tool::AutoNeutral => neutral(rgb, frame, edit, coverage),
        Tool::FlattenLight => flatten(rgb, frame, edit, coverage),
        Tool::HdrDetail => structure(rgb, frame, edit, coverage),
        _ => bokeh(rgb, frame, edit, coverage),
    }
}

/// Coverage-weighted samples of the selection, for statistics about it.
fn selected(rgb: &[f32], frame: Frame, coverage: &Coverage) -> Vec<([f32; 3], f32)> {
    let Some(rect) = area(coverage, frame, 0) else {
        return Vec::new();
    };
    // At most about a quarter of a million samples, on a fixed grid.
    let step = ((rect.len() / 250_000) as f32).sqrt().ceil().max(1.0) as usize;
    let mut out = Vec::new();
    for y in (rect.y0..rect.y0 + rect.h).step_by(step) {
        for x in (rect.x0..rect.x0 + rect.w).step_by(step) {
            let c = coverage.at(x, y, frame.width, frame.height);
            if c > 0.5 {
                let i = (y * frame.width + x) * 3;
                out.push(([rgb[i], rgb[i + 1], rgb[i + 2]], c));
            }
        }
    }
    out
}

fn dust(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let r = frame.radius_px(edit).clamp(1, 4) as isize;
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, r as usize + 1) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    // How far from the local median a speck must be, relative to it.
    let threshold = 0.04 + edit.tone.clamp(0.0, 1.0) * 0.4;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let mut lums = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
        for dy in -r..=r {
            for dx in -r..=r {
                let p = snapshot.at(x as isize + dx, y as isize + dy);
                lums.push((luma(p), p));
            }
        }
        lums.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (median_l, median) = lums[lums.len() / 2];
        let l = luma(old);
        if (l - median_l).abs() <= threshold * median_l.max(0.02) {
            return None;
        }
        Some(mix(old, median, k))
    });
}

fn deblock(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    const BLOCK: usize = 8;
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, 2) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    // A step larger than this, relative to the level, is an edge in the picture and stays.
    let limit = 0.03 + edit.tone.clamp(0.0, 1.0) * 0.12;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (xi, yi) = (x as isize, y as isize);
        let mut acc = old;
        let mut moved = false;
        // The pixel on each side of a vertical and a horizontal block boundary.
        for (axis, pos) in [(0, x % BLOCK), (1, y % BLOCK)] {
            let (before, after) = match pos {
                0 => (-1, 0),
                p if p == BLOCK - 1 => (0, 1),
                _ => continue,
            };
            let (a, b) = if axis == 0 {
                (snapshot.at(xi + before, yi), snapshot.at(xi + after, yi))
            } else {
                (snapshot.at(xi, yi + before), snapshot.at(xi, yi + after))
            };
            let (la, lb) = (luma(a), luma(b));
            if (la - lb).abs() > limit * la.max(lb).max(0.02) {
                continue;
            }
            let mean: [f32; 3] = std::array::from_fn(|c| (a[c] + b[c]) * 0.5);
            acc = mix(acc, mean, 0.5);
            moved = true;
        }
        moved.then(|| mix(old, acc, k))
    });
}

fn deband(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).max(2);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let planes = [0, 1, 2].map(|c| blur(&snapshot.plane(c), r.w, r.h, radius));
    let lum = snapshot.luma_plane();
    let mean = blur(&lum, r.w, r.h, radius);
    let sq = blur(
        &lum.iter().map(|v| v * v).collect::<Vec<_>>(),
        r.w,
        r.h,
        radius,
    );
    let seed = seed_of(edit);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        let level = mean[i].max(1e-4);
        // Only where the area is smooth: banding steps are tiny against the level.
        let spread = (sq[i] - mean[i] * mean[i]).max(0.0).sqrt() / level;
        let flat = 1.0 - smoothstep(0.01, 0.05, spread);
        if flat <= 0.0 {
            return None;
        }
        // A fixed half-level dither in the display encoding breaks the new steps up.
        let n = (hash(x as i64, y as i64, seed) - 0.5) / 255.0;
        let smooth: [f32; 3] = std::array::from_fn(|c| {
            let v = planes[c][i].max(0.0);
            (v.powf(1.0 / 2.2) + n).max(0.0).powf(2.2)
        });
        Some(mix(old, smooth, k * flat))
    });
}

fn levels(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let mut lums: Vec<f32> = selected(rgb, frame, coverage)
        .iter()
        .map(|(p, _)| luma(*p))
        .collect();
    if lums.len() < 16 {
        return;
    }
    lums.sort_by(f32::total_cmp);
    let at = |q: f32| lums[((lums.len() - 1) as f32 * q) as usize];
    // Measured in the display encoding, where "full range" is meant.
    let lo = at(0.005).max(0.0).powf(1.0 / 2.2);
    let hi = at(0.995).max(1e-4).powf(1.0 / 2.2);
    if hi - lo < 0.02 {
        return;
    }
    // At most three times the contrast, around the selection's own middle: stretching a nearly
    // uniform area (open water, a plain wall) to the full range turned it into a dark blob on
    // the first real photograph.
    let (lo, hi) = if hi - lo < 1.0 / 3.0 {
        let middle = (lo + hi) * 0.5;
        let half = 1.0 / 6.0;
        (middle - half, middle + half)
    } else {
        (lo, hi)
    };
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let l = luma(old).max(1e-6);
        let e = ((l.powf(1.0 / 2.2) - lo) / (hi - lo)).clamp(0.0, 1.0);
        let target = e.powf(2.2);
        Some(mix(old, old.map(|v| v * target / l), k))
    });
}

fn neutral(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let samples = selected(rgb, frame, coverage);
    let mut sum = [0.0_f64; 3];
    let mut total = 0.0_f64;
    for (p, c) in &samples {
        for k in 0..3 {
            sum[k] += f64::from(p[k] * c);
        }
        total += f64::from(*c);
    }
    if total <= 0.0 || sum.iter().any(|v| *v <= 1e-9) {
        return;
    }
    let mean = sum.map(|v| (v / total) as f32);
    let level = luma(mean);
    let gains = mean.map(|v| (level / v).clamp(0.5, 2.0));
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        (k > 0.0).then(|| mix(old, std::array::from_fn(|c| old[c] * gains[c]), k))
    });
}

fn flatten(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // The light field: luminance blurred at a third of the region's size, so a backdrop's
    // gradient is measured and the subject's detail is not.
    let radius = (rect.w.min(rect.h) / 6).max(4);
    let snapshot = Snapshot::take(rgb, frame, rect);
    let lum = snapshot.luma_plane();
    let field = blur(&lum, rect.w, rect.h, radius);
    let mean = {
        let samples = selected(rgb, frame, coverage);
        let n = samples.len().max(1) as f32;
        samples.iter().map(|(p, _)| luma(*p)).sum::<f32>() / n
    };
    if mean <= 1e-5 {
        return;
    }
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let f = field[snapshot.index(x, y)].max(1e-5);
        let gain = (mean / f).clamp(0.25, 4.0);
        Some(mix(old, old.map(|v| v * gain), k))
    });
}

fn structure(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = (frame.radius_px(edit) * 4).clamp(4, 96);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let log: Vec<f32> = snapshot
        .luma_plane()
        .iter()
        .map(|l| l.max(1e-5).ln())
        .collect();
    let base = blur(&log, r.w, r.h, radius);
    let gain = edit.texture.clamp(0.0, 2.0);
    let lift = edit.tone.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        let detail = log[i] - base[i];
        // Compress the base toward middle grey by `lift`, so shadows open as detail rises.
        let compressed = base[i] * (1.0 - 0.35 * lift) + (0.18_f32).ln() * 0.35 * lift;
        let target = (compressed + detail * (1.0 + gain)).exp();
        let l = log[i].exp();
        Some(mix(old, old.map(|v| v * target / l), k))
    });
}

fn bokeh(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let disc = (edit.radius * frame.short()).round().clamp(2.0, 60.0) as isize;
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, rect);
    let threshold = 0.4 + (1.0 - edit.tone.clamp(0.0, 1.0)) * 1.2;
    // Gather each block of half the disc radius into one source - at its brightest pixel, with
    // the light of the whole block - so a single bright point is never missed between samples.
    let step = (disc / 2).max(1) as usize;
    let mut glow = vec![[0.0_f32; 3]; rect.len()];
    let area_norm = 1.0 / (std::f32::consts::PI * (disc * disc) as f32);
    for by in (rect.y0..rect.y0 + rect.h).step_by(step) {
        for bx in (rect.x0..rect.x0 + rect.w).step_by(step) {
            let mut energy = [0.0_f32; 3];
            let mut brightest = (0.0_f32, bx, by);
            for y in by..(by + step).min(rect.y0 + rect.h) {
                for x in bx..(bx + step).min(rect.x0 + rect.w) {
                    let p = snapshot.at(x as isize, y as isize);
                    let l = luma(p);
                    let excess = l - threshold;
                    if excess <= 0.0 || coverage.at(x, y, frame.width, frame.height) <= 0.0 {
                        continue;
                    }
                    for c in 0..3 {
                        energy[c] += p[c] * excess / l.max(1e-4) * area_norm * 6.0;
                    }
                    if l > brightest.0 {
                        brightest = (l, x, y);
                    }
                }
            }
            if brightest.0 <= 0.0 {
                continue;
            }
            let (x, y) = (brightest.1, brightest.2);
            for dy in -disc..=disc {
                for dx in -disc..=disc {
                    let d = ((dx * dx + dy * dy) as f32).sqrt() / disc as f32;
                    if d > 1.0 {
                        continue;
                    }
                    // A brighter rim, as real out-of-focus highlights have.
                    let shape = smoothstep(1.0, 0.92, d) * (0.75 + 0.25 * smoothstep(0.6, 0.95, d));
                    let (tx, ty) = (x as isize + dx, y as isize + dy);
                    if tx < rect.x0 as isize
                        || ty < rect.y0 as isize
                        || tx >= (rect.x0 + rect.w) as isize
                        || ty >= (rect.y0 + rect.h) as isize
                    {
                        continue;
                    }
                    let g = &mut glow[snapshot.index(tx as usize, ty as usize)];
                    for c in 0..3 {
                        g[c] += energy[c] * shape;
                    }
                }
            }
        }
    }
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let g = glow[snapshot.index(x, y)];
        (g.iter().any(|v| *v > 0.0)).then(|| std::array::from_fn(|c| old[c] + g[c] * k))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn dust_removes_a_speck_and_keeps_a_wide_edge() {
        let (w, h) = (40, 40);
        let base = fill(w, h, |x, y| {
            if x == 10 && y == 10 {
                [0.9; 3]
            } else if x >= 25 {
                [0.6; 3]
            } else {
                [0.2; 3]
            }
        });
        let mut e = edit(Tool::DustScratches, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.05;
        e.tone = 0.1;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!((px(&out, w, 10, 10)[0] - 0.2).abs() < 0.01);
        assert_eq!(px(&out, w, 26, 20), px(&base, w, 26, 20));
    }

    #[test]
    fn jpeg_cleanup_softens_a_small_block_step_and_keeps_a_real_edge() {
        let (w, h) = (32, 16);
        // Blocks 8 wide: a small step at x = 8 (an artefact) and a large one at x = 16.
        let base = fill(w, h, |x, _| {
            if x < 8 {
                [0.30; 3]
            } else if x < 16 {
                [0.32; 3]
            } else {
                [0.8; 3]
            }
        });
        let e = edit(Tool::JpegCleanup, [0.5, 0.5, 0.6, 0.6]);
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(px(&out, w, 8, 8)[0] < 0.32 && px(&out, w, 7, 8)[0] > 0.30);
        assert_eq!(px(&out, w, 16, 8), px(&base, w, 16, 8));
    }

    #[test]
    fn auto_levels_stretches_a_flat_selection() {
        let (w, h) = (30, 30);
        let base = fill(w, h, |x, _| [0.08 + x as f32 * 0.004; 3]);
        let mut e = edit(Tool::AutoLevels, [0.5, 0.5, 0.6, 0.6]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let span = |rgb: &[f32]| luma(px(rgb, w, 28, 15)) - luma(px(rgb, w, 1, 15));
        assert!(span(&out) > span(&base) * 2.0);
        // ... and never more than three times the contrast of a flat area.
        let flat = fill(w, h, |x, _| [0.18 + x as f32 * 0.002; 3]);
        let mut flattened = flat.clone();
        run(&mut flattened, w, h, &e);
        let encoded = |rgb: &[f32], x: usize| luma(px(rgb, w, x, 15)).powf(1.0 / 2.2);
        let gain = (encoded(&flattened, 28) - encoded(&flattened, 1))
            / (encoded(&flat, 28) - encoded(&flat, 1));
        assert!(gain > 2.0 && gain <= 3.1, "{gain}");
    }

    #[test]
    fn auto_neutral_removes_the_selection_s_cast() {
        let (w, h) = (20, 20);
        let base = fill(w, h, |x, _| [0.5 + x as f32 * 0.005, 0.4, 0.25]);
        let mut e = edit(Tool::AutoNeutral, [0.5, 0.5, 0.6, 0.6]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let p = px(&out, w, 10, 10);
        assert!((p[0] - p[2]).abs() < (0.55 - 0.25) * 0.15, "{p:?}");
    }

    #[test]
    fn flatten_light_evens_a_backdrop_gradient() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |x, _| [0.1 + x as f32 * 0.01; 3]);
        let mut e = edit(Tool::FlattenLight, [0.5, 0.5, 0.6, 0.6]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let spread = |rgb: &[f32]| px(rgb, w, 50, 30)[0] - px(rgb, w, 10, 30)[0];
        assert!(spread(&out) < spread(&base) * 0.5);
    }

    #[test]
    fn hdr_detail_raises_local_contrast() {
        let (w, h) = (80, 80);
        let base = checker(w, h, 8);
        let mut e = edit(Tool::HdrDetail, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.05;
        e.texture = 1.0;
        e.tone = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let c = |rgb: &[f32]| luma(px(rgb, w, 36, 36)) / luma(px(rgb, w, 44, 36)).max(1e-4);
        assert!(c(&out) > c(&base) * 1.2 || 1.0 / c(&out) > 1.0 / c(&base) * 1.2);
    }

    #[test]
    fn bokeh_turns_a_point_light_into_a_disc() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |x, y| {
            if x == 30 && y == 30 {
                [8.0; 3]
            } else {
                [0.02; 3]
            }
        });
        let mut e = edit(Tool::BokehHighlights, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.12;
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        // Lit six pixels away in every direction, nothing far away.
        for (x, y) in [(36, 30), (24, 30), (30, 36), (30, 24)] {
            assert!(px(&out, w, x, y)[0] > 0.03, "({x},{y})");
        }
        assert!(px(&out, w, 50, 50)[0] < 0.021);
    }

    #[test]
    fn deband_smooths_a_stepped_sky() {
        let (w, h) = (80, 20);
        let base = fill(w, h, |x, _| [0.3 + (x / 10) as f32 * 0.004, 0.4, 0.6]);
        let mut e = edit(Tool::Deband, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.2;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        // Averaged over three columns, so the deliberate half-level dither does not count.
        let side = |rgb: &[f32], xs: std::ops::Range<usize>| {
            xs.clone().map(|x| px(rgb, w, x, 10)[0]).sum::<f32>() / xs.len() as f32
        };
        let step = |rgb: &[f32]| (side(rgb, 40..43) - side(rgb, 37..40)).abs();
        assert!(
            step(&out) < step(&base) * 0.8,
            "{} vs {}",
            step(&out),
            step(&base)
        );
    }
}
