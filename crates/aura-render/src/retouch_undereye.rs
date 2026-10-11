//! Measured dark-circle correction: the shadow under an eye is lifted toward the same
//! person's cheek, and never past it.
//!
//! A retoucher corrects a dark circle on a curves layer masked to the shadow: the shadow is
//! brought most of the way up to the cheek beside it, its purple or brown cast is matched to
//! that cheek, and the pores and fine lines on top of it are left exactly as they were. A
//! flat lift toward the neighbourhood's own average does none of that - it raises the pores
//! with the shadow, keeps the cast, and anything already as light as the cheek is pushed past
//! it into a grey-white patch. This is the retoucher's version, measured:
//!
//! 1. **Reference.** The cheek is read at render time on a disk around the operation's
//!    `source`, from its skin at or near its lower tones, so pores, marks, glints and
//!    sheen on it do not move the target - and the target follows whatever exposure and
//!    evening ran before.
//! 2. **Shadow.** The selected skin is low-passed at the operation's `radius`, averaging skin
//!    only: lashes, brow hair and spectacle rims (far darker than the skin around them) and
//!    glints (far brighter) are left out of the average.
//! 3. **Correct.** Where that low band is darker than the cheek, every channel is scaled so
//!    the low band moves toward the cheek's brightness and - by `tone` - its colour. The scale
//!    is smooth, so texture keeps its relative contrast. Skin as light as the cheek is not
//!    touched, nothing ends brighter than the cheek, and lashes are never lifted. `amount` is
//!    the share of the shadow removed; a natural trace of it stays.
//!
//! Nothing is generated: the target is this photograph's own cheek. ADR-0094.
// Every plane covers one bounded rectangle and every index is derived from it.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, quantile, smoothstep, Rect, Weighted};
use aura_recipe::retouch_tools::Edit;

/// The cheek is read over a disk this many low-band radii across.
const REFERENCE_RADII: f32 = 3.0;
/// Relative to the cheek's lower tones (its 40th percentile): above them a pixel counts less
/// toward the reference, and from here on not at all.
const SHEEN: f32 = 1.12;
/// Fewer cheek samples than this is not a measurement.
const MIN_REFERENCE: usize = 12;
/// Relative to the skin around it: below the lower bound a pixel is a lash, a brow hair or a
/// spectacle rim and is neither averaged nor lifted; above the upper one it is skin.
const DARK_DETAIL: [f32; 2] = [0.5, 0.75];
/// Relative to the skin around it: above the lower bound a pixel is a glint and is left out
/// of the skin average.
const GLINT: [f32; 2] = [1.35, 1.7];
/// Relative to the low band it is corrected from: above the lower bound a pixel is lit skin
/// beside the shadow, or a glint, rather than the shadow's own texture, and is lifted less;
/// at the upper bound not at all.
const BESIDE: [f32; 2] = [1.15, 1.35];
/// How far any one channel may be scaled, down (a cast removed) or up (a shadow lifted).
/// When one channel would need more, the whole correction is shortened so every channel
/// fits: clamping a single channel instead would change the hue - a deep red-brown crease
/// whose green and blue cannot rise far enough turns salmon.
const GAIN_RANGE: [f32; 2] = [0.6, 2.0];
/// The darkness, as a share of the cheek, at which the cast is fully matched.
const FULL_CAST: f32 = 0.15;
/// A low band less than this much darker than the cheek is the skin's own variation, not a
/// shadow, and is left alone; the correction is at full strength from the upper bound on.
const ONSET: [f32; 2] = [0.015, 0.05];
/// The second, finer low band, as a share of `radius`. Each pixel is corrected from the
/// lighter of the two, so skin beside a shadow - which the wide band sees as partly shadow -
/// is never lifted past the cheek into a bright rim.
const FINE_SHARE: f32 = 0.34;

fn pixel(rgb: &[f32], w: usize, x: usize, y: usize) -> [f32; 3] {
    let i = (y * w + x) * 3;
    [rgb[i], rgb[i + 1], rgb[i + 2]]
}

/// How much a pixel counts as skin, given its luminance relative to the skin around it: lashes,
/// hair and rims below, glints above, count for nothing.
fn skin_weight(relative: f32) -> f32 {
    smoothstep(DARK_DETAIL[0], DARK_DETAIL[1], relative)
        * (1.0 - smoothstep(GLINT[0], GLINT[1], relative))
}

