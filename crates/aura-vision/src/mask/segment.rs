//! The twenty classes, measured rather than predicted.
//!
//! # What is in this file and what is not
//!
//! Section 6.1 asks for "a single multi-class segmentation network at 768 px with a
//! lightweight decoder". [`SEG_HEAD_TRAINED`] is `false`, the head is registered and carded
//! and never consulted, and ADR-0037 decision 2 has the argument. What is here instead is the
//! part of section 6.1 that does not need weights, stated in the phase document's own words:
//!
//! > Skin masks are seeded by detected faces and extended by colour-space growth constrained
//! > to connected regions, which handles arms, shoulders and hands reliably.
//!
//! That sentence generalises. A wedding photograph is not an arbitrary image: it has people
//! in it whose faces phase 06 has already found and whose eyes phase 09 already needed
//! landmarks for, and the regions this phase names are mostly *positions relative to a face*
//! or *colorimetric neighbourhoods of a measured seed*. Six of the twenty - sky, greenery,
//! water, floor, window, background - are properties of the whole frame that have been
//! measurable since photography was invented.
//!
//! # The one class that cannot be measured, and says so
//!
//! [`MaskKind::Dress`] is the honest failure. "Clothing" versus "a bridal dress" is a
//! semantic distinction with no reliable colorimetric signature - a dark green saree and a
//! dark green jacket are the same pixels - and the head that would decide it is the one that
//! is not trained. What this file does instead is narrow: a clothing component that is
//! predominantly high-luminance and low-chroma *and* reaches the bottom of the frame is
//! called a dress, at a **lower confidence than any other class**, and everything else worn
//! is clothing. A photographer who shoots a red lehenga gets clothing, which is a mask that
//! works, rather than a dress, which would be a mask that lied.
//!
//! # Every threshold here is scene-conditioned or measured, never global
//!
//! Invariant 7. The skin locus is sampled from *this frame's own faces*, exactly as phase
//! 15's is and for the same reason: a fixed skin chromaticity is how an editor lightens dark
//! skin while believing it is correcting a cast. There is no skin colour constant in this
//! file, the phase gate scans for one, and `docs/skin-fairness.md` says so in the product's
//! own words.

use crate::contract::mask::{EdgeQuality, MaskKind, MaskReason};
use crate::face::detect::NormBox;
use crate::face::person::PersonBox;
use crate::face::FaceObservation;
use crate::mask::algebra::{self, Plane};
use crate::mask::matting;
use crate::mask::trimap;
use crate::mask::{MaskFrame, MaskPlane};

/// Whether the learned segmentation head is trained in this build.
///
/// It is not. The model is registered, signed and carded; `class_hint` returns `None`, and no
/// photograph in this build is segmented by a random projection. ADR-0037 decision 2.
pub const SEG_HEAD_TRAINED: bool = false;

/// The name the segmentation head is registered under.
pub const SEG_MODEL: &str = "semantic_segment";

// ---------------------------------------------------------------------------
// The thresholds. Every one of them has a sentence.
// ---------------------------------------------------------------------------

/// How far from the seed's chromaticity a pixel may sit and still be the same skin.
///
/// In the `(r - g, b - g)` plane of the linear working space, normalised by luminance. It is a
/// *distance from a measured seed*, not a position, which is the whole of the fairness
/// argument: the same tolerance around a light seed and around a dark one accepts the same
/// relative spread of the same person's own skin.
const SKIN_CHROMA_TOLERANCE: f32 = 0.085;

/// How far below and above the seed's luminance a pixel may sit and still be the same skin.
///
/// A ratio rather than a difference, and asymmetric: skin in shadow is a larger *relative*
/// step down than skin in a specular highlight is up, because a highlight is the light source
/// rather than the person. Two thirds down and one and a half up.
const SKIN_LUMA_LOW: f32 = 0.55;
/// The upper end of the same band.
const SKIN_LUMA_HIGH: f32 = 1.8;

/// How much brighter than the frame's median a pixel must be to be a window or a light source.
///
/// Four times, which at a normal exposure is about two stops above the scene and is where a
/// window stops being a bright wall. Relative to the frame rather than absolute, because an
/// indoor Hindu night ceremony and an outdoor daylight wedding do not share a brightness.
const WINDOW_LUMA_RATIO: f32 = 4.0;

/// How low a window's chroma must be. A bright saturated colour is a dress, not a window.
const WINDOW_CHROMA_MAX: f32 = 0.06;

/// How far down the frame sky may reach before it stops being sky.
///
/// Three fifths. A wedding photographed from below on a staircase puts sky lower than a
/// landscape does, and the connectivity constraint - sky must reach the top edge - is what
/// actually carries the decision. This is a bound on a pathological case rather than a prior.
const SKY_MAX_Y: f32 = 0.6;

/// How much bluer than green a pixel must be to be sky.
const SKY_BLUE_EXCESS: f32 = 0.02;

/// How much greener than both other channels a pixel must be to be greenery.
const GREEN_EXCESS: f32 = 0.03;

/// How far up the frame the floor may reach.
///
/// A quarter from the bottom, and the same caveat as [`SKY_MAX_Y`]: the connectivity to the
/// bottom edge is what decides, and this bounds the pathological case.
const FLOOR_MIN_Y: f32 = 0.75;

/// The largest texture a flat region - sky, water, floor - may carry, as a fraction of the
/// frame's own median gradient.
///
/// Relative to the frame, so a grainy ISO 12800 reception and a clean daylight ceremony are
/// judged against their own noise floors. Phase 09 established that a noise figure is only
/// meaningful against the scene it was measured in.
const FLAT_TEXTURE_RATIO: f32 = 0.8;

/// The floor under that threshold, as an absolute gradient in linear luminance.
///
/// Two per cent of the luminance scale per pixel. Without it a frame whose median gradient is
/// *zero* - a clean sky over a clean wall, which is a real photograph and is also every
/// synthetic fixture - would admit nothing at all as flat, and the sky mask would come back
/// empty on exactly the frames it is easiest on. A ratio alone is a threshold that divides by
/// something that can be zero.
const FLAT_TEXTURE_FLOOR: f32 = 0.02;

/// How far above the face box hair may extend, as a multiple of the face height.
const HAIR_ABOVE: f32 = 0.55;

/// How far below the face box hair may extend, as a multiple of the face height.
///
/// **Zero: the search stops at the chin line.** Hair does fall past it, and a longer search
/// runs into a problem this class cannot solve - below the chin is the torso, worn fabric is
/// often darker than skin, and a "darker than her own skin" test cannot tell a dark bob from a
/// dark jacket. What it would produce is a hair mask with a piece of somebody's suit in it,
/// which is worse for every consumer than a hair mask that stops at the shoulders.
///
/// The cost is that hair falling onto the shoulders is not in the hair class. It is still in
/// the subject and still in the matte, so a local exposure lift reaches it and only the
/// operations that ask for hair *specifically* are affected. The head that would draw the whole
/// hairline is the one that is not trained.
const HAIR_BELOW: f32 = 0.0;

/// How far to each side of the face box hair may extend, as a multiple of the face width.
const HAIR_SIDE: f32 = 0.35;

