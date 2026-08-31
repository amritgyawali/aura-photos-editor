//! What a correction does to a frame, how far two bodies are apart, and the port phase 25 reads.
//!
//! Three things live here because all three are statements about the *same* nine numbers, and
//! keeping them apart would be three places to get the sign wrong:
//!
//! * [`apply`] folds a transform into a [`CameraFrame`]'s readings - what the solver does to a
//!   candidate before it measures it;
//! * [`measure`] turns a set of pairs into an [`AppearanceDistance`] - the objective;
//! * [`Field`] is what phase 25 asks "which correction applies to this photograph", which is
//!   section 6.4's ordering expressed as a data dependency rather than as a convention.
//!
//! ## Additive axes and multiplicative axes are not the same kind of number
//!
//! Temperature, tint, exposure and saturation are **offsets**: composing two of them is a sum, and
//! the identity is zero. Channel gain and contrast shape are **ratios**: composing two of them is a
//! product, and the identity is one. Getting the two confused is invisible in a diff and obvious in
//! a gallery - subtracting two gains near one produces a number near zero, which reads as "no
//! correction" and means "multiply everything by nothing". Every function in this module keeps them
//! apart explicitly and [`Departure::to`][t] is the one place the distinction is written down.
//!
//! [t]: super::baseline::Departure::to
//!
//! ## The distance is measured on pairs, never on a frame
//!
//! [`measure`] takes matched pairs and reduces each one to four differences between its two frames.
//! A per-frame distance would be a distance to nothing: there is no absolute against which a Sony
//! frame is "2 dE00 wrong", only a Canon frame of the same room that says so. That is why section
//! 6.2 states the objective over pairs and why a body with no verified pairs cannot be solved at
//! all - it is a missing measuring stick rather than a missing computation.

use std::collections::BTreeMap;

use aura_core::contract::camera::{
    AppearanceDistance, CameraMatchService, CameraTransform, FlashState,
};
use aura_core::contract::gallery::ImageId;
use aura_core::contract::moment::CameraId;
use aura_core::{AuraResult, ProjectId};

use crate::skin_consistency;
use crate::tree::Frame;

use super::fingerprint::CameraFrame;

/// Apply a transform to one frame's readings, in place.
///
/// The frame's own solved answers move; nothing about the photograph does. What comes out is what
/// phase 15 and phase 16 *would have decided* had this body rendered like the reference, which is
/// the input phase 25's tree is then built over.
///
/// Skin is corrected last and separately, because a skin correction is a residual on top of the
/// white-balance move rather than an alternative to it: correcting the illuminant already moves
/// skin, and applying both as though they were independent would double the part they share.
pub fn apply(transform: &CameraTransform, frame: &mut CameraFrame) {
    if !transform.enabled {
        return;
    }
    if let Some(cct) = frame.cct_k.as_mut() {
        // Additive in kelvin and then floored, because a temperature below the locus is not a
        // temperature. The floor is phase 15's own domain rather than a number chosen here.
        *cct = (*cct + transform.d_cct).clamp(2000.0, 50_000.0);
    }
    if let Some(tint) = frame.tint.as_mut() {
        *tint = (*tint + transform.d_tint).clamp(-150.0, 150.0);
    }
    if let Some(ev) = frame.exposure_ev.as_mut() {
        *ev += transform.d_exposure;
    }
    if let Some(luma) = frame.subject_luma.as_mut() {
        // A stop is a doubling. An exposure offset of +0.5 EV multiplies subject luminance by
        // 2^0.5, which is the relation phase 15 solves its own exposure against - not an addition,
        // which would make a +0.3 EV correction brighten a shadow as much as a highlight.
        *luma = (*luma * (transform.d_exposure).exp2()).clamp(0.0, 1.0);
    }
    if let Some(saturation) = frame.saturation.as_mut() {
        *saturation = (*saturation + transform.d_saturation).clamp(-100.0, 100.0);
    }
    if let Some(contrast) = frame.contrast.as_mut() {
        // The mid-tone multiplier is what a single contrast number responds to; the shadow and
        // highlight terms reach the frame through the grade signature below, which is what the
        // objective's third term reads.
        let mid = transform.contrast_shape.get(1).copied().unwrap_or(1.0);
        *contrast = (*contrast * mid).clamp(-100.0, 100.0);
    }
    if let Some(signature) = frame.signature.as_mut() {
        apply_to_signature(transform, signature);
    }
    if let Some(white) = frame.white_uv.as_mut() {
        *white = shift_uv(*white, transform.d_cct, transform.d_tint);
    }
    if let Some(skin) = frame.skin_uv.as_mut() {
        let moved = shift_uv(*skin, transform.d_cct, transform.d_tint);
        *skin = [
            moved.first().copied().unwrap_or(0.0) + transform.skin_correction.d_uv[0],
            moved.get(1).copied().unwrap_or(0.0) + transform.skin_correction.d_uv[1],
        ];
    }
    if let Some(luma) = frame.skin_luma.as_mut() {
        *luma = (*luma * transform.d_exposure.exp2() + transform.skin_correction.d_luma)
            .clamp(0.0, 1.0);
    }
}

