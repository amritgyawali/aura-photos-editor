//! Synthetic faces, a reference detector that finds them, and synthetic identity
//! templates.
//!
//! Phase 06 has a measurement problem. Section 10.1 states its gates as numbers -
//! detection recall at or above 0.97, small-face recall at or above 0.90, false
//! positives on bokeh below one per cent, clustering F1 at or above 0.93 - and
//! there is no labelled wedding data in this repository and no trained model to
//! measure. Shipping the gates as prose that nobody runs would be worse than not
//! writing them down.
//!
//! So the gates are measured, on ground truth this module generates. Three pieces:
//!
//! * [`render_frame`] draws faces at known positions with known identities, plus
//!   bokeh highlights that are deliberately face-like in colour and deliberately
//!   structureless.
//! * [`ReferenceDetector`] finds them. It is a real detector - a colour-keyed blob
//!   pass followed by a landmark search inside each blob - and it is **a detector
//!   for this fixture format, not for photographs**. Its job is to exercise
//!   `crate::face::detect`'s decode, suppression, tiling and quality path with
//!   ground truth attached, so that when a trained SCRFD arrives the harness that
//!   measures it already exists and has been run.
//! * [`synthetic_template`] produces recognition templates with controlled
//!   intra-identity spread and inter-identity separation, including the *lookalike
//!   relative* case that the rank-order verification in `crate::face::cluster` is
//!   built to survive.
//!
//! What this module cannot do is tell you whether the shipped `face_detect` model
//! works. It does not. That is condition C1 in
//! `docs/progress/PHASE-06-EXIT.md`, and the honest position is that everything
//! around the model is measured and the model is not.

use crate::face::align::{Landmarks, LANDMARK_COUNT};
use crate::face::detect::{Detection, FoundBy, NormBox};

/// One synthetic face to be drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthFace {
    /// Centre, normalised to the frame.
    pub centre: [f32; 2],
    /// Height as a fraction of the frame. Width is 0.78 of this - a face is taller
    /// than it is wide, and a square ground truth box would flatter any detector
    /// measured against it.
    pub height: f32,
    /// Which identity this is. Ground truth for the clustering metrics.
    pub identity: u32,
    /// Yaw, in degrees, applied by shifting the landmarks rather than by rotating
    /// the drawing - which is enough to exercise the pose estimate and the quality
    /// gate.
    pub yaw: f32,
    /// True when a torso is drawn below the face.
    pub body: bool,
    /// True when the eyes and mouth are omitted. A face with no structure: what a
    /// hand over a face or a very soft background face looks like to a detector.
    pub featureless: bool,
}

impl SynthFace {
    /// A frontal face with a body.
    #[must_use]
    pub const fn new(centre: [f32; 2], height: f32, identity: u32) -> Self {
        Self {
            centre,
            height,
            identity,
            yaw: 0.0,
            body: true,
            featureless: false,
        }
    }

    /// The same face turned away.
    #[must_use]
    pub const fn turned(mut self, yaw: f32) -> Self {
        self.yaw = yaw;
        self
    }

    /// A bokeh highlight: face-coloured, round, and empty.
    #[must_use]
    pub const fn bokeh(centre: [f32; 2], height: f32) -> Self {
        Self {
            centre,
            height,
            identity: u32::MAX,
            yaw: 0.0,
            body: false,
            featureless: true,
        }
    }

    /// True when this is a highlight rather than a face.
    #[must_use]
    pub const fn is_bokeh(&self) -> bool {
        self.identity == u32::MAX
    }

    /// The ground truth box, normalised.
    #[must_use]
    pub fn truth_box(&self) -> NormBox {
        let width = self.height * FACE_ASPECT;
        NormBox::from_corners(
            self.centre[0] - width * 0.5,
            self.centre[1] - self.height * 0.5,
            self.centre[0] + width * 0.5,
            self.centre[1] + self.height * 0.5,
        )
    }
}

/// Face width over face height.
pub const FACE_ASPECT: f32 = 0.78;

/// The skin tone the renderer draws and the reference detector keys on.
///
/// A single tone, and that is a limitation worth naming: section 12's second
/// failure mode is "recognition accuracy varies across skin tones", and a fixture
/// set with one tone cannot measure that. The demographic analysis the model cards
/// require needs real consented data, and it is listed as condition C5 in the exit
/// report rather than approximated here. Approximating it would be worse than
/// admitting it: a fairness number computed from synthetic tones would be quoted.
pub const SKIN: [u8; 3] = [206, 168, 140];

