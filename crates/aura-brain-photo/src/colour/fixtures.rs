//! The synthetic ground truth every section 10.1 gate is measured against.
//!
//! Phase 15's `tone::fixtures` paints an illuminant and a subject luminance into the pixels
//! and reads them back through the real pipeline. This module does the same thing for
//! *content*: a frame with foliage at a known hue, a dress at a known luminance and an exit
//! sign at a known saturation, so that "the greenery was pulled toward the target" is a
//! measurement against a number that was chosen rather than a judgement about a photograph.
//!
//! The two modules share phase 15's `under` and `encode`, so a fixture here is rendered
//! through exactly the same reflectance-times-illuminant arithmetic as one there. What is
//! added is region painting: each fixture is a small number of rectangles with named
//! reflectances, and [`Truth`] says what was painted.
//!
//! ## They are painted under a neutral light, and phase 15's are not
//!
//! Section 3's input to this phase is the "exposure/WB-corrected linear image": by the time a
//! grade is decided, phase 15 has already removed the light's own colour. So a fixture here
//! paints **reflectances under a neutral light**, which is what the proxy actually looks like
//! at this point in the pipeline. Phase 15's fixtures paint under an illuminant because
//! estimating that illuminant is the whole of what they are for.
//!
//! Painting under a coloured light here would make the dress classifier fail on a white
//! dress, because a neutral surface under 5,200 K is not neutral in linear sRGB, and the
//! failure would be a fixture bug that reads exactly like a classifier bug.
//!
//! ## The subject's contrast is painted in encoded units
//!
//! A face is painted as a vertical ramp whose **encoded** luminance spans a chosen
//! interquartile range, because that is the space `analyse::subject_stats` measures in and the
//! space `tone_intent.toml`'s `subject_contrast` is written in. Two flat patches at two
//! exposures was the first design and it produced a subject spread of about one percent, which
//! no curve can take to the thirty the intent asks for - a fixture that cannot pass is not a
//! gate.
//!
//! **These are not photographs.** They prove the arithmetic - that the content classifier
//! finds foliage where foliage was painted, that the curve fitter reaches the spread it was
//! given, that the skin guard catches a move it should catch. They say nothing about a real
//! wedding, and condition C1 of `docs/progress/PHASE-16-EXIT.md` says so in the exit report's
//! own words.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::{FaceRef, ImageSubjects};
use aura_core::{AttrFlags, FaceId, IdentityId, SceneId};

use crate::tone::fixtures::{encode, SKIN_TONES};

/// Foliage as a camera records it: too yellow-green, too saturated.
///
/// Linear sRGB reflectance whose hue lands near 95 degrees, which is section 6.2's "outdoor
/// foliage is usually too yellow-green" as a number rather than as a sentence.
pub const FOLIAGE_REFLECTANCE: [f32; 3] = [0.16, 0.34, 0.07];

/// A clear sky.
pub const SKY_REFLECTANCE: [f32; 3] = [0.24, 0.36, 0.72];

/// A wedding dress. Bright and very nearly neutral, which is what makes it a dress rather
/// than a colour.
pub const DRESS_REFLECTANCE: [f32; 3] = [0.82, 0.82, 0.81];

/// Warm timber: a hall floor, a table, a pew.
pub const WOOD_REFLECTANCE: [f32; 3] = [0.30, 0.19, 0.10];

/// A fluorescent exit sign. Section 2.1's own example, as a reflectance.
pub const EXIT_SIGN_REFLECTANCE: [f32; 3] = [0.06, 0.62, 0.14];

/// A purple uplighter on a dance floor.
pub const UPLIGHT_REFLECTANCE: [f32; 3] = [0.42, 0.08, 0.55];

/// A neutral mid-grey background.
pub const BACKGROUND_REFLECTANCE: [f32; 3] = [0.22, 0.22, 0.22];

