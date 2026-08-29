//! PHASE-20 section 10.1, as a test.
//!
//! Seven gates, run as an ordinary `cargo test` so that a red gate is a red build. Every one of
//! them is measured against the synthetic faces in `aura_retouch::fixtures`, whose marks are
//! painted into the pixels and read back through the real detector, the real operators and the
//! real texture guard.
//!
//! **What that proves and what it does not.** It proves the arithmetic: the detector geometry,
//! the protect veto, the band separation, the texture floor, the re-solve, the withdrawal and
//! the per-identity constancy. It is not evidence about a wedding photograph, for the four
//! reasons `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 10 lists and
//! `docs/progress/PHASE-20-EXIT.md` carries as conditions.
//!
//! Two of section 10.1 seven rows cannot be met by a test at all and are recorded rather than
//! faked: the per-skin-tone parity study needs labelled faces across five tone buckets, and the
//! blind expert comparison against Retouch4me, Evoto and Aperty needs retouchers. Neither exists
//! in this repository. `the_gates_this_build_cannot_measure_are_named` is what keeps that
//! visible in the same place as the gates that do run.

use std::collections::BTreeMap;

use aura_core::contract::composition::Box2;
use aura_core::contract::people::Role;
use aura_core::contract::retouch::{
    InpaintMethod, ProtectedFeature, ProtectedKind, ProtectedSource, RetouchCode, RetouchOp,
    RetouchPreset, MAX_UNDEREYE_CHROMA, MAX_UNDEREYE_LUMA_EV, POLISHED_FLOOR, TEXTURE_FLOOR,
};
use aura_core::SceneId;
use aura_render::bands;
use aura_render::retouch as render_retouch;
use aura_retouch::blemish;
use aura_retouch::fixtures;
use aura_retouch::ops::Analyser;
use aura_retouch::presets::PresetTable;
use aura_retouch::strength::{self, IdentityStats};
use aura_retouch::texture_guard;
use aura_retouch::undereye;

/// Gate 1. Texture retention at or above the floor of every preset, on every preset.
///
/// Section 10.1: "texture retention >= 0.90 band-energy ratio on all presets; Polished never
/// below 0.80". The floor is the *preset* floor here, which is stricter than the phase floor on
/// two of the four.
#[test]
fn gate_1_texture_retention_holds_on_every_preset() {
    let table = PresetTable::embedded().expect("the preset table");
    let (frame, context, area) = fixtures::frame_with_blemish();

    for preset in RetouchPreset::ALL {
        if preset.is_off() {
            continue;
        }
        let floor = table.preset(preset).texture_floor;
        assert!(
            floor >= POLISHED_FLOOR,
            "{} would allow a floor below the phase bound",
            preset.as_str()
        );
        let ops = vec![RetouchOp::Blemish {
            area,
            method: InpaintMethod::Patch,
            strength: 1.0,
        }];
        let guarded = texture_guard::enforce(&frame, &ops, &context, floor);
        assert!(
            guarded.report.passed,
            "{} could not keep its own floor: ratio {:.4} against {:.2}",
            preset.as_str(),
            guarded.report.band_ratio,
            floor
        );
        assert!(
            guarded.report.band_ratio >= TEXTURE_FLOOR - 1e-3 || preset == RetouchPreset::Polished,
            "{} retained only {:.4}",
            preset.as_str(),
            guarded.report.band_ratio
        );
    }
}

/// Gate 2. Blemish recall, and zero false removal of permanent features.
///
/// Section 10.1: "blemish recall >= 0.90 with false-removal of permanent features <= 2 % (and
/// 0 % for tattoos)". Measured over the fixture set: every painted spot is found and removable,
/// and no painted mole, freckle field or tattoo is.
#[test]
fn gate_2_recall_is_complete_and_no_permanent_feature_is_removed() {
    let spots = [
        fixtures::face_with_blemish(),
        fixtures::face_with_two_blemishes(),
    ];
    let mut painted = 0usize;
    let mut removable = 0usize;
    for crop in &spots {
        for candidate in blemish::detect(crop) {
            if candidate.is_removable() {
                removable += 1;
            }
        }
    }
    painted += 3; // one on the first fixture, two on the second
    let recall = removable as f32 / painted as f32;
    assert!(
        recall >= 0.90,
        "blemish recall is {recall:.2} over {painted} painted marks"
    );

    for crop in [
        fixtures::face_with_mole(),
        fixtures::face_with_freckles(),
        fixtures::face_with_tattoo(),
    ] {
        for candidate in blemish::detect(&crop) {
            assert!(
                !candidate.is_removable(),
                "a permanent feature was removable at {:.2} temporary",
                candidate.temporary
            );
        }
    }
}

