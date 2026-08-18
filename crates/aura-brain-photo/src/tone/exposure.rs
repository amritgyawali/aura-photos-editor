//! Exposure, anchored on the subject and bounded by what the frame can take.
//!
//! PHASE-15 section 6.1, and section 1's whole argument in one sentence:
//!
//! > Exposure must be subject-referred, not average-referred: the bride's face is the anchor,
//! > not the mean luminance of a dark reception hall. That one change fixes most of what
//! > photographers hate about auto-exposure.
//!
//! ## The four steps, in order
//!
//! 1. **Measure the subject.** [`subject_luminance`] is the prominence-weighted mean
//!    luminance of the face regions, in perceptual units, because that is what "how bright
//!    does this face look" means and what section 6.1's own numbers - "ceremony faces at
//!    45-55 %" - are expressed in.
//! 2. **Aim.** The scene's band and its mood decide where inside the band to aim.
//!    [`crate::tone::targets::SceneTarget::aim_for`] is where "moody dance floors stay moody"
//!    becomes an aim point rather than a special case.
//! 3. **Bound.** The move is clamped so that new highlight clipping stays inside the scene's
//!    tolerance and so that a lift stays inside the body's measured shadow headroom. Both
//!    bounds emit their own reason, because a face that is still a third of a stop dark for
//!    a stated reason is a decision and a face that is dark for no stated reason is a bug.
//! 4. **Explain.** Every path through this module produces at least one reason. Invariant 2,
//!    and the schema's `reason_count` CHECK refuses a row without one.
//!
//! ## Perceptual in, stops out
//!
//! The band is perceptual and the answer is in stops, which are linear. The conversion is
//! [`perceptual_to_linear`] and it is the sRGB transfer function rather than a 2.2 gamma, for
//! the reason `stats::linearise` gives: the toe is where a dark reception's faces are, and a
//! 2.2 approximation would misjudge the very frames this phase exists for by about an eighth
//! of a stop - which is most of section 10.1's 0.15 EV tolerance spent on an approximation
//! nobody needed to make.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::FaceRef;
use aura_core::contract::tone::{ToneCode, EXPOSURE_RANGE};
use aura_vision::embed::descriptors::LumaPlane;

use crate::tone::stats::FrameStats;
use crate::tone::store::codec::{self, Explained};
use crate::tone::targets::{Mood, SceneTarget};

/// The smallest exposure move worth making, in stops.
///
/// A twentieth of a stop. Below it nothing is visible, and writing a recipe whose exposure is
/// 0.03 makes every frame in a wedding carry a non-default value - which turns "has AURA
/// touched this photograph" from a question with an answer into a column that is always true.
pub const DEAD_BAND_EV: f32 = 0.05;

/// The most this phase will move an exposure, in stops.
///
/// Three, against the recipe's own range of five. A frame that needs more than three stops is
/// not an exposure error; it is a frame shot at the wrong settings entirely, and lifting it
/// three stops produces something a photographer will look at and delete. Bounding it here
/// means the reason says the frame was held rather than the recipe silently containing a
/// number nobody would have chosen.
pub const MAX_MOVE_EV: f32 = 3.0;

