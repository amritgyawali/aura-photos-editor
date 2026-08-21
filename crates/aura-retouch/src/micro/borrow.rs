//! Whether a sibling frame may repair a destroyed region, and how well it aligned.
//!
//! PHASE-21 section 6.3, and it is the most consequential module in the phase:
//!
//! > Cross-frame borrowing is limited to small regions, requires high alignment confidence, and
//! > is always recorded in the recipe and the Explain panel so it is never a hidden composite.
//!
//! ## Five conditions, and the code checks each one
//!
//! A borrow happens only when all five hold. They are listed in `docs/retouch-ethics.md` section
//! 5 in a photographer's words; here they are as code:
//!
//! 1. **Small** - `region.w * region.h <= MAX_BORROW_AREA`, checked by
//!    [`super::glare::Sheet::may_borrow`] and again by `MicroOp::problem`.
//! 2. **Genuinely destroyed** - `clipped_fraction >= MIN_SPECULAR_FRACTION`, checked by the same
//!    method. This is the one that separates a repair from a composite: a blown lens carries no
//!    eye, and a soft sheen does.
//! 3. **Aligned** - [`align`] returns a score and [`MIN_ALIGNMENT`] is the floor. A borrow that
//!    does not align is refused rather than blended, because a misaligned eye socket is a face
//!    that is subtly wrong in a way nobody can name.
//! 4. **Same moment** - the caller only ever offers siblings from phase 08's own grouping.
//! 5. **Glare only** - there is no path into this module from any other operator, and no
//!    function here takes an operation kind.
//!
//! ## The alignment is landmark-seeded and then locally searched
//!
//! A similarity transform from the two eye landmarks puts the sibling's lens roughly where the
//! target's is. That is never exact - people move between frames of a burst - so [`align`] then
//! searches integer offsets within [`SEARCH_RADIUS`] and scores each with **zero-mean normalised
//! cross-correlation** over the *ring around* the destroyed region.
//!
//! Scoring on the ring rather than on the region itself is the whole trick, and it is the same
//! shape as phase 20's donor statistics being measured on the ring around a blemish. The region
//! is blown white in the target: correlating against it would score how white the sibling is, and
//! a completely white sibling patch would win. The ring is intact in both frames, so a high score
//! there means the two photographs agree about the tissue *around* the hole - which is what
//! "closely matching geometry" has to mean.
//!
//! Zero-mean and normalised because the two frames may differ in exposure by a third of a stop,
//! and a plain sum of squared differences would score that rather than the geometry.
//!
//! ## This module never composites
//!
//! It produces a [`Candidate`] carrying an aligned patch, and `aura_render::micro` blends it. The
//! split is the one phase 16 and phase 20 both used: the decision crate decides, the renderer
//! renders, and the number stored in the catalog describes what the renderer did.
//!
//! ## Everything here is linear
//!
//! Invariant 8.

use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{ImageId, MIN_ALIGNMENT};
use aura_core::contract::people::FaceRef;
use aura_render::micro::BorrowPatch;

use crate::texture_guard::Frame;

/// How far the local search looks, in pixels at proxy scale.
///
/// Six. The landmark seed is already close; this absorbs the sub-landmark drift between two
/// frames of a burst. A wider search is not more accurate, it is more likely to find a spurious
/// correlation somewhere else on a face.
pub const SEARCH_RADIUS: i32 = 6;

/// How wide the correlation ring is, as a multiple of the region's own shorter side.
///
/// Half. Wide enough to contain the brow, the socket and the frame of the glasses; narrow enough
/// that it is all still the same part of the same face.
pub const RING_WIDTH: f32 = 0.5;

/// The fewest ring samples an alignment score needs to mean anything.
pub const MIN_RING_SAMPLES: usize = 64;

/// One sibling frame, decoded and ready to be searched.
#[derive(Debug, Clone)]
pub struct SiblingFrame {
    /// Which photograph it is. **Recorded on the operation; this is the disclosure.**
    pub image: ImageId,
    /// Its pixels, linear RGB at the same rung as the target.
    pub frame: Frame,
    /// The same face on that frame, for the landmark seed.
    pub face: FaceRef,
}

/// One accepted borrow.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Where the borrowed pixels come from. Never `None` on an accepted candidate.
    pub source: ImageId,
    /// How well the ring around the region correlated, `0..1`.
    pub alignment: f32,
    /// The aligned donor pixels, ready for the renderer to blend.
    pub patch: BorrowPatch,
}

