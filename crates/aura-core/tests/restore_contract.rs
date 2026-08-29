//! PHASE-22. Properties of the frozen restoration contract that a later build must not break by
//! accident.
//!
//! Nothing here measures a photograph. These are the assertions that keep the *vocabulary*
//! honest and the *promises* structural: that every code has a sentence and a subject, that the
//! seven regions are a view of phase 18's vocabulary rather than a second one, that this phase's
//! mask port agrees with phase 19's about how much a doubtful region may do, and - the one this
//! phase exists for - that **a plan carrying a kept face above the identity ceiling is not a
//! representable sound state**.
//!
//! Two of these tests are about things that are *absent*. Section 2.2 puts upscaling and
//! generative reconstruction out of scope, and the enforcement is that there is nowhere in this
//! contract to put either; a test is what turns "there is no field for it" from an observation
//! into a rule.

use aura_core::contract::composition::Box2;
use aura_core::contract::local::{MaskField, MaskKind, FULL_MASK_CONFIDENCE, MIN_MASK_CONFIDENCE};
use aura_core::contract::micro::MicroRegion;
use aura_core::contract::restore::{
    ArtefactReport, DenoiseSpec, DenoiseTier, NoiseModel, RecoveredFace, RestoreCode, RestoreField,
    RestoreOverride, RestorePlan, RestoreReason, RestoreRegion, RestoreSubject, RestoreWhen,
    RunWhere, SharpenMask, SharpenSpec, MAX_DECONV_ITERATIONS, MAX_FACE_RECOVERY,
    MAX_IDENTITY_DRIFT, MAX_RECOVERED_FACES, MAX_RINGING, MAX_SHARPEN_AMOUNT,
    MIN_TEXTURE_RETENTION, SHARPEN_KERNEL_HI, SHARPEN_KERNEL_LO, SKIN_ATTENUATION, SOFT_FACE_HI,
    SOFT_FACE_LO,
};
use aura_core::contract::scene::SceneId;
use aura_core::PhotoId;

fn image() -> PhotoId {
    PhotoId::from_db("pht_00000000-0000-4000-8000-000000000022").expect("a photo id")
}

fn plan() -> RestorePlan {
    RestorePlan::nothing(
        image(),
        SceneId::CouplePortrait,
        RestoreReason::plain(RestoreCode::NoiseWithinTolerance, 0.0),
    )
}

fn box_at(x: f32, y: f32) -> Box2 {
    Box2 {
        x,
        y,
        w: 0.10,
        h: 0.10,
    }
}

fn clean_report() -> ArtefactReport {
    ArtefactReport {
        texture_retention: 0.97,
        ringing: 0.004,
        identity_drift: 0.0,
        measured_on: 40_000,
        resolves: 0,
        denoise_reduced: false,
        sharpen_reduced: false,
        face_skipped: false,
    }
}

fn spec() -> DenoiseSpec {
    DenoiseSpec {
        luminance: 0.30,
        colour: 0.45,
        detail: 0.50,
        sigma: 0.012,
        camera: "reference".to_string(),
        measured_model: false,
    }
}

fn sharpen() -> SharpenSpec {
    let mut excluded = [false; RestoreRegion::COUNT];
    for (index, region) in RestoreRegion::ALL.iter().enumerate() {
        if region.excluded_from_sharpen() {
            excluded[index] = true;
        }
    }
    SharpenSpec {
        kernel_sigma: 1.10,
        amount: 0.25,
        mask: SharpenMask {
            excluded,
            coverage: 0.31,
            from_regions: true,
        },
        skin_attenuation: SKIN_ATTENUATION,
        iterations: 2,
    }
}

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

#[test]
fn every_reason_code_has_a_slug_a_sentence_and_a_subject() {
    for code in RestoreCode::ALL {
        let slug = code.as_str();
        assert!(slug.starts_with("restore_"), "{slug} is not namespaced");
        assert!(
            slug.chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
            "{slug} is not a stable slug"
        );
        assert_eq!(
            RestoreCode::parse(slug),
            Some(code),
            "{slug} does not parse"
        );

        let text = code.user_text();
        assert!(text.len() > 20, "{slug} has no sentence");
        assert!(
            !text.contains("AURA-") && !text.contains("::"),
            "{slug} leaks an identifier into copy a photographer reads"
        );
        // The subject is what lets the panel group thirty codes into four groups. Every code has
        // one by construction; this asserts the match is exhaustive in the way the panel needs.
        assert!(
            RestoreSubject::ALL.contains(&code.subject()),
            "{slug} has no subject"
        );
    }
}