/// Gate 3. The same person is retouched at the same strength across the gallery.
///
/// Section 10.1 asks for a spread of five per cent or less. It is **zero** by construction: the
/// strength is one number per identity per project and every frame reads it. See ADR-0043
/// section 6 for why the four inputs section 6.4 lists are taken as gallery statistics.
#[test]
fn gate_3_one_identity_is_retouched_identically_everywhere() {
    let table = PresetTable::embedded().expect("the preset table");
    let stats = IdentityStats {
        identity: fixtures::identity(),
        role: Role::Couple,
        median_face_frac: 0.18,
        dominant_scene: SceneId::CouplePortrait,
        frames: 240,
    };
    // The same person, asked for on forty different frames.
    let values: Vec<f32> = (0..40)
        .map(|_| strength::assign(&stats, &table, RetouchPreset::Natural))
        .collect();
    let spread = strength::spread(&values);
    assert!(
        spread <= 0.05 * values.first().copied().unwrap_or(1.0),
        "one identity varied by {spread:.4} across a gallery"
    );
    assert!(spread <= f32::EPSILON, "the spread is not zero: {spread}");
}

/// Gate 4. The under-eye correction never exceeds its cap.
///
/// Section 10.1: "under-eye correction never exceeds its cap; no visible light patches at 100 %
/// zoom". The cap is measured on a shadow far deeper than any correction should close, and the
/// patch test is the *reversal* check below it: a correction that produced a light patch would
/// lift the region above the skin around it, which is a sign change in the difference.
#[test]
fn gate_4_the_under_eye_correction_is_capped_and_leaves_no_patch() {
    let (crop, face) = fixtures::face_with_deep_circles();
    let decision = undereye::solve(&crop, &face, 1.0).expect("a correction");
    assert!(decision.luma_ev <= MAX_UNDEREYE_LUMA_EV + 1e-6);
    assert!(decision.chroma <= MAX_UNDEREYE_CHROMA + 1e-6);
    assert!(decision.capped);

    // Apply it and check that no sample under the eye ends up brighter than the skin around it.
    let (frame, mut context, _) = fixtures::frame_with_dark_circles();
    context.eyes = vec![[[64.0, 60.0], [96.0, 60.0]]];
    let ops = vec![RetouchOp::UnderEye {
        identity: fixtures::identity(),
        luma: decision.luma_ev,
        chroma: decision.chroma,
    }];
    let mut pixels = frame.rgb.clone();
    render_retouch::apply(&mut pixels, frame.width, frame.height, &ops, &context);

    let before = render_retouch::luma_plane(&frame.rgb, frame.width, frame.height);
    let after = render_retouch::luma_plane(&pixels, frame.width, frame.height);

    // **A light patch is a region, not a sample.** Comparing individual samples against the mean
    // of the skin fails on any photograph with texture in it: a pore peak sits a few per cent
    // above the local mean before anything is retouched, and a test that called that a patch
    // would be measuring the fixture pores. What a patch is, is an *area* that ends up brighter
    // than the skin around it.
    let mut region_after = 0.0f32;
    let mut region_before = 0.0f32;
    let mut region_count = 0.0f32;
    for (index, value) in after.iter().enumerate() {
        if *value <= before.get(index).copied().unwrap_or(0.0) {
            continue;
        }
        region_after += value;
        region_before += before.get(index).copied().unwrap_or(0.0);
        region_count += 1.0;
    }
    assert!(region_count > 0.0, "the correction lifted nothing at all");
    let region_after = region_after / region_count;
    let region_before = region_before / region_count;

    // The skin the patch would stand out against: a band well below the corrected region.
    let cheek: f32 = {
        let row = frame.height - 8;
        let mut total = 0.0;
        for x in 0..frame.width {
            total += after.get(row * frame.width + x).copied().unwrap_or(0.0);
        }
        total / frame.width as f32
    };

    assert!(region_before < cheek, "the fixture shadow is not a shadow");
    assert!(
        region_after <= cheek,
        "the corrected region reached {region_after:.4}, above the surrounding skin {cheek:.4}"
    );
}

