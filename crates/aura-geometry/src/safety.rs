//! The hard filter. PHASE-23 section 6.3.
//!
//! Four rules, and none of them is a score term:
//!
//! 1. Every detected face fully inside, with [`SAFETY_MARGIN`] to spare.
//! 2. Every primary identity's hands inside. A guest's are not - section 6.3 says "primary
//!    identities' hands and joined hands", and a hand at the edge of a dance floor frame is
//!    not a reason to refuse every crop in the wedding.
//! 3. The long edge at or above the scene's resolution floor, measured against the frame **as
//!    shot**.
//! 4. Phase 11's crop hint region kept.
//!
//! ## Why it runs first
//!
//! Phase 12's rule - a guarantee outranks a preference - in the phase where the preference is
//! a tuned score and the guarantee is a bride's hands. A filter applied *after* the objective
//! invites exactly one repair: nudge the winning rectangle until the face is back inside. A
//! nudged crop is a different aspect ratio, a different resolution, or a fresh violation at
//! the opposite edge - and the nudge is the change nobody writes a test for.
//!
//! ## Why the counts are on the report
//!
//! A crop over a frame with no detected faces satisfies rule 1 trivially. Reporting that as a
//! passed safety check would make a build with no face detector look like a build whose crops
//! are provably safe, which is the exact reading phase 09's rule about denominators exists to
//! prevent. [`CropSafetyReport::faces_checked`] and `hands_checked` are counts for that
//! reason, and on this build the second is zero on every photograph in the product.

use aura_core::contract::geometry::{
    CropSafetyReport, CropVariant, GeometryCode, ProtectedKind, ProtectedRegion, RESOLUTION_FLOOR,
    SAFETY_MARGIN,
};
use aura_core::contract::integrity::CropRect;

/// What the filter checks against.
#[derive(Debug, Clone, Copy)]
pub struct SafetyInput<'a> {
    /// Everything that must stay whole, in the **corrected** frame's coordinates.
    pub regions: &'a [ProtectedRegion],
    /// Width over height of the frame.
    pub aspect: f32,
    /// The scene's resolution floor, already checked against [`RESOLUTION_FLOOR`].
    pub resolution_floor: f32,
}

impl SafetyInput<'static> {
    /// A frame with nothing to protect and the contract's own floor.
    ///
    /// For tests and for the rotation gate on a frame nobody has analysed. **Not** a default
    /// the pass may reach for: a frame with no regions because nothing was detected and a
    /// frame with no regions because nothing is there are different, and the counts on the
    /// report are what tell them apart.
    #[must_use]
    pub const fn permissive() -> Self {
        Self {
            regions: &[],
            aspect: 1.5,
            resolution_floor: RESOLUTION_FLOOR,
        }
    }
}

/// Why one rectangle was refused, or `None` when it passed.
#[must_use]
pub fn refusal(rect: CropRect, input: &SafetyInput<'_>) -> Option<GeometryCode> {
    if rect.is_empty() {
        return Some(GeometryCode::CropTooSmall);
    }
    // **Faces, then hands, then content, then resolution.** The order is the order of
    // seriousness rather than the order of cheapness, because the code that comes back is what
    // a photographer is shown and what section 11's histogram counts. A rectangle that cuts a
    // face *and* falls below the floor is a rectangle that cut a face; reporting it as "too
    // small" would make the zero-face-cut audit under-count exactly the candidates it exists
    // to find.
    for kind in [
        ProtectedKind::Face,
        ProtectedKind::Hands,
        ProtectedKind::KeyContent,
    ] {
        for region in input.regions.iter().filter(|r| r.kind == kind) {
            if region.is_enforced() && !region.is_inside(rect, SAFETY_MARGIN) {
                return Some(kind.refusal());
            }
        }
    }
    let variant = CropVariant {
        rect,
        ..CropVariant::original()
    };
    if variant.long_edge_fraction(input.aspect) < input.resolution_floor - 1e-4 {
        return Some(GeometryCode::CropTooSmall);
    }
    None
}

/// True when this rectangle passes every rule.
#[must_use]
pub fn is_safe(rect: CropRect, input: &SafetyInput<'_>) -> bool {
    refusal(rect, input).is_none()
}

/// Filter a list of candidates, counting what was refused and why.
///
/// The histogram is [`CropSafetyReport::refused`], which is section 11's
/// `geometry.crop_refused {reason_histogram}`.
#[must_use]
pub fn filter(
    candidates: Vec<CropRect>,
    input: &SafetyInput<'_>,
) -> (Vec<CropRect>, [u32; GeometryCode::REFUSAL_COUNT]) {
    let mut kept = Vec::with_capacity(candidates.len());
    let mut refused = [0u32; GeometryCode::REFUSAL_COUNT];
    for candidate in candidates {
        match refusal(candidate, input) {
            None => kept.push(candidate),
            Some(code) => {
                if let Some(slot) = code.refusal_index().and_then(|i| refused.get_mut(i)) {
                    *slot += 1;
                }
            }
        }
    }
    (kept, refused)
}

