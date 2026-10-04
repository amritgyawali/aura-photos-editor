//! Faces: found by the cascade, kept only with evidence, and measured for eyes and a mouth.
//!
//! # Five passes, one evidence rule
//!
//! The frontal cascade alone misses a head tilted past about fifteen degrees and a face turned
//! to one side, and those are half of all portraits. So the frame is scanned five ways:
//!
//! 1. frontal, upright;
//! 2. frontal, with the frame turned twenty degrees each way, so a tilted head is upright;
//! 3. the profile cascade, and the profile cascade on the mirrored frame;
//! 4. frontal on a locally equalised copy, which recovers faces the plain pass cannot see in
//!    a dark corner - and which is the pass the measured fairness gap needed (a very dark face
//!    on a dark ground has less luma structure for a luma cascade to read).
//!
//! More passes is more false positives, and the plain pass already finds wallpaper. So a
//! window becomes a face only with **evidence**: a skin-coloured centre (unless the photograph
//! has no colour), and either many agreeing windows or an eye found where an eye should be.
//! The extra passes are held to a stricter version of the same rule.
//!
//! # A person can always tell AURA where a face is
//!
//! A [`FaceHint`] is a box a photographer drew. It is honoured over every detection it
//! overlaps, carried in the recipe so a render can re-create it, and measured exactly like a
//! detected face. There is no setting in which a face the detector missed cannot be retouched.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::canvas::Canvas;
use crate::cascade::{Cascade, Detection, GreyImage, ScanParams};
use crate::skin;

/// The long edge the cascade scans. Faces under about 2.5 % of the long edge are not looked
/// for: a face that small is not one anybody retouches.
pub const DETECT_EDGE: u32 = 720;

/// The tilt the two turned passes correct, in radians. Twenty degrees.
pub const TILT: f32 = 0.349;

/// Mean eye centres inside a frontal cascade box, measured on 71 eye pairs from real
/// photographs (`tests/local_eval.rs`). Left and right are image-left and image-right.
const LEFT_EYE: [f32; 2] = [0.305, 0.380];
/// See [`LEFT_EYE`].
const RIGHT_EYE: [f32; 2] = [0.670, 0.385];

/// Which pass found a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaceSource {
    /// Drawn by a person.
    Hint,
    /// The upright frontal pass.
    Frontal,
    /// A frontal pass on a turned frame.
    Tilted,
    /// The frontal pass on a locally equalised frame.
    Equalised,
    /// The profile cascade.
    Profile,
}

impl FaceSource {
    /// Stable text for the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Frontal => "frontal",
            Self::Tilted => "tilted",
            Self::Equalised => "equalised",
            Self::Profile => "profile",
        }
    }
}

/// A box a photographer drew around a face, normalised to the frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FaceHint {
    /// Left edge, `0..1`.
    pub x: f32,
    /// Top edge, `0..1`.
    pub y: f32,
    /// Width, `0..1`.
    pub w: f32,
    /// Height, `0..1`.
    pub h: f32,
}

impl FaceHint {
    /// Parse the `hint:x,y,w,h` form a recipe mask target carries.
    #[must_use]
    pub fn parse(target: &str) -> Option<Self> {
        let body = target.strip_prefix("hint:")?;
        let mut parts = body.split(',').map(|p| p.trim().parse::<f32>().ok());
        let hint = Self {
            x: parts.next()??,
            y: parts.next()??,
            w: parts.next()??,
            h: parts.next()??,
        };
        let finite = [hint.x, hint.y, hint.w, hint.h]
            .iter()
            .all(|v| v.is_finite());
        (finite && hint.w > 0.005 && hint.h > 0.005).then_some(hint)
    }

    /// The `hint:x,y,w,h` form, at four decimals so it round-trips through a recipe.
    #[must_use]
    pub fn to_target(&self) -> String {
        format!(
            "hint:{:.4},{:.4},{:.4},{:.4}",
            self.x.clamp(0.0, 1.0),
            self.y.clamp(0.0, 1.0),
            self.w.clamp(0.0, 1.0),
            self.h.clamp(0.0, 1.0)
        )
    }
}