#[test]
fn no_two_reason_codes_share_a_slug() {
    let mut slugs: Vec<&str> = RestoreCode::ALL.iter().map(|c| c.as_str()).collect();
    slugs.sort_unstable();
    let before = slugs.len();
    slugs.dedup();
    assert_eq!(before, slugs.len(), "two codes share a slug");
    assert_eq!(before, RestoreCode::COUNT, "COUNT disagrees with ALL");
}

#[test]
fn two_thirds_of_the_codes_are_restraint() {
    // A phase whose section 6.2 has three refusals in four bullets should have far more codes
    // that say why something did *not* happen than codes that say it did. If a later build tips
    // this below a majority, something has been added that acts without a matching refusal.
    let restrained = RestoreCode::ALL
        .iter()
        .filter(|code| code.is_restraint())
        .count();
    assert!(
        restrained * 3 >= RestoreCode::COUNT * 2,
        "only {restrained} of {} codes are restraint",
        RestoreCode::COUNT
    );
}

#[test]
fn every_region_maps_onto_a_spelling_another_projection_already_uses() {
    // The property that makes `RestoreRegion` a view of phase 18's vocabulary rather than a
    // competing one. `aura-core` cannot see `aura_vision::contract::mask::MaskKind` - it depends
    // on no workspace crate - so what it can assert is that every spelling this phase uses is
    // one an existing projection of the same twenty classes already uses. Phase 19's `MaskKind`
    // covers six and phase 21's `MicroRegion` covers the other, and between them the union is
    // phase 18's own spelling for each of these seven. A new spelling invented here fails.
    for region in RestoreRegion::ALL {
        let spelling = region.as_mask_str();
        let in_phase_19 = MaskKind::ALL
            .iter()
            .any(|kind| kind.as_recipe_str() == spelling);
        let in_phase_21 = MicroRegion::ALL
            .iter()
            .any(|other| other.as_mask_str() == spelling);
        assert!(
            in_phase_19 || in_phase_21,
            "{region} maps to {spelling}, which no other projection of phase 18 uses"
        );
    }
    assert_eq!(RestoreRegion::ALL.len(), RestoreRegion::COUNT);

    // And no two regions share a spelling, which is what "exactly one class" means here.
    let mut spellings: Vec<&str> = RestoreRegion::ALL.iter().map(|r| r.as_mask_str()).collect();
    spellings.sort_unstable();
    let before = spellings.len();
    spellings.dedup();
    assert_eq!(before, spellings.len(), "two regions name the same class");
}

#[test]
fn only_sky_and_background_are_excluded_from_sharpening() {
    // Skin is deliberately absent: ADR-0047 section 4 argues it is attenuated rather than
    // excluded, because a face with literally no sharpening inside a sharpened frame reads as
    // soft rather than as protected. A later build that "protects skin properly" by excluding it
    // fails here and has to read the argument.
    for region in RestoreRegion::ALL {
        let excluded = region.excluded_from_sharpen();
        let expected = matches!(region, RestoreRegion::Sky | RestoreRegion::Background);
        assert_eq!(excluded, expected, "{region} is excluded: {excluded}");
    }
    assert!(!RestoreRegion::Skin.excluded_from_sharpen());
    const { assert!(SKIN_ATTENUATION > 0.0 && SKIN_ATTENUATION < 1.0) };
}

