//! PHASE-22 section 10.1: "Order of operations enforced (denoise before retouch, sharpen last) -
//! render-graph test."
//!
//! This is that test, and it is a separate file from `stage_order.rs` because the two prove
//! different things. That file asserts the properties phase 14 froze about its own pipeline; this
//! one asserts the four properties **phase 22's scope contract requires of it**, plus the two
//! routing changes ADR-0047 section 2 makes.
//!
//! The reason the file exists at all is that section 2.1's requirement and phase 14's frozen
//! `ORDER` looked incompatible: `Stage::Restoration` sits at index 19, *after* `Stage::Retouch`
//! at 18, and phase 14 documented it as "Denoise, face recovery, deblur". They are compatible
//! because restoration is not one stage - denoising is a sensor-domain operation and belongs at
//! `Stage::NoiseReduction`, thirteen stages earlier, which is where phase 14 already put it.

use aura_recipe::fixtures;
use aura_render::contract::render::{RenderPurpose, SkipReason};
use aura_render::graph::{self, Capabilities, InputKind, Stage};

fn caps() -> Capabilities {
    Capabilities {
        mask_generators: true,
        retouch_operators: true,
        restoration: true,
        geometry_models: true,
    }
}

// ---------------------------------------------------------------------------
// Section 2.1's order requirement
// ---------------------------------------------------------------------------

#[test]
fn denoise_runs_before_local_retouch() {
    // The first half of section 2.1. Every stage between the two treats noise as signal: clarity,
    // texture and dehaze amplify it, and retouch's frequency separation splits it into bands. A
    // denoise after retouch is retouching noise and then removing the retouched noise.
    assert!(
        Stage::NoiseReduction.index() < Stage::Retouch.index(),
        "denoise at {} runs after retouch at {}",
        Stage::NoiseReduction.index(),
        Stage::Retouch.index()
    );
    assert!(Stage::NoiseReduction.index() < Stage::Restoration.index());
    assert!(Stage::NoiseReduction.index() < Stage::Masks.index());
}

#[test]
fn denoise_runs_before_sharpening_and_before_every_stage_that_amplifies_it() {
    // The second half of the same sentence, and the four stages that make it matter.
    assert!(Stage::NoiseReduction.index() < Stage::Sharpen.index());
    for amplifier in [
        Stage::Clarity,
        Stage::Texture,
        Stage::Dehaze,
        Stage::Contrast,
    ] {
        assert!(
            Stage::NoiseReduction.index() < amplifier.index(),
            "{} runs before denoising and will amplify grain into structure",
            amplifier.as_str()
        );
    }
}

#[test]
fn sharpening_is_the_last_stage_that_changes_a_pixel_value() {
    // "sharpening as the last pixel operation before output transform". Two stages follow it and
    // neither changes what a pixel *is*: geometry resamples and the output transform encodes. A
    // build that inserted a new tonal or spatial stage after `Sharpen` would be sharpening a frame
    // and then changing it, which is how a gallery ends up looking different from its own preview.
    let after: Vec<Stage> = graph::ORDER
        .iter()
        .copied()
        .filter(|stage| stage.index() > Stage::Sharpen.index())
        .collect();
    assert_eq!(
        after,
        vec![Stage::Geometry, Stage::OutputTransform],
        "something other than geometry and the output transform runs after sharpening"
    );
}

#[test]
fn face_recovery_runs_after_retouch_and_before_sharpening() {
    // Where the third operation of this phase actually sits. It has to be after retouch because
    // it must run on the face that will be delivered, and before sharpening because a recovered
    // face that is then deconvolved has been sharpened twice.
    assert!(Stage::Retouch.index() < Stage::Restoration.index());
    assert!(Stage::Restoration.index() < Stage::Sharpen.index());
}

// ---------------------------------------------------------------------------
// ADR-0047 section 2's two routing changes
// ---------------------------------------------------------------------------

#[test]
fn the_denoise_tier_invalidates_from_the_noise_reduction_stage() {
    // The bug fix half of ADR-0047 section 2. `earliest_affected` answers "from which stage must
    // this render be recomputed"; a tier change that answered "from stage 19" would let a cache
    // serve the buffer it had already denoised under the previous tier.
    assert_eq!(
        graph::stage_for("restoration.denoise"),
        Some(Stage::NoiseReduction)
    );
    assert_eq!(
        graph::earliest_affected(&["restoration.denoise".to_string()]),
        Some(Stage::NoiseReduction)
    );
    // A tier change alongside a face-recovery change still invalidates from the earlier of the
    // two, which is the whole point of `earliest_affected` returning a minimum.
    assert_eq!(
        graph::earliest_affected(&[
            "restoration.face_recovery".to_string(),
            "restoration.denoise".to_string(),
        ]),
        Some(Stage::NoiseReduction)
    );
}