/// How much darker than the face's own skin a pixel must be to be hair.
///
/// Fifty-five per cent of the seed's luminance. Against the person's own skin rather than
/// against a constant, for the third time in this file and for the same reason.
///
/// The number is low because of what sits on the other side of it. Real hair is two to four
/// times darker than the same person's face; a mid-grey wall behind a light-skinned subject is
/// only about a fifth darker. A ratio of four fifths admits the wall and produces a hair mask
/// with the room in it - which is a mask that looks plausible in the panel and brightens a wall
/// the first time phase 19 lifts somebody's hair.
///
/// **This misses blonde hair against a dark background**, which is the honest limitation of a
/// measured hair class and is what the untrained head would fix. What catches it instead is the
/// subject matte: hair that is not in the hair class is still inside the subject, so a local
/// exposure lift through the subject still reaches it, and only the operations that ask
/// specifically for hair are affected.
const HAIR_LUMA_RATIO: f32 = 0.55;

/// How much darker than the face's own median a pixel must be to be facial hair.
///
/// Seventy per cent of the median, measured on this face rather than on a constant, for the
/// same reason the skin seed is.
const FACIAL_HAIR_RATIO: f32 = 0.7;

/// How bright and how unsaturated a clothing component must be, and how far down the frame it
/// must reach, before it is called a dress rather than clothing. See the module note.
const DRESS_LUMA_RATIO: f32 = 1.35;
/// The chroma ceiling for the same decision.
const DRESS_CHROMA_MAX: f32 = 0.05;
/// How close to the bottom of the frame a dress must reach.
const DRESS_REACH_Y: f32 = 0.9;

/// How far from the collar's chroma a pixel may sit and still be the same garment.
///
/// Wider than the skin tolerance, because fabric folds and skin does not: a sleeve in shadow
/// and the same sleeve in light are further apart in this plane than two parts of one cheek.
const CLOTH_CHROMA_TOLERANCE: f32 = 0.10;

/// The luminance band a garment may span, as a ratio of the collar sample.
///
/// Much wider than skin's for the same reason. A white dress runs from a blown highlight on a
/// fold to a deep shadow in a pleat inside one frame.
const CLOTH_LUMA_LOW: f32 = 0.35;
/// The upper end of the same band.
const CLOTH_LUMA_HIGH: f32 = 2.4;

/// Where the torso starts inside a body box that has no face bound to it.
///
/// A quarter of the way down. Phase 06's `person::head_region` uses the same proportion from
/// the other direction, and it is written down in exactly one other place for that reason.
const CHIN_FRACTION: f32 = 0.25;

/// How far the skin-safe zone is grown past skin and face, as a fraction of the analysis grid.
///
/// Two per cent of the short edge. Phase 16's guard measures skin pixels and it measures them
/// through a renderer whose spatial stages read a neighbourhood; a zone that stopped exactly
/// at the skin boundary would let clarity and sharpening move the pixel next to somebody's
/// cheek and call the guarantee kept.
const SKIN_SAFE_GROW: f32 = 0.02;

/// The confidence a class gets when a face seeded it.
const CONF_SEEDED: f32 = 0.86;
/// The confidence a whole-frame prior gets when there is no face.
const CONF_PRIOR: f32 = 0.42;
/// The confidence a colorimetric environment class gets.
const CONF_ENVIRONMENT: f32 = 0.70;
/// The confidence a derived class gets - it is only as good as what it was derived from.
const CONF_DERIVED: f32 = 0.78;
/// The confidence [`MaskKind::Dress`] gets. Lower than everything, deliberately.
const CONF_DRESS: f32 = 0.38;

/// Per-pixel measurements the classes are decided from.
///
/// Computed once for the frame. Every class below reads this rather than the pixels, which is
/// phase 05's rule applied inside one pass: five classes each recomputing a luminance is five
/// chances for two of them to disagree about how bright a pixel is.
#[derive(Debug, Clone)]
pub struct Features {
    /// Grid width.
    pub w: u32,
    /// Grid height.
    pub h: u32,
    /// Relative luminance per pixel.
    pub luma: Vec<f32>,
    /// `(r - g) / y` per pixel.
    pub cr: Vec<f32>,
    /// `(b - g) / y` per pixel.
    pub cb: Vec<f32>,
    /// Local gradient magnitude per pixel.
    pub texture: Vec<f32>,
    /// The frame's median luminance.
    pub median_luma: f32,
    /// The frame's median gradient.
    pub median_texture: f32,
}

/// Rec.709 luminance weights, the ones every other module in the workspace uses.
const LUMA_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];

impl Features {
    /// Measure a frame.
    #[must_use]
    pub fn measure(frame: &MaskFrame) -> Self {
        let width = frame.width;
        let height = frame.height;
        let count = (width as usize) * (height as usize);
        let mut luma = vec![0.0_f32; count];
        let mut cr = vec![0.0_f32; count];
        let mut cb = vec![0.0_f32; count];

        for y in 0..i64::from(height) {
            for x in 0..i64::from(width) {
                let pixel = frame.at(x, y);
                let index = (y as usize) * (width as usize) + (x as usize);
                let value = LUMA_WEIGHTS[0] * pixel[0]
                    + LUMA_WEIGHTS[1] * pixel[1]
                    + LUMA_WEIGHTS[2] * pixel[2];
                let denom = value.max(1e-4);
                if let Some(slot) = luma.get_mut(index) {
                    *slot = value;
                }
                if let Some(slot) = cr.get_mut(index) {
                    *slot = (pixel[0] - pixel[1]) / denom;
                }
                if let Some(slot) = cb.get_mut(index) {
                    *slot = (pixel[2] - pixel[1]) / denom;
                }
            }
        }

        let mut texture = vec![0.0_f32; count];
        for y in 0..i64::from(height) {
            for x in 0..i64::from(width) {
                let index = (y as usize) * (width as usize) + (x as usize);
                let here = sample(&luma, width, height, x, y);
                let dx = sample(&luma, width, height, x + 1, y) - here;
                let dy = sample(&luma, width, height, x, y + 1) - here;
                if let Some(slot) = texture.get_mut(index) {
                    *slot = dx.hypot(dy);
                }
            }
        }

        Self {
            median_luma: median(&luma),
            median_texture: median(&texture),
            w: width,
            h: height,
            luma,
            cr,
            cb,
            texture,
        }
    }

    /// The luminance at a pixel, or zero outside.
    #[must_use]
    pub fn luma_at(&self, x: i64, y: i64) -> f32 {
        sample(&self.luma, self.w, self.h, x, y)
    }

    /// The chroma pair at a pixel.
    #[must_use]
    pub fn chroma_at(&self, x: i64, y: i64) -> [f32; 2] {
        [
            sample(&self.cr, self.w, self.h, x, y),
            sample(&self.cb, self.w, self.h, x, y),
        ]
    }

    /// The chroma magnitude at a pixel.
    #[must_use]
    pub fn chroma_mag(&self, x: i64, y: i64) -> f32 {
        let c = self.chroma_at(x, y);
        c[0].hypot(c[1])
    }

    /// The gradient magnitude at a pixel.
    #[must_use]
    pub fn texture_at(&self, x: i64, y: i64) -> f32 {
        sample(&self.texture, self.w, self.h, x, y)
    }
}

fn sample(buffer: &[f32], w: u32, h: u32, x: i64, y: i64) -> f32 {
    if x < 0 || y < 0 || x >= i64::from(w) || y >= i64::from(h) {
        return 0.0;
    }
    buffer
        .get((y as usize) * (w as usize) + (x as usize))
        .copied()
        .unwrap_or(0.0)
}

