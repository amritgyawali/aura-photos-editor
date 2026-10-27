//! Hair and body skin. ADR-0150 group F, the automatic half; the hand-placed half (body hair,
//! cellulite, tan lines, hands, knuckles) is in `retouch_effects::evoto`.
//!
//! Every adjustment stays inside the parse's hair or body-skin region and measures against that
//! region's own local level, so a gloss follows the highlights the hair already has and a filled
//! parting takes the colour of the hair on either side of it.
use aura_portrait::Region;
use aura_recipe::studio_finish::HairBodyFinish;

use super::{along, each, ellipse, local, masked_blur, mix, smoothstep, weights, with_luma, Ctx};

pub(super) fn apply(rgb: &mut [f32], ctx: &Ctx<'_>, s: &HairBodyFinish) {
    let k = |v: f32| (v / 100.0).clamp(0.0, 1.0);
    if s.parting > 0.0 {
        parting(rgb, ctx, k(s.parting));
    }
    if s.richness > 0.0 {
        richness(rgb, ctx, k(s.richness));
    }
    if s.gloss > 0.0 {
        gloss(rgb, ctx, k(s.gloss));
    }
    if s.edge > 0.0 {
        edge(rgb, ctx, k(s.edge));
    }
    if s.definition > 0.0 {
        definition(rgb, ctx, k(s.definition));
    }
}

/// Shine along the highlights hair already has, and a little more contrast through it.
fn gloss(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    // The parse's own edge: a feathered one would stretch the contrast of the background
    // beside the hair into a halo.
    let hair = ctx.view(Region::Hair);
    let Some(area) = ctx.bounds(Region::Hair, 0.02) else {
        return;
    };
    let unit = ctx.face_unit();
    let l = area.luma(rgb, ctx.w);
    let on = weights(area, |x, y| hair.at(x, y));
    let level = masked_blur(area, &l, &on, 0.15 * unit);
    each(rgb, ctx.w, area, |x, y, old| {
        let wgt = hair.at(x, y) * k;
        if wgt <= 0.0 {
            return None;
        }
        let i = area.index(x, y);
        let m = level[i].max(1e-5);
        let highlight = smoothstep(0.0, 0.5, (l[i] - m) / m);
        let target = m + (l[i] - m) * (1.0 + 0.35 * wgt) + l[i] * 0.6 * wgt * highlight;
        Some(with_luma(old, target.max(0.0)))
    });
}

/// A bright parting line filled with the colour of the hair either side of it.
fn parting(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let hair = ctx.view(Region::Hair);
    for f in &ctx.faces {
        let area = ctx.around(f, -0.9, 0.9, -2.9, -0.7);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let on = weights(area, |x, y| hair.at(x, y));
        let (aw, ah) = (area.width(), area.height());
        let reach = (0.08 * f.io).max(2.0);
        // A parting runs from the brow toward the crown: blurring along it keeps it bright,
        // blurring across it does not. A highlight across the head does the opposite.
        let lengthwise = along(&l, aw, ah, f.v, reach);
        let crosswise = along(&l, aw, ah, f.u, reach);
        let level = masked_blur(area, &l, &on, 0.15 * f.io);
        let colour: Vec<Vec<f32>> = (0..3)
            .map(|c| masked_blur(area, &area.channel(rgb, ctx.w, c), &on, 0.12 * f.io))
            .collect();
        each(rgb, ctx.w, area, |x, y, old| {
            let zone = ellipse(local(f, x, y), [0.0, -1.8], [0.7, 1.0]) * hair.at(x, y);
            if zone <= 0.0 {
                return None;
            }
            let i = area.index(x, y);
            let m = level[i].max(1e-5);
            let bright = smoothstep(0.12, 0.35, (l[i] - m) / m);
            let line = smoothstep(0.04, 0.15, (lengthwise[i] - crosswise[i]) / m);
            let t = k * zone * bright * line;
            if t <= 0.0 {
                return None;
            }
            let fill = [colour[0][i], colour[1][i], colour[2][i]];
            Some(mix(old, fill, t * 0.9))
        });
    }
}

/// Richer hair colour: more chroma, and the tones spread a little about the hair's own median.
fn richness(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let hair = ctx.view(Region::Hair);
    let Some(area) = ctx.bounds(Region::Hair, 0.02) else {
        return;
    };
    let l = area.luma(rgb, ctx.w);
    let samples = (area.y0..area.y1)
        .flat_map(|y| (area.x0..area.x1).map(move |x| (x, y)))
        .map(|(x, y)| (l[area.index(x, y)], hair.at(x, y)));
    let Some(median) = super::percentile(samples, 0.5) else {
        return;
    };
    let median = median.max(1e-4);
    each(rgb, ctx.w, area, |x, y, old| {
        let wgt = hair.at(x, y) * k;
        if wgt <= 0.0 {
            return None;
        }
        let o = super::opponent(old);
        if o[0] <= 1e-6 {
            return None;
        }
        let y2 = median * (o[0] / median).powf(1.0 + 0.3 * wgt) * (1.0 - 0.08 * wgt);
        // Colour goes into the hair's body and mid-tones; a strand already catching the light
        // turned orange when it was given the same boost.
        let lit = smoothstep(median, median * 2.5, o[0]);
        let chroma = 1.0 + 0.22 * wgt * (1.0 - lit);
        Some(super::from_opponent([
            y2,
            o[1] / o[0] * y2 * chroma,
            o[2] / o[0] * y2 * chroma,
        ]))
    });
}

