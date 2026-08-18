//! The frames the rest of a chapter will be matched to.
//!
//! PHASE-15 section 6.4, in full:
//!
//! > For each Phase 07 segment, select 3-5 reference frames: high WB confidence, good subject
//! > exposure, primary identity present, no mixed light. Store them with the segment; Phase 25
//! > normalises the rest of the segment toward these anchors, which is the mechanism behind
//! > gallery-wide consistency.
//!
//! ## An anchor is not a correction
//!
//! Nothing in this module changes another frame, and nothing in migration 15 could. Phase 25
//! owns the normalisation; this owns the choice of what to normalise toward, and the two are
//! separate for the reason every "evidence, not decision" split in this product is: two
//! answers to "what should this chapter look like" is an album that does not match the
//! gallery.
//!
//! ## Four eligibility rules and one ranking
//!
//! Section 6.4's four conditions are **gates**: a frame either has a confident white balance,
//! a well-exposed subject, a principal person in it and one illuminant, or it is not a
//! candidate. What is left is then *ranked*, and the ranking adds the one thing section 6.4
//! does not name and phase 25 cannot do without:
//!
//! **an anchor has to be representative.** The best-exposed frame in a chapter can also be
//! the one frame shot from the doorway under a different light, and normalising eighty
//! frames toward it would move the entire chapter somewhere nobody shot. So the ranking
//! penalises distance from the chapter's own median temperature, and the effect is that
//! anchors come from the middle of what the chapter looks like rather than from its edge.
//!
//! ## Fewer than three is an answer
//!
//! A segment with two candidates gets **no anchors at all**, not two. Section 10.1 asks for
//! "at least three candidates" and the reason is not statistical tidiness: normalising a
//! chapter toward one or two frames that happen to qualify is how a whole chapter acquires a
//! colour cast that no single photograph had. `ToneService::reference_frames` returns an
//! empty list, phase 25 leaves that chapter alone, and the outline reports it.

use aura_core::contract::tone::{
    ImageId, ReferenceFrame, MAX_REFERENCE_FRAMES, MIN_REFERENCE_FRAMES,
};
use aura_core::SegmentId;

/// The white-balance confidence a candidate needs.
///
/// Well above `REVIEW_WB_BELOW`, because an anchor is not merely "not worth reviewing" - it
/// is the frame eighty others will be moved toward, and a mistake in it is eighty mistakes.
pub const MIN_ANCHOR_CONF: f32 = 0.72;

/// How far outside its scene's band an anchor's subject luminance may sit.
///
/// Zero: "good subject exposure" is exactly what the band means, and a frame whose subject is
/// outside it is a frame this phase decided to move. Expressed as a constant rather than
/// inlined so that a later release loosening it has to say so.
pub const BAND_SLACK: f32 = 0.0;

/// How much a kelvin of distance from the chapter's median costs, per kelvin.
///
/// Tuned so that 800 K of distance - a visible difference and about the width of a chapter
/// shot under one light - costs about a third of the score. That is enough to prefer a
/// slightly worse frame from the middle over a slightly better one from the edge, and not
/// enough to prefer a bad frame over a good one.
pub const MEDIAN_PENALTY_PER_K: f32 = 0.000_42;

/// One frame offered as a possible anchor.
///
/// Five booleans, which is two more than `clippy::struct_excessive_bools` likes, and the lint
/// is allowed rather than obeyed: four of the five are section 6.4's own four gates and the
/// fifth is the one this module argues for. Gathering them into a sub-struct to satisfy a
/// style rule would hide the fact that they are a checklist.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Candidate {
    /// The photograph.
    pub image_id: ImageId,
    /// Its white-balance confidence.
    pub wb_conf: f32,
    /// Its exposure confidence.
    pub exposure_conf: f32,
    /// Its solved temperature.
    pub temperature_k: f32,
    /// Its solved tint.
    pub tint: f32,
    /// Its subject luminance after the exposure moved.
    pub subject_luma: f32,
    /// True when a face anchored the exposure and the subject landed in the scene's band.
    pub subject_in_band: bool,
    /// True when one of the project's principal identities is in the frame.
    pub has_primary: bool,
    /// True when the frame carries more than one illuminant.
    pub mixed_light: bool,
    /// True when a saturated light was preserved rather than corrected.
    ///
    /// Not one of section 6.4's four conditions and it disqualifies anyway: a purple frame is
    /// a *correct* frame and a terrible anchor, because normalising a chapter toward it would
    /// spread the uplighter across every photograph in it.
    pub coloured_light: bool,
}

impl Candidate {
    /// True when this frame passes section 6.4's four gates.
    #[must_use]
    pub fn is_eligible(&self) -> bool {
        self.wb_conf >= MIN_ANCHOR_CONF
            && self.subject_in_band
            && self.has_primary
            && !self.mixed_light
            && !self.coloured_light
    }

    /// How good an anchor this is before the representativeness penalty, `0..1`.
    ///
    /// The **geometric** mean of the two confidences, so a frame that is certain about its
    /// colour and unsure about its exposure cannot average its way into being an anchor. The
    /// same fusion `ToneEstimate::confidence` uses, and for the same reason: an anchor has to
    /// be right about both.
    #[must_use]
    pub fn base_quality(&self) -> f32 {
        (self.wb_conf.clamp(0.0, 1.0) * self.exposure_conf.clamp(0.0, 1.0)).sqrt()
    }
}

