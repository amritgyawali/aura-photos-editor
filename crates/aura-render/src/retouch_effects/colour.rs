//! Colour tools: hue shift, invert, chroma key, vein reduction, colour unify and local white
//! balance. All but invert keep each pixel's luminance.
use super::{
    area, each, from_opponent, mix, opponent, sample, smoothstep, weight, Frame, Snapshot,
};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::HueShift
            | Tool::InvertColors
            | Tool::ChromaKey
            | Tool::VeinReduce
            | Tool::ColorUnify
            | Tool::LocalWhiteBalance
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    match edit.tool {
        Tool::HueShift => {
            let (s, c) = (edit.warmth * std::f32::consts::PI).sin_cos();
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                (k > 0.0).then(|| {
                    let [l, cb, cr] = opponent(old);
                    let turned = from_opponent([l, cb * c - cr * s, cb * s + cr * c]);
                    mix(old, turned, k)
                })
            });
        }
        Tool::InvertColors => each(rgb, frame, rect, |x, y, old| {
            let k = weight(edit, coverage, frame, x, y);
            (k > 0.0).then(|| {
                let inverted = old.map(|v| (1.0 - v.clamp(0.0, 1.0).powf(1.0 / 2.2)).powf(2.2));
                mix(old, inverted, k)
            })
        }),
        Tool::ChromaKey => chroma_key(rgb, frame, edit, coverage),
        Tool::VeinReduce => veins(rgb, frame, edit, coverage),
        Tool::ColorUnify => unify(rgb, frame, edit, coverage),
        _ => white_balance(rgb, frame, edit, coverage),
    }
}

/// A colour's chroma relative to its luminance, so light and shade of one colour compare equal.
fn chroma(p: [f32; 3]) -> [f32; 2] {
    let [l, cb, cr] = opponent(p);
    let l = l.max(1e-4);
    [cb / l, cr / l]
}

fn chroma_key(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(point) = edit.source else {
        return;
    };
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let key = chroma(sample(
        rgb,
        frame,
        [
            point[0] * frame.width as f32,
            point[1] * frame.height as f32,
        ],
    ));
    let key_len = key[0].hypot(key[1]).max(1e-4);
    let tolerance = 0.08 + edit.tone.clamp(0.0, 1.0) * 0.6;
    let replacement = edit.target_color.map(crate::retouch_color::to_working);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let c = chroma(old);
        let distance = (c[0] - key[0]).hypot(c[1] - key[1]);
        let matched = 1.0 - smoothstep(tolerance * 0.5, tolerance, distance);
        // Spill: the share of this pixel's chroma that points along the key.
        let along = ((c[0] * key[0] + c[1] * key[1]) / key_len).max(0.0);
        let [l, cb, cr] = opponent(old);
        let unit = [key[0] / key_len, key[1] / key_len];
        let despilled = from_opponent([
            l,
            cb - unit[0] * along * l.max(1e-4),
            cr - unit[1] * along * l.max(1e-4),
        ]);
        let base = mix(old, despilled, k);
        Some(replacement.map_or(base, |colour| mix(base, colour, k * matched)))
    });
}

fn veins(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).max(2);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let opp: Vec<[f32; 3]> = (0..r.len())
        .map(|i| opponent(snapshot.at((r.x0 + i % r.w) as isize, (r.y0 + i / r.w) as isize)))
        .collect();
    let plane = |k: usize| {
        blur(
            &opp.iter().map(|o| o[k]).collect::<Vec<_>>(),
            r.w,
            r.h,
            radius,
        )
    };
    let (ml, mcb, mcr) = (plane(0), plane(1), plane(2));
    let tone = edit.tone.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, _| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        let [l, cb, cr] = opp[i];
        let level = ml[i].max(1e-4);
        // A vein is bluer (more cb, less cr) and darker than the skin around it.
        let blue = smoothstep(0.0, 0.06, ((cb - mcb[i]) - (cr - mcr[i])) / level);
        let darker = smoothstep(0.0, 0.08, (ml[i] - l) / level);
        let t = k * blue;
        if t <= 0.0 {
            return None;
        }
        let new = from_opponent([
            l + (ml[i] - l) * t * tone * darker,
            cb + (mcb[i] - cb) * t,
            cr + (mcr[i] - cr) * t,
        ]);
        Some(new)
    });
}

fn unify(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // The selection's own average colour, weighted by how selected each pixel is.
    let mut sum = [0.0_f64; 2];
    let mut total = 0.0_f64;
    for y in rect.y0..rect.y0 + rect.h {
        for x in rect.x0..rect.x0 + rect.w {
            let c = coverage.at(x, y, frame.width, frame.height);
            if c <= 0.0 {
                continue;
            }
            let i = (y * frame.width + x) * 3;
            let p = [rgb[i], rgb[i + 1], rgb[i + 2]];
            if opponent(p)[0] < 1e-3 {
                continue;
            }
            let ch = chroma(p);
            sum[0] += f64::from(ch[0] * c);
            sum[1] += f64::from(ch[1] * c);
            total += f64::from(c);
        }
    }
    if total <= 0.0 {
        return;
    }
    let target = [(sum[0] / total) as f32, (sum[1] / total) as f32];
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let [l, _, _] = opponent(old);
        let c = chroma(old);
        let pulled = [c[0] + (target[0] - c[0]) * k, c[1] + (target[1] - c[1]) * k];
        Some(from_opponent([l, pulled[0] * l, pulled[1] * l]))
    });
}

