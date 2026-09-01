//! The ten inspections, and the port they read. PHASE-27 sections 2.1 and 6.1.
//!
//! ## Why the checks take a struct rather than thirteen services
//!
//! Every inspection here is a comparison between numbers phases 08 to 26 already measured. It would
//! be possible to hand each check a `&dyn ToneService` and let it fetch what it needs, and that
//! shape was rejected for three reasons.
//!
//! It would make every check a database read, so ten checks over a thousand frames would be ten
//! thousand round trips inside section 11's 90 s budget. It would make a check untestable without a
//! catalog, which is exactly backwards for the phase whose whole product is a *number*. And it
//! would hide the thing this phase most needs to be explicit about: **which inputs were absent.**
//!
//! [`Frame`] is that port. Every reading on it is an `Option`, [`api::collect`](crate::api) fills
//! it once per image through the frozen traits, and a check handed a frame with its own reading
//! missing returns [`Outcome::Skipped`] rather than [`Outcome::Clean`]. ADR-0055 section 8, and it
//! is phase 19's `MaskField` shape applied to thirteen inputs instead of one.
//!
//! ## Clean and Skipped are different values, and that is the point
//!
//! A wedding whose masks are absent must not report zero mask artefacts and read as a clean bill of
//! health. In this build, with several upstream heads untrained, that is the common case rather
//! than the exotic one - so the distinction is not defensive programming, it is the difference
//! between an honest report and a dangerous one.

pub mod cleanup;
pub mod consistency;
pub mod coverage;
pub mod crop;
pub mod duplicate;
pub mod exposure;
pub mod mask;
pub mod retouch;
pub mod sharpness;
pub mod skin;

use aura_core::contract::ids::IdentityId;
use aura_core::contract::qc::{Evidence, ImageId, QcCategory, QcCode};
use aura_core::contract::scene::SceneId;
use aura_vision::contract::mask::MaskKind;

use crate::policy::Thresholds;

// ---------------------------------------------------------------------------
// What one inspection produces
// ---------------------------------------------------------------------------

/// One measured problem: a number, the threshold it failed, and what to do about it.
///
/// Deliberately *not* a `QcTicket`. A finding has no id, no autonomy band and no status - those are
/// assigned by [`crate::ticket`], which is the one place a finding becomes a thing a photographer
/// sees. Keeping them separate is what lets a check be a pure function over [`Frame`] and a
/// threshold row, with no clock, no id generator and no catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Which inspection.
    pub category: QcCategory,
    /// What exactly. Always a code with `QcCode::is_finding`.
    pub code: QcCode,
    /// How far from acceptable, in `QcCategory::unit`.
    pub deviation: f32,
    /// What acceptable was.
    pub threshold: f32,
    /// What to look at.
    pub evidence: Evidence,
    /// The person this is about, when it is about one.
    pub identity: Option<IdentityId>,
    /// How much of the deviation a remedy is predicted to remove.
    ///
    /// A prediction rather than a promise, which is exactly why [`crate::reedit`] measures what was
    /// actually realised against it. Zero is legal and means "nothing mechanical will help", which
    /// [`crate::remedy`] turns into an escalation.
    pub expected_gain: f32,
    /// How sure, `0..1`.
    pub confidence: f32,
    /// Supporting reasons beyond the finding code itself.
    pub extra_reasons: Vec<(QcCode, f32)>,
}

impl Finding {
    /// A finding with no extra reasons and no identity.
    #[must_use]
    pub fn new(
        category: QcCategory,
        code: QcCode,
        deviation: f32,
        threshold: f32,
        expected_gain: f32,
        confidence: f32,
    ) -> Self {
        Self {
            category,
            code,
            deviation,
            threshold,
            evidence: Evidence::None,
            identity: None,
            expected_gain,
            confidence,
            extra_reasons: Vec::new(),
        }
    }

    /// The same finding, pointing at something.
    #[must_use]
    pub fn with_evidence(mut self, evidence: Evidence) -> Self {
        self.evidence = evidence.bounded();
        self
    }

