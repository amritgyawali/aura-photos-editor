//! PHASE-21 section 10.1, as a test.
//!
//! Eight gates, run as an ordinary `cargo test` so that a red gate is a red build. Every one of
//! them is measured against the synthetic frames in `aura_retouch::micro::fixtures`, whose
//! flyaways, glare sheets, lint and teeth are painted into the pixels and read back through the
//! real detectors, the real operators, the real naturalness guard and the real renderer.
//!
//! **What that proves and what it does not.** It proves the arithmetic: the background gate, the
//! area caps, the locus arithmetic, the catchlight protection, the information rule that
//! separates a repairable sheet from a closed eye, the alignment floor, the disclosure and every
//! contract ceiling. It is not evidence about a wedding photograph, for the reasons
//! `docs/adr/ADR-0043-micro-retouch-and-cross-frame-borrowing.md` section 6 lists and
//! `docs/progress/PHASE-21-EXIT.md` carries as conditions.
//!
//! Three of section 10.1's rows cannot be met by a test at all and are recorded rather than
//! faked: the naturalness audit needs four hundred frames judged by retouchers, the per-hair-type
//! coverage report needs a corpus with hair-type labels, and the 100 % zoom artefact audit needs
//! a person at a monitor. `the_gates_this_build_cannot_measure_are_named` is what keeps that
//! visible in the same place as the gates that do run.

use std::collections::BTreeMap;

use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{
    ClothingIssue, GlareMethod, MicroCode, MicroOp, MicroOverride, MicroRegion, NaturalnessGuard,
    CATCHLIGHT_FLOOR, HAIR_ENERGY_FLOOR, MAX_BORROW_AREA, MAX_CLOTHING_AREA, MAX_CLOTHING_STRENGTH,
    MAX_FLYAWAY_AREA, MAX_FLYAWAY_STRENGTH, MAX_IRIS_CLARITY, MAX_SCLERA, MAX_TEETH_LUMA_EV,
    MAX_TEETH_YELLOW, MIN_ALIGNMENT, MIN_SPECULAR_FRACTION, TEETH_EXCURSION_CEILING,
};
use aura_render::micro::{edge_energy, MicroContext};
use aura_retouch::micro::ops::{to_linear, upsample, Analyser};
use aura_retouch::micro::{borrow, clothing, fixtures, glare, guard, hair};
use aura_retouch::texture_guard::Frame;

