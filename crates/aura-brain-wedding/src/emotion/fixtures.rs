//! Frames whose expression is known by construction.
//!
//! ## Why this module exists, stated plainly
//!
//! The argument phase 06 made for `ReferenceDetector` and phase 09 for
//! `ReferenceEyeReader`, a third time and with the same force. **The two shipped heads
//! are placeholders**: `expression_head` and `interaction_head` 1.0.0 have the right
//! architectures and no training, so on a real photograph the eight channels describe a
//! random projection of a face crop. That is condition C1 of the phase 10 exit report and
//! it is a Sev 2 trigger.
//!
//! So section 10.1's gates - 0.80 pairwise agreement, 0.90 peak recall, 0.85 tears and
//! laughter F1, composure fairness, 0.80 reaction recall - cannot be measured against the
//! weights. They can be measured against the *algorithms*, which is a different and still
//! worthwhile claim: the gaze geometry, the prominence weighting, the nine features, the
//! scene and tradition table, the Bradley-Terry utility, the smoothing kernel, the
//! margin, the three reaction conditions and the resolver are all exactly what will run
//! when a trained head lands, and every one of them can be wrong.
//!
//! The fixtures here paint a **known answer into the pixels** and
//! [`ReferenceExpressionReader`] decodes it, so what is under test is everything around
//! the head.
//!
//! ## Why the markers go through the warp
//!
//! [`EmotionFrame::paint_expression`] places its markers in the *aligned crop's*
//! coordinates and maps them back into the source, so the reader finds them only if the
//! warp put them where the fixture asked. A reader that read the source directly would
//! pass with a broken alignment, which is exactly the failure phase 09's eye marker was
//! written to catch.

use aura_core::contract::emotion::Interaction;
use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::FaceRef;
use aura_core::FaceId;
use aura_infer::onnx::fixtures::{EXPRESSION_CHANNELS, EXPRESSION_CROP_SIDE, INTERACTION_CLASSES};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use aura_vision::face::align::ARCFACE_REFERENCE;

/// Side of every synthetic emotion frame.
///
/// 512, for `aura_brain_photo::fixtures::SIDE`'s reason: the analysers are
/// resolution-independent by construction and a 512 px fixture runs sixteen times faster
/// in a suite that builds several hundred of them. Large enough that a 112 px aligned
/// crop of a face occupying a fifth of the frame is an upscale of about a hundred source
/// pixels, which is the real case.
pub const EMOTION_SIDE: usize = 512;

/// Where the eight expression markers land in the 112 px aligned crop.
///
/// A ring around the centre at 28 px spacing, which is 24 px clear between neighbours -
/// comfortably more than the marker radius, so bilinear resampling cannot bleed one
/// channel into the next. The centre is deliberately empty: it is where
/// `aura_brain_photo::fixtures::ReferenceEyeReader::MARKER` sits, so one frame can carry
/// both phase 09's eye answer and phase 10's expression answer without either reader
/// seeing the other's.
pub const EXPRESSION_MARKERS: [(usize, usize); EXPRESSION_CHANNELS] = [
    (28, 28),
    (56, 28),
    (84, 28),
    (28, 56),
    (84, 56),
    (28, 84),
    (56, 84),
    (84, 84),
];

/// Radius of one marker in the aligned crop, in pixels.
pub const MARKER_RADIUS: f32 = 7.0;

/// A synthetic frame and what is true about it.
#[derive(Debug, Clone)]
pub struct EmotionFrame {
    /// Interleaved 8-bit sRGB.
    pub rgb: Vec<u8>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// The faces phase 06 would have found.
    pub faces: Vec<FaceRef>,
    /// What was painted into each face, in `FaceExpression::channels` order.
    pub truth: Vec<[f32; EXPRESSION_CHANNELS]>,
    /// The interaction the frame was built to contain, if any.
    pub interaction: Option<(Interaction, f32)>,
}

