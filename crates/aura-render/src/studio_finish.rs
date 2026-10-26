//! The Studio's finishing tools, rendered: feature colour and makeup, background replacement,
//! and face, body and liquify reshaping. ADR-0108.
//!
//! Runs after the retouch stack and before the Studio's local masks, on the whole frame:
//!
//! 1. **Colours** - hair, eyes, lips and brows take a chosen colour while keeping their own
//!    light and shade; blush and eyeshadow are a soft multiply placed from the face landmarks.
//! 2. **Background** - everything behind the people (or only the sky) is replaced by a colour, a
//!    gradient or a blur of itself. The person is kept by the portrait parse's background plane,
//!    feathered so a stray hair is blended rather than cut.
//! 3. **Light** - a key light shaped by each head, a fill on its shadow side and catchlights
//!    (ADR-0127, `crate::portrait_light`), before any reshape, so the light follows the face.
//! 4. **Shape** - every face slider, every body slider and every liquify stroke adds into one
//!    displacement field, and the frame is resampled through it once. One resample rather than
//!    one per slider, so twenty moves cost the same softness as one.
//! 5. **Glow** - a soft bloom from the finished frame's highlights.
//!
//! ADR-0150 adds a measured pass before the colours (skin, makeup, hair, attire and the people's
//! own light, `crate::studio_evoto`), the background's own adjustments after it is replaced, a
//! vignette that avoids faces at the very end, two more backgrounds, and twenty more face and
//! ten more body sliders in the one displacement field.
//!
//! The regions and landmarks come from `aura_portrait` and the frame as it arrived, exactly as
//! the portrait operators' do, so the four values a delivered file is re-created from still
//! decide every pixel. Nothing here runs unless a photographer moved a control: there is no
//! automatic reshape, and the automatic passes never write this extension.
//!
//! Every displacement primitive is bounded so the field cannot fold: a translation never moves
//! further than two fifths of its own radius, and a scale stays inside `-0.5..=0.5`. A slider at
//! 100 is a strong edit, never a tear.
// Every plane is `w * h` long and every index is clamped into it.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names
)]

use aura_portrait::{FaceGeometry, PortraitMap, Region};
use aura_raw::colour::curve::filmic_lite_inverse;
use aura_recipe::studio_finish::{
    self, Background, BackgroundMode, BodyShape, FaceShape, LiquifyMode, LiquifyStroke, Tint,
};
use aura_recipe::Recipe;
use rayon::prelude::*;

use crate::colour::{luma, set_luma};
use crate::cpu::Frame;

/// True when this recipe carries a finish that changes pixels.
#[must_use]
pub fn wants(recipe: &Recipe) -> bool {
    studio_finish::read(recipe).is_ok_and(|f| !f.is_identity())
}

fn subject_map(
    frame: &Frame,
    recipe: &Recipe,
    finish: &studio_finish::StudioFinish,
) -> Option<std::sync::Arc<PortraitMap>> {
    let parsed = crate::portrait::parse(frame, &crate::portrait::hints(recipe))?;
    Some(
        finish
            .subject_matte
            .as_ref()
            .and_then(|matte| with_subject(&parsed, matte))
            .map_or(parsed, std::sync::Arc::new),
    )
}

/// Check the same stored subject selection used by background repair rendering.
#[must_use]
pub fn background_repair_available(
    frame: &Frame,
    recipe: &Recipe,
    finish: &studio_finish::StudioFinish,
) -> bool {
    subject_map(frame, recipe, finish).is_some_and(|map| {
        let mask = crate::portrait::resolve(&map, Region::Background, frame.width, frame.height);
        mask.iter().any(|v| *v < 0.2) && mask.iter().any(|v| *v > 0.8)
    })
}

/// Apply the finish a recipe carries. A no-op for a recipe without one, and for any part that
/// needs the portrait parse when the frame cannot supply one.
pub(crate) fn apply(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    frame: Option<&Frame>,
) {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgb.len() != w * h * 3 {
        return;
    }
    let Ok(finish) = studio_finish::read(recipe) else {
        return;
    };
    if finish.is_identity() {
        return;
    }
    let map = if finish.needs_parse() {
        frame.and_then(|f| subject_map(f, recipe, &finish))
    } else {
        None
    };
    // The people as the learned selection found them, where one was frozen into the recipe:
    // every tool that tells them from their background reads it instead of the parse's colour
    // models. ADR-0150.
    if let Some(map) = map.as_deref() {
        // Measured on the photograph as parsed, before any colour or background replaces it.
        crate::studio_evoto::before_shape(rgb, w, h, &finish, map);
        apply_colours(rgb, w, h, &finish.colours, map);
        if let Some(background) = finish.background.filter(|b| b.amount > 0.0) {
            apply_background(rgb, w, h, background, map, finish.subject_matte.as_ref());
        }
        crate::studio_evoto::after_background(rgb, w, h, &finish, map);
        // Light follows the face as photographed, so it goes on before any reshape moves it.
        if finish.light.needs_faces() {
            crate::portrait_light::apply(rgb, w, h, &finish.light, map);
        }
    }
    let mut field = Field::new(w, h);
    if let Some(map) = map.as_deref() {
        let scale = Scale::of(map, w, h);
        let chosen = crate::studio_evoto::chosen_faces(map, &finish.faces);
        if !finish.face.is_identity() {
            for face in &chosen {
                face_warps(&mut field, &scale, map, face, &finish.face, rgb);
            }
        }
        if !finish.body.is_identity() {
            for face in &chosen {
                head_and_neck_warps(&mut field, &scale, face, &finish.body, rgb);
            }
            body_warps(
                &mut field,
                &scale,
                map,
                &finish.body,
                chosen.first().copied(),
            );
        }
    }
    for stroke in &finish.liquify {
        liquify(&mut field, stroke);
    }
    if field.touched {
        let source = rgb.to_vec();
        match map.as_deref() {
            Some(map) if finish.background_repair > 0.0 => field.resample_layered(
                &source,
                &region(map, Region::Background, w, h),
                finish.background_repair / 100.0,
                rgb,
            ),
            _ => field.resample(&source, rgb),
        }
    }
    // The glow blooms from the finished frame's highlights.
    crate::portrait_light::apply_glow(rgb, w, h, finish.light.glow);
    if let Some(map) = map.as_deref() {
        crate::studio_evoto::last(rgb, w, h, &finish, map);
    }
}

// ---- colour -----------------------------------------------------------------------------

/// sRGB `0..=1` as linear BT.709.
fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// A picked sRGB colour as linear Rec.2020 *display* light: what the output transform encodes
/// back to the same sRGB value, so a chosen white exports as that white.
#[must_use]
pub fn picked_colour(srgb: [f32; 3]) -> [f32; 3] {
    let l = srgb.map(|v| srgb_to_linear(v.clamp(0.0, 1.0)));
    // BT.709 to BT.2020 primaries (ITU-R BT.2087).
    let m = [
        [0.627_4, 0.329_3, 0.043_3],
        [0.069_1, 0.919_5, 0.011_4],
        [0.016_4, 0.088_0, 0.895_6],
    ];
    let d = [0, 1, 2].map(|r| m[r][0] * l[0] + m[r][1] * l[1] + m[r][2] * l[2]);
    // Undo the output roll-off, so the value lands where the picker said; the inverse is
    // unbounded at one, so stop just short of it.
    d.map(|v| filmic_lite_inverse(v.clamp(0.0, 0.995)))
}

/// The same colour as a linear ratio with its brightest channel at one, for a multiply.
fn tint_ratio(srgb: [f32; 3]) -> [f32; 3] {
    let l = picked_colour(srgb);
    let top = l.iter().copied().fold(1e-6, f32::max);
    l.map(|v| (v / top).max(0.02))
}

/// A region resolved onto the buffer.
pub(crate) fn region(map: &PortraitMap, region: Region, w: usize, h: usize) -> Vec<f32> {
    crate::portrait::resolve(map, region, w as u32, h as u32)
}

/// Lay `tint` over a region, keeping each pixel's light and shade. `lift` moves the brightness
/// that share of the way (in log terms) toward the colour's own, which a hair dye needs and an
/// iris does not.
fn colour_blend(rgb: &mut [f32], weight: &[f32], tint: Tint, lift: f32) {
    let target = picked_colour(tint.colour);
    let target_luma = luma(target).max(1e-4);
    let amount = (tint.amount / 100.0).clamp(0.0, 1.0);
    rgb.par_chunks_mut(3)
        .zip(weight.par_iter())
        .for_each(|(p, k)| {
            let k = k.clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                return;
            }
            let here = [p[0], p[1], p[2]];
            let l = luma(here).max(1e-5);
            let goal = (l.ln() + (target_luma.ln() - l.ln()) * lift * amount).exp();
            let coloured = set_luma(target, goal);
            for c in 0..3 {
                p[c] = here[c] + (coloured[c] - here[c]) * k;
            }
        });
}

/// Multiply a region by a colour, the way makeup sits on skin.
fn colour_multiply(rgb: &mut [f32], weight: &[f32], tint: Tint, depth: f32) {
    let ratio = tint_ratio(tint.colour);
    let amount = (tint.amount / 100.0).clamp(0.0, 1.0) * depth;
    rgb.par_chunks_mut(3)
        .zip(weight.par_iter())
        .for_each(|(p, k)| {
            let k = k.clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                return;
            }
            for c in 0..3 {
                p[c] *= 1.0 + (ratio[c] - 1.0) * k;
            }
        });
}

/// An elliptical soft spot in buffer pixels: one at the centre, zero at the rim.
fn spot(plane: &mut [f32], w: usize, h: usize, centre: [f32; 2], radii: [f32; 2], angle: f32) {
    let (rx, ry) = (radii[0].max(1.0), radii[1].max(1.0));
    let reach = rx.max(ry);
    let x0 = (centre[0] - reach).floor().max(0.0) as usize;
    let x1 = ((centre[0] + reach).ceil().max(0.0) as usize).min(w);
    let y0 = (centre[1] - reach).floor().max(0.0) as usize;
    let y1 = ((centre[1] + reach).ceil().max(0.0) as usize).min(h);
    let (s, c) = angle.sin_cos();
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - centre[0];
            let dy = y as f32 + 0.5 - centre[1];
            let u = (dx * c + dy * s) / rx;
            let v = (-dx * s + dy * c) / ry;
            let r2 = u * u + v * v;
            if r2 < 1.0 {
                let f = (1.0 - r2) * (1.0 - r2);
                let slot = &mut plane[y * w + x];
                *slot = slot.max(f);
            }
        }
    }
}

