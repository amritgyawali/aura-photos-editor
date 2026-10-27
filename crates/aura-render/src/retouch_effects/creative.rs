//! Creative looks: tilt-shift, duotone, posterize, halftone, pencil sketch, oil paint,
//! cross-process, infrared, mirror symmetry, lens flare, god rays and weather. ADR-0132.
use super::{
    area, each, from_opponent, hash, mix, opponent, seed_of, smoothstep, weight, Frame, Snapshot,
};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::TiltShift
            | Tool::Duotone
            | Tool::Posterize
            | Tool::Halftone
            | Tool::PencilSketch
            | Tool::OilPaint
            | Tool::CrossProcess
            | Tool::Infrared
            | Tool::MirrorSymmetry
            | Tool::LensFlare
            | Tool::GodRays
            | Tool::Weather
    )
}

fn encode(v: f32) -> f32 {
    v.clamp(0.0, 1.0).powf(1.0 / 2.2)
}

fn decode(e: f32) -> f32 {
    e.clamp(0.0, 1.0).powf(2.2)
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let tone = edit.tone.clamp(0.0, 1.0);
    match edit.tool {
        Tool::Posterize => {
            let levels = 2.0 + (tone * 14.0).round();
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                (k > 0.0).then(|| {
                    let p =
                        old.map(|v| decode((encode(v) * (levels - 1.0)).round() / (levels - 1.0)));
                    mix(old, p, k)
                })
            });
        }
        Tool::Duotone => {
            let light = edit
                .target_color
                .map_or([0.95, 0.85, 0.65], crate::retouch_color::to_working);
            // The shadow colour: the light colour's opposite, turned by `warmth`, kept dark.
            let [l, cb, cr] = opponent(light);
            let (s, c) = ((1.0 + edit.warmth) * std::f32::consts::PI).sin_cos();
            let dark = from_opponent([
                0.02,
                (cb * c - cr * s) * 0.15 / l.max(1e-3),
                (cb * s + cr * c) * 0.15 / l.max(1e-3),
            ]);
            let contrast = 0.6 + tone * 0.8;
            each(rgb, frame, rect, |x, y, old| {
                let k = weight(edit, coverage, frame, x, y);
                (k > 0.0).then(|| {
                    let t = ((encode(luma(old)) - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
                    let toned: [f32; 3] = std::array::from_fn(|i| {
                        decode(encode(dark[i]) + (encode(light[i]) - encode(dark[i])) * t)
                    });
                    mix(old, toned, k)
                })
            });
        }
        Tool::CrossProcess => each(rgb, frame, rect, |x, y, old| {
            let k = weight(edit, coverage, frame, x, y);
            (k > 0.0).then(|| {
                let [r, g, b] = old.map(encode);
                // Red and green get a contrast curve; blue is lifted in the shadows and held
                // down in the highlights, which is what pushes highlights yellow.
                let s =
                    |v: f32, amount: f32| v + amount * (v - 0.5) * (1.0 - (2.0 * v - 1.0).abs());
                let crossed = [s(r, 0.6), s(g, 0.35), 0.12 + b * 0.72];
                mix(old, crossed.map(decode), k)
            })
        }),
        Tool::Infrared => each(rgb, frame, rect, |x, y, old| {
            let k = weight(edit, coverage, frame, x, y);
            (k > 0.0).then(|| {
                // Foliage reflects infrared strongly: green drives the brightness.
                let ir = (0.15 * old[0] + 1.25 * old[1] - 0.4 * old[2]).max(0.0);
                let mono = [ir; 3];
                // False colour: red and blue swapped over the infrared brightness.
                let l = luma(old).max(1e-4);
                let swapped = [old[2] * ir / l, old[1] * ir / l, old[0] * ir / l];
                mix(old, mix(mono, swapped, tone), k)
            })
        }),
        Tool::TiltShift => tilt_shift(rgb, frame, edit, coverage),
        Tool::Halftone => halftone(rgb, frame, edit, coverage),
        Tool::PencilSketch => sketch(rgb, frame, edit, coverage),
        Tool::OilPaint => oil(rgb, frame, edit, coverage),
        Tool::MirrorSymmetry => mirror(rgb, frame, edit, coverage),
        Tool::LensFlare => flare(rgb, frame, edit, coverage),
        Tool::GodRays => rays(rgb, frame, edit, coverage),
        _ => weather(rgb, frame, edit, coverage),
    }
}

