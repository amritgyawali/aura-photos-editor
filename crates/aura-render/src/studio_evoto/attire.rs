//! What people are wearing. ADR-0150 group G, the automatic half; glasses reflections are
//! placed by hand in `retouch_effects::evoto`.
//!
//! Gold and henna are found by their colour and kept off skin by the parse; fabric is worked
//! inside the parse's clothing region; and the veil is measured on each face as what it
//! physically is - a layer of its own colour added over the face - so a face without one is
//! left nearly as it was.
use aura_portrait::Region;
use aura_recipe::studio_finish::AttireFinish;

use super::{
    each, from_opponent, masked_blur, mix, opponent, percentile, smoothstep, weights, with_luma,
    Area, Ctx,
};

pub(super) fn apply(rgb: &mut [f32], ctx: &Ctx<'_>, a: &AttireFinish) {
    let k = |v: f32| (v / 100.0).clamp(0.0, 1.0);
    if a.veil_clarity > 0.0 {
        veil(rgb, ctx, k(a.veil_clarity));
    }
    if a.red_recovery > 0.0 {
        red_recovery(rgb, ctx, k(a.red_recovery));
    }
    if a.white_garment > 0.0 {
        white_garment(rgb, ctx, k(a.white_garment));
    }
    if a.dark_garment > 0.0 {
        dark_garment(rgb, ctx, k(a.dark_garment));
    }
    if a.outfit_colour > 0.0 {
        outfit_colour(rgb, ctx, k(a.outfit_colour));
    }
    if a.embroidery > 0.0 {
        embroidery(rgb, ctx, k(a.embroidery));
    }
    if a.garment_edge > 0.0 {
        garment_edge(rgb, ctx, k(a.garment_edge));
    }
    if a.mehndi > 0.0 {
        mehndi(rgb, ctx, k(a.mehndi));
    }
    if a.gold > 0.0 {
        gold(rgb, ctx, k(a.gold));
    }
}

/// Hue in degrees, saturation and value, read on display-encoded values so the windows below
/// mean what they would on a screen.
pub(super) fn hsv(p: [f32; 3]) -> (f32, f32, f32) {
    let e = p.map(|v| v.max(0.0).powf(1.0 / 2.2));
    let hi = e[0].max(e[1]).max(e[2]);
    let lo = e[0].min(e[1]).min(e[2]);
    let d = hi - lo;
    if hi <= 1e-6 {
        return (0.0, 0.0, 0.0);
    }
    let s = d / hi;
    if d <= 1e-6 {
        return (0.0, 0.0, hi);
    }
    let h = if (hi - e[0]).abs() < 1e-9 {
        60.0 * ((e[1] - e[2]) / d).rem_euclid(6.0)
    } else if (hi - e[1]).abs() < 1e-9 {
        60.0 * ((e[2] - e[0]) / d + 2.0)
    } else {
        60.0 * ((e[0] - e[1]) / d + 4.0)
    };
    (h, s, hi)
}

/// One inside `lo..hi` degrees, falling to zero `soft` degrees outside it.
fn hue_window(h: f32, lo: f32, hi: f32, soft: f32) -> f32 {
    smoothstep(lo - soft, lo, h) * (1.0 - smoothstep(hi, hi + soft, h))
}

fn with_chroma(p: [f32; 3], gain: f32) -> [f32; 3] {
    let o = opponent(p);
    from_opponent([o[0], o[1] * gain, o[2] * gain])
}

/// Gold jewellery and zari: richer, crisper, warmer in its highlights. Never on skin.
fn gold(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.soft(Region::Skin, 2);
    let body_skin = ctx.soft(Region::BodySkin, 2);
    let area = ctx.whole();
    let l = area.luma(rgb, ctx.w);
    let soft = area.blur(&l, (0.015 * ctx.face_unit()).max(1.5));
    each(rgb, ctx.w, area, |x, y, old| {
        let (h, s, v) = hsv(old);
        let found =
            hue_window(h, 32.0, 60.0, 6.0) * smoothstep(0.35, 0.55, s) * smoothstep(0.12, 0.25, v);
        let g = found * (1.0 - skin.at(x, y).max(body_skin.at(x, y))) * k;
        if g <= 0.0 {
            return None;
        }
        let i = area.index(x, y);
        let detail = (l[i] - soft[i]).clamp(-0.5 * l[i], 0.5 * l[i]);
        let lift = 1.0 + 0.12 * g * smoothstep(0.45, 0.9, v);
        let target = (l[i] + detail * 0.6 * g) * lift;
        Some(with_luma(with_chroma(old, 1.0 + 0.35 * g), target))
    });
}