/// One face, with the landmarks every region is built from. Canvas pixels throughout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceGeometry {
    /// Centre of the cascade box.
    pub center: [f32; 2],
    /// Side of the (square) cascade box.
    pub size: f32,
    /// Rotation of the eye line from horizontal, radians, clockwise in image coordinates.
    pub roll: f32,
    /// Image-left eye centre.
    pub left_eye: [f32; 2],
    /// Image-right eye centre.
    pub right_eye: [f32; 2],
    /// Tip of the nose, estimated.
    pub nose: [f32; 2],
    /// Centre of the mouth.
    pub mouth: [f32; 2],
    /// Corner-to-corner width of the mouth.
    pub mouth_width: f32,
    /// How many of the two eyes were measured rather than placed by the prior.
    pub eyes_measured: u8,
    /// How many of the two eyes the eye cascade itself agreed with.
    pub eye_hits: u8,
    /// True when the mouth was measured rather than placed by the prior.
    pub mouth_measured: bool,
    /// `0..=1`: how sure the parse is this is a face.
    pub confidence: f32,
    /// Agreeing cascade windows.
    pub neighbours: u32,
    /// Which pass found it.
    pub source: FaceSource,
    /// Share of the face centre the broad skin prior admits.
    pub skin_fraction: f32,
}

impl FaceGeometry {
    /// A point given in box coordinates (`0..1` across the cascade box, in the face's own
    /// upright frame) mapped onto the canvas.
    #[must_use]
    pub fn at(&self, u: f32, v: f32) -> [f32; 2] {
        let (s, c) = self.roll.sin_cos();
        let dx = (u - 0.5) * self.size;
        let dy = (v - 0.5) * self.size;
        [
            self.center[0] + dx * c - dy * s,
            self.center[1] + dx * s + dy * c,
        ]
    }

    /// Distance between the eye centres. The unit every facial region is sized in.
    #[must_use]
    pub fn interocular(&self) -> f32 {
        let d = (self.right_eye[0] - self.left_eye[0]).hypot(self.right_eye[1] - self.left_eye[1]);
        d.max(self.size * 0.2)
    }

    /// The midpoint of the eyes.
    #[must_use]
    pub fn eye_mid(&self) -> [f32; 2] {
        [
            (self.left_eye[0] + self.right_eye[0]) * 0.5,
            (self.left_eye[1] + self.right_eye[1]) * 0.5,
        ]
    }

    /// The axis-aligned bounds of the cascade box: `x`, `y`, `w`, `h`.
    #[must_use]
    pub fn bbox(&self) -> [f32; 4] {
        let corners = [
            self.at(0.0, 0.0),
            self.at(1.0, 0.0),
            self.at(0.0, 1.0),
            self.at(1.0, 1.0),
        ];
        let xs = corners.iter().map(|p| p[0]);
        let ys = corners.iter().map(|p| p[1]);
        let x0 = xs.clone().fold(f32::INFINITY, f32::min);
        let x1 = xs.fold(f32::NEG_INFINITY, f32::max);
        let y0 = ys.clone().fold(f32::INFINITY, f32::min);
        let y1 = ys.fold(f32::NEG_INFINITY, f32::max);
        [x0, y0, x1 - x0, y1 - y0]
    }
}

/// A raw candidate, before evidence.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    center: [f32; 2],
    size: f32,
    roll: f32,
    neighbours: u32,
    source: FaceSource,
}