#[test]
fn the_tier_ladder_is_ordered_and_saturating() {
    let ranks: Vec<u8> = DenoiseTier::ALL.iter().map(|t| t.rank()).collect();
    assert_eq!(ranks, vec![0, 1, 2, 3]);
    assert_eq!(DenoiseTier::Strong.stronger(), DenoiseTier::Strong);
    assert_eq!(DenoiseTier::Off.weaker(), DenoiseTier::Off);
    assert_eq!(
        DenoiseTier::Strong.clamped_by(DenoiseTier::Light),
        DenoiseTier::Light
    );
    assert_eq!(
        DenoiseTier::Light.clamped_by(DenoiseTier::Strong),
        DenoiseTier::Light
    );
    // Sub-linear in the rank, deliberately: see `DenoiseTier::sigma_multiple`.
    let strong = DenoiseTier::Strong.sigma_multiple();
    let light = DenoiseTier::Light.sigma_multiple();
    assert!(strong < light * 3.0, "the ladder is linear in the rank");
    assert!(strong > light, "the ladder does not increase");
    assert_eq!(DenoiseTier::Off.sigma_multiple(), 0.0);
    // The recipe's own spelling. Phase 14 fixed `off` and the panel binds to these four.
    assert_eq!(DenoiseTier::Off.as_str(), "off");
    for tier in DenoiseTier::ALL {
        assert_eq!(DenoiseTier::parse(tier.as_str()), Some(tier));
    }
}

// ---------------------------------------------------------------------------
// The mask port
// ---------------------------------------------------------------------------

#[test]
fn the_mask_port_agrees_with_phase_19_at_both_boundaries() {
    // ADR-0047 accepts phase 19's three-line gating ramp being written a third time rather than
    // widening a frozen enum, and that is only acceptable while the three agree. This is what
    // makes a change to one that did not move the others fail the build.
    let field = |confidence: f32, edge: f32| RestoreField {
        region: RestoreRegion::Skin,
        identity: None,
        bounds: box_at(0.1, 0.1),
        width: 4,
        height: 4,
        alpha: vec![255; 16],
        confidence,
        edge_quality: edge,
        model_ver: 1,
    };
    let phase_19 = |confidence: f32, edge: f32| MaskField {
        kind: MaskKind::Skin,
        identity: None,
        bounds: box_at(0.1, 0.1),
        width: 4,
        height: 4,
        alpha: vec![255; 16],
        confidence,
        edge_quality: edge,
        model_ver: 1,
    };

    for (confidence, edge) in [
        (MIN_MASK_CONFIDENCE - 0.01, 1.0),
        (MIN_MASK_CONFIDENCE, 1.0),
        (FULL_MASK_CONFIDENCE, 1.0),
        (FULL_MASK_CONFIDENCE, 0.5),
        (0.80, 0.75),
    ] {
        let ours = field(confidence, edge).strength_scale();
        let theirs = phase_19(confidence, edge).strength_scale();
        assert!(
            (ours - theirs).abs() < 1e-6,
            "at confidence {confidence} edge {edge}: {ours} against phase 19's {theirs}"
        );
    }
    assert!(!field(MIN_MASK_CONFIDENCE - 0.01, 1.0).is_usable());
}

#[test]
fn an_unreadable_field_is_named_rather_than_used() {
    let base = RestoreField {
        region: RestoreRegion::Sky,
        identity: None,
        bounds: box_at(0.0, 0.0),
        width: 4,
        height: 4,
        alpha: vec![128; 16],
        confidence: 0.9,
        edge_quality: 0.9,
        model_ver: 1,
    };
    assert!(base.is_readable(), "{:?}", base.problem());
    assert!((base.coverage() - 128.0 / 255.0).abs() < 1e-3);

    let mut zero_side = base.clone();
    zero_side.width = 0;
    assert!(zero_side.problem().is_some());

    let mut too_big = base.clone();
    too_big.width = RestoreField::MAX_SIDE + 1;
    assert!(too_big.problem().is_some());

    let mut ragged = base.clone();
    ragged.alpha.pop();
    assert!(ragged.problem().is_some());

    let mut bad_confidence = base;
    bad_confidence.confidence = 1.5;
    assert!(bad_confidence.problem().is_some());
}

// ---------------------------------------------------------------------------
// The noise model
// ---------------------------------------------------------------------------

