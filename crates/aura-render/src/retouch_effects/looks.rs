//! Colour looks: split tone, film fade, bleach bypass, teal and orange, colour splash and sky
//! blue. ADR-0134. All but film fade and bleach bypass keep each pixel's luminance.
use super::{area, each, from_opponent, mix, opponent, sample, smoothstep, weight, Frame};
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect};
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::SplitTone
            | Tool::FilmFade
            | Tool::BleachBypass
            | Tool::TealOrange
            | Tool::ColourSplash
            | Tool::SkyBlue
    )
}

fn encode(v: f32) -> f32 {
    v.clamp(0.0, 1.0).powf(1.0 / 2.2)
}

fn decode(e: f32) -> f32 {
    e.clamp(0.0, 1.0).powf(2.2)
}

/// A colour's direction in the two colour differences, at unit length.
fn direction(colour: [f32; 3]) -> [f32; 2] {
    let [_, cb, cr] = opponent(colour);
    let n = cb.hypot(cr).max(1e-6);
    [cb / n, cr / n]
}

/// The hue angle of a pixel in the colour-difference plane, in radians.
fn hue(p: [f32; 3]) -> f32 {
    let [_, cb, cr] = opponent(p);
    cr.atan2(cb)
}

fn angle_between(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(std::f32::consts::TAU);
    d.min(std::f32::consts::TAU - d)
}

/// Shift a pixel's colour by `push` (a direction times a strength relative to its luminance),
/// keeping the luminance.
fn tint(p: [f32; 3], push: [f32; 2]) -> [f32; 3] {
    let [l, cb, cr] = opponent(p);
    from_opponent([l, cb + push[0] * l, cr + push[1] * l])
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let tone = edit.tone.clamp(0.0, 1.0);
    match edit.tool {
        Tool::SplitTone => {
            let high = direction(
                edit.target_color
                    .map_or([1.0, 0.75, 0.45], crate::retouch_color::to_working),
            );
            let (s, c) = ((1.0 + edit.warmth) * std::f32::consts::PI).sin_cos();
            let low = [high[0] * c - high[1] * s, high[0] * s + high[1] * c];
            // `tone` moves the crossover: 0.5 is middle grey in the display encoding.
            let pivot = 0.25 + tone * 0.5;
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                (k > 0.0).then(|| {
                    let t = smoothstep(pivot - 0.25, pivot + 0.25, encode(luma(old)));
                    let dir = [
                        low[0] * (1.0 - t) + high[0] * t,
                        low[1] * (1.0 - t) + high[1] * t,
                    ];
                    mix(old, tint(old, [dir[0] * 0.25, dir[1] * 0.25]), k)
                })
            });
        }
        Tool::FilmFade => {
            let lift = 0.04 + tone * 0.16;
            let cast = edit.warmth.clamp(-1.0, 1.0) * 0.05;
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                (k > 0.0).then(|| {
                    // A curve whose black point is raised and whose white point is softened.
                    let faded = old.map(|v| decode(lift + encode(v) * (0.96 - lift)));
                    let [l, cb, cr] = opponent(faded);
                    let muted = from_opponent([l, cb * 0.75 - cast * l, cr * 0.75 + cast * l]);
                    mix(old, muted, k)
                })
            });
        }
        Tool::BleachBypass => {
            let contrast = 1.15 + tone * 0.6;
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                (k > 0.0).then(|| {
                    let l = luma(old).max(1e-6);
                    let e = ((encode(l) - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
                    let target = decode(e);
                    // Silver left in the print: a third of the colour remains.
                    let grey = [target; 3];
                    let coloured = old.map(|v| v * target / l);
                    mix(old, mix(grey, coloured, 0.35), k)
                })
            });
        }
        Tool::TealOrange => teal_orange(rgb, frame, edit, coverage, rect, tone),
        Tool::ColourSplash => colour_splash(rgb, frame, edit, coverage, rect, tone),
        _ => sky_blue(rgb, frame, edit, coverage, rect, tone),
    }
}

fn teal_orange(
    rgb: &mut [f32],
    frame: Frame,
    edit: &Edit,
    coverage: &Coverage,
    rect: Rect,
    tone: f32,
) {
    let teal = direction([0.05, 0.35, 0.4]);
    let orange = direction([1.0, 0.5, 0.15]);
    let skin = hue([0.6, 0.38, 0.28]);
    let strength = 0.08 + tone * 0.22;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let e = encode(luma(old));
        let shadow = 1.0 - smoothstep(0.25, 0.55, e);
        let warmish = smoothstep(0.45, 0.75, e);
        // Skin is the reason the look exists: it is held at its own hue.
        let near_skin = 1.0 - smoothstep(0.35, 0.7, angle_between(hue(old), skin));
        let push = [
            teal[0] * shadow * strength + orange[0] * warmish * strength * 0.6,
            teal[1] * shadow * strength + orange[1] * warmish * strength * 0.6,
        ];
        let held = 1.0 - near_skin * 0.85;
        Some(mix(old, tint(old, [push[0] * held, push[1] * held]), k))
    });
}

