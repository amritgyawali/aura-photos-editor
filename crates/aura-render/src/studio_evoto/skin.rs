//! Skin finishing measured on each face. ADR-0150 group D.
//!
//! Every adjustment reads the face it changes: the aegyo-sal sits on that face's lower lids,
//! the glow on its own brightest high points, the stubble is evened toward that person's own
//! cheeks, and the colour taken off the skin is the colour of the clothing actually beside it.
use aura_portrait::Region;
use aura_recipe::studio_finish::SkinFinish;

use super::{
    along, each, ellipse, from_opponent, local, masked_blur, mean_colour, mix, near_mouth,
    opponent, percentile, smoothstep, weights, with_luma, Ctx,
};
use crate::colour::luma;

pub(super) fn apply(rgb: &mut [f32], ctx: &Ctx<'_>, s: &SkinFinish) {
    let k = |v: f32| (v / 100.0).clamp(0.0, 1.0);
    if s.colour_spill > 0.0 {
        colour_spill(rgb, ctx, k(s.colour_spill));
    }
    if s.stubble > 0.0 {
        stubble(rgb, ctx, k(s.stubble));
    }
    if s.rosy > 0.0 {
        rosy(rgb, ctx, k(s.rosy));
    }
    if s.pores > 0.0 {
        pores(rgb, ctx, k(s.pores));
    }
    if s.lip_lines > 0.0 {
        lip_lines(rgb, ctx, k(s.lip_lines));
    }
    if s.teeth_evening > 0.0 {
        teeth_evening(rgb, ctx, k(s.teeth_evening));
    }
    if s.aegyo_sal > 0.0 {
        aegyo_sal(rgb, ctx, k(s.aegyo_sal));
    }
    if s.glow > 0.0 {
        glow(rgb, ctx, k(s.glow));
    }
    if s.beard_define > 0.0 {
        beard_define(rgb, ctx, k(s.beard_define));
    }
    if s.feature_sharpen > 0.0 {
        feature_sharpen(rgb, ctx, k(s.feature_sharpen));
    }
}

/// The "eye smile": a soft lift along each lower lid and a faint shade under it.
fn aegyo_sal(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let under = ctx.view(Region::UnderEyes);
    let eyes = ctx.soft(Region::Eyes, 1);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.0, 1.0, -0.2, 0.6);
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let mut lift = 0.0_f32;
            let mut shade = 0.0_f32;
            for side in [-0.5_f32, 0.5] {
                lift = lift.max(ellipse(p, [side, 0.21], [0.3, 0.075]));
                shade = shade.max(ellipse(p, [side, 0.33], [0.28, 0.06]));
            }
            let on = skin.at(x, y).max(under.at(x, y)) * (1.0 - eyes.at(x, y));
            if on <= 0.0 || (lift <= 0.0 && shade <= 0.0) {
                return None;
            }
            let gain = 1.0 + k * on * (0.2 * lift - 0.09 * shade);
            Some(old.map(|v| v * gain))
        });
    }
}

/// Vertical lines across the lips: smoothed along the lip, kept across it.
fn lip_lines(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let lips = ctx.view(Region::Lips);
    for f in &ctx.faces {
        let area = ctx.around(f, -0.8, 0.8, f.mouth - 0.35, f.mouth + 0.35);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let smooth = along(&l, area.width(), area.height(), f.u, (0.06 * f.io).max(2.0));
        each(rgb, ctx.w, area, |x, y, old| {
            let wgt = lips.at(x, y) * near_mouth(f, x, y) * k;
            if wgt <= 0.0 {
                return None;
            }
            let i = area.index(x, y);
            let lines = l[i] - smooth[i];
            Some(with_luma(old, l[i] - lines * 0.9 * wgt))
        });
    }
}

