//! The background behind the people. ADR-0150 group H, the automatic half; removing a cast
//! shadow is placed by hand in `retouch_effects::evoto`, and the two new replacements - a colour
//! that keeps the backdrop's shadows, and a sky with clouds - are background modes in
//! `crate::studio_finish`.
//!
//! Everything here is weighted by the parse's background plane, feathered on the analysis grid,
//! so the people are never touched; the blurs are normalised over the background alone, so a
//! person never bleeds into the blur around them as a dark halo.
use aura_portrait::Region;
use aura_recipe::studio_finish::SceneFinish;

use super::{each, from_opponent, opponent, percentile, smoothstep, weights, Area, Ctx};
use crate::colour::luma;

pub(super) fn apply(rgb: &mut [f32], ctx: &Ctx<'_>, s: &SceneFinish) {
    let k = |v: f32| (v / 100.0).clamp(0.0, 1.0);
    if s.denoise > 0.0 {
        denoise(rgb, ctx, k(s.denoise));
    }
    if s.depth_blur > 0.0 {
        depth_blur(rgb, ctx, k(s.depth_blur));
    }
    if s.saturation != 0.0 || s.brightness != 0.0 || s.warmth != 0.0 {
        tone(rgb, ctx, s);
    }
    if s.backdrop_glow > 0.0 {
        backdrop_glow(rgb, ctx, k(s.backdrop_glow));
    }
    if s.light_wrap > 0.0 {
        light_wrap(rgb, ctx, k(s.light_wrap));
    }
}

fn short(ctx: &Ctx<'_>) -> f32 {
    ctx.w.min(ctx.h) as f32
}

/// The background's saturation, brightness and warmth, in one pass.
fn tone(rgb: &mut [f32], ctx: &Ctx<'_>, s: &SceneFinish) {
    let bg = ctx.soft(Region::Background, 1);
    let saturation = (1.0 + s.saturation / 100.0).max(0.0);
    let gain = 2.0_f32.powf(s.brightness / 100.0);
    let warm = s.warmth / 100.0;
    each(rgb, ctx.w, ctx.whole(), |x, y, old| {
        let t = bg.at(x, y);
        if t <= 0.0 {
            return None;
        }
        let o = opponent(old);
        let coloured = from_opponent([o[0], o[1] * saturation, o[2] * saturation]);
        let warmed = [
            coloured[0] * (1.0 + 0.12 * warm),
            coloured[1],
            coloured[2] * (1.0 - 0.12 * warm),
        ];
        let target = super::with_luma(warmed, o[0] * gain);
        Some(super::mix(old, target, t))
    });
}

/// Noise reduction on the background: an adaptive Wiener filter whose noise level is measured
/// on the background's own flattest parts.
fn denoise(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let bg = ctx.soft(Region::Background, 1);
    let area = ctx.whole();
    let radius = (0.0015 * short(ctx)).max(1.5);
    let mut out: Vec<Vec<f32>> = Vec::with_capacity(3);
    for c in 0..3 {
        let plane = area.channel(rgb, ctx.w, c);
        let mean = area.blur(&plane, radius);
        let squares: Vec<f32> = plane.iter().map(|v| v * v).collect();
        let mean_sq = area.blur(&squares, radius);
        let variance: Vec<f32> = mean_sq
            .iter()
            .zip(&mean)
            .map(|(s, m)| (s - m * m).max(0.0))
            .collect();
        let samples = (0..area.height())
            .step_by(3)
            .flat_map(|y| (0..area.width()).step_by(3).map(move |x| (x, y)))
            .map(|(x, y)| (variance[y * area.width() + x], bg.at(x, y)));
        let noise = percentile(samples, 0.3).unwrap_or(0.0);
        out.push(
            plane
                .iter()
                .zip(&mean)
                .zip(&variance)
                .map(|((v, m), var)| {
                    let keep = ((var - noise) / var.max(1e-12)).clamp(0.0, 1.0);
                    m + (v - m) * keep
                })
                .collect(),
        );
    }
    each(rgb, ctx.w, area, |x, y, old| {
        let t = bg.at(x, y) * k;
        if t <= 0.0 {
            return None;
        }
        let i = area.index(x, y);
        Some(super::mix(old, [out[0][i], out[1][i], out[2][i]], t))
    });
}

