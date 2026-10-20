//! Bounded dark-circle correction preserving the eye socket's original lighting.
//!
//! The cheek is a reference, not a replacement colour. A single limited exposure and
//! partial chroma adjustment are measured for the selected shadow. Original gradients
//! and texture survive; a smoothed inward correction field blends the result into skin.
//! The reference rejects sheen, pores and dark detail. The shadow is measured on
//! skin-only low bands, then receives one bounded exposure and partial colour gain.
//! It never becomes a uniform cheek-coloured patch. Already bright skin, glints,
//! lashes and authored zero selection are excluded from the correction.
//!
//! Nothing is generated: the target is this photograph's own cheek. ADR-0094.
// Every plane covers one bounded rectangle and every index is derived from it.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{blur, luma, quantile, smoothstep, Rect, Weighted};
use aura_recipe::retouch_tools::Edit;

pub(crate) const VERSION: &str = "socket-preserving-v2-reference-consensus";
/// Correct a share of excess darkness, retaining the socket's lighting shape.
const EXCESS_SHARE: f32 = 0.35;
/// A cheek reference cannot justify painting an eye socket up to cheek brightness.
const MAX_LIFT_STOPS: f32 = 0.32;
const MAX_CAST_SHARE: f32 = 0.18;
const MAX_CAST_LOG_GAIN: f32 = 0.06;

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
/// The darkness, as a share of the cheek, at which the limited cast adjustment reaches full strength.
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

fn chroma(p: [f32; 3]) -> [f32; 3] {
    let sum = p.iter().sum::<f32>().max(1e-6);
    p.map(|v| v / sum)
}

fn colour_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
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
    let mut disk_pixels = 0_usize;
    for y in y0..y1 {
        for x in x0..x1 {
            if (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) <= radius {
                disk_pixels += 1;
                let p = pixel(rgb, w, x, y);
                if p.iter().all(|v| v.is_finite() && *v >= 0.0) && luma(p) > 1e-6 {
                    samples.push(p);
                }
            }
        }
    }
    if samples.len() < MIN_REFERENCE || samples.len() * 5 < disk_pixels * 4 {
        return None;
    }
    let mut lumas: Vec<f32> = samples.iter().map(|p| luma(*p)).collect();
    // Anchored below the middle, so a sheen over up to half the cheek cannot become it.
    let anchor = quantile(&mut lumas, 0.4)?.max(1e-6);
    let weight = |p: [f32; 3]| {
        let relative = luma(p) / anchor;
        skin_weight(relative) * (1.0 - smoothstep(1.0, SHEEN, relative))
    };
    // Luminance alone cannot distinguish cheek skin from similarly bright clothing,
    // makeup or background. Require a dominant, compact chromaticity consensus among
    // the usable lower tones. This rejects ambiguous references, not particular skin hues.
    let colours: Vec<[f32; 3]> = samples
        .iter()
        .filter(|p| weight(**p) > 0.25)
        .map(|p| chroma(*p))
        .collect();
    if colours.len() < MIN_REFERENCE {
        return None;
    }
    let mut centre = [0.0; 3];
    for c in 0..3 {
        let mut channel: Vec<f32> = colours.iter().map(|p| p[c]).collect();
        centre[c] = quantile(&mut channel, 0.5)?;
    }
    let mut eligible_mass = 0.0_f64;
    let mut sum = [0.0_f64; 3];
    let mut n = 0.0_f64;
    for p in &samples {
        let luminance_weight = f64::from(weight(*p));
        eligible_mass += luminance_weight;
        let weight = luminance_weight
            * f64::from(1.0 - smoothstep(0.025, 0.07, colour_distance(chroma(*p), centre)));
        for c in 0..3 {
            sum[c] += f64::from(p[c]) * weight;
        }
        n += weight;
    }
    let mean = sum.map(|v| (v / n.max(1e-9)) as f32);
    // Never infer a cheek from a tiny surviving island or two incompatible populations.
    (n >= MIN_REFERENCE as f64
        && n >= samples.len() as f64 * 0.2
        && n >= eligible_mass * 0.65
        && luma(mean) > 1e-5)
        .then_some(mean)
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
    // One exposure/chroma adjustment per shadow, rather than a different gain
    // driving every pixel to a flat cheek colour. This retains the socket's own
    // gradients and makes a darker pore/crease stay darker after correction.
    let bases: Vec<[f32; 3]> = (0..rect.len())
        .map(|i| {
            let coarse = [wide[0][i], wide[1][i], wide[2][i]];
            let close = [fine[0][i], fine[1][i], fine[2][i]];
            if luma(close) > luma(coarse) {
                close
            } else {
                coarse
            }
        })
        .collect();
    let Some((colour_gains, exposure, reference_confidence)) =
        shadow_correction(&bases, &selected, cheek, edit.tone)
    else {
        return;
    };
    // Smooth the correction field inward. Authored zero coverage, lashes and
    // already bright skin stay excluded; smoothing never creates new support.
    let eligibility: Vec<f32> = rect
        .cells()
        .enumerate()
        .map(|(i, (x, y))| {
            let base_luma = luma(bases[i]).max(1e-6);
            let relative = luma(pixel(rgb, w, x, y)) / base_luma;
            let keep = smoothstep(DARK_DETAIL[0], DARK_DETAIL[1], relative)
                * (1.0 - smoothstep(BESIDE[0], BESIDE[1], relative));
            selected[i] * keep * smoothstep(ONSET[0], ONSET[1], 1.0 - base_luma / target_luma)
        })
        .collect();
    let smoothed = blur(&eligibility, rect.w, rect.h, radius.max(1));
    let [x0, y0, x1, y1] = coverage.bounds;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y - rect.y0) * rect.w + x - rect.x0;
            let k = eligibility[i].min(smoothed[i])
                * edit.amount.clamp(0.0, 1.0)
                * reference_confidence;
            if k <= 0.0 {
                continue;
            }
            let old = pixel(rgb, w, x, y);
            let lum = luma(old);
            let colour = std::array::from_fn(|c| old[c] * colour_gains[c]);
            let scale = lum * exposure / luma(colour).max(1e-6);
            let mut value = colour.map(|v| v * scale);
            // Nothing ends brighter than the cheek unless it already was: no lift turns the
            // skin under an eye white.
            let cap = target_luma.max(lum);
            let value_luma = luma(value);
            if value_luma > cap {
                value = value.map(|v| v * cap / value_luma);
            }
            for c in 0..3 {
                rgb[(y * w + x) * 3 + c] = old[c] + k * (value[c] - old[c]);
            }
        }
    }
}

