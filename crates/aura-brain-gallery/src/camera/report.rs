//! What was corrected, on what evidence, in a photographer's own words.
//!
//! Section 2.1 asks for the transforms to be "visible in a per-camera report" and section 13 makes
//! "the per-camera report explains what was corrected and on what evidence" an acceptance
//! criterion. This module is that criterion as a function, so the panel, the CLI gate and the exit
//! report all render the same sentences from the same rows.
//!
//! ## Evidence first, correction second
//!
//! Every report leads with **where the numbers came from** and only then says what they did. That
//! ordering is not presentation: a body corrected by 300 K from twenty verified pairs of its own
//! ceremony and a body corrected by 300 K from a fabricated brand baseline are the same arithmetic
//! and completely different claims, and a photographer deciding whether to accept the correction is
//! deciding about the evidence rather than about the number.
//!
//! It is also why [`CameraCode::withdraws`] outranks every action code in
//! [`CameraCode::default_weight`], and why [`Report::headline`] reads the reason set rather than
//! the transform's magnitude.
//!
//! ## Nothing here is stored
//!
//! Phase 09's rule, inherited for the nineteenth time: a stored sentence is copy a release can
//! change, and a catalog full of English cannot be translated. Every string in this module is
//! rendered from a code and a number at read time.

use std::fmt::Write as _;

use aura_core::contract::camera::{
    CameraCode, CameraFingerprint, CameraOutline, CameraTransform, FlashState, Reference,
    ShooterBias, TransformSource, CROSS_CAMERA_DE00_CEILING,
};
use aura_core::contract::moment::CameraId;

use super::shooter;

/// One body's report, for one flash state.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The body.
    pub camera_id: CameraId,
    /// Which of its two colour behaviours.
    pub flash: FlashState,
    /// The shooter label the catalog carries for it.
    pub shooter: Option<String>,
    /// True when this body is the one everything else is matched to.
    pub is_reference: bool,
    /// One line saying where the correction came from.
    pub evidence: String,
    /// One line per thing that was corrected, in the order a photographer would ask about them.
    pub corrections: Vec<String>,
    /// One line per thing that was deliberately not done.
    pub withdrawals: Vec<String>,
    /// The cross-camera skin difference left after matching, in dE00.
    pub skin_de00_after: f32,
    /// True when this body meets the phase's headline promise.
    pub meets_promise: bool,
    /// How far the body was moved, `0..1`, as the worst of its axes.
    pub magnitude: f32,
    /// How much this correction is worth, `0..1`.
    pub confidence: f32,
}

impl Report {
    /// The one sentence a panel shows collapsed.
    ///
    /// Reads the **reason set** rather than the magnitude, so a body corrected from a fabricated
    /// baseline never leads with how far it moved.
    #[must_use]
    pub fn headline(&self) -> &str {
        if self.is_reference {
            return "the camera everything else is matched to";
        }
        self.withdrawals
            .first()
            .map_or(self.evidence.as_str(), String::as_str)
    }
}

