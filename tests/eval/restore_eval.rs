//! PHASE-22 section 10.1, as an ordinary test so a red gate is a red build.
//!
//! Seven rows. Five are measurable here and two are not; the two that are not are named at the
//! bottom of every run rather than quietly skipped.
//!
//! | Section 10.1 row | Here |
//! |---|---|
//! | denoise PSNR/SSIM beats bilinear decisively | `the_denoiser_beats_the_bilinear_baseline` |
//! | expert preference >= 80 % at ISO >= 6400 | **not measurable** - no panel, no wedding |
//! | identity distance below threshold on 100 % | `no_delivered_face_moved_further_than_the_ceiling` |
//! | no ringing above threshold; skin and bokeh unaffected | `sharpening_never_rings` and friends |
//! | order of operations enforced | `crates/aura-render/tests/restoration_order.rs` |
//! | self-check reduces strength on adversarial smear | `the_self_check_steps_the_tier_down` |
//! | performance budget and VRAM | **waived** - this build links no `wgpu` backend |
//! | cloud offload declines gracefully | `nothing_in_this_build_reaches_a_provider` |
//!
//! **Every number here is measured on synthetic frames** whose noise, blur and structure were
//! painted into the pixels by `aura_restore::fixtures` and read back through the real detectors,
//! the real operators, the real renderer and the real store. That proves the arithmetic, the
//! thresholds, the refusals and the guarantees; it says nothing about a wedding. Conditions C1
//! and C4 of `docs/progress/PHASE-22-EXIT.md`.

use aura_core::contract::restore::{
    ArtefactReport, DenoiseTier, RestoreCode, RestoreRegion, RunWhere, MAX_FACE_RECOVERY,
    MAX_IDENTITY_DRIFT, MAX_RINGING, MIN_TEXTURE_RETENTION, SHARPEN_KERNEL_HI, SHARPEN_KERNEL_LO,
    SKIN_ATTENUATION,
};
use aura_render::restore::{self as render_restore, RestoreContext, RestoreOps};
use aura_restore::decide::{Analyser, RestoreFrame};
use aura_restore::face_recovery::IdentityProbe;
use aura_restore::fixtures;
use aura_restore::profiles::{NoiseTable, RestoreProfiles};
use aura_restore::schedule::{self, Capacity};
use aura_restore::{denoise, kernel, sharpen};
use std::collections::BTreeMap;

fn analyser() -> Analyser {
    Analyser::embedded(Capacity::default()).expect("the embedded tables load")
}

fn plan_of(
    frame: &RestoreFrame,
    probe: Option<&dyn IdentityProbe>,
) -> aura_core::contract::restore::RestorePlan {
    analyser().plan(frame, probe, true).expect("a plan").0
}

// ---------------------------------------------------------------------------
// Row 1: the denoiser beats a bilinear baseline decisively
// ---------------------------------------------------------------------------

#[test]
fn the_denoiser_beats_the_bilinear_baseline() {
    // Section 10.1's first row, measured the only honest way: against the clean plate the fixture
    // noise was actually added to, rather than against a blurred approximation of it. Both the
    // candidate and the baseline are compared to the same reference.
    let clean = fixtures::noisy_frame_plate();
    let frame = fixtures::noisy_frame();
    let side = fixtures::SIDE;

    let plan = plan_of(&frame, None);
    assert_ne!(
        plan.denoise,
        DenoiseTier::Off,
        "the dance-floor fixture was not denoised"
    );
    let spec = plan.denoise_spec.as_ref().expect("a tier carries a spec");

    let mut denoised = frame.pixels.clone();
    let context = RestoreContext {
        regions: BTreeMap::new(),
        sigma: Some(spec.sigma),
        faces: Vec::new(),
    };
    let applied = render_restore::apply(
        &mut denoised,
        side,
        side,
        &RestoreOps {
            luminance: spec.luminance,
            colour: spec.colour,
            detail: spec.detail,
            sharpen: None,
            face_recovery: 0.0,
        },
        &context,
    );
    assert!(applied.denoised);

    let baseline = fixtures::bilinear_baseline(&frame.pixels, side, side);

    let ours = fixtures::psnr(&clean, &denoised);
    let theirs = fixtures::psnr(&clean, &baseline);
    let noisy = fixtures::psnr(&clean, &frame.pixels);
    assert!(
        ours > noisy,
        "denoising made the frame worse than leaving it alone: {ours:.2} dB against {noisy:.2} dB"
    );
    assert!(
        ours > theirs + 1.0,
        "PSNR {ours:.2} dB is not decisively above the bilinear baseline's {theirs:.2} dB"
    );

    let ssim_ours = fixtures::ssim(&clean, &denoised, side, side);
    let ssim_theirs = fixtures::ssim(&clean, &baseline, side, side);
    assert!(
        ssim_ours > ssim_theirs,
        "SSIM {ssim_ours:.4} is not above the bilinear baseline's {ssim_theirs:.4}"
    );
}

