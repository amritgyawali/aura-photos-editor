//! Whether the eyes that matter are open, and whether it matters that they are not.
//!
//! PHASE-09 section 6.4, and the part of the phase that decides whether a photographer
//! keeps using the product. Section 1 says it plainly: "closed eyes during a kiss, a
//! prayer, a first-look reaction or a tearful hug are *good* photographs. A naive
//! eye-open detector destroys exactly the frames that sell weddings."
//!
//! So this module answers three questions in order, and the order is the argument:
//!
//! 1. **What are the eyes doing?** Five classes from a learned head - open, squint,
//!    closed, looking down, occluded. `LookingDown` exists as its own class because it
//!    looks like `Closed` to a classifier and means the opposite to a photographer.
//! 2. **Does this face gate the frame?** Only important subjects' eyes decide anything.
//!    A guest blinking in row four is not a defect; the bride blinking in a portrait is.
//!    Importance comes from phase 06 - this crate reads it and may not compute it.
//! 3. **Is the closure intentional?** Four rules, each of which a working photographer
//!    would recognise, applied only to faces that got past (2).
//!
//! ## The four intent rules, verbatim from section 6.4
//!
//! > closed eyes are acceptable when (scene in {kiss, vows, ritual, hug, first_look,
//! > speeches_emotional}) OR (mouth-smile strong and head tilted) OR (tears detected in
//! > Phase 10) OR (both partners' eyes closed simultaneously in a couple frame).
//!
//! Three of the four are implemented here. **The tears rule is phase 10's** and this
//! build cannot evaluate it: [`IntentInput::tears`] exists, is documented, is wired
//! through, and is always false until phase 10 fills it. That is condition C4 in the
//! phase 09 exit report, and the degradation is in the safe direction - a missing
//! exoneration makes a frame *look* more defective, so the failure is visible in a
//! review queue rather than silent in a delivery.
//!
//! ## Why the head does not decide intent
//!
//! Every one of the four rules depends on something outside the crop: the scene, the
//! partner's eyes, the expression in phase 10's terms. A model given the crop alone and
//! asked "was this deliberate" would be guessing with authority, and the phase document
//! puts the rules in front of a PM and a working photographer for sign-off (section 9,
//! `PM`) precisely because they are a *policy* rather than a measurement.

use std::sync::Arc;

use aura_core::contract::integrity::{CropRect, EyeOpenness, EyeState};
use aura_core::contract::people::FaceRef;
use aura_core::{AuraResult, Priority, SceneId};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{EYE_CLASSES, EYE_CROP_SIDE, EYE_STATE_MODEL};

/// Registry name of the eye-state head.
pub const EYE_MODEL: &str = EYE_STATE_MODEL;

/// Pinned version of the eye-state head.
pub const EYE_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The version stamped on every `face_eye_state.model_ver`.
pub const EYE_VER: u16 =
    EYE_MODEL_VERSION.major * 100 + EYE_MODEL_VERSION.minor * 10 + EYE_MODEL_VERSION.patch;

/// How prominent a face must be before its eyes gate the frame.
///
/// 0.18 of the frame's total prominence. Set from the shape of a wedding photograph
/// rather than from a sweep: a couple portrait puts 0.4 to 0.5 on each of two faces, a
/// family group puts 0.1 to 0.2 on each of six, and a reception wide shot puts under
/// 0.05 on each of twenty. The threshold is deliberately below the family-group figure,
/// because a closed-eyed uncle in a formal group *is* a defect, and comfortably above
/// the wide-shot figure, because a blinking guest in a crowd is not.
pub const GATING_PROMINENCE: f32 = 0.18;

/// How large a face must be before its eyes gate the frame, as a fraction of the frame.
///
/// A second, independent gate at 1.5 %. Prominence is a phase 06 number that needs an
/// identity assignment behind it; area needs nothing. A wedding with no face clustering
/// yet would otherwise have no gating faces at all and would report every frame as
/// eye-clean, which is the "reports nothing wrong because it looked at nothing" failure
/// that `AURA-ML-5035` exists to make visible elsewhere.
pub const GATING_AREA: f32 = 0.015;

/// How sure the head must be before a `Closed` reading is acted on.
///
/// 0.55, and low on purpose in one direction only: below it the state is recorded with
/// its confidence and **no flag is raised**. An uncertain closed-eye reading must not
/// produce a defect, because section 12's first failure mode is false rejection. An
/// uncertain *open* reading is harmless and needs no threshold.
pub const ACT_ON_CLOSED: f32 = 0.55;