/// The luminance of one pixel, the same weighting every module in this crate uses.
fn luma(rgb: &[f32], index: usize) -> f32 {
    let r = rgb.get(index * 3).copied().unwrap_or(0.0);
    let g = rgb.get(index * 3 + 1).copied().unwrap_or(0.0);
    let b = rgb.get(index * 3 + 2).copied().unwrap_or(0.0);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Gate 1. A stray strand over a quiet background is calmed, and it is calmed rather than erased.
///
/// Section 10.1's first row is "no bald patches or hairline damage on any fixture", and section
/// 6.1 is where the shape of the promise is: "reduce rather than remove". Both halves are
/// measured on the rendered pixels rather than on the parameter that was solved.
#[test]
fn gate_1_a_flyaway_is_calmed_and_never_erased() {
    let (image, pixels, context) = fixtures::flyaway_frame(false);
    let analyser = Analyser::new().expect("an analyser");
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");

    let flyaways: Vec<&MicroOp> = outcome
        .plan
        .ops
        .iter()
        .filter(|op| matches!(op, MicroOp::Flyaway { .. }))
        .collect();
    assert!(
        !flyaways.is_empty(),
        "the quiet-background fixture produced no flyaway operation: {:?}",
        outcome
            .plan
            .reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect::<Vec<_>>()
    );

    for op in &flyaways {
        assert!(
            op.strength() <= MAX_FLYAWAY_STRENGTH + 1e-4,
            "a flyaway ran at {:.3}, above the ceiling of {MAX_FLYAWAY_STRENGTH:.3}",
            op.strength()
        );
        assert!(
            op.strength() > 0.0,
            "a flyaway operation was stored at zero strength"
        );
    }

    // Reduce rather than remove: the strand is still there afterwards. The ceiling is what
    // guarantees it - at 0.60 the strongest permitted edit keeps two fifths of the contrast - and
    // this reads it off the rendered pixels rather than trusting the parameter.
    let before = to_linear(&pixels).expect("linear pixels");
    let after = outcome.rendered.as_ref().expect("rendered pixels");
    let (strand_before, strand_after) = strand_contrast(&before, after);
    println!(
        "gate 1: mean strand contrast {strand_before:.4} -> {strand_after:.4} ({:.0} % of it kept)",
        100.0 * strand_after / strand_before
    );
    assert!(
        strand_after < strand_before,
        "the strand was not calmed at all: {strand_before:.4} -> {strand_after:.4}"
    );
    assert!(
        strand_after > 0.25 * strand_before,
        "the strand was erased rather than calmed: {strand_before:.4} -> {strand_after:.4}"
    );
    assert!(outcome.plan.is_sound());
}

/// The mean luminance departure of the painted strand from the background beside it.
///
/// A **mean over the strand's own pixels** rather than a peak, and that is not a detail. The
/// operator feathers toward the edge of its region - which is what stops a reduction from leaving
/// a visible rectangle - so the first and last row of a strand are attenuated less than its
/// middle. A peak measures whichever row the feather touched least and would read a working
/// operator as a broken one.
fn strand_contrast(before: &Frame, after: &[f32]) -> (f32, f32) {
    let background = 0.4071;
    let mut sum_before = 0.0f32;
    let mut sum_after = 0.0f32;
    let mut samples = 0u32;
    for y in 0..before.height {
        for x in 80..96.min(before.width) {
            let index = y * before.width + x;
            let departure = luma(&before.rgb, index) - background;
            // The strand's own pixels: brighter than the background it lies on. The hair mass is
            // darker, so it cannot enter this average.
            if departure <= 0.10 {
                continue;
            }
            sum_before += departure;
            sum_after += (luma(after, index) - background).max(0.0);
            samples += 1;
        }
    }
    assert!(samples > 0, "the fixture carries no strand at all");
    (sum_before / samples as f32, sum_after / samples as f32)
}

/// Gate 2. The same strand over a busy background is refused, and the area cap always binds.
///
/// The background gate is the whole of what makes an untrained flyaway detector safe: a
/// measurement cannot tell a strand from a twig, so where the background carries its own detail
/// the operation is skipped rather than guessed.
#[test]
fn gate_2_a_busy_background_refuses_the_edit_and_the_area_cap_binds() {
    let (image, pixels, context) = fixtures::flyaway_frame(true);
    let analyser = Analyser::new().expect("an analyser");
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");

    assert!(
        !outcome
            .plan
            .ops
            .iter()
            .any(|op| matches!(op, MicroOp::Flyaway { .. })),
        "a flyaway was attenuated over a busy background"
    );
    assert!(
        outcome
            .plan
            .reasons
            .iter()
            .any(|reason| reason.code == MicroCode::BackgroundBusy
                || reason.code == MicroCode::NoFlyawayFound),
        "the refusal was silent: {:?}",
        outcome
            .plan
            .reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect::<Vec<_>>()
    );

    // The cap, on the fixture that does produce operations. A sum over the plan rather than a
    // per-operation bound, because ten separately-legal strands are a bald patch.
    let (image, pixels, context) = fixtures::planned_frame();
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
    let area: f32 = outcome
        .plan
        .ops
        .iter()
        .filter_map(|op| match op {
            MicroOp::Flyaway { region, .. } => Some(region.w * region.h),
            _ => None,
        })
        .sum();
    assert!(
        area <= MAX_FLYAWAY_AREA + 1e-6,
        "the flyaway operations cover {area:.5} of the frame, above {MAX_FLYAWAY_AREA:.5}"
    );

    // And the detector's own cap, one level down, on a fixture with a strand in it.
    let (_, pixels, _) = fixtures::flyaway_frame(false);
    let frame = to_linear(&pixels).expect("linear pixels");
    let mut alpha = vec![0.0f32; frame.width * frame.height];
    for y in 0..frame.height {
        for x in 0..frame.width {
            if let Some(slot) = alpha.get_mut(y * frame.width + x) {
                *slot = if x < 80 {
                    1.0
                } else if x < 88 {
                    0.30
                } else {
                    0.0
                };
            }
        }
    }
    let found = hair::detect(&frame, &alpha);
    assert!(
        hair::total_area(&found) <= MAX_FLYAWAY_AREA + 1e-6,
        "the detector offered {:.5} of the frame",
        hair::total_area(&found)
    );
}

/// Gate 3. Nothing this phase does moves the hair mass's own energy below the floor.
///
/// A bald patch is a large local loss of edge energy in the hair region and nothing else, which
/// is why the guarantee is measured there rather than inferred from the strength that was
/// applied. The measurement runs through the real renderer.
#[test]
fn gate_3_the_hair_keeps_its_energy() {
    let (image, pixels, context) = fixtures::planned_frame();
    let analyser = Analyser::new().expect("an analyser");
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");

    let report = outcome.plan.naturalness;
    assert!(
        report.hair_energy_ratio >= HAIR_ENERGY_FLOOR - 1e-4,
        "the hair lost energy: {:.4} against a floor of {HAIR_ENERGY_FLOOR:.4}",
        report.hair_energy_ratio
    );

    // The same measurement taken here rather than read off the report, so a guard that stopped
    // measuring would fail this gate rather than pass it with a stored 1.0.
    let before = to_linear(&pixels).expect("linear pixels");
    let after = outcome.rendered.as_ref().expect("rendered pixels");
    let hair_field = context
        .regions
        .get(&MicroRegion::Hair)
        .expect("a hair region");
    let plane = upsample(hair_field, before.width, before.height);
    let (energy_before, samples) = edge_energy(&before.rgb, before.width, before.height, &plane);
    let (energy_after, _) = edge_energy(after, before.width, before.height, &plane);
    assert!(samples > 0, "the hair region measured nothing");
    assert!(
        energy_after >= HAIR_ENERGY_FLOOR * energy_before,
        "measured directly, the hair lost energy: {energy_before:.5} -> {energy_after:.5}"
    );
}

/// Gate 4. Teeth stay inside the locus, and every ceiling binds before the operator does.
///
/// Section 10.1: "luminance and chroma stay inside the natural locus; ceiling-exceed attempts are
/// refused". The excursion is the *increase* in distance from the locus, so a plan that only
/// removed part of the excess measures zero - which is every plan the solver intends to produce.
#[test]
fn gate_4_teeth_stay_inside_the_locus() {
    let (image, pixels, context) = fixtures::planned_frame();
    let analyser = Analyser::new().expect("an analyser");
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");

    let mut saw_teeth = false;
    for op in &outcome.plan.ops {
        if let MicroOp::Teeth {
            luma,
            yellow_reduce,
            ..
        } = op
        {
            saw_teeth = true;
            assert!(
                *luma <= MAX_TEETH_LUMA_EV + 1e-4,
                "a teeth lift of {luma:.3} EV, above {MAX_TEETH_LUMA_EV:.3}"
            );
            assert!(
                *yellow_reduce <= MAX_TEETH_YELLOW + 1e-4,
                "a yellow reduction of {yellow_reduce:.3}, above {MAX_TEETH_YELLOW:.3}"
            );
            assert!(*luma >= 0.0 && *yellow_reduce >= 0.0);
        }
    }
    assert!(
        saw_teeth,
        "the end-to-end fixture produced no teeth operation, so this gate measured nothing"
    );

    println!(
        "gate 4: teeth excursion {:.5} against a ceiling of {TEETH_EXCURSION_CEILING:.5}",
        outcome.plan.naturalness.teeth_excursion
    );
    assert!(
        outcome.plan.naturalness.teeth_excursion < TEETH_EXCURSION_CEILING,
        "the correction pushed the teeth further from natural by {:.5}",
        outcome.plan.naturalness.teeth_excursion
    );
    assert!(outcome.plan.is_sound());
}

/// Gate 5. Catchlights survive, and no operation moves a pixel.
///
/// Section 10.1: "catchlights preserved (specular pixel test); no geometry change measurable".
/// The second half is measured as a luminance-weighted centroid of the iris region before and
/// after: an operator that scaled, warped or shifted an eye would move it.
#[test]
fn gate_5_catchlights_survive_and_nothing_moves() {
    let (image, pixels, context) = fixtures::catchlight_frame();
    let analyser = Analyser::new().expect("an analyser");
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");

    assert!(
        outcome.plan.naturalness.catchlight_ratio >= CATCHLIGHT_FLOOR - 1e-4,
        "the catchlights dimmed to {:.4}, below {CATCHLIGHT_FLOOR:.4}",
        outcome.plan.naturalness.catchlight_ratio
    );

    let before = to_linear(&pixels).expect("linear pixels");
    let after = outcome.rendered.as_ref().expect("rendered pixels");
    let iris = upsample(
        context
            .regions
            .get(&MicroRegion::Iris)
            .expect("an iris region"),
        before.width,
        before.height,
    );

    let centroid_before = centroid(&before.rgb, before.width, before.height, &iris);
    let centroid_after = centroid(after, before.width, before.height, &iris);
    let moved = (centroid_before.0 - centroid_after.0).hypot(centroid_before.1 - centroid_after.1);
    assert!(
        moved < 0.25,
        "the iris centroid moved {moved:.4} px: {centroid_before:?} -> {centroid_after:?}"
    );

    // The specular pixels themselves, rather than the ratio the guard stored.
    let peak_before = region_peak(&before.rgb, before.width, before.height, &iris);
    let peak_after = region_peak(after, before.width, before.height, &iris);
    println!(
        "gate 5: catchlight peak {peak_before:.4} -> {peak_after:.4}, iris centroid moved \
         {moved:.4} px"
    );
    assert!(
        peak_after >= CATCHLIGHT_FLOOR * peak_before,
        "measured directly, the catchlight dimmed: {peak_before:.4} -> {peak_after:.4}"
    );
}

/// The luminance-weighted centroid of a region, in pixels.
fn centroid(rgb: &[f32], width: usize, height: usize, region: &[f32]) -> (f32, f32) {
    let mut weight = 0.0f32;
    let mut sum_x = 0.0f32;
    let mut sum_y = 0.0f32;
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let alpha = region.get(index).copied().unwrap_or(0.0);
            if alpha <= 0.0 {
                continue;
            }
            let value = luma(rgb, index) * alpha;
            weight += value;
            sum_x += value * x as f32;
            sum_y += value * y as f32;
        }
    }
    if weight <= f32::EPSILON {
        return (0.0, 0.0);
    }
    (sum_x / weight, sum_y / weight)
}

