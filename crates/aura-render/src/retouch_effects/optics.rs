//! Optical tools: spin blur, pixelate, privacy blur, soft focus, Orton glow, local sharpening,
//! local noise reduction and defringing.
use super::{area, each, mix, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::SpinBlur
            | Tool::Pixelate
            | Tool::PrivacyBlur
            | Tool::SoftFocus
            | Tool::OrtonGlow
            | Tool::LocalSharpen
            | Tool::LocalDenoise
            | Tool::Defringe
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::SpinBlur => spin(rgb, frame, edit, coverage),
        Tool::Pixelate => pixelate(rgb, frame, edit, coverage),
        Tool::LocalSharpen => sharpen(rgb, frame, edit, coverage),
        Tool::LocalDenoise => denoise(rgb, frame, edit, coverage),
        Tool::Defringe => defringe(rgb, frame, edit, coverage),
        _ => blurred(rgb, frame, edit, coverage),
    }
}

/// The selection's rectangle, and a snapshot of it grown by `margin`, with its three channels
/// blurred by `radius`.
struct Blurred {
    snapshot: Snapshot,
    planes: [Vec<f32>; 3],
}

impl Blurred {
    fn new(rgb: &[f32], frame: Frame, coverage: &Coverage, radius: usize) -> Option<Self> {
        let source = area(coverage, frame, radius * 3)?;
        let snapshot = Snapshot::take(rgb, frame, source);
        let r = snapshot.rect();
        let planes = [0, 1, 2].map(|c| blur(&snapshot.plane(c), r.w, r.h, radius));
        Some(Self { snapshot, planes })
    }

    fn at(&self, x: usize, y: usize) -> [f32; 3] {
        let i = self.snapshot.index(x, y);
        [self.planes[0][i], self.planes[1][i], self.planes[2][i]]
    }
}

fn blurred(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(b) = Blurred::new(rgb, frame, coverage, radius) else {
        return;
    };
    let tone = edit.tone.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let soft = b.at(x, y);
        let new = match edit.tool {
            Tool::PrivacyBlur => soft,
            Tool::SoftFocus => {
                // Highlights spread into darker surroundings; darker areas soften by `tone`.
                let lighten: [f32; 3] = std::array::from_fn(|c| old[c].max(soft[c]));
                mix(lighten, soft, tone * 0.5)
            }
            _ => {
                // Orton: screen a brightened, blurred copy over the photograph, then restore
                // some contrast by multiplying with the blur's own shape.
                let bright = soft.map(|v| (v * (1.0 + tone * 2.0)).min(1.0));
                let screen: [f32; 3] = std::array::from_fn(|c| {
                    let a = old[c].min(1.0);
                    1.0 - (1.0 - a) * (1.0 - bright[c]) + (old[c] - a)
                });
                mix(
                    screen,
                    std::array::from_fn(|c| screen[c] * soft[c].min(1.0).sqrt()),
                    0.35,
                )
            }
        };
        Some(mix(old, new, k))
    });
}

fn spin(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    const SAMPLES: usize = 25;
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let centre = frame.source_or_centre(edit);
    let sweep = edit.tone.clamp(0.0, 1.0) * 30.0_f32.to_radians();
    // A sample is read at most `sweep / 2` radians around the centre from where it lands.
    let corner = [
        (rect.x0 as f32, rect.y0 as f32),
        ((rect.x0 + rect.w) as f32, rect.y0 as f32),
        (rect.x0 as f32, (rect.y0 + rect.h) as f32),
        ((rect.x0 + rect.w) as f32, (rect.y0 + rect.h) as f32),
    ]
    .iter()
    .map(|(x, y)| (x - centre[0]).hypot(y - centre[1]))
    .fold(0.0_f32, f32::max);
    let margin = (corner * (sweep * 0.5)).ceil() as usize + 2;
    let Some(source) = area(coverage, frame, margin) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 || sweep <= 0.0 {
            return None;
        }
        let dx = x as f32 + 0.5 - centre[0];
        let dy = y as f32 + 0.5 - centre[1];
        let mut sum = [0.0; 3];
        for i in 0..SAMPLES {
            let a = sweep * (i as f32 / (SAMPLES - 1) as f32 - 0.5);
            let (s, c) = a.sin_cos();
            let p = snapshot.bilinear(centre[0] + dx * c - dy * s, centre[1] + dx * s + dy * c);
            for ch in 0..3 {
                sum[ch] += p[ch];
            }
        }
        Some(mix(old, sum.map(|v| v / SAMPLES as f32), k))
    });
}