/// The median of a buffer, by sorting a copy.
///
/// A sort rather than a histogram, because the buffer is at most 768 by 512 and a histogram
/// would need a bin width, which is a constant nobody could defend across an indoor ceremony
/// and an outdoor one.
fn median(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted.get(sorted.len() / 2).copied().unwrap_or(0.0)
}

/// What the learned head would say about a pixel's class.
///
/// Always `None` in this build. It is a function rather than an absence so the call site that
/// will consult it reads the same before and after the head is trained, and so the phase gate
/// can assert that it returns nothing.
#[must_use]
pub fn class_hint(_features: &Features, _x: i64, _y: i64) -> Option<MaskKind> {
    if SEG_HEAD_TRAINED {
        // Unreachable in this build. When the head is trained this is where its argmax goes,
        // and the deterministic path below becomes the prior it is blended against.
        return None;
    }
    None
}

/// Measure every class of one frame.
///
/// Returns the person-bearing and environment classes. [`crate::mask::subject::run`] adds the
/// subject and [`finish`] derives background and the skin-safe zone from what is here, which
/// is why those three are not in this list.
#[must_use]
pub fn run(frame: &MaskFrame, faces: &[FaceObservation], persons: &[PersonBox]) -> Vec<MaskPlane> {
    let features = Features::measure(frame);
    let mut out = Vec::with_capacity(20);

    // ---- environment ---------------------------------------------------------------
    out.push(sky(&features));
    out.push(greenery(&features));
    out.push(water(&features));
    out.push(floor(&features));
    out.push(window(&features));

    // ---- people --------------------------------------------------------------------
    let seed = SkinSeed::from_faces(&features, faces);
    let skin_plane = skin(&features, &seed, faces);
    let face_plane = face_region(&features, faces, &skin_plane);
    let hair_plane = hair(&features, faces, &seed, &skin_plane);
    out.push(facial_hair(&features, faces, &seed, &skin_plane));
    for plane in eye_regions(&features, faces) {
        out.push(plane);
    }
    out.push(lips(&features, faces, &seed));
    out.push(eyebrows(&features, faces, &seed));
    out.push(teeth(&features, faces));
    let clothes = clothing(&features, persons, faces, &skin_plane, &hair_plane);
    out.extend(clothes);
    out.push(hair_plane);
    out.push(face_plane);
    out.push(skin_plane);
    out
}

/// Derive the classes that are functions of the others.
///
/// Background is the complement of the subject and the skin-safe zone is the dilated union of
/// skin and face. Both are *derived*, both say so with [`MaskReason::Derived`], and neither
/// re-measures a pixel - which is what keeps three modules from disagreeing about where the
/// subject ends.
pub fn finish(frame: &MaskFrame, planes: &mut Vec<MaskPlane>) {
    let (w, h) = (frame.width, frame.height);
    let empty = Plane::zeros(w, h);

    let subject = planes
        .iter()
        .find(|p| p.kind == MaskKind::Subject && p.identity.is_none())
        .map_or_else(|| empty.clone(), |p| p.plane.clone());
    let skin = planes
        .iter()
        .find(|p| p.kind == MaskKind::Skin && p.identity.is_none())
        .map_or_else(|| empty.clone(), |p| p.plane.clone());
    let face = planes
        .iter()
        .find(|p| p.kind == MaskKind::Face && p.identity.is_none())
        .map_or(empty, |p| p.plane.clone());

    let background = algebra::invert(&subject);
    let background_conf = planes
        .iter()
        .find(|p| p.kind == MaskKind::Subject)
        .map_or(CONF_PRIOR, |p| p.confidence);
    planes.push(MaskPlane {
        kind: MaskKind::Background,
        identity: None,
        plane: background,
        confidence: background_conf,
        edge_quality: planes
            .iter()
            .find(|p| p.kind == MaskKind::Subject)
            .map_or(0.3, |p| p.edge_quality),
        edge: EdgeQuality::Soft,
        reasons: vec![MaskReason::Derived],
    });

    let short = w.min(h) as f32;
    let grow = ((SKIN_SAFE_GROW * short).round() as u32).max(1);
    let safe = algebra::grow(&algebra::union(&skin, &face), grow);
    planes.push(MaskPlane {
        kind: MaskKind::SkinSafe,
        identity: None,
        plane: safe,
        confidence: planes
            .iter()
            .find(|p| p.kind == MaskKind::Skin)
            .map_or(CONF_PRIOR, |p| p.confidence),
        edge_quality: 1.0,
        edge: EdgeQuality::Binary,
        reasons: vec![MaskReason::Derived],
    });
}

// ---------------------------------------------------------------------------
// The skin seed
// ---------------------------------------------------------------------------

/// This frame's own skin colour, measured from the faces in it.
///
/// **There is no constant here and there is nowhere for one to live.** The seed is the median
/// chroma and luminance of the pixels inside the inner ellipse of each detected face box, and
/// a frame with no faces has no seed at all. That is the same defence phase 15's `SkinLocus`
/// and phase 17's `SkinBias` make - the schema cannot express an ideal skin tone - and it is
/// the third module in the product to make it.
#[derive(Debug, Clone, Default)]
pub struct SkinSeed {
    /// One entry per face that produced a usable sample.
    pub samples: Vec<SkinSample>,
}

/// One face's own skin colour.
#[derive(Debug, Clone, Copy)]
pub struct SkinSample {
    /// Median `(r - g) / y`.
    pub cr: f32,
    /// Median `(b - g) / y`.
    pub cb: f32,
    /// Median luminance.
    pub luma: f32,
    /// Which face it came from.
    pub face: usize,
}

impl SkinSeed {
    /// Sample every detected face.
    #[must_use]
    pub fn from_faces(features: &Features, faces: &[FaceObservation]) -> Self {
        let mut samples = Vec::new();
        for (index, face) in faces.iter().enumerate() {
            // The inner half of the box, which is cheek and forehead rather than the hair and
            // background a detector's box always includes at its corners.
            let inner = inset(&face.bbox, 0.25);
            let (x0, y0, x1, y1) = box_pixels(&inner, features.w, features.h);
            let mut crs = Vec::new();
            let mut cbs = Vec::new();
            let mut lumas = Vec::new();
            for y in y0..y1 {
                for x in x0..x1 {
                    let c = features.chroma_at(x, y);
                    crs.push(c[0]);
                    cbs.push(c[1]);
                    lumas.push(features.luma_at(x, y));
                }
            }
            if lumas.is_empty() {
                continue;
            }
            samples.push(SkinSample {
                cr: median(&crs),
                cb: median(&cbs),
                luma: median(&lumas),
                face: index,
            });
        }
        Self { samples }
    }

