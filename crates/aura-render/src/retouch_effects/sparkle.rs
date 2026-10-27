//! Sparkle, fairy lights and rainbow prism. ADR-0135.
use super::{area, each, hash, seed_of, smoothstep, weight, Frame, Snapshot};
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(tool, Tool::Sparkle | Tool::FairyLights | Tool::RainbowPrism)
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::Sparkle => sparkle(rgb, frame, edit, coverage),
        Tool::FairyLights => fairy(rgb, frame, edit, coverage),
        _ => rainbow(rgb, frame, edit, coverage),
    }
}

/// Sparkles never multiply past this many in one operation, however busy the selection.
const MAX_SPARKLES: usize = 400;

fn sparkle(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, rect);
    let length = (edit.radius * 3.0 * frame.short()).clamp(4.0, 120.0);
    let threshold = 0.5 + (1.0 - edit.tone.clamp(0.0, 1.0)) * 1.0;
    // The brightest pixel of every cell, when it is a highlight: one star per glint.
    let cell = (length * 0.5).ceil() as usize;
    let mut stars: Vec<(f32, f32, f32)> = Vec::new();
    for by in (rect.y0..rect.y0 + rect.h).step_by(cell) {
        for bx in (rect.x0..rect.x0 + rect.w).step_by(cell) {
            let mut best = (0.0_f32, 0, 0);
            for y in by..(by + cell).min(rect.y0 + rect.h) {
                for x in bx..(bx + cell).min(rect.x0 + rect.w) {
                    let l = luma(snapshot.at(x as isize, y as isize));
                    if l > best.0 && coverage.at(x, y, frame.width, frame.height) > 0.0 {
                        best = (l, x, y);
                    }
                }
            }
            if best.0 > threshold {
                stars.push((
                    best.1 as f32 + 0.5,
                    best.2 as f32 + 0.5,
                    (best.0 - threshold).min(2.0),
                ));
            }
        }
    }
    stars.sort_by(|a, b| b.2.total_cmp(&a.2));
    stars.truncate(MAX_SPARKLES);
    if stars.is_empty() {
        return;
    }
    // A ray's thickness grows with its length, so a star looks the same at any resolution.
    let thick = (length / 60.0).max(0.7);
    let band = thick * 3.0;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let mut add = 0.0_f32;
        for &(sx, sy, power) in &stars {
            let (dx, dy) = ((px - sx).abs(), (py - sy).abs());
            // Beyond three thicknesses from both axes of a star, nothing of it is visible.
            if !((dx <= length && dy <= band) || (dy <= length && dx <= band)) {
                continue;
            }
            // Four thin rays along the axes, tapering, plus a small core.
            let ray = |along: f32, across: f32| {
                let a = across / thick;
                (1.0 - along / length).max(0.0).powi(2) * (-a * a * 2.0).exp()
            };
            let core = (-(dx * dx + dy * dy) / (4.0 * thick * thick)).exp();
            add += (ray(dx, dy) + ray(dy, dx) + core) * (0.4 + power * 0.4);
        }
        (add > 0.0).then(|| old.map(|v| v + add * k))
    });
}

fn fairy(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let size = (edit.radius * frame.short()).max(2.0);
    let cell = (size * 4.0).ceil();
    let density = 0.15 + edit.tone.clamp(0.0, 1.0) * 0.6;
    let warm = edit
        .target_color
        .map_or([1.0, 0.75, 0.4], crate::retouch_color::to_working);
    let seed = seed_of(edit);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let (gx, gy) = ((px / cell).floor() as i64, (py / cell).floor() as i64);
        let mut light = 0.0_f32;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cx, cy) = (gx + dx, gy + dy);
                if hash(cx, cy, seed) >= density {
                    continue;
                }
                let ox = (cx as f32 + hash(cx, cy, seed ^ 0x1)) * cell;
                let oy = (cy as f32 + hash(cx, cy, seed ^ 0x2)) * cell;
                let r = size * (0.5 + hash(cx, cy, seed ^ 0x3));
                let d = (px - ox).hypot(py - oy) / r;
                // An out-of-focus orb: a soft disc with a slightly brighter rim.
                let orb = smoothstep(1.0, 0.85, d) * (0.6 + 0.4 * smoothstep(0.5, 0.95, d));
                light = light.max(orb * (0.4 + 0.6 * hash(cx, cy, seed ^ 0x4)));
            }
        }
        (light > 0.0).then(|| std::array::from_fn(|c| old[c] + warm[c] * light * k * 0.8))
    });
}

fn rainbow(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let c = frame.centre(edit);
    let (s, co) = (edit.warmth * std::f32::consts::PI).sin_cos();
    let width = (edit.radius * 4.0 * frame.short()).max(6.0);
    let strength = 0.2 + edit.tone.clamp(0.0, 1.0) * 0.4;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        // Signed distance across the band; the spectrum runs red to violet over its width.
        let across = (x as f32 + 0.5 - c[0]) * -s + (y as f32 + 0.5 - c[1]) * co;
        let t = across / width + 0.5;
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        let hue = t * 300.0;
        let spectrum = |h: f32| {
            let f = |n: f32| {
                let kk = (n + h / 60.0).rem_euclid(6.0);
                1.0 - (kk.min(4.0 - kk).clamp(0.0, 1.0))
            };
            [f(5.0), f(3.0), f(1.0)]
        };
        let colour = spectrum(hue);
        let envelope = (t * std::f32::consts::PI).sin();
        Some(std::array::from_fn(|i| {
            old[i] + colour[i] * envelope * strength * k * (0.3 + luma(old))
        }))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn sparkle_puts_a_star_on_a_glint_and_nothing_on_a_flat_area() {
        let (w, h) = (80, 80);
        let base = fill(w, h, |x, y| {
            if x == 40 && y == 40 {
                [5.0; 3]
            } else {
                [0.1; 3]
            }
        });
        let mut e = edit(Tool::Sparkle, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.05;
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        // A ray along the row from the glint, none on the diagonal at the same distance.
        assert!(px(&out, w, 46, 40)[0] > 0.2);
        assert!(px(&out, w, 46, 46)[0] < 0.12);
        let flat = fill(w, h, |_, _| [0.1; 3]);
        let mut untouched = flat.clone();
        run(&mut untouched, w, h, &e);
        assert_eq!(untouched, flat);
    }

    #[test]
    fn fairy_lights_add_warm_orbs() {
        let (w, h) = (80, 80);
        let base = fill(w, h, |_, _| [0.05; 3]);
        let mut e = edit(Tool::FairyLights, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.03;
        e.tone = 0.8;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let lit = out
            .chunks_exact(3)
            .filter(|p| p[0] > 0.2 && p[0] > p[2])
            .count();
        assert!(lit > 30 && lit < w * h / 2, "{lit}");
    }

    #[test]
    fn rainbow_runs_red_to_violet_across_its_band() {
        let (w, h) = (80, 80);
        let base = fill(w, h, |_, _| [0.2; 3]);
        let mut e = edit(Tool::RainbowPrism, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.1;
        e.warmth = 0.0;
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        // Band horizontal through the centre: red on one side, blue on the other.
        let a = px(&out, w, 40, 32);
        let b = px(&out, w, 40, 48);
        assert!((a[0] - a[2]) * (b[0] - b[2]) < 0.0, "{a:?} {b:?}");
    }
}
