//! Light and lens: spotlight, rim light, chromatic aberration, fisheye, anamorphic streak and
//! kaleidoscope. ADR-0134.
use super::{area, each, mix, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect};
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Spotlight
            | Tool::RimLight
            | Tool::ChromaticAberration
            | Tool::Fisheye
            | Tool::AnamorphicStreak
            | Tool::Kaleidoscope
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::Spotlight => spotlight(rgb, frame, edit, coverage),
        Tool::RimLight => rim(rgb, frame, edit, coverage),
        Tool::AnamorphicStreak => streak(rgb, frame, edit, coverage),
        _ => remap(rgb, frame, edit, coverage),
    }
}

/// Distance across the region's ellipse: 0 at the centre, 1 on the edge.
fn ellipse(frame: Frame, edit: &Edit, x: usize, y: usize) -> (f32, [f32; 2]) {
    let c = frame.centre(edit);
    let r = frame.reach(edit);
    let d = [
        (x as f32 + 0.5 - c[0]) / r[0],
        (y as f32 + 0.5 - c[1]) / r[1],
    ];
    (d[0].hypot(d[1]), d)
}

fn spotlight(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let spot = frame.source_or_centre(edit);
    // `radius` is the pool of light: 0.0005..0.05 of the short edge, times ten.
    let pool = (edit.radius * 10.0 * frame.short()).max(4.0);
    let dark = 0.25 + edit.tone.clamp(0.0, 1.0) * 0.6;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let d = (x as f32 + 0.5 - spot[0]).hypot(y as f32 + 0.5 - spot[1]) / pool;
        // In the pool a fifth of a stop brighter; outside, falling to `dark` of the light.
        let light = 1.15 - (1.15 - (1.0 - dark)) * smoothstep(0.6, 1.6, d);
        Some(mix(old, old.map(|v| v * light), k))
    });
}

fn rim(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 4) else {
        return;
    };
    let width = (edit.radius * frame.short()).round().clamp(1.0, 40.0) as usize;
    // The selection's own shape, blurred: inside near the edge it drops below one, and its
    // slope points out of the subject.
    let shape: Vec<f32> = (0..rect.len())
        .map(|i| {
            coverage.at(
                rect.x0 + i % rect.w,
                rect.y0 + i / rect.w,
                frame.width,
                frame.height,
            )
        })
        .collect();
    let soft = blur(&shape, rect.w, rect.h, width);
    let dir = {
        let n = edit.warmth.hypot(edit.tint).max(1e-3);
        [edit.warmth / n, edit.tint / n]
    };
    let colour = edit
        .target_color
        .map_or([1.0, 0.92, 0.8], crate::retouch_color::to_working);
    let at = |x: isize, y: isize| -> f32 {
        let cx = (x - rect.x0 as isize).clamp(0, rect.w as isize - 1) as usize;
        let cy = (y - rect.y0 as isize).clamp(0, rect.h as isize - 1) as usize;
        soft[cy * rect.w + cx]
    };
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (xi, yi) = (x as isize, y as isize);
        // The outward direction is where the blurred shape falls.
        let g = [
            at(xi - 1, yi) - at(xi + 1, yi),
            at(xi, yi - 1) - at(xi, yi + 1),
        ];
        let g_len = g[0].hypot(g[1]);
        if g_len < 1e-4 {
            return None;
        }
        let facing = ((g[0] * dir[0] + g[1] * dir[1]) / g_len).max(0.0);
        let edge = (1.0 - at(xi, yi)).clamp(0.0, 1.0) * 2.0;
        let glow = (edge * facing).min(1.0) * k * 0.8;
        let l = luma(old);
        Some(std::array::from_fn(|c| {
            old[c] + colour[c] * glow * (0.25 + l)
        }))
    });
}

fn streak(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let length = (edit.radius * 20.0 * frame.short()).clamp(8.0, frame.width as f32) as usize;
    let threshold = super::light_threshold(edit.tone);
    let colour = edit
        .target_color
        .map_or([0.35, 0.55, 1.0], crate::retouch_color::to_working);
    // Per row: the highlights' excess, spread far along the row and only a little across.
    let snapshot = Snapshot::take(rgb, frame, rect);
    let bright: Vec<f32> = snapshot
        .luma_plane()
        .iter()
        .map(|l| (l - threshold).max(0.0))
        .collect();
    let mut along = vec![0.0_f32; rect.len()];
    for y in 0..rect.h {
        let row = &bright[y * rect.w..(y + 1) * rect.w];
        // An exponential tail in both directions, two passes - cost independent of length.
        let decay = (-3.0 / length as f32).exp();
        let mut carry = 0.0;
        for x in 0..rect.w {
            carry = carry * decay + row[x];
            along[y * rect.w + x] += carry;
        }
        carry = 0.0;
        for x in (0..rect.w).rev() {
            carry = carry * decay + row[x];
            along[y * rect.w + x] += carry;
        }
    }
    let gain = 3.0 / length as f32;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let s = along[snapshot.index(x, y)] * gain * k;
        (s > 0.0).then(|| std::array::from_fn(|c| old[c] + colour[c] * s))
    });
}

