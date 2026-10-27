//! Finding reflections on spectacle lenses, so the Glare tool can place its own selections.
//!
//! For every face the portrait parse found, a lens-sized window around each eye is searched for
//! a reflection. A pixel is a candidate when its darkest channel stands well above the bright
//! end of that person's own skin (so the threshold follows the person and the light, never a
//! constant) and it is not strongly coloured. Candidates are grouped into blobs, and a blob is a
//! lens reflection only when three things hold together:
//!
//! - it reaches **beyond the eye opening**: a reflection lies on the glass, over the skin around
//!   the eye as well as the eye. A catchlight sits in the iris and the white of the eye sits
//!   between the lids, and both are refused;
//! - it is **big enough** for the face to be a reflection rather than a glint;
//! - its edge is **crisp**: a reflection of a window or a lamp ends abruptly, while a sheen on
//!   skin fades out over a broad gradient.
//!
//! Brows and hair are never candidates. The result is a proposal: every finding becomes a glare
//! selection a person previews before applying, and none is applied without them.

#![allow(clippy::indexing_slicing)] // Frame-sized planes, indexed within their bounds.

use aura_portrait::{PortraitMap, Region};

/// A reflection the detector found, shaped as a glare selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Reflection {
    /// The ellipse `[cx, cy, rx, ry]` in frame fractions: a glare edit's `region`.
    pub region: [f32; 4],
    /// `0..=1`: how sure the detector is that this is a lens reflection.
    pub confidence: f32,
    /// Which face (largest first) it belongs to.
    pub face: usize,
    /// Reflection pixels found, at the frame's resolution.
    pub pixels: usize,
}

/// Per-pixel readings the search works on, for the whole frame.
pub(crate) struct Planes {
    pub w: usize,
    pub h: usize,
    /// Darkest channel: the most a veil could have added.
    pub darkest: Vec<f32>,
    /// `(max - min) / max`: how coloured the pixel is.
    pub chroma: Vec<f32>,
    /// Inside the eye opening (sclera, iris, lashes between the lids).
    pub in_eye: Vec<bool>,
    /// Never a candidate: brows, hair and the nose.
    pub excluded: Vec<bool>,
    /// The central part of a spectacle lens around each eye, where a reflection must mostly lie.
    pub in_lens: Vec<bool>,
}

/// Blobs larger than this share of the lens window are a bright background seen through it.
const MAX_WINDOW_SHARE: f32 = 0.6;
/// A reflection is not strongly coloured; skin, lips and jewellery are.
const MAX_CHROMA: f32 = 0.4;
/// Below this score a blob is not reported.
const MIN_CONFIDENCE: f32 = 0.35;
/// Below this frame evidence an eye is not treated as behind a spectacle lens.
const MIN_FRAME: f32 = 0.7;

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One blob inside one lens window, with everything the decision about it read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blob {
    /// Pixel bounds `[x0, y0, x1, y1]` in the frame.
    pub bounds: [usize; 4],
    /// Pixels in the blob.
    pub pixels: usize,
    /// Share of them outside the eye opening.
    pub outside: f32,
    /// Mean colourfulness, against the limit this person's skin set.
    pub chroma: f32,
    /// How abruptly it ends (about one and above: crisp).
    pub crisp: f32,
    /// Log brightness over the face's skin and over the skin right around it, the smaller.
    pub strength: f32,
    /// Texture inside, as a share of the contrast (a smooth sheet is low).
    pub roughness: f32,
    /// `0..=1`, zero when refused.
    pub confidence: f32,
    /// Why it was kept or refused.
    pub verdict: &'static str,
}

impl Blob {
    /// True when the blob is reported as a lens reflection.
    #[must_use]
    pub fn is_reflection(&self) -> bool {
        self.verdict == REFLECTION
    }
}

const REFLECTION: &str = "reflection";

/// The person a lens window belongs to: how far apart their eyes are, and their skin's median
/// darkest channel and colourfulness.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Person {
    pub eye_px: f32,
    pub skin: f32,
    pub skin_chroma: f32,
}

