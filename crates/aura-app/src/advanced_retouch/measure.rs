//! What the advanced retouch measures before it decides anything. ADR-0093.
//!
//! Every function here reads pixels and returns numbers or proposals; none of them writes a
//! recipe. The orchestrator in [`super`] turns them into ordinary, editable retouch operations.
//! Each one has a refusal as well as an answer, because a stage that could not measure what
//! it needed must say so rather than act on a guess.
// Pixel indices are bounds-checked by construction (planes are `w * h` long and every loop is
// inside them); statistics convert freely between pixel counts and f32.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

use crate::portrait_features::Pixels;
use aura_core::contract::micro::{MAX_CLOTHING_STRENGTH, MAX_FLYAWAY_AREA, MAX_FLYAWAY_STRENGTH};
use aura_retouch::micro::{clothing, hair};
use aura_retouch::texture_guard::Frame;
use aura_vision::portrait::PortraitFace;
use aura_vision::skin;

/// Display-encoded Rec.709 luminance, `width * height` long.
#[must_use]
pub fn encoded_luma(px: &Pixels<'_>) -> Vec<f32> {
    let (w, h) = (px.width, px.height);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = px.encoded(x, y);
            out.push(p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722);
        }
    }
    out
}

/// Linear-light frame for the measured detectors in `aura-retouch`.
#[must_use]
pub fn linear_frame(px: &Pixels<'_>) -> Frame {
    let (w, h) = (px.width, px.height);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            rgb.extend(px.linear(x, y));
        }
    }
    Frame {
        rgb,
        width: w,
        height: h,
    }
}

/// A segmenter matte resampled to one coverage value per pixel of a `w x h` frame.
#[must_use]
pub fn matte_plane(m: &skin::Matte, w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0; w * h];
    let [l, t, r, b] = m.bounds;
    if w == 0 || h == 0 || r <= l || b <= t {
        return out;
    }
    let (x0, x1) = (
        (l * w as f32).floor() as usize,
        ((r * w as f32).ceil() as usize).min(w),
    );
    let (y0, y1) = (
        (t * h as f32).floor() as usize,
        ((b * h as f32).ceil() as usize).min(h),
    );
    for y in y0..y1 {
        for x in x0..x1 {
            out[y * w + x] = m.at((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
        }
    }
    out
}

/// Mean over a `(2r + 1)` square, clipped at the frame edge, from an integral image.
#[must_use]
pub fn box_mean(values: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if values.len() != w * h || w == 0 || h == 0 {
        return values.to_vec();
    }
    let stride = w + 1;
    let mut sum = vec![0.0_f64; stride * (h + 1)];
    for y in 0..h {
        let mut row = 0.0_f64;
        for x in 0..w {
            row += f64::from(values[y * w + x]);
            sum[(y + 1) * stride + x + 1] = sum[y * stride + x + 1] + row;
        }
    }
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        let (ya, yb) = (y.saturating_sub(r), (y + r + 1).min(h));
        for x in 0..w {
            let (xa, xb) = (x.saturating_sub(r), (x + r + 1).min(w));
            let total = sum[yb * stride + xb] - sum[ya * stride + xb] - sum[yb * stride + xa]
                + sum[ya * stride + xa];
            out[y * w + x] = (total / ((yb - ya) * (xb - xa)) as f64) as f32;
        }
    }
    out
}

/// Halve the resolution until the long edge is at most `long`, by box averaging.
fn downscale(values: &[f32], w: usize, h: usize, long: usize) -> (Vec<f32>, usize, usize) {
    let (mut v, mut w, mut h) = (values.to_vec(), w, h);
    while w.max(h) > long && w >= 4 && h >= 4 {
        let (nw, nh) = (w / 2, h / 2);
        let mut next = vec![0.0; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                next[y * nw + x] = (v[2 * y * w + 2 * x]
                    + v[2 * y * w + 2 * x + 1]
                    + v[(2 * y + 1) * w + 2 * x]
                    + v[(2 * y + 1) * w + 2 * x + 1])
                    * 0.25;
            }
        }
        (v, w, h) = (next, nw, nh);
    }
    (v, w, h)
}

fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    values.get(values.len() / 2).copied()
}

// ---- Stage 2: level horizon --------------------------------------------------------------

/// How far from level the photograph's own straight lines say it is.
#[derive(Debug, Clone, PartialEq)]
pub enum Tilt {
    /// Straight lines agree that the frame is level within a third of a degree.
    Level { lines: usize, degrees: f32 },
    /// Straight lines agree on a tilt: positive is clockwise.
    Tilted {
        degrees: f32,
        lines: usize,
        agreement: f32,
    },
    /// Not enough evidence, or lines that disagree; the frame is left as shot.
    Unsure(String),
}

/// Lines within this many degrees of horizontal or vertical are horizon or architecture.
const LINE_TOLERANCE_DEG: f32 = 6.0;
/// Below this the frame is already level.
const LEVEL_DEG: f32 = 0.35;
/// Above this a consistent angle reads as a deliberate Dutch angle, not an accident.
const DELIBERATE_DEG: f32 = 5.0;

