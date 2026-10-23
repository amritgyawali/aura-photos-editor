//! The people as a whole. ADR-0150 group I, and the tonal half of the double-chin slider.
//!
//! A group's faces are balanced against the group's own median face, never a target level; a
//! backlit subject is lifted by as much as its own contrast with the background asks for; and
//! the relight's shape comes from the person's own silhouette.
use aura_portrait::{Plane, Region};
use aura_recipe::studio_finish::SubjectFinish;

use super::{
    each, ellipse, from_opponent, local, masked_blur, mean_colour, opponent, percentile,
    smoothstep, weights, Ctx, OwnedView,
};
use crate::colour::luma;

pub(super) fn apply(rgb: &mut [f32], ctx: &Ctx<'_>, s: &SubjectFinish) {
    let k = |v: f32| (v / 100.0).clamp(0.0, 1.0);
    if s.backlight > 0.0 {
        backlight(rgb, ctx, k(s.backlight));
    }
    if s.exposure != 0.0 || s.warmth != 0.0 {
        exposure_and_warmth(rgb, ctx, s.exposure / 100.0, s.warmth / 100.0);
    }
    if s.group_balance > 0.0 {
        group_balance(rgb, ctx, k(s.group_balance));
    }
    if s.group_skin > 0.0 {
        group_skin(rgb, ctx, k(s.group_skin));
    }
    if s.harsh_shadows > 0.0 {
        harsh_shadows(rgb, ctx, k(s.harsh_shadows));
    }
    if s.face_brightness != 0.0 {
        face_brightness(rgb, ctx, s.face_brightness / 100.0);
    }
    if s.relight > 0.0 {
        relight(rgb, ctx, k(s.relight), s.relight_direction);
    }
    if s.clarity != 0.0 {
        clarity(rgb, ctx, (s.clarity / 100.0).clamp(-1.0, 1.0));
    }
}

/// The people's exposure (up to a stop either way) and warmth; the background is unchanged.
fn exposure_and_warmth(rgb: &mut [f32], ctx: &Ctx<'_>, exposure: f32, warmth: f32) {
    let person = ctx.soft(Region::Body, 1);
    let Some(area) = ctx.bounds(Region::Body, 0.02) else {
        return;
    };
    let gain = 2.0_f32.powf(exposure);
    each(rgb, ctx.w, area, |x, y, old| {
        let t = person.at(x, y);
        if t <= 0.0 {
            return None;
        }
        let l = luma(old);
        let warmed = [
            old[0] * (1.0 + 0.12 * warmth),
            old[1],
            old[2] * (1.0 - 0.12 * warmth),
        ];
        let target = super::with_luma(warmed, l * gain);
        Some(super::mix(old, target, t))
    });
}

/// Local contrast on the people; skin takes a quarter of it. Negative softens.
fn clarity(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let person = ctx.soft(Region::Body, 1);
    let skin = ctx.soft(Region::Skin, 1);
    let body_skin = ctx.soft(Region::BodySkin, 1);
    let Some(area) = ctx.bounds(Region::Body, 0.02) else {
        return;
    };
    let l = area.luma(rgb, ctx.w);
    let base = area.blur(&l, 0.2 * ctx.face_unit());
    each(rgb, ctx.w, area, |x, y, old| {
        let on_skin = skin.at(x, y).max(body_skin.at(x, y));
        let t = person.at(x, y) * (1.0 - 0.75 * on_skin) * k;
        if t == 0.0 {
            return None;
        }
        let i = area.index(x, y);
        let detail = (l[i] - base[i]).clamp(-0.6 * l[i], 0.6 * l[i]);
        Some(super::with_luma(old, (l[i] + detail * 0.8 * t).max(0.0)))
    });
}

