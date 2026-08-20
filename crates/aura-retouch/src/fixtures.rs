//! Synthetic faces whose marks are painted into the pixels.
//!
//! Every gate in PHASE-20 section 10.1 is measured against these. The generator paints skin with
//! pore-scale texture, then adds a red inflamed spot, a dark mole, a blotchy patch or a shadow
//! under the eyes at a known place and a known amplitude - and the measurement reads it back
//! through the real detector, the real operators and the real texture guard.
//!
//! **What that proves and what it does not.** It proves the detector geometry, the protect veto,
//! the band arithmetic, the texture floor, the re-solve and the store. It is not evidence about
//! a wedding photograph: the marks here are the ones this generator knows how to paint, the skin
//! is one reflectance with one texture, and nothing in this file has ever been near a camera.
//! That is condition C1 in `docs/progress/PHASE-20-EXIT.md`, and phases 06, 15, 16, 18 and 19 all
//! carry the same sentence about their own fixtures.
//!
//! Deterministic by construction: there is no random number generator anywhere in this module,
//! and the pore pattern is a fixed function of the coordinates. Invariant 4.

use std::collections::BTreeMap;

use aura_core::contract::composition::Box2;
use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::{MaskField, MaskKind};
use aura_core::contract::people::FaceRef;
use aura_core::contract::retouch::{
    ImageId, ProtectedFeature, ProtectedKind, ProtectedSource, RetouchPreset,
};
use aura_core::{FaceId, IdentityId, MaskId, PhotoId, SceneId};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use aura_render::retouch::RetouchContext;

use crate::blemish::FaceCrop;
use crate::ops::FrameContext;
use crate::texture_guard::Frame;

/// The linear luminance of the synthetic skin.
///
/// A middling reflectance under middling light. The point of the fixtures is the *mechanism*,
/// and every threshold this crate measures is relative to the skin it is measured on - so one
/// reflectance is enough to prove the arithmetic and is not enough to say anything about a
/// person. `docs/skin-fairness.md` is careful about the difference.
pub const SKIN_BASE: f32 = 0.34;

/// The amplitude of the pore-scale texture, as a fraction of the base.
///
/// Three and a half per cent, which is about what a real cheek carries on a 2048 px proxy.
pub const PORE_AMPLITUDE: f32 = 0.035;

/// A photograph id the fixtures use.
#[must_use]
pub fn photo() -> PhotoId {
    PhotoId::from_db("pht_00000000-0000-4000-8000-000000000020").unwrap_or_else(|_| PhotoId::new())
}

/// An identity the fixtures use.
#[must_use]
pub fn identity() -> IdentityId {
    IdentityId::from_db("idt_00000000-0000-4000-8000-000000000020")
        .unwrap_or_else(|_| IdentityId::new())
}

/// A mask id the fixtures use.
#[must_use]
pub fn mask_id() -> MaskId {
    MaskId::from_db("msk_00000000-0000-4000-8000-000000000020").unwrap_or_else(|_| MaskId::new())
}

/// Skin with pores and nothing else, as interleaved linear RGB.
#[must_use]
pub fn skin(width: usize, height: usize) -> Vec<f32> {
    let mut rgb = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let pore = pore_at(x, y);
            let base = SKIN_BASE * (1.0 + pore);
            rgb.extend_from_slice(&[base * 1.18, base, base * 0.82]);
        }
    }
    rgb
}

/// The pore pattern: a fixed, deterministic function of position.
///
/// Two interleaved frequencies rather than one, so the high band is not a single alternating
/// pattern that a box blur of exactly the right radius could cancel. A texture measurement that
/// only works on one spatial frequency is a measurement of the fixture.
#[must_use]
pub fn pore_at(x: usize, y: usize) -> f32 {
    let a = if (x + y).is_multiple_of(2) { 1.0 } else { -1.0 };
    let b = if (x / 2 + y / 3).is_multiple_of(2) {
        0.45
    } else {
        -0.45
    };
    (a + b) * PORE_AMPLITUDE * 0.5
}

