//! The injected-defect corpus. PHASE-27 section 8, step 1.
//!
//! ## Why this module is first in the implementation order
//!
//! Section 8: "Build the injected-defect corpus first - QC is tested by its ability to catch known
//! defects."
//!
//! That ordering is not a convention. A QC agent's gates are unusual in this product: every phase
//! from 09 onward measures *its own output* against ground truth, and this phase measures **other
//! phases' outputs**, so there is no natural label. The only way to know whether a check works is to
//! author a frame with a known problem in it and ask whether the check finds it - and the only way
//! to know whether a check is quiet is to author a frame with nothing wrong and ask whether it stays
//! silent.
//!
//! So this file defines both halves. [`DEFECTS`] is twenty frames, each with exactly one thing wrong
//! and the category that should catch it; [`clean_gallery`] is frames with nothing wrong, which is
//! what the false-ticket rate is measured against.
//!
//! ## What these fixtures are, and what they are not
//!
//! They are **readings**, not pixels. Nothing in this crate opens a photograph, so a fixture here is
//! a `Frame` whose numbers were chosen - a skin drift of 6.0 dE00, a ringing of 0.9, a crop that
//! cuts a face.
//!
//! That proves the arithmetic, the thresholds, the triage ordering, the loop's revert and the
//! store. **It proves nothing about a wedding photograph**, because the numbers phases 09 to 26
//! would produce on a real frame come from placeholder heads. That is condition C1 of the exit
//! report and it closes with phase 05's C10 rather than separately.
//!
//! ## Each defect is exactly one thing
//!
//! A fixture with two problems cannot tell a detection failure from a triage failure: if the check
//! fires once, was it right about the one it named? Twenty single-symptom frames plus a handful of
//! deliberately multi-symptom ones, kept apart, is what makes section 10.1's "90 % of injected
//! defects caught" a number rather than an impression.

use aura_core::contract::ids::IdentityId;
use aura_core::contract::qc::{ImageId, QcCategory, QcCode};
use aura_core::contract::scene::SceneId;
use aura_vision::contract::mask::MaskKind;

use crate::checks::{
    CleanupReading, CropReading, DuplicateReading, ExposureReading, Frame, MaskReading, MaskRegion,
    NodeReading, RetouchReading, SetContext, SharpnessReading, SkinReading,
};

/// One frame with one known problem, and what should catch it.
#[derive(Debug, Clone)]
pub struct Defect {
    /// What it is, for a failure message.
    pub name: &'static str,
    /// The frame.
    pub frame: Frame,
    /// The inspection that must find it.
    pub category: QcCategory,
    /// The exact code it must report.
    ///
    /// Not just the category: a build that reported a skin drift as a guard excursion would pass a
    /// category-level gate while sending every photographer to the wrong remedy.
    pub code: QcCode,
}

/// A frame with nothing wrong with it.
///
/// Every reading present and every one comfortably inside the reference row's thresholds. What the
/// false-ticket rate is measured against, and the baseline every defect below is a single edit away
/// from - which is what makes "exactly one thing wrong" true by construction rather than by
/// inspection.
#[must_use]
pub fn healthy(scene: SceneId) -> Frame {
    let identity = IdentityId::new();
    Frame {
        image_id: ImageId::new(),
        scene,
        runner_up: None,
        coverage_protected: false,
        user_edited: false,
        node: Some(NodeReading {
            target_cct_k: 5200.0,
            cct_tol: 200.0,
            target_tint: 0.0,
            tint_tol: 4.0,
            target_luma: 0.45,
            luma_tol: 0.20,
            target_signature: [0.5; 8],
            frame_signature: Some([0.5; 8]),
            frame_cct_k: 5200.0,
            frame_tint: 0.0,
            frame_luma: 0.45,
            anchors: vec![ImageId::new(), ImageId::new(), ImageId::new()],
            anchored: true,
        }),
        skin: Some(SkinReading {
            per_identity_de00: vec![(identity, 0.6)],
            guard_hue_shift_deg: 0.3,
            guard_chroma_change: 0.01,
            guard_measured: true,
        }),
        exposure: Some(ExposureReading {
            subject_luma: Some(0.45),
            target_luma: 0.45,
            clip_hi_after: 0.01,
            clip_lo_after: 0.0,
            clip_hi_before: 0.01,
            clip_lo_before: 0.0,
            shadow_headroom: Some(0.95),
        }),
        sharpness: Some(SharpnessReading {
            subject_sharpness: 0.80,
            relative_sharpness: 0.80,
            texture_retention: 0.97,
            ringing: 0.01,
            identity_drift: 0.0,
            selfcheck_measured_on: 4,
        }),
        retouch: Some(RetouchReading {
            texture_band_ratio: 0.95,
            texture_floor: 0.70,
            texture_withdrawn: false,
            texture_measured: true,
            catchlight_ratio: 1.0,
            hair_energy_ratio: 1.0,
            teeth_excursion: 0.0,
            allowance_used: 0.55,
        }),
        mask: Some(MaskReading {
            regions: vec![MaskRegion {
                kind: MaskKind::Face,
                confidence: 0.95,
                edge_quality: 0.93,
                applied_strength: 0.55,
            }],
            gated: Vec::new(),
        }),
        crop: Some(CropReading {
            faces_intact: true,
            resolution_ok: true,
            content_kept: true,
            long_edge_fraction: 0.92,
            long_edge_floor: 0.50,
            faces_checked: 2,
        }),
        cleanup: Some(CleanupReading {
            removals: Vec::new(),
        }),
        duplicate: Some(DuplicateReading {
            neighbour: ImageId::new(),
            hamming: 28,
            same_moment: false,
        }),
    }
}