/// Find every face on a canvas, plus any a person drew.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn detect(canvas: &Canvas, hints: &[FaceHint]) -> Vec<FaceGeometry> {
    if canvas.is_empty() {
        return Vec::new();
    }
    let Some(grey) = GreyImage::new(canvas.width, canvas.height, canvas.grey.clone()) else {
        return Vec::new();
    };
    let long = canvas.width.max(canvas.height);
    let scale = if long > DETECT_EDGE {
        DETECT_EDGE as f32 / long as f32
    } else {
        1.0
    };
    let small = grey.resize(
        ((canvas.width as f32 * scale).round() as u32).max(1),
        ((canvas.height as f32 * scale).round() as u32).max(1),
    );
    let colourful = canvas.is_colourful();

    // The six scans are independent, so they run side by side; each returns its candidates in
    // a fixed order and the passes are concatenated in a fixed order - invariant 4.
    let passes = [
        Pass::Frontal,
        Pass::Tilted(TILT),
        Pass::Tilted(-TILT),
        Pass::Equalised,
        Pass::Profile,
        Pass::ProfileMirrored,
    ];
    let half = if small.width.max(small.height) > 360 {
        small.resize((small.width / 2).max(1), (small.height / 2).max(1))
    } else {
        small.clone()
    };
    let found: Vec<Vec<Candidate>> = passes
        .par_iter()
        .map(|pass| pass.run(&small, &half))
        .collect();
    let mut candidates: Vec<Candidate> = found.into_iter().flatten().collect();
    for c in &mut candidates {
        c.center = [c.center[0] / scale, c.center[1] / scale];
        c.size /= scale;
    }
    candidates.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(b.neighbours.cmp(&a.neighbours))
            .then(a.center[0].total_cmp(&b.center[0]))
            .then(a.center[1].total_cmp(&b.center[1]))
    });

    let mut faces: Vec<FaceGeometry> = hints
        .iter()
        .map(|h| {
            let w = h.w * canvas.width as f32;
            let hh = h.h * canvas.height as f32;
            Candidate {
                center: [
                    (h.x + h.w * 0.5) * canvas.width as f32,
                    (h.y + h.h * 0.5) * canvas.height as f32,
                ],
                // A drawn box usually runs forehead to chin; the cascade's box is tighter.
                size: w.max(hh) * 0.85,
                roll: 0.0,
                neighbours: 0,
                source: FaceSource::Hint,
            }
        })
        .map(|c| measure(canvas, &c, colourful))
        .collect();

    // Cluster before measuring: one cluster per place in the frame, its members in pass
    // priority order. Measuring is the expensive half, so only the best few members of a
    // cluster are measured, and the first that carries evidence is the face.
    let overlaps = |a: [f32; 2], sa: f32, b: [f32; 2], sb: f32| -> bool {
        (a[0] - b[0]).hypot(a[1] - b[1]) < 0.5 * sa.max(sb)
    };
    let mut clusters: Vec<Vec<Candidate>> = Vec::new();
    for c in candidates {
        if faces
            .iter()
            .any(|f| overlaps(f.center, f.size, c.center, c.size))
        {
            continue;
        }
        match clusters.iter_mut().find(|cluster| {
            cluster
                .first()
                .is_some_and(|head| overlaps(head.center, head.size, c.center, c.size))
        }) {
            Some(cluster) => cluster.push(c),
            None => clusters.push(vec![c]),
        }
    }
    let verified: Vec<Option<FaceGeometry>> = clusters
        .par_iter()
        .map(|cluster| {
            cluster
                .iter()
                .take(3)
                .map(|c| measure(canvas, c, colourful))
                .find(|f| accept(f, colourful))
        })
        .collect();
    for face in verified.into_iter().flatten() {
        if !faces
            .iter()
            .any(|f| overlaps(f.center, f.size, face.center, face.size))
        {
            faces.push(face);
        }
    }
    faces.sort_by(|a, b| {
        b.size
            .total_cmp(&a.size)
            .then(a.center[0].total_cmp(&b.center[0]))
    });
    faces
}

/// One scan of the frame.
#[derive(Debug, Clone, Copy)]
enum Pass {
    Frontal,
    Tilted(f32),
    Equalised,
    Profile,
    ProfileMirrored,
}

impl Pass {
    /// Run one scan. The plain pass reads `small`; the five extra passes read `half`, a copy
    /// at half the resolution, because a tilted or turned face worth retouching is a large one
    /// and a quarter of the pixels is a quarter of the cost. Candidates come back in `small`'s
    /// coordinates either way.
    fn run(self, small: &GreyImage, half: &GreyImage) -> Vec<Candidate> {
        let mut out = Vec::new();
        let params = ScanParams::default();
        match self {
            Self::Frontal => {
                if let Some(cascade) = Cascade::frontal() {
                    let found = cascade.detect(small, params);
                    push_all(&mut out, &found, 0.0, FaceSource::Frontal, small);
                }
                return out;
            }
            Self::Tilted(angle) => {
                if let Some(cascade) = Cascade::frontal() {
                    let turned = rotate(half, -angle);
                    let found = cascade.detect(&turned, params);
                    push_all(&mut out, &found, angle, FaceSource::Tilted, half);
                }
            }
            Self::Equalised => {
                if let Some(cascade) = Cascade::frontal() {
                    let found = cascade.detect(&clahe(half, 8, 2.0), params);
                    push_all(&mut out, &found, 0.0, FaceSource::Equalised, half);
                }
            }
            Self::Profile => {
                if let Some(cascade) = Cascade::profile() {
                    let found = cascade.detect(half, params);
                    push_all(&mut out, &found, 0.0, FaceSource::Profile, half);
                }
            }
            Self::ProfileMirrored => {
                if let Some(cascade) = Cascade::profile() {
                    let found: Vec<Detection> = cascade
                        .detect(&half.mirrored(), params)
                        .into_iter()
                        .map(|d| Detection {
                            x: half.width as f32 - d.x - d.w,
                            ..d
                        })
                        .collect();
                    push_all(&mut out, &found, 0.0, FaceSource::Profile, half);
                }
            }
        }
        let k = small.width as f32 / half.width.max(1) as f32;
        for c in &mut out {
            c.center = [c.center[0] * k, c.center[1] * k];
            c.size *= k;
        }
        out
    }
}

