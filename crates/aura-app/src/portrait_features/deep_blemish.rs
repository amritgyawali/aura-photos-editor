//! Multiscale spot repair over segmented skin. No learned acne/permanent-mark claim.
//! Donors and complete repair ellipses must fit inside skin with feature exclusions.
//! Small enclosed mask holes are filled before exclusions, so a dark spot cannot
//! exclude its own repair. Each accepted proposal becomes an editable native heal.
#![allow(clippy::indexing_slicing, clippy::too_many_arguments)]

#[cfg(test)]
use super::distance;
use super::{base_edit, FeatureEdits, Geometry, Pixels, Settings, Tool};
use aura_vision::{portrait::PortraitFace, skin::Matte};

mod curvature;
#[cfg(test)]
mod diagnostics;
mod lighting;
mod refinement;
pub(crate) use refinement::refine;

#[derive(Clone, Copy)]
enum SurfaceKind {
    Finish,
    Blemishes,
    Spots,
}

#[derive(Clone, Debug)]
struct Spot {
    x: f32,
    y: f32,
    radius: f32,
    radii: [f32; 2],
    clean_ring_fit: bool,
    reference: [f32; 2],
    curved_heal: bool,
    chroma_only: bool,
    heal_samples: Vec<[f32; 2]>,
    score: f32,
    red: f32,
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

// Test the entire footprint, including an interpolation margin. A long narrow
// lesion can fit beside an opening even when its enclosing circle cannot.
fn ellipse_inside(mask: &[bool], w: usize, h: usize, centre: [f32; 2], radii: [f32; 2]) -> bool {
    let [x, y] = centre;
    let [rx, ry] = radii.map(|r| r * 1.1 + 1.0);
    if x - rx < 1.0 || y - ry < 1.0 || x + rx >= w as f32 - 1.0 || y + ry >= h as f32 - 1.0 {
        return false;
    }
    for yy in (y - ry).floor() as usize..=(y + ry).ceil() as usize {
        for xx in (x - rx).floor() as usize..=(x + rx).ceil() as usize {
            if ((xx as f32 - x) / rx).powi(2) + ((yy as f32 - y) / ry).powi(2) <= 1.0
                && !mask[yy * w + xx]
            {
                return false;
            }
        }
    }
    true
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
    let midpoint = median(
        lum.iter()
            .zip(mask)
            .filter(|(_, on)| **on)
            .map(|(value, _)| *value)
            .collect(),
    )
    .unwrap_or(0.5);
    let mut spots = Vec::new();
    for radius in [2.5_f32, 4.0, 8.0, 13.0] {
        let bg = aura_render::bands::blur(lum, w, h, (radius * scale).round().max(2.0) as usize);
        let score: Vec<_> = (0..w * h)
            .map(|i| {
                let dark = aura_render::retouch_analysis::darkness(
                    bg[i], fine[i], redness[i], midpoint, 0.08,
                );
                // Isolated bright pores are texture, not acne. A bright head must
                // also carry local redness to consume a repair slot.
                dark.max(0.0)
                    + (-dark).max(0.0).min(redness[i].max(0.0) * 4.0)
                    + redness[i].max(0.0) * 1.5
            })
            .collect();
        // Higher thresholds split touching marks that merge at the sensitive threshold.
        for tier in [1.0, 1.8, 2.8, 4.5, 7.0, 12.0, 20.0] {
            let selected: Vec<_> = (0..w * h)
                .map(|i| {
                    mask[i] && score[i] > threshold * tier && (tier <= 2.8 || redness[i] > 0.015)
                })
                .collect();
            for group in components(&selected, w, h) {
                let area = group.len() as f32;
                if area < (4.0 * scale * scale).max(3.0) || area > 450.0 * scale * scale {
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
                #[cfg(test)]
                let trace = |stage: &str| {
                    if std::env::var_os("AURA_SPOT_TRACE").is_some() {
                        println!("spot-trace {stage} {x:.2} {y:.2} area={area:.1} reach={reach:.2} aspect={:.2} red={rmax:.4}", (half + root) / (half - root).max(0.2));
                    }
                };
                // Positive color evidence must cover the component, not just one
                // red pixel on a neutral crease. Neutral marks keep the strict
                // compact-shape path; measured inflammation can be elongated.
                let inflamed = rmax > 0.025
                    && group.iter().map(|&i| redness[i].max(0.0)).sum::<f32>() / area > 0.012;
                if (half + root) / (half - root).max(0.2) > if inflamed { 25.0 } else { 5.5 }
                    || reach > 18.0 * scale
                {
                    #[cfg(test)]
                    trace("shape");
                    continue;
                }
                // Keep the measured lesion inside the fully repaired core; the
                // feather belongs on surrounding healthy skin, not on the acne rim.
                let repair = (reach * 1.4 + 1.5 * scale).max(3.5 * scale);
                let radii = if inflamed {
                    let mut axes = [0.0_f32; 2];
                    for &i in &group {
                        axes[0] = axes[0].max(((i % w) as f32 - x).abs());
                        axes[1] = axes[1].max(((i / w) as f32 - y).abs());
                    }
                    axes = axes.map(|a| (a * 1.4 + 1.5 * scale).max(3.5 * scale));
                    let extent = group
                        .iter()
                        .map(|&i| {
                            (((i % w) as f32 - x) / axes[0]).hypot(((i / w) as f32 - y) / axes[1])
                        })
                        .fold(0.0_f32, f32::max);
                    // Every measured lesion pixel lies inside the 75% solid core.
                    axes.map(|a| a * (extent / 0.70).max(1.0))
                } else {
                    [repair; 2]
                };
                let at = y.round() as usize * w + x.round() as usize;
                // A pale centre alone does not distinguish a pore from a pimple.
                // Read redness over the measured component: whiteheads can have
                // a neutral centre enclosed by an inflamed rim. A plain glint's
                // dark halo still has no red evidence and remains excluded.
                if (fine[at] > bg[at] && rmax <= 0.003)
                    || (if inflamed {
                        !ellipse_inside(mask, w, h, [x, y], radii)
                    } else {
                        inside[at] < repair * 1.1 + 1.0
                    })
                    || peak < threshold * 1.25
                {
                    #[cfg(test)]
                    trace("clearance-or-glint");
                    continue;
                }
                // A compact piece of a curved shadow may pass the shape tests.
                // Require compatible surrounding light before replacing texture:
                // otherwise a clean donor can erase the nose or cheek contour.
                let mut ring = [[0.0_f32; 3]; 16];
                let mut clean_ring = [true; 16];
                let mut ring_red = [0.0_f32; 16];
                let mut surrounded = true;
                for (n, sample) in ring.iter_mut().enumerate() {
                    let angle = std::f32::consts::TAU * n as f32 / 16.0;
                    let sx = (x + angle.cos() * radii[0] * 1.35).round();
                    let sy = (y + angle.sin() * radii[1] * 1.35).round();
                    if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                        surrounded = false;
                        break;
                    }
                    let j = sy as usize * w + sx as usize;
                    if !mask[j] {
                        surrounded = false;
                        break;
                    }
                    *sample = [
                        (sx - x) / (radii[0] * 1.35),
                        (sy - y) / (radii[1] * 1.35),
                        fine[j],
                    ];
                    clean_ring[n] = !(inflamed && redness[j] > 0.012 && score[j] > threshold);
                    ring_red[n] = fine_red[j];
                }
                let light_reference = lighting::reference(&ring, &clean_ring, fine[at], inflamed);
                let mut curved = inflamed
                    .then(|| {
                        curvature::reference(
                            &fine,
                            &redness,
                            &score,
                            mask,
                            w,
                            h,
                            [x, y],
                            radii,
                            threshold,
                        )
                    })
                    .flatten();
                if inflamed && light_reference.is_none() && curved.is_none() {
                    curved = curvature::contextual_reference(
                        &fine,
                        &redness,
                        &score,
                        mask,
                        w,
                        h,
                        [x, y],
                        radii,
                        threshold,
                    );
                }
                let contextual = curved.as_ref().is_some_and(|r| !r.samples.is_empty());
                let use_curved = light_reference.is_none()
                    && curved
                        .as_ref()
                        .is_some_and(|r| r.curvature > 0.015 || contextual);
                let chosen_light = if use_curved {
                    curved.as_ref().map(|r| r.light)
                } else {
                    light_reference
                };
                let healthy_red = median(
                    ring_red
                        .iter()
                        .zip(clean_ring)
                        .filter(|(_, clean)| *clean)
                        .map(|(r, _)| *r)
                        .collect(),
                );
                // Unsupported brightness forbids rebuilding texture. Strong,
                // independently supported inflammation can instead lose pigment
                // while retaining the original luminance and fine detail.
                let color_support = surrounded
                    && inflamed
                    && rmax > 0.06
                    && group.iter().map(|&i| redness[i].max(0.0)).sum::<f32>() / area > 0.025
                    && clean_ring.iter().filter(|v| **v).count() >= 10
                    && clean_ring
                        .chunks(4)
                        .all(|q| q.iter().filter(|v| **v).count() >= 2)
                    && healthy_red.is_some_and(|r| fine_red[at] > r + 0.025);
                let chroma_only = chosen_light.is_none() && color_support;
                let reference_luma =
                    if let Some(light) = chosen_light.filter(|_| surrounded || contextual) {
                        light
                    } else if chroma_only {
                        median(
                            ring.iter()
                                .zip(clean_ring)
                                .filter(|(_, clean)| *clean)
                                .map(|(p, _)| p[2])
                                .collect(),
                        )
                        .unwrap_or(fine[at])
                    } else {
                        #[cfg(test)]
                        trace(if surrounded { "lighting" } else { "ring-mask" });
                        continue;
                    };
                if rmax <= 0.006
                    && !lighting::context_consistent(
                        &fine,
                        w,
                        h,
                        [x, y],
                        (repair * 3.0).max(eye_distance * 0.10),
                        fine[at],
                    )
                {
                    #[cfg(test)]
                    trace("context");
                    continue;
                }
                #[cfg(test)]
                trace("accepted");
                // Large repairs first within a severity tier; suppress duplicate scales below.
                spots.push(Spot {
                    x,
                    y,
                    radius: radii[0].max(radii[1]),
                    radii,
                    clean_ring_fit: inflamed,
                    curved_heal: use_curved,
                    chroma_only,
                    heal_samples: if use_curved {
                        curved.map_or_else(Vec::new, |r| r.samples)
                    } else {
                        Vec::new()
                    },
                    reference: [
                        reference_luma,
                        median(
                            ring_red
                                .iter()
                                .zip(clean_ring)
                                .filter(|(_, clean)| *clean)
                                .map(|(red, _)| *red)
                                .collect(),
                        )
                        .unwrap_or(broad_red[at]),
                    ],
                    score: peak * area.powf(0.75),
                    red: rmax,
                });
            }
        }
    }
    spots.sort_by(|a, b| {
        a.chroma_only.cmp(&b.chroma_only).then(
            b.score
                .total_cmp(&a.score)
                .then(a.y.total_cmp(&b.y))
                .then(a.x.total_cmp(&b.x)),
        )
    });
    let mut unique: Vec<Spot> = Vec::new();
    for spot in spots {
        if unique.iter().all(|other| {
            ((spot.x - other.x) / (spot.radii[0] + other.radii[0]))
                .hypot((spot.y - other.y) / (spot.radii[1] + other.radii[1]))
                > 0.75
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
    for scale in [1.0, 0.7, 0.5, 0.35, 0.25] {
        let source_radius = spot.radius * scale;
        let mut best = None;
        let mut best_cost = f32::INFINITY;
        for reach in [3.0, 4.5, 6.0, 9.0, 13.0, 18.0, 24.0, 32.0, 48.0, 64.0] {
            for n in 0..32 {
                let angle = std::f32::consts::TAU * n as f32 / 32.0;
                let x = (spot.x + angle.cos() * spot.radius * reach).round();
                let y = (spot.y + angle.sin() * spot.radius * reach).round();
                if x < 1.0 || y < 1.0 || x >= w as f32 - 1.0 || y >= h as f32 - 1.0 {
                    continue;
                }
                let Some(cost) = donor_cost(
                    spot,
                    [x, y],
                    source_radius,
                    reach,
                    clean,
                    inside,
                    broad,
                    red,
                    w,
                ) else {
                    continue;
                };
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

fn configure_pigment_repair(
    edit: &mut aura_recipe::retouch_tools::Edit,
    radius: f32,
    minimum_dimension: usize,
) {
    edit.feather = 0.45;
    edit.tone = 0.9;
    edit.radius = (radius * 0.5 / minimum_dimension as f32).clamp(0.0005, 0.05);
    edit.skin = Some(aura_recipe::retouch_tools::SkinSettings {
        tolerance: 0.3,
        edge_protection: 0.0,
        connected: false,
    });
}

fn donor_cost(
    spot: &Spot,
    [x, y]: [f32; 2],
    source_radius: f32,
    reach: f32,
    clean: &[f32],
    inside: &[f32],
    broad: &[f32],
    red: &[f32],
    w: usize,
) -> Option<f32> {
    let source_radius = if spot.chroma_only {
        source_radius.max(6.0)
    } else {
        source_radius
    };
    let i = y as usize * w + x as usize;
    // Harmonic healing samples a rectangular donor plus a surrounding ring.
    if inside[i] < source_radius * 1.55 + 2.0 || clean[i] < source_radius * 1.55 + 1.0 {
        return None;
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
        return None;
    }
    Some(
        ((broad[i] - spot.reference[0]) / (spot.reference[0] + 0.08)).abs()
            + (red[i] - spot.reference[1]).abs() * 2.0
            + reach * 0.002
            + contaminated as f32 * 0.025,
    )
}

fn texture_donors(
    spot: &Spot,
    scale: f32,
    primary: [f32; 2],
    clean: &[f32],
    inside: &[f32],
    broad: &[f32],
    red: &[f32],
    w: usize,
    h: usize,
) -> Vec<[f32; 2]> {
    if scale >= 0.7 {
        return Vec::new();
    }
    let radius = spot.radius * scale;
    let mut choices = Vec::new();
    for reach in [3.0, 4.5, 6.0, 9.0, 13.0, 18.0, 24.0, 32.0, 48.0, 64.0] {
        for n in 0..32 {
            let a = std::f32::consts::TAU * n as f32 / 32.0;
            let point = [
                (spot.x + a.cos() * spot.radius * reach).round(),
                (spot.y + a.sin() * spot.radius * reach).round(),
            ];
            if point[0] < 1.0
                || point[1] < 1.0
                || point[0] >= w as f32 - 1.0
                || point[1] >= h as f32 - 1.0
            {
                continue;
            }
            if let Some(cost) = donor_cost(spot, point, radius, reach, clean, inside, broad, red, w)
                .filter(|c| *c < 0.45)
            {
                choices.push((cost, point));
            }
        }
    }
    // A polar search can step over an otherwise clean source, particularly far
    // from a large repair. Sample a bounded grid across the actual skin crop too.
    let stride = ((w * h) as f32 / 4096.0)
        .sqrt()
        .max(radius * 0.5)
        .ceil()
        .max(1.0) as usize;
    for y in (stride / 2..h).step_by(stride) {
        for x in (stride / 2..w).step_by(stride) {
            let point = [x as f32, y as f32];
            let distance = (point[0] - spot.x).hypot(point[1] - spot.y);
            if distance < spot.radius + radius * 1.55 + 2.0 {
                continue;
            }
            if let Some(cost) = donor_cost(
                spot,
                point,
                radius,
                distance / spot.radius,
                clean,
                inside,
                broad,
                red,
                w,
            )
            .filter(|c| *c < 0.45)
            {
                choices.push((cost, point));
            }
        }
    }
    choices.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1[0].total_cmp(&b.1[0]))
            .then(a.1[1].total_cmp(&b.1[1]))
    });
    let mut accepted = vec![primary];
    for (_, point) in choices {
        if accepted
            .iter()
            .all(|old| (old[0] - point[0]).hypot(old[1] - point[1]) >= radius * 2.8)
        {
            accepted.push(point);
        }
        if accepted.len() >= 8 {
            break;
        }
    }
    if accepted.len() < 3 {
        Vec::new()
    } else {
        accepted
    }
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
    // Reuse the connected, feature-safe skin fill used by frequency healing. Small
    // segmentation holes must not keep a remaining mark outside the residual search.
    let expanded = surface_selection(&g, px, matte, SurfaceKind::Spots).map(|alpha| Matte {
        bounds: matte.bounds,
        width: matte.width,
        height: matte.height,
        alpha,
    });
    let search_matte = expanded.as_ref().unwrap_or(matte);
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
            mask[i] = search_matte.at(
                (x + x0) as f32 / px.width as f32,
                (y + y0) as f32 / px.height as f32,
            ) > 0.20;
            donor_mask[i] = confidence > 0.55;
        }
    }
    close_holes(&mut mask, w, h, g.d * 0.15);
    let mut exclusions = g.exclusions();
    // Precise spot repairs use the actual openings rather than blanket nose,
    // glabella and smile-line disks. Shape and ring-light checks reject creases;
    // a true lesion beside one still needs a complete clean surrounding ring.
    exclusions.truncate(4);
    exclusions.push(lip_protection(&g));
    for y in 0..h {
        for x in 0..w {
            let point = [(x + x0) as f32 + 0.5, (y + y0) as f32 + 0.5];
            if exclusions.iter().any(|e| e.contains(point))
                || (super::eye_guard::spot_weight(&g, point, settings) < 1.0)
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
    let eligible_count = eligible.len();
    for (position, spot) in eligible.into_iter().enumerate() {
        if out.blemishes.len() >= usize::from(settings.max_spots).min(900) {
            out.report.findings.push(format!("Spot repair limit reached: {} measured candidates remain for review. Increase Most spots per face to include more.", eligible_count - position));
            break;
        }
        let Some((source, source_scale)) = donor(spot, &clean, &inside, &broad, &broad_red, w, h)
        else {
            no_donor += 1;
            continue;
        };
        let centre = [spot.x + x0 as f32 + 0.5, spot.y + y0 as f32 + 0.5];
        // The detector measures the image after frequency healing, so this is a
        // confirmed remaining defect. Fully repair its core; feather only the
        // healthy surrounding skin. Uniform under-strength heals leave acne visible.
        let mut edit = base_edit(
            format!("{prefix}{index}-spot-deep-{}", out.blemishes.len()),
            if spot.chroma_only {
                Tool::SkinUniformity
            } else {
                Tool::PatchHeal
            },
            if spot.chroma_only { 0.8 } else { 1.0 },
            px,
            [centre[0], centre[1], spot.radii[0], spot.radii[1]],
        );
        edit.source = Some([
            (source[0] + x0 as f32 + 0.5) / px.width as f32,
            (source[1] + y0 as f32 + 0.5) / px.height as f32,
        ]);
        if spot.chroma_only {
            configure_pigment_repair(&mut edit, spot.radius, px.width.min(px.height));
        } else {
            edit.feather = 0.25;
            edit.texture_heal = true;
            edit.clean_ring_fit = spot.clean_ring_fit;
            edit.curved_heal = spot.curved_heal;
            edit.heal_samples.clone_from(&spot.heal_samples);
            edit.source_scale = source_scale;
            edit.texture_sources = texture_donors(
                spot,
                source_scale,
                source,
                &clean,
                &inside,
                &broad,
                &broad_red,
                w,
                h,
            )
            .into_iter()
            .map(|p| {
                [
                    (p[0] + x0 as f32 + 0.5) / px.width as f32,
                    (p[1] + y0 as f32 + 0.5) / px.height as f32,
                ]
            })
            .collect();
        }
        // No coarse matte: the entire disk was checked against the repaired skin mask.
        // Applying the unfilled matte here would protect the centre of a dark blemish.
        out.blemishes.push(edit);
    }
    out.report.spots_healed = out.blemishes.len();
    let pigments = out
        .blemishes
        .iter()
        .filter(|e| e.tool == Tool::SkinUniformity)
        .count();
    out.report.findings.push(format!("Deep blemish cleanup: {} local-light-matched texture repairs, {} targeted redness repairs retaining luminance and detail; {} marks kept, {} spots skipped without a clean donor. {}",
        out.report.spots_healed-pigments,pigments,out.report.marks_kept,no_donor,
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
fn surface_selection(
    g: &Geometry,
    px: &Pixels<'_>,
    matte: &Matte,
    kind: SurfaceKind,
) -> Option<Vec<u8>> {
    let blemish = !matches!(kind, SurfaceKind::Finish);
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
    let mut exclusions = vec![super::Capsule::disk(
        super::add(g.nose, g.v, g.d * 0.06),
        g.d * 0.20,
    )];
    if blemish {
        exclusions.clear();
    }
    let mut brow_areas = Vec::with_capacity(2);
    for eye in g.eyes {
        exclusions.push(super::Capsule::disk(eye, g.d * 0.25));
        let brow = super::add(eye, g.v, -g.d * 0.32);
        exclusions.push(super::Capsule {
            a: super::add(brow, g.u, -g.d * 0.28),
            b: super::add(brow, g.u, g.d * 0.28),
            r: g.d * 0.085,
        });
        brow_areas.push(super::Capsule::disk(brow, g.d * 0.24));
    }
    exclusions.push(lip_protection(g));
    // Brow hair wherever it actually is, with a margin of two cells or 4 % of the eye distance.
    // Hair is darker than the skin right around it - not than this face's typical skin: on
    // the shadowed side of a face the whole forehead is darker than that, and is still skin.
    let reach = (g.d * 0.2 / cell).max(2.0) as usize;
    let local_skin = |i: usize| {
        let (x, y) = (i % w, i / w);
        let mut around = Vec::with_capacity((2 * reach + 1).pow(2));
        for row in y.saturating_sub(reach)..(y + reach + 1).min(h) {
            for j in row * w + x.saturating_sub(reach)..row * w + (x + reach + 1).min(w) {
                if skin_like[j] {
                    around.push(super::luma(colours[j]));
                }
            }
        }
        quantile(around, 0.6).unwrap_or(skin_luma)
    };
    let not_brow: Vec<bool> = (0..w * h)
        .map(|i| {
            !(brow_areas.iter().any(|area| area.contains(centre(i)))
                && super::luma(colours[i]) < local_skin(i) * 0.62)
        })
        .collect();
    let to_brow = clearance(&not_brow, w, h);
    let brow_margin = (g.d * 0.04 / cell).max(2.0);
    for i in 0..w * h {
        let point = centre(i);
        let on_brow = to_brow[i] <= brow_margin && brow_areas.iter().any(|a| a.contains(point));
        if on_brow
            || exclusions.iter().any(|e| e.contains(point))
            || (blemish
                && if matches!(kind, SurfaceKind::Spots) {
                    super::eye_guard::nostril_opening_weight(g, point) < 1.0
                } else {
                    super::eye_guard::nostril_weight(g, point) < 1.0
                })
        {
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
    let alpha = surface_selection(&g, px, matte, SurfaceKind::Finish)?;
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

/// Blemish selection includes nose skin; broad finishing retains its separate nose guard.
pub(crate) fn blemish_surface_matte(
    face: &PortraitFace,
    px: &Pixels<'_>,
    matte: &Matte,
) -> Option<aura_recipe::retouch_tools::Matte> {
    let g = Geometry::new(face, px)?;
    if g.d < 40.0 {
        return None;
    }
    let alpha = surface_selection(&g, px, matte, SurfaceKind::Blemishes)?;
    let mut mask = aura_recipe::retouch_tools::Matte::encode(
        matte.bounds,
        matte.width as u32,
        matte.height as u32,
        &alpha,
    );
    mask.refine_edges = false;
    Some(mask)
}

/// The id of the matte [`surface_matte`] is stored under.
pub(crate) fn surface_matte_id(prefix: &str, index: usize) -> String {
    format!("{prefix}{index}-surface")
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

/// Frequency healing over the surface selection (ADR-0090): the tone under every compact
/// mark is rebuilt from the clean skin around it and pore detail stays where it is.
///
/// One operation for the whole face rather than one per spot, so it is not limited by the
/// operation budget and it reaches the marks no clean donor patch fits beside. Its strength
/// is how completely the tone is rebuilt; dark marks that are not also redder than the skin
/// around them are kept unless *Remove dark marks* is on.
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
        Tool::FrequencyHeal,
        1.0,
        px,
        matte,
        surface_matte_id(prefix, index),
    );
    // Below a fiftieth of the eye distance is pores; marks are larger.
    edit.radius = (g.d * 0.02 / px.width.min(px.height) as f32).clamp(0.0005, 0.05);
    edit.tone = settings.frequency_heal.clamp(0.0, 1.0);
    // A mark's own relief - its dark core and lit rim - goes with it; ordinary pore contrast
    // under it stays, and the texture graft restores what the repair cost.
    edit.texture = 0.25;
    edit.sensitivity = Some(settings.blemish_sensitivity.clamp(0.0, 1.0));
    edit.keep_dark_marks = !settings.remove_dark_marks;
    Some(edit)
}

/// The analysis pixels as they will be once `edit` (a frequency heal over `surface`) has run,
/// as packed sRGB: what the spot repairs that follow it are planned on, so they are spent on
/// what frequency healing left rather than on marks it has already rebuilt.
pub(crate) fn after_frequency_heal(
    px: &Pixels<'_>,
    edit: &aura_recipe::retouch_tools::Edit,
    surface: &aura_recipe::retouch_tools::Matte,
) -> Option<Vec<u8>> {
    let mut mattes = std::collections::BTreeMap::new();
    mattes.insert(edit.matte.clone()?, surface.clone());
    after_edits(px, std::slice::from_ref(edit), &mattes)
}

fn after_edits(
    px: &Pixels<'_>,
    edits: &[aura_recipe::retouch_tools::Edit],
    mattes: &std::collections::BTreeMap<String, aura_recipe::retouch_tools::Matte>,
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
    aura_render::retouch_tools::apply_with_mattes(&mut linear, px.width, px.height, edits, mattes);
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
    fn confined_pigment_repair_preserves_a_shadow_transition_and_its_surroundings() {
        let (w, h) = (512, 512);
        let mut rgb = vec![0_u8; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let color = if x < 256 {
                    [160, 125, 105]
                } else {
                    [90, 70, 56]
                };
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&color);
                if (x as f32 - 256.0).hypot(y as f32 - 240.0) < 5.0 {
                    rgb[(y * w + x) * 3] = rgb[(y * w + x) * 3].saturating_add(55);
                    rgb[(y * w + x) * 3 + 1] = rgb[(y * w + x) * 3 + 1].saturating_sub(16);
                }
            }
        }
        let pixels = Pixels::new(&rgb, w as u32, h as u32).unwrap();
        let mut edit = base_edit(
            "test-pigment".into(),
            Tool::SkinUniformity,
            0.8,
            &pixels,
            [256.0, 240.0, 9.0, 9.0],
        );
        edit.source = Some([290.0 / w as f32, 240.0 / h as f32]);
        configure_pigment_repair(&mut edit, 9.0, w.min(h));
        let before: Vec<f32> = rgb.iter().map(|v| f32::from(*v) / 255.0).collect();
        let mut after = before.clone();
        aura_render::retouch_tools::apply(&mut after, w, h, &[edit]);
        assert!(before
            .iter()
            .zip(&after)
            .any(|(a, b)| (a - b).abs() > 0.005));
        for (i, (a, b)) in before
            .chunks_exact(3)
            .zip(after.chunks_exact(3))
            .enumerate()
        {
            let lum = |p: &[f32]| p[0] * 0.2627 + p[1] * 0.6780 + p[2] * 0.0593;
            assert!(
                (lum(a) - lum(b)).abs() < 0.000_001,
                "shadow brightness changed"
            );
            if (i % w).abs_diff(256) > 10 || (i / w).abs_diff(240) > 10 {
                assert!(a == b, "pigment repair changed skin outside its region");
            }
        }
    }
    #[test]
    fn clean_texture_patches_between_polar_search_rays_are_found_by_the_bounded_grid() {
        let (w, h) = (256, 256);
        let mut clean = vec![false; w * h];
        for [cx, cy] in [[94_usize, 64_usize], [187, 114], [149, 192]] {
            for y in cy - 8..=cy + 8 {
                for x in cx - 8..=cx + 8 {
                    clean[y * w + x] = true;
                }
            }
        }
        let clean = clearance(&clean, w, h);
        let inside = clearance(&vec![true; w * h], w, h);
        let spot = Spot {
            x: 64.0,
            y: 64.0,
            radius: 10.0,
            radii: [10.0; 2],
            clean_ring_fit: true,
            curved_heal: false,
            chroma_only: false,
            heal_samples: Vec::new(),
            score: 1.0,
            red: 0.1,
            reference: [0.4, 0.1],
        };
        let donors = texture_donors(
            &spot,
            0.35,
            [94.0, 64.0],
            &clean,
            &inside,
            &vec![0.4; w * h],
            &vec![0.1; w * h],
            w,
            h,
        );
        assert_eq!(donors.len(), 3);
        assert!(donors
            .iter()
            .any(|p| (p[0] - 187.0).hypot(p[1] - 114.0) < 4.0));
        assert!(donors
            .iter()
            .any(|p| (p[0] - 149.0).hypot(p[1] - 192.0) < 4.0));
    }
    #[test]
    fn extra_texture_donors_are_clean_separated_and_bounded() {
        let (w, h) = (128, 128);
        let clean = clearance(&vec![true; w * h], w, h);
        let spot = Spot {
            x: 64.0,
            y: 64.0,
            radius: 10.0,
            radii: [10.0; 2],
            clean_ring_fit: true,
            curved_heal: false,
            chroma_only: false,
            heal_samples: Vec::new(),
            score: 1.0,
            red: 0.1,
            reference: [0.4, 0.1],
        };
        let donors = texture_donors(
            &spot,
            0.5,
            [94.0, 64.0],
            &clean,
            &clean,
            &vec![0.4; w * h],
            &vec![0.1; w * h],
            w,
            h,
        );
        assert!((3..=8).contains(&donors.len()));
        for (n, p) in donors.iter().enumerate() {
            assert!(donor_cost(
                &spot,
                *p,
                5.0,
                3.0,
                &clean,
                &clean,
                &vec![0.4; w * h],
                &vec![0.1; w * h],
                w
            )
            .is_some());
            for other in donors.iter().skip(n + 1) {
                assert!((p[0] - other[0]).hypot(p[1] - other[1]) >= 14.0);
            }
        }
        assert!(texture_donors(
            &spot,
            1.0,
            [94.0, 64.0],
            &clean,
            &clean,
            &vec![0.4; w * h],
            &vec![0.1; w * h],
            w,
            h
        )
        .is_empty());
    }
    #[test]
    fn larger_donor_search_uses_full_size_clean_texture_before_repeating_a_small_patch() {
        let (w, h) = (700, 80);
        let mut clean = vec![false; w * h];
        for y in 19..=61 {
            for x in 479..=521 {
                clean[y * w + x] = true;
            }
        }
        let clean = clearance(&clean, w, h);
        let inside = clearance(&vec![true; w * h], w, h);
        let spot = Spot {
            x: 20.0,
            y: 40.0,
            radius: 10.0,
            radii: [10.0; 2],
            clean_ring_fit: true,
            curved_heal: false,
            chroma_only: false,
            heal_samples: Vec::new(),
            score: 1.0,
            red: 0.1,
            reference: [0.4, 0.1],
        };
        let (point, scale) = donor(
            &spot,
            &clean,
            &inside,
            &vec![0.4; w * h],
            &vec![0.1; w * h],
            w,
            h,
        )
        .unwrap();
        assert_eq!(scale, 1.0);
        assert_eq!(point, [500.0, 40.0]);
        assert!(point[0] - spot.x > 24.0 * spot.radius);
    }
    #[test]
    fn donor_matches_surrounding_skin_instead_of_the_blemished_center() {
        let (w, h) = (128, 128);
        let mut broad = vec![0.6; w * h];
        let mut red = vec![0.1; w * h];
        broad[64 * w + 64] = 0.2;
        red[64 * w + 64] = 0.2;
        let mut clean = vec![true; w * h];
        disk(&mut clean, w, h, 64.0, 64.0, 12.0);
        let clean = clearance(&clean, w, h);
        let inside = clearance(&vec![true; w * h], w, h);
        let mut spot = Spot {
            x: 64.0,
            y: 64.0,
            radius: 10.0,
            radii: [10.0; 2],
            clean_ring_fit: true,
            curved_heal: false,
            chroma_only: false,
            heal_samples: Vec::new(),
            score: 1.0,
            red: 0.2,
            reference: [0.2, 0.2],
        };
        assert!(
            donor(&spot, &clean, &inside, &broad, &red, w, h).is_none(),
            "legacy center matching must reject the healthy donor"
        );
        spot.reference = [0.6, 0.1];
        assert!(
            donor(&spot, &clean, &inside, &broad, &red, w, h).is_some(),
            "match the measured surrounding skin without relaxing donor cleanliness"
        );
    }
    #[test]
    fn strongly_inflamed_touching_cluster_is_split_into_local_repairs() {
        let (w, h) = (160, 160);
        let mut lum = vec![0.4; w * h];
        let mut red = vec![0.1; w * h];
        for y in 77..=83 {
            for x in 50..=110 {
                lum[y * w + x] = 0.30;
                red[y * w + x] = 0.18;
            }
        }
        for cx in [60, 100] {
            for y in 76..=84 {
                for x in cx - 4..=cx + 4 {
                    if (x as f32 - cx as f32).hypot(y as f32 - 80.0) <= 4.0 {
                        lum[y * w + x] = 0.10;
                        red[y * w + x] = 0.25;
                    }
                }
            }
        }
        let spots = candidates(&lum, &red, &vec![true; w * h], w, h, 250.0, 0.85).0;
        for cx in [60.0, 100.0] {
            assert!(
                spots.iter().any(|s| distance([s.x, s.y], [cx, 80.0]) < 3.0),
                "Deep cores of a touching cluster must be repairable: {spots:?}"
            );
        }
    }
    #[test]
    fn elongated_red_lesion_beside_protected_skin_is_locally_healed() {
        let (w, h) = (160, 160);
        let mut lum = vec![0.4; w * h];
        let mut red = vec![0.1; w * h];
        let mask: Vec<_> = (0..w * h).map(|i| i % w < 90).collect();
        for y in 68..=92 {
            for x in 77..=83 {
                if ((x as f32 - 80.0) / 3.0).powi(2) + ((y as f32 - 80.0) / 12.0).powi(2) <= 1.0 {
                    lum[y * w + x] = 0.28;
                    red[y * w + x] = 0.18;
                }
            }
        }
        let spots = candidates(&lum, &red, &mask, w, h, 250.0, 0.85).0;
        assert!(
            spots
                .iter()
                .any(|s| distance([s.x, s.y], [80.0, 80.0]) < 3.0),
            "Elongated inflamed lesion missed beside protected skin: {spots:?}"
        );
        let spot = spots
            .iter()
            .find(|s| distance([s.x, s.y], [80.0, 80.0]) < 3.0)
            .unwrap();
        let bytes = [102_u8; 3].repeat(w * h);
        let px = Pixels::new(&bytes, w as u32, h as u32).unwrap();
        let mut edit = base_edit(
            "elliptical-test".into(),
            Tool::PatchHeal,
            1.0,
            &px,
            [spot.x + 0.5, spot.y + 0.5, spot.radii[0], spot.radii[1]],
        );
        edit.source = Some([40.5 / w as f32, 80.5 / h as f32]);
        edit.texture_heal = true;
        edit.feather = 0.25;
        let mut rendered: Vec<_> = lum.iter().flat_map(|v| [*v; 3]).collect();
        let before = rendered.clone();
        aura_render::retouch_tools::apply(&mut rendered, w, h, &[edit]);
        for i in 0..w * h {
            if lum[i] < 0.3 {
                assert!(
                    (rendered[i * 3] - 0.4).abs() < 0.006,
                    "lesion rim not healed"
                );
            }
            if !mask[i] {
                assert_eq!(&rendered[i * 3..i * 3 + 3], &before[i * 3..i * 3 + 3]);
            }
        }
        let mut holed = mask.clone();
        holed[80 * w + 80] = false;
        assert!(!ellipse_inside(&holed, w, h, [spot.x, spot.y], spot.radii));
    }
    #[test]
    fn dense_acne_is_not_silently_stopped_at_the_old_220_spot_limit() {
        let face = finish_face();
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        for y in (80..430).step_by(12) {
            for x in (80..430).step_by(12) {
                fill(&mut rgb, [x, y, x + 3, y + 3], [95, 45, 35]);
            }
        }
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        let settings = Settings {
            deep_blemish_cleanup: true,
            remove_dark_marks: true,
            keep_freckles: false,
            blemish_sensitivity: 0.85,
            max_spots: 220,
            ..Settings::default()
        };
        let limited = plan(&face, 0, &px, "test-", &settings, Some(&matte));
        let extended = plan(
            &face,
            0,
            &px,
            "test-",
            &Settings {
                max_spots: 512,
                ..settings
            },
            Some(&matte),
        );
        assert_eq!(limited.blemishes.len(), 220);
        assert!(limited
            .report
            .findings
            .iter()
            .any(|s| s.contains("candidates remain")));
        assert!(extended.blemishes.len() > 220);
        assert_eq!(limited.blemishes, extended.blemishes[..220]);
    }
    #[test]
    fn faint_red_pimple_is_found_with_contrast_analysis() {
        let (w, h) = (160, 160);
        let mut lum = vec![0.4; w * h];
        let mut red = vec![0.1; w * h];
        for y in 0..h {
            for x in 0..w {
                if (x as f32 - 80.0).hypot(y as f32 - 80.0) < 5.0 {
                    lum[y * w + x] = 0.39;
                    red[y * w + x] = 0.112;
                }
            }
        }
        let spots = candidates(&lum, &red, &vec![true; w * h], w, h, 250.0, 0.85).0;
        assert!(
            spots
                .iter()
                .any(|s| distance([s.x, s.y], [80.0, 80.0]) < 3.0),
            "Faint red pimple missed: {spots:?}"
        );
    }
    #[test]
    fn isolated_red_mark_beside_the_nose_gets_a_precise_native_repair() {
        let face = finish_face();
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        fill(&mut rgb, [199, 313, 203, 317], [100, 55, 42]);
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        let settings = Settings {
            deep_blemish_cleanup: true,
            remove_dark_marks: true,
            keep_freckles: false,
            blemish_sensitivity: 0.85,
            ..Settings::default()
        };
        let mut edits = plan(&face, 0, &px, "test-", &settings, Some(&matte)).blemishes;
        assert!(
            !edits.is_empty(),
            "the wing guard must not hide an isolated red skin mark"
        );
        let mut mattes = std::collections::BTreeMap::new();
        super::super::eye_guard::protect(
            &face,
            &px,
            edits.iter_mut(),
            &mut mattes,
            "guard",
            &settings,
        )
        .unwrap();
        let before: Vec<f32> = rgb.iter().map(|v| f32::from(*v) / 255.0).collect();
        let mut after = before.clone();
        aura_render::retouch_tools::apply_with_mattes(&mut after, 512, 512, &edits, &mattes);
        assert!(after[(315 * 512 + 201) * 3] > before[(315 * 512 + 201) * 3] + 0.1);
        for (x, y) in [(194, 205), (318, 205), (234, 296), (278, 296)] {
            assert_eq!(
                &before[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3],
                &after[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3]
            );
        }
    }
    #[test]
    fn upper_cheek_pimple_below_the_eye_is_repairable_without_touching_the_eye() {
        let face = finish_face();
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        fill(&mut rgb, [200, 258, 205, 263], [100, 55, 42]);
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        let settings = Settings {
            deep_blemish_cleanup: true,
            remove_dark_marks: true,
            keep_freckles: false,
            blemish_sensitivity: 0.85,
            ..Settings::default()
        };
        let mut edits = plan(&face, 0, &px, "test-", &settings, Some(&matte)).blemishes;
        assert!(
            !edits.is_empty(),
            "a broad orbital guard must not hide upper-cheek acne"
        );
        let mut mattes = std::collections::BTreeMap::new();
        super::super::eye_guard::protect(
            &face,
            &px,
            edits.iter_mut(),
            &mut mattes,
            "guard",
            &settings,
        )
        .unwrap();
        let before: Vec<_> = rgb.iter().map(|v| f32::from(*v) / 255.0).collect();
        let mut after = before.clone();
        aura_render::retouch_tools::apply_with_mattes(&mut after, 512, 512, &edits, &mattes);
        assert!(after[(260 * 512 + 202) * 3] > before[(260 * 512 + 202) * 3] + 0.1);
        for (x, y) in [(194, 205), (318, 205), (234, 296), (278, 296)] {
            assert_eq!(
                &before[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3],
                &after[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3]
            );
        }
    }
    #[test]
    fn compact_red_spot_on_a_smooth_light_gradient_is_repairable() {
        let (w, h) = (160, 160);
        let mut lum = Vec::new();
        let mut red = vec![0.1; w * h];
        for y in 0..h {
            for x in 0..w {
                let base = (0.4 + (x as f32 - 80.0) * 0.008).max(0.03);
                let d = (x as f32 - 80.0).hypot(y as f32 - 80.0);
                lum.push(if d < 6.0 { base * 0.65 } else { base });
                if d < 6.0 {
                    red[y * w + x] += 0.07;
                }
            }
        }
        let spots = candidates(&lum, &red, &vec![true; w * h], w, h, 250.0, 0.85).0;
        assert!(
            spots
                .iter()
                .any(|s| distance([s.x, s.y], [80.0, 80.0]) < 3.0),
            "Gradient lesion missed: {spots:?}"
        );
    }
    #[test]
    fn small_neutral_mark_next_to_a_shadow_boundary_needs_wider_light_support() {
        let (w, h) = (160, 160);
        let mut lum = vec![0.4; w * h];
        let red = vec![0.1; w * h];
        for y in 0..h {
            for x in 0..w {
                if x > 98 {
                    lum[y * w + x] = 0.18;
                }
                if (x as f32 - 80.0).hypot(y as f32 - 80.0) < 3.0 {
                    lum[y * w + x] = 0.30;
                }
            }
        }
        let spots = candidates(&lum, &red, &vec![true; w * h], w, h, 250.0, 0.85).0;
        assert!(
            !spots
                .iter()
                .any(|s| distance([s.x, s.y], [80.0, 80.0]) < 4.0),
            "a tiny neutral mark near a contour lacks enough light support: {spots:?}"
        );
    }
    #[test]
    fn red_lesions_with_pale_centres_are_not_discarded_as_bright_pores() {
        let (w, h) = (160, 160);
        for radius in [4.0_f32, 8.0, 13.0] {
            let mut lum = vec![0.4; w * h];
            let mut red = vec![0.1; w * h];
            for y in 0..h {
                for x in 0..w {
                    let d = (x as f32 - 80.0).hypot(y as f32 - 80.0);
                    if d < radius {
                        red[y * w + x] += 0.07 * (1.0 - d / radius).sqrt();
                    }
                    if d < radius * 0.6 {
                        lum[y * w + x] = 0.46;
                        red[y * w + x] = 0.1;
                    }
                }
            }
            let mask = vec![true; w * h];
            let spots = candidates(&lum, &red, &mask, w, h, 250.0, 0.85).0;
            assert!(
                spots
                    .iter()
                    .any(|s| distance([s.x, s.y], [80.0, 80.0]) < 3.0),
                "Red lesion of radius {radius} missed: {spots:?}"
            );
        }
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
            radii: [4.0; 2],
            clean_ring_fit: false,
            curved_heal: false,
            chroma_only: false,
            heal_samples: Vec::new(),
            reference: [0.4, 0.1],
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
    fn blemish_surface_fills_small_holes_on_nose_skin_and_excludes_nostrils() {
        let face = finish_face();
        let mut rgb = [150_u8, 100, 75].repeat(512 * 512);
        // A compact mark on the bridge, outside the nostril openings.
        fill(&mut rgb, [251, 253, 261, 263], [95, 55, 42]);
        let px = Pixels::new(&rgb, 512, 512).unwrap();
        let mut matte = Matte {
            bounds: face.bounds,
            width: 96,
            height: 128,
            alpha: vec![255; 96 * 128],
        };
        for y in 57..62 {
            for x in 45..51 {
                matte.alpha[y * 96 + x] = 0;
            }
        }
        let mask = blemish_surface_matte(&face, &px, &matte).unwrap();
        let settings = Settings {
            frequency_heal: 0.9,
            remove_dark_marks: true,
            blemish_sensitivity: 0.8,
            ..Settings::default()
        };
        let mut edit = frequency_heal(&face, 0, &px, "test-", &settings, &matte).unwrap();
        let mut mattes = std::collections::BTreeMap::from([(edit.matte.clone().unwrap(), mask)]);
        super::super::eye_guard::protect(
            &face,
            &px,
            std::iter::once(&mut edit),
            &mut mattes,
            "guard",
            &settings,
        )
        .unwrap();
        let before: Vec<f32> = rgb.iter().map(|v| f32::from(*v) / 255.).collect();
        let coverage = aura_render::retouch_tools::selection_mask_with_mattes(
            &before, 512, 512, &edit, &mattes,
        );
        assert!(
            coverage[258 * 512 + 256] > 0.8,
            "a small segmentation hole must not exclude nose acne"
        );
        let mut after = before.clone();
        aura_render::retouch_tools::apply_with_mattes(&mut after, 512, 512, &[edit], &mattes);
        assert!(
            after[(258 * 512 + 256) * 3] > before[(258 * 512 + 256) * 3],
            "nose mark must actually be repaired"
        );
        for (x, y) in [(194, 205), (318, 205), (234, 296), (278, 296)] {
            assert_eq!(coverage[y * 512 + x], 0.);
            assert_eq!(
                &after[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3],
                &before[(y * 512 + x) * 3..(y * 512 + x) * 3 + 3]
            );
        }
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
        let healthy = before[(325 * 512 + 240) * 3];
        assert!(
            (softened[spot] - healthy).abs() < 0.03,
            "a confirmed residual must reach the surrounding skin tone rather than leave a dark core: {} vs {healthy}",
            softened[spot]
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
            radii: [12.0; 2],
            clean_ring_fit: false,
            curved_heal: false,
            chroma_only: false,
            heal_samples: Vec::new(),
            reference: [0.4, 0.1],
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
    fn frequency_healing_and_the_graft_are_opt_in_share_one_selection_and_spare_features() {
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
            &surface_matte_id(prefix, 0),
        )
        .unwrap();
        let surface = surface_matte(&face, &px, &matte).unwrap();
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
        let id = surface_matte_id(prefix, 0);
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
