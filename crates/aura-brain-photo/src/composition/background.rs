//! What is behind the subject, and how much of it is fighting them.
//!
//! PHASE-11 section 6.2. Four measurements over the part of the frame the subject is not
//! in: how busy it is, whether anything in it is brighter than the subject, whether
//! anything vertical grows out of a head, and whether a colour back there is competing.
//!
//! ## The saliency model that is not here
//!
//! Section 6.2 says "Phase 18's masks are not available yet, so use a light saliency model
//! here". This module uses a **measured** saliency instead of a learned one: the
//! background is everything outside the subject boxes phase 06 and the keypoint pass
//! already produced, and the busyness of it is contrast energy.
//!
//! That is a smaller claim than a saliency model makes, and it is deliberate. A learned
//! saliency head would be a third placeholder in a phase that already ships two, its
//! output would be untrainable here for the same reason everything else is, and section
//! 12's third failure mode already names the plan: "re-validate after Phase 18 ships real
//! masks and upgrade the background analyser then". The interface this module presents -
//! [`Background`] - does not change when that happens, and the phase 11 exit report tracks
//! it as a follow-up.
//!
//! ## Head merge is the one measurement here that is a *shape* question
//!
//! Section 6.2: "check for vertical structures within a small radius of head centroids -
//! the classic 'pole growing out of the head' error." Everything else in this module is a
//! statistic over a region; this is a search for one specific geometry, and it is the
//! finding a photographer is most impressed to be shown - because it is the mistake that
//! is invisible in the viewfinder and obvious in the album.
//!
//! ## Clutter is relative, exactly as phase 09's noise figure is
//!
//! [`Background::clutter`] is divided by [`SceneRule::clutter_tolerance`] before it
//! leaves this module, so `1.0` means "exactly as busy as this scene tolerates". The same
//! crowded room behind a `couple_portrait` and inside a `reception_entrance` frame
//! produces 1.5 and 0.55. Invariant 7 applied to a measurement rather than to a threshold.

use aura_core::contract::composition::Box2;
use aura_vision::embed::descriptors::LumaPlane;

use crate::composition::rules::SceneRule;

/// Cells across the frame for the coarse analysis grid.
///
/// Forty-eight by thirty-two, which is about forty pixels a cell on a 2048 px proxy. Fine
/// enough to separate a bright window from the wall around it and coarse enough that the
/// whole module is a few hundred microseconds against section 11's 30 ms budget.
pub const GRID_W: usize = 48;
/// Cells down the frame.
pub const GRID_H: usize = 32;

/// How much brighter than the subject a region has to be to pull the eye.
///
/// Eighteen per cent of full scale. Below that it is a bright background and above it is
/// something a viewer looks at instead of the bride.
pub const BLOB_MARGIN: f32 = 0.18;

/// The least a blob can be and still be reported, as a fraction of the frame.
///
/// Half a per cent. Smaller than that is a specular glint - a candle, a sequin, a ring -
/// and phase 09 already has a whole specular-exclusion path for exactly those, because
/// they are what a wedding is lit by.
pub const BLOB_MIN_AREA: f32 = 0.005;

/// Absolute luminance below which nothing is a bright blob however bright the subject is
/// not.
///
/// A dark frame has no bright blobs, only slightly less dark regions. Without this floor
/// every night-time dance frame would report six of them.
pub const BLOB_FLOOR: f32 = 0.62;

/// The most blobs reported. The panel shows evidence boxes and six is already more than a
/// reader looks at.
pub const MAX_BLOBS: usize = 4;

/// How far above and beside a head the merge search looks, as a multiple of face height.
pub const MERGE_RADIUS: f32 = 0.9;

/// How much of the searched strip a vertical structure must occupy to count as a merge.
pub const MERGE_CONTINUITY: f32 = 0.55;

/// Luminance step that counts as a vertical edge at one pixel.
pub const MERGE_EDGE_STEP: f32 = 0.10;

