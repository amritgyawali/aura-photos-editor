//! Portrait light, rendered: a key light shaped by each head, a fill on its shadow side,
//! catchlights in eyes that have none, and a soft glow. ADR-0127.
//!
//! # A head is round
//!
//! Relighting a face by brightening one half of a rectangle looks like a gradient laid over
//! somebody. What a light does to a face depends on which way each part of it faces, so every
//! detected face is given a shape: an ellipsoid placed and sized from its own landmarks - the
//! eyes, the eye line's tilt, the mouth - wide across the cheeks, taller than it is wide, and as
//! deep as it is wide. Each pixel's surface normal comes from where it falls on that shape, and a
//! virtual light shades it by how directly the normal faces the light, with a wrap term so the
//! light rolls softly round the side of the head the way a large source does. The cheek nearer
//! the light brightens, the far cheek and jaw fall into shadow, and the transition follows the
//! curve of the face rather than a straight line.
//!
//! The light is added in log terms and mostly renormalised, so a key light *models* a face more
//! than it brightens it, and it never changes a colour: every gain is a change of brightness that
//! keeps the pixel's own chromaticity, which is the rule every tone stage in this renderer keeps.
//!
//! # Fill measures the shadow that is there
//!
//! The fill does not assume where the shadow is; it reads it. Each face's skin brightness is
//! smoothed at a fifth of the eye distance, the median is taken over the skin, and skin that sits
//! below that median is lifted by as much as a stop at full travel - the shadow side of a face lit
//! from one side, the hollow under a brow lit from above. Skin above the median is left alone, and
//! eyes, brows, lips and hair are not skin, so they keep their own depth.
//!
//! # Catchlights are placed, never painted at random
//!
//! A catchlight is a reflection of the light in the cornea, so it is placed on the side of the
//! iris facing the light - the key light when there is one, otherwise up and to the camera's left
//! where most portrait light comes from - and both eyes of a face get it in the same place. Its
//! size follows the iris, its brightness follows the white of that same eye, and its colour leans
//! toward the light the white of the eye shows. An eye too small to read, an iris that is not
//! there, or an eye that already has a catchlight is left alone.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names
)]

use aura_portrait::{Plane, PortraitMap, Region};
use aura_recipe::studio_finish::Light;
use rayon::prelude::*;

use crate::colour::{luma, set_luma};
use crate::studio_finish::{Face, Scale};

/// The face ellipse's centre, below the eye line, in interocular units.
pub const FACE_CENTRE: f32 = 0.55;
/// The face ellipse's half-width, cheek to cheek, in interocular units.
pub const FACE_HALF_WIDTH: f32 = 1.05;
/// The face ellipse's half-height, hairline to chin, in interocular units.
pub const FACE_HALF_HEIGHT: f32 = 1.5;
/// How far past the face's edge the key light still reaches - the ears, the hairline, the top of
/// the neck - as a multiple of the ellipse.
pub const REACH: f32 = 1.45;
/// How far a soft light wraps round the side of the head.
pub const WRAP: f32 = 0.45;
/// The key light's intensity at full travel, against the light already on the face.
pub const KEY_MAX: f32 = 1.6;
/// How much of the brightness a key light adds is taken back, so it models more than it lifts.
pub const KEY_RENORMALISE: f32 = 0.75;
/// The most a fill lifts the deepest shadow on a face, in stops.
pub const FILL_MAX: f32 = 1.0;
/// Skin this many stops below the face's median gets the whole fill.
pub const FILL_DEPTH: f32 = 1.5;
/// The fill reads the face's light smoothed at this share of the eye distance.
pub const FILL_SMOOTHING: f32 = 0.2;
/// An iris smaller than this radius, in render pixels, is too small to place a catchlight in.
pub const SMALLEST_IRIS: f32 = 1.8;
/// A pixel this many times the iris's median brightness, and as bright as the white of the
/// eye, is a catchlight already.
pub const CATCHLIGHT_CONTRAST: f32 = 3.5;
/// A catchlight's soft radius as a share of the iris radius.
pub const CATCHLIGHT_SIZE: f32 = 0.14;
/// The glow's reach as a share of the shorter edge.
pub const GLOW_RADIUS: f32 = 0.015;

