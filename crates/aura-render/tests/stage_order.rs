//! The pipeline order, asserted rather than commented.
//!
//! Three orderings in ADR-0029 section 2 are load-bearing, and each of them fails in a way
//! that looks like a colour bug rather than like an ordering bug - which is why they are
//! tested here and not left to a diagram.

use aura_recipe::fixtures as recipes;
use aura_render::contract::render::{RenderPurpose, SkipReason};
use aura_render::graph::{self, Capabilities, InputKind, Stage, ORDER};

#[test]
fn highlight_recovery_runs_before_white_balance() {
    assert!(
        Stage::HighlightRecovery.index() < Stage::WhiteBalance.index(),
        "a clipped green channel reconstructed after a 1.9x red multiplier is a magenta veil \
         across every white dress and every window"
    );
}

#[test]
fn white_balance_runs_before_the_camera_matrix() {
    assert!(
        Stage::WhiteBalance.index() < Stage::CameraMatrix.index(),
        "white balance is a per-channel multiplier on camera-native RGB; after the matrix it \
         would be multiplying Rec.2020 primaries and would shift hue"
    );
}

#[test]
fn the_output_transform_is_last_and_nothing_follows_it() {
    assert_eq!(ORDER.last(), Some(&Stage::OutputTransform));
    assert_eq!(Stage::OutputTransform.index(), ORDER.len() - 1);
}

#[test]
fn geometry_runs_after_every_stage_that_reads_a_neighbourhood() {
    // Cropping before sharpening would sharpen against pixels that are no longer in the
    // frame at its edges, so a cropped export would have a different edge from an uncropped
    // one. Cropping after is what makes the tiler able to treat a crop as a tile grid.
    for spatial in [
        Stage::NoiseReduction,
        Stage::Clarity,
        Stage::Texture,
        Stage::Dehaze,
        Stage::Sharpen,
    ] {
        assert!(
            spatial.index() < Stage::Geometry.index(),
            "{spatial:?} must run before the crop"
        );
    }
}

#[test]
fn tone_runs_before_local_contrast_and_local_contrast_before_sharpening() {
    assert!(Stage::Tone.index() < Stage::Clarity.index());
    assert!(Stage::Clarity.index() < Stage::Sharpen.index());
}

#[test]
fn masks_run_after_the_global_grade_and_before_sharpening() {
    // A local exposure lift is relative to the graded frame, not to the raw one: a
    // photographer who lifts a face by a third of a stop expects a third of a stop *of what
    // they can see*.
    assert!(Stage::Vibrance.index() < Stage::Masks.index());
    assert!(Stage::Masks.index() < Stage::Sharpen.index());
}

#[test]
fn a_plan_never_reorders_the_stages_it_selects() {
    let mut recipe = recipes::reference();
    recipe.global.dehaze = 20;
    recipe.lens.vignette = 30;

    let plan = graph::plan(
        &recipe,
        RenderPurpose::Export,
        InputKind::CameraNative,
        Capabilities::default(),
    );
    let indices: Vec<usize> = plan.stages.iter().map(|s| s.index()).collect();
    let mut sorted = indices.clone();
    sorted.sort_unstable();
    assert_eq!(
        indices, sorted,
        "a plan is a filter of ORDER, never a re-sort"
    );
}

#[test]
fn every_skipped_stage_carries_a_reason() {
    let recipe = recipes::reference();
    let plan = graph::plan(
        &recipe,
        RenderPurpose::Interactive,
        InputKind::Working,
        Capabilities::default(),
    );
    // Every stage is either scheduled or noted. Nothing may simply be absent.
    for stage in ORDER {
        let scheduled = plan.stages.contains(&stage);
        let noted = plan.notes.iter().any(|n| n.stage == stage.as_str());
        assert!(
            scheduled || noted,
            "{} is neither run nor explained",
            stage.as_str()
        );
    }
}

#[test]
fn the_notes_that_matter_are_the_ones_a_photographer_can_act_on() {
    let plan = graph::plan(
        &recipes::reference(),
        RenderPurpose::Export,
        InputKind::Working,
        Capabilities::default(),
    );
    assert!(
        plan.has_caveats(),
        "the reference recipe asks for a face mask"
    );
    // "not requested" and "already done upstream" are not caveats. Reporting them would bury
    // the ones that are.
    assert!(!SkipReason::NotRequested.is_a_caveat());
    assert!(!SkipReason::UpstreamAlready.is_a_caveat());
    assert!(SkipReason::MaskGeneratorAbsent.is_a_caveat());
    assert!(SkipReason::OperatorAbsent.is_a_caveat());
}