fn tilt_shift(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
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
    let centre = frame.centre(edit);
    let reach = frame.reach(edit);
    // The sharp band is `tone` of the region's height either side of its centre line.
    let band = 0.1 + edit.tone.clamp(0.0, 1.0) * 0.5;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let d = ((y as f32 + 0.5 - centre[1]) / reach[1]).abs();
        let w = smoothstep(band, band + 0.4, d);
        let i = snapshot.index(x, y);
        let soft = [planes[0][i], planes[1][i], planes[2][i]];
        // Miniatures read as toys partly because they are saturated.
        let [l, cb, cr] = opponent(old);
        let vivid = from_opponent([l, cb * 1.25, cr * 1.25]);
        Some(mix(mix(old, vivid, k * 0.6), soft, k * w))
    });
}

fn halftone(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let cell = (edit.radius * frame.short()).max(3.0);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let ink = edit
        .target_color
        .map_or([0.01; 3], crate::retouch_color::to_working);
    let paper = [0.85_f32; 3];
    let (s, c) = (45.0_f32.to_radians()).sin_cos();
    // The tone of each screen cell, measured on its centre.
    let snapshot = Snapshot::take(rgb, frame, rect);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let u = (px * c + py * s) / cell;
        let v = (-px * s + py * c) / cell;
        let (cu, cv) = (u.floor() + 0.5, v.floor() + 0.5);
        let (cx, cy) = ((cu * c - cv * s) * cell, (cu * s + cv * c) * cell);
        let tone = encode(luma(snapshot.at(cx as isize, cy as isize)));
        // A dot whose area is the ink coverage the tone needs.
        let radius = ((1.0 - tone) / std::f32::consts::PI).sqrt();
        let d = (u - cu).hypot(v - cv);
        let inked = smoothstep(radius + 0.04, radius - 0.04, d);
        Some(mix(old, mix(paper, ink, inked), k))
    });
}

fn sketch(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).max(2);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let grey: Vec<f32> = snapshot.luma_plane().iter().map(|l| encode(*l)).collect();
    let inverted: Vec<f32> = grey.iter().map(|g| 1.0 - g).collect();
    let soft = blur(&inverted, r.w, r.h, radius);
    let keep = edit.tone.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        // Colour dodge of the grey by its blurred negative: flat areas go white, edges stay.
        let drawn = (grey[i] / (1.0 - soft[i]).max(1e-3)).min(1.0);
        let pencil = [decode(drawn); 3];
        let l = luma(old).max(1e-4);
        let tinted = old.map(|v| v / l * decode(drawn));
        Some(mix(old, mix(pencil, tinted, keep), k))
    });
}

fn oil(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let r = frame.radius_px(edit).clamp(2, 10);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, r + 1) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let sr = snapshot.rect();
    // Summed-area tables of each channel, luminance and its square: every quadrant's mean and
    // variance in four reads, whatever the radius.
    let (w, h) = (sr.w + 1, sr.h + 1);
    let mut tables = vec![vec![0.0_f64; w * h]; 5];
    for y in 0..sr.h {
        for x in 0..sr.w {
            let p = snapshot.at((sr.x0 + x) as isize, (sr.y0 + y) as isize);
            let l = luma(p);
            let values = [p[0], p[1], p[2], l, l * l];
            for (t, v) in tables.iter_mut().zip(values) {
                let i = (y + 1) * w + x + 1;
                t[i] = f64::from(v) + t[i - 1] + t[i - w] - t[i - w - 1];
            }
        }
    }
    let sum = |t: &[f64], x0: usize, y0: usize, x1: usize, y1: usize| {
        t[y1 * w + x1] - t[y0 * w + x1] - t[y1 * w + x0] + t[y0 * w + x0]
    };
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (lx, ly) = (x - sr.x0, y - sr.y0);
        let mut best = (f64::MAX, [0.0_f32; 3]);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let x0 = if dx == 0 { lx.saturating_sub(r) } else { lx };
            let y0 = if dy == 0 { ly.saturating_sub(r) } else { ly };
            let x1 = if dx == 0 {
                lx + 1
            } else {
                (lx + r + 1).min(sr.w)
            };
            let y1 = if dy == 0 {
                ly + 1
            } else {
                (ly + r + 1).min(sr.h)
            };
            let n = ((x1 - x0) * (y1 - y0)) as f64;
            if n <= 0.0 {
                continue;
            }
            let mean_l = sum(&tables[3], x0, y0, x1, y1) / n;
            let var = sum(&tables[4], x0, y0, x1, y1) / n - mean_l * mean_l;
            if var < best.0 {
                let colour = [0, 1, 2].map(|c| (sum(&tables[c], x0, y0, x1, y1) / n) as f32);
                best = (var, colour);
            }
        }
        Some(mix(old, best.1, k))
    });
}