#[test]
fn the_denoiser_keeps_the_fabric_it_is_cleaning() {
    // The second half of row 1: "chroma detail preserved on fabric fixtures". The lace plate has
    // energy in both the mid and the high band, and the self-check's own bound is what a denoise
    // has to clear on it.
    let frame = fixtures::noisy_frame();
    let plan = plan_of(&frame, None);
    let report = plan
        .selfcheck
        .expect("a plan that denoised measured something");
    assert!(
        report.texture_retention >= MIN_TEXTURE_RETENTION,
        "the fabric lost too much: {:.3} against {MIN_TEXTURE_RETENTION}",
        report.texture_retention
    );
    assert!(report.measured_on > 0, "nothing was measured");
}

// ---------------------------------------------------------------------------
// Row 3: identity preservation on 100 % of fixtures
// ---------------------------------------------------------------------------

#[test]
fn no_delivered_face_moved_further_than_the_ceiling() {
    // **The guarantee of this phase.** Section 10.1: "face embedding distance after face recovery
    // below threshold on 100 % of fixtures, or the operation is skipped."
    //
    // The probe is not phase 06's recogniser - it is untrained - so what this proves is that the
    // constraint refuses what it should refuse, and says nothing about whether a real embedding
    // would notice a real identity change. Condition C2.
    for probe in [fixtures::BandProbe::gentle(), fixtures::BandProbe::severe()] {
        let frame = fixtures::soft_face_frame();
        let plan = plan_of(&frame, Some(&probe));
        assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
        for face in &plan.recovered {
            if face.skipped {
                assert!(
                    face.skipped_because.is_some(),
                    "a skipped face carries no reason"
                );
                assert_eq!(face.strength, 0.0);
                continue;
            }
            assert!(
                face.identity_drift <= MAX_IDENTITY_DRIFT,
                "a delivered face moved {:.4}, above {MAX_IDENTITY_DRIFT}",
                face.identity_drift
            );
            assert!(face.strength <= MAX_FACE_RECOVERY);
        }
        assert!(plan.worst_kept_drift() <= MAX_IDENTITY_DRIFT);
    }
}

#[test]
fn a_face_too_blurred_to_recover_is_never_touched() {
    // Section 6.3: "never on heavily blurred faces where the model would hallucinate". The floor
    // is checked before any model is consulted, so an untrained head cannot be the thing that
    // saves a frame from it.
    let frame = fixtures::blurred_face_frame();
    let probe = fixtures::BandProbe::gentle();
    let plan = plan_of(&frame, Some(&probe));
    assert!(plan.face_recovery.is_none());
    for face in &plan.recovered {
        assert!(face.skipped);
        assert_eq!(face.skipped_because, Some(RestoreCode::FaceTooBlurred));
    }
}

#[test]
fn a_face_that_cannot_be_measured_is_never_recovered() {
    // A guarantee that cannot be measured is a guarantee that cannot be kept.
    let frame = fixtures::soft_face_frame();
    let probe = fixtures::BlindProbe;
    let plan = plan_of(&frame, Some(&probe));
    assert!(plan.face_recovery.is_none());
    for face in &plan.recovered {
        assert!(face.skipped);
    }
}

