//! Makeup placed from each face's landmarks. ADR-0150 group E.
//!
//! The liner and the lashes follow each eye's own lid line, measured from the top and bottom
//! edges of the eye opening the parse found; only an eye the parse could not open falls back to
//! a lid curve sized from the distance between the eyes. Lips are worked inside their own region,
//! and the enhancement of makeup already worn is measured against the person's own cheeks.
use aura_portrait::Region;
use aura_recipe::studio_finish::MakeupFinish;

use super::{
    each, ellipse, from_opponent, local, masked_blur, mix, near_mouth, opponent, percentile,
    smoothstep, weights, with_luma, Ctx, Face, OwnedView,
};
use crate::colour::luma;

pub(super) fn apply(rgb: &mut [f32], ctx: &Ctx<'_>, m: &MakeupFinish) {
    let k = |v: f32| (v / 100.0).clamp(0.0, 1.0);
    if m.enhance > 0.0 {
        enhance(rgb, ctx, k(m.enhance));
    }
    if m.bronzer > 0.0 {
        bronzer(rgb, ctx, k(m.bronzer));
    }
    if m.highlighter > 0.0 {
        highlighter(rgb, ctx, k(m.highlighter));
    }
    if m.brow_fill > 0.0 {
        brow_fill(rgb, ctx, k(m.brow_fill));
    }
    if m.lip_ombre > 0.0 {
        lip_ombre(rgb, ctx, k(m.lip_ombre));
    }
    if m.lip_liner > 0.0 {
        lip_liner(rgb, ctx, k(m.lip_liner));
    }
    if m.lashes > 0.0 || m.lower_lashes > 0.0 {
        lashes(rgb, ctx, k(m.lashes), k(m.lower_lashes));
    }
    if m.eyeliner > 0.0 {
        eyeliner(rgb, ctx, k(m.eyeliner), k(m.eyeliner_wing));
    }
}

// ---- the lid lines ----------------------------------------------------------------------

/// Half the width of an eye opening, in interocular units, when nothing better is measured.
const HALF_EYE: f32 = 0.27;
const COLUMNS: usize = 24;

/// One eye's lid lines in the face's frame, relative to the eye's centre.
struct Lids {
    /// Which way the outer corner is along `u`: -1 for the image-left eye.
    outer: f32,
    /// The eye's centre along `u`.
    centre: f32,
    /// Inner and outer extent of the opening, relative to `centre`, signed along `u`.
    span: (f32, f32),
    /// The upper and lower lid, sampled across `span`. On a closed eye, the top and the bottom
    /// of the lashes.
    top: [f32; COLUMNS],
    bottom: [f32; COLUMNS],
    /// No white of the eye shows: the lid is down and the lashes hang from its edge.
    closed: bool,
}

impl Lids {
    /// Measure an eye's opening from the parse's eye region, or fall back on a lid curve.
    fn measure(eyes: &OwnedView, f: &Face, side: f32) -> Self {
        let centre = side * 0.5;
        let mut top = [f32::NAN; COLUMNS];
        let mut bottom = [f32::NAN; COLUMNS];
        let mut found = 0;
        for (i, (t, b)) in top.iter_mut().zip(bottom.iter_mut()).enumerate() {
            let a = -HALF_EYE * 1.2 + 2.4 * HALF_EYE * (i as f32 + 0.5) / COLUMNS as f32;
            let mut first = None;
            let mut last = None;
            let mut v = -0.2_f32;
            while v <= 0.2 {
                let p = f.at(centre + a, v);
                let (x, y) = (p[0].max(0.0) as usize, p[1].max(0.0) as usize);
                if eyes.at(x, y) > 0.5 {
                    first.get_or_insert(v);
                    last = Some(v);
                }
                v += 0.01;
            }
            if let (Some(a0), Some(a1)) = (first, last) {
                *t = a0;
                *b = a1;
                found += 1;
            }
        }
        let mut lids = Self {
            outer: side,
            centre,
            span: (-HALF_EYE, HALF_EYE),
            top,
            bottom,
            closed: false,
        };
        if found >= COLUMNS / 4 {
            lids.close_gaps();
            lids.smooth();
        } else {
            lids.curve();
        }
        lids
    }

