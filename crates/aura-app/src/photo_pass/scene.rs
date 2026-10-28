//! What every step of the photo pass reads: the original's preview, measured once. ADR-0186.
//!
//! The planes here are shared so that twenty-five steps do not each walk the frame for the same
//! local mean or the same person zone. Nothing here decides anything.
// Planes are `w * h` long and every index is derived from a position inside them; statistics
// convert freely between pixel counts and f32.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

use crate::advanced_retouch::measure;
use crate::portrait_features::Pixels;
use aura_vision::portrait::PortraitFace;

/// What the catalogue knows about how the photograph was taken.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capture {
    pub iso: Option<u32>,
    /// Exposure time in seconds.
    pub shutter: Option<f32>,
    pub focal_mm: Option<f32>,
    /// The f-number.
    pub aperture: Option<f32>,
    /// Width of the original in pixels, for scaling measurements made on the preview.
    pub width: Option<u32>,
    /// The primary file's kind as ingest recorded it: `raw`, `jpeg`, `tiff`, `png`, `heif`.
    pub kind: Option<String>,
    /// The JPEG file's luminance quality, read from its own quantisation tables.
    pub jpeg_quality: Option<u8>,
}

impl Capture {
    /// True when the file carries only eight bits per channel, so its gradients can band.
    #[must_use]
    pub fn eight_bit(&self) -> bool {
        matches!(self.kind.as_deref(), Some("jpeg" | "png" | "heif"))
    }
}

/// One photograph, measured once for every step.
#[derive(Debug)]
pub struct Scene<'a> {
    pub px: Pixels<'a>,
    pub w: usize,
    pub h: usize,
    /// Display-encoded luminance.
    pub luma: Vec<f32>,
    /// 1 where a person may be: each face box widened, from above the head to the bottom.
    pub people: Vec<f32>,
    /// Local standard deviation of `luma` over 5 x 5 pixels: noise and fine detail.
    pub fine_std: Vec<f32>,
    /// Local mean and standard deviation of `luma` over 17 x 17 pixels: whether an area is plain.
    pub wide_mean: Vec<f32>,
    pub wide_std: Vec<f32>,
    pub faces: Vec<PortraitFace>,
    pub capture: Capture,
    /// The renderer's own working-space frame, when it could be built.
    pub frame: Option<aura_render::Frame>,
    /// The renderer's own parse of the faces and their regions, on its analysis grid.
    pub map: Option<std::sync::Arc<aura_portrait::PortraitMap>>,
}

impl<'a> Scene<'a> {
    #[must_use]
    pub fn new(
        px: Pixels<'a>,
        faces: Vec<PortraitFace>,
        capture: Capture,
        frame: Option<aura_render::Frame>,
    ) -> Self {
        let (w, h) = (px.width, px.height);
        let luma = measure::encoded_luma(&px);
        let squares: Vec<f32> = luma.iter().map(|v| v * v).collect();
        let deviation = |r: usize| {
            let mean = measure::box_mean(&luma, w, h, r);
            let mean2 = measure::box_mean(&squares, w, h, r);
            let std = mean
                .iter()
                .zip(&mean2)
                .map(|(m, m2)| (m2 - m * m).max(0.0).sqrt())
                .collect::<Vec<_>>();
            (mean, std)
        };
        let (_, fine_std) = deviation(2);
        let (wide_mean, wide_std) = deviation(8);
        let people = person_zone(&faces, w, h);
        Self {
            px,
            w,
            h,
            luma,
            people,
            fine_std,
            wide_mean,
            wide_std,
            faces,
            capture,
            frame,
            map: None,
        }
    }

    /// The same scene with the renderer's parse of its faces.
    #[must_use]
    pub fn with_map(mut self, map: Option<std::sync::Arc<aura_portrait::PortraitMap>>) -> Self {
        self.map = map;
        self
    }