/// The dark tone used for eyes and mouth.
pub const FEATURE: [u8; 3] = [46, 38, 34];

/// Clothing tone for the torso.
const CLOTH: [u8; 3] = [64, 70, 96];

/// Draw a frame containing synthetic faces.
///
/// The background is a smooth two-axis gradient with a fine checker texture on top.
/// The texture matters: a perfectly smooth background would make the occlusion
/// estimate in `crate::face::quality` see the whole frame as featureless, and the
/// gate would pass or fail for the wrong reason.
#[must_use]
pub fn render_frame(width: usize, height: usize, faces: &[SynthFace]) -> Vec<u8> {
    let mut rgb = vec![0u8; width * height * 3];

    for y in 0..height {
        for x in 0..width {
            let across = x as f32 / width.max(1) as f32;
            let down = y as f32 / height.max(1) as f32;
            // Gradient plus a two-pixel checker, so local variance is never zero.
            let checker = if (x / 2 + y / 2) % 2 == 0 { 6.0 } else { 0.0 };
            // Deliberately blue-dominant and dark. `SKIN` is red-dominant and bright, and
            // the colour key in `ReferenceDetector` has a tolerance of 46 per channel - so
            // a warm background gradient would key as skin somewhere in its range and the
            // fixture would generate its own false positives. The worst case here is
            // (90, 94, 124) at the bottom-right corner, which is 206 away from `SKIN` in
            // the same L1 metric the key uses, against a threshold of 138.
            let base = 24.0 + across * 40.0 + down * 26.0 + checker;
            for (channel, tint) in [0.0f32, 4.0, 30.0].into_iter().enumerate() {
                if let Some(slot) = rgb.get_mut((y * width + x) * 3 + channel) {
                    *slot = (base + tint).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    for face in faces {
        draw_face(&mut rgb, width, height, face);
    }
    rgb
}

/// Draw one face, and its body when it has one.
fn draw_face(rgb: &mut [u8], width: usize, height: usize, face: &SynthFace) {
    let box_norm = face.truth_box();
    // `i32` throughout: a drawing routine legitimately computes positions off the
    // top-left of the frame, and `put` is where that is checked.
    let left = (box_norm.x * width as f32) as i32;
    let top = (box_norm.y * height as f32) as i32;
    let box_w = (box_norm.w * width as f32).max(2.0);
    let box_h = (box_norm.h * height as f32).max(2.0);

    // The torso first, so the face draws over it.
    if face.body {
        let torso_top = top + box_h as i32;
        let torso_h = (box_h * 2.2) as i32;
        let torso_w = (box_w * 1.9) as i32;
        let torso_left = left + (box_w as i32 - torso_w) / 2;
        for y in torso_top..(torso_top + torso_h) {
            for x in torso_left..(torso_left + torso_w) {
                put(rgb, width, height, x, y, CLOTH);
            }
        }
    }

    // The face as a filled ellipse.
    let cx = left as f32 + box_w * 0.5;
    let cy = top as f32 + box_h * 0.5;
    let rx = box_w * 0.5;
    let ry = box_h * 0.5;
    let y_start = top.max(0);
    let y_end = (top + box_h as i32).min(i32::try_from(height).unwrap_or(i32::MAX));
    for y in y_start..y_end {
        for x in left.max(0)..(left + box_w as i32).min(i32::try_from(width).unwrap_or(i32::MAX)) {
            let dx = (x as f32 - cx) / rx.max(1.0);
            let dy = (y as f32 - cy) / ry.max(1.0);
            if dx * dx + dy * dy <= 1.0 {
                // A gentle vertical shade, plus a fine two-pixel texture.
                //
                // The texture is not decoration. Real skin has pores, shadow gradients
                // and specular falloff, and `crate::face::quality` measures exactly that:
                // a Laplacian variance for sharpness and a per-block standard deviation
                // for occlusion. A perfectly flat disc reads as a completely soft,
                // completely occluded face - so a fixture without texture would put every
                // synthetic face below the identity gate and no grouping test could run.
                //
                // Eight levels of amplitude: about 0.03 in luminance units, which is above
                // the 0.02 flatness floor and gives a Laplacian variance well past the
                // half-sharp point.
                let shade = 1.0 - 0.12 * dy;
                let texture = if (x / 2 + y / 2) % 2 == 0 { 8.0 } else { -8.0 };
                let tinted = [
                    (f32::from(SKIN[0]) * shade + texture).clamp(0.0, 255.0) as u8,
                    (f32::from(SKIN[1]) * shade + texture).clamp(0.0, 255.0) as u8,
                    (f32::from(SKIN[2]) * shade + texture).clamp(0.0, 255.0) as u8,
                ];
                put(rgb, width, height, x, y, tinted);
            }
        }
    }

    if face.featureless {
        return;
    }

    // Eyes, nose and mouth, at the ArcFace reference proportions so the pose
    // estimate has something faithful to measure. The yaw shifts them
    // horizontally, which is exactly the projection the estimate inverts.
    let shift = (face.yaw / 90.0).clamp(-1.0, 1.0) * box_w * 0.28;
    let feature_radius = (box_w * 0.075).max(1.0);
    let points = reference_points(left as f32, top as f32, box_w, box_h, shift);

    for (index, point) in points.iter().enumerate() {
        // The nose is drawn smaller: it is a highlight, not a dark feature, and
        // drawing it the same size as an eye would make the landmark search inside
        // the reference detector find three eyes.
        let radius = if index == crate::face::align::NOSE {
            feature_radius * 0.5
        } else {
            feature_radius
        };
        let colour = if index == crate::face::align::NOSE {
            [
                SKIN[0].saturating_sub(40),
                SKIN[1].saturating_sub(40),
                SKIN[2].saturating_sub(40),
            ]
        } else {
            FEATURE
        };
        let px = point[0];
        let py = point[1];
        let span = radius.ceil() as i32;
        for y in -span..=span {
            for x in -span..=span {
                if f64::from(x * x + y * y) <= f64::from(radius * radius) {
                    put(rgb, width, height, px as i32 + x, py as i32 + y, colour);
                }
            }
        }
    }

    // The mouth line between the two corners, so the mouth is a feature and not two
    // dots.
    if let (Some(left_mouth), Some(right_mouth)) = (
        points.get(crate::face::align::LEFT_MOUTH),
        points.get(crate::face::align::RIGHT_MOUTH),
    ) {
        let steps = (right_mouth[0] - left_mouth[0]).abs().max(1.0) as isize;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let x = left_mouth[0] + (right_mouth[0] - left_mouth[0]) * t;
            let y = left_mouth[1] + (right_mouth[1] - left_mouth[1]) * t;
            for thickness in -1..=1 {
                put(rgb, width, height, x as i32, y as i32 + thickness, FEATURE);
            }
        }
    }
}

/// The five landmark positions inside a face box, in frame pixels.
fn reference_points(left: f32, top: f32, box_w: f32, box_h: f32, shift: f32) -> [[f32; 2]; 5] {
    // The ArcFace reference layout, expressed as fractions of its 112 px crop, so
    // the drawing and `align::ARCFACE_REFERENCE` agree by construction.
    let fractions = crate::face::align::ARCFACE_REFERENCE;
    let mut out = [[0.0f32; 2]; LANDMARK_COUNT];
    for (slot, point) in out.iter_mut().zip(fractions.iter()) {
        *slot = [
            left + (point[0] / 112.0) * box_w + shift,
            top + (point[1] / 112.0) * box_h,
        ];
    }
    out
}

/// Write one pixel, ignoring anything outside the frame.
///
/// Signed coordinates, because a drawing routine legitimately computes positions that
/// fall off the top-left of the frame and the check for that is the point of this
/// function. `i32` rather than `isize` so the cast from a loop counter cannot wrap.
fn put(rgb: &mut [u8], width: usize, height: usize, x: i32, y: i32, colour: [u8; 3]) {
    if x < 0 || y < 0 {
        return;
    }
    let (x, y) = (x as usize, y as usize);
    if x >= width || y >= height {
        return;
    }
    for (channel, value) in colour.into_iter().enumerate() {
        if let Some(slot) = rgb.get_mut((y * width + x) * 3 + channel) {
            *slot = value;
        }
    }
}

/// A deterministic detector for the fixture format.
///
/// Two passes, and both are honest detection rather than a lookup of the ground
/// truth it was drawn from - the difference matters, because a "detector" that read
/// the fixture list would measure nothing.
///
/// 1. **Colour key and connected components.** Pixels within [`SKIN_TOLERANCE`] of
///    [`SKIN`] are marked, then flood-filled into components. Each component's
///    bounding box is a candidate.
/// 2. **Landmark search inside the box.** The darkest pixel in each of the five
///    reference quadrants becomes that landmark. A component with no dark pixels -
///    a bokeh highlight - yields five landmarks collapsed at its centre, a landmark
///    spread near zero, and rejection by `crate::face::detect::nms`. That is the
///    bokeh false-positive gate, measured rather than asserted.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReferenceDetector {
    /// Minimum component height in pixels. Smaller components are noise.
    pub min_height: usize,
}

/// How far from [`SKIN`] a pixel may be and still key.
pub const SKIN_TOLERANCE: i32 = 46;

impl ReferenceDetector {
    /// A detector that ignores components shorter than four pixels.
    #[must_use]
    pub const fn new() -> Self {
        Self { min_height: 4 }
    }

    /// Find every face-like component in a frame.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn detect(&self, rgb: &[u8], width: usize, height: usize) -> Vec<Detection> {
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return Vec::new();
        }

        let keyed: Vec<bool> = (0..width * height)
            .map(|index| {
                let read =
                    |channel: usize| i32::from(rgb.get(index * 3 + channel).copied().unwrap_or(0));
                let distance = (read(0) - i32::from(SKIN[0])).abs()
                    + (read(1) - i32::from(SKIN[1])).abs()
                    + (read(2) - i32::from(SKIN[2])).abs();
                distance <= SKIN_TOLERANCE * 3
            })
            .collect();

        let mut seen = vec![false; width * height];
        let mut out = Vec::new();

        for start in 0..width * height {
            if !keyed.get(start).copied().unwrap_or(false)
                || seen.get(start).copied().unwrap_or(true)
            {
                continue;
            }

            // Iterative flood fill. Iterative rather than recursive because a
            // component can be a megapixel and a recursive fill would overflow the
            // stack on exactly the frame that matters most.
            let mut stack = vec![start];
            let mut members: Vec<usize> = Vec::new();
            if let Some(flag) = seen.get_mut(start) {
                *flag = true;
            }
            while let Some(index) = stack.pop() {
                members.push(index);
                let x = index % width;
                let y = index / width;
                let push = |nx: usize, ny: usize, stack: &mut Vec<usize>, seen: &mut Vec<bool>| {
                    let neighbour = ny * width + nx;
                    if keyed.get(neighbour).copied().unwrap_or(false)
                        && !seen.get(neighbour).copied().unwrap_or(true)
                    {
                        if let Some(flag) = seen.get_mut(neighbour) {
                            *flag = true;
                        }
                        stack.push(neighbour);
                    }
                };
                if x > 0 {
                    push(x - 1, y, &mut stack, &mut seen);
                }
                if x + 1 < width {
                    push(x + 1, y, &mut stack, &mut seen);
                }
                if y > 0 {
                    push(x, y - 1, &mut stack, &mut seen);
                }
                if y + 1 < height {
                    push(x, y + 1, &mut stack, &mut seen);
                }
            }

            let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0usize, 0usize);
            for index in &members {
                let x = index % width;
                let y = index / width;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            let box_h = max_y.saturating_sub(min_y) + 1;
            let box_w = max_x.saturating_sub(min_x) + 1;
            if box_h < self.min_height {
                continue;
            }

            // Fill ratio against an ellipse of the same box. A face is an ellipse
            // with features cut out of it, so a real face sits near 0.7; a rectangle
            // of background that happened to key would sit near 1.0 and a thin
            // sliver near 0.2. Used as the objectness, which is what a real
            // detector's classification head would supply.
            let ellipse_area = std::f32::consts::PI * 0.25 * box_w as f32 * box_h as f32;
            let fill = if ellipse_area <= 0.0 {
                0.0
            } else {
                (members.len() as f32 / ellipse_area).clamp(0.0, 1.0)
            };
            let face_score = (0.35 + 0.6 * fill).clamp(0.0, 0.99);

            let face = NormBox::from_corners(
                min_x as f32 / width as f32,
                min_y as f32 / height as f32,
                (max_x + 1) as f32 / width as f32,
                (max_y + 1) as f32 / height as f32,
            );
            let landmarks = find_landmarks(rgb, width, height, min_x, min_y, box_w, box_h);

            // A body box, if there is clothing below. Same colour-key idea, one
            // sample line, because the fixture's torso is a solid rectangle.
            let person = find_body(rgb, width, height, min_x, max_y, box_w, box_h);
            let person_score = if person.is_some() { 0.80 } else { 0.0 };

            out.push(Detection {
                face,
                person,
                face_score,
                person_score,
                landmarks: to_normalised(&landmarks, width, height),
                stride: 8,
                found_by: FoundBy::Full,
            });
        }

        out
    }
}

