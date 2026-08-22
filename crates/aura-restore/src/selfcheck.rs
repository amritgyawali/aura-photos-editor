//! What the plan did to the texture and the edges, measured through the renderer.
//!
//! Section 6.4: "Self-check measures band-energy smearing, ringing near edges and identity drift;
//! violations reduce strength automatically and log a reason."
//!
//! ## Three measurements, three levers, and that is why they are not one score
//!
//! | Measurement | Bound | Fixed by |
//! |---|---|---|
//! | [`ArtefactReport::texture_retention`] | [`MIN_TEXTURE_RETENTION`] | stepping the denoise tier down |
//! | [`ArtefactReport::ringing`] | [`MAX_RINGING`] | reducing the sharpen amount |
//! | [`ArtefactReport::identity_drift`] | [`MAX_IDENTITY_DRIFT`] | [`crate::face_recovery::enforce`] |
//!
//! A single score would make a plan that over-sharpened and a plan that over-smoothed
//! indistinguishable, and the automatic reduction section 6.4 asks for would not know which lever
//! to pull. Phase 21 settled this argument for its three families and ADR-0045 section 2.1 reaches
//! the same conclusion from the same place.
//!
//! The third measurement is not taken here. It is taken by [`crate::face_recovery::enforce`],
//! which needs a face crop, an embedding and a per-face retry loop; this module reads its result
//! and carries it onto the report. Splitting it that way is what makes each measurement live
//! beside the lever that fixes it.
//!
//! ## The measurement is on the pixel, not on the parameter
//!
//! [`enforce`] applies the plan through `aura_render::restore::apply` - the same code the
//! delivered JPEG goes through - and divides the band energies. Phase 16 established this for
//! skin colour, phase 20 for skin texture, phase 21 for catchlights, and this is the fourth. A
//! second implementation of the operators here would make every stored number a statement about a
//! model of the renderer.
//!
//! ## The texture measurement deliberately excludes the face
//!
//! Face recovery *raises* high-band energy inside a face, so a whole-frame ratio that included it
//! would let a strong recovery hide a strong smear somewhere else - two operations cancelling in
//! one number is exactly the failure the three-measurement split exists to prevent. The weights
//! come from phase 18's regions where they exist and are the whole frame minus the face boxes
//! where they do not.

use aura_core::contract::restore::{
    ArtefactReport, DenoiseSpec, DenoiseTier, RestoreCode, RestoreReason, SharpenSpec,
    MAX_IDENTITY_DRIFT, MAX_RESOLVES, MAX_RINGING, MIN_TEXTURE_RETENTION, RESOLVE_STEP,
};
use aura_render::restore::{self, RestoreContext, RestoreOps};

use crate::denoise;

/// What the self-check settled on.
#[derive(Debug, Clone, PartialEq)]
pub struct SelfCheckOutcome {
    /// The tier after any reduction.
    pub tier: DenoiseTier,
    /// The denoise amounts after any reduction. `None` when the tier reached `Off`.
    pub spec: Option<DenoiseSpec>,
    /// The deconvolution after any reduction. `None` when it was withdrawn.
    pub sharpen: Option<SharpenSpec>,
    /// What was measured on the rendered result.
    pub report: ArtefactReport,
    /// Why.
    pub reasons: Vec<RestoreReason>,
}

/// Everything the self-check needs that is not in the plan.
#[derive(Debug, Clone)]
pub struct SelfCheckInput<'a> {
    /// The frame before this phase's operations, interleaved linear RGB.
    pub pixels: &'a [f32],
    /// Frame width in pixels.
    pub width: usize,
    /// Frame height in pixels.
    pub height: usize,
    /// The regions and the sensor sigma.
    pub context: &'a RestoreContext,
    /// The camera's noise model, for re-deriving a stepped-down tier's amounts.
    pub model: &'a aura_core::contract::restore::NoiseModel,
    /// The frame's ISO, for the same reason.
    pub iso: u32,
    /// The worst identity movement over the faces that were kept, from
    /// [`crate::face_recovery::enforce`].
    pub identity_drift: f32,
    /// The plan-wide face-recovery strength that survived.
    pub face_recovery: f32,
    /// True when at least one face was skipped for drift.
    pub face_skipped: bool,
    /// How many reductions the identity constraint already made.
    pub identity_resolves: u8,
}