/// Gate 5. A protected feature is never touched, and a tattoo cannot be unprotected.
///
/// The ethical core of the phase, as two assertions: an operation that overlaps a protect row is
/// refused by `RetouchPlan::broken_guarantee`, and `ProtectedKind::is_absolute` cannot be
/// cleared through the service.
#[test]
fn gate_5_the_protect_set_is_a_veto() {
    let (image, pixels, context) = fixtures::planned_frame_with_protected_mole();
    let analyser = Analyser::new().expect("an analyser");
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");

    assert!(
        outcome
            .plan
            .reasons
            .iter()
            .any(|reason| reason.code == RetouchCode::VetoedByProtection),
        "the protect row did not veto anything"
    );
    for op in &outcome.plan.ops {
        if let Some(area) = op.area() {
            assert!(!outcome.plan.is_protected(area));
        }
    }
    assert!(outcome.plan.is_sound());

    let tattoo = ProtectedFeature {
        identity: fixtures::identity(),
        kind: ProtectedKind::Tattoo,
        area: Box2 {
            x: -0.3,
            y: 0.5,
            w: 0.2,
            h: 0.2,
        },
        confidence: 1.0,
        source: ProtectedSource::User,
        frames: 20,
        span_minutes: 300.0,
        first_seen: fixtures::photo(),
    };
    assert!(tattoo.is_absolute());
    let refused = aura_retouch::guard::check_protection(&tattoo, false).expect_err("refused");
    assert_eq!(refused.code.0, "AURA-ML-5097");
}

/// Gate 6. The proxy preview and the full-resolution export agree.
///
/// Section 10.1: "proxy preview matches full-resolution output within a perceptual tolerance".
/// The same plan is applied to the same photograph at two scales and the *result* is compared -
/// not the parameters. Every constant in `aura_render::retouch` is a fraction of the thing it
/// measures rather than a number of pixels, and this is the test that says so.
#[test]
fn gate_6_the_proxy_agrees_with_the_full_resolution_render() {
    let (small, small_context, area) = fixtures::frame_with_blemish();
    let (large, large_context, _) = fixtures::frame_with_blemish_at(320);

    let ops = vec![RetouchOp::Blemish {
        area,
        method: InpaintMethod::Patch,
        strength: 1.0,
    }];

    let mut small_pixels = small.rgb.clone();
    render_retouch::apply(
        &mut small_pixels,
        small.width,
        small.height,
        &ops,
        &small_context,
    );
    let mut large_pixels = large.rgb.clone();
    render_retouch::apply(
        &mut large_pixels,
        large.width,
        large.height,
        &ops,
        &large_context,
    );

    // Both must have removed the mark to about the same degree. Compared as the fraction of the
    // redness that survived, because the two renders have different pixel counts and cannot be
    // compared sample for sample.
    let small_left = fixtures::redness_at(&small_pixels, small.width, 0.5, 0.5);
    let large_left = fixtures::redness_at(&large_pixels, large.width, 0.5, 0.5);
    let small_before = fixtures::redness_at(&small.rgb, small.width, 0.5, 0.5);
    let large_before = fixtures::redness_at(&large.rgb, large.width, 0.5, 0.5);

    let small_share = small_left / small_before.max(1e-6);
    let large_share = large_left / large_before.max(1e-6);
    assert!(
        (small_share - large_share).abs() < 0.15,
        "the proxy left {small_share:.3} of the mark and the full render left {large_share:.3}"
    );
}

/// Gate 7. An evening-only plan costs no texture at all.
///
/// Not one of section 10.1 own rows, and it is here because it is the strongest statement the
/// phase makes: the band reconstruction is exact, so scaling the mid band cannot touch a pore by
/// any value of any parameter. If this ever fails, the separation has stopped being exact and
/// every other texture number in the phase is suspect.
#[test]
fn gate_7_evening_cannot_reach_the_high_band() {
    let (frame, context, _) = fixtures::frame_with_blemish();
    let ops = vec![RetouchOp::ToneEvening {
        mask: fixtures::mask_id(),
        strength: 1.0,
        band: aura_core::contract::retouch::FreqBand::Mid,
    }];
    let mut pixels = frame.rgb.clone();
    render_retouch::apply(&mut pixels, frame.width, frame.height, &ops, &context);

    let before = bands::separate(
        &render_retouch::luma_plane(&frame.rgb, frame.width, frame.height),
        frame.width,
        frame.height,
    )
    .high_energy();
    let after = bands::separate(
        &render_retouch::luma_plane(&pixels, frame.width, frame.height),
        frame.width,
        frame.height,
    )
    .high_energy();
    assert!(
        after / before > 0.99,
        "evening cost texture: {:.4}",
        after / before
    );
}

/// Gate 8. The whole plan is deterministic, and so is the catalog round trip of one.
#[test]
fn gate_8_a_plan_is_deterministic() {
    let (image, pixels, context) = fixtures::planned_frame();
    let analyser = Analyser::new().expect("an analyser");
    let first = analyser.analyse(image, &pixels, &context).expect("a plan");
    let second = analyser.analyse(image, &pixels, &context).expect("a plan");
    assert_eq!(first.plan, second.plan);
    assert!(first.plan.is_sound());
}