/// Red fabric whose red channel flattened at the sensor's clip: the green channel still holds
/// the weave, so its relative detail is given back to the red, and the colour is eased off the
/// edge of the gamut a little.
fn red_recovery(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let area = ctx.whole();
    let radius = (0.02 * ctx.face_unit()).max(1.5);
    let r = area.channel(rgb, ctx.w, 0);
    let g = area.channel(rgb, ctx.w, 1);
    let rb = area.blur(&r, radius);
    let gb = area.blur(&g, radius);
    let rel = |v: &[f32], b: &[f32]| -> Vec<f32> {
        v.iter()
            .zip(b)
            .map(|(v, b)| (v - b).abs() / b.max(1e-4))
            .collect()
    };
    let r_detail = area.blur(&rel(&r, &rb), radius * 2.0);
    let g_detail = area.blur(&rel(&g, &gb), radius * 2.0);
    // Only inside solid red: at a border with embroidery the weave transfer drew a dark halo.
    let redness_of = |p: [f32; 3]| smoothstep(0.5, 0.7, (p[0] - p[1].max(p[2])) / p[0].max(1e-5));
    // Clipped is near the red fabric's own ceiling - not the frame's, which a lit sign or a sky
    // behind the bride set so high that no fabric ever reached it. Fabric below it, and fabric
    // at it that still has its weave in the red channel (the flatness test below), is left alone.
    let ceiling = {
        let mut reds: Vec<f32> = rgb
            .chunks_exact(3)
            .step_by(7)
            .filter(|p| redness_of([p[0], p[1], p[2]]) > 0.5)
            .map(|p| p[0])
            .collect();
        reds.sort_by(f32::total_cmp);
        reds.get(reds.len().saturating_sub(1) * 995 / 1000)
            .copied()
            .unwrap_or(1.0)
            .max(1e-4)
    };
    let red_plane: Vec<f32> = (0..area.width() * area.height())
        .map(|i| {
            let p = [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]];
            redness_of(p)
        })
        .collect();
    let solid = area.blur(&red_plane, radius * 3.0);
    each(rgb, ctx.w, area, |x, y, old| {
        let i = area.index(x, y);
        let red = redness_of(old) * smoothstep(0.85, 0.98, solid[i]);
        let clipped = smoothstep(0.8, 0.95, rb[i] / ceiling);
        let red = red * clipped;
        if red <= 0.0 {
            return None;
        }
        // Flat where the red channel holds much less of the weave than the green does.
        let flat = smoothstep(0.3, 0.7, 1.0 - r_detail[i] / g_detail[i].max(1e-4));
        let t = red * flat * k;
        if t <= 0.0 {
            return None;
        }
        let weave = (g[i] - gb[i]) / gb[i].max(1e-4);
        let rebuilt = rb[i] * (1.0 + weave.clamp(-0.6, 0.6));
        let out = [old[0] + (rebuilt - old[0]) * t, old[1], old[2]];
        Some(with_chroma(out, 1.0 - 0.12 * t))
    });
}

/// A net veil over a face, removed as the layer of its own colour it adds. A veil adds the
/// same light to everything behind it, so each channel's black level, measured on the face,
/// comes off, and one scale for all three channels returns the face's brightest light to its
/// level - one scale, because the veil dimmed every channel alike. On a face without a veil
/// this is a measured black point: a little more contrast, and no change of colour.
fn veil(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let face = ctx.view(Region::Face);
    let soft = ctx.soft(Region::Face, 5);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.4, 1.4, -1.5, f.mouth + 1.0);
        if area.is_empty() {
            continue;
        }
        let mut black = [0.0_f32; 3];
        let mut white = [0.0_f32; 3];
        let mut ok = true;
        for c in 0..3 {
            let plane = area.channel(rgb, ctx.w, c);
            let samples = || {
                (area.y0..area.y1)
                    .flat_map(|y| (area.x0..area.x1).map(move |x| (x, y)))
                    .map(|(x, y)| (plane[area.index(x, y)], face.at(x, y)))
            };
            match (percentile(samples(), 0.01), percentile(samples(), 0.97)) {
                (Some(lo), Some(hi)) if hi > lo * 1.2 => {
                    black[c] = lo;
                    white[c] = hi;
                }
                _ => ok = false,
            }
        }
        if !ok {
            continue;
        }
        let bright = crate::colour::luma(white);
        each(rgb, ctx.w, area, |x, y, old| {
            let wgt = soft.at(x, y) * k;
            if wgt <= 0.0 {
                return None;
            }
            let b = black.map(|v| v * wgt);
            let open = crate::colour::luma([white[0] - b[0], white[1] - b[1], white[2] - b[2]]);
            let scale = bright / open.max(1e-5);
            Some(std::array::from_fn(|c| ((old[c] - b[c]) * scale).max(0.0)))
        });
    }
}