#[test]
fn an_unmeasured_noise_model_caps_the_tier_below_strong() {
    // The whole of ADR-0047 section 3's asymmetry, as an assertion. An unmeasured model that
    // over-estimates the noise produces the smeared lace this phase exists to avoid, and there
    // are no camera files in this repository, so every shipped model is unmeasured.
    let reference = NoiseModel::reference();
    assert!(!reference.measured);
    assert_eq!(reference.tier_ceiling(), DenoiseTier::Standard);

    let mut measured = NoiseModel::reference();
    measured.measured = true;
    assert_eq!(measured.tier_ceiling(), DenoiseTier::Strong);

    // And the cap actually binds when a tier is clamped by it.
    assert_eq!(
        DenoiseTier::Strong.clamped_by(reference.tier_ceiling()),
        DenoiseTier::Standard
    );
}

#[test]
fn the_noise_model_grows_with_signal_and_with_iso() {
    let model = NoiseModel::reference();
    let dark = model.sigma_at(0.02, 100);
    let bright = model.sigma_at(0.80, 100);
    assert!(bright > dark, "shot noise does not grow with signal");

    let low = model.sigma_at(0.20, 100);
    let high = model.sigma_at(0.20, 12_800);
    assert!(high > low * 4.0, "sigma barely moved across seven stops");

    // Read noise alone at zero signal, and never negative.
    assert!(model.sigma_at(0.0, 100) > 0.0);
    assert!(model.sigma_at(-1.0, 100) >= 0.0);
    assert!(model.problem().is_none(), "{:?}", model.problem());
}

#[test]
fn a_malformed_noise_model_is_refused() {
    for mutate in [
        (|m: &mut NoiseModel| m.camera.clear()) as fn(&mut NoiseModel),
        |m: &mut NoiseModel| m.iso = 0,
        |m: &mut NoiseModel| m.read = -1.0,
        |m: &mut NoiseModel| m.shot = f32::NAN,
    ] {
        let mut model = NoiseModel::reference();
        mutate(&mut model);
        assert!(model.problem().is_some(), "{model:?} was accepted");
    }
}

// ---------------------------------------------------------------------------
// The ceilings
// ---------------------------------------------------------------------------

#[test]
fn every_ceiling_refuses_the_value_above_it() {
    // Section 5's numbers, each asserted to actually refuse. A ceiling that is only documented
    // is a ceiling until somebody writes a second caller.
    let mut over_amount = sharpen();
    over_amount.amount = MAX_SHARPEN_AMOUNT + 0.01;
    assert!(over_amount.problem().is_some(), "the amount cap is inert");

    let mut small_kernel = sharpen();
    small_kernel.kernel_sigma = SHARPEN_KERNEL_LO - 0.01;
    assert!(
        small_kernel.problem().is_some(),
        "the kernel floor is inert"
    );

    let mut big_kernel = sharpen();
    big_kernel.kernel_sigma = SHARPEN_KERNEL_HI + 0.01;
    assert!(
        big_kernel.problem().is_some(),
        "the kernel ceiling is inert"
    );

    let mut weak_skin = sharpen();
    weak_skin.skin_attenuation = SKIN_ATTENUATION - 0.01;
    assert!(
        weak_skin.problem().is_some(),
        "a plan may attenuate skin less than the contract requires"
    );

    let mut many_iterations = sharpen();
    many_iterations.iterations = MAX_DECONV_ITERATIONS + 1;
    assert!(many_iterations.problem().is_some());

    let mut zero_iterations = sharpen();
    zero_iterations.iterations = 0;
    assert!(zero_iterations.problem().is_some());

    // The face-recovery cap, on the plan.
    let mut too_strong = plan();
    too_strong.face_recovery = Some(MAX_FACE_RECOVERY + 0.01);
    assert!(too_strong.broken_guarantee().is_some());
}

#[test]
fn sharpening_may_not_run_without_regions_from_phase_18() {
    // ADR-0047 section 4, bullet 4, as a structural refusal rather than as an intention. The
    // obvious alternative - sharpen the whole frame at a lower amount - is what produces a
    // crunchy sky, and it is not representable on a sound plan.
    let mut blind = sharpen();
    blind.mask.from_regions = false;
    assert!(blind.problem().is_some(), "a blind sharpen was accepted");

    let mut unexcluded = sharpen();
    unexcluded.mask.excluded = [false; RestoreRegion::COUNT];
    assert!(
        unexcluded.problem().is_some(),
        "sky and bokeh were not excluded and the spec was accepted"
    );

    let mut acted = plan();
    acted.sharpen = Some(sharpen());
    acted.selfcheck = Some(clean_report());
    acted.region_covered = false;
    assert!(
        acted.broken_guarantee().is_some(),
        "a plan sharpened a frame it had no regions for"
    );
}

