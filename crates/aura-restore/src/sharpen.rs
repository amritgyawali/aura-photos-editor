//! Whether deconvolution would help here at all, and where it must not act.
//!
//! Section 6.2 has four bullets and three of them are refusals. Read together they describe an
//! operation whose default answer is no, and this module is the four preconditions that make that
//! true. Any one of them missing is a refusal with a reason code rather than a reduced amount.
//!
//! | Precondition | Reason when it fails |
//! |---|---|
//! | the estimated kernel is inside the band | [`RestoreCode::KernelTooSmall`], [`RestoreCode::KernelTooLarge`] |
//! | the blur is not motion | [`RestoreCode::MotionDominated`] |
//! | the focus landed on the subject | [`RestoreCode::GrossDefocus`] |
//! | phase 18 supplied regions | [`RestoreCode::SharpenNoRegions`] |
//!
//! ## The fourth is the one that will be argued with
//!
//! The obvious alternative is to sharpen the whole frame at a lower amount when there is no mask.
//! That is what every restoration tool that has ever produced a crunchy sky does. Skin, sky and
//! out-of-focus background are not regions where sharpening is *less welcome* - they are the
//! three regions where it is **visible as damage** and nowhere else, so an unmasked global
//! sharpen concentrates its entire artefact budget on the three places a photographer looks
//! first. Phase 19 wrote the general rule - a phase that consumes another phase's output owns no
//! fallback for it - and ADR-0045 section 4 records this as the sharpest case of it in the
//! product.
//!
//! ## Skin is the exception to the exception
//!
//! It is attenuated rather than excluded, because a face with literally no sharpening inside a
//! frame that was sharpened reads as *soft* rather than as protected.
//! [`aura_core::contract::restore::SKIN_ATTENUATION`] is the number and the profile table may
//! only raise it.
//!
//! ## The amount is capped by what the denoiser left
//!
//! Section 6.2's last bullet. This is the only coupling between the two operations of this phase
//! and it runs in one direction: sharpening reads what denoising left behind, and denoising never
//! reads what sharpening would like.

use aura_core::contract::integrity::MotionKind;
use aura_core::contract::restore::{
    RestoreCode, RestoreField, RestoreReason, RestoreRegion, SharpenMask, SharpenSpec,
    MAX_DECONV_ITERATIONS, MAX_SHARPEN_AMOUNT, SHARPEN_KERNEL_HI, SHARPEN_KERNEL_LO,
};

use crate::kernel::KernelEstimate;
use crate::profiles::RestoreProfiles;

/// How far the focus may sit from the subject and still be sharpened, `0..1`.
///
/// A quarter of the way to fully front- or back-focused. Phase 09's `focus_offset` is signed and
/// normalised, and beyond this the sharpest plane in the photograph is not the subject - so a
/// deconvolution would be recovering the background's detail and calling it the subject's.
pub const MAX_FOCUS_OFFSET: f32 = 0.25;

/// The residual noise sigma at which the amount is fully suppressed.
///
/// Three hundredths of diffuse white. Above this, deconvolution is amplifying noise faster than
/// it is recovering detail whatever the kernel says - which is why the cap is on the *post*
/// denoise residual rather than on the frame's original noise.
pub const NOISE_KILLS_SHARPEN: f32 = 0.03;

/// What the frame's evidence says about whether sharpening would help.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharpenEvidence {
    /// Phase 09's motion verdict.
    pub motion: MotionKind,
    /// How much of it, `0..1`.
    pub motion_severity: f32,
    /// Phase 09's focus offset, `-1..1`. Zero is on the subject.
    pub focus_offset: f32,
    /// The noise sigma left after denoising, in linear working-space units.
    pub residual_sigma: f32,
}

/// The decision, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct SharpenChoice {
    /// The deconvolution, when there is one.
    pub spec: Option<SharpenSpec>,
    /// Why.
    pub reasons: Vec<RestoreReason>,
}

impl SharpenChoice {
    /// A refusal carrying one reason.
    #[must_use]
    fn refused(code: RestoreCode) -> Self {
        Self {
            spec: None,
            reasons: vec![RestoreReason::plain(code, -1.0)],
        }
    }
}