/// The scenes in which closed eyes are the photograph.
///
/// Section 6.4's list, mapped onto phase 07's 22-scene vocabulary. Where a name in the
/// phase document has no exact scene - `kiss`, `hug`, `first_look` - the mapping is to
/// the scene that contains that moment, and the mapping is written here rather than
/// inferred so that a person adding a scene can see what they are joining.
#[must_use]
pub fn scene_permits_closed_eyes(scene: SceneId) -> bool {
    matches!(
        scene,
        // `kiss` and `vows` are scenes in their own right.
        SceneId::Kiss
            | SceneId::Vows
            // `ritual` is section 6.4's word and phase 07's scene, and it is the one
            // that carries a prayer - the case section 1 names.
            | SceneId::Ritual
            | SceneId::Ceremony
            // `rings` is the ring exchange, which sits inside the same held breath as
            // the vows and is separated from them only by what the hands are doing.
            | SceneId::Rings
            // `first_look` is a scene; `hug` is not, and a first look is where the hug
            // section 6.4 means actually happens.
            | SceneId::FirstLook
            // `speeches_emotional` has no separate scene in phase 07's vocabulary. The
            // whole of `Speeches` is admitted rather than none of it, because the
            // failure of admitting too much here is a missed blink and the failure of
            // admitting too little is a deleted photograph of somebody crying at a
            // toast.
            | SceneId::Speeches
            // Not in section 6.4's list, added with the same argument: a first dance is
            // a forehead-to-forehead scene and eyes are shut in half of the good ones.
            | SceneId::FirstDance
    )
}

/// Everything outside the crop that the intent rules need.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct IntentInput {
    /// The scene phase 07 classified, for rule 1.
    pub scene: SceneId,
    /// True when the mouth is smiling strongly and the head is tilted, for rule 2.
    ///
    /// Measured here from the face geometry rather than taken from phase 10: a tilted
    /// head with a wide smile is a laugh, and a laugh closes eyes. See
    /// [`laughing_from_geometry`].
    pub laughing: bool,
    /// True when phase 10 detected tears, for rule 3.
    ///
    /// **Always false in this build.** Phase 10 does not exist yet; the field is here so
    /// that wiring it is a one-line change rather than a redesign, and so that the rule
    /// it serves is visible in the code a photographer's sign-off covers.
    pub tears: bool,
    /// True when this face is one of a couple and the other partner's eyes are also
    /// closed, for rule 4.
    pub partner_also_closed: bool,
}

/// Which of the four rules justified a closure.
///
/// Returned rather than a bare boolean because the panel shows the photographer *why* a
/// closed-eye frame was not marked, and "the scene is a first kiss" is a different
/// sentence from "you were both mid-laugh".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntentRule {
    /// Nothing justified it.
    #[default]
    None,
    /// Rule 1: the scene is one where closed eyes are the photograph.
    Scene,
    /// Rule 2: a strong smile with a tilted head.
    Laughing,
    /// Rule 3: phase 10 detected tears.
    Tears,
    /// Rule 4: both partners' eyes closed at once.
    BothPartners,
}

impl IntentRule {
    /// True when this rule justifies the closure.
    #[must_use]
    pub const fn justifies(self) -> bool {
        !matches!(self, Self::None)
    }

    /// The sentence the panel shows.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::None => "nothing in the scene or the expression explains the closed eyes",
            Self::Scene => "closed eyes belong to this moment, so this is not marked as a fault",
            Self::Laughing => "a wide smile with a tilted head: this is a laugh, not a blink",
            Self::Tears => "there are tears here, so the closed eyes are part of the photograph",
            Self::BothPartners => {
                "both of them have their eyes closed at the same time, which is a moment rather \
                 than a mistake"
            }
        }
    }
}

/// Apply the four rules, in section 6.4's order.
#[must_use]
pub fn intent(input: IntentInput) -> IntentRule {
    if scene_permits_closed_eyes(input.scene) {
        IntentRule::Scene
    } else if input.laughing {
        IntentRule::Laughing
    } else if input.tears {
        IntentRule::Tears
    } else if input.partner_also_closed {
        IntentRule::BothPartners
    } else {
        IntentRule::None
    }
}

