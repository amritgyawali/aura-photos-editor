//! The Studio's local adjustments, rendered. ADR-0102.
//!
//! Each mask's components become one weight plane over the frame: AI selections from their
//! stored mattes, with edges re-derived from this frame by the same guided filter the retouch
//! mattes use; portrait regions from the cached parse; gradients and brush strokes from their
//! geometry. The mask's sliders are then applied at that weight - every parameter scaled rather
//! than the result blended, so two overlapping masks add, as in Lightroom.
//!
//! Runs after the retouch stack, so moving a mask's slider re-runs only this and what follows
//! it: the stack before it comes from its checkpoint (ADR-0098).
// Every plane is `w * h` long, built from validated dimensions; indices stay inside them.
#![allow(clippy::indexing_slicing)]

use aura_recipe::local_masks::{self, Component, LocalMask, Mode, Source};
use aura_recipe::retouch_tools::{LuminanceRange, Matte};
use aura_recipe::Recipe;
use rayon::prelude::*;
use std::collections::BTreeMap;

use crate::cpu::Frame;

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The working-space luminance the brush and the range read.
fn luma(p: &[f32]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}

/// One component's weight at every pixel, before inversion. `None` when it cannot be drawn
/// here - a matte that does not decode, a region with no parse.
fn component_plane(
    source: &Source,
    rgb: &[f32],
    w: usize,
    h: usize,
    mattes: &BTreeMap<String, Matte>,
    regions: &mut dyn FnMut(&str) -> Option<Vec<f32>>,
) -> Option<Vec<f32>> {
    match source {
        Source::Matte { matte, .. } => {
            let plane = crate::retouch_matte::MattePlane::render(mattes.get(matte)?, rgb, w, h)?;
            Some(
                (0..h)
                    .into_par_iter()
                    .flat_map_iter(|y| {
                        let plane = &plane;
                        (0..w).map(move |x| plane.at(x, y))
                    })
                    .collect(),
            )
        }
        Source::Region { region } => regions(region),
        Source::Linear { start, end } => {
            let (sx, sy) = (start[0] * w as f32, start[1] * h as f32);
            let (dx, dy) = (
                (end[0] - start[0]) * w as f32,
                (end[1] - start[1]) * h as f32,
            );
            let length2 = (dx * dx + dy * dy).max(1e-6);
            Some(
                (0..w * h)
                    .into_par_iter()
                    .map(|i| {
                        let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                        let t = ((x - sx) * dx + (y - sy) * dy) / length2;
                        1.0 - smoothstep(t)
                    })
                    .collect(),
            )
        }
        Source::Radial {
            centre,
            radii,
            angle,
            feather,
        } => {
            let (cx, cy) = (centre[0] * w as f32, centre[1] * h as f32);
            let (rx, ry) = (
                (radii[0] * w as f32).max(0.5),
                (radii[1] * h as f32).max(0.5),
            );
            let (sin, cos) = angle.to_radians().sin_cos();
            let inner = 1.0 - feather.clamp(0.0, 1.0);
            Some(
                (0..w * h)
                    .into_par_iter()
                    .map(|i| {
                        let (x, y) = ((i % w) as f32 + 0.5 - cx, (i / w) as f32 + 0.5 - cy);
                        // Into the ellipse's own axes.
                        let u = (x * cos + y * sin) / rx;
                        let v = (-x * sin + y * cos) / ry;
                        let d = u.hypot(v);
                        if d <= inner {
                            1.0
                        } else {
                            1.0 - smoothstep((d - inner) / (1.0 - inner).max(1e-3))
                        }
                    })
                    .collect(),
            )
        }
        Source::Brush { strokes, feather } => {
            let bounds = [0, 0, w, h];
            let mut plane = vec![0.0_f32; w * h];
            let mut stroke_plane = vec![0.0_f32; w * h];
            for stroke in strokes {
                stroke_plane.par_iter_mut().for_each(|v| *v = 0.0);
                crate::retouch_mask::paint_stroke(
                    &mut stroke_plane,
                    bounds,
                    stroke,
                    *feather,
                    w,
                    h,
                );
                plane
                    .par_iter_mut()
                    .zip(stroke_plane.par_iter())
                    .for_each(|(value, coverage)| {
                        let alpha = coverage * stroke.opacity;
                        *value = if stroke.erase {
                            *value * (1.0 - alpha)
                        } else {
                            value.max(alpha)
                        };
                    });
            }
            Some(plane)
        }
        Source::Luminance { range } => Some(
            rgb.par_chunks_exact(3)
                .map(|p| in_range(luma(p), range))
                .collect(),
        ),
    }
}