/// Add a triple to three consecutive samples, without indexing.
fn add(pixel: &mut [f32], delta: [f32; 3]) {
    for (channel, value) in pixel.iter_mut().zip(delta.iter()) {
        *channel += *value;
    }
}

/// Multiply three consecutive samples by a triple, without indexing.
fn scale(pixel: &mut [f32], factor: [f32; 3]) {
    for (channel, value) in pixel.iter_mut().zip(factor.iter()) {
        *channel *= *value;
    }
}

/// Paint a red inflamed spot: brighter in red, slightly darker in green and blue.
pub fn paint_spot(rgb: &mut [f32], width: usize, cx: usize, cy: usize, radius: f32, amount: f32) {
    let height = rgb.len() / (width * 3).max(1);
    for y in 0..height {
        for x in 0..width {
            let d = ((x as f32 - cx as f32).powi(2) + (y as f32 - cy as f32).powi(2)).sqrt();
            if d > radius {
                continue;
            }
            let falloff = 1.0 - d / radius;
            let slot = (y * width + x) * 3;
            if let Some(pixel) = rgb.get_mut(slot..slot + 3) {
                add(
                    pixel,
                    [
                        amount * falloff,
                        -amount * 0.22 * falloff,
                        -amount * 0.22 * falloff,
                    ],
                );
            }
        }
    }
}

/// Paint a mole: darker in every channel and *less* saturated, which is what separates it from
/// a spot in one frame.
pub fn paint_mole(rgb: &mut [f32], width: usize, cx: usize, cy: usize, radius: f32) {
    let height = rgb.len() / (width * 3).max(1);
    for y in 0..height {
        for x in 0..width {
            let d = ((x as f32 - cx as f32).powi(2) + (y as f32 - cy as f32).powi(2)).sqrt();
            if d > radius {
                continue;
            }
            let falloff = 1.0 - d / radius;
            let slot = (y * width + x) * 3;
            if let Some(pixel) = rgb.get_mut(slot..slot + 3) {
                let level = 1.0 - 0.55 * falloff;
                scale(pixel, [level * 0.93, level, level * 1.04]);
            }
        }
    }
}

/// Paint a broad blotch: a low-contrast lift over a wide area, which is mid-band unevenness.
pub fn paint_blotch(rgb: &mut [f32], width: usize, cx: usize, cy: usize, radius: f32, amount: f32) {
    let height = rgb.len() / (width * 3).max(1);
    for y in 0..height {
        for x in 0..width {
            let d = ((x as f32 - cx as f32).powi(2) + (y as f32 - cy as f32).powi(2)).sqrt();
            if d > radius {
                continue;
            }
            let falloff = (1.0 - d / radius).powf(0.6);
            let slot = (y * width + x) * 3;
            if let Some(pixel) = rgb.get_mut(slot..slot + 3) {
                add(
                    pixel,
                    [
                        amount * falloff * 1.1,
                        amount * falloff * 0.8,
                        amount * falloff * 0.7,
                    ],
                );
            }
        }
    }
}

/// Darken a rectangle, for the shadow under an eye.
pub fn paint_shadow(
    rgb: &mut [f32],
    width: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    level: f32,
) {
    let height = rgb.len() / (width * 3).max(1);
    for y in y0..y1.min(height) {
        for x in x0..x1.min(width) {
            let slot = (y * width + x) * 3;
            if let Some(pixel) = rgb.get_mut(slot..slot + 3) {
                // Darker, and bluer: a dark circle is a shadow with a vein under it rather than
                // a neutral one, which is why the correction has a chroma half at all.
                scale(pixel, [level * 0.96, level, level * 1.12]);
            }
        }
    }
}

/// A crop of even skin, all of it masked.
#[must_use]
pub fn even_face() -> FaceCrop {
    let (w, h) = (96, 96);
    FaceCrop {
        rgb: skin(w, h),
        width: w,
        height: h,
        skin: vec![1.0; w * h],
        bounds: Box2 {
            x: 0.3,
            y: 0.3,
            w: 0.4,
            h: 0.4,
        },
    }
}

