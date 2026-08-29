//! Which of four tiers this frame's measured noise asks for, and what that becomes.
//!
//! Section 6.1's last bullet is the whole module in one sentence:
//!
//! > Strength selection is evidence-based: the tier is chosen from the measured sigma relative to
//! > the scene tolerance, not from a global preference.
//!
//! ## There is no preference anywhere in here
//!
//! Phase 09 already produces the number section 6.1 asks for.
//! [`aura_core::contract::integrity::IntegrityResult::noise_sigma_rel`] is the measured sigma
//! **relative to what this scene tolerates at this ISO on this body**, where `1.0` is exactly the
//! tolerance - so the same absolute noise on a dance floor and in a family formal is already two
//! different numbers, and invariant 7 is satisfied by the *input* rather than by a threshold
//! table bolted on top of it.
//!
//! What this module adds is the two modifiers section 6.1 names, the scene ceiling, and the
//! conversion from a tier into the three numbers the renderer works in.
//!
//! ## The two modifiers are about where the noise lands, not how much of it there is
//!
//! Subject prominence and output size. A frame whose subject fills it has its noise on somebody's
//! face; a delivery whose long edge is over three thousand pixels is a print, where grain that is
//! invisible in a gallery thumbnail is the first thing anybody sees. Each may raise the tier by
//! one step and neither may raise it twice, because two modifiers that both fired would take a
//! frame from `Light` to `Strong` on evidence that is entirely about presentation.
//!
//! ## An unmeasured camera lowers the ceiling
//!
//! [`aura_core::contract::restore::NoiseModel::tier_ceiling`], and ADR-0047 section 3 for the
//! asymmetry: a model that under-estimates the noise under-denoises, which a photographer can see
//! and correct, while one that over-estimates it smears lace, which they cannot. Every noise
//! model in this build is unmeasured, so no frame in this build reaches [`DenoiseTier::Strong`].

use aura_core::contract::restore::{
    DenoiseSpec, DenoiseTier, NoiseModel, RestoreCode, RestoreReason,
};

use crate::profiles::RestoreProfiles;

/// The signal level the tier's sigma is quoted at, in linear working-space units.
///
/// Twenty per cent of diffuse white - a middle grey. The predicted sigma is signal-dependent, so
/// quoting one number for a frame means choosing a level to quote it at, and middle grey is where
/// most of a photograph's information sits and where noise is most visible: shadows are dark
/// enough to hide it and highlights are bright enough for shot noise to be a small fraction of
/// the signal.
pub const QUOTE_AT: f32 = 0.20;

/// How much of the chroma reduction a tier asks for, relative to its luminance reduction.
///
/// One and a third. Section 6.1: "Preserve chroma detail separately from luminance detail". The
/// two halves of the denoiser are asymmetric in *both* directions and it is worth being precise
/// about which is which: chroma is reduced **harder** than luminance, because chroma noise
/// carries no detail anybody wants, while the chroma *radius* is wider, because chroma noise is
/// spatially low-frequency. A build that inverted the amount would be smearing fabric in order to
/// remove grain, and `DenoiseSpec::problem` refuses one.
pub const CHROMA_OVER_LUMA: f32 = 1.35;

/// The sigma at which a full-strength luminance pass is warranted.
///
/// Five hundredths of diffuse white is visibly, unarguably noisy - about what a full-frame body
/// produces at ISO 25600 - and a frame noisier than that gets the operator's full strength rather
/// than an amount above one, which the contract refuses. It is the denominator that turns a
/// sensor's predicted sigma into the `0..1` scale the recipe carries.
pub const FULL_STRENGTH_SIGMA: f32 = 0.05;

/// The detail protection a tier asks for, `0..1`.
///
/// A half, flat across the tiers. It is deliberately not a function of the tier: `detail` decides
/// *how suspicious the denoiser is that a local step is real*, and that is a property of the
/// sensor's uncertainty rather than of how much noise a photographer wants removed. A `Strong`
/// tier with a lowered detail figure would be a denoiser that had become more willing to treat
/// structure as noise, which is exactly the failure the tier ladder exists to avoid.
pub const DETAIL: f32 = 0.50;

/// What this frame's noise asks for, before the plan is assembled.
#[derive(Debug, Clone, PartialEq)]
pub struct DenoiseChoice {
    /// The tier.
    pub tier: DenoiseTier,
    /// The three amounts and the sigma they came from. `None` when the tier is `Off`.
    pub spec: Option<DenoiseSpec>,
    /// Why.
    pub reasons: Vec<RestoreReason>,
}

/// Everything the tier decision reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseEvidence {
    /// Phase 09's `noise_sigma_rel`: the measured sigma relative to this scene's tolerance.
    ///
    /// `None` when the frame has no integrity verdict. Nothing is denoised in that case and the
    /// plan says so; a denoiser with no measurement is a blur.
    pub relative: Option<f32>,
    /// Phase 06's subject prominence, `0..1`.
    pub prominence: f32,
    /// The delivery's long edge in pixels.
    pub output_long_edge: u32,
    /// The frame's ISO, for the noise model.
    pub iso: u32,
}