/// Build one body's report from its stored rows.
#[must_use]
pub fn of_camera(
    transform: &CameraTransform,
    fingerprint: Option<&CameraFingerprint>,
    shooter_rows: &[ShooterBias],
    reference: Option<&Reference>,
    shooter_label: Option<&str>,
) -> Report {
    let is_reference = reference.is_some_and(|r| r.camera_id == transform.camera_id);

    let evidence = if is_reference {
        "This is the camera everything else at this wedding is matched to, so nothing about it \
         was changed."
            .to_string()
    } else {
        evidence_line(transform, fingerprint)
    };

    let mut corrections = Vec::new();
    if !is_reference && transform.enabled {
        push_colour_lines(transform, &mut corrections);
        push_shooter_lines(transform, shooter_rows, &mut corrections);
        if corrections.is_empty() {
            corrections.push(
                "Nothing needed changing: this camera already agreed with the reference."
                    .to_string(),
            );
        }
    }

    let mut withdrawals = Vec::new();
    for reason in &transform.reasons {
        if reason.code.withdraws() {
            let mut line = String::new();
            let _ = write!(&mut line, "{}", sentence_case(reason.code.user_text()));
            if reason.code == CameraCode::PairsInsufficient {
                let _ = write!(&mut line, " ({} verified pairs)", transform.evidence_pairs);
            }
            withdrawals.push(line);
        }
    }
    if !transform.enabled {
        withdrawals.insert(
            0,
            "Matching is switched off for this camera, so its photographs were left exactly as \
             they were."
                .to_string(),
        );
    }
    if transform.user_edited {
        withdrawals.insert(
            0,
            "You set this camera's correction yourself, and AURA has not changed it.".to_string(),
        );
    }

    Report {
        camera_id: transform.camera_id.clone(),
        flash: transform.flash,
        shooter: shooter_label.map(str::to_string),
        is_reference,
        evidence,
        corrections,
        withdrawals,
        skin_de00_after: transform.skin_correction.de00_after,
        meets_promise: transform.meets_skin_promise(),
        magnitude: transform.magnitude(),
        confidence: transform.confidence,
    }
}

/// Where a correction came from, in one sentence with the numbers in it.
fn evidence_line(transform: &CameraTransform, fingerprint: Option<&CameraFingerprint>) -> String {
    let state = match transform.flash {
        FlashState::Ambient => "available-light",
        FlashState::Flash => "flash",
    };
    let samples = fingerprint.map_or(0, |print| print.samples);
    match transform.source {
        TransformSource::MatchedPairs => format!(
            "Worked out from {} {state} photographs where both cameras were shooting the same \
             thing under the same light, measured against {samples} photographs from this camera. \
             {}",
            transform.evidence_pairs,
            heldout_clause(transform)
        ),
        TransformSource::Blended => format!(
            "Part measured here and part general: {} {state} photographs where both cameras \
             overlapped is fewer than AURA wants, so {:.0}% of the correction comes from this \
             wedding and the rest from what it knows about the brand. {}",
            transform.evidence_pairs,
            transform.blend * 100.0,
            heldout_clause(transform)
        ),
        TransformSource::BrandBaseline => {
            let why = if transform
                .reasons
                .iter()
                .any(|r| r.code == CameraCode::BaselineUnknownBrand)
            {
                "AURA has no measurements for this camera's manufacturer, so it has changed \
                 nothing rather than guess."
            } else if transform
                .reasons
                .iter()
                .any(|r| r.code == CameraCode::HeldOutFailed)
            {
                "The correction AURA worked out here did not hold up when it was checked against \
                 photographs it had not used, so what is applied is what AURA knows about the \
                 brand."
            } else {
                "These two cameras never photographed the same thing under the same light at this \
                 wedding, so what is applied is what AURA knows about the brand."
            };
            format!("No {state} evidence from this wedding. {why}")
        }
    }
}

/// What the held-out check said, or that it could not run.
fn heldout_clause(transform: &CameraTransform) -> String {
    match transform.heldout_improved() {
        Some(true) => format!(
            "Checked against {} photographs it had not used, and they matched better afterwards.",
            transform.heldout_pairs
        ),
        Some(false) => format!(
            "Checked against {} photographs it had not used, and they did not improve.",
            transform.heldout_pairs
        ),
        None => "There were too few spare photographs to check the correction against.".to_string(),
    }
}

