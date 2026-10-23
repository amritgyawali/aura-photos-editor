//! Body, attire and background repairs placed by hand. ADR-0150.
//!
//! Each is measured on the selection a photographer drew: a tan line is the boundary between
//! the selection's own two levels of skin, a cast shadow is its darker cluster and is relit by
//! the ratio to its lit one, body hair is what a closing fills, cellulite is the band of detail
//! at the size of a dimple. Nothing outside the selection moves.
use super::{area, each, from_opponent, opponent, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect};
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::BodyHairSoften
            | Tool::Cellulite
            | Tool::TanLine
            | Tool::HandRejuvenate
            | Tool::KnuckleEven
            | Tool::LensReflection
            | Tool::ShadowRemove
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::BodyHairSoften => body_hair(rgb, frame, edit, coverage),
        Tool::Cellulite => cellulite(rgb, frame, edit, coverage),
        Tool::TanLine => two_levels(rgb, frame, edit, coverage, Levels::TanLine),
        Tool::HandRejuvenate => hands(rgb, frame, edit, coverage),
        Tool::KnuckleEven => knuckles(rgb, frame, edit, coverage),
        Tool::LensReflection => lens_reflection(rgb, frame, edit, coverage),
        _ => two_levels(rgb, frame, edit, coverage, Levels::Shadow),
    }
}

/// The selection's rectangle with a margin for the filters, and its luminance.
fn prepare(
    rgb: &[f32],
    frame: Frame,
    coverage: &Coverage,
    margin: usize,
) -> Option<(Rect, Rect, Snapshot)> {
    let inner = area(coverage, frame, 0)?;
    let outer = area(coverage, frame, margin)?;
    let snap = Snapshot::take(rgb, frame, outer);
    Some((inner, outer, snap))
}

fn px_radius(frame: Frame, share: f32, lo: usize, hi: usize) -> usize {
    ((share * frame.short()).round() as usize).clamp(lo, hi)
}

/// How much each pixel's neighbourhood is like the selection's own skin: its colour, per unit of
/// luminance, against the selection's median. A selection drawn over an arm and a sleeve works on
/// the arm, and a dark line in a sari's weave is never taken for a hair. `loose` widens the
/// tolerance, for a tan line whose two sides differ in colour as well as light.
fn skin_gate(
    snap: &Snapshot,
    outer: Rect,
    median: [f32; 2],
    radius: usize,
    loose: f32,
) -> Vec<f32> {
    let planes: Vec<Vec<f32>> = (0..3)
        .map(|c| blur(&snap.plane(c), outer.w, outer.h, radius.max(1)))
        .collect();
    (0..outer.w * outer.h)
        .map(|i| {
            let o = opponent([planes[0][i], planes[1][i], planes[2][i]]);
            if o[0] <= 1e-5 {
                return 0.0;
            }
            let d = (o[1] / o[0] - median[0]).hypot(o[2] / o[0] - median[1]);
            1.0 - smoothstep(0.06 * loose, 0.16 * loose, d)
        })
        .collect()
}

/// A running maximum (or minimum) over `r` either side, along rows then columns.
fn extreme(plane: &[f32], w: usize, h: usize, r: usize, max: bool) -> Vec<f32> {
    let pick = |a: f32, b: f32| if max { a.max(b) } else { a.min(b) };
    let mut rows = vec![0.0_f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1));
            let mut v = plane[y * w + x0];
            for xx in x0..=x1 {
                v = pick(v, plane[y * w + xx]);
            }
            rows[y * w + x] = v;
        }
    }
    let mut out = vec![0.0_f32; w * h];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1));
        for x in 0..w {
            let mut v = rows[y0 * w + x];
            for yy in y0..=y1 {
                v = pick(v, rows[yy * w + x]);
            }
            out[y * w + x] = v;
        }
    }
    out
}