/// The peak luminance inside a region.
fn region_peak(rgb: &[f32], width: usize, height: usize, region: &[f32]) -> f32 {
    let mut peak = 0.0f32;
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if region.get(index).copied().unwrap_or(0.0) <= 0.0 {
                continue;
            }
            peak = peak.max(luma(rgb, index));
        }
    }
    peak
}

/// A lapel with `marks` pieces of lint painted on its left half, and none on its right.
///
/// The right half is the control. Section 10.1 asks for lint removal "with no fabric-texture
/// damage at 100 % zoom", and the sharpest form of that question is whether the operator touched
/// fabric it was never aimed at.
fn lapel(marks: usize) -> (Frame, Vec<f32>, Vec<(usize, usize)>) {
    let (width, height) = (256usize, 256usize);
    let mut rgb = vec![0.0f32; width * height * 3];
    let mut garment = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            // A woven texture, deterministic rather than random - invariant 4 - smooth enough
            // that the weave itself never departs from its own neighbourhood by `MIN_DEPARTURE`,
            // and well below `MAX_FABRIC_TEXTURE` so the fabric is not refused as patterned. A
            // blockier weave makes the fabric its own worst false positive, which is a fixture
            // measuring itself rather than the detector.
            let weave = 0.006 * (x as f32 * 0.5).sin() + 0.005 * (y as f32 * 0.37).sin();
            let value = 0.10 + weave;
            for channel in 0..3 {
                if let Some(slot) = rgb.get_mut(index * 3 + channel) {
                    *slot = value;
                }
            }
            if y >= 64 {
                if let Some(slot) = garment.get_mut(index) {
                    *slot = 1.0;
                }
            }
        }
    }

    let mut painted = Vec::new();
    for mark in 0..marks {
        // Spread across the left half only, at a spacing wider than the detector's own
        // neighbourhood so two marks are never one component.
        let cx = 24 + (mark % 5) * 20;
        let cy = 88 + (mark / 5) * 24;
        for y in cy..cy + 3 {
            for x in cx..cx + 3 {
                let index = y * width + x;
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut(index * 3 + channel) {
                        *slot = 0.62;
                    }
                }
            }
        }
        painted.push((cx + 1, cy + 1));
    }

    (Frame { rgb, width, height }, garment, painted)
}