/// Measure the tilt from long straight edges: the horizon, door frames, walls, a table.
///
/// A chain counts only when it is straight (it stays within about a pixel of its own chord),
/// long, and close to horizontal or vertical. Their length-weighted median is the tilt, and it
/// is acted on only when most of the measured length agrees with it - converging verticals of
/// a building lean in opposite directions and never agree, so a keystone is not mistaken for a
/// tilt.
///
/// `exclude` (one value per pixel of `px`) marks people: a chain that runs mostly over them -
/// a striped shirt, an arm, a strand of hair - is never evidence of level.
#[must_use]
pub fn tilt(px: &Pixels<'_>, exclude: Option<&[f32]>) -> Tilt {
    let luma = encoded_luma(px);
    let (luma, w, h) = downscale(&luma, px.width, px.height, 1024);
    let chains = aura_geometry::lens::track_edges(&luma, w, h);
    let on_person = |p: &[f32; 2]| -> f32 {
        exclude
            .filter(|plane| plane.len() == px.width * px.height)
            .map_or(0.0, |plane| {
                let x = ((p[0] * px.width as f32) as usize).min(px.width - 1);
                let y = ((p[1] * px.height as f32) as usize).min(px.height - 1);
                plane[y * px.width + x]
            })
    };
    let mut lines: Vec<(f32, f32)> = Vec::new();
    for chain in &chains {
        let (Some(a), Some(b)) = (chain.points.first(), chain.points.last()) else {
            continue;
        };
        let over = chain.points.iter().filter(|p| on_person(p) > 0.3).count();
        if over * 5 > chain.points.len() {
            continue;
        }
        let (dx, dy) = ((b[0] - a[0]) * w as f32, (b[1] - a[1]) * h as f32);
        let length = dx.hypot(dy);
        if length < 0.15 * w.min(h) as f32 {
            continue;
        }
        // Straightness: the furthest point from the chord, in pixels.
        let bow = chain
            .points
            .iter()
            .map(|p| {
                let (px_, py_) = ((p[0] - a[0]) * w as f32, (p[1] - a[1]) * h as f32);
                (px_ * dy - py_ * dx).abs() / length.max(1e-3)
            })
            .fold(0.0_f32, f32::max);
        if bow > (0.006 * length).max(1.5) {
            continue;
        }
        let deviation = if dx.abs() >= dy.abs() {
            let (dx, dy) = if dx < 0.0 { (-dx, -dy) } else { (dx, dy) };
            dy.atan2(dx).to_degrees()
        } else {
            let (dx, dy) = if dy < 0.0 { (-dx, -dy) } else { (dx, dy) };
            (-dx).atan2(dy).to_degrees()
        };
        if deviation.abs() <= LINE_TOLERANCE_DEG {
            lines.push((deviation, length));
        }
    }
    if lines.len() < 3 {
        return Tilt::Unsure(format!(
            "only {} long straight line{} to measure the horizon from",
            lines.len(),
            if lines.len() == 1 { "" } else { "s" }
        ));
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: f32 = lines.iter().map(|l| l.1).sum();
    let mut running = 0.0;
    let mut centre = 0.0;
    for (deviation, length) in &lines {
        running += length;
        if running >= total * 0.5 {
            centre = *deviation;
            break;
        }
    }
    let agreeing: Vec<&(f32, f32)> = lines
        .iter()
        .filter(|l| (l.0 - centre).abs() <= 0.5)
        .collect();
    let agreed: f32 = agreeing.iter().map(|l| l.1).sum();
    let agreement = agreed / total.max(1e-3);
    if agreeing.len() < 3 || agreement < 0.6 || agreed < 0.8 * w.min(h) as f32 {
        return Tilt::Unsure(format!(
            "{} straight lines disagree about level ({:.0}% agreement)",
            lines.len(),
            agreement * 100.0
        ));
    }
    if centre.abs() < LEVEL_DEG {
        return Tilt::Level {
            lines: agreeing.len(),
            degrees: centre,
        };
    }
    if centre.abs() > DELIBERATE_DEG {
        return Tilt::Unsure(format!(
            "the lines agree on {centre:+.1}°, which reads as a deliberate angle rather than an accident"
        ));
    }
    Tilt::Tilted {
        degrees: centre,
        lines: agreeing.len(),
        agreement,
    }
}

// ---- Stages 3 and 12: small marks healed from clean surroundings --------------------------

/// One healing-brush repair: a target ellipse and a clean donor beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Repair {
    /// Normalised centre and radii of the target ellipse.
    pub region: [f32; 4],
    /// Normalised donor centre.
    pub source: [f32; 2],
    /// How far the mark departed from its surroundings, `0..1`.
    pub departure: f32,
    /// `lint`, `thread` or `stain`.
    pub kind: &'static str,
}

/// What a mark search found and why some were left alone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkSearch {
    pub repairs: Vec<Repair>,
    pub found: usize,
    /// Refused because the surface around them is patterned (lace, weave, foliage, brick).
    pub textured: usize,
    /// Refused because they are larger than a small distraction: an object, not a speck.
    pub too_large: usize,
    /// Refused because no clean donor of the same surface was close enough.
    pub no_donor: usize,
    /// Refused because they touch something that is not the surface (hair ends).
    pub excluded: usize,
    /// So many candidates that they are the surface's own texture; none was touched.
    pub pattern: bool,
}

fn mean_std(luma: &[f32], w: usize, h: usize, c: [f32; 2], r: [f32; 2]) -> Option<(f32, f32)> {
    let (cx, cy) = (c[0] * w as f32, c[1] * h as f32);
    let (rx, ry) = ((r[0] * w as f32).max(1.0), (r[1] * h as f32).max(1.0));
    let (mut s, mut s2, mut n) = (0.0_f32, 0.0_f32, 0.0_f32);
    let step = ((rx.max(ry) / 6.0).floor() as usize).max(1);
    let mut y = (cy - ry).floor().max(0.0) as usize;
    while (y as f32) <= cy + ry && y < h {
        let mut x = (cx - rx).floor().max(0.0) as usize;
        while (x as f32) <= cx + rx && x < w {
            let (u, v) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
            if u * u + v * v <= 1.0 {
                let l = luma[y * w + x];
                s += l;
                s2 += l * l;
                n += 1.0;
            }
            x += step;
        }
        y += step;
    }
    (n >= 3.0).then(|| {
        let m = s / n;
        (m, (s2 / n - m * m).max(0.0).sqrt())
    })
}