/// Rule 2's measurement: is this face mid-laugh.
///
/// A laugh is a tilted head with a wide smile, and phase 06 gives us the geometry for
/// the first half exactly - the two eye landmarks' relative height *is* the head roll.
/// The second half is not on the contract, so this build uses the eye separation against
/// the face width as a weak proxy for an open, wide expression, and requires a real
/// tilt before it will say anything at all.
///
/// **This is the weakest rule of the four and it is deliberately the hardest to
/// satisfy**: an over-eager laugh rule would exonerate genuine blinks, and the safe
/// direction for a *trust* feature is to under-claim. When phase 10 lands, its
/// expression head replaces the second half of this function and the tilt stays.
#[must_use]
pub fn laughing_from_geometry(face: &FaceRef) -> bool {
    if !face.has_eyes() || face.bbox.w <= f32::EPSILON {
        return false;
    }
    let left = face.eyes[0];
    let right = face.eyes[1];
    let dx = right[0] - left[0];
    let dy = right[1] - left[1];
    if dx.abs() <= f32::EPSILON {
        return false;
    }
    let roll_degrees = (dy / dx).atan().to_degrees().abs();
    // Twelve degrees. A level head at a wedding is within four or five; anything past
    // twelve is somebody leaning into somebody else, which is the posture the rule is
    // about.
    let tilted = roll_degrees >= 12.0;
    // Eye separation over face width. A face turned away compresses it; a face looking
    // straight ahead with a wide expression sits above 0.36.
    let separation = dx.hypot(dy) / face.bbox.w;
    tilted && separation >= 0.36
}

/// The eye-state head.
#[derive(Debug, Clone)]
pub struct EyeStateModel {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl EyeStateModel {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(EYE_MODEL, EYE_MODEL_VERSION),
        }
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// Load the head before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Classify a batch of aligned crops.
    ///
    /// One call rather than one per face, for the reason `FaceEmbedder::run_batch`
    /// gives: a group photograph has thirty faces and thirty separate calls would throw
    /// away phase 03's memory ledger and batch scheduler.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest,
    /// `AURA-ML-5011` when the work was cancelled, and whatever the loader raised.
    pub fn run_batch(
        &self,
        crops: &[Vec<f32>],
        prio: Priority,
    ) -> AuraResult<Vec<(EyeOpenness, f32)>> {
        if crops.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(crops.len());
        for crop in crops {
            inputs.push(vec![TensorView::new(
                vec![1, 3, EYE_CROP_SIDE, EYE_CROP_SIDE],
                crop,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            let Some(tensor) = result.outputs.first() else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "one eye-state tensor",
                    "no outputs",
                ));
            };
            out.push(decode(&tensor.data));
        }
        Ok(out)
    }
}

/// Turn the head's five probabilities into a state and a confidence.
///
/// The confidence is the winning probability rather than the top-1 margin, because the
/// five classes are not equally confusable: `Open` against `Squint` is a continuum and a
/// narrow margin between them is normal and harmless, whereas `Open` against `Closed` is
/// a genuine disagreement. A margin would report the first as uncertainty and the second
/// as confidence, which is backwards for the decision that follows.
#[must_use]
pub fn decode(probabilities: &[f32]) -> (EyeOpenness, f32) {
    let mut best = (EyeOpenness::Open, 0.0f32);
    for (index, state) in EyeOpenness::ALL.into_iter().enumerate() {
        let value = probabilities.get(index).copied().unwrap_or(0.0);
        if value > best.1 {
            best = (state, value);
        }
    }
    if probabilities.len() < EYE_CLASSES {
        // A head that returned fewer classes than the contract says is a manifest
        // disagreement, and the safe reading of a half-tensor is "open": it raises no
        // flag and lowers no score.
        return (EyeOpenness::Open, 0.0);
    }
    best
}

/// True when this face's eyes decide anything about the frame.
///
/// Either gate is enough. See [`GATING_PROMINENCE`] and [`GATING_AREA`] for why there
/// are two.
#[must_use]
pub fn gates(face: &FaceRef, prominence: f32) -> bool {
    prominence >= GATING_PROMINENCE || face.area_frac >= GATING_AREA
}

/// Assemble one face's [`EyeState`].
#[must_use]
pub fn state_for(
    face: &FaceRef,
    reading: (EyeOpenness, f32),
    prominence: f32,
    intent_input: IntentInput,
) -> (EyeState, IntentRule) {
    let (state, confidence) = reading;
    let gating = gates(face, prominence);
    // Intent is only asked about a closed reading that is both confident enough to act
    // on and important enough to matter. Asking it about every face would put a
    // "closed eyes belong to this moment" sentence on frames where nobody's eyes are
    // closed.
    let rule = if state.is_closed() && confidence >= ACT_ON_CLOSED && gating {
        intent(intent_input)
    } else {
        IntentRule::None
    };
    (
        EyeState {
            face_id: face.face_id,
            identity: face.identity_id,
            state,
            confidence,
            intentional: rule.justifies(),
            // A closed reading below `ACT_ON_CLOSED` is recorded and does not gate. The
            // state is still stored, because a support engineer arguing about a missed
            // blink needs to see that the head *did* say closed and was not believed.
            gates: gating && !(state.is_closed() && confidence < ACT_ON_CLOSED),
            area_frac: face.area_frac,
            crop: eye_crop(face),
        },
        rule,
    )
}