    /// True when no face produced a sample.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// How well a pixel matches any of the seeds, `0.0 ..= 1.0`.
    ///
    /// The best match rather than the mean, because a frame with a light-skinned and a
    /// dark-skinned person in it has two loci and averaging them produces a third that
    /// belongs to neither. Phase 15 accumulates a locus *per identity* for exactly this
    /// reason, and this is the same argument inside one frame.
    #[must_use]
    pub fn match_at(&self, features: &Features, x: i64, y: i64) -> f32 {
        let c = features.chroma_at(x, y);
        let l = features.luma_at(x, y);
        let mut best = 0.0_f32;
        for sample in &self.samples {
            let chroma_distance = (c[0] - sample.cr).hypot(c[1] - sample.cb);
            if chroma_distance > SKIN_CHROMA_TOLERANCE {
                continue;
            }
            let ratio = if sample.luma > 1e-5 {
                l / sample.luma
            } else {
                0.0
            };
            if !(SKIN_LUMA_LOW..=SKIN_LUMA_HIGH).contains(&ratio) {
                continue;
            }
            let chroma_score = 1.0 - chroma_distance / SKIN_CHROMA_TOLERANCE;
            best = best.max(chroma_score.clamp(0.0, 1.0));
        }
        best
    }
}

// ---------------------------------------------------------------------------
// The person-bearing classes
// ---------------------------------------------------------------------------

fn skin(features: &Features, seed: &SkinSeed, faces: &[FaceObservation]) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    if seed.is_empty() {
        // No face, no seed, and no guess. A whole-frame skin prior would be a region that
        // later phases would smooth, and the one thing worse than not finding somebody's skin
        // is finding it on a wall.
        return MaskPlane {
            kind: MaskKind::Skin,
            identity: None,
            plane,
            confidence: 0.0,
            edge_quality: 0.0,
            edge: EdgeQuality::Unknown,
            reasons: vec![MaskReason::NoFaces, MaskReason::HeadUntrained],
        };
    }
    for y in 0..i64::from(features.h) {
        for x in 0..i64::from(features.w) {
            plane.set(x, y, seed.match_at(features, x, y));
        }
    }
    // The connectivity constraint section 6.1 asks for: a skin-coloured wall is skin-coloured
    // and is not connected to anybody's face, so it does not survive.
    let hard = algebra::threshold(&plane, 0.15);
    let connected = keep_connected_to_faces(&hard, faces);
    let plane = algebra::intersect(&plane, &connected);
    MaskPlane {
        kind: MaskKind::Skin,
        identity: None,
        plane,
        confidence: CONF_SEEDED,
        edge_quality: 0.72,
        edge: EdgeQuality::Soft,
        reasons: vec![
            MaskReason::SeededByFace,
            MaskReason::ColourGrown,
            MaskReason::HeadUntrained,
        ],
    }
}

/// Keep only the components of a region that a detected face sits inside.
///
/// This is the connectivity constraint section 6.1 asks for, and it is what stops a
/// skin-coloured wall from being skin: a wall is skin-coloured and is not connected to
/// anybody's face. The test is the *face centre*, not the strongest match, because the
/// strongest match in a frame with two people is on one of them and the other person's arm
/// would then be discarded.
fn keep_connected_to_faces(hard: &Plane, faces: &[FaceObservation]) -> Plane {
    let labels = crate::mask::instance::label_components(hard);
    let mut keep = std::collections::BTreeSet::new();
    for face in faces {
        let centre = face.bbox.centre();
        let x = (centre[0] * hard.w as f32) as i64;
        let y = (centre[1] * hard.h as f32) as i64;
        // The centre of a detector's box can land on a nose highlight that fell outside the
        // colour band, so a small neighbourhood is searched rather than one pixel. A quarter
        // of the box, which is cheek either side of the nose on every face geometry.
        let radius = ((face.bbox.w * hard.w as f32) / 4.0).max(1.0).ceil() as i64;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let id = labels.at(x + dx, y + dy);
                if id > 0 {
                    keep.insert(id);
                }
            }
        }
    }
    labels.select(&keep)
}

fn face_region(features: &Features, faces: &[FaceObservation], skin: &MaskPlane) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    if faces.is_empty() {
        return MaskPlane {
            kind: MaskKind::Face,
            identity: None,
            plane,
            confidence: 0.0,
            edge_quality: 0.0,
            edge: EdgeQuality::Unknown,
            reasons: vec![MaskReason::NoFaces, MaskReason::HeadUntrained],
        };
    }
    for face in faces {
        paint_ellipse(&mut plane, &face.bbox, 1.0);
    }
    // The face is the ellipse *and* the skin inside it, so hair falling across a forehead and
    // the background at the box's corners are both excluded. That intersection is the whole
    // difference between a face mask and a face rectangle.
    let refined = algebra::intersect(&plane, &skin.plane);
    let coverage = if plane.area() > 0.0 {
        (refined.area() / plane.area()) as f32
    } else {
        0.0
    };
    // A face box whose interior is mostly not skin-coloured is a face the seed did not
    // describe - a heavy shadow, a strong colour cast, a mask over the mouth. Falling back to
    // the ellipse keeps a usable region and the confidence says it was a fallback.
    let (plane, confidence, reasons) = if coverage < 0.35 {
        (
            plane,
            CONF_PRIOR,
            vec![MaskReason::SeededByFace, MaskReason::LowContrastBoundary],
        )
    } else {
        (
            refined,
            CONF_SEEDED,
            vec![MaskReason::SeededByFace, MaskReason::ColourGrown],
        )
    };
    MaskPlane {
        kind: MaskKind::Face,
        identity: None,
        plane,
        confidence,
        edge_quality: 0.8,
        edge: EdgeQuality::Soft,
        reasons,
    }
}

fn hair(
    features: &Features,
    faces: &[FaceObservation],
    seed: &SkinSeed,
    skin: &MaskPlane,
) -> MaskPlane {
    let mut search = Plane::zeros(features.w, features.h);
    if faces.is_empty() || seed.is_empty() {
        return MaskPlane {
            kind: MaskKind::Hair,
            identity: None,
            plane: search,
            confidence: 0.0,
            edge_quality: 0.0,
            edge: EdgeQuality::Unknown,
            reasons: vec![MaskReason::NoFaces, MaskReason::HeadUntrained],
        };
    }
    for face in faces {
        let region = NormBox::from_corners(
            face.bbox.x - face.bbox.w * HAIR_SIDE,
            face.bbox.y - face.bbox.h * HAIR_ABOVE,
            face.bbox.x + face.bbox.w * (1.0 + HAIR_SIDE),
            face.bbox.y + face.bbox.h * (1.0 + HAIR_BELOW),
        );
        paint_ellipse(&mut search, &region, 1.0);
    }
    // Hair is what is in the search region, is not skin, and is darker than *this person's own
    // skin*. Darkness against a measured seed rather than against the frame's median, because
    // the frame's median is dominated by whatever is behind the person - a dark wall makes a
    // frame-relative test call the wall hair, and a bright one makes it call the hair nothing.
    let mut plane = Plane::zeros(features.w, features.h);
    for y in 0..i64::from(features.h) {
        for x in 0..i64::from(features.w) {
            if search.at(x, y) <= 0.0 {
                continue;
            }
            let not_skin = 1.0 - skin.plane.at(x, y);
            if not_skin < 0.5 {
                continue;
            }
            let luma = features.luma_at(x, y);
            let nearest = seed
                .samples
                .iter()
                .map(|s| s.luma)
                .fold(f32::INFINITY, f32::min);
            if !nearest.is_finite() || luma >= nearest * HAIR_LUMA_RATIO {
                continue;
            }
            plane.set(x, y, not_skin);
        }
    }
    // Hair is one of the four classes whose boundary is the point, so it is matted rather
    // than thresholded. The band is a fraction of its own area; see ADR-0037 decision 4.
    let band = trimap::band_radius(&plane);
    let map = trimap::build(&plane, band);
    let matted = matting::refine(features, &plane, &map);
    let edge_quality = matting::edge_quality(features, &matted, &map);
    let matted = matted.alpha;
    MaskPlane {
        kind: MaskKind::Hair,
        identity: None,
        plane: matted,
        confidence: CONF_SEEDED * 0.9,
        edge_quality,
        edge: if edge_quality >= 0.6 {
            EdgeQuality::Matted
        } else {
            EdgeQuality::Soft
        },
        reasons: vec![
            MaskReason::SeededByFace,
            MaskReason::Matted,
            MaskReason::HeadUntrained,
        ],
    }
}