/// Measure one bounded exposure and colour correction for the selected socket.
fn shadow_correction(
    bases: &[[f32; 3]],
    selected: &[f32],
    cheek: [f32; 3],
    tone: f32,
) -> Option<([f32; 3], f32, f32)> {
    let target_luma = luma(cheek);
    let cheek_sum = cheek.iter().sum::<f32>().max(1e-6);
    let mut shadow_lumas: Vec<f32> = bases
        .iter()
        .zip(selected)
        .filter_map(|(p, a)| {
            let l = luma(*p);
            (*a > 0.2 && l > 1e-6 && l < target_luma * (1.0 - ONSET[1])).then_some(l)
        })
        .collect();
    if shadow_lumas.len() < MIN_REFERENCE {
        return None;
    }
    let shadow_luma = quantile(&mut shadow_lumas, 0.5).unwrap_or(target_luma);
    let exposure = ((target_luma / shadow_luma).log2() * EXCESS_SHARE)
        .clamp(0.0, MAX_LIFT_STOPS)
        .exp2();
    let mut shadow = [0.0_f32; 3];
    let mut count = 0.0;
    for (p, a) in bases.iter().zip(selected) {
        let l = luma(*p);
        if *a > 0.2 && (0.75 * shadow_luma..=1.25 * shadow_luma).contains(&l) {
            for c in 0..3 {
                shadow[c] += p[c] * a;
            }
            count += a;
        }
    }
    if count <= 1e-6 {
        return None;
    }
    shadow = shadow.map(|v| v / count);
    // Even a coherent reference can be the wrong surface. Reduce confidence when its
    // colour is incompatible with the measured socket; severe disagreement is a no-op.
    // This is exposure independent and does not force skin toward a fixed target hue.
    let reference_confidence =
        1.0 - smoothstep(0.20, 0.40, colour_distance(chroma(cheek), chroma(shadow)));
    if reference_confidence <= 0.0 {
        return None;
    }
    let shadow_sum = shadow.iter().sum::<f32>().max(1e-6);
    let deficit = (1.0 - shadow_luma / target_luma).max(0.0);
    let cast = tone.clamp(0.0, 1.0) * MAX_CAST_SHARE * smoothstep(0.0, FULL_CAST, deficit);
    let colour_gains: [f32; 3] = std::array::from_fn(|c| {
        let ratio = (cheek[c] / cheek_sum) / (shadow[c] / shadow_sum).max(1e-6);
        (ratio.max(1e-6).ln() * cast)
            .clamp(-MAX_CAST_LOG_GAIN, MAX_CAST_LOG_GAIN)
            .exp()
    });
    Some((colour_gains, exposure, reference_confidence))
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
            repair_passes: None,
            target_color: None,
            sensitivity: None,
            keep_dark_marks: false,
            mole_edge: None,
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
        let before = luma([0.22, 0.15, 0.16]);
        assert!(after > before * 1.02 && after < before * 1.25, "{after}");
        assert!(
            (cheek - after) >= (cheek - before) * 0.55,
            "socket shading flattened"
        );
        for y in 25..60 {
            for x in 40..80 {
                assert!(luma(at(&rgb, x, y)) <= cheek * 1.0001, "{x},{y}");
            }
        }
    }

    #[test]
    fn the_cast_is_reduced_without_replacing_the_original_skin_colour() {
        let mut rgb = face();
        render(&mut rgb, &edit());
        let p = at(&rgb, 60, 42);
        let chroma = |p: [f32; 3]| p.map(|v| v / (p[0] + p[1] + p[2]));
        let cheek = chroma([0.40, 0.26, 0.20]);
        let before = chroma([0.22, 0.15, 0.16]);
        let after = chroma(p);
        let distance = |a: [f32; 3], b: [f32; 3]| (0..3).map(|c| (a[c] - b[c]).abs()).sum::<f32>();
        assert!(distance(after, cheek) < distance(before, cheek));
        assert!(distance(after, cheek) > distance(before, cheek) * 0.55);
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
        assert!(saturation(p) < saturation(crease) - 0.005, "{p:?}");
        assert!(
            saturation(p) > saturation(crease) - 0.08,
            "cast was washed out: {p:?}"
        );
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
    fn an_ambiguous_reference_or_an_unrelated_surface_cannot_recolour_the_socket() {
        for mixed in [false, true] {
            let mut rgb = face();
            for y in 70..H {
                for x in 0..W {
                    let colour = if mixed && x < W / 2 {
                        [0.40, 0.26, 0.20]
                    } else {
                        // Similar luminance to skin, but an incompatible green surface.
                        [0.10, 0.35, 0.10]
                    };
                    rgb[(y * W + x) * 3..(y * W + x) * 3 + 3].copy_from_slice(&colour);
                }
            }
            let before = rgb.clone();
            render(&mut rgb, &edit());
            assert_eq!(rgb, before, "mixed={mixed}");
        }
    }

    #[test]
    fn reference_requires_enough_finite_support_and_rejects_minority_colour_contamination() {
        let clean = [0.40, 0.26, 0.20].repeat(W * H);
        let expected = reference(&clean, W, H, [0.5, 0.5], 15.0).unwrap();
        let mut contaminated = clean.clone();
        for (i, p) in contaminated.chunks_exact_mut(3).enumerate() {
            if i % 5 == 0 {
                p.copy_from_slice(&[0.10, 0.35, 0.10]);
            }
        }
        let measured = reference(&contaminated, W, H, [0.5, 0.5], 15.0).unwrap();
        assert!(colour_distance(chroma(expected), chroma(measured)) < 1e-4);
        let mut invalid = clean;
        for (i, p) in invalid.chunks_exact_mut(3).enumerate() {
            if i % 5 != 0 {
                p.fill(f32::NAN);
            }
        }
        assert!(reference(&invalid, W, H, [0.5, 0.5], 15.0).is_none());
    }

    #[test]
    fn amount_scales_the_bounded_correction_linearly() {
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

    #[test]
    fn a_socket_gradient_keeps_its_shape_instead_of_becoming_flat_cheek_colour() {
        let mut rgb = face();
        for y in 25..60 {
            for x in 0..W {
                let gain = 0.5 + x as f32 * 0.003;
                let p = [0.40, 0.26, 0.20].map(|v| v * gain);
                rgb[(y * W + x) * 3..(y * W + x) * 3 + 3].copy_from_slice(&p);
            }
        }
        let before = luma(at(&rgb, 45, 42)) / luma(at(&rgb, 75, 42));
        render(&mut rgb, &edit());
        let after = luma(at(&rgb, 45, 42)) / luma(at(&rgb, 75, 42));
        assert!(
            (after - before).abs() < 0.01,
            "socket slope changed: {before} -> {after}"
        );
    }

    #[test]
    fn a_very_bright_cheek_cannot_create_a_pale_patch_at_any_exposure() {
        for exposure in [0.04, 0.2, 1.0, 3.0] {
            let mut rgb = face();
            for y in 25..60 {
                for x in 0..W {
                    rgb[(y * W + x) * 3..(y * W + x) * 3 + 3].copy_from_slice(&[0.06, 0.04, 0.035]);
                }
            }
            for p in &mut rgb {
                *p *= exposure;
            }
            let before = rgb.clone();
            render(&mut rgb, &edit());
            for (old, new) in before.chunks_exact(3).zip(rgb.chunks_exact(3)) {
                let a = luma([old[0], old[1], old[2]]);
                let b = luma([new[0], new[1], new[2]]);
                assert!(
                    (b / a).log2() <= MAX_LIFT_STOPS + 1e-5,
                    "lift exceeded the safe bound"
                );
                assert!(new.iter().all(|v| v.is_finite() && *v >= 0.0));
            }
            assert!(luma(at(&rgb, 60, 42)) > luma(at(&before, 60, 42)));
        }
    }
}