impl EmotionFrame {
    /// A textured frame with `count` faces across the middle.
    ///
    /// The faces are laid out left to right with a gap, so that `gaze::within_reach` is
    /// satisfiable between neighbours and the mutual-gaze test has something to find.
    #[must_use]
    pub fn with_faces(count: usize) -> Self {
        let width = EMOTION_SIDE;
        let height = EMOTION_SIDE;
        let mut rgb = vec![0u8; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let value = background_texture(x, y);
                write_pixel(&mut rgb, width, x, y, [value; 3]);
            }
        }

        let mut faces = Vec::with_capacity(count);
        let mut truth = Vec::with_capacity(count);
        let span = 1.0 / (count as f32 + 1.0);
        for index in 0..count {
            let box_width = (span * 0.7).min(0.26);
            let bbox = CropRect {
                x: span * (index as f32 + 1.0) - box_width / 2.0,
                y: 0.32,
                w: box_width,
                h: box_width,
            }
            .clamped();
            faces.push(emotion_face(bbox, i32::try_from(index).unwrap_or(0)));
            truth.push([0.0f32; EXPRESSION_CHANNELS]);
        }

        let mut frame = Self {
            rgb,
            width,
            height,
            faces,
            truth,
            interaction: None,
        };
        for index in 0..count {
            frame.paint_face(index);
        }
        frame
    }

    /// Paint one face's rectangle, so its crop is not flat background.
    fn paint_face(&mut self, index: usize) {
        let Some(face) = self.faces.get(index).copied() else {
            return;
        };
        let (x0, y0, x1, y1) = to_pixels(face.bbox, self.width, self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let value = face_texture(x, y);
                write_pixel(&mut self.rgb, self.width, x, y, [value; 3]);
            }
        }
    }

    /// Paint the eight-channel answer [`ReferenceExpressionReader`] decodes.
    ///
    /// The markers are placed in **destination** space and mapped back into the source
    /// through the forward similarity, so the reader needs to know nothing about how the
    /// warp was computed - which is what makes it a reference rather than a second copy
    /// of the implementation.
    pub fn paint_expression(&mut self, index: usize, channels: [f32; EXPRESSION_CHANNELS]) {
        let Some(face) = self.faces.get(index).copied() else {
            return;
        };
        let Some(map) = ForwardMap::of(&face, self.width, self.height) else {
            return;
        };

        for (slot, (dx, dy)) in EXPRESSION_MARKERS.into_iter().enumerate() {
            let value =
                (channels.get(slot).copied().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0).round() as u8;
            let (sx, sy) = map.source_of(dx as f32, dy as f32);
            let radius = MARKER_RADIUS * map.scale;
            paint_disc(
                &mut self.rgb,
                self.width,
                self.height,
                sx,
                sy,
                radius,
                [value; 3],
            );
        }
        if let Some(slot) = self.truth.get_mut(index) {
            *slot = channels;
        }
    }

    /// Turn one face towards the left or right of the frame, for the gaze tests.
    ///
    /// Moves both eye landmarks horizontally inside the box. Positive `amount` turns the
    /// head towards the frame's **right**, which moves the visible eyes left - the sign
    /// `gaze::turn_of` negates, and getting it backwards is the mistake the eval harness
    /// checks for by name.
    ///
    /// The markers are re-painted afterwards by the caller if the expression matters,
    /// because moving the eyes moves the warp.
    pub fn turn_face(&mut self, index: usize, amount: f32) {
        let Some(face) = self.faces.get_mut(index) else {
            return;
        };
        let shift = -amount * face.bbox.w;
        face.eyes[0][0] += shift;
        face.eyes[1][0] += shift;
    }

    /// Give one face an identity, so the prominence and couple paths are exercised.
    pub fn assign(&mut self, index: usize, identity: aura_core::IdentityId) {
        if let Some(face) = self.faces.get_mut(index) {
            face.identity_id = Some(identity);
        }
    }

    /// The `PixelBuffer` the analyser takes.
    #[must_use]
    pub fn buffer(&self) -> PixelBuffer {
        PixelBuffer {
            width: self.width as u32,
            height: self.height as u32,
            data: PixelData::Srgb8(self.rgb.clone()),
            colour_space: ColourSpace::Srgb,
            // `Demosaiced` rather than `Embedded`, because phase 02's rule is that pixels
            // carry their provenance and a fixture is a rendered buffer rather than a
            // camera JPEG. Nothing in this phase branches on it; a fixture that lied about
            // it would still be a fixture that lied.
            source: PixelSource::Demosaiced,
            decode_ms: 0,
        }
    }
}