    /// The same finding, about a person.
    #[must_use]
    pub const fn about(mut self, identity: IdentityId) -> Self {
        self.identity = Some(identity);
        self
    }

    /// The same finding with one more reason.
    #[must_use]
    pub fn because(mut self, code: QcCode, weight: f32) -> Self {
        self.extra_reasons.push((code, weight));
        self
    }

    /// How far past the threshold, as a multiple of it.
    #[must_use]
    pub fn severity(&self) -> f32 {
        if self.threshold <= 0.0 || !self.deviation.is_finite() {
            return 0.0;
        }
        (self.deviation / self.threshold).max(0.0)
    }
}

/// What one inspection concluded.
///
/// Three variants, and the third is the one this phase exists to be honest about. `Clean` is a
/// claim - the product looked and found nothing. `Skipped` is the absence of one.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The check ran and found nothing.
    Clean,
    /// The check ran and found these.
    Found(Vec<Finding>),
    /// The check could not run, because this input was absent.
    ///
    /// The string names what was missing, in the terms a runbook uses - `"no scene node target"`,
    /// `"no integrity verdict"`. It is a diagnostic rather than user copy: the sentence a
    /// photographer reads is `QcCode::CheckSkipped::user_text`.
    Skipped(&'static str),
}

impl Outcome {
    /// The findings, or none.
    #[must_use]
    pub fn findings(self) -> Vec<Finding> {
        match self {
            Self::Found(list) => list,
            Self::Clean | Self::Skipped(_) => Vec::new(),
        }
    }

    /// True when this inspection actually ran.
    #[must_use]
    pub const fn ran(&self) -> bool {
        !matches!(self, Self::Skipped(_))
    }

    /// `Found` when the list is non-empty, `Clean` when it is.
    ///
    /// The constructor every check ends with, so that "found nothing" is never an empty `Found`
    /// somebody later reads as a finding count of zero *and* a check that produced results.
    #[must_use]
    pub fn from_findings(list: Vec<Finding>) -> Self {
        if list.is_empty() {
            Self::Clean
        } else {
            Self::Found(list)
        }
    }
}

// ---------------------------------------------------------------------------
// The input port
// ---------------------------------------------------------------------------

/// What phase 25 decided about this frame's lighting group.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeReading {
    /// The group's target colour temperature, in kelvin.
    pub target_cct_k: f32,
    /// How far a frame may sit from it before the group stops being one light.
    pub cct_tol: f32,
    /// The group's target tint.
    pub target_tint: f32,
    /// Its tolerance.
    pub tint_tol: f32,
    /// The group's target subject luminance, `0..1`.
    pub target_luma: f32,
    /// Its tolerance, in EV.
    pub luma_tol: f32,
    /// The group's eight-number grade character.
    pub target_signature: [f32; 8],
    /// This frame's own eight-number grade character, after everything.
    ///
    /// `None` when the frame carries no stored signature, which skips the signature half of the
    /// consistency check rather than passing it. The alternative - defaulting to the node's own
    /// target - would read as a perfect match on every frame nobody could measure, which is exactly
    /// the failure ADR-0055 section 8 exists to prevent. In this build most frames are that case,
    /// because a signature is built from phase 05's stored descriptors and phase 25's own pass is
    /// what fills them.
    pub frame_signature: Option<[f32; 8]>,
    /// This frame's colour temperature after normalisation.
    pub frame_cct_k: f32,
    /// Its tint after normalisation.
    pub frame_tint: f32,
    /// Its subject luminance after normalisation.
    pub frame_luma: f32,
    /// The frames the target was built from.
    pub anchors: Vec<ImageId>,
    /// True when the node has a target at all.
    ///
    /// A node with three frames and no anchors has no target, and a consistency check against a
    /// node like that is not a check that passed. `GalleryCode::NodeUnanchored` upstream, and
    /// `Outcome::Skipped` here.
    pub anchored: bool,
}