/// The cheek's own colour on a disk around `centre`, averaged the way the shadow is: skin
/// counted, marks, hair and glints left out - and only its darker half. A lit cheek often
/// carries a sheen, which is lighter and greyer than the skin under it; matching a shadow to
/// the sheen would leave an ashy, whitish patch under the eye, most visibly on dark skin.
fn reference(rgb: &[f32], w: usize, h: usize, centre: [f32; 2], radius: f32) -> Option<[f32; 3]> {
    let cx = centre[0] * w as f32;
    let cy = centre[1] * h as f32;
    let x0 = (cx - radius).floor().max(0.0) as usize;
    let y0 = (cy - radius).floor().max(0.0) as usize;
    let x1 = ((cx + radius).ceil().max(0.0) as usize).min(w);
    let y1 = ((cy + radius).ceil().max(0.0) as usize).min(h);
    let mut samples = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            if (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) <= radius {
                let p = pixel(rgb, w, x, y);
                if p.iter().all(|v| v.is_finite()) {
                    samples.push(p);
                }
            }
        }
    }
    if samples.len() < MIN_REFERENCE {
        return None;
    }
    let mut lumas: Vec<f32> = samples.iter().map(|p| luma(*p)).collect();
    // Anchored below the middle, so a sheen over up to half the cheek cannot become it.
    let anchor = quantile(&mut lumas, 0.4)?.max(1e-6);
    let mut sum = [0.0_f64; 3];
    let mut n = 0.0_f64;
    for p in &samples {
        let relative = luma(*p) / anchor;
        let weight = f64::from(skin_weight(relative) * (1.0 - smoothstep(1.0, SHEEN, relative)));
        for c in 0..3 {
            sum[c] += f64::from(p[c]) * weight;
        }
        n += weight;
    }
    let mean = sum.map(|v| (v / n.max(1e-9)) as f32);
    (n >= 1.0 && luma(mean) > 1e-5).then_some(mean)
}