/// One perceptual value in linear light.
///
/// The sRGB electro-optical transfer function, the same curve `stats::linearise` applies to
/// an 8-bit value, expressed for a continuous input.
#[must_use]
pub fn perceptual_to_linear(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// One linear value in perceptual units.
#[must_use]
pub fn linear_to_perceptual(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// What the faces in this frame are worth measuring, and how bright they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Subject {
    /// Prominence-weighted mean face luminance, perceptual, `0..1`.
    pub luma: f32,
    /// The total weight behind that mean.
    ///
    /// Not a confidence: it is the sum of `area_frac * quality` over the faces, so a frame
    /// with one large sharp face and a frame with twenty tiny soft ones are told apart.
    pub weight: f32,
    /// The largest contributing face, for the evidence crop.
    pub anchor: CropRect,
}

/// The smallest total face weight that anchors an exposure.
///
/// Below it the "subject" is a face eleven pixels across at the back of a hall, and anchoring
/// a whole frame's exposure on it produces a blown foreground. The frame falls through to the
/// scene model instead, and says so.
pub const MIN_SUBJECT_WEIGHT: f32 = 0.0015;

/// Measure the prominence-weighted luminance of the faces in a frame.
///
/// Returns `None` when there is nothing worth anchoring on. `None` rather than a zero,
/// because zero is a legitimate measurement - a face in a silhouette - and the two lead to
/// opposite decisions.
#[must_use]
pub fn subject_luminance(faces: &[FaceRef], plane: &LumaPlane) -> Option<Subject> {
    if faces.is_empty() || plane.width == 0 || plane.height == 0 {
        return None;
    }
    let mut total = 0.0f64;
    let mut weight_total = 0.0f64;
    let mut anchor = CropRect::FULL;
    let mut best_weight = 0.0f32;

    for face in faces {
        // The middle half of the box, as the skin sampler uses: the corners of a face box
        // are hair and background, and both are darker than skin, so including them would
        // make every measured face too dark and every frame slightly over-exposed.
        let area = face.bbox.expanded(-0.5).clamped();
        if area.is_empty() {
            continue;
        }
        let Some(mean) = mean_in(plane, &area) else {
            continue;
        };
        let weight = face.area_frac.max(0.0) * face.quality.clamp(0.0, 1.0);
        if weight <= 0.0 {
            continue;
        }
        total += f64::from(mean) * f64::from(weight);
        weight_total += f64::from(weight);
        if weight > best_weight {
            best_weight = weight;
            anchor = face.bbox;
        }
    }
    if weight_total <= f64::from(MIN_SUBJECT_WEIGHT) {
        return None;
    }
    Some(Subject {
        luma: (total / weight_total) as f32,
        weight: weight_total as f32,
        anchor,
    })
}

/// Mean perceptual luminance inside a normalised rectangle.
fn mean_in(plane: &LumaPlane, area: &CropRect) -> Option<f32> {
    let x0 = ((area.x * plane.width as f32) as usize).min(plane.width.saturating_sub(1));
    let y0 = ((area.y * plane.height as f32) as usize).min(plane.height.saturating_sub(1));
    let x1 = (((area.x + area.w) * plane.width as f32) as usize).min(plane.width);
    let y1 = (((area.y + area.h) * plane.height as f32) as usize).min(plane.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut total = 0.0f64;
    let mut count = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            total += f64::from(
                plane
                    .values
                    .get(y * plane.width + x)
                    .copied()
                    .unwrap_or(0.0),
            );
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    Some((total / f64::from(count)) as f32)
}

/// Everything outside the pixels that the exposure solve depends on.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// The scene's bands and policy.
    pub target: SceneTarget,
    /// Phase 09's measured shadow headroom for this body at this ISO, in stops.
    ///
    /// Taken rather than recomputed: `IntegrityService` owns what a lift costs in noise, and
    /// a second answer to "how far can this body be pushed" is two exposure decisions that
    /// disagree. Zero when the body is uncalibrated, which bounds every lift at the
    /// conservative baseline rather than at a guess.
    pub shadow_headroom_ev: f32,
    /// True when the frame reads as backlit.
    pub backlit: bool,
}

/// What the exposure solve decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    /// The exposure offset, in stops.
    pub ev: f32,
    /// How sure it is, `0..1`.
    pub confidence: f32,
    /// The subject luminance before the move, or zero when no face anchored it.
    pub subject_before: f32,
    /// Where the band wanted it.
    pub subject_target: f32,
    /// True when a face anchored the decision.
    pub face_anchored: bool,
    /// True when the backlit rule applied.
    pub backlit: bool,
    /// How much new highlight clipping the move adds, as a fraction of the frame.
    pub clipping_added: f32,
    /// Why, in the order the module decided them, each with the numbers its sentence
    /// names.
    pub reasons: Vec<Explained>,
}