/// Each face's median skin luminance, with the face it belongs to.
fn face_levels(rgb: &[f32], ctx: &Ctx<'_>) -> Vec<Option<f32>> {
    let skin = ctx.view(Region::Skin);
    let face = ctx.view(Region::Face);
    ctx.faces
        .iter()
        .map(|f| {
            let area = ctx.around(f, -1.0, 1.0, -0.8, f.mouth + 0.5);
            let samples = (area.y0..area.y1)
                .step_by(2)
                .flat_map(|y| (area.x0..area.x1).step_by(2).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let i = (y * ctx.w + x) * 3;
                    (
                        luma([rgb[i], rgb[i + 1], rgb[i + 2]]),
                        skin.at(x, y) * face.at(x, y),
                    )
                });
            percentile(samples, 0.5)
        })
        .collect()
}

/// The region a face's person-level change covers: the head and shoulders around it.
fn around_face(f: &super::Face, x: usize, y: usize) -> f32 {
    ellipse(local(f, x, y), [0.0, 0.7], [1.9, 2.7])
}

/// Darker faces in a group lifted toward the group's median face. Never darkens anyone.
fn group_balance(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let levels = face_levels(rgb, ctx);
    let mut known: Vec<f32> = levels.iter().flatten().copied().collect();
    if known.len() < 2 {
        return;
    }
    known.sort_by(f32::total_cmp);
    let median = known[known.len() / 2];
    let person = ctx.soft(Region::Body, 1);
    for (f, level) in ctx.faces.iter().zip(&levels) {
        let Some(level) = level else {
            continue;
        };
        if *level >= median * 0.98 {
            continue;
        }
        let gain = (median / level.max(1e-5)).powf(k).min(1.8);
        let area = ctx.around(f, -2.0, 2.0, -1.6, 3.6);
        each(rgb, ctx.w, area, |x, y, old| {
            let t = around_face(f, x, y) * person.at(x, y);
            (t > 0.0).then(|| old.map(|v| v * (1.0 + (gain - 1.0) * t)))
        });
    }
}