/// A normalised blur of the frame over `weight`: what the background looks like blurred
/// without the people in it.
fn background_blur(
    rgb: &[f32],
    area: Area,
    w: usize,
    weight: &[f32],
    radius: f32,
) -> [Vec<f32>; 3] {
    let norm = area.blur(weight, radius);
    std::array::from_fn(|c| {
        let plane: Vec<f32> = area
            .channel(rgb, w, c)
            .iter()
            .zip(weight)
            .map(|(v, k)| v * k)
            .collect();
        area.blur(&plane, radius)
            .iter()
            .zip(&norm)
            .zip(area.channel(rgb, w, c))
            .map(|((b, n), v)| if *n > 1e-3 { b / n } else { v })
            .collect()
    })
}

/// A blur that grows with distance from the people.
fn depth_blur(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let crisp = ctx.view(Region::Background);
    let bg = ctx.soft(Region::Background, 1);
    // How near the people a pixel is: their plane spread over about a tenth of the frame.
    let near = ctx.soft(Region::Body, 34);
    let area = ctx.whole();
    let weight = weights(area, |x, y| crisp.at(x, y));
    let s = short(ctx);
    let levels: Vec<[Vec<f32>; 3]> = [0.004, 0.012, 0.028]
        .iter()
        .map(|r| {
            background_blur(
                rgb,
                area,
                ctx.w,
                &weight,
                (r * s * (0.4 + 0.6 * k)).max(1.0),
            )
        })
        .collect();
    each(rgb, ctx.w, area, |x, y, old| {
        let t = bg.at(x, y);
        if t <= 0.0 {
            return None;
        }
        let depth = 1.0 - smoothstep(0.0, 0.4, near.at(x, y));
        let level = (0.35 + 0.65 * depth) * 3.0 * k;
        let i = area.index(x, y);
        let pick = |n: usize| [levels[n][0][i], levels[n][1][i], levels[n][2][i]];
        let blurred = if level <= 1.0 {
            super::mix(old, pick(0), level)
        } else if level <= 2.0 {
            super::mix(pick(0), pick(1), level - 1.0)
        } else {
            super::mix(pick(1), pick(2), level - 2.0)
        };
        Some(super::mix(old, blurred, t))
    });
}

/// A soft pool of light on the backdrop behind each head.
fn backdrop_glow(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let bg = ctx.soft(Region::Background, 1);
    let pools: Vec<([f32; 2], f32)> = ctx
        .faces
        .iter()
        .map(|f| (f.at(0.0, -0.4), 4.0 * f.io))
        .collect();
    if pools.is_empty() {
        return;
    }
    each(rgb, ctx.w, ctx.whole(), |x, y, old| {
        let t = bg.at(x, y);
        if t <= 0.0 {
            return None;
        }
        let mut light = 0.0_f32;
        for (c, r) in &pools {
            let d2 = ((x as f32 + 0.5 - c[0]).powi(2) + (y as f32 + 0.5 - c[1]).powi(2)) / (r * r);
            if d2 < 1.0 {
                light = light.max((1.0 - d2) * (1.0 - d2));
            }
        }
        let g = t * light * k;
        (g > 0.0).then(|| old.map(|v| v * (1.0 + 0.7 * g)))
    });
}