/// A crop with one red spot on it.
#[must_use]
pub fn face_with_blemish() -> FaceCrop {
    let mut crop = even_face();
    // Radius three on a ninety-six sample crop: six samples across, which is inside
    // `MAX_BLEMISH_FRACTION` of the face and well above the two-sample scale of a pore. A mark
    // painted much larger than this is a birthmark, and the detector says so.
    paint_spot(&mut crop.rgb, crop.width, 48, 48, 3.0, 0.095);
    crop
}

/// A crop with one dark mole on it.
#[must_use]
pub fn face_with_mole() -> FaceCrop {
    let mut crop = even_face();
    paint_mole(&mut crop.rgb, crop.width, 40, 56, 3.0);
    crop
}

/// A crop with broad blotchy patches.
#[must_use]
pub fn blotchy_face() -> FaceCrop {
    let mut crop = even_face();
    // Radius ten and eight, not twenty: the wide blur that defines the low band has a radius of
    // a twelfth of the crop, so a patch much larger than that lands in `low` and is *lighting*
    // rather than unevenness. Painting one there would test nothing - phase 19 owns lighting.
    paint_blotch(&mut crop.rgb, crop.width, 34, 40, 12.0, 0.060);
    paint_blotch(&mut crop.rgb, crop.width, 66, 58, 10.0, 0.050);
    paint_blotch(&mut crop.rgb, crop.width, 50, 76, 11.0, 0.045);
    crop
}

/// A face crop with landmarks, for the under-eye tests.
fn face_ref(bounds: Box2, left: [f32; 2], right: [f32; 2]) -> FaceRef {
    FaceRef {
        face_id: FaceId::from_db("fce_00000000-0000-4000-8000-000000000020")
            .unwrap_or_else(|_| FaceId::new()),
        identity_id: Some(identity()),
        bbox: CropRect {
            x: bounds.x,
            y: bounds.y,
            w: bounds.w,
            h: bounds.h,
        },
        eyes: [left, right],
        area_frac: bounds.w * bounds.h,
        centrality: 0.9,
        sharpness: 0.8,
        quality: 0.8,
        votes: true,
    }
}

/// A crop of even skin with eye landmarks over it.
#[must_use]
pub fn even_face_with_eyes() -> (FaceCrop, FaceRef) {
    let crop = even_face();
    let face = face_ref(crop.bounds, [0.42, 0.42], [0.58, 0.42]);
    (crop, face)
}

/// A crop with an ordinary shadow under each eye.
#[must_use]
pub fn face_with_dark_circles() -> (FaceCrop, FaceRef) {
    let (mut crop, face) = even_face_with_eyes();
    // The eyes sit at 0.42 and 0.58 of the frame; the crop covers 0.3..0.7, so in crop
    // coordinates they are at 29 and 67 of 96, on row 29.
    paint_shadow(&mut crop.rgb, crop.width, 21, 30, 38, 40, 0.80);
    paint_shadow(&mut crop.rgb, crop.width, 59, 30, 76, 40, 0.80);
    (crop, face)
}

/// A crop with a shadow far deeper than the cap allows.
#[must_use]
pub fn face_with_deep_circles() -> (FaceCrop, FaceRef) {
    let (mut crop, face) = even_face_with_eyes();
    paint_shadow(&mut crop.rgb, crop.width, 21, 30, 38, 40, 0.35);
    paint_shadow(&mut crop.rgb, crop.width, 59, 30, 76, 40, 0.35);
    (crop, face)
}

/// A crop with two red spots on it, for the recall gate.
#[must_use]
pub fn face_with_two_blemishes() -> FaceCrop {
    let mut crop = even_face();
    paint_spot(&mut crop.rgb, crop.width, 34, 38, 3.0, 0.095);
    paint_spot(&mut crop.rgb, crop.width, 62, 60, 3.0, 0.090);
    crop
}