    /// The upper lid's edge, measured down each column: the first sample two fifths of the way
    /// from the lid above the eye to the column's darkest - the upper lashes, on an open eye and
    /// a closed one alike. The eye region's top is the same ellipse open or shut, so a liner on it
    /// crossed the middle of a closed lid. On a closed eye the lashes' tips are where that dark
    /// band ends. A column without a clear dark band keeps the region's line. ADR-0150.
    fn follow_lashes(&mut self, luma: &dyn Fn(f32, f32) -> f32, closed: bool) {
        const STEP: f32 = 0.005;
        const FROM: f32 = -0.2;
        const SAMPLES: usize = 85;
        self.closed = closed;
        for i in 0..COLUMNS {
            let a = Self::a_of(i);
            if self.outwardness(a).is_none() {
                continue;
            }
            let profile: Vec<f32> = (0..SAMPLES)
                .map(|k| {
                    let v = FROM + k as f32 * STEP;
                    (luma(a, v - STEP) + luma(a, v) + luma(a, v + STEP)) / 3.0
                })
                .collect();
            let darkest = profile.iter().copied().fold(f32::INFINITY, f32::min);
            // The lid is the top quarter of the column, above any eye.
            let Some(lid) = percentile(profile[..SAMPLES / 4].iter().map(|v| (*v, 1.0)), 0.5)
            else {
                continue;
            };
            if darkest > 0.8 * lid {
                continue;
            }
            let cut = darkest + 0.4 * (lid - darkest);
            // Two samples running, so one dark speck on the lid is not its edge.
            let Some(top) = (2..SAMPLES - 1).find(|k| profile[*k] < cut && profile[k + 1] < cut)
            else {
                continue;
            };
            self.top[i] = FROM + top as f32 * STEP;
            if closed {
                let mut bottom = top;
                while bottom + 1 < SAMPLES && profile[bottom + 1] < cut {
                    bottom += 1;
                }
                self.bottom[i] = FROM + bottom as f32 * STEP;
            }
        }
        self.smooth();
    }

    /// A median of five columns: a lid is a smooth curve, and one column's lash or shadow made a
    /// liner with steps in it.
    fn smooth(&mut self) {
        for line in [&mut self.top, &mut self.bottom] {
            let old = *line;
            for (i, v) in line.iter_mut().enumerate() {
                let mut near: Vec<f32> = (i.saturating_sub(2)..(i + 3).min(COLUMNS))
                    .map(|j| old[j])
                    .filter(|v| v.is_finite())
                    .collect();
                if near.is_empty() {
                    continue;
                }
                near.sort_by(f32::total_cmp);
                *v = near[near.len() / 2];
            }
        }
    }

    /// Measure both eyes of a face: the upper lid on its lashes, and whether the eye is shut.
    fn both(ctx: &Ctx<'_>, rgb: &[f32], eyes: &OwnedView, f: &Face) -> [Self; 2] {
        let sclera = ctx.view(Region::Sclera);
        let (w, h) = (ctx.w, ctx.h);
        let at = |p: [f32; 2]| {
            (
                (p[0].max(0.0) as usize).min(w - 1),
                (p[1].max(0.0) as usize).min(h - 1),
            )
        };
        [-1.0_f32, 1.0].map(|side| {
            let mut lids = Self::measure(eyes, f, side);
            let centre = side * 0.5;
            let mut white = 0.0_f32;
            for j in 0..8 {
                for i in 0..12 {
                    let a = -0.3 + 0.6 * (i as f32 + 0.5) / 12.0;
                    let v = -0.15 + 0.3 * (j as f32 + 0.5) / 8.0;
                    let (x, y) = at(f.at(centre + a, v));
                    white += sclera.at(x, y);
                }
            }
            let luma = |a: f32, v: f32| {
                let (x, y) = at(f.at(centre + a, v));
                let i = (y * w + x) * 3;
                luma([rgb[i], rgb[i + 1], rgb[i + 2]])
            };
            lids.follow_lashes(&luma, white / 96.0 < 0.03);
            lids
        })
    }

