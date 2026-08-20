//! What a mask is good enough for, and the contract that carries it into phases 19 to 24.
//!
//! Section 6.4:
//!
//! > Mask confidence below threshold disables aggressive operations (skin smoothing,
//! > generative cleanup) and records the reason, so a bad mask can never cause a visible
//! > artefact.
//!
//! # The gate is a *ceiling*, not a switch
//!
//! [`allowance`] returns a number in `0.0 ..= 1.0` that a later phase multiplies its own
//! strength by, and only the two named aggressive operations are refused outright. The
//! alternative - a threshold below which nothing applies - turns a graded response into a
//! cliff, and a cliff is what silently leaves half a gallery unedited: a veil at
//! `edge_quality = 0.55` carries a third of a stop of local exposure perfectly well and cannot
//! carry skin smoothing. ADR-0037 decision 6.
//!
//! # Why the fusion is geometric
//!
//! Phase 12 fused four sub-scores as a weighted geometric mean so that no signal could rescue
//! another, and this is the same argument with two: a face mask that is *certainly* a face with
//! an undetermined boundary is not a mask that may carry skin smoothing, and an arithmetic mean
//! would say it was. The multiplication is in [`crate::contract::mask::Mask::allowance`], on the
//! frozen shape, so a consumer holding only a `Mask` gets the same answer as one holding a
//! `MaskPlane`.

use aura_core::contract::error::AuraError;

use crate::contract::mask::{EdgeQuality, Mask, MaskKind, MaskReason, AGGRESSIVE_FLOOR};
use crate::mask::errors;
use crate::mask::MaskPlane;

/// Operations later phases ask permission for.
///
/// A closed set, because "is this operation aggressive" is a question this phase has to answer
/// for operators that do not exist yet - phases 20, 21, 22 and 24 each add some - and a
/// boolean flag on the caller's side would let the caller decide. The two named in section 6.4
/// are the two that are refused; the rest are scaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    /// A local exposure, contrast or colour move. Phase 19.
    LocalTone,
    /// Skin smoothing. Phase 20. Named in section 6.4 as aggressive.
    SkinSmooth,
    /// Blemish and stray-hair removal. Phase 21.
    MicroRetouch,
    /// Denoise, sharpen and face detail recovery. Phase 22.
    Restoration,
    /// Generative fill. Phase 24. Named in section 6.4 as aggressive.
    GenerativeCleanup,
}

impl Operation {
    /// Stable text for telemetry and the ledger.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalTone => "local_tone",
            Self::SkinSmooth => "skin_smooth",
            Self::MicroRetouch => "micro_retouch",
            Self::Restoration => "restoration",
            Self::GenerativeCleanup => "generative_cleanup",
        }
    }

    /// True when section 6.4 calls this aggressive.
    #[must_use]
    pub const fn is_aggressive(self) -> bool {
        matches!(self, Self::SkinSmooth | Self::GenerativeCleanup)
    }
}

/// What a later phase is allowed to do through this mask.
///
/// No `PartialEq`: `note` is an `AuraError`, which carries a context map and deliberately does
/// not compare. A test that wanted to compare two allowances compares the three fields that
/// mean something.
#[derive(Debug, Clone)]
pub struct Allowance {
    /// The strength ceiling, `0.0 ..= 1.0`. Multiply, do not compare.
    pub ceiling: f32,
    /// `false` when the operation is refused outright.
    pub permitted: bool,
    /// Why, when the ceiling is below one. Empty when nothing is limiting.
    pub reasons: Vec<MaskReason>,
    /// The warning to surface, when there is one.
    pub note: Option<AuraError>,
}

/// Decide what one operation may do through one mask.
///
/// The single entry point. Phases 19 to 24 call this and multiply; there is no second way to
/// ask, which is what keeps "may this mask carry skin smoothing" from having two answers.
#[must_use]
pub fn allowance(mask: &Mask, operation: Operation) -> Allowance {
    let ceiling = mask.allowance();
    let aggressive_ok = ceiling >= AGGRESSIVE_FLOOR;
    let permitted = !operation.is_aggressive() || aggressive_ok;

    let mut reasons = Vec::new();
    if ceiling < 1.0 {
        // The limiting half is named rather than implied, because the two are fixed by
        // different things: a photographer can re-brush a boundary and cannot re-brush a class.
        if mask.edge_quality < mask.confidence {
            reasons.push(match mask.edge {
                EdgeQuality::Binary => MaskReason::Derived,
                EdgeQuality::Unknown => MaskReason::LowContrastBoundary,
                _ => MaskReason::Matted,
            });
        } else {
            reasons.push(MaskReason::HeadUntrained);
        }
    }
    for reason in &mask.reasons {
        if matches!(
            reason,
            MaskReason::LowContrastBoundary
                | MaskReason::FrameNotSharp
                | MaskReason::TooSmallToMatte
                | MaskReason::NoFaces
        ) && !reasons.contains(reason)
        {
            reasons.push(*reason);
        }
    }

    let note = if permitted && ceiling >= 0.999 {
        None
    } else {
        Some(errors::quality_limited(mask.kind.as_str(), ceiling))
    };

    Allowance {
        ceiling: if permitted { ceiling } else { 0.0 },
        permitted,
        reasons,
        note,
    }
}

