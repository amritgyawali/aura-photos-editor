//! The five-point similarity transform, and the pose estimate that falls out of
//! it for free.
//!
//! Alignment is the step that decides whether a recognition template means
//! anything. Two frames of the same person, one at a slight tilt and one straight
//! on, produce templates a long way apart unless the crop is normalised first, and
//! a wedding is twelve hours of tilted candids. So every template in this product
//! is computed from a 112 px crop warped by a similarity transform that puts the
//! five landmarks on [`ARCFACE_REFERENCE`].
//!
//! **The reference geometry is not ours and must not be adjusted.**
//! [`ARCFACE_REFERENCE`] is the published `ArcFace` 112x112 layout. Every trained
//! recognition model in this family expects it; changing a coordinate by a pixel
//! silently degrades every template without failing anything, which is why the
//! constant carries a warning rather than a tuning comment.
//!
//! **Why a similarity transform and not an affine one.** An affine fit to five
//! points can shear, and a sheared face is a different face to a recogniser
//! trained on unsheared crops. Four degrees of freedom - scale, rotation and two
//! translations - is exactly the family of transforms that removes framing without
//! changing the subject. The fit is the closed-form Umeyama solution, so it is
//! deterministic to the bit on every machine: no iteration, no convergence
//! threshold, no dependence on the order the points arrive in.

use aura_infer::onnx::fixtures::FACE_CROP_SIDE;

/// The five points, in the order every model in this family expects: left eye,
/// right eye, nose tip, left mouth corner, right mouth corner.
///
/// "Left" is the *image's* left, not the subject's. Getting that backwards mirrors
/// every crop in the product and is invisible in any test that does not compare
/// against a real model, which is why it is written down here.
pub const LANDMARK_COUNT: usize = 5;

/// Index of the left eye in a landmark array.
pub const LEFT_EYE: usize = 0;
/// Index of the right eye.
pub const RIGHT_EYE: usize = 1;
/// Index of the nose tip.
pub const NOSE: usize = 2;
/// Index of the left mouth corner.
pub const LEFT_MOUTH: usize = 3;
/// Index of the right mouth corner.
pub const RIGHT_MOUTH: usize = 4;

/// The published `ArcFace` reference landmarks for a 112x112 crop, in pixels.
///
/// **Do not tune these.** They are a property of the trained model family, not of
/// AURA. See this module's header.
pub const ARCFACE_REFERENCE: [[f32; 2]; LANDMARK_COUNT] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

/// Where the reference nose sits between the eye line and the mouth line.
///
/// `(71.7366 - 51.599) / (92.285 - 51.599)`, computed once so the pitch estimate
/// has a neutral to measure against rather than a magic 0.5.
pub const REFERENCE_NOSE_RATIO: f32 = 0.494_88;

/// Five landmarks in the coordinate system of one image.
///
/// Normalised to the frame - `0..1` on both axes - so a box found on a tier 1
/// thumbnail is the same box on a tier 2 proxy. Every stored landmark is
/// normalised; every landmark handed to a warp is in pixels. The two are different
/// types in practice and the same type in Rust, so the naming does the work:
/// anything called `norm` is normalised.
pub type Landmarks = [[f32; 2]; LANDMARK_COUNT];

/// Head pose, in degrees, estimated from the five landmarks.
///
/// **This is a geometric estimate, not a prediction.** It is a rigid three-point
/// model - eye line, nose, mouth line - and it is accurate enough for the one job
/// it has: deciding whether a face is turned too far away to be worth voting on an
/// identity. It is not accurate enough to drive a retouch decision, and phase 20
/// must not use it for one.
///
/// Sign conventions, written down because every codebase picks different ones:
///
/// * `yaw` is positive when the subject's face turns towards the image's right.
/// * `pitch` is positive when the chin drops - the subject is looking down.
/// * `roll` is positive when the head tilts clockwise in the frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose {
    /// Left-right rotation, `-90..90`.
    pub yaw: f32,
    /// Up-down rotation, `-90..90`.
    pub pitch: f32,
    /// In-plane rotation, `-180..180`.
    pub roll: f32,
}

impl Pose {
    /// How far from frontal, as one number in `0..1`.
    ///
    /// The three angles combined the way the quality gate wants them: yaw
    /// dominates, because a profile hides one eye and half a face; pitch matters
    /// less; roll barely matters at all, because alignment removes it. The weights
    /// are the gate's, not the geometry's, and they live in
    /// [`crate::face::quality`] - this method is the shape they are applied to.
    #[must_use]
    pub fn frontality_penalty(&self) -> f32 {
        let yaw = (self.yaw.abs() / 90.0).clamp(0.0, 1.0);
        let pitch = (self.pitch.abs() / 90.0).clamp(0.0, 1.0);
        let roll = (self.roll.abs() / 180.0).clamp(0.0, 1.0);
        (0.6 * yaw + 0.3 * pitch + 0.1 * roll).clamp(0.0, 1.0)
    }
}