/// Each face's skin colour pulled toward the group's median skin colour.
fn group_skin(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let colours: Vec<Option<[f32; 2]>> = ctx
        .faces
        .iter()
        .map(|f| {
            let area = ctx.around(f, -1.0, 1.0, -0.8, f.mouth + 0.5);
            mean_colour(rgb, ctx.w, area, |x, y| skin.at(x, y)).map(|c| {
                let o = opponent(c);
                [o[1] / o[0].max(1e-5), o[2] / o[0].max(1e-5)]
            })
        })
        .collect();
    let known: Vec<[f32; 2]> = colours.iter().flatten().copied().collect();
    if known.len() < 2 {
        return;
    }
    let median = |c: usize| {
        let mut v: Vec<f32> = known.iter().map(|p| p[c]).collect();
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    let target = [median(0), median(1)];
    for (f, own) in ctx.faces.iter().zip(&colours) {
        let Some(own) = own else {
            continue;
        };
        let shift = [
            (target[0] - own[0]) * k * 0.7,
            (target[1] - own[1]) * k * 0.7,
        ];
        if shift[0].hypot(shift[1]) < 1e-4 {
            continue;
        }
        let area = ctx.around(f, -2.0, 2.0, -1.6, 3.6);
        each(rgb, ctx.w, area, |x, y, old| {
            let t = around_face(f, x, y) * skin.at(x, y);
            if t <= 0.0 {
                return None;
            }
            let o = opponent(old);
            Some(from_opponent([
                o[0],
                o[1] + shift[0] * t * o[0],
                o[2] + shift[1] * t * o[0],
            ]))
        });
    }
}

/// Hard shadows across a face softened toward the face's own median.
fn harsh_shadows(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let face = ctx.soft(Region::Face, 2);
    let levels = face_levels(rgb, ctx);
    for (f, level) in ctx.faces.iter().zip(&levels) {
        let Some(median) = level else {
            continue;
        };
        let area = ctx.around(f, -1.4, 1.4, -1.5, f.mouth + 1.0);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let on = weights(area, |x, y| skin.at(x, y));
        let low = masked_blur(area, &l, &on, 0.25 * f.io);
        each(rgb, ctx.w, area, |x, y, old| {
            let t = skin.at(x, y) * face.at(x, y) * k;
            if t <= 0.0 {
                return None;
            }
            let shadow = ((median - low[area.index(x, y)]) / median.max(1e-5)).max(0.0);
            let gain = (1.0 + 0.9 * shadow * t).min(1.8);
            (gain > 1.0).then(|| old.map(|v| v * gain))
        });
    }
}

/// A backlit, dark subject lifted while the bright background stays: by as much as the
/// subject's own contrast with the background asks for, most in its shadows.
fn backlight(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let crisp = ctx.view(Region::Body);
    let person = ctx.soft(Region::Body, 1);
    let background = ctx.view(Region::Background);
    let whole = ctx.whole();
    let mean_l =
        |weight: &dyn Fn(usize, usize) -> f32| mean_colour(rgb, ctx.w, whole, weight).map(luma);
    let (Some(subject), Some(behind)) = (
        mean_l(&|x, y| crisp.at(x, y)),
        mean_l(&|x, y| background.at(x, y)),
    ) else {
        return;
    };
    let ratio = subject / behind.max(1e-5);
    if ratio >= 0.7 {
        return;
    }
    let gain = (0.7 / ratio.max(0.05)).powf(k).min(4.0);
    let Some(area) = ctx.bounds(Region::Body, 0.02) else {
        return;
    };
    each(rgb, ctx.w, area, |x, y, old| {
        let t = person.at(x, y);
        if t <= 0.0 {
            return None;
        }
        let l = luma(old);
        let shadows = 1.0 - smoothstep(subject, subject * 4.0, l);
        let g = 1.0 + (gain - 1.0) * shadows * t;
        Some(old.map(|v| v * g))
    });
}

/// The faces alone, brighter or darker.
fn face_brightness(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let face = ctx.soft(Region::Face, 4);
    let neck = ctx.soft(Region::Neck, 3);
    let gain = 2.0_f32.powf(k * 0.7);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.6, 1.6, -1.8, f.mouth + 2.0);
        each(rgb, ctx.w, area, |x, y, old| {
            let t = face.at(x, y).max(neck.at(x, y) * 0.6);
            (t > 0.0).then(|| old.map(|v| v * (1.0 + (gain - 1.0) * t)))
        });
    }
}

/// A soft directional light across the whole person, shaped by their own silhouette: the
/// person's plane, blurred, is read as a relief whose slope faces the light or does not.
fn relight(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32, direction: f32) {
    let Some(plane) = ctx.map.plane(Region::Body) else {
        return;
    };
    let (pw, ph) = (plane.width as usize, plane.height as usize);
    let relief = crate::bands::blur(&plane.values, pw, ph, (pw.max(ph) / 40).max(2));
    let angle = direction.to_radians();
    let light = {
        let l = [angle.sin(), -0.35, angle.cos()];
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        l.map(|v| v / n)
    };
    let mut shade = vec![0.0_f32; pw * ph];
    let at = |x: usize, y: usize| relief[y.min(ph - 1) * pw + x.min(pw - 1)];
    let (mut sum, mut n) = (0.0_f64, 0.0_f64);
    for y in 0..ph {
        for x in 0..pw {
            let gx = at(x + 1, y) - at(x.saturating_sub(1), y);
            let gy = at(x, y + 1) - at(x, y.saturating_sub(1));
            // A surface leaning out of the silhouette faces away from its centre.
            let normal = [-gx * 6.0, -gy * 6.0, 1.0];
            let len = (normal[0] * normal[0] + normal[1] * normal[1] + 1.0).sqrt();
            let lit = ((normal[0] * light[0] + normal[1] * light[1] + light[2]) / len).max(0.0);
            shade[y * pw + x] = lit;
            if plane.values[y * pw + x] > 0.5 {
                sum += f64::from(lit);
                n += 1.0;
            }
        }
    }
    if n < 1.0 {
        return;
    }
    let mean = (sum / n) as f32;
    let shade = OwnedView::new(
        Some(Plane::from_values(plane.width, plane.height, shade)),
        ctx.w,
        ctx.h,
    );
    let person = ctx.soft(Region::Body, 1);
    let Some(area) = ctx.bounds(Region::Body, 0.02) else {
        return;
    };
    each(rgb, ctx.w, area, |x, y, old| {
        let t = person.at(x, y) * k;
        if t <= 0.0 {
            return None;
        }
        let g = (1.0 + 0.6 * t * (shade.at(x, y) - mean)).max(0.3);
        Some(old.map(|v| v * g))
    });
}