fn white_balance(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(point) = edit.source else {
        return;
    };
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let neutral = sample(
        rgb,
        frame,
        [
            point[0] * frame.width as f32,
            point[1] * frame.height as f32,
        ],
    );
    let level = opponent(neutral)[0];
    if level <= 1e-4 || neutral.iter().any(|v| *v <= 1e-5) {
        return;
    }
    // Gains that make the sample grey at its own luminance.
    let gains = neutral.map(|v| (level / v).clamp(0.25, 4.0));
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        (k > 0.0).then(|| mix(old, std::array::from_fn(|c| old[c] * gains[c]), k))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use super::{chroma, opponent};
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn hue_shift_turns_colour_and_keeps_luminance() {
        let (w, h) = (20, 20);
        let base = fill(w, h, |_, _| [0.5, 0.2, 0.1]);
        let mut e = edit(Tool::HueShift, [0.5, 0.5, 0.6, 0.6]);
        e.warmth = 0.5;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let (a, b) = (px(&base, w, 10, 10), px(&out, w, 10, 10));
        assert!((opponent(a)[0] - opponent(b)[0]).abs() < 1e-4);
        assert!((a[0] - b[0]).abs() > 0.05, "{a:?} -> {b:?}");
    }

    #[test]
    fn invert_twice_is_the_photo() {
        let (w, h) = (20, 20);
        let base = checker(w, h, 3);
        let e = edit(Tool::InvertColors, [0.5, 0.5, 0.6, 0.6]);
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert_ne!(px(&out, w, 10, 10), px(&base, w, 10, 10));
        run(&mut out, w, h, &e);
        for (a, b) in base.iter().zip(&out) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn chroma_key_replaces_the_green_screen_and_keeps_the_subject() {
        let (w, h) = (40, 30);
        let base = fill(w, h, |x, _| {
            if x < 25 {
                [0.08, 0.5, 0.1]
            } else {
                [0.6, 0.4, 0.3]
            }
        });
        let mut e = edit(Tool::ChromaKey, [0.5, 0.5, 0.8, 0.8]);
        e.source = Some([0.2, 0.5]);
        e.target_color = Some([0.2, 0.3, 0.8]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let screen = px(&out, w, 10, 15);
        assert!(screen[2] > screen[1], "background became blue: {screen:?}");
        let subject = px(&out, w, 35, 15);
        let was = px(&base, w, 35, 15);
        assert!((subject[0] - was[0]).abs() < 0.05, "{was:?} -> {subject:?}");
    }

    #[test]
    fn local_white_balance_makes_the_sample_neutral() {
        let (w, h) = (20, 20);
        let base = fill(w, h, |_, _| [0.5, 0.4, 0.25]);
        let mut e = edit(Tool::LocalWhiteBalance, [0.5, 0.5, 0.6, 0.6]);
        e.source = Some([0.5, 0.5]);
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let p = px(&out, w, 10, 10);
        assert!(
            (p[0] - p[1]).abs() < 1e-3 && (p[1] - p[2]).abs() < 1e-3,
            "{p:?}"
        );
    }

    #[test]
    fn unify_narrows_the_spread_of_colours() {
        let (w, h) = (30, 30);
        let base = fill(w, h, |x, y| {
            if (x + y) % 2 == 0 {
                [0.5, 0.3, 0.1]
            } else {
                [0.3, 0.25, 0.3]
            }
        });
        let e = edit(Tool::ColorUnify, [0.5, 0.5, 0.6, 0.6]);
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let spread = |rgb: &[f32]| {
            let a = chroma(px(rgb, w, 15, 15));
            let b = chroma(px(rgb, w, 16, 15));
            (a[0] - b[0]).hypot(a[1] - b[1])
        };
        assert!(spread(&out) < spread(&base) * 0.1);
    }

    #[test]
    fn veins_are_evened_toward_the_skin_around_them() {
        let (w, h) = (60, 60);
        let skin = [0.55, 0.38, 0.3];
        let vein = [0.42, 0.34, 0.36];
        let base = fill(w, h, |x, _| if (29..31).contains(&x) { vein } else { skin });
        let mut e = edit(Tool::VeinReduce, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.08;
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let before = chroma(px(&base, w, 30, 30));
        let after = chroma(px(&out, w, 30, 30));
        let target = chroma(skin);
        let d = |c: [f32; 2]| (c[0] - target[0]).hypot(c[1] - target[1]);
        assert!(d(after) < d(before) * 0.6, "{before:?} -> {after:?}");
        assert_eq!(px(&out, w, 10, 30), px(&base, w, 10, 30));
    }
}