/// The texture measure embroidery is found by: local mean absolute detail over the local level.
fn texture(area: Area, l: &[f32], fine: &[f32], radius: f32) -> Vec<f32> {
    let detail: Vec<f32> = l.iter().zip(fine).map(|(v, b)| (v - b).abs()).collect();
    let spread = area.blur(&detail, radius);
    let level = area.blur(l, radius);
    spread
        .iter()
        .zip(&level)
        .map(|(s, m)| s / m.max(1e-4))
        .collect()
}

/// Embroidery, beadwork and lace brought forward; plain fabric left as it is.
fn embroidery(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let clothing = ctx.view(Region::Clothing);
    let Some(area) = ctx.bounds(Region::Clothing, 0.01) else {
        return;
    };
    let unit = ctx.face_unit();
    let l = area.luma(rgb, ctx.w);
    let fine = area.blur(&l, (0.012 * unit).max(1.0));
    let busy = texture(area, &l, &fine, (0.05 * unit).max(2.0));
    each(rgb, ctx.w, area, |x, y, old| {
        let i = area.index(x, y);
        let wgt = clothing.at(x, y) * smoothstep(0.03, 0.1, busy[i]) * k;
        if wgt <= 0.0 {
            return None;
        }
        let detail = (l[i] - fine[i]).clamp(-0.5 * l[i], 0.5 * l[i]);
        Some(with_luma(old, l[i] + detail * 0.9 * wgt))
    });
}

/// A garment's silhouette against the background, smoothed along its own edge.
fn garment_edge(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let cloth = ctx.soft(Region::Clothing, 1);
    let background = ctx.soft(Region::Background, 2);
    let Some(area) = ctx.bounds(Region::Clothing, 0.02) else {
        return;
    };
    let before: Vec<f32> = rgb.to_vec();
    let (w, h) = (ctx.w, ctx.h);
    // One analysis pixel in buffer pixels: the step the edge's direction is read over.
    let step = (w as f32 / ctx.map.width.max(1) as f32).max(1.0);
    let reach = (0.02 * ctx.face_unit()).max(2.0).round() as i32;
    let read = |fx: f32, fy: f32| -> [f32; 3] {
        let x = (fx.round().max(0.0) as usize).min(w - 1);
        let y = (fy.round().max(0.0) as usize).min(h - 1);
        let i = (y * w + x) * 3;
        [before[i], before[i + 1], before[i + 2]]
    };
    each(rgb, ctx.w, area, |x, y, old| {
        let c = cloth.at(x, y);
        let band =
            (4.0 * c * (1.0 - c)).clamp(0.0, 1.0) * smoothstep(0.1, 0.4, background.at(x, y));
        let t = band * k * 0.85;
        if t <= 0.0 {
            return None;
        }
        let s = step.round() as usize;
        let gx = cloth.at((x + s).min(w - 1), y) - cloth.at(x.saturating_sub(s), y);
        let gy = cloth.at(x, (y + s).min(h - 1)) - cloth.at(x, y.saturating_sub(s));
        let len = gx.hypot(gy);
        if len <= 1e-5 {
            return None;
        }
        let tangent = [-gy / len, gx / len];
        let mut sum = [0.0_f32; 3];
        let mut total = 0.0_f32;
        for j in -reach..=reach {
            let kk = (reach + 1 - j.abs()) as f32;
            let p = read(
                x as f32 + tangent[0] * j as f32,
                y as f32 + tangent[1] * j as f32,
            );
            for ch in 0..3 {
                sum[ch] += p[ch] * kk;
            }
            total += kk;
        }
        Some(mix(old, sum.map(|v| v / total), t))
    });
}