/// Why a borrow was not made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The region still carries information, so reducing it is the right repair.
    ///
    /// Checked by the caller through [`super::glare::Sheet::may_borrow`] before this module is
    /// reached; the variant exists so the reason a caller records has a name.
    StillInformative,
    /// No sibling was offered at all.
    NoSibling,
    /// A sibling was offered and none of them aligned well enough.
    NoAlignment,
    /// The sibling has the same sheet in the same place, so it repairs nothing.
    SiblingAlsoGlared,
}

/// Choose the best sibling to repair one region from, or say why none was chosen.
///
/// `siblings` are frames from the same moment carrying the same identity. They are searched in
/// the order given and the best alignment wins; ties break toward the earlier entry, so a caller
/// that orders siblings deterministically gets a deterministic borrow. Invariant 4.
///
/// # Errors
///
/// None. A refusal is a value rather than an error: not borrowing is the ordinary outcome and the
/// caller turns it into a reason code.
pub fn choose(
    target: &Frame,
    region: Box2,
    target_face: &FaceRef,
    siblings: &[SiblingFrame],
) -> Result<Candidate, Refusal> {
    if siblings.is_empty() {
        return Err(Refusal::NoSibling);
    }
    let window = to_pixels(region, target.width, target.height);
    if window.2 == 0 || window.3 == 0 {
        return Err(Refusal::NoSibling);
    }

    let mut best: Option<Candidate> = None;
    let mut saw_glared = false;

    for sibling in siblings {
        // The sibling must actually be better. A second frame with the same reflection in the
        // same place repairs nothing, and blending it in would be a composite for no gain.
        if aura_render::micro::clipped_fraction(
            &sibling.frame.rgb,
            sibling.frame.width,
            sibling.frame.height,
            mapped_region(region, target_face, &sibling.face),
        ) >= aura_core::contract::micro::MIN_SPECULAR_FRACTION
        {
            saw_glared = true;
            continue;
        }

        let Some(candidate) = align(target, window, target_face, sibling) else {
            continue;
        };
        if candidate.alignment < MIN_ALIGNMENT {
            continue;
        }
        let better = best
            .as_ref()
            .is_none_or(|current| candidate.alignment > current.alignment);
        if better {
            best = Some(candidate);
        }
    }

    match best {
        Some(candidate) => Ok(candidate),
        None if saw_glared => Err(Refusal::SiblingAlsoGlared),
        None => Err(Refusal::NoAlignment),
    }
}

/// Align one sibling to one region and score how well its surroundings agree.
///
/// `window` is the destroyed region in target pixels, `(x, y, w, h)`. Returns `None` when there
/// is not enough ring to score.
#[must_use]
pub fn align(
    target: &Frame,
    window: (usize, usize, usize, usize),
    target_face: &FaceRef,
    sibling: &SiblingFrame,
) -> Option<Candidate> {
    let (wx, wy, ww, wh) = window;
    if ww == 0 || wh == 0 {
        return None;
    }

    // --- the landmark seed --------------------------------------------------------------------
    //
    // A similarity transform taking the sibling's two eye centres onto the target's. Scale and
    // translation only: a rotation between two frames of a burst is a fraction of a degree, and
    // fitting one from two points on a moving head adds a free parameter that the local search
    // then has to undo.
    let seed = seed_transform(target, target_face, sibling)?;

    let ring = ring_samples(target, window);
    if ring.len() < MIN_RING_SAMPLES {
        return None;
    }

    let mut best_score = -1.0f32;
    let mut best_offset = (0i32, 0i32);
    for dy in -SEARCH_RADIUS..=SEARCH_RADIUS {
        for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
            let score = correlate(&ring, sibling, seed, (dx, dy));
            // Strictly greater: the first offset to reach a score keeps it, and the scan order is
            // fixed, so two machines break a tie the same way.
            if score > best_score {
                best_score = score;
                best_offset = (dx, dy);
            }
        }
    }
    if best_score <= 0.0 {
        return None;
    }

    // --- lift the aligned patch ----------------------------------------------------------------
    let mut rgb = vec![0.0f32; ww * wh * 3];
    for row in 0..wh {
        for col in 0..ww {
            let (sx, sy) = seed.map(
                (wx + col) as f32 + best_offset.0 as f32,
                (wy + row) as f32 + best_offset.1 as f32,
            );
            let sample = bilinear(&sibling.frame, sx, sy);
            for channel in 0..3 {
                if let (Some(slot), Some(value)) = (
                    rgb.get_mut((row * ww + col) * 3 + channel),
                    sample.get(channel),
                ) {
                    *slot = *value;
                }
            }
        }
    }

    Some(Candidate {
        source: sibling.image,
        alignment: best_score.clamp(0.0, 1.0),
        patch: BorrowPatch {
            x: wx,
            y: wy,
            w: ww,
            h: wh,
            rgb,
        },
    })
}