    fn a_of(i: usize) -> f32 {
        -HALF_EYE * 1.2 + 2.4 * HALF_EYE * (i as f32 + 0.5) / COLUMNS as f32
    }

    /// Fill the columns the eye region did not reach, and record where it stops.
    fn close_gaps(&mut self) {
        let measured: Vec<usize> = (0..COLUMNS).filter(|i| self.top[*i].is_finite()).collect();
        let (Some(&lo), Some(&hi)) = (measured.first(), measured.last()) else {
            self.curve();
            return;
        };
        self.span = (Self::a_of(lo), Self::a_of(hi));
        let mut last = (self.top[lo], self.bottom[lo]);
        for i in 0..COLUMNS {
            if self.top[i].is_finite() {
                last = (self.top[i], self.bottom[i]);
            } else {
                self.top[i] = last.0;
                self.bottom[i] = last.1;
            }
        }
    }

    /// The fallback: an almond of the usual proportions.
    fn curve(&mut self) {
        for i in 0..COLUMNS {
            let a = Self::a_of(i) / HALF_EYE;
            let open = (1.0 - a * a).max(0.0);
            self.top[i] = -0.075 * open;
            self.bottom[i] = 0.05 * open;
        }
        self.span = (-HALF_EYE, HALF_EYE);
    }

    fn sample(line: &[f32; COLUMNS], a: f32) -> f32 {
        let t = ((a + HALF_EYE * 1.2) / (2.4 * HALF_EYE) * COLUMNS as f32 - 0.5)
            .clamp(0.0, (COLUMNS - 1) as f32);
        let i = t.floor() as usize;
        let j = (i + 1).min(COLUMNS - 1);
        let f = t - i as f32;
        line[i] + (line[j] - line[i]) * f
    }

    /// `0` at the inner corner rising to `1` at the outer one, and outside the opening.
    fn outwardness(&self, a: f32) -> Option<f32> {
        let (lo, hi) = self.span;
        if a < lo - 0.02 || a > hi + 0.02 {
            return None;
        }
        let t = ((a - lo) / (hi - lo).max(1e-3)).clamp(0.0, 1.0);
        Some(if self.outer > 0.0 { t } else { 1.0 - t })
    }

    /// The outer corner's position and the top lid's height there.
    fn outer_corner(&self) -> (f32, f32) {
        let a = if self.outer > 0.0 {
            self.span.1
        } else {
            self.span.0
        };
        (a, Self::sample(&self.top, a))
    }
}

// ---- eyes -------------------------------------------------------------------------------