/// Channel-sum contrast a candidate must have against the local skin tone before it
/// counts as a facial feature.
///
/// Two hundred and fifty, out of a maximum of 765, and the number decides whether the
/// bokeh gate works at all. The arithmetic, because guessing it wrong is what makes a
/// blurred highlight read as a face:
///
/// * **A drawn eye or mouth** is [`FEATURE`], channel sum 118. The brightest skin inside
///   an ellipse is 600 - the base tone at the top of the vertical shade, plus the
///   texture - so the contrast is 482, nearly twice the threshold.
/// * **A bokeh highlight** is skin-coloured with the same shade and texture as a face and
///   no features at all. Its own internal range is 600 at the top down to 428 at the
///   bottom: a contrast of 172, comfortably under the threshold.
///
/// The margin between 172 and 482 is where this constant has to sit, and 250 is in the
/// middle of it. Set it at 150 and the *darkest shaded pixel of a highlight* qualifies as
/// a feature, five landmarks land in five different places, and a structureless blob
/// produces the landmark spread of a real face.
const MIN_FEATURE_CONTRAST: i32 = 250;

/// The darkest pixel in each of the five reference windows, when there is one.
///
/// Two constraints on a candidate, and both are load-bearing:
///
/// 1. **Inside the inscribed ellipse.** A search window around an eye extends past the
///    box's corner into the background, which is darker than any facial feature - so
///    without this the landmarks would snap to the frame's background and a blob would
///    read as a wide-spread face.
/// 2. **Darker than the local skin tone by [`MIN_FEATURE_CONTRAST`].** See that constant.
///
/// A window with no qualifying candidate collapses to the box centre, which is what
/// makes the landmark spread near zero and lets `detect::nms` refuse the detection.
fn find_landmarks(
    rgb: &[u8],
    width: usize,
    height: usize,
    left: usize,
    top: usize,
    box_w: usize,
    box_h: usize,
) -> Landmarks {
    let windows = crate::face::align::ARCFACE_REFERENCE;
    // The search windows must not overlap, and the reference geometry fixes how big they
    // may be. The eyes are 0.314 box widths apart, so a horizontal radius past 0.157 lets
    // the nose window reach an eye - and an eye is far darker than a nose, so the nose
    // landmark snaps to it and the pose estimate reads a frontal face as a full profile.
    // The eye, nose and mouth rows are 0.18 box heights apart, so the vertical radius has
    // the same constraint at half of that.
    let radius_x = (box_w as f32 * 0.12).max(1.0);
    let radius_y = (box_h as f32 * 0.07).max(1.0);

    let centre = [
        left as f32 + box_w as f32 * 0.5,
        top as f32 + box_h as f32 * 0.5,
    ];
    let semi = [(box_w as f32 * 0.5).max(1.0), (box_h as f32 * 0.5).max(1.0)];
    // Two ellipses, and the second one matters. The component's bounding box is one
    // pixel wider than the drawn ellipse in places, so its inscribed ellipse clips a ring
    // of background - and background is darker than any facial feature, so a landmark
    // search that could reach it would snap five points to five different edge pixels and
    // give a structureless blob the landmark spread of a real face. The search is
    // therefore constrained to 92 % of the semi-axes; the skin reference is measured over
    // the full one.
    let inside = |x: f32, y: f32| -> bool {
        let dx = (x - centre[0]) / semi[0];
        let dy = (y - centre[1]) / semi[1];
        dx * dx + dy * dy <= 1.0
    };
    let well_inside = |x: f32, y: f32| -> bool {
        let dx = (x - centre[0]) / (semi[0] * 0.92);
        let dy = (y - centre[1]) / (semi[1] * 0.92);
        dx * dx + dy * dy <= 1.0
    };
    let channel_sum = |x: usize, y: usize| -> i32 {
        let index = (y * width + x) * 3;
        i32::from(rgb.get(index).copied().unwrap_or(255))
            + i32::from(rgb.get(index + 1).copied().unwrap_or(255))
            + i32::from(rgb.get(index + 2).copied().unwrap_or(255))
    };

    // The local skin reference: the brightest tone inside the ellipse, which for a face
    // is the unshaded cheek and for a highlight is its own centre. Taking the maximum
    // rather than the mean means a face that is half in shadow still has a reference the
    // features are clearly darker than.
    let mut skin_reference = 0i32;
    let y_start = top;
    let y_end = (top + box_h).min(height);
    for y in y_start..y_end {
        for x in left..(left + box_w).min(width) {
            if inside(x as f32, y as f32) {
                skin_reference = skin_reference.max(channel_sum(x, y));
            }
        }
    }

    let mut out = [[0.0f32; 2]; LANDMARK_COUNT];
    for (slot, reference) in out.iter_mut().zip(windows.iter()) {
        let window_x = left as f32 + (reference[0] / 112.0) * box_w as f32;
        let window_y = top as f32 + (reference[1] / 112.0) * box_h as f32;
        let mut best: Option<(f32, f32, i32)> = None;

        let x_start = (window_x - radius_x).max(0.0) as usize;
        let x_end = ((window_x + radius_x) as usize).min(width.saturating_sub(1));
        let y_start = (window_y - radius_y).max(0.0) as usize;
        let y_end = ((window_y + radius_y) as usize).min(height.saturating_sub(1));

        for y in y_start..=y_end.max(y_start) {
            for x in x_start..=x_end.max(x_start) {
                if !well_inside(x as f32, y as f32) {
                    continue;
                }
                let sum = channel_sum(x, y);
                if sum > skin_reference - MIN_FEATURE_CONTRAST {
                    continue;
                }
                if best.is_none_or(|(_, _, previous)| sum < previous) {
                    best = Some((x as f32, y as f32, sum));
                }
            }
        }

        *slot = match best {
            Some((x, y, _)) => [x, y],
            None => centre,
        };
    }
    out
}