/// Hue separation, in degrees, past which a background colour is competing rather than
/// harmonising.
pub const COMPETING_HUE_DEG: f32 = 30.0;

/// Saturation below which a background colour cannot compete however much of it there is.
pub const COMPETING_SATURATION: f32 = 0.45;

/// What is going on behind the people.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Background {
    /// Edge density behind the subject, divided by this scene's tolerance.
    pub clutter: f32,
    /// The raw density, before the scene divided it. Carried for the panel, which says
    /// "busier than a portrait usually is" and needs both halves to say it.
    pub raw_density: f32,
    /// Regions brighter than the subject, largest first.
    pub bright_blobs: Vec<Box2>,
    /// True when something vertical grows out of a head.
    pub head_merge: bool,
    /// Where, when it does.
    pub merge_at: Option<Box2>,
    /// How much a background colour competes with the subject, `0..1`.
    pub colour_competition: f32,
    /// Where the competing colour is, when there is one.
    pub competing_at: Option<Box2>,
    /// True when there was a subject to measure a background against.
    ///
    /// False makes every number above a statement about the whole frame, which is a
    /// different claim - and the reason `CompositionCode::NoGeometry` exists.
    pub subject_aware: bool,
}

/// Measure the background.
///
/// `subject` is the union box from [`super::placement::subject_box`]; `heads` are the face
/// boxes, used only by the merge search.
#[must_use]
pub fn analyse(
    plane: &LumaPlane,
    rgb: &[u8],
    width: usize,
    height: usize,
    subject: Option<Box2>,
    heads: &[Box2],
    rule: &SceneRule,
) -> Background {
    let luma = cell_means(plane);
    let subject_luma = subject.map_or(0.5, |area| region_mean(&luma, area));
    let density = background_density(plane, subject);
    let clutter = density / rule.clutter_tolerance.max(f32::EPSILON);

    let bright_blobs = blobs(&luma, subject, subject_luma);
    let (head_merge, merge_at) = merge(plane, heads);
    // Sample colour from the largest detected head when there is one. A derived full-body
    // rectangle is intentionally generous and can contain the very exit sign we are
    // trying to detect; letting that sign define the subject hue makes it disappear by
    // comparing red with itself. The whole subject box still masks background cells.
    let colour_sample = heads
        .iter()
        .copied()
        .max_by(|left, right| (left.w * left.h).total_cmp(&(right.w * right.h)))
        .or(subject);
    let (colour_competition, competing_at) =
        colour_competition(rgb, width, height, subject, colour_sample);

    Background {
        clutter,
        raw_density: density,
        bright_blobs,
        head_merge,
        merge_at,
        colour_competition,
        competing_at,
        subject_aware: subject.is_some(),
    }
}