#[test]
fn the_other_restoration_fields_still_invalidate_from_the_restoration_stage() {
    for path in ["restoration.face_recovery", "restoration.deblur"] {
        assert_eq!(
            graph::stage_for(path),
            Some(Stage::Restoration),
            "{path} does not route to the restoration stage"
        );
    }
}

#[test]
fn a_tier_alone_does_not_enable_the_restoration_stage() {
    // The other half of ADR-0047 section 2. Denoising happens at stage 6, so a plan that reported
    // `Stage::Restoration` for a denoise-only frame would be reporting a stage that ran and
    // changed nothing - and `RenderNote` exists so a photographer can tell what did and did not
    // happen.
    let mut recipe = fixtures::neutral("hash22", "reference");
    recipe.restoration.denoise = "standard".to_string();
    recipe.global.noise.luminance = 30;
    recipe.global.noise.colour = 45;
    recipe.global.noise.model = "restore_reference_v1".to_string();

    let plan = graph::plan(
        &recipe,
        RenderPurpose::Export,
        InputKind::CameraNative,
        caps(),
    );
    assert!(
        plan.stages.contains(&Stage::NoiseReduction),
        "a denoise tier did not enable the stage that performs it"
    );
    assert!(
        !plan.stages.contains(&Stage::Restoration),
        "a denoise tier enabled a stage with nothing to do"
    );
    let note = plan
        .notes
        .iter()
        .find(|note| note.stage == Stage::Restoration.as_str());
    assert_eq!(
        note.map(|note| note.reason),
        Some(SkipReason::NotRequested),
        "the skipped restoration stage carries the wrong reason"
    );
}

#[test]
fn face_recovery_enables_the_restoration_stage_and_says_when_it_cannot() {
    let mut recipe = fixtures::neutral("hash22", "reference");
    recipe.restoration.face_recovery = 20;

    let with_models = graph::plan(
        &recipe,
        RenderPurpose::Export,
        InputKind::CameraNative,
        caps(),
    );
    assert!(with_models.stages.contains(&Stage::Restoration));

    // A build with no restoration models says so rather than silently doing nothing. Phase 14's
    // rule: a render says what it skipped, and `SkipReason::RestorationAbsent` names the phase.
    let without = graph::plan(
        &recipe,
        RenderPurpose::Export,
        InputKind::CameraNative,
        Capabilities::default(),
    );
    assert!(!without.stages.contains(&Stage::Restoration));
    let note = without
        .notes
        .iter()
        .find(|note| note.stage == Stage::Restoration.as_str())
        .expect("a skipped restoration stage carries a note");
    assert_eq!(note.reason, SkipReason::RestorationAbsent);
    assert!(note.reason.is_a_caveat());
}

#[test]
fn restoration_never_runs_on_the_interactive_path() {
    // Section 6.4, at the render graph rather than at the type. `RestoreWhen` has no interactive
    // variant, and this is the second layer: even a recipe that asks for face recovery does not
    // get it while somebody is dragging a slider.
    let mut recipe = fixtures::neutral("hash22", "reference");
    recipe.restoration.face_recovery = 20;

    let purpose = RenderPurpose::Interactive;
    let plan = graph::plan(&recipe, purpose, InputKind::CameraNative, caps());
    assert!(
        !plan.stages.contains(&Stage::Restoration),
        "restoration ran on the {purpose:?} path"
    );
    let note = plan
        .notes
        .iter()
        .find(|note| note.stage == Stage::Restoration.as_str())
        .expect("a skipped restoration stage carries a note");
    assert_eq!(note.reason, SkipReason::InteractivePath);
    assert!(Stage::Restoration.is_heavy());
}

#[test]
fn denoising_is_not_heavy_and_therefore_survives_the_interactive_path() {
    // The consequence of splitting the phase across two stages, and it is a feature rather than
    // an accident: a photographer editing a dance-floor frame sees it denoised while they work,
    // because the denoise is at stage 6 with the rest of the tonal pipeline. Only the two
    // expensive operations wait for export.
    assert!(!Stage::NoiseReduction.is_heavy());

    let mut recipe = fixtures::neutral("hash22", "reference");
    recipe.global.noise.luminance = 30;
    recipe.global.noise.colour = 45;

    let plan = graph::plan(
        &recipe,
        RenderPurpose::Interactive,
        InputKind::CameraNative,
        caps(),
    );
    assert!(plan.stages.contains(&Stage::NoiseReduction));
}