/// Solve the exposure for one frame.
///
/// `scene_model_ev` is the learned faceless exposure, or `None` when the head is unavailable
/// or untrained. The distinction between "no faces and a model" and "no faces and no model"
/// is the difference between [`ToneCode::NoFaceSceneModel`] and
/// [`ToneCode::ExposureUnavailable`], and it is visible in the panel because the second is a
/// photograph nobody has decided anything about.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn solve(
    subject: Option<Subject>,
    stats: &FrameStats,
    bounds: Bounds,
    scene_model_ev: Option<f32>,
) -> Solution {
    let target = bounds.target;
    let mut reasons: Vec<Explained> = Vec::new();

    // The clipping allowance. A backlit frame is allowed to blow its background, which is
    // section 6.1's dedicated rule and is what a photographer does by hand.
    let clip_allowance = if bounds.backlit {
        target.backlight_allowance.max(target.clip_tolerance)
    } else {
        target.clip_tolerance
    };

    let Some(subject) = subject else {
        return faceless(stats, bounds, clip_allowance, scene_model_ev, &mut reasons);
    };

    let aim = target.aim_for(target.mood);
    let current_linear = perceptual_to_linear(subject.luma).max(1e-5);
    let wanted_linear = perceptual_to_linear(aim).max(1e-5);
    let wanted_ev = (wanted_linear / current_linear).log2();

    let in_band = target.in_band(subject.luma);
    let below = subject.luma < target.subject_luma_min;

    // Mood. `Preserve` aims at the bottom of the band rather than its middle, and says so
    // when the frame was actually below it - a preserve-mood scene whose subject was already
    // correctly exposed has nothing to preserve and should not claim to.
    if below && target.mood == Mood::Preserve {
        reasons.push(codec::plain(ToneCode::MoodPreserved, 0.0));
    }

    let desired = wanted_ev.clamp(-MAX_MOVE_EV, MAX_MOVE_EV);
    let mut ev = desired;
    let mut held_highlights = false;
    let mut held_shadows = false;

    // Bound 1: highlights. Walk down from the desired move until the added clipping fits.
    // A search rather than a closed form, because `clipping_after` reads a histogram and the
    // relationship between stops and clipped fraction is whatever the photograph's own
    // highlight distribution says it is.
    if ev > 0.0 && stats.clipping_added(ev) > clip_allowance {
        let mut best = 0.0f32;
        let mut step = ev;
        // Sixteen bisection steps take a three-stop range to about a thousandth of a stop,
        // which is four orders of magnitude finer than anything visible and costs sixteen
        // histogram scans of 256 entries.
        for _ in 0..16 {
            step /= 2.0;
            let candidate = best + step;
            if stats.clipping_added(candidate) <= clip_allowance {
                best = candidate;
            }
        }
        ev = best;
        held_highlights = true;
    }

    // Bound 2: shadows. A lift amplifies whatever the sensor recorded down there, and phase
    // 09 already measured how much this body has. The scene scales it, because a dance floor
    // is allowed to be noisier than a family portrait.
    let lift_ceiling = (bounds.shadow_headroom_ev * target.shadow_lift_scale).max(0.0);
    if ev > lift_ceiling && lift_ceiling > 0.0 {
        ev = lift_ceiling;
        held_shadows = true;
    }

    if ev.abs() < DEAD_BAND_EV {
        ev = 0.0;
    }
    let ev = ev.clamp(EXPOSURE_RANGE.0, EXPOSURE_RANGE.1);
    let clipping_added = stats.clipping_added(ev);

    // The reasons, in the order a photographer would want to read them: what was wrong, then
    // what stopped the fix.
    if in_band && ev.abs() < DEAD_BAND_EV {
        reasons.push(codec::plain_at(
            ToneCode::SubjectExposed,
            0.0,
            subject.anchor,
        ));
    } else {
        let code = if below || wanted_ev > 0.0 {
            ToneCode::SubjectUnderexposed
        } else {
            ToneCode::SubjectOverexposed
        };
        reasons.push(codec::reason_with(
            code,
            -0.04,
            &[
                subject.luma * 100.0,
                target.subject_luma_min * 100.0,
                target.subject_luma_max * 100.0,
            ],
            Some(subject.anchor),
        ));
    }
    if bounds.backlit {
        reasons.push(codec::plain_at(
            ToneCode::BacklitSubject,
            0.0,
            subject.anchor,
        ));
    }
    if held_highlights {
        reasons.push(codec::plain(ToneCode::ExposureHeldForHighlights, -0.06));
    }
    if held_shadows {
        reasons.push(codec::plain(ToneCode::ExposureHeldForShadows, -0.06));
    }

    // Confidence. A face-anchored exposure that reached its band is the strongest claim this
    // phase makes; every constraint that stopped it short is a claim about the photograph
    // rather than about the subject, and costs a little.
    let mut confidence = 0.88f32;
    if held_highlights {
        confidence -= 0.06;
    }
    if held_shadows {
        confidence -= 0.06;
    }
    if !target.measured {
        confidence -= crate::tone::targets::UNTARGETED_CONFIDENCE_PENALTY;
    }
    // A frame anchored on very little face is a frame anchored on very little.
    confidence *= (subject.weight / 0.02).clamp(0.55, 1.0);

    Solution {
        ev,
        confidence: confidence.clamp(0.0, 1.0),
        subject_before: subject.luma,
        subject_target: aim,
        face_anchored: true,
        backlit: bounds.backlit,
        clipping_added,
        reasons,
    }
}