fn facial_hair(
    features: &Features,
    faces: &[FaceObservation],
    seed: &SkinSeed,
    skin: &MaskPlane,
) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    if faces.is_empty() || seed.is_empty() {
        return absent(MaskKind::FacialHair, features);
    }
    for (index, face) in faces.iter().enumerate() {
        let Some(sample) = seed.samples.iter().find(|s| s.face == index) else {
            continue;
        };
        // Below the nose landmark, inside the face box. The landmarks are phase 06's and are
        // normalised to the frame, which is why no scaling appears here.
        let nose_y = face
            .landmarks
            .get(2)
            .map_or(face.bbox.y + face.bbox.h * 0.55, |p| p[1]);
        let region = NormBox::from_corners(
            face.bbox.x,
            nose_y,
            face.bbox.x + face.bbox.w,
            face.bbox.y + face.bbox.h,
        );
        let (x0, y0, x1, y1) = box_pixels(&region, features.w, features.h);
        for y in y0..y1 {
            for x in x0..x1 {
                let l = features.luma_at(x, y);
                if l < sample.luma * FACIAL_HAIR_RATIO && skin.plane.at(x, y) < 0.5 {
                    plane.set(x, y, 1.0);
                }
            }
        }
    }
    let present = plane.coverage() > 0.0005;
    MaskPlane {
        kind: MaskKind::FacialHair,
        identity: None,
        plane,
        confidence: if present { CONF_SEEDED * 0.75 } else { 0.0 },
        edge_quality: if present { 0.55 } else { 0.0 },
        edge: if present {
            EdgeQuality::Binary
        } else {
            EdgeQuality::Unknown
        },
        reasons: vec![MaskReason::SeededByFace, MaskReason::HeadUntrained],
    }
}

/// The three eye classes, which share one traversal because they share one geometry.
fn eye_regions(features: &Features, faces: &[FaceObservation]) -> Vec<MaskPlane> {
    let mut eyes = Plane::zeros(features.w, features.h);
    let mut sclera = Plane::zeros(features.w, features.h);
    let mut iris = Plane::zeros(features.w, features.h);
    if faces.is_empty() {
        return vec![
            absent(MaskKind::Eyes, features),
            absent(MaskKind::Sclera, features),
            absent(MaskKind::Iris, features),
        ];
    }
    for face in faces {
        // The eye radius is a proportion of the face box rather than a constant: phase 09
        // needed the same number for its eye-region sharpness and settled on a sixth of the
        // face width, which is what this is.
        let radius = (face.bbox.w * f32::from(features.w as u16) / 6.0).max(2.0);
        for index in [0_usize, 1] {
            let Some(point) = face.landmarks.get(index) else {
                continue;
            };
            let cx = point[0] * features.w as f32;
            let cy = point[1] * features.h as f32;
            paint_disc(&mut eyes, cx, cy, radius);
            // Inside the eye region, the iris is the darkest third and the sclera is the
            // brightest. Measured against the eye's own pixels, which is what makes it work
            // for a brown iris and a blue one.
            let mut values = Vec::new();
            let r = radius.ceil() as i64;
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx as f32).hypot(dy as f32) > radius {
                        continue;
                    }
                    values.push(features.luma_at(cx as i64 + dx, cy as i64 + dy));
                }
            }
            if values.is_empty() {
                continue;
            }
            let mid = median(&values);
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx as f32).hypot(dy as f32) > radius {
                        continue;
                    }
                    let x = cx as i64 + dx;
                    let y = cy as i64 + dy;
                    let l = features.luma_at(x, y);
                    if l < mid * 0.7 {
                        iris.set(x, y, 1.0);
                    } else if l > mid * 1.25 {
                        sclera.set(x, y, 1.0);
                    }
                }
            }
        }
    }
    vec![
        MaskPlane {
            kind: MaskKind::Eyes,
            identity: None,
            plane: eyes,
            confidence: CONF_SEEDED,
            edge_quality: 0.6,
            edge: EdgeQuality::Binary,
            reasons: vec![MaskReason::SeededByFace],
        },
        MaskPlane {
            kind: MaskKind::Sclera,
            identity: None,
            plane: sclera,
            confidence: CONF_SEEDED * 0.8,
            edge_quality: 0.5,
            edge: EdgeQuality::Binary,
            reasons: vec![MaskReason::SeededByFace, MaskReason::ColourGrown],
        },
        MaskPlane {
            kind: MaskKind::Iris,
            identity: None,
            plane: iris,
            confidence: CONF_SEEDED * 0.8,
            edge_quality: 0.5,
            edge: EdgeQuality::Binary,
            reasons: vec![MaskReason::SeededByFace, MaskReason::ColourGrown],
        },
    ]
}

fn lips(features: &Features, faces: &[FaceObservation], seed: &SkinSeed) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    if faces.is_empty() {
        return absent(MaskKind::Lips, features);
    }
    for (index, face) in faces.iter().enumerate() {
        let (Some(left), Some(right)) = (face.landmarks.get(3), face.landmarks.get(4)) else {
            continue;
        };
        let cx = f32::midpoint(left[0], right[0]) * features.w as f32;
        let cy = f32::midpoint(left[1], right[1]) * features.h as f32;
        let half_w = ((right[0] - left[0]).abs() * features.w as f32 * 0.75).max(2.0);
        let half_h = (half_w * 0.45).max(1.5);
        let sample = seed.samples.iter().find(|s| s.face == index);
        for dy in -(half_h.ceil() as i64)..=(half_h.ceil() as i64) {
            for dx in -(half_w.ceil() as i64)..=(half_w.ceil() as i64) {
                let nx = dx as f32 / half_w;
                let ny = dy as f32 / half_h;
                if nx.hypot(ny) > 1.0 {
                    continue;
                }
                let x = cx as i64 + dx;
                let y = cy as i64 + dy;
                // Lips are redder than the same person's cheek. Against their own seed, not
                // against a constant, which is what makes it work across skin tones.
                let redder = sample.is_none_or(|s| features.chroma_at(x, y)[0] > s.cr + 0.01);
                if redder {
                    plane.set(x, y, 1.0);
                }
            }
        }
    }
    MaskPlane {
        kind: MaskKind::Lips,
        identity: None,
        plane,
        confidence: CONF_SEEDED * 0.85,
        edge_quality: 0.55,
        edge: EdgeQuality::Binary,
        reasons: vec![MaskReason::SeededByFace, MaskReason::ColourGrown],
    }
}