fn colour_splash(
    rgb: &mut [f32],
    frame: Frame,
    edit: &Edit,
    coverage: &Coverage,
    rect: Rect,
    tone: f32,
) {
    let Some(point) = edit.source else {
        return;
    };
    let kept = hue(sample(
        rgb,
        frame,
        [
            point[0] * frame.width as f32,
            point[1] * frame.height as f32,
        ],
    ));
    let tolerance = 0.25 + tone * 0.9;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let [l, cb, cr] = opponent(old);
        // A colour with almost no chroma has no hue worth keeping.
        let colourful = smoothstep(0.02, 0.08, cb.hypot(cr) / l.max(1e-4));
        let keep = colourful
            * (1.0 - smoothstep(tolerance * 0.6, tolerance, angle_between(hue(old), kept)));
        Some(mix(old, [l; 3], k * (1.0 - keep)))
    });
}

fn sky_blue(
    rgb: &mut [f32],
    frame: Frame,
    edit: &Edit,
    coverage: &Coverage,
    rect: Rect,
    tone: f32,
) {
    let sky = direction(
        edit.target_color
            .map_or([0.25, 0.5, 1.0], crate::retouch_color::to_working),
    );
    let blue = sky[1].atan2(sky[0]);
    let strength = 0.1 + tone * 0.35;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let [l, cb, cr] = opponent(old);
        let chroma = cb.hypot(cr) / l.max(1e-4);
        let e = encode(l);
        // Sky: bright, and either already bluish or washed out. Cloud tops and skin
        // are left: the whitest pixels barely move and warm hues are excluded.
        let bluish = 1.0 - smoothstep(0.6, 1.2, angle_between(hue(old), blue));
        let washed = 1.0 - smoothstep(0.05, 0.2, chroma);
        let bright = smoothstep(0.35, 0.6, e) * (1.0 - smoothstep(0.9, 0.99, e));
        let w = bright * bluish.max(washed);
        (w > 0.0).then(|| {
            mix(
                old,
                tint(old, [sky[0] * strength, sky[1] * strength]),
                k * w,
            )
        })
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::opponent;
    use super::super::testing::*;
    use super::{angle_between, hue};
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn colour_splash_keeps_the_sampled_colour_and_greys_the_rest() {
        let (w, h) = (40, 20);
        let base = fill(w, h, |x, _| {
            if x < 20 {
                [0.6, 0.05, 0.05]
            } else {
                [0.05, 0.4, 0.08]
            }
        });
        let mut e = edit(Tool::ColourSplash, [0.5, 0.5, 0.8, 0.8]);
        e.source = Some([0.25, 0.5]);
        e.feather = 0.0;
        e.tone = 0.2;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert_eq!(px(&out, w, 10, 10), px(&base, w, 10, 10), "red is kept");
        let g = px(&out, w, 30, 10);
        assert!(
            (g[0] - g[1]).abs() < 1e-3 && (g[1] - g[2]).abs() < 1e-3,
            "green turns grey: {g:?}"
        );
    }

    #[test]
    fn split_tone_and_teal_orange_keep_luminance() {
        let (w, h) = (30, 10);
        for tool in [Tool::SplitTone, Tool::TealOrange] {
            let base = fill(w, h, |x, _| [0.02 + x as f32 * 0.02; 3]);
            let mut e = edit(tool, [0.5, 0.5, 0.8, 0.8]);
            e.feather = 0.0;
            let mut out = base.clone();
            run(&mut out, w, h, &e);
            for x in [2, 15, 28] {
                let (a, b) = (px(&base, w, x, 5), px(&out, w, x, 5));
                assert!((luma(a) - luma(b)).abs() < 1e-4, "{tool:?} at {x}");
            }
            assert_ne!(out, base, "{tool:?} changed nothing");
        }
    }

    #[test]
    fn teal_orange_holds_skin_hue() {
        let (w, h) = (10, 10);
        let skin = [0.55, 0.36, 0.27];
        let base = fill(w, h, |_, _| skin);
        let mut e = edit(Tool::TealOrange, [0.5, 0.5, 0.8, 0.8]);
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(angle_between(hue(px(&out, w, 5, 5)), hue(skin)) < 0.08);
    }

    #[test]
    fn film_fade_lifts_black_and_bleach_bypass_drains_colour() {
        let (w, h) = (10, 10);
        let mut e = edit(Tool::FilmFade, [0.5, 0.5, 0.8, 0.8]);
        e.tone = 0.5;
        let mut out = fill(w, h, |_, _| [0.0; 3]);
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 5, 5)) > 0.002);
        let red = fill(w, h, |_, _| [0.6, 0.1, 0.1]);
        let mut bleached = red.clone();
        e.tool = Tool::BleachBypass;
        run(&mut bleached, w, h, &e);
        let chroma = |p: [f32; 3]| {
            let [l, cb, cr] = opponent(p);
            cb.hypot(cr) / l
        };
        assert!(chroma(px(&bleached, w, 5, 5)) < chroma(px(&red, w, 5, 5)) * 0.6);
    }

    #[test]
    fn sky_blue_turns_a_washed_sky_blue_and_leaves_skin() {
        let (w, h) = (20, 20);
        let base = fill(w, h, |x, _| {
            if x < 10 {
                [0.55, 0.58, 0.62]
            } else {
                [0.55, 0.36, 0.27]
            }
        });
        let mut e = edit(Tool::SkyBlue, [0.5, 0.5, 0.8, 0.8]);
        e.tone = 1.0;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let sky = px(&out, w, 5, 10);
        assert!(sky[2] - sky[0] > 0.15, "{sky:?}");
        assert_eq!(px(&out, w, 15, 10), px(&base, w, 15, 10));
    }
}