fn apply_colours(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    colours: &studio_finish::Colours,
    map: &PortraitMap,
) {
    let live = |t: Option<Tint>| t.filter(|t| t.amount > 0.0);
    if let Some(t) = live(colours.hair) {
        colour_blend(rgb, &region(map, Region::Hair, w, h), t, 0.35);
    }
    if let Some(t) = live(colours.eyes) {
        colour_blend(rgb, &region(map, Region::Iris, w, h), t, 0.1);
    }
    if let Some(t) = live(colours.lips) {
        colour_blend(rgb, &region(map, Region::Lips, w, h), t, 0.15);
    }
    if let Some(t) = live(colours.eyebrows) {
        colour_blend(rgb, &region(map, Region::Eyebrows, w, h), t, 0.2);
    }
    if map.faces.is_empty() {
        return;
    }
    let scale = Scale::of(map, w, h);
    if let Some(t) = live(colours.blush) {
        let mut plane = vec![0.0_f32; w * h];
        for face in &map.faces {
            let f = scale.face(face);
            for side in [-1.0, 1.0] {
                spot(
                    &mut plane,
                    w,
                    h,
                    f.at(side * 0.8, f.mouth * 0.55),
                    [0.55 * f.io, 0.4 * f.io],
                    f.roll,
                );
            }
        }
        // On skin only, so a cheek's blush never lands on hair or a collar.
        let skin = region(map, Region::Skin, w, h);
        plane
            .par_iter_mut()
            .zip(skin.par_iter())
            .for_each(|(p, s)| *p *= s.clamp(0.0, 1.0));
        colour_multiply(rgb, &plane, t, 0.45);
    }
    if let Some(t) = live(colours.eyeshadow) {
        let mut plane = vec![0.0_f32; w * h];
        for face in &map.faces {
            let f = scale.face(face);
            for eye in [f.left_eye, f.right_eye] {
                let lid = [eye[0] + f.v[0] * -0.2 * f.io, eye[1] + f.v[1] * -0.2 * f.io];
                spot(&mut plane, w, h, lid, [0.38 * f.io, 0.2 * f.io], f.roll);
            }
        }
        // Never over the eye itself.
        let eyes = region(map, Region::Eyes, w, h);
        plane
            .par_iter_mut()
            .zip(eyes.par_iter())
            .for_each(|(p, e)| *p *= 1.0 - e.clamp(0.0, 1.0));
        colour_multiply(rgb, &plane, t, 0.6);
    }
}

// ---- background -------------------------------------------------------------------------

fn short_edge(w: usize, h: usize) -> f32 {
    w.min(h) as f32
}

fn apply_background(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    b: Background,
    map: &PortraitMap,
    subject: Option<&aura_recipe::retouch_tools::Matte>,
) {
    let sky = matches!(b.mode, BackgroundMode::Sky | BackgroundMode::CloudSky);
    let which = if sky { Region::Sky } else { Region::Background };
    // A learned selection of the people, refined against the photograph's own edges at full
    // resolution, is the cleanest line between them and what replaces their background.
    let refined = subject
        .filter(|_| !sky)
        .and_then(|m| crate::retouch_matte::MattePlane::render(m, rgb, w, h));
    let mut weight = match &refined {
        Some(person) => (0..w * h)
            .into_par_iter()
            .map(|i| 1.0 - person.at(i % w, i / w).clamp(0.0, 1.0))
            .collect(),
        None => region(map, which, w, h),
    };
    if weight.iter().all(|v| *v <= 1e-3) {
        return;
    }
    let feather = ((b.feather / 100.0) * 0.012 * short_edge(w, h)).round() as usize;
    if feather > 0 {
        weight = crate::spatial::blur_plane(&weight, w, h, feather);
    }
    let amount = (b.amount / 100.0).clamp(0.0, 1.0);
    match b.mode {
        BackgroundMode::Colour => {
            let c = picked_colour(b.colour);
            fill(rgb, &weight, amount, w, |_| c);
        }
        BackgroundMode::KeepShadows => keep_shadows(rgb, w, h, &weight, amount, b.colour),
        BackgroundMode::CloudSky => cloud_sky(rgb, w, h, &weight, amount, b),
        BackgroundMode::Gradient => {
            let (top, bottom) = (picked_colour(b.colour), picked_colour(b.colour2));
            fill(rgb, &weight, amount, w, |y| {
                lerp3(top, bottom, y as f32 / h.max(2) as f32)
            });
        }
        BackgroundMode::Sky => {
            // The gradient runs from the top of the frame to the lowest row that is still sky.
            let horizon = (0..h)
                .rev()
                .find(|y| weight[y * w..(y + 1) * w].iter().any(|v| *v > 0.5))
                .unwrap_or(h / 2)
                .max(1);
            let (top, low) = (picked_colour(b.colour), picked_colour(b.colour2));
            // Keep the sky's own clouds as a faint texture so the replacement is not flat.
            let base = crate::spatial::luma_plane(rgb, w, h);
            let mean = {
                let (mut s, mut n) = (0.0_f64, 0.0_f64);
                for (l, k) in base.iter().zip(weight.iter()) {
                    s += f64::from(l * k);
                    n += f64::from(*k);
                }
                if n > 0.0 {
                    (s / n) as f32
                } else {
                    0.18
                }
            }
            .max(1e-4);
            rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
                let c = lerp3(top, low, (y as f32 / horizon as f32).min(1.0));
                for x in 0..w {
                    let k = weight[y * w + x].clamp(0.0, 1.0) * amount;
                    if k <= 1e-4 {
                        continue;
                    }
                    let texture = (base[y * w + x] / mean).clamp(0.6, 1.6).powf(0.35);
                    for ch in 0..3 {
                        let p = &mut row[x * 3 + ch];
                        *p += (c[ch] * texture - *p) * k;
                    }
                }
            });
        }
        BackgroundMode::Blur => {
            let radius = ((b.amount / 100.0) * 0.035 * short_edge(w, h))
                .round()
                .max(1.0) as usize;
            // A normalised convolution: the background blurred over the background only, so the
            // person does not bleed into it as a dark halo.
            let mut planes = [
                vec![0.0_f32; w * h],
                vec![0.0_f32; w * h],
                vec![0.0_f32; w * h],
            ];
            for (i, k) in weight.iter().enumerate() {
                for (c, plane) in planes.iter_mut().enumerate() {
                    plane[i] = rgb[i * 3 + c] * k;
                }
            }
            let norm = crate::spatial::blur_plane(&weight, w, h, radius);
            let blurred: Vec<Vec<f32>> = planes
                .iter()
                .map(|p| crate::spatial::blur_plane(p, w, h, radius))
                .collect();
            rgb.par_chunks_mut(3).enumerate().for_each(|(i, p)| {
                let k = weight[i].clamp(0.0, 1.0);
                if k <= 1e-4 || norm[i] <= 1e-4 {
                    return;
                }
                for c in 0..3 {
                    let soft = blurred[c][i] / norm[i];
                    p[c] += (soft - p[c]) * k;
                }
            });
        }
    }
}

/// A flat colour that keeps the shadows on the backdrop: the backdrop's own light, measured
/// against its lit level and smoothed of its texture, multiplies the new colour. ADR-0150.
fn keep_shadows(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    weight: &[f32],
    amount: f32,
    colour: [f32; 3],
) {
    let c = picked_colour(colour);
    let base = crate::spatial::luma_plane(rgb, w, h);
    let mut lit: Vec<f32> = base
        .iter()
        .zip(weight)
        .step_by(7)
        .filter(|(_, k)| **k > 0.5)
        .map(|(l, _)| *l)
        .collect();
    if lit.len() < 16 {
        fill(rgb, weight, amount, w, |_| c);
        return;
    }
    lit.sort_by(f32::total_cmp);
    let level = lit[(lit.len() - 1) * 85 / 100].max(1e-4);
    // The backdrop's light without its texture, measured over the backdrop alone.
    let radius = (0.01 * short_edge(w, h)).round().max(1.0) as usize;
    let masked: Vec<f32> = base.iter().zip(weight).map(|(l, k)| l * k).collect();
    let top = crate::spatial::blur_plane(&masked, w, h, radius);
    let bottom = crate::spatial::blur_plane(weight, w, h, radius);
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let i = y * w + x;
            let k = weight[i].clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                continue;
            }
            let here = if bottom[i] > 1e-3 {
                top[i] / bottom[i]
            } else {
                base[i]
            };
            let shade = (here / level).clamp(0.15, 1.0);
            for ch in 0..3 {
                let p = &mut row[x * 3 + ch];
                *p += (c[ch] * shade - *p) * k;
            }
        }
    });
}

/// A deterministic value in `0..1` for one lattice point.
fn lattice(x: i64, y: i64, seed: u64) -> f32 {
    let mut v = seed ^ 0x9e37_79b9_7f4a_7c15;
    for part in [x as u64, y as u64] {
        v ^= part.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        v = v.rotate_left(31).wrapping_mul(0x94d0_49bb_1331_11eb);
    }
    v ^= v >> 29;
    (v >> 40) as f32 / (1_u64 << 24) as f32
}

/// Smooth value noise in `0..1`, five octaves, `cell` pixels across the coarsest.
fn clouds_at(x: f32, y: f32, cell: f32) -> f32 {
    let mut total = 0.0_f32;
    let mut norm = 0.0_f32;
    let mut size = cell.max(2.0);
    let mut weight = 1.0_f32;
    for octave in 0..5_u64 {
        let (fx, fy) = (x / size, y / size * 1.8);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
        let (ix, iy) = (x0 as i64, y0 as i64);
        let a = lattice(ix, iy, octave);
        let b = lattice(ix + 1, iy, octave);
        let c = lattice(ix, iy + 1, octave);
        let d = lattice(ix + 1, iy + 1, octave);
        let top = a + (b - a) * sx;
        let bottom = c + (d - c) * sx;
        total += (top + (bottom - top) * sy) * weight;
        norm += weight;
        weight *= 0.5;
        size *= 0.5;
    }
    total / norm
}

/// The sky replaced by a gradient with soft clouds, thinning toward the top. ADR-0150.
fn cloud_sky(rgb: &mut [f32], w: usize, h: usize, weight: &[f32], amount: f32, b: Background) {
    let horizon = (0..h)
        .rev()
        .find(|y| weight[y * w..(y + 1) * w].iter().any(|v| *v > 0.5))
        .unwrap_or(h / 2)
        .max(1);
    let (top, low) = (picked_colour(b.colour), picked_colour(b.colour2));
    let cloud = picked_colour([0.97, 0.97, 0.98]);
    let shadow = picked_colour([0.78, 0.8, 0.84]);
    let cell = w.max(h) as f32 / 4.0;
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let depth = (y as f32 / horizon as f32).min(1.0);
        let sky = lerp3(top, low, depth);
        for x in 0..w {
            let k = weight[y * w + x].clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                continue;
            }
            let n = clouds_at(x as f32, y as f32, cell);
            let density = smooth(0.52, 0.78, n) * (0.55 + 0.45 * depth);
            // The underside of a cloud is in its own shade.
            let under = smooth(
                0.0,
                0.08,
                clouds_at(x as f32, y as f32 - cell * 0.04, cell) - n,
            );
            let white = lerp3(cloud, shadow, under * 0.6);
            let c = lerp3(sky, white, density * 0.9);
            for ch in 0..3 {
                let p = &mut row[x * 3 + ch];
                *p += (c[ch] - *p) * k;
            }
        }
    });
}