fn mirror(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, frame_rect(frame));
    let centre = frame.centre(edit);
    let vertical = edit.tint > 0.0;
    // Which half is kept: the right (or lower) half when `warmth` is positive.
    let keep_after = edit.warmth > 0.0;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (p, c) = if vertical {
            (y as f32 + 0.5, centre[1])
        } else {
            (x as f32 + 0.5, centre[0])
        };
        let after = p > c;
        if after == keep_after {
            return None;
        }
        let m = 2.0 * c - p;
        let source = if vertical {
            snapshot.bilinear(x as f32 + 0.5, m)
        } else {
            snapshot.bilinear(m, y as f32 + 0.5)
        };
        Some(mix(old, source, k))
    });
}

fn frame_rect(frame: Frame) -> crate::retouch_planes::Rect {
    crate::retouch_planes::Rect {
        x0: 0,
        y0: 0,
        w: frame.width,
        h: frame.height,
    }
}

fn flare(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let light = frame.source_or_centre(edit);
    let middle = [frame.width as f32 / 2.0, frame.height as f32 / 2.0];
    let size = frame.short() * (0.02 + edit.radius * 2.0);
    // Ghosts on the line from the light through the middle: position, size, colour.
    let ghosts: [(f32, f32, [f32; 3]); 6] = [
        (0.0, 3.0, [1.0, 0.9, 0.7]),
        (0.45, 0.8, [0.4, 0.8, 0.5]),
        (0.75, 1.6, [0.3, 0.5, 1.0]),
        (1.2, 0.6, [1.0, 0.6, 0.3]),
        (1.55, 2.2, [0.6, 0.4, 1.0]),
        (1.85, 1.1, [0.5, 1.0, 0.9]),
    ];
    let strength = 0.15 + edit.tone.clamp(0.0, 1.0) * 0.5;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let mut add = [0.0_f32; 3];
        for (t, scale, colour) in ghosts {
            let gx = light[0] + (middle[0] - light[0]) * t;
            let gy = light[1] + (middle[1] - light[1]) * t;
            let d = (px - gx).hypot(py - gy) / (size * scale);
            // The light itself glows; ghosts are discs with a brighter rim.
            let g = if t == 0.0 {
                (-d * d * 2.0).exp()
            } else {
                smoothstep(1.0, 0.85, d) * (0.4 + 0.6 * smoothstep(0.5, 0.95, d)) * 0.35
            };
            for c in 0..3 {
                add[c] += colour[c] * g;
            }
        }
        Some(std::array::from_fn(|c| old[c] + add[c] * strength * k))
    });
}

fn rays(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    const SAMPLES: usize = 40;
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, frame_rect(frame));
    let light = frame.source_or_centre(edit);
    let threshold = 0.3 + (1.0 - edit.tone.clamp(0.0, 1.0)) * 0.6;
    let warm = edit
        .target_color
        .map_or([1.0, 0.85, 0.6], crate::retouch_color::to_working);
    let reach = 0.35 + edit.tone.clamp(0.0, 1.0) * 0.4;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        // March toward the light, collecting the bright gaps it shines through.
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let mut lit = 0.0_f32;
        let mut decay = 1.0_f32;
        for i in 0..SAMPLES {
            let t = i as f32 / SAMPLES as f32 * reach;
            let s = snapshot.bilinear(px + (light[0] - px) * t, py + (light[1] - py) * t);
            lit += (luma(s) - threshold).max(0.0) * decay;
            decay *= 0.96;
        }
        let g = lit / SAMPLES as f32 * 1.5 * k;
        Some(std::array::from_fn(|c| old[c] + warm[c] * g))
    });
}

