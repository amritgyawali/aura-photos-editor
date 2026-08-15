//! The synthetic ground truth every section 10.1 gate is measured against.
//!
//! There are no camera files in this repository - phase 02's first exit condition, still
//! open - and no consented face data. Every number in section 10.1 therefore has to be
//! measured against something whose answer is known by construction, and this module is
//! that something.
//!
//! **Read what that does and does not prove.** A blur ladder built here has a known
//! ordering, so a monotonic sharpness response is a real result about the analyser. A
//! synthetic eye marker has a known state, so a blink F1 of 1.000 is a real result about
//! the *gating, intent and flag* logic - and says nothing whatever about the eye-state
//! head's weights, which are a placeholder. Phase 06 drew exactly this line with its
//! `ReferenceDetector` and phase 07 with its synthetic weddings; this is the third time
//! and the honesty is the same.
//!
//! ## What is here
//!
//! | Builder | What it proves |
//! |---|---|
//! | [`blur_ladder`] | monotonic sharpness, and where the soft threshold lands |
//! | [`focus_miss`] | the sign convention: negative is front focus |
//! | [`shallow_depth_of_field`] | creamy bokeh is never called soft |
//! | [`camera_shake`] / [`panned`] | shake and a pan are distinguished |
//! | [`iso_ladder`] | the noise estimate is within 15 % of a known sigma |
//! | [`clipped`] / [`candle`] | a blown dress is a loss, a flame is not |
//! | [`cross_camera_pair`] | a 24 MP and a 61 MP body score within 0.05 |
//! | [`group_frame`] | the closed-eye ratio matches a human count exactly |
//! | [`ReferenceEyeReader`] | the eye pipeline, with the head replaced by a known answer |

use aura_core::contract::integrity::{CropRect, EyeOpenness};
use aura_core::contract::people::FaceRef;
use aura_core::FaceId;
use uuid::Uuid;

/// Side of every synthetic frame.
///
/// 512 rather than 2048. The analysers are resolution-independent by construction -
/// every one of them normalises by contrast, by tile count or by the calibration row -
/// and a 512 px fixture runs sixteen times faster in a test suite that builds several
/// hundred of them. The one place resolution matters is the noise estimator's tile
/// count, and 512 gives it 256 tiles, which is well above [`crate::integrity::noise::MIN_TILES`].
pub const SIDE: usize = 512;

/// Where the subject sits in every synthetic frame.
pub const FACE_RECT: CropRect = CropRect {
    x: 0.35,
    y: 0.30,
    w: 0.30,
    h: 0.30,
};

/// A synthetic frame and what is true about it.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Interleaved 8-bit sRGB.
    pub rgb: Vec<u8>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// The faces phase 06 would have found.
    pub faces: Vec<FaceRef>,
    /// What the fixture was built to be.
    pub truth: Truth,
}

/// What a fixture's builder knows and the analyser is being asked to recover.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Truth {
    /// Blur radius applied to the subject, in pixels. Zero is sharp.
    pub subject_blur: usize,
    /// Blur radius applied to the background.
    pub background_blur: usize,
    /// Noise sigma added, in `0..1` luminance units.
    pub sigma: f32,
    /// The eye state painted into each face's marker.
    pub eyes: EyeOpenness,
    /// How many of the faces have closed eyes.
    pub closed_faces: usize,
    /// True when the fixture is a deliberate pan.
    pub panned: bool,
    /// True when the fixture is camera shake.
    pub shaken: bool,
}