/// Choose a tier and turn it into three amounts.
///
/// The one entry point, so there is no second way to ask how much noise a frame should lose.
#[must_use]
pub fn choose(
    evidence: NoiseEvidence,
    model: &NoiseModel,
    scene_ceiling: DenoiseTier,
    profiles: &RestoreProfiles,
) -> DenoiseChoice {
    let mut reasons = Vec::new();

    let Some(relative) = evidence.relative else {
        reasons.push(RestoreReason::plain(RestoreCode::NoNoiseReading, -1.0));
        return DenoiseChoice {
            tier: DenoiseTier::Off,
            spec: None,
            reasons,
        };
    };

    let measured = profiles.tier_for(relative);
    if measured == DenoiseTier::Off {
        reasons.push(RestoreReason::plain(RestoreCode::NoiseWithinTolerance, 1.0));
        return DenoiseChoice {
            tier: DenoiseTier::Off,
            spec: None,
            reasons,
        };
    }
    reasons.push(RestoreReason::plain(
        RestoreCode::TierFromMeasuredNoise,
        (relative - 1.0).clamp(0.0, 1.0),
    ));

    // The two modifiers. **At most one step in total**, which is why this is a single `if` chain
    // rather than two independent bumps: two modifiers that both fired would take a frame from
    // `Light` to `Strong` on evidence that is entirely about presentation rather than about how
    // much noise is in the photograph.
    let mut tier = measured;
    if profiles.prominence_raises(evidence.prominence) {
        tier = tier.stronger();
        reasons.push(RestoreReason::plain(RestoreCode::TierRaisedForSubject, 0.3));
    } else if profiles.output_raises(evidence.output_long_edge) {
        tier = tier.stronger();
        reasons.push(RestoreReason::plain(RestoreCode::TierRaisedForOutput, 0.2));
    }

    // Then the two ceilings, weakest wins. The scene's ceiling is a product decision in an
    // editable file; the camera's is a consequence of nobody having measured that sensor.
    let before_scene = tier;
    tier = tier.clamped_by(scene_ceiling);
    if tier != before_scene {
        reasons.push(RestoreReason::plain(RestoreCode::TierCappedByScene, -0.3));
    }

    let camera_ceiling = model.tier_ceiling();
    let before_camera = tier;
    tier = tier.clamped_by(camera_ceiling);
    if tier != before_camera {
        reasons.push(RestoreReason::plain(
            RestoreCode::TierCappedUnmeasuredCamera,
            -0.3,
        ));
    }
    if model.camera == "reference" {
        reasons.push(RestoreReason::plain(RestoreCode::ReferenceNoiseModel, -0.1));
    }

    if tier == DenoiseTier::Off {
        return DenoiseChoice {
            tier,
            spec: None,
            reasons,
        };
    }

    reasons.push(RestoreReason::plain(
        RestoreCode::ChromaFavouredOverLuminance,
        0.1,
    ));
    DenoiseChoice {
        tier,
        spec: Some(spec_for(tier, model, evidence.iso)),
        reasons,
    }
}