/// Search one lens window `[x0, y0, x1, y1]` for reflections brighter than `threshold`.
#[allow(clippy::too_many_lines)] // One search, read top to bottom.
pub(crate) fn lens_blobs(
    planes: &Planes,
    window: [usize; 4],
    threshold: f32,
    person: Person,
) -> Vec<Blob> {
    let Person {
        eye_px,
        skin,
        skin_chroma,
    } = person;
    let [x0, y0, x1, y1] = window;
    let w = planes.w;
    if x1 <= x0 + 2 || y1 <= y0 + 2 || x1 > w || y1 > planes.h {
        return Vec::new();
    }
    let (ww, wh) = (x1 - x0, y1 - y0);
    // Seeds are clearly less coloured than this person's skin (a sunlit patch of skin is
    // skin-coloured) and well above its bright end. A blob then grows from them into the
    // weaker part of the same reflection, so one sheet is measured as one blob rather than as
    // fragments whose surroundings are the rest of the reflection.
    let chroma_limit = (0.65 * skin_chroma).clamp(0.12, MAX_CHROMA);
    let loose_chroma = (0.85 * skin_chroma).clamp(0.15, 0.5);
    let loose = (0.8 * threshold).max(1.25 * skin);
    let seed = |x: usize, y: usize| {
        let i = y * w + x;
        planes.darkest[i] > threshold && planes.chroma[i] < chroma_limit && !planes.excluded[i]
    };
    let grows = |x: usize, y: usize| {
        let i = y * w + x;
        planes.darkest[i] > loose && planes.chroma[i] < loose_chroma && !planes.excluded[i]
    };
    let mut label = vec![0_u32; ww * wh];
    let mut blobs = Vec::new();
    let mut next = 0_u32;
    let min_area = (0.05 * eye_px).powi(2).max(6.0);
    let min_outside = (0.035 * eye_px).powi(2).max(4.0);
    for sy in y0..y1 {
        for sx in x0..x1 {
            if label[(sy - y0) * ww + sx - x0] != 0 || !seed(sx, sy) {
                continue;
            }
            next += 1;
            let mut stack = vec![(sx, sy)];
            label[(sy - y0) * ww + sx - x0] = next;
            let mut members = Vec::new();
            while let Some((x, y)) = stack.pop() {
                members.push((x, y));
                for (dx, dy) in [
                    (-1_i64, -1_i64),
                    (0, -1),
                    (1, -1),
                    (-1, 0),
                    (1, 0),
                    (-1, 1),
                    (0, 1),
                    (1, 1),
                ] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < x0 as i64 || ny < y0 as i64 || nx >= x1 as i64 || ny >= y1 as i64 {
                        continue;
                    }
                    let (nx, ny) = (nx as usize, ny as usize);
                    let k = (ny - y0) * ww + nx - x0;
                    if label[k] == 0 && grows(nx, ny) {
                        label[k] = next;
                        stack.push((nx, ny));
                    }
                }
            }
            let area = members.len();
            if area < 4 {
                continue;
            }
            let outside = members
                .iter()
                .filter(|(x, y)| !planes.in_eye[y * w + x])
                .count();
            // Contrast over the skin, and how abruptly the blob ends: the steepest step out of
            // each boundary pixel, as a share of the contrast, per tenth of the eye spacing.
            let mean = members
                .iter()
                .map(|(x, y)| planes.darkest[y * w + x])
                .sum::<f32>()
                / area as f32;
            let contrast = (mean - skin).max(1e-4);
            let (mut steps, mut edge) = (0.0_f32, 0usize);
            for &(x, y) in &members {
                let here = planes.darkest[y * w + x];
                let mut steepest = 0.0_f32;
                for (dx, dy) in [(-1_i64, 0_i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= planes.h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    let inside = nx >= x0 as i64
                        && ny >= y0 as i64
                        && (nx as usize) < x1
                        && (ny as usize) < y1
                        && label[(ny as usize - y0) * ww + nx as usize - x0] == next;
                    if !inside {
                        steepest = steepest.max(here - planes.darkest[j]);
                    }
                }
                if steepest > 0.0 {
                    steps += steepest;
                    edge += 1;
                }
            }
            let crisp = steps / edge.max(1) as f32 / contrast * (0.1 * eye_px);
            // Texture inside: a reflection is a smooth sheet, a sheen on skin is speckled by its
            // pores. The mean absolute Laplacian over interior pixels, as a share of the contrast.
            let member_at = |x: usize, y: usize| {
                x >= x0 && y >= y0 && x < x1 && y < y1 && label[(y - y0) * ww + x - x0] == next
            };
            let (mut rough, mut interior) = (0.0_f32, 0usize);
            for &(x, y) in &members {
                if x == 0 || y == 0 || x + 1 >= w || y + 1 >= planes.h {
                    continue;
                }
                if !(member_at(x - 1, y)
                    && member_at(x + 1, y)
                    && member_at(x, y - 1)
                    && member_at(x, y + 1))
                {
                    continue;
                }
                let i = y * w + x;
                let lap = 4.0 * planes.darkest[i]
                    - planes.darkest[i - 1]
                    - planes.darkest[i + 1]
                    - planes.darkest[i - w]
                    - planes.darkest[i + w];
                rough += lap.abs();
                interior += 1;
            }
            let roughness = rough / interior.max(1) as f32 / contrast;
            // Brighter than the skin right around it, not only than the face as a whole: the lit
            // side of a face is bright everywhere, a reflection stands out from where it is.
            let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0, 0);
            for &(x, y) in &members {
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x + 1);
                by1 = by1.max(y + 1);
            }
            let pad = (0.12 * eye_px).max(2.0) as usize;
            let (rx0, ry0) = (bx0.saturating_sub(pad), by0.saturating_sub(pad));
            let (rx1, ry1) = ((bx1 + pad).min(w), (by1 + pad).min(planes.h));
            let (mut around, mut count) = (0.0_f32, 0usize);
            for y in ry0..ry1 {
                for x in rx0..rx1 {
                    let i = y * w + x;
                    let member = x >= x0
                        && y >= y0
                        && x < x1
                        && y < y1
                        && label[(y - y0) * ww + x - x0] == next;
                    if !member && !planes.in_eye[i] && !planes.excluded[i] {
                        around += planes.darkest[i];
                        count += 1;
                    }
                }
            }
            let local = if count > 0 {
                around / count as f32
            } else {
                skin
            };
            let strength = (mean / skin.max(1e-4))
                .ln()
                .min((mean / local.max(1e-4)).ln());
            let share = outside as f32 / area as f32;
            let on_lens = members
                .iter()
                .filter(|(x, y)| planes.in_lens[y * w + x])
                .count() as f32
                / area as f32;
            let chroma = members
                .iter()
                .map(|(x, y)| planes.chroma[y * w + x])
                .sum::<f32>()
                / area as f32;
            let mut confidence = smoothstep(0.8, 2.0, crisp)
                * smoothstep(0.25, 0.6, share)
                * smoothstep(0.25, 0.7, strength);
            let verdict = if (area as f32) < min_area {
                "too small for the face"
            } else if area as f32 > MAX_WINDOW_SHARE * (ww * wh) as f32 {
                "fills the lens: a bright background"
            } else if (outside as f32) < min_outside || share < 0.3 {
                "inside the eye opening"
            } else if on_lens < 0.6 {
                "away from the lens: cheek, brow, temple or nose"
            } else if smoothstep(0.8, 2.0, crisp) < MIN_CONFIDENCE {
                "soft edge: a sheen"
            } else if smoothstep(0.25, 0.7, strength) < MIN_CONFIDENCE {
                "not brighter than the skin around it"
            } else if confidence < MIN_CONFIDENCE {
                "weak"
            } else {
                REFLECTION
            };
            if verdict != REFLECTION {
                confidence = 0.0;
            }
            blobs.push(Blob {
                bounds: [bx0, by0, bx1, by1],
                pixels: area,
                outside: share,
                chroma,
                crisp,
                strength,
                roughness,
                confidence,
                verdict,
            });
        }
    }
    blobs
}