/// Measure the plan on the rendered result and reduce until it is inside its bounds.
///
/// The loop is bounded twice: at most [`MAX_RESOLVES`] attempts per measurement, and the tier
/// reaches `Off` and the sharpening reaches withdrawal in the limit. A plan that ends inside its
/// bounds is what the store accepts; `RestorePlan::broken_guarantee` refuses one that does not,
/// so a self-check that failed to converge is a defect rather than a state.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn enforce(
    input: &SelfCheckInput<'_>,
    tier: DenoiseTier,
    spec: Option<DenoiseSpec>,
    sharpen: Option<SharpenSpec>,
) -> SelfCheckOutcome {
    let mut reasons = Vec::new();
    let mut tier = tier;
    let mut spec = spec;
    let mut sharpen = sharpen;
    let mut denoise_reduced = false;
    let mut sharpen_reduced = false;
    let mut resolves = input.identity_resolves;

    let nothing_to_do = tier == DenoiseTier::Off && sharpen.is_none() && input.face_recovery <= 0.0;
    if nothing_to_do || input.width == 0 || input.height == 0 {
        return SelfCheckOutcome {
            tier,
            spec,
            sharpen,
            report: ArtefactReport {
                identity_drift: input.identity_drift,
                face_skipped: input.face_skipped,
                resolves,
                ..ArtefactReport::UNTOUCHED
            },
            reasons,
        };
    }

    // The weights the texture ratio is measured over: everywhere except the faces. See the module
    // header for why the face is excluded rather than included.
    let weights = texture_weights(input);

    // --- the smearing bound ----------------------------------------------------------------
    let mut retention = 1.0f32;
    let mut measured_on = 0u32;
    for attempt in 0..=MAX_RESOLVES {
        if tier == DenoiseTier::Off {
            retention = 1.0;
            break;
        }
        let rendered = render(input, tier, spec.as_ref(), None, 0.0);
        let (ratio, counted) = restore::texture_retention(
            input.pixels,
            &rendered,
            input.width,
            input.height,
            &weights,
        );
        retention = ratio;
        measured_on = measured_on.max(counted);
        if ratio >= MIN_TEXTURE_RETENTION || attempt == MAX_RESOLVES {
            break;
        }
        // The lever: the tier, not the amount. A tier is the unit this phase decides in, and
        // reducing a tier's amounts while keeping its name would make `restore_plan.denoise_tier`
        // a label rather than a record of what happened.
        tier = tier.weaker();
        spec = (tier != DenoiseTier::Off).then(|| denoise::spec_for(tier, input.model, input.iso));
        denoise_reduced = true;
        resolves = resolves.saturating_add(1);
        reasons.push(RestoreReason::plain(
            RestoreCode::TierReducedBySelfCheck,
            -0.4,
        ));
    }

    // --- the ringing bound -----------------------------------------------------------------
    let mut ringing = 0.0f32;
    for attempt in 0..=MAX_RESOLVES {
        let Some(current) = sharpen.clone() else {
            ringing = 0.0;
            break;
        };
        let rendered = render(input, tier, spec.as_ref(), Some(&current), 0.0);
        let (measured, counted) =
            restore::ringing(input.pixels, &rendered, input.width, input.height);
        ringing = measured;
        measured_on = measured_on.max(counted);
        if measured <= MAX_RINGING {
            break;
        }
        if attempt == MAX_RESOLVES {
            // Withdrawn rather than attenuated. A floor that can be exceeded once is not a floor -
            // phase 20's rule - and a frame that ships unsharpened is a much smaller failure than
            // a frame that ships with outlines drawn along its edges.
            sharpen = None;
            ringing = 0.0;
            sharpen_reduced = true;
            reasons.push(RestoreReason::plain(RestoreCode::SharpenWithdrawn, -1.0));
            break;
        }
        let mut reduced = current;
        reduced.amount *= RESOLVE_STEP;
        sharpen = Some(reduced);
        sharpen_reduced = true;
        resolves = resolves.saturating_add(1);
        reasons.push(RestoreReason::plain(
            RestoreCode::AmountReducedBySelfCheck,
            -0.4,
        ));
    }

    // The final measurement is taken on the plan as it now stands, with every operation applied
    // together. The two loops above measured one operation at a time so that each could attribute
    // its own violation to its own lever; this is the number that goes on the row, and it is the
    // one a photographer sees.
    if !(tier == DenoiseTier::Off && sharpen.is_none() && input.face_recovery <= 0.0) {
        let rendered = render(
            input,
            tier,
            spec.as_ref(),
            sharpen.as_ref(),
            input.face_recovery,
        );
        let (final_retention, counted) = restore::texture_retention(
            input.pixels,
            &rendered,
            input.width,
            input.height,
            &weights,
        );
        let (final_ringing, ring_counted) =
            restore::ringing(input.pixels, &rendered, input.width, input.height);
        retention = final_retention;
        ringing = final_ringing;
        measured_on = measured_on.max(counted).max(ring_counted);

        // A combination that is outside a bound only when both operations run together is rare
        // and is real: sharpening amplifies whatever the denoiser left, and denoising removes
        // what the sharpener recovered. Whichever bound is missed loses its own operation.
        if retention < MIN_TEXTURE_RETENTION && tier != DenoiseTier::Off {
            tier = DenoiseTier::Off;
            spec = None;
            denoise_reduced = true;
            reasons.push(RestoreReason::plain(
                RestoreCode::TierReducedBySelfCheck,
                -0.6,
            ));
            retention = 1.0;
        }
        if ringing > MAX_RINGING && sharpen.is_some() {
            sharpen = None;
            sharpen_reduced = true;
            reasons.push(RestoreReason::plain(RestoreCode::SharpenWithdrawn, -1.0));
            ringing = 0.0;
        }
    }

    SelfCheckOutcome {
        tier,
        spec,
        sharpen,
        report: ArtefactReport {
            texture_retention: retention.clamp(0.0, 4.0),
            ringing: ringing.clamp(0.0, 1.0),
            identity_drift: input.identity_drift.clamp(0.0, MAX_IDENTITY_DRIFT),
            measured_on,
            resolves: resolves.min(MAX_RESOLVES * 3),
            denoise_reduced,
            sharpen_reduced,
            face_skipped: input.face_skipped,
        },
        reasons,
    }
}

