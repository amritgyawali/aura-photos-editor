//! The synthetic ground truth every section 10.1 gate is measured against.
//!
//! PHASE-21 has no labelled corpus - section 9 gives DATA a seven-day task to build one and there
//! is none in this repository - so every fixture here is a frame whose flyaways, glare sheet,
//! lint, teeth colour and iris detail were **painted into the pixels** and are read back through
//! the real detectors, the real operators and the real renderer.
//!
//! That proves the arithmetic. It says nothing about a wedding photograph, and
//! `docs/progress/PHASE-21-EXIT.md` says so as condition C1.
//!
//! ## What each fixture is for
//!
//! | Fixture | What it proves |
//! |---|---|
//! | [`planned_frame`] | the whole pass end to end, on a frame with something of each kind |
//! | [`flyaway_frame`] | a strand over a quiet background is calmed and one over foliage is not |
//! | [`catchlight_frame`] | the guard withdraws the eye family when a catchlight would dull |
//! | [`glare_frame`] | a blown sheet is found, and its sibling repairs it |
//! | [`ceiling_frame`] | every contract ceiling refuses an attempt to exceed it |
//!
//! Every fixture is built from arithmetic rather than from a stored image, so the repository
//! carries no pixels and the fixtures cannot drift from the code that reads them.

use std::collections::BTreeMap;

use aura_core::contract::integrity::CropRect;
use aura_core::contract::micro::{ImageId, MicroField, MicroRegion};
use aura_core::contract::people::FaceRef;
use aura_core::{FaceId, IdentityId, PhotoId, SceneId};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

use crate::micro::ops::{MicroFrame, Sibling};

/// The side of every synthetic frame, in pixels.
///
/// 256 rather than 128, and the reason is worth stating because it caught a real defect. Several
/// contract ceilings - `MAX_FLYAWAY_AREA`, `MAX_CLOTHING_AREA`, `MAX_BORROW_AREA` - are fractions
/// of the **frame**, while the things they bound are a few pixels across at any resolution. On a
/// small fixture a perfectly ordinary strand or lint is a large fraction of the frame and every
/// operation is refused, so the fixture ends up testing itself rather than the detector. 256 is
/// the smallest side at which the real proportions fit.
pub const SIDE: usize = 256;

/// The grid side of every synthetic region field.
pub const FIELD_SIDE: u16 = 128;

/// The frame's neutral, D65 in `u'v'`. What phase 15 would have measured on daylight.
pub const NEUTRAL: [f32; 2] = [0.1978, 0.4683];

/// A stable photo id for a fixture.
#[must_use]
pub fn photo(tag: u8) -> ImageId {
    let text = format!("pht_00000000-0000-4000-8000-0000000001{tag:02}");
    PhotoId::from_db(&text).unwrap_or_else(|_| PhotoId::new())
}

/// A stable identity id for a fixture.
#[must_use]
pub fn identity(tag: u8) -> IdentityId {
    let text = format!("idt_00000000-0000-4000-8000-0000000001{tag:02}");
    IdentityId::from_db(&text).unwrap_or_else(|_| IdentityId::new())
}

/// One face filling most of the frame, with landmarks.
#[must_use]
pub fn face() -> FaceRef {
    FaceRef {
        face_id: FaceId::new(),
        identity_id: Some(identity(1)),
        bbox: CropRect {
            x: 0.20,
            y: 0.15,
            w: 0.60,
            h: 0.70,
        },
        eyes: [[0.38, 0.38], [0.62, 0.38]],
        area_frac: 0.42,
        centrality: 0.95,
        sharpness: 0.85,
        quality: 0.85,
        votes: true,
    }
}

/// A painter over one synthetic frame.
#[derive(Debug, Clone)]
pub struct Canvas {
    /// Interleaved linear RGB.
    pub rgb: Vec<f32>,
    /// Side in pixels.
    pub side: usize,
}

impl Canvas {
    /// A frame filled with one linear value.
    #[must_use]
    pub fn filled(value: [f32; 3]) -> Self {
        let mut rgb = Vec::with_capacity(SIDE * SIDE * 3);
        for _ in 0..SIDE * SIDE {
            rgb.extend_from_slice(&value);
        }
        Self { rgb, side: SIDE }
    }

    /// Paint one rectangle.
    pub fn rect(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, value: [f32; 3]) {
        for y in y0..y1.min(self.side) {
            for x in x0..x1.min(self.side) {
                let slot = (y * self.side + x) * 3;
                for channel in 0..3 {
                    if let (Some(target), Some(source)) =
                        (self.rgb.get_mut(slot + channel), value.get(channel))
                    {
                        *target = *source;
                    }
                }
            }
        }
    }