#[test]
fn chroma_reduction_is_never_below_luminance_reduction() {
    // Section 6.1: chroma noise carries no detail anybody wants and luminance noise is half a
    // stop from being grain. A build that inverts the two smears fabric to remove grain.
    let mut inverted = spec();
    inverted.luminance = 0.60;
    inverted.colour = 0.30;
    assert!(
        inverted.problem().is_some(),
        "an inverted spec was accepted"
    );
    assert!(spec().problem().is_none(), "{:?}", spec().problem());
}

// ---------------------------------------------------------------------------
// The identity guarantee
// ---------------------------------------------------------------------------

#[test]
fn a_kept_face_above_the_identity_ceiling_is_not_a_sound_plan() {
    // **The guarantee of this phase.** Section 6.3 says the product never changes what someone
    // looks like, and this is that sentence as a state the type system refuses to call sound.
    let kept = RecoveredFace {
        identity: None,
        bounds: box_at(0.4, 0.3),
        sharpness: 0.55,
        strength: 0.20,
        identity_drift: MAX_IDENTITY_DRIFT + 0.001,
        resolves: 3,
        skipped: false,
        skipped_because: None,
    };
    assert!(
        kept.problem().is_some(),
        "a face was kept above the identity ceiling"
    );

    let mut acted = plan();
    acted.face_recovery = Some(0.20);
    acted.recovered = vec![kept];
    acted.selfcheck = Some(clean_report());
    assert!(acted.broken_guarantee().is_some());

    // The same face, skipped, is sound - and it has to carry a reason.
    let mut skipped = acted.recovered[0].clone();
    skipped.skipped = true;
    skipped.strength = 0.0;
    skipped.skipped_because = Some(RestoreCode::IdentityDriftSkipped);
    assert!(skipped.problem().is_none(), "{:?}", skipped.problem());

    let mut reasonless = skipped.clone();
    reasonless.skipped_because = None;
    assert!(
        reasonless.problem().is_some(),
        "a skipped face had no reason"
    );

    let mut still_acting = skipped;
    still_acting.skipped_because = Some(RestoreCode::IdentityDriftSkipped);
    still_acting.strength = 0.10;
    assert!(still_acting.problem().is_some());
}

#[test]
fn the_soft_face_band_is_narrow_and_has_a_floor() {
    // Section 6.3: never on heavily blurred faces, where a prior returns the prior. The floor is
    // what makes that structural, and the band being narrow is what stops a face-prior model
    // running on faces that did not need it.
    const { assert!(SOFT_FACE_LO > 0.0, "there is no floor at all") };
    const { assert!(SOFT_FACE_LO < SOFT_FACE_HI, "the band is inverted") };
    const {
        assert!(
            SOFT_FACE_HI - SOFT_FACE_LO < 0.35,
            "the soft-face band is not narrow"
        )
    };
    // And a plan may not carry more face records than the cap, so a 60-face frame cannot write
    // 60 rows that all say the same thing.
    let mut crowded = plan();
    crowded.face_recovery = Some(0.10);
    crowded.selfcheck = Some(clean_report());
    crowded.recovered = (0..=MAX_RECOVERED_FACES)
        .map(|i| RecoveredFace {
            identity: None,
            bounds: box_at(0.01 * i as f32, 0.0),
            sharpness: 0.55,
            strength: 0.0,
            identity_drift: 0.0,
            resolves: 0,
            skipped: true,
            skipped_because: Some(RestoreCode::FaceSharpEnough),
        })
        .collect();
    assert!(crowded.broken_guarantee().is_some());
}