/// A line along each upper lid, thicker toward the outer corner, with an optional wing.
fn eyeliner(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32, wing: f32) {
    let eyes = ctx.soft(Region::Eyes, 0);
    let ink = [0.05_f32, 0.035, 0.03];
    for f in &ctx.faces {
        let lids = Lids::both(ctx, rgb, &eyes, f);
        let area = ctx.around(f, -1.05, 1.05, -0.35, 0.15);
        let thick = 0.016 + 0.02 * k;
        let darkness = 0.55 + 0.4 * k;
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let mut wgt = 0.0_f32;
            for l in &lids {
                let a = p[0] - l.centre;
                if let Some(out) = l.outwardness(a) {
                    let th = thick * (0.35 + 0.65 * out);
                    // Height above the lid line, in face units.
                    let d = Lids::sample(&l.top, a) - p[1];
                    let band = smoothstep(-0.006, 0.003, d) * (1.0 - smoothstep(th * 0.6, th, d));
                    wgt = wgt.max(band);
                }
                if wing > 0.0 {
                    // The wing: from the outer corner, up and out at about eighteen degrees.
                    let (ca, cb) = l.outer_corner();
                    let along_u = (a - ca) * l.outer;
                    let length = 0.17 * wing;
                    if along_u > 0.0 && along_u < length {
                        let rise = along_u * 0.33;
                        let taper = 1.0 - along_u / length;
                        let th = thick * taper;
                        let d = (cb - rise) - p[1];
                        let band = smoothstep(-0.004, 0.002, d + th * 0.3)
                            * (1.0 - smoothstep(th * 0.6, th.max(1e-3), d + th * 0.3));
                        wgt = wgt.max(band * taper.sqrt());
                    }
                }
            }
            if wgt <= 0.0 {
                return None;
            }
            let t = wgt * darkness;
            let inked = with_luma(ink, luma(old) * 0.12);
            Some(mix(old, inked, t))
        });
    }
}

/// The lashes already there, made fuller and darker; the lower ones defined.
fn lashes(rgb: &mut [f32], ctx: &Ctx<'_>, upper: f32, lower: f32) {
    let eyes = ctx.soft(Region::Eyes, 0);
    let skin = ctx.view(Region::Skin);
    for f in &ctx.faces {
        let lids = Lids::both(ctx, rgb, &eyes, f);
        let area = ctx.around(f, -1.05, 1.05, -0.35, 0.25);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let on_skin = weights(area, |x, y| skin.at(x, y));
        let lid_level = masked_blur(area, &l, &on_skin, (0.08 * f.io).max(2.0));
        let thick = area.blur(&l, 1.0);
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let mut band = 0.0_f32;
            for lid in &lids {
                let a = p[0] - lid.centre;
                let Some(out) = lid.outwardness(a) else {
                    continue;
                };
                let above = Lids::sample(&lid.top, a) - p[1];
                if lid.closed {
                    // Down: the lashes hang from the lid's edge to their tips.
                    let tips = Lids::sample(&lid.bottom, a) - p[1];
                    let on = smoothstep(-0.006, 0.004, -above) * smoothstep(-0.012, 0.004, tips);
                    band = band.max(on * upper * (0.5 + 0.5 * out));
                    continue;
                }
                let up = smoothstep(-0.01, 0.005, above) * (1.0 - smoothstep(0.03, 0.06, above));
                band = band.max(up * upper * (0.5 + 0.5 * out));
                let below = p[1] - Lids::sample(&lid.bottom, a);
                let down =
                    smoothstep(-0.008, 0.004, below) * (1.0 - smoothstep(0.015, 0.035, below));
                band = band.max(down * lower * 0.7);
            }
            if band <= 0.0 {
                return None;
            }
            let i = area.index(x, y);
            // Only what is already darker than the lid - the lashes - is deepened.
            let dark = smoothstep(0.05, 0.45, 1.0 - thick[i] / lid_level[i].max(1e-5));
            let t = band * dark;
            if t <= 0.0 {
                return None;
            }
            Some(old.map(|v| v * (1.0 - 0.55 * t)))
        });
    }
}