    /// Paint a deterministic speckle, for a background nothing may be calmed against.
    pub fn speckle(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, value: [f32; 3]) {
        for y in y0..y1.min(self.side) {
            for x in x0..x1.min(self.side) {
                if (x * 7 + y * 3) % 5 != 0 {
                    continue;
                }
                self.rect(x, y, x + 1, y + 1, value);
            }
        }
    }

    /// Turn the canvas into a proxy buffer at [`crate::micro::ops::MICRO_LEVEL`].
    #[must_use]
    pub fn buffer(&self) -> PixelBuffer {
        let mut values = Vec::with_capacity(self.rgb.len());
        for value in &self.rgb {
            values.push(aura_raw::colour::curve::scene_to_linear_u16(*value));
        }
        PixelBuffer {
            width: self.side as u32,
            height: self.side as u32,
            data: PixelData::Linear16(values),
            colour_space: ColourSpace::LinearRec2020,
            source: PixelSource::Demosaiced,
            decode_ms: 0,
        }
    }
}

/// A region field covering one rectangle of the frame, at full confidence.
#[must_use]
pub fn field(region: MicroRegion, bounds: CropRect) -> MicroField {
    let mut alpha = vec![0u8; usize::from(FIELD_SIDE) * usize::from(FIELD_SIDE)];
    let x0 = (bounds.x * f32::from(FIELD_SIDE)).round().max(0.0) as usize;
    let y0 = (bounds.y * f32::from(FIELD_SIDE)).round().max(0.0) as usize;
    let x1 = (((bounds.x + bounds.w) * f32::from(FIELD_SIDE)).round() as usize)
        .min(usize::from(FIELD_SIDE));
    let y1 = (((bounds.y + bounds.h) * f32::from(FIELD_SIDE)).round() as usize)
        .min(usize::from(FIELD_SIDE));
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(slot) = alpha.get_mut(y * usize::from(FIELD_SIDE) + x) {
                *slot = 255;
            }
        }
    }
    MicroField {
        region,
        identity: Some(identity(1)),
        bounds: aura_core::contract::composition::Box2 {
            x: bounds.x,
            y: bounds.y,
            w: bounds.w,
            h: bounds.h,
        },
        width: FIELD_SIDE,
        height: FIELD_SIDE,
        alpha,
        confidence: 0.95,
        edge_quality: 0.90,
        model_ver: 1,
    }
}

/// A region field with a soft boundary: full inside, a halo band around it.
///
/// The hair case. A hard field would leave no halo for [`crate::micro::hair`] to look in, and a
/// fixture with no halo would make the flyaway detector look broken rather than the fixture.
#[must_use]
pub fn haired_field(bounds: CropRect, halo: f32) -> MicroField {
    let mut base = field(MicroRegion::Hair, bounds);
    let side = usize::from(FIELD_SIDE);
    let band = ((halo * f32::from(FIELD_SIDE)).round() as usize).max(1);
    let x1 = (((bounds.x + bounds.w) * f32::from(FIELD_SIDE)).round() as usize).min(side);
    let y0 = (bounds.y * f32::from(FIELD_SIDE)).round().max(0.0) as usize;
    let y1 = (((bounds.y + bounds.h) * f32::from(FIELD_SIDE)).round() as usize).min(side);
    for y in y0..y1 {
        for x in x1..(x1 + band).min(side) {
            if let Some(slot) = base.alpha.get_mut(y * side + x) {
                // Inside `hair::OUTSIDE_MIN ..= hair::OUTSIDE_MAX`: the halo the detector looks in.
                *slot = 76;
            }
        }
    }
    base
}