/// The three tools that read the frame somewhere else: chromatic aberration, fisheye and
/// kaleidoscope. Each falls to nothing at the ellipse edge, so none of them tears.
fn remap(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
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
    let segments = 4.0 + (edit.tone.clamp(0.0, 1.0) * 4.0).round() * 2.0;
    let strength = edit.amount.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, old| {
        let cover = coverage.at(x, y, frame.width, frame.height);
        if cover <= 0.0 {
            return None;
        }
        let (d, v) = ellipse(frame, edit, x, y);
        if d >= 1.0 {
            return None;
        }
        let falloff = (1.0 - d * d).powi(2) * cover;
        let back = |u: [f32; 2]| [c[0] + u[0] * reach[0], c[1] + u[1] * reach[1]];
        match edit.tool {
            Tool::ChromaticAberration => {
                let s = edit.radius * 4.0 * strength * falloff * d;
                let red = back([v[0] * (1.0 - s), v[1] * (1.0 - s)]);
                let blue = back([v[0] * (1.0 + s), v[1] * (1.0 + s)]);
                Some([
                    snapshot.bilinear(red[0], red[1])[0],
                    old[1],
                    snapshot.bilinear(blue[0], blue[1])[2],
                ])
            }
            Tool::Fisheye => {
                // A sphere seen from the front: the centre is magnified, the edge compressed,
                // and at the edge the map is the identity again.
                let r = d.max(1e-6);
                let bent = (r * std::f32::consts::FRAC_PI_2).sin().powf(1.0 + strength);
                let s = 1.0 + (bent / r - 1.0) * (1.0 - smoothstep(0.85, 1.0, d));
                let p = back([v[0] * s, v[1] * s]);
                Some(mix(old, snapshot.bilinear(p[0], p[1]), cover))
            }
            _ => {
                let a = v[1].atan2(v[0]);
                let wedge = std::f32::consts::TAU / segments;
                let mut local = a.rem_euclid(wedge);
                if local > wedge * 0.5 {
                    local = wedge - local;
                }
                let p = back([d * local.cos(), d * local.sin()]);
                Some(mix(
                    old,
                    snapshot.bilinear(p[0], p[1]),
                    falloff.sqrt() * strength,
                ))
            }
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn spotlight_is_brighter_at_its_source_than_away_from_it() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |_, _| [0.3; 3]);
        let mut e = edit(Tool::Spotlight, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.02;
        e.source = Some([0.3, 0.3]);
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(px(&out, w, 18, 18)[0] > 0.3);
        assert!(px(&out, w, 50, 50)[0] < 0.25);
    }

    #[test]
    fn rim_light_glows_on_the_side_facing_the_light() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |_, _| [0.2; 3]);
        let mut e = edit(Tool::RimLight, [0.5, 0.5, 0.25, 0.25]);
        e.feather = 0.0;
        e.radius = 0.06;
        e.warmth = 1.0;
        e.tint = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        // The right edge of the disc glows; the left edge does not.
        let right = luma(px(&out, w, 43, 30));
        let left = luma(px(&out, w, 16, 30));
        assert!(
            right > 0.25 && right > left + 0.04,
            "right {right} left {left}"
        );
    }

    #[test]
    fn fisheye_and_kaleidoscope_change_only_the_ellipse_and_leave_its_edge() {
        let (w, h) = (60, 60);
        for tool in [Tool::Fisheye, Tool::Kaleidoscope, Tool::ChromaticAberration] {
            let base = checker(w, h, 4);
            let e = edit(tool, [0.5, 0.5, 0.35, 0.35]);
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

    #[test]
    fn anamorphic_streak_spreads_a_highlight_sideways_not_up() {
        let (w, h) = (80, 40);
        let base = fill(w, h, |x, y| {
            if x == 40 && y == 20 {
                [10.0; 3]
            } else {
                [0.02; 3]
            }
        });
        let mut e = edit(Tool::AnamorphicStreak, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.2;
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let side = px(&out, w, 55, 20)[2];
        let up = px(&out, w, 40, 8)[2];
        assert!(side > 0.05 && up < 0.021, "side {side} up {up}");
    }

    #[test]
    fn anamorphic_streak_reaches_a_white_light_in_an_ordinary_photograph_at_its_defaults() {
        // A display-referred frame tops out at 1: a lamp is 1, not 10.
        let (w, h) = (80, 40);
        let base = fill(w, h, |x, y| {
            if (38..43).contains(&x) && (19..22).contains(&y) {
                [1.0; 3]
            } else {
                [0.05; 3]
            }
        });
        let mut e = edit(Tool::AnamorphicStreak, [0.5, 0.5, 0.6, 0.6]);
        // The window's defaults for this tool.
        e.radius = 0.02;
        e.tone = 0.5;
        e.amount = 0.8;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let lift = |x| px(&out, w, x, 20)[2] - px(&base, w, x, 20)[2];
        assert!(
            lift(48) > 0.02 && lift(48) > lift(55) && lift(55) > 0.0,
            "a streak reaches sideways and fades"
        );
    }
}
