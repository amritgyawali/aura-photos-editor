//! Which of the sixteen bits are true, and what sentence explains each.
//!
//! PHASE-11 section 8's seventh step, second half: "emit reasons with evidence boxes".
//! Section 13's second acceptance criterion - "the overlay shows exactly why a frame was
//! marked badly composed" - is what this module has to satisfy, and it is a higher bar
//! than raising a bit: every penalty here carries a sentence in the product's voice and,
//! wherever there is somewhere to point, a rectangle.
//!
//! ## The three bits that are not defects, and the twelve codes that are not penalties
//!
//! This is the module where section 12's first failure mode is either prevented or
//! shipped. A build that only ever explains what is *wrong* with a frame is a build a
//! photographer stops believing on the first dutch angle it calls a mistake - so the
//! exonerations are emitted with the same care as the penalties, and three of them
//! (`IntentionalTilt`, `DeliberateCrop`, `Centred`) exist purely to say "this looks like a
//! violation and is not one".
//!
//! ## At most six reasons, and the ranking is part of the explanation
//!
//! `CompositionResult::MAX_REASONS` is six, matching phases 09 and 10. A panel listing
//! nine reasons is a panel nobody reads. The order is by absolute weight, penalties first,
//! so the sentence at the top is the one that moved the score most - and a frame with no
//! penalties at all still carries `Clean`, because invariant 2 requires a decision to have
//! an explanation and "nothing was wrong" is one.

use aura_core::contract::composition::{
    Box2, CompositionCode, CompositionFlags, CompositionReason, CompositionResult,
};

use crate::composition::aesthetic::Aesthetic;
use crate::composition::background::Background;
use crate::composition::crop_audit::CropAudit;
use crate::composition::horizon::HorizonResult;
use crate::composition::placement::{HeadroomVerdict, Placement};
use crate::composition::rules::SceneRule;

/// Everything the flag decision reads.
#[derive(Debug, Clone, Copy)]
pub struct Evidence<'a> {
    /// The line structure.
    pub horizon: &'a HorizonResult,
    /// What the frame edge cut.
    pub audit: &'a CropAudit,
    /// Where everything is.
    pub placement: &'a Placement,
    /// What is behind them.
    pub background: &'a Background,
    /// What taste said.
    pub aesthetic: Aesthetic,
    /// The scene's bands.
    pub rule: &'a SceneRule,
    /// True when no face and no pose was found.
    pub subjectless: bool,
    /// True when faces existed but the pose head was unavailable or incomplete.
    pub keypoints_degraded: bool,
}

/// The flags and the sentences.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Verdict {
    /// Everything true of the framing.
    pub flags: CompositionFlags,
    /// Why, at most six, strongest penalty first.
    pub reasons: Vec<CompositionReason>,
}