/// A crop with a field of freckles: many small marks, none of them inflamed.
///
/// The case that most needs to survive. A freckled face carries a dozen candidates, every one of
/// them the right size for a blemish, and the only thing separating them from spots is that they
/// are not red - which is why the colour term carries most of the single-frame decision.
#[must_use]
pub fn face_with_freckles() -> FaceCrop {
    let mut crop = even_face();
    for (x, y) in [
        (30, 34),
        (38, 30),
        (46, 36),
        (54, 32),
        (62, 38),
        (34, 46),
        (44, 50),
        (58, 48),
        (66, 44),
    ] {
        paint_mole(&mut crop.rgb, crop.width, x, y, 2.0);
    }
    crop
}

/// A crop with a tattoo on it: a large, flat, dark region.
#[must_use]
pub fn face_with_tattoo() -> FaceCrop {
    let mut crop = even_face();
    paint_mole(&mut crop.rgb, crop.width, 48, 60, 16.0);
    crop
}

/// A whole frame with one blemish on it, and the context to retouch it through.
#[must_use]
pub fn frame_with_blemish() -> (Frame, RetouchContext, Box2) {
    let (w, h) = (160, 160);
    let mut rgb = skin(w, h);
    paint_spot(&mut rgb, w, 80, 80, 5.0, 0.075);
    let frame = Frame {
        rgb,
        width: w,
        height: h,
    };
    let context = RetouchContext {
        skin: vec![1.0; w * h],
        eyes: Vec::new(),
    };
    let area = Box2 {
        x: 74.0 / w as f32,
        y: 74.0 / h as f32,
        w: 12.0 / w as f32,
        h: 12.0 / h as f32,
    };
    (frame, context, area)
}

/// The same frame at any size, for the proxy-versus-export gate.
///
/// The mark is painted at the same *fraction* of the frame rather than at the same number of
/// pixels, which is what makes the comparison meaningful: section 10.1 asks whether the preview
/// a photographer approved matches what ships, and the two differ only in resolution.
#[must_use]
pub fn frame_with_blemish_at(side: usize) -> (Frame, RetouchContext, Box2) {
    let mut rgb = skin(side, side);
    let radius = side as f32 * (5.0 / 160.0);
    paint_spot(&mut rgb, side, side / 2, side / 2, radius, 0.075);
    let frame = Frame {
        rgb,
        width: side,
        height: side,
    };
    let context = RetouchContext {
        skin: vec![1.0; side * side],
        eyes: Vec::new(),
    };
    let area = Box2 {
        x: 74.0 / 160.0,
        y: 74.0 / 160.0,
        w: 12.0 / 160.0,
        h: 12.0 / 160.0,
    };
    (frame, context, area)
}

/// A whole frame with a shadow under each eye.
#[must_use]
pub fn frame_with_dark_circles() -> (Frame, RetouchContext, Box2) {
    let (w, h) = (160, 160);
    let mut rgb = skin(w, h);
    paint_shadow(&mut rgb, w, 54, 62, 76, 72, 0.62);
    paint_shadow(&mut rgb, w, 86, 62, 108, 72, 0.62);
    let frame = Frame {
        rgb,
        width: w,
        height: h,
    };
    let context = RetouchContext {
        skin: vec![1.0; w * h],
        eyes: vec![[[64.0, 60.0], [96.0, 60.0]]],
    };
    let area = Box2 {
        x: 54.0 / w as f32,
        y: 62.0 / h as f32,
        w: 54.0 / w as f32,
        h: 10.0 / h as f32,
    };
    (frame, context, area)
}