/// A gallery of frames with nothing wrong with them.
///
/// The false-ticket denominator. Section 10.1 gates the rate at 8 %, which on a corpus this size is
/// a very small number of tickets - deliberately, because the failure this gate exists to catch is a
/// build that fires on everything and buries the queue.
#[must_use]
pub fn clean_gallery(count: usize) -> Vec<Frame> {
    let scenes = [
        SceneId::Ceremony,
        SceneId::FamilyPortrait,
        SceneId::DanceFloor,
        SceneId::Details,
        SceneId::CouplePortrait,
        SceneId::Speeches,
        SceneId::GoldenHour,
        SceneId::Candid,
    ];
    (0..count)
        .map(|index| {
            let scene = scenes
                .get(index % scenes.len())
                .copied()
                .unwrap_or(SceneId::Unknown);
            healthy(scene)
        })
        .collect()
}

/// Twenty frames, each with exactly one known problem.
///
/// Every category is represented and every one is reachable at the reference thresholds. The scene
/// on each frame is chosen so the shipped table's row for that scene would also catch it, which is
/// what stops a gate passing on the permissive `unknown` row while failing on every real wedding.
#[must_use]
// Twenty-one defects, one per line of a table. Splitting it into helpers would put each
// defect's parameters in a different place from every other defect's, which is the one thing a
// fixture corpus must not do - the whole value of it is being able to read the whole corpus at
// once and see what is missing.
#[allow(clippy::too_many_lines)]
pub fn defects() -> Vec<Defect> {
    let mut out = Vec::new();

    // --- Consistency -----------------------------------------------------------------------
    let mut frame = healthy(SceneId::Ceremony);
    if let Some(node) = frame.node.as_mut() {
        // Three of its group's own tolerances warm.
        node.frame_cct_k = 5200.0 + 3.0 * node.cct_tol;
    }
    out.push(Defect {
        name: "a ceremony frame three tolerances warm of its lighting group",
        frame,
        category: QcCategory::Consistency,
        code: QcCode::ConsistencyDrift,
    });

    let mut frame = healthy(SceneId::Ceremony);
    if let Some(node) = frame.node.as_mut() {
        node.frame_signature = Some([0.95; 8]);
    }
    out.push(Defect {
        name: "a frame graded unlike its own reference frames",
        frame,
        category: QcCategory::Consistency,
        code: QcCode::SignatureDrift,
    });

    // --- Skin ------------------------------------------------------------------------------
    let mut frame = healthy(SceneId::FamilyPortrait);
    if let Some(skin) = frame.skin.as_mut() {
        skin.per_identity_de00 = vec![(IdentityId::new(), 6.0)];
    }
    out.push(Defect {
        name: "somebody's skin six dE00 from their own gallery target",
        frame,
        category: QcCategory::Skin,
        code: QcCode::SkinDrift,
    });

    let mut frame = healthy(SceneId::FamilyPortrait);
    if let Some(skin) = frame.skin.as_mut() {
        skin.guard_hue_shift_deg = 18.0;
    }
    out.push(Defect {
        name: "a grade that moved skin's hue eighteen degrees",
        frame,
        category: QcCategory::Skin,
        code: QcCode::SkinGuardExceeded,
    });

    // --- Exposure --------------------------------------------------------------------------
    let mut frame = healthy(SceneId::Ceremony);
    if let Some(exposure) = frame.exposure.as_mut() {
        exposure.subject_luma = Some(0.45 + 0.35);
    }
    out.push(Defect {
        name: "a ceremony frame nearly two stops above its scene band",
        frame,
        category: QcCategory::Exposure,
        code: QcCode::ExposureRegression,
    });

    let mut frame = healthy(SceneId::Ceremony);
    if let Some(exposure) = frame.exposure.as_mut() {
        exposure.clip_hi_before = 0.02;
        exposure.clip_hi_after = 0.40;
    }
    out.push(Defect {
        name: "an edit that blew out a third of the frame the original held",
        frame,
        category: QcCategory::Exposure,
        code: QcCode::ClippingIntroduced,
    });

    let mut frame = healthy(SceneId::Ceremony);
    if let Some(exposure) = frame.exposure.as_mut() {
        exposure.shadow_headroom = Some(0.30);
    }
    out.push(Defect {
        name: "an edit that crushed the shadows the original held",
        frame,
        category: QcCategory::Exposure,
        code: QcCode::ShadowsCrushed,
    });

    // --- Sharpness -------------------------------------------------------------------------
    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.relative_sharpness = 0.02;
        sharp.subject_sharpness = 0.02;
    }
    out.push(Defect {
        name: "a portrait whose subject is softer than its own background",
        frame,
        category: QcCategory::Sharpness,
        code: QcCode::SharpnessBelowFloor,
    });

    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.ringing = 0.55;
    }
    out.push(Defect {
        name: "sharpening that left a bright halo along an edge",
        frame,
        category: QcCategory::Sharpness,
        code: QcCode::RingingDetected,
    });

    let mut frame = healthy(SceneId::DanceFloor);
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.texture_retention = 0.35;
    }
    out.push(Defect {
        name: "denoising that took two thirds of the frame's texture",
        frame,
        category: QcCategory::Sharpness,
        code: QcCode::TextureLost,
    });

    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.identity_drift = 0.40;
    }
    out.push(Defect {
        name: "a recovered face that moved away from the person",
        frame,
        category: QcCategory::Sharpness,
        code: QcCode::IdentityDrift,
    });

    // --- Retouch ---------------------------------------------------------------------------
    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(retouch) = frame.retouch.as_mut() {
        retouch.texture_band_ratio = 0.20;
        retouch.texture_floor = 0.80;
    }
    out.push(Defect {
        name: "skin retouched below its own texture floor",
        frame,
        category: QcCategory::Retouch,
        code: QcCode::TextureFloorMissed,
    });

    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(retouch) = frame.retouch.as_mut() {
        retouch.teeth_excursion = 0.70;
    }
    out.push(Defect {
        name: "teeth evened past what a photograph can carry",
        frame,
        category: QcCategory::Retouch,
        code: QcCode::NaturalnessMissed,
    });

    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(retouch) = frame.retouch.as_mut() {
        retouch.allowance_used = 1.60;
    }
    out.push(Defect {
        name: "a frame that spent more than its per-image local allowance",
        frame,
        category: QcCategory::Retouch,
        code: QcCode::AllowanceExceeded,
    });

    // --- Mask ------------------------------------------------------------------------------
    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(mask) = frame.mask.as_mut() {
        mask.regions = vec![MaskRegion {
            kind: MaskKind::Face,
            confidence: 0.90,
            edge_quality: 0.06,
            applied_strength: 1.0,
        }];
    }
    out.push(Defect {
        name: "a full-strength edit through a boundary nobody could determine",
        frame,
        category: QcCategory::Mask,
        code: QcCode::MaskEdgeArtefact,
    });

    let mut frame = healthy(SceneId::CouplePortrait);
    if let Some(mask) = frame.mask.as_mut() {
        mask.regions = vec![MaskRegion {
            kind: MaskKind::Face,
            confidence: 0.0,
            edge_quality: 0.0,
            applied_strength: 0.80,
        }];
    }
    out.push(Defect {
        name: "a local adjustment that ran with no region behind it",
        frame,
        category: QcCategory::Mask,
        code: QcCode::MaskUncovered,
    });

    // --- Crop ------------------------------------------------------------------------------
    let mut frame = healthy(SceneId::FamilyPortrait);
    if let Some(crop) = frame.crop.as_mut() {
        crop.faces_intact = false;
    }
    out.push(Defect {
        name: "a delivered crop that cuts a face",
        frame,
        category: QcCategory::Crop,
        code: QcCode::CropUnsafe,
    });

    let mut frame = healthy(SceneId::Details);
    if let Some(crop) = frame.crop.as_mut() {
        crop.long_edge_floor = 0.80;
        crop.long_edge_fraction = 0.25;
    }
    out.push(Defect {
        name: "a crop far below its purpose's resolution floor",
        frame,
        category: QcCategory::Crop,
        code: QcCode::CropResolutionLow,
    });

    // --- Cleanup ---------------------------------------------------------------------------
    let mut frame = healthy(SceneId::ReceptionEntrance);
    if let Some(cleanup) = frame.cleanup.as_mut() {
        cleanup.removals = vec![(0.75, true)];
    }
    out.push(Defect {
        name: "a generative removal that left a visible mark",
        frame,
        category: QcCategory::Cleanup,
        code: QcCode::CleanupArtefact,
    });

    let mut frame = healthy(SceneId::ReceptionEntrance);
    if let Some(cleanup) = frame.cleanup.as_mut() {
        // The photograph looks perfect. The record is what failed.
        cleanup.removals = vec![(0.0, false)];
    }
    out.push(Defect {
        name: "a removal that reached the gallery with no disclosure row",
        frame,
        category: QcCategory::Cleanup,
        code: QcCode::CleanupUndisclosed,
    });

    // --- Duplicate -------------------------------------------------------------------------
    let mut frame = healthy(SceneId::Candid);
    if let Some(duplicate) = frame.duplicate.as_mut() {
        duplicate.hamming = 1;
        duplicate.same_moment = false;
    }
    out.push(Defect {
        name: "two near-identical frames both delivered",
        frame,
        category: QcCategory::Duplicate,
        code: QcCode::DuplicateLeak,
    });

    out
}