/// Decide every bit and write every sentence.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn decide(evidence: &Evidence<'_>) -> Verdict {
    let mut flags = CompositionFlags::EMPTY;
    let mut reasons: Vec<CompositionReason> = Vec::new();

    // --- The horizon -------------------------------------------------------
    let horizon = evidence.horizon;
    if horizon.intentional {
        flags = flags.union(CompositionFlags::INTENTIONAL_TILT);
        reasons.push(CompositionReason::frame(
            CompositionCode::IntentionalTilt,
            format!(
                "the frame is tilted by {:.0} degrees with the subject centred and no clear \
                 horizon, which reads as a choice rather than a mistake",
                horizon.tilt_deg.abs()
            ),
            0.0,
        ));
    } else if !horizon.is_measured() {
        reasons.push(CompositionReason::frame(
            CompositionCode::HorizonUncertain,
            CompositionCode::HorizonUncertain.user_text(),
            0.0,
        ));
    } else if horizon.tilt_deg.abs() > evidence.rule.tilt_tolerance_deg {
        flags = flags.union(CompositionFlags::HORIZON_TILTED);
        let excess = horizon.tilt_deg.abs() - evidence.rule.tilt_tolerance_deg;
        reasons.push(CompositionReason::frame(
            CompositionCode::HorizonTilted,
            format!(
                "the horizon is {:.1} degrees off level, which is more than this kind of \
                 photograph usually carries",
                horizon.tilt_deg.abs()
            ),
            -(excess / 10.0).clamp(0.02, 0.35),
        ));
    }
    if horizon.convergence > 0.55 {
        flags = flags.union(CompositionFlags::VERTICALS_CONVERGING);
        reasons.push(CompositionReason::frame(
            CompositionCode::VerticalsConverging,
            CompositionCode::VerticalsConverging.user_text(),
            -(horizon.convergence - 0.55).clamp(0.02, 0.20),
        ));
    }

    // --- What the edges cut ------------------------------------------------
    let audit = evidence.audit;
    for cut in audit.flagged() {
        let code = if cut.at_joint {
            CompositionCode::JointCut
        } else {
            CompositionCode::LimbCut
        };
        flags = flags.union(if cut.at_joint {
            CompositionFlags::JOINT_CUT
        } else {
            CompositionFlags::LIMB_CUT
        });
        let text = if cut.at_joint {
            format!(
                "the {} edge of the frame cuts through {}, which reads as a severed limb",
                cut.edge,
                cut.joint.title()
            )
        } else {
            format!(
                "the {} edge of the frame cuts a limb between joints, near {}",
                cut.edge,
                cut.joint.title()
            )
        };
        reasons.push(CompositionReason::at(
            code,
            text,
            -cut.severity * 0.4,
            cut.area,
        ));
    }
    if audit.head_crop {
        flags = flags.union(CompositionFlags::HEAD_CROPPED);
        reasons.push(CompositionReason::frame(
            CompositionCode::HeadCropped,
            CompositionCode::HeadCropped.user_text(),
            -0.18,
        ));
    } else if audit.head_crop_allowed && audit.head_crop_depth > 0.0 {
        reasons.push(CompositionReason::frame(
            CompositionCode::DeliberateCrop,
            CompositionCode::DeliberateCrop.user_text(),
            0.0,
        ));
    }
    for intrusion in audit.intrusions.iter().take(2) {
        flags = flags.union(CompositionFlags::EDGE_INTRUSION);
        reasons.push(CompositionReason::at(
            CompositionCode::EdgeIntrusion,
            CompositionCode::EdgeIntrusion.user_text(),
            -0.08,
            *intrusion,
        ));
    }

    // --- Placement ---------------------------------------------------------
    let placement = evidence.placement;
    match placement.headroom_verdict {
        HeadroomVerdict::Tight if !audit.head_crop && audit.head_crop_depth <= 0.0 => {
            flags = flags.union(CompositionFlags::HEADROOM_TIGHT);
            reasons.push(CompositionReason::frame(
                CompositionCode::HeadroomTight,
                CompositionCode::HeadroomTight.user_text(),
                -evidence.rule.headroom_penalty,
            ));
        }
        HeadroomVerdict::Excessive => {
            flags = flags.union(CompositionFlags::HEADROOM_EXCESSIVE);
            reasons.push(CompositionReason::frame(
                CompositionCode::HeadroomExcessive,
                CompositionCode::HeadroomExcessive.user_text(),
                -evidence.rule.headroom_penalty,
            ));
        }
        _ => {}
    }
    if placement.centred {
        flags = flags.union(CompositionFlags::CENTRED);
        reasons.push(CompositionReason::frame(
            CompositionCode::Centred,
            CompositionCode::Centred.user_text(),
            0.0,
        ));
    } else if placement.on_power_point {
        reasons.push(CompositionReason::frame(
            CompositionCode::ThirdsAligned,
            CompositionCode::ThirdsAligned.user_text(),
            0.0,
        ));
    } else if placement.off_placement && evidence.rule.thirds_weight > 0.0 {
        reasons.push(CompositionReason::frame(
            CompositionCode::OffThirds,
            CompositionCode::OffThirds.user_text(),
            -evidence.rule.thirds_weight * 0.10,
        ));
    }
    if evidence.rule.balance_weight > 0.0 {
        if placement.balance < 0.45 {
            flags = flags.union(CompositionFlags::OFF_BALANCE);
            reasons.push(CompositionReason::frame(
                CompositionCode::OffBalance,
                CompositionCode::OffBalance.user_text(),
                -(0.45 - placement.balance) * evidence.rule.balance_weight,
            ));
        } else if placement.balance > 0.80 {
            reasons.push(CompositionReason::frame(
                CompositionCode::Balanced,
                CompositionCode::Balanced.user_text(),
                0.0,
            ));
        }
    }

    // --- The background ----------------------------------------------------
    let background = evidence.background;
    if background.subject_aware {
        if background.clutter > 1.0 {
            flags = flags.union(CompositionFlags::CLUTTERED_BACKGROUND);
            reasons.push(CompositionReason::frame(
                CompositionCode::ClutteredBackground,
                CompositionCode::ClutteredBackground.user_text(),
                -((background.clutter - 1.0).clamp(0.02, 0.30)),
            ));
        } else if background.clutter < 0.55 {
            reasons.push(CompositionReason::frame(
                CompositionCode::CleanBackground,
                CompositionCode::CleanBackground.user_text(),
                0.0,
            ));
        }
        if let Some(blob) = background.bright_blobs.first() {
            flags = flags.union(CompositionFlags::BRIGHT_BLOB);
            reasons.push(CompositionReason::at(
                CompositionCode::BrightBlob,
                CompositionCode::BrightBlob.user_text(),
                -0.12,
                *blob,
            ));
        }
        if background.head_merge {
            flags = flags.union(CompositionFlags::HEAD_MERGE);
            reasons.push(match background.merge_at {
                Some(area) => CompositionReason::at(
                    CompositionCode::HeadMerge,
                    CompositionCode::HeadMerge.user_text(),
                    -0.22,
                    area,
                ),
                None => CompositionReason::frame(
                    CompositionCode::HeadMerge,
                    CompositionCode::HeadMerge.user_text(),
                    -0.22,
                ),
            });
        }
        if background.colour_competition > 0.35 {
            flags = flags.union(CompositionFlags::COLOUR_COMPETITION);
            let weight = -background.colour_competition * 0.20;
            reasons.push(match background.competing_at {
                Some(area) => CompositionReason::at(
                    CompositionCode::ColourCompetition,
                    CompositionCode::ColourCompetition.user_text(),
                    weight,
                    area,
                ),
                None => CompositionReason::frame(
                    CompositionCode::ColourCompetition,
                    CompositionCode::ColourCompetition.user_text(),
                    weight,
                ),
            });
        }
    }

    // --- The four withdrawals ----------------------------------------------
    if evidence.subjectless {
        flags = flags.union(CompositionFlags::NO_GEOMETRY);
        reasons.push(CompositionReason::frame(
            CompositionCode::NoGeometry,
            CompositionCode::NoGeometry.user_text(),
            0.0,
        ));
    }
    if evidence.keypoints_degraded {
        reasons.push(CompositionReason::frame(
            CompositionCode::KeypointsUnavailable,
            CompositionCode::KeypointsUnavailable.user_text(),
            0.0,
        ));
    }
    if !evidence.rule.measured {
        reasons.push(CompositionReason::frame(
            CompositionCode::NoRule,
            CompositionCode::NoRule.user_text(),
            0.0,
        ));
    }
    if !evidence.aesthetic.learned {
        reasons.push(CompositionReason::frame(
            CompositionCode::AestheticUnavailable,
            CompositionCode::AestheticUnavailable.user_text(),
            0.0,
        ));
    }

    let reasons = rank(reasons);
    // A local overlay flag without a retained, local reason cannot satisfy the product
    // contract. MAX_REASONS is frozen at six, so in an unusually crowded verdict the
    // lowest-priority local marks are withheld rather than pointing at unexplained pixels.
    for flag in LOCALIZABLE_FLAGS {
        if flags.contains(flag)
            && !reasons
                .iter()
                .any(|reason| reason.evidence.is_some() && flag_for_code(reason.code) == Some(flag))
        {
            flags = flags.without(flag);
        }
    }

    Verdict { flags, reasons }
}