/// Correct the dark circle the operation selects. Without a `source` there is no cheek to
/// measure against and nothing is changed.
pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    let Some(source) = edit.source else {
        return;
    };
    let radius = (edit.radius * w.min(h) as f32).round().max(1.0) as usize;
    let Some(cheek) = reference(rgb, w, h, source, radius as f32 * REFERENCE_RADII) else {
        return;
    };
    let target_luma = luma(cheek);
    let Some(rect) = Rect::around(coverage.bounds, radius * 3, w, h) else {
        return;
    };
    let selected: Vec<f32> = rect.cells().map(|(x, y)| coverage.at(x, y, w, h)).collect();
    let channels: [Vec<f32>; 3] = std::array::from_fn(|c| {
        rect.cells()
            .map(|(x, y)| rgb[(y * w + x) * 3 + c])
            .collect()
    });
    let lumas: Vec<f32> = rect
        .cells()
        .map(|(x, y)| luma(pixel(rgb, w, x, y)))
        .collect();
    // The skin around each pixel, first over everything selected, then without the lashes,
    // hair and glints that first estimate shows up.
    let rough = Weighted::new(&selected, rect.w, rect.h, radius).mean(&lumas);
    let skin: Vec<f32> = selected
        .iter()
        .zip(&lumas)
        .zip(&rough)
        .map(|((s, l), r)| s * skin_weight(l / r.max(1e-6)))
        .collect();
    let wide = Weighted::new(&skin, rect.w, rect.h, radius);
    let wide: [Vec<f32>; 3] = std::array::from_fn(|c| wide.mean(&channels[c]));
    let fine_radius = ((radius as f32 * FINE_SHARE).round() as usize).max(1);
    let fine = Weighted::new(&skin, rect.w, rect.h, fine_radius);
    let fine: [Vec<f32>; 3] = std::array::from_fn(|c| fine.mean(&channels[c]));
    let cheek_sum = (cheek[0] + cheek[1] + cheek[2]).max(1e-6);
    let [x0, y0, x1, y1] = coverage.bounds;
    for y in y0..y1 {
        for x in x0..x1 {
            let a = coverage.at(x, y, w, h) * edit.amount;
            if a <= 0.0 {
                continue;
            }
            let i = (y - rect.y0) * rect.w + x - rect.x0;
            let coarse = [wide[0][i], wide[1][i], wide[2][i]];
            let close = [fine[0][i], fine[1][i], fine[2][i]];
            let base = if luma(close) > luma(coarse) {
                close
            } else {
                coarse
            };
            let base_luma = luma(base).max(1e-6);
            let deficit = (target_luma - base_luma) / target_luma.max(1e-6);
            let onset = smoothstep(ONSET[0], ONSET[1], deficit);
            if onset <= 0.0 {
                // Already as light as the cheek: nothing to lift, no cast to match.
                continue;
            }
            let old = pixel(rgb, w, x, y);
            let lum = luma(old);
            let relative = lum / base_luma;
            let keep = smoothstep(DARK_DETAIL[0], DARK_DETAIL[1], relative)
                * (1.0 - smoothstep(BESIDE[0], BESIDE[1], relative));
            if keep <= 0.0 {
                continue;
            }
            // The cast is matched in proportion to how dark the shadow is: a faint shadow
            // keeps its own colour.
            let cast = edit.tone.clamp(0.0, 1.0) * smoothstep(0.0, FULL_CAST, deficit);
            let base_sum = (base[0] + base[1] + base[2]).max(1e-6);
            let colour: [f32; 3] = std::array::from_fn(|c| {
                base[c] / base_sum + cast * (cheek[c] / cheek_sum - base[c] / base_sum)
            });
            let scale = target_luma / luma(colour).max(1e-6);
            let gains: [f32; 3] = std::array::from_fn(|c| colour[c] * scale / base[c].max(1e-6));
            let reach = gains.iter().fold(1.0_f32, |reach, g| {
                if *g > GAIN_RANGE[1] {
                    reach.min((GAIN_RANGE[1] - 1.0) / (g - 1.0))
                } else if *g < GAIN_RANGE[0] {
                    reach.min((1.0 - GAIN_RANGE[0]) / (1.0 - g))
                } else {
                    reach
                }
            });
            let mut value: [f32; 3] =
                std::array::from_fn(|c| old[c] * (1.0 + reach * (gains[c] - 1.0)));
            // Nothing ends brighter than the cheek unless it already was: no lift turns the
            // skin under an eye white.
            let cap = target_luma.max(lum);
            let value_luma = luma(value);
            if value_luma > cap {
                value = value.map(|v| v * cap / value_luma);
            }
            let k = a * keep * onset;
            for c in 0..3 {
                rgb[(y * w + x) * 3 + c] = old[c] + k * (value[c] - old[c]);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_recipe::retouch_tools::Tool;

    const W: usize = 120;
    const H: usize = 120;

    fn edit() -> Edit {
        Edit {
            id: "undereye".into(),
            tool: Tool::UnderEye,
            enabled: true,
            region: [0.5, 0.35, 0.25, 0.12],
            source: Some([0.5, 0.75]),
            amount: 1.0,
            feather: 0.0,
            radius: 0.03,
            source_scale: 1.0,
            preserve_microtexture: false,
            texture_heal: false,
            clean_ring_fit: false,
            curved_heal: false,
            heal_samples: Vec::new(),
            texture_sources: Vec::new(),
            target_color: None,
            sensitivity: None,
            keep_dark_marks: false,
            texture: 1.0,
            tone: 1.0,
            warmth: 0.0,
            tint: 0.0,
            mask: None,
            skin: None,
            selection: None,
            matte: None,
        }
    }

    /// Cheek skin with a fine pore pattern, and a purple shadow band above it.
    fn face() -> Vec<f32> {
        let mut rgb = Vec::with_capacity(W * H * 3);
        for y in 0..H {
            for x in 0..W {
                let pore = if (x * 7 + y * 13) % 5 == 0 { 0.9 } else { 1.0 };
                let skin = [0.40, 0.26, 0.20];
                let shadow = (25..60).contains(&y);
                let p = if shadow { [0.22, 0.15, 0.16] } else { skin };
                rgb.extend(p.map(|v| v * pore));
            }
        }
        rgb
    }

    fn render(rgb: &mut [f32], edit: &Edit) {
        let coverage = Coverage::new(edit, W, H);
        apply(rgb, W, H, edit, &coverage);
    }

    fn at(rgb: &[f32], x: usize, y: usize) -> [f32; 3] {
        pixel(rgb, W, x, y)
    }

    #[test]
    fn a_shadow_is_lifted_toward_the_cheek_and_never_past_it() {
        let mut rgb = face();
        render(&mut rgb, &edit());
        let cheek = luma([0.40, 0.26, 0.20]);
        let after = luma(at(&rgb, 60, 42));
        assert!(after > luma([0.22, 0.15, 0.16]) * 1.3, "{after}");
        for y in 25..60 {
            for x in 40..80 {
                assert!(luma(at(&rgb, x, y)) <= cheek * 1.0001, "{x},{y}");
            }
        }
    }

    #[test]
    fn the_cast_is_matched_to_the_cheek() {
        let mut rgb = face();
        render(&mut rgb, &edit());
        let p = at(&rgb, 60, 42);
        let chroma = |p: [f32; 3]| p.map(|v| v / (p[0] + p[1] + p[2]));
        let cheek = chroma([0.40, 0.26, 0.20]);
        let before = chroma([0.22, 0.15, 0.16]);
        let after = chroma(p);
        let distance = |a: [f32; 3], b: [f32; 3]| (0..3).map(|c| (a[c] - b[c]).abs()).sum::<f32>();
        assert!(distance(after, cheek) < distance(before, cheek) * 0.3);
    }

    #[test]
    fn pores_keep_their_relative_contrast() {
        let mut rgb = face();
        render(&mut rgb, &edit());
        // Two neighbours inside the shadow, one a pore: their ratio is unchanged.
        let pore = (55..W)
            .map(|x| (x, 42))
            .find(|(x, y)| (x * 7 + y * 13) % 5 == 0)
            .unwrap();
        let skin = (pore.0 - 1, 42);
        let ratio = luma(at(&rgb, pore.0, pore.1)) / luma(at(&rgb, skin.0, skin.1));
        assert!((ratio - 0.9).abs() < 0.03, "{ratio}");
    }

    #[test]
    fn skin_as_light_as_the_cheek_and_lashes_are_not_touched() {
        let mut rgb = face();
        // A lash: a thin, very dark line inside the shadow.
        for x in 30..90 {
            let i = (30 * W + x) * 3;
            rgb[i..i + 3].copy_from_slice(&[0.02, 0.015, 0.012]);
        }
        let before = rgb.clone();
        // A selection reaching well into the cheek below the shadow.
        let mut op = edit();
        op.region = [0.5, 0.4, 0.25, 0.25];
        render(&mut rgb, &op);
        assert_eq!(at(&rgb, 60, 30), at(&before, 60, 30));
        assert!(luma(at(&rgb, 60, 42)) > luma(at(&before, 60, 42)));
        // Rows 60.. are cheek-coloured: nothing to lift there, and no bright rim beside the
        // shadow either.
        let cheek = luma([0.40, 0.26, 0.20]);
        for y in 60..76 {
            for x in 40..80 {
                if y >= 64 {
                    assert_eq!(at(&rgb, x, y), at(&before, x, y), "{x},{y}");
                }
                assert!(luma(at(&rgb, x, y)) <= cheek * 1.0001, "{x},{y}");
            }
        }
    }

    #[test]
    fn a_deep_red_crease_moves_toward_the_cheek_without_turning_salmon() {
        let crease = [0.20, 0.05, 0.045];
        let mut rgb = face();
        for y in 25..60 {
            for x in 0..W {
                let i = (y * W + x) * 3;
                rgb[i..i + 3].copy_from_slice(&crease);
            }
        }
        render(&mut rgb, &edit());
        let p = at(&rgb, 61, 42);
        let saturation = |p: [f32; 3]| {
            let max = p[0].max(p[1]).max(p[2]);
            (max - p[0].min(p[1]).min(p[2])) / max
        };
        assert!(luma(p) > luma(crease), "{p:?}");
        assert!(saturation(p) < saturation(crease) - 0.05, "{p:?}");
        // Green and blue rose together: the hue did not swing.
        let ratio = |p: [f32; 3]| p[2] / p[1];
        assert!((ratio(p) - ratio(crease)).abs() < 0.2, "{p:?}");
    }

    #[test]
    fn a_sheen_on_the_cheek_is_not_the_target() {
        let mut rgb = face();
        // A grey sheen over every other cheek pixel below the shadow.
        for y in 70..H {
            for x in (y % 2..W).step_by(2) {
                let i = (y * W + x) * 3;
                rgb[i..i + 3].copy_from_slice(&[0.62, 0.55, 0.53]);
            }
        }
        render(&mut rgb, &edit());
        let skin = luma([0.40, 0.26, 0.20]);
        for x in 40..80 {
            assert!(luma(at(&rgb, x, 42)) <= skin * 1.0001, "{x}");
        }
    }

    #[test]
    fn without_a_cheek_reference_nothing_changes() {
        let mut rgb = face();
        let before = rgb.clone();
        let mut op = edit();
        op.source = None;
        render(&mut rgb, &op);
        assert_eq!(rgb, before);
    }

    #[test]
    fn amount_is_the_share_of_the_shadow_removed() {
        let mut half = face();
        let mut full = face();
        let mut op = edit();
        render(&mut full, &op);
        op.amount = 0.5;
        render(&mut half, &op);
        let before = luma([0.22, 0.15, 0.16]);
        let lift_full = luma(at(&full, 61, 42)) - before;
        let lift_half = luma(at(&half, 61, 42)) - before;
        assert!(
            (lift_half / lift_full - 0.5).abs() < 0.05,
            "{lift_half} {lift_full}"
        );
    }
}