/// The face's high points, `0..=1`, in the face's own frame.
pub(super) fn high_points(p: [f32; 2], nose: f32, mouth: f32) -> f32 {
    let mut s = ellipse(p, [0.0, -0.8], [0.62, 0.34]);
    s = s.max(ellipse(p, [0.0, nose * 0.55], [0.14, nose * 0.55]));
    s = s.max(ellipse(p, [0.0, nose], [0.16, 0.13]));
    s = s.max(ellipse(p, [0.0, mouth + 0.62], [0.24, 0.16]));
    for side in [-1.0_f32, 1.0] {
        s = s.max(ellipse(p, [side * 0.7, 0.45], [0.38, 0.24]));
    }
    s
}

/// A dewy glow where the face's high points already catch the light.
fn glow(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.3, 1.3, -1.3, f.mouth + 0.9);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let soft = area.blur(&l, 0.1 * f.io);
        let samples = (area.y0..area.y1)
            .flat_map(|y| (area.x0..area.x1).map(move |x| (x, y)))
            .map(|(x, y)| (soft[area.index(x, y)], skin.at(x, y)));
        let Some(mid) = percentile(samples.clone(), 0.5) else {
            continue;
        };
        let top = percentile(samples, 0.95)
            .unwrap_or(mid * 1.3)
            .max(mid * 1.05);
        each(rgb, ctx.w, area, |x, y, old| {
            let spots = high_points(local(f, x, y), f.nose, f.mouth) * skin.at(x, y);
            if spots <= 0.0 {
                return None;
            }
            let lit = smoothstep(mid, top, soft[area.index(x, y)]);
            let g = k * spots * (0.35 + 0.65 * lit);
            let l = luma(old);
            // Strong enough to read as glow at full strength: half again on a high point
            // already in the light, a quarter where it is not.
            let brighter = old.map(|v| v * (1.0 + 0.5 * g));
            // A sheen carries less of the skin's colour than the skin does.
            Some(mix(brighter, [l * (1.0 + 0.5 * g); 3], 0.15 * g))
        });
    }
}

/// Mean colour of two cheek patches, the reference several adjustments measure against.
pub(super) fn cheeks(rgb: &[f32], ctx: &Ctx<'_>, f: &super::Face) -> Option<[f32; 3]> {
    let skin = ctx.view(Region::Skin);
    let area = ctx.around(f, -1.0, 1.0, 0.2, 0.75);
    mean_colour(rgb, ctx.w, area, |x, y| {
        let p = local(f, x, y);
        let patch =
            ellipse(p, [-0.62, 0.45], [0.24, 0.17]).max(ellipse(p, [0.62, 0.45], [0.24, 0.17]));
        patch * skin.at(x, y)
    })
}