/// The fold under a double chin, lifted toward the skin around it. The shape half of the
/// slider is a warp in `crate::studio_finish`.
pub(super) fn double_chin_shade(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.0, 1.0, f.chin - 0.3, f.chin + 0.6);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let on = weights(area, |x, y| skin.at(x, y));
        let level = masked_blur(area, &l, &on, 0.15 * f.io);
        each(rgb, ctx.w, area, |x, y, old| {
            let zone = ellipse(local(f, x, y), [0.0, f.chin + 0.15], [0.75, 0.25]) * skin.at(x, y);
            if zone <= 0.0 {
                return None;
            }
            let i = area.index(x, y);
            let fold = ((level[i] - l[i]) / level[i].max(1e-5)).max(0.0);
            let gain = 1.0 + 0.8 * fold * zone * k;
            (gain > 1.0).then(|| old.map(|v| v * gain))
        });
    }
}

/// Darker corners that never reach a face.
pub(super) fn face_vignette(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let person = ctx.soft(Region::Body, 4);
    let (cx, cy) = (ctx.w as f32 * 0.5, ctx.h as f32 * 0.5);
    let reach = cx.hypot(cy);
    each(rgb, ctx.w, ctx.whole(), |x, y, old| {
        let r = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) / reach;
        let v = smoothstep(0.35, 1.0, r) * k * 0.65;
        if v <= 0.0 {
            return None;
        }
        let mut protect = person.at(x, y) * 0.5;
        for f in &ctx.faces {
            protect = protect.max(ellipse(local(f, x, y), [0.0, 0.6], [2.6, 3.2]));
        }
        let dark = v * (1.0 - protect.clamp(0.0, 1.0));
        (dark > 0.0).then(|| old.map(|c| c * (1.0 - dark)))
    });
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::super::testing::{self, *};
    use super::*;
    use aura_portrait::FaceGeometry;

    fn run_with(
        rgb: Vec<f32>,
        map: &aura_portrait::PortraitMap,
        s: SubjectFinish,
    ) -> (Vec<f32>, Vec<f32>) {
        let before = rgb.clone();
        let mut after = rgb;
        let ctx = Ctx::new(W, H, map, &[]);
        apply(&mut after, &ctx, &s);
        (before, after)
    }

    #[test]
    fn subject_exposure_brightens_the_person_and_not_the_background() {
        let (before, after) = run_with(
            testing::frame(),
            &testing::map(),
            SubjectFinish {
                exposure: 50.0,
                ..SubjectFinish::default()
            },
        );
        assert!(luma(px(&after, 120, 120)) > luma(px(&before, 120, 120)) * 1.35);
        assert_eq!(px(&before, 5, 5), px(&after, 5, 5));
    }

    #[test]
    fn a_vignette_darkens_corners_and_spares_the_face() {
        let before = testing::frame();
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        face_vignette(&mut after, &ctx, 1.0);
        assert!(luma(px(&after, 2, 238)) < luma(px(&before, 2, 238)) * 0.7);
        assert_eq!(px(&before, 120, 120), px(&after, 120, 120));
    }

    #[test]
    fn group_balance_lifts_the_darker_face_toward_the_other_and_never_darkens() {
        // Two faces: the left one painted a stop darker.
        let mut map = testing::map();
        let shift = |g: FaceGeometry, dx: f32| FaceGeometry {
            center: [g.center[0] + dx, g.center[1]],
            left_eye: [g.left_eye[0] + dx, g.left_eye[1]],
            right_eye: [g.right_eye[0] + dx, g.right_eye[1]],
            nose: [g.nose[0] + dx, g.nose[1]],
            mouth: [g.mouth[0] + dx, g.mouth[1]],
            size: 50.0,
            ..g
        };
        let base = testing::face();
        map.faces = vec![shift(base, -60.0), shift(base, 60.0)];
        let mut rgb = vec![0.0_f32; W * H * 3];
        let mut skin = vec![0.0_f32; W * H];
        for y in 0..H {
            for x in 0..W {
                let i = (y * W + x) * 3;
                let left =
                    ((x as f32 - 60.0) / 22.0).powi(2) + ((y as f32 - 115.0) / 30.0).powi(2) < 1.0;
                let right =
                    ((x as f32 - 180.0) / 22.0).powi(2) + ((y as f32 - 115.0) / 30.0).powi(2) < 1.0;
                let c = if left {
                    SKIN.map(|v| v * 0.5)
                } else if right {
                    SKIN
                } else {
                    BACKGROUND
                };
                rgb[i..i + 3].copy_from_slice(&c);
                if left || right {
                    skin[y * W + x] = 1.0;
                }
            }
        }
        for (r, plane) in &mut map.planes {
            if matches!(r, Region::Skin | Region::Face | Region::Body) {
                plane.values.clone_from(&skin);
            } else if *r == Region::Background {
                plane.values = skin.iter().map(|v| 1.0 - v).collect();
            } else {
                plane.values.fill(0.0);
            }
        }
        let (before, after) = run_with(
            rgb,
            &map,
            SubjectFinish {
                group_balance: 100.0,
                ..SubjectFinish::default()
            },
        );
        assert!(luma(px(&after, 60, 115)) > luma(px(&before, 60, 115)) * 1.6);
        assert_eq!(px(&before, 180, 115), px(&after, 180, 115));
    }

    #[test]
    fn backlight_lifts_a_dark_subject_against_a_bright_background_only() {
        let mut rgb = testing::frame();
        for (i, p) in rgb.chunks_exact_mut(3).enumerate() {
            let (x, y) = (i % W, i / W);
            if part(x, y) == Region::Background {
                p.copy_from_slice(&[0.9, 0.9, 0.9]);
            } else {
                for v in p.iter_mut() {
                    *v *= 0.25;
                }
            }
        }
        let (before, after) = run_with(
            rgb,
            &testing::map(),
            SubjectFinish {
                backlight: 100.0,
                ..SubjectFinish::default()
            },
        );
        assert!(luma(px(&after, 120, 120)) > luma(px(&before, 120, 120)) * 1.5);
        assert_eq!(px(&before, 5, 5), px(&after, 5, 5));
        // A subject brighter than a dark background is left alone.
        let mut evening = testing::frame();
        for (i, p) in evening.chunks_exact_mut(3).enumerate() {
            if part(i % W, i / W) == Region::Background {
                p.copy_from_slice(&[0.05, 0.05, 0.06]);
            }
        }
        let (b2, a2) = run_with(
            evening,
            &testing::map(),
            SubjectFinish {
                backlight: 100.0,
                ..SubjectFinish::default()
            },
        );
        assert_eq!(b2, a2);
    }

    #[test]
    fn relight_from_the_left_brightens_the_left_edge_more_than_the_right() {
        let (before, after) = run_with(
            testing::frame(),
            &testing::map(),
            SubjectFinish {
                relight: 100.0,
                relight_direction: -90.0,
                ..SubjectFinish::default()
            },
        );
        let gain = |x, y| luma(px(&after, x, y)) / luma(px(&before, x, y));
        assert!(gain(78, 150) > gain(162, 150) + 0.01);
    }
}
