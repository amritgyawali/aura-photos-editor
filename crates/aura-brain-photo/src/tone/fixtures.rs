//! Synthetic weddings whose light is known by construction.
//!
//! Every section 10.1 gate in this phase is measured against these. Section 8's step 1 asks
//! DATA for "RAW + expert final edits with exposure/WB parameters across traditions and
//! lighting types"; there is no such corpus in this repository and there is no way to invent
//! one. What *can* be built - and what these are - is a set of frames where the illuminant
//! and the surface reflectances were chosen, so the right answer is arithmetic rather than
//! opinion.
//!
//! **That proves the algorithms and says nothing about real photographs.** Condition C2 of
//! `docs/progress/PHASE-15-EXIT.md`, and the gate prints it on every run.
//!
//! ## How a frame is built
//!
//! Physically, in one direction only:
//!
//! ```text
//! reflectance (a surface)  x  illuminant (a light)  =  linear RGB  ->  sRGB encode  -> u8
//! ```
//!
//! So a fixture states what the surfaces *are* and what the light *is*, and the frame follows.
//! A solver that recovers the illuminant has, by construction, recovered the surfaces - which
//! is exactly the property section 6.2's scoring is built on and exactly the property a
//! fixture painted by eye could not guarantee.
//!
//! ## The five skin reflectances
//!
//! [`SKIN_TONES`] spans a range of skin reflectances at roughly even steps in lightness. They
//! are **reflectances**, not appearances, and they carry no label: the fairness gate in
//! `tests/eval/tone_eval.rs` groups by index because a measurement needs groups, and nothing
//! in the product ever sees them. Section 6.3's Monk-scale buckets are that grouping and they
//! live in the harness for that reason.
//!
//! The property the gate asserts is the one that matters and it is checkable here: **the
//! solver's error must not depend on which of the five was used.** A solver that was more
//! accurate on light skin than on dark skin would fail, and it would fail for the arithmetic
//! reason such solvers fail in the field - a chromaticity error read against a lower
//! luminance is a larger relative error, and any step that normalises by luminance without
//! saying so bakes that in.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::{FaceRef, ImageSubjects};
use aura_core::{AttrFlags, FaceId, IdentityId, SceneId};
use aura_raw::colour::illuminant::{cct_to_uv, linear_srgb_to_uv, uv_to_linear_srgb};

use crate::tone::illuminant::AsShot;

/// Five skin reflectances, light to dark.
///
/// Linear sRGB reflectances with a plausible skin hue - roughly `R > G > B` with the ratio
/// held nearly constant - so the five differ mostly in lightness, which is what a
/// tone-group comparison has to isolate. They are not measurements of anybody; they are five
/// points on a line through the region human skin occupies.
pub const SKIN_TONES: [[f32; 3]; 5] = [
    [0.62, 0.47, 0.39],
    [0.44, 0.32, 0.26],
    [0.30, 0.21, 0.17],
    [0.19, 0.13, 0.10],
    [0.11, 0.074, 0.058],
];

/// A neutral surface: a wedding dress, a tablecloth, a sheet of paper.
///
/// Not 1.0. A real white surface reflects about 85 % and a fixture at full white would clip
/// under any illuminant brighter than the dimmest, which would hide the thing the neutral
/// detector actually has to do - find an unclipped bright uniform region.
pub const NEUTRAL_REFLECTANCE: [f32; 3] = [0.85, 0.85, 0.85];

/// A mid-grey background.
pub const BACKGROUND_REFLECTANCE: [f32; 3] = [0.24, 0.23, 0.22];

/// A strongly coloured surface, for the frames where grey-world is catastrophically wrong.
///
/// A deep red mandap cloth. A frame that is mostly this is the case section 6.2's
/// multi-hypothesis design exists for: grey-world says the light is cyan, and it is not.
pub const MANDAP_REFLECTANCE: [f32; 3] = [0.42, 0.09, 0.07];