/// Apply a transform to a phase 25 gallery frame, in place.
///
/// **This is section 6.4's ordering.** `crate::api::collect_frames` calls it while assembling the
/// consistency pass's input, so every node, every change point, every anchor and every target in
/// phase 25 is computed over already-comparable numbers. Reversing the two produces a gallery in
/// which each node's target is the average of two brands' colour science and every frame is
/// normalised toward a look neither camera can produce.
///
/// A disabled transform moves nothing, which is the per-body kill switch reaching the one place it
/// has to reach.
pub fn apply_to_gallery(transform: &CameraTransform, frame: &mut Frame) {
    if !transform.enabled {
        return;
    }
    if let Some(cct) = frame.cct_k.as_mut() {
        *cct = (*cct + transform.d_cct).clamp(2000.0, 50_000.0);
    }
    if let Some(tint) = frame.tint.as_mut() {
        *tint = (*tint + transform.d_tint).clamp(-150.0, 150.0);
    }
    if let Some(ev) = frame.exposure_ev.as_mut() {
        *ev += transform.d_exposure;
    }
    if let Some(luma) = frame.subject_luma.as_mut() {
        *luma = (*luma * transform.d_exposure.exp2()).clamp(0.0, 1.0);
    }
    if let Some(saturation) = frame.saturation.as_mut() {
        *saturation = (*saturation + transform.d_saturation).clamp(-100.0, 100.0);
    }
    if let Some(contrast) = frame.contrast.as_mut() {
        let mid = transform.contrast_shape.get(1).copied().unwrap_or(1.0);
        *contrast = (*contrast * mid).clamp(-100.0, 100.0);
    }
    if let Some(signature) = frame.signature.as_mut() {
        apply_to_signature(transform, signature);
    }
}

/// Move an eight-number grade signature the way a transform moves a body's colour character.
///
/// The signature's layout is phase 25's: shadow hue and spread, highlight hue and spread, shadow
/// and highlight chroma, mid-tone slope, black point. A camera transform touches the two chromas
/// through saturation, the slope through the mid contrast multiplier and the black point through
/// the shadow multiplier; the four hue terms move with tint, which is the axis a hue shift shows up
/// on when the cause is a body's matrix rather than a photographer's grade.
fn apply_to_signature(transform: &CameraTransform, signature: &mut [f32; 8]) {
    let hue_shift = transform.d_tint / 400.0;
    let sat_scale = 1.0 + transform.d_saturation / 100.0;
    let shadow = transform.contrast_shape.first().copied().unwrap_or(1.0);
    let mid = transform.contrast_shape.get(1).copied().unwrap_or(1.0);
    if let Some(v) = signature.get_mut(0) {
        *v = (*v + hue_shift).clamp(-1.0, 1.0);
    }
    if let Some(v) = signature.get_mut(2) {
        *v = (*v + hue_shift).clamp(-1.0, 1.0);
    }
    if let Some(v) = signature.get_mut(4) {
        *v = (*v * sat_scale).clamp(0.0, 1.0);
    }
    if let Some(v) = signature.get_mut(5) {
        *v = (*v * sat_scale).clamp(0.0, 1.0);
    }
    if let Some(v) = signature.get_mut(6) {
        *v = (*v * mid).clamp(-1.0, 1.0);
    }
    if let Some(v) = signature.get_mut(7) {
        *v = (*v * shadow).clamp(0.0, 1.0);
    }
}