/// What this frame's skin looks like against each person's own gallery target.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinReading {
    /// Per identity: how far this frame's skin sits from that person's gallery target, in dE00.
    pub per_identity_de00: Vec<(IdentityId, f32)>,
    /// The largest hue shift phase 16's guard measured, in degrees.
    pub guard_hue_shift_deg: f32,
    /// The largest chroma change it measured.
    pub guard_chroma_change: f32,
    /// True when phase 16 measured the guard on real pixels rather than reporting an unmeasured
    /// default.
    ///
    /// False turns the guard half of this check into a skip. A `SkinGuardReport` with
    /// `measured = false` carries zeroes, and zeroes read as a perfect result.
    pub guard_measured: bool,
}

/// What phases 09, 15 and 16 left the frame's exposure at.
#[derive(Debug, Clone, PartialEq)]
pub struct ExposureReading {
    /// Subject luminance the edit landed on, `0..1`.
    ///
    /// `None` skips the drift half of the check rather than passing it. Nothing in this build
    /// records the luminance a finished frame actually landed on - phase 15 stores the band it
    /// solved toward and phase 25 stores the move it still owes, and neither is the delivered
    /// value - so filling this from a proxy would report every frame as sitting exactly on its
    /// target. That is a clean bill of health nobody measured, which is the one thing this phase
    /// exists not to produce. Same shape, and the same reason, as `NodeReading::frame_signature`.
    pub subject_luma: Option<f32>,
    /// The scene band's centre for this frame.
    pub target_luma: f32,
    /// Fraction of the frame clipped white after the edit.
    pub clip_hi_after: f32,
    /// Fraction clipped black after the edit.
    pub clip_lo_after: f32,
    /// Fraction clipped white before it.
    pub clip_hi_before: f32,
    /// Fraction clipped black before it.
    pub clip_lo_before: f32,
    /// Phase 09's measured shadow headroom, `0..1`. Lower is less room.
    ///
    /// `None` skips the crushed-shadow half. No frozen contract carries the finished frame's
    /// remaining shadow room: phase 09 measures noise before the edit and phase 16 turns a
    /// headroom in EV into a lift allowance without storing either, so a number here would be an
    /// inference dressed as a measurement.
    pub shadow_headroom: Option<f32>,
}

/// What phases 09 and 22 left the frame's detail at.
#[derive(Debug, Clone, PartialEq)]
pub struct SharpnessReading {
    /// Subject sharpness after restoration, normalised `0..1`.
    pub subject_sharpness: f32,
    /// Subject sharpness relative to the background, `0..1`.
    pub relative_sharpness: f32,
    /// Phase 22's texture retention after denoising, `0..1`. One is nothing lost.
    pub texture_retention: f32,
    /// Phase 22's measured ringing, `0..1`. Zero is none.
    pub ringing: f32,
    /// The largest identity drift on any recovered face, `0..1`.
    pub identity_drift: f32,
    /// How many regions the self-check was measured on. Zero means it did not run.
    pub selfcheck_measured_on: u32,
}

/// What phases 19, 20 and 21 left the frame's skin and shaping at.
#[derive(Debug, Clone, PartialEq)]
pub struct RetouchReading {
    /// Phase 20's measured high-band energy ratio after retouch.
    pub texture_band_ratio: f32,
    /// The floor it was held to.
    pub texture_floor: f32,
    /// True when phase 20 withdrew the whole plan rather than ship one that missed its floor.
    pub texture_withdrawn: bool,
    /// True when phase 20 measured the ratio on real pixels.
    pub texture_measured: bool,
    /// Phase 21's catchlight ratio after the eye operations. One is unchanged.
    pub catchlight_ratio: f32,
    /// Its hairline energy ratio.
    pub hair_energy_ratio: f32,
    /// Its teeth excursion beyond the frame's own neutral.
    pub teeth_excursion: f32,
    /// The share of phase 19's per-image perceptual allowance that was spent, `0..`.
    pub allowance_used: f32,
}

/// What phase 18 determined about the regions this frame was edited through.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskReading {
    /// Per region: which kind, how confident the class was, how well determined the boundary was,
    /// and how strongly an operation ran inside it.
    pub regions: Vec<MaskRegion>,
    /// Operations phase 19 gated because no region supported them.
    ///
    /// A gated operation is not an artefact - it is the product declining to edit - but a frame
    /// where an operation ran at full strength *and* the region is listed here is a contradiction
    /// worth a ticket.
    pub gated: Vec<MaskKind>,
}

