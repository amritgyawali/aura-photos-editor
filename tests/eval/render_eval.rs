//! PHASE-14 section 10.1, as a test suite. A red gate is a red build.
//!
#![allow(clippy::disallowed_methods)] // test code: an unwrap here is an assertion

//! Seven rows are named there. Six are asserted here and the seventh - the 100 MP export
//! inside a VRAM budget - is asserted as the budget arithmetic plus the tiling identity,
//! because allocating 100 MP of `f32` inside a CI test is the very thing the streaming
//! exists to avoid.
//!
//! # What a green run of this file means
//!
//! That the arithmetic is stable, that a tiled render equals a whole one, that a recipe
//! round-trips, that a photographer's slider survives every automated pass, and that a v1
//! document still renders under a simulated v2.
//!
//! # What it does not mean
//!
//! That any colour is *correct*. The fixtures are authored synthetic pixels and the bodies
//! are eight synthetic bench profiles, because there are no camera files and no photographed
//! ColorChecker in this repository - phase 02's exit conditions, carried forward through
//! ADR-0006 and still open. Section 10.1 asks for twelve camera bodies across six scene
//! types; what runs here is eight synthetic bodies across five authored frames. That gap is
//! condition C2 in the phase 14 exit report and it is printed by the phase gate on every run
//! rather than left in a comment.

use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_recipe::{fixtures as recipes, schema, EditSource, Recipe};
use aura_render::contract::render::{
    OutputColour, OutputSpec, RenderLevel, RenderPurpose, RenderedData,
};
use aura_render::cpu::CpuEngine;
use aura_render::{fixtures, golden, graph, tiles};
use time::OffsetDateTime;

fn engine(frame: aura_render::Frame) -> CpuEngine {
    CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame)),
        FixedClock::at(OffsetDateTime::UNIX_EPOCH),
    )
}

fn graded() -> Recipe {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.global.exposure = 0.31;
    recipe.global.contrast = 11;
    recipe.global.temperature = 4930;
    recipe.global.tint = 8;
    recipe.global.highlights = -31;
    recipe.global.shadows = 22;
    recipe.global.whites = 7;
    recipe.global.blacks = -9;
    recipe.global.clarity = 6;
    recipe.global.texture = 4;
    recipe.global.vibrance = 8;
    recipe.global.sharpen.amount = 40;
    recipe
}

// ---------------------------------------------------------------------------
// Row 1. Golden renders match the reference within tolerance, across bodies and scenes.
// ---------------------------------------------------------------------------

#[test]
fn golden_renders_are_stable_across_every_bench_body() {
    // Stability, not accuracy. See the module documentation.
    let recipe = graded();
    for frame in fixtures::bench_bodies(96, 72) {
        let engine = engine(frame.clone());
        let first = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("first");
        let second = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("second");

        let result = golden::compare(&first.data, &second.data);
        assert!(result.identical, "{} is not deterministic", frame.camera);
        assert!(result.within_tolerance);
        assert_eq!(golden::digest(&first.data), golden::digest(&second.data));
    }
}

#[test]
fn different_bodies_produce_different_renders() {
    // The cross-body suite would prove nothing if the eight profiles collapsed to one.
    let recipe = graded();
    let mut digests = std::collections::BTreeSet::new();
    for frame in fixtures::bench_bodies(64, 48) {
        let engine = engine(frame.clone());
        let out = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("render");
        digests.insert(golden::digest(&out.data));
    }
    assert_eq!(digests.len(), 8, "eight bodies must render eight ways");
}

#[test]
fn every_scene_fixture_renders_and_stays_in_range() {
    let recipe = graded();
    let scenes: Vec<(&str, aura_render::Frame)> = vec![
        ("flat neutral", fixtures::grey_frame(64, 48, 0.18)),
        ("scene-referred ramp", fixtures::ramp_frame(64, 48)),
        ("multi-scale detail", fixtures::detail_frame(64, 48)),
        ("clipped highlight", fixtures::clipped_frame(64, 48)),
        ("colour patches", fixtures::patch_frame(8)),
    ];
    for (name, frame) in scenes {
        let engine = engine(frame.clone());
        let out = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect(name);
        let RenderedData::Eight(bytes) = &out.data else {
            panic!("{name}: eight-bit output requested");
        };
        assert_eq!(
            bytes.len(),
            (out.width as usize) * (out.height as usize) * 3
        );
        // Nothing NaN, nothing negative: the type says so, but a stage that produced a NaN
        // would quantise to zero and look like a black frame rather than like a bug.
        assert!(bytes.iter().any(|b| *b > 0), "{name} rendered black");
    }
}

