//! Light and creative tools: light leak, glitch, haze, starburst and film halation.
use super::{area, each, hash, mix, seed_of, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::LightLeak | Tool::Glitch | Tool::Haze | Tool::Starburst | Tool::Halation
    )
}

const WARM: [f32; 3] = [1.0, 0.45, 0.15];

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let colour = |fallback: [f32; 3]| {
        edit.target_color
            .map_or(fallback, crate::retouch_color::to_working)
    };
    match edit.tool {
        Tool::LightLeak => {
            let centre = frame.source_or_centre(edit);
            let reach = frame.reach(edit);
            let spread = reach[0].max(reach[1]) * 1.2;
            let tint = colour(WARM);
            let gain = 0.4 + edit.tone.clamp(0.0, 1.0) * 0.8;
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                if k <= 0.0 {
                    return None;
                }
                let d = (x as f32 + 0.5 - centre[0]).hypot(y as f32 + 0.5 - centre[1]) / spread;
                let g = (-d * d).exp() * k * gain;
                Some(std::array::from_fn(|c| old[c] + tint[c] * g))
            });
        }
        Tool::Haze => {
            let veil = colour([0.62, 0.64, 0.68]);
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                (k > 0.0).then(|| mix(old, veil, k * (0.3 + 0.5 * edit.tone.clamp(0.0, 1.0))))
            });
        }
        Tool::Starburst => {
            let centre = frame.source_or_centre(edit);
            let reach = frame.reach(edit);
            let length = reach[0].max(reach[1]);
            let rays = 4.0 + (edit.tone.clamp(0.0, 1.0) * 6.0).round() * 2.0;
            let light = colour([1.0, 0.95, 0.85]);
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                if k <= 0.0 {
                    return None;
                }
                let dx = x as f32 + 0.5 - centre[0];
                let dy = y as f32 + 0.5 - centre[1];
                let d = dx.hypot(dy) / length;
                let ray = (dy.atan2(dx) * rays * 0.5).cos().abs().powi(24) * (-3.0 * d).exp();
                let core = (-(d / 0.12).powi(2)).exp();
                let g = (ray * 0.8 + core) * k;
                Some(std::array::from_fn(|c| old[c] + light[c] * g))
            });
        }
        Tool::Glitch => glitch(rgb, frame, edit, coverage),
        _ => halation(rgb, frame, edit, coverage),
    }
}

fn glitch(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let shift = frame.radius_px(edit).max(1);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, shift * 4) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let band = (shift * 2).max(2);
    let seed = seed_of(edit);
    let slices = edit.tone.clamp(0.0, 1.0) * 0.6;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let row = (y / band) as i64;
        let offset = if hash(row, 0, seed) < slices {
            ((hash(row, 1, seed) - 0.5) * 6.0 * shift as f32) as isize
        } else {
            0
        };
        let (x, y) = (x as isize + offset, y as isize);
        let s = shift as isize;
        let glitched = [
            snapshot.at(x + s, y)[0],
            snapshot.at(x, y)[1],
            snapshot.at(x - s, y)[2],
        ];
        Some(mix(old, glitched, k))
    });
}

fn halation(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    const RED: [f32; 3] = [1.0, 0.32, 0.12];
    let radius = frame.radius_px(edit).max(2);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    // Highlights above a threshold `tone` lowers, in linear light.
    let threshold = 0.35 + (1.0 - edit.tone.clamp(0.0, 1.0)) * 0.65;
    let bright: Vec<f32> = snapshot
        .luma_plane()
        .iter()
        .map(|l| (l - threshold).max(0.0) * smoothstep(threshold, threshold * 1.5, *l))
        .collect();
    let glow = blur(&bright, r.w, r.h, radius);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let g = glow[snapshot.index(x, y)] * 2.5 * k;
        // Halation shows in the darker surround, not on top of the highlight itself.
        let surround = 1.0 - smoothstep(threshold * 0.7, threshold, luma(old));
        Some(std::array::from_fn(|c| old[c] + RED[c] * g * surround))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn light_leak_and_starburst_are_strongest_at_their_source() {
        let (w, h) = (60, 60);
        for tool in [Tool::LightLeak, Tool::Starburst] {
            let base = fill(w, h, |_, _| [0.1, 0.1, 0.1]);
            let mut e = edit(tool, [0.5, 0.5, 0.45, 0.45]);
            e.source = Some([0.3, 0.3]);
            let mut out = base.clone();
            run(&mut out, w, h, &e);
            let near = px(&out, w, 18, 18)[0];
            let far = px(&out, w, 50, 50)[0];
            assert!(near > far + 0.05, "{tool:?}: {near} vs {far}");
        }
    }

    #[test]
    fn haze_lowers_contrast() {
        let (w, h) = (30, 30);
        let base = checker(w, h, 3);
        let e = edit(Tool::Haze, [0.5, 0.5, 0.6, 0.6]);
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let c = |rgb: &[f32]| (px(rgb, w, 15, 15)[1] - px(rgb, w, 18, 15)[1]).abs();
        assert!(c(&out) < c(&base) * 0.8);
    }

    #[test]
    fn halation_puts_red_around_a_highlight() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |x, y| {
            if (27..33).contains(&x) && (27..33).contains(&y) {
                [3.0; 3]
            } else {
                [0.05; 3]
            }
        });
        let mut e = edit(Tool::Halation, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.08;
        e.tone = 0.8;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let ring = px(&out, w, 36, 30);
        assert!(ring[0] > 0.06 && ring[0] > ring[2] * 1.5, "{ring:?}");
    }

    #[test]
    fn glitch_separates_the_channels() {
        let (w, h) = (40, 20);
        let base = fill(w, h, |x, _| if x < 20 { [0.1; 3] } else { [0.8; 3] });
        let mut e = edit(Tool::Glitch, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.15;
        e.tone = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let p = px(&out, w, 18, 10);
        assert!(p[0] > p[2] + 0.3, "red reads from the right: {p:?}");
    }
}