// ---------------------------------------------------------------------------
// Row 4: no ringing above threshold; skin and bokeh measurably unaffected
// ---------------------------------------------------------------------------

#[test]
fn sharpening_never_rings_above_the_ceiling() {
    for frame in [fixtures::soft_frame(), fixtures::noisy_frame()] {
        let plan = plan_of(&frame, None);
        let Some(report) = plan.selfcheck else {
            continue;
        };
        assert!(
            report.ringing <= MAX_RINGING,
            "a stored plan rings at {:.4}, above {MAX_RINGING}",
            report.ringing
        );
    }
}

#[test]
fn sky_and_out_of_focus_background_are_bit_identical_after_sharpening() {
    // "skin and bokeh measurably unaffected", in the strongest form the phase can offer for two
    // of the three: an excluded region is not attenuated, it is untouched. ADR-0047 section 4.
    let width = 64;
    let height = 32;
    let plate = fixtures::edge_plate(width, height, 6);
    let mut pixels = plate.clone();

    let mut subject = vec![0.0f32; width * height];
    let mut sky = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if y < height / 2 {
                subject[index] = 1.0;
            } else {
                sky[index] = 1.0;
            }
        }
    }
    let mut regions = BTreeMap::new();
    regions.insert(RestoreRegion::Subject, subject);
    regions.insert(RestoreRegion::Sky, sky);
    let context = RestoreContext {
        regions,
        sigma: None,
        faces: Vec::new(),
    };

    let mut excluded = [false; RestoreRegion::COUNT];
    for (index, region) in RestoreRegion::ALL.iter().enumerate() {
        if region.excluded_from_sharpen() {
            excluded[index] = true;
        }
    }
    let spec = aura_core::contract::restore::SharpenSpec {
        kernel_sigma: 1.4,
        amount: 0.4,
        mask: aura_core::contract::restore::SharpenMask {
            excluded,
            coverage: 0.5,
            from_regions: true,
        },
        skin_attenuation: SKIN_ATTENUATION,
        iterations: 3,
    };
    let applied = render_restore::apply(
        &mut pixels,
        width,
        height,
        &RestoreOps {
            sharpen: Some(spec),
            ..RestoreOps::default()
        },
        &context,
    );
    assert!(applied.sharpened);

    for y in height / 2..height {
        for x in 0..width {
            let base = (y * width + x) * 3;
            assert_eq!(
                pixels.get(base),
                plate.get(base),
                "the sky moved at ({x}, {y})"
            );
        }
    }
}

#[test]
fn skin_is_attenuated_rather_than_excluded() {
    // The third region, and the one where the answer is different. A face with literally no
    // sharpening inside a sharpened frame reads as soft rather than as protected, so skin keeps a
    // fifth of the amount. `SharpenSpec::problem` refuses a spec that withheld less.
    let profiles = RestoreProfiles::embedded().expect("the profile table loads");
    assert!(profiles.skin_attenuation() >= SKIN_ATTENUATION);
    assert!(
        profiles.skin_attenuation() < 1.0,
        "skin is excluded entirely"
    );
    assert!(!RestoreRegion::Skin.excluded_from_sharpen());
}

#[test]
fn a_frame_with_no_regions_is_never_sharpened() {
    // The fourth precondition, which on this build refuses every frame in every wedding: phase 18
    // supplies nothing. Condition C3.
    let mut frame = fixtures::soft_frame();
    frame.regions.clear();
    let plan = plan_of(&frame, None);
    assert!(plan.sharpen.is_none());
    assert!(!plan.region_covered);
    assert!(plan
        .reasons
        .iter()
        .any(|reason| reason.code == RestoreCode::SharpenNoRegions));
}