/// Find small lint, threads, stains or dust on `region` and plan a healing repair for each.
///
/// The detector is phase 21's clothing anomaly search, which refuses on patterned surfaces;
/// this adds an area ceiling and a donor that is the same clean surface beside the mark.
#[must_use]
pub fn marks(
    frame: &Frame,
    region: &[f32],
    exclude: Option<&[f32]>,
    max_area: f32,
    limit: usize,
) -> MarkSearch {
    let (w, h) = (frame.width, frame.height);
    let mut out = MarkSearch::default();
    if w < 16 || h < 16 || region.len() != w * h {
        return out;
    }
    let luma: Vec<f32> = frame
        .rgb
        .chunks_exact(3)
        .map(|p| p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722)
        .collect();
    let found = clothing::detect(frame, region);
    out.found = found.len();
    // As many candidates as the detector will report is not a set of marks: it is the weave,
    // the grain or the texture of the surface, and the honest answer is to touch none of it.
    if found.len() >= clothing::MAX_CANDIDATES {
        out.textured = found.len();
        out.pattern = true;
        return out;
    }
    let boxes: Vec<[f32; 4]> = found
        .iter()
        .map(|m| [m.region.x, m.region.y, m.region.w, m.region.h])
        .collect();
    let covered = |p: [f32; 2]| -> f32 {
        let (x, y) = ((p[0] * w as f32) as usize, (p[1] * h as f32) as usize);
        if x >= w || y >= h {
            0.0
        } else {
            region[y * w + x]
        }
    };
    for mark in &found {
        if out.repairs.len() >= limit {
            break;
        }
        if mark.fabric_busy {
            out.textured += 1;
            continue;
        }
        let area = mark.region.w * mark.region.h;
        if mark.too_large || area > max_area {
            out.too_large += 1;
            continue;
        }
        if !mark.is_actionable() {
            continue;
        }
        // Hair ends lying on a collar or a backdrop are hair, not lint or dust.
        if let Some(plane) = exclude {
            let x0 = ((mark.region.x - mark.region.w) * w as f32).max(0.0) as usize;
            let y0 = ((mark.region.y - mark.region.h) * h as f32).max(0.0) as usize;
            let x1 = (((mark.region.x + 2.0 * mark.region.w) * w as f32) as usize + 1).min(w);
            let y1 = (((mark.region.y + 2.0 * mark.region.h) * h as f32) as usize + 1).min(h);
            if plane.len() == w * h && (y0..y1).any(|y| (x0..x1).any(|x| plane[y * w + x] > 0.05)) {
                out.excluded += 1;
                continue;
            }
        }
        let centre = [
            mark.region.x + mark.region.w * 0.5,
            mark.region.y + mark.region.h * 0.5,
        ];
        let radius = [
            (mark.region.w * 0.8 + 1.5 / w as f32).max(0.0012),
            (mark.region.h * 0.8 + 1.5 / h as f32).max(0.0012),
        ];
        // The tone the repair must match: a ring just outside the target.
        let ring = [
            [centre[0] + radius[0] * 1.6, centre[1]],
            [centre[0] - radius[0] * 1.6, centre[1]],
            [centre[0], centre[1] + radius[1] * 1.6],
            [centre[0], centre[1] - radius[1] * 1.6],
        ];
        let ring: Vec<f32> = ring
            .iter()
            .filter_map(|p| mean_std(&luma, w, h, *p, [radius[0] * 0.5, radius[1] * 0.5]))
            .map(|(m, _)| m)
            .collect();
        if ring.is_empty() {
            out.no_donor += 1;
            continue;
        }
        let ring_mean = ring.iter().sum::<f32>() / ring.len() as f32;
        let mut best: Option<([f32; 2], f32)> = None;
        for scale in [2.6_f32, 3.4] {
            for (dx, dy) in [
                (1.0_f32, 0.0_f32),
                (-1.0, 0.0),
                (0.0, 1.0),
                (0.0, -1.0),
                (0.707, 0.707),
                (-0.707, 0.707),
                (0.707, -0.707),
                (-0.707, -0.707),
            ] {
                let d = [
                    centre[0] + dx * radius[0] * scale,
                    centre[1] + dy * radius[1] * scale,
                ];
                if d[0] < radius[0]
                    || d[1] < radius[1]
                    || d[0] > 1.0 - radius[0]
                    || d[1] > 1.0 - radius[1]
                {
                    continue;
                }
                // The donor must be the same surface all the way across, and no other mark.
                let inside = [
                    d,
                    [d[0] + radius[0], d[1]],
                    [d[0] - radius[0], d[1]],
                    [d[0], d[1] + radius[1]],
                    [d[0], d[1] - radius[1]],
                ]
                .iter()
                .all(|p| covered(*p) >= 0.8);
                let clear = boxes.iter().all(|b| {
                    d[0] + radius[0] < b[0]
                        || d[0] - radius[0] > b[0] + b[2]
                        || d[1] + radius[1] < b[1]
                        || d[1] - radius[1] > b[1] + b[3]
                });
                if !inside || !clear {
                    continue;
                }
                let Some((m, s)) = mean_std(&luma, w, h, d, radius) else {
                    continue;
                };
                let score = (m - ring_mean).abs() / ring_mean.max(0.01) + s * 4.0;
                if best.is_none_or(|(_, b)| score < b) {
                    best = Some((d, score));
                }
            }
        }
        let Some((source, score)) = best else {
            out.no_donor += 1;
            continue;
        };
        if score > 0.35 {
            out.no_donor += 1;
            continue;
        }
        out.repairs.push(Repair {
            region: [centre[0], centre[1], radius[0], radius[1]],
            source,
            departure: mark.departure,
            kind: match mark.kind {
                aura_core::contract::micro::ClothingIssue::Thread => "thread",
                aura_core::contract::micro::ClothingIssue::Stain => "stain",
                _ => "lint",
            },
        });
    }
    out
}

/// The strength a mark repair is applied at.
#[must_use]
pub fn repair_amount(departure: f32) -> f32 {
    (0.55 + departure * 3.0).clamp(0.55, MAX_CLOTHING_STRENGTH)
}

// ---- Stage 4: stray hair -----------------------------------------------------------------

/// A stray strand to pull toward the quiet background behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct Stray {
    pub region: [f32; 4],
    pub amount: f32,
}

/// More candidates than this around the hair is its own soft edge, not stray strands.
pub const MAX_STRAY_CANDIDATES: usize = 40;
/// A stray must stand clear of the hair mass by this share of the frame's short side.
pub const MIN_STRAY_DISTANCE: f32 = 0.003;

/// What the stray-hair search found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StraySearch {
    pub strays: Vec<Stray>,
    pub found: usize,
    /// Left alone because the background behind them has detail of its own.
    pub busy: usize,
    /// The candidates were the hair's own soft edge, so none was touched.
    pub soft_edge: bool,
}