/// Decide whether to deconvolve, how hard, and where not to.
///
/// The one entry point, so there is no second way to ask whether a frame should be sharpened.
#[must_use]
pub fn choose(
    estimate: KernelEstimate,
    evidence: SharpenEvidence,
    fields: &[RestoreField],
    scene_allows: bool,
    profiles: &RestoreProfiles,
) -> SharpenChoice {
    if !scene_allows {
        // A scene that forbids sharpening is not a refusal about this frame; it is a product
        // decision about this kind of photograph, and `restore_profiles.toml` carries the written
        // reason. The code says the kernel was never consulted rather than inventing a verdict.
        return SharpenChoice::refused(RestoreCode::KernelTooSmall);
    }

    // 1. Motion. Section 2.2 puts motion-blur removal out of scope, and a symmetric kernel
    //    deconvolving a directional blur produces a doubled edge - a worse photograph rather
    //    than a softer one.
    if matches!(
        evidence.motion,
        MotionKind::SubjectMotion | MotionKind::CameraShake
    ) && evidence.motion_severity > 0.0
    {
        return SharpenChoice::refused(RestoreCode::MotionDominated);
    }

    // 2. Gross defocus. Front and back focus by another name.
    if evidence.focus_offset.abs() > MAX_FOCUS_OFFSET {
        return SharpenChoice::refused(RestoreCode::GrossDefocus);
    }

    // 3. The kernel band.
    if !estimate.is_reliable() || estimate.sigma < SHARPEN_KERNEL_LO {
        return SharpenChoice::refused(RestoreCode::KernelTooSmall);
    }
    if estimate.sigma > SHARPEN_KERNEL_HI {
        return SharpenChoice::refused(RestoreCode::KernelTooLarge);
    }

    // 4. The regions. See the module header: this refuses rather than reducing.
    let usable: Vec<&RestoreField> = fields
        .iter()
        .filter(|field| field.is_readable() && field.is_usable())
        .collect();
    if usable.is_empty() || !usable.iter().any(|f| f.region == RestoreRegion::Subject) {
        return SharpenChoice::refused(RestoreCode::SharpenNoRegions);
    }

    let mut reasons = vec![RestoreReason::plain(RestoreCode::KernelInBand, 0.6)];

    // The amount. It rises with how much blur there is to recover and falls with how much noise
    // is left to amplify, and the two are multiplied rather than added so that either one being
    // hostile is enough to suppress the operation.
    let recoverable = ((estimate.sigma - SHARPEN_KERNEL_LO)
        / (SHARPEN_KERNEL_HI - SHARPEN_KERNEL_LO))
        .clamp(0.0, 1.0);
    let noise_headroom = (1.0 - (evidence.residual_sigma / NOISE_KILLS_SHARPEN)).clamp(0.0, 1.0);
    if noise_headroom < 1.0 {
        reasons.push(RestoreReason::plain(
            RestoreCode::AmountCappedByNoise,
            -(1.0 - noise_headroom).min(1.0),
        ));
    }

    // The region quality ceiling. Phase 18's rule for the fourth phase running: a region says how
    // much may be done with it, and a later phase multiplies. The weakest usable field decides,
    // because the deconvolution acts through all of them at once and a confident subject mask
    // does not make a doubtful sky mask safe to trust.
    let region_scale = usable
        .iter()
        .map(|field| field.strength_scale())
        .fold(1.0_f32, f32::min);

    let ceiling = profiles.max_sharpen().min(MAX_SHARPEN_AMOUNT);
    let amount = (recoverable * noise_headroom * region_scale * ceiling).clamp(0.0, ceiling);
    if amount <= 0.0 {
        return SharpenChoice {
            spec: None,
            reasons: vec![RestoreReason::plain(RestoreCode::AmountCappedByNoise, -1.0)],
        };
    }

    // Where it must not act. Sky and background are excluded outright and skin is attenuated;
    // the exclusion set is read from the contract rather than listed here, so a region added to
    // the vocabulary cannot be silently left unprotected.
    let mut excluded = [false; RestoreRegion::COUNT];
    for (index, region) in RestoreRegion::ALL.iter().enumerate() {
        if region.excluded_from_sharpen() {
            if let Some(slot) = excluded.get_mut(index) {
                *slot = true;
            }
        }
    }
    reasons.push(RestoreReason::plain(RestoreCode::SkyAndBokehExcluded, 0.2));
    reasons.push(RestoreReason::plain(RestoreCode::SkinAttenuated, 0.2));

    // The coverage: how much of the frame is left once the exclusions are taken out. A number
    // rather than a flag, because "AURA sharpened this frame" and "AURA sharpened four per cent
    // of this frame" are different sentences and the panel shows the second.
    let excluded_coverage: f32 = usable
        .iter()
        .filter(|field| field.region.excluded_from_sharpen())
        .map(|field| field.coverage())
        .sum();
    let coverage = (1.0 - excluded_coverage).clamp(0.0, 1.0);

    // The iteration count rises with the kernel, because a wider kernel needs more Richardson-Lucy
    // steps to converge - and it is capped low, because the ringing amplitude grows with the
    // count while the recovered detail saturates well before it does.
    let iterations = (1.0 + recoverable * f32::from(MAX_DECONV_ITERATIONS - 1)).round() as u8;

    SharpenChoice {
        spec: Some(SharpenSpec {
            kernel_sigma: estimate.sigma,
            amount,
            mask: SharpenMask {
                excluded,
                coverage,
                from_regions: true,
            },
            skin_attenuation: profiles.skin_attenuation(),
            iterations: iterations.clamp(1, MAX_DECONV_ITERATIONS),
        }),
        reasons,
    }
}

