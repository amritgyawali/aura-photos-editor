//! Turning measurements into the fourteen bits, and into sentences a photographer can
//! argue with.
//!
//! PHASE-09 section 6.5: "every penalty writes a `Reason` with a code, human text,
//! weight and an evidence crop rectangle so the UI can literally show the soft eye."
//! This module is where a number becomes a claim, and it is therefore where the phase
//! is most able to do harm.
//!
//! ## Every threshold here is a function of the scene
//!
//! Invariant 7, and it is not decoration. `SceneProfile::max_acceptable_blur` is 0.15
//! for a family portrait and 0.55 for a dance floor; the *same* subject sharpness is a
//! defect in one and unremarkable in the other. Nothing in this module compares against
//! a constant that was not derived from the profile, except the two structural bounds
//! documented at [`ALWAYS_SOFT`] and [`ALWAYS_CLEAN`].
//!
//! ## Exonerations are emitted, not merely implied
//!
//! Eight of the twenty-one reason codes withdraw a claim rather than making one, and
//! this module emits them **even when no penalty was raised**. A photographer looking at
//! a shallow-depth-of-field portrait wants to see "the background is soft on purpose,
//! and her eyes are sharp" - not an empty panel that could equally mean "nothing was
//! checked". Migration 9's fifth property is that "not checked" is not "clean", and this
//! is that rule applied to the interface rather than to the schema.

use aura_core::contract::integrity::{
    CropRect, IntegrityFlags, MotionKind, Reason, ReasonCode,
};
use aura_core::SceneProfile;

use crate::integrity::calibration::Calibration;
use crate::integrity::exposure::ExposureResult;
use crate::integrity::eyes::IntentRule;
use crate::integrity::focus::FocusResult;
use crate::integrity::motion::MotionResult;
use crate::integrity::noise::NoiseResult;

/// Below this normalised sharpness, a subject is soft in any scene.
///
/// 0.22 - about a quarter of what the body should reach. A structural floor rather than
/// a scene threshold: a dance floor tolerates a lot of softness and does not tolerate an
/// unrecognisable face, and a profile edited to `max_acceptable_blur = 1.0` should not be
/// able to turn the flag off entirely.
pub const ALWAYS_SOFT: f32 = 0.22;

/// Above this normalised sharpness, a subject is sharp in any scene.
///
/// 0.86 - comfortably above the 0.80 a frame that exactly meets its body's expectation
/// scores. The mirror of [`ALWAYS_SOFT`]: a profile edited to `max_acceptable_blur = 0.0`
/// should not be able to call a tack-sharp frame soft.
pub const ALWAYS_CLEAN: f32 = 0.86;

/// How far above its tolerance the noise must be before the flag is raised.
///
/// 1.25, not 1.0. `noise_sigma_rel` is already scene-relative, so 1.0 is exactly the
/// tolerance and flagging there would flag every frame that is merely at the edge of
/// what the scene allows. The quarter is the margin section 12's first failure mode
/// asks for.
pub const NOISE_MARGIN: f32 = 1.25;

/// How much of a group must be blinking before the group itself is flagged.
///
/// A third of the gating subjects. Below that the individual `EYES_CLOSED` flags say
/// everything there is to say; above it the frame has a different problem - a group shot
/// that cannot be used at all rather than one person to clone out - and phase 12's group
/// rules need to see the difference.
pub const GROUP_BLINK_RATIO: f32 = 0.34;

/// One face's contribution to the eye verdict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeVerdict {
    /// The evidence rectangle.
    pub crop: CropRect,
    /// True when the eyes are shut.
    pub closed: bool,
    /// True when they are narrowed.
    pub squinting: bool,
    /// True when this face decides anything.
    pub gates: bool,
    /// Which rule justified a closure, if any.
    pub rule: IntentRule,
}

/// Everything the frame measured, gathered for the flag decision.
#[derive(Debug, Clone)]
pub struct Evidence<'a> {
    /// The focus analysis.
    pub focus: &'a FocusResult,
    /// The motion analysis.
    pub motion: &'a MotionResult,
    /// The exposure analysis.
    pub exposure: &'a ExposureResult,
    /// The noise analysis.
    pub noise: &'a NoiseResult,
    /// One entry per face, in prominence order.
    pub eyes: &'a [EyeVerdict],
    /// Fraction of gating subjects with closed eyes.
    pub closed_eye_ratio: f32,
    /// True when phase 06 found nothing to be subject-aware about.
    pub subjectless: bool,
    /// The scene's tolerances and weights.
    pub profile: &'a SceneProfile,
    /// The body's measurements, or the fallback.
    pub calibration: &'a Calibration,
}

/// What the flag decision produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// The bits.
    pub flags: IntegrityFlags,
    /// The sentences, penalties first and strongest penalty at the top.
    pub reasons: Vec<Reason>,
}