/// What a fixture knows about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Truth {
    /// The illuminant the frame was rendered under, in `u'v'`.
    pub illuminant_uv: [f32; 2],
    /// Its correlated colour temperature, when it has an honest one.
    pub illuminant_k: f32,
    /// The second illuminant, for a mixed-light frame.
    pub second_uv: Option<[f32; 2]>,
    /// The skin reflectance used, as an index into [`SKIN_TONES`].
    pub skin_index: Option<usize>,
    /// The reflectance of the skin, if any.
    pub skin_reflectance: Option<[f32; 3]>,
    /// The perceptual luminance the faces were rendered at.
    pub subject_luma: f32,
    /// True when the frame contains a neutral surface.
    pub has_neutral: bool,
    /// True when the frame is deliberately mixed-light.
    pub mixed: bool,
    /// True when the light is a saturated creative choice.
    pub coloured: bool,
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
    /// What the camera would have recorded.
    pub as_shot: AsShot,
    /// What is known by construction.
    pub truth: Truth,
}

/// How a fixture is described before it is painted.
#[derive(Debug, Clone)]
pub struct Spec {
    /// Frame width.
    pub width: usize,
    /// Frame height.
    pub height: usize,
    /// The main illuminant, in `u'v'`.
    pub illuminant_uv: [f32; 2],
    /// A second illuminant covering the right-hand third, for mixed-light frames.
    pub second_uv: Option<[f32; 2]>,
    /// The background surface.
    pub background: [f32; 3],
    /// The skin reflectance, and how many faces to paint.
    pub skin: Option<([f32; 3], usize)>,
    /// How bright the whole frame is, as a multiplier on the illuminant. One is correct.
    pub exposure: f32,
    /// True to paint a large neutral surface across the lower third.
    pub neutral: bool,
    /// True to paint a small blown highlight, which a specular exclusion must ignore.
    pub specular: bool,
    /// The scene phase 07 would report.
    pub scene: SceneId,
    /// The attributes phase 07 would report.
    pub attrs: AttrFlags,
    /// What the camera recorded.
    pub as_shot: AsShot,
    /// True when the light is a saturated creative choice.
    pub coloured: bool,
    /// Which entry of [`SKIN_TONES`] the skin came from, when it came from there.
    pub skin_index: Option<usize>,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            width: 384,
            height: 256,
            illuminant_uv: cct_to_uv(5500.0),
            second_uv: None,
            background: BACKGROUND_REFLECTANCE,
            skin: None,
            exposure: 1.0,
            neutral: false,
            specular: false,
            scene: SceneId::Candid,
            attrs: AttrFlags::EMPTY,
            as_shot: AsShot {
                temperature_k: 5500.0,
                tint: 0.0,
                flash_fired: false,
            },
            coloured: false,
            skin_index: None,
        }
    }
}