#[test]
fn the_plan_counts_its_own_refusals() {
    let mut acted = plan();
    acted.face_recovery = Some(0.20);
    acted.selfcheck = Some(clean_report());
    acted.recovered = vec![
        RecoveredFace {
            identity: None,
            bounds: box_at(0.1, 0.1),
            sharpness: 0.55,
            strength: 0.20,
            identity_drift: 0.03,
            resolves: 0,
            skipped: false,
            skipped_because: None,
        },
        RecoveredFace {
            identity: None,
            bounds: box_at(0.5, 0.1),
            sharpness: 0.50,
            strength: 0.0,
            identity_drift: 0.11,
            resolves: 3,
            skipped: true,
            skipped_because: Some(RestoreCode::IdentityDriftSkipped),
        },
    ];
    assert!(acted.is_sound(), "{:?}", acted.broken_guarantee());
    assert_eq!(acted.faces_recovered(), 1);
    assert_eq!(acted.faces_skipped_for_identity(), 1);
    // The worst *kept* drift, not the worst drift: a refused face is the guarantee working and
    // must not read as a violation of it.
    assert!((acted.worst_kept_drift() - 0.03).abs() < 1e-6);
}

// ---------------------------------------------------------------------------
// The plan's own checks
// ---------------------------------------------------------------------------

#[test]
fn a_plan_with_no_reason_is_refused() {
    let mut empty = plan();
    empty.reasons.clear();
    assert!(empty.broken_guarantee().is_some(), "invariant 2 is inert");
}

#[test]
fn a_tier_without_a_spec_and_a_spec_without_a_tier_are_both_refused() {
    // A tier alone is not reproducible - the same `Standard` on two bodies at two ISOs is two
    // different renders - so a stored plan that names one without the numbers it became is a
    // plan phase 27 cannot audit. ADR-0047 section 2.1.
    let mut bare = plan();
    bare.denoise = DenoiseTier::Standard;
    bare.selfcheck = Some(clean_report());
    assert!(
        bare.broken_guarantee().is_some(),
        "a bare tier was accepted"
    );

    let mut orphan = plan();
    orphan.denoise_spec = Some(spec());
    assert!(
        orphan.broken_guarantee().is_some(),
        "an orphan spec was accepted"
    );

    let mut sound = plan();
    sound.denoise = DenoiseTier::Standard;
    sound.denoise_spec = Some(spec());
    sound.selfcheck = Some(clean_report());
    assert!(sound.is_sound(), "{:?}", sound.broken_guarantee());
    assert!(!sound.is_noop());
}

#[test]
fn a_plan_that_moved_pixels_carries_a_self_check() {
    let mut acted = plan();
    acted.denoise = DenoiseTier::Light;
    acted.denoise_spec = Some(spec());
    assert!(
        acted.broken_guarantee().is_some(),
        "a plan changed pixels with nothing measured on the result"
    );

    // And a stored self-check may never still be outside its own bounds: the guard re-solves
    // until it is clean or the operation is withdrawn, so a dirty report reaching the store is a
    // defect rather than a state.
    let mut dirty = acted.clone();
    let mut report = clean_report();
    report.texture_retention = MIN_TEXTURE_RETENTION - 0.05;
    dirty.selfcheck = Some(report);
    assert!(dirty.broken_guarantee().is_some());

    let mut ringing = acted;
    let mut report = clean_report();
    report.ringing = MAX_RINGING * 2.0;
    ringing.selfcheck = Some(report);
    assert!(ringing.broken_guarantee().is_some());
}

#[test]
fn a_report_that_measured_nothing_cannot_claim_a_violation() {
    // Phase 21's rule: a ratio over eleven samples is arithmetic rather than evidence. A report
    // with `measured_on = 0` describes a frame nothing was rendered for, and a violation on one
    // is a bug in the measurement rather than a finding about the photograph.
    let mut report = ArtefactReport::UNTOUCHED;
    report.ringing = MAX_RINGING * 3.0;
    assert!(report.problem().is_some());
    assert!(ArtefactReport::UNTOUCHED.problem().is_none());
    assert!(ArtefactReport::UNTOUCHED.is_clean());
}

