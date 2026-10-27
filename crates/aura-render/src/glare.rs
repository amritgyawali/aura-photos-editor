//! Lens-reflection removal by veil subtraction. No missing eye detail is invented.
//!
//! A reflection on a spectacle lens adds light: what the camera recorded is the scene behind the
//! lens plus a smooth veil, often tinted green or purple by an anti-reflective coating. So the
//! correction subtracts, it does not paint:
//!
//! 1. **What lies underneath** is estimated by filling the selection from the photograph around
//!    it (push-pull). Pixels inside the selection never feed this estimate: a veil over an iris
//!    can read as bright as the skin around the lens and would become its own "scene beneath".
//! 2. **The veil** is the amount by which each pixel's darkest channel rises above that
//!    estimate, averaged over the pixels where it is visible so texture under it keeps its
//!    contrast. A veil adds to every channel, so the darkest channel bounds it. Where the
//!    reflection crosses something darker than the lens's surroundings - an iris, lashes - the
//!    veil cannot be seen directly, so it is carried across gaps the reflection encloses. A
//!    pixel clearly darker than the veil is in front of the lens (a frame) and keeps its value.
//! 3. **Its colour** is the median per-channel ratio over the strongest part of the reflection,
//!    removed with it, so a green coating sheen goes rather than turning grey.
//!
//! Detail clipped to white by the reflection is gone from the file; there it lands on the tone
//! around it, never on an invented eye.

#![allow(clippy::indexing_slicing)] // Fixed-size pixel buffers, indexed within their bounds.

use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::Edit;
use rayon::prelude::*;

/// Excess over the underlying estimate that is still treated as the photograph's own texture.
const TOLERANCE: f32 = 0.12;
/// A coloured veil may lean this far from neutral, per channel, and no further.
const MAX_TINT: f32 = 1.8;
/// No channel is taken below this share of what it recorded.
const FLOOR: f32 = 0.08;

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn min3(p: [f32; 3]) -> f32 {
    p[0].min(p[1]).min(p[2])
}

/// The veil of the reflection inside one working box, per pixel (darkest-channel units), its
/// colour (darkest channel one), and the estimate of the scene underneath.
struct Veil {
    amount: Vec<f32>,
    tint: [f32; 3],
}