fn smooth(low: f32, high: f32, v: f32) -> f32 {
    let t = ((v - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The parse with its people and background taken from a stored selection of the people, on the
/// parse's own grid: the person is the selection, the background its complement, and hair,
/// clothing, body skin and sky are kept inside the side of the line they belong to. `None` when
/// the selection does not decode. ADR-0150.
fn with_subject(
    map: &PortraitMap,
    matte: &aura_recipe::retouch_tools::Matte,
) -> Option<PortraitMap> {
    let alpha = matte.decode()?;
    let (mw, mh) = (matte.width as usize, matte.height as usize);
    if mw == 0 || mh == 0 || alpha.len() != mw * mh {
        return None;
    }
    let [l, t, r, b] = matte.bounds;
    let (pw, ph) = (map.width as usize, map.height as usize);
    let cell = |x: usize, y: usize| f32::from(alpha[y.min(mh - 1) * mw + x.min(mw - 1)]) / 255.0;
    let person: Vec<f32> = (0..pw * ph)
        .into_par_iter()
        .map(|i| {
            let u = (((i % pw) as f32 + 0.5) / pw as f32 - l) / (r - l).max(1e-6);
            let v = (((i / pw) as f32 + 0.5) / ph as f32 - t) / (b - t).max(1e-6);
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                return 0.0;
            }
            let fx = (u * mw as f32 - 0.5).clamp(0.0, (mw - 1) as f32);
            let fy = (v * mh as f32 - 0.5).clamp(0.0, (mh - 1) as f32);
            let (x0, y0) = (fx as usize, fy as usize);
            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
            let top = cell(x0, y0) + (cell(x0 + 1, y0) - cell(x0, y0)) * tx;
            let bottom = cell(x0, y0 + 1) + (cell(x0 + 1, y0 + 1) - cell(x0, y0 + 1)) * tx;
            (top + (bottom - top) * ty).clamp(0.0, 1.0)
        })
        .collect();
    let mut out = map.clone();
    for (region, plane) in &mut out.planes {
        if plane.values.len() != pw * ph {
            continue;
        }
        match region {
            Region::Body => plane.values.clone_from(&person),
            Region::Background => {
                plane.values = person.iter().map(|p| 1.0 - p).collect();
            }
            Region::Sky => {
                for (v, p) in plane.values.iter_mut().zip(&person) {
                    *v = v.min(1.0 - p);
                }
            }
            Region::Hair | Region::Clothing | Region::BodySkin => {
                for (v, p) in plane.values.iter_mut().zip(&person) {
                    *v = v.min(*p);
                }
            }
            _ => {}
        }
    }
    Some(out)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t)
}

fn fill(
    rgb: &mut [f32],
    weight: &[f32],
    amount: f32,
    w: usize,
    colour: impl Fn(usize) -> [f32; 3] + Sync,
) {
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let c = colour(y);
        for x in 0..w {
            let k = weight[y * w + x].clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                continue;
            }
            for ch in 0..3 {
                let p = &mut row[x * 3 + ch];
                *p += (c[ch] - *p) * k;
            }
        }
    });
}

// ---- the displacement field -------------------------------------------------------------

/// A backward displacement: the output pixel at `p` shows the input at `p + d(p)`.
struct Field {
    w: usize,
    h: usize,
    dx: Vec<f32>,
    dy: Vec<f32>,
    touched: bool,
}

/// One bounded deformation, in buffer pixels.
#[derive(Debug, Clone, Copy)]
enum Warp {
    /// Moves what is inside the ellipse by `v`, fully at the centre and not at all at the rim.
    Move {
        c: [f32; 2],
        r: [f32; 2],
        angle: f32,
        v: [f32; 2],
    },
    /// Enlarges (`a > 0`) or shrinks what is inside, separately along the ellipse's two axes.
    Scale {
        c: [f32; 2],
        r: [f32; 2],
        angle: f32,
        a: [f32; 2],
    },
    /// Takes the displacement already in the field back toward none.
    Restore { c: [f32; 2], r: f32, k: f32 },
    /// Turns what is inside the circle by `angle` radians (clockwise on screen), fully at the
    /// centre and not at all at the rim. ADR-0150.
    Rotate { c: [f32; 2], r: f32, angle: f32 },
}

/// The falloff every primitive shares: smooth, one at the centre, zero with zero slope at the rim.
fn falloff(r2: f32) -> f32 {
    if r2 >= 1.0 {
        0.0
    } else {
        (1.0 - r2) * (1.0 - r2)
    }
}

impl Field {
    /// Resample with the background kept in place (ADR-0150 background repair).
    ///
    /// The subject is warped exactly as the field says. The background is a separate layer
    /// whose displacement follows the subject at its edge and relaxes to `1 - amount` of the
    /// field further out, so straight lines behind a reshaped person stay straight beyond a
    /// short, smooth transition. Following at the edge is what keeps an uncertain, soft
    /// boundary (out-of-focus clothing, hair) from tearing into a doubled contour or a seam,
    /// and it means no strip of background is ever uncovered without a source. The transition
    /// starts at two and a half times the largest displacement and is widened wherever it would
    /// fold the layer where the warp itself does not (see `layer_relax`).
    /// Pixels the field does not move are returned unchanged, whatever the segmentation says.
    fn resample_layered(&self, src: &[f32], background: &[f32], amount: f32, out: &mut [f32]) {
        let (w, h) = (self.w, self.h);
        if self.dx.len() != w * h || background.len() != w * h || src.len() != w * h * 3 {
            self.resample(src, out);
            return;
        }
        let amount = amount.clamp(0.0, 1.0);
        let plate = background_plate(src, background, w, h);
        let largest = self
            .dx
            .iter()
            .zip(&self.dy)
            .map(|(x, y)| x.hypot(*y))
            .fold(0.0_f32, f32::max);
        let reach = (0.03 * w.min(h) as f32).max(2.5 * largest).max(2.0);
        let relax = self.layer_relax(background, amount, reach);
        out.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let i = y * w + x;
                let (dx, dy) = (self.dx[i], self.dy[i]);
                let (sx, sy) = (x as f32 + dx, y as f32 + dy);
                let warped = bilinear3(src, w, h, sx, sy);
                // Within a pixel of no displacement the plain warp is kept, so the layer never
                // repaints a soft subject edge that nothing moved.
                let moved = dx.hypot(dy).min(1.0);
                let gate = moved * moved * (3.0 - 2.0 * moved);
                if gate <= 0.0 {
                    row[x * 3..x * 3 + 3].copy_from_slice(&warped);
                    continue;
                }
                let relax = relax[i];
                let keep = 1.0 - amount * relax;
                let subject = (1.0 - bilinear1(background, w, h, sx, sy)).clamp(0.0, 1.0);
                let (bx, by) = (x as f32 + dx * keep, y as f32 + dy * keep);
                // At the subject the layer is the plain warp; the filled plate fades in only as
                // the layer straightens, so an uncertain edge is never repainted with fill.
                let plated = bilinear3(&plate, w, h, bx, by);
                let behind = if relax >= 1.0 {
                    plated
                } else {
                    let own = bilinear3(src, w, h, bx, by);
                    std::array::from_fn(|c| own[c] + relax * (plated[c] - own[c]))
                };
                for c in 0..3 {
                    let layered = subject * warped[c] + (1.0 - subject) * behind[c];
                    row[x * 3 + c] = if gate >= 1.0 {
                        layered
                    } else {
                        warped[c] + gate * (layered - warped[c])
                    };
                }
            }
        });
    }

    /// How far the background layer relaxes from the subject's warp toward straight, per pixel:
    /// zero at the subject, one beyond the transition. The transition starts at `reach`; where
    /// relaxing over it would fold the layer somewhere the warp itself does not fold - heavily
    /// stacked strokes can - it is widened, and if no width is safe the layer follows the warp
    /// everywhere, which is the unrepaired result. Repair never doubles background content.
    fn layer_relax(&self, background: &[f32], amount: f32, reach: f32) -> Vec<f32> {
        let (w, h) = (self.w, self.h);
        let distance = distance_from_subject(background, w, h);
        let ramp = |width: f32| -> Vec<f32> {
            distance
                .iter()
                .map(|d| {
                    let t = (d / width).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                })
                .collect()
        };
        if w < 2 || h < 2 || amount <= 0.0 {
            return ramp(reach);
        }
        // The smallest local area scale of `p -> p + d * keep`, by forward differences.
        let area = |keep: &dyn Fn(usize) -> f32, i: usize| {
            let (r, d) = (i + 1, i + w);
            let a = 1.0 + self.dx[r] * keep(r) - self.dx[i] * keep(i);
            let b = self.dx[d] * keep(d) - self.dx[i] * keep(i);
            let c = self.dy[r] * keep(r) - self.dy[i] * keep(i);
            let e = 1.0 + self.dy[d] * keep(d) - self.dy[i] * keep(i);
            (a * e - b * c).min(a).min(e)
        };
        let warp = |_: usize| 1.0;
        for widen in [1.0, 1.6, 2.6, 4.2, 6.8] {
            let relax = ramp(reach * widen);
            let keep = |k: usize| 1.0 - amount * relax[k];
            let folds = (0..h - 1).into_par_iter().any(|y| {
                (0..w - 1).any(|x| {
                    let i = y * w + x;
                    area(&keep, i) <= 0.0 && area(&warp, i) > 0.0
                })
            });
            if !folds {
                return relax;
            }
        }
        vec![0.0; w * h]
    }

    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            dx: Vec::new(),
            dy: Vec::new(),
            touched: false,
        }
    }

    fn ensure(&mut self) {
        if self.dx.is_empty() {
            self.dx = vec![0.0; self.w * self.h];
            self.dy = vec![0.0; self.w * self.h];
        }
    }

    fn add(&mut self, warp: Warp) {
        let (c, reach) = match warp {
            Warp::Move { c, r, .. } | Warp::Scale { c, r, .. } => (c, r[0].max(r[1])),
            Warp::Restore { c, r, .. } | Warp::Rotate { c, r, .. } => (c, r),
        };
        if !reach.is_finite() || reach < 0.5 || !c[0].is_finite() || !c[1].is_finite() {
            return;
        }
        let x0 = (c[0] - reach).floor().max(0.0) as usize;
        let x1 = ((c[0] + reach).ceil().max(0.0) as usize).min(self.w);
        let y0 = (c[1] - reach).floor().max(0.0) as usize;
        let y1 = ((c[1] + reach).ceil().max(0.0) as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        self.ensure();
        self.touched = true;
        let w = self.w;
        let rows = self.dx[y0 * w..y1 * w]
            .par_chunks_mut(w)
            .zip(self.dy[y0 * w..y1 * w].par_chunks_mut(w));
        rows.enumerate().for_each(|(j, (rx_row, ry_row))| {
            let y = (y0 + j) as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5 - c[0];
                let py = y - c[1];
                match warp {
                    Warp::Move { r, angle, v, .. } => {
                        let (s, co) = angle.sin_cos();
                        let u = (px * co + py * s) / r[0];
                        let t = (-px * s + py * co) / r[1];
                        let f = falloff(u * u + t * t);
                        rx_row[x] -= v[0] * f;
                        ry_row[x] -= v[1] * f;
                    }
                    Warp::Scale { r, angle, a, .. } => {
                        let (s, co) = angle.sin_cos();
                        let lu = px * co + py * s;
                        let lt = -px * s + py * co;
                        let (u, t) = (lu / r[0], lt / r[1]);
                        let f = falloff(u * u + t * t);
                        if f > 0.0 {
                            // Sampling nearer the centre enlarges; further away shrinks.
                            let du = -lu * a[0] * f;
                            let dt = -lt * a[1] * f;
                            rx_row[x] += du * co - dt * s;
                            ry_row[x] += du * s + dt * co;
                        }
                    }
                    Warp::Restore { r, k, .. } => {
                        let f = falloff((px * px + py * py) / (r * r));
                        rx_row[x] *= 1.0 - k * f;
                        ry_row[x] *= 1.0 - k * f;
                    }
                    Warp::Rotate { r, angle, .. } => {
                        let f = falloff((px * px + py * py) / (r * r));
                        if f > 0.0 {
                            // The output shows the input turned back by the same angle.
                            let (s, co) = (-angle * f).sin_cos();
                            rx_row[x] += px * co - py * s - px;
                            ry_row[x] += px * s + py * co - py;
                        }
                    }
                }
            }
        });
    }

    /// A vertical stretch of everything below `from`, ramped in over `ramp` rows above it, so
    /// legs lengthen without a crease at the hip. `s > 0` lengthens.
    fn stretch_below(&mut self, from: f32, ramp: f32, s: f32, x_range: (f32, f32, f32)) {
        if s == 0.0 || !from.is_finite() {
            return;
        }
        self.ensure();
        self.touched = true;
        let w = self.w;
        let (left, right, soft) = x_range;
        let factor = 1.0 / (1.0 + s) - 1.0;
        self.dy.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let y = y as f32 + 0.5;
            // Below `from` the stretch grows with distance; inside the ramp it eases in.
            let depth = if y >= from {
                y - from + ramp * 0.5
            } else if y > from - ramp {
                let t = (y - (from - ramp)) / ramp;
                t * t * ramp * 0.5
            } else {
                return;
            };
            for (x, slot) in row.iter_mut().enumerate() {
                let x = x as f32 + 0.5;
                let edge = ((x - left).min(right - x) / soft).clamp(0.0, 1.0);
                let k = edge * edge * (3.0 - 2.0 * edge);
                *slot += depth * factor * k;
            }
        });
    }

    /// A vertical stretch of the rows between `from` and `to` by `1 + s`, with everything below
    /// `to` carried down by what the band grew, eased in above `from` and out below `to` so
    /// neither end creases. ADR-0150.
    fn stretch_band(&mut self, from: f32, to: f32, s: f32, x_range: (f32, f32, f32)) {
        if s == 0.0 || !from.is_finite() || !to.is_finite() || to <= from {
            return;
        }
        self.ensure();
        self.touched = true;
        let w = self.w;
        let (left, right, soft) = x_range;
        let span = to - from;
        let ease = span * 0.15;
        let grown = span * s;
        self.dy.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let y = y as f32 + 0.5;
            // How far this output row looks back up the frame, smoothly from zero to `grown`.
            let back = if y <= from - ease {
                return;
            } else if y < from + span * (1.0 + s) {
                let t = ((y - (from - ease)) / (span * (1.0 + s) + ease)).clamp(0.0, 1.0);
                grown * t * t * (3.0 - 2.0 * t)
            } else {
                grown
            };
            for (x, slot) in row.iter_mut().enumerate() {
                let x = x as f32 + 0.5;
                let edge = ((x - left).min(right - x) / soft).clamp(0.0, 1.0);
                let k = edge * edge * (3.0 - 2.0 * edge);
                *slot -= back * k;
            }
        });
    }

    fn resample(&self, src: &[f32], out: &mut [f32]) {
        let (w, h) = (self.w, self.h);
        out.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let i = y * w + x;
                let sx = (x as f32 + self.dx[i]).clamp(0.0, (w - 1) as f32);
                let sy = (y as f32 + self.dy[i]).clamp(0.0, (h - 1) as f32);
                let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
                for c in 0..3 {
                    let a = src[(y0 * w + x0) * 3 + c];
                    let b = src[(y0 * w + x1) * 3 + c];
                    let d = src[(y1 * w + x0) * 3 + c];
                    let e = src[(y1 * w + x1) * 3 + c];
                    let top = a + (b - a) * fx;
                    let bottom = d + (e - d) * fx;
                    row[x * 3 + c] = top + (bottom - top) * fy;
                }
            }
        });
    }
}