/// How much a luminance belongs to a range given in stops around 18 % grey.
fn in_range(luma: f32, range: &LuminanceRange) -> f32 {
    let ev = (luma.max(0.18 * 2.0_f32.powi(-16)) / 0.18)
        .log2()
        .clamp(-16.0, 16.0);
    let distance = (range.low - ev).max(ev - range.high).max(0.0);
    if range.softness <= 0.0 {
        return if distance <= 0.0 { 1.0 } else { 0.0 };
    }
    smoothstep(1.0 - distance / range.softness)
}

/// A mask's components combined in order, scaled by its amount.
fn weights(
    mask: &LocalMask,
    rgb: &[f32],
    w: usize,
    h: usize,
    mattes: &BTreeMap<String, Matte>,
    regions: &mut dyn FnMut(&str) -> Option<Vec<f32>>,
) -> Vec<f32> {
    let mut built: Option<Vec<f32>> = None;
    for Component {
        mode,
        invert,
        source,
    } in &mask.components
    {
        let Some(mut plane) = component_plane(source, rgb, w, h, mattes, regions) else {
            continue;
        };
        if *invert {
            plane
                .par_iter_mut()
                .for_each(|v| *v = 1.0 - v.clamp(0.0, 1.0));
        }
        built = Some(match (built, mode) {
            (None, Mode::Add) => plane,
            // Taking away from, or intersecting with, nothing is nothing.
            (None, Mode::Subtract | Mode::Intersect) => vec![0.0; w * h],
            (Some(mut so_far), mode) => {
                so_far
                    .par_iter_mut()
                    .zip(plane.par_iter())
                    .for_each(|(a, b)| {
                        let b = b.clamp(0.0, 1.0);
                        *a = match mode {
                            Mode::Add => a.max(b),
                            Mode::Subtract => *a * (1.0 - b),
                            Mode::Intersect => *a * b,
                        };
                    });
                so_far
            }
        });
    }
    let amount = mask.amount.clamp(0.0, 1.0);
    let mut out = built.unwrap_or_else(|| vec![0.0; w * h]);
    out.par_iter_mut()
        .for_each(|v| *v = v.clamp(0.0, 1.0) * amount);
    out
}

/// Local contrast at `radius` pixels, `amount` per unit weight, applied where `weight` is.
fn local_detail(rgb: &mut [f32], w: usize, h: usize, radius: usize, amount: f32, weight: &[f32]) {
    if amount == 0.0 || radius == 0 {
        return;
    }
    let log: Vec<f32> = crate::spatial::luma_plane(rgb, w, h)
        .iter()
        .map(|v| v.max(1e-5).ln())
        .collect();
    let low = crate::spatial::blur_plane(&log, w, h, radius);
    rgb.par_chunks_mut(3)
        .zip(weight.par_iter())
        .enumerate()
        .for_each(|(i, (pixel, k))| {
            if *k <= 1e-4 {
                return;
            }
            let detail = log[i] - low[i];
            let target = (low[i] + detail * (1.0 + amount * k)).exp();
            let out = crate::colour::set_luma([pixel[0], pixel[1], pixel[2]], target.max(0.0));
            pixel.copy_from_slice(&out);
        });
}

