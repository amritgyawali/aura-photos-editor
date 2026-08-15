//! Was the blur a mistake or a decision.
//!
//! PHASE-09 section 6.2, and section 12's second failure mode: "intentional motion or
//! closed eyes flagged as defects". A dance-floor drag, a panned exit and a missed
//! shutter speed all produce a smeared subject. Only one of them is a mistake, and a
//! product that cannot tell them apart deletes the frames a wedding is sold on.
//!
//! ## Three signals, none of them sufficient alone
//!
//! **Direction.** [`structure_tensor`] gives the dominant gradient orientation and how
//! strongly the region agrees about it. Motion blur is directional: it smears along one
//! axis and leaves the perpendicular axis alone, so a smeared region has *high*
//! coherence, whereas an out-of-focus region is smeared equally in every direction and
//! has low coherence. **That is the single measurement that separates motion blur from
//! a focus miss**, and no amount of sharpness measuring can substitute for it.
//!
//! **Where the smear is.** A global smear across subject and background together is the
//! camera moving. A smear on the subject with a sharp background is the subject moving.
//! A smear on the background with a sharp subject is a pan, which is a decision.
//!
//! **What the photographer set.** Section 6.2 asks for the reciprocal rule - a shutter
//! slower than one over the focal length risks shake - plus the stabilisation flag and
//! the scene. `dance_floor` and `exit` expect motion; `family_portrait` does not.
//!
//! ## The rule that is not negotiable
//!
//! Section 6.2: "never flag intentional motion as a defect; instead expose
//! `INTENTIONAL_MOTION` so Phase 12 can treat it as a stylistic keeper." So
//! [`MotionKind::Intentional`] is checked **before** the two defect classes, and the
//! scene's expectation can only ever move a verdict *towards* it. There is no path in
//! this module by which a scene that expects motion makes a frame more defective.

use aura_core::contract::integrity::{CropRect, MotionKind};
use aura_vision::embed::descriptors::LumaPlane;

use crate::integrity::focus::PixelRect;

/// What EXIF and the scene say about a frame, for the motion decision.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MotionContext {
    /// Exposure time in seconds. `None` when EXIF has none, which is not rare on files
    /// that have been through a third-party editor.
    pub shutter_s: Option<f32>,
    /// Focal length in millimetres, 35 mm equivalent.
    pub focal_mm: Option<f32>,
    /// True when the body or the lens reported stabilisation active.
    pub stabilised: bool,
    /// True when the scene is one where motion is expected - `dance_floor`, `exit`,
    /// `first_dance`, `sparkler_exit`.
    ///
    /// Supplied by the caller from phase 07's label, because invariant 7 makes every
    /// threshold a function of the scene and this crate may not classify one.
    pub expects_motion: bool,
}

impl MotionContext {
    /// The slowest shutter this focal length is safe at, in seconds.
    ///
    /// One over the focal length, the rule every photographer knows, plus three stops
    /// when the body says stabilisation was active. Three rather than the five or six
    /// manufacturers claim: the claim is measured on a static subject by somebody
    /// bracing themselves, and a wedding photographer at a reception is holding a camera
    /// in one hand.
    ///
    /// `None` when either number is missing, and the caller must then decide without
    /// this signal rather than assuming a default - a made-up shutter speed produces a
    /// confident wrong answer about whether a frame was hand-holdable.
    #[must_use]
    pub fn safe_shutter_s(&self) -> Option<f32> {
        let focal = self.focal_mm?;
        if focal <= 0.0 {
            return None;
        }
        let stabilisation = if self.stabilised { 8.0 } else { 1.0 };
        Some(stabilisation / focal)
    }

    /// How many stops below the reciprocal rule the exposure was.
    ///
    /// Positive means slower than safe. `None` when either EXIF value is missing.
    #[must_use]
    pub fn stops_below_safe(&self) -> Option<f32> {
        let safe = self.safe_shutter_s()?;
        let shutter = self.shutter_s?;
        if shutter <= 0.0 || safe <= 0.0 {
            return None;
        }
        Some((shutter / safe).log2())
    }
}

/// The direction a region's gradients agree on, and how strongly.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Orientation {
    /// Dominant gradient angle in radians, `-π/2..π/2`.
    pub angle: f32,
    /// How much the region agrees about it, `0..1`.
    ///
    /// One is a perfectly directional texture - a motion smear, a picket fence - and
    /// zero is isotropic, which is what defocus and noise both look like.
    pub coherence: f32,
    /// Mean gradient energy, for weighting one region against another.
    pub energy: f32,
}