/// Flags whose UI representation points at a concrete region of the photograph.
pub const LOCALIZABLE_FLAGS: [CompositionFlags; 6] = [
    CompositionFlags::JOINT_CUT,
    CompositionFlags::LIMB_CUT,
    CompositionFlags::EDGE_INTRUSION,
    CompositionFlags::BRIGHT_BLOB,
    CompositionFlags::HEAD_MERGE,
    CompositionFlags::COLOUR_COMPETITION,
];

/// The flag represented by a reason code, when that code corresponds to a bit.
///
/// Placement's `OffThirds` has no frozen bit and exonerations intentionally return none.
#[must_use]
pub const fn flag_for_code(code: CompositionCode) -> Option<CompositionFlags> {
    match code {
        CompositionCode::HorizonTilted => Some(CompositionFlags::HORIZON_TILTED),
        CompositionCode::IntentionalTilt => Some(CompositionFlags::INTENTIONAL_TILT),
        CompositionCode::VerticalsConverging => Some(CompositionFlags::VERTICALS_CONVERGING),
        CompositionCode::HeadroomTight => Some(CompositionFlags::HEADROOM_TIGHT),
        CompositionCode::HeadroomExcessive => Some(CompositionFlags::HEADROOM_EXCESSIVE),
        CompositionCode::HeadCropped => Some(CompositionFlags::HEAD_CROPPED),
        CompositionCode::JointCut => Some(CompositionFlags::JOINT_CUT),
        CompositionCode::LimbCut => Some(CompositionFlags::LIMB_CUT),
        CompositionCode::EdgeIntrusion => Some(CompositionFlags::EDGE_INTRUSION),
        CompositionCode::Centred => Some(CompositionFlags::CENTRED),
        CompositionCode::OffBalance => Some(CompositionFlags::OFF_BALANCE),
        CompositionCode::ClutteredBackground => Some(CompositionFlags::CLUTTERED_BACKGROUND),
        CompositionCode::BrightBlob => Some(CompositionFlags::BRIGHT_BLOB),
        CompositionCode::HeadMerge => Some(CompositionFlags::HEAD_MERGE),
        CompositionCode::ColourCompetition => Some(CompositionFlags::COLOUR_COMPETITION),
        CompositionCode::NoGeometry => Some(CompositionFlags::NO_GEOMETRY),
        CompositionCode::Clean
        | CompositionCode::HorizonUncertain
        | CompositionCode::DeliberateCrop
        | CompositionCode::OffThirds
        | CompositionCode::ThirdsAligned
        | CompositionCode::Balanced
        | CompositionCode::CleanBackground
        | CompositionCode::KeypointsUnavailable
        | CompositionCode::NoRule
        | CompositionCode::AestheticUnavailable => None,
    }
}