impl Frame {
    /// A mid-grey frame with a textured subject and a textured background.
    ///
    /// **The subject's texture is deliberately not a one-pixel checker**, and the first
    /// version of this module used one. A Nyquist-frequency checker is destroyed
    /// completely by a box blur of radius three, which takes the region's contrast to
    /// nearly zero - and acutance is a ratio with contrast in the denominator, so the
    /// ladder stopped being monotonic exactly where it mattered. It also measures an
    /// acutance around five times what a real photograph reaches, which put every
    /// fixture at the saturating end of `focus::normalise` and made the cross-camera
    /// fairness gate meaningless.
    ///
    /// So the subject carries **three frequencies** - periods of four, eight and sixteen
    /// pixels - at falling amplitude. A blur removes them in order, the contrast survives
    /// to the end of the ladder, and the measured acutance lands in the same range as the
    /// `expected_mtf50` figures in the shipped calibration table. The background's
    /// texture is coarser still, so a band and a face are distinguishable before any blur
    /// is applied.
    #[must_use]
    pub fn base() -> Self {
        let mut rgb = vec![0u8; SIDE * SIDE * 3];
        for y in 0..SIDE {
            for x in 0..SIDE {
                // A gentle vertical gradient, so the frame has a median near mid grey and
                // the exposure analysis has something ordinary to say about it.
                let gradient = 96.0 + 48.0 * (y as f32 / SIDE as f32);
                let coarse = if (x / 16 + y / 16) % 2 == 0 { 18.0 } else { -18.0 };
                let value = (gradient + coarse).clamp(0.0, 255.0) as u8;
                write(&mut rgb, x, y, [value, value, value]);
            }
        }
        let mut frame = Self {
            rgb,
            width: SIDE,
            height: SIDE,
            faces: vec![face(FACE_RECT)],
            truth: Truth::default(),
        };
        frame.paint_subject();
        frame
    }

    /// Paint the subject's texture and its eye marker.
    fn paint_subject(&mut self) {
        let rect = pixels(FACE_RECT, self.width, self.height);
        for y in rect.1..rect.3 {
            for x in rect.0..rect.2 {
                write(&mut self.rgb, x, y, [subject_texture(x, y); 3]);
            }
        }
        self.paint_eye_marker(0, EyeOpenness::Open);
    }

    /// Paint the marker [`ReferenceEyeReader`] decodes.
    ///
    /// At the midpoint between the two eye landmarks, which the alignment maps to a known
    /// position in the 112 px crop - so the reader can find it without knowing anything
    /// about how the warp was computed. Five levels, one per class, spaced fifty apart so
    /// that bilinear resampling cannot move a reading into the next class.
    pub fn paint_eye_marker(&mut self, face: usize, state: EyeOpenness) {
        let Some(reference) = self.faces.get(face) else {
            return;
        };
        if !reference.has_eyes() {
            return;
        }
        let mid_x = f32::midpoint(reference.eyes[0][0], reference.eyes[1][0]) * self.width as f32;
        let mid_y = f32::midpoint(reference.eyes[0][1], reference.eyes[1][1]) * self.height as f32;
        let level = marker_level(state);
        let radius = (self.width / 40).max(3);
        let cx = mid_x.round().max(0.0) as usize;
        let cy = mid_y.round().max(0.0) as usize;
        for y in cy.saturating_sub(radius)..(cy + radius).min(self.height) {
            for x in cx.saturating_sub(radius)..(cx + radius).min(self.width) {
                write(&mut self.rgb, x, y, [level, level, level]);
            }
        }
    }

    /// Blur one rectangle with a separable box filter.
    ///
    /// A box filter rather than a Gaussian: it is isotropic, which is what makes it a
    /// *defocus* fixture rather than a motion one, and its effect on acutance is
    /// monotonic in the radius with no shoulder - so a ladder built from it has a known
    /// ordering rather than a hoped-for one.
    pub fn blur_rect(&mut self, rect: CropRect, radius: usize) {
        if radius == 0 {
            return;
        }
        let (x0, y0, x1, y1) = pixels(rect, self.width, self.height);
        let source = self.rgb.clone();
        for y in y0..y1 {
            for x in x0..x1 {
                let mut totals = [0u32; 3];
                let mut count = 0u32;
                for sy in y.saturating_sub(radius)..=(y + radius).min(self.height - 1) {
                    for sx in x.saturating_sub(radius)..=(x + radius).min(self.width - 1) {
                        for (channel, total) in totals.iter_mut().enumerate() {
                            *total += u32::from(
                                source
                                    .get((sy * self.width + sx) * 3 + channel)
                                    .copied()
                                    .unwrap_or(0),
                            );
                        }
                        count += 1;
                    }
                }
                if count == 0 {
                    continue;
                }
                let averaged = [
                    (totals[0] / count) as u8,
                    (totals[1] / count) as u8,
                    (totals[2] / count) as u8,
                ];
                write(&mut self.rgb, x, y, averaged);
            }
        }
    }