/// Turn one tier into the three amounts the renderer works in, under one camera at one ISO.
///
/// **This is what makes a tier reproducible.** The same `Standard` on a 12 MP body at ISO 1600
/// and a 61 MP body at ISO 12800 is two completely different renders, because the sigma the
/// sensor predicts differs by an order of magnitude. The amount is the tier's own multiple of
/// that sigma, converted into the `0..1` scale the recipe carries by dividing by the largest
/// sigma the operator is useful at.
#[must_use]
pub fn spec_for(tier: DenoiseTier, model: &NoiseModel, iso: u32) -> DenoiseSpec {
    let sigma = model.sigma_at(QUOTE_AT, iso);
    let luminance = (sigma * tier.sigma_multiple() / FULL_STRENGTH_SIGMA).clamp(0.0, 1.0);
    let colour = (luminance * CHROMA_OVER_LUMA).clamp(0.0, 1.0);
    DenoiseSpec {
        // The chroma amount is clamped at 1.0 and the luminance one is not raised to match, so a
        // very noisy frame can reach a point where the two are equal. `DenoiseSpec::problem`
        // requires chroma to be at or above luminance, never strictly above, for exactly this.
        luminance: luminance.min(colour),
        colour,
        detail: DETAIL,
        sigma,
        camera: model.camera.clone(),
        measured_model: model.measured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::NoiseTable;

    fn profiles() -> RestoreProfiles {
        RestoreProfiles::embedded().expect("the embedded profile table loads")
    }

    fn evidence(relative: f32) -> NoiseEvidence {
        NoiseEvidence {
            relative: Some(relative),
            prominence: 0.2,
            output_long_edge: 1600,
            iso: 6400,
        }
    }

    fn measured_model() -> NoiseModel {
        let mut model = NoiseModel::reference();
        model.camera = "sony ilce-7m3".to_string();
        model.measured = true;
        model
    }

    #[test]
    fn a_frame_inside_its_scene_tolerance_is_not_denoised_at_all() {
        // The most important arm in the module. `noise_sigma_rel` below 1.0 is a photograph whose
        // noise the scene already carries well, and removing it would be removing something a
        // photographer had implicitly accepted.
        let choice = choose(
            evidence(0.85),
            &measured_model(),
            DenoiseTier::Strong,
            &profiles(),
        );
        assert_eq!(choice.tier, DenoiseTier::Off);
        assert!(choice.spec.is_none());
        assert!(choice
            .reasons
            .iter()
            .any(|r| r.code == RestoreCode::NoiseWithinTolerance));
    }

    #[test]
    fn a_frame_with_no_measurement_is_not_denoised_and_says_so() {
        let mut ev = evidence(3.0);
        ev.relative = None;
        let choice = choose(ev, &measured_model(), DenoiseTier::Strong, &profiles());
        assert_eq!(choice.tier, DenoiseTier::Off);
        assert!(choice
            .reasons
            .iter()
            .any(|r| r.code == RestoreCode::NoNoiseReading));
    }

    #[test]
    fn the_tier_climbs_with_the_measured_noise() {
        let profiles = profiles();
        let model = measured_model();
        let tiers: Vec<DenoiseTier> = [1.1_f32, 2.0, 3.5]
            .into_iter()
            .map(|relative| choose(evidence(relative), &model, DenoiseTier::Strong, &profiles).tier)
            .collect();
        assert_eq!(
            tiers,
            vec![
                DenoiseTier::Light,
                DenoiseTier::Standard,
                DenoiseTier::Strong
            ]
        );
    }

    #[test]
    fn at_most_one_modifier_may_raise_a_tier() {
        // Two modifiers that both fired would take a frame from `Light` to `Strong` on evidence
        // that is entirely about presentation.
        let profiles = profiles();
        let model = measured_model();
        let mut both = evidence(1.1);
        both.prominence = 0.9;
        both.output_long_edge = 6000;
        let choice = choose(both, &model, DenoiseTier::Strong, &profiles);
        assert_eq!(choice.tier, DenoiseTier::Standard);
        assert_eq!(
            choice
                .reasons
                .iter()
                .filter(|r| matches!(
                    r.code,
                    RestoreCode::TierRaisedForSubject | RestoreCode::TierRaisedForOutput
                ))
                .count(),
            1
        );
    }

    #[test]
    fn an_unmeasured_camera_can_never_reach_the_strongest_tier() {
        // ADR-0047 section 3, at the decision rather than at the type. Every model in this build
        // is unmeasured, so no frame in this build is denoised at `Strong`.
        let table = NoiseTable::embedded().expect("the embedded noise models load");
        let profiles = profiles();
        for model in table.bodies() {
            let choice = choose(evidence(9.0), model, DenoiseTier::Strong, &profiles);
            assert_eq!(
                choice.tier,
                DenoiseTier::Standard,
                "{} reached {}",
                model.camera,
                choice.tier
            );
            assert!(choice
                .reasons
                .iter()
                .any(|r| r.code == RestoreCode::TierCappedUnmeasuredCamera));
        }
    }

    #[test]
    fn a_scene_ceiling_binds_and_is_named() {
        let choice = choose(
            evidence(3.5),
            &measured_model(),
            DenoiseTier::Light,
            &profiles(),
        );
        assert_eq!(choice.tier, DenoiseTier::Light);
        assert!(choice
            .reasons
            .iter()
            .any(|r| r.code == RestoreCode::TierCappedByScene));
    }

    #[test]
    fn the_same_tier_is_a_different_amount_on_two_bodies() {
        // The property that makes conditioning worth doing: a tier alone is not reproducible.
        let table = NoiseTable::embedded().expect("the embedded noise models load");
        let big_wells = table.model_for("SONY", "ILCE-7SM3");
        let small_wells = table.model_for("SONY", "ILCE-7RM4");
        let a = spec_for(DenoiseTier::Standard, &big_wells, 12_800);
        let b = spec_for(DenoiseTier::Standard, &small_wells, 12_800);
        assert!(
            b.luminance > a.luminance * 1.4,
            "{} against {}",
            b.luminance,
            a.luminance
        );
        assert!(a.problem().is_none(), "{:?}", a.problem());
        assert!(b.problem().is_none(), "{:?}", b.problem());
    }

    #[test]
    fn the_same_body_asks_for_more_at_a_higher_iso() {
        let model = measured_model();
        let low = spec_for(DenoiseTier::Standard, &model, 800);
        let high = spec_for(DenoiseTier::Standard, &model, 12_800);
        assert!(high.luminance > low.luminance);
        assert!(high.sigma > low.sigma);
    }

    #[test]
    fn chroma_is_never_reduced_less_than_luminance_at_any_tier_or_iso() {
        let table = NoiseTable::embedded().expect("the embedded noise models load");
        for model in table.bodies() {
            for tier in [
                DenoiseTier::Light,
                DenoiseTier::Standard,
                DenoiseTier::Strong,
            ] {
                for iso in [100_u32, 1600, 12_800, 102_400] {
                    let spec = spec_for(tier, model, iso);
                    assert!(
                        spec.problem().is_none(),
                        "{} at {tier} {iso}: {:?}",
                        model.camera,
                        spec.problem()
                    );
                }
            }
        }
    }
}