/// Stray strands just outside the hair mass, over a quiet background (phase 21's detector).
///
/// Strands inside the hair are hair and are never touched; strands over foliage, a crowd or a
/// patterned wall are indistinguishable from it and are left; candidates that are not thin and
/// clear of the hair, or a silhouette that yields dozens of them, are the hair's own edge.
/// Contrast is reduced, never removed, and the total area is capped.
#[must_use]
pub fn strays(frame: &Frame, hair_plane: &[f32]) -> StraySearch {
    let found = hair::detect(frame, hair_plane);
    let mut out = StraySearch {
        found: found.len(),
        ..StraySearch::default()
    };
    let (w, h) = (frame.width as f32, frame.height as f32);
    out.busy = found.iter().filter(|s| s.background_busy).count();
    // Measured on real portraits, a soft hair edge yields hundreds of candidates all along the
    // silhouette. That is the hair's own edge, and fading it notches the outline; only a
    // silhouette with a handful of clearly separate strands is cleaned.
    if found.len() > MAX_STRAY_CANDIDATES {
        out.soft_edge = true;
        return out;
    }
    let mut area = 0.0;
    for strand in &found {
        let (sw, sh) = (strand.region.w * w, strand.region.h * h);
        let thin = sw.max(sh) >= 4.0 * sw.min(sh).max(1.0);
        if strand.background_busy
            || !strand.is_actionable()
            || !thin
            || strand.distance < MIN_STRAY_DISTANCE
            || out.strays.len() >= 12
        {
            continue;
        }
        let a = strand.region.w * strand.region.h;
        if area + a > MAX_FLYAWAY_AREA {
            break;
        }
        area += a;
        out.strays.push(Stray {
            region: [
                strand.region.x + strand.region.w * 0.5,
                strand.region.y + strand.region.h * 0.5,
                (strand.region.w * 0.6 + 1.0 / w).max(0.001),
                (strand.region.h * 0.6 + 1.0 / h).max(0.001),
            ],
            amount: (strand.contrast / 0.12 * MAX_FLYAWAY_STRENGTH)
                .clamp(0.3, MAX_FLYAWAY_STRENGTH),
        });
    }
    out
}

// ---- Stage 13: jewellery and hot reflections ---------------------------------------------

/// A clipped specular reflection on the outfit or jewellery.
#[derive(Debug, Clone, PartialEq)]
pub struct Hot {
    pub region: [f32; 4],
    pub peak: f32,
}

/// What the reflection search decided.
#[derive(Debug, Clone, PartialEq)]
pub enum Reflections {
    Found(Vec<Hot>),
    /// So many small reflections that they are the garment (sequins, crystals, lamé).
    Sparkle(usize),
}

/// Small, clipped specular hot spots on a person's outfit and jewellery, outside the face.
///
/// A professional tames a jewellery reflection that has burnt to white; it does not remove the
/// sparkle. Only reflections that are both nearly white and much brighter than the pixels
/// around them qualify, faces (catchlights, teeth) are excluded, and a garment covered in
/// them is reported as sparkle and left alone.
///
/// `background` is the segmented background: a bright patch of it seen through a gap between
/// an arm and a sleeve is not a reflection on anything the person wears.
#[must_use]
pub fn reflections(
    px: &Pixels<'_>,
    person: &[f32],
    background: Option<&[f32]>,
    faces: &[PortraitFace],
) -> Reflections {
    let (w, h) = (px.width, px.height);
    if person.len() != w * h || w < 16 || h < 16 {
        return Reflections::Found(Vec::new());
    }
    let luma = encoded_luma(px);
    let local = box_mean(&luma, w, h, 6);
    let in_face = |x: usize, y: usize| {
        let (fx, fy) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
        faces.iter().any(|f| {
            let [l, t, r, b] = f.bounds;
            let (mx, my) = ((r - l) * 0.1, (b - t) * 0.1);
            fx > l - mx && fx < r + mx && fy > t - my && fy < b + my
        })
    };
    let behind = |i: usize| {
        background
            .filter(|b| b.len() == w * h)
            .map_or(0.0, |b| b[i])
    };
    let hot = |i: usize| {
        luma[i] >= 0.94 && luma[i] - local[i] >= 0.2 && person[i] >= 0.8 && behind(i) < 0.2
    };
    let max_area = ((w * h) as f32 * 0.00006).max(4.0) as usize;
    let mut seen = vec![false; w * h];
    let mut spots = Vec::new();
    let mut count = 0_usize;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if seen[i] || !hot(i) || in_face(x, y) {
                continue;
            }
            let mut stack = vec![i];
            seen[i] = true;
            let (mut x0, mut y0, mut x1, mut y1) = (x, y, x, y);
            let (mut area, mut peak, mut top) = (0_usize, 0.0_f32, 0.0_f32);
            while let Some(j) = stack.pop() {
                area += 1;
                let (jx, jy) = (j % w, j / w);
                x0 = x0.min(jx);
                x1 = x1.max(jx);
                y0 = y0.min(jy);
                y1 = y1.max(jy);
                peak = peak.max(luma[j] - local[j]);
                top = top.max(luma[j]);
                // Wrapping below zero lands far outside the frame and is skipped like any other
                // neighbour beyond the edge.
                let (l, r, u, d) = (jx.wrapping_sub(1), jx + 1, jy.wrapping_sub(1), jy + 1);
                for (nx, ny) in [
                    (l, jy),
                    (r, jy),
                    (jx, u),
                    (jx, d),
                    (l, u),
                    (r, d),
                    (l, d),
                    (r, u),
                ] {
                    if nx >= w || ny >= h {
                        continue;
                    }
                    let k = ny * w + nx;
                    if !seen[k] && hot(k) {
                        seen[k] = true;
                        stack.push(k);
                    }
                }
            }
            let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
            if area < 2 || area > max_area || bw.max(bh) > 4 * bw.min(bh) + 2 {
                continue;
            }
            // A reflection is a point of light, darker on every side. A white stripe, a collar
            // or a white shirt carries on past the bright spot and is not one.
            let (cx, cy) = ((x0 + x1) as f32 * 0.5, (y0 + y1) as f32 * 0.5);
            let reach = bw.max(bh) as f32 * 0.5 + 3.0;
            let darker = (0..12)
                .filter(|k| {
                    let a = *k as f32 * std::f32::consts::TAU / 12.0;
                    let (x, y) = (
                        (cx + reach * a.cos()).round(),
                        (cy + reach * a.sin()).round(),
                    );
                    x >= 0.0
                        && y >= 0.0
                        && (x as usize) < w
                        && (y as usize) < h
                        && luma[y as usize * w + x as usize] <= top - 0.15
                })
                .count();
            if darker < 10 {
                continue;
            }
            count += 1;
            let r = (bw.max(bh) as f32 * 0.5 * 2.0 + 2.0).max(3.0);
            spots.push(Hot {
                region: [
                    (x0 + x1 + 1) as f32 * 0.5 / w as f32,
                    (y0 + y1 + 1) as f32 * 0.5 / h as f32,
                    r / w as f32,
                    r / h as f32,
                ],
                peak,
            });
        }
    }
    if count > 40 {
        return Reflections::Sparkle(count);
    }
    spots.sort_by(|a, b| b.peak.total_cmp(&a.peak));
    spots.truncate(10);
    Reflections::Found(spots)
}