/// Bilinear sample of a single plane, clamped at the frame edge.
fn bilinear1(plane: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let sx = x.clamp(0.0, (w - 1) as f32);
    let sy = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
    let top = plane[y0 * w + x0] + (plane[y0 * w + x1] - plane[y0 * w + x0]) * fx;
    let bottom = plane[y1 * w + x0] + (plane[y1 * w + x1] - plane[y1 * w + x0]) * fx;
    top + (bottom - top) * fy
}

/// Bilinear sample of an interleaved RGB buffer, clamped at the frame edge.
fn bilinear3(rgb: &[f32], w: usize, h: usize, x: f32, y: f32) -> [f32; 3] {
    let sx = x.clamp(0.0, (w - 1) as f32);
    let sy = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
    std::array::from_fn(|c| {
        let a = rgb[(y0 * w + x0) * 3 + c];
        let b = rgb[(y0 * w + x1) * 3 + c];
        let d = rgb[(y1 * w + x0) * 3 + c];
        let e = rgb[(y1 * w + x1) * 3 + c];
        let top = a + (b - a) * fx;
        let bottom = d + (e - d) * fx;
        top + (bottom - top) * fy
    })
}

/// How far each pixel is from the subject, in pixels (zero on it): the pixels whose
/// background confidence is below one half.
fn distance_from_subject(background: &[f32], w: usize, h: usize) -> Vec<f32> {
    let subject: Vec<bool> = background.iter().map(|b| *b < 0.5).collect();
    crate::fill::distance_to(&subject, w, h)
}