/// What a fixture knows about itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Truth {
    /// The hue foliage was painted at, in degrees, when the frame has any.
    pub greenery_hue_deg: Option<f32>,
    /// True when a bright near-neutral region was painted.
    pub has_dress: bool,
    /// The hue of the deliberately distracting region, when there is one.
    pub distractor_hue_deg: Option<f32>,
    /// Which entry of `SKIN_TONES` the faces were painted with.
    pub skin_index: Option<usize>,
    /// The luminance spread the subject region was painted at.
    pub subject_spread: f32,
    /// True when the frame's colour is a creative choice that must survive.
    pub coloured_light: bool,
}

/// One synthetic frame and everything known about it.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Row-major 8-bit sRGB.
    pub rgb: Vec<u8>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// The faces in it.
    pub subjects: ImageSubjects,
    /// What phase 07 would say it is.
    pub scene: SceneId,
    /// Phase 07's attribute bits.
    pub attrs: AttrFlags,
    /// What is known by construction.
    pub truth: Truth,
    /// A human-readable name, for a gate's output.
    pub name: &'static str,
}

/// One painted rectangle.
#[derive(Debug, Clone, Copy)]
struct Region {
    /// Normalised bounds.
    rect: CropRect,
    /// Linear reflectance.
    reflectance: [f32; 3],
    /// A multiplier on the whole region.
    exposure: f32,
    /// The **encoded** luminance range painted vertically across the region.
    ///
    /// Zero for a flat region. Non-zero makes the region a ramp whose interquartile spread is
    /// this number, which is the unit `analyse::subject_stats` measures in and the unit
    /// `tone_intent.toml`'s `subject_contrast` is written in.
    spread: f32,
}

impl Region {
    /// A flat region.
    const fn flat(rect: CropRect, reflectance: [f32; 3], exposure: f32) -> Self {
        Self {
            rect,
            reflectance,
            exposure,
            spread: 0.0,
        }
    }
}

/// The linear luminance of one triple.
fn luma_of(linear: [f32; 3]) -> f32 {
    0.2126 * linear.first().copied().unwrap_or(0.0)
        + 0.7152 * linear.get(1).copied().unwrap_or(0.0)
        + 0.0722 * linear.get(2).copied().unwrap_or(0.0)
}