fn eyebrows(features: &Features, faces: &[FaceObservation], seed: &SkinSeed) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    if faces.is_empty() {
        return absent(MaskKind::Eyebrows, features);
    }
    for (index, face) in faces.iter().enumerate() {
        let sample = seed.samples.iter().find(|s| s.face == index);
        for eye in [0_usize, 1] {
            let Some(point) = face.landmarks.get(eye) else {
                continue;
            };
            let cx = point[0] * features.w as f32;
            let cy = point[1] * features.h as f32 - face.bbox.h * features.h as f32 * 0.08;
            let half_w = (face.bbox.w * features.w as f32 * 0.18).max(2.0);
            let half_h = (face.bbox.h * features.h as f32 * 0.05).max(1.0);
            for dy in -(half_h.ceil() as i64)..=(half_h.ceil() as i64) {
                for dx in -(half_w.ceil() as i64)..=(half_w.ceil() as i64) {
                    let x = cx as i64 + dx;
                    let y = cy as i64 + dy;
                    let darker =
                        sample.is_none_or(|s| features.luma_at(x, y) < s.luma * FACIAL_HAIR_RATIO);
                    if darker {
                        plane.set(x, y, 1.0);
                    }
                }
            }
        }
    }
    MaskPlane {
        kind: MaskKind::Eyebrows,
        identity: None,
        plane,
        confidence: CONF_SEEDED * 0.8,
        edge_quality: 0.5,
        edge: EdgeQuality::Binary,
        reasons: vec![MaskReason::SeededByFace, MaskReason::ColourGrown],
    }
}

fn teeth(features: &Features, faces: &[FaceObservation]) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    if faces.is_empty() {
        return absent(MaskKind::Teeth, features);
    }
    for face in faces {
        let (Some(left), Some(right)) = (face.landmarks.get(3), face.landmarks.get(4)) else {
            continue;
        };
        let cx = f32::midpoint(left[0], right[0]) * features.w as f32;
        let cy = f32::midpoint(left[1], right[1]) * features.h as f32;
        let half_w = ((right[0] - left[0]).abs() * features.w as f32 * 0.6).max(2.0);
        let half_h = (half_w * 0.35).max(1.0);
        // Teeth are the brightest, least saturated thing inside the mouth. A closed mouth has
        // none, and the emptiness is the right answer rather than a failure.
        let mut values = Vec::new();
        for dy in -(half_h.ceil() as i64)..=(half_h.ceil() as i64) {
            for dx in -(half_w.ceil() as i64)..=(half_w.ceil() as i64) {
                values.push(features.luma_at(cx as i64 + dx, cy as i64 + dy));
            }
        }
        if values.is_empty() {
            continue;
        }
        let mid = median(&values);
        for dy in -(half_h.ceil() as i64)..=(half_h.ceil() as i64) {
            for dx in -(half_w.ceil() as i64)..=(half_w.ceil() as i64) {
                let x = cx as i64 + dx;
                let y = cy as i64 + dy;
                if features.luma_at(x, y) > mid * 1.4 && features.chroma_mag(x, y) < 0.08 {
                    plane.set(x, y, 1.0);
                }
            }
        }
    }
    let present = plane.coverage() > 0.0001;
    MaskPlane {
        kind: MaskKind::Teeth,
        identity: None,
        plane,
        confidence: if present { CONF_SEEDED * 0.7 } else { 0.0 },
        edge_quality: if present { 0.45 } else { 0.0 },
        edge: if present {
            EdgeQuality::Binary
        } else {
            EdgeQuality::Unknown
        },
        reasons: vec![MaskReason::SeededByFace, MaskReason::ColourGrown],
    }
}

