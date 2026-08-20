//! Synthetic frames whose regions are painted into the pixels, and the ground truth that says
//! where they are.
//!
//! # Why the answer is painted rather than asserted
//!
//! Phase 10 established this and phases 15 and 16 followed it: a fixture whose right answer is
//! *authored beside* the pixels proves that the harness agrees with itself. A fixture whose
//! right answer is *painted into* the pixels and read back through the real pipeline proves
//! that the arithmetic recovers something that is genuinely there.
//!
//! So [`AuthoredScene`] draws a person - an ellipse of one reflectance for skin, a darker
//! annulus above it for hair, a rectangle below it for clothing - onto a background of another
//! colour, and hands back both the frame and the exact planes it drew. The gates in
//! `tests/eval/mask_eval.rs` measure the pipeline's output against those planes with
//! [`crate::mask::algebra::iou`].
//!
//! **This proves the arithmetic and says nothing about a wedding photograph.** There is no
//! labelled wedding data in this repository - section 9's DATA task asks for 12,000 labelled
//! frames including veils, ethnic attire and varied skin tones, and it did not happen and
//! cannot happen here. That is condition C1 of the phase 18 exit report and it is a Sev 2
//! trigger.
//!
//! # The five reflectances
//!
//! [`SKIN_REFLECTANCES`] is five *reflectance values*, not five people. It is the same
//! construction phase 15 and phase 16 use for their fairness gates and it carries the same
//! caveat, which `docs/skin-fairness.md` states in the product's own words: it proves the
//! mechanism is seeded from the frame's own pixels rather than from a constant, and it proves
//! nothing about a real person.

use aura_core::PhotoId;

use crate::contract::mask::MaskKind;
use crate::face::align::{Landmarks, Pose};
use crate::face::detect::{FoundBy, NormBox};
use crate::face::person::{Association, PeopleCount, PersonBox};
use crate::face::quality::FaceQuality;
use crate::face::{FaceObservation, FramePeople};
use crate::mask::algebra::Plane;
use crate::mask::MaskFrame;

/// The width every authored frame is drawn at.
pub const WIDTH: u32 = 384;
/// The height every authored frame is drawn at.
pub const HEIGHT: u32 = 256;

/// Five skin reflectances, dark to light.
///
/// Luminance multipliers on one illuminant, spanning roughly the range the Monk scale covers.
/// **They are reflectances and not people**; see the module note.
pub const SKIN_REFLECTANCES: [f32; 5] = [0.09, 0.16, 0.28, 0.42, 0.58];

/// What was painted, and where.
#[derive(Debug, Clone)]
pub struct AuthoredScene {
    /// The photograph.
    pub image_id: PhotoId,
    /// The pixels, in the linear working space.
    pub frame: MaskFrame,
    /// Phase 06's answer, as it would have been if the detector worked.
    pub people: FramePeople,
    /// The regions that were painted, by class.
    pub truth: Vec<(MaskKind, Plane)>,
}

impl AuthoredScene {
    /// The ground-truth plane for a class, or an empty one.
    #[must_use]
    pub fn truth_of(&self, kind: MaskKind) -> Plane {
        self.truth
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or_else(|| Plane::zeros(WIDTH, HEIGHT), |(_, p)| p.clone())
    }
}

/// How the scene is lit and what is behind the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// A flat mid-grey wall. The easy case, and the one the mIoU gates are measured on.
    Wall,
    /// Sky above and greenery below. The outdoor daylight reference wedding.
    Garden,
    /// A wall the same luminance as the subject's hair. The hard case: section 10.1's
    /// "dark-suit boundaries", where a boundary exists semantically and barely exists in the
    /// pixels, and where `edge_quality` has to come back low rather than come back wrong.
    LowContrast,
}