/// Move a chromaticity by a temperature and a tint offset, in CIE 1976 `u'v'`.
///
/// **In `u'v'` rather than by interpolating a colour temperature**, which is phase 15's own
/// correction and its hardest-won lesson: interpolating a temperature walks along the Planckian
/// locus, so it can never reach an off-locus light - and the light a wedding is actually shot under
/// is very often off it. The temperature term moves along the locus and the tint term moves
/// perpendicular to it, which is what the two axes mean.
#[must_use]
pub fn shift_uv(uv: [f32; 2], d_cct: f32, d_tint: f32) -> [f32; 2] {
    use aura_raw::colour::illuminant::{cct_from_uv, cct_to_uv};
    let base = cct_from_uv(uv);
    if !base.is_finite() || base <= 0.0 {
        return uv;
    }
    let moved = cct_to_uv((base + d_cct).clamp(2000.0, 50_000.0));
    // The residual is whatever the frame was off the locus by; it survives the temperature move
    // untouched, which is the whole point of doing this in `u'v'`.
    let residual = [
        uv.first().copied().unwrap_or(0.0) - cct_to_uv(base).first().copied().unwrap_or(0.0),
        uv.get(1).copied().unwrap_or(0.0) - cct_to_uv(base).get(1).copied().unwrap_or(0.0),
    ];
    // Four tint units is 0.002 in v', which is phase 15's own equivalence read backwards.
    let tint = d_tint * 0.0005;
    [
        (moved.first().copied().unwrap_or(0.0) + residual[0]).clamp(0.0, 1.0),
        (moved.get(1).copied().unwrap_or(0.0) + residual[1] + tint).clamp(0.0, 1.0),
    ]
}

/// One pair of frames, reduced to what the objective reads.
///
/// The solver works on these rather than on `CameraFrame`s, because a fit that walked the frames
/// would re-derive the same four differences at every step of every axis - nine parameters times
/// twenty steps times a hundred and sixty pairs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairReading {
    /// The reference frame's skin chromaticity and luminance.
    pub left_skin: Option<([f32; 2], f32)>,
    /// The other frame's, before any correction.
    pub right_skin: Option<([f32; 2], f32)>,
    /// The reference frame's illuminant chromaticity.
    pub left_white: [f32; 2],
    /// The other frame's.
    pub right_white: [f32; 2],
    /// The reference frame's temperature, in kelvin.
    pub left_cct: f32,
    /// The other frame's.
    pub right_cct: f32,
    /// The reference frame's grade signature.
    pub left_signature: [f32; 8],
    /// The other frame's.
    pub right_signature: [f32; 8],
    /// The reference frame's contrast, in the recipe's units.
    pub left_contrast: f32,
    /// The other frame's.
    pub right_contrast: f32,
    /// The reference frame's subject luminance.
    pub left_luma: f32,
    /// The other frame's.
    pub right_luma: f32,
}

impl PairReading {
    /// Reduce two frames to a reading, or `None` when either lacks what the objective needs.
    #[must_use]
    pub fn of(left: &CameraFrame, right: &CameraFrame) -> Option<Self> {
        Some(Self {
            left_skin: left.skin_uv.zip(left.skin_luma),
            right_skin: right.skin_uv.zip(right.skin_luma),
            left_white: left.white_uv?,
            right_white: right.white_uv?,
            left_cct: left.cct_k?,
            right_cct: right.cct_k?,
            left_signature: left.signature.unwrap_or([0.0; 8]),
            right_signature: right.signature.unwrap_or([0.0; 8]),
            left_contrast: left.contrast.unwrap_or(0.0),
            right_contrast: right.contrast.unwrap_or(0.0),
            left_luma: left.subject_luma.unwrap_or(0.5),
            right_luma: right.subject_luma.unwrap_or(0.5),
        })
    }
}