/// The inverse of [`encode`], for placing a ramp in encoded units.
fn decode(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Paint a frame from a background and a list of regions.
///
/// The light is neutral; see the module header. `regions` are painted in order, so a later
/// one covers an earlier one.
fn paint(width: usize, height: usize, background: [f32; 3], regions: &[Region]) -> Vec<u8> {
    let mut rgb = vec![0u8; width * height * 3];
    for y in 0..height {
        let ny = y as f32 / (height.max(2) - 1) as f32;
        for x in 0..width {
            let nx = x as f32 / (width.max(2) - 1) as f32;
            let mut colour = background;
            for region in regions {
                let r = region.rect;
                if nx < r.x || nx > r.x + r.w || ny < r.y || ny > r.y + r.h {
                    continue;
                }
                let base = [
                    region.reflectance.first().copied().unwrap_or(0.0) * region.exposure,
                    region.reflectance.get(1).copied().unwrap_or(0.0) * region.exposure,
                    region.reflectance.get(2).copied().unwrap_or(0.0) * region.exposure,
                ];
                colour = if region.spread <= 0.0 {
                    base
                } else {
                    // The ramp is placed in ENCODED units and then converted back, so the
                    // region's interquartile spread is exactly `region.spread`. A ramp built
                    // in linear light would have a spread that depends on how bright the
                    // region happens to be, which is the bug this replaced.
                    let t = if r.h <= 0.0 { 0.5 } else { (ny - r.y) / r.h };
                    let centre = f32::from(encode(luma_of(base))) / 255.0;
                    let wanted = (centre + (t - 0.5) * region.spread * 2.0).clamp(0.01, 0.99);
                    let target = decode(wanted);
                    let scale = target / luma_of(base).max(1e-4);
                    [base[0] * scale, base[1] * scale, base[2] * scale]
                };
            }
            let index = (y * width + x) * 3;
            for (channel, value) in colour.iter().enumerate() {
                if let Some(slot) = rgb.get_mut(index + channel) {
                    *slot = encode(*value);
                }
            }
        }
    }
    rgb
}

/// One face, with the fields this phase reads.
///
/// `bbox` and `area_frac` are the only two that matter here - the guard samples inside the
/// box and the subject statistics are measured over it - but the rest are filled with
/// plausible values so a fixture can be handed to phase 09's or phase 15's code unchanged.
fn face_at(rect: CropRect, identity: u8) -> FaceRef {
    FaceRef {
        face_id: FaceId::new(),
        identity_id: Some(IdentityId::new()),
        bbox: rect,
        eyes: [
            [rect.x + rect.w * 0.32, rect.y + rect.h * 0.40],
            [rect.x + rect.w * 0.68, rect.y + rect.h * 0.40],
        ],
        area_frac: rect.w * rect.h,
        centrality: 1.0 - ((rect.x + rect.w / 2.0) - 0.5).abs() * 2.0,
        sharpness: 0.8,
        quality: 0.8,
        votes: identity == 0,
    }
}

/// Two faces side by side in the middle of the frame, each a vertical ramp so the subject
/// region has a known interquartile spread.
///
/// `spread` is in **encoded** units, which is what `analyse::subject_stats` measures and what
/// `tone_intent.toml`s `subject_contrast` is written in. See the module header.
fn couple(skin: [f32; 3], spread: f32) -> (Vec<Region>, ImageSubjects) {
    let left = CropRect {
        x: 0.30,
        y: 0.32,
        w: 0.17,
        h: 0.26,
    };
    let right = CropRect {
        x: 0.53,
        y: 0.32,
        w: 0.17,
        h: 0.26,
    };
    let regions = vec![
        Region {
            rect: left,
            reflectance: skin,
            exposure: 1.0,
            spread,
        },
        Region {
            rect: right,
            reflectance: skin,
            exposure: 1.0,
            spread,
        },
    ];
    let subjects = ImageSubjects {
        faces: vec![face_at(left, 0), face_at(right, 1)],
        dominant: None,
        prominence: std::collections::BTreeMap::new(),
        subject_focus_score: 0.8,
        people_count: 2,
        weights_ver: 1,
    };
    (regions, subjects)
}

/// An outdoor couple portrait: foliage below, sky above, two faces in the middle.
///
/// The frame the greenery gate is measured on. The foliage is painted at
/// [`FOLIAGE_REFLECTANCE`], which lands near 95 degrees - exactly the yellow-green section
/// 6.2 names - and the couple-portrait intent row asks for 128.
#[must_use]
pub fn outdoor_portrait(skin_index: usize) -> Frame {
    let (width, height) = (384, 256);
    let (faces, subjects) = couple(tone_of(skin_index), 0.21);
    let mut regions = vec![
        Region::flat(
            CropRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 0.30,
            },
            SKY_REFLECTANCE,
            1.0,
        ),
        Region::flat(
            CropRect {
                x: 0.0,
                y: 0.55,
                w: 1.0,
                h: 0.45,
            },
            FOLIAGE_REFLECTANCE,
            1.0,
        ),
    ];
    regions.extend(faces);
    Frame {
        rgb: paint(width, height, BACKGROUND_REFLECTANCE, &regions),
        width,
        height,
        subjects,
        scene: SceneId::CouplePortrait,
        attrs: AttrFlags::EMPTY,
        truth: Truth {
            greenery_hue_deg: Some(hue_of(FOLIAGE_REFLECTANCE)),
            has_dress: false,
            distractor_hue_deg: None,
            skin_index: Some(skin_index),
            subject_spread: 0.21,
            coloured_light: false,
        },
        name: "outdoor_portrait",
    }
}

