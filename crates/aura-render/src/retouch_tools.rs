//! Deterministic, explicitly targeted native retouching. ADR-0073.
//!
//! An operation may also carry a segmentation matte (ADR-0082): a person's face skin, body
//! skin, hair or clothes as measured by the bundled segmenter at analysis time and stored in
//! the recipe, so rendering stays deterministic and runs no model.
// Dimensions are validated on entry and every coordinate is clamped to the frame.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use crate::retouch_matte::MattePlane;
use aura_recipe::retouch_tools::{Edit, Matte, Tool};
use std::collections::BTreeMap;

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
    apply_with_mattes(rgb, width, height, edits, &BTreeMap::new());
}

/// Every matte the enabled operations refer to, rendered once from the frame as it was before
/// any of them ran - so every operation that shares a matte selects exactly the same pixels.
fn matte_planes(
    rgb: &[f32],
    width: usize,
    height: usize,
    edits: &[Edit],
    mattes: &BTreeMap<String, Matte>,
) -> BTreeMap<String, Option<MattePlane>> {
    let mut planes = BTreeMap::new();
    for id in edits
        .iter()
        .filter(|e| e.enabled && e.amount > 0.0)
        .filter_map(|e| e.matte.as_ref())
    {
        if !planes.contains_key(id) {
            let plane = mattes
                .get(id)
                .and_then(|m| MattePlane::render(m, rgb, width, height));
            planes.insert(id.clone(), plane);
        }
    }
    planes
}

/// The operation's selection on the current frame, limited by its matte. `None` when the
/// operation names a matte that is missing or selects nothing.
fn coverage_of(
    edit: &Edit,
    width: usize,
    height: usize,
    rgb: &[f32],
    planes: &BTreeMap<String, Option<MattePlane>>,
) -> Option<Coverage> {
    let coverage = Coverage::for_edit(edit, width, height, rgb);
    match &edit.matte {
        None => Some(coverage),
        Some(id) => match planes.get(id) {
            Some(Some(plane)) => Some(coverage.with_matte(plane, width, height)),
            _ => None,
        },
    }
}

/// [`apply`], with the segmentation mattes operations refer to. An operation whose matte is
/// missing or selects nothing is skipped rather than applied to its whole region.
pub fn apply_with_mattes(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    edits: &[Edit],
    mattes: &BTreeMap<String, Matte>,
) {
    if width == 0 || height == 0 || rgb.len() != width.saturating_mul(height).saturating_mul(3) {
        return;
    }
    let planes = matte_planes(rgb, width, height, edits, mattes);
    // A texture graft measures the texture the skin had from the frame as it is now, before
    // any operation has run - not from whatever the operations before it left behind.
    let references: BTreeMap<&str, crate::retouch_texture::Reference> = edits
        .iter()
        .filter(|e| e.enabled && e.amount > 0.0 && e.tool == Tool::TextureGraft)
        .filter_map(|edit| {
            let coverage = coverage_of(edit, width, height, rgb, &planes)?;
            crate::retouch_texture::Reference::capture(rgb, width, height, edit, coverage.bounds)
                .map(|reference| (edit.id.as_str(), reference))
        })
        .collect();
    for edit in edits.iter().filter(|e| e.enabled && e.amount > 0.0) {
        let Some(coverage) = coverage_of(edit, width, height, rgb, &planes) else {
            continue;
        };
        if matches!(
            edit.tool,
            Tool::SkinSmooth | Tool::SkinUniformity | Tool::PortraitDodgeBurn
        ) {
            crate::retouch_skin::apply(rgb, width, height, edit, &coverage);
        } else if edit.tool == Tool::PatchHeal {
            crate::retouch_heal::apply(rgb, width, height, edit, &coverage);
        } else if edit.tool == Tool::FrequencyHeal {
            crate::retouch_clear::apply(rgb, width, height, edit, &coverage);
        } else if edit.tool == Tool::TextureGraft {
            crate::retouch_texture::apply(
                rgb,
                width,
                height,
                edit,
                &coverage,
                references.get(edit.id.as_str()),
            );
        } else if edit.tool == Tool::AutoBlemish {
            auto_spots(rgb, width, height, edit, &coverage);
        } else {
            let refine_edges = edit
                .matte
                .as_ref()
                .and_then(|id| mattes.get(id))
                .is_none_or(|matte| matte.refine_edges);
            apply_one(rgb, width, height, edit, &coverage, None, refine_edges);
        }
    }
}

/// Authored selection coverage, independent of tool strength and detection.
/// Invalid buffer dimensions return an empty mask. Parameters must be validated.
#[must_use]
pub fn selection_mask(rgb: &[f32], width: usize, height: usize, edit: &Edit) -> Vec<f32> {
    selection_mask_with_mattes(rgb, width, height, edit, &BTreeMap::new())
}

/// [`selection_mask`], with the segmentation mattes the operation may refer to.
#[must_use]
pub fn selection_mask_with_mattes(
    rgb: &[f32],
    width: usize,
    height: usize,
    edit: &Edit,
    mattes: &BTreeMap<String, Matte>,
) -> Vec<f32> {
    if width == 0 || height == 0 || rgb.len() != width.saturating_mul(height).saturating_mul(3) {
        return Vec::new();
    }
    let mut coverage = Coverage::for_edit(edit, width, height, rgb);
    if let Some(id) = &edit.matte {
        match mattes
            .get(id)
            .and_then(|m| MattePlane::render(m, rgb, width, height))
        {
            Some(plane) => coverage = coverage.with_matte(&plane, width, height),
            None => return vec![0.0; width * height],
        }
    }
    // Sampled skin tools change only skin like the sample (and, when asked, connected to
    // it); show exactly that, so the preview proves no background is selected.
    if matches!(
        edit.tool,
        Tool::SkinSmooth | Tool::SkinUniformity | Tool::PortraitDodgeBurn
    ) && edit.source.is_some()
    {
        return crate::retouch_skin::selection(rgb, width, height, edit, &coverage);
    }
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| coverage.at(x, y, width, height))
        .collect()
}