// ---- Stage 14: background toning ---------------------------------------------------------

/// A backdrop brighter than this is high key by design and is never toned down.
pub const HIGH_KEY: f32 = 0.85;

/// How much brighter the background is than the subject's skin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackgroundTone {
    /// Median display luminance of the background.
    pub background: f32,
    /// Median display luminance of the subject's face skin.
    pub subject: f32,
    /// Fraction of linear brightness the background is lowered by, `0` when none.
    pub reduction: f32,
}

/// Measure whether the background pulls the eye away from the person.
///
/// A background clearly brighter than the person's own face draws the eye first; it is
/// lowered gently, never below the subject, and a background already darker is left exactly
/// as it was lit.
#[must_use]
pub fn background_tone(
    px: &Pixels<'_>,
    background: &[f32],
    face: &[f32],
) -> Option<BackgroundTone> {
    let (w, h) = (px.width, px.height);
    if background.len() != w * h || face.len() != w * h {
        return None;
    }
    let luma = encoded_luma(px);
    let mut bg: Vec<f32> = (0..w * h)
        .filter(|i| background[*i] >= 0.9)
        .map(|i| luma[i])
        .collect();
    let mut skin: Vec<f32> = (0..w * h)
        .filter(|i| face[*i] >= 0.9)
        .map(|i| luma[i])
        .collect();
    if bg.len() < (w * h) / 50 || skin.len() < 64 {
        return None;
    }
    let (background, subject) = (median(&mut bg)?, median(&mut skin)?);
    // A white or near-white backdrop is a high-key look, not a distraction: lowering it only
    // turns white into grey.
    let reduction = if background > subject + 0.06 && background <= HIGH_KEY {
        ((background - subject) * 0.5).clamp(0.04, 0.12)
    } else {
        0.0
    };
    Some(BackgroundTone {
        background,
        subject,
        reduction,
    })
}

/// Where Evoto's automatic portrait pass lands a plain grey studio backdrop, display-encoded.
///
/// Measured from Evoto's own "Beauty & Fashion" before/after on its homepage: the seamless grey
/// behind three people went from about 0.81 to about 0.915 (sRGB code 206 to 234), the same
/// factor on red, green and blue, with its light fall-off kept and nothing clipped. ADR-0109.
pub const BACKDROP_TARGET: f32 = 0.915;

/// A plain studio backdrop's reading, and the lift that takes it to [`BACKDROP_TARGET`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropLift {
    /// Median display luminance of the backdrop as photographed.
    pub background: f32,
    /// Fine texture: the 75th percentile of the luminance's departure from its 5-pixel mean.
    pub texture: f32,
    /// Spread between the brightest and darkest channel of the median backdrop colour.
    pub chroma: f32,
    /// Linear gain the lift applies, `1` when none.
    pub gain: f32,
}

/// A backdrop is lifted only when it is plain studio paper or a plain wall: fine texture under
/// about three code values.
pub const PLAIN_TEXTURE: f32 = 0.012;
/// ...and close to neutral, so a coloured backdrop keeps its colour's depth.
pub const NEUTRAL_CHROMA: f32 = 0.06;
/// ...and not a deliberate low-key backdrop.
pub const LOW_KEY: f32 = 0.45;

fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Measure a backdrop and solve the lift that brings it to [`BACKDROP_TARGET`].
///
/// `exposure_gain` is the linear brightening the foundation stage already applies to the whole
/// frame, so the lift makes up only what is left. `strength` (`0..1`) scales the way there in
/// linear light. `None` when there is too little backdrop to read; a reading with `gain == 1`
/// when the backdrop is textured, coloured, low-key or already bright, so the report can say why.
#[must_use]
pub fn backdrop_lift(
    px: &Pixels<'_>,
    background: &[f32],
    exposure_gain: f32,
    strength: f32,
) -> Option<BackdropLift> {
    let (w, h) = (px.width, px.height);
    if background.len() != w * h || w < 8 || h < 8 {
        return None;
    }
    let luma = encoded_luma(px);
    let mean = box_mean(&luma, w, h, 2);
    let inside: Vec<usize> = (0..w * h).filter(|i| background[*i] >= 0.9).collect();
    if inside.len() < (w * h) / 20 {
        return None;
    }
    let mut level: Vec<f32> = inside.iter().map(|i| luma[*i]).collect();
    let mut fine: Vec<f32> = inside.iter().map(|i| (luma[*i] - mean[*i]).abs()).collect();
    let bg = median(&mut level)?;
    fine.sort_by(f32::total_cmp);
    let texture = fine.get(fine.len() * 3 / 4).copied().unwrap_or(1.0);
    let channel = |c: usize| {
        let mut v: Vec<f32> = inside.iter().map(|i| px.encoded(i % w, i / w)[c]).collect();
        median(&mut v).unwrap_or(0.0)
    };
    let rgb = [channel(0), channel(1), channel(2)];
    let chroma = rgb.iter().copied().fold(0.0, f32::max) - rgb.iter().copied().fold(1.0, f32::min);
    let mut reading = BackdropLift {
        background: bg,
        texture,
        chroma,
        gain: 1.0,
    };
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 || texture > PLAIN_TEXTURE || chroma > NEUTRAL_CHROMA || bg < LOW_KEY {
        return Some(reading);
    }
    let after = srgb_decode(bg) * exposure_gain.max(0.01);
    let target = srgb_decode(BACKDROP_TARGET);
    if after >= target * 0.97 {
        return Some(reading);
    }
    // Interpolated in log space, so half strength is half the stops.
    let full = target / after;
    reading.gain = full.powf(strength).min(2.0_f32.powf(0.75));
    Some(reading)
}

/// The dodge amount that raises linear brightness by `gain` (the dodge is +0.75 EV at 100 %).
#[must_use]
pub fn dodge_amount(gain: f32) -> f32 {
    ((gain - 1.0) / (2.0_f32.powf(0.75) - 1.0)).clamp(0.0, 0.95)
}