/// Fine hair on arms or legs: thin structures darker than what a closing of the skin fills them
/// with, lifted to that level.
fn body_hair(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let r = frame.radius_px(edit).clamp(1, 6);
    let Some((inner, outer, snap)) = prepare(rgb, frame, coverage, r + 1) else {
        return;
    };
    let l = snap.luma_plane();
    let closed = extreme(
        &extreme(&l, outer.w, outer.h, r, true),
        outer.w,
        outer.h,
        r,
        false,
    );
    let threshold = 0.03 + (1.0 - edit.tone.clamp(0.0, 1.0)) * 0.1;
    let Some(median) = median_chroma(rgb, frame, coverage, inner) else {
        return;
    };
    let skin = skin_gate(&snap, outer, median, r * 2, 1.0);
    each(rgb, frame, inner, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snap.index(x, y);
        let depth = (closed[i] - l[i]) / closed[i].max(1e-5);
        let hair = smoothstep(threshold, threshold * 2.5, depth);
        let t = hair * k * skin[i];
        (t > 0.0).then(|| with_luma(old, l[i] + (closed[i] - l[i]) * t))
    });
}

/// Cellulite: the band of light and shade at the size of a dimple taken out, finer texture and
/// the body's own shape kept. A shade too deep to be a dimple - a fold, a seam - is left.
fn cellulite(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let r = frame.radius_px(edit).clamp(3, 120) as f32;
    let margin = (r * 2.0) as usize + 2;
    let Some((inner, outer, snap)) = prepare(rgb, frame, coverage, margin) else {
        return;
    };
    let l = snap.luma_plane();
    let fine = blur(&l, outer.w, outer.h, (r / 3.0).round().max(1.0) as usize);
    let coarse = blur(&l, outer.w, outer.h, (r * 1.5).round().max(2.0) as usize);
    let Some(median) = median_chroma(rgb, frame, coverage, inner) else {
        return;
    };
    let skin = skin_gate(
        &snap,
        outer,
        median,
        (r / 3.0).round().max(1.0) as usize,
        1.0,
    );
    each(rgb, frame, inner, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snap.index(x, y);
        let band = fine[i] - coarse[i];
        let shallow = 1.0 - smoothstep(0.15, 0.35, band.abs() / coarse[i].max(1e-5));
        Some(with_luma(old, l[i] - band * 0.85 * k * shallow * skin[i]))
    });
}

/// Hands: fine wrinkles softened, redder knuckles and bluer veins moved toward the hand's own
/// median colour.
fn hands(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let r = frame.radius_px(edit).clamp(1, 40) as f32;
    let margin = (r * 4.0) as usize + 2;
    let Some((inner, outer, snap)) = prepare(rgb, frame, coverage, margin) else {
        return;
    };
    let l = snap.luma_plane();
    let fine = blur(&l, outer.w, outer.h, r.round() as usize);
    let coarse = blur(&l, outer.w, outer.h, (r * 4.0).round() as usize);
    let Some(median) = median_chroma(rgb, frame, coverage, inner) else {
        return;
    };
    let even = edit.tone.clamp(0.0, 1.0);
    let skin = skin_gate(&snap, outer, median, r.round().max(1.0) as usize, 1.2);
    each(rgb, frame, inner, |x, y, old| {
        let i = snap.index(x, y);
        let k = weight(edit, coverage, frame, x, y) * skin[i];
        if k <= 0.0 {
            return None;
        }
        let band = fine[i] - coarse[i];
        let wrinkle = if band < 0.0 { band * 0.7 } else { band * 0.3 };
        let y2 = (l[i] - wrinkle * k).max(0.0);
        let o = opponent(old);
        let (cb, cr) = (o[1] / o[0].max(1e-5), o[2] / o[0].max(1e-5));
        // Only what is redder, or bluer, than the hand itself.
        let red = smoothstep(0.0, 0.06, cr - median[1]);
        let blue = smoothstep(0.0, 0.05, cb - median[0]);
        let t = k * even * red.max(blue);
        let cb2 = cb + (median[0] - cb) * t;
        let cr2 = cr + (median[1] - cr) * t;
        Some(from_opponent([y2, cb2 * y2, cr2 * y2]))
    });
}

