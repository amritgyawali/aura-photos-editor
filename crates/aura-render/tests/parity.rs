//! The parity harness, tested against a perturbation rather than against a device.
//!
//! There is no accelerator in this build (ADR-0029 section 4), so nothing can disagree with
//! the reference. What can still be established - and what matters the day a backend lands -
//! is that the harness **catches** a disagreement. So the reference is compared against a
//! deliberately shifted copy of itself, at several magnitudes, and the gate is asserted to
//! fire at exactly the documented tolerance.
//!
//! A green run of this file means "the harness works". It does not mean "the accelerator
//! agrees", and the phase 14 exit report says so in the same words.

use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_recipe::fixtures as recipes;
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::CpuEngine;
use aura_render::gpu::{Absent, GpuBackend};
use aura_render::graph::{self, Capabilities, InputKind, Stage};
use aura_render::{fixtures, parity};
use time::OffsetDateTime;

fn reference_buffer() -> Vec<f32> {
    let frame = fixtures::detail_frame(96, 72);
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame.clone())),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    );
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.global.exposure = 0.4;
    recipe.global.contrast = 25;
    recipe.global.vibrance = 30;

    let plan = graph::plan(
        &recipe,
        RenderPurpose::Export,
        InputKind::Working,
        Capabilities::default(),
    );
    let (rgb, _, _, _) = engine.working_buffer(&frame, &recipe, &plan, RenderLevel::Full, None);
    rgb
}

#[test]
fn the_harness_passes_a_backend_that_agrees() {
    let reference = reference_buffer();
    let comparison = parity::compare(Stage::Exposure, &reference, &reference);
    comparison.check().expect("identical buffers must agree");
    assert!(comparison.samples > 10_000);
}

#[test]
fn the_harness_catches_a_backend_that_drifts() {
    let reference = reference_buffer();
    for factor in [1.01f32, 1.5, 4.0, 100.0] {
        let candidate = parity::perturb(&reference, parity::TOLERANCE * factor);
        let comparison = parity::compare(Stage::Tone, &reference, &candidate);
        let err = comparison
            .check()
            .expect_err("a drift of {factor}x the tolerance must fail");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8010");
    }
}

#[test]
fn the_harness_passes_a_drift_inside_the_tolerance() {
    let reference = reference_buffer();
    for factor in [0.0f32, 0.1, 0.5, 0.99] {
        let candidate = parity::perturb(&reference, parity::TOLERANCE * factor);
        parity::compare(Stage::Curve, &reference, &candidate)
            .check()
            .expect("inside the tolerance");
    }
}

#[test]
fn a_parity_failure_names_the_stage_and_blocks_the_build() {
    let reference = reference_buffer();
    let candidate = parity::perturb(&reference, 0.05);
    let err = parity::compare(Stage::Hsl, &reference, &candidate)
        .check()
        .expect_err("must fail");
    assert_eq!(err.severity, aura_core::Severity::RunBlocking);
    assert!(err.context.iter().any(|(k, v)| k == "stage" && v == "hsl"));
    assert!(err.context.iter().any(|(k, _)| k == "tolerance"));
}

#[test]
fn every_stage_would_be_compared_by_the_gate() {
    // The day a backend lands, `check_all` runs over one comparison per stage. Asserting the
    // shape of that list now is what stops a stage being added later and quietly never
    // compared.
    let reference = vec![0.25f32; 300];
    let comparisons: Vec<_> = graph::ORDER
        .iter()
        .map(|stage| parity::compare(*stage, &reference, &reference))
        .collect();
    assert_eq!(comparisons.len(), graph::ORDER.len());
    parity::check_all(&comparisons).expect("a reference agrees with itself everywhere");
}

#[test]
fn this_build_has_no_backend_to_compare_against() {
    // Stated as a test so that the day one appears, this fails and somebody has to come here
    // and wire the real comparison in.
    let backend = Absent;
    let unsupported: Vec<&str> = graph::ORDER
        .iter()
        .filter(|stage| !backend.supports(**stage))
        .map(|stage| stage.as_str())
        .collect();
    assert_eq!(
        unsupported.len(),
        graph::ORDER.len(),
        "a backend appeared; wire it into the parity gate"
    );
    assert_eq!(backend.max_texture(), 0);
}

#[test]
fn a_render_reports_the_missing_backend_as_a_degradation() {
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(fixtures::grey_frame(8, 8, 0.2))),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    );
    use aura_render::contract::render::RenderService;
    let caps = engine.capabilities();
    assert_eq!(caps.backend, aura_render::Backend::Cpu);
    assert_eq!(
        caps.precision_bits, 24,
        "f32 arithmetic, ADR-0029 section 3"
    );
    let err = engine.degradation().expect("a degradation is reported");
    assert_eq!(err.code.to_string(), "AURA-RENDER-8001");
    let _ = OutputSpec::default();
}
