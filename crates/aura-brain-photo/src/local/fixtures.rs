//! The synthetic ground truth every section 10.1 gate is measured against.
//!
//! **Nothing here is a photograph.** Each fixture is a frame whose answer was *chosen first*
//! and then painted into the pixels: a face at a known luminance under a known background, a
//! group whose spread is known by construction, a specular patch of a known size at a known
//! desaturation. What that proves is the arithmetic. It says nothing about a wedding, and
//! `docs/progress/PHASE-19-EXIT.md` condition C1 says so in the exit report rather than in a
//! footnote.
//!
//! It also, for the first time in this crate, has to paint a **mask**. Phase 18 owns masks and
//! this phase has no generator - so the fixtures supply [`MaskField`]s directly, exactly as a
//! phase 18 build would. That is the contract-first handoff of the phase ritual's step 4: a
//! lane consumes another lane's work through the frozen interface, using a fixture until the
//! real implementation lands. The masks here are *perfect* - confidence one, edge quality one,
//! aligned exactly with the painted regions - which is the best case, and
//! [`Frame::with_mask_quality`] is how the gates measure what happens when they are not.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::{MaskField, MaskKind};
use aura_core::contract::people::FaceRef;
use aura_core::{FaceId, IdentityId, SceneId};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use uuid::Uuid;

use crate::local::plan::FrameContext;

/// The side of every fixture frame, in pixels.
///
/// Three hundred and eighty-four: large enough that a face box covering a fifth of it is
/// 76 px across, which is above the shapeable floor and wide enough for the frequency
/// separation to have something to separate, and small enough that a gate over a dozen
/// fixtures runs in well under a second.
pub const SIDE: usize = 384;

/// One painted frame and the answer it was painted from.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The pixels.
    pub buffer: PixelBuffer,
    /// Everything the analyser is told about it.
    pub context: FrameContext,
    /// What the faces' mean luminances were painted at, in the same order as the faces.
    pub face_luma: Vec<f32>,
    /// What the background's mean luminance was painted at.
    pub background_luma: f32,
    /// A human name, for a failing gate's message.
    pub name: &'static str,
}

impl Frame {
    /// Weaken every mask, to measure section 6.4's confidence scaling.
    #[must_use]
    pub fn with_mask_quality(mut self, confidence: f32, edge_quality: f32) -> Self {
        for mask in &mut self.context.masks {
            mask.confidence = confidence;
            mask.edge_quality = edge_quality;
        }
        self
    }

    /// Remove every mask, to measure what a build with no phase 18 does.
    #[must_use]
    pub fn without_masks(mut self) -> Self {
        self.context.masks.clear();
        self
    }

    /// Set the scene, to measure the per-scene policy.
    #[must_use]
    pub fn in_scene(mut self, scene: SceneId) -> Self {
        self.context.scene = scene;
        self
    }

    /// Set the measured noise, to measure the dynamic cap.
    #[must_use]
    pub fn with_noise(mut self, noise: f32) -> Self {
        self.context.noise = noise;
        self
    }
}

/// A painter over one fixture frame.
#[derive(Debug, Clone)]
struct Canvas {
    rgb: Vec<u8>,
}

impl Canvas {
    fn new(luma: f32) -> Self {
        let value = encode(luma);
        Self {
            rgb: vec![value; SIDE * SIDE * 3],
        }
    }

    /// Fill a rectangle with a neutral of this perceptual luminance.
    fn fill(&mut self, rect: CropRect, luma: f32) {
        self.fill_rgb(rect, [encode(luma), encode(luma), encode(luma)]);
    }

    /// Fill a rectangle with a warm skin-like tone at this luminance.
    ///
    /// Chromatic on purpose: the shine detector's second condition is that sheen is *less*
    /// saturated than the skin around it, and a grey face would make that test vacuous.
    fn fill_skin(&mut self, rect: CropRect, luma: f32) {
        let base = f32::from(encode(luma));
        self.fill_rgb(
            rect,
            [
                (base * 1.10).min(255.0) as u8,
                base as u8,
                (base * 0.80) as u8,
            ],
        );
    }

    fn fill_rgb(&mut self, rect: CropRect, colour: [u8; 3]) {
        let rect = rect.clamped();
        let x0 = (rect.x * SIDE as f32) as usize;
        let y0 = (rect.y * SIDE as f32) as usize;
        let x1 = (((rect.x + rect.w) * SIDE as f32) as usize).min(SIDE);
        let y1 = (((rect.y + rect.h) * SIDE as f32) as usize).min(SIDE);
        for y in y0..y1 {
            for x in x0..x1 {
                let index = (y * SIDE + x) * 3;
                if let Some(slot) = self.rgb.get_mut(index..index + 3) {
                    slot.copy_from_slice(&colour);
                }
            }
        }
    }