/// The burn amount that lowers linear brightness by `reduction` (the burn is -0.75 EV at 100 %).
#[must_use]
pub fn burn_amount(reduction: f32) -> f32 {
    (reduction / (1.0 - (-0.75_f32).exp2())).clamp(0.05, 0.4)
}

// ---- Stage 12 of the guide: the dodge-and-burn visual aid -----------------------------------

/// The black-and-white, contrast-boosted reading of one face's skin that a retoucher uses to
/// see uneven light: how much it varies at a micro and a medium scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualAid {
    pub micro: f32,
    pub medium: f32,
}

/// Measure luminance irregularity on the skin, with colour removed and contrast exaggerated.
#[must_use]
pub fn visual_aid(px: &Pixels<'_>, skin_plane: &[f32], face: &PortraitFace) -> Option<VisualAid> {
    let (w, h) = (px.width, px.height);
    if skin_plane.len() != w * h {
        return None;
    }
    let face_px = (face.bounds[2] - face.bounds[0]) * w as f32;
    if face_px < 40.0 {
        return None;
    }
    let luma = encoded_luma(px);
    let core: Vec<usize> = (0..w * h).filter(|i| skin_plane[*i] >= 0.85).collect();
    if core.len() < 200 {
        return None;
    }
    let mean = core.iter().map(|i| luma[*i]).sum::<f32>() / core.len() as f32;
    // Black and white, then a steep contrast curve around the skin's own mean.
    let aid: Vec<f32> = luma
        .iter()
        .map(|l| (0.5 + (l - mean) * 3.0).clamp(0.0, 1.0))
        .collect();
    let r1 = ((face_px * 0.012).round() as usize).max(1);
    let r2 = ((face_px * 0.06).round() as usize).max(r1 + 2);
    let fine = box_mean(&aid, w, h, r1);
    let broad = box_mean(&aid, w, h, r2);
    let n = core.len() as f32;
    let micro = core.iter().map(|i| (aid[*i] - fine[*i]).abs()).sum::<f32>() / n;
    let medium = core
        .iter()
        .map(|i| (fine[*i] - broad[*i]).abs())
        .sum::<f32>()
        / n;
    Some(VisualAid { micro, medium })
}

/// A multiplier for a dodge-and-burn strength from what the visual aid measured against a
/// typical reading: more uneven skin gets a little more, even skin a little less.
#[must_use]
pub fn aid_gain(measured: f32, typical: f32) -> f32 {
    (measured / typical.max(1e-4)).sqrt().clamp(0.7, 1.3)
}

// ---- Stage 18: quality control -----------------------------------------------------------

/// Display luminance of 8-bit RGB.
fn luma8(rgb: &[u8]) -> Vec<f32> {
    rgb.chunks_exact(3)
        .map(|p| {
            (f32::from(p[0]) * 0.2126 + f32::from(p[1]) * 0.7152 + f32::from(p[2]) * 0.0722) / 255.0
        })
        .collect()
}

/// Fine-texture energy kept on the skin: the mean high-pass magnitude after, over before.
#[must_use]
pub fn texture_retention(
    before: &[u8],
    after: &[u8],
    w: usize,
    h: usize,
    skin_plane: &[f32],
) -> Option<f32> {
    if before.len() != w * h * 3 || after.len() != before.len() || skin_plane.len() != w * h {
        return None;
    }
    let (a, b) = (luma8(before), luma8(after));
    let (la, lb) = (box_mean(&a, w, h, 1), box_mean(&b, w, h, 1));
    let (mut ea, mut eb, mut n) = (0.0_f32, 0.0_f32, 0_usize);
    for i in 0..w * h {
        if skin_plane[i] >= 0.85 {
            ea += (a[i] - la[i]).abs();
            eb += (b[i] - lb[i]).abs();
            n += 1;
        }
    }
    (n >= 200 && ea > 1e-4).then(|| eb / ea)
}

/// How far the skin's average colour moved, in chromaticity units (`r/(r+g+b)` and
/// `b/(r+g+b)`), and its luminance ratio.
#[must_use]
pub fn skin_shift(
    before: &[u8],
    after: &[u8],
    w: usize,
    h: usize,
    skin_plane: &[f32],
) -> Option<(f32, f32)> {
    if before.len() != w * h * 3 || after.len() != before.len() || skin_plane.len() != w * h {
        return None;
    }
    let mean = |rgb: &[u8]| {
        let (mut s, mut n) = ([0.0_f32; 3], 0.0_f32);
        for i in 0..w * h {
            if skin_plane[i] >= 0.85 {
                for c in 0..3 {
                    s[c] += f32::from(rgb[i * 3 + c]);
                }
                n += 1.0;
            }
        }
        (n >= 200.0).then(|| s.map(|v| v / n))
    };
    let (a, b) = (mean(before)?, mean(after)?);
    let chroma = |p: [f32; 3]| {
        let t = (p[0] + p[1] + p[2]).max(1.0);
        [p[0] / t, p[2] / t]
    };
    let (ca, cb) = (chroma(a), chroma(b));
    let la = a[0] * 0.2126 + a[1] * 0.7152 + a[2] * 0.0722;
    let lb = b[0] * 0.2126 + b[1] * 0.7152 + b[2] * 0.0722;
    Some(((ca[0] - cb[0]).hypot(ca[1] - cb[1]), lb / la.max(1.0)))
}

/// Share of pixels with a channel at the top of the 8-bit range.
#[must_use]
pub fn clipped(rgb: &[u8]) -> f32 {
    let n = rgb.len() / 3;
    if n == 0 {
        return 0.0;
    }
    rgb.chunks_exact(3)
        .filter(|p| p.iter().any(|v| *v >= 254))
        .count() as f32
        / n as f32
}