/// Henna on hands and arms deepened: colour that is darker and more saturated than the skin
/// around it, in the henna's orange-to-brown hues.
fn mehndi(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let body = ctx.view(Region::BodySkin);
    let Some(area) = ctx
        .bounds(Region::BodySkin, 0.02)
        .or_else(|| ctx.bounds(Region::Skin, 0.02))
    else {
        return;
    };
    let unit = ctx.face_unit();
    let l = area.luma(rgb, ctx.w);
    let on = weights(area, |x, y| skin.at(x, y).max(body.at(x, y)));
    let level = masked_blur(area, &l, &on, 0.12 * unit);
    let chroma: Vec<f32> = (area.y0..area.y1)
        .flat_map(|y| (area.x0..area.x1).map(move |x| (x, y)))
        .map(|(x, y)| {
            let i = (y * ctx.w + x) * 3;
            let o = opponent([rgb[i], rgb[i + 1], rgb[i + 2]]);
            o[1].hypot(o[2]) / o[0].max(1e-5)
        })
        .collect();
    let chroma_level = masked_blur(area, &chroma, &on, 0.12 * unit);
    each(rgb, ctx.w, area, |x, y, old| {
        let i = area.index(x, y);
        let wgt = on[i];
        if wgt <= 0.0 {
            return None;
        }
        let (h, _, _) = hsv(old);
        let darker = 1.0 - l[i] / level[i].max(1e-5);
        let richer = chroma[i] / chroma_level[i].max(1e-5);
        let m = hue_window(h, 5.0, 35.0, 8.0)
            * smoothstep(0.1, 0.25, darker)
            * (1.0 - smoothstep(0.65, 0.85, darker))
            * smoothstep(1.05, 1.3, richer);
        let t = m * wgt * k;
        if t <= 0.0 {
            return None;
        }
        Some(with_luma(
            with_chroma(old, 1.0 + 0.35 * t),
            l[i] * (1.0 - 0.28 * t),
        ))
    });
}

/// White garments made clean white: their shared colour cast removed, their folds kept.
fn white_garment(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let clothing = ctx.view(Region::Clothing);
    let Some(area) = ctx.bounds(Region::Clothing, 0.0) else {
        return;
    };
    let whiteness = |p: [f32; 3]| {
        let (_, s, v) = hsv(p);
        (1.0 - smoothstep(0.12, 0.22, s)) * smoothstep(0.5, 0.65, v)
    };
    let Some(cast) = super::mean_colour(rgb, ctx.w, area, |x, y| {
        let i = (y * ctx.w + x) * 3;
        clothing.at(x, y) * whiteness([rgb[i], rgb[i + 1], rgb[i + 2]])
    }) else {
        return;
    };
    let c = opponent(cast);
    let (cb, cr) = (c[1] / c[0].max(1e-5), c[2] / c[0].max(1e-5));
    each(rgb, ctx.w, area, |x, y, old| {
        let t = clothing.at(x, y) * whiteness(old) * k;
        if t <= 0.0 {
            return None;
        }
        let o = opponent(old);
        let y2 = o[0] * (1.0 + 0.08 * t);
        let (b, r) = (
            o[1] / o[0].max(1e-5) - cb * t,
            o[2] / o[0].max(1e-5) - cr * t,
        );
        Some(from_opponent([y2, b * y2, r * y2]))
    });
}

/// Detail lifted out of crushed black suits and dark fabric, which stay black.
fn dark_garment(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let clothing = ctx.view(Region::Clothing);
    let Some(area) = ctx.bounds(Region::Clothing, 0.0) else {
        return;
    };
    let unit = ctx.face_unit();
    let l = area.luma(rgb, ctx.w);
    let on = weights(area, |x, y| clothing.at(x, y));
    let level = masked_blur(area, &l, &on, (0.05 * unit).max(2.0));
    each(rgb, ctx.w, area, |x, y, old| {
        let (_, s, v) = hsv(old);
        let dark = (1.0 - smoothstep(0.22, 0.34, v)) * (1.0 - smoothstep(0.3, 0.45, s));
        let t = clothing.at(x, y) * dark * k;
        if t <= 0.0 {
            return None;
        }
        let i = area.index(x, y);
        let detail = l[i] - level[i];
        Some(with_luma(
            old,
            (level[i] * (1.0 + 0.25 * t) + detail * (1.0 + t)).max(0.0),
        ))
    });
}