/// Shaved stubble's blue-grey shadow evened toward the person's own cheeks. A beard - hair
/// much darker than the cheek - is left alone.
fn stubble(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let beard = ctx.view(Region::FacialHair);
    let mouth = ctx.soft(Region::Mouth, 2);
    for f in &ctx.faces {
        let Some(cheek) = cheeks(rgb, ctx, f) else {
            continue;
        };
        let reference = opponent(cheek);
        if reference[0] <= 1e-4 {
            continue;
        }
        let (rb, rr) = (reference[1] / reference[0], reference[2] / reference[0]);
        let area = ctx.around(f, -1.2, 1.2, 0.2, f.mouth + 1.15);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        // The local level of the skin alone, so the lips and the hair do not darken it.
        let on_skin = weights(area, |x, y| skin.at(x, y) * (1.0 - mouth.at(x, y)));
        let broad = masked_blur(area, &l, &on_skin, 0.2 * f.io);
        // Stubble is texture: many small dark dots, far more of them than the same person's
        // cheeks carry. Without that texture a darker or bluer jaw is light, powder or makeup,
        // and is left exactly as it is.
        let fine = masked_blur(area, &l, &on_skin, (0.015 * f.io).max(1.0));
        let dots: Vec<f32> = l
            .iter()
            .zip(&fine)
            .map(|(v, b)| ((b - v) / b.max(1e-5)).max(0.0))
            .collect();
        let density = masked_blur(area, &dots, &on_skin, 0.1 * f.io);
        let (mut cheek_dots, mut n) = (0.0_f64, 0.0_f64);
        for y in area.y0..area.y1 {
            for x in area.x0..area.x1 {
                let p = local(f, x, y);
                let patch = ellipse(p, [-0.62, 0.45], [0.24, 0.17]).max(ellipse(
                    p,
                    [0.62, 0.45],
                    [0.24, 0.17],
                )) * skin.at(x, y);
                if patch > 0.1 {
                    cheek_dots += f64::from(density[area.index(x, y)] * patch);
                    n += f64::from(patch);
                }
            }
        }
        if n < 4.0 {
            continue;
        }
        let cheek_dots = ((cheek_dots / n) as f32).max(1e-4);
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let textured = smoothstep(1.4, 2.2, density[area.index(x, y)] / cheek_dots);
            let zone = textured
                * ellipse(p, [0.0, f.mouth + 0.25], [1.05, 0.95])
                * (1.0 - mouth.at(x, y))
                * skin.at(x, y).max(beard.at(x, y) * 0.7);
            if zone <= 0.0 {
                return None;
            }
            let o = opponent(old);
            if o[0] <= 1e-5 {
                return None;
            }
            let (cb, cr) = (o[1] / o[0], o[2] / o[0]);
            let local_l = broad[area.index(x, y)].max(1e-5);
            let darkness = 1.0 - local_l / reference[0];
            let blueness = (cb - rb) + (rr - cr);
            // Hair is far darker than a shadow; a shadow is a little darker and bluer.
            let hairless = smoothstep(0.35, 0.6, o[0] / reference[0]);
            let t = k * zone * hairless * smoothstep(0.0, 0.18, darkness.max(blueness * 2.0));
            if t <= 0.0 {
                return None;
            }
            let lift = (reference[0] / local_l).clamp(1.0, 1.8);
            let y2 = o[0] * (1.0 + (lift - 1.0) * t * 0.8);
            let cb2 = cb + (rb - cb) * t;
            let cr2 = cr + (rr - cr) * t;
            Some(from_opponent([y2, cb2 * y2, cr2 * y2]))
        });
    }
}

/// A healthier complexion: the skin's colour turned a little from yellow toward red.
fn rosy(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let face = ctx.soft(Region::Face, 3);
    let angle = -k * 0.16;
    let (s, c) = angle.sin_cos();
    for f in &ctx.faces {
        let area = ctx.around(f, -1.4, 1.4, -1.5, f.mouth + 1.0);
        each(rgb, ctx.w, area, |x, y, old| {
            let wgt = skin.at(x, y) * face.at(x, y);
            if wgt <= 0.0 {
                return None;
            }
            let o = opponent(old);
            // Rotate (cb, cr) toward +cr, the red axis, and add a touch of colour.
            let (cb, cr) = (o[1], o[2]);
            let gain = 1.0 + 0.06 * k;
            let cb2 = (cb * c - cr * s) * gain;
            let cr2 = (cb * s + cr * c) * gain;
            let t = wgt;
            Some(from_opponent([
                o[0],
                cb + (cb2 - cb) * t,
                cr + (cr2 - cr) * t,
            ]))
        });
    }
}

/// A darker or yellower tooth brought toward the others; nothing is lifted past the brightest.
fn teeth_evening(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let teeth = ctx.view(Region::Teeth);
    if !teeth.present() {
        return;
    }
    for f in &ctx.faces {
        let area = ctx.around(f, -0.7, 0.7, f.mouth - 0.3, f.mouth + 0.3);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let on_teeth = weights(area, |x, y| teeth.at(x, y));
        let local_l = masked_blur(area, &l, &on_teeth, (0.05 * f.io).max(1.5));
        let samples = || {
            (area.y0..area.y1)
                .flat_map(|y| (area.x0..area.x1).map(move |x| (x, y)))
                .map(|(x, y)| (l[area.index(x, y)], teeth.at(x, y)))
        };
        let (Some(median), Some(bright)) =
            (percentile(samples(), 0.5), percentile(samples(), 0.85))
        else {
            continue;
        };
        let Some(mean) = mean_colour(rgb, ctx.w, area, |x, y| {
            if teeth.at(x, y) > 0.5 {
                1.0
            } else {
                0.0
            }
        }) else {
            continue;
        };
        let m = opponent(mean);
        let (mb, mr) = (m[1] / m[0].max(1e-5), m[2] / m[0].max(1e-5));
        each(rgb, ctx.w, area, |x, y, old| {
            let wgt = teeth.at(x, y) * k;
            let here = local_l[area.index(x, y)];
            if wgt <= 0.0 || here >= median || here <= 1e-5 {
                return None;
            }
            let o = opponent(old);
            let gain = (median / here).min(1.6);
            let y2 = (o[0] * (1.0 + (gain - 1.0) * wgt)).min(bright.max(o[0]));
            let darker = smoothstep(0.0, 0.3, 1.0 - here / median);
            let t = wgt * darker;
            let (cb, cr) = (o[1] / o[0].max(1e-5), o[2] / o[0].max(1e-5));
            Some(from_opponent([
                y2,
                (cb + (mb - cb) * t) * y2,
                (cr + (mr - cr) * t) * y2,
            ]))
        });
    }
}