/// The destination-to-source map the aligned warp inverts.
///
/// Exactly `aura_vision::face::align::warp_crop_from_eyes`'s inner transform, written out
/// here so a fixture can place a marker at a chosen crop position. If the two ever
/// disagree the markers land in the wrong place and every phase 10 gate fails loudly,
/// which is the correct way for that particular mistake to surface.
#[derive(Debug, Clone, Copy)]
pub struct ForwardMap {
    a: f32,
    b: f32,
    sx0: f32,
    sy0: f32,
    left: [f32; 2],
    /// Source pixels per destination pixel.
    pub scale: f32,
}

impl ForwardMap {
    /// Build the map for one face in one frame, or `None` when the landmarks collapse.
    #[must_use]
    pub fn of(face: &FaceRef, width: usize, height: usize) -> Option<Self> {
        if !face.has_eyes() {
            return None;
        }
        let sx0 = face.eyes[0][0] * width as f32;
        let sy0 = face.eyes[0][1] * height as f32;
        let sx1 = face.eyes[1][0] * width as f32;
        let sy1 = face.eyes[1][1] * height as f32;
        let from_x = sx1 - sx0;
        let from_y = sy1 - sy0;
        let source_span = from_x.hypot(from_y);
        if source_span <= f32::EPSILON {
            return None;
        }
        let left = *ARCFACE_REFERENCE.first()?;
        let right = *ARCFACE_REFERENCE.get(1)?;
        let onto_x = right[0] - left[0];
        let onto_y = right[1] - left[1];
        let target_span = onto_x.hypot(onto_y);
        let scale = source_span / target_span;
        let angle = from_y.atan2(from_x) - onto_y.atan2(onto_x);
        let (sin, cos) = angle.sin_cos();
        Some(Self {
            a: cos * scale,
            b: -sin * scale,
            sx0,
            sy0,
            left,
            scale,
        })
    }

    /// Where in the source a destination pixel comes from.
    #[must_use]
    pub fn source_of(&self, column: f32, row: f32) -> (f32, f32) {
        let dx = column - self.left[0];
        let dy = row - self.left[1];
        (
            self.a.mul_add(dx, self.b * dy) + self.sx0,
            (-self.b).mul_add(dx, self.a * dy) + self.sy0,
        )
    }
}

/// The expression reader that replaces the head in every gate.
///
/// It decodes the markers [`EmotionFrame::paint_expression`] painted, so its answer is
/// the fixture's answer and the thing under test is everything *around* the head.
///
/// Deliberately explicit at the call site rather than a feature flag, so that no product
/// path can reach it by accident.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReferenceExpressionReader;

impl ReferenceExpressionReader {
    /// Read one preprocessed crop tensor.
    ///
    /// The tensor is NCHW `0..1`, which is what `ExpressionHead::run_batch` takes, so the
    /// reader sits exactly where the head sits and no call site has to know which is in
    /// use.
    #[must_use]
    pub fn read(tensor: &[f32]) -> [f32; EXPRESSION_CHANNELS] {
        let side = EXPRESSION_CROP_SIDE;
        let mut out = [0.0f32; EXPRESSION_CHANNELS];
        for (slot, (x, y)) in EXPRESSION_MARKERS.into_iter().enumerate() {
            if x >= side || y >= side {
                continue;
            }
            // Channel 0 of the NCHW tensor. The markers are grey so any channel would
            // do, and reading one rather than averaging three keeps the reader trivially
            // auditable.
            if let Some(out_slot) = out.get_mut(slot) {
                *out_slot = tensor
                    .get(y * side + x)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
            }
        }
        out
    }
}