/// Order the reasons and keep the six that matter.
///
/// Penalties first, strongest first, then the exonerations in the order they were made. A
/// frame with nothing to say carries `Clean`, because every decision needs a reason and
/// "nothing was wrong here" is one.
#[must_use]
pub fn rank(mut reasons: Vec<CompositionReason>) -> Vec<CompositionReason> {
    if reasons.is_empty() {
        return vec![CompositionReason::frame(
            CompositionCode::Clean,
            CompositionCode::Clean.user_text(),
            0.0,
        )];
    }
    reasons.sort_by(|left, right| {
        left.weight
            .total_cmp(&right.weight)
            .then_with(|| left.code.cmp(&right.code))
    });

    // Degradation must remain visible even when the frame also has many violations. Then
    // reserve one reason per local flag so a retained overlay bit always has pixels behind
    // it. Fill the remaining panel slots in the normal strongest-penalty order.
    let mut selected = Vec::with_capacity(CompositionResult::MAX_REASONS);
    for code in [
        CompositionCode::AestheticUnavailable,
        CompositionCode::KeypointsUnavailable,
        CompositionCode::NoGeometry,
        CompositionCode::NoRule,
    ] {
        take_first(&mut reasons, &mut selected, |reason| reason.code == code);
    }
    for flag in LOCALIZABLE_FLAGS {
        take_first(&mut reasons, &mut selected, |reason| {
            reason.evidence.is_some() && flag_for_code(reason.code) == Some(flag)
        });
    }
    let remaining = CompositionResult::MAX_REASONS.saturating_sub(selected.len());
    selected.extend(reasons.into_iter().take(remaining));
    selected.sort_by(|left, right| {
        left.weight
            .total_cmp(&right.weight)
            .then_with(|| left.code.cmp(&right.code))
    });
    let mut reasons = selected;

    // A frame whose only reasons are exonerations still needs one that says so first, so
    // that the panel's headline is "this is fine" rather than "the horizon was not
    // measurable".
    if reasons.iter().all(|reason| !reason.is_penalty())
        && !reasons
            .iter()
            .any(|reason| matches!(reason.code, CompositionCode::Clean))
        && reasons.len() < CompositionResult::MAX_REASONS
    {
        reasons.insert(
            0,
            CompositionReason::frame(
                CompositionCode::Clean,
                CompositionCode::Clean.user_text(),
                0.0,
            ),
        );
    }
    reasons
}

fn take_first(
    source: &mut Vec<CompositionReason>,
    selected: &mut Vec<CompositionReason>,
    predicate: impl Fn(&CompositionReason) -> bool,
) {
    if selected.len() >= CompositionResult::MAX_REASONS {
        return;
    }
    if let Some(position) = source.iter().position(predicate) {
        selected.push(source.remove(position));
    }
}

/// The rectangle a whole-frame reason would point at, when a caller needs one anyway.
///
/// The panel draws nothing for a `None` evidence box; this exists for the overlay's
/// thirds-grid mode, which needs a rectangle to anchor the grid to even when the reason is
/// about the frame as a whole.
#[must_use]
pub const fn whole_frame() -> Box2 {
    Box2::FULL
}