#[test]
fn a_neutral_render_stays_neutral() {
    // The most useful assertion in the suite: a stage with a channel-order bug or a matrix
    // that is not white-normalised tints a grey, and nothing else catches it.
    let frame = fixtures::grey_frame(32, 32, 0.18);
    let engine = engine(frame.clone());
    for space in [
        OutputColour::Srgb,
        OutputColour::AdobeRgb,
        OutputColour::DisplayP3,
    ] {
        let out = engine
            .render_frame(
                &frame,
                &recipes::neutral(recipes::FIXTURE_HASH, "Bench-01"),
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec {
                    colour_space: space,
                    bit_depth: 8,
                    icc: None,
                },
            )
            .expect("render");
        let RenderedData::Eight(bytes) = &out.data else {
            panic!("eight-bit");
        };
        for triple in bytes.chunks(3) {
            let r = triple.first().copied().unwrap_or(0);
            let g = triple.get(1).copied().unwrap_or(0);
            let b = triple.get(2).copied().unwrap_or(0);
            assert!(
                r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1,
                "{space:?} tinted a neutral: {r},{g},{b}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Row 2. GPU vs CPU parity within 1/1024 per stage; failure blocks the build.
// ---------------------------------------------------------------------------

#[test]
fn the_parity_gate_is_wired_for_every_stage_and_catches_drift() {
    use aura_render::parity;

    let reference = vec![0.42f32; 3_000];
    let comparisons: Vec<_> = graph::ORDER
        .iter()
        .map(|stage| parity::compare(*stage, &reference, &reference))
        .collect();
    assert_eq!(comparisons.len(), graph::ORDER.len());
    parity::check_all(&comparisons).expect("a reference agrees with itself");

    let drifted = parity::perturb(&reference, parity::TOLERANCE * 2.0);
    let err = parity::compare(graph::Stage::Tone, &reference, &drifted)
        .check()
        .expect_err("drift must fail");
    assert_eq!(err.code.to_string(), "AURA-RENDER-8010");
    assert_eq!(err.severity, aura_core::Severity::RunBlocking);
}

// ---------------------------------------------------------------------------
// Row 3. Recipe round-trip produces an identical hash; XMP preserves its subset.
// ---------------------------------------------------------------------------

#[test]
fn a_recipe_round_trip_produces_an_identical_render_hash() {
    let recipe = recipes::reference();
    let text = aura_recipe::canonical(&recipe).expect("canonical");
    let parsed: Recipe = serde_json::from_str(&text).expect("parse");

    let spec = OutputSpec::default();
    assert_eq!(
        graph::render_hash(&recipe.image.content_hash, &recipe, &spec).expect("a"),
        graph::render_hash(&parsed.image.content_hash, &parsed, &spec).expect("b")
    );
}

#[test]
fn a_recipe_round_trip_produces_identical_pixels() {
    let frame = fixtures::detail_frame(80, 60);
    let engine = engine(frame.clone());
    let recipe = graded();
    let parsed: Recipe =
        serde_json::from_str(&aura_recipe::canonical(&recipe).expect("canonical")).expect("parse");

    let a = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("a");
    let b = engine
        .render_frame(
            &frame,
            &parsed,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("b");
    assert_eq!(a.data, b.data);
}

#[test]
fn the_xmp_round_trip_preserves_the_lightroom_subset() {
    let recipe = recipes::reference();
    let packet = aura_recipe::xmp::write(&recipe);
    let back = aura_recipe::xmp::read(
        &packet,
        &recipes::neutral(&recipe.image.content_hash, "ILCE-7M4"),
    )
    .expect("read");

    for path in aura_recipe::xmp::SUBSET {
        if *path == "bw" {
            continue; // The mix has no crs: equivalent; only the flag round-trips.
        }
        assert!(
            !schema::leaf_differs(&recipe, &back, path),
            "{path} did not survive the XMP round trip"
        );
    }
}

#[test]
fn nothing_outside_the_subset_reaches_the_xmp() {
    // The privacy half of the interchange decision: the file a photographer hands to a
    // Lightroom user must not carry an identity, a decision id or a style profile.
    let packet = aura_recipe::xmp::write(&recipes::reference());
    for secret in ["identity:bride", "d_8f21", "amrit_v3", "skin_smooth"] {
        assert!(!packet.contains(secret), "{secret} leaked into the XMP");
    }
}

// ---------------------------------------------------------------------------
// Row 4. `user_edited_fields` is never overwritten by an AI or QC pass.
// ---------------------------------------------------------------------------

#[test]
fn a_photographers_parameter_survives_every_automated_pass() {
    let mut base = recipes::reference();
    base.global.exposure = 0.75;
    base.global.temperature = 6100;
    base.provenance.source = EditSource::User;
    base.provenance.user_edited_fields = vec![
        "global.exposure".to_string(),
        "global.temperature".to_string(),
    ];

    // Twenty passes in a row, each proposing something different, as phases 15 to 27 will.
    let mut current = base.clone();
    for step in 0u16..20 {
        let mut proposal = recipes::reference();
        proposal.global.exposure = -2.0 + f32::from(step) * 0.1;
        proposal.global.temperature = 3000 + u32::from(step) * 100;
        proposal.global.shadows = step as i16;
        let source = match step % 3 {
            0 => EditSource::Ai,
            1 => EditSource::Qc,
            _ => EditSource::Preset,
        };
        let (merged, report) = schema::merge(&current, &proposal, source).expect("merge");
        assert!(
            report.refused.contains(&"global.exposure".to_string()),
            "step {step} did not refuse the protected exposure"
        );
        current = merged;
    }

    assert!((current.global.exposure - 0.75).abs() < 1e-6);
    assert_eq!(current.global.temperature, 6100);
    assert_eq!(current.global.shadows, 19, "unprotected fields still moved");
    assert_eq!(
        current.provenance.user_edited_fields,
        vec![
            "global.exposure".to_string(),
            "global.temperature".to_string()
        ]
    );
}

#[test]
fn a_protected_parameter_changes_no_pixels_after_an_automated_pass() {
    let frame = fixtures::detail_frame(64, 48);
    let engine = engine(frame.clone());

    let mut base = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    base.global.exposure = 1.0;
    base.provenance.user_edited_fields = vec!["global.exposure".to_string()];

    let before = engine
        .render_frame(
            &frame,
            &base,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("before");

    let mut proposal = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    proposal.global.exposure = -3.0;
    let (merged, _) = schema::merge(&base, &proposal, EditSource::Ai).expect("merge");

    let after = engine
        .render_frame(
            &frame,
            &merged,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("after");

    assert_eq!(
        before.data, after.data,
        "an automated pass moved pixels the photographer had settled"
    );
}

// ---------------------------------------------------------------------------
// Row 5. A full-resolution export completes without exceeding the memory budget.
// ---------------------------------------------------------------------------

#[test]
fn a_hundred_megapixel_export_is_streamed_and_a_streamed_render_is_identical() {
    assert!(
        tiles::working_bytes_for(11_648, 8_736) > aura_render::cpu::DEFAULT_WORKING_BYTES,
        "a 100 MP frame must not fit whole, or the streaming path is never taken"
    );

    let frame = fixtures::detail_frame(640, 480);
    let engine = engine(frame.clone());
    let mut recipe = graded();
    recipe.global.noise.luminance = 20;
    recipe.global.noise.colour = 30;

    let whole = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("whole");
    let streamed = tiles::render_streamed(
        &engine,
        &frame,
        &recipe,
        RenderLevel::Full,
        RenderPurpose::Export,
        &OutputSpec::default(),
        4096,
    )
    .expect("streamed");
    assert_eq!(whole.data, streamed.data);
}

// ---------------------------------------------------------------------------
// Row 6. The interactive budget. Asserted as a *shape* here and as a number in
// `crates/aura-perf/tests/develop_budgets.rs`, which is where budgets live.
// ---------------------------------------------------------------------------

#[test]
fn the_interactive_path_runs_fewer_stages_than_the_export_path() {
    let mut recipe = graded();
    recipe.global.dehaze = 40;
    recipe.restoration.denoise = "auto".to_string();
    recipe.retouch.push(aura_recipe::RetouchOp {
        op: "skin_smooth".to_string(),
        strength: 0.4,
        protect_texture: 0.8,
        mask: Some("skin".to_string()),
        borrowed_from: None,
    });

    let interactive = graph::plan(
        &recipe,
        RenderPurpose::Interactive,
        graph::InputKind::Working,
        graph::Capabilities::default(),
    );
    let export = graph::plan(
        &recipe,
        RenderPurpose::Export,
        graph::InputKind::Working,
        graph::Capabilities::default(),
    );
    assert!(interactive.stages.len() < export.stages.len());
    assert!(!interactive.stages.contains(&graph::Stage::Dehaze));
    assert!(export.stages.contains(&graph::Stage::Dehaze));
}

#[test]
fn a_proxy_render_lands_on_2048_pixels_and_a_slider_invalidates_only_downstream() {
    let frame = fixtures::detail_frame(3000, 2000);
    let engine = engine(frame.clone());
    let out = engine
        .render_frame(
            &frame,
            &graded(),
            RenderLevel::Proxy2048,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .expect("render");
    assert_eq!(out.width.max(out.height), 2048);

    // Section 6.3's invalidation rule: moving saturation must not re-run exposure.
    let late = graph::earliest_affected(&["global.saturation".to_string()]).expect("stage");
    assert!(late.index() > graph::Stage::Exposure.index());
    assert!(late.index() > graph::Stage::WhiteBalance.index());
}

// ---------------------------------------------------------------------------
// Row 7. v1 recipes render correctly after a simulated schema v2 migration.
// ---------------------------------------------------------------------------

#[test]
fn a_v1_recipe_renders_identically_under_a_simulated_v2_engine() {
    let frame = fixtures::detail_frame(64, 48);
    let engine = engine(frame.clone());
    let recipe = recipes::reference();

    let before = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("v1");

    let mut from_v2 = recipe.clone();
    from_v2.schema = aura_recipe::SCHEMA_VERSION + 1;
    from_v2.extra.insert(
        "film_emulation".to_string(),
        serde_json::json!({"stock": "portra_400"}),
    );
    let (migrated, warning) = aura_recipe::migrate::to_current(&from_v2).expect("migrate");
    assert_eq!(
        warning.map(|e| e.code.to_string()),
        Some("AURA-RENDER-8003".to_string())
    );

    let mut as_v1 = migrated;
    as_v1.schema = 1;
    as_v1.extra.clear();
    let after = engine
        .render_frame(
            &frame,
            &as_v1,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("v2");

    assert_eq!(before.data, after.data);
}

// ---------------------------------------------------------------------------
// The invariants this phase inherits.
// ---------------------------------------------------------------------------

#[test]
fn every_render_says_what_it_skipped() {
    // Invariant 9. A stage that could not run is reported rather than silently omitted.
    let frame = fixtures::detail_frame(32, 24);
    let engine = engine(frame.clone());
    let out = engine
        .render_frame(
            &frame,
            &recipes::reference(),
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .expect("render");

    assert!(!out.notes.is_empty());
    let caveats: Vec<&str> = out
        .notes
        .iter()
        .filter(|n| n.reason.is_a_caveat())
        .map(|n| n.stage.as_str())
        .collect();
    assert!(
        caveats.contains(&"masks"),
        "the reference recipe's face mask needs phase 18: {caveats:?}"
    );
}

#[test]
fn a_render_is_reproducible_from_four_stored_values() {
    // ADR-0029 section 5. The content hash, the canonical recipe, the engine and the output
    // spec are all a delivered file needs to be re-created.
    let recipe = recipes::reference();
    let spec = OutputSpec {
        colour_space: OutputColour::DisplayP3,
        bit_depth: 16,
        icc: Some("Display P3".to_string()),
    };
    let hash = graph::render_hash(&recipe.image.content_hash, &recipe, &spec).expect("hash");
    assert_eq!(hash.len(), 64);
    assert_eq!(
        hash,
        graph::render_hash(&recipe.image.content_hash, &recipe, &spec).expect("again")
    );
    assert!(graph::engine_string().starts_with("aura-render/1.0.0"));
}

#[test]
fn no_render_ever_writes_to_a_raw() {
    // Invariant 1, as the property of the type that makes it structural: a request names an
    // image and an output *spec*, and there is nowhere in the shape to put a destination.
    let request_fields = std::mem::size_of::<aura_render::contract::render::RenderRequest>();
    assert!(request_fields > 0);
    // The real assertion is `crates/aura-render/tests/colour_discipline.rs`, which greps the
    // crate for a filesystem call. This one is here so that section 10.1's list has a row
    // pointing at it.
    let source = include_str!("../../crates/aura-render/src/contract/render.rs");
    for opener in ["PathBuf", "std::fs", "File"] {
        assert!(
            !source.contains(opener),
            "the frozen render contract must not be able to name a destination"
        );
    }
}