fn px(rgb: &[f32], i: usize) -> [f32; 3] {
    [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A region plane read at render-buffer positions without resolving it to the buffer's size.
struct Reader<'a> {
    plane: Option<&'a Plane>,
    sx: f32,
    sy: f32,
}

impl<'a> Reader<'a> {
    fn new(map: &'a PortraitMap, region: Region, w: usize, h: usize) -> Self {
        let plane = map.plane(region);
        let (pw, ph) = plane.map_or((1, 1), |p| (p.width, p.height));
        Self {
            plane,
            sx: pw as f32 / w.max(1) as f32,
            sy: ph as f32 / h.max(1) as f32,
        }
    }

    fn at(&self, x: f32, y: f32) -> f32 {
        self.plane
            .map_or(0.0, |p| p.sample((x + 0.5) * self.sx, (y + 0.5) * self.sy))
    }
}

/// The light's direction in camera coordinates: `x` right, `y` down, `z` toward the camera.
fn light_vector(light: &Light) -> [f32; 3] {
    let (st, ct) = light.direction.to_radians().sin_cos();
    let (sp, cp) = light.height.to_radians().sin_cos();
    [st * cp, -sp, ct * cp]
}

/// A face's ellipse coordinates at a buffer position: across and down, in ellipse units.
fn ellipse(face: &Face, x: f32, y: f32) -> (f32, f32) {
    let dx = x - face.mid[0];
    let dy = y - face.mid[1];
    let across = (dx * face.u[0] + dy * face.u[1]) / face.io;
    let down = (dx * face.v[0] + dy * face.v[1]) / face.io;
    (
        across / FACE_HALF_WIDTH,
        (down - FACE_CENTRE) / FACE_HALF_HEIGHT,
    )
}

/// How fully a pixel at ellipse radius `r` belongs to the head the light falls on.
fn reach(r: f32) -> f32 {
    1.0 - smoothstep(1.0, REACH, r)
}

/// The wrapped Lambert shade of the ellipsoid at ellipse coordinates `(ex, ey)`, for a light
/// already turned into the face's own upright frame.
fn shade(ex: f32, ey: f32, l: [f32; 3]) -> f32 {
    let r2 = ex * ex + ey * ey;
    let nz = (1.0 - r2).max(0.0).sqrt();
    // The gradient of an ellipsoid as deep as it is wide, in units where the half-width is one.
    let n = [ex, ey * (FACE_HALF_WIDTH / FACE_HALF_HEIGHT), nz];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    let d = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / len;
    ((d + WRAP) / (1.0 + WRAP)).clamp(0.0, 1.0)
}

/// The mean shade over the face, for renormalising: a fixed grid over the unit disc, so it
/// depends on the light and the face's shape and on nothing in the photograph.
fn mean_shade(l: [f32; 3]) -> f32 {
    const N: usize = 24;
    let (mut sum, mut count) = (0.0f32, 0usize);
    for j in 0..N {
        for i in 0..N {
            let ex = (i as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let ey = (j as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            if ex * ex + ey * ey <= 1.0 {
                sum += shade(ex, ey, l);
                count += 1;
            }
        }
    }
    sum / count.max(1) as f32
}

/// The light turned into a face's own upright frame.
fn face_light(face: &Face, l: [f32; 3]) -> [f32; 3] {
    [
        l[0] * face.u[0] + l[1] * face.u[1],
        l[0] * face.v[0] + l[1] * face.v[1],
        l[2],
    ]
}

/// One face's fill: its skin brightness smoothed on a small grid, and the median of it.
struct Fill {
    x0: f32,
    y0: f32,
    step: f32,
    gw: usize,
    gh: usize,
    base: Vec<f32>,
    median: f32,
}

impl Fill {
    /// Measure a face's fill from the buffer. `None` when the face has too little skin to read.
    fn measure(rgb: &[f32], w: usize, h: usize, face: &Face, skin: &Reader<'_>) -> Option<Self> {
        let half = REACH * FACE_HALF_HEIGHT.max(FACE_HALF_WIDTH) * face.io;
        let centre = [
            face.mid[0] + face.v[0] * FACE_CENTRE * face.io,
            face.mid[1] + face.v[1] * FACE_CENTRE * face.io,
        ];
        let x0 = (centre[0] - half).max(0.0);
        let y0 = (centre[1] - half).max(0.0);
        let x1 = (centre[0] + half).min(w as f32);
        let y1 = (centre[1] + half).min(h as f32);
        if x1 - x0 < 4.0 || y1 - y0 < 4.0 {
            return None;
        }
        // About a hundred grid cells across the face, never finer than a pixel.
        let step = ((x1 - x0).max(y1 - y0) / 96.0).max(1.0);
        let gw = (((x1 - x0) / step).ceil() as usize).max(1);
        let gh = (((y1 - y0) / step).ceil() as usize).max(1);
        let mut sum = vec![0.0f32; gw * gh];
        let mut weight = vec![0.0f32; gw * gh];
        let (ix0, iy0) = (x0 as usize, y0 as usize);
        let (ix1, iy1) = ((x1.ceil() as usize).min(w), (y1.ceil() as usize).min(h));
        for y in iy0..iy1 {
            for x in ix0..ix1 {
                let s = skin.at(x as f32, y as f32);
                if s <= 0.05 {
                    continue;
                }
                let gx = (((x as f32 - x0) / step) as usize).min(gw - 1);
                let gy = (((y as f32 - y0) / step) as usize).min(gh - 1);
                let l = luma(px(rgb, y * w + x)).max(1e-5).log2();
                sum[gy * gw + gx] += l * s;
                weight[gy * gw + gx] += s;
            }
        }
        let total: f32 = weight.iter().sum();
        if total < 32.0 {
            return None;
        }
        // Smooth skin brightness with the weights, so a cell with no skin borrows from its
        // neighbours rather than reading as black.
        let r = ((FILL_SMOOTHING * face.io / step).round() as usize).max(1);
        let blurred_sum = crate::spatial::blur_plane(&sum, gw, gh, r);
        let blurred_weight = crate::spatial::blur_plane(&weight, gw, gh, r);
        let base: Vec<f32> = blurred_sum
            .iter()
            .zip(blurred_weight.iter())
            .map(|(s, w)| if *w > 1e-6 { s / w } else { 0.0 })
            .collect();
        // The skin median, from the cells that hold skin.
        let mut cells: Vec<f32> = base
            .iter()
            .zip(weight.iter())
            .filter(|(_, w)| **w > 0.5)
            .map(|(b, _)| *b)
            .collect();
        if cells.is_empty() {
            return None;
        }
        cells.sort_by(f32::total_cmp);
        let median = cells[cells.len() / 2];
        Some(Self {
            x0,
            y0,
            step,
            gw,
            gh,
            base,
            median,
        })
    }

    fn base_at(&self, x: f32, y: f32) -> f32 {
        let gx = ((x - self.x0) / self.step - 0.5).clamp(0.0, (self.gw - 1) as f32);
        let gy = ((y - self.y0) / self.step - 0.5).clamp(0.0, (self.gh - 1) as f32);
        let (ix, iy) = (gx.floor() as usize, gy.floor() as usize);
        let (jx, jy) = ((ix + 1).min(self.gw - 1), (iy + 1).min(self.gh - 1));
        let (tx, ty) = (gx - ix as f32, gy - iy as f32);
        let at = |x: usize, y: usize| self.base[y * self.gw + x];
        let top = at(ix, iy) * (1.0 - tx) + at(jx, iy) * tx;
        let bottom = at(ix, jy) * (1.0 - tx) + at(jx, jy) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// Key light and fill on every face, as one gain per pixel in stops.
fn apply_key_and_fill(rgb: &mut [f32], w: usize, h: usize, light: &Light, map: &PortraitMap) {
    let scale = Scale::of(map, w, h);
    let faces: Vec<Face> = map.faces.iter().map(|g| scale.face(g)).collect();
    if faces.is_empty() {
        return;
    }
    let background = Reader::new(map, Region::Background, w, h);
    let skin = Reader::new(map, Region::Skin, w, h);
    let neck = Reader::new(map, Region::Neck, w, h);
    let l = light_vector(light);
    let k = KEY_MAX * light.key / 100.0;
    let lit: Vec<([f32; 3], f32)> = faces
        .iter()
        .map(|f| {
            let fl = face_light(f, l);
            (fl, KEY_RENORMALISE * (1.0 + k * mean_shade(fl)).log2())
        })
        .collect();
    let fills: Vec<Option<Fill>> = if light.fill > 0.0 {
        faces
            .iter()
            .map(|f| Fill::measure(rgb, w, h, f, &skin))
            .collect()
    } else {
        faces.iter().map(|_| None).collect()
    };
    let fill = FILL_MAX * light.fill / 100.0;
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let yf = y as f32;
        for (x, pixel) in row.chunks_mut(3).enumerate() {
            let xf = x as f32;
            let mut gain = 0.0f32;
            let mut person: Option<f32> = None;
            for (i, face) in faces.iter().enumerate() {
                let (ex, ey) = ellipse(face, xf, yf);
                let r = (ex * ex + ey * ey).sqrt();
                if r >= REACH {
                    continue;
                }
                let on_person = *person.get_or_insert_with(|| 1.0 - background.at(xf, yf));
                if k > 0.0 {
                    let (fl, renorm) = lit[i];
                    let g = (1.0 + k * shade(ex, ey, fl)).log2() - renorm;
                    gain += g * reach(r) * on_person;
                }
                if let Some(f) = &fills[i] {
                    let depth = f.median - f.base_at(xf, yf);
                    let skinness = skin.at(xf, yf).max(neck.at(xf, yf) * 0.7);
                    gain += fill * smoothstep(0.0, FILL_DEPTH, depth) * skinness * reach(r);
                }
            }
            if gain.abs() < 1e-5 {
                continue;
            }
            let value = [pixel[0], pixel[1], pixel[2]];
            let out = set_luma(value, luma(value) * gain.clamp(-1.5, 1.5).exp2());
            pixel.copy_from_slice(&out);
        }
    });
}

/// What one eye looks like to the catchlight stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeReading {
    /// The iris centre, in render pixels.
    pub centre: [f32; 2],
    /// The iris radius, in render pixels.
    pub radius: f32,
    /// True when the iris already holds a reflection of a light.
    pub has_catchlight: bool,
    /// The white of the eye's typical colour, when it could be read.
    pub sclera: Option<[f32; 3]>,
    /// The iris's median brightness.
    pub iris_level: f32,
}

/// The parse's planes an eye is read from.
struct EyePlanes<'a> {
    iris: Reader<'a>,
    sclera: Reader<'a>,
}

/// The median of a list, or `None` for one too short to trust.
fn median_of(values: &mut [f32], fewest: usize) -> Option<f32> {
    if values.len() < fewest {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values[values.len() / 2])
}

/// Read one eye near `at` (render pixels) on a face whose eye distance is `io`.
///
/// The parse's iris region is soft: it reaches onto the lid above an iris and onto the white at
/// its sides. So the iris is read in two passes - its median brightness first, then only the
/// pixels near that brightness as its core - which keeps the lid's skin and the white's edge out
/// of both the centre and the catchlight test.
fn read_eye(
    rgb: &[f32],
    w: usize,
    h: usize,
    at: [f32; 2],
    io: f32,
    planes: &EyePlanes<'_>,
) -> Option<EyeReading> {
    let window = (0.32 * io).max(3.0);
    let x0 = (at[0] - window).floor().max(0.0) as usize;
    let y0 = (at[1] - window).floor().max(0.0) as usize;
    let x1 = ((at[0] + window).ceil() as usize).min(w);
    let y1 = ((at[1] + window).ceil() as usize).min(h);
    let mut iris_pixels = Vec::new();
    let mut whites = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let (xf, yf) = (x as f32, y as f32);
            let p = px(rgb, y * w + x);
            let a = planes.iris.at(xf, yf);
            if a > 0.3 {
                iris_pixels.push((xf, yf, a, luma(p)));
            }
            if planes.sclera.at(xf, yf) > 0.8 {
                whites.push((luma(p), p));
            }
        }
    }
    let mut levels: Vec<f32> = iris_pixels
        .iter()
        .filter(|p| p.2 >= 0.5)
        .map(|p| p.3)
        .collect();
    let iris_level = median_of(&mut levels, 6)?.max(1e-4);
    // The core: iris pixels near the iris's own brightness.
    let core: Vec<&(f32, f32, f32, f32)> = iris_pixels
        .iter()
        .filter(|p| p.3 <= 3.0 * iris_level)
        .collect();
    let sw: f32 = core.iter().map(|p| p.2).sum();
    if sw < 6.0 {
        return None;
    }
    let centre = [
        core.iter().map(|p| p.0 * p.2).sum::<f32>() / sw,
        core.iter().map(|p| p.1 * p.2).sum::<f32>() / sw,
    ];
    let radius = (sw / std::f32::consts::PI).sqrt();
    if radius < SMALLEST_IRIS || radius > 0.35 * io {
        return None;
    }
    // The white of the eye: its median, so the edge pixels the region shares with the iris and
    // the lids do not pull it down.
    let sclera = if whites.len() >= 4 {
        whites.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mid = &whites[(whites.len() / 4)..=(whites.len() * 3 / 4)];
        let n = mid.len().max(1) as f32;
        let mut mean = [0.0f32; 3];
        for (_, p) in mid {
            for (m, v) in mean.iter_mut().zip(p.iter()) {
                *m += v / n;
            }
        }
        Some(mean)
    } else {
        None
    };
    let white_level = sclera.map_or(iris_level * 6.0, |s| luma(s) * 0.75);
    // A catchlight already: inside the iris core's disc, far brighter than the iris and close to
    // the brightness of the white of the eye.
    let has_catchlight = iris_pixels.iter().any(|p| {
        (p.0 - centre[0]).hypot(p.1 - centre[1]) <= 0.8 * radius
            && p.3 >= (iris_level * CATCHLIGHT_CONTRAST).max(white_level)
    });
    Some(EyeReading {
        centre,
        radius,
        has_catchlight,
        sclera,
        iris_level,
    })
}