fn push_all(
    out: &mut Vec<Candidate>,
    found: &[Detection],
    roll: f32,
    source: FaceSource,
    image: &GreyImage,
) {
    let cx = image.width as f32 * 0.5;
    let cy = image.height as f32 * 0.5;
    let (s, c) = roll.sin_cos();
    for d in found {
        // A detection in a frame turned by `-roll` maps back through a rotation by `roll`.
        let dx = d.cx() - cx;
        let dy = d.cy() - cy;
        out.push(Candidate {
            center: [cx + dx * c - dy * s, cy + dx * s + dy * c],
            size: d.w.max(d.h),
            roll,
            neighbours: d.neighbours,
            source,
        });
    }
}

/// The evidence rule. Stricter for every pass but the plain one.
///
/// Measured on the evaluation set: every false positive the six passes produced had fewer
/// than nine agreeing windows *and* either a skin fraction under 0.65, no eye the eye cascade
/// agreed with, or no mouth - a clock, a badge, a flag, a table top. Every true face with that
/// few windows had all three.
fn accept(face: &FaceGeometry, colourful: bool) -> bool {
    let skin = if colourful { face.skin_fraction } else { 1.0 };
    let n = face.neighbours;
    match face.source {
        FaceSource::Hint => true,
        FaceSource::Frontal => {
            skin >= 0.45
                && (n >= 10
                    || (n >= 4 && face.eyes_measured == 2 && skin >= 0.65 && face.mouth_measured)
                    || (n >= 6 && face.eye_hits >= 1 && skin >= 0.75))
        }
        FaceSource::Tilted => {
            skin >= 0.6 && n >= 5 && face.eyes_measured == 2 && face.mouth_measured
        }
        FaceSource::Equalised => skin >= 0.7 && n >= 6 && face.eye_hits >= 1 && face.mouth_measured,
        FaceSource::Profile => {
            skin >= 0.6 && ((n >= 6 && face.eye_hits >= 1) || (n >= 8 && skin >= 0.75))
        }
    }
}

/// Measure eyes, a mouth, a skin fraction and a confidence for one candidate.
fn measure(canvas: &Canvas, c: &Candidate, colourful: bool) -> FaceGeometry {
    let mut face = FaceGeometry {
        center: c.center,
        size: c.size,
        roll: c.roll,
        left_eye: [0.0; 2],
        right_eye: [0.0; 2],
        nose: [0.0; 2],
        mouth: [0.0; 2],
        mouth_width: 0.0,
        eyes_measured: 0,
        eye_hits: 0,
        mouth_measured: false,
        confidence: 0.0,
        neighbours: c.neighbours,
        source: c.source,
        skin_fraction: 0.0,
    };
    face.left_eye = face.at(LEFT_EYE[0], LEFT_EYE[1]);
    face.right_eye = face.at(RIGHT_EYE[0], RIGHT_EYE[1]);

    // Skin: the centre of the box, under the broad prior.
    let mut total = 0.0;
    let mut n = 0.0;
    for j in 0..12 {
        for i in 0..12 {
            let u = 0.25 + 0.5 * (i as f32 + 0.5) / 12.0;
            let v = 0.35 + 0.5 * (j as f32 + 0.5) / 12.0;
            let p = face.at(u, v);
            total += skin::prior(canvas.lab_at(p[0] as i64, p[1] as i64));
            n += 1.0;
        }
    }
    face.skin_fraction = total / n;

    let (left, right, measured, hits) = locate_eyes(canvas, &face);
    face.left_eye = left;
    face.right_eye = right;
    face.eyes_measured = measured;
    face.eye_hits = hits;
    // The eye line is the better measure of roll whenever both eyes were found.
    if measured == 2 {
        let roll = (right[1] - left[1]).atan2(right[0] - left[0]);
        if (roll - c.roll).abs() < 0.45 {
            face.roll = roll;
        }
    }

    let (mouth, width, mouth_measured) = locate_mouth(canvas, &face, colourful);
    face.mouth = mouth;
    face.mouth_width = width;
    face.mouth_measured = mouth_measured;
    let mid = face.eye_mid();
    face.nose = [
        mid[0] + (mouth[0] - mid[0]) * 0.62,
        mid[1] + (mouth[1] - mid[1]) * 0.62,
    ];

    let agreement = 1.0 - (-(c.neighbours as f32) / 10.0).exp();
    let eyes = f32::from(measured) / 2.0;
    let skin_term = if colourful {
        skin::ramp(face.skin_fraction, 0.2, 0.6)
    } else {
        0.6
    };
    face.confidence = if c.source == FaceSource::Hint {
        1.0
    } else {
        (0.4 * agreement + 0.3 * eyes + 0.2 * skin_term + 0.1 * f32::from(u8::from(mouth_measured)))
            .clamp(0.0, 1.0)
    };
    face
}