/// Draw one person against a backdrop.
///
/// `reflectance` is an index into [`SKIN_REFLECTANCES`]. `people` carries the boxes and
/// landmarks phase 06 *would* have produced - the detector in this build finds no faces, and
/// supplying them is what lets this phase's arithmetic be measured without waiting for phase
/// 06's condition C1 to close.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn one_person(reflectance: usize, backdrop: Backdrop) -> AuthoredScene {
    let skin_luma = SKIN_REFLECTANCES
        .get(reflectance.min(SKIN_REFLECTANCES.len() - 1))
        .copied()
        .unwrap_or(0.28);

    let mut rgb = vec![0.0_f32; (WIDTH as usize) * (HEIGHT as usize) * 3];
    let mut skin = Plane::zeros(WIDTH, HEIGHT);
    let mut hair = Plane::zeros(WIDTH, HEIGHT);
    let mut clothing = Plane::zeros(WIDTH, HEIGHT);
    let mut sky = Plane::zeros(WIDTH, HEIGHT);
    let mut greenery = Plane::zeros(WIDTH, HEIGHT);

    // The face, as a normalised box. Everything below is drawn relative to it, so a change to
    // the geometry moves the pixels and the truth together.
    let face_box = NormBox::from_corners(0.40, 0.16, 0.60, 0.46);
    let fx0 = face_box.x * WIDTH as f32;
    let fy0 = face_box.y * HEIGHT as f32;
    let fw = face_box.w * WIDTH as f32;
    let fh = face_box.h * HEIGHT as f32;
    let cx = fx0 + fw / 2.0;
    let cy = fy0 + fh / 2.0;

    let hair_luma = skin_luma * 0.35;
    let cloth_luma = skin_luma * 0.5;

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let nx = x as f32 / WIDTH as f32;
            let ny = y as f32 / HEIGHT as f32;
            let index = ((y as usize) * (WIDTH as usize) + (x as usize)) * 3;

            // ---- backdrop -------------------------------------------------------------
            let mut colour = match backdrop {
                Backdrop::Wall => [0.34, 0.34, 0.34],
                Backdrop::LowContrast => {
                    // Within a few per cent of the hair, which is exactly the case where a
                    // boundary is semantically obvious and colorimetrically nearly absent.
                    let v = hair_luma * 1.05;
                    [v, v, v]
                }
                Backdrop::Garden => {
                    if ny < 0.42 {
                        sky.set(i64::from(x), i64::from(y), 1.0);
                        // Blue in excess of green, flat, brighter than the scene: what
                        // `segment::sky` measures.
                        [0.42, 0.52, 0.78]
                    } else {
                        greenery.set(i64::from(x), i64::from(y), 1.0);
                        [0.16, 0.30, 0.13]
                    }
                }
            };

            // A little structure everywhere, so the frame's median gradient is not zero and
            // the texture-relative thresholds have something to be relative to.
            let grain = if (x / 8 + y / 8) % 2 == 0 { 1.0 } else { 0.97 };
            for channel in &mut colour {
                *channel *= grain;
            }

            // ---- clothing -------------------------------------------------------------
            // The torso starts at the chin line, which is where a torso starts. It used to
            // start higher and overlap the hair, and the overlap made every person class
            // ambiguous about which one owned those pixels - which is a property of the
            // fixture rather than of a photograph, and it was measuring the fixture.
            let body = NormBox::from_corners(0.33, 0.47, 0.67, 1.0);
            if nx >= body.x && nx <= body.x + body.w && ny >= body.y {
                clothing.set(i64::from(x), i64::from(y), 1.0);
                colour = [cloth_luma * 0.9, cloth_luma, cloth_luma * 1.1];
            }

            // ---- hair -----------------------------------------------------------------
            // An annulus around and above the face ellipse, drawn over the clothing and under
            // the skin. The truth planes follow the paint order exactly - a pixel belongs to
            // the class that is visible in it and to no other - because a consumer that unions
            // the person classes would otherwise count the overlap twice.
            let hx = (x as f32 - cx) / (fw * 0.78);
            let hy = (y as f32 - (cy - fh * 0.30)) / (fh * 0.78);
            if hx.hypot(hy) <= 1.0 {
                clothing.set(i64::from(x), i64::from(y), 0.0);
                hair.set(i64::from(x), i64::from(y), 1.0);
                colour = [hair_luma, hair_luma * 0.97, hair_luma * 0.93];
            }

            // ---- skin -----------------------------------------------------------------
            let sxn = (x as f32 - cx) / (fw * 0.5);
            let syn = (y as f32 - cy) / (fh * 0.5);
            if sxn.hypot(syn) <= 1.0 {
                hair.set(i64::from(x), i64::from(y), 0.0);
                skin.set(i64::from(x), i64::from(y), 1.0);
                // Skin is warmer than neutral: red above green above blue, at the reflectance
                // this scene was asked for. The *ratio* is what `SkinSeed` measures and the
                // *level* is what varies across the five, which is the whole point of the
                // fairness fixture.
                colour = [skin_luma * 1.22, skin_luma, skin_luma * 0.82];
            }

            if let Some(slot) = rgb.get_mut(index..index + 3) {
                slot.copy_from_slice(&colour);
            }
        }
    }

    let image_id = PhotoId::new();
    let landmarks: Landmarks = [
        [
            face_box.x + face_box.w * 0.30,
            face_box.y + face_box.h * 0.38,
        ],
        [
            face_box.x + face_box.w * 0.70,
            face_box.y + face_box.h * 0.38,
        ],
        [
            face_box.x + face_box.w * 0.50,
            face_box.y + face_box.h * 0.58,
        ],
        [
            face_box.x + face_box.w * 0.35,
            face_box.y + face_box.h * 0.78,
        ],
        [
            face_box.x + face_box.w * 0.65,
            face_box.y + face_box.h * 0.78,
        ],
    ];
    let face = observation(image_id, face_box, landmarks);
    let person = PersonBox {
        bbox: NormBox::from_corners(0.30, 0.14, 0.70, 1.0),
        det_score: 0.9,
        face: Some(0),
        assoc_score: 0.9,
        assoc_kind: Association::Contained,
    };

    let mut truth = vec![
        (MaskKind::Skin, skin.clone()),
        (MaskKind::Face, skin),
        (MaskKind::Hair, hair.clone()),
        (MaskKind::Clothing, clothing.clone()),
    ];
    // The subject is everything that was painted as a person, which is what makes the subject
    // gate a statement about composition rather than a second drawing.
    let mut subject = Plane::zeros(WIDTH, HEIGHT);
    for (_, plane) in &truth {
        subject = crate::mask::algebra::union(&subject, plane);
    }
    subject = crate::mask::algebra::union(&subject, &clothing);
    truth.push((MaskKind::Subject, subject.clone()));
    truth.push((MaskKind::Background, crate::mask::algebra::invert(&subject)));
    if backdrop == Backdrop::Garden {
        truth.push((
            MaskKind::Sky,
            crate::mask::algebra::subtract(&sky, &subject),
        ));
        truth.push((
            MaskKind::Greenery,
            crate::mask::algebra::subtract(&greenery, &subject),
        ));
    }

    AuthoredScene {
        image_id,
        frame: MaskFrame::new(rgb, WIDTH, HEIGHT),
        people: FramePeople {
            image_id,
            faces: vec![face],
            persons: vec![person],
            count: people_count(1),
            tiled: false,
            tile_reasons: Vec::new(),
            passes: 0,
            pixel_tier: 2,
            pixel_source: "aura_render",
            infer_ms: 0.0,
        },
        truth,
    }
}