/// Every readable eye in the frame, face by face.
#[must_use]
pub fn read_eyes(rgb: &[f32], width: u32, height: u32, map: &PortraitMap) -> Vec<Vec<EyeReading>> {
    let (w, h) = (width as usize, height as usize);
    let scale = Scale::of(map, w, h);
    let planes = eye_planes(map, w, h);
    map.faces
        .iter()
        .map(|g| {
            if g.eyes_measured == 0 {
                return Vec::new();
            }
            let face = scale.face(g);
            [face.left_eye, face.right_eye]
                .iter()
                .filter_map(|e| read_eye(rgb, w, h, *e, face.io, &planes))
                .collect()
        })
        .collect()
}

fn eye_planes(map: &PortraitMap, w: usize, h: usize) -> EyePlanes<'_> {
    EyePlanes {
        iris: Reader::new(map, Region::Iris, w, h),
        sclera: Reader::new(map, Region::Sclera, w, h),
    }
}

/// Where in the iris a light from `l` is reflected: the side facing it.
fn catchlight_offset(l: [f32; 3]) -> [f32; 2] {
    let (x, y) = (l[0], l[1]);
    let len = x.hypot(y);
    if len < 0.15 {
        // A light from the camera reflects near the centre; put it up and to the camera's left,
        // where most portrait light comes from, so it reads as a light rather than a pupil.
        let d = [-0.55f32, -0.83];
        return d;
    }
    [x / len, y / len]
}