/// Mean absolute gradient over the frame outside the subject box.
///
/// The whole frame when there is no subject, which is honest rather than convenient: a
/// photograph of a room has a busyness, and reporting it as a background busyness is only
/// wrong if somebody reads it as a statement about a person who is not there.
/// [`Background::subject_aware`] is how they find out.
#[must_use]
pub fn background_density(plane: &LumaPlane, subject: Option<Box2>) -> f32 {
    if plane.width < 3 || plane.height < 3 {
        return 0.0;
    }
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };
    let mut total = 0.0f64;
    let mut count = 0u32;
    for y in 0..plane.height - 1 {
        let ny = y as f32 / plane.height as f32;
        for x in 0..plane.width - 1 {
            let nx = x as f32 / plane.width as f32;
            if let Some(area) = subject {
                if nx >= area.x && nx <= area.x + area.w && ny >= area.y && ny <= area.y + area.h {
                    continue;
                }
            }
            let here = at(x, y);
            total += f64::from((at(x + 1, y) - here).abs() + (at(x, y + 1) - here).abs());
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (total / f64::from(count)) as f32
}

/// Mean luminance of each cell of the analysis grid.
#[must_use]
pub fn cell_means(plane: &LumaPlane) -> Vec<f32> {
    let mut sums = vec![0.0f32; GRID_W * GRID_H];
    let mut counts = vec![0u32; GRID_W * GRID_H];
    if plane.width == 0 || plane.height == 0 {
        return sums;
    }
    for y in 0..plane.height {
        let row = (y * GRID_H) / plane.height;
        for x in 0..plane.width {
            let column = (x * GRID_W) / plane.width;
            let index = row * GRID_W + column;
            let value = plane
                .values
                .get(y * plane.width + x)
                .copied()
                .unwrap_or(0.0);
            if let (Some(sum), Some(count)) = (sums.get_mut(index), counts.get_mut(index)) {
                *sum += value;
                *count += 1;
            }
        }
    }
    sums.iter()
        .zip(&counts)
        .map(|(sum, count)| {
            if *count == 0 {
                0.0
            } else {
                sum / *count as f32
            }
        })
        .collect()
}

/// Mean of the grid cells inside one normalised rectangle.
#[must_use]
pub fn region_mean(cells: &[f32], area: Box2) -> f32 {
    let area = area.clamped();
    let x0 = (area.x * GRID_W as f32).floor() as usize;
    let x1 = (((area.x + area.w) * GRID_W as f32).ceil() as usize).min(GRID_W);
    let y0 = (area.y * GRID_H as f32).floor() as usize;
    let y1 = (((area.y + area.h) * GRID_H as f32).ceil() as usize).min(GRID_H);
    let mut total = 0.0f32;
    let mut count = 0u32;
    for row in y0..y1 {
        for column in x0..x1 {
            if let Some(cell) = cells.get(row * GRID_W + column) {
                total += cell;
                count += 1;
            }
        }
    }
    if count == 0 {
        0.5
    } else {
        total / count as f32
    }
}

/// Connected regions brighter than the subject, largest first.
///
/// A flood fill over the analysis grid rather than over pixels: a bright blob is a thing a
/// viewer sees from across a room, and a pixel-accurate boundary for it is precision
/// nobody uses. Four-connected, because eight-connectivity joins two windows across a
/// diagonal wall corner and calls them one distraction.
#[must_use]
pub fn blobs(cells: &[f32], subject: Option<Box2>, subject_luma: f32) -> Vec<Box2> {
    let threshold = (subject_luma + BLOB_MARGIN).max(BLOB_FLOOR);
    let mut hot = vec![false; GRID_W * GRID_H];
    for (index, cell) in cells.iter().enumerate() {
        if *cell < threshold {
            continue;
        }
        let column = index % GRID_W;
        let row = index / GRID_W;
        let nx = (column as f32 + 0.5) / GRID_W as f32;
        let ny = (row as f32 + 0.5) / GRID_H as f32;
        if let Some(area) = subject {
            if nx >= area.x && nx <= area.x + area.w && ny >= area.y && ny <= area.y + area.h {
                continue;
            }
        }
        if let Some(slot) = hot.get_mut(index) {
            *slot = true;
        }
    }

    let mut seen = vec![false; GRID_W * GRID_H];
    let mut out: Vec<(Box2, usize)> = Vec::new();
    for start in 0..GRID_W * GRID_H {
        if !hot.get(start).copied().unwrap_or(false) || seen.get(start).copied().unwrap_or(true) {
            continue;
        }
        let mut stack = vec![start];
        let (mut x0, mut x1) = (GRID_W, 0usize);
        let (mut y0, mut y1) = (GRID_H, 0usize);
        let mut size = 0usize;
        while let Some(index) = stack.pop() {
            if seen.get(index).copied().unwrap_or(true) || !hot.get(index).copied().unwrap_or(false)
            {
                continue;
            }
            if let Some(slot) = seen.get_mut(index) {
                *slot = true;
            }
            size += 1;
            let column = index % GRID_W;
            let row = index / GRID_W;
            x0 = x0.min(column);
            x1 = x1.max(column);
            y0 = y0.min(row);
            y1 = y1.max(row);
            if column > 0 {
                stack.push(index - 1);
            }
            if column + 1 < GRID_W {
                stack.push(index + 1);
            }
            if row > 0 {
                stack.push(index - GRID_W);
            }
            if row + 1 < GRID_H {
                stack.push(index + GRID_W);
            }
        }
        let area = size as f32 / (GRID_W * GRID_H) as f32;
        if area < BLOB_MIN_AREA {
            continue;
        }
        out.push((
            Box2 {
                x: x0 as f32 / GRID_W as f32,
                y: y0 as f32 / GRID_H as f32,
                w: (x1 + 1 - x0) as f32 / GRID_W as f32,
                h: (y1 + 1 - y0) as f32 / GRID_H as f32,
            }
            .clamped(),
            size,
        ));
    }
    out.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    out.into_iter()
        .take(MAX_BLOBS)
        .map(|(area, _)| area)
        .collect()
}

/// The pole growing out of somebody's head.
///
/// For each head, a strip above and slightly wider than the face box is searched column by
/// column for a run of vertical edge. A column whose edge run covers more than
/// [`MERGE_CONTINUITY`] of the strip is a structure rather than texture, and a structure
/// directly above a head is the finding.
///
/// Searching **above** the head rather than around it is the whole discrimination. A
/// doorframe beside somebody is a doorframe; the same doorframe behind their crown is the
/// photograph everybody regrets.
#[must_use]
pub fn merge(plane: &LumaPlane, heads: &[Box2]) -> (bool, Option<Box2>) {
    if plane.width < 8 || plane.height < 8 {
        return (false, None);
    }
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };
    for head in heads {
        let strip_h = head.h * MERGE_RADIUS;
        let y1 = (head.y * plane.height as f32).round().max(1.0) as usize;
        let y0 = (((head.y - strip_h) * plane.height as f32).round().max(1.0)) as usize;
        if y1.saturating_sub(y0) < 4 {
            continue;
        }
        let x0 = (((head.x - head.w * 0.15) * plane.width as f32)
            .round()
            .max(1.0)) as usize;
        let x1 = ((((head.x + head.w * 1.15) * plane.width as f32).round()) as usize)
            .min(plane.width.saturating_sub(2));
        if x1 <= x0 {
            continue;
        }
        let rows = y1 - y0;
        for x in x0..x1 {
            let mut run = 0u32;
            for y in y0..y1 {
                if (at(x + 1, y) - at(x - 1, y)).abs() > MERGE_EDGE_STEP {
                    run += 1;
                }
            }
            if f64::from(run) as f32 / rows as f32 >= MERGE_CONTINUITY {
                let evidence = Box2 {
                    x: (x as f32 / plane.width as f32) - head.w * 0.25,
                    y: head.y - strip_h,
                    w: head.w * 0.5,
                    h: strip_h + head.h * 0.3,
                }
                .clamped();
                return (true, Some(evidence));
            }
        }
    }
    (false, None)
}