fn pixelate(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let block = frame.radius_px(edit).max(2);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // Blocks on a grid anchored at the frame's corner, measured on whole blocks.
    let bx0 = rect.x0 / block * block;
    let by0 = rect.y0 / block * block;
    let bx1 = ((rect.x0 + rect.w).div_ceil(block) * block).min(frame.width);
    let by1 = ((rect.y0 + rect.h).div_ceil(block) * block).min(frame.height);
    let cols = (bx1 - bx0).div_ceil(block);
    let rows = (by1 - by0).div_ceil(block);
    let mut means = vec![[0.0_f32; 3]; cols * rows];
    for (j, row) in means.chunks_mut(cols).enumerate() {
        for (i, mean) in row.iter_mut().enumerate() {
            let (x0, y0) = (bx0 + i * block, by0 + j * block);
            let (x1, y1) = (
                (x0 + block).min(frame.width),
                (y0 + block).min(frame.height),
            );
            let mut n = 0.0;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = (y * frame.width + x) * 3;
                    for c in 0..3 {
                        mean[c] += rgb[p + c];
                    }
                    n += 1.0;
                }
            }
            *mean = mean.map(|v| v / f32::max(n, 1.0));
        }
    }
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let cell = means[((y - by0) / block) * cols + (x - bx0) / block];
        Some(mix(old, cell, k))
    });
}

fn sharpen(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).min(12);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let lum = snapshot.luma_plane();
    let soft = blur(&lum, r.w, r.h, radius);
    let gain = edit.texture.clamp(0.0, 2.0);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        let l = lum[i].max(1e-5);
        let target = (l + gain * k * (l - soft[i])).max(0.0);
        Some(old.map(|v| v * target / l))
    });
}

fn denoise(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).clamp(1, 16);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(b) = Blurred::new(rgb, frame, coverage, radius) else {
        return;
    };
    let r = b.snapshot.rect();
    let lum = b.snapshot.luma_plane();
    let soft_lum = blur(&lum, r.w, r.h, radius);
    // Colour noise: the ratio of each channel to luminance, smoothed fully.
    let ratio: Vec<[f32; 3]> = (0..r.len())
        .map(|i| {
            let x = r.x0 + i % r.w;
            let y = r.y0 + i / r.w;
            let p = b.snapshot.at(x as isize, y as isize);
            let l = lum[i].max(1e-5);
            p.map(|v| v / l)
        })
        .collect();
    let smooth_ratio: [Vec<f32>; 3] = [0, 1, 2].map(|c| {
        blur(
            &ratio.iter().map(|p| p[c]).collect::<Vec<_>>(),
            r.w,
            r.h,
            radius,
        )
    });
    // Luminance noise: a difference small against the local level is noise; a large one is an
    // edge and is kept.
    let sigma = 0.02 + edit.tone.clamp(0.0, 1.0) * 0.12;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = b.snapshot.index(x, y);
        let level = soft_lum[i].max(1e-4);
        let relative = (lum[i] - soft_lum[i]).abs() / level;
        let keep = 1.0 - (-(relative / sigma).powi(2)).exp();
        let l =
            lum[i] + (soft_lum[i] - lum[i]) * (1.0 - keep) * edit.tone.clamp(0.0, 1.0).max(0.25);
        let colour = [smooth_ratio[0][i], smooth_ratio[1][i], smooth_ratio[2][i]];
        let new = colour.map(|c| (c * l).max(0.0));
        Some(mix(old, new, k))
    });
}

fn defringe(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, 6) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let lum = snapshot.luma_plane();
    let soft = blur(&lum, r.w, r.h, 2);
    // Lateral colour fringes sit beside highlights at or near clipping; the edge of a printed
    // pattern on purple cloth does not, however sharp it is.
    let bright = max_filter(&lum, r.w, r.h, 4);
    let green = edit.tint > 0.0;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        let l = luma(old).max(1e-5);
        // Fringes live next to high-contrast edges.
        let edge = smoothstep(0.03, 0.2, (lum[i] - soft[i]).abs() / soft[i].max(1e-4))
            .max(smoothstep(0.05, 0.3, (lum[i] - soft[i]).abs()));
        let [nr, ng, nb] = old.map(|v| v / l);
        let purple = smoothstep(0.05, 0.35, nr.min(nb) - ng);
        let lime = if green {
            smoothstep(0.05, 0.35, ng - nr.max(nb))
        } else {
            0.0
        };
        let fringe = edge * purple.max(lime) * smoothstep(0.55, 0.9, bright[i]);
        if fringe <= 0.0 {
            return None;
        }
        Some(mix(old, [l, l, l], k * fringe))
    });
}