#[allow(clippy::too_many_lines)] // One measurement, read top to bottom.
fn measure(box_rgb: &[f32], cover: &[f32], bw: usize, bh: usize) -> Veil {
    let n = bw * bh;
    let darkest: Vec<f32> = box_rgb
        .chunks_exact(3)
        .map(|p| min3([p[0], p[1], p[2]]).max(0.0))
        .collect();
    // Pass 1: underneath from outside the selection only. A feathered selection reaches well
    // into the reflection at low strength, so anything it touches at all is unknown here -
    // otherwise the reflection would be part of its own estimate and nothing would stand out.
    let outside: Vec<f32> = cover
        .iter()
        .map(|c| 1.0 - smoothstep(0.0, 0.02, *c))
        .collect();
    // Pixels inside the selection are never part of this estimate: a veil over an iris can
    // read as bright as the skin around the lens, and would then be its own "scene beneath".
    // Unveiled content inside the selection is handled below, by darkness and support.
    let under = crate::fill::push_pull(box_rgb, 3, &outside, bw, bh);
    let under_darkest: Vec<f32> = under
        .chunks_exact(3)
        .map(|p| min3([p[0], p[1], p[2]]).max(0.0))
        .collect();
    // The veil is smooth; a slightly blurred darkest channel keeps a fine bright detail inside
    // the reflection (a catchlight, an iris fleck) from being taken as veil.
    // The average runs over pixels where the veil is visible only, so an iris or lashes
    // beside the measurement do not pull it down.
    let radius = (bw.max(bh) / 60).max(1);
    let visible: Vec<f32> = (0..n)
        .map(|i| {
            if darkest[i] > under_darkest[i] * (1.0 + TOLERANCE) {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let weighted: Vec<f32> = darkest.iter().zip(&visible).map(|(d, k)| d * k).collect();
    let sum = crate::bands::blur(&weighted, bw, bh, radius);
    let support = crate::bands::blur(&visible, bw, bh, radius);
    let smooth: Vec<f32> = (0..n)
        .map(|i| {
            if support[i] > 1e-4 {
                sum[i] / support[i]
            } else {
                darkest[i]
            }
        })
        .collect();
    let measured: Vec<f32> = (0..n)
        .map(|i| {
            if cover[i] <= 0.0 {
                return 0.0;
            }
            let floor = under_darkest[i] * (1.0 + TOLERANCE);
            // A pixel no brighter than the scene beneath is not under the veil at all.
            let own = darkest[i] - floor;
            if own <= 0.0 {
                return 0.0;
            }
            // The tolerance only decides where the veil is visible; what is removed is the
            // whole smooth rise above the scene, so texture under it keeps its contrast. Never
            // more than twice the pixel's own rise, so nothing ends further below the scene
            // than it was above it.
            let rise = darkest[i] - under_darkest[i];
            (smooth[i] - under_darkest[i]).clamp(0.0, 2.0 * rise)
        })
        .collect();
    // Colour of the veil, from its strongest third: the median, per channel, of each pixel's
    // rise over its darkest channel's rise. What lies under the veil differs from the scene
    // estimate pixel by pixel, so a mean would read that difference as colour; when the
    // ratios disagree the colour is pulled back toward neutral.
    let peak = measured.iter().copied().fold(0.0_f32, f32::max);
    let mut ratios: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    if peak > 1e-4 {
        for i in 0..n {
            let rise = darkest[i] - under_darkest[i];
            if measured[i] < peak / 3.0 || rise <= 0.02 || cover[i] < 0.5 {
                continue;
            }
            for (c, list) in ratios.iter_mut().enumerate() {
                list.push(((box_rgb[i * 3 + c] - under[i * 3 + c]) / rise).clamp(0.0, 4.0));
            }
        }
    }
    let tint = if ratios[0].len() >= 8 {
        let median = |values: &mut Vec<f32>| {
            values.sort_by(f32::total_cmp);
            values[values.len() / 2]
        };
        let mut centre = [0.0_f32; 3];
        let mut spread = 0.0_f32;
        for (c, list) in ratios.iter_mut().enumerate() {
            centre[c] = median(list);
            let mut deviation: Vec<f32> = list.iter().map(|v| (v - centre[c]).abs()).collect();
            spread = spread.max(median(&mut deviation));
        }
        let base = min3(centre).max(1e-5);
        let trust = (1.0 - spread / 0.35).clamp(0.0, 1.0);
        centre.map(|v| (1.0 + trust * (v / base - 1.0)).clamp(1.0, MAX_TINT))
    } else {
        [1.0; 3]
    };
    // Where the reflection crosses something darker than its surroundings - an iris, a pupil,
    // lashes - "content plus veil" can still read darker than the skin around the lens, so the
    // veil cannot be seen there directly. A reflection is a smooth sheet: carry the veil measured
    // around such a gap across it, within the reflection's own footprint (a gap the reflection
    // surrounds, never a band beyond its edge).
    // Outside the selection there is known to be no veil, so the carried veil falls off
    // toward the selection's edge instead of carrying its plateau onto a brow beside it.
    let confidence: Vec<f32> = (0..n)
        .map(|i| {
            if cover[i] <= 0.0 {
                1.0
            } else if measured[i] <= 0.0 {
                0.0
            } else {
                smoothstep(0.0, 0.02, darkest[i] - under_darkest[i] * (1.0 + TOLERANCE))
            }
        })
        .collect();
    let present: Vec<bool> = measured.iter().map(|v| *v > 0.005).collect();
    let leak = (bw.max(bh) as f32 / 40.0).max(2.0);
    let enclosed = crate::fill::enclosed(&present, cover, bw, bh, leak);
    let carried = crate::fill::push_pull(&measured, 1, &confidence, bw, bh);
    let amount: Vec<f32> = (0..n)
        .map(|i| {
            if cover[i] <= 0.0 {
                return 0.0;
            }
            let veil = carried[i].max(0.0);
            let support = if enclosed[i] { 1.0 } else { 0.0 };
            // Nothing under a veil is darker than the veil. A pixel well below it is in front
            // of the lens (a frame, an occluding finger) and keeps its value exactly.
            let behind = smoothstep(0.6 * veil, 0.9 * veil, darkest[i]);
            let gap = (veil * support * behind).min(darkest[i] * (1.0 - FLOOR));
            measured[i].max(gap)
        })
        .collect();
    Veil { amount, tint }
}

/// Remove the reflection the edit selects. Pixels outside the selection, and pixels no brighter
/// than the scene estimated beneath them, are returned exactly.
pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    let [x0, y0, x1, y1] = coverage.bounds;
    if x1 <= x0 || y1 <= y0 || edit.amount <= 0.0 || rgb.len() != w * h * 3 {
        return;
    }
    // Enough of the surroundings to say what the lens shows around the reflection.
    let margin = ((x1 - x0).max(y1 - y0) / 4).max(4);
    let bx = x0.saturating_sub(margin);
    let by = y0.saturating_sub(margin);
    let bw = (x1 + margin).min(w) - bx;
    let bh = (y1 + margin).min(h) - by;
    let mut box_rgb = Vec::with_capacity(bw * bh * 3);
    let mut cover = Vec::with_capacity(bw * bh);
    for y in by..by + bh {
        box_rgb.extend_from_slice(&rgb[(y * w + bx) * 3..(y * w + bx + bw) * 3]);
        for x in bx..bx + bw {
            cover.push(coverage.at(x, y, w, h).clamp(0.0, 1.0));
        }
    }
    let veil = measure(&box_rgb, &cover, bw, bh);
    let strength = edit.amount.clamp(0.0, 1.0);
    rgb[by * w * 3..(by + bh) * w * 3]
        .par_chunks_mut(w * 3)
        .enumerate()
        .for_each(|(j, row)| {
            for i in 0..bw {
                let k = j * bw + i;
                // The veil is already zero away from the reflection, so the feather only has to
                // soften the selection's outer edge: removal is even across the reflection
                // instead of fading out into a bright ring.
                let a = smoothstep(0.0, 0.4, cover[k]) * strength;
                let v = veil.amount[k];
                if a <= 0.0 || v <= 0.0 {
                    continue;
                }
                let p = &mut row[(bx + i) * 3..(bx + i) * 3 + 3];
                for (value, tint) in p.iter_mut().zip(veil.tint) {
                    let old = *value;
                    let removed = (old - v * tint).max(old * FLOOR);
                    *value = old + a * (removed - old);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::luma;

    fn edit(region: [f32; 4], amount: f32) -> Edit {
        edit_feathered(region, amount, 0.1)
    }

    fn edit_feathered(region: [f32; 4], amount: f32, feather: f32) -> Edit {
        let mut e: Edit = serde_json::from_value(serde_json::json!({
            "id": "glare", "tool": "glare", "enabled": true,
            "region": region, "source": null, "amount": amount, "feather": feather,
            "radius": 0.02, "texture": 1.0, "tone": 0.5, "warmth": 0.0, "tint": 0.0
        }))
        .unwrap();
        e.amount = amount;
        e
    }

    fn fill(
        rgb: &mut [f32],
        w: usize,
        x: std::ops::Range<usize>,
        y: std::ops::Range<usize>,
        v: [f32; 3],
    ) {
        for yy in y {
            for xx in x.clone() {
                rgb[(yy * w + xx) * 3..(yy * w + xx) * 3 + 3].copy_from_slice(&v);
            }
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // Untouched and dark pixels must be bit-identical.
    fn a_reflection_is_removed_while_dark_detail_and_outside_pixels_are_exact() {
        let (w, h) = (160, 120);
        let mut rgb = vec![0.08; w * h * 3];
        // A bright neutral reflection with a dark bar (a frame, a lash) through it.
        fill(&mut rgb, w, 55..105, 40..80, [0.70, 0.72, 0.74]);
        fill(&mut rgb, w, 78..81, 45..75, [0.004; 3]);
        let before = rgb.clone();
        let e = edit([0.5, 0.5, 0.25, 0.3], 1.0);
        let coverage = Coverage::new(&e, w, h);
        apply(&mut rgb, w, h, &e, &coverage);
        // Inside the reflection the brightness comes most of the way down to the lens around it.
        let i = (60 * w + 65) * 3;
        assert!(
            luma([rgb[i], rgb[i + 1], rgb[i + 2]]) < 0.2,
            "{:?}",
            &rgb[i..i + 3]
        );
        // The reflection's slight blue cast went with it: the lens is neutral again.
        assert!((rgb[i + 2] - rgb[i]).abs() < 0.01, "{:?}", &rgb[i..i + 3]);
        for y in 45..75 {
            for x in 78..81 {
                let j = (y * w + x) * 3;
                assert_eq!(rgb[j..j + 3], before[j..j + 3], "dark detail at ({x}, {y})");
            }
        }
        for y in 0..20 {
            let j = (y * w) * 3;
            assert_eq!(rgb[j..j + w * 3], before[j..j + w * 3], "row {y} moved");
        }
    }

    #[test]
    fn a_widely_feathered_selection_still_finds_the_reflection() {
        // The desktop's default glare draft: half strength, 65 % feather.
        let (w, h) = (512, 384);
        let grey = 0.076;
        let mut rgb = vec![grey; w * h * 3];
        for y in 120..264 {
            for x in 170..342 {
                let (dx, dy) = ((x as f32 - 256.0) / 86.0, (y as f32 - 192.0) / 72.0);
                if dx * dx + dy * dy <= 1.0 {
                    rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[0.72, 0.75, 0.79]);
                }
            }
        }
        let before = rgb.clone();
        let e = edit_feathered([0.5, 0.5, 0.2, 0.25], 0.5, 0.65);
        let coverage = Coverage::new(&e, w, h);
        apply(&mut rgb, w, h, &e, &coverage);
        let i = (192 * w + 256) * 3;
        assert!(rgb[i] < before[i] - 0.2, "{} vs {}", rgb[i], before[i]);
        // Half strength at the centre: about half way down to the grey around it.
        assert!(rgb[i] > grey + 0.15, "{}", rgb[i]);
    }

    #[test]
    fn texture_under_the_veil_keeps_its_contrast() {
        let (w, h) = (160, 120);
        let mut rgb = vec![0.10; w * h * 3];
        for y in 30..90 {
            for x in 40..120 {
                // Fine stripes of the scene, plus a uniform veil.
                let scene = if x % 4 < 2 { 0.08 } else { 0.14 };
                let v = scene + 0.45;
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[v; 3]);
            }
        }
        let e = edit([0.5, 0.5, 0.3, 0.3], 1.0);
        let coverage = Coverage::new(&e, w, h);
        apply(&mut rgb, w, h, &e, &coverage);
        let at = |x: usize| rgb[(60 * w + x) * 3];
        let contrast = at(82) - at(80);
        assert!(at(80) < 0.3, "veil left: {}", at(80));
        assert!(contrast > 0.04, "stripes flattened: {contrast}");
    }

    #[test]
    fn a_coloured_coating_sheen_loses_its_colour() {
        let (w, h) = (160, 120);
        let mut rgb = vec![0.12; w * h * 3];
        fill(&mut rgb, w, 50..110, 35..85, [0.42, 0.62, 0.40]);
        let e = edit([0.5, 0.5, 0.3, 0.3], 1.0);
        let coverage = Coverage::new(&e, w, h);
        apply(&mut rgb, w, h, &e, &coverage);
        let i = (60 * w + 80) * 3;
        let green = rgb[i + 1] - rgb[i].max(rgb[i + 2]);
        assert!(green < 0.06, "green sheen left: {:?}", &rgb[i..i + 3]);
        assert!(rgb[i..i + 3].iter().all(|v| v.is_finite() && *v >= 0.0));
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn strength_scales_the_removal_and_zero_does_nothing() {
        let (w, h) = (120, 90);
        let mut base = vec![0.1; w * h * 3];
        fill(&mut base, w, 40..80, 30..60, [0.6; 3]);
        let run = |amount: f32| {
            let mut rgb = base.clone();
            let e = edit([0.5, 0.5, 0.3, 0.3], amount);
            let coverage = Coverage::new(&e, w, h);
            apply(&mut rgb, w, h, &e, &coverage);
            rgb[(45 * w + 60) * 3]
        };
        let (zero, half, full) = (run(0.0), run(0.5), run(1.0));
        assert_eq!(zero, 0.6);
        assert!(full < half && half < zero, "{full} {half} {zero}");
    }
}