/// Sample the canvas grey in a face's upright frame: `u` across `u0..u1`, `v` across
/// `v0..v1`, at `scale` output pixels per box width.
fn upright_grey(canvas: &Canvas, face: &FaceGeometry, window: [f32; 4], side: u32) -> GreyImage {
    let [u0, v0, u1, v1] = window;
    let width = side;
    let height = ((v1 - v0) / (u1 - u0) * side as f32).round().max(1.0) as u32;
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        let v = v0 + (v1 - v0) * (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let u = u0 + (u1 - u0) * (x as f32 + 0.5) / width as f32;
            let p = face.at(u, v);
            pixels.push(sample_grey(canvas, p[0], p[1]));
        }
    }
    GreyImage {
        width,
        height,
        pixels,
    }
}

fn sample_grey(canvas: &Canvas, x: f32, y: f32) -> u8 {
    let fx = x - 0.5;
    let fy = y - 0.5;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = fx - x0;
    let ty = fy - y0;
    let g =
        |xx: i64, yy: i64| f32::from(canvas.grey.get(canvas.index(xx, yy)).copied().unwrap_or(0));
    let (ix, iy) = (x0 as i64, y0 as i64);
    let top = g(ix, iy) * (1.0 - tx) + g(ix + 1, iy) * tx;
    let bottom = g(ix, iy + 1) * (1.0 - tx) + g(ix + 1, iy + 1) * tx;
    (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8
}

/// The eye band, in box coordinates: `u0`, `v0`, `u1`, `v1`.
const EYE_BAND: [f32; 4] = [0.0, 0.12, 1.0, 0.64];
/// Pixels across the face when the eye band is resampled for the eye cascade.
const EYE_SIDE: u32 = 180;

/// Find both eyes: the eye cascade first, then the darkest blob near the expected position.
fn locate_eyes(canvas: &Canvas, face: &FaceGeometry) -> ([f32; 2], [f32; 2], u8, u8) {
    let band = upright_grey(canvas, face, EYE_BAND, EYE_SIDE);
    let to_uv = |x: f32, y: f32| -> [f32; 2] {
        [
            EYE_BAND[0] + (EYE_BAND[2] - EYE_BAND[0]) * x / band.width as f32,
            EYE_BAND[1] + (EYE_BAND[3] - EYE_BAND[1]) * y / band.height as f32,
        ]
    };
    let mut hits: Vec<([f32; 2], f32)> = Vec::new();
    if let Some(eye) = Cascade::eye() {
        let params = ScanParams {
            scale_factor: 1.1,
            min_neighbours: 1,
            min_size: 16,
            max_size: (EYE_SIDE as f32 * 0.42) as u32,
        };
        for d in eye.detect(&band, params) {
            hits.push((to_uv(d.cx(), d.cy()), d.w / EYE_SIDE as f32));
        }
    }
    let pick = |prior: [f32; 2], left: bool| -> Option<[f32; 2]> {
        hits.iter()
            .filter(|(c, w)| {
                (if left { c[0] < 0.5 } else { c[0] >= 0.5 })
                    && (0.2..=0.56).contains(&c[1])
                    && (0.1..=0.42).contains(w)
                    && (c[0] - prior[0]).abs() < 0.17
            })
            .min_by(|a, b| {
                let da = (a.0[0] - prior[0]).hypot(a.0[1] - prior[1]);
                let db = (b.0[0] - prior[0]).hypot(b.0[1] - prior[1]);
                da.total_cmp(&db)
            })
            .map(|(c, _)| *c)
    };
    let mut measured = 0;
    let mut hit_count = 0;
    let mut eyes = [LEFT_EYE, RIGHT_EYE];
    for (slot, (prior, left)) in eyes.iter_mut().zip([(LEFT_EYE, true), (RIGHT_EYE, false)]) {
        let start = pick(prior, left);
        hit_count += u8::from(start.is_some());
        let around = start.unwrap_or(prior);
        if let Some(refined) = darkest_blob(&band, around, start.is_some()) {
            *slot = refined;
            measured += 1;
        } else if let Some(hit) = start {
            *slot = hit;
            measured += 1;
        }
    }
    // A single measured eye places the other by symmetry about the box's centre line, which is
    // better than the prior when the head is turned.
    if measured == 1 {
        let [l, r] = eyes;
        let found_left = (l[0] - LEFT_EYE[0]).abs() + (l[1] - LEFT_EYE[1]).abs()
            > (r[0] - RIGHT_EYE[0]).abs() + (r[1] - RIGHT_EYE[1]).abs();
        if found_left {
            eyes[1] = [l[0] + (RIGHT_EYE[0] - LEFT_EYE[0]), l[1]];
        } else {
            eyes[0] = [r[0] - (RIGHT_EYE[0] - LEFT_EYE[0]), r[1]];
        }
    }
    let [l, r] = eyes;
    (
        face.at(l[0], l[1]),
        face.at(r[0], r[1]),
        measured,
        hit_count,
    )
}

/// The darkest compact blob near an expected eye position, in box coordinates.
///
/// `None` when nothing in the window is darker than its surroundings by a margin - an eye
/// that cannot be told from the skin around it is not measured, it is placed.
fn darkest_blob(band: &GreyImage, around: [f32; 2], trusted: bool) -> Option<[f32; 2]> {
    let w = band.width as f32;
    let h = band.height as f32;
    let span_u = EYE_BAND[2] - EYE_BAND[0];
    let span_v = EYE_BAND[3] - EYE_BAND[1];
    let cx = (around[0] - EYE_BAND[0]) / span_u * w;
    let cy = (around[1] - EYE_BAND[1]) / span_v * h;
    let rx = 0.09 / span_u * w;
    let ry = 0.07 / span_v * h;
    let x0 = (cx - rx).floor().max(0.0) as u32;
    let x1 = ((cx + rx).ceil() as u32).min(band.width.saturating_sub(1));
    let y0 = (cy - ry).floor().max(0.0) as u32;
    let y1 = ((cy + ry).ceil() as u32).min(band.height.saturating_sub(1));
    if x1 <= x0 + 2 || y1 <= y0 + 2 {
        return None;
    }
    // A 3x3 mean so a single dark pixel of noise is not an iris.
    let smooth = |x: u32, y: u32| -> f32 {
        let mut s = 0.0;
        for dy in -1_i64..=1 {
            for dx in -1_i64..=1 {
                s += band.value(i64::from(x) + dx, i64::from(y) + dy);
            }
        }
        s / 9.0
    };
    let mut values = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            values.push((smooth(x, y), x, y));
        }
    }
    let mut sorted: Vec<f32> = values.iter().map(|v| v.0).collect();
    sorted.sort_by(f32::total_cmp);
    let darkest = sorted.first().copied()?;
    let median = sorted.get(sorted.len() / 2).copied()?;
    // The margin is relative to how bright the window is. An iris against the darkest skin is
    // a few grey levels darker than the lid around it, not fourteen, and a fixed margin is a
    // threshold that fails exactly the faces the equalised pass exists for.
    let margin = if trusted {
        5.0
    } else {
        (6.0 + 0.1 * median).min(14.0)
    };
    if median - darkest < margin {
        return None;
    }
    let cut = darkest + (median - darkest) * 0.3;
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut total = 0.0;
    for (v, x, y) in &values {
        if *v <= cut {
            let weight = cut - v + 1.0;
            sx += weight * (*x as f32 + 0.5);
            sy += weight * (*y as f32 + 0.5);
            total += weight;
        }
    }
    if total <= 0.0 {
        return None;
    }
    Some([
        EYE_BAND[0] + span_u * (sx / total) / w,
        EYE_BAND[1] + span_v * (sy / total) / h,
    ])
}