/// One line per colour axis that actually moved.
fn push_colour_lines(transform: &CameraTransform, out: &mut Vec<String>) {
    if transform.d_cct.abs() >= 15.0 {
        let direction = if transform.d_cct > 0.0 {
            "warmer"
        } else {
            "cooler"
        };
        out.push(format!(
            "Made {direction} by {:.0} K, so whites and greys from this camera agree with the \
             reference camera's.",
            transform.d_cct.abs()
        ));
    }
    if transform.d_tint.abs() >= 0.5 {
        let direction = if transform.d_tint > 0.0 {
            "magenta"
        } else {
            "green"
        };
        out.push(format!(
            "Moved {:.1} toward {direction}, which is where the two cameras' colour differs most \
             under artificial light.",
            transform.d_tint.abs()
        ));
    }
    if transform.d_saturation.abs() >= 0.5 {
        let direction = if transform.d_saturation > 0.0 {
            "richer"
        } else {
            "less saturated"
        };
        out.push(format!(
            "Made {direction} by {:.1}, to match the reference camera's colour.",
            transform.d_saturation.abs()
        ));
    }
    let shape = transform
        .contrast_shape
        .iter()
        .map(|c| (c - 1.0).abs())
        .fold(0.0_f32, f32::max);
    if shape >= 0.01 {
        out.push(format!(
            "Contrast reshaped by up to {:.0}%, mostly in how the highlights fall away - the \
             difference between two manufacturers that is hardest to see one frame at a time.",
            shape * 100.0
        ));
    }
    let gain = transform
        .channel_gain
        .iter()
        .map(|g| (g - 1.0).abs())
        .fold(0.0_f32, f32::max);
    if gain >= 0.005 {
        out.push(format!(
            "Individual colour channels adjusted by up to {:.1}%, which corrects what a white \
             balance cannot: two cameras can agree about a grey card and still disagree about a \
             deep red.",
            gain * 100.0
        ));
    }
    let skin = transform.skin_correction;
    let skin_moved = (skin.d_uv[0] * skin.d_uv[0] + skin.d_uv[1] * skin.d_uv[1]).sqrt();
    if skin_moved > 1e-5 || skin.d_luma.abs() > 1e-4 {
        let mut line = format!(
            "Skin brought into line with the reference camera's: {:.1} dE00 apart before, {:.1} \
             after.",
            skin.de00_before, skin.de00_after
        );
        if skin.capped {
            line.push_str(
                " The correction reached the furthest AURA will move skin on a whole camera and \
                 stopped there.",
            );
        }
        if !skin.locus_valid {
            line.push_str(
                " Going further would have pushed skin to a colour skin does not come in.",
            );
        }
        out.push(line);
    }
}

/// One line about the photographer, when there is one.
fn push_shooter_lines(transform: &CameraTransform, rows: &[ShooterBias], out: &mut Vec<String>) {
    let folded = shooter::folded_ev(rows, &transform.camera_id);
    if folded.abs() < 0.01 {
        return;
    }
    let direction = if folded > 0.0 { "brighter" } else { "darker" };
    let measured = rows
        .iter()
        .filter(|row| row.camera_id == transform.camera_id && row.is_usable())
        .map(|row| row.measured_ev.abs())
        .fold(0.0_f32, f32::max);
    let mut line = format!(
        "Exposure moved {:.2} of a stop {direction} to bring this photographer partly into line \
         with the main photographer.",
        folded.abs()
    );
    if measured > folded.abs() + 0.01 {
        let _ = write!(
            &mut line,
            " They work up to {measured:.2} of a stop differently, and AURA deliberately corrected \
             less than all of it so their own way of working is still visible."
        );
    }
    out.push(line);
}

/// The whole project's report, worst-evidence first.
///
/// Ordering matters here for the same reason it does inside one report: the bodies a photographer
/// needs to look at are the ones whose corrections rest on the least evidence, not the ones that
/// moved furthest.
#[must_use]
pub fn of_project(
    transforms: &[CameraTransform],
    fingerprints: &[CameraFingerprint],
    shooter_rows: &[ShooterBias],
    reference: Option<&Reference>,
    labels: &[(CameraId, String)],
) -> Vec<Report> {
    let mut out: Vec<Report> = transforms
        .iter()
        .map(|transform| {
            let fingerprint = fingerprints.iter().find(|print| {
                print.camera_id == transform.camera_id && print.flash == transform.flash
            });
            let label = labels
                .iter()
                .find(|(id, _)| id == &transform.camera_id)
                .map(|(_, label)| label.as_str());
            of_camera(transform, fingerprint, shooter_rows, reference, label)
        })
        .collect();
    out.sort_by(|a, b| {
        a.is_reference
            .cmp(&b.is_reference)
            .then_with(|| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                b.skin_de00_after
                    .partial_cmp(&a.skin_de00_after)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.camera_id.as_str().cmp(b.camera_id.as_str()))
    });
    out
}