    /// The faces the renderer's parse found, largest first.
    #[must_use]
    pub fn map_faces(&self) -> Vec<aura_portrait::FaceGeometry> {
        let mut faces = self.map.as_ref().map_or_else(Vec::new, |m| m.faces.clone());
        faces.sort_by(|a, b| b.size.total_cmp(&a.size));
        faces
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.w * self.h
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True when nobody was found in the frame.
    #[must_use]
    pub fn no_people(&self) -> bool {
        self.faces.is_empty()
    }

    /// Share of the frame inside the person zone.
    #[must_use]
    pub fn people_share(&self) -> f32 {
        self.people.iter().sum::<f32>() / self.len().max(1) as f32
    }

    /// Display-encoded RGB at a plane index.
    #[must_use]
    pub fn rgb(&self, i: usize) -> [f32; 3] {
        self.px.encoded(i % self.w, i / self.w)
    }

    /// The linear frame the retouch detectors measure.
    #[must_use]
    pub fn linear(&self) -> aura_retouch::texture_guard::Frame {
        measure::linear_frame(&self.px)
    }

    /// A coarse selection of everything outside the person zone, for operations that must not
    /// reach anybody; `None` when nobody is in the frame.
    #[must_use]
    pub fn outside_people(&self) -> Option<aura_recipe::retouch_tools::Matte> {
        if self.faces.is_empty() {
            return None;
        }
        let gw = 128_usize.min(self.w);
        let gh = ((self.h * gw) / self.w.max(1)).max(1);
        let mut alpha = Vec::with_capacity(gw * gh);
        for gy in 0..gh {
            for gx in 0..gw {
                let (x, y) = (gx * self.w / gw, gy * self.h / gh);
                alpha.push(if self.people[y * self.w + x] > 0.5 {
                    0
                } else {
                    255
                });
            }
        }
        let mut matte = aura_recipe::retouch_tools::Matte::encode(
            [0.0, 0.0, 1.0, 1.0],
            gw as u32,
            gh as u32,
            &alpha,
        );
        // The zone is a box round each person, not an outline: nothing to snap to.
        matte.refine_edges = false;
        Some(matte)
    }

    /// Share of a normalised ellipse's bounding box that lies in the person zone.
    #[must_use]
    pub fn people_in(&self, region: [f32; 4]) -> f32 {
        let [cx, cy, rx, ry] = region;
        let x0 = ((cx - rx).max(0.0) * self.w as f32) as usize;
        let x1 = (((cx + rx).min(1.0) * self.w as f32) as usize).min(self.w);
        let y0 = ((cy - ry).max(0.0) * self.h as f32) as usize;
        let y1 = (((cy + ry).min(1.0) * self.h as f32) as usize).min(self.h);
        let (mut inside, mut n) = (0.0_f32, 0_usize);
        for y in (y0..y1).step_by(4) {
            for x in (x0..x1).step_by(4) {
                inside += self.people[y * self.w + x];
                n += 1;
            }
        }
        inside / n.max(1) as f32
    }

    /// How many original pixels one preview pixel stands for, at least 1.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.capture.width.map_or(1.0, |full| {
            (full as f32 / self.w.max(self.h) as f32).max(1.0)
        })
    }
}

/// Each face box widened to the shoulders, from half a face above it to the bottom of the frame:
/// where a person's head, hair, body and clothes can be. Scene steps stay out of it.
fn person_zone(faces: &[PortraitFace], w: usize, h: usize) -> Vec<f32> {
    let mut plane = vec![0.0; w * h];
    for face in faces {
        let [l, t, r, _] = face.bounds;
        let fw = (r - l).max(0.0);
        let x0 = ((l - 1.2 * fw) * w as f32).max(0.0) as usize;
        let x1 = (((r + 1.2 * fw) * w as f32) as usize).min(w);
        let y0 = ((t - 0.6 * fw) * h as f32).max(0.0) as usize;
        for y in y0..h {
            plane[y * w + x0..y * w + x1].fill(1.0);
        }
    }
    plane
}

/// Display-encoded opponent colour: luminance, green-magenta and blue-yellow.
#[must_use]
pub fn opponent(p: [f32; 3]) -> [f32; 3] {
    let l = p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722;
    [l, p[1] - 0.5 * (p[0] + p[2]), p[2] - 0.5 * (p[0] + p[1])]
}

/// Hexcone hue in `0..1`, as the renderer's HSL bands read it; `None` for a grey.
#[must_use]
pub fn hue(p: [f32; 3]) -> Option<f32> {
    let max = p[0].max(p[1]).max(p[2]);
    let min = p[0].min(p[1]).min(p[2]);
    let d = max - min;
    if d < 1e-4 {
        return None;
    }
    let h = if (max - p[0]).abs() < f32::EPSILON {
        ((p[1] - p[2]) / d).rem_euclid(6.0)
    } else if (max - p[1]).abs() < f32::EPSILON {
        (p[2] - p[0]) / d + 2.0
    } else {
        (p[0] - p[1]) / d + 4.0
    };
    Some(h / 6.0)
}

/// Hexcone saturation in `0..1`.
#[must_use]
pub fn saturation(p: [f32; 3]) -> f32 {
    let max = p[0].max(p[1]).max(p[2]);
    let min = p[0].min(p[1]).min(p[2]);
    if max <= 1e-4 {
        0.0
    } else {
        (max - min) / max
    }
}

/// A connected area of marked cells on a coarse grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blob {
    /// Cell bounds, inclusive-exclusive.
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
    pub cells: usize,
}