/// A scale, a rotation and a translation. Four degrees of freedom, no shear.
///
/// Stored as `scale` times a unit rotation rather than as a 2x2 matrix, because
/// inverting it is then a division rather than a determinant, and the inverse is
/// what the warp actually uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimilarityTransform {
    /// Uniform scale factor.
    pub scale: f32,
    /// Cosine of the rotation.
    pub cos: f32,
    /// Sine of the rotation.
    pub sin: f32,
    /// Translation in x.
    pub tx: f32,
    /// Translation in y.
    pub ty: f32,
}

impl SimilarityTransform {
    /// The transform that does nothing.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            scale: 1.0,
            cos: 1.0,
            sin: 0.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Map a source point to the destination.
    #[must_use]
    pub fn forward(&self, point: [f32; 2]) -> [f32; 2] {
        let [x, y] = point;
        [
            self.scale * (self.cos * x - self.sin * y) + self.tx,
            self.scale * (self.sin * x + self.cos * y) + self.ty,
        ]
    }

    /// Map a destination point back to the source.
    ///
    /// Returns the point unchanged when the scale has collapsed, which happens
    /// only for five coincident landmarks - a detection that should already have
    /// been refused by the quality gate. Returning the input rather than
    /// dividing by zero keeps the warp producing a black crop instead of a buffer
    /// of values that are not numbers.
    #[must_use]
    pub fn inverse(&self, point: [f32; 2]) -> [f32; 2] {
        if self.scale.abs() <= f32::EPSILON {
            return point;
        }
        let [x, y] = [point[0] - self.tx, point[1] - self.ty];
        let inv = 1.0 / self.scale;
        [
            inv * (self.cos * x + self.sin * y),
            inv * (-self.sin * x + self.cos * y),
        ]
    }
}

/// The closed-form least-squares similarity transform taking `from` onto `to`.
///
/// This is Umeyama's solution specialised to two dimensions, which collapses to
/// two dot products and an `atan2`. Written out rather than pulled from a linear
/// algebra crate for two reasons: it is nine lines, and a general implementation
/// would use an iterative SVD whose last bit depends on the library version -
/// which would put invariant 4 at the mercy of a `cargo update`.
///
/// Determinism note: both accumulations run over a fixed five-element array in a
/// fixed order, so the result is bit-identical on every machine.
#[must_use]
pub fn similarity_from_points(from: &Landmarks, to: &Landmarks) -> SimilarityTransform {
    let mean = |points: &Landmarks| -> [f32; 2] {
        let mut sum = [0.0f32; 2];
        for point in points {
            sum[0] += point[0];
            sum[1] += point[1];
        }
        let scale = 1.0 / LANDMARK_COUNT as f32;
        [sum[0] * scale, sum[1] * scale]
    };

    let source_mean = mean(from);
    let target_mean = mean(to);

    // `along` is the component of the correlation that survives no rotation;
    // `across` is the component that a rotation would align. Their ratio is the
    // angle and their magnitude is the scale.
    let mut along = 0.0f32;
    let mut across = 0.0f32;
    let mut source_energy = 0.0f32;
    for index in 0..LANDMARK_COUNT {
        let (Some(source), Some(target)) = (from.get(index), to.get(index)) else {
            continue;
        };
        let px = source[0] - source_mean[0];
        let py = source[1] - source_mean[1];
        let qx = target[0] - target_mean[0];
        let qy = target[1] - target_mean[1];
        along += qx * px + qy * py;
        across += qy * px - qx * py;
        source_energy += px * px + py * py;
    }

    if source_energy <= f32::EPSILON {
        // Five coincident points. There is no transform; say so with an identity
        // rather than with an infinity.
        return SimilarityTransform::identity();
    }

    let magnitude = (along * along + across * across).sqrt();
    let scale = magnitude / source_energy;
    let (cos, sin) = if magnitude <= f32::EPSILON {
        (1.0, 0.0)
    } else {
        (along / magnitude, across / magnitude)
    };

    let tx = target_mean[0] - scale * (cos * source_mean[0] - sin * source_mean[1]);
    let ty = target_mean[1] - scale * (sin * source_mean[0] + cos * source_mean[1]);

    SimilarityTransform {
        scale,
        cos,
        sin,
        tx,
        ty,
    }
}