    /// Paint a horizontal luminance ramp, so the frequency separation has form to find.
    fn ramp(&mut self, rect: CropRect, from: f32, to: f32) {
        let rect = rect.clamped();
        let x0 = (rect.x * SIDE as f32) as usize;
        let y0 = (rect.y * SIDE as f32) as usize;
        let x1 = (((rect.x + rect.w) * SIDE as f32) as usize).min(SIDE);
        let y1 = (((rect.y + rect.h) * SIDE as f32) as usize).min(SIDE);
        let span = (x1.saturating_sub(x0)).max(1) as f32;
        for y in y0..y1 {
            for x in x0..x1 {
                let t = (x - x0) as f32 / span;
                let luma = from + (to - from) * t;
                let base = f32::from(encode(luma));
                let index = (y * SIDE + x) * 3;
                if let Some(slot) = self.rgb.get_mut(index..index + 3) {
                    slot.copy_from_slice(&[
                        (base * 1.10).min(255.0) as u8,
                        base as u8,
                        (base * 0.80) as u8,
                    ]);
                }
            }
        }
    }

    fn finish(self) -> PixelBuffer {
        PixelBuffer {
            width: SIDE as u32,
            height: SIDE as u32,
            data: PixelData::Srgb8(self.rgb),
            colour_space: ColourSpace::Srgb,
            source: PixelSource::Demosaiced,
            decode_ms: 0,
        }
    }
}

/// The 8-bit encoding of a perceptual luminance.
fn encode(luma: f32) -> u8 {
    (luma.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A stable UUID for a fixture, so a re-run addresses the same face.
///
/// Invariant 4. A random id here would make every fixture's plan differ between runs in the
/// one field a `BTreeMap` orders by, and a determinism gate would fail for a reason that has
/// nothing to do with the arithmetic it was written to check.
fn deterministic_uuid(domain: u8, ordinal: u8) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = domain;
    bytes[15] = ordinal;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// A mask field that exactly covers one rectangle.
///
/// Perfect by construction - confidence one, edge quality one - which is what a fixture should
/// supply: the gates measure the *arithmetic*, and a fixture with a deliberately ragged mask
/// would measure the mask instead. [`Frame::with_mask_quality`] is how a gate asks the other
/// question.
#[must_use]
pub fn mask_over(kind: MaskKind, rects: &[CropRect], identity: Option<IdentityId>) -> MaskField {
    const GRID: u16 = 64;
    let mut alpha = vec![0u8; usize::from(GRID) * usize::from(GRID)];
    for rect in rects {
        let rect = rect.clamped();
        let x0 = (rect.x * f32::from(GRID)) as usize;
        let y0 = (rect.y * f32::from(GRID)) as usize;
        let x1 = (((rect.x + rect.w) * f32::from(GRID)) as usize).min(usize::from(GRID));
        let y1 = (((rect.y + rect.h) * f32::from(GRID)) as usize).min(usize::from(GRID));
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(slot) = alpha.get_mut(y * usize::from(GRID) + x) {
                    *slot = 255;
                }
            }
        }
    }
    let bounds = rects.first().copied().unwrap_or(CropRect::FULL);
    MaskField {
        kind,
        identity,
        bounds,
        width: GRID,
        height: GRID,
        alpha,
        confidence: 1.0,
        edge_quality: 1.0,
        model_ver: 1,
    }
}

/// The complement of a set of rectangles, as a background field.
#[must_use]
pub fn background_mask(rects: &[CropRect]) -> MaskField {
    let mut field = mask_over(MaskKind::Background, &[CropRect::FULL], None);
    for rect in rects {
        let rect = rect.clamped();
        let x0 = (rect.x * f32::from(field.width)) as usize;
        let y0 = (rect.y * f32::from(field.height)) as usize;
        let x1 =
            (((rect.x + rect.w) * f32::from(field.width)) as usize).min(usize::from(field.width));
        let y1 =
            (((rect.y + rect.h) * f32::from(field.height)) as usize).min(usize::from(field.height));
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(slot) = field.alpha.get_mut(y * usize::from(field.width) + x) {
                    *slot = 0;
                }
            }
        }
    }
    field
}