/// More colour in the clothing - most where it has least - and none in the skin.
fn outfit_colour(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let clothing = ctx.view(Region::Clothing);
    let skin = ctx.soft(Region::Skin, 2);
    let body_skin = ctx.soft(Region::BodySkin, 2);
    let Some(area) = ctx.bounds(Region::Clothing, 0.0) else {
        return;
    };
    each(rgb, ctx.w, area, |x, y, old| {
        let t = clothing.at(x, y) * (1.0 - skin.at(x, y).max(body_skin.at(x, y))) * k;
        if t <= 0.0 {
            return None;
        }
        let (_, s, _) = hsv(old);
        Some(with_chroma(old, 1.0 + 0.5 * t * (1.0 - s)))
    });
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::super::testing::{self, *};
    use super::*;
    use crate::colour::luma;

    fn run_on(mut rgb: Vec<f32>, a: AttireFinish) -> (Vec<f32>, Vec<f32>) {
        let before = rgb.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(&mut rgb, &ctx, &a);
        (before, rgb)
    }

    fn paint(rgb: &mut [f32], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>, c: [f32; 3]) {
        for y in ys {
            for x in xs.clone() {
                let i = (y * W + x) * 3;
                rgb[i..i + 3].copy_from_slice(&c);
            }
        }
    }

    #[test]
    fn hue_is_read_as_a_screen_would_show_it() {
        let (h, s, _) = hsv([0.6, 0.35, 0.03]);
        assert!((30.0..60.0).contains(&h), "{h}");
        assert!(s > 0.5);
        let (h, _, _) = hsv([0.5, 0.02, 0.03]);
        assert!(!(5.0..60.0).contains(&h), "{h}");
    }

    #[test]
    fn gold_is_enriched_and_skin_of_the_same_hue_is_not() {
        let mut rgb = testing::frame();
        // A gold necklace on the background beside the neck, and gold-hued paint on the cheek.
        paint(&mut rgb, 150..170, 185..195, [0.6, 0.35, 0.03]);
        paint(&mut rgb, 85..95, 125..135, [0.6, 0.35, 0.03]);
        let (before, after) = run_on(
            rgb,
            AttireFinish {
                gold: 100.0,
                ..AttireFinish::default()
            },
        );
        let chroma = |p: [f32; 3]| (p[0] - p[2]) / luma(p);
        assert!(chroma(px(&after, 160, 190)) > chroma(px(&before, 160, 190)) * 1.15);
        assert!((chroma(px(&after, 90, 130)) - chroma(px(&before, 90, 130))).abs() < 0.02);
    }

    #[test]
    fn the_veil_layer_comes_off_a_veiled_face_and_barely_touches_a_bare_one() {
        let bare = testing::frame();
        let (b0, a0) = run_on(
            bare,
            AttireFinish {
                veil_clarity: 100.0,
                ..AttireFinish::default()
            },
        );
        let bare_change =
            (luma(px(&a0, 120, 130)) - luma(px(&b0, 120, 130))).abs() / luma(px(&b0, 120, 130));
        // The same face behind a red veil: a third of its light is the veil's own.
        let mut veiled = testing::frame();
        for p in veiled.chunks_exact_mut(3) {
            p[0] = p[0] * 0.67 + 0.18;
            p[1] = p[1] * 0.67 + 0.03;
            p[2] = p[2] * 0.67 + 0.03;
        }
        let (b1, a1) = run_on(
            veiled,
            AttireFinish {
                veil_clarity: 100.0,
                ..AttireFinish::default()
            },
        );
        let redness = |p: [f32; 3]| p[0] / luma(p);
        assert!(redness(px(&a1, 120, 130)) < redness(px(&b1, 120, 130)) * 0.9);
        // Without a veil it is a black point: some contrast, never a colour change.
        assert!(bare_change < 0.15, "{bare_change}");
        let hue = |p: [f32; 3]| (p[0] / p[1], p[2] / p[1]);
        let (h0, h1) = (hue(px(&b0, 120, 130)), hue(px(&a0, 120, 130)));
        assert!(
            (h0.0 - h1.0).abs() < 0.15 && (h0.1 - h1.1).abs() < 0.1,
            "{h0:?} {h1:?}"
        );
    }

    #[test]
    fn white_garments_lose_their_cast_and_skin_does_not_move() {
        let mut rgb = testing::frame();
        paint(&mut rgb, 50..190, 205..240, [0.62, 0.66, 0.55]);
        let (before, after) = run_on(
            rgb,
            AttireFinish {
                white_garment: 100.0,
                ..AttireFinish::default()
            },
        );
        let cast = |p: [f32; 3]| (p[1] - (p[0] + p[2]) * 0.5).abs() / luma(p);
        assert!(cast(px(&after, 120, 220)) < cast(px(&before, 120, 220)) * 0.3);
        assert_eq!(moved(&before, &after, Region::Skin, false, 1e-6), 0);
    }

    #[test]
    fn outfit_colour_and_dark_garment_stay_in_the_clothing() {
        let (before, after) = run_on(
            testing::frame(),
            AttireFinish {
                outfit_colour: 100.0,
                dark_garment: 100.0,
                embroidery: 100.0,
                ..AttireFinish::default()
            },
        );
        assert_eq!(moved(&before, &after, Region::Clothing, true, 1e-6), 0);
        assert!(moved(&before, &after, Region::Clothing, false, 1e-3) > 100);
    }
}