/// One region, and what was done inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaskRegion {
    /// Which class.
    pub kind: MaskKind,
    /// How sure the class is, `0..1`.
    pub confidence: f32,
    /// How well determined the boundary is, `0..1`.
    pub edge_quality: f32,
    /// How strongly the strongest operation inside it ran, `0..1`.
    ///
    /// Phase 18's rule is that a region says how much may be done with it and a later phase
    /// multiplies. This check reads the *product*: a region at allowance 0.3 with an operation at
    /// 0.9 is an edit through a boundary that did not support it, and that is the artefact.
    pub applied_strength: f32,
}

impl MaskRegion {
    /// Phase 18's allowance: the geometric mean of the two independent uncertainties.
    ///
    /// Recomputed here rather than passed, because it is the *definition* of what a region permits
    /// and a second stored copy is a second answer that can drift from `Mask::allowance`.
    #[must_use]
    pub fn allowance(&self) -> f32 {
        let confidence = self.confidence.clamp(0.0, 1.0);
        let edge = self.edge_quality.clamp(0.0, 1.0);
        (confidence * edge).max(0.0).sqrt()
    }

    /// How much the applied strength exceeded what the region supports, `0..1`.
    #[must_use]
    pub fn overreach(&self) -> f32 {
        (self.applied_strength.clamp(0.0, 1.0) - self.allowance()).max(0.0)
    }
}

/// What phase 23 left the frame's edges at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropReading {
    /// True when every face survived the delivered crop.
    pub faces_intact: bool,
    /// True when the delivered crop meets its resolution floor.
    pub resolution_ok: bool,
    /// True when it kept enough of the frame's content.
    pub content_kept: bool,
    /// The delivered crop's long edge as a fraction of the original's, `0..1`.
    pub long_edge_fraction: f32,
    /// The floor for this frame's purpose.
    pub long_edge_floor: f32,
    /// How many faces the safety report actually checked. Zero means it could not check any.
    pub faces_checked: u32,
}

/// What phase 24 removed from this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct CleanupReading {
    /// Per removal: the self-check's measured artefact score, `0..1`, and whether it is disclosed.
    pub removals: Vec<(f32, bool)>,
}

/// This frame's nearest neighbour in the **delivered gallery**.
///
/// Nearest in the gallery rather than in the project, which is the whole check: two near-identical
/// frames that both survived the cull are a duplicate leak, and a near-identical frame that phase
/// 12 rejected is phase 12 working.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuplicateReading {
    /// The neighbour.
    pub neighbour: ImageId,
    /// The 64-bit difference-hash distance between them, `0..=64`.
    pub hamming: u32,
    /// True when phase 08 put both frames in the same moment.
    ///
    /// Two frames of one moment being similar is what a moment *is*; the leak is when both were
    /// delivered anyway. A pair from two different moments at a small hamming distance is a
    /// stronger finding, because phase 08 did not even think they were the same shot.
    pub same_moment: bool,
}

/// Everything one inspection round needs to know about one photograph.
///
/// Every reading is an `Option`, and `None` means the upstream phase produced nothing for this
/// frame - which is a *skip*, never a pass. See this module's header.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame {
    /// Which photograph.
    pub image_id: ImageId,
    /// What kind of scene it is. Every threshold in this phase is a function of it. Invariant 7.
    pub scene: SceneId,
    /// The best alternative in the same moment, when phase 12 found one.
    pub runner_up: Option<ImageId>,
    /// True when a coverage guarantee is holding this frame in the gallery.
    ///
    /// Read by [`replace`](crate::replace): a protected frame may still be swapped, and only for a
    /// runner-up that carries the same guarantee.
    pub coverage_protected: bool,
    /// True when the photographer edited this frame's parameters by hand.
    ///
    /// Nothing in this phase may propose a parameter remedy on one. The finding is still reported,
    /// with `QcCode::UserEdited` as its outcome, because a photographer is entitled to know their
    /// own frame sits outside its group - and is entitled to have AURA leave it alone.
    pub user_edited: bool,
    /// Phase 25's lighting group.
    pub node: Option<NodeReading>,
    /// Phases 15, 16 and 25 on skin.
    pub skin: Option<SkinReading>,
    /// Phases 09, 15 and 16 on exposure.
    pub exposure: Option<ExposureReading>,
    /// Phases 09 and 22 on detail.
    pub sharpness: Option<SharpnessReading>,
    /// Phases 19, 20 and 21 on skin and shaping.
    pub retouch: Option<RetouchReading>,
    /// Phase 18 on regions.
    pub mask: Option<MaskReading>,
    /// Phase 23 on edges.
    pub crop: Option<CropReading>,
    /// Phase 24 on removals.
    pub cleanup: Option<CleanupReading>,
    /// The nearest delivered neighbour.
    pub duplicate: Option<DuplicateReading>,
}