fn log_step(a: f32, b: f32) -> f32 {
    ((a.max(1e-4)).ln() - (b.max(1e-4)).ln()).abs()
}

/// How much of a spectacle rim surrounds an eye, `0..=1`: walking outward from the eye in
/// sixteen directions below and beside it (the brows above are left out), the share of
/// directions that cross a step in brightness at least six times the face's own skin texture
/// between a fifth and a half of the eye spacing - where a lens rim runs. A bare face mostly has
/// skin there; a frame, metal or plastic, stands out from it. Creases and eye make-up can score
/// too, so this is evidence that raises or lowers trust, measured on real wedding photographs
/// (2026-10-26: glasses mostly 0.7 and above, bare faces mostly below).
pub(crate) fn frame_evidence(planes: &Planes, eye: [f32; 2], eye_px: f32, floor: f32) -> f32 {
    let (w, h) = (planes.w, planes.h);
    let directions = 16;
    let mut crossed = 0;
    let mut counted = 0;
    for k in 0..directions {
        // From pointing outward-left, through straight down, to outward-right.
        let angle = std::f32::consts::PI * (k as f32 + 0.5) / directions as f32;
        let (dx, dy) = (-angle.cos(), angle.sin());
        let mut previous: Option<f32> = None;
        let mut strongest = 0.0_f32;
        let mut seen = false;
        let steps = (0.25 * eye_px).max(4.0) as usize;
        for j in 0..=steps {
            let r = eye_px * (0.22 + 0.26 * j as f32 / steps as f32);
            let (x, y) = (eye[0] + dx * r, eye[1] + dy * r);
            if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                previous = None;
                continue;
            }
            let i = y as usize * w + x as usize;
            if planes.excluded[i] || planes.in_eye[i] {
                previous = None;
                continue;
            }
            seen = true;
            let value = planes.darkest[i];
            if let Some(last) = previous {
                strongest = strongest.max(log_step(value, last));
            }
            previous = Some(value);
        }
        if seen {
            counted += 1;
            if strongest > 6.0 * floor {
                crossed += 1;
            }
        }
    }
    if counted == 0 {
        0.0
    } else {
        crossed as f32 / counted as f32
    }
}