/// How much redder than neutral one point of a frame is, for the gates that compare two renders.
#[must_use]
pub fn redness_at(rgb: &[f32], width: usize, fx: f32, fy: f32) -> f32 {
    let height = rgb.len() / (width * 3).max(1);
    let x = ((fx * width as f32) as usize).min(width.saturating_sub(1));
    let y = ((fy * height as f32) as usize).min(height.saturating_sub(1));
    let slot = (y * width + x) * 3;
    let Some(pixel) = rgb.get(slot..slot + 3) else {
        return 0.0;
    };
    let mut channels = pixel.iter();
    let red = channels.next().copied().unwrap_or(0.0);
    let green = channels.next().copied().unwrap_or(0.0);
    (red - green).max(0.0)
}

/// A proxy buffer of a face with a spot on it, and everything a plan needs.
///
/// The face fills a quarter of the frame, which is a portrait - the case the phase exists for.
#[must_use]
pub fn planned_frame() -> (ImageId, PixelBuffer, FrameContext) {
    let (w, h) = (256, 256);
    let mut rgb = skin(w, h);
    // Two marks: one red spot that must go, and one that is deliberately ambiguous.
    paint_spot(&mut rgb, w, 120, 120, 4.0, 0.085);

    let buffer = PixelBuffer {
        width: w as u32,
        height: h as u32,
        data: PixelData::Srgb8(to_srgb8(&rgb)),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    };

    let bounds = Box2 {
        x: 0.25,
        y: 0.25,
        w: 0.50,
        h: 0.50,
    };
    let face = face_ref(bounds, [0.40, 0.40], [0.60, 0.40]);

    let mut strengths = BTreeMap::new();
    strengths.insert(identity(), 0.8);

    let context = FrameContext {
        scene: SceneId::CouplePortrait,
        faces: vec![face],
        skin: Some((mask_id(), skin_field())),
        identity_strength: strengths,
        protected: Vec::new(),
        evened_by_local: Vec::new(),
        preset: RetouchPreset::Natural,
        enabled: true,
    };

    (photo(), buffer, context)
}

/// The same frame, with the mark declared to be a mole this product will not remove.
#[must_use]
pub fn planned_frame_with_protected_mole() -> (ImageId, PixelBuffer, FrameContext) {
    let (image, buffer, mut context) = planned_frame();
    // The mark is at (120, 120) of 256, which is (0.469, 0.469) in frame coordinates. The eyes
    // are at 0.40 and 0.60 on row 0.40, so the face frame origin is (0.50, 0.40) and the unit is
    // 0.20: the mark lands at about (-0.16, 0.34).
    let area = Box2 {
        x: -0.22,
        y: 0.28,
        w: 0.12,
        h: 0.12,
    };
    context.protected.push(ProtectedFeature {
        identity: identity(),
        kind: ProtectedKind::Mole,
        area,
        confidence: 0.9,
        source: ProtectedSource::User,
        frames: 1,
        span_minutes: 0.0,
        first_seen: photo(),
    });
    (image, buffer, context)
}

/// A skin field covering the middle of the frame.
#[must_use]
pub fn skin_field() -> MaskField {
    let side = 64u16;
    let mut alpha = vec![0u8; usize::from(side) * usize::from(side)];
    for y in 0..usize::from(side) {
        for x in 0..usize::from(side) {
            let fx = x as f32 / f32::from(side);
            let fy = y as f32 / f32::from(side);
            let inside = fx > 0.24 && fx < 0.76 && fy > 0.24 && fy < 0.76;
            if let Some(slot) = alpha.get_mut(y * usize::from(side) + x) {
                *slot = if inside { 255 } else { 0 };
            }
        }
    }
    MaskField {
        kind: MaskKind::Skin,
        identity: Some(identity()),
        bounds: CropRect {
            x: 0.24,
            y: 0.24,
            w: 0.52,
            h: 0.52,
        },
        width: side,
        height: side,
        alpha,
        confidence: 0.9,
        edge_quality: 0.85,
        model_ver: 1,
    }
}

/// Encode a linear buffer as the sRGB bytes a proxy carries.
#[must_use]
pub fn to_srgb8(rgb: &[f32]) -> Vec<u8> {
    rgb.iter()
        .map(|value| {
            (aura_raw::colour::curve::srgb_encode(*value) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        })
        .collect()
}