    /// Smear one rectangle along one axis.
    ///
    /// The motion fixture. Horizontal only, because the structure tensor's whole job is
    /// to notice that the smear has a direction and a fixture that smeared both ways
    /// would be a defocus fixture with extra steps.
    pub fn smear_rect(&mut self, rect: CropRect, length: usize) {
        if length == 0 {
            return;
        }
        let (x0, y0, x1, y1) = pixels(rect, self.width, self.height);
        let source = self.rgb.clone();
        for y in y0..y1 {
            for x in x0..x1 {
                let mut totals = [0u32; 3];
                let mut count = 0u32;
                for sx in x.saturating_sub(length)..=(x + length).min(self.width - 1) {
                    for (channel, total) in totals.iter_mut().enumerate() {
                        *total += u32::from(
                            source
                                .get((y * self.width + sx) * 3 + channel)
                                .copied()
                                .unwrap_or(0),
                        );
                    }
                    count += 1;
                }
                if count == 0 {
                    continue;
                }
                let averaged = [
                    (totals[0] / count) as u8,
                    (totals[1] / count) as u8,
                    (totals[2] / count) as u8,
                ];
                write(&mut self.rgb, x, y, averaged);
            }
        }
    }

    /// Add deterministic noise of a known standard deviation.
    ///
    /// A fixed-seed xorshift, summed over twelve uniform draws so that the distribution
    /// is close enough to Gaussian for Immerkær's estimator - which assumes Gaussian - to
    /// be measured fairly. Deterministic, because invariant 4 applies to fixtures too: a
    /// noise gate that passes on one machine and fails on another is not a gate.
    pub fn add_noise(&mut self, sigma: f32, seed: u64) {
        let mut state = seed | 1;
        let mut next = || -> f32 {
            let mut sum = 0.0f32;
            for _ in 0..12 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                sum += ((state >> 40) as f32) / 16_777_216.0;
            }
            sum - 6.0
        };
        for index in 0..self.width * self.height {
            let offset = next() * sigma * 255.0;
            for channel in 0..3 {
                if let Some(slot) = self.rgb.get_mut(index * 3 + channel) {
                    *slot = (f32::from(*slot) + offset).clamp(0.0, 255.0) as u8;
                }
            }
        }
        self.truth.sigma = sigma;
    }

    /// Reduce a rectangle's edge response by a fraction, without destroying its
    /// structure.
    ///
    /// A blend between the frame and a one-pixel-blurred copy of it. Acutance is very
    /// nearly linear in the blend weight at that radius, so `soften(rect, 0.2)` reduces
    /// the measured acutance of the rectangle by about a fifth.
    ///
    /// This is what [`cross_camera_pair`] uses, and the reason it exists rather than a
    /// second call to [`Frame::blur_rect`]: the fairness fixture has to model a
    /// *specific* softening - the one a bigger sensor's extra downsample performs, which
    /// the calibration table records as a ratio - and a blur radius is a much blunter
    /// instrument than the effect being modelled.
    pub fn soften(&mut self, rect: CropRect, amount: f32) {
        let weight = amount.clamp(0.0, 1.0);
        if weight <= f32::EPSILON {
            return;
        }
        let mut blurred = self.clone();
        blurred.blur_rect(rect, 1);
        let (x0, y0, x1, y1) = pixels(rect, self.width, self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                for channel in 0..3 {
                    let index = (y * self.width + x) * 3 + channel;
                    let sharp = f32::from(self.rgb.get(index).copied().unwrap_or(0));
                    let soft = f32::from(blurred.rgb.get(index).copied().unwrap_or(0));
                    if let Some(slot) = self.rgb.get_mut(index) {
                        *slot = sharp.mul_add(1.0 - weight, soft * weight).round() as u8;
                    }
                }
            }
        }
    }

    /// Push every pixel towards white until a fraction of the frame clips.
    pub fn blow_highlights(&mut self, stops: f32) {
        let gain = 2.0f32.powf(stops);
        for slot in &mut self.rgb {
            *slot = (f32::from(*slot) * gain).clamp(0.0, 255.0) as u8;
        }
    }

    /// Crush every pixel towards black.
    pub fn crush_shadows(&mut self, stops: f32) {
        let gain = 2.0f32.powf(-stops);
        for slot in &mut self.rgb {
            *slot = (f32::from(*slot) * gain).clamp(0.0, 255.0) as u8;
        }
    }
}