/// The rectangle the panel zooms to for an eye reason.
///
/// The eye landmarks' bounding box grown by 140 %, or the face box when there are no
/// landmarks. Section 13's last criterion is that the card shows the exact crop that
/// caused each penalty, and for a blink that crop is a pair of eyes with enough face
/// around them to be recognisable - an eye at 100 % zoom with no brow or cheek in the
/// frame is unreadable.
#[must_use]
pub fn eye_crop(face: &FaceRef) -> CropRect {
    if !face.has_eyes() {
        return face.bbox.expanded(0.15);
    }
    let (left, right) = (face.eyes[0], face.eyes[1]);
    let x0 = left[0].min(right[0]);
    let x1 = left[0].max(right[0]);
    let y0 = left[1].min(right[1]);
    let y1 = left[1].max(right[1]);
    CropRect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.01),
        h: (y1 - y0).max(0.01),
    }
    .expanded(1.4)
}

/// How many of the gating subjects have their eyes closed.
///
/// Section 6.4's group path: "count how many primary/secondary subjects have closed eyes
/// and expose `closed_eye_ratio` for Phase 12's group rules."
///
/// Returns `(ratio, gating_faces)` and the denominator is returned beside the ratio on
/// purpose - a ratio of 0.0 over six gating faces is a clean group photograph, and a
/// ratio of 0.0 over zero gating faces is a frame nobody's eyes were judged in. Storing
/// only the ratio would make those two indistinguishable, which is the "not checked is
/// not clean" rule that migration 9 states as its fifth property.
///
/// **Intentional closures count towards the ratio.** A frame where all six people have
/// their eyes shut during a blessing is genuinely a frame where all six have their eyes
/// shut, and phase 12's group rule is entitled to know that; whether it is a *defect* is
/// what [`EyeState::intentional`] answers, separately and per face.
#[must_use]
pub fn closed_ratio(states: &[EyeState]) -> (f32, u32) {
    let gating: Vec<&EyeState> = states.iter().filter(|state| state.gates).collect();
    if gating.is_empty() {
        return (0.0, 0);
    }
    let closed = gating
        .iter()
        .filter(|state| state.state.is_closed())
        .count();
    let total = u32::try_from(gating.len()).unwrap_or(u32::MAX);
    (
        f64::from(u32::try_from(closed).unwrap_or(0)) as f32 / f64::from(total) as f32,
        total,
    )
}

/// Whether a couple frame has both partners' eyes closed, for rule 4.
///
/// `couple` is the identities phase 06 calls primary. Two faces, both closed, both
/// belonging to that set - which is the case the rule is about and is why it takes the
/// whole frame's readings rather than one face's.
#[must_use]
pub fn both_partners_closed(
    readings: &[(FaceRef, EyeOpenness)],
    couple: &[aura_core::IdentityId],
) -> bool {
    if couple.len() < 2 {
        return false;
    }
    let closed_partners = readings
        .iter()
        .filter(|(face, state)| {
            state.is_closed()
                && face
                    .identity_id
                    .is_some_and(|identity| couple.contains(&identity))
        })
        .count();
    closed_partners >= 2
}

// ---------------------------------------------------------------------------
// Getting the crop the head reads
// ---------------------------------------------------------------------------

/// Where the left eye sits in the canonical 112 px crop, in pixels.
///
/// `ArcFace`'s reference position, and the same one `aura_vision::face::align` warps to.
/// Two crops of one face produced by two different canonical layouts are two different
/// inputs, and a head trained on one would be being asked about the other.
pub const CANONICAL_LEFT_EYE: [f32; 2] = [38.29, 51.69];

/// Where the right eye sits in the canonical crop.
pub const CANONICAL_RIGHT_EYE: [f32; 2] = [73.53, 51.50];