/// The background's light wrapped onto the people's edges, so they sit in it.
fn light_wrap(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let person = ctx.view(Region::Body);
    let crisp = ctx.view(Region::Background);
    let Some(area) = ctx.bounds(Region::Body, 0.03) else {
        return;
    };
    let radius = (0.02 * short(ctx)).max(2.0);
    let bg_weight = weights(area, |x, y| crisp.at(x, y));
    let around = background_blur(rgb, area, ctx.w, &bg_weight, radius);
    let inside = weights(area, |x, y| person.at(x, y));
    let deep = area.blur(&inside, radius * 0.6);
    each(rgb, ctx.w, area, |x, y, old| {
        let i = area.index(x, y);
        let wrap = (inside[i] * (1.0 - deep[i]) * 2.0).clamp(0.0, 1.0);
        let t = wrap * k;
        if t <= 0.0 {
            return None;
        }
        let light = [around[0][i], around[1][i], around[2][i]];
        let l = luma(old).max(1e-5);
        // Bright surroundings wrap more than dark ones.
        let lift = smoothstep(0.0, 1.5, luma(light) / l);
        Some(std::array::from_fn(|c| old[c] + light[c] * 0.5 * t * lift))
    });
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::super::testing::{self, *};
    use super::*;

    fn run(s: SceneFinish) -> (Vec<f32>, Vec<f32>) {
        let before = testing::frame();
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(&mut after, &ctx, &s);
        (before, after)
    }

    #[test]
    fn background_tone_never_reaches_the_person() {
        let (before, after) = run(SceneFinish {
            saturation: -100.0,
            brightness: 60.0,
            warmth: 50.0,
            denoise: 100.0,
            depth_blur: 100.0,
            ..SceneFinish::default()
        });
        // Well inside the person, the feathered edge aside.
        assert_eq!(px(&before, 120, 120), px(&after, 120, 120));
        assert_eq!(px(&before, 120, 225), px(&after, 120, 225));
        let corner = px(&after, 5, 5);
        assert!((corner[0] - corner[2]).abs() < 0.08, "{corner:?}");
        assert!(luma(corner) > luma(px(&before, 5, 5)) * 1.3);
    }

    #[test]
    fn the_backdrop_glows_behind_the_head_and_not_in_the_far_corner() {
        let (before, after) = run(SceneFinish {
            backdrop_glow: 100.0,
            ..SceneFinish::default()
        });
        assert!(luma(px(&after, 40, 60)) > luma(px(&before, 40, 60)) * 1.1);
        assert_eq!(px(&before, 2, 238), px(&after, 2, 238));
    }

    #[test]
    fn a_light_wrap_brightens_only_the_person_and_most_at_the_edge() {
        let mut before = testing::frame();
        for p in before.chunks_exact_mut(3) {
            if p == BACKGROUND {
                p.copy_from_slice(&[0.9, 0.9, 0.9]);
            }
        }
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(
            &mut after,
            &ctx,
            &SceneFinish {
                light_wrap: 100.0,
                ..SceneFinish::default()
            },
        );
        assert_eq!(moved(&before, &after, Region::Background, false, 1e-6), 0);
        let gain = |x, y| luma(px(&after, x, y)) / luma(px(&before, x, y));
        assert!(gain(72, 120) > gain(110, 120));
    }

    #[test]
    fn denoise_quietens_a_noisy_background() {
        let mut before = testing::frame();
        for (i, p) in before.chunks_exact_mut(3).enumerate() {
            let (x, y) = (i % W, i / W);
            if part(x, y) == Region::Background {
                let n = if (x * 7 + y * 13) % 5 < 2 {
                    0.04
                } else {
                    -0.03
                };
                for v in p.iter_mut() {
                    *v += n;
                }
            }
        }
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(
            &mut after,
            &ctx,
            &SceneFinish {
                denoise: 100.0,
                ..SceneFinish::default()
            },
        );
        let spread = |rgb: &[f32]| {
            let row: Vec<f32> = (5..40).map(|x| luma(px(rgb, x, 20))).collect();
            let m = row.iter().sum::<f32>() / row.len() as f32;
            row.iter().map(|v| (v - m).abs()).sum::<f32>()
        };
        assert!(spread(&after) < spread(&before) * 0.6);
    }
}