/// Encode one linear value as 8-bit sRGB.
#[must_use]
pub fn encode(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// What one surface looks like under one light, in linear RGB.
#[must_use]
pub fn under(reflectance: [f32; 3], illuminant_uv: [f32; 2], exposure: f32) -> [f32; 3] {
    let light = uv_to_linear_srgb(illuminant_uv);
    [
        reflectance[0] * light[0] * exposure,
        reflectance[1] * light[1] * exposure,
        reflectance[2] * light[2] * exposure,
    ]
}

/// Paint one frame from a specification.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render(spec: &Spec) -> Frame {
    let mut rgb = vec![0u8; spec.width * spec.height * 3];
    let background = under(spec.background, spec.illuminant_uv, spec.exposure);
    let second_background = spec
        .second_uv
        .map(|uv| under(spec.background, uv, spec.exposure));
    let neutral = under(NEUTRAL_REFLECTANCE, spec.illuminant_uv, spec.exposure);

    // The background, with the second illuminant on the right-hand third when there is one.
    // A hard boundary rather than a gradient: the mixed-light detector groups grid cells, and
    // a fixture with a gradient would be testing where the detector draws a line rather than
    // whether it finds two lights.
    let boundary = spec.width * 2 / 3;
    for y in 0..spec.height {
        for x in 0..spec.width {
            let base = (y * spec.width + x) * 3;
            let colour = if x >= boundary {
                second_background.unwrap_or(background)
            } else if spec.neutral && y >= spec.height * 2 / 3 {
                neutral
            } else {
                background
            };
            // A gentle texture so that a block's chroma variance is a measurement rather than
            // exactly zero. Deterministic, and small enough not to move any mean.
            let texture = 1.0 + 0.02 * (((x * 7 + y * 13) % 11) as f32 / 11.0 - 0.5);
            for channel in 0..3 {
                if let (Some(slot), Some(value)) =
                    (rgb.get_mut(base + channel), colour.get(channel))
                {
                    *slot = encode(*value * texture);
                }
            }
        }
    }

    // The faces: squares along the middle band, painted with the skin reflectance under the
    // frame's own light.
    let mut faces = Vec::new();
    let mut prominence = std::collections::BTreeMap::new();
    let mut subject_linear = [0.0f32; 3];
    if let Some((skin, count)) = spec.skin {
        let lit = under(skin, spec.illuminant_uv, spec.exposure);
        subject_linear = lit;
        let side = spec.height / 4;
        for index in 0..count.max(1) {
            let centre_x = spec.width * (index + 1) / (count.max(1) + 1);
            let centre_y = spec.height / 2;
            let x0 = centre_x.saturating_sub(side / 2);
            let y0 = centre_y.saturating_sub(side / 2);
            let x1 = (x0 + side).min(spec.width);
            let y1 = (y0 + side).min(spec.height);
            for y in y0..y1 {
                for x in x0..x1 {
                    let base = (y * spec.width + x) * 3;
                    // A little shading across the face, so the skin sampler's luminance trim
                    // has something to trim. The mean is unchanged by construction: the ramp
                    // is symmetric about the centre of the box.
                    let ramp = 1.0 + 0.10 * ((x - x0) as f32 / (x1 - x0).max(1) as f32 - 0.5);
                    for channel in 0..3 {
                        if let (Some(slot), Some(value)) =
                            (rgb.get_mut(base + channel), lit.get(channel))
                        {
                            *slot = encode(*value * ramp);
                        }
                    }
                }
            }
            let bbox = CropRect {
                x: x0 as f32 / spec.width as f32,
                y: y0 as f32 / spec.height as f32,
                w: (x1 - x0) as f32 / spec.width as f32,
                h: (y1 - y0) as f32 / spec.height as f32,
            };
            let identity = IdentityId::new();
            faces.push(FaceRef {
                face_id: FaceId::new(),
                identity_id: Some(identity),
                bbox,
                eyes: [
                    [bbox.x + bbox.w * 0.35, bbox.y + bbox.h * 0.40],
                    [bbox.x + bbox.w * 0.65, bbox.y + bbox.h * 0.40],
                ],
                area_frac: bbox.w * bbox.h,
                centrality: 0.8,
                sharpness: 0.9,
                quality: 0.9,
                votes: true,
            });
            prominence.insert(identity, 1.0 / count.max(1) as f32);
        }
    }

    // One small blown highlight, for the frames that test the specular exclusion.
    if spec.specular {
        let side = spec.height / 24;
        let x0 = spec.width / 8;
        let y0 = spec.height / 8;
        for y in y0..(y0 + side).min(spec.height) {
            for x in x0..(x0 + side).min(spec.width) {
                let base = (y * spec.width + x) * 3;
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut(base + channel) {
                        *slot = 255;
                    }
                }
            }
        }
    }

    let subject_luma = if spec.skin.is_some() {
        let linear =
            0.2126 * subject_linear[0] + 0.7152 * subject_linear[1] + 0.0722 * subject_linear[2];
        crate::tone::exposure::linear_to_perceptual(linear)
    } else {
        0.0
    };

    Frame {
        rgb,
        width: spec.width,
        height: spec.height,
        subjects: ImageSubjects {
            faces,
            dominant: None,
            prominence,
            subject_focus_score: 0.9,
            people_count: spec.skin.map_or(0, |(_, count)| count as u32),
            weights_ver: 1,
        },
        scene: spec.scene,
        attrs: spec.attrs,
        as_shot: spec.as_shot,
        truth: Truth {
            illuminant_uv: spec.illuminant_uv,
            illuminant_k: aura_raw::colour::illuminant::cct_from_uv(spec.illuminant_uv),
            second_uv: spec.second_uv,
            skin_index: spec.skin_index,
            skin_reflectance: spec.skin.map(|(skin, _)| skin),
            subject_luma,
            has_neutral: spec.neutral,
            mixed: spec.second_uv.is_some(),
            coloured: spec.coloured,
        },
    }
}