/// Apply one mask's sliders at its weight.
fn apply_one(rgb: &mut [f32], w: usize, h: usize, mask: &LocalMask, weight: &[f32]) {
    let p = mask.params;
    let point = aura_recipe::MaskParams {
        clarity: None,
        texture: None,
        tint: None,
        ..p
    };
    let tint = p.tint.map(|t| {
        let base = crate::colour::white_balance(crate::colour::REFERENCE_KELVIN, 0.0);
        let shifted = crate::colour::white_balance(crate::colour::REFERENCE_KELVIN, f32::from(t));
        [0, 1, 2].map(|c| {
            if base[c] > 1e-6 {
                shifted[c] / base[c]
            } else {
                1.0
            }
        })
    });
    rgb.par_chunks_mut(3)
        .zip(weight.par_iter())
        .for_each(|(pixel, k)| {
            if *k <= 1e-4 {
                return;
            }
            let mut value =
                crate::cpu::apply_mask_params([pixel[0], pixel[1], pixel[2]], &point, *k);
            if let Some(ratio) = tint {
                for c in 0..3 {
                    value[c] *= 1.0 + (ratio[c] - 1.0) * k;
                }
            }
            pixel.copy_from_slice(&value);
        });
    // The two detail sliders use the same radii as the global ones, in this buffer's pixels.
    if let Some(clarity) = p.clarity {
        local_detail(rgb, w, h, 16, f32::from(clarity) / 100.0, weight);
    }
    if let Some(texture) = p.texture {
        local_detail(rgb, w, h, 4, f32::from(texture) / 100.0, weight);
    }
}

/// A region plane for `slug`, from the frame's cached portrait parse.
fn region_source<'a>(
    frame: Option<&'a Frame>,
    recipe: &'a Recipe,
    w: usize,
    h: usize,
) -> impl FnMut(&str) -> Option<Vec<f32>> + 'a {
    let mut parsed = None;
    move |slug: &str| {
        let region = aura_portrait::Region::parse(slug)?;
        if parsed.is_none() {
            parsed = Some(
                frame.and_then(|f| crate::portrait::parse(f, &crate::portrait::hints(recipe))),
            );
        }
        let map = parsed.as_ref()?.as_ref()?;
        Some(crate::portrait::resolve(
            map,
            region,
            u32::try_from(w).ok()?,
            u32::try_from(h).ok()?,
        ))
    }
}

/// Apply every enabled mask in order. A no-op for a recipe without masks.
pub(crate) fn apply(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    frame: Option<&Frame>,
) {
    let (w, h) = (width as usize, height as usize);
    if rgb.len() != w * h * 3 || w == 0 || h == 0 {
        return;
    }
    let Ok(masks) = local_masks::read(recipe) else {
        return;
    };
    let active: Vec<&LocalMask> = masks
        .iter()
        .filter(|m| m.enabled && m.amount > 0.0 && !m.params.is_empty())
        .collect();
    if active.is_empty() {
        return;
    }
    let mattes = local_masks::read_mattes(recipe).unwrap_or_default();
    let mut regions = region_source(frame, recipe, w, h);
    for mask in active {
        let weight = weights(mask, rgb, w, h, &mattes, &mut regions);
        apply_one(rgb, w, h, mask, &weight);
    }
}

