//! Optics and distortion: iris blur, echo, ripple, frosted glass, pixel sort and VHS. ADR-0135.
use super::{area, each, hash, mix, opponent, seed_of, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect};
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::IrisBlur
            | Tool::EchoDouble
            | Tool::Ripple
            | Tool::FrostedGlass
            | Tool::PixelSort
            | Tool::VhsRetro
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::IrisBlur => iris(rgb, frame, edit, coverage),
        Tool::PixelSort => sort(rgb, frame, edit, coverage),
        Tool::VhsRetro => vhs(rgb, frame, edit, coverage),
        _ => displace(rgb, frame, edit, coverage),
    }
}

fn iris(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).max(2);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let soft = [0, 1, 2].map(|c| blur(&snapshot.plane(c), r.w, r.h, radius));
    let c = frame.centre(edit);
    let reach = frame.reach(edit);
    let focus = 0.1 + edit.tone.clamp(0.0, 1.0) * 0.6;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let d = ((x as f32 + 0.5 - c[0]) / reach[0]).hypot((y as f32 + 0.5 - c[1]) / reach[1]);
        let w = smoothstep(focus, (focus + 0.4).min(1.0), d);
        let i = snapshot.index(x, y);
        Some(mix(old, [soft[0][i], soft[1][i], soft[2][i]], w * k))
    });
}

/// Echo, ripple and frosted glass: each pixel reads the frame somewhere nearby.
fn displace(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let snapshot = Snapshot::take(
        rgb,
        frame,
        Rect {
            x0: 0,
            y0: 0,
            w: frame.width,
            h: frame.height,
        },
    );
    let c = frame.centre(edit);
    let reach = frame.reach(edit);
    let px_radius = (edit.radius * frame.short()).max(1.0);
    let wavelength = (px_radius * 4.0).max(4.0);
    let seed = seed_of(edit);
    let strength = edit.amount.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, old| {
        let cover = coverage.at(x, y, frame.width, frame.height);
        if cover <= 0.0 {
            return None;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let d = ((px - c[0]) / reach[0]).hypot((py - c[1]) / reach[1]);
        let falloff = if d >= 1.0 {
            0.0
        } else {
            (1.0 - d * d).powi(2) * cover
        };
        match edit.tool {
            Tool::EchoDouble => {
                let ghost =
                    snapshot.bilinear(px - edit.warmth * reach[0], py - edit.tint * reach[1]);
                // Screened, as two exposures on one frame add light.
                let screened: [f32; 3] = std::array::from_fn(|i| {
                    let (a, b) = (old[i].min(1.0), ghost[i].min(1.0));
                    1.0 - (1.0 - a) * (1.0 - b) + (old[i] - a)
                });
                Some(mix(old, screened, cover * strength * 0.6))
            }
            Tool::Ripple => {
                (falloff > 0.0).then(|| {
                    let amp = wavelength * 0.15 * strength * falloff;
                    let r = (px - c[0]).hypot(py - c[1]);
                    // Rings spreading from the centre, displacing along the radius.
                    let shift = (r / wavelength * std::f32::consts::TAU).sin() * amp;
                    let (ux, uy) = if r > 1e-3 {
                        ((px - c[0]) / r, (py - c[1]) / r)
                    } else {
                        (0.0, 0.0)
                    };
                    snapshot.bilinear(px + ux * shift, py + uy * shift)
                })
            }
            _ => (falloff > 0.0).then(|| {
                let reach_px = px_radius * strength * falloff;
                let dx = (hash(x as i64, y as i64, seed) - 0.5) * 2.0 * reach_px;
                let dy = (hash(x as i64, y as i64, seed ^ 0x3) - 0.5) * 2.0 * reach_px;
                snapshot.bilinear(px + dx, py + dy)
            }),
        }
    });
}