fn apply_catchlights(rgb: &mut [f32], w: usize, h: usize, light: &Light, map: &PortraitMap) {
    let eyes = read_eyes(rgb, w as u32, h as u32, map);
    let planes = eye_planes(map, w, h);
    let dir = catchlight_offset(light_vector(light));
    let amount = (light.catchlights / 100.0).clamp(0.0, 1.0);
    let source = rgb.to_vec();
    let level_at = |x: f32, y: f32| -> f32 {
        let (xi, yi) = (x.round().max(0.0) as usize, y.round().max(0.0) as usize);
        luma(px(&source, yi.min(h - 1) * w + xi.min(w - 1)))
    };
    // A face whose eyes already reflect a light keeps them as they are - both of them. A
    // catchlight in one eye and not the other reads as a mistake, and one eye that seems to have
    // none while the other has one is far more often a misreading than a different light.
    for eye in eyes
        .iter()
        .filter(|face| !face.iter().any(|e| e.has_catchlight))
        .flatten()
    {
        // The dark of the eye - iris and pupil - is where a cornea reflects; a lid is skin and
        // the white is white. Brightness tells them apart where the parse's soft regions do not.
        let dark = |x: f32, y: f32| -> f32 {
            planes.iris.at(x, y).min(1.0)
                * (1.0 - smoothstep(2.0 * eye.iris_level, 5.0 * eye.iris_level, level_at(x, y)))
        };
        // Toward the light by up to two fifths of the iris, but only onto the dark of the eye.
        let at = [0.42f32, 0.34, 0.26, 0.18, 0.1, 0.0]
            .iter()
            .map(|t| {
                [
                    eye.centre[0] + dir[0] * t * eye.radius,
                    eye.centre[1] + dir[1] * t * eye.radius,
                ]
            })
            .find(|p| dark(p[0], p[1]) >= 0.5);
        let Some(at) = at else {
            continue;
        };
        // About a third of the iris across, which is what a softbox or a window makes.
        let size = (CATCHLIGHT_SIZE * eye.radius).clamp(0.6, 6.0);
        // As bright as a reflection of a light is: well above the white of the eye, and leaning
        // only a quarter of the way toward the white's colour - an eye white is often pink, and
        // the light it reflects is not.
        let (level, tint) = eye.sclera.map_or((1.3, [1.0; 3]), |s| {
            let l = luma(s).max(1e-4);
            ((2.0 * l).clamp(1.0, 3.0), s.map(|v| 0.75 + 0.25 * v / l))
        });
        let target = tint.map(|t| t * level);
        let reach = (size * 2.5).ceil() as isize;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let x = (at[0] + dx as f32).round();
                let y = (at[1] + dy as f32).round();
                if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                    continue;
                }
                let d = (x - at[0]).hypot(y - at[1]) / size;
                let alpha = amount * (-d * d * 0.9).exp() * dark(x, y);
                if alpha < 1e-3 {
                    continue;
                }
                let i = (y as usize * w + x as usize) * 3;
                for c in 0..3 {
                    let v = rgb[i + c];
                    rgb[i + c] = v + alpha * (target[c] - v).max(0.0);
                }
            }
        }
    }
}

