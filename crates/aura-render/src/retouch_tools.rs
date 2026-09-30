//! Deterministic, explicitly targeted native retouching. No learned segmentation. ADR-0068.
use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::{Edit, Tool};

fn luma(v: [f32; 3]) -> f32 {
    v[0] * 0.2627 + v[1] * 0.6780 + v[2] * 0.0593
}
fn pixel(rgb: &[f32], width: usize, x: usize, y: usize) -> [f32; 3] {
    let i = (y * width + x) * 3;
    [rgb[i], rgb[i + 1], rgb[i + 2]]
}

fn sample(rgb: &[f32], w: usize, h: usize, point: [f32; 2]) -> [f32; 3] {
    let cx = (point[0] * w as f32) as usize;
    let cy = (point[1] * h as f32) as usize;
    let mut sum = [0.0; 3];
    let mut n: f32 = 0.0;
    for y in cy.saturating_sub(2)..=(cy + 2).min(h - 1) {
        for x in cx.saturating_sub(2)..=(cx + 2).min(w - 1) {
            let p = pixel(rgb, w, x, y);
            for c in 0..3 {
                sum[c] += p[c];
            }
            n += 1.0;
        }
    }
    sum.map(|v| v / n.max(1.0))
}

/// Apply normalized operations before crop. Buffers contain linear Rec.2020 RGB.
pub fn apply(rgb: &mut [f32], width: usize, height: usize, edits: &[Edit]) {
    if width == 0 || height == 0 || rgb.len() != width.saturating_mul(height).saturating_mul(3) {
        return;
    }
    for edit in edits.iter().filter(|e| e.enabled && e.amount > 0.0) {
        let coverage = Coverage::new(edit, width, height);
        if matches!(
            edit.tool,
            Tool::SkinSmooth | Tool::SkinUniformity | Tool::PortraitDodgeBurn
        ) {
            crate::retouch_skin::apply(rgb, width, height, edit, &coverage);
        } else if edit.tool == Tool::AutoBlemish {
            auto_spots(rgb, width, height, edit, &coverage);
        } else {
            apply_one(rgb, width, height, edit, &coverage, None);
        }
    }
}