/// How far apart two bodies look across a set of pairs, optionally after a transform.
///
/// Section 6.2's objective. Every term is a **mean over the pairs** rather than a median, and that
/// is the one place in this phase where a mean is right: the pairs have already been through
/// background verification, so the outliers a median would protect against were removed by the
/// filter rather than by the statistic - and a mean is differentiable in the parameters, which is
/// what makes a coordinate descent over it converge.
#[must_use]
pub fn measure(
    readings: &[PairReading],
    transform: Option<&CameraTransform>,
) -> AppearanceDistance {
    if readings.is_empty() {
        return AppearanceDistance::default();
    }
    let mut skin = 0.0_f32;
    let mut skin_count = 0_u32;
    let mut white = 0.0_f32;
    let mut signature = 0.0_f32;
    let mut contrast = 0.0_f32;

    for reading in readings {
        let Corrected {
            white: right_white,
            signature: right_signature,
            contrast: right_contrast,
            skin: right_skin,
        } = match transform {
            Some(t) => corrected(reading, t),
            None => Corrected {
                white: reading.right_white,
                signature: reading.right_signature,
                contrast: reading.right_contrast,
                skin: reading.right_skin,
            },
        };

        if let (Some((left_uv, left_luma)), Some((right_uv, right_luma))) =
            (reading.left_skin, right_skin)
        {
            skin += skin_consistency::de00_between(left_uv, left_luma, right_uv, right_luma);
            skin_count += 1;
        }

        let du = reading.left_white[0] - right_white[0];
        let dv = reading.left_white[1] - right_white[1];
        white += (du * du + dv * dv).sqrt() / AppearanceDistance::UV_SCALE;

        signature += signature_distance(&reading.left_signature, &right_signature);
        contrast +=
            (reading.left_contrast - right_contrast).abs() / AppearanceDistance::CONTRAST_SCALE;
    }

    #[allow(clippy::cast_precision_loss)]
    let n = readings.len() as f32;
    AppearanceDistance {
        // Zero when no pair carried a skin reading, which is this build on a real photograph -
        // `SKIN_FIELD_AVAILABLE` is false. A zero here is an unmeasured term rather than a met
        // promise, and `CameraOutline::skin_de00_before` being zero is how the panel says so.
        skin_de00: if skin_count == 0 {
            0.0
        } else {
            skin / f64::from(skin_count) as f32
        },
        white_point: white / n,
        grade_signature: signature / n,
        contrast: contrast / n,
    }
}

/// One half of a pair, after a transform has been applied to it.
///
/// A named struct rather than a four-tuple: `(white, signature, contrast, skin)` read at a call
/// site is four positions a reader has to count, and the skin half is itself an optional pair -
/// so the tuple was a `([f32; 2], [f32; 8], f32, Option<([f32; 2], f32)>)` that nobody could check
/// at a glance.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Corrected {
    /// The white point, in CIE 1976 `u'v'`.
    white: [f32; 2],
    /// The eight-number grade signature.
    signature: [f32; 8],
    /// The contrast reading, in the recipe's units.
    contrast: f32,
    /// The skin chromaticity and luminance, when this half carries a skin reading at all.
    skin: Option<([f32; 2], f32)>,
}

/// What a transform does to the non-reference half of one reading.
fn corrected(reading: &PairReading, transform: &CameraTransform) -> Corrected {
    let white = shift_uv(reading.right_white, transform.d_cct, transform.d_tint);
    let mut signature = reading.right_signature;
    apply_to_signature(transform, &mut signature);
    let mid = transform.contrast_shape.get(1).copied().unwrap_or(1.0);
    let contrast = (reading.right_contrast * mid).clamp(-100.0, 100.0);
    let skin = reading.right_skin.map(|(uv, luma)| {
        let moved = shift_uv(uv, transform.d_cct, transform.d_tint);
        (
            [
                moved.first().copied().unwrap_or(0.0) + transform.skin_correction.d_uv[0],
                moved.get(1).copied().unwrap_or(0.0) + transform.skin_correction.d_uv[1],
            ],
            (luma * transform.d_exposure.exp2() + transform.skin_correction.d_luma).clamp(0.0, 1.0),
        )
    });
    Corrected {
        white,
        signature,
        contrast,
        skin,
    }
}

/// The distance between two grade signatures, `0..1`.
///
/// The contract's own `NodeTarget::signature_distance`, re-exported through a name this module can
/// use without importing phase 25's target type. One implementation, in the frozen contract, for
/// the reason two copies of a colour conversion is two answers to what a person's skin looks like.
#[must_use]
pub fn signature_distance(a: &[f32; 8], b: &[f32; 8]) -> f32 {
    aura_core::contract::gallery::NodeTarget::signature_distance(a, b)
}

// ---------------------------------------------------------------------------
// The port phase 25 reads
// ---------------------------------------------------------------------------

/// Which camera transform applies to which photograph.
///
/// **The one route by which a camera correction reaches phase 25**, and therefore the one place
/// section 6.4's ordering is enforced. It is a resolved map rather than a trait object for the
/// reason phase 15's `skin_loci` is read once per pass: a 4,000-image wedding would otherwise make
/// four thousand round trips through a service whose answer changes twice a day.
///
/// An empty field is a consistency pass that runs exactly as it did before this phase existed,
/// which is what makes phase 26 additive rather than a change to phase 25.
#[derive(Debug, Clone, Default)]
pub struct Field {
    by_image: BTreeMap<ImageId, CameraTransform>,
}