/// The readings for a whole frame and its parse.
fn planes(rgb: &[f32], w: usize, h: usize, map: &PortraitMap) -> Planes {
    let resolve = |region| crate::portrait::resolve(map, region, w as u32, h as u32);
    let (eyes, iris, sclera) = (
        resolve(Region::Eyes),
        resolve(Region::Iris),
        resolve(Region::Sclera),
    );
    let (brows, hair, nose) = (
        resolve(Region::Eyebrows),
        resolve(Region::Hair),
        resolve(Region::Nose),
    );
    // The eye opening, from the landmarks: about half the eye spacing wide and an eighth of it
    // tall. The parse's own eye planes may extend it a little (an open eye, a tilted head),
    // never far: a strong reflection over an eye is easily labelled as the white of it, and
    // past this shape it is counted as lying on the lens. A catchlight the eye planes missed is
    // still inside the opening. The lens core is the middle of a spectacle lens around it.
    let mut openings = vec![false; w * h];
    let mut extended = vec![false; w * h];
    let mut in_lens = vec![false; w * h];
    let (sx, sy) = (w as f32 / map.width as f32, h as f32 / map.height as f32);
    for face in &map.faces {
        let (l, r) = (
            [face.left_eye[0] * sx, face.left_eye[1] * sy],
            [face.right_eye[0] * sx, face.right_eye[1] * sy],
        );
        let e = (r[0] - l[0]).hypot(r[1] - l[1]);
        let (ax, ay) = ((0.3 * e).max(1.0), (0.13 * e).max(1.0));
        let (lx, ly) = ((0.42 * e).max(1.0), (0.27 * e).max(1.0));
        for c in [l, r] {
            let (x0, x1) = ((c[0] - lx).max(0.0) as usize, ((c[0] + lx) as usize).min(w));
            let (y0, y1) = ((c[1] - ly).max(0.0) as usize, ((c[1] + ly) as usize).min(h));
            for y in y0..y1 {
                for x in x0..x1 {
                    let (dx, dy) = (x as f32 - c[0], y as f32 - c[1]);
                    let eye = (dx / ax).powi(2) + (dy / ay).powi(2);
                    let i = y * w + x;
                    if eye <= 1.0 {
                        openings[i] = true;
                    }
                    if eye <= 1.7 {
                        extended[i] = true;
                    }
                    if (dx / lx).powi(2) + (dy / ly).powi(2) <= 1.0 {
                        in_lens[i] = true;
                    }
                }
            }
        }
    }
    let mut darkest = Vec::with_capacity(w * h);
    let mut chroma = Vec::with_capacity(w * h);
    for p in rgb.chunks_exact(3) {
        let lo = p[0].min(p[1]).min(p[2]).max(0.0);
        let hi = p[0].max(p[1]).max(p[2]).max(1e-6);
        darkest.push(lo);
        chroma.push(((hi - lo) / hi).clamp(0.0, 1.0));
    }
    Planes {
        w,
        h,
        darkest,
        chroma,
        in_eye: (0..w * h)
            .map(|i| openings[i] || (eyes[i].max(iris[i]).max(sclera[i]) > 0.3 && extended[i]))
            .collect(),
        excluded: (0..w * h)
            .map(|i| brows[i] > 0.5 || hair[i] > 0.5 || nose[i] > 0.5)
            .collect(),
        in_lens,
    }
}