/// A body box below a face box, if clothing is there.
fn find_body(
    rgb: &[u8],
    width: usize,
    height: usize,
    left: usize,
    face_bottom: usize,
    box_w: usize,
    box_h: usize,
) -> Option<NormBox> {
    let probe_y = face_bottom + box_h / 4;
    if probe_y >= height {
        return None;
    }
    let probe_x = left + box_w / 2;
    if probe_x >= width {
        return None;
    }
    let index = (probe_y * width + probe_x) * 3;
    let read = |channel: usize| i32::from(rgb.get(index + channel).copied().unwrap_or(0));
    let distance = (read(0) - i32::from(CLOTH[0])).abs()
        + (read(1) - i32::from(CLOTH[1])).abs()
        + (read(2) - i32::from(CLOTH[2])).abs();
    if distance > 60 {
        return None;
    }

    let body_w = (box_w as f32 * 1.9) as usize;
    let body_h = (box_h as f32 * 2.2) as usize;
    let body_left = left.saturating_sub((body_w.saturating_sub(box_w)) / 2);
    Some(NormBox::from_corners(
        body_left as f32 / width as f32,
        face_bottom.saturating_sub(box_h) as f32 / height as f32,
        (body_left + body_w).min(width) as f32 / width as f32,
        (face_bottom + body_h).min(height) as f32 / height as f32,
    ))
}