/// A ladder of frames whose subject is progressively more out of focus.
///
/// Radii 0, 1, 2, 3, 4, 6, 8. Section 10.1's first gate: the sharpness response must be
/// monotonic across it. Seven rungs rather than three because the interesting part of the
/// curve is between 1 and 4 - that is where a photographer's judgement changes - and two
/// rungs either side of a threshold prove nothing about its shape.
#[must_use]
pub fn blur_ladder() -> Vec<Frame> {
    [0usize, 1, 2, 3, 4, 6, 8]
        .into_iter()
        .map(|radius| {
            let mut frame = Frame::base();
            frame.blur_rect(FACE_RECT, radius);
            frame.truth.subject_blur = radius;
            frame
        })
        .collect()
}

/// A frame whose sharp plane is behind or in front of the subject.
///
/// `behind` chooses which. Section 10.1: back and front focus must be correctly *signed*,
/// and the sign convention - negative is front focus - is easy to invert by accident.
#[must_use]
pub fn focus_miss(behind: bool) -> Frame {
    let mut frame = Frame::base();
    frame.blur_rect(FACE_RECT, 4);
    frame.truth.subject_blur = 4;
    // Blur the band that is *not* the sharp one, so that one band stands out rather than
    // both being equally textured.
    let dull = if behind {
        CropRect { x: 0.0, y: 0.62, w: 1.0, h: 0.38 }
    } else {
        CropRect { x: 0.0, y: 0.0, w: 1.0, h: 0.28 }
    };
    frame.blur_rect(dull, 6);
    frame
}

/// A sharp subject against a heavily defocused background.
///
/// Section 13's second acceptance criterion, as a fixture: this frame must never be
/// called soft.
#[must_use]
pub fn shallow_depth_of_field() -> Frame {
    let mut frame = Frame::base();
    frame.blur_rect(CropRect { x: 0.0, y: 0.0, w: 1.0, h: 0.28 }, 9);
    frame.blur_rect(CropRect { x: 0.0, y: 0.62, w: 1.0, h: 0.38 }, 9);
    frame.truth.background_blur = 9;
    frame
}

/// The whole frame smeared along one axis.
#[must_use]
pub fn camera_shake() -> Frame {
    let mut frame = Frame::base();
    frame.smear_rect(CropRect::FULL, 7);
    frame.truth.shaken = true;
    frame
}

/// The background smeared, the subject held.
#[must_use]
pub fn panned() -> Frame {
    let mut frame = Frame::base();
    frame.smear_rect(CropRect { x: 0.0, y: 0.0, w: 1.0, h: 0.28 }, 9);
    frame.smear_rect(CropRect { x: 0.0, y: 0.62, w: 1.0, h: 0.38 }, 9);
    frame.truth.panned = true;
    frame
}

/// A ladder of frames with known noise at rising ISO.
///
/// Section 10.1: the estimated sigma must be within 15 % of the measured one. Returned as
/// `(frame, iso)` because the gate is about the *normalisation* as much as about the
/// estimate: the same sigma at ISO 100 and at ISO 12800 must produce very different
/// relative figures.
#[must_use]
pub fn iso_ladder() -> Vec<(Frame, u32)> {
    [
        (0.006f32, 100u32),
        (0.012, 800),
        (0.024, 3200),
        (0.048, 12800),
    ]
    .into_iter()
    .map(|(sigma, iso)| {
        // A flat frame, not the textured base: the estimator picks the flattest quarter
        // of the tiles, and a fixture whose flattest tiles still carry a checker would be
        // measuring the checker.
        let mut frame = Frame::base();
        frame.blur_rect(CropRect::FULL, 3);
        frame.add_noise(sigma, 0x51A5_0000 + u64::from(iso));
        (frame, iso)
    })
    .collect()
}

/// A frame with genuinely blown highlights over a large area.
#[must_use]
pub fn clipped() -> Frame {
    let mut frame = Frame::base();
    frame.blow_highlights(1.6);
    frame
}

/// A frame with a small bright light source in a dark room.
///
/// Section 10.1: candle and sparkler frames must not be flagged. The flame is 0.6 % of
/// the frame and everything around it is dark, which is exactly what
/// [`crate::integrity::exposure::specular_fraction`] tests for.
#[must_use]
pub fn candle() -> Frame {
    let mut frame = Frame::base();
    frame.crush_shadows(2.2);
    let light = CropRect { x: 0.47, y: 0.10, w: 0.06, h: 0.06 };
    let (x0, y0, x1, y1) = pixels(light, frame.width, frame.height);
    for y in y0..y1 {
        for x in x0..x1 {
            write(&mut frame.rgb, x, y, [255, 255, 250]);
        }
    }
    frame
}