/// Gate 6. Lint recall is at or above the floor, and untouched fabric keeps its weave.
#[test]
fn gate_6_lint_is_found_and_the_fabric_survives() {
    const MARKS: usize = 20;
    const RECALL_FLOOR: f32 = 0.85;

    let (frame, garment, painted) = lapel(MARKS);
    let found = clothing::detect(&frame, &garment);
    let actionable: Vec<&clothing::Mark> = found.iter().filter(|m| m.is_actionable()).collect();

    let mut hits = 0usize;
    for (cx, cy) in &painted {
        let x = *cx as f32 / frame.width as f32;
        let y = *cy as f32 / frame.height as f32;
        if actionable.iter().any(|mark| {
            let region = mark.region;
            x >= region.x - 0.01
                && x <= region.x + region.w + 0.01
                && y >= region.y - 0.01
                && y <= region.y + region.h + 0.01
        }) {
            hits += 1;
        }
    }
    let recall = hits as f32 / MARKS as f32;
    println!("gate 6: lint recall {recall:.3} over {MARKS} painted marks");
    assert!(
        recall >= RECALL_FLOOR,
        "lint recall {recall:.3} is below {RECALL_FLOOR:.3} ({hits}/{MARKS})"
    );

    for mark in &actionable {
        let area = mark.region.w * mark.region.h;
        assert!(
            area <= MAX_CLOTHING_AREA + 1e-6,
            "a clothing candidate covers {area:.6} of the frame, above {MAX_CLOTHING_AREA:.6}"
        );
        assert!(
            !matches!(mark.kind, ClothingIssue::Strap | ClothingIssue::Crease),
            "the measured detector named an opt-in-only kind: {:?}",
            mark.kind
        );
    }

    // The control half. Everything the operator was aimed at is on the left; the right half must
    // come back with the same weave it went in with.
    let ops: Vec<MicroOp> = actionable
        .iter()
        .map(|mark| MicroOp::Clothing {
            region: mark.region,
            kind: mark.kind,
            strength: MAX_CLOTHING_STRENGTH,
        })
        .collect();
    let mut regions = BTreeMap::new();
    regions.insert(MicroRegion::Clothing, garment.clone());
    let context = MicroContext {
        regions,
        ..MicroContext::empty()
    };
    let guarded = guard::enforce(&frame, &ops, &context);

    let mut control = vec![0.0f32; frame.width * frame.height];
    for y in 64..frame.height {
        for x in frame.width / 2..frame.width {
            if let Some(slot) = control.get_mut(y * frame.width + x) {
                *slot = 1.0;
            }
        }
    }
    let (before, samples) = edge_energy(&frame.rgb, frame.width, frame.height, &control);
    let (after, _) = edge_energy(&guarded.rendered, frame.width, frame.height, &control);
    assert!(samples > 0, "the control half measured nothing");
    assert!(
        (after - before).abs() <= 1e-4 * before.max(1e-6),
        "fabric nobody aimed at changed: {before:.6} -> {after:.6}"
    );
}