/// Choose one segment's anchors, best first.
///
/// Returns an empty list when fewer than [`MIN_REFERENCE_FRAMES`] frames are eligible. See
/// the module header: that is an answer, not a failure.
#[must_use]
pub fn select(
    segment: SegmentId,
    candidates: &[Candidate],
    analysis_ver: u16,
) -> Vec<ReferenceFrame> {
    let _ = analysis_ver;
    let eligible: Vec<&Candidate> = candidates.iter().filter(|c| c.is_eligible()).collect();
    if eligible.len() < MIN_REFERENCE_FRAMES {
        return Vec::new();
    }

    // The chapter's own middle. A median rather than a mean, because the frames that make an
    // anchor unrepresentative are exactly the outliers a mean would be dragged toward.
    let mut temperatures: Vec<f32> = eligible.iter().map(|c| c.temperature_k).collect();
    temperatures.sort_by(f32::total_cmp);
    let median = temperatures
        .get(temperatures.len() / 2)
        .copied()
        .unwrap_or(5500.0);

    let mut ranked: Vec<(f32, &Candidate)> = eligible
        .into_iter()
        .map(|candidate| {
            let distance = (candidate.temperature_k - median).abs();
            let penalty = (distance * MEDIAN_PENALTY_PER_K).clamp(0.0, 0.9);
            (
                (candidate.base_quality() * (1.0 - penalty)).clamp(0.0, 1.0),
                candidate,
            )
        })
        .collect();
    // Best first, and ties broken by photograph id so the answer does not depend on the order
    // the pass happened to visit a chapter in.
    ranked.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.image_id.to_db().cmp(&b.1.image_id.to_db()))
    });

    ranked
        .into_iter()
        .take(MAX_REFERENCE_FRAMES)
        .enumerate()
        .map(|(rank, (quality, candidate))| ReferenceFrame {
            image_id: candidate.image_id,
            segment,
            rank: rank as u16,
            wb_conf: candidate.wb_conf,
            temperature_k: candidate.temperature_k,
            tint: candidate.tint,
            subject_luma: candidate.subject_luma,
            quality,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::PhotoId;

    fn candidate(temperature_k: f32, wb_conf: f32) -> Candidate {
        Candidate {
            image_id: PhotoId::new(),
            wb_conf,
            exposure_conf: 0.85,
            temperature_k,
            tint: 0.0,
            subject_luma: 0.50,
            subject_in_band: true,
            has_primary: true,
            mixed_light: false,
            coloured_light: false,
        }
    }

    #[test]
    fn two_eligible_frames_produce_no_anchors_at_all() {
        let segment = SegmentId::new();
        let candidates = [candidate(5500.0, 0.90), candidate(5400.0, 0.88)];
        assert!(
            select(segment, &candidates, 1).is_empty(),
            "normalising a chapter toward two frames is how a chapter acquires a cast"
        );
    }

    #[test]
    fn an_outlier_loses_to_the_middle_of_the_chapter() {
        // The best-scoring frame is the one shot from the doorway at 3000 K while everything
        // else was at 5500. It must not be rank 0, or eighty frames get moved toward a
        // doorway.
        let segment = SegmentId::new();
        let mut outlier = candidate(3000.0, 0.99);
        outlier.exposure_conf = 0.99;
        let candidates = [
            outlier,
            candidate(5500.0, 0.90),
            candidate(5450.0, 0.90),
            candidate(5550.0, 0.90),
            candidate(5480.0, 0.90),
        ];
        let chosen = select(segment, &candidates, 1);
        assert_eq!(chosen.len(), MAX_REFERENCE_FRAMES);
        let first = chosen.first().map_or(0.0, |frame| frame.temperature_k);
        assert!(
            (first - 5500.0).abs() < 200.0,
            "rank 0 came from the edge of the chapter: {first} K"
        );
    }

    #[test]
    fn a_mixed_light_frame_is_never_an_anchor() {
        let segment = SegmentId::new();
        let mut mixed = candidate(5500.0, 0.99);
        mixed.mixed_light = true;
        let candidates = [
            mixed,
            candidate(5500.0, 0.80),
            candidate(5500.0, 0.80),
            candidate(5500.0, 0.80),
        ];
        let chosen = select(segment, &candidates, 1);
        assert_eq!(chosen.len(), 3);
        assert!(chosen.iter().all(|frame| frame.image_id != mixed.image_id));
    }

    #[test]
    fn a_preserved_coloured_frame_is_never_an_anchor() {
        let segment = SegmentId::new();
        let mut purple = candidate(5500.0, 0.99);
        purple.coloured_light = true;
        let candidates = [
            purple,
            candidate(5500.0, 0.80),
            candidate(5500.0, 0.80),
            candidate(5500.0, 0.80),
        ];
        let chosen = select(segment, &candidates, 1);
        assert!(chosen.iter().all(|frame| frame.image_id != purple.image_id));
    }

    #[test]
    fn the_answer_does_not_depend_on_input_order() {
        let segment = SegmentId::new();
        let candidates = [
            candidate(5500.0, 0.90),
            candidate(5450.0, 0.90),
            candidate(5550.0, 0.90),
            candidate(5480.0, 0.90),
        ];
        let forward = select(segment, &candidates, 1);
        let mut reversed = candidates;
        reversed.reverse();
        let backward = select(segment, &reversed, 1);
        let forward_ids: Vec<String> = forward.iter().map(|f| f.image_id.to_db()).collect();
        let backward_ids: Vec<String> = backward.iter().map(|f| f.image_id.to_db()).collect();
        assert_eq!(forward_ids, backward_ids);
    }
}