/// Two people side by side, for the group-photo bleed test.
///
/// Section 10.1: "in group photos, per-identity skin masks do not bleed between adjacent
/// people." The two are drawn at *different* reflectances, so a bleed is visible in the pixels
/// as well as in the assignment.
#[must_use]
pub fn two_people() -> AuthoredScene {
    let mut scene = one_person(1, Backdrop::Wall);
    let second_box = NormBox::from_corners(0.70, 0.18, 0.88, 0.46);
    let skin_luma = SKIN_REFLECTANCES.get(4).copied().unwrap_or(0.58);

    let fx0 = second_box.x * WIDTH as f32;
    let fy0 = second_box.y * HEIGHT as f32;
    let fw = second_box.w * WIDTH as f32;
    let fh = second_box.h * HEIGHT as f32;
    let cx = fx0 + fw / 2.0;
    let cy = fy0 + fh / 2.0;

    let mut skin = scene.truth_of(MaskKind::Skin);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let sxn = (x as f32 - cx) / (fw * 0.5);
            let syn = (y as f32 - cy) / (fh * 0.5);
            if sxn.hypot(syn) > 1.0 {
                continue;
            }
            skin.set(i64::from(x), i64::from(y), 1.0);
            let index = ((y as usize) * (WIDTH as usize) + (x as usize)) * 3;
            if let Some(slot) = scene.frame.rgb.get_mut(index..index + 3) {
                slot.copy_from_slice(&[skin_luma * 1.22, skin_luma, skin_luma * 0.82]);
            }
        }
    }

    let landmarks: Landmarks = [
        [
            second_box.x + second_box.w * 0.30,
            second_box.y + second_box.h * 0.38,
        ],
        [
            second_box.x + second_box.w * 0.70,
            second_box.y + second_box.h * 0.38,
        ],
        [
            second_box.x + second_box.w * 0.50,
            second_box.y + second_box.h * 0.58,
        ],
        [
            second_box.x + second_box.w * 0.35,
            second_box.y + second_box.h * 0.78,
        ],
        [
            second_box.x + second_box.w * 0.65,
            second_box.y + second_box.h * 0.78,
        ],
    ];
    scene
        .people
        .faces
        .push(observation(scene.image_id, second_box, landmarks));
    scene.people.persons.push(PersonBox {
        bbox: NormBox::from_corners(0.68, 0.16, 0.92, 1.0),
        det_score: 0.85,
        face: Some(1),
        assoc_score: 0.85,
        assoc_kind: Association::Contained,
    });

    for entry in &mut scene.truth {
        if entry.0 == MaskKind::Skin {
            entry.1 = skin.clone();
        }
    }
    scene
}

