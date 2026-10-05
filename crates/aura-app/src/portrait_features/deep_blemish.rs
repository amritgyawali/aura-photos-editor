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

// Deterministic four-connected components, including zero-valued mask holes.
fn components(mask: &[bool], w: usize, h: usize) -> Vec<Vec<usize>> {
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
                dark.max(0.0) + (-dark).max(0.0) * 0.65 + redness[i].max(0.0) * 1.5
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
                let repair = (reach * 1.2 + 1.5 * scale).max(3.5 * scale);
                let at = y.round() as usize * w + x.round() as usize;
                if inside[at] < repair * 1.1 + 1.0 || peak < threshold * 1.25 {
                    continue;
                }
                // Large repairs first within a severity tier; suppress duplicate scales below.
                spots.push(Spot {
                    x,
                    y,
                    radius: repair,
                    score: peak * area.sqrt(),
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
    let mut lum = vec![0.0_f32; w * h];
    let mut red = vec![0.0_f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let p = px.encoded(x + x0, y + y0);
            let i = y * w + x;
            lum[i] = p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722;
            red[i] = (p[0] - (p[1] + p[2]) * 0.5) / (p[0] + p[1] + p[2] + 0.05);
            mask[i] = matte.at(
                (x + x0) as f32 / px.width as f32,
                (y + y0) as f32 / px.height as f32,
            ) > 0.55;
        }
    }
    close_holes(&mut mask, w, h, g.d * 0.07);
    let mut exclusions = g.exclusions();
    exclusions.push(super::Capsule {
        a: super::add(g.mouth[0], g.v, g.d * 0.08),
        b: super::add(g.mouth[1], g.v, g.d * 0.08),
        r: g.d * 0.23,
    });
    for y in 0..h {
        for x in 0..w {
            let point = [(x + x0) as f32 + 0.5, (y + y0) as f32 + 0.5];
            if exclusions.iter().any(|e| e.contains(point)) {
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
        clean[i] &= contrast < 0.085 && raw_contrast < 0.20 && redness < 0.025;
    }
    for spot in &spots {
        disk(&mut clean, w, h, spot.x, spot.y, spot.radius * 0.8);
    }
    let clean = clearance(&clean, w, h);
    let inside = clearance(&mask, w, h);
    let mut no_donor = 0;
    let red_threshold = 0.012 - settings.blemish_sensitivity * 0.006;
    let eligible: Vec<_> = spots
        .iter()
        .filter(|s| {
            let keep = !settings.remove_dark_marks && s.red < red_threshold;
            if keep {
                out.report.marks_kept += 1;
            }
            !keep
        })
        .collect();
    // Keep-freckles means a dense field is opt-out unless dark-mark removal was requested.
    if settings.keep_freckles
        && !settings.remove_dark_marks
        && eligible.len() > super::FRECKLE_FIELD
    {
        out.report.findings.push(
            "Deep cleanup: dense mark pattern kept; turn off Keep freckles to repair it.".into(),
        );
        return out;
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
        let mut edit = base_edit(
            format!("{prefix}{index}-spot-deep-{}", out.blemishes.len()),
            Tool::PatchHeal,
            1.0,
            px,
            [centre[0], centre[1], spot.radius, spot.radius],
        );
        edit.source = Some([
            (source[0] + x0 as f32 + 0.5) / px.width as f32,
            (source[1] + y0 as f32 + 0.5) / px.height as f32,
        ]);
        edit.feather = 0.20;
        edit.source_scale = source_scale;
        // No coarse matte: the entire disk was checked against the repaired skin mask.
        // Applying the unfilled matte here would protect the centre of a dark blemish.
        out.blemishes.push(edit);
    }
    out.report.spots_healed = out.blemishes.len();
    out.report.findings.push(format!("Deep blemish cleanup: {} texture-transfer repairs across segmented face skin; {} marks kept, {} spots skipped without a clean donor. {}",
        out.report.spots_healed,out.report.marks_kept,no_donor,
        if settings.remove_dark_marks {"Dark-mark removal enabled; review freckles and beauty marks."} else {"Dark marks protected."}));
    out
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
    let [l, t, r, b] = matte.bounds;
    let (w, h) = (matte.width, matte.height);
    if w < 3 || h < 3 {
        return None;
    }
    let cell = (((r - l) * px.width as f32 / w as f32).max((b - t) * px.height as f32 / h as f32))
        .max(1.0);
    let mut selected: Vec<_> = matte.alpha.iter().map(|a| *a > 64).collect();
    close_holes(&mut selected, w, h, g.d * 0.30 / cell);
    // Creases are protected during spot replacement, but a continuous surface
    // finish must span them: otherwise they become conspicuous untreated strips.
    let mut exclusions = vec![super::Capsule::disk(
        super::add(g.nose, g.v, g.d * 0.06),
        g.d * 0.20,
    )];
    for eye in g.eyes {
        exclusions.push(super::Capsule::disk(eye, g.d * 0.25));
        exclusions.push(super::Capsule::disk(
            super::add(eye, g.v, -g.d * 0.32),
            g.d * 0.19,
        ));
    }
    exclusions.push(super::Capsule {
        a: super::add(g.mouth[0], g.v, g.d * 0.08),
        b: super::add(g.mouth[1], g.v, g.d * 0.08),
        r: g.d * 0.25,
    });
    for y in 0..h {
        for x in 0..w {
            let point = [
                (l + (x as f32 + 0.5) / w as f32 * (r - l)) * px.width as f32,
                (t + (y as f32 + 0.5) / h as f32 * (b - t)) * px.height as f32,
            ];
            if exclusions.iter().any(|e| e.contains(point)) {
                selected[y * w + x] = false;
            }
        }
    }
    let distance = clearance(&selected, w, h);
    let feather = (g.d * 0.14 / cell).max(2.0);
    let alpha: Vec<_> = distance
        .iter()
        .map(|d| {
            let v = ((d - 1.0) / feather).clamp(0.0, 1.0);
            (v * v * (3.0 - 2.0 * v) * 255.0).round() as u8
        })
        .collect();
    let mut mask =
        aura_recipe::retouch_tools::Matte::encode(matte.bounds, w as u32, h as u32, &alpha);
    mask.refine_edges = false;
    let mut edit = base_edit(
        format!("{prefix}{index}-surface-finish"),
        Tool::Frequency,
        (settings.smoothing * 1.15).min(1.0),
        px,
        [
            (l + r) * 0.5 * px.width as f32,
            (t + b) * 0.5 * px.height as f32,
            (r - l) * px.width as f32,
            (b - t) * px.height as f32,
        ],
    );
    edit.feather = 0.0;
    edit.radius = (g.d * 0.025 / px.width.min(px.height) as f32).clamp(0.001, 0.02);
    edit.texture = settings.texture.clamp(0.0, 1.0);
    edit.preserve_microtexture = true;
    edit.tone = 0.95;
    edit.matte = Some(format!("{prefix}{index}-surface"));
    Some((edit, mask))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let eye = (205 * 512 + 195) * 3;
        assert_eq!(&rendered[eye..eye + 3], &before[eye..eye + 3]);
        let lip = (380 * 512 + 255) * 3;
        assert_eq!(&rendered[lip..lip + 3], &before[lip..lip + 3]);
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
}