impl Frame {
    /// A frame with nothing but an id and a scene. Every check skips.
    #[must_use]
    pub fn empty(image_id: ImageId, scene: SceneId) -> Self {
        Self {
            image_id,
            scene,
            ..Self::default()
        }
    }

    /// How many of the eight per-frame readings are present.
    ///
    /// What `QcOutline::inspection_completeness` is built from, one frame at a time.
    #[must_use]
    pub fn readings_present(&self) -> usize {
        usize::from(self.node.is_some())
            + usize::from(self.skin.is_some())
            + usize::from(self.exposure.is_some())
            + usize::from(self.sharpness.is_some())
            + usize::from(self.retouch.is_some())
            + usize::from(self.mask.is_some())
            + usize::from(self.crop.is_some())
            + usize::from(self.cleanup.is_some())
    }
}

/// What the two gallery-scoped checks need, once per project.
///
/// Coverage and duplicates are facts about the *set*, so they are measured once rather than per
/// frame. `QcCategory::is_gallery_scoped` is the same distinction in the contract, and it is what
/// makes the re-inspection in [`crate::reedit`] re-run them over the gallery instead of over one
/// photograph.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SetContext {
    /// Must-have rules that are not covered at all, with the rule's own name.
    pub missing_rules: Vec<String>,
    /// Must-have rules covered only by a frame that lost a veto.
    pub weak_rules: Vec<String>,
    /// Identities appearing fewer times than the minimum, with their count and the minimum.
    pub under_covered: Vec<(IdentityId, u32, u32)>,
    /// True when phase 12 produced a coverage report at all.
    ///
    /// False is a skip. A project with no selection has no coverage to be missing.
    pub coverage_available: bool,
}

// ---------------------------------------------------------------------------
// Running the battery
// ---------------------------------------------------------------------------

/// Every per-frame inspection, in `QcCategory::ALL` order.
///
/// A `const` array of function pointers rather than a `Vec<Box<dyn Inspection>>`, because there are
/// exactly ten of them, they are known at compile time, and the array's length is what
/// `checks_run` is counted against. A registry that could be added to at runtime would make the
/// denominator of section 10.1's completeness figure a function of what happened to be loaded.
type FrameCheck = fn(&Frame, &Thresholds) -> Outcome;

/// The eight checks that look at one photograph.
///
/// Coverage and duplicate are not here: coverage is measured once over the set, and duplicate needs
/// a neighbour that only exists once the delivered gallery is known. [`run_frame`] runs these eight
/// plus the duplicate check, which does take a `Frame`; [`coverage::inspect`] is called separately.
pub const FRAME_CHECKS: [FrameCheck; 9] = [
    consistency::inspect,
    skin::inspect,
    exposure::inspect,
    sharpness::inspect,
    retouch::inspect,
    mask::inspect,
    crop::inspect,
    cleanup::inspect,
    duplicate::inspect,
];

/// One frame through every per-frame inspection.
///
/// Returns the outcomes in [`FRAME_CHECKS`] order, so a caller can count what ran and what skipped
/// without matching on the findings. The checks are independent and read-only by construction -
/// nine `fn(&Frame, &Thresholds)` pointers with no shared state - which is what makes section 6.1's
/// "run in parallel across the gallery on all cores" a property rather than an aspiration.
#[must_use]
pub fn run_frame(frame: &Frame, thresholds: &Thresholds) -> Vec<Outcome> {
    FRAME_CHECKS
        .iter()
        .map(|check| check(frame, thresholds))
        .collect()
}

