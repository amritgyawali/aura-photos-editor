//! Where the action in a moment was strongest.
//!
//! ## The curve, the kernel and the argmax
//!
//! Section 6.2: "within a moment, build an expression/interaction curve over frames and
//! find the argmax with a smoothing kernel; `peak_margin` records how clearly the peak
//! wins."
//!
//! The curve at frame *i* is
//!
//! ```text
//! c[i] = 0.60 * max over faces of expression intensity
//!      + 0.40 * strongest interaction strength
//! ```
//!
//! then convolved with `[0.25, 0.50, 0.25]`, reflected at the ends. Sixty-forty because
//! the faces are the more reliable of the two signals - eight readings over up to thirty
//! faces against one reading over the whole frame - and because a moment with no
//! interaction detected must still have a peak.
//!
//! **The kernel is the part that earns its place.** A raw argmax over fourteen burst
//! frames picks whichever frame the head happened to read half a percent higher, which
//! is noise; the kernel makes the peak the frame at the top of a *rise*, which is what a
//! person means by the apex of a kiss. Reflected at the ends rather than zero-padded,
//! because zero-padding pulls the first and last frames down and a bouquet toss whose
//! apex is the final frame is a real and common shape.
//!
//! ## Why a margin, and why it is allowed to fail
//!
//! Fourteen frames of a bracketed detail shot genuinely have no apex. `MIN_MARGIN` is
//! where the product stops pretending otherwise: below it the kind is `Flat`,
//! `AURA-ML-5042` is logged, and `EmotionOutline::peak_rate` reports the truth about the
//! wedding rather than a number that always looks good.
//!
//! The alternative - always naming a peak - fails in a specific way. Phase 29 builds
//! album spreads around peak frames, and a peak chosen by a rounding error is a spread
//! built around a rounding error.

use aura_core::contract::emotion::{
    EmotionCode, EmotionReason, FaceExpression, ImageId, Interaction, MomentPeak, PeakKind,
};
use aura_core::{MomentId, SceneId};

/// How much the face term contributes to the curve.
pub const FACE_WEIGHT: f32 = 0.60;

/// How much the interaction term contributes.
pub const INTERACTION_WEIGHT: f32 = 0.40;

/// The smoothing kernel, section 6.2's.
///
/// Three taps rather than five. A wedding moment is three to six frames at this
/// product's measured cadence, and a five-tap kernel over four frames is an average
/// dressed as a peak detector.
pub const KERNEL: [f32; 3] = [0.25, 0.50, 0.25];

/// One frame's contribution to a moment's curve.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The photograph.
    pub image: ImageId,
    /// Every face's reading.
    pub faces: Vec<FaceExpression>,
    /// The interactions detected, strongest first.
    pub interactions: Vec<(Interaction, f32)>,
    /// What phase 07 says the frame is of, for the peak kind.
    pub scene: SceneId,
}

impl Frame {
    /// This frame's raw curve value, before smoothing.
    #[must_use]
    pub fn value(&self) -> f32 {
        let faces = self
            .faces
            .iter()
            .map(FaceExpression::intensity)
            .fold(0.0f32, f32::max);
        let interaction = self
            .interactions
            .first()
            .map_or(0.0, |(_, strength)| *strength);
        (FACE_WEIGHT * faces + INTERACTION_WEIGHT * interaction).clamp(0.0, 1.0)
    }
}

/// What one moment's peak analysis produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// The smoothed value at each frame, in the moment's frame order.
    pub smoothed: Vec<f32>,
    /// The index of the strongest frame.
    pub index: usize,
    /// `(top - runner_up) / top`, or zero when there is no runner-up.
    pub margin: f32,
}

impl Curve {
    /// Where one frame sits on the curve relative to the peak, `0..1`.
    ///
    /// Section 5's `peak_proximity`, and it is 1.0 at the peak **by construction** rather
    /// than by a threshold: it is the smoothed value divided by the peak's. A frame in a
    /// flat moment lands near 1.0, which is the honest reading - if nothing separated
    /// itself, every frame is as near the peak as every other.
    #[must_use]
    pub fn proximity(&self, index: usize) -> f32 {
        let Some(top) = self.smoothed.get(self.index).copied() else {
            return NEUTRAL_PROXIMITY;
        };
        if top <= f32::EPSILON {
            return NEUTRAL_PROXIMITY;
        }
        self.smoothed
            .get(index)
            .map_or(NEUTRAL_PROXIMITY, |value| (value / top).clamp(0.0, 1.0))
    }
}