#[test]
fn the_frozen_denoise_reason_field_is_still_answerable() {
    // Section 5 freezes `denoise_reason`; ADR-0047 section 2.1 widens the list to three
    // decisions and keeps the frozen question answerable. This is that promise.
    let mut acted = plan();
    acted.reasons = vec![
        RestoreReason::plain(RestoreCode::TierFromMeasuredNoise, 0.6),
        RestoreReason::plain(RestoreCode::KernelTooLarge, -0.3),
        RestoreReason::plain(RestoreCode::FaceTooBlurred, -0.2),
    ];
    assert_eq!(acted.denoise_reasons().len(), 1);
    assert_eq!(acted.reasons_for(RestoreSubject::Sharpen).len(), 1);
    assert_eq!(acted.reasons_for(RestoreSubject::FaceRecovery).len(), 1);
    assert_eq!(acted.reasons_for(RestoreSubject::Plan).len(), 0);
}

#[test]
fn a_plan_above_the_reason_cap_is_refused() {
    let mut noisy = plan();
    noisy.reasons = RestoreCode::ALL
        .iter()
        .take(RestorePlan::MAX_REASONS + 1)
        .map(|code| RestoreReason::plain(*code, 0.1))
        .collect();
    assert!(noisy.broken_guarantee().is_some());
}

// ---------------------------------------------------------------------------
// What is deliberately absent
// ---------------------------------------------------------------------------

#[test]
fn nothing_in_this_contract_can_express_an_upscale_or_a_synthesis() {
    // Section 2.2 puts upscaling beyond native resolution and generative reconstruction out of
    // scope. The enforcement is that there is nowhere to put either, and this is what turns that
    // from an observation into a rule: the debug rendering of every shape in this contract is
    // scanned for a field name that would carry one.
    let sharpen = sharpen();
    let field = RestoreField {
        region: RestoreRegion::Subject,
        identity: None,
        bounds: box_at(0.0, 0.0),
        width: 2,
        height: 2,
        alpha: vec![255; 4],
        confidence: 0.9,
        edge_quality: 0.9,
        model_ver: 1,
    };
    let rendered = format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}",
        plan(),
        sharpen,
        spec(),
        field,
        clean_report(),
        NoiseModel::reference()
    );
    for forbidden in [
        "scale",
        "upscale",
        "resample",
        "synthes",
        "generat",
        "inpaint",
        "landmark",
        "warp",
        "displace",
        "source_image",
        "borrowed",
    ] {
        assert!(
            !rendered.to_lowercase().contains(forbidden),
            "a restoration shape carries a `{forbidden}` field"
        );
    }
}

#[test]
fn restoration_can_never_be_scheduled_onto_the_interactive_path() {
    // Section 6.4, as a type with no third variant. There is nowhere to say "interactive".
    assert_eq!(RestoreWhen::ALL.len(), 2);
    for when in RestoreWhen::ALL {
        assert_ne!(when.as_str(), "interactive");
        assert_eq!(RestoreWhen::parse(when.as_str()), Some(when));
    }
}

#[test]
fn the_cloud_destination_exists_and_is_the_only_one_that_leaves_the_device() {
    // ADR-0047 section 7: the variant is frozen by section 5 and nothing in this build returns
    // it. What the contract can say is which destination sends the photograph away, so that a
    // consent check has one predicate to read rather than a match somebody could widen.
    for destination in RunWhere::ALL {
        assert_eq!(
            destination.leaves_the_device(),
            destination == RunWhere::Cloud
        );
        assert_eq!(RunWhere::parse(destination.as_str()), Some(destination));
    }
}

#[test]
fn an_override_has_a_tier_and_no_strength_anywhere() {
    // Phase 21's rule, inherited: a ceiling can be lowered by a studio and raised by nobody. A
    // photographer chooses which tier; how far that tier goes is a product decision.
    let rendered = format!("{:?}", RestoreOverride::default());
    for forbidden in ["strength", "ceiling", "amount", "sigma"] {
        assert!(
            !rendered.to_lowercase().contains(forbidden),
            "the override carries a `{forbidden}` field"
        );
    }
    assert!(RestoreOverride::default().is_empty());
    assert!(RestoreOverride::default().problem().is_some());

    let chosen = RestoreOverride {
        denoise: Some(DenoiseTier::Light),
        ..RestoreOverride::default()
    };
    assert!(!chosen.is_empty());
    assert!(chosen.problem().is_none());
}