/// Sparse brows filled with the colour of their own darkest hairs.
fn brow_fill(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let brows = ctx.soft(Region::Eyebrows, 1);
    let crisp = ctx.view(Region::Eyebrows);
    for f in &ctx.faces {
        let area = ctx.around(f, -1.0, 1.0, -0.65, -0.05);
        if area.is_empty() {
            continue;
        }
        let l = area.luma(rgb, ctx.w);
        let samples = (area.y0..area.y1)
            .flat_map(|y| (area.x0..area.x1).map(move |x| (x, y)))
            .map(|(x, y)| (l[area.index(x, y)], crisp.at(x, y)));
        let Some(dark) = percentile(samples, 0.33) else {
            continue;
        };
        let Some(colour) = super::mean_colour(rgb, ctx.w, area, |x, y| {
            let i = area.index(x, y);
            if crisp.at(x, y) > 0.5 && l[i] <= dark * 1.1 {
                1.0
            } else {
                0.0
            }
        }) else {
            continue;
        };
        each(rgb, ctx.w, area, |x, y, old| {
            let wgt = brows.at(x, y) * k;
            let here = l[area.index(x, y)];
            if wgt <= 0.0 || here <= dark {
                return None;
            }
            let gap = smoothstep(0.0, 0.5, 1.0 - dark / here.max(1e-5));
            let filled = with_luma(colour, here * 0.35 + dark * 0.65);
            Some(mix(old, filled, wgt * gap * 0.85))
        });
    }
}

// ---- lips -------------------------------------------------------------------------------

/// The lips' outer border, defined in a deeper shade of their own colour. The ring is taken
/// from the whole mouth, so the edge the lips share with the teeth is not outlined.
fn lip_liner(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let lips = ctx.view(Region::Lips);
    let mouth = ctx.view(Region::Mouth);
    for f in &ctx.faces {
        let area = ctx.around(f, -0.8, 0.8, f.mouth - 0.4, f.mouth + 0.4);
        if area.is_empty() {
            continue;
        }
        let on = weights(area, |x, y| {
            mouth.at(x, y).max(lips.at(x, y)) * near_mouth(f, x, y)
        });
        let soft = area.blur(&on, (0.02 * f.io).max(1.0));
        each(rgb, ctx.w, area, |x, y, old| {
            let b = soft[area.index(x, y)];
            let lip = lips.at(x, y) * near_mouth(f, x, y);
            if b <= 0.1 || lip <= 0.0 {
                return None;
            }
            let ring = (4.0 * b * (1.0 - b)).clamp(0.0, 1.0) * k * lip;
            if ring <= 0.0 {
                return None;
            }
            let o = opponent(old);
            let deeper =
                from_opponent([o[0], o[1] * (1.0 + 0.3 * ring), o[2] * (1.0 + 0.3 * ring)]);
            Some(deeper.map(|v| v * (1.0 - 0.35 * ring)))
        });
    }
}

/// Gradient lips: deeper at the centre, softer toward the edges.
fn lip_ombre(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let lips = ctx.view(Region::Lips);
    for f in &ctx.faces {
        let area = ctx.around(f, -0.8, 0.8, f.mouth - 0.4, f.mouth + 0.4);
        each(rgb, ctx.w, area, |x, y, old| {
            let on = lips.at(x, y) * near_mouth(f, x, y);
            if on <= 0.0 {
                return None;
            }
            let p = local(f, x, y);
            let centre = ellipse(p, [0.0, f.mouth], [f.mouth_width * 0.32, 0.12]);
            let o = opponent(old);
            let deep = centre * on * k;
            let edge = (1.0 - centre) * on * k;
            let chroma = 1.0 + 0.4 * deep - 0.15 * edge;
            let y2 = o[0] * (1.0 - 0.25 * deep) * (1.0 + 0.06 * edge);
            Some(from_opponent([
                y2,
                o[1] / o[0].max(1e-5) * y2 * chroma,
                o[2] / o[0].max(1e-5) * y2 * chroma,
            ]))
        });
    }
}

// ---- face -------------------------------------------------------------------------------