/// A face reference with plausible landmarks over a box.
#[must_use]
pub fn face_at(bbox: CropRect, identity: Option<IdentityId>, ordinal: u8) -> FaceRef {
    let eye_y = bbox.y + bbox.h * 0.42;
    FaceRef {
        face_id: FaceId::from_uuid(deterministic_uuid(0x19, ordinal)),
        identity_id: identity,
        bbox,
        eyes: [
            [bbox.x + bbox.w * 0.32, eye_y],
            [bbox.x + bbox.w * 0.68, eye_y],
        ],
        area_frac: bbox.w * bbox.h,
        centrality: 0.8,
        sharpness: 0.8,
        quality: 0.8,
        votes: true,
    }
}

// ---------------------------------------------------------------------------
// The fixtures
// ---------------------------------------------------------------------------

/// A single face, correctly lit, against a calm background.
///
/// **The frame nothing should happen to.** Section 0's mission is invisible editing, and the
/// first thing an invisible editor has to get right is leaving a good photograph alone.
#[must_use]
pub fn already_right() -> Frame {
    let face = CropRect {
        x: 0.38,
        y: 0.22,
        w: 0.24,
        h: 0.30,
    };
    let mut canvas = Canvas::new(0.46);
    canvas.fill_skin(face, 0.48);
    let mut context = FrameContext::new(SceneId::CouplePortrait);
    context.band = 0.48;
    context.faces = vec![face_at(face, None, 0)];
    context.masks = vec![
        mask_over(MaskKind::Face, &[face], None),
        mask_over(MaskKind::Subject, &[face], None),
        background_mask(&[face]),
        mask_over(MaskKind::Skin, &[face], None),
    ];
    Frame {
        buffer: canvas.finish(),
        context,
        face_luma: vec![0.48],
        background_luma: 0.46,
        name: "already_right",
    }
}

/// A face two stops down under a mandap, against a background that is not competing.
///
/// The frame section 1 exists for.
#[must_use]
pub fn face_in_shadow() -> Frame {
    let face = CropRect {
        x: 0.38,
        y: 0.22,
        w: 0.24,
        h: 0.30,
    };
    let mut canvas = Canvas::new(0.30);
    canvas.fill_skin(face, 0.18);
    let mut context = FrameContext::new(SceneId::Ceremony);
    context.band = 0.48;
    context.faces = vec![face_at(face, None, 0)];
    context.masks = vec![
        mask_over(MaskKind::Face, &[face], None),
        mask_over(MaskKind::Subject, &[face], None),
        background_mask(&[face]),
        mask_over(MaskKind::Skin, &[face], None),
    ];
    Frame {
        buffer: canvas.finish(),
        context,
        face_luma: vec![0.18],
        background_luma: 0.30,
        name: "face_in_shadow",
    }
}

/// A subject against a window three stops brighter than they are.
///
/// The frame section 6.2 exists for, and the one the mean-luminance criterion is written
/// against.
#[must_use]
pub fn bright_window() -> Frame {
    let face = CropRect {
        x: 0.20,
        y: 0.24,
        w: 0.20,
        h: 0.26,
    };
    let subject = CropRect {
        x: 0.14,
        y: 0.20,
        w: 0.32,
        h: 0.72,
    };
    let mut canvas = Canvas::new(0.86);
    canvas.fill_skin(subject, 0.34);
    canvas.fill_skin(face, 0.36);
    let mut context = FrameContext::new(SceneId::GettingReadyBride);
    context.band = 0.52;
    context.faces = vec![face_at(face, None, 0)];
    context.masks = vec![
        mask_over(MaskKind::Face, &[face], None),
        mask_over(MaskKind::Subject, &[subject], None),
        background_mask(&[subject]),
        mask_over(MaskKind::Skin, &[face], None),
    ];
    Frame {
        buffer: canvas.finish(),
        context,
        face_luma: vec![0.36],
        background_luma: 0.86,
        name: "bright_window",
    }
}