/// Darker knuckles and elbows lifted toward the selection's own median level, and their
/// colour toward its median colour by as much as they were darker.
fn knuckles(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let r = frame.radius_px(edit).clamp(2, 120) as f32;
    let margin = (r * 3.0) as usize + 2;
    let Some((inner, outer, snap)) = prepare(rgb, frame, coverage, margin) else {
        return;
    };
    let l = snap.luma_plane();
    let low = blur(&l, outer.w, outer.h, (r * 3.0).round() as usize);
    let Some(median) = median_chroma(rgb, frame, coverage, inner) else {
        return;
    };
    let mut levels: Vec<f32> = selected_points(frame, coverage, inner)
        .map(|(x, y)| low[snap.index(x, y)])
        .collect();
    if levels.len() < 8 {
        return;
    }
    levels.sort_by(f32::total_cmp);
    let level = levels[levels.len() / 2].max(1e-5);
    // Darker knuckles are browner than the hand, so the gate is a little looser.
    let skin = skin_gate(&snap, outer, median, r.round().max(1.0) as usize, 1.5);
    each(rgb, frame, inner, |x, y, old| {
        let i = snap.index(x, y);
        let k = weight(edit, coverage, frame, x, y) * skin[i];
        if k <= 0.0 {
            return None;
        }
        let deficit = ((level * 0.97 - low[i]) / level).max(0.0);
        if deficit <= 0.0 {
            return None;
        }
        let gain = 1.0 + (deficit * 1.6).min(0.6) * k;
        let o = opponent(old);
        let y2 = o[0] * gain;
        let t = (deficit * 1.5 * k).min(1.0);
        let (cb, cr) = (o[1] / o[0].max(1e-5), o[2] / o[0].max(1e-5));
        Some(from_opponent([
            y2,
            (cb + (median[0] - cb) * t) * y2,
            (cr + (median[1] - cr) * t) * y2,
        ]))
    });
}

/// The coloured reflection of a lens coating - green, magenta, blue - taken back to the colour
/// around it, and its extra brightness down by `tone`.
fn lens_reflection(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let r = frame.radius_px(edit).clamp(2, 200) as f32;
    let margin = (r * 3.0) as usize + 2;
    let Some((inner, outer, snap)) = prepare(rgb, frame, coverage, margin) else {
        return;
    };
    let reach = (r * 3.0).round() as usize;
    let l = snap.luma_plane();
    let around_l = blur(&l, outer.w, outer.h, reach);
    let chans: Vec<Vec<f32>> = (0..3)
        .map(|c| blur(&snap.plane(c), outer.w, outer.h, reach))
        .collect();
    let dim = edit.tone.clamp(0.0, 1.0);
    each(rgb, frame, inner, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snap.index(x, y);
        let around = opponent([chans[0][i], chans[1][i], chans[2][i]]);
        let (sb, sr) = (
            around[1] / around[0].max(1e-5),
            around[2] / around[0].max(1e-5),
        );
        let o = opponent(old);
        let (cb, cr) = (o[1] / o[0].max(1e-5), o[2] / o[0].max(1e-5));
        let off = (cb - sb).hypot(cr - sr);
        let t = smoothstep(0.03, 0.12, off) * k;
        if t <= 0.0 {
            return None;
        }
        let excess = (l[i] - around_l[i]).max(0.0);
        let y2 = l[i] - excess * t * dim;
        Some(from_opponent([
            y2,
            (cb + (sb - cb) * t) * y2,
            (cr + (sr - cr) * t) * y2,
        ]))
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Levels {
    /// The lighter skin brought to the darker (`tone` under one half) or the darker to the
    /// lighter.
    TanLine,
    /// The darker side relit to the lighter.
    Shadow,
}

/// The selection's two levels of light, found by two-means on its smoothed luminance, and the
/// colour ratio between them applied across a soft boundary.
fn two_levels(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage, mode: Levels) {
    let smooth = px_radius(frame, 0.004, 1, 60);
    let edge = match mode {
        Levels::TanLine => px_radius(frame, edit.radius.max(0.002), 1, 120),
        Levels::Shadow => 1 + (edit.tone.clamp(0.0, 1.0) * 0.03 * frame.short()).round() as usize,
    };
    let margin = smooth * 3 + edge * 3 + 2;
    let Some((inner, outer, snap)) = prepare(rgb, frame, coverage, margin) else {
        return;
    };
    let l = snap.luma_plane();
    let lb = blur(&l, outer.w, outer.h, smooth);
    let points: Vec<(usize, usize)> = selected_points(frame, coverage, inner).collect();
    let values: Vec<f32> = points.iter().map(|&(x, y)| lb[snap.index(x, y)]).collect();
    let Some((dark, light)) = two_means(&values) else {
        return;
    };
    if light < dark * 1.05 {
        return;
    }
    let mid = (dark + light) * 0.5;
    let span = (light - dark) * 0.25;
    // Each pixel's membership of the lighter level, then softened into a boundary.
    let lighter: Vec<f32> = lb
        .iter()
        .map(|v| smoothstep(mid - span, mid + span, *v))
        .collect();
    let lighter = blur(&lighter, outer.w, outer.h, edge);
    let mean = |want_light: bool| -> Option<[f32; 3]> {
        let mut sum = [0.0_f64; 3];
        let mut n = 0.0_f64;
        for &(x, y) in &points {
            let i = snap.index(x, y);
            let is_light = lb[i] > mid;
            if is_light == want_light {
                let p = snap.at(x as isize, y as isize);
                for c in 0..3 {
                    sum[c] += f64::from(p[c]);
                }
                n += 1.0;
            }
        }
        (n >= 4.0).then(|| sum.map(|s| (s / n) as f32))
    };
    let (Some(dark_c), Some(light_c)) = (mean(false), mean(true)) else {
        return;
    };
    // The two levels must be one surface in two lights: a shadow is a little bluer than the lit
    // side, never a different material. Two materials are refused rather than "relit".
    let colour = |p: [f32; 3]| {
        let o = opponent(p);
        [o[1] / o[0].max(1e-5), o[2] / o[0].max(1e-5)]
    };
    let (cd, cl) = (colour(dark_c), colour(light_c));
    // Open shade is lit by the sky, so a shadow may be bluer than its lit side: the blue-yellow
    // axis counts half. A tan is redder than the skin beside it, so a tan line is looser still.
    let apart = ((cd[0] - cl[0]) * 0.5).hypot(cd[1] - cl[1]);
    let (lo, hi) = match mode {
        Levels::Shadow => (0.18, 0.35),
        Levels::TanLine => (0.25, 0.5),
    };
    let same = 1.0 - smoothstep(lo, hi, apart);
    if same <= 0.0 {
        return;
    }
    let ratio = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|c| (a[c] / b[c].max(1e-5)).clamp(0.25, 4.0))
    };
    let darken_light = mode == Levels::TanLine && edit.tone < 0.5;
    let (gain, on_light) = if darken_light {
        (ratio(dark_c, light_c), true)
    } else {
        (ratio(light_c, dark_c), false)
    };
    each(rgb, frame, inner, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let m = lighter[snap.index(x, y)];
        let member = if on_light { m } else { 1.0 - m };
        let t = member * k * same;
        (t > 0.0).then(|| std::array::from_fn(|c| old[c] * (1.0 + (gain[c] - 1.0) * t)))
    });
}