/// Frames with several interacting problems, for the triage and planner gates.
///
/// Kept apart from [`defects`] deliberately. A multi-symptom frame cannot answer "did the check
/// find the defect", because there are several - so mixing them into the detection corpus would
/// make that gate's denominator ambiguous. What these are for is section 6.2's triage: does the loop
/// work the root cause first, and does the planner get asked?
#[must_use]
pub fn multi_symptom() -> Vec<(&'static str, Frame)> {
    let mut out = Vec::new();

    // Colour, skin and retouch all wrong. The colour is the cause: a frame whose white balance is
    // off has skin that reads magenta and retouching that looks heavy.
    let mut frame = healthy(SceneId::Ceremony);
    if let Some(node) = frame.node.as_mut() {
        node.frame_cct_k = 5200.0 + 4.0 * node.cct_tol;
    }
    if let Some(skin) = frame.skin.as_mut() {
        skin.per_identity_de00 = vec![(IdentityId::new(), 7.0)];
    }
    if let Some(retouch) = frame.retouch.as_mut() {
        retouch.texture_band_ratio = 0.30;
        retouch.texture_floor = 0.75;
    }
    out.push(("a frame whose white balance caused everything else", frame));

    // Soft *and* ringing: sharpening more fixes the first and worsens the second. Section 6.2's
    // contradictory case, and there is no amount that satisfies both.
    let mut frame = healthy(SceneId::DanceFloor);
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.relative_sharpness = 0.02;
        sharp.subject_sharpness = 0.02;
        sharp.ringing = 0.60;
    }
    out.push(("a frame that is both soft and already ringing", frame));

    // A frame with more problems than any single remedy can address, which is the `MultiSymptom`
    // escalation and section 7's planner trigger.
    let mut frame = healthy(SceneId::DanceFloor);
    if let Some(node) = frame.node.as_mut() {
        node.frame_cct_k = 5200.0 + 4.0 * node.cct_tol;
        node.frame_signature = Some([0.95; 8]);
    }
    if let Some(skin) = frame.skin.as_mut() {
        skin.per_identity_de00 = vec![(IdentityId::new(), 7.0)];
        skin.guard_hue_shift_deg = 20.0;
    }
    if let Some(exposure) = frame.exposure.as_mut() {
        exposure.subject_luma = Some(0.90);
        exposure.clip_hi_before = 0.0;
        exposure.clip_hi_after = 0.5;
        exposure.shadow_headroom = Some(0.10);
    }
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.relative_sharpness = 0.02;
        sharp.ringing = 0.6;
        sharp.texture_retention = 0.2;
    }
    out.push((
        "a frame with more wrong with it than one remedy can fix",
        frame,
    ));

    out
}