/// The retouch seen in a mirror: how much more one half of a face was changed than the other.
/// A flipped image shows a one-sided retouch at once; `1` is perfectly balanced.
#[must_use]
pub fn mirror_balance(
    before: &[u8],
    after: &[u8],
    w: usize,
    h: usize,
    face: &PortraitFace,
) -> Option<(f32, f32)> {
    if before.len() != w * h * 3 || after.len() != before.len() {
        return None;
    }
    let [l, t, r, b] = face.bounds;
    let (x0, x1) = ((l * w as f32) as usize, ((r * w as f32) as usize).min(w));
    let (y0, y1) = ((t * h as f32) as usize, ((b * h as f32) as usize).min(h));
    if x1 <= x0 + 8 || y1 <= y0 + 8 {
        return None;
    }
    let mid = usize::midpoint(x0, x1);
    let (mut left, mut right) = (0.0_f32, 0.0_f32);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * 3;
            let d: f32 = (0..3)
                .map(|c| (f32::from(after[i + c]) - f32::from(before[i + c])).abs())
                .sum::<f32>()
                / 765.0;
            if x < mid {
                left += d;
            } else {
                right += d;
            }
        }
    }
    let area = ((x1 - x0) * (y1 - y0)) as f32;
    let change = (left + right) / area;
    Some((left.max(right) / left.min(right).max(1e-4), change))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(rgb: &[u8], w: u32, h: u32) -> Pixels<'_> {
        Pixels::new(rgb, w, h).unwrap()
    }

    /// A frame of grey with dark straight lines tilted by `degrees` (clockwise positive).
    fn lines(degrees: f32, w: usize, h: usize) -> Vec<u8> {
        let (s, c) = degrees.to_radians().sin_cos();
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (x as f32 - w as f32 / 2.0, y as f32 - h as f32 / 2.0);
                // Coordinate across the lines after undoing the rotation.
                let v = -px * s + py * c;
                let on = (v.rem_euclid(60.0)) < 3.0;
                rgb.extend(if on { [40, 40, 40] } else { [190, 190, 190] });
            }
        }
        rgb
    }

    #[test]
    fn a_tilted_frame_is_measured_and_a_level_one_is_left() {
        let (w, h) = (480, 320);
        let tilted = lines(2.0, w, h);
        match tilt(&pixels(&tilted, w as u32, h as u32), None) {
            Tilt::Tilted { degrees, lines, .. } => {
                assert!((degrees - 2.0).abs() < 0.4, "{degrees}");
                assert!(lines >= 3);
            }
            other => panic!("{other:?}"),
        }
        let level = lines(0.0, w, h);
        assert!(matches!(
            tilt(&pixels(&level, w as u32, h as u32), None),
            Tilt::Level { .. }
        ));
        // The same lines on a person (a striped shirt) say nothing about level.
        let person = vec![1.0_f32; w * h];
        assert!(matches!(
            tilt(&pixels(&tilted, w as u32, h as u32), Some(&person)),
            Tilt::Unsure(_)
        ));
        let flat = vec![128_u8; w * h * 3];
        assert!(matches!(
            tilt(&pixels(&flat, w as u32, h as u32), None),
            Tilt::Unsure(_)
        ));
    }

    #[test]
    fn a_steep_angle_is_treated_as_deliberate() {
        let (w, h) = (480, 320);
        let steep = lines(8.0, w, h);
        assert!(matches!(
            tilt(&pixels(&steep, w as u32, h as u32), None),
            Tilt::Unsure(_)
        ));
    }

    #[test]
    fn a_speck_on_a_plain_backdrop_gets_a_clean_donor() {
        let (w, h) = (200, 160);
        let mut rgb = vec![0.18_f32; w * h * 3];
        for y in 78..82 {
            for x in 98..102 {
                for c in 0..3 {
                    rgb[(y * w + x) * 3 + c] = 0.04;
                }
            }
        }
        let frame = Frame {
            rgb,
            width: w,
            height: h,
        };
        let region = vec![1.0; w * h];
        let search = marks(&frame, &region, None, 0.001, 8);
        assert_eq!(search.repairs.len(), 1, "{search:?}");
        let repair = &search.repairs[0];
        assert!((repair.region[0] - 0.5).abs() < 0.02 && (repair.region[1] - 0.5).abs() < 0.02);
        let (dx, dy) = (
            repair.source[0] - repair.region[0],
            repair.source[1] - repair.region[1],
        );
        assert!(
            dx.abs() > repair.region[2] || dy.abs() > repair.region[3],
            "donor overlaps"
        );
        // The same mark outside the allowed region is never looked at.
        let none = vec![0.0; w * h];
        assert!(marks(&frame, &none, None, 0.001, 8).repairs.is_empty());
        // Next to hair, the same speck is a hair end and is left.
        let hair = vec![1.0; w * h];
        let near = marks(&frame, &region, Some(&hair), 0.001, 8);
        assert!(near.repairs.is_empty() && near.excluded == 1, "{near:?}");
    }

    #[test]
    fn sparkle_is_left_and_a_single_reflection_is_tamed() {
        let (w, h) = (200, 200);
        let mut rgb = vec![90_u8; w * h * 3];
        let person = vec![1.0_f32; w * h];
        let put = |rgb: &mut Vec<u8>, x: usize, y: usize| {
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let i = ((y + dy) * w + x + dx) * 3;
                rgb[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        };
        put(&mut rgb, 100, 150);
        match reflections(&pixels(&rgb, w as u32, h as u32), &person, None, &[]) {
            Reflections::Found(spots) => assert_eq!(spots.len(), 1),
            other @ Reflections::Sparkle(_) => panic!("{other:?}"),
        }
        // The same white seen through a gap to a bright background is not a reflection.
        let background = vec![1.0_f32; w * h];
        assert!(matches!(
            reflections(&pixels(&rgb, w as u32, h as u32), &person, Some(&background), &[]),
            Reflections::Found(spots) if spots.is_empty()
        ));
        // A white stripe on a shirt carries on past any bright spot in it: not a reflection.
        let mut stripe = vec![90_u8; w * h * 3];
        for y in 116..124 {
            for x in 0..w {
                stripe[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[232, 232, 232]);
            }
        }
        put(&mut stripe, 60, 119);
        assert!(matches!(
            reflections(&pixels(&stripe, w as u32, h as u32), &person, None, &[]),
            Reflections::Found(spots) if spots.is_empty()
        ));
        // The same point of light on the grey is one.
        let mut point = vec![90_u8; w * h * 3];
        put(&mut point, 60, 119);
        assert!(matches!(
            reflections(&pixels(&point, w as u32, h as u32), &person, None, &[]),
            Reflections::Found(spots) if spots.len() == 1
        ));
        for k in 0..60 {
            put(&mut rgb, 10 + (k % 12) * 15, 10 + (k / 12) * 15);
        }
        assert!(matches!(
            reflections(&pixels(&rgb, w as u32, h as u32), &person, None, &[]),
            Reflections::Sparkle(_)
        ));
    }

    #[test]
    fn a_background_brighter_than_the_face_is_lowered_and_a_dark_one_is_not() {
        let (w, h) = (100, 100);
        let mut rgb = vec![200_u8; w * h * 3];
        let mut face = vec![0.0_f32; w * h];
        let mut background = vec![1.0_f32; w * h];
        for y in 30..70 {
            for x in 30..70 {
                let i = y * w + x;
                rgb[i * 3..i * 3 + 3].copy_from_slice(&[150, 120, 100]);
                face[i] = 1.0;
                background[i] = 0.0;
            }
        }
        let tone = background_tone(&pixels(&rgb, w as u32, h as u32), &background, &face).unwrap();
        assert!(tone.reduction > 0.0 && tone.reduction <= 0.12);
        assert!(burn_amount(tone.reduction) <= 0.4);
        for p in rgb.chunks_exact_mut(3) {
            if p[0] == 200 {
                p.copy_from_slice(&[40, 40, 40]);
            }
        }
        let tone = background_tone(&pixels(&rgb, w as u32, h as u32), &background, &face).unwrap();
        assert!(tone.reduction.abs() < f32::EPSILON);
        // A white seamless is kept white.
        for p in rgb.chunks_exact_mut(3) {
            if p[0] == 40 {
                p.copy_from_slice(&[245, 245, 245]);
            }
        }
        let tone = background_tone(&pixels(&rgb, w as u32, h as u32), &background, &face).unwrap();
        assert!(tone.reduction.abs() < f32::EPSILON);
    }

    #[test]
    fn quality_measurements_see_lost_texture_and_moved_colour() {
        let (w, h) = (64, 64);
        let textured: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let v = if (i % w + i / w) % 2 == 0 { 150 } else { 170 };
                [v, v - 30, v - 50]
            })
            .collect();
        let flat: Vec<u8> = (0..w * h).flat_map(|_| [160_u8, 130, 110]).collect();
        let skin = vec![1.0; w * h];
        assert!(texture_retention(&textured, &textured, w, h, &skin).unwrap() > 0.99);
        assert!(texture_retention(&textured, &flat, w, h, &skin).unwrap() < 0.05);
        let (shift, _) = skin_shift(&textured, &textured, w, h, &skin).unwrap();
        assert!(shift < 1e-6);
        let blue: Vec<u8> = textured
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2].saturating_add(40)])
            .collect();
        assert!(skin_shift(&textured, &blue, w, h, &skin).unwrap().0 > 0.02);
        assert!(clipped(&[255, 0, 0, 10, 10, 10]) > 0.49);
    }

    /// A flat backdrop at one sRGB grey with a code value of noise, and a background plane.
    fn backdrop(grey: [u8; 3], noise: u8) -> (Vec<u8>, u32, u32, Vec<f32>) {
        let (w, h) = (64_usize, 48_usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for i in 0..w * h {
            let n = if noise == 0 {
                0
            } else {
                (i * 7 % (2 * usize::from(noise) + 1)) as u8
            };
            for c in grey {
                rgb.push(c.saturating_add(n).saturating_sub(noise));
            }
        }
        (rgb, w as u32, h as u32, vec![1.0; w * h])
    }

    #[test]
    fn evotos_grey_backdrop_is_lifted_to_where_evoto_lands_it() {
        // Evoto's homepage "Beauty & Fashion" example: backdrop sRGB 206 -> about 234.
        let (rgb, w, h, plane) = backdrop([206, 206, 206], 1);
        let px = pixels(&rgb, w, h);
        assert_eq!(px.width * px.height, plane.len());
        let lift = backdrop_lift(&px, &plane, 1.0, 1.0).unwrap();
        assert!(lift.gain > 1.2, "{lift:?}");
        let after = srgb_decode(lift.background) * lift.gain;
        let encoded = if after <= 0.003_130_8 {
            after * 12.92
        } else {
            1.055 * after.powf(1.0 / 2.4) - 0.055
        };
        assert!(
            (encoded * 255.0 - 233.3).abs() < 2.5,
            "lands at {}",
            encoded * 255.0
        );
        // The same multiplier on every channel, so it cannot tint.
        assert!(dodge_amount(lift.gain) > 0.0 && dodge_amount(lift.gain) < 0.95);
        // Half strength is half the stops.
        let half = backdrop_lift(&px, &plane, 1.0, 0.5).unwrap();
        assert!((half.gain.log2() * 2.0 - lift.gain.log2()).abs() < 1e-3);
        // An exposure the foundation already adds is not added twice.
        let brighter = backdrop_lift(&px, &plane, 1.2, 1.0).unwrap();
        assert!((brighter.gain * 1.2 - lift.gain).abs() < 1e-3);
    }

    #[test]
    fn scenes_colour_dark_and_bright_backdrops_are_not_lifted() {
        let textured = backdrop([150, 150, 150], 20);
        let px = pixels(&textured.0, textured.1, textured.2);
        assert!((backdrop_lift(&px, &textured.3, 1.0, 1.0).unwrap().gain - 1.0).abs() < 1e-6);
        let coloured = backdrop([200, 150, 120], 0);
        let px = pixels(&coloured.0, coloured.1, coloured.2);
        assert!((backdrop_lift(&px, &coloured.3, 1.0, 1.0).unwrap().gain - 1.0).abs() < 1e-6);
        let dark = backdrop([60, 60, 60], 1);
        let px = pixels(&dark.0, dark.1, dark.2);
        assert!((backdrop_lift(&px, &dark.3, 1.0, 1.0).unwrap().gain - 1.0).abs() < 1e-6);
        let white = backdrop([240, 240, 240], 1);
        let px = pixels(&white.0, white.1, white.2);
        assert!((backdrop_lift(&px, &white.3, 1.0, 1.0).unwrap().gain - 1.0).abs() < 1e-6);
        // Switched off is off.
        let grey = backdrop([206, 206, 206], 1);
        let px = pixels(&grey.0, grey.1, grey.2);
        assert!((backdrop_lift(&px, &grey.3, 1.0, 0.0).unwrap().gain - 1.0).abs() < 1e-6);
    }
}