/// The transform that puts one face's landmarks on the `ArcFace` reference.
///
/// `pixels` must be landmarks in image pixels, not normalised ones.
#[must_use]
pub fn alignment_for(pixels: &Landmarks) -> SimilarityTransform {
    similarity_from_points(pixels, &ARCFACE_REFERENCE)
}

/// Warp an interleaved 8-bit sRGB frame into a 112x112 aligned crop.
///
/// Bilinear resampling, and the choice matters in both directions: nearest
/// neighbour would make a template depend on sub-pixel landmark noise, and
/// anything wider than bilinear would blur a 48 px face into mush at the moment
/// the quality gate is trying to decide whether it is sharp enough to vote.
///
/// Samples outside the frame are black. A face at the very edge of a wide ceremony
/// frame is a real case - it is most of what the tiled pass recovers - and
/// clamping to the edge pixel instead would smear a column of the frame's border
/// across the crop and make it look like a hair or a wall.
#[must_use]
pub fn warp_crop(
    rgb: &[u8],
    width: usize,
    height: usize,
    transform: &SimilarityTransform,
) -> Vec<u8> {
    let side = FACE_CROP_SIDE;
    let mut out = vec![0u8; side * side * 3];

    for row in 0..side {
        for column in 0..side {
            // Pixel centres, so the crop is not shifted by half a pixel.
            let destination = [column as f32 + 0.5, row as f32 + 0.5];
            let [sx, sy] = transform.inverse(destination);
            let x = sx - 0.5;
            let y = sy - 0.5;

            let x0 = x.floor();
            let y0 = y.floor();
            let fx = x - x0;
            let fy = y - y0;

            for channel in 0..3 {
                let sample = |ox: f32, oy: f32| -> f32 {
                    let px = x0 + ox;
                    let py = y0 + oy;
                    if px < 0.0 || py < 0.0 {
                        return 0.0;
                    }
                    let (ix, iy) = (px as usize, py as usize);
                    if ix >= width || iy >= height {
                        return 0.0;
                    }
                    let index = (iy * width + ix) * 3 + channel;
                    f32::from(rgb.get(index).copied().unwrap_or(0))
                };

                let top = sample(0.0, 0.0) * (1.0 - fx) + sample(1.0, 0.0) * fx;
                let bottom = sample(0.0, 1.0) * (1.0 - fx) + sample(1.0, 1.0) * fx;
                let value = top * (1.0 - fy) + bottom * fy;

                if let Some(slot) = out.get_mut((row * side + column) * 3 + channel) {
                    *slot = value.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// Estimate head pose from five landmarks in image pixels.
///
/// The three angles come out of three measurements, and each one is the simplest
/// measurement that is not wrong:
///
/// * **Roll** is the angle of the eye line. Exact, up to landmark noise.
/// * **Yaw** uses the horizontal displacement of the nose from the eye midpoint,
///   after the roll has been removed, divided by half the inter-ocular distance.
///   Under a rigid three-point model that ratio is `sin(yaw)` for moderate angles,
///   so the estimate is an `asin`. It saturates at 90 degrees rather than
///   extrapolating: past a three-quarter profile one eye is occluded, the landmark
///   is a guess, and a bigger number would be a more confident guess.
/// * **Pitch** uses where the nose sits between the eye line and the mouth line,
///   compared with [`REFERENCE_NOSE_RATIO`]. Foreshortening moves it towards the
///   mouth when the chin drops and towards the eyes when the chin lifts.
#[must_use]
pub fn pose_from_landmarks(pixels: &Landmarks) -> Pose {
    let (Some(left_eye), Some(right_eye), Some(nose), Some(left_mouth), Some(right_mouth)) = (
        pixels.get(LEFT_EYE),
        pixels.get(RIGHT_EYE),
        pixels.get(NOSE),
        pixels.get(LEFT_MOUTH),
        pixels.get(RIGHT_MOUTH),
    ) else {
        return Pose::default();
    };

    let across = right_eye[0] - left_eye[0];
    let down = right_eye[1] - left_eye[1];
    let interocular = (across * across + down * down).sqrt();
    if interocular <= f32::EPSILON {
        return Pose::default();
    }

    let roll_rad = down.atan2(across);
    let roll = roll_rad.to_degrees();

    let eye_mid = [
        (left_eye[0] + right_eye[0]) * 0.5,
        (left_eye[1] + right_eye[1]) * 0.5,
    ];
    let mouth_mid = [
        (left_mouth[0] + right_mouth[0]) * 0.5,
        (left_mouth[1] + right_mouth[1]) * 0.5,
    ];

    // Remove the roll before measuring anything else, so a tilted head does not
    // read as a turned one.
    let (cos, sin) = (roll_rad.cos(), roll_rad.sin());
    let derotate = |point: &[f32; 2]| -> [f32; 2] {
        let x = point[0] - eye_mid[0];
        let y = point[1] - eye_mid[1];
        [cos * x + sin * y, -sin * x + cos * y]
    };

    let nose_local = derotate(nose);
    let mouth_local = derotate(&mouth_mid);

    let yaw_ratio = (nose_local[0] / (interocular * 0.5)).clamp(-1.0, 1.0);
    let yaw = yaw_ratio.asin().to_degrees();

    let span = mouth_local[1];
    let pitch = if span.abs() <= f32::EPSILON {
        0.0
    } else {
        let ratio = nose_local[1] / span;
        let deviation = ((ratio - REFERENCE_NOSE_RATIO) * 2.0).clamp(-1.0, 1.0);
        deviation.asin().to_degrees()
    };

    Pose { yaw, pitch, roll }
}

/// Scale normalised landmarks up into image pixels.
#[must_use]
pub fn to_pixels(norm: &Landmarks, width: usize, height: usize) -> Landmarks {
    let mut out = [[0.0f32; 2]; LANDMARK_COUNT];
    for (slot, point) in out.iter_mut().zip(norm.iter()) {
        *slot = [point[0] * width as f32, point[1] * height as f32];
    }
    out
}

/// Scale landmarks in image pixels down into `0..1`.
#[must_use]
pub fn to_normalised(pixels: &Landmarks, width: usize, height: usize) -> Landmarks {
    let mut out = [[0.0f32; 2]; LANDMARK_COUNT];
    if width == 0 || height == 0 {
        return out;
    }
    for (slot, point) in out.iter_mut().zip(pixels.iter()) {
        *slot = [point[0] / width as f32, point[1] / height as f32];
    }
    out
}

/// Landmarks as the `landmarks_json` column stores them.
///
/// Hand-rolled rather than `serde_json`, because the shape is fixed at five pairs
/// and a fixed number of decimal places is what makes the column comparable
/// between two runs. Six places is about a tenth of a pixel on a 45 MP frame.
#[must_use]
pub fn to_json(norm: &Landmarks) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(96);
    out.push('[');
    for (index, point) in norm.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        // Writing into a String cannot fail; the result is discarded deliberately.
        let _ = write!(out, "[{:.6},{:.6}]", point[0], point[1]);
    }
    out.push(']');
    out
}

/// Parse what [`to_json`] wrote.
///
/// Returns `None` for anything that is not exactly five pairs of numbers. A
/// landmark array of the wrong length is a schema change nobody recorded, and
/// guessing at the missing points would put a face's alignment - and therefore its
/// template - somewhere nobody chose.
#[must_use]
pub fn from_json(text: &str) -> Option<Landmarks> {
    let mut numbers = Vec::with_capacity(LANDMARK_COUNT * 2);
    for piece in text.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e')) {
        if piece.is_empty() {
            continue;
        }
        numbers.push(piece.parse::<f32>().ok()?);
    }
    if numbers.len() != LANDMARK_COUNT * 2 {
        return None;
    }
    let mut out = [[0.0f32; 2]; LANDMARK_COUNT];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = [*numbers.get(index * 2)?, *numbers.get(index * 2 + 1)?];
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// The two-point path
// ---------------------------------------------------------------------------

/// Warp a face into the canonical 112 px crop from **two** eye points in pixels.
///
/// **A two-point similarity, not [`alignment_for`]'s five-point one, and the difference
/// is worth stating.** [`similarity_from_points`] solves a least-squares fit over five
/// landmarks - two eyes, the nose and two mouth corners - which is more robust to one bad
/// landmark and is what the *recogniser* needs. Two points is what
/// `aura_core::FaceRef` carries, because the ADR-0019 amendment deliberately left the
/// nose and the mouth corners out: they are the half of the landmark set that makes a
/// face identifiable, and a type that ranks subjects has no business holding them.
///
/// Two points determine a similarity transform exactly - scale, rotation and translation -
/// which is all the alignment an eye-state or an expression question needs: it puts the
/// eyes on a horizontal line at a known separation, which is precisely the normalisation
/// that makes a head tilted into a hug comparable with a head held level.
///
/// What is lost is robustness to a single mis-detected eye. A face whose landmarks are
/// wrong produces a crop that is wrong, and any head then answers about the wrong pixels.
/// The mitigation is `FaceRef::has_eyes` and the quality gate phase 06 already applies.
///
/// Lives here rather than in either brain crate because **two phases read it**: phase
/// 09's eye state and phase 10's expression head take the same 112 px crop of the same
/// face, and two copies of a warp is two crops that drift apart while looking identical.
/// `aura-brain-photo` and `aura-brain-wedding` deliberately do not depend on each other,
/// so this is the only place both can see.
#[must_use]
pub fn warp_crop_from_eyes(
    rgb: &[u8],
    width: usize,
    height: usize,
    left_eye: [f32; 2],
    right_eye: [f32; 2],
) -> Vec<u8> {
    let side = FACE_CROP_SIDE;
    let mut out = vec![0u8; side * side * 3];
    if width == 0 || height == 0 {
        return out;
    }

    let [sx0, sy0] = left_eye;
    let [sx1, sy1] = right_eye;
    let from_x = sx1 - sx0;
    let from_y = sy1 - sy0;
    let source_span = from_x.hypot(from_y);
    if source_span <= f32::EPSILON {
        return out;
    }

    let (Some(canonical_left), Some(canonical_right)) =
        (ARCFACE_REFERENCE.first(), ARCFACE_REFERENCE.get(1))
    else {
        return out;
    };
    let onto_x = canonical_right[0] - canonical_left[0];
    let onto_y = canonical_right[1] - canonical_left[1];
    let target_span = onto_x.hypot(onto_y);

    // The inverse transform: for each destination pixel, where in the source it came
    // from. Inverse rather than forward, because a forward warp leaves holes.
    let scale = source_span / target_span;
    let angle = from_y.atan2(from_x) - onto_y.atan2(onto_x);
    let (sin, cos) = angle.sin_cos();
    let a = cos * scale;
    let b = -sin * scale;

    for row in 0..side {
        for column in 0..side {
            let dx = column as f32 - canonical_left[0];
            let dy = row as f32 - canonical_left[1];
            let sx = a.mul_add(dx, b * dy) + sx0;
            let sy = (-b).mul_add(dx, a * dy) + sy0;
            let sample = sample_bilinear_clamped(rgb, width, height, sx, sy);
            let base = (row * side + column) * 3;
            for (channel, value) in sample.into_iter().enumerate() {
                if let Some(slot) = out.get_mut(base + channel) {
                    *slot = value;
                }
            }
        }
    }
    out
}

/// Bilinear sample of an interleaved 8-bit sRGB buffer, clamped at the edges.
///
/// Bilinear rather than the box filter phase 05 uses elsewhere, and the reason is the
/// direction of the resample: an aligned face crop is usually an *upscale* of a 90 px
/// face to 112 px, and a box filter on an upscale is nearest-neighbour with extra steps.
///
/// Clamped rather than black outside the frame, unlike [`warp_crop`]. The two-point path
/// is used on faces that phase 06 has already gated for quality, so an edge face here is
/// a face whose crop is mostly inside; clamping keeps the eye region readable where a
/// black border would give the head a hard edge to answer about.
fn sample_bilinear_clamped(rgb: &[u8], width: usize, height: usize, x: f32, y: f32) -> [u8; 3] {
    if width == 0 || height == 0 {
        return [0; 3];
    }
    let cx = x.clamp(0.0, (width - 1) as f32);
    let cy = y.clamp(0.0, (height - 1) as f32);
    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = cx - x0 as f32;
    let fy = cy - y0 as f32;

    let mut out = [0u8; 3];
    for (channel, slot) in out.iter_mut().enumerate() {
        let at = |px: usize, py: usize| -> f32 {
            f32::from(
                rgb.get((py * width + px) * 3 + channel)
                    .copied()
                    .unwrap_or(0),
            )
        };
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        *slot = (top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Turn one aligned 112 px crop into a head's input tensor.
///
/// NCHW, `0..1`, sRGB, with nothing subtracted - the same three steps and the same
/// absence of parameters as `aura_vision::face::embed::preprocess_crop`, because every
/// manifest that takes this crop declares `range: 0..1`, and a normalisation the
/// manifest does not describe is drift waiting to happen.
///
/// Shared by phases 06, 09 and 10 for [`warp_crop_from_eyes`]'s reason.
#[must_use]
pub fn preprocess_face_crop(crop: &[u8]) -> Vec<f32> {
    let side = FACE_CROP_SIDE;
    let mut out = vec![0.0f32; 3 * side * side];
    for channel in 0..3 {
        for row in 0..side {
            for column in 0..side {
                let source = (row * side + column) * 3 + channel;
                let destination = channel * side * side + row * side + column;
                if let Some(slot) = out.get_mut(destination) {
                    *slot = f32::from(crop.get(source).copied().unwrap_or(0)) / 255.0;
                }
            }
        }
    }
    out
}