/// The noise a tier leaves behind, in linear working-space units.
///
/// The input to [`SharpenEvidence::residual_sigma`], and the one place the two operations of this
/// phase touch. `luminance` is the tier's own luminance amount, `0..1`; what is left is what the
/// operator did not remove, which is not zero even at full strength because an edge-preserving
/// denoiser deliberately leaves the noise that sits on an edge.
#[must_use]
pub fn residual(sigma: f32, luminance: f32) -> f32 {
    // A floor of a fifth: an edge-preserving filter at full strength leaves roughly this much,
    // and pretending it leaves none is how a sharpener ends up amplifying grain it was told had
    // been removed.
    const FLOOR: f32 = 0.20;
    sigma * (1.0 - luminance.clamp(0.0, 1.0)).max(FLOOR)
}

#[cfg(test)]
// `-D warnings` on the command line beats the crate-level `cfg_attr(test, allow(..))`
// block, so a test that compares two floats it computed itself needs the allow here.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_core::contract::composition::Box2;

    fn profiles() -> RestoreProfiles {
        RestoreProfiles::embedded().expect("the embedded profile table loads")
    }

    fn field(region: RestoreRegion) -> RestoreField {
        RestoreField {
            region,
            identity: None,
            bounds: Box2 {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            width: 8,
            height: 8,
            alpha: vec![200; 64],
            confidence: 0.95,
            edge_quality: 0.95,
            model_ver: 1,
        }
    }

    fn fields() -> Vec<RestoreField> {
        vec![
            field(RestoreRegion::Subject),
            field(RestoreRegion::Skin),
            field(RestoreRegion::Sky),
        ]
    }

    fn estimate(sigma: f32) -> KernelEstimate {
        KernelEstimate {
            sigma,
            edges: 400,
            peak: 0.5,
        }
    }

    fn clean() -> SharpenEvidence {
        SharpenEvidence {
            motion: MotionKind::None,
            motion_severity: 0.0,
            focus_offset: 0.0,
            residual_sigma: 0.002,
        }
    }

    #[test]
    fn a_frame_that_passes_all_four_preconditions_is_sharpened() {
        let choice = choose(estimate(1.2), clean(), &fields(), true, &profiles());
        let spec = choice.spec.expect("a frame in the band is sharpened");
        assert!(spec.problem().is_none(), "{:?}", spec.problem());
        assert!(spec.amount > 0.0);
        assert!(spec.mask.from_regions);
        assert!(spec.mask.excludes(RestoreRegion::Sky));
        assert!(spec.mask.excludes(RestoreRegion::Background));
        assert!(!spec.mask.excludes(RestoreRegion::Skin));
        assert!(choice
            .reasons
            .iter()
            .any(|r| r.code == RestoreCode::KernelInBand));
    }

    #[test]
    fn motion_is_refused_rather_than_deconvolved() {
        // Section 2.2 puts motion-blur removal out of scope, and this is that exclusion at the
        // operator rather than in a document.
        for motion in [MotionKind::SubjectMotion, MotionKind::CameraShake] {
            let mut evidence = clean();
            evidence.motion = motion;
            evidence.motion_severity = 0.4;
            let choice = choose(estimate(1.2), evidence, &fields(), true, &profiles());
            assert!(choice.spec.is_none(), "{motion:?} was sharpened");
            assert_eq!(choice.reasons[0].code, RestoreCode::MotionDominated);
        }
    }

    #[test]
    fn gross_defocus_is_refused_in_both_directions() {
        for offset in [-0.6_f32, 0.6] {
            let mut evidence = clean();
            evidence.focus_offset = offset;
            let choice = choose(estimate(1.2), evidence, &fields(), true, &profiles());
            assert!(choice.spec.is_none());
            assert_eq!(choice.reasons[0].code, RestoreCode::GrossDefocus);
        }
    }

    #[test]
    fn a_kernel_outside_the_band_is_refused_at_both_ends() {
        let small = choose(
            estimate(SHARPEN_KERNEL_LO - 0.1),
            clean(),
            &fields(),
            true,
            &profiles(),
        );
        assert_eq!(small.reasons[0].code, RestoreCode::KernelTooSmall);

        let large = choose(
            estimate(SHARPEN_KERNEL_HI + 0.1),
            clean(),
            &fields(),
            true,
            &profiles(),
        );
        assert_eq!(large.reasons[0].code, RestoreCode::KernelTooLarge);
    }

    #[test]
    fn an_unreliable_estimate_is_refused_even_inside_the_band() {
        let mut sparse = estimate(1.2);
        sparse.edges = 3;
        let choice = choose(sparse, clean(), &fields(), true, &profiles());
        assert!(choice.spec.is_none());
    }

    #[test]
    fn no_regions_means_no_sharpening_rather_than_a_weaker_sharpening() {
        // ADR-0045 section 4, bullet 4. The whole argument of this module, as one assertion.
        let choice = choose(estimate(1.2), clean(), &[], true, &profiles());
        assert!(choice.spec.is_none(), "a blind sharpen was planned");
        assert_eq!(choice.reasons[0].code, RestoreCode::SharpenNoRegions);

        // And regions without a subject are still no regions: the deconvolution has to know where
        // it may act, not only where it may not.
        let no_subject = vec![field(RestoreRegion::Sky), field(RestoreRegion::Skin)];
        let choice = choose(estimate(1.2), clean(), &no_subject, true, &profiles());
        assert_eq!(choice.reasons[0].code, RestoreCode::SharpenNoRegions);
    }

    #[test]
    fn a_doubtful_region_lowers_the_amount_rather_than_the_exclusions() {
        // Phase 18's rule for the fourth phase running: a region says how much may be done with
        // it, and a later phase multiplies.
        let confident = choose(estimate(1.6), clean(), &fields(), true, &profiles())
            .spec
            .expect("a confident frame is sharpened");
        let mut doubtful_fields = fields();
        for field in &mut doubtful_fields {
            field.edge_quality = 0.35;
        }
        let doubtful = choose(estimate(1.6), clean(), &doubtful_fields, true, &profiles())
            .spec
            .expect("a doubtful frame is still sharpened, more gently");
        assert!(
            doubtful.amount < confident.amount * 0.6,
            "{} against {}",
            doubtful.amount,
            confident.amount
        );
        // The exclusions do not move: a doubtful mask is a reason to do less, never a reason to
        // stop protecting the sky.
        assert_eq!(doubtful.mask.excluded, confident.mask.excluded);
    }

    #[test]
    fn residual_noise_suppresses_the_amount_and_names_itself() {
        let mut noisy = clean();
        noisy.residual_sigma = NOISE_KILLS_SHARPEN * 0.9;
        let choice = choose(estimate(1.6), noisy, &fields(), true, &profiles());
        let quiet = choose(estimate(1.6), clean(), &fields(), true, &profiles())
            .spec
            .expect("a quiet frame is sharpened");
        if let Some(spec) = choice.spec {
            assert!(spec.amount < quiet.amount * 0.3, "{}", spec.amount);
        }
        assert!(choice
            .reasons
            .iter()
            .any(|r| r.code == RestoreCode::AmountCappedByNoise));

        // And past the threshold it stops entirely.
        let mut hopeless = clean();
        hopeless.residual_sigma = NOISE_KILLS_SHARPEN * 1.5;
        assert!(
            choose(estimate(1.6), hopeless, &fields(), true, &profiles())
                .spec
                .is_none()
        );
    }

    #[test]
    fn a_scene_that_forbids_sharpening_is_never_sharpened() {
        let choice = choose(estimate(1.2), clean(), &fields(), false, &profiles());
        assert!(choice.spec.is_none());
    }

    #[test]
    fn the_residual_never_reaches_zero() {
        // An edge-preserving filter at full strength leaves the noise that sits on an edge, and
        // pretending it leaves none is how a sharpener amplifies grain it was told had gone.
        assert!(residual(0.02, 1.0) > 0.0);
        assert!(residual(0.02, 0.0) > residual(0.02, 1.0));
        assert_eq!(residual(0.0, 0.5), 0.0);
    }

    #[test]
    fn a_wider_kernel_asks_for_more_iterations_and_never_more_than_the_cap() {
        let narrow = choose(estimate(1.05), clean(), &fields(), true, &profiles())
            .spec
            .expect("in band");
        let wide = choose(estimate(2.1), clean(), &fields(), true, &profiles())
            .spec
            .expect("in band");
        assert!(wide.iterations >= narrow.iterations);
        assert!(wide.iterations <= MAX_DECONV_ITERATIONS);
        assert!(narrow.iterations >= 1);
    }
}