/// Clothing, and the one component that may be called a dress. See the module note.
#[allow(clippy::too_many_lines)]
fn clothing(
    features: &Features,
    persons: &[PersonBox],
    faces: &[FaceObservation],
    skin: &MaskPlane,
    hair: &MaskPlane,
) -> Vec<MaskPlane> {
    let mut worn = Plane::zeros(features.w, features.h);
    // Below the chin line, always. A body box contains a head, and a torso does not - the
    // pixels between somebody's ears and their collar are hair, face and whatever is behind
    // them, and a clothing class that claimed the whole box would claim all three. `head_region`
    // in phase 06 is the same proportion read the other way round.
    let chin = |body: &NormBox, face: Option<&FaceObservation>| -> f32 {
        face.map_or(body.y + body.h * CHIN_FRACTION, |f| f.bbox.y + f.bbox.h)
    };
    let boxes: Vec<NormBox> = if persons.is_empty() {
        // No body boxes: a body is below its face, about three face heights tall and two wide.
        // The proportions are phase 06's `person::head_region` read backwards, which is the
        // only place in the product they are written down.
        faces
            .iter()
            .map(|f| {
                NormBox::from_corners(
                    f.bbox.x - f.bbox.w * 0.5,
                    f.bbox.y + f.bbox.h,
                    f.bbox.x + f.bbox.w * 1.5,
                    f.bbox.y + f.bbox.h * 4.0,
                )
            })
            .collect()
    } else {
        persons
            .iter()
            .map(|p| {
                let face = p.face.and_then(|i| faces.get(i));
                let top = chin(&p.bbox, face).max(p.bbox.y);
                NormBox::from_corners(p.bbox.x, top, p.bbox.x + p.bbox.w, p.bbox.y + p.bbox.h)
            })
            .collect()
    };
    if boxes.is_empty() {
        return vec![
            absent(MaskKind::Clothing, features),
            absent(MaskKind::Dress, features),
        ];
    }
    // A body box is a rectangle and a person is not, so most of the box's area at the
    // shoulders and between the arms is whatever is behind them. Taking the whole box is how a
    // clothing mask ends up with a stripe of wall down each side - which phase 19 then lifts.
    //
    // What replaces it is the method this file already uses for skin: measure a seed from a
    // place the class is almost certainly present, then grow by colour. The collar is that
    // place - directly below the chin, on the face's own centre line - and it is available on
    // every frame that has a face, which is every frame this class is produced for.
    let mut seeds: Vec<(f32, f32, f32)> = Vec::new();
    for (index, body) in boxes.iter().enumerate() {
        let face = persons
            .get(index)
            .and_then(|p| p.face)
            .and_then(|i| faces.get(i))
            .or_else(|| faces.get(index));
        let width = face.map_or(body.w * 0.3, |f| f.bbox.w * 0.6);
        let centre = face.map_or(body.x + body.w / 2.0, |f| f.bbox.x + f.bbox.w / 2.0);
        let collar = NormBox::from_corners(
            centre - width / 2.0,
            body.y,
            centre + width / 2.0,
            body.y + (body.h * 0.25).min(0.2),
        );
        let (x0, y0, x1, y1) = box_pixels(&collar, features.w, features.h);
        let mut crs = Vec::new();
        let mut cbs = Vec::new();
        let mut lumas = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                if skin.plane.at(x, y) >= 0.4 || hair.plane.at(x, y) >= 0.4 {
                    continue;
                }
                let c = features.chroma_at(x, y);
                crs.push(c[0]);
                cbs.push(c[1]);
                lumas.push(features.luma_at(x, y));
            }
        }
        if !lumas.is_empty() {
            seeds.push((median(&crs), median(&cbs), median(&lumas)));
        }
    }

    for body in &boxes {
        let (x0, y0, x1, y1) = box_pixels(body, features.w, features.h);
        for y in y0..y1 {
            for x in x0..x1 {
                // Not skin and not hair. Hair falls onto shoulders and a body box contains
                // both, so clothing has to be defined against what has already been claimed -
                // otherwise the same pixels are in two classes and every consumer that unions
                // them counts them twice.
                if skin.plane.at(x, y) >= 0.4 || hair.plane.at(x, y) >= 0.4 {
                    continue;
                }
                if seeds.is_empty() {
                    worn.set(x, y, 1.0);
                    continue;
                }
                let c = features.chroma_at(x, y);
                let l = features.luma_at(x, y);
                let matched = seeds.iter().any(|(cr, cb, luma)| {
                    let ratio = if *luma > 1e-5 { l / *luma } else { 0.0 };
                    (c[0] - cr).hypot(c[1] - cb) <= CLOTH_CHROMA_TOLERANCE
                        && (CLOTH_LUMA_LOW..=CLOTH_LUMA_HIGH).contains(&ratio)
                });
                if matched {
                    worn.set(x, y, 1.0);
                }
            }
        }
    }
    // The dress test, applied to the union rather than per component: a component-wise test
    // would split a dress in two at a sash and call one half clothing.
    let (mut bright, mut total) = (0.0_f64, 0.0_f64);
    let mut lowest_y = 0.0_f32;
    for y in 0..i64::from(features.h) {
        for x in 0..i64::from(features.w) {
            if worn.at(x, y) <= 0.0 {
                continue;
            }
            total += 1.0;
            if features.luma_at(x, y) > features.median_luma * DRESS_LUMA_RATIO
                && features.chroma_mag(x, y) < DRESS_CHROMA_MAX
            {
                bright += 1.0;
            }
            lowest_y = lowest_y.max(y as f32 / features.h.max(1) as f32);
        }
    }
    let dressy = total > 0.0 && bright / total > 0.5 && lowest_y >= DRESS_REACH_Y;
    let empty = Plane::zeros(features.w, features.h);
    if dressy {
        vec![
            MaskPlane {
                kind: MaskKind::Clothing,
                identity: None,
                plane: empty,
                confidence: 0.0,
                edge_quality: 0.0,
                edge: EdgeQuality::Unknown,
                reasons: vec![MaskReason::Derived],
            },
            MaskPlane {
                kind: MaskKind::Dress,
                identity: None,
                plane: worn,
                // Lower than every other class in the file, and the module note says why.
                confidence: CONF_DRESS,
                edge_quality: 0.5,
                edge: EdgeQuality::Binary,
                reasons: vec![MaskReason::ColourGrown, MaskReason::HeadUntrained],
            },
        ]
    } else {
        vec![
            MaskPlane {
                kind: MaskKind::Clothing,
                identity: None,
                plane: worn,
                confidence: CONF_DERIVED,
                edge_quality: 0.5,
                edge: EdgeQuality::Binary,
                reasons: vec![MaskReason::ColourGrown, MaskReason::HeadUntrained],
            },
            MaskPlane {
                kind: MaskKind::Dress,
                identity: None,
                plane: empty,
                confidence: 0.0,
                edge_quality: 0.0,
                edge: EdgeQuality::Unknown,
                reasons: vec![MaskReason::HeadUntrained],
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// The environment classes
// ---------------------------------------------------------------------------

fn sky(features: &Features) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    for y in 0..i64::from(features.h) {
        let ny = y as f32 / features.h.max(1) as f32;
        if ny > SKY_MAX_Y {
            continue;
        }
        for x in 0..i64::from(features.w) {
            let c = features.chroma_at(x, y);
            let flat = features.texture_at(x, y) <= flat_ceiling(features);
            let bright = features.luma_at(x, y) >= features.median_luma;
            // Blue in excess of green, flat, and brighter than the scene. An overcast sky is
            // not blue and is caught by the brightness and flatness terms; a blue wall is
            // caught by the connectivity below.
            if c[1] > SKY_BLUE_EXCESS && flat && bright {
                plane.set(x, y, 1.0);
            }
        }
    }
    let connected = keep_touching_edge(&plane, Edge::Top);
    let present = connected.coverage() > 0.001;
    MaskPlane {
        kind: MaskKind::Sky,
        identity: None,
        plane: connected,
        confidence: if present { CONF_ENVIRONMENT } else { 0.0 },
        edge_quality: if present { 0.65 } else { 0.0 },
        edge: if present {
            EdgeQuality::Binary
        } else {
            EdgeQuality::Unknown
        },
        reasons: vec![MaskReason::ColourGrown, MaskReason::HeadUntrained],
    }
}

fn greenery(features: &Features) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    for y in 0..i64::from(features.h) {
        for x in 0..i64::from(features.w) {
            let c = features.chroma_at(x, y);
            // Green in excess of both red and blue. In the `(r - g, b - g)` plane that is
            // both coordinates being negative by a margin, which is one comparison rather
            // than a hue angle and a saturation.
            if c[0] < -GREEN_EXCESS && c[1] < -GREEN_EXCESS {
                plane.set(x, y, 1.0);
            }
        }
    }
    let present = plane.coverage() > 0.001;
    MaskPlane {
        kind: MaskKind::Greenery,
        identity: None,
        plane,
        confidence: if present { CONF_ENVIRONMENT } else { 0.0 },
        edge_quality: if present { 0.5 } else { 0.0 },
        edge: if present {
            EdgeQuality::Binary
        } else {
            EdgeQuality::Unknown
        },
        reasons: vec![MaskReason::ColourGrown, MaskReason::HeadUntrained],
    }
}

fn water(features: &Features) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    for y in 0..i64::from(features.h) {
        let ny = y as f32 / features.h.max(1) as f32;
        if ny < 0.4 {
            continue;
        }
        for x in 0..i64::from(features.w) {
            let c = features.chroma_at(x, y);
            let flat = features.texture_at(x, y) <= flat_ceiling(features);
            let dim = features.luma_at(x, y) < features.median_luma;
            if c[1] > SKY_BLUE_EXCESS && flat && dim {
                plane.set(x, y, 1.0);
            }
        }
    }
    let present = plane.coverage() > 0.002;
    MaskPlane {
        kind: MaskKind::Water,
        identity: None,
        plane,
        confidence: if present { CONF_ENVIRONMENT * 0.8 } else { 0.0 },
        edge_quality: if present { 0.45 } else { 0.0 },
        edge: if present {
            EdgeQuality::Binary
        } else {
            EdgeQuality::Unknown
        },
        reasons: vec![MaskReason::ColourGrown, MaskReason::HeadUntrained],
    }
}

fn floor(features: &Features) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    for y in 0..i64::from(features.h) {
        let ny = y as f32 / features.h.max(1) as f32;
        if ny < FLOOR_MIN_Y {
            continue;
        }
        for x in 0..i64::from(features.w) {
            if features.texture_at(x, y) <= flat_ceiling(features) {
                plane.set(x, y, 1.0);
            }
        }
    }
    let connected = keep_touching_edge(&plane, Edge::Bottom);
    let present = connected.coverage() > 0.002;
    MaskPlane {
        kind: MaskKind::Floor,
        identity: None,
        plane: connected,
        confidence: if present { CONF_ENVIRONMENT * 0.7 } else { 0.0 },
        edge_quality: if present { 0.4 } else { 0.0 },
        edge: if present {
            EdgeQuality::Binary
        } else {
            EdgeQuality::Unknown
        },
        reasons: vec![MaskReason::ColourGrown, MaskReason::HeadUntrained],
    }
}

