//! Multiscale spot repair over segmented skin. No learned acne/permanent-mark claim.
//! Donors and complete repair disks must fit inside skin with feature exclusions.
//! Small enclosed mask holes are filled before exclusions, so a dark spot cannot
//! exclude its own repair. Each accepted proposal becomes an editable native heal.
#![allow(clippy::indexing_slicing, clippy::too_many_arguments)]

use super::{base_edit, distance, FeatureEdits, Geometry, Pixels, Settings, Tool};
use aura_vision::{portrait::PortraitFace, skin::Matte};

#[derive(Clone, Debug)]
struct Spot {
    x: f32,
    y: f32,
    radius: f32,
    score: f32,
    red: f32,
}

/// A dense pattern is ambiguous; retain its weak colour departures while still allowing
/// distinctly red, compact candidates to be reviewed as repairs. Darkness never grants this
/// exception, and redness is a departure from the same person's surrounding skin. ADR-0096.
fn protect_dense_pattern(spots: &mut Vec<&Spot>, settings: &Settings) -> usize {
    if !settings.keep_freckles || settings.remove_dark_marks || spots.len() <= super::FRECKLE_FIELD
    {
        return 0;
    }
    let before = spots.len();
    spots.retain(|s| s.red >= 0.025);
    before - spots.len()
}

// Deterministic four-connected components, including zero-valued mask holes.
pub(super) fn components(mask: &[bool], w: usize, h: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; mask.len()];
    let mut out = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut group = vec![start];
        let mut next = 0;
        while next < group.len() {
            let i = group[next];
            next += 1;
            let (x, y) = (i % w, i / w);
            for j in [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then_some(i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then_some(i + w),
            ]
            .into_iter()
            .flatten()
            {
                if mask[j] && !seen[j] {
                    seen[j] = true;
                    group.push(j);
                }
            }
        }
        out.push(group);
    }
    out
}

fn close_holes(mask: &mut [bool], w: usize, h: usize, radius: f32) {
    let holes: Vec<_> = mask.iter().map(|v| !v).collect();
    for group in components(&holes, w, h) {
        if group.len() as f32 > std::f32::consts::PI * radius * radius {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        let mut edge = false;
        for &i in &group {
            let (x, y) = (i % w, i / w);
            edge |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if !edge && (x1 - x0).max(y1 - y0) as f32 <= radius * 2.0 {
            for i in group {
                mask[i] = true;
            }
        }
    }
}

// Eight-neighbour chamfer distance to a protected pixel or the image boundary.
// A square fits inside this distance / sqrt(2), a disk inside distance / 1.08.
fn clearance(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    let mut d = vec![0.0_f32; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            if mask[i] {
                d[i] = (d[i - 1] + 1.0)
                    .min(d[i - w] + 1.0)
                    .min(d[i - w - 1] + std::f32::consts::SQRT_2)
                    .min(d[i - w + 1] + std::f32::consts::SQRT_2);
            }
        }
    }
    for y in (1..h.saturating_sub(1)).rev() {
        for x in (1..w.saturating_sub(1)).rev() {
            let i = y * w + x;
            if mask[i] {
                d[i] = d[i]
                    .min(d[i + 1] + 1.0)
                    .min(d[i + w] + 1.0)
                    .min(d[i + w + 1] + std::f32::consts::SQRT_2)
                    .min(d[i + w - 1] + std::f32::consts::SQRT_2);
            }
        }
    }
    d
}

fn disk(mask: &mut [bool], w: usize, h: usize, x: f32, y: f32, r: f32) {
    let x0 = (x - r).floor().max(0.0) as usize;
    let y0 = (y - r).floor().max(0.0) as usize;
    for yy in y0..((y + r).ceil() as usize + 1).min(h) {
        for xx in x0..((x + r).ceil() as usize + 1).min(w) {
            if (xx as f32 - x).hypot(yy as f32 - y) <= r {
                mask[yy * w + xx] = false;
            }
        }
    }
}

fn candidates(
    lum: &[f32],
    red: &[f32],
    mask: &[bool],
    w: usize,
    h: usize,
    eye_distance: f32,
    sensitivity: f32,
) -> (Vec<Spot>, Vec<f32>, Vec<f32>) {
    let scale = (eye_distance / 250.0).clamp(0.25, 4.0);
    let fine_r = (scale * 0.8).round().max(1.0) as usize;
    let fine = aura_render::bands::blur(lum, w, h, fine_r);
    let broad = aura_render::bands::blur(lum, w, h, (scale * 9.0).round().max(3.0) as usize);
    let fine_red = aura_render::bands::blur(red, w, h, fine_r);
    let broad_red = aura_render::bands::blur(red, w, h, (scale * 9.0).round().max(3.0) as usize);
    let redness: Vec<_> = fine_red
        .iter()
        .zip(&broad_red)
        .map(|(a, b)| a - b)
        .collect();
    let inside = clearance(mask, w, h);
    let threshold = 0.06 - sensitivity * 0.035;
    let mut spots = Vec::new();
    for radius in [4.0_f32, 8.0, 13.0] {
        let bg = aura_render::bands::blur(lum, w, h, (radius * scale).round().max(2.0) as usize);
        let score: Vec<_> = (0..w * h)
            .map(|i| {
                let dark = (bg[i] - fine[i]) / (bg[i] + 0.08);
                // Isolated bright pores are texture, not acne. A bright head must
                // also carry local redness to consume a repair slot.
                dark.max(0.0)
                    + (-dark).max(0.0).min(redness[i].max(0.0) * 4.0)
                    + redness[i].max(0.0) * 1.5
            })
            .collect();
        // Higher thresholds split touching marks that merge at the sensitive threshold.
        for tier in [1.0, 1.8, 2.8] {
            let selected: Vec<_> = (0..w * h)
                .map(|i| mask[i] && score[i] > threshold * tier)
                .collect();
            for group in components(&selected, w, h) {
                let area = group.len() as f32;
                if area < (9.0 * scale * scale).max(3.0) || area > 450.0 * scale * scale {
                    continue;
                }
                let x = group.iter().map(|i| (i % w) as f32).sum::<f32>() / area;
                let y = group.iter().map(|i| (i / w) as f32).sum::<f32>() / area;
                let (mut xx, mut yy, mut xy, mut reach, mut peak, mut rmax) =
                    (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
                for &i in &group {
                    let dx = (i % w) as f32 - x;
                    let dy = (i / w) as f32 - y;
                    xx += dx * dx;
                    yy += dy * dy;
                    xy += dx * dy;
                    reach = reach.max(dx.hypot(dy));
                    peak = peak.max(score[i]);
                    rmax = rmax.max(redness[i]);
                }
                let half = (xx + yy) * 0.5 / area;
                let root = (((xx - yy) * 0.5).powi(2) + xy * xy).sqrt() / area;
                if (half + root) / (half - root).max(0.2) > 5.5 || reach > 18.0 * scale {
                    continue;
                }
                // Keep the measured lesion inside the fully repaired core; the
                // feather belongs on surrounding healthy skin, not on the acne rim.
                let repair = (reach * 1.4 + 1.5 * scale).max(3.5 * scale);
                let at = y.round() as usize * w + x.round() as usize;
                // A dark halo around an isolated bright pore is not a dark lesion.
                if (fine[at] > bg[at] && redness[at] <= 0.003)
                    || inside[at] < repair * 1.1 + 1.0
                    || peak < threshold * 1.25
                {
                    continue;
                }
                // Large repairs first within a severity tier; suppress duplicate scales below.
                spots.push(Spot {
                    x,
                    y,
                    radius: repair,
                    score: peak * area.powf(0.75),
                    red: rmax,
                });
            }
        }
    }
    spots.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.y.total_cmp(&b.y))
            .then(a.x.total_cmp(&b.x))
    });
    let mut unique: Vec<Spot> = Vec::new();
    for spot in spots {
        if unique.iter().all(|other| {
            distance([spot.x, spot.y], [other.x, other.y]) > (spot.radius + other.radius) * 0.75
        }) {
            unique.push(spot);
        }
    }
    (unique, broad, broad_red)
}