fn apply_one(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    clip: Option<&Coverage>,
) {
    let [cx, cy, rx, ry] = edit.region;
    let radius = (edit.radius * w.min(h) as f32).round().max(1.0) as usize;
    let margin = radius * 12 + 2;
    let [x0, y0, x1, y1] = coverage.bounds;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let bx = x0.saturating_sub(margin);
    let by = y0.saturating_sub(margin);
    let bw = (x1 + margin).min(w) - bx;
    let bh = (y1 + margin).min(h) - by;
    let needs_bands = matches!(
        edit.tool,
        Tool::Frequency
            | Tool::MicroDodgeBurn
            | Tool::Wrinkle
            | Tool::Fabric
            | Tool::Backdrop
            | Tool::EyeDetail
            | Tool::UnderEye
    );
    let mut narrow = Vec::new();
    let mut wide = Vec::new();
    if needs_bands {
        for c in 0..3 {
            let plane: Vec<f32> = (by..by + bh)
                .flat_map(|y| (bx..bx + bw).map(move |x| (y * w + x) * 3 + c))
                .map(|i| rgb[i])
                .collect();
            narrow.push(crate::bands::blur(&plane, bw, bh, radius));
            wide.push(crate::bands::blur(&plane, bw, bh, radius * 3));
        }
    }
    let center = sample(rgb, w, h, [cx, cy]);
    let mut source = edit.source;
    if edit.tool == Tool::Heal && source.is_none() {
        // Fixed-order search among disjoint nearby patches. Never sample over the target.
        let mut best = f32::INFINITY;
        for (dx, dy) in [
            (1., 0.),
            (-1., 0.),
            (0., 1.),
            (0., -1.),
            (0.707, 0.707),
            (-0.707, 0.707),
            (0.707, -0.707),
            (-0.707, -0.707),
        ] {
            let p = [cx + dx * rx * 2.5, cy + dy * ry * 2.5];
            if p[0] < rx || p[0] > 1.0 - rx || p[1] < ry || p[1] > 1.0 - ry {
                continue;
            }
            let candidate = sample(rgb, w, h, p);
            let score = (luma(candidate) - luma(center)).abs();
            if score < best {
                best = score;
                source = Some(p);
            }
        }
    }
    if matches!(edit.tool, Tool::Heal | Tool::Clone) && source.is_none() {
        return;
    }
    let donor_mean = source.map(|p| sample(rgb, w, h, p)).unwrap_or(center);
    // Match donor tone to a ring outside the target, not to the blemish itself.
    let mut ring = [0.0; 3];
    let mut ring_n = 0.0;
    for (dx, dy) in [(1.15, 0.), (-1.15, 0.), (0., 1.15), (0., -1.15)] {
        let p = [(cx + dx * rx).clamp(0., 1.), (cy + dy * ry).clamp(0., 1.)];
        let v = sample(rgb, w, h, p);
        for c in 0..3 {
            ring[c] += v[c];
        }
        ring_n += 1.0;
    }
    let ring = ring.map(|v| v / ring_n);
    let mut patches = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        for x in x0..x1 {
            let a = coverage.at(x, y, w, h)
                * clip.map_or(1.0, |mask| mask.at(x, y, w, h))
                * edit.amount;
            if a <= 0.0 {
                continue;
            }
            let old = pixel(rgb, w, x, y);
            let lum = luma(old).max(0.00001);
            let i = (y - by) * bw + x - bx;
            let low = if needs_bands {
                [narrow[0][i], narrow[1][i], narrow[2][i]]
            } else {
                old
            };
            let broad = if needs_bands {
                [wide[0][i], wide[1][i], wide[2][i]]
            } else {
                old
            };
            let mut value = old;
            match edit.tool {
                Tool::Heal | Tool::Clone => {
                    let Some(p) = source else {
                        continue;
                    };
                    let sx = x as f32 + (p[0] - cx) * w as f32;
                    let sy = y as f32 + (p[1] - cy) * h as f32;
                    if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                        continue;
                    }
                    value = pixel(rgb, w, sx as usize, sy as usize);
                    if edit.tool == Tool::Heal {
                        for c in 0..3 {
                            value[c] += (ring[c] - donor_mean[c]).clamp(-0.15, 0.15);
                        }
                    }
                }
                Tool::Frequency | Tool::Wrinkle | Tool::Fabric => {
                    let amount = if edit.tool == Tool::Wrinkle { 0.5 } else { 1.0 };
                    for c in 0..3 {
                        value[c] = old[c]
                            + edit.tone * amount * (broad[c] - low[c])
                            + (edit.texture - 1.0) * (old[c] - low[c]);
                    }
                }
                Tool::Backdrop => {
                    value = broad;
                }
                Tool::MicroDodgeBurn => {
                    let delta = (luma(broad) - luma(low)).clamp(-0.08, 0.08);
                    value = old.map(|v| v * ((lum + delta).max(0.0) / lum));
                }
                Tool::Dodge => {
                    value = old.map(|v| v * 2.0_f32.powf(0.75));
                }
                Tool::Burn => {
                    value = old.map(|v| v * 2.0_f32.powf(-0.75));
                }
                Tool::UnderEye => {
                    let lift = (luma(broad) - lum).max(0.0).min(0.12);
                    value = old.map(|v| v * (lum + lift) / lum);
                }
                Tool::SkinColor | Tool::Makeup => {
                    value = [
                        old[0] * (1.0 + edit.warmth * 0.25 + edit.tint * 0.12),
                        old[1] * (1.0 - edit.tint * 0.12),
                        old[2] * (1.0 - edit.warmth * 0.25 + edit.tint * 0.12),
                    ];
                    let new_luma = luma(value).max(0.00001);
                    value = value.map(|v| v * lum / new_luma);
                }
                Tool::ColorMatch => {
                    let src_l = luma(donor_mean).max(0.00001);
                    let dst_l = luma(center).max(0.00001);
                    for c in 0..3 {
                        value[c] = old[c] + lum * (donor_mean[c] / src_l - center[c] / dst_l);
                    }
                }
                Tool::Mattify | Tool::Glare => {
                    let threshold = (luma(center) * 0.8).max(0.12);
                    let highlight =
                        ((lum - threshold) / (1.0 - threshold).max(0.1)).clamp(0.0, 1.0);
                    value = old.map(|v| v * (1.0 - highlight * 0.35));
                }
                Tool::Teeth => {
                    // Reduce yellow chroma rather than replacing teeth with flat white.
                    let yellow = ((old[0] + old[1]) * 0.5 - old[2]).max(0.0);
                    value = [
                        old[0] - yellow * 0.15,
                        old[1] - yellow * 0.15,
                        old[2] + yellow * 0.7,
                    ];
                    value = value.map(|v| v * 1.06);
                }
                Tool::EyeClean => {
                    value[0] -= (old[0] - (old[1] + old[2]) * 0.5).max(0.0) * 0.7;
                    let new_luma = luma(value).max(0.00001);
                    value = value.map(|v| v * lum / new_luma);
                }
                Tool::EyeDetail => {
                    for c in 0..3 {
                        value[c] += (old[c] - low[c]) * 0.65;
                    }
                }
                Tool::RedEye => {
                    if old[0] > old[1].max(old[2]) * 1.5 {
                        value[0] = (old[1] + old[2]) * 0.5;
                    }
                }
                Tool::AutoBlemish
                | Tool::SkinSmooth
                | Tool::SkinUniformity
                | Tool::PortraitDodgeBurn => {}
            }
            let out = std::array::from_fn::<_, 3, _>(|c| old[c] + a * (value[c] - old[c]));
            patches.push(((y * w + x) * 3, out));
        }
    }
    for (i, value) in patches {
        rgb[i..i + 3].copy_from_slice(&value);
    }
}