/// The project's own headline, in one paragraph.
///
/// What `aura-cli verify --phase 26` prints and what the panel puts above the per-camera list. It
/// says the number of bodies, where their corrections came from, and whether the phase's promise
/// holds - in that order, because "we matched four cameras" means nothing without "three of them
/// from a bundled baseline".
#[must_use]
pub fn summary(outline: &CameraOutline) -> String {
    if outline.cameras == 0 {
        return "No cameras were found in this project.".to_string();
    }
    if outline.cameras == 1 {
        return "One camera shot this wedding, so there is nothing to match it to.".to_string();
    }
    let mut text = format!(
        "{} cameras. {} matched from photographs where two cameras overlapped at this wedding, \
         {} partly, {} from what AURA knows about the brand alone.",
        outline.cameras, outline.solved_from_pairs, outline.blended, outline.baseline_only
    );
    if outline.pairs > 0 {
        let _ = write!(
            &mut text,
            " {} overlapping pairs were used and {} were rejected because the surroundings said \
             the light had changed between them.",
            outline.pairs, outline.pairs_rejected
        );
    }
    if outline.skin_de00_before > 0.0 {
        let _ = write!(
            &mut text,
            " Skin between cameras was {:.1} dE00 apart and is now {:.1}.",
            outline.skin_de00_before, outline.skin_de00_after
        );
        if outline.meets_skin_promise() {
            let _ = write!(
                &mut text,
                " Every camera is inside the {CROSS_CAMERA_DE00_CEILING:.1} dE00 AURA promises."
            );
        } else {
            let _ = write!(
                &mut text,
                " The worst camera is still {:.1} dE00 away, above the \
                 {CROSS_CAMERA_DE00_CEILING:.1} AURA promises.",
                outline.worst_skin_de00
            );
        }
    } else {
        text.push_str(
            " Skin was not measured at this wedding, so no claim is made about how skin from the \
             different cameras compares.",
        );
    }
    if outline.shooters_measured > 0 {
        let _ = write!(
            &mut text,
            " {} photographers' exposure habits were measured and {} were deliberately corrected \
             by less than the whole difference.",
            outline.shooters_measured, outline.shooters_capped
        );
    }
    if !outline.unknown_brands.is_empty() {
        let _ = write!(
            &mut text,
            " AURA has no measurements for: {}.",
            outline.unknown_brands.join(", ")
        );
    }
    text
}

