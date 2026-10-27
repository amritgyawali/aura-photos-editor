//! Stylised colour: channel swap, sunset glow, day to night, Lomo, instant film and pop art.
//! ADR-0135.
use super::{area, each, from_opponent, mix, opponent, smoothstep, weight, Frame};
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::ChannelSwap
            | Tool::SunsetGlow
            | Tool::DayToNight
            | Tool::Lomo
            | Tool::InstantFilm
            | Tool::PopArt
    )
}

fn encode(v: f32) -> f32 {
    v.clamp(0.0, 1.0).powf(1.0 / 2.2)
}

fn decode(e: f32) -> f32 {
    e.clamp(0.0, 1.0).powf(2.2)
}

/// An S-curve in the display encoding, `amount` 0 being the identity.
fn s_curve(e: f32, amount: f32) -> f32 {
    e + amount * (e - 0.5) * (1.0 - (2.0 * e - 1.0).abs())
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let tone = edit.tone.clamp(0.0, 1.0);
    let centre = frame.centre(edit);
    let reach = frame.reach(edit);
    let warm = edit
        .target_color
        .map_or([1.0, 0.5, 0.25], crate::retouch_color::to_working);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let new = match edit.tool {
            Tool::ChannelSwap => {
                let [r, g, b] = old;
                if edit.warmth < -0.33 {
                    [b, r, g]
                } else if edit.warmth > 0.33 {
                    [g, b, r]
                } else {
                    [b, g, r]
                }
            }
            Tool::SunsetGlow => {
                // Strongest at the top of the region, fading to nothing at its bottom.
                let top = centre[1] - reach[1];
                let t = 1.0 - ((y as f32 + 0.5 - top) / (2.0 * reach[1])).clamp(0.0, 1.0);
                let glow = t * t * (0.35 + 0.5 * tone);
                let lit: [f32; 3] = std::array::from_fn(|c| {
                    old[c] * (1.0 - glow * 0.5) + warm[c] * glow * (0.15 + luma(old))
                });
                lit
            }
            Tool::DayToNight => {
                let l = luma(old);
                let dark = 0.15 + (1.0 - tone) * 0.25;
                let [_, cb, cr] = opponent(old);
                // Moonlight: darker, desaturated, toward blue.
                let ld = l * dark;
                let night = from_opponent([ld, cb * 0.2 + 0.25 * ld, cr * 0.2 - 0.12 * ld]);
                // Lights stay lit, and warm.
                let lamp = smoothstep(0.6, 0.95, encode(l));
                mix(night, old.map(|v| v * 1.05), lamp)
            }
            Tool::Lomo => {
                let [l, cb, cr] = opponent(old);
                let e = s_curve(encode(l), 0.6);
                let target = decode(e);
                let rich = from_opponent([
                    target,
                    cb * 1.45 * target / l.max(1e-4),
                    cr * 1.45 * target / l.max(1e-4),
                ]);
                // Cross-processed tint: cyan shadows, yellow highlights.
                let shift = [(e - 0.5) * 0.08, (e - 0.5) * 0.03, -(e - 0.5) * 0.1];
                let shifted: [f32; 3] =
                    std::array::from_fn(|c| (rich[c] + shift[c] * target).max(0.0));
                // A dark ring toward the region's edge.
                let d = ((x as f32 + 0.5 - centre[0]) / reach[0])
                    .hypot((y as f32 + 0.5 - centre[1]) / reach[1]);
                let ring = 1.0 - 0.6 * smoothstep(0.45, 1.0, d) * (0.5 + 0.5 * tone);
                shifted.map(|v| v * ring)
            }
            Tool::InstantFilm => {
                let [r, g, b] = old.map(encode);
                // Soft contrast, lifted cyan-leaning shadows, creamy warm highlights.
                let soft = |v: f32| 0.07 + v * 0.86;
                let lifted = [soft(r) * 0.98, soft(g), soft(b) * 1.02 + 0.02 * (1.0 - b)];
                let cream = smoothstep(0.6, 0.95, (r + g + b) / 3.0);
                let warmed = [
                    lifted[0] + 0.03 * cream,
                    lifted[1] + 0.015 * cream,
                    lifted[2] - 0.03 * cream,
                ];
                warmed.map(decode)
            }
            _ => {
                // Pop art: four flat colours by brightness.
                let palette = [
                    [0.02, 0.02, 0.15],
                    [0.85, 0.05, 0.35],
                    warm,
                    [0.95, 0.9, 0.2],
                ];
                let e = encode(luma(old));
                let level = ((e * 4.0) as usize).min(3);
                palette[level]
            }
        };
        Some(mix(old, new, k))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn channel_swap_moves_red_to_blue() {
        let (w, h) = (10, 10);
        let base = fill(w, h, |_, _| [0.8, 0.2, 0.1]);
        let mut e = edit(Tool::ChannelSwap, [0.5, 0.5, 0.8, 0.8]);
        e.warmth = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let p = px(&out, w, 5, 5);
        assert!(
            (p[0] - 0.1).abs() < 1e-5 && (p[1] - 0.2).abs() < 1e-5 && (p[2] - 0.8).abs() < 1e-5,
            "{p:?}"
        );
    }

    #[test]
    fn day_to_night_darkens_and_keeps_lamps_lit() {
        let (w, h) = (20, 10);
        let base = fill(w, h, |x, _| {
            if x < 10 {
                [0.3, 0.3, 0.25]
            } else {
                [0.95, 0.85, 0.6]
            }
        });
        let mut e = edit(Tool::DayToNight, [0.5, 0.5, 0.8, 0.8]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let scene = px(&out, w, 5, 5);
        assert!(luma(scene) < 0.1 && scene[2] > scene[0], "{scene:?}");
        assert!(luma(px(&out, w, 15, 5)) > 0.8);
    }

    #[test]
    fn pop_art_reduces_to_four_colours() {
        let (w, h) = (64, 4);
        let base = fill(w, h, |x, _| [(x as f32 / 64.0).powf(2.2); 3]);
        let mut e = edit(Tool::PopArt, [0.5, 0.5, 1.0, 1.0]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let mut colours: Vec<[u32; 3]> = out
            .chunks_exact(3)
            .map(|p| {
                [
                    (p[0] * 100.0).round() as u32,
                    (p[1] * 100.0).round() as u32,
                    (p[2] * 100.0).round() as u32,
                ]
            })
            .collect();
        colours.sort_unstable();
        colours.dedup();
        assert!(colours.len() <= 4, "{}", colours.len());
    }

    #[test]
    fn sunset_glow_is_warmest_at_the_top() {
        let (w, h) = (20, 40);
        let base = fill(w, h, |_, _| [0.3; 3]);
        let mut e = edit(Tool::SunsetGlow, [0.5, 0.5, 0.8, 0.45]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let warmth = |p: [f32; 3]| p[0] - p[2];
        assert!(warmth(px(&out, w, 10, 3)) > warmth(px(&out, w, 10, 35)) + 0.05);
    }

    #[test]
    fn lomo_and_instant_film_change_the_photograph() {
        let (w, h) = (40, 40);
        for tool in [Tool::Lomo, Tool::InstantFilm] {
            let base = checker(w, h, 5);
            let e = edit(tool, [0.5, 0.5, 0.45, 0.45]);
            let mut out = base.clone();
            run(&mut out, w, h, &e);
            assert_ne!(out, base, "{tool:?}");
        }
    }
}