/// Sharpen eyes, lashes, brows and lips, never the skin around them.
fn feature_sharpen(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let eyes = ctx.soft(Region::Eyes, 2);
    let brows = ctx.view(Region::Eyebrows);
    let lips = ctx.view(Region::Lips);
    let iris = ctx.view(Region::Iris);
    let skin = ctx.view(Region::Skin);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.0, 1.0, -0.65, f.mouth + 0.35);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let soft = area.blur(&l, (0.025 * f.io).max(1.0));
        each(rgb, ctx.w, area, |x, y, old| {
            let features = eyes
                .at(x, y)
                .max(brows.at(x, y))
                .max(lips.at(x, y) * near_mouth(f, x, y))
                .max(iris.at(x, y));
            let wgt = features * (1.0 - 0.7 * skin.at(x, y)) * k;
            if wgt <= 0.0 {
                return None;
            }
            let i = area.index(x, y);
            let detail = (l[i] - soft[i]).clamp(-0.5 * l[i], 0.5 * l[i]);
            Some(with_luma(old, l[i] + detail * 1.2 * wgt))
        });
    }
}

/// A beard or moustache deepened and defined.
fn beard_define(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let beard = ctx.view(Region::FacialHair);
    if !beard.present() {
        return;
    }
    for f in &ctx.faces {
        let area = ctx.around(f, -1.3, 1.3, f.nose - 0.3, f.mouth + 1.3);
        if area.is_empty() {
            continue;
        }
        let Some(cheek) = cheeks(rgb, ctx, f) else {
            continue;
        };
        let cheek_l = luma(cheek).max(1e-5);
        let l = area.luma(rgb, ctx.w);
        let soft = area.blur(&l, (0.04 * f.io).max(1.0));
        let local_l = area.blur(&l, (0.08 * f.io).max(2.0));
        each(rgb, ctx.w, area, |x, y, old| {
            // A beard is hair, well darker than the skin; a region the parse called facial hair
            // that is as light as the cheeks - powder, a shadow, red sindoor - is left alone.
            let hair = 1.0 - smoothstep(0.55, 0.8, local_l[area.index(x, y)] / cheek_l);
            let wgt = beard.at(x, y) * hair * k;
            if wgt <= 0.0 {
                return None;
            }
            let i = area.index(x, y);
            let detail = (l[i] - soft[i]).clamp(-0.5 * l[i], 0.5 * l[i]);
            let target = (l[i] + detail * 0.8 * wgt) * (1.0 - 0.14 * wgt);
            Some(with_luma(old, target))
        });
    }
}