/// Landmarks in pixels to landmarks in `0..1`.
fn to_normalised(pixels: &Landmarks, width: usize, height: usize) -> Landmarks {
    crate::face::align::to_normalised(pixels, width, height)
}

/// A deterministic recognition template for one identity.
///
/// `identity` chooses a direction on the unit sphere; `variation` perturbs it. The
/// perturbation is scaled so that:
///
/// * two templates of the same identity are within about
///   `spread * sqrt(2)` in cosine distance, and
/// * two templates of different identities are near `sqrt(2)` apart, because two
///   independent random directions in 512 dimensions are very nearly orthogonal.
///
/// That second property is why this is usable as ground truth at all: in high
/// dimensions, random identities are *reliably* far apart, so a clustering failure
/// in a test is a failure of the algorithm and not of the fixture.
///
/// `lookalike_of` is the case that matters. Passing another identity blends the two
/// base directions, producing a pair of identities whose separation is a chosen
/// fraction of normal - the sisters of section 12's third failure mode, and the
/// input the rank-order verification in `crate::face::cluster` exists to survive.
#[must_use]
pub fn synthetic_template(
    identity: u32,
    variation: u32,
    spread: f32,
    lookalike_of: Option<(u32, f32)>,
) -> Vec<f32> {
    let dimension = crate::face::embed::TEMPLATE_DIM;
    // Wrapping throughout: these are hash mixes, not arithmetic, and the workspace builds
    // with overflow checks on.
    let mut base = direction(mix(u64::from(identity)));

    if let Some((other, closeness)) = lookalike_of {
        let sibling = direction(mix(u64::from(other)));
        let blend = closeness.clamp(0.0, 0.99);
        for (slot, value) in base.iter_mut().zip(sibling.iter()) {
            *slot = *slot * (1.0 - blend) + value * blend;
        }
    }

    let noise = direction(
        ((u64::from(identity) << 32) | u64::from(variation))
            .wrapping_mul(0xD6E8_FEB8_6659_FD93)
            .wrapping_add(7),
    );
    let mut out = Vec::with_capacity(dimension);
    for index in 0..dimension {
        let signal = base.get(index).copied().unwrap_or(0.0);
        let jitter = noise.get(index).copied().unwrap_or(0.0);
        out.push(signal + jitter * spread);
    }
    aura_index::query::normalise(&mut out);
    out
}