/// A flat indoor ceremony: wood, two faces, less separation than the scene wants.
///
/// The frame the contrast and curve gates are measured on. The subject spread is painted at
/// 0.17, below the ceremony intent's 0.28, so a working curve fitter has to add separation
/// and a broken one visibly does not - and the gap is inside what `MAX_GAIN` can close, which
/// a fixture that cannot be satisfied would not be.
#[must_use]
pub fn flat_ceremony(skin_index: usize) -> Frame {
    let (width, height) = (384, 256);
    let (faces, subjects) = couple(tone_of(skin_index), 0.17);
    let mut regions = vec![Region::flat(
        CropRect {
            x: 0.0,
            y: 0.60,
            w: 1.0,
            h: 0.40,
        },
        WOOD_REFLECTANCE,
        1.0,
    )];
    regions.extend(faces);
    Frame {
        rgb: paint(width, height, BACKGROUND_REFLECTANCE, &regions),
        width,
        height,
        subjects,
        scene: SceneId::Ceremony,
        attrs: AttrFlags::TUNGSTEN,
        truth: Truth {
            greenery_hue_deg: None,
            has_dress: false,
            distractor_hue_deg: None,
            skin_index: Some(skin_index),
            subject_spread: 0.17,
            coloured_light: false,
        },
        name: "flat_ceremony",
    }
}

/// A dress across the lower half, no faces at all.
///
/// The frame constraint (c) is measured on: the shoulder must stay below the dress's own
/// luminance, so the gradient across a fold survives. The dress is painted as a gentle ramp,
/// because a flat white rectangle is a fixture that cannot fail the thing it tests.
#[must_use]
pub fn dress_detail() -> Frame {
    let (width, height) = (384, 256);
    let regions = vec![Region {
        rect: CropRect {
            x: 0.0,
            y: 0.35,
            w: 1.0,
            h: 0.65,
        },
        reflectance: DRESS_REFLECTANCE,
        exposure: 1.0,
        spread: 0.10,
    }];
    Frame {
        rgb: paint(width, height, BACKGROUND_REFLECTANCE, &regions),
        width,
        height,
        subjects: ImageSubjects::default(),
        scene: SceneId::Details,
        attrs: AttrFlags::EMPTY,
        truth: Truth {
            greenery_hue_deg: None,
            has_dress: true,
            distractor_hue_deg: None,
            skin_index: None,
            subject_spread: 0.10,
            coloured_light: false,
        },
        name: "dress_detail",
    }
}

/// Two faces and a fluorescent exit sign in the corner.
///
/// Section 2.1's own example, and the frame the harmony gate is measured on. The sign is far
/// from the subject's hue and far past the vows intent's distractor ceiling, so a working
/// solver reduces that band and leaves the rest of the frame where it was.
#[must_use]
pub fn exit_sign_vows(skin_index: usize) -> Frame {
    let (width, height) = (384, 256);
    let (faces, subjects) = couple(tone_of(skin_index), 0.19);
    let mut regions = faces;
    regions.push(Region::flat(
        CropRect {
            x: 0.80,
            y: 0.06,
            w: 0.18,
            h: 0.16,
        },
        EXIT_SIGN_REFLECTANCE,
        1.4,
    ));
    Frame {
        rgb: paint(width, height, BACKGROUND_REFLECTANCE, &regions),
        width,
        height,
        subjects,
        scene: SceneId::Vows,
        attrs: AttrFlags::EMPTY,
        truth: Truth {
            greenery_hue_deg: None,
            has_dress: false,
            distractor_hue_deg: Some(hue_of(EXIT_SIGN_REFLECTANCE)),
            skin_index: Some(skin_index),
            subject_spread: 0.19,
            coloured_light: false,
        },
        name: "exit_sign_vows",
    }
}