/// A soft sheen on cheekbones, nose bridge, cupid's bow, brow bone and chin.
fn highlighter(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let shimmer = [1.0_f32, 0.96, 0.88];
    for f in &ctx.faces {
        let area = ctx.around(f, -1.3, 1.3, -0.5, f.mouth + 0.8);
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let mut s = ellipse(p, [0.0, 0.3], [0.06, 0.28]);
            s = s.max(ellipse(p, [0.0, f.mouth - 0.17], [0.1, 0.04]));
            s = s.max(ellipse(p, [0.0, f.mouth + 0.6], [0.1, 0.06]));
            for side in [-1.0_f32, 1.0] {
                s = s.max(ellipse(p, [side * 0.8, 0.32], [0.3, 0.1]));
                s = s.max(ellipse(p, [side * 0.62, -0.22], [0.18, 0.05]));
            }
            let wgt = s * skin.at(x, y) * k;
            if wgt <= 0.0 {
                return None;
            }
            let l = luma(old) * (1.0 + 0.3 * wgt);
            let bright = old.map(|v| v * (1.0 + 0.3 * wgt));
            Some(mix(bright, with_luma(shimmer, l), 0.15 * wgt))
        });
    }
}

/// A warm sun-kissed tone around the face's edge, its temples and under its cheekbones.
fn bronzer(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let face = ctx.soft(Region::Face, 6);
    // A warm ratio with the luminance of one, so the bronzer warms and the darkening is separate.
    let warm = {
        let r = [1.0_f32, 0.84, 0.66];
        let l = luma(r);
        r.map(|v| v / l)
    };
    for f in &ctx.faces {
        let area = ctx.around(f, -1.5, 1.5, -1.5, f.mouth + 1.0);
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let fs = face.at(x, y);
            let mut s = smoothstep(0.1, 0.45, fs) * (1.0 - smoothstep(0.5, 0.92, fs));
            for side in [-1.0_f32, 1.0] {
                s = s.max(ellipse(p, [side * 0.85, 0.8], [0.3, 0.14]));
                s = s.max(ellipse(p, [side * 0.95, -0.5], [0.2, 0.3]));
            }
            let wgt = s * skin.at(x, y) * k;
            if wgt <= 0.0 {
                return None;
            }
            let t = 0.4 * wgt;
            let tinted = [0, 1, 2].map(|c| old[c] * (1.0 + (warm[c] - 1.0) * t));
            Some(with_luma(tinted, luma(old) * (1.0 - 0.08 * wgt)))
        });
    }
}