/// Two-means on a list: the darker and lighter centres, or `None` with too few values.
fn two_means(values: &[f32]) -> Option<(f32, f32)> {
    if values.len() < 8 {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut a = sorted[sorted.len() / 4];
    let mut b = sorted[sorted.len() * 3 / 4];
    for _ in 0..12 {
        let mid = (a + b) * 0.5;
        let (mut sa, mut na, mut sb, mut nb) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        for v in &sorted {
            if *v <= mid {
                sa += f64::from(*v);
                na += 1.0;
            } else {
                sb += f64::from(*v);
                nb += 1.0;
            }
        }
        if na < 1.0 || nb < 1.0 {
            return None;
        }
        a = (sa / na) as f32;
        b = (sb / nb) as f32;
    }
    Some((a, b))
}

/// Points of the selection on a grid of at most about sixty thousand.
fn selected_points(
    frame: Frame,
    coverage: &Coverage,
    rect: Rect,
) -> impl Iterator<Item = (usize, usize)> + '_ {
    let step = ((rect.len() / 60_000) as f32).sqrt().ceil().max(1.0) as usize;
    (rect.y0..rect.y0 + rect.h)
        .step_by(step)
        .flat_map(move |y| {
            (rect.x0..rect.x0 + rect.w)
                .step_by(step)
                .map(move |x| (x, y))
        })
        .filter(move |&(x, y)| coverage.at(x, y, frame.width, frame.height) > 0.5)
}