/// A purple dance floor with two faces on it.
///
/// The frame that proves phase 16 cannot undo phase 15's decision by another route. The
/// uplighting is exactly as saturated as an exit sign and it is the photograph, so the
/// dance-floor intent's ceiling must leave it alone.
#[must_use]
pub fn purple_dance_floor(skin_index: usize) -> Frame {
    let (width, height) = (384, 256);
    let (faces, subjects) = couple(tone_of(skin_index), 0.23);
    let mut regions = vec![Region::flat(
        CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        UPLIGHT_REFLECTANCE,
        0.9,
    )];
    regions.extend(faces);
    Frame {
        rgb: paint(width, height, UPLIGHT_REFLECTANCE, &regions),
        width,
        height,
        subjects,
        scene: SceneId::DanceFloor,
        attrs: AttrFlags::STAGE,
        truth: Truth {
            greenery_hue_deg: None,
            has_dress: false,
            distractor_hue_deg: Some(hue_of(UPLIGHT_REFLECTANCE)),
            skin_index: Some(skin_index),
            subject_spread: 0.23,
            coloured_light: true,
        },
        name: "purple_dance_floor",
    }
}

/// A frame that arrives already blown, with no faces.
///
/// The frame the clipping gate is measured on: the guard must not re-solve for clipping that
/// was there before the grade, because darkening every face in an exit photograph to recover
/// a sparkler is the wrong trade.
#[must_use]
pub fn blown_exit() -> Frame {
    let (width, height) = (384, 256);
    let regions = vec![
        Region::flat(
            CropRect {
                x: 0.30,
                y: 0.10,
                w: 0.40,
                h: 0.30,
            },
            DRESS_REFLECTANCE,
            6.0,
        ),
        Region::flat(
            CropRect {
                x: 0.0,
                y: 0.60,
                w: 1.0,
                h: 0.40,
            },
            WOOD_REFLECTANCE,
            0.4,
        ),
    ];
    Frame {
        rgb: paint(width, height, BACKGROUND_REFLECTANCE, &regions),
        width,
        height,
        subjects: ImageSubjects::default(),
        scene: SceneId::Exit,
        attrs: AttrFlags::EMPTY,
        truth: Truth {
            greenery_hue_deg: None,
            has_dress: true,
            distractor_hue_deg: None,
            skin_index: None,
            subject_spread: 0.0,
            coloured_light: false,
        },
        name: "blown_exit",
    }
}

#[must_use]
pub fn tone_of(index: usize) -> [f32; 3] {
    SKIN_TONES
        .get(index % SKIN_TONES.len())
        .copied()
        .unwrap_or([0.44, 0.32, 0.26])
}

/// The hue of one linear reflectance, in degrees.
///
/// Used by the fixtures to state their own truth, so a gate compares the classifier's answer
/// against the reflectance rather than against a number somebody typed twice.
#[must_use]
pub fn hue_of(linear: [f32; 3]) -> f32 {
    let (hue, _, _) = crate::colour::content::hsl_of(
        linear.first().copied().unwrap_or(0.0),
        linear.get(1).copied().unwrap_or(0.0),
        linear.get(2).copied().unwrap_or(0.0),
    );
    hue
}

/// Every fixture, once per skin tone where the fixture has faces.
///
/// Thirty-two frames: five scenes with faces at five reflectances each, plus two faceless
/// ones. The per-tone repetition is what makes section 10.1's fairness gate possible, and the
/// grouping lives in `tests/eval` rather than in the product - nothing here stores a
/// skin-tone bucket.
#[must_use]
pub fn every_case() -> Vec<Frame> {
    let mut out = Vec::new();
    for index in 0..SKIN_TONES.len() {
        out.push(outdoor_portrait(index));
        out.push(flat_ceremony(index));
        out.push(exit_sign_vows(index));
        out.push(purple_dance_floor(index));
    }
    out.push(dress_detail());
    out.push(blown_exit());
    out
}