/// A family formal where one person is two stops down on everybody else.
///
/// Section 10.1's group-fairness criterion, on the frame it was written for.
#[must_use]
pub fn uneven_group() -> Frame {
    let boxes: Vec<CropRect> = (0..4)
        .map(|i| CropRect {
            x: 0.06 + 0.23 * i as f32,
            y: 0.30,
            w: 0.16,
            h: 0.22,
        })
        .collect();
    let lumas = [0.16f32, 0.46, 0.50, 0.48];
    let mut canvas = Canvas::new(0.40);
    for (rect, luma) in boxes.iter().zip(lumas.iter()) {
        canvas.fill_skin(*rect, *luma);
    }
    let mut context = FrameContext::new(SceneId::FamilyPortrait);
    context.band = 0.50;
    context.faces = boxes
        .iter()
        .enumerate()
        .map(|(index, rect)| {
            face_at(
                *rect,
                Some(IdentityId::from_uuid(deterministic_uuid(0x1A, index as u8))),
                index as u8,
            )
        })
        .collect();
    let mut masks = vec![
        mask_over(MaskKind::Face, &boxes, None),
        mask_over(MaskKind::Subject, &boxes, None),
        background_mask(&boxes),
        mask_over(MaskKind::Skin, &boxes, None),
    ];
    masks.shrink_to_fit();
    context.masks = masks;
    Frame {
        buffer: canvas.finish(),
        context,
        face_luma: lumas.to_vec(),
        background_luma: 0.40,
        name: "uneven_group",
    }
}

/// A face with a small, bright, desaturated patch on the forehead.
///
/// The frame section 6.3's shine control exists for.
#[must_use]
pub fn shiny_forehead() -> Frame {
    let face = CropRect {
        x: 0.34,
        y: 0.18,
        w: 0.30,
        h: 0.38,
    };
    let sheen = CropRect {
        x: 0.44,
        y: 0.22,
        w: 0.08,
        h: 0.06,
    };
    let mut canvas = Canvas::new(0.42);
    canvas.fill_skin(face, 0.52);
    // Bright and near-neutral: the light's own colour rather than the skin's.
    canvas.fill(sheen, 0.97);
    let mut context = FrameContext::new(SceneId::Speeches);
    context.band = 0.50;
    context.faces = vec![face_at(face, None, 0)];
    context.masks = vec![
        mask_over(MaskKind::Face, &[face], None),
        mask_over(MaskKind::Subject, &[face], None),
        background_mask(&[face]),
        mask_over(MaskKind::Skin, &[face], None),
    ];
    Frame {
        buffer: canvas.finish(),
        context,
        face_luma: vec![0.52],
        background_luma: 0.42,
        name: "shiny_forehead",
    }
}

/// A large face lit from one side, with a real luminance gradient across it.
///
/// The frame the dodge-and-burn geometry is measured on: the light direction is known by
/// construction because the ramp was painted from dark to light, left to right.
#[must_use]
pub fn modelled_face() -> Frame {
    let face = CropRect {
        x: 0.30,
        y: 0.14,
        w: 0.40,
        h: 0.50,
    };
    let mut canvas = Canvas::new(0.34);
    canvas.ramp(face, 0.32, 0.62);
    let mut context = FrameContext::new(SceneId::CouplePortrait);
    context.band = 0.48;
    context.faces = vec![face_at(face, None, 0)];
    context.masks = vec![
        mask_over(MaskKind::Face, &[face], None),
        mask_over(MaskKind::Subject, &[face], None),
        background_mask(&[face]),
        mask_over(MaskKind::Skin, &[face], None),
    ];
    Frame {
        buffer: canvas.finish(),
        context,
        face_luma: vec![0.47],
        background_luma: 0.34,
        name: "modelled_face",
    }
}

/// A dark, saturated dance-floor frame with one small face.
///
/// The scene section 6.4 names as minimal shaping, and the frame that proves the policy table
/// reaches the arithmetic.
#[must_use]
pub fn dance_floor() -> Frame {
    let face = CropRect {
        x: 0.45,
        y: 0.40,
        w: 0.09,
        h: 0.12,
    };
    let mut canvas = Canvas::new(0.16);
    canvas.fill_rgb(
        CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 0.35,
        },
        [128, 24, 180],
    );
    canvas.fill_skin(face, 0.24);
    let mut context = FrameContext::new(SceneId::DanceFloor);
    context.band = 0.35;
    context.faces = vec![face_at(face, None, 0)];
    context.masks = vec![
        mask_over(MaskKind::Face, &[face], None),
        mask_over(MaskKind::Subject, &[face], None),
        background_mask(&[face]),
        mask_over(MaskKind::Skin, &[face], None),
    ];
    Frame {
        buffer: canvas.finish(),
        context,
        face_luma: vec![0.24],
        background_luma: 0.16,
        name: "dance_floor",
    }
}

/// Every fixture, in documentation order.
#[must_use]
pub fn all() -> Vec<Frame> {
    vec![
        already_right(),
        face_in_shadow(),
        bright_window(),
        uneven_group(),
        shiny_forehead(),
        modelled_face(),
        dance_floor(),
    ]
}