/// Warp a face into the canonical 112 px crop using the two eye landmarks.
///
/// **A two-point similarity, not phase 06's five-point one, and the difference is worth
/// stating.** `aura_vision::face::align::alignment_for` solves a least-squares fit over
/// five landmarks - two eyes, the nose and two mouth corners - which is more robust to
/// one bad landmark and is what the *recogniser* needs. Phase 09 has two points, because
/// two points is what the `FaceRef` amendment in ADR-0019 section 3 added and the nose
/// and mouth corners are the half of the landmark set that makes a face identifiable.
///
/// Two points determine a similarity transform exactly (scale, rotation and
/// translation), which is all the alignment an eye-state question needs: it puts the eyes
/// on a horizontal line at a known separation, which is precisely the normalisation that
/// makes a head tilted into a hug comparable with a head held level.
///
/// What is lost is robustness to a single mis-detected eye. A face whose landmarks are
/// wrong produces a crop that is wrong, and the head then answers about the wrong pixels.
/// The mitigation is [`FaceRef::has_eyes`] and the quality gate phase 06 already applies
/// before a face reaches here.
#[must_use]
pub fn align_from_eyes(rgb: &[u8], width: usize, height: usize, face: &FaceRef) -> Vec<u8> {
    let side = EYE_CROP_SIDE;
    let mut out = vec![0u8; side * side * 3];
    if width == 0 || height == 0 || !face.has_eyes() {
        return out;
    }

    // Source eye positions in pixels.
    let sx0 = face.eyes[0][0] * width as f32;
    let sy0 = face.eyes[0][1] * height as f32;
    let sx1 = face.eyes[1][0] * width as f32;
    let sy1 = face.eyes[1][1] * height as f32;

    let from_x = sx1 - sx0;
    let from_y = sy1 - sy0;
    let source_span = from_x.hypot(from_y);
    if source_span <= f32::EPSILON {
        return out;
    }
    let onto_x = CANONICAL_RIGHT_EYE[0] - CANONICAL_LEFT_EYE[0];
    let onto_y = CANONICAL_RIGHT_EYE[1] - CANONICAL_LEFT_EYE[1];
    let target_span = onto_x.hypot(onto_y);

    // The inverse transform: for each destination pixel, where in the source it came
    // from. Inverse rather than forward, because a forward warp leaves holes.
    let scale = source_span / target_span;
    let angle = from_y.atan2(from_x) - onto_y.atan2(onto_x);
    let (sin, cos) = angle.sin_cos();
    let a = cos * scale;
    let b = -sin * scale;

    for row in 0..side {
        for column in 0..side {
            let dx = column as f32 - CANONICAL_LEFT_EYE[0];
            let dy = row as f32 - CANONICAL_LEFT_EYE[1];
            let sx = a.mul_add(dx, b * dy) + sx0;
            let sy = (-b).mul_add(dx, a * dy) + sy0;
            let sample = sample_bilinear(rgb, width, height, sx, sy);
            let base = (row * side + column) * 3;
            for (channel, value) in sample.into_iter().enumerate() {
                if let Some(slot) = out.get_mut(base + channel) {
                    *slot = value;
                }
            }
        }
    }
    out
}

/// Bilinear sample of an interleaved 8-bit sRGB buffer, clamped at the edges.
///
/// Bilinear rather than the box filter phase 05 uses elsewhere, and the reason is the
/// direction of the resample: an aligned face crop is usually an *upscale* of a 90 px
/// face to 112 px, and a box filter on an upscale is nearest-neighbour with extra steps.
fn sample_bilinear(rgb: &[u8], width: usize, height: usize, x: f32, y: f32) -> [u8; 3] {
    let cx = x.clamp(0.0, (width - 1) as f32);
    let cy = y.clamp(0.0, (height - 1) as f32);
    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = cx - x0 as f32;
    let fy = cy - y0 as f32;

    let mut out = [0u8; 3];
    for (channel, slot) in out.iter_mut().enumerate() {
        let at = |px: usize, py: usize| -> f32 {
            f32::from(rgb.get((py * width + px) * 3 + channel).copied().unwrap_or(0))
        };
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        *slot = (top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Turn one aligned crop into the head's input tensor.
///
/// NCHW, `0..1`, sRGB, with nothing subtracted - the same three steps and the same
/// absence of parameters as `aura_vision::face::embed::preprocess_crop`, because the
/// manifest declares `range: 0..1` and a normalisation the manifest does not describe is
/// drift waiting to happen.
#[must_use]
pub fn preprocess_crop(crop: &[u8]) -> Vec<f32> {
    let side = EYE_CROP_SIDE;
    let mut out = vec![0.0f32; 3 * side * side];
    for channel in 0..3 {
        for row in 0..side {
            for column in 0..side {
                let source = (row * side + column) * 3 + channel;
                let destination = channel * side * side + row * side + column;
                if let Some(slot) = out.get_mut(destination) {
                    *slot = f32::from(crop.get(source).copied().unwrap_or(0)) / 255.0;
                }
            }
        }
    }
    out
}