/// A scale-and-translate map from target pixels to sibling pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed {
    scale: f32,
    tx: f32,
    ty: f32,
}

impl Seed {
    /// Map one target pixel coordinate into the sibling.
    #[must_use]
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale + self.tx, y * self.scale + self.ty)
    }
}

fn seed_transform(target: &Frame, target_face: &FaceRef, sibling: &SiblingFrame) -> Option<Seed> {
    if !target_face.has_eyes() || !sibling.face.has_eyes() {
        return None;
    }
    let (tw, th) = (target.width as f32, target.height as f32);
    let (sw, sh) = (sibling.frame.width as f32, sibling.frame.height as f32);

    let t_left = [target_face.eyes[0][0] * tw, target_face.eyes[0][1] * th];
    let t_right = [target_face.eyes[1][0] * tw, target_face.eyes[1][1] * th];
    let s_left = [sibling.face.eyes[0][0] * sw, sibling.face.eyes[0][1] * sh];
    let s_right = [sibling.face.eyes[1][0] * sw, sibling.face.eyes[1][1] * sh];

    let t_sep = ((t_right[0] - t_left[0]).powi(2) + (t_right[1] - t_left[1]).powi(2)).sqrt();
    let s_sep = ((s_right[0] - s_left[0]).powi(2) + (s_right[1] - s_left[1]).powi(2)).sqrt();
    if t_sep <= 1e-3 || s_sep <= 1e-3 {
        return None;
    }
    let scale = s_sep / t_sep;

    let t_mid = [
        f32::midpoint(t_left[0], t_right[0]),
        f32::midpoint(t_left[1], t_right[1]),
    ];
    let s_mid = [
        f32::midpoint(s_left[0], s_right[0]),
        f32::midpoint(s_left[1], s_right[1]),
    ];

    Some(Seed {
        scale,
        tx: s_mid[0] - t_mid[0] * scale,
        ty: s_mid[1] - t_mid[1] * scale,
    })
}

/// The intact samples around a destroyed region, as `(x, y, luminance)` in target pixels.
fn ring_samples(target: &Frame, window: (usize, usize, usize, usize)) -> Vec<(f32, f32, f32)> {
    let (wx, wy, ww, wh) = window;
    let band = ((ww.min(wh) as f32 * RING_WIDTH).round() as usize).max(2);
    let x0 = wx.saturating_sub(band);
    let y0 = wy.saturating_sub(band);
    let x1 = (wx + ww + band).min(target.width);
    let y1 = (wy + wh + band).min(target.height);

    let mut out = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            if x >= wx && x < wx + ww && y >= wy && y < wy + wh {
                continue;
            }
            let slot = (y * target.width + x) * 3;
            let Some(rgb) = target.rgb.get(slot..slot + 3) else {
                continue;
            };
            out.push((
                x as f32,
                y as f32,
                0.2126 * rgb.first().copied().unwrap_or(0.0)
                    + 0.7152 * rgb.get(1).copied().unwrap_or(0.0)
                    + 0.0722 * rgb.get(2).copied().unwrap_or(0.0),
            ));
        }
    }
    out
}