/// Build the report for the rectangle that was actually chosen.
#[must_use]
pub fn report(
    chosen: CropRect,
    input: &SafetyInput<'_>,
    refused: [u32; GeometryCode::REFUSAL_COUNT],
) -> CropSafetyReport {
    let faces_checked = input
        .regions
        .iter()
        .filter(|region| region.kind == ProtectedKind::Face && region.is_enforced())
        .count() as u32;
    let hands_checked = input
        .regions
        .iter()
        .filter(|region| region.kind == ProtectedKind::Hands && region.is_enforced())
        .count() as u32;
    let cut = |kind: ProtectedKind| {
        input
            .regions
            .iter()
            .filter(|region| region.kind == kind && region.is_enforced())
            .all(|region| region.is_inside(chosen, SAFETY_MARGIN))
    };
    let variant = CropVariant {
        rect: chosen,
        ..CropVariant::original()
    };
    CropSafetyReport {
        faces_intact: cut(ProtectedKind::Face) && cut(ProtectedKind::Hands),
        resolution_ok: variant.long_edge_fraction(input.aspect) >= input.resolution_floor - 1e-4,
        content_kept: cut(ProtectedKind::KeyContent),
        faces_checked,
        hands_checked,
        refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECT: f32 = 1.5;

    fn region(kind: ProtectedKind, x: f32, y: f32, primary: bool) -> ProtectedRegion {
        ProtectedRegion {
            kind,
            identity: None,
            rect: CropRect {
                x,
                y,
                w: 0.08,
                h: 0.10,
            },
            primary,
        }
    }

    fn input(regions: &[ProtectedRegion]) -> SafetyInput<'_> {
        SafetyInput {
            regions,
            aspect: ASPECT,
            resolution_floor: 0.60,
        }
    }

    fn centred(size: f32) -> CropRect {
        CropRect {
            x: (1.0 - size) / 2.0,
            y: (1.0 - size) / 2.0,
            w: size,
            h: size,
        }
    }

    #[test]
    fn a_crop_that_cuts_a_face_is_refused_and_named() {
        let regions = [region(ProtectedKind::Face, 0.05, 0.45, false)];
        let out = refusal(centred(0.8), &input(&regions));
        assert_eq!(out, Some(GeometryCode::CropCutsFace));
    }

    #[test]
    fn a_primary_identitys_hands_are_protected_and_a_guests_are_not() {
        let primary = [region(ProtectedKind::Hands, 0.05, 0.45, true)];
        assert_eq!(
            refusal(centred(0.8), &input(&primary)),
            Some(GeometryCode::CropCutsHands)
        );
        let guest = [region(ProtectedKind::Hands, 0.05, 0.45, false)];
        assert_eq!(refusal(centred(0.8), &input(&guest)), None);
    }

    #[test]
    fn the_resolution_floor_is_measured_against_the_frame_as_shot() {
        let regions: [ProtectedRegion; 0] = [];
        // A 0.55-wide crop of a 3:2 frame: long edge is 0.55 of the original long edge.
        let small = CropRect {
            x: 0.2,
            y: 0.2,
            w: 0.55,
            h: 0.55,
        };
        assert_eq!(
            refusal(small, &input(&regions)),
            Some(GeometryCode::CropTooSmall)
        );
        let big = CropRect {
            x: 0.1,
            y: 0.1,
            w: 0.80,
            h: 0.80,
        };
        assert_eq!(refusal(big, &input(&regions)), None);
    }

    #[test]
    fn a_face_is_reported_before_a_content_loss() {
        let regions = [
            region(ProtectedKind::KeyContent, 0.02, 0.02, true),
            region(ProtectedKind::Face, 0.05, 0.45, true),
        ];
        assert_eq!(
            refusal(centred(0.8), &input(&regions)),
            Some(GeometryCode::CropCutsFace)
        );
    }

    #[test]
    fn the_histogram_counts_every_refusal_by_its_own_reason() {
        let regions = [region(ProtectedKind::Face, 0.05, 0.45, true)];
        let candidates = vec![
            centred(0.8),  // cuts the face
            centred(0.55), // cuts the face AND is too small - the face is the reported reason
            CropRect {
                x: 0.02,
                y: 0.42,
                w: 0.50,
                h: 0.50,
            }, // keeps the face, falls below the floor
            CropRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            }, // safe
        ];
        let (kept, refused) = filter(candidates, &input(&regions));
        assert_eq!(kept.len(), 1);
        assert_eq!(refused.iter().sum::<u32>(), 3);
        assert_eq!(refused[0], 2, "face refusals");
        assert_eq!(refused[2], 1, "resolution refusals");
    }

    #[test]
    fn an_unchecked_frame_reports_zero_rather_than_a_pass() {
        let regions: [ProtectedRegion; 0] = [];
        let out = report(CropRect::FULL, &input(&regions), [0; 4]);
        assert!(out.all_clear());
        assert!(!out.is_evidence(), "nothing was checked, so nothing is proven");
        assert_eq!(out.faces_checked, 0);
        assert_eq!(out.hands_checked, 0);
    }

    #[test]
    fn a_checked_frame_reports_what_it_checked() {
        let regions = [
            region(ProtectedKind::Face, 0.30, 0.30, true),
            region(ProtectedKind::Face, 0.60, 0.30, false),
            region(ProtectedKind::Hands, 0.45, 0.60, true),
            region(ProtectedKind::Hands, 0.90, 0.90, false),
        ];
        let out = report(CropRect::FULL, &input(&regions), [0; 4]);
        assert_eq!(out.faces_checked, 2);
        assert_eq!(out.hands_checked, 1, "a guest's hands are not checked");
        assert!(out.is_evidence());
        assert!(out.all_clear());
    }

    #[test]
    fn a_region_flush_against_the_edge_is_treated_as_cut() {
        // The rounding argument: a face that ends exactly at the boundary is cut by one pixel
        // of whatever rounding mode the resampler used.
        let regions = [region(ProtectedKind::Face, 0.20, 0.45, true)];
        let flush = CropRect {
            x: 0.20,
            y: 0.10,
            w: 0.75,
            h: 0.85,
        };
        assert_eq!(
            refusal(flush, &input(&regions)),
            Some(GeometryCode::CropCutsFace)
        );
    }
}