/// Lipstick, eyeshadow and blush already worn, made richer: whatever differs from the
/// person's own cheek colour inside those zones differs a little more.
fn enhance(rgb: &mut [f32], ctx: &Ctx<'_>, k: f32) {
    let skin = ctx.view(Region::Skin);
    let lips = ctx.view(Region::Lips);
    let eyes = ctx.soft(Region::Eyes, 1);
    for f in &ctx.faces {
        let Some(cheek) = super::skin::cheeks(rgb, ctx, f) else {
            continue;
        };
        let r = opponent(cheek);
        if r[0] <= 1e-5 {
            continue;
        }
        let (rb, rr) = (r[1] / r[0], r[2] / r[0]);
        let area = ctx.around(f, -1.3, 1.3, -0.5, f.mouth + 0.4);
        each(rgb, ctx.w, area, |x, y, old| {
            let p = local(f, x, y);
            let mut lids = 0.0_f32;
            let mut blush = 0.0_f32;
            for side in [-1.0_f32, 1.0] {
                lids = lids.max(ellipse(p, [side * 0.5, -0.14], [0.32, 0.13]));
                blush = blush.max(ellipse(p, [side * 0.75, 0.55], [0.35, 0.25]));
            }
            let zone = (lips.at(x, y) * near_mouth(f, x, y))
                .max(lids * (1.0 - eyes.at(x, y)) * skin.at(x, y))
                .max(blush * skin.at(x, y) * 0.7);
            if zone <= 0.0 {
                return None;
            }
            let o = opponent(old);
            if o[0] <= 1e-5 {
                return None;
            }
            let (cb, cr) = (o[1] / o[0], o[2] / o[0]);
            let (db, dr) = (cb - rb, cr - rr);
            let worn = smoothstep(0.03, 0.12, db.hypot(dr));
            let gain = 1.0 + 0.7 * k * zone * worn;
            Some(from_opponent([
                o[0],
                (rb + db * gain) * o[0],
                (rr + dr * gain) * o[0],
            ]))
        });
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::super::testing::{self, *};
    use super::*;

    fn run(m: MakeupFinish) -> (Vec<f32>, Vec<f32>) {
        let before = testing::frame();
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(&mut after, &ctx, &m);
        (before, after)
    }

    #[test]
    fn eyeliner_darkens_just_above_each_eye_and_nothing_below_it() {
        let (before, after) = run(MakeupFinish {
            eyeliner: 100.0,
            ..MakeupFinish::default()
        });
        // The top of the left eye opening is at y = 95.
        let above = |rgb: &[f32]| luma(px(rgb, 100, 94));
        assert!(
            above(&after) < above(&before) * 0.7,
            "{} -> {}",
            above(&before),
            above(&after)
        );
        assert!((luma(px(&after, 100, 112)) - luma(px(&before, 100, 112))).abs() < 1e-6);
        assert_eq!(moved(&before, &after, Region::Lips, false, 1e-6), 0);
    }

    #[test]
    fn a_wing_reaches_past_the_outer_corner_only_with_a_liner() {
        let (_, plain) = run(MakeupFinish {
            eyeliner: 100.0,
            ..MakeupFinish::default()
        });
        let (before, winged) = run(MakeupFinish {
            eyeliner: 100.0,
            eyeliner_wing: 100.0,
            ..MakeupFinish::default()
        });
        // Past the left eye's outer corner (x = 89), rising slowly from the eye line.
        let wing_px = (83..89)
            .flat_map(|x| (93..102).map(move |y| (x, y)))
            .filter(|&(x, y)| luma(px(&winged, x, y)) < luma(px(&plain, x, y)) - 1e-3)
            .count();
        assert!(wing_px > 0);
        let (b2, wing_alone) = run(MakeupFinish {
            eyeliner_wing: 100.0,
            ..MakeupFinish::default()
        });
        assert_eq!(b2, wing_alone);
        assert!(!before.is_empty());
    }

    #[test]
    fn lip_liner_works_only_on_the_lips_and_most_at_their_border() {
        let (before, after) = run(MakeupFinish {
            lip_liner: 100.0,
            ..MakeupFinish::default()
        });
        assert_eq!(moved(&before, &after, Region::Lips, true, 1e-6), 0);
        let change = |x, y| luma(px(&before, x, y)) - luma(px(&after, x, y));
        // Border (the lips' left end) against the middle of the lower lip, next to the teeth.
        assert!(change(104, 152) > change(120, 153) + 1e-3);
    }

    #[test]
    fn highlighter_and_bronzer_stay_on_skin() {
        let (before, after) = run(MakeupFinish {
            highlighter: 100.0,
            bronzer: 100.0,
            ..MakeupFinish::default()
        });
        assert_eq!(moved(&before, &after, Region::Skin, true, 1e-6), 0);
        assert!(moved(&before, &after, Region::Skin, false, 1e-3) > 100);
    }

    #[test]
    fn brow_fill_darkens_gaps_in_a_sparse_brow() {
        let mut before = testing::frame();
        // Gaps: every third column of the left brow painted as skin.
        for y in 80..95 {
            for x in (88..113).step_by(3) {
                if part(x, y) == Region::Eyebrows {
                    let i = (y * W + x) * 3;
                    before[i..i + 3].copy_from_slice(&SKIN);
                }
            }
        }
        let mut after = before.clone();
        let map = testing::map();
        let ctx = Ctx::new(W, H, &map, &[]);
        apply(
            &mut after,
            &ctx,
            &MakeupFinish {
                brow_fill: 100.0,
                ..MakeupFinish::default()
            },
        );
        assert!(luma(px(&after, 100, 87)) < luma(px(&before, 100, 87)) * 0.75);
    }
}