/// Every finding for one frame, worst first, capped at `MAX_TICKETS_PER_IMAGE`.
///
/// When the cap bites, the surviving list is replaced by a single `MultiSymptom` finding: a frame
/// that failed nine checks does not need the tenth reported, it needs a person. That is also
/// section 7's planner trigger, which is why the code is a finding rather than an outcome.
#[must_use]
pub fn findings_for(frame: &Frame, thresholds: &Thresholds) -> Vec<Finding> {
    let mut all: Vec<Finding> = run_frame(frame, thresholds)
        .into_iter()
        .flat_map(Outcome::findings)
        .collect();
    all.sort_by(|left, right| {
        right
            .severity()
            .total_cmp(&left.severity())
            .then_with(|| {
                left.category
                    .triage_rank()
                    .cmp(&right.category.triage_rank())
            })
            .then_with(|| left.code.cmp(&right.code))
    });
    if all.len() > aura_core::contract::qc::MAX_TICKETS_PER_IMAGE {
        let worst = all.first().map_or(1.0, Finding::severity);
        let count = all.len() as f32;
        let mut multi = Finding::new(
            QcCategory::Consistency,
            QcCode::MultiSymptom,
            count,
            aura_core::contract::qc::MAX_TICKETS_PER_IMAGE as f32,
            0.0,
            // Confidence in "there are many problems here" is high exactly because it is a count
            // rather than a judgement about any one of them.
            0.95,
        );
        multi.extra_reasons.push((QcCode::EscalatedToHuman, 1.0));
        let _ = worst;
        return vec![multi];
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Thresholds;

    #[test]
    fn an_empty_frame_skips_every_check_and_passes_none() {
        let frame = Frame::empty(ImageId::new(), SceneId::Unknown);
        let thresholds = Thresholds::reference();
        let outcomes = run_frame(&frame, &thresholds);
        assert_eq!(outcomes.len(), FRAME_CHECKS.len());
        for outcome in &outcomes {
            assert!(
                matches!(outcome, Outcome::Skipped(_)),
                "an absent reading must skip rather than pass: {outcome:?}"
            );
            assert!(!outcome.ran());
        }
    }

    #[test]
    fn clean_and_skipped_are_different_values() {
        // The whole of ADR-0055 section 8, as an assertion. If these ever compared equal, a wedding
        // nobody could check would read as a wedding with nothing wrong with it.
        assert_ne!(Outcome::Clean, Outcome::Skipped("no reading"));
        assert!(Outcome::Clean.ran());
        assert!(!Outcome::Skipped("x").ran());
    }

    #[test]
    fn an_empty_finding_list_is_clean_rather_than_an_empty_found() {
        assert_eq!(Outcome::from_findings(Vec::new()), Outcome::Clean);
    }

    #[test]
    fn a_regions_allowance_is_the_geometric_mean_so_neither_number_can_rescue_the_other() {
        let confident_but_ragged = MaskRegion {
            kind: MaskKind::Face,
            confidence: 1.0,
            edge_quality: 0.04,
            applied_strength: 0.0,
        };
        assert!(confident_but_ragged.allowance() < 0.25);
        let both_fair = MaskRegion {
            edge_quality: 0.49,
            confidence: 0.49,
            ..confident_but_ragged
        };
        assert!(both_fair.allowance() > confident_but_ragged.allowance());
    }

    #[test]
    fn overreach_is_zero_when_the_region_supports_what_was_done() {
        let region = MaskRegion {
            kind: MaskKind::Face,
            confidence: 0.9,
            edge_quality: 0.9,
            applied_strength: 0.5,
        };
        assert_eq!(region.overreach(), 0.0);
        let pushed = MaskRegion {
            applied_strength: 1.0,
            ..region
        };
        assert!(pushed.overreach() > 0.0);
    }
}