/// What a frame with no siblings gets.
///
/// Half: neither the peak nor off it. The same convention
/// `IntegrityResult::relative_sharpness` uses for a frame alone in a moment, deliberately,
/// so that two phases which both answer "where does this frame sit among its siblings"
/// answer it on the same scale.
pub const NEUTRAL_PROXIMITY: f32 = 0.5;

/// Above this proximity a frame is described as near the peak.
///
/// Only a reason-writing threshold. Nothing is decided at it.
pub const NEAR_PEAK: f32 = 0.85;

/// Below this proximity a frame is described as off-peak.
pub const OFF_PEAK: f32 = 0.55;

/// Build and smooth a moment's curve.
///
/// Ties are broken by the earlier frame, which is the moment's timeline order and
/// therefore deterministic. Invariant 4: two runs over the same wedding pick the same
/// frame, and a tie broken by iteration order would not.
#[must_use]
pub fn curve(frames: &[Frame]) -> Curve {
    let raw: Vec<f32> = frames.iter().map(Frame::value).collect();
    let smoothed = smooth(&raw);

    let mut index = 0usize;
    let mut top = f32::NEG_INFINITY;
    for (position, value) in smoothed.iter().enumerate() {
        if *value > top {
            top = *value;
            index = position;
        }
    }
    if !top.is_finite() {
        top = 0.0;
    }

    let runner_up = smoothed
        .iter()
        .enumerate()
        .filter(|(position, _)| *position != index)
        .map(|(_, value)| *value)
        .fold(f32::NEG_INFINITY, f32::max);
    let margin = if !runner_up.is_finite() || top <= f32::EPSILON {
        // No runner-up: a moment of one frame. Zero rather than one, because a peak that
        // beat nothing has not been shown to be a peak - and `MomentPeak::is_resolved`
        // then says so.
        0.0
    } else {
        ((top - runner_up) / top).clamp(0.0, 1.0)
    };

    Curve {
        smoothed,
        index,
        margin,
    }
}

/// Convolve with [`KERNEL`], reflecting at the ends.
///
/// Reflection rather than zero-padding; see this module's header.
#[must_use]
pub fn smooth(values: &[f32]) -> Vec<f32> {
    if values.len() < 3 {
        return values.to_vec();
    }
    let last = values.len() - 1;
    values
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let before = if index == 0 { 1 } else { index - 1 };
            let after = if index == last { last - 1 } else { index + 1 };
            KERNEL[0] * values.get(before).copied().unwrap_or(0.0)
                + KERNEL[1] * values.get(index).copied().unwrap_or(0.0)
                + KERNEL[2] * values.get(after).copied().unwrap_or(0.0)
        })
        .collect()
}

/// What kind of peak this is.
///
/// **Derived from the scene and the interaction, not predicted by a head.** Section 6.2
/// says the four named kinds are "trained as explicit peak types"; there is no such head
/// in this build and there is no labelled data to train one on, so the four are inferred
/// from evidence that already exists. That is condition C5 of the phase 10 exit report.
///
/// The derivation is deliberately conservative - both the scene *and* the interaction
/// have to agree, except for a tear release which is a face reading - so a wrong kind
/// needs two independent things to be wrong. A moment that satisfies none of the four is
/// [`PeakKind::Expression`], which claims only that something peaked.
#[must_use]
pub fn kind_of(frame: &Frame, resolved: bool) -> PeakKind {
    if !resolved {
        return PeakKind::Flat;
    }
    let has = |wanted: Interaction| frame.interactions.iter().any(|(kind, _)| *kind == wanted);

    if frame.scene == SceneId::Kiss && has(Interaction::Kiss) {
        return PeakKind::KissApex;
    }
    if frame.scene == SceneId::Rings && has(Interaction::RingExchange) {
        return PeakKind::RingSlide;
    }
    // A tear release is the one kind that is a face reading rather than an interaction,
    // and it is gated on `TEARS_CERTAIN` for section 12's fourth failure mode: a moment
    // labelled "tear release" that was not one is the single most embarrassing thing
    // this phase could put in front of a photographer.
    if frame.faces.iter().any(FaceExpression::reads_as_crying) {
        return PeakKind::TearRelease;
    }
    // The bouquet. There is no bouquet detector, so the evidence is the scene plus a
    // group reacting - which is what a toss looks like from the front and is as far as
    // this build can honestly go.
    if matches!(frame.scene, SceneId::Exit | SceneId::Candid) && has(Interaction::GroupCheer) {
        return PeakKind::BouquetInAir;
    }
    PeakKind::Expression
}