/// Colour cast onto skin by bright clothing beside it, taken out along that colour only.
fn colour_spill(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let clothing = ctx.view(Region::Clothing);
    let near = ctx.soft(Region::Clothing, 14);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.7, 1.7, -1.6, f.mouth + 2.0);
        if area.is_empty() {
            continue;
        }
        // The face's centre - nose and inner cheeks - is the skin least reached by a spill.
        let Some(centre) = mean_colour(rgb, ctx.w, area, |x, y| {
            ellipse(local(f, x, y), [0.0, 0.45], [0.55, 0.32]) * skin.at(x, y)
        }) else {
            continue;
        };
        let Some(cloth) = mean_colour(rgb, ctx.w, area, |x, y| {
            let i = (y * ctx.w + x) * 3;
            let p = [rgb[i], rgb[i + 1], rgb[i + 2]];
            clothing.at(x, y) * saturation(p)
        }) else {
            continue;
        };
        let r = normalised(centre);
        let c = normalised(cloth);
        let dir = [c[0] - r[0], c[1] - r[1]];
        let len = dir[0].hypot(dir[1]);
        if len < 0.05 {
            continue;
        }
        let unit = [dir[0] / len, dir[1] / len];
        each(rgb, ctx.w, area, |x, y, old| {
            let wgt = skin.at(x, y) * k;
            if wgt <= 0.0 {
                return None;
            }
            let o = opponent(old);
            if o[0] <= 1e-5 {
                return None;
            }
            let here = [o[1] / o[0], o[2] / o[0]];
            let along = (here[0] - r[0]) * unit[0] + (here[1] - r[1]) * unit[1];
            if along <= 0.0 {
                return None;
            }
            let t = wgt * (0.45 + 0.55 * near.at(x, y));
            let cb = here[0] - unit[0] * along * t;
            let cr = here[1] - unit[1] * along * t;
            Some(from_opponent([o[0], cb * o[0], cr * o[0]]))
        });
    }
}

fn saturation(p: [f32; 3]) -> f32 {
    let hi = p[0].max(p[1]).max(p[2]);
    let lo = p[0].min(p[1]).min(p[2]);
    if hi <= 1e-5 {
        0.0
    } else {
        smoothstep(0.25, 0.7, (hi - lo) / hi)
    }
}

/// `(cb, cr)` per unit of luminance.
fn normalised(p: [f32; 3]) -> [f32; 2] {
    let o = opponent(p);
    let y = o[0].max(1e-5);
    [o[1] / y, o[2] / y]
}