/// One mask's weight plane over a `width x height` buffer, for the overlay a photographer
/// paints against. `None` when the recipe has no such mask.
#[must_use]
pub fn coverage(
    rgb: &[f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    frame: Option<&Frame>,
    id: &str,
) -> Option<Vec<f32>> {
    let (w, h) = (width as usize, height as usize);
    if rgb.len() != w * h * 3 {
        return None;
    }
    let masks = local_masks::read(recipe).ok()?;
    let mask = masks.iter().find(|m| m.id == id)?;
    let mattes = local_masks::read_mattes(recipe).unwrap_or_default();
    let mut regions = region_source(frame, recipe, w, h);
    let full = LocalMask {
        amount: 1.0,
        ..mask.clone()
    };
    Some(weights(&full, rgb, w, h, &mattes, &mut regions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::local_masks::{Component, LocalMask, Mode, Source};
    use aura_recipe::MaskParams;

    fn mask(components: Vec<Component>, params: MaskParams) -> LocalMask {
        LocalMask {
            id: "m".into(),
            name: "m".into(),
            enabled: true,
            amount: 1.0,
            components,
            params,
        }
    }

    fn add(source: Source) -> Component {
        Component {
            mode: Mode::Add,
            invert: false,
            source,
        }
    }

    fn plane(m: &LocalMask, w: usize, h: usize) -> Vec<f32> {
        let rgb = vec![0.18_f32; w * h * 3];
        weights(m, &rgb, w, h, &BTreeMap::new(), &mut |_| None)
    }

    #[test]
    fn a_linear_gradient_runs_from_full_to_none() {
        let m = mask(
            vec![add(Source::Linear {
                start: [0.5, 0.0],
                end: [0.5, 1.0],
            })],
            MaskParams::default(),
        );
        let p = plane(&m, 10, 100);
        assert!(p[5] > 0.99, "{}", p[5]);
        assert!(p[99 * 10 + 5] < 0.01);
        assert!((p[50 * 10 + 5] - 0.5).abs() < 0.05);
    }

    #[test]
    fn a_radial_mask_is_full_inside_and_none_outside_and_inverts() {
        let radial = Source::Radial {
            centre: [0.5, 0.5],
            radii: [0.25, 0.25],
            angle: 0.0,
            feather: 0.5,
        };
        let p = plane(
            &mask(vec![add(radial.clone())], MaskParams::default()),
            64,
            64,
        );
        assert!(p[32 * 64 + 32] > 0.99);
        assert!(p[2 * 64 + 2] < 1e-3);
        let inverted = Component {
            invert: true,
            ..add(radial)
        };
        let q = plane(&mask(vec![inverted], MaskParams::default()), 64, 64);
        assert!(q[32 * 64 + 32] < 1e-3);
        assert!(q[2 * 64 + 2] > 0.99);
    }

    #[test]
    fn components_add_subtract_and_intersect_in_order() {
        let left = Source::Linear {
            start: [0.0, 0.5],
            end: [0.05, 0.5],
        };
        let top = Source::Linear {
            start: [0.5, 0.0],
            end: [0.5, 0.05],
        };
        let whole = Source::Radial {
            centre: [0.5, 0.5],
            radii: [2.0, 2.0],
            angle: 0.0,
            feather: 0.0,
        };
        let sub = mask(
            vec![
                add(whole.clone()),
                Component {
                    mode: Mode::Subtract,
                    invert: false,
                    source: left.clone(),
                },
            ],
            MaskParams::default(),
        );
        let p = plane(&sub, 100, 100);
        assert!(
            p[50 * 100] < 0.05 && p[50 * 100 + 90] > 0.99,
            "{} {}",
            p[50 * 100],
            p[50 * 100 + 90]
        );
        let both = mask(
            vec![
                add(Source::Linear {
                    start: [0.0, 0.5],
                    end: [1.0, 0.5],
                }),
                Component {
                    mode: Mode::Intersect,
                    invert: true,
                    source: top,
                },
            ],
            MaskParams::default(),
        );
        let q = plane(&both, 100, 100);
        assert!(q[0] < 0.05, "the top row is intersected away: {}", q[0]);
        assert!(q[50 * 100 + 1] > 0.9);
    }

    #[test]
    fn sliders_apply_only_where_the_mask_is() {
        let (w, h) = (32, 32);
        let mut rgb = vec![0.18_f32; w * h * 3];
        let m = mask(
            vec![add(Source::Linear {
                start: [0.0, 0.5],
                end: [0.5, 0.5],
            })],
            MaskParams {
                exposure: Some(1.0),
                tint: Some(20),
                ..MaskParams::default()
            },
        );
        let weight = plane(&m, w, h);
        apply_one(&mut rgb, w, h, &m, &weight);
        let left = &rgb[(16 * w) * 3..(16 * w) * 3 + 3];
        let right = &rgb[(16 * w + 30) * 3..(16 * w + 30) * 3 + 3];
        assert!(luma(left) > 0.3, "{left:?}");
        assert!(left[1] < left[0], "a magenta tint lowers green: {left:?}");
        assert!((luma(right) - 0.18).abs() < 1e-4, "{right:?}");
    }
}