/// Every blob the search considered, per face and eye, for a person checking the detector.
#[derive(Debug, Clone)]
pub struct Inspection {
    /// Which face (largest first).
    pub face: usize,
    /// The lens window searched, `[x0, y0, x1, y1]` in frame pixels.
    pub window: [usize; 4],
    /// The brightness a candidate had to exceed, and this person's skin median.
    pub threshold: f32,
    /// The face's skin median darkest channel and colourfulness.
    pub skin: [f32; 2],
    /// How far apart the face's eyes are, in frame pixels.
    pub eye_px: f32,
    /// `0..=1`: share of directions below and beside this eye that cross a spectacle rim.
    pub frame: f32,
    /// What was found there.
    pub blobs: Vec<Blob>,
}

/// Find lens reflections on the faces of a parsed frame. `rgb` is the working-space frame the
/// parse was made from, `w` by `h`.
#[must_use]
pub fn find_reflections(rgb: &[f32], w: usize, h: usize, map: &PortraitMap) -> Vec<Reflection> {
    let mut found = Vec::new();
    for look in inspect(rgb, w, h, map) {
        // Without the look of a spectacle rim around the eye, a reflection-like highlight is far
        // more often the white of an eye, a catchlight or a sheen on skin than a lens.
        if look.frame < MIN_FRAME {
            continue;
        }
        let blobs: Vec<&Blob> = look.blobs.iter().filter(|b| b.is_reflection()).collect();
        if blobs.is_empty() {
            continue;
        }
        let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0, 0);
        let mut pixels = 0;
        let mut confidence = 0.0_f32;
        for blob in &blobs {
            bx0 = bx0.min(blob.bounds[0]);
            by0 = by0.min(blob.bounds[1]);
            bx1 = bx1.max(blob.bounds[2]);
            by1 = by1.max(blob.bounds[3]);
            pixels += blob.pixels;
            confidence = confidence.max(blob.confidence);
        }
        // An ellipse around the reflection with room for the lens around it, which the removal
        // reads as "the scene beneath".
        let eye_px = look.eye_px;
        let rx = ((bx1 - bx0) as f32 * 0.5 * 1.3 + 0.05 * eye_px).max(0.08 * eye_px);
        let ry = ((by1 - by0) as f32 * 0.5 * 1.3 + 0.05 * eye_px).max(0.08 * eye_px);
        let cx = (bx0 + bx1) as f32 * 0.5;
        let cy = (by0 + by1) as f32 * 0.5;
        found.push(Reflection {
            region: [cx / w as f32, cy / h as f32, rx / w as f32, ry / h as f32],
            confidence,
            face: look.face,
            pixels,
        });
    }
    merge_overlapping(found)
}