// ---------------------------------------------------------------------------
// The named fixtures. Section 2.1's four hard cases, plus the two that are easy.
// ---------------------------------------------------------------------------

/// A daylight ceremony: the case everything should get right.
#[must_use]
pub fn daylight_ceremony(skin_index: usize) -> Frame {
    render(&Spec {
        illuminant_uv: cct_to_uv(5600.0),
        skin: Some((tone(skin_index), 2)),
        skin_index: Some(skin_index),
        neutral: true,
        scene: SceneId::Ceremony,
        exposure: 0.62,
        ..Spec::default()
    })
}

/// A tungsten reception: section 2.1's first hard case.
///
/// 2800 K, a tablecloth in frame, and the camera's own as-shot value left at daylight -
/// which is what a body set to auto does in a warm room and is precisely why the frame looks
/// orange before anything corrects it.
#[must_use]
pub fn tungsten_reception(skin_index: usize) -> Frame {
    render(&Spec {
        illuminant_uv: cct_to_uv(2800.0),
        skin: Some((tone(skin_index), 2)),
        skin_index: Some(skin_index),
        neutral: true,
        scene: SceneId::Speeches,
        attrs: AttrFlags::EMPTY
            .with(AttrFlags::TUNGSTEN, true)
            .with(AttrFlags::INDOOR, true),
        exposure: 0.55,
        as_shot: AsShot {
            temperature_k: 4600.0,
            tint: 0.0,
            flash_fired: false,
        },
        ..Spec::default()
    })
}

/// Daylight through a window against LED room light: section 2.1's second hard case.
#[must_use]
pub fn mixed_daylight_led(skin_index: usize) -> Frame {
    render(&Spec {
        illuminant_uv: cct_to_uv(3200.0),
        second_uv: Some(cct_to_uv(6500.0)),
        skin: Some((tone(skin_index), 1)),
        skin_index: Some(skin_index),
        scene: SceneId::Ceremony,
        attrs: AttrFlags::EMPTY
            .with(AttrFlags::MIXED_LIGHT, true)
            .with(AttrFlags::INDOOR, true),
        exposure: 0.60,
        as_shot: AsShot {
            temperature_k: 4200.0,
            tint: 0.0,
            flash_fired: false,
        },
        ..Spec::default()
    })
}

/// A backlit exit: section 2.1's third hard case.
///
/// The subject is a stop and a half under and the background is blown, which is what a
/// photographer shooting into a doorway gets and what section 6.1's backlit rule is for.
#[must_use]
pub fn backlit_exit(skin_index: usize) -> Frame {
    let mut frame = render(&Spec {
        illuminant_uv: cct_to_uv(5200.0),
        skin: Some((tone(skin_index), 1)),
        skin_index: Some(skin_index),
        background: [0.92, 0.92, 0.92],
        scene: SceneId::Exit,
        attrs: AttrFlags::EMPTY.with(AttrFlags::BACKLIT, true),
        exposure: 0.85,
        ..Spec::default()
    });
    // Darken the faces after the fact: a backlit subject is under-exposed relative to a
    // background that is correctly exposed, and rendering both at one exposure cannot produce
    // that.
    darken_faces(&mut frame, 0.30);
    frame
}