/// The structure tensor of one region, as an [`Orientation`].
///
/// The eigen-decomposition of `[[Σgx², Σgxgy], [Σgxgy, Σgy²]]`, written out rather than
/// solved generally because a 2×2 symmetric matrix has a closed form and pulling in a
/// linear algebra crate for it would be six hundred lines of dependency for eight lines
/// of arithmetic.
///
/// Coherence is `(λ₁ - λ₂) / (λ₁ + λ₂)`, which is the standard normalised anisotropy and
/// is scale-free - so a bright region and a dim one with the same directional structure
/// return the same number, which is exactly what a comparison between a face and a
/// background band needs.
#[must_use]
pub fn structure_tensor(plane: &LumaPlane, rect: PixelRect) -> Orientation {
    let mut gxx = 0.0f64;
    let mut gyy = 0.0f64;
    let mut gxy = 0.0f64;
    let mut count = 0u32;
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };

    for y in rect.y0..rect.y1.saturating_sub(1) {
        for x in rect.x0..rect.x1.saturating_sub(1) {
            let here = at(x, y);
            let gx = at(x + 1, y) - here;
            let gy = at(x, y + 1) - here;
            gxx += f64::from(gx * gx);
            gyy += f64::from(gy * gy);
            gxy += f64::from(gx * gy);
            count += 1;
        }
    }
    if count == 0 {
        return Orientation::default();
    }
    let n = f64::from(count);
    let (gxx, gyy, gxy) = (gxx / n, gyy / n, gxy / n);
    let trace = gxx + gyy;
    if trace <= f64::EPSILON {
        return Orientation::default();
    }
    let span = ((gxx - gyy).powi(2) + 4.0 * gxy * gxy).sqrt();
    Orientation {
        // Half the arctangent, because the tensor is quadratic in the gradient: an edge
        // and its opposite produce the same structure and must produce the same angle.
        angle: 0.5 * (2.0 * gxy).atan2(gxx - gyy) as f32,
        coherence: (span / trace).clamp(0.0, 1.0) as f32,
        energy: trace as f32,
    }
}

/// Everything the motion analysis produced for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionResult {
    /// What kind of motion, if any.
    pub kind: MotionKind,
    /// How much of it, `0..1`. Zero when [`MotionResult::kind`] is [`MotionKind::None`].
    pub severity: f32,
    /// The subject region's orientation.
    pub subject: Orientation,
    /// The background band's orientation.
    pub background: Orientation,
    /// How closely the two agree on a direction, `0..1`.
    ///
    /// One means the whole frame smeared the same way, which is a camera that moved.
    /// Carried out of the analysis because it is the number a support engineer wants
    /// when arguing with a `CAMERA_SHAKE` verdict.
    pub agreement: f32,
    /// Stops below the reciprocal rule, when EXIF said.
    pub stops_below_safe: Option<f32>,
}

/// Coherence a region must reach before its blur is called directional.
///
/// 0.35. Below it the region is either sharp - in which case there is no blur to
/// classify - or isotropically soft, which is a focus problem and belongs to
/// [`crate::integrity::focus`]. Set from the synthetic ladder in
/// [`crate::fixtures`]: a Gaussian blur measures 0.06 to 0.15 and a directional smear of
/// the same magnitude measures 0.55 to 0.80, so the threshold sits in a wide empty gap.
pub const DIRECTIONAL_COHERENCE: f32 = 0.35;

/// How closely two regions must agree on an angle to count as one smear.
///
/// Twenty degrees, expressed as the cosine of twice the angle difference so that
/// perpendicular is -1 and parallel is 1. Doubled because gradient orientation is
/// modulo π: a smear at 10° and one at 190° are the same smear.
pub const AGREEMENT_THRESHOLD: f32 = 0.88;

/// How much sharper the subject must be than the background for a pan.
///
/// 1.6. A pan is the photographer tracking the subject while the shutter is open, so
/// the *background* carries the smear and the subject does not.
pub const PAN_RATIO: f32 = 1.6;