/// A `FaceRef` for one rectangle, with eyes where a face's eyes are.
///
/// The same 30 %/70 % width and 38 % height landmark positions
/// `aura_brain_photo::fixtures::face` uses, so a frame built by either module aligns the
/// same way. The id is derived from the geometry plus a salt, because two fixtures with
/// the same subject rectangle would otherwise share a `faces.id` primary key - the
/// mistake phase 09's gate found rather than review.
#[must_use]
pub fn emotion_face(bbox: CropRect, salt: i32) -> FaceRef {
    let quantise = |value: f32| i64::from((value * 10_000.0).round() as i32);
    let seed = quantise(bbox.x)
        .wrapping_mul(1_000_003)
        .wrapping_add(quantise(bbox.y).wrapping_mul(7_919))
        .wrapping_add(quantise(bbox.w).wrapping_mul(104_729))
        .wrapping_add(i64::from(salt).wrapping_mul(15_485_863));
    let mut bytes = [0u8; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        let shift = (index % 8) * 8;
        *slot = ((seed >> shift) as u8) ^ (index as u8).wrapping_mul(31);
    }
    FaceRef {
        face_id: FaceId::from_uuid(uuid::Uuid::from_bytes(bytes)),
        identity_id: None,
        bbox,
        eyes: [
            [bbox.x + bbox.w * 0.30, bbox.y + bbox.h * 0.38],
            [bbox.x + bbox.w * 0.70, bbox.y + bbox.h * 0.38],
        ],
        area_frac: bbox.w * bbox.h,
        centrality: 1.0 - ((bbox.x + bbox.w / 2.0) - 0.5).abs() * 2.0,
        sharpness: 0.80,
        quality: 0.80,
        votes: true,
    }
}

// ---------------------------------------------------------------------------
// The seven expression profiles the gates are written against
// ---------------------------------------------------------------------------
//
// Order is always `FaceExpression::channels`: smile, genuineness, laughter, tears,
// surprise, tenderness, composed, discomfort.

/// A face that is laughing.
#[must_use]
pub const fn laughing() -> [f32; EXPRESSION_CHANNELS] {
    [0.85, 0.90, 0.92, 0.02, 0.20, 0.30, 0.05, 0.02]
}

/// A face that reads as crying, above `TEARS_CERTAIN`.
#[must_use]
pub const fn crying() -> [f32; EXPRESSION_CHANNELS] {
    [0.20, 0.80, 0.05, 0.94, 0.10, 0.70, 0.15, 0.05]
}

/// A face holding a smile for the camera.
#[must_use]
pub const fn posed() -> [f32; EXPRESSION_CHANNELS] {
    [0.80, 0.20, 0.05, 0.02, 0.05, 0.15, 0.35, 0.05]
}

/// A composed face.
///
/// **The fixture the cultural-fairness gate turns on.** Composure high, smile at the
/// floor, everything else quiet - which is what a couple in a forty-minute rite look
/// like, and what a Western-trained model reads as nothing happening.
#[must_use]
pub const fn composed() -> [f32; EXPRESSION_CHANNELS] {
    [0.10, 0.50, 0.02, 0.05, 0.05, 0.55, 0.92, 0.03]
}

/// A face smiling broadly and unposed.
#[must_use]
pub const fn smiling() -> [f32; EXPRESSION_CHANNELS] {
    [0.88, 0.82, 0.30, 0.02, 0.10, 0.35, 0.10, 0.02]
}

/// A face that reads as awkward.
#[must_use]
pub const fn awkward() -> [f32; EXPRESSION_CHANNELS] {
    [0.15, 0.10, 0.02, 0.02, 0.25, 0.05, 0.20, 0.80]
}

/// A face expressing nothing in particular.
#[must_use]
pub const fn flat() -> [f32; EXPRESSION_CHANNELS] {
    [0.06, 0.10, 0.02, 0.01, 0.03, 0.08, 0.20, 0.04]
}