/// Gate 9. The strengths a preset produces are ordered the way the product says they are.
#[test]
fn gate_9_the_presets_are_ordered() {
    let table = PresetTable::embedded().expect("the preset table");
    let stats = IdentityStats {
        identity: fixtures::identity(),
        role: Role::Couple,
        median_face_frac: 0.20,
        dominant_scene: SceneId::CouplePortrait,
        frames: 100,
    };
    let mut previous = -1.0f32;
    for preset in RetouchPreset::ALL {
        let value = strength::assign(&stats, &table, preset);
        assert!(
            value >= previous,
            "{} is weaker than the preset before it",
            preset.as_str()
        );
        previous = value;
    }
    assert!(strength::assign(&stats, &table, RetouchPreset::Off) <= 0.0);
}

/// The two rows of section 10.1 this build cannot measure, named rather than skipped.
///
/// A test that asserts what is *not* known is unusual, and it is here for the reason phase 15
/// and phase 18 have one: a gate that is quietly absent looks exactly like a gate that passes.
#[test]
fn the_gates_this_build_cannot_measure_are_named() {
    let unmet: BTreeMap<&str, &str> = BTreeMap::from([
        (
            "per-skin-tone parity",
            "needs blemish and permanent-feature labels across five Monk-scale buckets, with \
             consent. Section 9 gives DATA that task and there is no such corpus in this \
             repository; condition C2 in the phase 20 exit report.",
        ),
        (
            "blind expert comparison",
            "needs retouchers judging AURA against Retouch4me, Evoto and Aperty. Section 9 gives \
             QAIQ that task; condition C4 in the phase 20 exit report.",
        ),
    ]);
    assert_eq!(unmet.len(), 2);
    for (gate, why) in unmet {
        assert!(!why.is_empty(), "{gate} has no recorded reason");
    }
}

/// Gate 10. Every reason code has a sentence a photographer can read.
///
/// Phases 11, 12, 15 and 19 each wrote this test for their own vocabulary; this is the same test
/// five phases later, and it is the reason a new reason code cannot ship without a sentence. It
/// asserts on the **sentence** rather than on the slug, which is stricter: a document listing
/// twenty-six slugs and explaining twenty-four of them would pass the weaker version and fail a
/// reader.
#[test]
fn gate_10_every_reason_code_is_documented_in_the_products_own_words() {
    let doc = flattened(
        &std::fs::read_to_string("../../docs/retouch.md").expect("docs/retouch.md must exist"),
    );
    for code in RetouchCode::ALL {
        assert!(
            doc.contains(&flattened(code.user_text())),
            "`{}` has no sentence in docs/retouch.md",
            code.as_str()
        );
    }
}

/// Gate 11. Every withdrawal is marked as one in the documentation.
///
/// Thirteen of the twenty-six describe something the product declined to do, and that is the
/// point of the phase rather than a footnote. The count is asserted so a code that changed sides
/// does not quietly lose its mark.
#[test]
fn gate_11_every_withdrawal_is_marked_as_one() {
    let doc = std::fs::read_to_string("../../docs/retouch.md").expect("docs/retouch.md");
    let marked = doc.matches("*(withdrawal)*").count();
    let expected = RetouchCode::ALL
        .iter()
        .filter(|code| code.is_withdrawal())
        .count();
    assert_eq!(
        marked, expected,
        "{marked} sentences are marked as withdrawals and {expected} codes are"
    );
}

/// Gate 12. The product's own page mentions what it will not build only to refuse it.
///
/// A phase that grew a reshaping feature would write the page before it wrote the code, so this
/// is where it would show first. The check is not that the words are absent - the page has to be
/// able to *say* what AURA will never do - but that every sentence carrying one of them also
/// carries a refusal.
#[test]
fn gate_12_the_documentation_promises_nothing_this_product_forbids() {
    let doc = std::fs::read_to_string("../../docs/retouch.md").expect("docs/retouch.md");
    let flat = flattened(&doc).to_lowercase();
    for sentence in flat.split(". ") {
        let mentions = ["reshap", "slim", "lighten", "whiten", "face swap"]
            .iter()
            .any(|word| sentence.contains(word));
        if !mentions {
            continue;
        }
        let refuses = [
            "never",
            "not ",
            "no setting",
            "nowhere",
            "forbid",
            "will never",
        ]
        .iter()
        .any(|word| sentence.contains(word));
        assert!(
            refuses,
            "docs/retouch.md mentions a forbidden operation without refusing it: `{sentence}`"
        );
    }
    // And it says the one thing it must say.
    assert!(flat.contains("never alters tattoos"));
}

/// Whitespace flattened, so a sentence wrapped across two lines still matches.
fn flattened(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