/// Two frames of one scene as a 24 MP and a 61 MP body would render it to a proxy.
///
/// Section 10.1's cross-camera fairness gate. The second is *slightly softer per output
/// pixel*, because a 61 MP frame reduced to 2048 px has averaged more sensor pixels into
/// each proxy pixel than a 24 MP frame reduced to the same width - which is the whole
/// reason `expected_mtf50` falls as resolution rises. A product without the calibration
/// division would score the second frame lower and prefer the cheaper camera.
///
/// Returned as `((frame, make, model), (frame, make, model))` so a test can look both
/// bodies up in the shipped table.
#[must_use]
pub fn cross_camera_pair() -> ((Frame, &'static str, &'static str), (Frame, &'static str, &'static str)) {
    let low = Frame::base();
    let mut high = Frame::base();
    // The shipped table records the a7R IV at 0.27 against the a7 III's 0.34 - a ratio of
    // 0.794 - so the fixture softens the high-resolution frame by exactly the complement
    // of that. **The fixture models the effect; the test proves the calibration division
    // removes it.** A softening chosen to make the test pass would be circular; this one
    // is read off the same table the analyser divides by.
    high.soften(FACE_RECT, 1.0 - 0.27 / 0.34);
    (
        (low, "SONY", "ILCE-7M3"),
        (high, "SONY", "ILCE-7RM4"),
    )
}

/// A group frame with a known number of closed eyes.
///
/// Section 10.1: the closed-eye ratio must match a human count **exactly** on labelled
/// group frames. `faces` people, `closed` of them blinking, arranged in a row so that
/// every face box is the same size and every one of them gates.
#[must_use]
pub fn group_frame(faces: usize, closed: usize) -> Frame {
    let mut frame = Frame::base();
    let count = faces.clamp(1, 8);
    frame.faces.clear();
    let span = 1.0 / count as f32;
    for index in 0..count {
        let rect = CropRect {
            x: index as f32 * span + span * 0.15,
            y: 0.34,
            w: span * 0.70,
            h: 0.26,
        };
        frame.faces.push(face(rect));
    }
    // Paint each face's texture and marker after the boxes exist, so the marker lands on
    // the right eye midpoint for each.
    for index in 0..count {
        let rect = frame.faces.get(index).map_or(FACE_RECT, |found| found.bbox);
        let (x0, y0, x1, y1) = pixels(rect, frame.width, frame.height);
        for y in y0..y1 {
            for x in x0..x1 {
                write(&mut frame.rgb, x, y, [subject_texture(x, y); 3]);
            }
        }
        let state = if index < closed.min(count) {
            EyeOpenness::Closed
        } else {
            EyeOpenness::Open
        };
        frame.paint_eye_marker(index, state);
    }
    frame.truth.closed_faces = closed.min(count);
    frame
}

/// The eye-state reader that replaces the head in every gate.
///
/// Phase 06's `ReferenceDetector` argument, applied to eyes: **this is the only way to
/// state a blink accuracy at all** while the shipped head is a placeholder. It decodes
/// the marker [`Frame::paint_eye_marker`] painted, so its answer is the fixture's answer
/// and the thing under test is everything *around* the head - the alignment, the gating,
/// the four intent rules, the group ratio and the flag decision.
///
/// It is deliberately explicit at the call site rather than a feature flag, so that no
/// product path can reach it by accident.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReferenceEyeReader;

impl ReferenceEyeReader {
    /// Where in the 112 px aligned crop the marker lands.
    ///
    /// The midpoint of the two canonical eye positions, rounded. The alignment maps the
    /// source eye midpoint here by construction, so this reader needs to know nothing
    /// about how the warp was computed - which is what makes it a *reference* rather
    /// than a second copy of the implementation.
    pub const MARKER: (usize, usize) = (56, 52);

    /// Read one aligned crop.
    #[must_use]
    pub fn read(crop: &[u8], side: usize) -> (EyeOpenness, f32) {
        let (x, y) = Self::MARKER;
        if x >= side || y >= side {
            return (EyeOpenness::Open, 0.0);
        }
        let value = crop.get((y * side + x) * 3).copied().unwrap_or(0);
        let slot = (u32::from(value) + 25) / 50;
        let state = EyeOpenness::ALL
            .get(slot.min(4) as usize)
            .copied()
            .unwrap_or(EyeOpenness::Open);
        // A confidence of 0.92 rather than 1.0. A reference reader that answered with
        // perfect certainty would never exercise `eyes::ACT_ON_CLOSED`, and that
        // threshold is one of the things the gate is supposed to be testing.
        (state, 0.92)
    }
}