/// Classify a frame's motion.
///
/// `subject_rect` and `background_rect` are the same regions
/// [`crate::integrity::focus`] measured, so the two analyses cannot disagree about where
/// the subject is. `subject_sharpness` and `background_sharpness` are its raw acutance
/// figures.
#[must_use]
pub fn analyse(
    plane: &LumaPlane,
    subject_rect: Option<CropRect>,
    background_rect: Option<CropRect>,
    subject_sharpness: f32,
    background_sharpness: f32,
    context: MotionContext,
) -> MotionResult {
    let orient = |rect: Option<CropRect>| -> Orientation {
        rect.and_then(|r| PixelRect::from_norm(r, plane.width, plane.height))
            .map(|pixels| structure_tensor(plane, pixels))
            .unwrap_or_default()
    };
    let subject = orient(subject_rect);
    let background = orient(background_rect);
    let agreement = agreement_of(subject, background);
    let stops = context.stops_below_safe();

    let result = |kind: MotionKind, severity: f32| MotionResult {
        kind,
        severity: severity.clamp(0.0, 1.0),
        subject,
        background,
        agreement,
        stops_below_safe: stops,
    };

    // 1. Intentional, first and deliberately. Section 6.2 forbids ever calling this a
    //    defect, and a check that runs after the defect classes would depend on them
    //    happening not to fire.
    //
    //    Two independent routes to it, because the two things a photographer does on
    //    purpose look nothing alike:
    //      * a **pan** - the background smeared directionally, the subject sharper than
    //        it by `PAN_RATIO`;
    //      * a **scene that expects motion** - a dance floor, an exit - where a
    //        directional smear is the photograph rather than a fault in it.
    let directional = subject.coherence >= DIRECTIONAL_COHERENCE;
    // **The subject must not itself be smeared.** The first version of this check tested
    // only the background's coherence and the sharpness ratio, and the phase 09 eval
    // harness caught it immediately: a camera-shake frame smears the whole image, so its
    // background is directional *and* its subject can still out-measure a coarse
    // background band on acutance - which made every shaken frame read as a pan. A pan is
    // the photographer tracking the subject, so the defining property is that the subject
    // came out *un*smeared, and that is what this clause says.
    let panning = !directional
        && background.coherence >= DIRECTIONAL_COHERENCE
        && background_sharpness > 0.0
        && subject_sharpness / background_sharpness.max(f32::EPSILON) >= PAN_RATIO;
    if panning {
        return result(MotionKind::Intentional, background.coherence);
    }
    if context.expects_motion && directional {
        return result(MotionKind::Intentional, subject.coherence);
    }

    // 2. Camera shake: the whole frame smeared the same way. The EXIF check is
    //    corroboration rather than a gate - a photographer who bumped the tripod at
    //    1/250 has shake that the reciprocal rule says is impossible - but it raises the
    //    severity when it agrees, because a slow shutter is the mechanism.
    let global = directional
        && background.coherence >= DIRECTIONAL_COHERENCE
        && agreement >= AGREEMENT_THRESHOLD;
    if global {
        let from_smear = subject.coherence.midpoint(background.coherence);
        let from_exif = stops.map_or(0.0, |value| (value / 3.0).clamp(0.0, 1.0));
        return result(MotionKind::CameraShake, from_smear.max(from_exif));
    }

    // 3. Subject motion: the subject smeared, the background not. The background
    //    sharpness comparison is what stops a soft-focus portrait landing here.
    if directional && background_sharpness > subject_sharpness {
        return result(MotionKind::SubjectMotion, subject.coherence);
    }

    // 4. Nothing to say. Note that a slow shutter alone never produces a verdict: the
    //    pixels have to show the smear. A tripod frame at four seconds is not shaken.
    result(MotionKind::None, 0.0)
}

/// How closely two orientations agree, `0..1`.
///
/// `cos(2Δθ)` mapped from `-1..1` onto `0..1`, and doubled because gradient orientation
/// is modulo π. Zero when either region has no directional structure to agree about -
/// which is not "they disagree" but "the question does not apply", and is why the
/// caller checks both coherences before consulting this.
#[must_use]
pub fn agreement_of(left: Orientation, right: Orientation) -> f32 {
    if left.coherence <= f32::EPSILON || right.coherence <= f32::EPSILON {
        return 0.0;
    }
    let delta = 2.0 * (left.angle - right.angle);
    delta.cos().midpoint(1.0).clamp(0.0, 1.0)
}