fn sort(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // Runs whose brightness lies between the thresholds are sorted; darker or brighter pixels
    // break a run, which is what keeps the subject recognisable.
    let low = 0.15 + (1.0 - edit.tone.clamp(0.0, 1.0)) * 0.3;
    let high = 0.95;
    let w = frame.width;
    let mut rows: Vec<Vec<[f32; 3]>> = (rect.y0..rect.y0 + rect.h)
        .map(|y| {
            (rect.x0..rect.x0 + rect.w)
                .map(|x| {
                    let i = (y * w + x) * 3;
                    [rgb[i], rgb[i + 1], rgb[i + 2]]
                })
                .collect()
        })
        .collect();
    for (row_index, row) in rows.iter_mut().enumerate() {
        let y = rect.y0 + row_index;
        let selected = |x: usize, p: [f32; 3]| {
            let e = luma(p).max(0.0).powf(1.0 / 2.2);
            coverage.at(rect.x0 + x, y, frame.width, frame.height) > 0.5
                && (low..=high).contains(&e)
        };
        let mut start = 0;
        while start < row.len() {
            if !selected(start, row[start]) {
                start += 1;
                continue;
            }
            let mut end = start;
            while end < row.len() && selected(end, row[end]) {
                end += 1;
            }
            row[start..end].sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
            start = end;
        }
    }
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        (k > 0.0).then(|| mix(old, rows[y - rect.y0][x - rect.x0], k))
    });
}

fn vhs(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let bleed = (frame.short() * 0.006).round().max(1.0) as usize;
    let Some(source) = area(coverage, frame, bleed * 4) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    // Colour spreads sideways on tape: the two colour differences blurred along rows only.
    let opp: Vec<[f32; 3]> = (0..r.len())
        .map(|i| opponent(snapshot.at((r.x0 + i % r.w) as isize, (r.y0 + i / r.w) as isize)))
        .collect();
    let row_blur = |k: usize| -> Vec<f32> {
        let plane: Vec<f32> = opp.iter().map(|o| o[k]).collect();
        let mut out = vec![0.0; plane.len()];
        for y in 0..r.h {
            for x in 0..r.w {
                let lo = x.saturating_sub(bleed * 3);
                let hi = (x + bleed).min(r.w - 1);
                let n = (hi - lo + 1) as f32;
                out[y * r.w + x] = (lo..=hi).map(|j| plane[y * r.w + j]).sum::<f32>() / n;
            }
        }
        out
    };
    let (cb, cr) = (row_blur(1), row_blur(2));
    let seed = seed_of(edit);
    let noise = 0.02 + edit.tone.clamp(0.0, 1.0) * 0.05;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        let l = opp[i][0];
        let scan = if (y / 2) % 2 == 0 { 1.0 } else { 0.82 };
        let n = 1.0 + (hash(x as i64 / 2, y as i64, seed) - 0.5) * noise * 2.0;
        let tape = super::from_opponent([l * scan * n, cb[i] * 1.2, cr[i] * 1.2]);
        Some(mix(old, tape, k))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn iris_blur_keeps_the_centre_sharp_and_softens_the_edge() {
        let (w, h) = (80, 80);
        let base = checker(w, h, 2);
        let mut e = edit(Tool::IrisBlur, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.05;
        e.tone = 0.2;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let contrast = |x: usize, y: usize| (px(&out, w, x, y)[0] - px(&out, w, x + 2, y)[0]).abs();
        assert_eq!(px(&out, w, 40, 40), px(&base, w, 40, 40));
        assert!(contrast(12, 40) < contrast(39, 40) * 0.5);
    }

    #[test]
    fn pixel_sort_orders_a_run_by_brightness() {
        let (w, h) = (20, 4);
        let values = [0.5, 0.2, 0.7, 0.3, 0.6, 0.25, 0.4, 0.55];
        let base = fill(w, h, |x, _| [values[x % values.len()]; 3]);
        let mut e = edit(Tool::PixelSort, [0.5, 0.5, 1.0, 1.0]);
        e.tone = 1.0;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let row: Vec<f32> = (0..w).map(|x| luma(px(&out, w, x, 2))).collect();
        let sorted = row.windows(2).filter(|p| p[0] <= p[1] + 1e-6).count();
        assert!(sorted >= w - 3, "{row:?}");
    }

    #[test]
    fn echo_ripple_frosted_and_vhs_change_only_inside() {
        let (w, h) = (60, 60);
        for tool in [
            Tool::EchoDouble,
            Tool::Ripple,
            Tool::FrostedGlass,
            Tool::VhsRetro,
        ] {
            let base = checker(w, h, 3);
            let mut e = edit(tool, [0.5, 0.5, 0.35, 0.35]);
            e.warmth = 0.3;
            e.tint = 0.2;
            e.radius = 0.03;
            let mut out = base.clone();
            run(&mut out, w, h, &e);
            assert_ne!(out, base, "{tool:?} changed nothing");
            assert_eq!(
                changed_outside(&base, &out, w, h, e.region, 0.02),
                0,
                "{tool:?}"
            );
        }
    }
}