/// A one-hot interaction vector, for a fixture that plants a known interaction.
///
/// The other eight slots sit at 0.05 rather than at zero, which is below
/// `interaction::DETECTION_FLOOR` and is what a real multi-label head's quiet channels
/// look like. A vector of exact zeroes would let a bug in the floor comparison pass.
#[must_use]
pub fn interaction_vector(kind: Interaction, strength: f32) -> [f32; INTERACTION_CLASSES] {
    let mut out = [0.05f32; INTERACTION_CLASSES];
    if let Some(slot) = out.get_mut(kind.as_index()) {
        *slot = strength.clamp(0.0, 1.0);
    }
    out
}

/// A run of frames whose expression rises to a peak and falls away.
///
/// `count` frames peaking at `peak_at`. A triangle rather than a Gaussian, because a
/// triangle has a single unambiguous argmax and what is under test is the kernel and the
/// margin rather than the product's ability to find a maximum.
///
/// Every frame carries one face. The peak frame's laughter is 0.95 and the shoulders fall
/// by `step` per frame, so the margin the detector should recover is known in advance.
#[must_use]
pub fn moment_run(count: usize, peak_at: usize, step: f32) -> Vec<EmotionFrame> {
    (0..count)
        .map(|index| {
            let mut frame = EmotionFrame::with_faces(1);
            let distance = index.abs_diff(peak_at) as f32;
            let intensity = (0.95 - distance * step).clamp(0.05, 0.95);
            let mut channels = laughing();
            if let Some(slot) = channels.get_mut(2) {
                *slot = intensity;
            }
            if let Some(slot) = channels.get_mut(0) {
                *slot = intensity * 0.9;
            }
            frame.paint_expression(0, channels);
            frame
        })
        .collect()
}

/// A flat run: `count` frames with the same expression in every one.
///
/// The fixture `AURA-ML-5042` is written against. A bracketed detail shot genuinely has
/// no apex, and a peak detector that names one anyway is a peak detector phase 29 would
/// build an album spread around.
#[must_use]
pub fn flat_run(count: usize) -> Vec<EmotionFrame> {
    (0..count)
        .map(|_| {
            let mut frame = EmotionFrame::with_faces(1);
            frame.paint_expression(0, posed());
            frame
        })
        .collect()
}

/// Background texture: a fine checker, so a global average does not collapse.
fn background_texture(x: usize, y: usize) -> u8 {
    let base = 96u8;
    let checker = if (x / 8 + y / 8).is_multiple_of(2) {
        12
    } else {
        0
    };
    base.saturating_add(checker)
}

/// Face texture: coarser and lighter than the background.
fn face_texture(x: usize, y: usize) -> u8 {
    let base = 150u8;
    let checker = if (x / 5 + y / 5).is_multiple_of(2) {
        18
    } else {
        0
    };
    base.saturating_add(checker)
}

/// A rectangle in normalised coordinates as pixel bounds.
fn to_pixels(rect: CropRect, width: usize, height: usize) -> (usize, usize, usize, usize) {
    let x0 = ((rect.x * width as f32).max(0.0) as usize).min(width);
    let y0 = ((rect.y * height as f32).max(0.0) as usize).min(height);
    let x1 = ((((rect.x + rect.w) * width as f32).max(0.0)) as usize).min(width);
    let y1 = ((((rect.y + rect.h) * height as f32).max(0.0)) as usize).min(height);
    (x0, y0, x1, y1)
}

/// Paint a filled disc of one colour.
fn paint_disc(
    rgb: &mut [u8],
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    colour: [u8; 3],
) {
    let r = radius.max(1.0);
    let x0 = ((cx - r).floor().max(0.0) as usize).min(width);
    let y0 = ((cy - r).floor().max(0.0) as usize).min(height);
    let x1 = (((cx + r).ceil().max(0.0)) as usize).min(width);
    let y1 = (((cy + r).ceil().max(0.0)) as usize).min(height);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx.hypot(dy) <= r {
                write_pixel(rgb, width, x, y, colour);
            }
        }
    }
}

/// Write one interleaved sRGB pixel.
fn write_pixel(rgb: &mut [u8], width: usize, x: usize, y: usize, colour: [u8; 3]) {
    let base = (y * width + x) * 3;
    for (channel, value) in colour.into_iter().enumerate() {
        if let Some(slot) = rgb.get_mut(base + channel) {
            *slot = value;
        }
    }
}