/// Capitalise the first letter of a rendered reason, which is written lower case.
fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => {
            let mut out: String = first.to_uppercase().collect();
            out.push_str(chars.as_str());
            if !out.ends_with('.') {
                out.push('.');
            }
            out
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use aura_core::contract::camera::{CameraReason, ReferenceSource, SkinCorrection};
    use aura_core::ProjectId;

    use super::*;

    fn transform(source: TransformSource, pairs: u32) -> CameraTransform {
        let mut t = CameraTransform::identity(
            CameraId::new("cam_b"),
            FlashState::Ambient,
            CameraId::new("cam_a"),
            1,
            1,
        );
        t.source = source;
        t.evidence_pairs = pairs;
        t.d_cct = -220.0;
        t.confidence = 0.7;
        t.skin_correction = SkinCorrection {
            d_uv: [0.004, 0.002],
            d_luma: 0.01,
            de00_before: 3.4,
            de00_after: 1.1,
            locus_valid: true,
            capped: false,
        };
        t
    }

    fn reference() -> Reference {
        Reference {
            project: ProjectId::new(),
            camera_id: CameraId::new("cam_a"),
            source: ReferenceSource::PrimaryShooter,
            frames: 2400,
            shooter: Some("primary".to_string()),
        }
    }

    #[test]
    fn a_report_leads_with_evidence_and_names_the_numbers() {
        let mut t = transform(TransformSource::MatchedPairs, 34);
        t.heldout_pairs = 11;
        t.heldout_before = aura_core::contract::camera::AppearanceDistance {
            skin_de00: 3.4,
            ..Default::default()
        };
        t.heldout_after = aura_core::contract::camera::AppearanceDistance {
            skin_de00: 1.0,
            ..Default::default()
        };
        t.reasons = vec![CameraReason::of(CameraCode::SolvedFromPairs)];
        let report = of_camera(&t, None, &[], Some(&reference()), Some("second"));
        assert!(report.evidence.contains("34"), "{}", report.evidence);
        assert!(report.evidence.contains("11"), "{}", report.evidence);
        assert!(!report.is_reference);
        assert!(report
            .corrections
            .iter()
            .any(|line| line.contains("cooler")));
        assert!(report.corrections.iter().any(|line| line.contains("dE00")));
    }

    #[test]
    fn a_baseline_only_body_leads_with_the_withdrawal_rather_than_the_number() {
        let mut t = transform(TransformSource::BrandBaseline, 0);
        t.reasons = vec![CameraReason::of(CameraCode::BaselineOnly)];
        let report = of_camera(&t, None, &[], Some(&reference()), None);
        assert!(!report.withdrawals.is_empty());
        assert_eq!(report.headline(), report.withdrawals[0]);
        assert!(
            report.evidence.contains("knows about the brand"),
            "{}",
            report.evidence
        );
    }

    #[test]
    fn the_reference_body_reports_that_nothing_was_changed() {
        let mut t = transform(TransformSource::MatchedPairs, 0);
        t.camera_id = CameraId::new("cam_a");
        let report = of_camera(&t, None, &[], Some(&reference()), Some("primary"));
        assert!(report.is_reference);
        assert!(report.corrections.is_empty());
        assert_eq!(
            report.headline(),
            "the camera everything else is matched to"
        );
    }

    #[test]
    fn a_held_out_check_that_did_not_run_is_said_out_loud() {
        let t = transform(TransformSource::MatchedPairs, 20);
        let report = of_camera(&t, None, &[], Some(&reference()), None);
        assert!(
            report.evidence.contains("too few spare photographs"),
            "{}",
            report.evidence
        );
    }

    #[test]
    fn a_project_summary_says_what_the_evidence_was_before_what_it_did() {
        let outline = CameraOutline {
            cameras: 4,
            solved_from_pairs: 1,
            blended: 0,
            baseline_only: 3,
            pairs: 18,
            pairs_rejected: 44,
            skin_de00_before: 3.2,
            skin_de00_after: 1.4,
            worst_skin_de00: 1.9,
            shooters_measured: 2,
            shooters_capped: 2,
            ..CameraOutline::default()
        };
        let text = summary(&outline);
        let brand_at = text.find("from what AURA knows").expect("evidence clause");
        let skin_at = text.find("Skin between cameras").expect("skin clause");
        assert!(
            brand_at < skin_at,
            "evidence must come before the result: {text}"
        );
        assert!(text.contains("Every camera is inside"));
    }

    #[test]
    fn an_unmeasured_skin_term_is_stated_rather_than_reported_as_a_pass() {
        // This build, on a real photograph: `SKIN_FIELD_AVAILABLE` is false, so the skin term is
        // zero. Zero must never render as "the promise holds".
        let outline = CameraOutline {
            cameras: 2,
            solved_from_pairs: 1,
            skin_de00_before: 0.0,
            skin_de00_after: 0.0,
            ..CameraOutline::default()
        };
        let text = summary(&outline);
        assert!(text.contains("Skin was not measured"), "{text}");
        assert!(!text.contains("promises"));
    }

    #[test]
    fn one_camera_is_a_sentence_and_not_a_report() {
        let outline = CameraOutline {
            cameras: 1,
            ..CameraOutline::default()
        };
        assert!(summary(&outline).contains("nothing to match"));
    }
}