/// The marker level for one eye state.
#[must_use]
pub fn marker_level(state: EyeOpenness) -> u8 {
    let index = EyeOpenness::ALL
        .iter()
        .position(|candidate| *candidate == state)
        .unwrap_or(0);
    u8::try_from(index * 50).unwrap_or(0)
}

/// A `FaceRef` for one rectangle, with eyes where a face's eyes are.
///
/// The eye landmarks sit at 30 % and 70 % of the box's width and 38 % of its height,
/// which is where they are on a face. The face id is derived from the rectangle so that
/// two calls with the same box produce the same id - invariant 4 applied to a fixture.
#[must_use]
pub fn face(bbox: CropRect) -> FaceRef {
    let quantise = |value: f32| (value * 10_000.0).round() as i32;
    let mut hasher = blake3_of(&[
        quantise(bbox.x),
        quantise(bbox.y),
        quantise(bbox.w),
        quantise(bbox.h),
    ]);
    if let Some(slot) = hasher.get_mut(6) {
        *slot = (*slot & 0x0f) | 0x80;
    }
    if let Some(slot) = hasher.get_mut(8) {
        *slot = (*slot & 0x3f) | 0x80;
    }
    FaceRef {
        face_id: FaceId::from_uuid(Uuid::from_bytes(hasher)),
        identity_id: None,
        bbox,
        eyes: [
            [bbox.x + bbox.w * 0.30, bbox.y + bbox.h * 0.38],
            [bbox.x + bbox.w * 0.70, bbox.y + bbox.h * 0.38],
        ],
        area_frac: bbox.w * bbox.h,
        centrality: 1.0 - ((bbox.x + bbox.w / 2.0) - 0.5).abs() * 2.0,
        sharpness: 0.8,
        quality: 0.8,
        votes: true,
    }
}

/// Sixteen deterministic bytes from four integers.
///
/// A tiny xorshift rather than a hash crate: `aura-brain-photo` has no `blake3`
/// dependency, this is a fixture id and not a content address, and adding a dependency to
/// a test helper is how a crate's dependency list stops describing what it does.
fn blake3_of(values: &[i32; 4]) -> [u8; 16] {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for value in values {
        state ^= u64::from(value.unsigned_abs()).wrapping_mul(0x0100_0000_01B3);
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
    }
    let mut out = [0u8; 16];
    for (index, slot) in out.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *slot = ((state >> (8 * (index % 8))) & 0xFF) as u8;
    }
    out
}

/// The subject's three-frequency texture at one pixel.
///
/// Periods of four, eight and sixteen pixels, at falling amplitude, on a mid-grey base.
/// A blur of radius one removes the first, radius three the second, radius six the third,
/// so the ladder degrades in three visible steps rather than falling off a cliff - and
/// the contrast never collapses to the point where the acutance ratio is a division of
/// two small numbers.
#[must_use]
pub fn subject_texture(x: usize, y: usize) -> u8 {
    let fine: f32 = if (x / 2 + y / 2) % 2 == 0 { 26.0 } else { -26.0 };
    let mid: f32 = if (x / 4 + y / 4) % 2 == 0 { 20.0 } else { -20.0 };
    let coarse: f32 = if (x / 8 + y / 8) % 2 == 0 { 14.0 } else { -14.0 };
    (128.0 + fine + mid + coarse).clamp(0.0, 255.0) as u8
}

/// A normalised rectangle in pixels, clamped.
fn pixels(rect: CropRect, width: usize, height: usize) -> (usize, usize, usize, usize) {
    let rect = rect.clamped();
    let scale = |value: f32, span: usize| -> usize {
        ((value * span as f32).round().max(0.0) as usize).min(span)
    };
    (
        scale(rect.x, width),
        scale(rect.y, height),
        scale(rect.x + rect.w, width),
        scale(rect.y + rect.h, height),
    )
}

fn write(rgb: &mut [u8], x: usize, y: usize, value: [u8; 3]) {
    let base = (y * SIDE + x) * 3;
    for (channel, sample) in value.into_iter().enumerate() {
        if let Some(slot) = rgb.get_mut(base + channel) {
            *slot = sample;
        }
    }
}
