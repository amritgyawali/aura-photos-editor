//! Calming what is behind the subject.
//!
//! PHASE-19 section 6.2's second and third bullets: measure the competition explicitly, and
//! fall off in a way that does not trace an outline.
//!
//! ## Why this module is small
//!
//! Because the *decision* lives in [`crate::local::subject`], with the subject half it is
//! paired with, and section 6.2's first rule is that the two are never taken apart. Splitting
//! the arithmetic across two modules would create exactly the seam where somebody later
//! applies one without the other.
//!
//! What is left here is the part that is genuinely about the background alone: reading phase
//! 11's own findings about what is behind the subject, and the edge-aware falloff.

use aura_core::contract::composition::{CompositionFlags, CompositionResult};
use aura_core::contract::local::{BackgroundBalanceDelta, MaskField};

use crate::local::measure::{FrameMeasure, RegionStats};

/// How many bright blobs phase 11 found behind the subject.
///
/// Section 6.2 asks for a "count of high-luminance blobs (from Phase 11)" as one of the three
/// measured triggers. Phase 11 stores it as a flag rather than as a count -
/// [`CompositionFlags::BRIGHT_BLOB`] - so this is the honest reading of what is actually
/// there: one when the flag is set and phase 11 saw a blob brighter than the subject, plus one
/// each for the two related findings that describe the same failure differently.
///
/// **It is not a new measurement.** `CompositionService` is the only way to ask how a frame is
/// composed, and re-finding bright regions here would be a second answer to a question phase
/// 11 owns.
#[must_use]
pub fn bright_blobs(composition: Option<&CompositionResult>) -> u8 {
    let Some(result) = composition else {
        return 0;
    };
    let mut count = 0u8;
    if result.flags.contains(CompositionFlags::BRIGHT_BLOB) {
        count += 1;
    }
    if result.flags.contains(CompositionFlags::HEAD_MERGE) {
        count += 1;
    }
    if result.flags.contains(CompositionFlags::COLOUR_COMPETITION) {
        count += 1;
    }
    count
}

/// The background statistics, from the background mask when there is one.
///
/// **There is no fallback that draws a background.** When phase 18 has supplied no background
/// field this returns `None`, the paired operation does not run, and the plan records
/// `MaskUnavailable`. A geometric substitute - "everything outside the subject box" - is a
/// mask, and phase 19 does not own masks; the outline it would trace is the artefact section
/// 12's first row names.
#[must_use]
pub fn stats(frame: &FrameMeasure, background: Option<&MaskField>) -> Option<RegionStats> {
    let field = background?;
    if !field.is_usable() || !field.is_readable() {
        return None;
    }
    let stats = frame.region(field);
    if stats.is_empty() {
        return None;
    }
    Some(stats)
}

/// The feather a background reduction should carry, given the mask's own edge quality.
///
/// Section 6.2 asks for "edge-aware falloff using the subject alpha plus a guided filter so
/// the background reduction does not trace a visible outline". The guided filter lives in
/// `aura-render`'s `local_apply` shader, where the full-resolution matte is; what is decided
/// here is how wide to ask it to be, and the rule is the one that surprises people: **a worse
/// mask gets a wider feather, not a smaller edit**.
///
/// The reasoning is that the visible artefact is the *gradient* across the boundary, which is
/// the edit's magnitude divided by the transition's width. Reducing the magnitude and
/// widening the transition both help; widening it is free, and reducing the magnitude costs
/// the edit. So the strength scaling in [`MaskField::strength_scale`] handles the confidence
/// and this handles the edge.
#[must_use]
pub fn feather_for(edge_quality: f32) -> f32 {
    let clean = edge_quality.clamp(0.0, 1.0);
    // 0.55 for a perfect matte, 0.95 for a hopeless one.
    0.95f32.mul_add(1.0 - clean, 0.55 * clean).clamp(0.55, 0.95)
}

/// Apply the feather decision to a solved delta.
#[must_use]
pub fn with_edge_aware_feather(
    mut delta: BackgroundBalanceDelta,
    edge_quality: f32,
) -> BackgroundBalanceDelta {
    delta.feather = feather_for(edge_quality);
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_composition_reading_means_no_blobs_rather_than_a_guess() {
        assert_eq!(bright_blobs(None), 0);
    }

    #[test]
    fn a_worse_mask_gets_a_wider_feather() {
        assert!(feather_for(0.1) > feather_for(0.95));
        assert!(feather_for(1.0) >= 0.55);
        assert!(feather_for(0.0) <= 0.95);
    }

    #[test]
    fn there_is_no_background_without_a_background_mask() {
        let frame = FrameMeasure::of(&[128u8; 3 * 16], 4, 4);
        assert!(stats(&frame, None).is_none());
    }
}