/// The selection's median colour, as `(cb, cr)` per unit of luminance.
fn median_chroma(rgb: &[f32], frame: Frame, coverage: &Coverage, rect: Rect) -> Option<[f32; 2]> {
    let (mut cbs, mut crs): (Vec<f32>, Vec<f32>) = selected_points(frame, coverage, rect)
        .filter_map(|(x, y)| {
            let i = (y * frame.width + x) * 3;
            let o = opponent([rgb[i], rgb[i + 1], rgb[i + 2]]);
            (o[0] > 1e-4).then(|| (o[1] / o[0], o[2] / o[0]))
        })
        .unzip();
    if cbs.len() < 8 {
        return None;
    }
    cbs.sort_by(f32::total_cmp);
    crs.sort_by(f32::total_cmp);
    Some([cbs[cbs.len() / 2], crs[crs.len() / 2]])
}

fn with_luma(p: [f32; 3], target: f32) -> [f32; 3] {
    let l = luma(p);
    if l <= 1e-6 {
        return [target.max(0.0); 3];
    }
    p.map(|v| v * target.max(0.0) / l)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{edit, fill, px, run};
    use super::*;

    #[test]
    fn a_cast_shadow_is_relit_to_the_level_around_it() {
        let (w, h) = (80, 60);
        // A wall at 0.5, with a bluish shadow over its left half.
        let before = fill(w, h, |x, _| {
            if x < 40 {
                [0.18, 0.2, 0.26]
            } else {
                [0.5, 0.5, 0.5]
            }
        });
        let mut after = before.clone();
        let e = edit(Tool::ShadowRemove, [0.5, 0.5, 0.6, 0.6]);
        run(&mut after, w, h, &e);
        let p = px(&after, w, 30, 30);
        assert!(luma(p) > 0.42, "{p:?}");
        assert!(
            (p[2] - p[0]).abs() < 0.05,
            "the shadow's blue goes with it: {p:?}"
        );
        let lit = px(&after, w, 60, 30);
        assert!(lit.iter().all(|v| (v - 0.5).abs() < 1e-4), "{lit:?}");
    }

    #[test]
    fn a_tan_line_is_evened_in_the_direction_asked() {
        let (w, h) = (80, 60);
        let before = fill(w, h, |_, y| {
            if y < 30 {
                [0.6, 0.42, 0.32]
            } else {
                [0.4, 0.26, 0.18]
            }
        });
        for (tone, darker) in [(0.2, true), (0.8, false)] {
            let mut after = before.clone();
            let mut e = edit(Tool::TanLine, [0.5, 0.5, 0.6, 0.6]);
            e.tone = tone;
            run(&mut after, w, h, &e);
            let (top, bottom) = (luma(px(&after, w, 40, 15)), luma(px(&after, w, 40, 45)));
            assert!((top - bottom).abs() < 0.05, "{tone}: {top} {bottom}");
            if darker {
                assert!(top < luma(px(&before, w, 40, 15)) * 0.8);
            } else {
                assert!(bottom > luma(px(&before, w, 40, 45)) * 1.2);
            }
        }
    }

    #[test]
    fn fine_dark_hairs_are_lifted_and_broad_shade_is_not() {
        let (w, h) = (60, 40);
        let before = fill(w, h, |x, y| {
            let hair = x % 9 == 0 && y > 5;
            let shade = x > 45;
            let base = if shade { 0.25 } else { 0.45 };
            if hair {
                [base * 0.4, base * 0.35, base * 0.3]
            } else {
                [base, base * 0.75, base * 0.6]
            }
        });
        let mut after = before.clone();
        let mut e = edit(Tool::BodyHairSoften, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.04;
        e.tone = 0.6;
        run(&mut after, w, h, &e);
        assert!(luma(px(&after, w, 27, 20)) > luma(px(&before, w, 27, 20)) * 1.6);
        let broad = (luma(px(&after, w, 50, 20)) - luma(px(&before, w, 50, 20))).abs();
        assert!(broad < 0.01, "{broad}");
    }

    #[test]
    fn a_green_coating_reflection_takes_the_colour_around_it() {
        let (w, h) = (60, 60);
        let before = fill(w, h, |x, y| {
            let spot = (x as f32 - 30.0).hypot(y as f32 - 30.0) < 6.0;
            if spot {
                [0.3, 0.6, 0.35]
            } else {
                [0.45, 0.32, 0.25]
            }
        });
        let mut after = before.clone();
        let mut e = edit(Tool::LensReflection, [0.5, 0.5, 0.3, 0.3]);
        e.radius = 0.08;
        run(&mut after, w, h, &e);
        let p = px(&after, w, 30, 30);
        assert!(p[0] > p[1], "{p:?}");
    }
}