/// The largest value within `radius` of each sample, separably along rows then columns.
fn max_filter(plane: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let along = |src: &[f32], step: usize, len: usize, lines: usize, stride: usize| {
        let mut out = vec![0.0; src.len()];
        for line in 0..lines {
            for k in 0..len {
                let lo = k.saturating_sub(radius);
                let hi = (k + radius).min(len - 1);
                out[line * stride + k * step] = (lo..=hi)
                    .map(|j| src[line * stride + j * step])
                    .fold(f32::MIN, f32::max);
            }
        }
        out
    };
    let rows = along(plane, 1, w, h, w);
    along(&rows, w, h, w, 1)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use aura_recipe::retouch_tools::Tool;

    fn variance(
        rgb: &[f32],
        w: usize,
        xs: std::ops::Range<usize>,
        ys: std::ops::Range<usize>,
    ) -> f32 {
        let values: Vec<f32> = ys
            .flat_map(|y| xs.clone().map(move |x| (x, y)))
            .map(|(x, y)| px(rgb, w, x, y)[1])
            .collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
    }

    #[test]
    fn privacy_blur_and_pixelate_hide_detail_and_sharpen_adds_it() {
        let (w, h) = (60, 60);
        let base = checker(w, h, 2);
        let v0 = variance(&base, w, 20..40, 20..40);
        let mut e = edit(Tool::PrivacyBlur, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.08;
        let mut blurred = base.clone();
        run(&mut blurred, w, h, &e);
        assert!(variance(&blurred, w, 20..40, 20..40) < v0 * 0.2);

        e.tool = Tool::Pixelate;
        e.radius = 0.1;
        let mut blocks = base.clone();
        run(&mut blocks, w, h, &e);
        // Inside one 6 x 6 block every pixel is the same.
        assert_eq!(px(&blocks, w, 24, 24), px(&blocks, w, 29, 29));

        let soft = blurred;
        let vs = variance(&soft, w, 20..40, 20..40);
        let mut e = edit(Tool::LocalSharpen, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.04;
        e.texture = 2.0;
        let mut sharp = soft.clone();
        run(&mut sharp, w, h, &e);
        assert!(variance(&sharp, w, 20..40, 20..40) > vs);
    }

    #[test]
    fn denoise_removes_speckle_and_keeps_a_strong_edge() {
        let (w, h) = (60, 60);
        let noisy = fill(w, h, |x, y| {
            let base = if x < 30 { 0.1 } else { 0.7 };
            let n = super::super::hash(x as i64, y as i64, 7) - 0.5;
            [
                base * (1.0 + n * 0.12),
                base * (1.0 - n * 0.1),
                base * (1.0 + n * 0.15),
            ]
        });
        let mut e = edit(Tool::LocalDenoise, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.03;
        e.tone = 1.0;
        let mut clean = noisy.clone();
        run(&mut clean, w, h, &e);
        assert!(variance(&clean, w, 5..25, 10..50) < variance(&noisy, w, 5..25, 10..50) * 0.5);
        // The edge between the halves is still a step.
        assert!(px(&clean, w, 33, 30)[1] - px(&clean, w, 26, 30)[1] > 0.4);
    }

    #[test]
    fn defringe_leaves_a_purple_pattern_edge_with_no_highlight_beside_it() {
        let (w, h) = (40, 40);
        // Lavender print on purple cloth: a sharp edge, nothing near clipping.
        let base = fill(w, h, |x, _| {
            if (x / 4).is_multiple_of(2) {
                [0.35, 0.18, 0.4]
            } else {
                [0.12, 0.05, 0.15]
            }
        });
        let mut e = edit(Tool::Defringe, [0.5, 0.5, 0.6, 0.6]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert_eq!(out, base);
    }

    #[test]
    fn defringe_neutralises_purple_at_an_edge_and_leaves_flat_purple_alone() {
        let (w, h) = (60, 40);
        let base = fill(w, h, |x, y| {
            if y > 30 {
                [0.4, 0.15, 0.4] // a purple object, no edge nearby
            } else if x < 30 {
                [0.05, 0.05, 0.05]
            } else if x < 33 {
                [0.5, 0.2, 0.55] // fringe on the bright side of an edge
            } else {
                [0.9, 0.9, 0.9]
            }
        });
        let mut e = edit(Tool::Defringe, [0.5, 0.5, 0.6, 0.8]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let fringe = px(&out, w, 31, 15);
        assert!((fringe[0] - fringe[1]).abs() < 0.12, "{fringe:?}");
        assert_eq!(px(&out, w, 45, 36), px(&base, w, 45, 36));
    }

    #[test]
    fn spin_blur_blurs_around_the_centre_not_toward_it() {
        let (w, h) = (80, 80);
        // Rings: a rotational blur leaves them as they are; spokes it smears.
        let rings = fill(w, h, |x, y| {
            let d = (x as f32 + 0.5 - 40.0).hypot(y as f32 + 0.5 - 40.0);
            if (d as usize / 4).is_multiple_of(2) {
                [0.6; 3]
            } else {
                [0.1; 3]
            }
        });
        let spokes = fill(w, h, |x, y| {
            let a = (y as f32 + 0.5 - 40.0).atan2(x as f32 + 0.5 - 40.0);
            if (((a + 4.0) * 4.0) as usize).is_multiple_of(2) {
                [0.6; 3]
            } else {
                [0.1; 3]
            }
        });
        let mut e = edit(Tool::SpinBlur, [0.5, 0.5, 0.45, 0.45]);
        e.tone = 1.0;
        let moved = |base: &[f32]| {
            let mut out = base.to_vec();
            run(&mut out, w, h, &e);
            base.iter()
                .zip(&out)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                / base.len() as f32
        };
        let (ring, spoke) = (moved(&rings), moved(&spokes));
        assert!(ring < spoke * 0.3, "rings {ring} spokes {spoke}");
    }
}