fn donor(
    spot: &Spot,
    clean: &[f32],
    inside: &[f32],
    broad: &[f32],
    red: &[f32],
    w: usize,
    h: usize,
) -> Option<([f32; 2], f32)> {
    let at = spot.y.round() as usize * w + spot.x.round() as usize;
    for scale in [1.0, 0.7, 0.5, 0.35, 0.25] {
        let source_radius = spot.radius * scale;
        let mut best = None;
        let mut best_cost = f32::INFINITY;
        for reach in [3.0, 4.5, 6.0, 9.0, 13.0, 18.0, 24.0] {
            for n in 0..32 {
                let angle = std::f32::consts::TAU * n as f32 / 32.0;
                let x = (spot.x + angle.cos() * spot.radius * reach).round();
                let y = (spot.y + angle.sin() * spot.radius * reach).round();
                if x < 1.0 || y < 1.0 || x >= w as f32 - 1.0 || y >= h as f32 - 1.0 {
                    continue;
                }
                let i = y as usize * w + x as usize;
                // Harmonic healing samples a rectangular donor plus a surrounding ring.
                if inside[i] < source_radius * 1.55 + 2.0 || clean[i] < source_radius * 1.55 + 1.0 {
                    continue;
                }
                // The full source square must be clean, including reflected corners
                // when a smaller source supplies texture to a larger target.
                let mut contaminated = 0;
                for sy in -3..=3 {
                    for sx in -3..=3 {
                        let dx = (x + sx as f32 * source_radius / 3.0).round() as usize;
                        let dy = (y + sy as f32 * source_radius / 3.0).round() as usize;
                        if clean[dy * w + dx] < 1.0 {
                            contaminated += 1;
                        }
                    }
                }
                if contaminated > 0 {
                    continue;
                }
                let cost = ((broad[i] - broad[at]) / (broad[at] + 0.08)).abs()
                    + (red[i] - red[at]).abs() * 2.0
                    + reach * 0.002
                    + contaminated as f32 * 0.025;
                if cost < best_cost {
                    best_cost = cost;
                    best = Some([x, y]);
                }
            }
        }
        if let Some(point) = best.filter(|_| best_cost < 0.45) {
            return Some((point, scale));
        }
    }
    None
}

// Landmark mouth corners can sit outside the visible lip. Inset the capsule's
// segment before adding its lip-height radius, rather than protecting a cheek-wide
// circle around each corner. This still covers the original corner coordinates.
fn lip_protection(g: &Geometry) -> super::Capsule {
    super::Capsule {
        a: super::add(super::add(g.mouth[0], g.u, g.d * 0.10), g.v, g.d * 0.08),
        b: super::add(super::add(g.mouth[1], g.u, -g.d * 0.10), g.v, g.d * 0.08),
        r: g.d * 0.18,
    }
}

pub(crate) fn plan(
    face: &PortraitFace,
    index: usize,
    px: &Pixels<'_>,
    prefix: &str,
    settings: &Settings,
    matte: Option<&Matte>,
) -> FeatureEdits {
    let mut out = FeatureEdits::default();
    let Some(g) = Geometry::new(face, px) else {
        return out;
    };
    let Some(matte) = matte else {
        out.report
            .findings
            .push("Deep blemish cleanup skipped: detect face skin first.".into());
        return out;
    };
    if g.d < 40.0 {
        return out;
    }
    let [l, t, r, b] = matte.bounds;
    let x0 = (l * px.width as f32).floor().max(0.0) as usize;
    let y0 = (t * px.height as f32).floor().max(0.0) as usize;
    let x1 = ((r * px.width as f32).ceil() as usize).min(px.width);
    let y1 = ((b * px.height as f32).ceil() as usize).min(px.height);
    if x1 <= x0 + 4 || y1 <= y0 + 4 {
        return out;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut mask = vec![false; w * h];
    let mut donor_mask = vec![false; w * h];
    let mut lum = vec![0.0_f32; w * h];
    let mut red = vec![0.0_f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let p = px.encoded(x + x0, y + y0);
            let i = y * w + x;
            lum[i] = p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722;
            red[i] = (p[0] - (p[1] + p[2]) * 0.5) / (p[0] + p[1] + p[2] + 0.05);
            let confidence = matte.at(
                (x + x0) as f32 / px.width as f32,
                (y + y0) as f32 / px.height as f32,
            );
            mask[i] = confidence > 0.20;
            donor_mask[i] = confidence > 0.55;
        }
    }
    close_holes(&mut mask, w, h, g.d * 0.15);
    let mut exclusions = g.exclusions();
    if let Some(lips) = exclusions.get_mut(6) {
        *lips = lip_protection(&g);
    }
    for y in 0..h {
        for x in 0..w {
            let point = [(x + x0) as f32 + 0.5, (y + y0) as f32 + 0.5];
            if exclusions.iter().any(|e| e.contains(point))
                || (super::eye_guard::feature_weight(&g, point, settings) < 1.0)
            {
                mask[y * w + x] = false;
            }
        }
    }
    let (spots, broad, broad_red) =
        candidates(&lum, &red, &mask, w, h, g.d, settings.blemish_sensitivity);
    // Reserve every detected mark from donor selection, including marks deliberately kept.
    let mut clean = mask.clone();
    let fine = aura_render::bands::blur(&lum, w, h, (g.d / 250.0).round().max(1.0) as usize);
    for i in 0..clean.len() {
        // Also reject bright heads and irregular clusters excluded from the compact
        // candidate list. Otherwise a missed lesion can be copied into healthy skin.
        let contrast = (fine[i] - broad[i]).abs() / (broad[i] + 0.08);
        let redness = (red[i] - broad_red[i]).max(0.0);
        let raw_contrast = (lum[i] - broad[i]).abs() / (broad[i] + 0.08);
        clean[i] &= donor_mask[i] && contrast < 0.085 && raw_contrast < 0.20 && redness < 0.025;
    }
    for spot in &spots {
        disk(&mut clean, w, h, spot.x, spot.y, spot.radius * 0.8);
    }
    let clean = clearance(&clean, w, h);
    let inside = clearance(&mask, w, h);
    let mut no_donor = 0;
    let red_threshold = 0.012 - settings.blemish_sensitivity * 0.006;
    let mut eligible: Vec<_> = spots
        .iter()
        .filter(|s| {
            let keep = !settings.remove_dark_marks && s.red < red_threshold;
            if keep {
                out.report.marks_kept += 1;
            }
            !keep
        })
        .collect();
    let protected = protect_dense_pattern(&mut eligible, settings);
    if protected > 0 {
        out.report.marks_kept += protected;
        out.report.findings.push(format!(
            "Deep cleanup: kept {protected} ambiguous marks in a dense pattern; only {} distinctly red compact candidates remain for repair. Natural dark marks stay protected; review the proposed repairs.",
            eligible.len()
        ));
    }
    for spot in eligible {
        if out.blemishes.len() >= usize::from(settings.max_spots).min(220) {
            break;
        }
        let Some((source, source_scale)) = donor(spot, &clean, &inside, &broad, &broad_red, w, h)
        else {
            no_donor += 1;
            continue;
        };
        let centre = [spot.x + x0 as f32 + 0.5, spot.y + y0 as f32 + 0.5];
        // After frequency healing, repair only the remaining defect. A full replacement
        // here can flatten already-corrected skin and leave a circular texture boundary.
        let residual = settings.frequency_heal > 0.0;
        let mut edit = base_edit(
            format!("{prefix}{index}-spot-deep-{}", out.blemishes.len()),
            Tool::PatchHeal,
            if residual { 0.65 } else { 1.0 },
            px,
            [centre[0], centre[1], spot.radius, spot.radius],
        );
        edit.source = Some([
            (source[0] + x0 as f32 + 0.5) / px.width as f32,
            (source[1] + y0 as f32 + 0.5) / px.height as f32,
        ]);
        edit.feather = if residual { 0.65 } else { 0.25 };
        edit.texture_heal = true;
        edit.source_scale = source_scale;
        // No coarse matte: the entire disk was checked against the repaired skin mask.
        // Applying the unfilled matte here would protect the centre of a dark blemish.
        out.blemishes.push(edit);
    }
    out.report.spots_healed = out.blemishes.len();
    out.report.findings.push(format!("Deep blemish cleanup: {} local-light-matched texture repairs across segmented face skin; {} marks kept, {} spots skipped without a clean donor. {}",
        out.report.spots_healed,out.report.marks_kept,no_donor,
        if settings.remove_dark_marks {"Dark-mark removal enabled; review freckles and beauty marks."} else {"Dark marks protected."}));
    out
}