impl Field {
    /// An empty field: no photograph carries a camera correction.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a field from a resolved map.
    #[must_use]
    pub fn from_map(by_image: BTreeMap<ImageId, CameraTransform>) -> Self {
        Self { by_image }
    }

    /// Build a field by resolving each photograph through the frozen service.
    ///
    /// # Errors
    ///
    /// Whatever `CameraMatchService::transform_for_image` returns.
    pub fn from_service(service: &dyn CameraMatchService, images: &[ImageId]) -> AuraResult<Self> {
        let mut by_image = BTreeMap::new();
        for image in images {
            if let Some(transform) = service.transform_for_image(*image)? {
                by_image.insert(*image, transform);
            }
        }
        Ok(Self { by_image })
    }

    /// Build a field from a project's transforms plus each photograph's body and flash state.
    ///
    /// What the pass itself uses, because it already holds both halves and resolving four thousand
    /// photographs through the service would be four thousand queries against rows it just wrote.
    #[must_use]
    pub fn from_transforms(
        transforms: &[CameraTransform],
        frames: &[(ImageId, CameraId, FlashState)],
    ) -> Self {
        let mut index: BTreeMap<(String, FlashState), &CameraTransform> = BTreeMap::new();
        for transform in transforms {
            index.insert(
                (transform.camera_id.as_str().to_string(), transform.flash),
                transform,
            );
        }
        let mut by_image = BTreeMap::new();
        for (image, camera, flash) in frames {
            if let Some(transform) = index.get(&(camera.as_str().to_string(), *flash)) {
                // A disabled body is **absent** from the field rather than present as an identity.
                // The two look the same in a gallery and mean opposite things, and phase 25 must
                // not report "this camera needed no correction" for a camera nobody corrected.
                if transform.enabled {
                    by_image.insert(*image, (*transform).clone());
                }
            }
        }
        Self { by_image }
    }

    /// The correction that applies to one photograph, or `None`.
    #[must_use]
    pub fn for_image(&self, image: ImageId) -> Option<&CameraTransform> {
        self.by_image.get(&image)
    }

    /// How many photographs carry a correction.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_image.len()
    }

    /// True when no photograph does.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_image.is_empty()
    }

    /// Fold this field into a set of phase 25 frames, in place.
    ///
    /// Returns how many frames moved. **Section 6.4's ordering, in one call.**
    pub fn apply_to_gallery_frames(&self, frames: &mut [Frame]) -> usize {
        let mut moved = 0;
        for frame in frames.iter_mut() {
            if let Some(transform) = self.for_image(frame.image) {
                apply_to_gallery(transform, frame);
                moved += 1;
            }
        }
        moved
    }
}

/// Which project a field was built for, for the pass's own bookkeeping.
///
/// A newtype rather than a bare id so a caller cannot pass a photograph id where a project is
/// expected. Phase 01's rule, applied to the one place this module holds one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldProject(pub ProjectId);

#[cfg(test)]
mod tests {
    use aura_core::contract::camera::{FlashState, MAX_T_CCT_K};

    use super::*;

    fn transform(d_cct: f32, d_tint: f32, d_ev: f32) -> CameraTransform {
        let mut t = CameraTransform::identity(
            CameraId::new("cam_b"),
            FlashState::Ambient,
            CameraId::new("cam_a"),
            1,
            1,
        );
        t.d_cct = d_cct;
        t.d_tint = d_tint;
        t.d_exposure = d_ev;
        t
    }

    #[test]
    fn a_temperature_move_walks_the_locus_and_keeps_the_frame_off_it() {
        // Phase 15's hardest-won lesson, as an assertion. A frame 0.004 off the Planckian locus in
        // v' is 0.004 off it after a 300 K correction; a temperature interpolation would have put
        // it back on.
        use aura_raw::colour::illuminant::cct_to_uv;
        let on_locus = cct_to_uv(4000.0);
        let off_locus = [on_locus[0], on_locus[1] + 0.004];
        let moved = shift_uv(off_locus, 300.0, 0.0);
        let expected = cct_to_uv(4300.0);
        assert!(
            (moved[1] - (expected[1] + 0.004)).abs() < 5e-4,
            "moved {moved:?} expected {:?}",
            [expected[0], expected[1] + 0.004]
        );
    }

    #[test]
    fn a_zero_transform_moves_nothing() {
        let identity = CameraTransform::identity(
            CameraId::new("cam_b"),
            FlashState::Ambient,
            CameraId::new("cam_a"),
            1,
            1,
        );
        let uv = [0.20_f32, 0.47];
        let moved = shift_uv(uv, identity.d_cct, identity.d_tint);
        assert!((moved[0] - uv[0]).abs() < 1e-4);
        assert!((moved[1] - uv[1]).abs() < 1e-4);
    }