/// One selection per reflection: a face found twice, or two eyes' windows reaching the same
/// lens, must not remove the same reflection twice. Overlapping ellipses become the one ellipse
/// around both.
fn merge_overlapping(mut found: Vec<Reflection>) -> Vec<Reflection> {
    let mut merged: Vec<Reflection> = Vec::new();
    found.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    for next in found {
        let [cx, cy, rx, ry] = next.region;
        if let Some(into) = merged.iter_mut().find(|m| {
            let [mx, my, mrx, mry] = m.region;
            (cx - mx).abs() < rx + mrx && (cy - my).abs() < ry + mry
        }) {
            let [mx, my, mrx, mry] = into.region;
            let (x0, x1) = ((mx - mrx).min(cx - rx), (mx + mrx).max(cx + rx));
            let (y0, y1) = ((my - mry).min(cy - ry), (my + mry).max(cy + ry));
            into.region = [
                (x0 + x1) * 0.5,
                (y0 + y1) * 0.5,
                (x1 - x0) * 0.5,
                (y1 - y0) * 0.5,
            ];
            into.pixels += next.pixels;
        } else {
            merged.push(next);
        }
    }
    merged
}

/// How many distinct faces the search can look at: confident enough, eyes far enough apart to
/// judge, and not the same face found twice (eye centres within half an eye spacing).
#[must_use]
pub fn searched_faces(w: usize, h: usize, map: &PortraitMap) -> usize {
    if map.width == 0 || map.height == 0 {
        return 0;
    }
    let (sx, sy) = (w as f32 / map.width as f32, h as f32 / map.height as f32);
    let mut seen: Vec<([f32; 2], f32)> = Vec::new();
    for face in &map.faces {
        let l = [face.left_eye[0] * sx, face.left_eye[1] * sy];
        let r = [face.right_eye[0] * sx, face.right_eye[1] * sy];
        let e = (r[0] - l[0]).hypot(r[1] - l[1]);
        if face.confidence < 0.4 || e < 28.0 {
            continue;
        }
        let c = [(l[0] + r[0]) * 0.5, (l[1] + r[1]) * 0.5];
        if !seen
            .iter()
            .any(|(o, oe)| (o[0] - c[0]).hypot(o[1] - c[1]) < 0.5 * e.max(*oe))
        {
            seen.push((c, e));
        }
    }
    seen.len()
}