/// Fill notches in the selection's outline that are narrower than `2 * radius`, where the
/// photograph itself looks like this person's skin.
///
/// A segmenter leaves a dark mark at the edge of a face out of the skin, and a mark that is
/// not in the selection can never be repaired. A morphological closing puts back exactly the
/// cells a notch removed; `allowed` keeps it from reaching hair or background in a genuine
/// concavity of the outline.
fn close_notches(mask: &mut [bool], w: usize, h: usize, radius: f32, allowed: &[bool]) {
    let outside: Vec<bool> = mask.iter().map(|v| !v).collect();
    // For an unselected cell: distance to the selection (or the grid's edge).
    let near = clearance(&outside, w, h);
    let grown: Vec<bool> = mask
        .iter()
        .zip(&near)
        .map(|(on, d)| *on || *d <= radius)
        .collect();
    // For a grown cell: distance back out of the grown region.
    let depth = clearance(&grown, w, h);
    for ((cell, depth), allowed) in mask.iter_mut().zip(depth).zip(allowed) {
        if !*cell && depth > radius && *allowed {
            *cell = true;
        }
    }
}

/// Extend the selection into `candidates` that touch it, at most `steps` cells outward.
fn grow(mask: &mut [bool], w: usize, h: usize, candidates: &[bool], steps: usize) {
    let mut frontier: Vec<usize> = (0..mask.len()).filter(|i| mask[*i]).collect();
    for _ in 0..steps {
        let mut next = Vec::new();
        for i in frontier {
            let (x, y) = (i % w, i / w);
            for j in [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then_some(i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then_some(i + w),
            ]
            .into_iter()
            .flatten()
            {
                if !mask[j] && candidates[j] {
                    mask[j] = true;
                    next.push(j);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
}

/// Mean linear colour of the photograph under every cell of a matte grid.
fn cell_colours(px: &Pixels<'_>, bounds: [f32; 4], w: usize, h: usize) -> Vec<[f32; 3]> {
    let [l, t, r, b] = bounds;
    let step_x = (r - l) * px.width as f32 / w as f32;
    let step_y = (b - t) * px.height as f32 / h as f32;
    (0..w * h)
        .map(|i| {
            let cx = l * px.width as f32 + ((i % w) as f32 + 0.5) * step_x;
            let cy = t * px.height as f32 + ((i / w) as f32 + 0.5) * step_y;
            let mut sum = [0.0_f32; 3];
            for (dx, dy) in [
                (0.0, 0.0),
                (-0.3, -0.3),
                (0.3, -0.3),
                (-0.3, 0.3),
                (0.3, 0.3),
            ] {
                let p = px.linear(
                    (cx + dx * step_x).max(0.0) as usize,
                    (cy + dy * step_y).max(0.0) as usize,
                );
                for c in 0..3 {
                    sum[c] += p[c] * 0.2;
                }
            }
            sum
        })
        .collect()
}

fn median(values: Vec<f32>) -> Option<f32> {
    quantile(values, 0.5)
}

/// The value a share `q` of `values` lies below.
fn quantile(mut values: Vec<f32>, q: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let at = ((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    let (_, value, _) = values.select_nth_unstable_by(at, f32::total_cmp);
    Some(*value)
}

/// The continuous, feature-protected face-skin selection every deep finishing operation
/// shares: a feathered alpha per matte cell.
///
/// Small enclosed holes and narrow skin-like notches in the segmenter's outline are filled, so
/// a dark mark the segmenter left out is inside the selection, and skin-coloured cells inside
/// the face's oval that touch it are added, so shadowed skin the segmenter missed is too. Eyes,
/// the nose tip and lips are removed by landmark. A brow is removed where it is: the cells
/// inside the brow area that are clearly darker than the skin around them, plus a narrow band
/// along the brow line for brows that are no darker than the skin - so a forehead above the
/// brow, lit or in shadow, is not left as an untreated patch.
fn surface_selection(g: &Geometry, px: &Pixels<'_>, matte: &Matte, nose: bool) -> Option<Vec<u8>> {
    let [l, t, r, b] = matte.bounds;
    let (w, h) = (matte.width, matte.height);
    if w < 3 || h < 3 {
        return None;
    }
    let cell = (((r - l) * px.width as f32 / w as f32).max((b - t) * px.height as f32 / h as f32))
        .max(1.0);
    let mut selected: Vec<_> = matte.alpha.iter().map(|a| *a > 64).collect();
    let colours = cell_colours(px, matte.bounds, w, h);
    let warmth = |p: [f32; 3]| (p[0] - p[2]) / (p[0] + p[1] + p[2] + 1e-4);
    let confident = |f: &dyn Fn([f32; 3]) -> f32| {
        median(
            colours
                .iter()
                .zip(&selected)
                .filter(|(_, on)| **on)
                .map(|(p, _)| f(*p))
                .collect(),
        )
    };
    let skin_luma = confident(&super::luma)?;
    let skin_warmth = confident(&warmth)?;
    let value = |p: [f32; 3]| p[0].max(p[1]).max(p[2]);
    let skin_value = confident(&value)?;
    close_holes(&mut selected, w, h, g.d * 0.30 / cell);
    // A mark on the shadow side of a face can be a tenth as bright as the lit skin, so
    // brightness alone cannot tell it from hair. Warmth can: measured on a dark-skinned
    // portrait, shadowed marks kept 80 % of the skin's red-over-blue share, black hair a
    // quarter of it and a grey backdrop none.
    let skin_like: Vec<bool> = colours
        .iter()
        .map(|p| super::luma(*p) > skin_luma * 0.05 && warmth(*p) > skin_warmth * 0.6)
        .collect();
    close_notches(&mut selected, w, h, g.d * 0.16 / cell, &skin_like);
    // The segmenter is unsure of skin in deep shadow. Where it saw some evidence of skin, the
    // photograph is skin-coloured and the cell touches confident skin, it is the same face.
    let hinted: Vec<bool> = matte
        .alpha
        .iter()
        .zip(&skin_like)
        .map(|(a, like)| *a > 12 && *like)
        .collect();
    grow(&mut selected, w, h, &hinted, (g.d * 0.35 / cell) as usize);
    let centre = |i: usize| {
        [
            (l + ((i % w) as f32 + 0.5) / w as f32 * (r - l)) * px.width as f32,
            (t + ((i / w) as f32 + 0.5) / h as f32 * (b - t)) * px.height as f32,
        ]
    };
    // Where the segmenter saw no skin at all - a band of shadowed cheek and jaw, which on
    // darker skin can be most of one side of a face - only the photograph can say it is skin.
    // Inside the oval the detector drew around the face, a skin-coloured cell that touches the
    // selection is the same face; outside it, the same colour is a neck or an ear.
    let [fl, ft, fr, fb] = g.bounds;
    let oval_centre = [(fl + fr) * 0.5, (ft + fb) * 0.5];
    let oval_axes = [((fr - fl) * 0.5).max(1.0), ((fb - ft) * 0.5).max(1.0)];
    let in_face: Vec<bool> = (0..w * h)
        .map(|i| {
            let [x, y] = centre(i);
            skin_like[i]
                && ((x - oval_centre[0]) / oval_axes[0]).powi(2)
                    + ((y - oval_centre[1]) / oval_axes[1]).powi(2)
                    <= 1.0
        })
        .collect();
    grow(&mut selected, w, h, &in_face, (g.d * 0.35 / cell) as usize);
    // Creases are protected during spot replacement, but a continuous surface
    // finish must span them: otherwise they become conspicuous untreated strips.
    // A smoothing finish leaves the nose tip alone. Acne clear works on the nose too - its
    // marks are as visible as anybody's cheek - and leaves only the nostrils out.
    let mut exclusions = vec![if nose {
        nostrils(g)
    } else {
        super::Capsule::disk(super::add(g.nose, g.v, g.d * 0.06), g.d * 0.20)
    }];
    let mut brow_areas = Vec::with_capacity(2);
    for eye in g.eyes {
        exclusions.push(super::Capsule::disk(eye, g.d * 0.25));
        let brow = super::add(eye, g.v, -g.d * 0.32);
        // Acne clear tells a mark from a brow hair itself - hair is darker and not redder, and
        // long and thin - so its selection leaves out only the brow hair actually there, and
        // the marks just above and between the brows are repaired too.
        if !nose {
            exclusions.push(super::Capsule {
                a: super::add(brow, g.u, -g.d * 0.28),
                b: super::add(brow, g.u, g.d * 0.28),
                r: g.d * 0.085,
            });
        }
        brow_areas.push(super::Capsule::disk(brow, g.d * 0.24));
    }
    exclusions.push(lip_protection(g));
    // Brow hair wherever it actually is, with a margin of two cells or 4 % of the eye distance.
    // Hair is darker than the skin right around it - not than this face's typical skin: on
    // the shadowed side of a face the whole forehead is darker than that, and is still skin.
    let reach = (g.d * 0.2 / cell).max(2.0) as usize;
    let local = |i: usize, f: &dyn Fn([f32; 3]) -> f32, fallback: f32| {
        let (x, y) = (i % w, i / w);
        let mut around = Vec::with_capacity((2 * reach + 1).pow(2));
        for row in y.saturating_sub(reach)..(y + reach + 1).min(h) {
            for j in row * w + x.saturating_sub(reach)..row * w + (x + reach + 1).min(w) {
                if skin_like[j] {
                    around.push(f(colours[j]));
                }
            }
        }
        quantile(around, 0.6).unwrap_or(fallback)
    };
    let local_skin = |i: usize| local(i, &super::luma, skin_luma);
    let not_brow: Vec<bool> = (0..w * h)
        .map(|i| {
            !(brow_areas.iter().any(|area| area.contains(centre(i)))
                && super::luma(colours[i]) < local_skin(i) * 0.62)
        })
        .collect();
    let to_brow = clearance(&not_brow, w, h);
    let brow_margin = (g.d * 0.04 / cell).max(2.0);
    // The nostrils wherever they actually are. The shapes above sit where the landmarks put the
    // nose, and on a turned or tilted face a nostril can lie outside them; a smoothing finish
    // that reaches one fills the opening with skin tone. An opening is a shadow, so every
    // channel is far below the nose skin right around it. An inflamed mark can be as dark in
    // luminance - it loses its green - but keeps its red, so the test is on the brightest
    // channel: under a fifth of the local skin's, in linear light, is an opening (about half
    // its sRGB code value), and is left out with a margin.
    // Only across the base of the nose, where nostrils are, however far to the side the face
    // has turned them: the side of the nose above it can be as deep in shadow, and its marks
    // are acne clear's to repair.
    let base = super::add(g.nose, g.v, g.d * 0.12);
    let nose_area = super::Capsule {
        a: super::add(base, g.u, -g.d * 0.35),
        b: super::add(base, g.u, g.d * 0.35),
        r: g.d * 0.15,
    };
    let not_nostril: Vec<bool> = (0..w * h)
        .map(|i| {
            !(nose_area.contains(centre(i))
                && value(colours[i]) < local(i, &value, skin_value) * 0.2)
        })
        .collect();
    let to_nostril = clearance(&not_nostril, w, h);
    let nostril_margin = (g.d * 0.05 / cell).max(2.0);
    for i in 0..w * h {
        let point = centre(i);
        let on_brow = to_brow[i] <= brow_margin && brow_areas.iter().any(|a| a.contains(point));
        let on_nostril = to_nostril[i] <= nostril_margin;
        if on_brow || on_nostril || exclusions.iter().any(|e| e.contains(point)) {
            selected[i] = false;
        }
    }
    let distance = clearance(&selected, w, h);
    let feather = (g.d * 0.08 / cell).max(2.0);
    Some(
        distance
            .iter()
            .map(|d| {
                let v = ((d - 1.0) / feather).clamp(0.0, 1.0);
                (v * v * (3.0 - 2.0 * v) * 255.0).round() as u8
            })
            .collect(),
    )
}

/// The recipe matte for [`surface_selection`], stored once per face and shared by the
/// frequency heal, the surface finish and the texture graft.
pub(crate) fn surface_matte(
    face: &PortraitFace,
    px: &Pixels<'_>,
    matte: &Matte,
) -> Option<aura_recipe::retouch_tools::Matte> {
    let g = Geometry::new(face, px)?;
    if g.d < 40.0 {
        return None;
    }
    let alpha = surface_selection(&g, px, matte, false)?;
    let mut mask = aura_recipe::retouch_tools::Matte::encode(
        matte.bounds,
        matte.width as u32,
        matte.height as u32,
        &alpha,
    );
    // Already protected and feathered: dark blemishes must not cut holes in their own repair.
    mask.refine_edges = false;
    Some(mask)
}

/// The id of the matte [`surface_matte`] is stored under.
pub(crate) fn surface_matte_id(prefix: &str, index: usize) -> String {
    format!("{prefix}{index}-surface")
}

/// Both nostrils and the columella between them, below the nose tip: dark, enclosed by skin
/// and the right size for a mark, so nothing that looks for marks may ever see them.
fn nostrils(g: &Geometry) -> super::Capsule {
    let centre = super::add(g.nose, g.v, g.d * 0.11);
    super::Capsule {
        a: super::add(centre, g.u, -g.d * 0.15),
        b: super::add(centre, g.u, g.d * 0.15),
        r: g.d * 0.075,
    }
}

/// What acne clear runs over (ADR-0092): the surface selection with the nose included - its
/// bridge, sides and tip - and only the nostrils left out.
pub(crate) fn heal_matte(
    face: &PortraitFace,
    px: &Pixels<'_>,
    matte: &Matte,
) -> Option<aura_recipe::retouch_tools::Matte> {
    let g = Geometry::new(face, px)?;
    if g.d < 40.0 {
        return None;
    }
    let alpha = surface_selection(&g, px, matte, true)?;
    let mut mask = aura_recipe::retouch_tools::Matte::encode(
        matte.bounds,
        matte.width as u32,
        matte.height as u32,
        &alpha,
    );
    // Already protected and feathered: dark blemishes must not cut holes in their own repair.
    mask.refine_edges = false;
    Some(mask)
}

/// The id of the matte [`heal_matte`] is stored under.
pub(crate) fn heal_matte_id(prefix: &str, index: usize) -> String {
    format!("{prefix}{index}-heal")
}

/// What the texture restore runs over: the segmented face skin - the nose included, which the
/// surface selection leaves out - and wherever the surface selection reached beyond it, so the
/// shadowed skin the segmenter missed gets its pores back after healing and smoothing too.
pub(crate) fn restore_matte(
    face: &Matte,
    surface: &aura_recipe::retouch_tools::Matte,
) -> Option<aura_recipe::retouch_tools::Matte> {
    let reached = surface.decode()?;
    if reached.len() != face.alpha.len() {
        return None;
    }
    let alpha: Vec<u8> = face
        .alpha
        .iter()
        .zip(&reached)
        .map(|(a, b)| (*a).max(*b))
        .collect();
    Some(aura_recipe::retouch_tools::Matte::encode(
        face.bounds,
        u32::try_from(face.width).ok()?,
        u32::try_from(face.height).ok()?,
        &alpha,
    ))
}

/// The id of the matte [`restore_matte`] is stored under.
pub(crate) fn restore_matte_id(prefix: &str, index: usize) -> String {
    format!("{prefix}{index}-skin")
}

/// An operation over the whole surface selection of one face.
fn surface_edit(
    id: String,
    tool: Tool,
    amount: f32,
    px: &Pixels<'_>,
    matte: &Matte,
    matte_id: String,
) -> aura_recipe::retouch_tools::Edit {
    let [l, t, r, b] = matte.bounds;
    let mut edit = base_edit(
        id,
        tool,
        amount,
        px,
        [
            (l + r) * 0.5 * px.width as f32,
            (t + b) * 0.5 * px.height as f32,
            (r - l) * px.width as f32,
            (b - t) * px.height as f32,
        ],
    );
    edit.feather = 0.0;
    edit.matte = Some(matte_id);
    edit
}

/// A continuous finishing mask with feature exclusions and a soft inward edge.
/// It does not reclassify darker blemishes as protected structure at render time.
pub(crate) fn surface_finish(
    face: &PortraitFace,
    index: usize,
    px: &Pixels<'_>,
    prefix: &str,
    settings: &Settings,
    matte: &Matte,
) -> Option<(
    aura_recipe::retouch_tools::Edit,
    aura_recipe::retouch_tools::Matte,
)> {
    let g = Geometry::new(face, px)?;
    if g.d < 40.0 || settings.smoothing <= 0.0 {
        return None;
    }
    let mask = surface_matte(face, px, matte)?;
    let mut edit = surface_edit(
        format!("{prefix}{index}-surface-finish"),
        Tool::Frequency,
        (settings.smoothing * 1.15).min(1.0),
        px,
        matte,
        surface_matte_id(prefix, index),
    );
    edit.radius = (g.d * 0.025 / px.width.min(px.height) as f32).clamp(0.001, 0.02);
    edit.texture = settings.texture.clamp(0.0, 1.0);
    edit.preserve_microtexture = true;
    edit.tone = 0.95;
    Some((edit, mask))
}

/// Acne clear over the heal selection (ADR-0092), in the slot frequency healing had
/// (ADR-0090): every mark across this face - those in a dense cluster, on the nose and between
/// the brows included - is measured against a robust estimate of the clean skin around it, and
/// the tone and colour under it are rebuilt from that skin, keeping the pores. Flat redness is
/// then evened in colour only.
///
/// One operation for the whole face rather than one per spot, so it is not limited by the
/// operation budget and it reaches the marks no clean donor patch fits beside. Its strength
/// is how completely the tone is rebuilt; dark marks that are not also redder or browner than
/// the skin around them are kept unless *Remove dark marks* is on.
pub(crate) fn frequency_heal(
    face: &PortraitFace,
    index: usize,
    px: &Pixels<'_>,
    prefix: &str,
    settings: &Settings,
    matte: &Matte,
) -> Option<aura_recipe::retouch_tools::Edit> {
    let g = Geometry::new(face, px)?;
    if g.d < 40.0 || settings.frequency_heal <= 0.0 {
        return None;
    }
    let mut edit = surface_edit(
        format!("{prefix}{index}-clear"),
        Tool::AcneClear,
        1.0,
        px,
        matte,
        heal_matte_id(prefix, index),
    );
    // Below a fiftieth of the eye distance is pores; marks are larger.
    edit.radius = (g.d * 0.02 / px.width.min(px.height) as f32).clamp(0.0005, 0.05);
    edit.tone = settings.frequency_heal.clamp(0.0, 1.0);
    // A mark's own relief - its dark core and lit rim - goes with it; ordinary pore contrast
    // under it stays, and the texture restore puts back what the repair cost.
    edit.texture = 0.25;
    edit.sensitivity = Some(settings.blemish_sensitivity.clamp(0.0, 1.0));
    edit.keep_dark_marks = !settings.remove_dark_marks;
    // Acne leaves flat redness behind; even its colour, never its brightness.
    edit.preserve_microtexture = true;
    Some(edit)
}

/// The analysis pixels as they will be once `edit` (a frequency heal over `surface`) has run,
/// as packed sRGB: what the spot repairs that follow it are planned on, so they are spent on
/// what frequency healing left rather than on marks it has already rebuilt.
#[cfg(test)]
pub(crate) fn after_frequency_heal(
    px: &Pixels<'_>,
    edit: &aura_recipe::retouch_tools::Edit,
    surface: &aura_recipe::retouch_tools::Matte,
) -> Option<Vec<u8>> {
    use aura_raw::colour::curve::{srgb_decode, srgb_encode};
    use aura_raw::colour::matrix::{invert, mul, REC2020_TO_XYZ_D65, SRGB_TO_XYZ_D65};
    let to_working = aura_render::colour::narrow(mul(invert(REC2020_TO_XYZ_D65)?, SRGB_TO_XYZ_D65));
    let to_display = aura_render::output::working_to_output(aura_render::OutputColour::Srgb);
    let mut linear: Vec<f32> = px
        .data
        .chunks_exact(3)
        .flat_map(|p| {
            aura_render::colour::apply_f32(
                to_working,
                [p[0], p[1], p[2]].map(|v| srgb_decode(f32::from(v) / 255.0)),
            )
        })
        .collect();
    let mut mattes = std::collections::BTreeMap::new();
    mattes.insert(edit.matte.clone()?, surface.clone());
    aura_render::retouch_tools::apply_with_mattes(
        &mut linear,
        px.width,
        px.height,
        std::slice::from_ref(edit),
        &mattes,
    );
    Some(
        linear
            .chunks_exact(3)
            .flat_map(|p| {
                aura_render::colour::apply_f32(to_display, [p[0], p[1], p[2]])
                    .map(|v| (srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8)
            })
            .collect(),
    )
}

/// The texture restore over a face's whole skin (ADR-0090): the photograph's own pore
/// detail is put back where healing and smoothing removed it, glints and pits limited to this
/// skin's range, and detail is borrowed from clean skin only where a blemish was rebuilt.
///
/// `matte_id` names the stored selection it runs over. The automatic pass gives it the
/// segmented face skin rather than the surface selection, so the nose - which the surface
/// finish leaves out - gets its pores back too.
pub(crate) fn texture_graft(
    face: &PortraitFace,
    index: usize,
    px: &Pixels<'_>,
    prefix: &str,
    settings: &Settings,
    matte: &Matte,
    matte_id: &str,
) -> Option<aura_recipe::retouch_tools::Edit> {
    let g = Geometry::new(face, px)?;
    if g.d < 40.0 || settings.texture_graft <= 0.0 {
        return None;
    }
    let mut edit = surface_edit(
        format!("{prefix}{index}-texture-graft"),
        Tool::TextureGraft,
        1.0,
        px,
        matte,
        matte_id.to_owned(),
    );
    // Pores, and the finest grain of skin around them: about an eightieth of the eye
    // distance. Smaller misses what smoothing removed; larger brings back blotches.
    edit.radius = (g.d * 0.012 / px.width.min(px.height) as f32).clamp(0.0005, 0.05);
    // The level asked for, relative to the clean skin this face had: 60 % at the lowest
    // setting, all of it at the highest.
    edit.texture = 0.6 + 0.4 * settings.texture_graft.clamp(0.0, 1.0);
    // How firmly glints are limited follows the shine control.
    edit.tone = (settings.shine * 1.2).clamp(0.0, 1.0);
    Some(edit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dense_pattern_keeps_freckles_but_does_not_hide_distinct_red_spots() {
        let weak = Spot {
            x: 20.0,
            y: 20.0,
            radius: 3.0,
            score: 1.0,
            red: 0.012,
        };
        let red = Spot {
            red: 0.05,
            ..weak.clone()
        };
        let mut pattern = vec![&weak; 30];
        pattern.extend([&red; 4]);
        assert_eq!(
            protect_dense_pattern(&mut pattern, &Settings::default()),
            30
        );
        assert_eq!(pattern.len(), 4);
        assert!(pattern.iter().all(|s| s.red == red.red));
        let mut freckles = vec![&weak; 30];
        protect_dense_pattern(&mut freckles, &Settings::default());
        assert!(freckles.is_empty());
        let mut explicit = vec![&weak; 30];
        let asked = Settings {
            remove_dark_marks: true,
            ..Settings::default()
        };
        assert_eq!(protect_dense_pattern(&mut explicit, &asked), 0);
        assert_eq!(explicit.len(), 30);
    }
    #[test]
    fn only_small_enclosed_holes_are_filled() {
        let mut mask = vec![true; 40 * 40];
        mask[20 * 40 + 20] = false;
        for y in 0..10 {
            mask[y * 40 + 5] = false;
        }
        close_holes(&mut mask, 40, 40, 3.0);
        assert!(mask[20 * 40 + 20]);
        assert!(!mask[9 * 40 + 5]);
    }
    #[test]
    fn uniform_skin_and_long_edges_are_not_spots() {
        let mask = vec![true; 128 * 128];
        let red = vec![0.1; 128 * 128];
        let mut lum = vec![0.4; 128 * 128];
        assert!(candidates(&lum, &red, &mask, 128, 128, 100.0, 0.9)
            .0
            .is_empty());
        for y in 20..108 {
            lum[y * 128 + 64] = 0.1;
            lum[y * 128 + 65] = 0.1;
        }
        assert!(candidates(&lum, &red, &mask, 128, 128, 100.0, 0.9)
            .0
            .is_empty());
    }
    #[test]
    fn dark_spot_is_detected_on_light_and_deep_skin_but_not_excluded_features() {
        for base in [0.18, 0.4, 0.75] {
            let mut lum = vec![base; 128 * 128];
            let red = vec![0.1; 128 * 128];
            let mut mask = vec![true; 128 * 128];
            for y in 60..=66 {
                for x in 60..=66 {
                    if (x as f32 - 63.0).hypot(y as f32 - 63.0) < 3.5 {
                        lum[y * 128 + x] = base * 0.7;
                    }
                }
            }
            let spots = candidates(&lum, &red, &mask, 128, 128, 100.0, 0.9).0;
            assert_eq!(spots.len(), 1, "base={base}, spots={spots:?}");
            disk(&mut mask, 128, 128, 63.0, 63.0, 15.0);
            assert!(candidates(&lum, &red, &mask, 128, 128, 100.0, 0.9)
                .0
                .is_empty());
        }
    }
    #[test]
    fn bright_pores_do_not_spend_the_acne_budget() {
        let mut lum = vec![0.4; 128 * 128];
        for y in (16..112).step_by(12) {
            for x in (16..112).step_by(12) {
                for dy in 0..3 {
                    for dx in 0..3 {
                        lum[(y + dy) * 128 + x + dx] = 0.65;
                    }
                }
            }
        }
        let spots = candidates(
            &lum,
            &vec![0.1; 128 * 128],
            &vec![true; 128 * 128],
            128,
            128,
            100.0,
            0.9,
        )
        .0;
        assert!(
            spots.is_empty(),
            "highlight pores consumed repair budget: {}",
            spots.len()
        );
    }
    #[test]
    fn donor_is_disjoint_and_never_uses_another_spot() {
        let spot = Spot {
            x: 64.0,
            y: 64.0,
            radius: 4.0,
            score: 1.0,
            red: 0.1,
        };
        let mut clean = vec![true; 128 * 128];
        disk(&mut clean, 128, 128, 64.0, 64.0, 9.0);
        disk(&mut clean, 128, 128, 76.0, 64.0, 10.0);
        let dist = clearance(&clean, 128, 128);
        let (source, scale) = donor(
            &spot,
            &dist,
            &clearance(&vec![true; 128 * 128], 128, 128),
            &vec![0.4; 128 * 128],
            &vec![0.1; 128 * 128],
            128,
            128,
        )
        .expect("clean donor");
        assert_eq!(scale, 1.0);
        assert!(distance(source, [64.0, 64.0]) >= 12.0);
        // All pixels that will be copied into the circular repair must be clean.
        for y in -4_i32..=4 {
            for x in -4_i32..=4 {
                if x * x + y * y <= 16 {
                    assert!(
                        clean[(source[1] as i32 + y) as usize * 128
                            + (source[0] as i32 + x) as usize]
                    );
                }
            }
        }
        assert!(donor(
            &spot,
            &vec![0.0; 128 * 128],
            &clearance(&vec![true; 128 * 128], 128, 128),
            &vec![0.4; 128 * 128],
            &vec![0.1; 128 * 128],
            128,
            128
        )
        .is_none());
    }
    #[test]
    fn native_repair_removes_spot_preserves_eye_and_respects_dark_mark_option() {
        let face = PortraitFace {
            bounds: [0.2, 0.1, 0.8, 0.95],
            landmarks: [
                [0.38, 0.4],
                [0.62, 0.4],
                [0.5, 0.55],
                [0.41, 0.7],
                [0.59, 0.7],
            ],
            confidence: 0.95,
        };
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        for (cx, cy) in [(170, 290), (195, 205), (255, 380)] {
            for y in cy - 4..=cy + 4 {
                for x in cx - 4..=cx + 4 {
                    if (x as f32 - cx as f32).hypot(y as f32 - cy as f32) <= 4.0 {
                        rgb[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3]
                            .copy_from_slice(&[100, 67, 50]);
                    }
                }
            }
        }
        let matte = Matte {
            bounds: [0.2, 0.1, 0.8, 0.95],
            width: 64,
            height: 96,
            alpha: vec![255; 64 * 96],
        };
        let pixels = Pixels::new(&rgb, 512, 512).unwrap();
        let mut settings = Settings {
            deep_blemish_cleanup: true,
            max_spots: 220,
            keep_freckles: false,
            blemish_sensitivity: 0.9,
            ..Settings::default()
        };
        let protected = plan(
            &face,
            0,
            &pixels,
            "auto-portrait-v1-",
            &settings,
            Some(&matte),
        );
        assert!(protected.blemishes.is_empty());
        settings.remove_dark_marks = true;
        let planned = plan(
            &face,
            0,
            &pixels,
            "auto-portrait-v1-",
            &settings,
            Some(&matte),
        );
        assert_eq!(planned.blemishes.len(), 1);
        aura_recipe::retouch_tools::validate(&planned.blemishes).unwrap();
        assert_eq!(
            planned.blemishes,
            plan(
                &face,
                0,
                &pixels,
                "auto-portrait-v1-",
                &settings,
                Some(&matte)
            )
            .blemishes
        );
        let mut rendered: Vec<f32> = rgb.iter().map(|v| f32::from(*v) / 255.0).collect();
        let before = rendered.clone();
        aura_render::retouch_tools::apply(&mut rendered, 512, 512, &planned.blemishes);
        let spot = (290 * 512 + 170) * 3;
        assert!(
            rendered[spot] > before[spot] + 0.1,
            "native heal must actually remove dark defect"
        );
        for y in 286..=294 {
            for x in 166..=174 {
                if (x as f32 - 170.0).hypot(y as f32 - 290.0) <= 4.0 {
                    let i = (y * 512 + x) * 3;
                    assert!(
                        (rendered[i] - 150.0 / 255.0).abs() < 0.01,
                        "lesion rim must be repaired before blending into healthy skin"
                    );
                }
            }
        }
        let eye = (205 * 512 + 195) * 3;
        assert_eq!(&rendered[eye..eye + 3], &before[eye..eye + 3]);
        let lip = (380 * 512 + 255) * 3;
        assert_eq!(&rendered[lip..lip + 3], &before[lip..lip + 3]);
        settings.frequency_heal = 0.9;
        let residual = plan(
            &face,
            0,
            &pixels,
            "auto-portrait-v1-",
            &settings,
            Some(&matte),
        );
        let mut softened = before.clone();
        aura_render::retouch_tools::apply(&mut softened, 512, 512, &residual.blemishes);
        assert!(
            softened[spot] > before[spot],
            "residual repair still reduces the mark"
        );
        assert!(
            softened[spot] < rendered[spot],
            "residual repair retains some original detail"
        );
        assert_eq!(&softened[eye..eye + 3], &before[eye..eye + 3]);
        let mask =
            aura_render::retouch_tools::selection_mask(&before, 512, 512, &planned.blemishes[0]);
        for (i, alpha) in mask.iter().enumerate() {
            if *alpha == 0.0 {
                assert_eq!(&rendered[i * 3..i * 3 + 3], &before[i * 3..i * 3 + 3]);
            }
        }
        assert!(
            plan(&face, 0, &pixels, "auto-portrait-v1-", &settings, None)
                .blemishes
                .is_empty()
        );
    }

    #[test]
    fn dense_skin_can_use_a_smaller_clean_donor() {
        let spot = Spot {
            x: 64.0,
            y: 64.0,
            radius: 12.0,
            score: 1.0,
            red: 0.1,
        };
        let clean: Vec<_> = (0..128 * 128)
            .map(|i| ((i % 128) as f32 - 28.0).hypot((i / 128) as f32 - 64.0) <= 6.0)
            .collect();
        let (point, scale) = donor(
            &spot,
            &clearance(&clean, 128, 128),
            &clearance(&vec![true; 128 * 128], 128, 128),
            &vec![0.4; 128 * 128],
            &vec![0.1; 128 * 128],
            128,
            128,
        )
        .unwrap();
        assert!(scale < 1.0);
        assert!(distance(point, [28.0, 64.0]) < 2.0);
    }

    #[test]
    fn surface_finish_keeps_pores_without_touching_eyes_lips_or_background() {
        let face = PortraitFace {
            bounds: [0.2, 0.1, 0.8, 0.95],
            confidence: 0.95,
            landmarks: [
                [0.38, 0.4],
                [0.62, 0.4],
                [0.5, 0.55],
                [0.41, 0.7],
                [0.59, 0.7],
            ],
        };
        let rgb = [150_u8, 100, 75].repeat(512 * 512);
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        let settings = Settings {
            smoothing: 0.8,
            texture: 0.85,
            ..Settings::default()
        };
        let (edit, mask) =
            surface_finish(&face, 0, &px, "auto-portrait-v1-", &settings, &matte).unwrap();
        assert!(!mask.refine_edges);
        assert_eq!(
            crate::portrait_auto::group_of(&edit.id),
            Some(crate::portrait_auto::Group::Finishing)
        );
        let mut mattes = std::collections::BTreeMap::new();
        mattes.insert(edit.matte.clone().unwrap(), mask);
        let mut pixels: Vec<f32> = rgb.iter().map(|v| f32::from(*v) / 255.0).collect();
        for y in 0..512 {
            for x in 0..512 {
                for c in 0..3 {
                    pixels[(y * 512 + x) * 3 + c] += if x % 2 == 0 { 0.025 } else { -0.025 };
                }
            }
        }
        for (cx, cy) in [(195, 205), (317, 205), (255, 370)] {
            for y in cy - 5..=cy + 5 {
                for x in cx - 10..=cx + 10 {
                    pixels[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3].fill(0.02);
                }
            }
        }
        let before = pixels.clone();
        let coverage = aura_render::retouch_tools::selection_mask_with_mattes(
            &pixels, 512, 512, &edit, &mattes,
        );
        aura_render::retouch_tools::apply_with_mattes(&mut pixels, 512, 512, &[edit], &mattes);
        for (i, a) in coverage.iter().enumerate() {
            if *a == 0.0 {
                assert_eq!(&pixels[i * 3..i * 3 + 3], &before[i * 3..i * 3 + 3]);
            }
        }
        for (x, y) in [(195, 205), (317, 205), (255, 370), (5, 5)] {
            assert_eq!(coverage[y * 512 + x], 0.0);
        }
        let at = (290 * 512 + 170) * 3;
        assert!(coverage[290 * 512 + 170] > 0.9);
        let retained = (pixels[at] - pixels[at + 3]).abs() / (before[at] - before[at + 3]).abs();
        assert!(
            retained > 0.7 && retained < 1.1,
            "retained pore contrast {retained}"
        );
    }

    fn finish_face() -> PortraitFace {
        PortraitFace {
            bounds: [0.2, 0.1, 0.8, 0.95],
            confidence: 0.95,
            landmarks: [
                [0.38, 0.4],
                [0.62, 0.4],
                [0.5, 0.55],
                [0.41, 0.7],
                [0.59, 0.7],
            ],
        }
    }

    fn fill(rgb: &mut [u8], [x0, y0, x1, y1]: [usize; 4], colour: [u8; 3]) {
        for y in y0..y1 {
            for x in x0..x1 {
                rgb[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3].copy_from_slice(&colour);
            }
        }
    }

    #[test]
    fn the_surface_selection_reaches_shadowed_skin_and_follows_the_brow() {
        let face = finish_face();
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        // Grid cells are 3.2 px wide and 3.4 px tall, starting at (102.4, 51.2).
        let cell = |x: usize, y: usize| [102 + x * 16 / 5, 51 + y * 17 / 5];
        let (w, h) = (96, 128);
        let mut alpha = vec![255_u8; w * h];
        // Left of the face: background the segmenter did not select.
        for y in 0..h {
            for x in 0..12 {
                alpha[y * w + x] = 0;
            }
        }
        fill(&mut rgb, [0, 0, 140, 512], [90, 90, 90]);
        // Two notches the segmenter cut into the cheek. One is a dark mark in shadow...
        for (rows, colour) in [(70..76, [60_u8, 38, 28]), (40..46, [8, 8, 8])] {
            for y in rows {
                for x in 12..20 {
                    alpha[y * w + x] = 0;
                    let [px, py] = cell(x, y);
                    fill(&mut rgb, [px, py, px + 5, py + 5], colour);
                }
            }
        }
        // Right of the face: skin in deep shadow the segmenter was unsure of, and above it a
        // grey wall it was equally unsure of.
        for y in 0..h {
            for x in 84..w {
                alpha[y * w + x] = 40;
                let [px, py] = cell(x, y);
                let colour = if y < 40 { [90, 90, 90] } else { [70, 45, 33] };
                fill(&mut rgb, [px, py, px + 5, py + 5], colour);
            }
        }
        // A band of shadowed cheek the segmenter saw nothing of at all, inside the face...
        for y in 50..95 {
            for x in 70..84 {
                alpha[y * w + x] = 0;
                let [px, py] = cell(x, y);
                fill(&mut rgb, [px, py, px + 5, py + 5], [70, 45, 33]);
            }
        }
        // ...and skin below the face's outline - a neck - that it equally did not select.
        fill(&mut rgb, [102, 432, 140, 487], [150, 100, 75]);
        // Brows: dark hair where a brow is. The forehead above the left one is lit; above the
        // right one it is in shadow, darker than this face's typical skin and still skin.
        fill(&mut rgb, [290, 125, 350, 160], [75, 50, 37]);
        for eye in [195, 317] {
            fill(&mut rgb, [eye - 35, 160, eye + 35, 171], [40, 28, 22]);
        }
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: w,
            height: h,
            alpha,
        };
        let surface = surface_matte(&face, &px, &matte).unwrap();
        assert!(!surface.refine_edges);
        let selected = surface.decode().unwrap();
        let at = |x: usize, y: usize| selected[y * w + x];
        assert!(
            at(18, 73) > 200,
            "the shadowed mark is outside: {}",
            at(18, 73)
        );
        assert_eq!(at(16, 43), 0, "hair in a notch was selected");
        assert!(at(88, 90) > 200, "hinted shadow skin was not reached");
        assert!(
            at(78, 72) > 200,
            "shadowed cheek inside the face, with no hint, was not reached: {}",
            at(78, 72)
        );
        assert_eq!(
            at(4, 120),
            0,
            "skin outside the face's outline was selected"
        );
        assert_eq!(at(90, 20), 0, "a grey wall was selected as skin");
        assert_eq!(at(4, 60), 0, "background was selected");
        // The brow itself is out; the forehead a fifth of the eye distance above it is in.
        let column = (195.0_f32 - 102.4) / 3.2;
        let row = |y: f32| ((y - 51.2) / 3.4) as usize;
        assert_eq!(at(column as usize, row(165.0)), 0, "the brow was selected");
        assert!(
            at(column as usize, row(138.0)) > 150,
            "lit forehead above the brow is still left out: {}",
            at(column as usize, row(138.0))
        );
        let right = ((317.0_f32 - 102.4) / 3.2) as usize;
        assert_eq!(at(right, row(165.0)), 0, "the brow in shadow was selected");
        assert!(
            at(right, row(140.0)) > 150,
            "the shadowed forehead above the brow was taken for brow: {}",
            at(right, row(140.0))
        );
        // Eyes and lips stay out, as before.
        assert_eq!(at(column as usize, row(205.0)), 0);
        assert_eq!(at(((255.0_f32 - 102.4) / 3.2) as usize, row(370.0)), 0);
    }

    #[test]
    fn the_heal_selection_includes_the_nose_and_leaves_out_the_nostrils() {
        let face = finish_face();
        let rgb = [150_u8, 100, 75].repeat(512 * 512);
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        let cell = |x: f32, y: f32| ((y - 51.2) / 3.4) as usize * 96 + ((x - 102.4) / 3.2) as usize;
        let heal = heal_matte(&face, &px, &matte).unwrap();
        assert!(!heal.refine_edges);
        let heal = heal.decode().unwrap();
        let surface = surface_matte(&face, &px, &matte).unwrap().decode().unwrap();
        // The nose tip and the nose beside it: out of the smoothing finish, in acne clear.
        for [x, y] in [[256.0, 270.0], [248.0, 275.0]] {
            assert_eq!(
                surface[cell(x, y)],
                0,
                "the finish reaches the nose at {x},{y}"
            );
            assert!(
                heal[cell(x, y)] > 150,
                "acne clear misses the nose at {x},{y}: {}",
                heal[cell(x, y)]
            );
        }
        // Both nostrils and the columella between them stay out.
        for x in [242.0, 256.0, 270.0] {
            assert_eq!(heal[cell(x, 296.0)], 0, "a nostril at {x} is selected");
        }
        // The brow bone: no band along the brow line where there is no dark brow hair.
        assert!(
            heal[cell(195.0, 166.0)] > 150,
            "skin under a missing brow is out"
        );
    }

    #[test]
    fn a_nostril_away_from_where_the_landmarks_put_it_is_still_left_out() {
        // A turned face: one nostril lies well to the side of the nose landmark, outside every
        // landmark shape. A red mark beside the nose stays in for acne clear.
        let face = finish_face();
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        fill(&mut rgb, [286, 290, 300, 302], [35, 22, 16]);
        fill(&mut rgb, [300, 268, 306, 274], [135, 75, 60]);
        // An inflamed mark across the base of the nose: under half the skin's luminance, and
        // still red. And a mark on the shadowed side of the nose above it, darker still.
        fill(&mut rgb, [216, 298, 224, 306], [95, 28, 28]);
        fill(&mut rgb, [231, 258, 239, 266], [55, 20, 20]);
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        let cell = |x: f32, y: f32| ((y - 51.2) / 3.4) as usize * 96 + ((x - 102.4) / 3.2) as usize;
        let heal = heal_matte(&face, &px, &matte).unwrap().decode().unwrap();
        let surface = surface_matte(&face, &px, &matte).unwrap().decode().unwrap();
        for [x, y] in [[288.0, 292.0], [293.0, 296.0], [298.0, 300.0]] {
            assert_eq!(
                surface[cell(x, y)],
                0,
                "the finish reaches the nostril at {x},{y}"
            );
            assert_eq!(
                heal[cell(x, y)],
                0,
                "acne clear reaches the nostril at {x},{y}"
            );
        }
        for [x, y] in [[303.0, 271.0], [220.0, 302.0], [235.0, 262.0]] {
            assert!(
                heal[cell(x, y)] > 150,
                "the red mark at {x},{y} beside the nose is out: {}",
                heal[cell(x, y)]
            );
        }
    }

    #[test]
    fn acne_clear_and_the_graft_are_opt_in_share_one_selection_and_spare_features() {
        let face = finish_face();
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        // Pores: a fixed pattern of a few codes, so there is texture to keep and to borrow.
        for (i, pixel) in rgb.chunks_exact_mut(3).enumerate() {
            let (x, y) = (i % 512, i / 512);
            let v = [0_u8, 2, 4, 6, 8][(x * 7 + y * 13) % 5];
            for c in pixel {
                *c = (*c + v).saturating_sub(4);
            }
        }
        // Three inflamed marks on the cheeks, and features that must not move.
        let marks = [[170_usize, 290_usize], [340, 300], [200, 330]];
        for [cx, cy] in marks {
            for y in cy - 5..=cy + 5 {
                for x in cx - 5..=cx + 5 {
                    if (x as f32 - cx as f32).hypot(y as f32 - cy as f32) <= 5.0 {
                        rgb[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3]
                            .copy_from_slice(&[128, 62, 50]);
                    }
                }
            }
        }
        fill(&mut rgb, [185, 200, 205, 210], [20, 20, 20]);
        fill(&mut rgb, [245, 365, 265, 375], [120, 50, 55]);
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        let prefix = "auto-portrait-v1-";
        let off = Settings::default();
        assert!(frequency_heal(&face, 0, &px, prefix, &off, &matte).is_none());
        assert!(texture_graft(&face, 0, &px, prefix, &off, &matte, "face").is_none());
        let settings = Settings {
            frequency_heal: 1.0,
            texture_graft: 0.75,
            blemish_sensitivity: 0.8,
            ..Settings::default()
        };
        let clear = frequency_heal(&face, 0, &px, prefix, &settings, &matte).unwrap();
        let graft = texture_graft(
            &face,
            0,
            &px,
            prefix,
            &settings,
            &matte,
            &heal_matte_id(prefix, 0),
        )
        .unwrap();
        let surface = heal_matte(&face, &px, &matte).unwrap();
        aura_recipe::retouch_tools::validate(&[clear.clone(), graft.clone()]).unwrap();
        // Frequency healing is saved with the skin step, so it runs before everything else
        // on the face; the graft is saved with the finishing step, so it runs last.
        assert_eq!(
            crate::portrait_auto::group_of(&clear.id),
            Some(crate::portrait_auto::Group::Skin)
        );
        assert_eq!(
            crate::portrait_auto::group_of(&graft.id),
            Some(crate::portrait_auto::Group::Finishing)
        );
        // Acne clear works in the heal selection - the surface selection with the nose.
        assert_eq!(clear.tool, Tool::AcneClear);
        let id = heal_matte_id(prefix, 0);
        assert_eq!(clear.matte.as_deref(), Some(id.as_str()));
        assert_eq!(graft.matte.as_deref(), Some(id.as_str()));
        // Moles are kept unless the photographer asked for dark marks to go.
        assert!(clear.keep_dark_marks);
        let asked = Settings {
            remove_dark_marks: true,
            ..settings
        };
        assert!(
            !frequency_heal(&face, 0, &px, prefix, &asked, &matte)
                .unwrap()
                .keep_dark_marks
        );
        let healed = after_frequency_heal(&px, &clear, &surface).unwrap();
        assert_eq!(healed.len(), rgb.len());
        for [cx, cy] in marks {
            let i = (cy * 512 + cx) * 3;
            assert!(
                healed[i + 1] > rgb[i + 1] + 25,
                "mark at {cx},{cy}: green {} -> {}",
                rgb[i + 1],
                healed[i + 1]
            );
        }
        // Eye, lips and everything outside the selection: the same bytes.
        for [x, y] in [[195_usize, 205_usize], [255, 370], [5, 5], [500, 500]] {
            let i = (y * 512 + x) * 3;
            assert_eq!(&healed[i..i + 3], &rgb[i..i + 3], "pixel {x},{y} moved");
        }
        assert_eq!(healed, after_frequency_heal(&px, &clear, &surface).unwrap());
    }
}