/// How much a background colour competes with the subject, `0..1`.
///
/// Section 6.2: "compare background chroma clusters against subject skin/clothing chroma;
/// a saturated red exit sign behind a white dress is flagged." The measure is the largest
/// product of a background cell's saturation, its brightness and its area share, among
/// cells whose hue is more than [`COMPETING_HUE_DEG`] from the subject's dominant hue.
///
/// Hue *distance* rather than a named colour list, because "red exit sign behind a white
/// dress" and "green fire door behind a red lehenga" are the same photograph and only one
/// of them is on anybody's list.
#[must_use]
pub fn colour_competition(
    rgb: &[u8],
    width: usize,
    height: usize,
    subject: Option<Box2>,
    colour_sample: Option<Box2>,
) -> (f32, Option<Box2>) {
    if width == 0 || height == 0 {
        return (0.0, None);
    }
    let Some(area) = subject else {
        return (0.0, None);
    };
    let sample = colour_sample.unwrap_or(area);

    let mut subject_hue_x = 0.0f32;
    let mut subject_hue_y = 0.0f32;
    let mut subject_weight = 0.0f32;
    let mut cells: Vec<(f32, f32, f32, usize)> = vec![(0.0, 0.0, 0.0, 0); GRID_W * GRID_H];

    for y in 0..height {
        let ny = y as f32 / height as f32;
        let row = (y * GRID_H) / height;
        for x in 0..width {
            let nx = x as f32 / width as f32;
            let base = (y * width + x) * 3;
            let r = rgb.get(base).copied().unwrap_or(0);
            let g = rgb.get(base + 1).copied().unwrap_or(0);
            let b = rgb.get(base + 2).copied().unwrap_or(0);
            let (hue, saturation, value) = hsv(r, g, b);
            let inside_sample = nx >= sample.x
                && nx <= sample.x + sample.w
                && ny >= sample.y
                && ny <= sample.y + sample.h;
            if inside_sample {
                let weight = saturation * value;
                let radians = hue.to_radians();
                subject_hue_x += weight * radians.cos();
                subject_hue_y += weight * radians.sin();
                subject_weight += weight;
            }
            let inside_subject =
                nx >= area.x && nx <= area.x + area.w && ny >= area.y && ny <= area.y + area.h;
            if inside_subject {
                continue;
            }
            let column = (x * GRID_W) / width;
            if let Some(cell) = cells.get_mut(row * GRID_W + column) {
                cell.0 += hue;
                cell.1 += saturation;
                cell.2 += value;
                cell.3 += 1;
            }
        }
    }

    // A white dress or grey suit has no meaningful hue. That is not an absence of a
    // subject colour reading: it is the case in section 6.2's worked example, and any
    // sufficiently saturated background colour competes with a neutral subject. Only use
    // hue separation when the subject has enough chroma to define a hue.
    let subject_hue = (subject_weight > f32::EPSILON).then(|| {
        subject_hue_y
            .atan2(subject_hue_x)
            .to_degrees()
            .rem_euclid(360.0)
    });

    let mut best = 0.0f32;
    let mut best_at = None;
    for (index, (hue_sum, sat_sum, value_sum, count)) in cells.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let hue = hue_sum / *count as f32;
        let saturation = sat_sum / *count as f32;
        let value = value_sum / *count as f32;
        if saturation < COMPETING_SATURATION {
            continue;
        }
        let separation = subject_hue.map_or(180.0, |subject| hue_distance(hue, subject));
        if subject_hue.is_some() && separation < COMPETING_HUE_DEG {
            continue;
        }
        let score = (saturation * value * (separation / 180.0)).clamp(0.0, 1.0);
        if score > best {
            best = score;
            let column = index % GRID_W;
            let row = index / GRID_W;
            best_at = Some(
                Box2 {
                    x: column as f32 / GRID_W as f32,
                    y: row as f32 / GRID_H as f32,
                    w: 1.0 / GRID_W as f32,
                    h: 1.0 / GRID_H as f32,
                }
                .expanded(2.0),
            );
        }
    }
    (best, best_at)
}

/// Hue in degrees, saturation and value, all from one 8-bit pixel.
#[must_use]
pub fn hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max <= r + f32::EPSILON && max >= r - f32::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max <= g + f32::EPSILON && max >= g - f32::EPSILON {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    (hue.rem_euclid(360.0), saturation, max)
}

/// The shorter way round the colour wheel, in degrees.
#[must_use]
pub fn hue_distance(a: f32, b: f32) -> f32 {
    let raw = (a - b).abs() % 360.0;
    if raw > 180.0 {
        360.0 - raw
    } else {
        raw
    }
}