/// Find the mouth below the eyes: Hsu, Abdel-Mottaleb and Jain's mouth map on a colour
/// photograph, the darkest horizontal line on a grey one.
fn locate_mouth(canvas: &Canvas, face: &FaceGeometry, colourful: bool) -> ([f32; 2], f32, bool) {
    let iod = face.interocular();
    let mid = face.eye_mid();
    let (s, c) = face.roll.sin_cos();
    // Down the face is perpendicular to the eye line.
    let down = [-s, c];
    let across = [c, s];
    let expected = [mid[0] + down[0] * iod * 1.08, mid[1] + down[1] * iod * 1.08];
    let prior_width = iod * 0.88;

    // Sample a window around the expected mouth in the face's own frame.
    let cols = 40_usize;
    let rows = 28_usize;
    let half_w = iod * 0.75;
    let half_h = iod * 0.42;
    let mut cells = Vec::with_capacity(cols * rows);
    for j in 0..rows {
        let t = -1.0 + 2.0 * (j as f32 + 0.5) / rows as f32;
        for i in 0..cols {
            let r = -1.0 + 2.0 * (i as f32 + 0.5) / cols as f32;
            let p = [
                expected[0] + across[0] * r * half_w + down[0] * t * half_h,
                expected[1] + across[1] * r * half_w + down[1] * t * half_h,
            ];
            let index = canvas.index(p[0] as i64, p[1] as i64);
            cells.push((r, t, index));
        }
    }

    let score: Vec<f32> = if colourful {
        let maps: Vec<(f32, f32)> = cells
            .iter()
            .map(|(_, _, index)| {
                let ycc = canvas.ycbcr(*index);
                let cr = ycc[2] / 255.0;
                let cb = (ycc[1] / 255.0).max(0.05);
                (cr * cr, cr / cb)
            })
            .collect();
        let n = maps.len().max(1) as f32;
        let mean_sq: f32 = maps.iter().map(|m| m.0).sum::<f32>() / n;
        let mean_ratio: f32 = maps.iter().map(|m| m.1).sum::<f32>() / n;
        let eta = 0.95 * mean_sq / mean_ratio.max(1e-4);
        maps.iter()
            .map(|(sq, ratio)| sq * (sq - eta * ratio).powi(2))
            .collect()
    } else {
        // Darkness, so the line between the lips scores highest.
        cells
            .iter()
            .map(|(_, _, index)| 255.0 - f32::from(canvas.grey.get(*index).copied().unwrap_or(0)))
            .collect()
    };
    let max = score.iter().copied().fold(0.0_f32, f32::max);
    let mean = score.iter().sum::<f32>() / score.len().max(1) as f32;
    if max <= 1e-9 || max < mean * 1.8 {
        return (expected, prior_width, false);
    }
    let cut = mean + (max - mean) * 0.45;
    let mut sr = 0.0;
    let mut st = 0.0;
    let mut total = 0.0;
    let mut spread: Vec<f32> = Vec::new();
    for ((r, t, _), v) in cells.iter().zip(score.iter()) {
        if *v >= cut {
            // Distance from the expected position counts against a candidate, so a red scarf
            // at the edge of the window does not pull the mouth sideways.
            let weight = (v - cut) * (1.0 - 0.5 * r.abs()) * (1.0 - 0.4 * t.abs());
            sr += weight * r;
            st += weight * t;
            total += weight;
            spread.push(*r);
        }
    }
    if total <= 1e-9 || spread.len() < 4 {
        return (expected, prior_width, false);
    }
    let r = sr / total;
    let t = st / total;
    spread.sort_by(f32::total_cmp);
    let lo = spread.get(spread.len() / 20).copied().unwrap_or(-0.5);
    let hi = spread
        .get(spread.len() - 1 - spread.len() / 20)
        .copied()
        .unwrap_or(0.5);
    let width = ((hi - lo) * half_w).clamp(iod * 0.55, iod * 1.3);
    let centre = [
        expected[0] + across[0] * r * half_w + down[0] * t * half_h,
        expected[1] + across[1] * r * half_w + down[1] * t * half_h,
    ];
    (centre, if colourful { width } else { prior_width }, true)
}