/// A frizzy hair edge calmed against the background.
fn edge(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let hair = ctx.soft(Region::Hair, 2);
    let background = ctx.soft(Region::Background, 2);
    let Some(area) = ctx.bounds(Region::Hair, 0.04) else {
        return;
    };
    let radius = (0.04 * ctx.face_unit()).max(1.5);
    let planes: Vec<Vec<f32>> = (0..3)
        .map(|c| area.blur(&area.channel(rgb, ctx.w, c), radius))
        .collect();
    each(rgb, ctx.w, area, |x, y, old| {
        let h = hair.at(x, y);
        let band =
            (4.0 * h * (1.0 - h)).clamp(0.0, 1.0) * smoothstep(0.1, 0.5, background.at(x, y));
        let t = band * k * 0.75;
        if t <= 0.0 {
            return None;
        }
        let i = area.index(x, y);
        Some(mix(old, [planes[0][i], planes[1][i], planes[2][i]], t))
    });
}

/// The light and shade the body already has - muscle, collarbones, arms - brought forward.
fn definition(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let body = ctx.view(Region::BodySkin);
    let Some(area) = ctx.bounds(Region::BodySkin, 0.02) else {
        return;
    };
    // Body skin is the colour of the person's face; red silk the parse took for an arm is not.
    let Some(face_skin) = ctx
        .faces
        .first()
        .and_then(|f| super::skin::cheeks(rgb, ctx, f))
        .map(|c| {
            let o = super::opponent(c);
            [o[1] / o[0].max(1e-5), o[2] / o[0].max(1e-5)]
        })
    else {
        return;
    };
    let skin_like = |p: [f32; 3]| {
        let o = super::opponent(p);
        let d = (o[1] / o[0].max(1e-5) - face_skin[0]).hypot(o[2] / o[0].max(1e-5) - face_skin[1]);
        1.0 - smoothstep(0.08, 0.2, d)
    };
    let unit = ctx.face_unit();
    let l = area.luma(rgb, ctx.w);
    let on = weights(area, |x, y| {
        let i = (y * ctx.w + x) * 3;
        body.at(x, y) * skin_like([rgb[i], rgb[i + 1], rgb[i + 2]])
    });
    let mid = masked_blur(area, &l, &on, 0.06 * unit);
    let wide = masked_blur(area, &l, &on, 0.35 * unit);
    each(rgb, ctx.w, area, |x, y, old| {
        let i = area.index(x, y);
        let wgt = on[i] * k;
        if wgt <= 0.0 {
            return None;
        }
        let shape = (mid[i] - wide[i]).clamp(-0.4 * wide[i], 0.4 * wide[i]);
        Some(with_luma(old, (l[i] + shape * 0.9 * wgt).max(0.0)))
    });
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::super::testing::{self, *};
    use super::*;
    use crate::colour::luma;

    fn run_on(mut rgb: Vec<f32>, s: HairBodyFinish) -> (Vec<f32>, Vec<f32>) {
        let before = rgb.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(&mut rgb, &ctx, &s);
        (before, rgb)
    }

    #[test]
    fn hair_adjustments_never_leave_the_hair() {
        let (before, after) = run_on(
            testing::frame(),
            HairBodyFinish {
                gloss: 100.0,
                parting: 100.0,
                richness: 100.0,
                ..HairBodyFinish::default()
            },
        );
        assert_eq!(moved(&before, &after, Region::Hair, true, 1e-6), 0);
    }

    #[test]
    fn a_parting_is_filled_and_a_highlight_across_the_head_is_not() {
        let mut rgb = testing::frame();
        // A bright parting line down the centre of the head, and a highlight band across it.
        for y in 25..55 {
            for x in 119..122 {
                if part(x, y) == Region::Hair {
                    let i = (y * W + x) * 3;
                    rgb[i..i + 3].copy_from_slice(&[0.25, 0.18, 0.15]);
                }
            }
        }
        for y in 40..43 {
            for x in 98..116 {
                if part(x, y) == Region::Hair {
                    let i = (y * W + x) * 3;
                    rgb[i..i + 3].copy_from_slice(&[0.25, 0.18, 0.15]);
                }
            }
        }
        let (before, after) = run_on(
            rgb,
            HairBodyFinish {
                parting: 100.0,
                ..HairBodyFinish::default()
            },
        );
        assert!(luma(px(&after, 120, 32)) < luma(px(&before, 120, 32)) * 0.7);
        assert!(luma(px(&after, 106, 41)) > luma(px(&before, 106, 41)) * 0.95);
    }

    #[test]
    fn richness_adds_colour_to_hair() {
        let mut rgb = testing::frame();
        for y in 0..H {
            for x in 0..W {
                if part(x, y) == Region::Hair {
                    let i = (y * W + x) * 3;
                    let v = if (x / 3 + y / 3) % 2 == 0 { 0.08 } else { 0.14 };
                    rgb[i..i + 3].copy_from_slice(&[v * 1.4, v, v * 0.7]);
                }
            }
        }
        let (before, after) = run_on(
            rgb,
            HairBodyFinish {
                richness: 100.0,
                ..HairBodyFinish::default()
            },
        );
        let chroma = |p: [f32; 3]| (p[0] - p[2]) / luma(p);
        assert!(chroma(px(&after, 120, 30)) > chroma(px(&before, 120, 30)) * 1.08);
    }
}