/// Search every lens window and report every blob considered, kept or refused.
#[must_use]
pub fn inspect(rgb: &[f32], w: usize, h: usize, map: &PortraitMap) -> Vec<Inspection> {
    if rgb.len() != w * h * 3 || w < 8 || h < 8 || map.width == 0 || map.height == 0 {
        return Vec::new();
    }
    if map.faces.is_empty() {
        return Vec::new();
    }
    let planes = planes(rgb, w, h, map);
    let skin_plane = crate::portrait::resolve(map, Region::Skin, w as u32, h as u32);
    let (sx, sy) = (w as f32 / map.width as f32, h as f32 / map.height as f32);
    let mut found = Vec::new();
    for (index, face) in map.faces.iter().enumerate() {
        if face.confidence < 0.4 {
            continue;
        }
        let at = |p: [f32; 2]| [p[0] * sx, p[1] * sy];
        let (left, right) = (at(face.left_eye), at(face.right_eye));
        let eye_px = (right[0] - left[0]).hypot(right[1] - left[1]);
        // Too small to tell a reflection from a highlight on the skin.
        if eye_px < 28.0 {
            continue;
        }
        // The bright end of this person's own skin, from the face box.
        let b = face.bbox();
        let (fx0, fy0) = ((b[0] * sx).max(0.0) as usize, (b[1] * sy).max(0.0) as usize);
        let fx1 = (((b[0] + b[2]) * sx) as usize).min(w);
        let fy1 = (((b[1] + b[3]) * sy) as usize).min(h);
        let samples: Vec<usize> = (fy0..fy1)
            .flat_map(|y| (fx0..fx1).map(move |x| y * w + x))
            .filter(|i| skin_plane[*i] > 0.6 && !planes.in_eye[*i])
            .collect();
        if samples.len() < 50 {
            continue;
        }
        let mut skin: Vec<f32> = samples.iter().map(|i| planes.darkest[*i]).collect();
        let mut colour: Vec<f32> = samples.iter().map(|i| planes.chroma[*i]).collect();
        skin.sort_by(f32::total_cmp);
        colour.sort_by(f32::total_cmp);
        let median = skin[skin.len() / 2];
        let high = skin[skin.len() * 9 / 10];
        // The face's own texture: the median step between neighbouring skin pixels, in log
        // brightness so a dark face and a bright one are compared alike.
        let mut texture: Vec<f32> = samples
            .iter()
            .filter(|i| *i % w + 1 < w)
            .map(|i| log_step(planes.darkest[*i], planes.darkest[*i + 1]))
            .collect();
        texture.sort_by(f32::total_cmp);
        let gradient_floor = texture[texture.len() / 2].max(0.01);
        let person = Person {
            eye_px,
            skin: median,
            skin_chroma: colour[colour.len() / 2],
        };
        let threshold = (high * 1.15).max(median * 1.6).max(0.02);
        for eye in [left, right] {
            let window = [
                (eye[0] - 0.55 * eye_px).max(0.0) as usize,
                (eye[1] - 0.4 * eye_px).max(0.0) as usize,
                ((eye[0] + 0.55 * eye_px) as usize).min(w),
                ((eye[1] + 0.45 * eye_px) as usize).min(h),
            ];
            found.push(Inspection {
                face: index,
                window,
                threshold,
                skin: [person.skin, person.skin_chroma],
                eye_px,
                frame: frame_evidence(&planes, eye, eye_px, gradient_floor),
                blobs: lens_blobs(&planes, window, threshold, person),
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 120 x 90 lens window of skin with an eye opening in the middle.
    fn lens() -> Planes {
        let (w, h) = (120, 90);
        let mut in_eye = vec![false; w * h];
        for y in 38..52 {
            for x in 35..85 {
                in_eye[y * w + x] = true;
            }
        }
        Planes {
            w,
            h,
            darkest: vec![0.2; w * h],
            chroma: vec![0.5; w * h],
            in_eye,
            excluded: vec![false; w * h],
            in_lens: (0..w * h)
                .map(|i| {
                    let (x, y) = ((i % w) as f32 - 60.0, (i / w) as f32 - 45.0);
                    (x / 45.0).powi(2) + (y / 30.0).powi(2) <= 1.0
                })
                .collect(),
        }
    }

    fn paint(p: &mut Planes, x: std::ops::Range<usize>, y: std::ops::Range<usize>, v: f32) {
        for yy in y {
            for xx in x.clone() {
                let i = yy * p.w + xx;
                p.darkest[i] = v;
                p.chroma[i] = 0.05;
            }
        }
    }

    const WINDOW: [usize; 4] = [0, 0, 120, 90];
    fn found(p: &Planes, window: [usize; 4]) -> Vec<Blob> {
        lens_blobs(p, window, 0.3, PERSON)
            .into_iter()
            .filter(Blob::is_reflection)
            .collect()
    }

    const PERSON: Person = Person {
        eye_px: 100.0,
        skin: 0.2,
        skin_chroma: 0.5,
    };

    #[test]
    fn a_crisp_reflection_across_the_eye_and_the_lens_is_found() {
        let mut p = lens();
        paint(&mut p, 50..80, 25..60, 0.7);
        let blobs = found(&p, WINDOW);
        assert_eq!(blobs.len(), 1, "{blobs:?}");
        assert_eq!(blobs[0].bounds, [50, 25, 80, 60]);
        assert!(blobs[0].confidence > 0.6, "{}", blobs[0].confidence);
    }

    #[test]
    fn the_white_of_the_eye_and_a_catchlight_are_not_reflections() {
        let mut p = lens();
        // Sclera: bright and neutral, but between the lids.
        paint(&mut p, 36..58, 39..51, 0.6);
        // A catchlight in the iris.
        paint(&mut p, 62..65, 43..46, 0.9);
        assert!(found(&p, WINDOW).is_empty());
    }

    #[test]
    fn a_soft_sheen_on_skin_is_not_a_reflection() {
        let mut p = lens();
        // A broad sheen under the eye, fading over most of its radius.
        for y in 50..90 {
            for x in 30..90 {
                let d = ((x as f32 - 60.0) / 30.0).hypot((y as f32 - 70.0) / 20.0);
                if d < 1.0 {
                    let i = y * p.w + x;
                    p.darkest[i] = 0.2 + 0.4 * (1.0 - d);
                    p.chroma[i] = 0.1;
                }
            }
        }
        let blobs = found(&p, WINDOW);
        assert!(blobs.is_empty(), "{blobs:?}");
    }

    #[test]
    fn coloured_and_excluded_pixels_are_never_candidates() {
        let mut p = lens();
        paint(&mut p, 50..80, 10..30, 0.7);
        for y in 10..30 {
            for x in 50..80 {
                p.excluded[y * p.w + x] = true;
            }
        }
        paint(&mut p, 10..30, 60..80, 0.7);
        for y in 60..80 {
            for x in 10..30 {
                p.chroma[y * p.w + x] = 0.8;
            }
        }
        assert!(found(&p, WINDOW).is_empty());
    }

    #[test]
    fn a_skin_coloured_sun_patch_and_a_lit_side_of_the_face_are_not_reflections() {
        let mut p = lens();
        // A crisp sun patch, as colourful as the skin.
        paint(&mut p, 50..80, 25..60, 0.7);
        for y in 25..60 {
            for x in 50..80 {
                p.chroma[y * p.w + x] = 0.45;
            }
        }
        assert!(found(&p, WINDOW).is_empty());
        // The whole lens window brightly lit and pale: nothing stands out from its surroundings.
        let mut lit = lens();
        for (d, c) in lit.darkest.iter_mut().zip(lit.chroma.iter_mut()) {
            *d = 0.6;
            *c = 0.1;
        }
        paint(&mut lit, 50..80, 25..60, 0.66);
        assert!(found(&lit, WINDOW).is_empty());
    }

    #[test]
    fn a_crisp_highlight_away_from_the_lens_is_not_a_reflection() {
        let mut p = lens();
        // A sharp sheen at the edge of the window, as on a cheekbone or a temple.
        paint(&mut p, 0..18, 70..90, 0.7);
        let blobs = found(&p, WINDOW);
        assert!(blobs.is_empty(), "{blobs:?}");
    }

    #[test]
    fn overlapping_proposals_become_one_selection() {
        let at = |cx: f32, confidence: f32| Reflection {
            region: [cx, 0.3, 0.02, 0.01],
            confidence,
            face: 0,
            pixels: 10,
        };
        let merged = merge_overlapping(vec![at(0.40, 0.6), at(0.43, 0.9), at(0.7, 0.5)]);
        assert_eq!(merged.len(), 2);
        assert!(
            (merged[0].region[0] - 0.415).abs() < 1e-6
                && (merged[0].region[2] - 0.035).abs() < 1e-6
        );
        assert!((merged[0].confidence - 0.9).abs() < 1e-6 && merged[0].pixels == 20);
    }

    #[test]
    fn a_tiny_glint_is_too_small_for_the_face() {
        let mut p = lens();
        paint(&mut p, 20..23, 20..23, 0.9);
        assert!(found(&p, WINDOW).is_empty());
    }
}