/// Assemble one moment's peak.
///
/// `frames` must be in the moment's timeline order, which is the order
/// `Moment::image_ids` is in.
#[must_use]
pub fn peak_of(moment: MomentId, frames: &[Frame]) -> Option<(MomentPeak, Curve)> {
    let built = curve(frames);
    let winner = frames.get(built.index)?;
    let resolved = built.margin >= MomentPeak::MIN_MARGIN;
    let kind = kind_of(winner, resolved);

    let mut reasons = Vec::with_capacity(MomentPeak::MAX_REASONS);
    if resolved {
        reasons.push(EmotionReason::frame(
            EmotionCode::PeakFrame,
            format!(
                "of the {} frames in this moment, this one is the strongest by {:.0}%",
                frames.len(),
                built.margin * 100.0
            ),
            built.margin,
        ));
    } else {
        reasons.push(EmotionReason::frame(
            EmotionCode::Unremarkable,
            format!(
                "the {} frames in this moment are within {:.0}% of each other, so none of them is \
                 the one",
                frames.len(),
                built.margin * 100.0
            ),
            0.0,
        ));
    }
    if let Some((interaction, strength)) = winner.interactions.first() {
        reasons.push(EmotionReason::frame(
            EmotionCode::InteractionDetected,
            format!("{} here, at {:.0}%", interaction.title(), strength * 100.0),
            *strength,
        ));
    }
    if let Some(face) = winner
        .faces
        .iter()
        .max_by(|left, right| left.intensity().total_cmp(&right.intensity()))
    {
        if face.intensity() > 0.0 {
            reasons.push(EmotionReason::at(
                lead_code(face),
                lead_code(face).user_text(),
                face.intensity(),
                face.crop,
            ));
        }
    }
    reasons.truncate(MomentPeak::MAX_REASONS);

    // The confidence is the margin and the winning frame's face confidence together, not
    // the margin alone: a peak that wins clearly on readings nobody is sure about is not
    // a confident peak. Geometric so that either being near zero drags it down, which is
    // the same argument phase 09's composite makes.
    let readings = winner
        .faces
        .iter()
        .map(|face| face.confidence)
        .fold(0.0f32, f32::max)
        .max(
            winner
                .interactions
                .first()
                .map_or(0.0, |(_, strength)| *strength),
        );
    let confidence = (built.margin.max(0.01) * readings.max(0.01))
        .sqrt()
        .clamp(0.0, 1.0);

    Some((
        MomentPeak {
            moment_id: moment,
            image_id: winner.image,
            index: u32::try_from(built.index).unwrap_or(0),
            frames: u32::try_from(frames.len()).unwrap_or(0),
            margin: built.margin,
            kind,
            confidence,
            reasons,
            user_chosen: false,
        },
        built,
    ))
}

/// The reason code one face's strongest channel deserves.
///
/// Used by the peak reasons and by [`crate::emotion::score`], written once here so the
/// two never disagree about what a face at `laughter 0.8` is called.
#[must_use]
pub fn lead_code(face: &FaceExpression) -> EmotionCode {
    if face.reads_as_crying() {
        return EmotionCode::Tears;
    }
    let mut best = (EmotionCode::Unremarkable, 0.0f32);
    for (code, value) in [
        (EmotionCode::Laughter, face.laughter),
        (EmotionCode::Tenderness, face.tenderness),
        (EmotionCode::Surprise, face.surprise),
        (EmotionCode::Composure, face.composed),
    ] {
        if value > best.1 {
            best = (code, value);
        }
    }
    if face.smile > best.1 {
        best = (
            if face.posed_smile() {
                EmotionCode::PosedSmile
            } else {
                EmotionCode::GenuineSmile
            },
            face.smile,
        );
    }
    best.0
}