/// A gallery whose coverage is intact.
#[must_use]
pub fn healthy_coverage() -> SetContext {
    SetContext {
        coverage_available: true,
        ..SetContext::default()
    }
}

/// A gallery missing a must-have and short of one person.
#[must_use]
pub fn broken_coverage() -> SetContext {
    SetContext {
        missing_rules: vec!["rings".into()],
        weak_rules: vec!["cake".into()],
        under_covered: vec![(IdentityId::new(), 1, 5)],
        coverage_available: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks;
    use crate::policy::Thresholds;

    #[test]
    fn a_healthy_frame_has_every_reading_and_no_findings() {
        let thresholds = Thresholds::reference();
        for frame in clean_gallery(16) {
            assert_eq!(frame.readings_present(), 8, "every reading is present");
            let findings = checks::findings_for(&frame, &thresholds);
            assert!(
                findings.is_empty(),
                "the clean baseline must produce no tickets, got {:?}",
                findings.iter().map(|f| f.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_healthy_frame_produces_no_findings_under_the_shipped_thresholds_either() {
        // The shipped table is tighter than the reference row in nineteen places, and a baseline
        // that only stayed quiet under the permissive one would make the false-ticket gate a
        // measurement of the reference row rather than of the product.
        let thresholds = Thresholds::shipped().expect("the shipped table loads");
        for frame in clean_gallery(16) {
            let findings = checks::findings_for(&frame, &thresholds);
            assert!(
                findings.is_empty(),
                "{:?} fired on a clean frame in {}",
                findings.iter().map(|f| f.code).collect::<Vec<_>>(),
                frame.scene
            );
        }
    }

    #[test]
    fn every_defect_is_caught_by_the_check_that_owns_it() {
        let thresholds = Thresholds::reference();
        for defect in defects() {
            let findings = checks::findings_for(&defect.frame, &thresholds);
            assert!(
                findings.iter().any(|finding| finding.code == defect.code),
                "'{}' was not caught; got {:?}",
                defect.name,
                findings.iter().map(|f| f.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn every_defect_is_caught_under_the_shipped_thresholds_too() {
        let thresholds = Thresholds::shipped().expect("the shipped table loads");
        for defect in defects() {
            let findings = checks::findings_for(&defect.frame, &thresholds);
            assert!(
                findings.iter().any(|finding| finding.code == defect.code),
                "'{}' was not caught under the shipped row for {}",
                defect.name,
                defect.frame.scene
            );
        }
    }

    #[test]
    fn every_defect_is_exactly_one_thing() {
        // A fixture with two problems cannot tell a detection failure from a triage failure. Each
        // frame here is one edit away from `healthy`, so this holds by construction - and this test
        // is what keeps it true when somebody adds the twenty-first.
        let thresholds = Thresholds::reference();
        for defect in defects() {
            let findings = checks::findings_for(&defect.frame, &thresholds);
            let categories: std::collections::BTreeSet<_> =
                findings.iter().map(|finding| finding.category).collect();
            assert_eq!(
                categories.len(),
                1,
                "'{}' produced findings in {:?}",
                defect.name,
                categories
            );
            assert_eq!(categories.into_iter().next(), Some(defect.category));
        }
    }

    #[test]
    fn every_category_is_represented_except_coverage() {
        // Coverage is a fact about the set and is exercised by `broken_coverage` instead.
        let covered: std::collections::BTreeSet<_> = defects()
            .into_iter()
            .map(|defect| defect.category)
            .collect();
        for category in QcCategory::ALL {
            if category == QcCategory::Coverage {
                continue;
            }
            assert!(
                covered.contains(&category),
                "{category} has no injected defect"
            );
        }
    }

    #[test]
    fn a_multi_symptom_frame_produces_several_findings() {
        let thresholds = Thresholds::reference();
        for (name, frame) in multi_symptom() {
            let findings = checks::findings_for(&frame, &thresholds);
            assert!(
                findings.len() >= 2 || findings.iter().any(|f| f.code == QcCode::MultiSymptom),
                "'{name}' produced {} findings",
                findings.len()
            );
        }
    }

    #[test]
    fn a_frame_with_everything_wrong_escalates_whole_rather_than_filing_nine_tickets() {
        let thresholds = Thresholds::reference();
        let worst = multi_symptom()
            .into_iter()
            .last()
            .expect("the third fixture")
            .1;
        let findings = checks::findings_for(&worst, &thresholds);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::MultiSymptom);
    }

    #[test]
    fn broken_coverage_reports_all_three_kinds() {
        let findings = crate::checks::coverage::inspect(&broken_coverage()).findings();
        let codes: std::collections::BTreeSet<_> =
            findings.iter().map(|finding| finding.code).collect();
        assert!(codes.contains(&QcCode::CoverageMissing));
        assert!(codes.contains(&QcCode::CoverageWeak));
        assert!(codes.contains(&QcCode::IdentityUnderCovered));
        assert_eq!(
            crate::checks::coverage::inspect(&healthy_coverage()),
            crate::checks::Outcome::Clean
        );
    }
}