/// One identity handle to a seed. A multiplicative hash, wrapping.
fn mix(value: u64) -> u64 {
    value.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1)
}

/// A deterministic unit vector from a seed. xorshift64*, as in
/// `aura_infer::onnx::fixtures`, so no dependency can change the fixtures.
fn direction(seed: u64) -> Vec<f32> {
    let dimension = crate::face::embed::TEMPLATE_DIM;
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(dimension);
    for _ in 0..dimension {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bits = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let unit = (bits >> 40) as f32 / 16_777_216.0;
        out.push(unit * 2.0 - 1.0);
    }
    aura_index::query::normalise(&mut out);
    out
}

/// Detection recall against ground truth, at an intersection-over-union threshold.
///
/// Section 10.1's metric. Greedy matching by intersection over union: each ground truth
/// box claims the
/// best unclaimed detection above the threshold, so one detection cannot satisfy two
/// faces - which is the mistake that makes a merged double-detection look like perfect
/// recall.
#[must_use]
pub fn recall_at_iou(truth: &[SynthFace], found: &[Detection], iou: f32) -> (usize, usize) {
    let real: Vec<&SynthFace> = truth.iter().filter(|face| !face.is_bokeh()).collect();
    let mut claimed = vec![false; found.len()];
    let mut matched = 0usize;

    for face in &real {
        let target = face.truth_box();
        let mut best: Option<(usize, f32)> = None;
        for (index, detection) in found.iter().enumerate() {
            if claimed.get(index).copied().unwrap_or(true) {
                continue;
            }
            let overlap = target.iou(&detection.face);
            if overlap < iou {
                continue;
            }
            if best.is_none_or(|(_, previous)| overlap > previous) {
                best = Some((index, overlap));
            }
        }
        if let Some((index, _)) = best {
            if let Some(flag) = claimed.get_mut(index) {
                *flag = true;
            }
            matched += 1;
        }
    }
    (matched, real.len())
}

/// Detections that matched no real face. The false-positive count.
#[must_use]
pub fn false_positives(truth: &[SynthFace], found: &[Detection], iou: f32) -> usize {
    let real: Vec<NormBox> = truth
        .iter()
        .filter(|face| !face.is_bokeh())
        .map(SynthFace::truth_box)
        .collect();
    found
        .iter()
        .filter(|detection| !real.iter().any(|target| target.iou(&detection.face) >= iou))
        .count()
}