/// A grey image turned by `angle` radians about its centre, edges replicated.
#[must_use]
pub fn rotate(image: &GreyImage, angle: f32) -> GreyImage {
    let (s, c) = angle.sin_cos();
    let cx = image.width as f32 * 0.5;
    let cy = image.height as f32 * 0.5;
    let mut pixels = Vec::with_capacity(image.pixels.len());
    for y in 0..image.height {
        for x in 0..image.width {
            // Output pixel p' samples input p = c + R(-angle)(p' - c).
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let sx = cx + dx * c + dy * s - 0.5;
            let sy = cy - dx * s + dy * c - 0.5;
            let x0 = sx.floor();
            let y0 = sy.floor();
            let tx = sx - x0;
            let ty = sy - y0;
            let (ix, iy) = (x0 as i64, y0 as i64);
            let top = image.value(ix, iy) * (1.0 - tx) + image.value(ix + 1, iy) * tx;
            let bottom = image.value(ix, iy + 1) * (1.0 - tx) + image.value(ix + 1, iy + 1) * tx;
            pixels.push((top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8);
        }
    }
    GreyImage {
        width: image.width,
        height: image.height,
        pixels,
    }
}

/// Contrast-limited adaptive histogram equalisation over a `tiles x tiles` grid.
#[must_use]
pub fn clahe(image: &GreyImage, tiles: u32, clip: f32) -> GreyImage {
    let tiles = tiles.max(1);
    let tw = (image.width as f32 / tiles as f32).max(1.0);
    let th = (image.height as f32 / tiles as f32).max(1.0);
    let mut maps = vec![[0_u8; 256]; (tiles * tiles) as usize];
    for ty in 0..tiles {
        for tx in 0..tiles {
            let x0 = (tx as f32 * tw) as u32;
            let x1 = (((tx + 1) as f32 * tw) as u32).min(image.width);
            let y0 = (ty as f32 * th) as u32;
            let y1 = (((ty + 1) as f32 * th) as u32).min(image.height);
            let mut histogram = [0_f32; 256];
            let mut count = 0.0_f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let v = image
                        .pixels
                        .get((y * image.width + x) as usize)
                        .copied()
                        .unwrap_or(0);
                    if let Some(slot) = histogram.get_mut(usize::from(v)) {
                        *slot += 1.0;
                    }
                    count += 1.0;
                }
            }
            if count <= 0.0 {
                continue;
            }
            let limit = (clip * count / 256.0).max(1.0);
            let mut excess = 0.0;
            for slot in &mut histogram {
                if *slot > limit {
                    excess += *slot - limit;
                    *slot = limit;
                }
            }
            let bonus = excess / 256.0;
            let mut cumulative = 0.0;
            if let Some(map) = maps.get_mut((ty * tiles + tx) as usize) {
                for (slot, h) in map.iter_mut().zip(histogram.iter()) {
                    cumulative += h + bonus;
                    *slot = (255.0 * cumulative / count).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    let lookup = |tx: i64, ty: i64, v: u8| -> f32 {
        let tx = tx.clamp(0, i64::from(tiles) - 1) as u32;
        let ty = ty.clamp(0, i64::from(tiles) - 1) as u32;
        f32::from(
            maps.get((ty * tiles + tx) as usize)
                .and_then(|m| m.get(usize::from(v)))
                .copied()
                .unwrap_or(v),
        )
    };
    let mut pixels = Vec::with_capacity(image.pixels.len());
    for y in 0..image.height {
        let fy = (y as f32 + 0.5) / th - 0.5;
        let ty0 = fy.floor();
        let wy = fy - ty0;
        for x in 0..image.width {
            let fx = (x as f32 + 0.5) / tw - 0.5;
            let tx0 = fx.floor();
            let wx = fx - tx0;
            let v = image
                .pixels
                .get((y * image.width + x) as usize)
                .copied()
                .unwrap_or(0);
            let (ix, iy) = (tx0 as i64, ty0 as i64);
            let top = lookup(ix, iy, v) * (1.0 - wx) + lookup(ix + 1, iy, v) * wx;
            let bottom = lookup(ix, iy + 1, v) * (1.0 - wx) + lookup(ix + 1, iy + 1, v) * wx;
            pixels.push((top * (1.0 - wy) + bottom * wy).round().clamp(0.0, 255.0) as u8);
        }
    }
    GreyImage {
        width: image.width,
        height: image.height,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hint_round_trips_through_its_target_form() {
        let hint = FaceHint {
            x: 0.25,
            y: 0.125,
            w: 0.2,
            h: 0.3,
        };
        let parsed = FaceHint::parse(&hint.to_target()).unwrap();
        assert!((parsed.x - 0.25).abs() < 1e-4 && (parsed.h - 0.3).abs() < 1e-4);
        assert!(FaceHint::parse("hint:a,b,c,d").is_none());
        assert!(FaceHint::parse("eyes").is_none());
        assert!(FaceHint::parse("hint:0.1,0.1,0,0.2").is_none());
    }

    #[test]
    fn box_coordinates_follow_the_roll() {
        let face = FaceGeometry {
            center: [100.0, 100.0],
            size: 40.0,
            roll: std::f32::consts::FRAC_PI_2,
            left_eye: [0.0; 2],
            right_eye: [0.0; 2],
            nose: [0.0; 2],
            mouth: [0.0; 2],
            mouth_width: 0.0,
            eyes_measured: 0,
            eye_hits: 0,
            mouth_measured: false,
            confidence: 0.0,
            neighbours: 0,
            source: FaceSource::Frontal,
            skin_fraction: 0.0,
        };
        // Turned a quarter clockwise, "right" in the face's frame is "down" on the canvas.
        let p = face.at(1.0, 0.5);
        assert!((p[0] - 100.0).abs() < 1e-3 && (p[1] - 120.0).abs() < 1e-3);
    }

    #[test]
    fn rotating_by_zero_is_the_identity_and_equalising_spreads_the_histogram() {
        let pixels: Vec<u8> = (0..64 * 48).map(|i| (100 + (i % 7) * 3) as u8).collect();
        let image = GreyImage::new(64, 48, pixels).unwrap();
        assert_eq!(rotate(&image, 0.0), image);
        let eq = clahe(&image, 4, 4.0);
        let (lo, hi) = eq
            .pixels
            .iter()
            .fold((255_u8, 0_u8), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(hi - lo > 18, "{lo}..{hi}");
    }

    #[test]
    fn an_empty_canvas_has_no_faces() {
        let canvas = Canvas::from_srgb8(&[90; 3 * 64 * 64], 64, 64, 64).unwrap();
        assert!(detect(&canvas, &[]).is_empty());
    }

    #[test]
    fn a_hint_is_always_a_face() {
        let canvas = Canvas::from_srgb8(&[90; 3 * 64 * 64], 64, 64, 64).unwrap();
        let faces = detect(
            &canvas,
            &[FaceHint {
                x: 0.25,
                y: 0.25,
                w: 0.5,
                h: 0.5,
            }],
        );
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].source, FaceSource::Hint);
        assert!((faces[0].confidence - 1.0).abs() < 1e-6);
    }
}