/// Key light, fill and catchlights on every face the parse found.
pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, light: &Light, map: &PortraitMap) {
    if light.key > 0.0 || light.fill > 0.0 {
        apply_key_and_fill(rgb, w, h, light, map);
    }
    if light.catchlights > 0.0 {
        apply_catchlights(rgb, w, h, light, map);
    }
}

/// A soft glow from the brightest parts of the frame: the highlights bloom outward, a little of
/// the whole frame is softened toward its own blur, and the darks are left alone.
pub(crate) fn apply_glow(rgb: &mut [f32], w: usize, h: usize, glow: f32) {
    let a = (glow / 100.0).clamp(0.0, 1.0);
    if a <= 0.0 || w == 0 || h == 0 {
        return;
    }
    let radius = (w.min(h) as f32 * GLOW_RADIUS).max(2.0);
    // Blur on a reduced grid so the cost does not grow with the radius: about four cells per
    // radius, area-averaged down and bilinearly back.
    let factor = (radius / 4.0).floor().max(1.0) as usize;
    let (gw, gh) = (w.div_ceil(factor), h.div_ceil(factor));
    let mut bloom = vec![0.0f32; gw * gh * 3];
    let mut soft = vec![0.0f32; gw * gh * 3];
    let mut count = vec![0.0f32; gw * gh];
    for y in 0..h {
        for x in 0..w {
            let g = (y / factor) * gw + x / factor;
            let p = px(rgb, y * w + x);
            let high = smoothstep(0.45, 1.6, luma(p));
            for c in 0..3 {
                bloom[g * 3 + c] += p[c] * high;
                soft[g * 3 + c] += p[c];
            }
            count[g] += 1.0;
        }
    }
    let r = ((radius / factor as f32).round() as usize).max(1);
    let mut planes = Vec::with_capacity(6);
    for layer in [&bloom, &soft] {
        for c in 0..3 {
            let plane: Vec<f32> = (0..gw * gh)
                .map(|g| layer[g * 3 + c] / count[g].max(1.0))
                .collect();
            planes.push(crate::spatial::blur_plane(&plane, gw, gh, r));
        }
    }
    let sample = |plane: &[f32], x: usize, y: usize| -> f32 {
        let gx = ((x as f32 + 0.5) / factor as f32 - 0.5).clamp(0.0, (gw - 1) as f32);
        let gy = ((y as f32 + 0.5) / factor as f32 - 0.5).clamp(0.0, (gh - 1) as f32);
        let (ix, iy) = (gx.floor() as usize, gy.floor() as usize);
        let (jx, jy) = ((ix + 1).min(gw - 1), (iy + 1).min(gh - 1));
        let (tx, ty) = (gx - ix as f32, gy - iy as f32);
        let top = plane[iy * gw + ix] * (1.0 - tx) + plane[iy * gw + jx] * tx;
        let bottom = plane[jy * gw + ix] * (1.0 - tx) + plane[jy * gw + jx] * tx;
        top * (1.0 - ty) + bottom * ty
    };
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for (x, pixel) in row.chunks_mut(3).enumerate() {
            for c in 0..3 {
                let bloom = sample(&planes[c], x, y);
                let soft = sample(&planes[3 + c], x, y);
                let v = pixel[c];
                pixel[c] = (v + 0.12 * a * (soft - v)).max(0.0) + 0.6 * a * bloom;
            }
        }
    });
}