/// Pore-sized texture on the nose and inner cheeks reduced; finer grain and larger shape kept.
fn pores(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let nose = ctx.soft(Region::Nose, 2);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.2, 1.2, -1.2, f.mouth + 0.3);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let on_skin = weights(area, |x, y| skin.at(x, y));
        let fine = masked_blur(area, &l, &on_skin, (0.012 * f.io).max(1.0));
        let coarse = masked_blur(area, &l, &on_skin, (0.05 * f.io).max(2.0));
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let mut zone = nose.at(x, y);
            for side in [-1.0_f32, 1.0] {
                zone = zone.max(ellipse(p, [side * 0.5, 0.45], [0.38, 0.28]));
            }
            zone = zone.max(ellipse(p, [0.0, -0.8], [0.45, 0.25]) * 0.6);
            let wgt = zone * skin.at(x, y) * k;
            if wgt <= 0.0 {
                return None;
            }
            let i = area.index(x, y);
            let band = fine[i] - coarse[i];
            // A pore is a small dip; a nostril or a fold is a large one and is not texture.
            let small = 1.0 - smoothstep(0.08, 0.2, band.abs() / coarse[i].max(1e-5));
            // Pores are dark: their side of the band goes most of the way, the bright side half.
            let removed = if band < 0.0 { band * 0.8 } else { band * 0.4 };
            Some(with_luma(old, l[i] - removed * wgt * small))
        });
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::super::testing::{self, *};
    use super::*;
    use aura_recipe::studio_finish::SkinFinish;

    fn run(s: SkinFinish) -> (Vec<f32>, Vec<f32>) {
        let before = testing::frame();
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(&mut after, &ctx, &s);
        (before, after)
    }

    #[test]
    fn nothing_moves_outside_the_face_for_any_skin_adjustment() {
        let all = SkinFinish {
            aegyo_sal: 100.0,
            lip_lines: 100.0,
            glow: 100.0,
            stubble: 100.0,
            rosy: 100.0,
            teeth_evening: 100.0,
            feature_sharpen: 100.0,
            beard_define: 100.0,
            colour_spill: 100.0,
            pores: 100.0,
        };
        let (before, after) = run(all);
        assert_eq!(moved(&before, &after, Region::Background, false, 1e-6), 0);
        assert_eq!(moved(&before, &after, Region::Hair, false, 1e-6), 0);
        assert_eq!(moved(&before, &after, Region::Clothing, false, 1e-6), 0);
    }

    #[test]
    fn aegyo_sal_lifts_skin_just_under_the_eyes_and_nothing_inside_them() {
        let (before, after) = run(SkinFinish {
            aegyo_sal: 100.0,
            ..SkinFinish::default()
        });
        let under = px(&after, 100, 109);
        assert!(
            luma(under) > luma(px(&before, 100, 109)) * 1.05,
            "{under:?}"
        );
        assert_eq!(moved(&before, &after, Region::Eyes, false, 1e-6), 0);
    }

    #[test]
    fn rosy_turns_skin_redder_without_changing_its_brightness() {
        let (before, after) = run(SkinFinish {
            rosy: 100.0,
            ..SkinFinish::default()
        });
        let (a, b) = (px(&before, 120, 170), px(&after, 120, 170));
        assert!((luma(a) - luma(b)).abs() < 1e-4);
        let red = |p: [f32; 3]| p[0] / p[2].max(1e-5);
        let green = |p: [f32; 3]| p[1] / p[0].max(1e-5);
        assert!(green(b) < green(a), "{a:?} -> {b:?}");
        assert!(red(b) > red(a) * 0.98, "{a:?} -> {b:?}");
    }

    #[test]
    fn glow_brightens_the_forehead_and_not_the_jaw_edge() {
        let (before, after) = run(SkinFinish {
            glow: 100.0,
            ..SkinFinish::default()
        });
        assert!(luma(px(&after, 120, 70)) > luma(px(&before, 120, 70)) * 1.04);
        assert!((luma(px(&after, 78, 150)) - luma(px(&before, 78, 150))).abs() < 1e-5);
    }

    #[test]
    fn lip_lines_are_smoothed_along_the_lip() {
        let mut before = testing::frame();
        // Paint vertical lines across the lips.
        for y in 145..156 {
            for x in (106..134).step_by(4) {
                let i = (y * W + x) * 3;
                if part(x, y) == Region::Lips {
                    for c in 0..3 {
                        before[i + c] *= 0.6;
                    }
                }
            }
        }
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(
            &mut after,
            &ctx,
            &SkinFinish {
                lip_lines: 100.0,
                ..SkinFinish::default()
            },
        );
        let spread = |rgb: &[f32]| {
            let row: Vec<f32> = (108..132).map(|x| luma(px(rgb, x, 145))).collect();
            let mean = row.iter().sum::<f32>() / row.len() as f32;
            row.iter().map(|v| (v - mean).abs()).sum::<f32>()
        };
        assert!(spread(&after) < spread(&before) * 0.5);
    }

    #[test]
    fn colour_spill_takes_out_red_from_clothing_beside_the_skin() {
        let mut before = testing::frame();
        // The neck, next to red clothing, has taken some of its colour.
        for y in 190..205 {
            for x in 102..138 {
                let i = (y * W + x) * 3;
                before[i] *= 1.25;
                before[i + 1] *= 0.92;
            }
        }
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(
            &mut after,
            &ctx,
            &SkinFinish {
                colour_spill: 100.0,
                ..SkinFinish::default()
            },
        );
        let redness = |p: [f32; 3]| p[0] / luma(p);
        assert!(redness(px(&after, 120, 200)) < redness(px(&before, 120, 200)));
        // The face centre, the reference, is untouched.
        let centre_moved = (luma(px(&after, 120, 120)) - luma(px(&before, 120, 120))).abs();
        assert!(centre_moved < 1e-4);
    }

    #[test]
    fn choosing_another_face_leaves_this_one_alone() {
        let before = testing::frame();
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[[0.05, 0.05]]);
        apply(
            &mut after,
            &ctx,
            &SkinFinish {
                glow: 100.0,
                rosy: 100.0,
                ..SkinFinish::default()
            },
        );
        assert_eq!(before, after);
    }
}