/// Zero-mean normalised cross-correlation of a ring against a sibling at one offset.
///
/// Returns `-1..1`; the caller treats anything at or below zero as no alignment at all.
fn correlate(
    ring: &[(f32, f32, f32)],
    sibling: &SiblingFrame,
    seed: Seed,
    offset: (i32, i32),
) -> f32 {
    let mut a: Vec<f32> = Vec::with_capacity(ring.len());
    let mut b: Vec<f32> = Vec::with_capacity(ring.len());
    for (x, y, value) in ring {
        let (sx, sy) = seed.map(*x + offset.0 as f32, *y + offset.1 as f32);
        if sx < 0.0
            || sy < 0.0
            || sx >= sibling.frame.width as f32
            || sy >= sibling.frame.height as f32
        {
            continue;
        }
        let sample = bilinear(&sibling.frame, sx, sy);
        a.push(*value);
        b.push(0.2126 * sample[0] + 0.7152 * sample[1] + 0.0722 * sample[2]);
    }
    if a.len() < MIN_RING_SAMPLES {
        return -1.0;
    }

    let n = a.len() as f64;
    let mean_a = a.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let mean_b = b.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let mut cov = 0.0f64;
    let mut var_a = 0.0f64;
    let mut var_b = 0.0f64;
    for (va, vb) in a.iter().zip(b.iter()) {
        let da = f64::from(*va) - mean_a;
        let db = f64::from(*vb) - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    if var_a <= f64::EPSILON || var_b <= f64::EPSILON {
        // One of the two rings is flat. A flat ring correlates with everything, so scoring it
        // high would let a featureless patch of cheek win - refused instead.
        return -1.0;
    }
    (cov / (var_a.sqrt() * var_b.sqrt())) as f32
}

/// Where a target region lands on a sibling, in normalised coordinates.
///
/// Used only to ask whether the sibling has the same sheet. Approximate by construction - it is a
/// landmark-relative offset rather than a solved alignment - which is the right precision for a
/// question whose answer is "is this one also blown".
fn mapped_region(region: Box2, target_face: &FaceRef, sibling_face: &FaceRef) -> Box2 {
    let t_mid = [
        f32::midpoint(target_face.eyes[0][0], target_face.eyes[1][0]),
        f32::midpoint(target_face.eyes[0][1], target_face.eyes[1][1]),
    ];
    let s_mid = [
        f32::midpoint(sibling_face.eyes[0][0], sibling_face.eyes[1][0]),
        f32::midpoint(sibling_face.eyes[0][1], sibling_face.eyes[1][1]),
    ];
    let t_sep = ((target_face.eyes[1][0] - target_face.eyes[0][0]).powi(2)
        + (target_face.eyes[1][1] - target_face.eyes[0][1]).powi(2))
    .sqrt()
    .max(1e-4);
    let s_sep = ((sibling_face.eyes[1][0] - sibling_face.eyes[0][0]).powi(2)
        + (sibling_face.eyes[1][1] - sibling_face.eyes[0][1]).powi(2))
    .sqrt()
    .max(1e-4);
    let scale = s_sep / t_sep;
    Box2 {
        x: s_mid[0] + (region.x - t_mid[0]) * scale,
        y: s_mid[1] + (region.y - t_mid[1]) * scale,
        w: region.w * scale,
        h: region.h * scale,
    }
    .clamped()
}

fn bilinear(frame: &Frame, x: f32, y: f32) -> [f32; 3] {
    let cx = x.clamp(0.0, (frame.width.saturating_sub(1)) as f32);
    let cy = y.clamp(0.0, (frame.height.saturating_sub(1)) as f32);
    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(frame.width.saturating_sub(1));
    let y1 = (y0 + 1).min(frame.height.saturating_sub(1));
    let tx = cx - x0 as f32;
    let ty = cy - y0 as f32;

    let mut out = [0.0f32; 3];
    for channel in 0..3 {
        let at = |px: usize, py: usize| -> f32 {
            frame
                .rgb
                .get((py * frame.width + px) * 3 + channel)
                .copied()
                .unwrap_or(0.0)
        };
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
        if let Some(slot) = out.get_mut(channel) {
            *slot = top + (bottom - top) * ty;
        }
    }
    out
}

fn to_pixels(region: Box2, width: usize, height: usize) -> (usize, usize, usize, usize) {
    let clamped = region.clamped();
    let x = (clamped.x * width as f32).floor().max(0.0) as usize;
    let y = (clamped.y * height as f32).floor().max(0.0) as usize;
    let w = ((clamped.w * width as f32).ceil() as usize).min(width.saturating_sub(x));
    let h = ((clamped.h * height as f32).ceil() as usize).min(height.saturating_sub(y));
    (x, y, w, h)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_core::contract::integrity::CropRect;
    use aura_core::{FaceId, PhotoId};

    fn face(shift: f32) -> FaceRef {
        FaceRef {
            face_id: FaceId::new(),
            identity_id: None,
            bbox: CropRect {
                x: 0.25 + shift,
                y: 0.20,
                w: 0.50,
                h: 0.60,
            },
            eyes: [[0.40 + shift, 0.40], [0.60 + shift, 0.40]],
            area_frac: 0.30,
            centrality: 0.9,
            sharpness: 0.8,
            quality: 0.8,
            votes: true,
        }
    }

    fn image(tag: u8) -> ImageId {
        let text = format!("pht_00000000-0000-4000-8000-0000000000{tag:02}");
        PhotoId::from_db(&text).expect("a photo id")
    }

    /// A frame carrying a deterministic, structured pattern - so a correlation over it means
    /// something - optionally with a blown patch across the lens region.
    fn textured(width: usize, height: usize, blown: bool, shift: usize) -> Frame {
        let mut rgb = vec![0.0f32; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let sx = (x + shift) as f32;
                let value = 0.25
                    + 0.18 * ((sx * 0.31).sin() * (y as f32 * 0.27).cos())
                    + 0.06 * ((sx * 0.11 + y as f32 * 0.09).sin());
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut((y * width + x) * 3 + channel) {
                        *slot = value.clamp(0.02, 0.85);
                    }
                }
            }
        }
        if blown {
            for y in 38..44 {
                for x in 42..54 {
                    for channel in 0..3 {
                        if let Some(slot) = rgb.get_mut((y * width + x) * 3 + channel) {
                            *slot = 1.30;
                        }
                    }
                }
            }
        }
        Frame { rgb, width, height }
    }

    #[test]
    fn a_matching_sibling_aligns_and_is_chosen() {
        let target = textured(100, 100, true, 0);
        let sibling = SiblingFrame {
            image: image(2),
            frame: textured(100, 100, false, 0),
            face: face(0.0),
        };
        let region = Box2 {
            x: 0.42,
            y: 0.38,
            w: 0.12,
            h: 0.06,
        };
        let candidate = choose(&target, region, &face(0.0), &[sibling]).expect("a borrow");
        assert!(
            candidate.alignment >= MIN_ALIGNMENT,
            "alignment {} is below the floor",
            candidate.alignment
        );
        assert_eq!(candidate.source, image(2));
        assert_eq!(
            candidate.patch.rgb.len(),
            candidate.patch.w * candidate.patch.h * 3
        );
    }

    #[test]
    fn a_sibling_with_the_same_sheet_repairs_nothing_and_is_refused() {
        let target = textured(100, 100, true, 0);
        let sibling = SiblingFrame {
            image: image(3),
            frame: textured(100, 100, true, 0),
            face: face(0.0),
        };
        let region = Box2 {
            x: 0.42,
            y: 0.38,
            w: 0.12,
            h: 0.06,
        };
        assert_eq!(
            choose(&target, region, &face(0.0), &[sibling]),
            Err(Refusal::SiblingAlsoGlared)
        );
    }

    #[test]
    fn no_sibling_is_a_refusal_rather_than_a_guess() {
        let target = textured(100, 100, true, 0);
        let region = Box2 {
            x: 0.42,
            y: 0.38,
            w: 0.12,
            h: 0.06,
        };
        assert_eq!(
            choose(&target, region, &face(0.0), &[]),
            Err(Refusal::NoSibling)
        );
    }

    #[test]
    fn an_unrelated_sibling_does_not_reach_the_alignment_floor() {
        let target = textured(100, 100, true, 0);
        // A completely different pattern behind the same landmarks.
        let mut frame = textured(100, 100, false, 0);
        for (index, value) in frame.rgb.iter_mut().enumerate() {
            *value = 0.10 + 0.55 * (((index * 7919) % 97) as f32 / 97.0);
        }
        let sibling = SiblingFrame {
            image: image(4),
            frame,
            face: face(0.0),
        };
        let region = Box2 {
            x: 0.42,
            y: 0.38,
            w: 0.12,
            h: 0.06,
        };
        assert_eq!(
            choose(&target, region, &face(0.0), &[sibling]),
            Err(Refusal::NoAlignment)
        );
    }

    #[test]
    fn choosing_is_deterministic() {
        let target = textured(100, 100, true, 0);
        let region = Box2 {
            x: 0.42,
            y: 0.38,
            w: 0.12,
            h: 0.06,
        };
        let siblings = || {
            vec![SiblingFrame {
                image: image(2),
                frame: textured(100, 100, false, 0),
                face: face(0.0),
            }]
        };
        let first = choose(&target, region, &face(0.0), &siblings()).expect("a borrow");
        let second = choose(&target, region, &face(0.0), &siblings()).expect("a borrow");
        assert_eq!(first.alignment, second.alignment);
        assert_eq!(first.patch, second.patch);
    }
}