#[test]
fn motion_and_gross_defocus_are_refused() {
    // Section 2.2's exclusion at the operator: a symmetric kernel deconvolving a directional blur
    // produces a doubled edge, which is a worse photograph rather than a softer one.
    for (frame, expected) in [
        (fixtures::motion_frame(), RestoreCode::MotionDominated),
        (fixtures::back_focus_frame(), RestoreCode::GrossDefocus),
    ] {
        let plan = plan_of(&frame, None);
        assert!(plan.sharpen.is_none(), "{expected} was sharpened anyway");
        assert!(
            plan.reasons.iter().any(|reason| reason.code == expected),
            "no reason named {expected}"
        );
    }
}

#[test]
fn the_kernel_band_is_above_the_estimator_own_floor() {
    // Phase 22's own defect, as a gate rather than only as a unit test. A Sobel gradient ridge
    // across a perfect step edge is two samples wide, so a contract floor below sigma 0.849 is a
    // floor no photograph is ever under - and every frame in every wedding would be deconvolved.
    // ADR-0047 section 11.1.
    let perfect = fixtures::edge_plate(96, 96, 8);
    let measured = kernel::estimate(&perfect, 96, 96);
    assert!(measured.is_reliable());
    assert!(
        measured.sigma < SHARPEN_KERNEL_LO,
        "the sharpest image that can exist measures {} against a floor of {SHARPEN_KERNEL_LO}",
        measured.sigma
    );
    const { assert!(SHARPEN_KERNEL_LO < SHARPEN_KERNEL_HI) };
}

// ---------------------------------------------------------------------------
// Row 6: the self-check reduces strength on an adversarial smear fixture
// ---------------------------------------------------------------------------

#[test]
fn the_self_check_steps_the_tier_down_on_a_frame_it_would_smear() {
    // Section 10.1: "Self-check reduces strength automatically on adversarial smear fixtures."
    // The adversarial fixture is a lace plate at a sigma far above what its texture can survive:
    // the tier the evidence asks for loses more than a tenth of the high band, and the lever that
    // fixes it is the tier rather than the amount.
    let side = fixtures::SIDE;
    let plate = fixtures::lace_plate(side, side);
    let profiles = RestoreProfiles::embedded().expect("the profile table loads");
    let cameras = NoiseTable::embedded().expect("the camera table loads");
    let mut model = cameras.model_for("SONY", "ILCE-7M3");
    model.measured = true;

    let asked = DenoiseTier::Strong;
    let spec = denoise::spec_for(asked, &model, 102_400);
    let context = RestoreContext {
        regions: BTreeMap::new(),
        sigma: Some(spec.sigma.max(0.2)),
        faces: Vec::new(),
    };
    let outcome = aura_restore::selfcheck::enforce(
        &aura_restore::selfcheck::SelfCheckInput {
            pixels: &plate,
            width: side,
            height: side,
            context: &context,
            model: &model,
            iso: 102_400,
            identity_drift: 0.0,
            face_recovery: 0.0,
            face_skipped: false,
            identity_resolves: 0,
        },
        asked,
        Some(spec),
        None,
    );
    assert!(
        outcome.tier.rank() < asked.rank(),
        "the tier stayed at {} with retention {:.3}",
        outcome.tier,
        outcome.report.texture_retention
    );
    assert!(outcome.report.denoise_reduced);
    assert!(outcome.report.is_clean(), "{:?}", outcome.report);
    assert!(profiles.version() >= 1);
}

#[test]
fn a_stored_plan_is_never_outside_its_own_bounds() {
    // The store is the last place a bad row can be stopped, and `broken_guarantee` is asked there
    // as well as in the solver. This asserts the property over every fixture at once.
    for frame in [
        fixtures::clean_frame(),
        fixtures::noisy_frame(),
        fixtures::soft_frame(),
        fixtures::motion_frame(),
        fixtures::back_focus_frame(),
        fixtures::soft_face_frame(),
        fixtures::blurred_face_frame(),
        fixtures::no_sharpen_scene_frame(),
        fixtures::unmeasured_frame(),
    ] {
        let plan = plan_of(&frame, None);
        assert!(
            plan.is_sound(),
            "{}: {:?}",
            frame.image_id,
            plan.broken_guarantee()
        );
        if let Some(report) = plan.selfcheck {
            assert!(report.is_clean(), "{}: {report:?}", frame.image_id);
        }
        assert!(!plan.reasons.is_empty(), "invariant 2");
    }
}