/// A frame with nobody in it. The details, the flat-lays and the venue shots.
#[must_use]
pub fn no_people() -> AuthoredScene {
    let mut rgb = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let grain = if (x / 6 + y / 6) % 2 == 0 { 0.36 } else { 0.33 };
            rgb.extend_from_slice(&[grain, grain * 0.98, grain * 0.95]);
        }
    }
    let image_id = PhotoId::new();
    AuthoredScene {
        image_id,
        frame: MaskFrame::new(rgb, WIDTH, HEIGHT),
        people: FramePeople {
            image_id,
            faces: Vec::new(),
            persons: Vec::new(),
            count: people_count(1),
            tiled: false,
            tile_reasons: Vec::new(),
            passes: 0,
            pixel_tier: 2,
            pixel_source: "aura_render",
            infer_ms: 0.0,
        },
        truth: Vec::new(),
    }
}

/// A people count that names what the fixture drew.
///
/// Built rather than defaulted, because `PeopleCount` carries reasons and invariant 2 applies
/// to a fixture as much as to a measurement - a count with no reasons is a count nothing can
/// explain, and a test that accepted one would accept one from the real detector too.
fn people_count(faces: usize) -> PeopleCount {
    PeopleCount {
        faces,
        bodies: faces,
        headless: 0,
        bodiless_faces: 0,
        total: faces,
        reasons: vec!["authored fixture".to_string()],
    }
}

/// A quality verdict that lets the face vote.
///
/// The fixture's faces are drawn sharp, frontal and unoccluded, so the verdict says so. It is
/// authored rather than measured for the same reason `template` is `None`: measuring it here
/// would measure phase 06's placeholder head instead of this phase's arithmetic.
fn usable_quality() -> FaceQuality {
    FaceQuality {
        usable: 0.9,
        blur: 0.1,
        occlusion: 0.0,
        pose_penalty: 0.05,
        px_height: 80,
        model_usable: None,
        votes: true,
        reasons: vec!["authored fixture".to_string()],
        quality_ver: 1,
    }
}

/// A face observation carrying the geometry and nothing that claims to be a measurement.
///
/// `template` is `None` and `det_score` is a constant. This is a *fixture*, and giving it a
/// plausible recognition template would let a test accidentally measure phase 06's placeholder
/// recogniser instead of this phase's arithmetic.
fn observation(image_id: PhotoId, bbox: NormBox, landmarks: Landmarks) -> FaceObservation {
    FaceObservation {
        face_id: aura_core::FaceId::new(),
        image_id,
        bbox,
        person_bbox: None,
        landmarks,
        det_score: 0.92,
        pose: Pose::default(),
        quality: usable_quality(),
        area_frac: bbox.area_frac(),
        centrality: 0.8,
        sharpness: 0.7,
        template: None,
        crop: None,
        found_by: FoundBy::Full,
        px_height: (bbox.h * HEIGHT as f32) as u32,
        child_score: 0.0,
    }
}