/// The normalised sharpness below which this scene calls a subject soft.
///
/// Interpolates between [`ALWAYS_SOFT`] and 0.72 as the scene's blur tolerance goes from
/// 1.0 to 0.0, then clamps into the structural bounds. A family portrait
/// (`max_acceptable_blur` 0.15) lands at 0.65; a dance floor (0.55) at 0.44.
#[must_use]
pub fn soft_threshold(profile: &SceneProfile) -> f32 {
    let tolerance = profile.max_acceptable_blur.clamp(0.0, 1.0);
    (0.72 - tolerance * (0.72 - ALWAYS_SOFT)).clamp(ALWAYS_SOFT, ALWAYS_CLEAN)
}

/// Decide the flags and write the reasons.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn decide(evidence: &Evidence<'_>) -> Verdict {
    let mut flags = IntegrityFlags::EMPTY;
    let mut reasons: Vec<Reason> = Vec::new();

    // --- Subject, or the absence of one -------------------------------------
    if evidence.subjectless {
        flags = flags.union(IntegrityFlags::NO_SUBJECT_DETECTED);
        reasons.push(Reason::frame(
            ReasonCode::NoSubject,
            "no face or person was found here, so the sharpness reading is about the whole frame \
             rather than about a subject",
            0.0,
        ));
    }

    // --- Focus ---------------------------------------------------------------
    let threshold = soft_threshold(evidence.profile);
    let subject_crop = evidence
        .eyes
        .first()
        .map_or(CropRect::FULL, |eye| eye.crop);
    if evidence.focus.subject < threshold {
        flags = flags.union(IntegrityFlags::SUBJECT_SOFT);
        // The penalty scales with how far below the threshold it fell, so a frame that
        // just missed is not marked as heavily as one that is unusable.
        let depth = ((threshold - evidence.focus.subject) / threshold).clamp(0.0, 1.0);
        reasons.push(Reason::at(
            ReasonCode::SubjectSoft,
            if evidence.subjectless {
                "the middle of this frame is softer than this camera should manage"
            } else {
                "the subject is softer than this camera should manage in this kind of photograph"
            },
            -0.20 - 0.35 * depth,
            subject_crop,
        ));
    } else if evidence.focus.shallow_depth_of_field {
        // Section 13's second criterion, said out loud. The *structural* guarantee is
        // that `focus.subject` was measured on the subject's own regions and a soft
        // background could not lower it; this is the sentence that tells a photographer
        // the product understood what they did.
        reasons.push(Reason::at(
            ReasonCode::ShallowDepthOfField,
            "the background is soft and the subject is sharp: this reads as a deliberate shallow \
             depth of field, not as a missed focus",
            0.0,
            subject_crop,
        ));
    }

    // Back and front focus are separate claims from softness and can accompany it.
    if evidence.focus.offset > 0.0 && flags.contains(IntegrityFlags::SUBJECT_SOFT) {
        flags = flags.union(IntegrityFlags::BACK_FOCUS);
        reasons.push(Reason::frame(
            ReasonCode::BackFocus,
            "what is behind the subject is sharper than the subject is, which usually means the \
             focus landed behind them",
            -0.10,
        ));
    } else if evidence.focus.offset < 0.0 && flags.contains(IntegrityFlags::SUBJECT_SOFT) {
        flags = flags.union(IntegrityFlags::FRONT_FOCUS);
        reasons.push(Reason::frame(
            ReasonCode::FrontFocus,
            "something in front of the subject is sharper than the subject is, which usually \
             means the focus landed short",
            -0.10,
        ));
    }

    // --- Motion --------------------------------------------------------------
    match evidence.motion.kind {
        MotionKind::Intentional => {
            flags = flags.union(IntegrityFlags::INTENTIONAL_MOTION);
            reasons.push(Reason::frame(
                ReasonCode::IntentionalMotion,
                "the blur here runs in one direction and the subject is held: this reads as a \
                 deliberate pan or drag rather than a mistake",
                0.0,
            ));
        }
        MotionKind::CameraShake => {
            flags = flags.union(IntegrityFlags::CAMERA_SHAKE);
            let head = "the whole frame is smeared in one direction, which is the camera moving";
            let text = match evidence.motion.stops_below_safe {
                Some(stops) if stops > 0.0 => format!(
                    "{head} - the shutter was about {stops:.1} stops slower than this focal \
                     length is usually hand-holdable at"
                ),
                _ => head.to_string(),
            };
            reasons.push(Reason::frame(
                ReasonCode::CameraShake,
                text,
                -0.15 - 0.35 * evidence.motion.severity,
            ));
        }
        MotionKind::SubjectMotion => {
            flags = flags.union(IntegrityFlags::SUBJECT_MOTION);
            reasons.push(Reason::at(
                ReasonCode::SubjectMotion,
                "the subject is smeared while the background is not, so they moved during the \
                 exposure",
                -0.12 - 0.28 * evidence.motion.severity,
                subject_crop,
            ));
        }
        MotionKind::None => {}
    }

    // --- Exposure ------------------------------------------------------------
    if evidence.exposure.clipping.specular > 0.5 && evidence.exposure.clipping.high > 0.0 {
        reasons.push(Reason::frame(
            ReasonCode::SpecularHighlight,
            "the brightest parts of this frame are lights rather than blown detail, so they are \
             not counted against it",
            0.0,
        ));
    }
    if evidence.exposure.highlight_lost {
        flags = flags.union(IntegrityFlags::HIGHLIGHT_LOST);
        reasons.push(Reason::frame(
            ReasonCode::HighlightLost,
            "the highlights are clipped past what this camera can bring back",
            -0.18 - 0.30 * evidence.exposure.clipping.high.min(1.0),
        ));
    } else if evidence.exposure.clipping.high > crate::integrity::exposure::CLIP_IGNORE {
        reasons.push(Reason::frame(
            ReasonCode::HighlightRecoverable,
            "some highlights are clipped, but within what this camera can bring back",
            0.0,
        ));
    }
    if evidence.exposure.shadow_lost {
        flags = flags.union(IntegrityFlags::SHADOW_LOST);
        reasons.push(Reason::frame(
            ReasonCode::ShadowLost,
            "the shadows are below this camera's noise floor at this ISO, so lifting them would \
             show more noise than detail",
            -0.18 - 0.25 * evidence.exposure.clipping.low.min(1.0),
        ));
    }
    if evidence.exposure.mixed_light {
        flags = flags.union(IntegrityFlags::MIXED_LIGHT_RISK);
        reasons.push(Reason::frame(
            ReasonCode::MixedLight,
            "two different colours of light are falling on this scene, so one white balance will \
             not correct all of it",
            -0.05,
        ));
    }

    // --- Noise ---------------------------------------------------------------
    if evidence.noise.measured {
        if evidence.noise.sigma_rel > NOISE_MARGIN {
            flags = flags.union(IntegrityFlags::HEAVY_NOISE);
            let over = (evidence.noise.sigma_rel - NOISE_MARGIN).min(2.0) / 2.0;
            reasons.push(Reason::frame(
                ReasonCode::HeavyNoise,
                "there is more noise here than this kind of photograph carries well",
                -0.12 - 0.28 * over,
            ));
        } else if evidence.noise.sigma_rel > 0.85 {
            reasons.push(Reason::frame(
                ReasonCode::NoiseWithinScene,
                "this frame is noisy, and no noisier than this kind of photograph at this ISO \
                 usually is",
                0.0,
            ));
        }
    }

    // --- Eyes ----------------------------------------------------------------
    // Ordered: the unjustified closures first, because those are the only ones that
    // become a defect; then the justified ones, which are the sentences that keep a
    // photographer's trust; then squints.
    for eye in evidence.eyes.iter().filter(|eye| eye.gates && eye.closed) {
        if eye.rule.justifies() {
            flags = flags.union(IntegrityFlags::EYES_CLOSED_OK);
            reasons.push(Reason::at(ReasonCode::EyesClosedOk, eye.rule.text(), 0.0, eye.crop));
        } else {
            flags = flags.union(IntegrityFlags::EYES_CLOSED);
            reasons.push(Reason::at(
                ReasonCode::EyesClosed,
                "their eyes are closed here, and nothing about this moment explains it",
                -0.30,
                eye.crop,
            ));
        }
    }
    if evidence.closed_eye_ratio >= GROUP_BLINK_RATIO && evidence.eyes.len() >= 3 {
        reasons.push(Reason::frame(
            ReasonCode::GroupBlink,
            "several people in this group have their eyes closed at the same time",
            -0.20,
        ));
    }
    if let Some(squint) = evidence
        .eyes
        .iter()
        .find(|eye| eye.gates && eye.squinting && !eye.closed)
    {
        flags = flags.union(IntegrityFlags::SQUINT);
        reasons.push(Reason::at(
            ReasonCode::Squint,
            "they are squinting here, which usually means the sun or a flash was in their eyes",
            -0.08,
            squint.crop,
        ));
    }

    // --- Calibration ---------------------------------------------------------
    if !evidence.calibration.measured {
        reasons.push(Reason::frame(
            ReasonCode::Uncalibrated,
            "AURA has not measured this camera model yet, so every reading here is a cautious \
             one",
            0.0,
        ));
    }

    // --- Nothing to say ------------------------------------------------------
    if reasons.is_empty() {
        reasons.push(Reason::frame(
            ReasonCode::Clean,
            "focus, motion, exposure, noise and eyes all read normally here",
            0.0,
        ));
    }

    // Strongest penalty first, then exonerations in the order they were made. A stable
    // sort, so two reasons of equal weight keep the order this function wrote them in -
    // which is the order they are argued in, and a panel that reshuffled it between two
    // renders would look broken.
    reasons.sort_by(|left, right| left.weight.total_cmp(&right.weight));
    reasons.truncate(aura_core::contract::integrity::IntegrityResult::MAX_REASONS);

    Verdict { flags, reasons }
}