/// Where frequency healing would rebuild tone: 0 for untouched skin, 1 inside a compact
/// mark, with the feathered rim in between. Measured on the given frame exactly as rendering
/// would, so a preview of it is a preview of the repair. Zero for any other tool.
#[must_use]
pub fn frequency_heal_marks(
    rgb: &[f32],
    width: usize,
    height: usize,
    edit: &Edit,
    mattes: &BTreeMap<String, Matte>,
) -> Vec<f32> {
    if width == 0 || height == 0 || rgb.len() != width.saturating_mul(height).saturating_mul(3) {
        return Vec::new();
    }
    if edit.tool != Tool::FrequencyHeal {
        return vec![0.0; width * height];
    }
    let planes = matte_planes(rgb, width, height, std::slice::from_ref(edit), mattes);
    coverage_of(edit, width, height, rgb, &planes).map_or_else(
        || vec![0.0; width * height],
        |coverage| crate::retouch_clear::mark_plane(rgb, width, height, edit, &coverage),
    )
}

#[allow(clippy::too_many_lines)]
fn apply_one(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    edit: &Edit,
    coverage: &Coverage,
    clip: Option<&Coverage>,
    refine_matte_edges: bool,
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
    let mut fine = Vec::new();
    let separate_pores = edit.tool == Tool::Frequency && edit.preserve_microtexture;
    // A protected surface finish averages only selected skin. Hair, lips and eyes
    // must not darken the blur near its boundary or be selected by a colour gate.
    let weights: Option<Vec<f32>> = (!refine_matte_edges).then(|| {
        (by..by + bh)
            .flat_map(|y| (bx..bx + bw).map(move |x| coverage.at(x, y, w, h)))
            .collect()
    });
    let weighted_blur = |plane: &[f32], r: usize| {
        if let Some(weights) = &weights {
            let values: Vec<_> = plane.iter().zip(weights).map(|(v, a)| v * a).collect();
            let values = crate::bands::blur(&values, bw, bh, r);
            let blurred_weights = crate::bands::blur(weights, bw, bh, r);
            values
                .iter()
                .zip(blurred_weights)
                .zip(plane)
                .map(|((v, a), old)| if a > 1e-5 { v / a } else { *old })
                .collect()
        } else {
            crate::bands::blur(plane, bw, bh, r)
        }
    };
    if needs_bands {
        for c in 0..3 {
            let plane: Vec<f32> = (by..by + bh)
                .flat_map(|y| (bx..bx + bw).map(move |x| (y * w + x) * 3 + c))
                .map(|i| rgb[i])
                .collect();
            narrow.push(weighted_blur(&plane, radius));
            wide.push(weighted_blur(&plane, radius * 3));
            if separate_pores {
                fine.push(weighted_blur(&plane, (radius / 5).max(1)));
            }
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
    let donor_mean = source.map_or(center, |p| sample(rgb, w, h, p));
    // Segmented operations on skin and hair skip much darker structures inside the matte.
    // Clothes and backdrops keep their own shadows and are not guarded.
    let guard_reference = (refine_matte_edges
        && edit.matte.is_some()
        && !matches!(
            edit.tool,
            Tool::Fabric | Tool::Backdrop | Tool::Heal | Tool::Clone
        ))
    .then_some(donor_mean);
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
            let mut a = coverage.at(x, y, w, h)
                * clip.map_or(1.0, |mask| mask.at(x, y, w, h))
                * edit.amount;
            if a <= 0.0 {
                continue;
            }
            if let Some(reference) = guard_reference {
                // A matte is coarser than a lash or a beard hair; never work on pixels far
                // darker than the region's own reference.
                let ratio = luma(pixel(rgb, w, x, y)) / luma(reference).max(1e-6);
                let t = ((ratio - 0.3) / 0.2).clamp(0.0, 1.0);
                a *= t * t * (3.0 - 2.0 * t);
            }
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
                        value[c] = if separate_pores {
                            // Keep the real fine-detail band while attenuating larger
                            // irregularities. Both corrections have zero response to
                            // constant colour and preserve broad face illumination.
                            old[c] + edit.tone * (broad[c] - low[c])
                                - edit.tone * 0.8 * (fine[c][i] - low[c])
                                + (edit.texture - 1.0) * (old[c] - fine[c][i])
                        } else {
                            old[c]
                                + edit.tone * amount * (broad[c] - low[c])
                                + (edit.texture - 1.0) * (old[c] - low[c])
                        };
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
                    let lift = (luma(broad) - lum).clamp(0.0, 0.12);
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
                    // Red-eye red has green and blue about equal. Brown irises, skin and lips
                    // have much less blue than green and are left alone.
                    if old[0] > old[1].max(old[2]) * 1.5 && old[2] >= old[1] * 0.5 {
                        value[0] = (old[1] + old[2]) * 0.5;
                    }
                }
                Tool::PatchHeal
                | Tool::FrequencyHeal
                | Tool::TextureGraft
                | Tool::AutoBlemish
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
    let min_coverage = if edit.matte.is_some() {
        // Well inside the segmented skin, so a repair never reaches across its edge.
        0.5
    } else if edit.mask.is_some() {
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
        apply_one(rgb, w, h, &spot, &spot_coverage, Some(coverage), true);
        chosen.push((x, y));
    }
}