/// The no-face path: the scene model, or nothing at all.
fn faceless(
    stats: &FrameStats,
    bounds: Bounds,
    clip_allowance: f32,
    scene_model_ev: Option<f32>,
    reasons: &mut Vec<Explained>,
) -> Solution {
    let Some(model_ev) = scene_model_ev else {
        reasons.push(codec::plain(ToneCode::ExposureUnavailable, -0.10));
        return Solution {
            ev: 0.0,
            // Deliberately low and deliberately not zero: the frame is left as the camera
            // recorded it, which is a defensible answer and not a confident one.
            confidence: 0.30,
            subject_before: 0.0,
            subject_target: 0.0,
            face_anchored: false,
            backlit: bounds.backlit,
            clipping_added: 0.0,
            reasons: std::mem::take(reasons),
        };
    };

    let mut ev = model_ev.clamp(-MAX_MOVE_EV, MAX_MOVE_EV);
    let mut held = false;
    if ev > 0.0 && stats.clipping_added(ev) > clip_allowance {
        let mut best = 0.0f32;
        let mut step = ev;
        for _ in 0..16 {
            step /= 2.0;
            let candidate = best + step;
            if stats.clipping_added(candidate) <= clip_allowance {
                best = candidate;
            }
        }
        ev = best;
        held = true;
    }
    let ceiling = (bounds.shadow_headroom_ev * bounds.target.shadow_lift_scale).max(0.0);
    let mut held_shadows = false;
    if ev > ceiling && ceiling > 0.0 {
        ev = ceiling;
        held_shadows = true;
    }
    if ev.abs() < DEAD_BAND_EV {
        ev = 0.0;
    }
    let ev = ev.clamp(EXPOSURE_RANGE.0, EXPOSURE_RANGE.1);

    reasons.push(codec::plain(ToneCode::NoFaceSceneModel, -0.12));
    if held {
        reasons.push(codec::plain(ToneCode::ExposureHeldForHighlights, -0.06));
    }
    if held_shadows {
        reasons.push(codec::plain(ToneCode::ExposureHeldForShadows, -0.06));
    }

    let mut confidence = 0.62f32;
    if held {
        confidence -= 0.06;
    }
    if held_shadows {
        confidence -= 0.06;
    }
    if !bounds.target.measured {
        confidence -= crate::tone::targets::UNTARGETED_CONFIDENCE_PENALTY;
    }

    Solution {
        ev,
        confidence: confidence.clamp(0.0, 1.0),
        subject_before: 0.0,
        subject_target: 0.0,
        face_anchored: false,
        backlit: bounds.backlit,
        clipping_added: stats.clipping_added(ev),
        reasons: std::mem::take(reasons),
    }
}