/// Gate 7. A borrow aligns within tolerance, is small, and is disclosed.
///
/// Section 10.1: "borrowed regions align within tolerance and are always disclosed in the recipe
/// and Explain panel". The disclosure half is checked here on the plan, and again by the phase
/// gate on the stored row and the recipe.
#[test]
fn gate_7_a_borrow_aligns_and_is_disclosed() {
    let (image, pixels, context) = fixtures::glare_frame();
    let analyser = Analyser::new().expect("an analyser");
    let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");

    let borrows: Vec<&MicroOp> = outcome
        .plan
        .ops
        .iter()
        .filter(|op| op.borrowed_from().is_some())
        .collect();

    if borrows.is_empty() {
        // A refusal is a legitimate outcome, and it has to say which one it was.
        assert!(
            outcome.plan.reasons.iter().any(|reason| matches!(
                reason.code,
                MicroCode::BorrowNoAlignedSibling
                    | MicroCode::BorrowRefusedInformative
                    | MicroCode::BorrowRefusedTooLarge
                    | MicroCode::GlareReduced
            )),
            "no borrow and no reason: {:?}",
            outcome
                .plan
                .reasons
                .iter()
                .map(|reason| reason.code.as_str())
                .collect::<Vec<_>>()
        );
    } else {
        assert!(
            !outcome.plan.borrowed_from().is_empty(),
            "a composite plan named no source"
        );
        assert!(outcome
            .plan
            .reasons
            .iter()
            .any(|reason| reason.code == MicroCode::BorrowedFromSibling));
        for op in &borrows {
            let region = op.region().expect("a borrow names a region");
            assert!(
                region.w * region.h <= MAX_BORROW_AREA + 1e-6,
                "a borrow covers {:.6} of the frame, above {MAX_BORROW_AREA:.6}",
                region.w * region.h
            );
            assert!(matches!(
                op,
                MicroOp::Glare {
                    method: GlareMethod::BorrowFrom { .. },
                    ..
                }
            ));
        }
    }

    // The alignment floor, measured one level down on the same fixture, so the gate holds whether
    // or not the end-to-end pass chose to borrow.
    let target = to_linear(&pixels).expect("linear pixels");
    let sibling = context.siblings.first().expect("a sibling");
    let sibling_frame = borrow::SiblingFrame {
        image: sibling.image,
        frame: to_linear(&sibling.pixels).expect("linear sibling pixels"),
        face: sibling.faces.first().copied().expect("a sibling face"),
    };
    let face = context.faces.first().expect("a face");
    let eyes = upsample(
        context
            .regions
            .get(&MicroRegion::Eyes)
            .expect("an eye region"),
        target.width,
        target.height,
    );
    let sheets = glare::detect(&target, &eyes, &context.faces);
    let sheet = sheets.first().expect("the painted sheet was not detected");
    assert!(
        sheet.clipped_fraction >= MIN_SPECULAR_FRACTION,
        "the painted sheet reads as {:.3} clipped, below {MIN_SPECULAR_FRACTION:.3}",
        sheet.clipped_fraction
    );
    match borrow::choose(&target, sheet.region, face, &[sibling_frame]) {
        Ok(candidate) => assert!(
            candidate.alignment >= MIN_ALIGNMENT,
            "a borrow was chosen at alignment {:.3}, below {MIN_ALIGNMENT:.3}",
            candidate.alignment
        ),
        Err(refusal) => panic!("the aligned sibling was refused: {refusal:?}"),
    }
}