impl Blob {
    /// The normalised ellipse `[cx, cy, rx, ry]` that covers the blob's rectangle with `margin`
    /// cells to spare on each side.
    #[must_use]
    pub fn ellipse(&self, gw: usize, gh: usize, margin: f32) -> [f32; 4] {
        let (x0, x1) = (self.x0 as f32 - margin, self.x1 as f32 + margin);
        let (y0, y1) = (self.y0 as f32 - margin, self.y1 as f32 + margin);
        // An ellipse through a rectangle's corners has radii sqrt(2) times its half sides.
        [
            (f32::midpoint(x0, x1) / gw as f32).clamp(0.0, 1.0),
            (f32::midpoint(y0, y1) / gh as f32).clamp(0.0, 1.0),
            ((x1 - x0) / 2.0 / gw as f32 * std::f32::consts::SQRT_2).clamp(0.002, 1.0),
            ((y1 - y0) / 2.0 / gh as f32 * std::f32::consts::SQRT_2).clamp(0.002, 1.0),
        ]
    }
}

/// Four-connected areas of `marked` cells on a `gw x gh` grid, largest first.
#[must_use]
pub fn blobs(marked: &[bool], gw: usize, gh: usize) -> Vec<Blob> {
    let mut seen = vec![false; marked.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..marked.len().min(gw * gh) {
        if !marked[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut blob = Blob {
            x0: start % gw,
            y0: start / gw,
            x1: start % gw + 1,
            y1: start / gw + 1,
            cells: 0,
        };
        while let Some(i) = stack.pop() {
            let (x, y) = (i % gw, i / gw);
            blob.cells += 1;
            blob.x0 = blob.x0.min(x);
            blob.y0 = blob.y0.min(y);
            blob.x1 = blob.x1.max(x + 1);
            blob.y1 = blob.y1.max(y + 1);
            let mut visit = |j: usize| {
                if marked[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < gw {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - gw);
            }
            if y + 1 < gh {
                visit(i + gw);
            }
        }
        out.push(blob);
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.cells));
    out
}

/// A coarse grid over a plane: the share of each `cell x cell` block where `test` holds.
#[must_use]
pub fn grid_share(
    w: usize,
    h: usize,
    cell: usize,
    test: impl Fn(usize) -> bool,
) -> (Vec<f32>, usize, usize) {
    let cell = cell.max(1);
    let (gw, gh) = (w.div_ceil(cell), h.div_ceil(cell));
    let mut hits = vec![0_u32; gw * gh];
    let mut totals = vec![0_u32; gw * gh];
    for y in 0..h {
        for x in 0..w {
            let g = (y / cell) * gw + x / cell;
            totals[g] += 1;
            if test(y * w + x) {
                hits[g] += 1;
            }
        }
    }
    let shares = hits
        .iter()
        .zip(&totals)
        .map(|(a, b)| *a as f32 / (*b).max(1) as f32)
        .collect();
    (shares, gw, gh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobs_are_found_largest_first_and_cover_their_cells() {
        let (gw, gh) = (6, 4);
        let mut marked = vec![false; gw * gh];
        for i in [0, 1, 6, 7, 8] {
            marked[i] = true;
        }
        marked[23] = true;
        let found = blobs(&marked, gw, gh);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].cells, 5);
        assert_eq!(
            (found[0].x0, found[0].y0, found[0].x1, found[0].y1),
            (0, 0, 3, 2)
        );
        let e = found[0].ellipse(gw, gh, 0.0);
        assert!((e[0] - 0.25).abs() < 1e-6 && (e[1] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn the_person_zone_runs_from_above_the_face_to_the_bottom() {
        let face = PortraitFace {
            bounds: [0.4, 0.2, 0.5, 0.35],
            landmarks: [[0.0; 2]; 5],
            confidence: 0.9,
        };
        let zone = person_zone(&[face], 100, 100);
        assert!(zone[99 * 100 + 45] > 0.5, "below the face is the body");
        assert!(zone[5 * 100 + 45] < 0.5, "well above the head is not");
        assert!(zone[50 * 100 + 5] < 0.5, "far to the side is not");
    }

    #[test]
    fn hue_and_opponent_read_display_colour() {
        assert!(hue([0.5; 3]).is_none());
        assert!(hue([1.0, 0.0, 0.0]).is_some_and(|h| h.abs() < 1e-6));
        assert!(hue([0.0, 0.0, 1.0]).is_some_and(|h| (h - 2.0 / 3.0).abs() < 1e-6));
        let [_, gm, by] = opponent([0.4, 0.5, 0.4]);
        assert!(gm > 0.0 && by < 0.0);
    }
}