    #[test]
    fn exposure_scales_subject_luminance_rather_than_adding_to_it() {
        // A stop is a doubling. An addition would brighten a shadow as much as a highlight, which
        // is not what an exposure offset does to a photograph.
        let mut frame = super::super::fixtures::plain_frame("cam_b");
        frame.subject_luma = Some(0.40);
        apply(&transform(0.0, 0.0, 1.0), &mut frame);
        assert!((frame.subject_luma.unwrap_or(0.0) - 0.80).abs() < 1e-3);
    }

    #[test]
    fn a_disabled_transform_is_a_no_operation() {
        let mut t = transform(500.0, 8.0, 0.3);
        t.enabled = false;
        let mut frame = super::super::fixtures::plain_frame("cam_b");
        let before = frame.clone();
        apply(&t, &mut frame);
        assert_eq!(frame, before);
    }

    #[test]
    fn a_disabled_body_is_absent_from_the_field_rather_than_an_identity_inside_it() {
        let mut t = transform(300.0, 0.0, 0.0);
        t.enabled = false;
        let image = ImageId::new();
        let field = Field::from_transforms(
            &[t],
            &[(image, CameraId::new("cam_b"), FlashState::Ambient)],
        );
        assert!(field.for_image(image).is_none());
        assert!(field.is_empty());
    }

    #[test]
    fn the_distance_falls_when_a_transform_closes_a_known_gap() {
        // The objective doing its job: two bodies 400 K apart, and the transform that says so.
        use aura_raw::colour::illuminant::cct_to_uv;
        let readings: Vec<PairReading> = (0..10)
            .map(|_| PairReading {
                left_skin: None,
                right_skin: None,
                left_white: cct_to_uv(5200.0),
                right_white: cct_to_uv(4800.0),
                left_cct: 5200.0,
                right_cct: 4800.0,
                left_signature: [0.1; 8],
                right_signature: [0.1; 8],
                left_contrast: 10.0,
                right_contrast: 10.0,
                left_luma: 0.45,
                right_luma: 0.45,
            })
            .collect();
        let before = measure(&readings, None);
        let after = measure(&readings, Some(&transform(400.0, 0.0, 0.0)));
        assert!(
            after.total() < before.total(),
            "before {} after {}",
            before.total(),
            after.total()
        );
        assert!(after.white_point < 0.2, "{}", after.white_point);
    }

    #[test]
    fn an_empty_set_of_pairs_is_a_zero_distance_and_not_a_perfect_score() {
        let distance = measure(&[], None);
        assert_eq!(distance.total(), 0.0);
        assert_eq!(
            distance.reduction_to(&distance),
            0.0,
            "nothing measured is not all removed"
        );
    }

    #[test]
    fn a_bounded_transform_never_leaves_a_frame_outside_the_locus_domain() {
        let mut frame = super::super::fixtures::plain_frame("cam_b");
        frame.cct_k = Some(2100.0);
        apply(&transform(-MAX_T_CCT_K, 0.0, 0.0), &mut frame);
        assert!(frame.cct_k.unwrap_or(0.0) >= 2000.0);
    }

    #[test]
    fn the_gallery_hook_moves_the_same_axes_as_the_camera_hook() {
        // Section 6.4's ordering only works if the two applications agree; a frame that reached
        // phase 25 with a different temperature from the one the solver measured would make every
        // node target subtly wrong.
        let t = transform(250.0, 4.0, 0.2);
        let mut camera_frame = super::super::fixtures::plain_frame("cam_b");
        let mut gallery_frame = super::super::fixtures::plain_gallery_frame(camera_frame.image);
        apply(&t, &mut camera_frame);
        apply_to_gallery(&t, &mut gallery_frame);
        assert_eq!(camera_frame.cct_k, gallery_frame.cct_k);
        assert_eq!(camera_frame.tint, gallery_frame.tint);
        assert_eq!(camera_frame.exposure_ev, gallery_frame.exposure_ev);
        assert_eq!(camera_frame.subject_luma, gallery_frame.subject_luma);
        assert_eq!(camera_frame.saturation, gallery_frame.saturation);
        assert_eq!(camera_frame.contrast, gallery_frame.contrast);
        assert_eq!(camera_frame.signature, gallery_frame.signature);
    }
}
