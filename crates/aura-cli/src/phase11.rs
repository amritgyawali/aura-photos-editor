//! The phase 11 mechanical gate.
//!
//! This is the assembly proof for composition: migration 11, the shipped scene rule
//! table, both registered ONNX heads through the real inference service, explainable
//! judgements and crop hints, query-derived resume, photographer dismissals, version
//! invalidation, deterministic output, telemetry summaries and the quantitative fixture
//! gates from section 10.1.
//!
//! The two shipped heads are placeholders. The gate therefore makes the same distinction
//! as phases 06, 09 and 10: the real heads prove model registration and execution, while
//! the reference-pose fixtures prove geometric accuracy against known ground truth. The
//! distinction is printed at the end of every run rather than hidden in a test helper.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_photo::composition::fixtures::{self, Canvas};
use aura_brain_photo::composition::keypoints::{AESTHETIC_MODEL_VERSION, KEYPOINT_MODEL_VERSION};
use aura_brain_photo::composition::store::Provenance;
use aura_brain_photo::composition::{
    Analyser, Composition, CompositionStore, FrameContext, RuleTable, ANALYSIS_VER, MODEL_VER,
};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::composition::{
    Box2, CompositionCode, CompositionFlags, CompositionResult, CompositionService,
};
use aura_core::contract::people::ImageSubjects;
use aura_core::{AuraResult, MomentId, PhotoId, Priority, ProjectId, SceneId};
use aura_infer::contract::infer::{
    BatchDefaults, HardwarePlan, InferRequest, InferService, ModelClass, ModelRef, Precision,
    TensorView,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures as onnx_fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

const ANGLE_GATE: f32 = 0.4;
const CROP_F1_GATE: f64 = 0.90;
const MERGE_RECALL_GATE: f64 = 0.85;
const MERGE_FP_GATE: f64 = 0.10;
const AGREEMENT_GATE: f64 = 0.78;

/// Run the phase 11 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase11-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 11 and all three objects it owns.
    let catalog_path = work.join("phase11.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 11 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 11, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "image_composition"),
        ("view", "v_composition_coverage"),
        ("view", "v_composition_flags"),
        ("index", "idx_composition_project"),
    ] {
        match schema_object(&catalog, kind, name) {
            Ok(true) => println!("  {kind} {name}: present"),
            Ok(false) => {
                eprintln!("  {kind} {name}: missing");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  {kind} {name}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 2. The scene-conditioned product decisions load and cover the frozen taxonomy.
    let rules = match RuleTable::embedded() {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("rules: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let missing = rules.unruled();
    println!(
        "rules: version {}, {} scene rows, neutral fallback measured={}",
        rules.version(),
        rules.rows(),
        rules.neutral().measured
    );
    if missing.is_empty() && rules.rows() == SceneId::ALL.len().saturating_sub(1) {
        println!("  every named scene has its own framing row");
    } else {
        eprintln!("  incomplete rule table; missing {missing:?}");
        failures += 1;
    }

    // 3. Register, warm and execute both real heads. This is deliberately separate from
    //    the reference-pose accuracy gates below: the real weights are placeholders, but
    //    the runtime assembly is still a claim this gate can and must prove.
    let infer = match gate_engine(&work, Arc::clone(&clock)) {
        Ok(engine) => engine,
        Err(err) => {
            eprintln!("infer: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let analyser = match Analyser::new(Arc::clone(&infer)) {
        Ok(built) => built,
        Err(err) => {
            eprintln!("analyser: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match analyser.warmup() {
        Ok(()) => println!(
            "heads: {} and {} loaded on {}",
            onnx_fixtures::POSE_MODEL,
            onnx_fixtures::AESTHETIC_MODEL,
            analyser.ep()
        ),
        Err(err) => {
            eprintln!("heads: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match exercise_registered_heads(&infer) {
        Ok((pose_values, aesthetic_values))
            if pose_values == onnx_fixtures::POSE_OUTPUTS && aesthetic_values == 1 =>
        {
            println!(
                "  registered architecture fixtures executed: {pose_values} pose values, \
                 {aesthetic_values} aesthetic value"
            );
        }
        Ok((pose_values, aesthetic_values)) => {
            eprintln!(
                "  registered heads returned {pose_values} pose and {aesthetic_values} \
                 aesthetic values"
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("head execution: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let mut real_canvas = fixtures::bright_blob();
    real_canvas.add_face(
        Box2 {
            x: 0.50,
            y: 0.42,
            w: 0.12,
            h: 0.14,
        },
        52,
    );
    let started = clock.monotonic_ms();
    match analyser.analyse(
        PhotoId::new(),
        &buffer_of(&real_canvas),
        &context_of(&real_canvas, real_canvas.truth.scene),
        Priority::AiBatch,
    ) {
        Ok(result) => {
            let elapsed = clock.monotonic_ms().saturating_sub(started);
            let aesthetic_withheld = result
                .reasons
                .iter()
                .any(|reason| reason.code == CompositionCode::AestheticUnavailable);
            let keypoints_withheld = result
                .reasons
                .iter()
                .any(|reason| reason.code == CompositionCode::KeypointsUnavailable)
                && result.keypoint_subjects == 0
                && result.joint_cuts.is_empty();
            let evidence = result
                .reasons
                .iter()
                .filter(|reason| reason.evidence.is_some())
                .count();
            println!(
                "placeholder product path: {} trusted subjects, {} reasons, {evidence} evidence \
                 boxes, {} ms",
                result.keypoint_subjects,
                result.reasons.len(),
                elapsed
            );
            if !aesthetic_withheld || !keypoints_withheld || result.reasons.is_empty() {
                eprintln!(
                    "  untrained pose/aesthetic outputs were trusted instead of explicitly \
                     withheld"
                );
                failures += 1;
            } else {
                println!(
                    "  untrained outputs were not trusted: KeypointsUnavailable and \
                     AestheticUnavailable are explicit"
                );
            }
        }
        Err(err) => {
            eprintln!("real inference: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 4. End-to-end fixture judgements. Poses come from painted ground truth and taste
    //    comes from the deterministic reference scorer; the registered-but-untrained
    //    architecture fixtures above are never promoted to observations.
    let reference = analyser.clone().with_reference_poses();
    let project = ProjectId::new();
    let fixture_rows: Vec<(&str, Canvas)> = vec![
        ("intentional_dutch", fixtures::intentional_dutch()),
        ("accidental_dutch", fixtures::accidental_dutch()),
        ("cut_at_wrist", fixtures::cut_at_wrist()),
        ("bright_blob", fixtures::bright_blob()),
        ("head_merge", fixtures::head_merge()),
        ("colour_clash", fixtures::colour_clash()),
        ("flat_lay", fixtures::flat_lay()),
        ("level", fixtures::tilted(0.2)),
    ];
    let photos = match seed_project(&catalog, &project, fixture_rows.len()) {
        Ok(ids) => ids,
        Err(err) => {
            eprintln!("seed: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let mut readings: Vec<(&str, CompositionResult)> = Vec::new();
    for ((name, canvas), photo) in fixture_rows.iter().zip(&photos) {
        match reference.analyse(
            *photo,
            &buffer_of(canvas),
            &context_of(canvas, canvas.truth.scene),
            Priority::AiBatch,
        ) {
            Ok(result) => readings.push((*name, result)),
            Err(err) => {
                eprintln!("judgement {name}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    let portrait_frames = fixture_rows
        .iter()
        .filter(|(_, canvas)| !canvas.faces.is_empty())
        .count();
    let hinted = readings
        .iter()
        .filter(|(_, result)| result.crop_suggestion_hint.is_some())
        .count();
    let explained = readings
        .iter()
        .filter(|(_, result)| {
            !result.reasons.is_empty()
                && result.reasons.len() <= CompositionResult::MAX_REASONS
                && (0.0..=1.0).contains(&result.confidence)
        })
        .count();
    let evidence = readings
        .iter()
        .flat_map(|(_, result)| &result.reasons)
        .filter(|reason| reason.evidence.is_some())
        .count();
    println!(
        "judgements: {} of {} scored; {explained} explained; {evidence} evidence boxes; \
         {hinted}/{portrait_frames} portrait crop hints",
        readings.len(),
        fixture_rows.len()
    );
    if readings.len() != fixture_rows.len()
        || explained != readings.len()
        || evidence < 3
        || hinted < portrait_frames
    {
        eprintln!("  end-to-end explanation or crop-hint coverage is incomplete");
        failures += 1;
    }

    // The two trust-sensitive worked examples: style is respected, and a saturated
    // distraction is actually named and localised.
    if let Some(intentional) = named(&readings, "intentional_dutch") {
        if intentional.tilt_intentional
            && intentional
                .flags
                .contains(CompositionFlags::INTENTIONAL_TILT)
            && !intentional.flags.contains(CompositionFlags::HORIZON_TILTED)
        {
            println!("intentional tilt: exonerated and retained as a positive flag");
        } else {
            eprintln!(
                "intentional tilt: intentional={} flags={}",
                intentional.tilt_intentional, intentional.flags
            );
            failures += 1;
        }
    } else {
        failures += 1;
    }
    if let Some(colour) = named(&readings, "colour_clash") {
        let local_reason = colour.reasons.iter().any(|reason| {
            reason.code == CompositionCode::ColourCompetition && reason.evidence.is_some()
        });
        if colour.colour_competition > 0.35
            && colour.flags.contains(CompositionFlags::COLOUR_COMPETITION)
            && local_reason
        {
            println!(
                "colour competition: {:.3}, flagged with local evidence",
                colour.colour_competition
            );
        } else {
            eprintln!(
                "colour competition: {:.3}, flags={}, local_reason={local_reason}",
                colour.colour_competition, colour.flags
            );
            failures += 1;
        }
    } else {
        failures += 1;
    }

    // 5. Query-derived resume, persistence, moment ranking and version invalidation.
    let store = Arc::new(CompositionStore::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
    ));
    let service = Composition::new(Arc::clone(&store));
    let midpoint = readings.len() / 2;
    for (_, result) in readings.iter().take(midpoint) {
        if let Err(err) = store.put(&project, result, &Provenance::default()) {
            eprintln!("store first half: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    let current_pending = store
        .pending(&project, MODEL_VER, ANALYSIS_VER, rules.version())
        .unwrap_or_default();
    if current_pending.len() == readings.len().saturating_sub(midpoint) {
        println!(
            "resume: {} stored, {} returned by the pending query",
            midpoint,
            current_pending.len()
        );
    } else {
        eprintln!(
            "resume: expected {} pending, found {}",
            readings.len().saturating_sub(midpoint),
            current_pending.len()
        );
        failures += 1;
    }
    for (_, result) in readings
        .iter()
        .filter(|(_, result)| current_pending.contains(&result.image_id))
    {
        if let Err(err) = store.put(&project, result, &Provenance::default()) {
            eprintln!("resume write: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match store.pending(&project, MODEL_VER, ANALYSIS_VER, rules.version()) {
        Ok(pending) if pending.is_empty() => println!("  resumed pass has no work left"),
        Ok(pending) => {
            eprintln!("  resumed pass left {} rows pending", pending.len());
            failures += 1;
        }
        Err(err) => {
            eprintln!("  pending: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let moment = match seed_moment(&catalog, &project, &photos) {
        Ok(id) => Some(id),
        Err(err) => {
            eprintln!("moment: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };
    if let Some(moment) = moment {
        match store.refresh_relative_composition(&project) {
            Ok(changed) => match service.within_moment(moment) {
                Ok(ranked)
                    if changed == readings.len()
                        && ranked.len() == readings.len()
                        && ranked
                            .first()
                            .is_some_and(|(_, rank)| (*rank - 1.0).abs() < 1e-6)
                        && ranked.last().is_some_and(|(_, rank)| rank.abs() < 1e-6) =>
                {
                    println!("moment: {} frames ranked from 1.0 to 0.0", ranked.len());
                }
                Ok(ranked) => {
                    eprintln!("moment: changed {changed}, ranks {ranked:?}");
                    failures += 1;
                }
                Err(err) => {
                    eprintln!("moment ranking: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            },
            Err(err) => {
                eprintln!("moment refresh: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    if let Some(bright) = named(&readings, "bright_blob") {
        match service.of_image(bright.image_id) {
            Ok(Some(round_trip))
                if round_trip.flags.contains(CompositionFlags::BRIGHT_BLOB)
                    && round_trip
                        .reasons
                        .iter()
                        .any(|reason| reason.evidence.is_some()) =>
            {
                println!("persistence: flags, reasons and evidence round-trip");
            }
            Ok(Some(round_trip)) => {
                eprintln!("persistence: incomplete round-trip: {}", round_trip.flags);
                failures += 1;
            }
            Ok(None) => {
                eprintln!("persistence: bright-blob row disappeared");
                failures += 1;
            }
            Err(err) => {
                eprintln!("persistence: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }

        if let Err(err) = service.dismiss(bright.image_id, CompositionFlags::BRIGHT_BLOB) {
            eprintln!("dismiss: [{}] {}", err.code, err.detail);
            failures += 1;
        } else if let Err(err) = store.put(&project, bright, &Provenance::default()) {
            eprintln!("dismiss re-analysis: [{}] {}", err.code, err.detail);
            failures += 1;
        } else {
            match service.of_image(bright.image_id) {
                Ok(Some(after))
                    if after.user_reviewed
                        && !after.flags.contains(CompositionFlags::BRIGHT_BLOB) =>
                {
                    println!("dismissal: photographer decision survived re-analysis");
                }
                Ok(Some(after)) => {
                    eprintln!(
                        "dismissal: reviewed={} flags={}",
                        after.user_reviewed, after.flags
                    );
                    failures += 1;
                }
                Ok(None) => {
                    eprintln!("dismissal: row disappeared");
                    failures += 1;
                }
                Err(err) => {
                    eprintln!("dismissal: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }
    }

    for (label, model, analysis, rule_version) in [
        (
            "model",
            MODEL_VER.saturating_add(1),
            ANALYSIS_VER,
            rules.version(),
        ),
        (
            "analysis",
            MODEL_VER,
            ANALYSIS_VER.saturating_add(1),
            rules.version(),
        ),
        (
            "rules",
            MODEL_VER,
            ANALYSIS_VER,
            rules.version().saturating_add(1),
        ),
    ] {
        match store.pending(&project, model, analysis, rule_version) {
            Ok(pending) if pending.len() == readings.len() => {
                println!(
                    "versions: a {label} bump invalidates all {} rows",
                    pending.len()
                );
            }
            Ok(pending) => {
                eprintln!(
                    "versions: a {label} bump invalidated {} of {} rows",
                    pending.len(),
                    readings.len()
                );
                failures += 1;
            }
            Err(err) => {
                eprintln!("versions {label}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    match service.outline(project) {
        Ok(outline) => println!(
            "telemetry composition.scored: images={} mean_score={:.3} flag_histogram={:?}; \
             composition.tilt: mean_abs_deg={:.3} intentional_ratio={:.3}; \
             coverage={:.0}% keypoint_aware={:.0}% reviewed={} hinted={}",
            outline.scored,
            outline.mean_score,
            outline.flag_histogram,
            outline.mean_abs_tilt,
            outline.intentional_ratio,
            outline.coverage * 100.0,
            outline.keypoint_aware * 100.0,
            outline.reviewed,
            outline.hinted
        ),
        Err(err) => {
            eprintln!("telemetry: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 6. Determinism through the real aesthetic head, with the same pixels, context and id.
    let deterministic_canvas = fixtures::head_merge();
    let deterministic_id = PhotoId::new();
    let deterministic_buffer = buffer_of(&deterministic_canvas);
    let deterministic_context = context_of(&deterministic_canvas, deterministic_canvas.truth.scene);
    let first = reference.analyse(
        deterministic_id,
        &deterministic_buffer,
        &deterministic_context,
        Priority::AiBatch,
    );
    let second = reference.analyse(
        deterministic_id,
        &deterministic_buffer,
        &deterministic_context,
        Priority::AiBatch,
    );
    match (first, second) {
        (Ok(left), Ok(right)) if left == right => println!(
            "determinism: byte-equivalent judgement fields at score {:.6}",
            left.composition_score
        ),
        (Ok(left), Ok(right)) => {
            eprintln!(
                "determinism: scores {:.6} and {:.6}, flags {} and {}",
                left.composition_score, right.composition_score, left.flags, right.flags
            );
            failures += 1;
        }
        _ => {
            eprintln!("determinism: one of two identical reads failed");
            failures += 1;
        }
    }

    // 7. The numerical section 10.1 gates, against constructed ground truth.
    let metrics = match Analyser::new(Arc::new(NullInfer::new())) {
        Ok(built) => built.with_reference_poses(),
        Err(err) => {
            eprintln!("metric analyser: [{}] {}", err.code, err.detail);
            return finish(failures + 1);
        }
    };
    failures += quantitative_gates(&metrics);

    println!();
    println!("open conditions carried into the exit report:");
    println!("  pose and aesthetic weights are placeholders; synthetic poses gate geometry");
    println!("  pairwise agreement is over authored fixtures, not photographer labels");
    println!("  the score calibration is the identity map until real labels exist");
    println!("  gravity metadata is not wired by any catalogued camera body");
    println!("  background saliency must be revalidated after Phase 18 segmentation ships");

    finish(failures)
}

fn quantitative_gates(analyser: &Analyser) -> usize {
    let mut failures = 0usize;

    let mut worst_angle = 0.0f32;
    for canvas in fixtures::tilt_ladder() {
        match judge(analyser, &canvas, SceneId::Venue) {
            Some(result) if result.horizon_source.is_measured() => {
                worst_angle = worst_angle.max((result.tilt_deg - canvas.truth.tilt_deg).abs());
            }
            _ => failures += 1,
        }
    }
    println!("metric horizon: worst error {worst_angle:.3} deg (gate <= {ANGLE_GATE:.1})");
    if worst_angle > ANGLE_GATE {
        failures += 1;
    }

    let cases = [
        (fixtures::cut_at_wrist(), true),
        (fixtures::cut_mid_limb(), true),
        (fixtures::tight_but_uncut(), false),
    ];
    let (mut tp, mut fp, mut missed) = (0.0f64, 0.0f64, 0.0f64);
    for (canvas, expected) in &cases {
        let found = judge(analyser, canvas, canvas.truth.scene)
            .is_some_and(|result| !result.flagged_cuts().is_empty());
        match (*expected, found) {
            (true, true) => tp += 1.0,
            (true, false) => missed += 1.0,
            (false, true) => fp += 1.0,
            (false, false) => {}
        }
    }
    let precision = if tp + fp == 0.0 { 0.0 } else { tp / (tp + fp) };
    let recall = if tp + missed == 0.0 {
        0.0
    } else {
        tp / (tp + missed)
    };
    let crop_f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    println!(
        "metric crop audit: precision {precision:.3}, recall {recall:.3}, F1 {crop_f1:.3} \
         (gate >= {CROP_F1_GATE:.2})"
    );
    if crop_f1 < CROP_F1_GATE {
        failures += 1;
    }

    let positives = [fixtures::head_merge()];
    let negatives = [
        fixtures::clean_behind_head(),
        fixtures::bright_blob(),
        fixtures::tight_but_uncut(),
        fixtures::flat_lay(),
    ];
    let merge_hits = positives
        .iter()
        .filter(|canvas| {
            judge(analyser, canvas, canvas.truth.scene).is_some_and(|result| result.head_merge)
        })
        .count();
    let merge_false = negatives
        .iter()
        .filter(|canvas| {
            judge(analyser, canvas, canvas.truth.scene).is_some_and(|result| result.head_merge)
        })
        .count();
    let merge_recall = merge_hits as f64 / positives.len() as f64;
    let merge_fp = merge_false as f64 / negatives.len() as f64;
    println!(
        "metric head merge: recall {merge_recall:.3} (>= {MERGE_RECALL_GATE:.2}), \
         false-positive rate {merge_fp:.3} (< {MERGE_FP_GATE:.2})"
    );
    if merge_recall < MERGE_RECALL_GATE || merge_fp >= MERGE_FP_GATE {
        failures += 1;
    }

    let pairs = fixtures::composition_pairs();
    let agreed = pairs
        .iter()
        .filter(|(worse, better)| {
            let low = judge(analyser, worse, worse.truth.scene)
                .map_or(0.0, |result| result.composition_score);
            let high = judge(analyser, better, better.truth.scene)
                .map_or(0.0, |result| result.composition_score);
            high > low
        })
        .count();
    let agreement = agreed as f64 / pairs.len() as f64;
    println!(
        "metric pairwise agreement: {agreement:.3} over {} pairs (gate >= {AGREEMENT_GATE:.2})",
        pairs.len()
    );
    if agreement < AGREEMENT_GATE {
        failures += 1;
    }

    let cropped = fixtures::head_cropped();
    let strict = judge(analyser, &cropped, SceneId::FamilyPortrait);
    let deliberate = judge(analyser, &cropped, SceneId::CouplePortrait);
    let contextual = strict.is_some_and(|result| {
        result.flags.contains(CompositionFlags::HEAD_CROPPED) && result.headroom < 0.0
    }) && deliberate.is_some_and(|result| {
        !result.flags.contains(CompositionFlags::HEAD_CROPPED)
            && result
                .reasons
                .iter()
                .any(|reason| reason.code == CompositionCode::DeliberateCrop)
    });
    println!("metric head crop: context-dependent={contextual}");
    if !contextual {
        failures += 1;
    }

    failures
}

fn finish(failures: usize) -> ExitCode {
    println!();
    if failures == 0 {
        println!("phase 11 gate: all mechanical checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase 11 gate: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

fn named<'a>(
    rows: &'a [(&'static str, CompositionResult)],
    name: &str,
) -> Option<&'a CompositionResult> {
    rows.iter()
        .find(|(label, _)| *label == name)
        .map(|(_, result)| result)
}

fn judge(analyser: &Analyser, canvas: &Canvas, scene: SceneId) -> Option<CompositionResult> {
    analyser
        .analyse(
            PhotoId::new(),
            &buffer_of(canvas),
            &context_of(canvas, scene),
            Priority::AiBatch,
        )
        .ok()
}

fn buffer_of(canvas: &Canvas) -> PixelBuffer {
    PixelBuffer {
        width: canvas.width as u32,
        height: canvas.height as u32,
        data: PixelData::Srgb8(canvas.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_of(canvas: &Canvas, scene: SceneId) -> FrameContext {
    FrameContext {
        scene,
        subjects: ImageSubjects {
            faces: canvas.faces.clone(),
            people_count: u32::try_from(canvas.faces.len()).unwrap_or(0),
            weights_ver: 1,
            ..ImageSubjects::default()
        },
        scene_known: true,
        gravity_deg: None,
    }
}

fn schema_object(catalog: &Arc<Catalog>, kind: &str, name: &str) -> AuraResult<bool> {
    let kind = kind.to_string();
    let name = name.to_string();
    catalog.read(move |conn| {
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                rusqlite::params![kind, name],
                |row| row.get(0),
            )
            .map_err(|err| aura_core::errors::db::statement_failed("phase 11 schema", &err))?;
        Ok(found == 1)
    })
}

fn seed_project(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    count: usize,
) -> AuraResult<Vec<PhotoId>> {
    let project_key = project.to_db();
    let ids: Vec<PhotoId> = (0..count).map(|_| PhotoId::new()).collect();
    let rows: Vec<String> = ids.iter().map(PhotoId::to_db).collect();
    catalog.writer().transact(move |tx| {
        tx.execute(
            "INSERT INTO project (project_id, name, created_at, updated_at)
             VALUES (?1, 'phase11', '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
            rusqlite::params![project_key],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("phase 11 project", &err))?;
        for (index, photo) in rows.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
                         '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
                rusqlite::params![
                    photo,
                    project_key,
                    format!("2026-08-15T12:00:{:02}Z", index % 60),
                ],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("phase 11 photo", &err))?;
        }
        Ok(())
    })?;
    Ok(ids)
}

fn seed_moment(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    photos: &[PhotoId],
) -> AuraResult<MomentId> {
    let moment = MomentId::new();
    let moment_key = moment.to_db();
    let project_key = project.to_db();
    let members: Vec<String> = photos.iter().map(PhotoId::to_db).collect();
    catalog.writer().transact(move |tx| {
        tx.execute(
            "INSERT INTO moments (id, project_id, start_ts, end_ts, frame_count,
                                  confidence, reasons, created_at, updated_at)
             VALUES (?1, ?2, 0, 1000, ?3, 0.9, '[\"seeded by the phase 11 gate\"]',
                     '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
            rusqlite::params![
                moment_key,
                project_key,
                i64::try_from(members.len()).unwrap_or(0),
            ],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("phase 11 moment", &err))?;
        for (position, photo) in members.iter().enumerate() {
            tx.execute(
                "INSERT INTO moment_images (moment_id, photo_id, position, burst_ix)
                 VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![moment_key, photo, i64::try_from(position).unwrap_or(0),],
            )
            .map_err(|err| {
                aura_core::errors::db::statement_failed("phase 11 moment member", &err)
            })?;
        }
        Ok(())
    })?;
    Ok(moment)
}

/// Execute the two registered architecture fixtures directly.
///
/// Product analysis intentionally skips them while their model cards say the weights are
/// untrained. Direct execution proves the loader, registry, interpreter and tensor
/// contracts without turning random output into a composition claim.
fn exercise_registered_heads(infer: &Arc<dyn InferService>) -> AuraResult<(usize, usize)> {
    let pose_input =
        vec![0.0f32; 3 * onnx_fixtures::POSE_CROP_SIDE * onnx_fixtures::POSE_CROP_SIDE];
    let pose_view = TensorView::new(
        vec![
            1,
            3,
            onnx_fixtures::POSE_CROP_SIDE,
            onnx_fixtures::POSE_CROP_SIDE,
        ],
        &pose_input,
    )?;
    let pose = infer.run(InferRequest::new(
        ModelRef::new(onnx_fixtures::POSE_MODEL, KEYPOINT_MODEL_VERSION),
        vec![pose_view],
        Priority::AiBatch,
    ))?;
    let pose_values = pose.outputs.first().map_or(0, |tensor| tensor.data.len());

    let aesthetic_input = vec![0.0f32; onnx_fixtures::AESTHETIC_FEATURES];
    let aesthetic_view =
        TensorView::new(vec![1, onnx_fixtures::AESTHETIC_FEATURES], &aesthetic_input)?;
    let aesthetic = infer.run(InferRequest::new(
        ModelRef::new(onnx_fixtures::AESTHETIC_MODEL, AESTHETIC_MODEL_VERSION),
        vec![aesthetic_view],
        Priority::AiBatch,
    ))?;
    let aesthetic_values = aesthetic
        .outputs
        .first()
        .map_or(0, |tensor| tensor.data.len());
    Ok((pose_values, aesthetic_values))
}

/// An inference service over the two phase 11 graphs, written to disk for this run.
fn gate_engine(
    directory: &std::path::Path,
    clock: Arc<dyn Clock>,
) -> AuraResult<Arc<dyn InferService>> {
    let pose = directory.join("pose_keypoints.onnx");
    let aesthetic = directory.join("aesthetic_head.onnx");
    for (path, model) in [
        (&pose, onnx_fixtures::pose_keypoints()),
        (&aesthetic, onnx_fixtures::aesthetic_head()),
    ] {
        std::fs::write(path, serialise(&model))
            .map_err(|err| aura_core::errors::io::unreadable(path, &err))?;
    }

    let source = Arc::new(
        StaticModelSource::new()
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    onnx_fixtures::POSE_MODEL,
                    KEYPOINT_MODEL_VERSION,
                ),
                &pose,
                Precision::Fp32,
                ModelClass::Embedding,
                8,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    onnx_fixtures::AESTHETIC_MODEL,
                    AESTHETIC_MODEL_VERSION,
                ),
                &aesthetic,
                Precision::Fp32,
                ModelClass::Embedding,
                1,
            ),
    );
    let mut plan = HardwarePlan::conservative(4, "2026-08-15T00:00:00Z".to_string());
    plan.default_batch = BatchDefaults {
        embedding: 8,
        segmentation: 2,
        retouch: 1,
    };
    Ok(Arc::new(InferEngine::new(
        BackendRegistry::with_reference(),
        source,
        plan,
        clock,
    )))
}

/// An inference service that refuses model execution for the metric-only reference path.
#[derive(Debug)]
struct NullInfer {
    plan: HardwarePlan,
}

impl NullInfer {
    fn new() -> Self {
        Self {
            plan: HardwarePlan::conservative(1, "1970-01-01T00:00:00Z".to_string()),
        }
    }
}

impl InferService for NullInfer {
    fn run(
        &self,
        _request: aura_infer::contract::infer::InferRequest<'_>,
    ) -> Result<aura_infer::contract::infer::InferResult, aura_infer::contract::infer::InferError>
    {
        Err(aura_core::errors::ml::cancelled("reference metric path"))
    }

    fn run_batch(
        &self,
        _model: aura_infer::contract::infer::ModelRef,
        _inputs: Vec<Vec<aura_infer::contract::infer::TensorView<'_>>>,
        _priority: Priority,
    ) -> Result<
        Vec<aura_infer::contract::infer::InferResult>,
        aura_infer::contract::infer::InferError,
    > {
        Err(aura_core::errors::ml::cancelled("reference metric path"))
    }

    fn plan(&self) -> &HardwarePlan {
        &self.plan
    }

    fn warmup(
        &self,
        _models: &[aura_infer::contract::infer::ModelRef],
    ) -> Result<(), aura_infer::contract::infer::InferError> {
        Ok(())
    }
}