/// What Auto light proposes for a photograph, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct LightSuggestion {
    /// The proposal. The identity when nothing was measured or nothing is needed.
    pub light: Light,
    /// Faces the measurement was made on.
    pub faces: usize,
    /// How many stops brighter the lit side of the main face is than its shadow side.
    pub contrast: Option<f32>,
    /// The decisions, in words.
    pub reasons: Vec<String>,
}

/// How a face is lit: the brightness difference between its two sides and between its upper
/// and lower halves, in stops, read on its skin.
fn face_reading(
    rgb: &[f32],
    w: usize,
    h: usize,
    face: &Face,
    skin: &Reader<'_>,
) -> Option<(f32, f32)> {
    let half = FACE_HALF_HEIGHT.max(FACE_HALF_WIDTH) * face.io;
    let centre = [
        face.mid[0] + face.v[0] * FACE_CENTRE * face.io,
        face.mid[1] + face.v[1] * FACE_CENTRE * face.io,
    ];
    let x0 = (centre[0] - half).max(0.0) as usize;
    let y0 = (centre[1] - half).max(0.0) as usize;
    let x1 = ((centre[0] + half).ceil() as usize).min(w);
    let y1 = ((centre[1] + half).ceil() as usize).min(h);
    let step = ((x1.saturating_sub(x0)).max(y1.saturating_sub(y0)) / 160).max(1);
    let (mut left, mut right, mut upper, mut lower) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for y in (y0..y1).step_by(step) {
        for x in (x0..x1).step_by(step) {
            let (xf, yf) = (x as f32, y as f32);
            if skin.at(xf, yf) < 0.5 {
                continue;
            }
            let (ex, ey) = ellipse(face, xf, yf);
            if ex * ex + ey * ey > 0.8 {
                continue;
            }
            let l = luma(px(rgb, y * w + x)).max(1e-5).log2();
            if ex < -0.2 {
                left.push(l);
            } else if ex > 0.2 {
                right.push(l);
            }
            if ey < -0.15 {
                upper.push(l);
            } else if ey > 0.15 {
                lower.push(l);
            }
        }
    }
    let median = |v: &mut Vec<f32>| -> Option<f32> {
        if v.len() < 12 {
            return None;
        }
        v.sort_by(f32::total_cmp);
        Some(v[v.len() / 2])
    };
    let across = median(&mut right)? - median(&mut left)?;
    let down = match (median(&mut upper), median(&mut lower)) {
        (Some(u), Some(d)) => u - d,
        _ => 0.0,
    };
    Some((across, down))
}