/// Gate 8. A region whose record survives is never borrowed into.
///
/// This is the rule that separates a glare repair from the eye swap section 2.2 forbids, and it
/// is not about the mechanism: a specular sheet has destroyed the record and a closed eye *is*
/// the record. The test lowers the sheet's clipping and asserts the refusal.
#[test]
fn gate_8_a_borrow_is_refused_where_the_record_survives() {
    let (_, pixels, context) = fixtures::glare_frame();
    let mut target = to_linear(&pixels).expect("linear pixels");
    // Pull the sheet back to a bright sheen: the same geometry, the same brightness ordering, but
    // the pixels underneath still carry information.
    for value in &mut target.rgb {
        *value = value.min(0.80);
    }
    let eyes = upsample(
        context
            .regions
            .get(&MicroRegion::Eyes)
            .expect("an eye region"),
        target.width,
        target.height,
    );
    let sheets = glare::detect(&target, &eyes, &context.faces);
    for sheet in &sheets {
        assert!(
            !sheet.may_borrow(),
            "a sheet at {:.3} clipped was offered for borrowing",
            sheet.clipped_fraction
        );
    }
}

/// Gate 9. Every contract ceiling is refused when a caller tries to exceed it.
///
/// Section 6.4: "guard code enforces the ceilings; a CI test attempts to exceed each ceiling and
/// asserts refusal". Nine attempts, one per bound, each of them the kind of value a hurried
/// caller or a hand-edited config file would produce.
#[test]
fn gate_9_every_ceiling_is_refused() {
    let identity = fixtures::identity(1);
    let region = Box2 {
        x: 0.10,
        y: 0.10,
        w: 0.02,
        h: 0.02,
    };

    let attempts: Vec<(&str, MicroOp)> = vec![
        (
            "flyaway strength",
            MicroOp::Flyaway {
                region,
                strength: MAX_FLYAWAY_STRENGTH + 0.05,
            },
        ),
        (
            "teeth luminance",
            MicroOp::Teeth {
                identity,
                luma: MAX_TEETH_LUMA_EV + 0.05,
                yellow_reduce: 0.10,
            },
        ),
        (
            "teeth yellow",
            MicroOp::Teeth {
                identity,
                luma: 0.05,
                yellow_reduce: MAX_TEETH_YELLOW + 0.05,
            },
        ),
        (
            "sclera",
            MicroOp::Eyes {
                identity,
                sclera: MAX_SCLERA + 0.05,
                iris_clarity: 0.05,
            },
        ),
        (
            "iris clarity",
            MicroOp::Eyes {
                identity,
                sclera: 0.05,
                iris_clarity: MAX_IRIS_CLARITY + 0.05,
            },
        ),
        (
            "clothing strength",
            MicroOp::Clothing {
                region,
                kind: ClothingIssue::Lint,
                strength: MAX_CLOTHING_STRENGTH + 0.05,
            },
        ),
        (
            "clothing area",
            MicroOp::Clothing {
                region: Box2 {
                    x: 0.1,
                    y: 0.1,
                    w: 0.5,
                    h: 0.5,
                },
                kind: ClothingIssue::Lint,
                strength: 0.5,
            },
        ),
        (
            "borrow area",
            MicroOp::Glare {
                region: Box2 {
                    x: 0.1,
                    y: 0.1,
                    w: 0.5,
                    h: 0.5,
                },
                method: GlareMethod::BorrowFrom {
                    source: fixtures::photo(9),
                    alignment: 0.95,
                },
            },
        ),
    ];

    for (what, op) in attempts {
        assert!(
            op.problem().is_some(),
            "the {what} ceiling was not enforced: {op:?}"
        );
    }

    // The guard's own configuration cannot be loosened either. A studio file that raised a bound
    // above the contract's is refused rather than clamped, because a promise a text file can
    // retract is not a promise.
    let loose = NaturalnessGuard {
        teeth_max_luma: MAX_TEETH_LUMA_EV + 0.10,
        ..NaturalnessGuard::CEILING
    };
    assert!(
        loose.problem().is_some(),
        "a guard that raised the teeth ceiling was accepted"
    );
    assert!(!loose.is_sound());

    // And an override that sets nothing is refused rather than stored as a no-op.
    assert!(guard::check_override(&MicroOverride {
        allowed: None,
        clothing: None,
        borrowing: None,
    })
    .is_err());
}

/// The rows of section 10.1 that this build cannot measure, named in the same place as the ones
/// it can.
///
/// Phase 20's rule, and phase 14's before it: a gate that is not run is recorded rather than
/// quietly dropped, so the exit report and the test file cannot disagree about what was proven.
#[test]
fn the_gates_this_build_cannot_measure_are_named() {
    let unmeasured = [
        "naturalness audit: 400 frames judged natural by retouchers at or above 95 %",
        "per-hair-type coverage: flyaway results across hair types, from a labelled corpus",
        "100 % zoom artefact audit: fabric and hairline inspected by a person at a monitor",
        "blind preference against Retouch4me and Evoto on this phase's five operations",
    ];
    assert_eq!(unmeasured.len(), 4);
    for row in unmeasured {
        assert!(!row.is_empty());
    }
}