fn window(features: &Features) -> MaskPlane {
    let mut plane = Plane::zeros(features.w, features.h);
    for y in 0..i64::from(features.h) {
        for x in 0..i64::from(features.w) {
            if features.luma_at(x, y) > features.median_luma * WINDOW_LUMA_RATIO
                && features.chroma_mag(x, y) < WINDOW_CHROMA_MAX
            {
                plane.set(x, y, 1.0);
            }
        }
    }
    let present = plane.coverage() > 0.0005;
    MaskPlane {
        kind: MaskKind::Window,
        identity: None,
        plane,
        confidence: if present { CONF_ENVIRONMENT } else { 0.0 },
        edge_quality: if present { 0.6 } else { 0.0 },
        edge: if present {
            EdgeQuality::Binary
        } else {
            EdgeQuality::Unknown
        },
        reasons: vec![MaskReason::ColourGrown, MaskReason::HeadUntrained],
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// The gradient at or below which a pixel counts as flat.
///
/// The frame's own median scaled by [`FLAT_TEXTURE_RATIO`], with [`FLAT_TEXTURE_FLOOR`] under
/// it so a frame with no texture at all still has a usable threshold.
fn flat_ceiling(features: &Features) -> f32 {
    (features.median_texture * FLAT_TEXTURE_RATIO).max(FLAT_TEXTURE_FLOOR)
}

/// Which edge a region has to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Top,
    Bottom,
}

/// Keep only the components of a region that touch an edge of the frame.
///
/// This is what separates sky from a blue wall and a floor from a table top, and it is the
/// same connectivity argument the skin growth makes. A region that is sky-coloured, flat and
/// bright but does not reach the top of the photograph is something else.
fn keep_touching_edge(plane: &Plane, edge: Edge) -> Plane {
    let labels = crate::mask::instance::label_components(plane);
    let mut keep = std::collections::BTreeSet::new();
    let y = match edge {
        Edge::Top => 0,
        Edge::Bottom => i64::from(plane.h) - 1,
    };
    for x in 0..i64::from(plane.w) {
        let id = labels.at(x, y);
        if id > 0 {
            keep.insert(id);
        }
    }
    labels.select(&keep)
}

/// A box, inset by a fraction of its own size on every side.
fn inset(b: &NormBox, fraction: f32) -> NormBox {
    NormBox::from_corners(
        b.x + b.w * fraction,
        b.y + b.h * fraction,
        b.x + b.w * (1.0 - fraction),
        b.y + b.h * (1.0 - fraction),
    )
}

/// A normalised box as pixel bounds on a grid, clamped.
fn box_pixels(b: &NormBox, w: u32, h: u32) -> (i64, i64, i64, i64) {
    let x0 = ((b.x * w as f32).floor() as i64).clamp(0, i64::from(w));
    let y0 = ((b.y * h as f32).floor() as i64).clamp(0, i64::from(h));
    let x1 = (((b.x + b.w) * w as f32).ceil() as i64).clamp(x0, i64::from(w));
    let y1 = (((b.y + b.h) * h as f32).ceil() as i64).clamp(y0, i64::from(h));
    (x0, y0, x1, y1)
}

/// Paint the inscribed ellipse of a normalised box.
fn paint_ellipse(plane: &mut Plane, b: &NormBox, value: f32) {
    let (x0, y0, x1, y1) = box_pixels(b, plane.w, plane.h);
    let cx = (x0 + x1) as f32 / 2.0;
    let cy = (y0 + y1) as f32 / 2.0;
    let rx = ((x1 - x0) as f32 / 2.0).max(0.5);
    let ry = ((y1 - y0) as f32 / 2.0).max(0.5);
    for y in y0..y1 {
        for x in x0..x1 {
            let nx = (x as f32 - cx) / rx;
            let ny = (y as f32 - cy) / ry;
            if nx.hypot(ny) <= 1.0 {
                plane.set(x, y, plane.at(x, y).max(value));
            }
        }
    }
}

/// Paint a disc in pixel coordinates.
fn paint_disc(plane: &mut Plane, cx: f32, cy: f32, r: f32) {
    let ri = r.ceil() as i64;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if (dx as f32).hypot(dy as f32) <= r {
                plane.set(cx as i64 + dx, cy as i64 + dy, 1.0);
            }
        }
    }
}

/// A class that is not present in this photograph.
///
/// Zero everywhere with confidence zero and [`EdgeQuality::Unknown`], which is different from
/// a class that is present and badly determined. A later phase reading an absent mask gets
/// nothing to edit; reading a bad one gets something it must be careful with.
fn absent(kind: MaskKind, features: &Features) -> MaskPlane {
    MaskPlane {
        kind,
        identity: None,
        plane: Plane::zeros(features.w, features.h),
        confidence: 0.0,
        edge_quality: 0.0,
        edge: EdgeQuality::Unknown,
        reasons: vec![MaskReason::NoFaces],
    }
}

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    fn flat(w: u32, h: u32, rgb: [f32; 3]) -> MaskFrame {
        let mut buffer = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..(w * h) {
            buffer.extend_from_slice(&rgb);
        }
        MaskFrame::new(buffer, w, h)
    }

    #[test]
    fn the_head_is_not_consulted() {
        assert!(!SEG_HEAD_TRAINED);
        let features = Features::measure(&flat(8, 8, [0.5, 0.5, 0.5]));
        assert!(class_hint(&features, 4, 4).is_none());
    }

    #[test]
    fn a_frame_with_no_faces_produces_no_skin_rather_than_a_guess() {
        let frame = flat(32, 32, [0.4, 0.3, 0.25]);
        let planes = run(&frame, &[], &[]);
        let skin = planes
            .iter()
            .find(|p| p.kind == MaskKind::Skin)
            .expect("skin plane");
        assert!(skin.plane.is_empty());
        assert_eq!(skin.confidence, 0.0);
        assert!(skin.reasons.contains(&MaskReason::NoFaces));
    }

    #[test]
    fn sky_must_reach_the_top_of_the_frame() {
        // A blue patch in the middle of a grey frame is not sky.
        let mut frame = flat(64, 64, [0.4, 0.4, 0.4]);
        for y in 20..40 {
            for x in 20..40 {
                let base = ((y * 64 + x) * 3) as usize;
                frame.rgb[base] = 0.35;
                frame.rgb[base + 1] = 0.42;
                frame.rgb[base + 2] = 0.75;
            }
        }
        let features = Features::measure(&frame);
        assert!(sky(&features).plane.is_empty());
    }

    #[test]
    fn there_is_no_skin_colour_constant_in_this_file() {
        // The fairness argument, as a test rather than a sentence. Every constant in this
        // module is a tolerance, a ratio or a proportion; none of them is a chromaticity.
        let source = include_str!("segment.rs");
        for line in source.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("const ") {
                continue;
            }
            for banned in [
                "SKIN_CR",
                "SKIN_CB",
                "SKIN_HUE",
                "IDEAL_SKIN",
                "SKIN_TARGET",
            ] {
                assert!(
                    !trimmed.contains(banned),
                    "a skin colour constant appeared: {trimmed}"
                );
            }
        }
    }
}