/// Score a freshly measured plane and settle its edge word.
///
/// Called once per plane on the way out of the pipeline, so the relationship between the
/// number and the word is decided in one place. A plane stored as a run length has a hard
/// boundary by construction and is [`EdgeQuality::Binary`] whatever its matting said, because
/// the softness would not survive the encoding - and a caller told otherwise would feather
/// against an edge that is not there.
pub fn settle(plane: &mut MaskPlane) {
    if plane.plane.is_empty() {
        plane.confidence = 0.0;
        plane.edge_quality = 0.0;
        plane.edge = EdgeQuality::Unknown;
        return;
    }
    if matches!(plane.kind.stored_as(), crate::contract::mask::Storage::Rle) {
        plane.edge = EdgeQuality::Binary;
        // A hard boundary is not a bad one and it is not a good one. Two thirds is the number
        // that lets a run-length class carry a local tone move at full strength and stops it
        // carrying skin smoothing on its own.
        plane.edge_quality = plane.edge_quality.max(0.66);
    }
    if plane.reasons.is_empty() {
        // Invariant 2. A region with no reason is a bug, and the fallback names the one thing
        // that is always true of a mask in this build.
        plane.reasons.push(MaskReason::HeadUntrained);
    }
}

/// The number the outline reports as mean edge quality.
///
/// Weighted by each region's area, not a plain mean over kinds. Sixteen tiny regions - two
/// irises, two eyebrows, some teeth - and one enormous background would otherwise let the
/// small ones dominate a figure a photographer reads as "how good are the masks on this
/// wedding".
#[must_use]
pub fn mean_edge_quality(masks: &[Mask]) -> f32 {
    let mut weight = 0.0_f64;
    let mut total = 0.0_f64;
    for mask in masks {
        let (w, h) = mask.payload.dimensions();
        let area = f64::from(w) * f64::from(h);
        if area <= 0.0 {
            continue;
        }
        weight += area;
        total += area * f64::from(mask.edge_quality);
    }
    if weight <= 0.0 {
        return 0.0;
    }
    (total / weight) as f32
}

/// The same figure for confidence.
#[must_use]
pub fn mean_confidence(masks: &[Mask]) -> f32 {
    let mut weight = 0.0_f64;
    let mut total = 0.0_f64;
    for mask in masks {
        let (w, h) = mask.payload.dimensions();
        let area = f64::from(w) * f64::from(h);
        if area <= 0.0 {
            continue;
        }
        weight += area;
        total += area * f64::from(mask.confidence);
    }
    if weight <= 0.0 {
        return 0.0;
    }
    (total / weight) as f32
}

/// How many masks are below the aggressive floor.
#[must_use]
pub fn low_quality_count(masks: &[Mask]) -> u64 {
    masks
        .iter()
        .filter(|m| !m.plane_is_empty() && m.allowance() < AGGRESSIVE_FLOOR)
        .count() as u64
}

/// A small extension so the count above does not have to reach into the payload.
trait PlaneIsEmpty {
    fn plane_is_empty(&self) -> bool;
}

impl PlaneIsEmpty for Mask {
    fn plane_is_empty(&self) -> bool {
        self.payload.is_empty()
    }
}

/// True when a kind is one a photographer would expect on every frame with a person in it.
///
/// Used by the outline to decide whether an absent mask is a gap or a photograph that simply
/// has no teeth in it. A frame with no sky is not a frame with a missing sky mask.
#[must_use]
pub const fn is_expected(kind: MaskKind) -> bool {
    matches!(
        kind,
        MaskKind::Skin
            | MaskKind::Face
            | MaskKind::Hair
            | MaskKind::Subject
            | MaskKind::Background
            | MaskKind::SkinSafe
    )
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
    use crate::contract::mask::{MaskPayload, MaskReason};
    use aura_core::contract::ids::MaskId;
    use aura_core::PhotoId;

    fn mask(confidence: f32, edge_quality: f32, user_edited: bool) -> Mask {
        Mask {
            id: MaskId::new(),
            image_id: PhotoId::new(),
            kind: MaskKind::Skin,
            identity: None,
            payload: MaskPayload::Alpha8 {
                w: 4,
                h: 4,
                alpha: vec![255; 16],
            },
            feather: 0.0,
            confidence,
            edge_quality,
            edge: EdgeQuality::Matted,
            reasons: vec![MaskReason::SeededByFace],
            user_edited,
            model_ver: 1,
        }
    }

    #[test]
    fn a_confident_class_with_a_bad_edge_cannot_smooth_skin() {
        // The module note's own example. An arithmetic mean of 0.98 and 0.10 is 0.54, which is
        // above the floor; the geometric mean is 0.31, which is not.
        let m = mask(0.98, 0.10, false);
        let out = allowance(&m, Operation::SkinSmooth);
        assert!(!out.permitted);
        assert_eq!(out.ceiling, 0.0);
        assert!(out.note.is_some());
    }

    #[test]
    fn the_same_mask_still_carries_a_local_tone_move() {
        let m = mask(0.98, 0.10, false);
        let out = allowance(&m, Operation::LocalTone);
        assert!(out.permitted);
        assert!(out.ceiling > 0.25 && out.ceiling < 0.4);
    }

    #[test]
    fn a_hand_edited_mask_is_never_gated() {
        let m = mask(0.10, 0.10, true);
        let out = allowance(&m, Operation::GenerativeCleanup);
        assert!(out.permitted);
        assert_eq!(out.ceiling, 1.0);
    }

    #[test]
    fn the_mean_is_weighted_by_area() {
        let mut small = mask(1.0, 1.0, false);
        small.payload = MaskPayload::Alpha8 {
            w: 1,
            h: 1,
            alpha: vec![255],
        };
        let mut big = mask(0.0, 0.0, false);
        big.payload = MaskPayload::Alpha8 {
            w: 100,
            h: 100,
            alpha: vec![255; 10_000],
        };
        assert!(mean_edge_quality(&[small, big]) < 0.01);
    }
}