/// Measure every face's light and propose a portrait light for the photograph.
///
/// Read on the largest face, because a group is lit by one light and the person nearest the
/// camera shows it best:
///
/// - **Harsh side light** (one side of the face more than about a stop darker): a fill sized to
///   the difference, and no key light - the light is already there.
/// - **Flat light** (both sides within a third of a stop): a gentle key light to give the face
///   shape, from the side that is already a little brighter, or from the camera's left.
/// - **In between**: a light key from where the light already comes from, and a light fill.
/// - **Catchlights** in every readable eye that has none.
#[must_use]
pub fn suggest(rgb: &[f32], width: u32, height: u32, map: &PortraitMap) -> LightSuggestion {
    let (w, h) = (width as usize, height as usize);
    let mut light = Light::default();
    let mut reasons = Vec::new();
    if map.faces.is_empty() {
        reasons.push("No face was found, so no light is proposed.".to_owned());
        return LightSuggestion {
            light,
            faces: 0,
            contrast: None,
            reasons,
        };
    }
    let scale = Scale::of(map, w, h);
    let skin = Reader::new(map, Region::Skin, w, h);
    let main = map
        .faces
        .iter()
        .map(|g| scale.face(g))
        .max_by(|a, b| a.io.total_cmp(&b.io));
    let reading = main
        .as_ref()
        .and_then(|f| face_reading(rgb, w, h, f, &skin));
    let mut contrast = None;
    if let Some((across, down)) = reading {
        let ratio = across.abs();
        contrast = Some(ratio);
        let side = if across >= 0.0 { 1.0 } else { -1.0 };
        let from = if side > 0.0 { "right" } else { "left" };
        if ratio > 1.0 {
            light.fill = ((ratio - 0.6) * 55.0).clamp(15.0, 60.0).round();
            reasons.push(format!(
                "The face's {from} side is {ratio:.1} stops brighter than its other side: a fill of {} opens the shadow side, and no key light is added because the light is already there.",
                light.fill
            ));
        } else if ratio < 0.33 {
            light.key = 30.0;
            light.direction = if ratio > 0.1 { side * 45.0 } else { -45.0 };
            light.height = 35.0;
            reasons.push(format!(
                "The face is lit evenly (its two sides within {ratio:.1} stops): a soft key light of 30 from the {} gives it shape.",
                if light.direction > 0.0 { "right" } else { "left" }
            ));
        } else {
            light.key = 18.0;
            light.direction = side * (30.0 + 40.0 * ratio).min(70.0);
            light.height = if down > 0.2 { 40.0 } else { 25.0 };
            light.fill = 12.0;
            reasons.push(format!(
                "The light comes from the {from} ({ratio:.1} stops between the sides): a light key of 18 from the same side models the face, and a fill of 12 keeps the shadow side open."
            ));
        }
    } else {
        reasons.push(
            "The face shows too little skin to read its light, so no key or fill is proposed."
                .to_owned(),
        );
    }
    let eyes = read_eyes(rgb, width, height, map);
    let readable = eyes.iter().filter(|f| !f.is_empty()).count();
    let lacking = eyes
        .iter()
        .filter(|f| !f.is_empty() && !f.iter().any(|e| e.has_catchlight))
        .count();
    if lacking > 0 {
        light.catchlights = 45.0;
        reasons.push(format!(
            "{lacking} of {readable} faces with readable eyes show no reflection of a light: catchlights of 45 are added, on the side of each iris facing the light."
        ));
    } else if readable > 0 {
        reasons.push("Every face with readable eyes already has catchlights.".to_owned());
    } else {
        reasons.push("No eye was large and open enough to place a catchlight in.".to_owned());
    }
    LightSuggestion {
        light,
        faces: map.faces.len(),
        contrast,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(direction: f32) -> [f32; 3] {
        light_vector(&Light {
            key: 50.0,
            direction,
            height: 0.0,
            ..Light::default()
        })
    }

    #[test]
    fn a_light_from_the_left_shades_the_left_cheek_brighter() {
        let l = lit(-90.0);
        assert!(shade(-0.6, 0.0, l) > shade(0.6, 0.0, l) + 0.3);
        // A light from the camera shades the centre most.
        let front = lit(0.0);
        assert!(shade(0.0, 0.0, front) > shade(0.8, 0.0, front));
    }

    #[test]
    fn the_shade_is_smooth_and_bounded_across_the_face() {
        let l = lit(-45.0);
        let mut last = shade(-1.0, 0.0, l);
        for i in 1..=200 {
            let ex = -1.0 + i as f32 / 100.0;
            let s = shade(ex, 0.0, l);
            assert!((0.0..=1.0).contains(&s));
            assert!((s - last).abs() < 0.08, "a step in the shade at {ex}");
            last = s;
        }
    }

    #[test]
    fn the_mean_shade_depends_only_on_the_light() {
        let a = mean_shade(lit(-45.0));
        let b = mean_shade(lit(-45.0));
        assert_eq!(a.to_bits(), b.to_bits());
        assert!(a > 0.2 && a < 1.0);
    }

    #[test]
    fn a_frontal_light_places_its_catchlight_up_and_to_the_left() {
        let d = catchlight_offset(lit(0.0));
        assert!(d[0] < 0.0 && d[1] < 0.0);
        // A light from the right reflects on the right of the iris.
        let d = catchlight_offset(lit(80.0));
        assert!(d[0] > 0.5);
    }

    #[test]
    fn glow_blooms_round_a_highlight_and_leaves_the_dark_alone() {
        let (w, h) = (96, 96);
        let mut rgb = vec![0.02f32; w * h * 3];
        for y in 44..52 {
            for x in 44..52 {
                for c in 0..3 {
                    rgb[(y * w + x) * 3 + c] = 2.0;
                }
            }
        }
        let before = rgb.clone();
        apply_glow(&mut rgb, w, h, 80.0);
        let near = (40 * w + 48) * 3;
        let far = (5 * w + 5) * 3;
        assert!(
            rgb[near] > before[near] * 2.0,
            "{} -> {}",
            before[near],
            rgb[near]
        );
        assert!(
            (rgb[far] - before[far]).abs() < 0.01,
            "the dark corner moved"
        );
    }
}