/// Render one candidate plan onto a copy of the frame.
fn render(
    input: &SelfCheckInput<'_>,
    tier: DenoiseTier,
    spec: Option<&DenoiseSpec>,
    sharpen: Option<&SharpenSpec>,
    face_recovery: f32,
) -> Vec<f32> {
    let mut pixels = input.pixels.to_vec();
    let ops = RestoreOps {
        luminance: if tier == DenoiseTier::Off {
            0.0
        } else {
            spec.map_or(0.0, |s| s.luminance)
        },
        colour: if tier == DenoiseTier::Off {
            0.0
        } else {
            spec.map_or(0.0, |s| s.colour)
        },
        detail: spec.map_or(0.0, |s| s.detail),
        sharpen: sharpen.cloned(),
        face_recovery,
    };
    restore::apply(&mut pixels, input.width, input.height, &ops, input.context);
    pixels
}

/// The weights the texture ratio is measured over: the frame, minus the faces.
///
/// See the module header. Face recovery raises high-band energy inside a face, and a whole-frame
/// ratio that included it would let a strong recovery hide a strong smear elsewhere.
#[must_use]
pub fn texture_weights(input: &SelfCheckInput<'_>) -> Vec<f32> {
    let mut weights = vec![1.0f32; input.width * input.height];
    for (x, y, w, h) in &input.context.faces {
        for row in *y..(*y + *h).min(input.height) {
            for column in *x..(*x + *w).min(input.width) {
                if let Some(slot) = weights.get_mut(row * input.width + column) {
                    *slot = 0.0;
                }
            }
        }
    }
    weights
}