fn weather(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let size = (edit.radius * frame.short()).max(1.0);
    let rain = edit.tint > 0.0;
    let density = 0.05 + edit.tone.clamp(0.0, 1.0) * 0.5;
    let seed = seed_of(edit);
    // Rain falls at `warmth` of 45 degrees from vertical.
    let (sa, ca) = (edit.warmth * std::f32::consts::FRAC_PI_4).sin_cos();
    let cell = (size * 4.0).ceil();
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        // Rain lives in a frame rotated so its streaks run down the cell's long axis.
        let (u, v) = if rain {
            (px * ca - py * sa, px * sa + py * ca)
        } else {
            (px, py)
        };
        let (gu, gv) = (
            (u / cell).floor() as i64,
            (v / (if rain { cell * 4.0 } else { cell })).floor() as i64,
        );
        let mut cover = 0.0_f32;
        for dv in -1..=1 {
            for du in -1..=1 {
                let (cu, cv) = (gu + du, gv + dv);
                if hash(cu, cv, seed) >= density {
                    continue;
                }
                let fu = (cu as f32 + hash(cu, cv, seed ^ 7)) * cell;
                let span = if rain { cell * 4.0 } else { cell };
                let fv = (cv as f32 + hash(cu, cv, seed ^ 9)) * span;
                let r = size * (0.4 + 0.6 * hash(cu, cv, seed ^ 0xb));
                let d = if rain {
                    // A thin streak four cells long.
                    ((u - fu) / (r * 0.35)).hypot((v - fv) / (r * 6.0))
                } else {
                    (u - fu).hypot(v - fv) / r
                };
                cover = cover.max(smoothstep(1.0, 0.4, d) * (0.5 + 0.5 * hash(cu, cv, seed ^ 0xd)));
            }
        }
        if cover <= 0.0 {
            return None;
        }
        let flake = if rain { 0.55 } else { 0.9 };
        Some(mix(
            old,
            [flake; 3],
            cover * k * if rain { 0.5 } else { 1.0 },
        ))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn posterize_leaves_only_a_few_levels() {
        let (w, h) = (64, 8);
        let base = fill(w, h, |x, _| [(x as f32 / 64.0).powf(2.2); 3]);
        // The whole frame: the corners of a smaller ellipse are rightly left alone.
        let mut e = edit(Tool::Posterize, [0.5, 0.5, 1.0, 1.0]);
        e.tone = 0.0;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let mut levels: Vec<u32> = out
            .chunks_exact(3)
            .map(|p| (p[0] * 1000.0) as u32)
            .collect();
        levels.sort_unstable();
        levels.dedup();
        assert!(levels.len() <= 2, "{levels:?}");
    }

    #[test]
    fn duotone_maps_dark_to_dark_and_light_to_the_chosen_colour() {
        let (w, h) = (20, 10);
        let base = fill(w, h, |x, _| if x < 10 { [0.01; 3] } else { [0.9; 3] });
        let mut e = edit(Tool::Duotone, [0.5, 0.5, 0.6, 0.6]);
        e.target_color = Some([1.0, 0.6, 0.2]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let light = px(&out, w, 15, 5);
        assert!(light[0] > light[2] * 2.0, "{light:?}");
        assert!(luma(px(&out, w, 5, 5)) < luma(light) * 0.2);
    }

    #[test]
    fn oil_paint_flattens_texture_but_keeps_a_boundary() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |x, y| {
            let n = super::super::hash(x as i64, y as i64, 3) * 0.1;
            if x < 30 {
                [0.2 + n; 3]
            } else {
                [0.7 + n; 3]
            }
        });
        let mut e = edit(Tool::OilPaint, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.1;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let spread = |rgb: &[f32]| {
            (10..20)
                .map(|x| px(rgb, w, x, 30)[0])
                .fold(0.0_f32, f32::max)
                - (10..20)
                    .map(|x| px(rgb, w, x, 30)[0])
                    .fold(1.0_f32, f32::min)
        };
        assert!(spread(&out) < spread(&base) * 0.6);
        assert!(px(&out, w, 33, 30)[0] - px(&out, w, 26, 30)[0] > 0.4);
    }

    #[test]
    fn mirror_makes_the_region_symmetric() {
        let (w, h) = (40, 20);
        let base = fill(w, h, |x, _| [x as f32 / 40.0; 3]);
        let mut e = edit(Tool::MirrorSymmetry, [0.5, 0.5, 0.5, 0.6]);
        e.feather = 0.0;
        e.warmth = -1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!((px(&out, w, 10, 10)[0] - px(&out, w, 29, 10)[0]).abs() < 0.01);
    }

    #[test]
    fn pencil_sketch_whitens_flat_areas_and_draws_edges() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |x, _| if x < 30 { [0.3; 3] } else { [0.05; 3] });
        let mut e = edit(Tool::PencilSketch, [0.5, 0.5, 0.6, 0.6]);
        e.tone = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 10, 30)) > 0.7);
        assert!(luma(px(&out, w, 31, 30)) < luma(px(&out, w, 10, 30)));
    }

    #[test]
    fn halftone_prints_dots_of_ink_on_paper() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |_, _| [0.2; 3]);
        let mut e = edit(Tool::Halftone, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.1;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let inked = out.chunks_exact(3).filter(|p| p[0] < 0.05).count();
        let paper = out.chunks_exact(3).filter(|p| p[0] > 0.7).count();
        assert!(inked > 100 && paper > 100, "{inked} {paper}");
        // A dark tone is mostly ink and a light one mostly paper - not the other way round.
        let light = fill(w, h, |_, _| [0.7; 3]);
        let mut lighter = light.clone();
        run(&mut lighter, w, h, &e);
        let ink = |rgb: &[f32]| rgb.chunks_exact(3).filter(|p| p[0] < 0.05).count();
        assert!(
            ink(&out) > ink(&lighter) * 2,
            "{} vs {}",
            ink(&out),
            ink(&lighter)
        );
    }

    #[test]
    fn infrared_turns_green_foliage_white() {
        let (w, h) = (10, 10);
        let base = fill(w, h, |_, _| [0.1, 0.5, 0.1]);
        let mut e = edit(Tool::Infrared, [0.5, 0.5, 0.6, 0.6]);
        e.tone = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let p = px(&out, w, 5, 5);
        assert!(p[0] > 0.5 && (p[0] - p[1]).abs() < 1e-5);
    }

    #[test]
    fn tilt_shift_keeps_the_band_sharp_and_blurs_away_from_it() {
        let (w, h) = (60, 60);
        let base = checker(w, h, 2);
        let mut e = edit(Tool::TiltShift, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.06;
        e.tone = 0.1;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let contrast = |y: usize| {
            (px(&out, w, 30, y)[0] - px(&out, w, 32, y)[0])
                .abs()
                .max((px(&out, w, 31, y)[0] - px(&out, w, 33, y)[0]).abs())
        };
        assert!(contrast(30) > contrast(5) * 2.0);
    }

    #[test]
    fn weather_adds_flakes_or_streaks() {
        let (w, h) = (80, 80);
        for rain in [false, true] {
            let base = fill(w, h, |_, _| [0.1; 3]);
            let mut e = edit(Tool::Weather, [0.5, 0.5, 0.6, 0.6]);
            e.radius = 0.02;
            e.tone = 0.8;
            e.tint = if rain { 1.0 } else { 0.0 };
            let mut out = base.clone();
            run(&mut out, w, h, &e);
            let lit = out.chunks_exact(3).filter(|p| p[0] > 0.2).count();
            // Flakes and streaks, not a white sheet.
            assert!(lit > 20 && lit < w * h / 3, "rain={rain}: {lit}");
        }
    }

    #[test]
    fn lens_flare_and_god_rays_add_light_toward_their_source() {
        let (w, h) = (80, 80);
        let base = fill(w, h, |x, y| {
            if (8..14).contains(&x) && (8..14).contains(&y) {
                [3.0; 3]
            } else {
                [0.05; 3]
            }
        });
        for tool in [Tool::LensFlare, Tool::GodRays] {
            let mut e = edit(tool, [0.5, 0.5, 0.6, 0.6]);
            e.source = Some([0.14, 0.14]);
            e.tone = 1.0;
            let mut out = base.clone();
            run(&mut out, w, h, &e);
            let added: f32 = out.iter().zip(&base).map(|(a, b)| a - b).sum();
            assert!(added > 1.0, "{tool:?}: {added}");
            // Light is added near the source, and most of the frame is left as it was.
            let untouched = out
                .iter()
                .zip(&base)
                .filter(|(a, b)| (*a - *b).abs() < 1e-3)
                .count();
            assert!(untouched > out.len() / 3, "{tool:?} lit the whole frame");
        }
    }
}