/// The photograph's background with the subject taken out and the hole filled from around it.
///
/// Confident background pixels are kept exactly; the subject and its soft edge are filled by
/// push-pull from the background that meets them, which continues its colours and broad lines
/// across a strip revealed by a reshape.
fn background_plate(src: &[f32], background: &[f32], w: usize, h: usize) -> Vec<f32> {
    // Certain background only: a soft hair or skin edge is filled, never copied.
    let weight: Vec<f32> = background
        .iter()
        .map(|b| {
            let t = ((b - 0.8) / 0.18).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect();
    crate::fill::push_pull(src, 3, &weight, w, h)
}

/// A translation, bounded so it cannot fold: at most two fifths of the shorter radius.
fn moving(c: [f32; 2], r: [f32; 2], angle: f32, v: [f32; 2]) -> Warp {
    let cap = 0.4 * r[0].min(r[1]);
    let len = v[0].hypot(v[1]);
    let v = if len > cap && len > 0.0 {
        [v[0] * cap / len, v[1] * cap / len]
    } else {
        v
    };
    Warp::Move { c, r, angle, v }
}

/// A turn, bounded to about eleven degrees so the field cannot fold.
fn turning(c: [f32; 2], r: f32, angle: f32) -> Warp {
    Warp::Rotate {
        c,
        r,
        angle: angle.clamp(-0.2, 0.2),
    }
}

/// A scale, bounded to `-0.5..=0.5` on each axis.
fn scaling(c: [f32; 2], r: [f32; 2], angle: f32, a: [f32; 2]) -> Warp {
    Warp::Scale {
        c,
        r,
        angle,
        a: a.map(|v| v.clamp(-0.5, 0.5)),
    }
}

// ---- faces ------------------------------------------------------------------------------

/// The analysis grid onto the buffer.
pub(crate) struct Scale {
    sx: f32,
    sy: f32,
}

/// One face in buffer pixels, in its own upright frame: `u` across, `v` down, in units of
/// the distance between the eyes.
pub(crate) struct Face {
    pub(crate) mid: [f32; 2],
    pub(crate) u: [f32; 2],
    pub(crate) v: [f32; 2],
    pub(crate) io: f32,
    pub(crate) roll: f32,
    pub(crate) left_eye: [f32; 2],
    pub(crate) right_eye: [f32; 2],
    /// Eye line to nose and to mouth, in interocular units.
    pub(crate) nose: f32,
    pub(crate) mouth: f32,
    /// Mouth width in interocular units.
    pub(crate) mouth_width: f32,
    /// Eye line to the bottom of the chin, in interocular units: measured with
    /// [`Face::measure_chin`], a fixed distance below the mouth until then.
    pub(crate) chin: f32,
}

impl Scale {
    pub(crate) fn of(map: &PortraitMap, w: usize, h: usize) -> Self {
        Self {
            sx: w as f32 / map.width.max(1) as f32,
            sy: h as f32 / map.height.max(1) as f32,
        }
    }

    fn point(&self, p: [f32; 2]) -> [f32; 2] {
        [p[0] * self.sx, p[1] * self.sy]
    }

    pub(crate) fn face(&self, g: &FaceGeometry) -> Face {
        let k = (self.sx + self.sy) * 0.5;
        let left_eye = self.point(g.left_eye);
        let right_eye = self.point(g.right_eye);
        let mid = [
            (left_eye[0] + right_eye[0]) * 0.5,
            (left_eye[1] + right_eye[1]) * 0.5,
        ];
        let io = (g.interocular() * k).max(2.0);
        let (s, c) = g.roll.sin_cos();
        let u = [c, s];
        let v = [-s, c];
        let along = |p: [f32; 2]| ((p[0] - mid[0]) * v[0] + (p[1] - mid[1]) * v[1]) / io;
        // The landmarks are measured or placed by a prior; keep them where a face can be.
        let nose = along(self.point(g.nose)).clamp(0.4, 1.0);
        let mouth = along(self.point(g.mouth)).clamp(nose + 0.2, 1.6);
        Face {
            mid,
            u,
            v,
            io,
            roll: g.roll,
            left_eye,
            right_eye,
            nose,
            mouth,
            mouth_width: (g.mouth_width * k / io).clamp(0.4, 1.4),
            chin: mouth + 0.8,
        }
    }
}

impl Face {
    /// The bottom of the chin, measured: down the face's centre line below the mouth, the
    /// strongest step from light to dark is the jaw's edge over the neck in its shadow. A face
    /// looking down has its chin much nearer its mouth than the average one, and a chin tool
    /// that assumed the average worked on the necklace. Keeps the prior when no step stands out.
    /// ADR-0150.
    pub(crate) fn measure_chin(mut self, rgb: &[f32], w: usize, h: usize) -> Self {
        const STEPS: usize = 48;
        let (lo, hi) = (self.mouth + 0.3, self.mouth + 1.05);
        let luma_at = |p: [f32; 2]| {
            let x = (p[0].max(0.0) as usize).min(w.saturating_sub(1));
            let y = (p[1].max(0.0) as usize).min(h.saturating_sub(1));
            let i = (y * w + x) * 3;
            rgb.get(i..i + 3).map_or(0.0, |p| luma([p[0], p[1], p[2]]))
        };
        let profile: Vec<f32> = (0..STEPS)
            .map(|k| {
                let v = lo + (hi - lo) * k as f32 / (STEPS - 1) as f32;
                [-0.2_f32, -0.1, 0.0, 0.1, 0.2]
                    .iter()
                    .map(|u| luma_at(self.at(*u, v)))
                    .sum::<f32>()
                    / 5.0
            })
            .collect();
        let smooth: Vec<f32> = (0..STEPS)
            .map(|k| {
                let a = profile[k.saturating_sub(1)];
                let c = profile[(k + 1).min(STEPS - 1)];
                (a + 2.0 * profile[k] + c) / 4.0
            })
            .collect();
        let mean = smooth.iter().sum::<f32>() / STEPS as f32;
        let mut best = (0.0_f32, None);
        for k in 2..STEPS - 2 {
            let step = smooth[k - 2] - smooth[k + 2];
            if step > best.0 {
                best = (step, Some(k));
            }
        }
        if let (step, Some(k)) = best {
            if step > 0.06 * mean {
                self.chin = lo + (hi - lo) * k as f32 / (STEPS - 1) as f32;
            }
        }
        self
    }

    /// A point `x` across and `y` down from the eye line, in interocular units.
    pub(crate) fn at(&self, x: f32, y: f32) -> [f32; 2] {
        [
            self.mid[0] + (self.u[0] * x + self.v[0] * y) * self.io,
            self.mid[1] + (self.u[1] * x + self.v[1] * y) * self.io,
        ]
    }

    fn across(&self, k: f32) -> [f32; 2] {
        [self.u[0] * k * self.io, self.u[1] * k * self.io]
    }

    fn down(&self, k: f32) -> [f32; 2] {
        [self.v[0] * k * self.io, self.v[1] * k * self.io]
    }

    fn r(&self, x: f32, y: f32) -> [f32; 2] {
        [x * self.io, y * self.io]
    }
}

// One block per slider, in the order the panel lists them; splitting it would scatter the list.
#[allow(clippy::too_many_lines)]
fn face_warps(
    field: &mut Field,
    scale: &Scale,
    map: &PortraitMap,
    g: &FaceGeometry,
    s: &FaceShape,
    rgb: &[f32],
) {
    let f = scale.face(g).measure_chin(rgb, field.w, field.h);
    let t = |v: f32| (v / 100.0).clamp(-1.0, 1.0);
    let m = f.mouth;
    // Whole head first, so the features move within the resized head.
    if s.head_size != 0.0 {
        field.add(scaling(
            f.at(0.0, m * 0.35),
            f.r(1.7, 2.1),
            f.roll,
            [t(s.head_size) * 0.16; 2],
        ));
    }
    for side in [-1.0_f32, 1.0] {
        // Inward is toward the midline: the opposite sign of the side.
        let inward = |k: f32| f.across(-side * k);
        if s.slim != 0.0 {
            field.add(moving(
                f.at(side * 1.05, m * 0.8),
                f.r(0.8, 1.05),
                f.roll,
                inward(t(s.slim) * 0.16),
            ));
        }
        if s.jaw != 0.0 {
            field.add(moving(
                f.at(side * 0.85, m + 0.4),
                f.r(0.6, 0.55),
                f.roll,
                inward(t(s.jaw) * 0.14),
            ));
        }
        if s.cheekbones != 0.0 {
            field.add(moving(
                f.at(side * 1.05, 0.45),
                f.r(0.55, 0.5),
                f.roll,
                inward(t(s.cheekbones) * 0.11),
            ));
        }
        let eye = if side < 0.0 { f.left_eye } else { f.right_eye };
        if s.eye_size != 0.0 {
            field.add(scaling(
                eye,
                f.r(0.55, 0.42),
                f.roll,
                [t(s.eye_size) * 0.32; 2],
            ));
        }
        if s.eye_distance != 0.0 {
            field.add(moving(
                eye,
                f.r(0.5, 0.45),
                f.roll,
                f.across(side * t(s.eye_distance) * 0.09),
            ));
        }
        let corner = f.at(side * f.mouth_width * 0.5, m);
        if s.mouth_width != 0.0 {
            field.add(moving(
                corner,
                f.r(0.38, 0.32),
                f.roll,
                f.across(side * t(s.mouth_width) * 0.1),
            ));
        }
        if s.smile != 0.0 {
            field.add(moving(
                corner,
                f.r(0.3, 0.3),
                f.roll,
                f.down(-t(s.smile) * 0.07),
            ));
        }
    }
    if s.chin != 0.0 {
        field.add(moving(
            f.at(0.0, m + 0.8),
            f.r(0.65, 0.6),
            f.roll,
            f.down(t(s.chin) * 0.17),
        ));
    }
    if s.forehead != 0.0 {
        field.add(moving(
            f.at(0.0, -1.25),
            f.r(1.25, 0.75),
            f.roll,
            f.down(-t(s.forehead) * 0.17),
        ));
    }
    if s.nose_width != 0.0 {
        field.add(scaling(
            f.at(0.0, f.nose),
            f.r(0.42, 0.45),
            f.roll,
            [-t(s.nose_width) * 0.3, 0.0],
        ));
    }
    if s.nose_length != 0.0 {
        field.add(moving(
            f.at(0.0, f.nose),
            f.r(0.4, 0.42),
            f.roll,
            f.down(t(s.nose_length) * 0.09),
        ));
    }
    if s.lips != 0.0 {
        field.add(scaling(
            f.at(0.0, m),
            f.r(f.mouth_width * 0.65, 0.34),
            f.roll,
            [t(s.lips) * 0.08, t(s.lips) * 0.3],
        ));
    }
    more_face_warps(field, &f, s);
    if s.eye_symmetry > 0.0 {
        eye_symmetry(field, map, g, &f, s.eye_symmetry / 100.0);
    }
}

/// The twenty ADR-0150 face sliders, in the order the panel lists them.
#[allow(clippy::too_many_lines)]
fn more_face_warps(field: &mut Field, f: &Face, s: &FaceShape) {
    let t = |v: f32| (v / 100.0).clamp(-1.0, 1.0);
    let m = f.mouth;
    for side in [-1.0_f32, 1.0] {
        let eye = if side < 0.0 { f.left_eye } else { f.right_eye };
        let outward = |k: f32| f.across(side * k);
        if s.eye_height != 0.0 {
            field.add(moving(
                eye,
                f.r(0.5, 0.42),
                f.roll,
                f.down(-t(s.eye_height) * 0.06),
            ));
        }
        if s.eye_tilt != 0.0 {
            // A clockwise turn lifts the image-left eye's outer corner and lowers the other's.
            field.add(turning(eye, 0.45 * f.io, -side * t(s.eye_tilt) * 0.12));
        }
        let eye_u = side * 0.5;
        if s.inner_corner != 0.0 {
            field.add(moving(
                f.at(eye_u - side * 0.22, 0.0),
                f.r(0.17, 0.13),
                f.roll,
                outward(-t(s.inner_corner) * 0.035),
            ));
        }
        if s.outer_corner != 0.0 {
            field.add(moving(
                f.at(eye_u + side * 0.24, 0.0),
                f.r(0.19, 0.14),
                f.roll,
                outward(t(s.outer_corner) * 0.04),
            ));
        }
        if s.nostrils != 0.0 {
            field.add(moving(
                f.at(side * 0.19, f.nose + 0.02),
                f.r(0.14, 0.13),
                f.roll,
                outward(-t(s.nostrils) * 0.035),
            ));
        }
        if s.cheek_apples != 0.0 {
            // Fuller apples show at the face's own outline, so the cheek's edge moves out as
            // well as the apple swelling.
            field.add(scaling(
                f.at(side * 0.6, 0.62),
                f.r(0.36, 0.3),
                f.roll,
                [t(s.cheek_apples) * 0.2; 2],
            ));
            field.add(moving(
                f.at(side * 1.0, 0.75),
                f.r(0.38, 0.38),
                f.roll,
                outward(t(s.cheek_apples) * 0.05),
            ));
        }
        if s.brow_height != 0.0 {
            field.add(moving(
                f.at(side * 0.5, -0.36),
                f.r(0.5, 0.26),
                f.roll,
                f.down(-t(s.brow_height) * 0.06),
            ));
        }
        if s.brow_tilt != 0.0 {
            field.add(moving(
                f.at(side * 0.78, -0.34),
                f.r(0.24, 0.2),
                f.roll,
                f.down(-t(s.brow_tilt) * 0.045),
            ));
        }
        if s.brow_thickness != 0.0 {
            field.add(scaling(
                f.at(side * 0.5, -0.36),
                f.r(0.46, 0.15),
                f.roll,
                [0.0, t(s.brow_thickness) * 0.35],
            ));
        }
        if s.temples != 0.0 {
            field.add(moving(
                f.at(side * 1.05, -0.45),
                f.r(0.38, 0.5),
                f.roll,
                outward(t(s.temples) * 0.06),
            ));
        }
    }
    if s.nose_bridge != 0.0 {
        field.add(scaling(
            f.at(0.0, f.nose * 0.5),
            f.r(0.27, f.nose * 0.6),
            f.roll,
            [-t(s.nose_bridge) * 0.45, 0.0],
        ));
    }
    if s.nose_tip != 0.0 {
        field.add(scaling(
            f.at(0.0, f.nose),
            f.r(0.2, 0.18),
            f.roll,
            [-t(s.nose_tip) * 0.25; 2],
        ));
    }
    if s.nose_lift != 0.0 {
        field.add(moving(
            f.at(0.0, f.nose + 0.05),
            f.r(0.24, 0.22),
            f.roll,
            f.down(-t(s.nose_lift) * 0.05),
        ));
    }
    if s.upper_lip != 0.0 {
        field.add(scaling(
            f.at(0.0, m - 0.07),
            f.r(f.mouth_width * 0.62, 0.2),
            f.roll,
            [0.0, t(s.upper_lip) * 0.5],
        ));
    }
    if s.lower_lip != 0.0 {
        field.add(scaling(
            f.at(0.0, m + 0.1),
            f.r(f.mouth_width * 0.62, 0.22),
            f.roll,
            [0.0, t(s.lower_lip) * 0.5],
        ));
    }
    if s.mouth_height != 0.0 {
        field.add(moving(
            f.at(0.0, m),
            f.r(f.mouth_width * 0.75, 0.36),
            f.roll,
            f.down(-t(s.mouth_height) * 0.06),
        ));
    }
    if s.face_length != 0.0 {
        field.add(scaling(
            f.at(0.0, (f.nose + m) * 0.5 + 0.3),
            f.r(1.1, 1.0),
            f.roll,
            [0.0, t(s.face_length) * 0.12],
        ));
    }
    if s.chin_width != 0.0 {
        field.add(scaling(
            f.at(0.0, f.chin - 0.12),
            f.r(0.5, 0.35),
            f.roll,
            [-t(s.chin_width) * 0.3, 0.0],
        ));
    }
    if s.hair_volume != 0.0 {
        field.add(scaling(
            f.at(0.0, -1.7),
            f.r(1.6, 1.0),
            f.roll,
            [t(s.hair_volume) * 0.12, t(s.hair_volume) * 0.15],
        ));
    }
}

/// How open each eye is: the eye region's weight summed over a box around each eye centre, on
/// the analysis grid.
fn eye_openings(map: &PortraitMap, g: &FaceGeometry) -> Option<[f32; 2]> {
    let plane = map.plane(Region::Eyes)?;
    let io = g.interocular().max(2.0);
    let (hw, hh) = (0.28 * io, 0.2 * io);
    let open = |c: [f32; 2]| {
        let mut sum = 0.0_f32;
        let (x0, x1) = ((c[0] - hw).floor() as i64, (c[0] + hw).ceil() as i64);
        let (y0, y1) = ((c[1] - hh).floor() as i64, (c[1] + hh).ceil() as i64);
        for y in y0..=y1 {
            for x in x0..=x1 {
                sum += plane.at(x, y);
            }
        }
        sum
    };
    let both = [open(g.left_eye), open(g.right_eye)];
    (both[0] > 1.0 && both[1] > 1.0).then_some(both)
}

/// The smaller eye opened toward the larger: mostly in height, a little in width.
fn eye_symmetry(field: &mut Field, map: &PortraitMap, g: &FaceGeometry, f: &Face, k: f32) {
    let Some([left, right]) = eye_openings(map, g) else {
        return;
    };
    let ratio = left.min(right) / left.max(right);
    if ratio >= 0.97 {
        return;
    }
    let eye = if left < right {
        f.left_eye
    } else {
        f.right_eye
    };
    let open = ((1.0 / ratio - 1.0) * k * 0.8).min(0.25);
    field.add(scaling(eye, f.r(0.5, 0.4), f.roll, [open * 0.25, open]));
}

/// The ADR-0150 body sliders that belong to each face: the neck, the double chin, the head's
/// tilt.
fn head_and_neck_warps(
    field: &mut Field,
    scale: &Scale,
    g: &FaceGeometry,
    s: &BodyShape,
    rgb: &[f32],
) {
    let f = scale.face(g).measure_chin(rgb, field.w, field.h);
    let t = |v: f32| (v / 100.0).clamp(-1.0, 1.0);
    let m = f.mouth;
    if s.neck_width != 0.0 {
        field.add(scaling(
            f.at(0.0, f.chin + 0.45),
            f.r(0.75, 0.5),
            f.roll,
            [-t(s.neck_width) * 0.25, 0.0],
        ));
    }
    if s.double_chin > 0.0 {
        let k = (s.double_chin / 100.0).clamp(0.0, 1.0);
        field.add(scaling(
            f.at(0.0, f.chin + 0.12),
            f.r(0.8, 0.35),
            f.roll,
            [0.0, -k * 0.3],
        ));
        field.add(moving(
            f.at(0.0, f.chin + 0.08),
            f.r(0.7, 0.28),
            f.roll,
            f.down(-k * 0.05),
        ));
    }
    if s.head_level > 0.0 && g.roll.abs() > 0.005 {
        let k = (s.head_level / 100.0).clamp(0.0, 1.0);
        // The head turned about its own middle, back toward a level eye line.
        field.add(turning(f.at(0.0, m * 0.45), 2.3 * f.io, -g.roll * k));
    }
}

// ---- the body ---------------------------------------------------------------------------

/// The person's bounds in buffer pixels, and their width at a given height.
struct Person {
    plane: Vec<f32>,
    w: usize,
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
}

impl Person {
    /// The person under a face: columns more than a few face widths either side are someone
    /// else in a group photograph. ADR-0150; the whole body region when there is no face.
    fn find(map: &PortraitMap, w: usize, h: usize, face: Option<(f32, f32)>) -> Option<Self> {
        let mut plane = region(map, Region::Body, w, h);
        if let Some((cx, size)) = face {
            let (lo, hi) = (cx - size * 2.4, cx + size * 2.4);
            for (i, v) in plane.iter_mut().enumerate() {
                let x = (i % w) as f32 + 0.5;
                if x < lo || x > hi {
                    *v = 0.0;
                }
            }
        }
        let (mut top, mut bottom, mut left, mut right) = (h, 0, w, 0);
        for y in 0..h {
            for x in 0..w {
                if plane[y * w + x] > 0.5 {
                    top = top.min(y);
                    bottom = bottom.max(y);
                    left = left.min(x);
                    right = right.max(x);
                }
            }
        }
        (top < bottom && left < right).then_some(Self {
            plane,
            w,
            top,
            bottom,
            left,
            right,
        })
    }

    fn height(&self) -> f32 {
        (self.bottom - self.top) as f32
    }

    fn y_at(&self, share: f32) -> f32 {
        self.top as f32 + self.height() * share
    }

    /// The left and right edge of the person on one row.
    fn extent(&self, y: f32) -> Option<(f32, f32)> {
        let y = (y.max(0.0) as usize).min(self.bottom);
        let row = &self.plane[y * self.w..(y + 1) * self.w];
        let left = row.iter().position(|v| *v > 0.5)?;
        let right = row.iter().rposition(|v| *v > 0.5)?;
        (right > left).then_some((left as f32, right as f32))
    }
}

fn body_warps(
    field: &mut Field,
    scale: &Scale,
    map: &PortraitMap,
    s: &BodyShape,
    main: Option<&FaceGeometry>,
) {
    let main = main.or_else(|| map.faces.first());
    // A body reshape needs a person, and the evidence of one is a face: found by the parse, or
    // drawn by the photographer where it missed one. The parse's body plane and the learned
    // subject selection both answer "what stands out", which on a still life or a pod of dolphins
    // is not a person - a slim or a height slider would warp the frame.
    if main.is_none() {
        return;
    }
    let around = main.map(|g| {
        let f = scale.face(g);
        (f.mid[0], g.size * (scale.sx + scale.sy) * 0.5)
    });
    let Some(person) = Person::find(map, field.w, field.h, around) else {
        return;
    };
    let t = |v: f32| (v / 100.0).clamp(-1.0, 1.0);
    let height = person.height();
    // Where the shoulders are: below the main face when there is one.
    let face = main.map(|g| scale.face(g));
    let shoulders_y = face
        .as_ref()
        .map_or_else(|| person.y_at(0.2), |f| f.at(0.0, f.mouth + 1.6)[1])
        .clamp(person.top as f32, person.bottom as f32);
    let below = |share: f32| shoulders_y + (person.bottom as f32 - shoulders_y) * share;
    let mut sides = |y: f32, slider: f32, reach: f32, depth: f32, outward: bool| {
        if slider == 0.0 {
            return;
        }
        let Some((left, right)) = person.extent(y) else {
            return;
        };
        let width = right - left;
        let r = [width * reach, height * 0.09];
        let push = width * depth * t(slider);
        let sign = if outward { -1.0 } else { 1.0 };
        field.add(moving([left, y], r, 0.0, [push * sign, 0.0]));
        field.add(moving([right, y], r, 0.0, [-push * sign, 0.0]));
    };
    sides(shoulders_y, s.shoulders, 0.22, 0.05, true);
    sides(below(0.22), s.arms, 0.18, 0.05, false);
    sides(below(0.38), s.waist, 0.3, 0.07, false);
    sides(below(0.52), s.hips, 0.3, 0.06, false);
    sides(below(0.68), s.thighs, 0.25, 0.06, false);
    sides(below(0.86), s.calves, 0.18, 0.05, false);
    if s.slim != 0.0 {
        let cx = (person.left + person.right) as f32 * 0.5;
        let cy = below(0.4);
        let rx = (person.right - person.left) as f32 * 0.75;
        field.add(scaling(
            [cx, cy],
            [rx, height * 0.75],
            0.0,
            [-t(s.slim) * 0.12, 0.0],
        ));
    }
    if s.neck != 0.0 {
        if let Some(f) = &face {
            // The head rises; the falloff's lower edge sits on the shoulders, so the neck stretches.
            field.add(moving(
                f.at(0.0, f.mouth * 0.4),
                f.r(1.8, 2.2),
                f.roll,
                f.down(-t(s.neck) * 0.22),
            ));
        }
    }
    more_body_warps(field, &person, height, shoulders_y, &below, s);
    if s.legs != 0.0 {
        let hip = below(0.55);
        let margin = (person.right - person.left) as f32 * 0.6;
        field.stretch_below(
            hip,
            height * 0.08,
            t(s.legs) * 0.14,
            (
                person.left as f32 - margin,
                person.right as f32 + margin,
                margin.max(1.0),
            ),
        );
    }
}

/// The ADR-0150 body sliders that move the silhouette's own lines: the stomach, the hips, the
/// shoulder line, the torso and the whole height.
fn more_body_warps(
    field: &mut Field,
    person: &Person,
    height: f32,
    shoulders_y: f32,
    below: &dyn Fn(f32) -> f32,
    s: &BodyShape,
) {
    let t = |v: f32| (v / 100.0).clamp(-1.0, 1.0);
    if s.belly != 0.0 {
        belly(field, person, height, below, t(s.belly));
    }
    if s.hip_lift != 0.0 {
        if let Some((left, right)) = person.extent(below(0.6)) {
            let width = right - left;
            let cx = (left + right) * 0.5;
            for dx in [-0.22, 0.22] {
                field.add(moving(
                    [cx + width * dx, below(0.6)],
                    [width * 0.28, height * 0.08],
                    0.0,
                    [0.0, -height * 0.025 * t(s.hip_lift)],
                ));
            }
        }
    }
    if s.shoulder_slope != 0.0 {
        if let Some((left, right)) = person.extent(shoulders_y) {
            let width = right - left;
            for x in [left, right] {
                field.add(moving(
                    [x, shoulders_y],
                    [width * 0.2, height * 0.06],
                    0.0,
                    [0.0, -height * 0.02 * t(s.shoulder_slope)],
                ));
            }
        }
    }
    let margin = (person.right - person.left) as f32 * 0.6;
    let x_range = (
        person.left as f32 - margin,
        person.right as f32 + margin,
        margin.max(1.0),
    );
    if s.torso != 0.0 {
        field.stretch_band(shoulders_y, below(0.45), t(s.torso) * 0.1, x_range);
    }
    if s.height != 0.0 {
        field.stretch_below(shoulders_y, height * 0.05, t(s.height) * 0.08, x_range);
    }
}

/// A stomach flattened on the side the silhouette shows it: each side is pushed in by how far
/// it bulges past the line from the ribs to the hips, and by a little more.
fn belly(field: &mut Field, person: &Person, height: f32, below: &dyn Fn(f32) -> f32, k: f32) {
    let (Some(ribs), Some(waist), Some(hips)) = (
        person.extent(below(0.28)),
        person.extent(below(0.42)),
        person.extent(below(0.55)),
    ) else {
        return;
    };
    let width = waist.1 - waist.0;
    let bulge_left = ((ribs.0 + hips.0) * 0.5 - waist.0).clamp(0.0, width * 0.08);
    let bulge_right = (waist.1 - (ribs.1 + hips.1) * 0.5).clamp(0.0, width * 0.08);
    let r = [width * 0.22, height * 0.1];
    let y = below(0.42);
    field.add(moving(
        [waist.0, y],
        r,
        0.0,
        [(bulge_left + width * 0.01) * k, 0.0],
    ));
    field.add(moving(
        [waist.1, y],
        r,
        0.0,
        [-(bulge_right + width * 0.01) * k, 0.0],
    ));
}

// ---- liquify ----------------------------------------------------------------------------

fn liquify(field: &mut Field, stroke: &LiquifyStroke) {
    if stroke.strength <= 0.0 || stroke.points.is_empty() {
        return;
    }
    let (w, h) = (field.w as f32, field.h as f32);
    let radius = stroke.radius * w.min(h);
    let to_px = |p: [f32; 2]| [p[0] * w, p[1] * h];
    let k = stroke.strength.clamp(0.0, 1.0);
    match stroke.mode {
        LiquifyMode::Push => {
            // Each segment pushes the pixels under the brush along itself.
            for pair in stroke.points.windows(2) {
                let (a, b) = (to_px(pair[0]), to_px(pair[1]));
                let v = [(b[0] - a[0]) * k, (b[1] - a[1]) * k];
                if v[0].hypot(v[1]) > 0.01 {
                    field.add(moving(a, [radius; 2], 0.0, v));
                }
            }
        }
        LiquifyMode::Bloat | LiquifyMode::Pinch => {
            let sign = if stroke.mode == LiquifyMode::Bloat {
                1.0
            } else {
                -1.0
            };
            // Dabs at most a third of a radius apart, so a drag is one continuous change.
            let mut last: Option<[f32; 2]> = None;
            for p in &stroke.points {
                let p = to_px(*p);
                if last.is_some_and(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < radius / 3.0) {
                    continue;
                }
                last = Some(p);
                field.add(scaling(p, [radius; 2], 0.0, [sign * k * 0.25; 2]));
            }
        }
        LiquifyMode::Restore => {
            for p in &stroke.points {
                field.add(Warp::Restore {
                    c: to_px(*p),
                    r: radius,
                    k: k * 0.5,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::studio_finish::StudioFinish;

    #[test]
    fn a_picked_white_lands_on_white_through_the_output_transform() {
        let c = picked_colour([1.0, 1.0, 1.0]);
        let out = crate::output::map_one(
            c,
            crate::output::working_to_output(crate::contract::render::OutputColour::Srgb),
            crate::contract::render::OutputColour::Srgb,
        );
        for v in out {
            assert!(v > 0.99, "{out:?}");
        }
        let grey = picked_colour([0.5, 0.5, 0.5]);
        let out = crate::output::map_one(
            grey,
            crate::output::working_to_output(crate::contract::render::OutputColour::Srgb),
            crate::contract::render::OutputColour::Srgb,
        );
        for v in out {
            assert!((v - 0.5).abs() < 0.02, "{out:?}");
        }
    }

    fn ramp(w: usize, h: usize) -> Vec<f32> {
        (0..w * h)
            .flat_map(|i| {
                let v = (i % w) as f32 / w as f32;
                [v, v, v]
            })
            .collect()
    }

    #[test]
    fn an_empty_field_changes_nothing_and_a_restore_undoes_a_push() {
        let (w, h) = (64, 48);
        let mut field = Field::new(w, h);
        assert!(!field.touched);
        let stroke = LiquifyStroke {
            mode: LiquifyMode::Push,
            radius: 0.2,
            strength: 1.0,
            points: vec![[0.4, 0.5], [0.5, 0.5]],
        };
        liquify(&mut field, &stroke);
        assert!(field.touched);
        let moved = field.dx.iter().map(|v| v.abs()).fold(0.0, f32::max);
        assert!(moved > 1.0);
        for _ in 0..30 {
            liquify(
                &mut field,
                &LiquifyStroke {
                    mode: LiquifyMode::Restore,
                    radius: 0.4,
                    strength: 1.0,
                    points: vec![[0.4, 0.5]],
                },
            );
        }
        let left = field.dx.iter().map(|v| v.abs()).fold(0.0, f32::max);
        assert!(left < moved * 0.05, "{left} of {moved}");
    }

    #[test]
    fn a_push_moves_content_along_the_stroke() {
        let (w, h) = (100, 60);
        let src = ramp(w, h);
        let mut field = Field::new(w, h);
        liquify(
            &mut field,
            &LiquifyStroke {
                mode: LiquifyMode::Push,
                radius: 0.3,
                strength: 1.0,
                points: vec![[0.5, 0.5], [0.55, 0.5]],
            },
        );
        let mut out = src.clone();
        field.resample(&src, &mut out);
        // Pushed right: the pixel at the centre now shows something from its left.
        let i = (30 * w + 50) * 3;
        assert!(out[i] < src[i] - 0.02, "{} vs {}", out[i], src[i]);
        // Far from the brush nothing moves.
        let j = (2 * w + 2) * 3;
        assert!((out[j] - src[j]).abs() < 1e-6);
    }

    /// A grey frame with bright vertical background lines and a darker subject column.
    fn lined(w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
        let mut rgb = vec![0.3; w * h * 3];
        let mut background = vec![1.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if (30..50).contains(&x) {
                    background[i] = 0.0;
                    rgb[i * 3..i * 3 + 3].copy_from_slice(&[0.6, 0.4, 0.3]);
                } else if x % 10 == 1 {
                    rgb[i * 3..i * 3 + 3].copy_from_slice(&[0.9, 0.9, 0.9]);
                }
            }
        }
        (rgb, background)
    }

    #[test]
    #[allow(clippy::float_cmp)] // Background and subject preservation must be exact.
    fn background_repair_restores_lines_preserves_subject_and_respects_strength() {
        let (w, h) = (80, 60);
        let (source, background) = lined(w, h);
        let mut field = Field::new(w, h);
        field.ensure();
        field.touched = true;
        field.dx.fill(3.0);
        let mut warped = source.clone();
        field.resample(&source, &mut warped);
        let mut repaired = source.clone();
        field.resample_layered(&source, &background, 1.0, &mut repaired);
        // Background beyond the transition is exactly where it was: lines are not moved.
        for x in [1, 5, 11, 71] {
            let i = (20 * w + x) * 3;
            assert_eq!(repaired[i], source[i], "x = {x}");
        }
        assert_ne!(warped[(20 * w + 11) * 3], source[(20 * w + 11) * 3]);
        // The subject keeps its displacement exactly.
        let i = (20 * w + 38) * 3;
        assert_eq!(repaired[i], warped[i]);
        // Beside the subject the background follows it and relaxes smoothly: each line beyond
        // the old edge appears once along the row, never doubled.
        let row: Vec<f32> = (47..74).map(|x| repaired[(20 * w + x) * 3]).collect();
        let peaks = row
            .windows(3)
            .filter(|p| p[1] > 0.6 && p[1] >= p[0] && p[1] >= p[2])
            .count();
        assert_eq!(peaks, 3, "{row:?}");
        // Half strength moves the far background half as far.
        let mut half = source.clone();
        field.resample_layered(&source, &background, 0.5, &mut half);
        let i = (20 * w + 10) * 3;
        let expected = 0.3 + 0.5 * (0.9 - 0.3);
        assert!(
            (half[i] - expected).abs() < 1e-5,
            "{} vs {expected}",
            half[i]
        );
    }

    #[test]
    fn the_background_layer_never_folds_beside_the_subject() {
        let (w, h) = (120, 40);
        let (source, background) = lined(w, h);
        let mut field = Field::new(w, h);
        field.ensure();
        field.touched = true;
        field.dx.fill(-9.0);
        let distance = distance_from_subject(&background, w, h);
        let reach = (0.03 * w.min(h) as f32).max(2.5 * 9.0);
        let xs: Vec<f32> = (0..w)
            .map(|x| {
                let t = (distance[20 * w + x] / reach).clamp(0.0, 1.0);
                x as f32 - 9.0 * (1.0 - t * t * (3.0 - 2.0 * t))
            })
            .collect();
        assert!(xs.windows(2).all(|p| p[1] > p[0]), "{xs:?}");
        let mut out = source.clone();
        field.resample_layered(&source, &background, 1.0, &mut out);
        assert!(out.iter().all(|v| v.is_finite()));
    }

    /// Folds along rows of the plain warp and of the background layer at full repair.
    fn layer_folds(field: &Field, background: &[f32]) -> (usize, usize) {
        let (w, h) = (field.w, field.h);
        let largest = field
            .dx
            .iter()
            .zip(&field.dy)
            .map(|(x, y)| x.hypot(*y))
            .fold(0.0_f32, f32::max);
        let reach = (0.03 * w.min(h) as f32).max(2.5 * largest).max(2.0);
        let relax = field.layer_relax(background, 1.0, reach);
        let (mut plain, mut layer) = (0, 0);
        for y in 0..h {
            for x in 1..w {
                let (i, j) = (y * w + x, y * w + x - 1);
                if 1.0 + field.dx[i] - field.dx[j] <= 0.0 {
                    plain += 1;
                }
                let keep = |k: usize| 1.0 - relax[k];
                if 1.0 + field.dx[i] * keep(i) - field.dx[j] * keep(j) <= 0.0 {
                    layer += 1;
                }
            }
        }
        (plain, layer)
    }

    #[test]
    fn the_background_layer_does_not_fold_under_brush_moves() {
        let (w, h) = (240, 160);
        let background: Vec<f32> = (0..w * h)
            .map(|i| {
                if (100..140).contains(&(i % w)) {
                    0.0
                } else {
                    1.0
                }
            })
            .collect();
        // One move at its cap, across the subject's edge, either way: the plain warp never folds
        // and neither may the layer.
        for v in [400.0, -400.0] {
            let mut field = Field::new(w, h);
            field.add(moving([100.0, 80.0], [60.0, 60.0], 0.0, [v, 0.0]));
            assert_eq!(layer_folds(&field, &background), (0, 0), "move {v}");
        }
        // Stacked moves can fold the plain warp; repair must not add folds of its own.
        let mut field = Field::new(w, h);
        for (cx, v) in [(100.0, 400.0), (140.0, -400.0), (120.0, 400.0)] {
            field.add(moving([cx, 80.0], [60.0, 60.0], 0.0, [v, 0.0]));
        }
        let (plain, layer) = layer_folds(&field, &background);
        assert!(layer <= plain, "layer {layer} > plain {plain}");
    }

    #[test]
    #[allow(clippy::float_cmp)] // Untouched pixels must be bit-identical.
    fn background_repair_leaves_pixels_the_field_does_not_move() {
        let (w, h) = (80, 60);
        let (source, mut background) = lined(w, h);
        // A soft, uncertain edge that layering alone would repaint.
        for y in 0..h {
            background[y * w + 29] = 0.5;
        }
        let mut field = Field::new(w, h);
        field.add(moving([60.0, 50.0], [6.0, 6.0], 0.0, [2.0, 0.0]));
        let mut out = source.clone();
        field.resample_layered(&source, &background, 1.0, &mut out);
        for y in 0..40 {
            for x in 0..w {
                let i = (y * w + x) * 3;
                assert_eq!(out[i..i + 3], source[i..i + 3], "({x}, {y})");
            }
        }
    }

    #[test]
    fn the_background_plate_keeps_background_and_fills_the_subject_smoothly() {
        let (w, h) = (64, 40);
        let mut rgb = vec![0.0; w * h * 3];
        let mut background = vec![1.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                // A horizontal ramp the fill should continue across the hole.
                let v = x as f32 / w as f32;
                rgb[i * 3..i * 3 + 3].copy_from_slice(&[v, v, v]);
                if (20..44).contains(&x) {
                    background[i] = 0.0;
                    rgb[i * 3..i * 3 + 3].copy_from_slice(&[1.0, 0.0, 0.0]);
                }
            }
        }
        let plate = background_plate(&rgb, &background, w, h);
        assert!(plate.iter().all(|v| v.is_finite()));
        for x in [0, 10, 19, 44, 63] {
            let i = (20 * w + x) * 3;
            assert!((plate[i] - rgb[i]).abs() < 1e-6, "kept background at {x}");
        }
        // No subject colour survives in the fill, and it sits between the two sides.
        for x in 20..44 {
            let i = (20 * w + x) * 3;
            assert!((plate[i] - plate[i + 1]).abs() < 1e-4, "red leaked at {x}");
            assert!((0.25..0.75).contains(&plate[i]), "fill {} at {x}", plate[i]);
        }
    }

    #[test]
    fn the_field_never_folds_at_full_strength() {
        let (w, h) = (120, 120);
        let mut field = Field::new(w, h);
        for _ in 0..3 {
            field.add(moving([60.0, 60.0], [30.0, 30.0], 0.0, [500.0, 0.0]));
        }
        // The cap holds per primitive: the source x is monotone along a row through the centre.
        let row = 60 * w;
        let xs: Vec<f32> = (0..w).map(|x| x as f32 + field.dx[row + x]).collect();
        // Three stacked moves at the cap can at most flatten, the single-move guarantee is strict.
        let mut single = Field::new(w, h);
        single.add(moving([60.0, 60.0], [30.0, 30.0], 0.0, [500.0, 0.0]));
        let s: Vec<f32> = (0..w).map(|x| x as f32 + single.dx[row + x]).collect();
        assert!(s.windows(2).all(|p| p[1] > p[0]), "single move folded");
        assert!(xs.iter().all(|v| v.is_finite()));
    }

    /// No output pixel may sample further back along a row or a column than its neighbour
    /// does by a whole pixel: a fold would show the same input twice.
    fn folds(field: &Field) -> usize {
        if field.dx.is_empty() {
            return 0;
        }
        let (w, h) = (field.w, field.h);
        let mut n = 0;
        for y in 0..h {
            for x in 1..w {
                let i = y * w + x;
                if 1.0 + field.dx[i] - field.dx[i - 1] <= 0.0 {
                    n += 1;
                }
            }
        }
        for y in 1..h {
            for x in 0..w {
                let i = y * w + x;
                if 1.0 + field.dy[i] - field.dy[i - w] <= 0.0 {
                    n += 1;
                }
            }
        }
        n
    }

    fn largest_move(field: &Field) -> Option<(usize, usize, f32)> {
        let w = field.w;
        field
            .dx
            .iter()
            .zip(&field.dy)
            .enumerate()
            .map(|(i, (dx, dy))| (i % w, i / w, dx.hypot(*dy)))
            .max_by(|a, b| a.2.total_cmp(&b.2))
    }

    #[test]
    fn every_new_face_slider_moves_its_feature_and_never_folds() {
        use crate::studio_evoto::testing::{self as t, H, W};
        let map = t::map();
        let scale = Scale::of(&map, W, H);
        let g = t::face();
        let names = FaceShape::default().sliders().map(|(n, _)| n);
        // The twenty ADR-0150 sliders are the last twenty.
        for name in &names[13..] {
            for value in [100.0, -100.0] {
                if *name == "eyeSymmetry" {
                    continue;
                }
                let shape: FaceShape =
                    serde_json::from_value(serde_json::json!({ *name: value })).unwrap();
                let mut field = Field::new(W, H);
                face_warps(&mut field, &scale, &map, &g, &shape, &[]);
                assert!(field.touched, "{name} {value}");
                assert_eq!(folds(&field), 0, "{name} {value} folded");
                let (x, y, d) = largest_move(&field).unwrap();
                assert!(d > 0.3, "{name} {value} moved {d}");
                // Every face slider stays on and around the head.
                let dist = (x as f32 - 120.0).hypot(y as f32 - 110.0);
                assert!(dist < 110.0, "{name} {value} moved ({x}, {y})");
            }
        }
    }

    #[test]
    fn every_new_body_slider_moves_the_person_and_never_folds() {
        use crate::studio_evoto::testing::{self as t, H, W};
        let map = t::map();
        let scale = Scale::of(&map, W, H);
        let g = t::face();
        let names = BodyShape::default().sliders().map(|(n, _)| n);
        for name in &names[7..] {
            for value in [100.0, -100.0] {
                if (*name == "doubleChin" || *name == "headLevel") && value < 0.0 {
                    continue;
                }
                let shape: BodyShape =
                    serde_json::from_value(serde_json::json!({ *name: value })).unwrap();
                let mut field = Field::new(W, H);
                head_and_neck_warps(&mut field, &scale, &g, &shape, &[]);
                body_warps(&mut field, &scale, &map, &shape, None);
                if *name == "headLevel" {
                    // The painted face is level, so there is nothing to turn.
                    assert!(!field.touched, "{name}");
                    continue;
                }
                assert!(field.touched, "{name} {value}");
                assert_eq!(folds(&field), 0, "{name} {value} folded");
            }
        }
    }

    #[test]
    fn body_sliders_leave_a_frame_without_a_person_alone() {
        use crate::studio_evoto::testing::{self as t, H, W};
        let mut map = t::map();
        map.faces.clear();
        let scale = Scale::of(&map, W, H);
        let shape = BodyShape {
            slim: 100.0,
            height: 100.0,
            legs: 100.0,
            ..BodyShape::default()
        };
        let mut field = Field::new(W, H);
        body_warps(&mut field, &scale, &map, &shape, None);
        assert!(!field.touched, "warped a frame with no face in it");
        // The same frame with its face found (or drawn) is reshaped.
        let mut field = Field::new(W, H);
        body_warps(&mut field, &scale, &t::map(), &shape, None);
        assert!(field.touched);
    }

    #[test]
    fn a_tilted_head_is_turned_toward_level_and_by_no_more_than_the_tilt() {
        use crate::studio_evoto::testing::{self as t, H, W};
        let map = t::map();
        let scale = Scale::of(&map, W, H);
        let mut g = t::face();
        g.roll = 0.1;
        let shape = BodyShape {
            head_level: 100.0,
            ..BodyShape::default()
        };
        let mut field = Field::new(W, H);
        head_and_neck_warps(&mut field, &scale, &g, &shape, &[]);
        assert!(field.touched);
        assert_eq!(folds(&field), 0);
        // A point beside the head's middle is carried up or down - never further than the turn.
        let f = scale.face(&g);
        let c = f.at(0.0, f.mouth * 0.45);
        let probe = ((c[1]) as usize * W) + (c[0] as usize + 20);
        let moved = field.dy[probe].abs();
        assert!(moved > 0.5 && moved < 20.0 * 0.1 * 1.05, "{moved}");
    }

    #[test]
    fn eye_symmetry_opens_only_the_smaller_eye() {
        use crate::studio_evoto::testing::{self as t, H, W};
        let mut map = t::map();
        // The image-left eye half as open as the other.
        for (region, plane) in &mut map.planes {
            if *region == Region::Eyes {
                for y in 0..H {
                    for x in 0..W {
                        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                        let left =
                            ((fx - 100.0) / 11.0).powi(2) + ((fy - 100.0) / 2.5).powi(2) <= 1.0;
                        let right =
                            ((fx - 140.0) / 11.0).powi(2) + ((fy - 100.0) / 5.0).powi(2) <= 1.0;
                        plane.values[y * W + x] = if left || right { 1.0 } else { 0.0 };
                    }
                }
            }
        }
        let scale = Scale::of(&map, W, H);
        let shape = FaceShape {
            eye_symmetry: 100.0,
            ..FaceShape::default()
        };
        let mut field = Field::new(W, H);
        face_warps(&mut field, &scale, &map, &t::face(), &shape, &[]);
        assert!(field.touched);
        assert_eq!(folds(&field), 0);
        let near = |cx: usize| {
            (95..106)
                .flat_map(|y| (cx - 8..cx + 8).map(move |x| (x, y)))
                .map(|(x, y)| field.dy[y * W + x].abs())
                .fold(0.0_f32, f32::max)
        };
        assert!(near(100) > 0.5);
        assert!(near(140) < 1e-3);
        // Two eyes of a size are left as they are.
        let mut level = Field::new(W, H);
        face_warps(&mut level, &scale, &t::map(), &t::face(), &shape, &[]);
        assert!(!level.touched);
    }

    #[test]
    fn a_turn_moves_a_point_along_its_circle_by_at_most_its_angle() {
        let mut field = Field::new(100, 100);
        field.add(turning([50.0, 50.0], 40.0, 0.5));
        // Clamped to 0.2 radians: a point 10 px right of the centre looks back up the frame.
        let i = 50 * 100 + 60;
        let (dx, dy) = (field.dx[i], field.dy[i]);
        let angle = dy.atan2(10.0 + dx);
        assert!(angle < 0.0 && angle.abs() <= 0.2 + 1e-4, "{angle}");
        assert_eq!(folds(&field), 0);
    }

    #[test]
    fn a_body_stretch_lengthens_below_the_hip_only() {
        let (w, h) = (40, 100);
        let mut field = Field::new(w, h);
        field.stretch_below(50.0, 8.0, 0.1, (-100.0, 200.0, 1.0));
        assert!(field.dy[10 * w + 20].abs() < 1e-6);
        assert!(field.dy[90 * w + 20] < -1.0);
    }

    #[test]
    fn a_recipe_without_a_finish_is_left_alone() {
        let recipe = aura_recipe::fixtures::reference();
        let mut rgb = ramp(16, 8);
        let before = rgb.clone();
        apply(&mut rgb, 16, 8, &recipe, None);
        assert_eq!(rgb, before);
        assert!(!wants(&recipe));
    }

    #[test]
    fn a_liquify_only_finish_renders_without_a_parse() {
        let mut recipe = aura_recipe::fixtures::reference();
        let finish = StudioFinish {
            liquify: vec![LiquifyStroke {
                mode: LiquifyMode::Bloat,
                radius: 0.3,
                strength: 1.0,
                points: vec![[0.5, 0.5]],
            }],
            ..StudioFinish::default()
        };
        studio_finish::write(&mut recipe, &finish).unwrap();
        assert!(wants(&recipe));
        let mut rgb = ramp(64, 64);
        let before = rgb.clone();
        apply(&mut rgb, 64, 64, &recipe, None);
        assert_ne!(rgb, before);
    }
}