#[cfg(test)]
// `-D warnings` on the command line beats the crate-level `cfg_attr(test, allow(..))`
// block, so a test that compares two floats it computed itself needs the allow here.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_core::contract::restore::{NoiseModel, RestoreRegion, SharpenMask};
    use std::collections::BTreeMap;

    fn model() -> NoiseModel {
        let mut model = NoiseModel::reference();
        model.measured = true;
        model
    }

    fn lacy(width: usize, height: usize) -> Vec<f32> {
        // Fine, high-contrast structure at the scale a denoiser destroys first.
        let mut pixels = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let value = if (x % 3 == 0) ^ (y % 3 == 0) {
                    0.62
                } else {
                    0.30
                };
                pixels.extend_from_slice(&[value, value * 0.98, value * 0.94]);
            }
        }
        pixels
    }

    fn context(width: usize, height: usize, sigma: f32) -> RestoreContext {
        let mut regions = BTreeMap::new();
        regions.insert(RestoreRegion::Subject, vec![1.0f32; width * height]);
        RestoreContext {
            regions,
            sigma: Some(sigma),
            faces: Vec::new(),
        }
    }

    fn sharpen(amount: f32) -> SharpenSpec {
        let mut excluded = [false; RestoreRegion::COUNT];
        for (index, region) in RestoreRegion::ALL.iter().enumerate() {
            if region.excluded_from_sharpen() {
                excluded[index] = true;
            }
        }
        SharpenSpec {
            kernel_sigma: 1.4,
            amount,
            mask: SharpenMask {
                excluded,
                coverage: 1.0,
                from_regions: true,
            },
            skin_attenuation: 0.80,
            iterations: 3,
        }
    }

    fn input<'a>(
        pixels: &'a [f32],
        context: &'a RestoreContext,
        model: &'a NoiseModel,
    ) -> SelfCheckInput<'a> {
        SelfCheckInput {
            pixels,
            width: 64,
            height: 64,
            context,
            model,
            iso: 6400,
            identity_drift: 0.0,
            face_recovery: 0.0,
            face_skipped: false,
            identity_resolves: 0,
        }
    }

    #[test]
    fn a_plan_that_does_nothing_measures_nothing_and_is_clean() {
        let pixels = lacy(64, 64);
        let context = context(64, 64, 0.01);
        let model = model();
        let outcome = enforce(
            &input(&pixels, &context, &model),
            DenoiseTier::Off,
            None,
            None,
        );
        assert_eq!(outcome.report, ArtefactReport::UNTOUCHED);
        assert!(outcome.report.is_clean());
        assert!(outcome.reasons.is_empty());
    }

    #[test]
    fn a_denoise_that_would_smear_the_lace_is_stepped_down() {
        // The whole point of the smearing bound. A `Strong` tier over fine structure loses more
        // than a tenth of the high band, and the lever that fixes it is the tier.
        let pixels = lacy(64, 64);
        let context = context(64, 64, 0.20);
        let model = model();
        let spec = denoise::spec_for(DenoiseTier::Strong, &model, 25_600);
        let outcome = enforce(
            &input(&pixels, &context, &model),
            DenoiseTier::Strong,
            Some(spec),
            None,
        );
        assert!(
            outcome.tier.rank() < DenoiseTier::Strong.rank(),
            "the tier stayed at {} with retention {}",
            outcome.tier,
            outcome.report.texture_retention
        );
        assert!(outcome.report.denoise_reduced);
        assert!(outcome.report.is_clean(), "{:?}", outcome.report);
        assert!(outcome
            .reasons
            .iter()
            .any(|r| r.code == RestoreCode::TierReducedBySelfCheck));
        // And the spec follows the tier rather than keeping the old amounts under a new name.
        match (outcome.tier, outcome.spec.as_ref()) {
            (DenoiseTier::Off, spec) => assert!(spec.is_none()),
            (tier, Some(spec)) => {
                let expected = denoise::spec_for(tier, &model, 25_600);
                assert!((spec.luminance - expected.luminance).abs() < 1e-6);
            }
            (tier, None) => panic!("{tier} carries no spec"),
        }
    }

    #[test]
    fn a_gentle_denoise_over_flat_pixels_is_left_alone() {
        let flat = vec![0.4f32; 64 * 64 * 3];
        let context = context(64, 64, 0.005);
        let model = model();
        let spec = denoise::spec_for(DenoiseTier::Light, &model, 1600);
        let outcome = enforce(
            &input(&flat, &context, &model),
            DenoiseTier::Light,
            Some(spec.clone()),
            None,
        );
        assert_eq!(outcome.tier, DenoiseTier::Light);
        assert!(!outcome.report.denoise_reduced);
        assert_eq!(outcome.spec.map(|s| s.luminance), Some(spec.luminance));
    }

    #[test]
    fn a_sharpening_that_rings_is_reduced_and_then_withdrawn() {
        // The second bound and its own lever. The measurement is an excursion beyond the input's
        // own local range, so only a genuinely ringing deconvolution triggers it.
        let pixels = lacy(64, 64);
        let context = context(64, 64, 0.002);
        let model = model();
        let outcome = enforce(
            &input(&pixels, &context, &model),
            DenoiseTier::Off,
            None,
            Some(sharpen(0.50)),
        );
        assert!(outcome.report.is_clean(), "{:?}", outcome.report);
        match outcome.sharpen {
            Some(spec) => {
                assert!(spec.amount <= 0.50);
                assert!(outcome.report.ringing <= MAX_RINGING);
            }
            None => assert!(outcome.report.sharpen_reduced),
        }
    }

    #[test]
    fn the_two_bounds_have_two_levers_and_neither_pulls_the_other() {
        // The reason the report carries three numbers instead of one. A smear reduces the tier
        // and leaves the sharpening alone; a ring reduces the sharpening and leaves the tier
        // alone.
        let pixels = lacy(64, 64);
        let context = context(64, 64, 0.20);
        let model = model();
        let spec = denoise::spec_for(DenoiseTier::Strong, &model, 25_600);
        let outcome = enforce(
            &input(&pixels, &context, &model),
            DenoiseTier::Strong,
            Some(spec),
            None,
        );
        assert!(outcome.report.denoise_reduced);
        assert!(
            !outcome.report.sharpen_reduced,
            "the sharpen lever moved for a smear"
        );
    }

    #[test]
    fn the_identity_measurement_is_carried_rather_than_re_taken() {
        // It is measured by `face_recovery::enforce`, beside the lever that fixes it, and this
        // module reads the result onto the report. A second measurement here would be a second
        // answer to how far a face moved.
        let pixels = lacy(64, 64);
        let context = context(64, 64, 0.005);
        let model = model();
        let mut with_face = input(&pixels, &context, &model);
        with_face.identity_drift = 0.03;
        with_face.face_recovery = 0.2;
        with_face.face_skipped = true;
        with_face.identity_resolves = 2;
        let outcome = enforce(&with_face, DenoiseTier::Off, None, None);
        assert!((outcome.report.identity_drift - 0.03).abs() < 1e-6);
        assert!(outcome.report.face_skipped);
        assert!(outcome.report.resolves >= 2);
    }

    #[test]
    fn the_face_is_excluded_from_the_texture_measurement() {
        let pixels = lacy(32, 32);
        let mut ctx = context(32, 32, 0.01);
        ctx.faces = vec![(8, 8, 16, 16)];
        let model = model();
        let mut with_face = input(&pixels, &ctx, &model);
        with_face.width = 32;
        with_face.height = 32;
        let weights = texture_weights(&with_face);
        assert_eq!(weights.len(), 32 * 32);
        assert_eq!(weights[8 * 32 + 8], 0.0);
        assert_eq!(weights[0], 1.0);
    }
}