/// The end-to-end fixture: one face, hair with a stray strand, teeth, eyes, and a lint on a lapel.
///
/// Every geometry here is at the same *proportion* it would be on a real 2048 px proxy, which is
/// what makes the contract's frame-fraction ceilings reachable. See [`SIDE`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn planned_frame() -> (ImageId, PixelBuffer, MicroFrame) {
    let mut canvas = Canvas::filled([0.34, 0.35, 0.36]);

    // Hair mass on the left, a narrow matte halo beside it, quiet background beyond. It stops
    // above the lapel: a hair matte that ran down over a jacket would put the boundary between
    // two garments inside the halo, and the detector would read that step as a strand.
    canvas.rect(0, 0, 80, 220, [0.05, 0.045, 0.05]);
    // One bright stray strand in the halo: one pixel wide, twenty tall, **above the face
    // rectangle painted below**. Painting it lower put it underneath the skin and the fixture
    // silently stopped containing a flyaway at all - which the end-to-end test is what caught.
    canvas.rect(83, 12, 84, 32, [0.74, 0.72, 0.70]);

    // A face's skin, bright enough that the teeth clamp has headroom.
    canvas.rect(60, 48, 200, 220, [0.44, 0.34, 0.29]);
    // Teeth: strongly yellow - the blue channel is a quarter of the red - and uneven across the
    // row. Two thirds sit in shadow at the back of the mouth and a third catch the light, so the
    // median and the upper quartile differ and the evening half has something to do. An evenly
    // painted row would have median == upper and would silently test nothing.
    canvas.rect(112, 144, 148, 160, [0.40, 0.35, 0.10]);
    canvas.rect(136, 144, 148, 160, [0.52, 0.46, 0.14]);
    // Sclera: visibly bloodshot - far enough outside the locus that the redness half has
    // something to remove. Iris: flat, with a catchlight in it.
    canvas.rect(88, 92, 116, 112, [0.72, 0.30, 0.31]);
    canvas.rect(148, 92, 176, 112, [0.72, 0.30, 0.31]);
    canvas.rect(94, 96, 110, 108, [0.20, 0.19, 0.20]);
    canvas.rect(154, 96, 170, 108, [0.20, 0.19, 0.20]);
    canvas.rect(100, 100, 104, 104, [1.30, 1.30, 1.30]);
    canvas.rect(160, 100, 164, 104, [1.30, 1.30, 1.30]);

    // A lapel with one piece of lint on it.
    canvas.rect(60, 224, 200, SIDE, [0.08, 0.08, 0.09]);
    canvas.rect(120, 236, 124, 240, [0.58, 0.58, 0.58]);

    let mut regions = BTreeMap::new();
    // The hair field is full inside the mass and a narrow halo band outside it - the shape phase
    // 18's trimap produces. Without the halo there is nowhere for a flyaway to live.
    regions.insert(
        MicroRegion::Hair,
        haired_field(
            CropRect {
                x: 0.0,
                y: 0.0,
                w: 0.3125,
                h: 0.8594,
            },
            0.027,
        ),
    );
    regions.insert(
        MicroRegion::Skin,
        field(
            MicroRegion::Skin,
            CropRect {
                x: 0.2344,
                y: 0.1875,
                w: 0.5469,
                h: 0.6719,
            },
        ),
    );
    regions.insert(
        MicroRegion::Teeth,
        field(
            MicroRegion::Teeth,
            CropRect {
                x: 0.4375,
                y: 0.5625,
                w: 0.1406,
                h: 0.0625,
            },
        ),
    );
    regions.insert(
        MicroRegion::Sclera,
        field(
            MicroRegion::Sclera,
            CropRect {
                x: 0.3437,
                y: 0.3594,
                w: 0.1094,
                h: 0.0781,
            },
        ),
    );
    regions.insert(
        MicroRegion::Iris,
        field(
            MicroRegion::Iris,
            CropRect {
                x: 0.3672,
                y: 0.375,
                w: 0.0625,
                h: 0.0469,
            },
        ),
    );
    regions.insert(
        MicroRegion::Eyes,
        field(
            MicroRegion::Eyes,
            CropRect {
                x: 0.3437,
                y: 0.3594,
                w: 0.3438,
                h: 0.0781,
            },
        ),
    );
    regions.insert(
        MicroRegion::Clothing,
        field(
            MicroRegion::Clothing,
            CropRect {
                x: 0.2344,
                y: 0.875,
                w: 0.5469,
                h: 0.125,
            },
        ),
    );

    let context = MicroFrame {
        scene: SceneId::CouplePortrait,
        faces: vec![face()],
        regions,
        neutral: Some(NEUTRAL),
        ..MicroFrame::new(SceneId::CouplePortrait)
    };

    (photo(1), canvas.buffer(), context)
}

/// A frame whose only content is a stray strand, over a background of a chosen busyness.
///
/// `busy = false` is the case that must be calmed; `busy = true` is the case that must not be.
#[must_use]
pub fn flyaway_frame(busy: bool) -> (ImageId, PixelBuffer, MicroFrame) {
    let mut canvas = Canvas::filled([0.40, 0.41, 0.40]);
    canvas.rect(0, 0, 80, SIDE, [0.05, 0.045, 0.05]);
    if busy {
        canvas.speckle(87, 0, SIDE, SIDE, [0.90, 0.88, 0.85]);
    }
    canvas.rect(83, 12, 84, 32, [0.78, 0.76, 0.74]);

    let mut regions = BTreeMap::new();
    regions.insert(
        MicroRegion::Hair,
        haired_field(
            CropRect {
                x: 0.0,
                y: 0.0,
                w: 0.3125,
                h: 1.0,
            },
            0.027,
        ),
    );

    let context = MicroFrame {
        scene: SceneId::CouplePortrait,
        faces: vec![face()],
        regions,
        neutral: Some(NEUTRAL),
        allowed: [true, false, false, false, false],
        ..MicroFrame::new(SceneId::CouplePortrait)
    };
    (photo(2), canvas.buffer(), context)
}