/// A purple dance floor: section 2.1's fourth hard case, and the one this phase must not
/// "fix".
#[must_use]
pub fn purple_dance_floor(skin_index: usize) -> Frame {
    // A saturated magenta wash: nowhere near the Planckian locus, which is what makes it a
    // creative choice rather than a cast.
    let purple = [0.30f32, 0.16, 0.44];
    render(&Spec {
        illuminant_uv: linear_srgb_to_uv(purple),
        skin: Some((tone(skin_index), 3)),
        skin_index: Some(skin_index),
        scene: SceneId::DanceFloor,
        attrs: AttrFlags::EMPTY
            .with(AttrFlags::STAGE, true)
            .with(AttrFlags::NIGHT, true)
            .with(AttrFlags::INDOOR, true),
        exposure: 0.35,
        coloured: true,
        as_shot: AsShot {
            temperature_k: 3800.0,
            tint: 0.0,
            flash_fired: false,
        },
        ..Spec::default()
    })
}

/// A detail shot with nobody in it: section 10.1's faceless gate.
#[must_use]
pub fn faceless_details() -> Frame {
    render(&Spec {
        illuminant_uv: cct_to_uv(4000.0),
        skin: None,
        neutral: true,
        scene: SceneId::Details,
        exposure: 0.50,
        specular: true,
        ..Spec::default()
    })
}

/// A red mandap: the frame where grey-world is catastrophically wrong.
///
/// Not one of section 2.1's four, and here because it is the case that decides whether the
/// multi-hypothesis design was worth building. The light is neutral daylight and two thirds
/// of the frame is deep red cloth, so grey-world reports a cyan illuminant and would take the
/// whole photograph green.
#[must_use]
pub fn red_mandap(skin_index: usize) -> Frame {
    render(&Spec {
        illuminant_uv: cct_to_uv(5200.0),
        background: MANDAP_REFLECTANCE,
        skin: Some((tone(skin_index), 2)),
        skin_index: Some(skin_index),
        scene: SceneId::Ritual,
        attrs: AttrFlags::EMPTY.with(AttrFlags::INDOOR, true),
        exposure: 0.70,
        ..Spec::default()
    })
}

/// The skin reflectance at one index, wrapping.
#[must_use]
pub fn tone(index: usize) -> [f32; 3] {
    SKIN_TONES
        .get(index % SKIN_TONES.len())
        .copied()
        .unwrap_or([0.30, 0.21, 0.17])
}

/// Scale every face region's pixels by a factor.
///
/// Used by [`backlit_exit`], which needs the subject under-exposed relative to a correctly
/// exposed background - two exposures in one frame, which `render` cannot express because it
/// takes one.
fn darken_faces(frame: &mut Frame, factor: f32) {
    let faces: Vec<CropRect> = frame.subjects.faces.iter().map(|face| face.bbox).collect();
    for area in faces {
        let x0 = ((area.x * frame.width as f32) as usize).min(frame.width);
        let y0 = ((area.y * frame.height as f32) as usize).min(frame.height);
        let x1 = (((area.x + area.w) * frame.width as f32) as usize).min(frame.width);
        let y1 = (((area.y + area.h) * frame.height as f32) as usize).min(frame.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let base = (y * frame.width + x) * 3;
                for channel in 0..3 {
                    let Some(slot) = frame.rgb.get_mut(base + channel) else {
                        continue;
                    };
                    let linear = crate::tone::stats::linearise(*slot) * factor;
                    *slot = encode(linear);
                }
            }
        }
    }
    let scaled = crate::tone::exposure::perceptual_to_linear(frame.truth.subject_luma) * factor;
    frame.truth.subject_luma = crate::tone::exposure::linear_to_perceptual(scaled);
}

/// Every named fixture, once per skin tone.
///
/// What the fairness gate iterates. Seven scenes times five reflectances is thirty-five
/// frames, which is what makes "the error must not depend on which reflectance was used" a
/// measurement rather than an assertion.
#[must_use]
pub fn every_case() -> Vec<Frame> {
    let mut out = Vec::new();
    for index in 0..SKIN_TONES.len() {
        out.push(daylight_ceremony(index));
        out.push(tungsten_reception(index));
        out.push(mixed_daylight_led(index));
        out.push(backlit_exit(index));
        out.push(purple_dance_floor(index));
        out.push(red_mandap(index));
    }
    out.push(faceless_details());
    out
}