fn auto_spots(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    // Measured spot proposals within an explicit region; not a trained blemish classifier.
    // Painted opacity controls repair strength, not whether a spot can be detected.
    let min_coverage = if edit.mask.is_some() {
        f32::EPSILON
    } else {
        0.9
    };
    let plane: Vec<f32> = rgb
        .chunks_exact(3)
        .map(|p| luma([p[0], p[1], p[2]]))
        .collect();
    let r = ((w.min(h) as f32) * 0.002).round().max(1.0) as usize;
    let low = crate::bands::blur(&plane, w, h, r);
    let mut candidates = Vec::new();
    for y in r * 3..h.saturating_sub(r * 3) {
        for x in r * 3..w.saturating_sub(r * 3) {
            if coverage.at(x, y, w, h) < min_coverage {
                continue;
            }
            let i = y * w + x;
            let d = low[i] - plane[i];
            let edge = (low[i - r] - low[i + r]).abs() + (low[i - r * w] - low[i + r * w]).abs();
            if d > 0.02 + low[i] * 0.15
                && edge < 0.04
                && plane[i] <= plane[i - 1]
                && plane[i] < plane[i + 1]
                && plane[i] <= plane[i - w]
                && plane[i] < plane[i + w]
            {
                candidates.push((d, x, y));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    let mut chosen: Vec<(usize, usize)> = Vec::new();
    for (_, x, y) in candidates {
        if chosen.len() >= 32 {
            break;
        }
        if chosen
            .iter()
            .any(|(px, py)| px.abs_diff(x) + py.abs_diff(y) < r * 6)
        {
            continue;
        }
        let mut spot = edit.clone();
        spot.tool = Tool::Heal;
        spot.source = None;
        spot.mask = None;
        spot.region = [
            (x as f32 + 0.5) / w as f32,
            (y as f32 + 0.5) / h as f32,
            r as f32 * 1.5 / w as f32,
            r as f32 * 1.5 / h as f32,
        ];
        // Stay within the selected area, including the repair edge.
        if coverage.at(x.saturating_sub(r * 2), y, w, h) < min_coverage
            || coverage.at((x + r * 2).min(w - 1), y, w, h) < min_coverage
            || coverage.at(x, y.saturating_sub(r * 2), w, h) < min_coverage
            || coverage.at(x, (y + r * 2).min(h - 1), w, h) < min_coverage
        {
            continue;
        }
        let spot_coverage = Coverage::new(&spot, w, h);
        apply_one(rgb, w, h, &spot, &spot_coverage, Some(coverage));
        chosen.push((x, y));
    }
}