// ---------------------------------------------------------------------------
// Row 8: the cloud offload declines gracefully, with identical decisions locally
// ---------------------------------------------------------------------------

#[test]
fn nothing_in_this_build_reaches_a_provider() {
    // Section 10.1's last row, and section 7's one sentence. `RunWhere::Cloud` exists because
    // section 5 freezes it; nothing returns it, and the decisions a frame gets are identical
    // whether or not a photographer has consented to anything. ADR-0047 section 7.
    for gpu in [false, true] {
        for cloud_consent in [false, true] {
            let capacity = Capacity { gpu, cloud_consent };
            let destination = schedule::where_to_run(capacity, 4.0).0;
            assert_ne!(destination, RunWhere::Cloud);
            assert!(!destination.leaves_the_device());
        }
    }

    // And the plan is byte-for-byte the same with consent given and withheld.
    let frame = fixtures::noisy_frame();
    let with_consent = Analyser::embedded(Capacity {
        gpu: false,
        cloud_consent: true,
    })
    .expect("tables")
    .plan(&frame, None, true)
    .expect("a plan")
    .0;
    let without = plan_of(&frame, None);
    assert_eq!(with_consent.denoise, without.denoise);
    assert_eq!(with_consent.run_where, without.run_where);
    assert_eq!(with_consent.reasons.len(), without.reasons.len());
}

// ---------------------------------------------------------------------------
// The tables, and the two conditions this build cannot close
// ---------------------------------------------------------------------------

#[test]
fn no_frame_in_this_build_reaches_the_strongest_tier() {
    // ADR-0047 section 3, as a gate. Every noise model that ships is derived from a specification
    // rather than measured, so `Strong` is unreachable - and a build where it became reachable
    // without a photographed reference arriving is a build that lost the asymmetry.
    let cameras = NoiseTable::embedded().expect("the camera table loads");
    let profiles = RestoreProfiles::embedded().expect("the profile table loads");
    for model in cameras.bodies() {
        assert!(!model.measured, "{} claims to be measured", model.camera);
        let choice = denoise::choose(
            denoise::NoiseEvidence {
                relative: Some(12.0),
                prominence: 0.9,
                output_long_edge: 6000,
                iso: 25_600,
            },
            model,
            DenoiseTier::Strong,
            &profiles,
        );
        assert_eq!(choice.tier, DenoiseTier::Standard, "{}", model.camera);
    }
}

#[test]
fn the_residual_never_reaches_zero_so_sharpening_is_always_capped() {
    // The one coupling between the two operations of this phase, and it runs in one direction.
    assert!(sharpen::residual(0.02, 1.0) > 0.0);
    assert!(sharpen::residual(0.02, 0.0) > sharpen::residual(0.02, 1.0));
}

#[test]
fn what_this_build_does_not_prove() {
    // Printed on every run rather than left in a document. Phase 20 and 21 both do this and the
    // reason is the same: a gate file that only lists what passed reads as a phase that measured
    // everything.
    let untouched = ArtefactReport::UNTOUCHED;
    assert!(untouched.is_clean());
    println!(
        "PHASE-22 section 10.1 rows this build cannot measure:\n\
         - expert preference >= 80 % at ISO 3200/6400/12800/25600 (no panel, no wedding): C4\n\
         - the competitive study against DxO, Topaz and Lightroom (same): C4\n\
         - denoise <= 2.5 s per 45 MP on the reference GPU (no wgpu backend): C6\n\
         - the identity constraint against a *trained* recogniser (phase 06 C1): C2\n\
         - sharpening through *real* regions (phase 18 has no generator wired in): C3\n\
         Every number above is measured on synthetic frames and is not a claim about a photograph."
    );
}