/// A frame with a catchlight in each iris, for the guard's specular-pixel test.
#[must_use]
pub fn catchlight_frame() -> (ImageId, PixelBuffer, MicroFrame) {
    let mut canvas = Canvas::filled([0.30, 0.30, 0.31]);
    canvas.rect(60, 48, 200, 220, [0.44, 0.34, 0.29]);
    canvas.rect(88, 92, 116, 112, [0.66, 0.46, 0.47]);
    canvas.rect(94, 96, 110, 108, [0.18, 0.17, 0.19]);
    canvas.rect(100, 100, 104, 104, [1.60, 1.60, 1.60]);

    let mut regions = BTreeMap::new();
    regions.insert(
        MicroRegion::Sclera,
        field(
            MicroRegion::Sclera,
            CropRect {
                x: 0.3437,
                y: 0.3594,
                w: 0.1094,
                h: 0.0781,
            },
        ),
    );
    regions.insert(
        MicroRegion::Iris,
        field(
            MicroRegion::Iris,
            CropRect {
                x: 0.3672,
                y: 0.375,
                w: 0.0625,
                h: 0.0469,
            },
        ),
    );

    let context = MicroFrame {
        scene: SceneId::CouplePortrait,
        faces: vec![face()],
        regions,
        neutral: Some(NEUTRAL),
        allowed: [false, false, true, false, false],
        ..MicroFrame::new(SceneId::CouplePortrait)
    };
    (photo(3), canvas.buffer(), context)
}

/// A frame with a blown glare sheet over one lens, and a sibling of the same instant without one.
///
/// The sibling is built from the same arithmetic with the sheet left off, which is what makes the
/// ring around the destroyed region correlate: two frames of one burst differ by a little motion
/// and a little noise, not by their content.
#[must_use]
pub fn glare_frame() -> (ImageId, PixelBuffer, MicroFrame) {
    let paint = |sheet: bool| {
        let mut canvas = Canvas::filled([0.30, 0.30, 0.31]);
        canvas.rect(60, 48, 200, 220, [0.44, 0.34, 0.29]);
        // Structure around the lens, so the correlation ring has something to agree about.
        for y in 76..140 {
            for x in 76..200 {
                let value = 0.30
                    + 0.16 * ((x as f32 * 0.37).sin() * (y as f32 * 0.29).cos())
                    + 0.05 * ((x as f32 * 0.13 + y as f32 * 0.11).sin());
                canvas.rect(x, y, x + 1, y + 1, [value, value * 0.82, value * 0.74]);
            }
        }
        // The eye under the lens.
        canvas.rect(88, 92, 116, 112, [0.62, 0.46, 0.46]);
        canvas.rect(94, 96, 110, 108, [0.16, 0.15, 0.17]);
        canvas.rect(100, 100, 104, 104, [1.40, 1.40, 1.40]);
        if sheet {
            // A clipped sheet across part of the lens: ten by eight, which is above the sheet
            // floor and inside `MAX_BORROW_AREA`. The record inside it is gone.
            canvas.rect(96, 94, 106, 102, [1.60, 1.60, 1.60]);
        }
        canvas
    };

    let mut regions = BTreeMap::new();
    regions.insert(
        MicroRegion::Eyes,
        field(
            MicroRegion::Eyes,
            CropRect {
                x: 0.3437,
                y: 0.3437,
                w: 0.3125,
                h: 0.0938,
            },
        ),
    );
    regions.insert(
        MicroRegion::Iris,
        field(
            MicroRegion::Iris,
            CropRect {
                x: 0.3672,
                y: 0.375,
                w: 0.0625,
                h: 0.0469,
            },
        ),
    );

    let context = MicroFrame {
        scene: SceneId::CouplePortrait,
        faces: vec![face()],
        regions,
        neutral: Some(NEUTRAL),
        allowed: [false, false, false, false, true],
        borrowing: true,
        siblings: vec![Sibling {
            image: photo(5),
            pixels: paint(false).buffer(),
            faces: vec![face()],
        }],
        ..MicroFrame::new(SceneId::CouplePortrait)
    };

    (photo(4), paint(true).buffer(), context)
}

/// A frame whose regions are all present and whose scene permits everything.
///
/// Used by the phase gate to attempt each contract ceiling and assert the refusal, which is
/// section 6.4's "a CI test attempts to exceed each ceiling and asserts refusal".
#[must_use]
pub fn ceiling_frame() -> (ImageId, PixelBuffer, MicroFrame) {
    let (_, pixels, context) = planned_frame();
    (photo(6), pixels, context)
}
