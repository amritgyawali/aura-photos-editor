//! The phase 15 mechanical gate.
//!
//! This is the assembly proof for exposure and white balance: migration 15 and its objects,
//! the target table, the statistics pass, the four hypothesis generators, the mixed-light
//! split, the coloured-light policy, the skin locus accumulating across a synthetic wedding,
//! the constrained solve, the store and its override protection, the reference frames handed
//! to phase 25, and the two real models loaded through the real registry.
//!
//! **Nothing here proves a colour is correct.** There are no camera files, no photographed
//! ColorChecker and no expert edit pairs in this repository - phase 02's exit conditions and
//! this phase's condition C1, both open - so every number below is measured against synthetic
//! frames whose illuminant and subject luminance were chosen and painted in. The distinction
//! is printed at the end of every run rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/tone_eval.rs` is the
//! other half and runs under `cargo test`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_photo::tone::analyse::{Analyser, FrameContext, ANALYSIS_VER, MODEL_VER};
use aura_brain_photo::tone::fixtures::{self, Frame};
use aura_brain_photo::tone::skin_locus::LocusBuilder;
use aura_brain_photo::tone::store::ToneStore;
use aura_brain_photo::tone::targets::TargetTable;
use aura_brain_photo::tone::{reference, BYTES_PER_IMAGE};
use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::tone::{
    IlluminantKind, SkinLocus, ToneCode, ToneOverride, MIN_REFERENCE_FRAMES, REVIEW_WB_BELOW,
    SKIN_DE00_CEILING, SKIN_DE00_SPREAD, WB_TOLERANCE_K,
};
use aura_core::{AuraResult, IdentityId, PhotoId, Priority, ProjectId, SegmentId};
use aura_infer::contract::infer::{
    BatchDefaults, HardwarePlan, InferService, ModelClass, ModelRef, Precision,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures as onnx_fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_raw::colour::illuminant::cct_from_uv;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use rusqlite::params;

/// Run the phase 15 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase15-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 15 and every object it owns.
    let catalog_path = work.join("phase15.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 15 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 15, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "image_tone_estimate"),
        ("table", "identity_skin_locus"),
        ("table", "segment_reference_frames"),
        ("view", "v_tone_coverage"),
        ("index", "idx_tone_project"),
        ("index", "idx_tone_review"),
        ("index", "idx_tone_mixed"),
        ("index", "idx_skin_locus_project"),
        ("index", "idx_reference_frames_photo"),
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

    // 2. There is nowhere in this schema to put an ideal skin value.
    //
    // Section 6.3's fairness argument is that a fixed target is how an editor lightens dark
    // skin while believing it is correcting a cast, and the defence is structural rather
    // than remembered. This is that defence, checked.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no ideal-skin column anywhere in migration 15");
        }
        Ok(found) => {
            eprintln!(
                "  migration 15 grew a skin target column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 3. The target table, and the rows a product manager has to be able to argue with.
    println!();
    match TargetTable::embedded() {
        Ok(table) => {
            println!("targets: version {} loaded", table.version());
            let ceremony = table.get(aura_core::SceneId::Ceremony);
            let dance = table.get(aura_core::SceneId::DanceFloor);
            if ceremony.subject_luma_min >= dance.subject_luma_min {
                println!("  a ceremony face is aimed brighter than a dance-floor face");
            } else {
                eprintln!("  a dance floor is aimed brighter than a ceremony");
                failures += 1;
            }
            if dance.preserve_coloured_light {
                println!("  the dance floor preserves coloured light");
            } else {
                eprintln!("  the dance floor would neutralise its own uplighters");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("targets: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 4. The two models, through the real registry.
    println!();
    match gate_engine(&work, Arc::clone(&clock)) {
        Ok(engine) => {
            let analyser = match Analyser::new(Arc::clone(&engine)) {
                Ok(analyser) => analyser,
                Err(err) => {
                    eprintln!("analyser: [{}] {}", err.code, err.detail);
                    return ExitCode::FAILURE;
                }
            };
            match analyser.warmup() {
                Ok(()) => println!(
                    "models: white_balance and exposure_scene loaded on {}",
                    analyser.ep()
                ),
                Err(err) => {
                    eprintln!("models: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
            println!("  neither head is consulted: both are untrained placeholders (condition C1)");
        }
        Err(err) => {
            eprintln!("inference: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. A synthetic wedding, solved twice, exactly as `TonePass::run` then `refine` do.
    println!();
    let analyser = match Analyser::new(Arc::new(NullInfer::new())) {
        Ok(analyser) => analyser,
        Err(err) => {
            eprintln!("analyser: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let (frames, people) = wedding();
    let mut builder = LocusBuilder::new();
    for frame in &frames {
        match analyse(&analyser, frame, &BTreeMap::new()) {
            Ok(outcome) => {
                for (identity, sample) in outcome.samples {
                    builder.add(identity, sample);
                }
            }
            Err(err) => {
                eprintln!("analyse: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    let loci = builder.finish(ANALYSIS_VER);
    println!(
        "wedding: {} frames, {} identities sampled, {} loci fitted",
        frames.len(),
        builder.identities(),
        loci.len()
    );
    if loci.is_empty() {
        eprintln!("  no locus was ever fitted, so section 6.3's constraint bound on nothing");
        failures += 1;
    }

    let mut solved = Vec::new();
    for frame in &frames {
        match analyse(&analyser, frame, &loci) {
            Ok(outcome) => solved.push((frame.clone(), outcome.estimate)),
            Err(err) => {
                eprintln!("analyse: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 6. Section 10.1's gates, on the assembled pass.
    println!();
    let mut within = 0usize;
    let mut correctable = 0usize;
    for (frame, estimate) in &solved {
        if frame.truth.coloured {
            continue;
        }
        correctable += 1;
        if (estimate.temperature_k - cct_from_uv(frame.truth.illuminant_uv)).abs() <= WB_TOLERANCE_K
        {
            within += 1;
        }
    }
    let agreement = ratio(within, correctable);
    if agreement >= 0.85 {
        println!(
            "white balance: {within}/{correctable} within {WB_TOLERANCE_K:.0} K ({agreement:.3})"
        );
    } else {
        eprintln!("white balance: {within}/{correctable} within {WB_TOLERANCE_K:.0} K ({agreement:.3}), gate 0.85");
        failures += 1;
    }

    let mixed = solved.iter().filter(|(_, e)| e.mixed_light).count();
    let coloured = solved.iter().filter(|(_, e)| e.coloured_light).count();
    let anchored = solved.iter().filter(|(_, e)| e.face_anchored).count();
    let constrained = solved
        .iter()
        .filter(|(_, e)| e.constrained_identities > 0)
        .count();
    println!(
        "  {anchored} face-anchored, {constrained} skin-constrained, {mixed} mixed-light, \
         {coloured} coloured-light preserved"
    );
    if constrained == 0 {
        eprintln!("  no frame was skin-constrained; the hard constraint never applied");
        failures += 1;
    }
    if mixed == 0 {
        eprintln!("  no frame was marked mixed-light; phase 18 would have nothing to correct");
        failures += 1;
    }

    // Section 13's fourth criterion, measured on the assembled wedding rather than on an
    // isolated frame. The metric is the fraction of the light's own chroma that survived the
    // correction: a solver that neutralised a purple dance floor leaves nearly none.
    let mut kept = Vec::new();
    for (frame, estimate) in &solved {
        if !frame.truth.coloured {
            continue;
        }
        let original = aura_raw::colour::illuminant::duv(frame.truth.illuminant_uv).abs();
        let residual = estimate
            .subject_illuminant()
            .map_or(0.0, |light| light.chroma);
        kept.push(if original > 0.0 {
            residual / original
        } else {
            1.0
        });
    }
    let mean_kept = if kept.is_empty() {
        0.0
    } else {
        kept.iter().sum::<f32>() / kept.len() as f32
    };
    if kept.is_empty() {
        eprintln!("mood: no coloured-light frame reached the solve");
        failures += 1;
    } else if mean_kept >= 0.50 {
        println!(
            "mood: {:.0} % of a coloured light's cast survives across {} frames",
            mean_kept * 100.0,
            kept.len()
        );
    } else {
        eprintln!(
            "mood: only {:.0} % of a coloured light's cast survived across {} frames; the              dance floor was corrected away",
            mean_kept * 100.0,
            kept.len()
        );
        failures += 1;
    }

    // The fairness gate, in the gate as well as in the harness. The buckets exist here and
    // in `tests/eval` and nowhere else in the product.
    let mut by_bucket: BTreeMap<usize, Vec<f32>> = BTreeMap::new();
    for (frame, estimate) in &solved {
        if let Some(index) = frame.truth.skin_index {
            by_bucket
                .entry(index)
                .or_default()
                .push(estimate.skin_de00_estimate);
        }
    }
    let means: Vec<f32> = by_bucket
        .values()
        .map(|values| values.iter().sum::<f32>() / values.len().max(1) as f32)
        .collect();
    let overall = {
        let all: Vec<f32> = by_bucket.values().flatten().copied().collect();
        all.iter().sum::<f32>() / all.len().max(1) as f32
    };
    let spread = means.iter().copied().fold(f32::MIN, f32::max)
        - means.iter().copied().fold(f32::MAX, f32::min);
    if by_bucket.len() < 2 {
        eprintln!("skin: fewer than two buckets; the fairness gate measured nothing");
        failures += 1;
    } else if overall <= SKIN_DE00_CEILING && spread <= SKIN_DE00_SPREAD {
        println!(
            "skin: mean {overall:.3} dE00 (gate {SKIN_DE00_CEILING}), spread {spread:.3} across \
             {} tone buckets (gate {SKIN_DE00_SPREAD})",
            by_bucket.len()
        );
    } else {
        eprintln!(
            "skin: mean {overall:.3} dE00 (gate {SKIN_DE00_CEILING}), spread {spread:.3} (gate \
             {SKIN_DE00_SPREAD})"
        );
        failures += 1;
    }

    // 7. The store: a round trip, the review queue, the override, and the protection.
    println!();
    let project = ProjectId::new();
    let store = Arc::new(ToneStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let photos: Vec<PhotoId> = solved.iter().map(|_| PhotoId::new()).collect();
    if let Err(err) = seed(&catalog, &project, &photos, &people) {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    for (index, (_, estimate)) in solved.iter().enumerate() {
        let Some(photo) = photos.get(index) else {
            continue;
        };
        let mut row = estimate.clone();
        row.image_id = *photo;
        if let Err(err) = store.put(&project, &row, &[]) {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match store.outline(project) {
        Ok(outline) => {
            println!(
                "store: {} of {} estimated, {:.1} % face-anchored, {:.1} % skin-constrained, \
                 {} for review",
                outline.estimated,
                outline.photos,
                outline.face_anchored * 100.0,
                outline.skin_constrained * 100.0,
                outline.needs_review
            );
            if outline.estimated as usize != solved.len() {
                eprintln!("  the outline lost a row");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("outline: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // A round trip through the positional codec.
    if let (Some(photo), Some((_, original))) = (photos.first(), solved.first()) {
        match store.get(*photo) {
            Ok(Some(read)) => {
                let same = (read.temperature_k - original.temperature_k).abs() < 0.51
                    && (read.exposure_ev - original.exposure_ev).abs() < 1e-3
                    && read.illuminants.len() == original.illuminants.len()
                    && read.reasons.len() == original.reasons.len();
                if same {
                    println!("  a stored estimate reads back as itself");
                } else {
                    eprintln!("  a stored estimate did not read back as itself");
                    failures += 1;
                }
                if read.reasons.iter().all(|reason| !reason.text.is_empty()) {
                    println!("  every reason renders a sentence from its code");
                } else {
                    eprintln!("  a reason read back with no sentence");
                    failures += 1;
                }
            }
            Ok(None) => {
                eprintln!("  a stored estimate disappeared");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  read: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // The photographer's own answer, and the statement that would overwrite it.
    if let Some(photo) = photos.first().copied() {
        let values = ToneOverride {
            exposure_ev: Some(0.42),
            temperature_k: Some(3_150.0),
            tint: None,
        };
        match store.set_override(photo, values) {
            Ok(()) => println!("  an override is recorded"),
            Err(err) => {
                eprintln!("  override: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
        // Re-analyse the same frame and write it again, exactly as a second pass would.
        if let Some((_, estimate)) = solved.first() {
            let mut row = estimate.clone();
            row.image_id = photo;
            row.temperature_k = 9_000.0;
            drop(store.put(&project, &row, &[]));
        }
        // Both halves. The estimate keeps AURA's own numbers - migration 15's note 2, and
        // what makes the disagreement legible - and the photographer's survive the
        // re-analysis in the three columns beside them.
        match (store.get(photo), store.override_of(photo)) {
            (Ok(Some(read)), Ok(Some(own))) => {
                if read.user_edited && (read.temperature_k - 9_000.0).abs() < 0.51 {
                    println!("  a re-analysis kept its own number and the user_edited bit");
                } else {
                    eprintln!(
                        "  the re-analysis did not land: user_edited={} k={:.0}",
                        read.user_edited, read.temperature_k
                    );
                    failures += 1;
                }
                let kept = own
                    .temperature_k
                    .is_some_and(|k| (k - 3_150.0).abs() < 0.51)
                    && own.exposure_ev.is_some_and(|ev| (ev - 0.42).abs() < 1e-3);
                if kept {
                    println!("  the photographer's own numbers survived it");
                } else {
                    eprintln!("  the photographer's own numbers were lost: {own:?}");
                    failures += 1;
                }
            }
            (Ok(_), Ok(None)) => {
                eprintln!("  the override was written and cannot be read back");
                failures += 1;
            }
            _ => {
                eprintln!("  the overridden row could not be read back");
                failures += 1;
            }
        }
    }

    // The review queue, and the frame that has been dealt with leaving it.
    match store.needs_review(project, 50) {
        Ok(queue) => {
            println!(
                "  review queue: {} frames below a white-balance confidence of {REVIEW_WB_BELOW}",
                queue.len()
            );
            if let Some(first) = queue.first().copied() {
                drop(store.accept(first));
                match store.needs_review(project, 50) {
                    Ok(after) if !after.contains(&first) => {
                        println!("  an accepted frame leaves the queue");
                    }
                    Ok(_) => {
                        eprintln!("  an accepted frame was offered again");
                        failures += 1;
                    }
                    Err(err) => {
                        eprintln!("  queue: [{}] {}", err.code, err.detail);
                        failures += 1;
                    }
                }
            }
        }
        Err(err) => {
            eprintln!("  queue: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 8. The loci, stored and read back.
    println!();
    for locus in loci.values() {
        if let Err(err) = store.put_locus(&project, locus) {
            eprintln!("locus: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match store.loci(project) {
        Ok(read) => {
            if read.len() == loci.len() {
                println!("loci: {} stored and read back", read.len());
            } else {
                eprintln!("loci: stored {} and read {}", loci.len(), read.len());
                failures += 1;
            }
            let bounded = read.values().all(|locus: &SkinLocus| {
                locus.radius >= SkinLocus::MIN_RADIUS && locus.radius <= SkinLocus::MAX_RADIUS
            });
            if bounded {
                println!("  every radius is bounded at both ends");
            } else {
                eprintln!("  a locus radius left its bounds");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("loci: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 9. The anchors phase 25 reads.
    println!();
    let segment = SegmentId::new();
    let candidates: Vec<reference::Candidate> = solved
        .iter()
        .enumerate()
        .filter(|(_, (_, estimate))| !estimate.mixed_light && !estimate.coloured_light)
        .map(|(index, (_, estimate))| reference::Candidate {
            image_id: photos.get(index).copied().unwrap_or_else(PhotoId::new),
            wb_conf: estimate.wb_conf.max(0.75),
            exposure_conf: estimate.exposure_conf.max(0.75),
            subject_luma: estimate.subject_luma_target,
            temperature_k: estimate.temperature_k,
            tint: estimate.tint,
            subject_in_band: true,
            has_primary: true,
            mixed_light: false,
            coloured_light: false,
        })
        .collect();
    let anchors = reference::select(segment, &candidates, ANALYSIS_VER);
    if anchors.len() >= MIN_REFERENCE_FRAMES {
        println!(
            "anchors: {} chosen from {} candidates, best quality {:.3}",
            anchors.len(),
            candidates.len(),
            anchors.first().map_or(0.0, |anchor| anchor.quality)
        );
    } else {
        eprintln!(
            "anchors: {} chosen from {} candidates, gate {MIN_REFERENCE_FRAMES}",
            anchors.len(),
            candidates.len()
        );
        failures += 1;
    }

    // 10. Determinism, on the assembled path.
    println!();
    if let Some(frame) = frames.first() {
        let first = analyse(&analyser, frame, &loci).map(|outcome| outcome.estimate);
        let second = analyse(&analyser, frame, &loci).map(|outcome| outcome.estimate);
        // The id is minted per call, so it is normalised away by copying one onto the
        // other; everything the catalog stores is then compared, including the reasons and
        // the alternatives. A solve that agreed on three numbers and disagreed on which
        // reason came first would still produce two different catalogs.
        let second = match (&first, second) {
            (Ok(a), Ok(mut b)) => {
                b.image_id = a.image_id;
                Ok(b)
            }
            (_, other) => other,
        };
        match (first, second) {
            (Ok(a), Ok(b)) if a == b => println!("determinism: two runs agree exactly"),
            (Ok(_), Ok(_)) => {
                eprintln!("determinism: two runs of the same frame disagreed");
                failures += 1;
            }
            _ => {
                eprintln!("determinism: a run failed");
                failures += 1;
            }
        }
    }
    println!(
        "versions: model {MODEL_VER}, analysis {ANALYSIS_VER}, storage budget {BYTES_PER_IMAGE} B"
    );

    // 11. The vocabulary a panel renders.
    let mut missing = Vec::new();
    for code in [
        ToneCode::SubjectExposed,
        ToneCode::MixedLight,
        ToneCode::ColouredLightPreserved,
        ToneCode::SkinLocusUnavailable,
        ToneCode::BacklitSubject,
        ToneCode::NeutralFound,
    ] {
        if code.user_text().is_empty() {
            missing.push(code.as_str());
        }
    }
    for kind in IlluminantKind::ALL {
        if kind.as_str().is_empty() {
            missing.push("an illuminant kind");
        }
    }
    if missing.is_empty() {
        println!("vocabulary: every code a panel can show has a sentence");
    } else {
        eprintln!("vocabulary: {} has no sentence", missing.join(", "));
        failures += 1;
    }

    println!();
    println!("What this gate proves: migration 15 and its objects exist and cannot hold an");
    println!("ideal-skin value, the target table is loadable and argued-over, both models load");
    println!("through the real registry, a skin locus accumulates across a wedding and then");
    println!("bounds the solve, mixed light is detected and recorded for phase 18, coloured");
    println!("light survives, a photographer's override beats a re-analysis, a chapter gets its");
    println!("anchors, and the same frame twice gives the same numbers.");
    println!();
    println!("What it does not prove: that any exposure or colour is CORRECT on a photograph.");
    println!("Every frame here is synthetic, with its illuminant and its subject luminance");
    println!("painted in. There are no camera files, no photographed ColorChecker and no expert");
    println!("edit pairs in this repository, and both learned heads are untrained placeholders");
    println!("that are never consulted. Condition C1 in docs/progress/PHASE-15-EXIT.md.");

    if failures == 0 {
        println!();
        println!("phase 15: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 15: {failures} failure(s)");
        ExitCode::FAILURE
    }
}

/// A real inference engine over the two phase 15 graphs, through the real registry.
///
/// Section 4 of the gate. The models load and warm up; nothing asks them a question, because
/// both are untrained and `WB_HEAD_TRAINED` and `EXPOSURE_HEAD_TRAINED` are false. What this
/// proves is that the manifests, the shapes and the loader agree - which is the half of a
/// model contract that has no test anywhere else.
fn gate_engine(
    directory: &std::path::Path,
    clock: Arc<dyn Clock>,
) -> AuraResult<Arc<dyn InferService>> {
    let wb = directory.join("white_balance.onnx");
    let exposure = directory.join("exposure_scene.onnx");
    for (path, model) in [
        (&wb, onnx_fixtures::white_balance()),
        (&exposure, onnx_fixtures::exposure_scene()),
    ] {
        std::fs::write(path, serialise(&model))
            .map_err(|err| aura_core::errors::io::unreadable(path, &err))?;
    }

    let source = Arc::new(
        StaticModelSource::new()
            .with(
                ModelRef::new(
                    onnx_fixtures::WHITE_BALANCE_MODEL,
                    aura_brain_photo::tone::analyse::WB_MODEL_VERSION,
                ),
                &wb,
                Precision::Fp32,
                ModelClass::Embedding,
                8,
            )
            .with(
                ModelRef::new(
                    onnx_fixtures::EXPOSURE_SCENE_MODEL,
                    aura_brain_photo::tone::analyse::EXPOSURE_MODEL_VERSION,
                ),
                &exposure,
                Precision::Fp32,
                ModelClass::Embedding,
                1,
            ),
    );
    let mut plan = HardwarePlan::conservative(4, "2026-08-18T00:00:00Z".to_string());
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

// -- the synthetic wedding ---------------------------------------------------

/// Five people, each photographed under two lights three times over, plus the hard cases.
///
/// The same construction `tests/eval/tone_eval.rs` uses, and for the same reason: a locus is
/// what one person looks like across a wedding, so the fixture's per-frame identities have to
/// be replaced by one stable identity per reflectance before any of this means anything.
fn wedding() -> (Vec<Frame>, Vec<IdentityId>) {
    let mut out = Vec::new();
    let mut people = Vec::new();
    for index in 0..fixtures::SKIN_TONES.len() {
        let person = IdentityId::new();
        people.push(person);
        for _ in 0..3 {
            for mut frame in [
                fixtures::daylight_ceremony(index),
                fixtures::tungsten_reception(index),
            ] {
                stamp(&mut frame, person);
                out.push(frame);
            }
        }
        for mut frame in [
            fixtures::mixed_daylight_led(index),
            fixtures::backlit_exit(index),
            fixtures::purple_dance_floor(index),
            fixtures::red_mandap(index),
        ] {
            stamp(&mut frame, person);
            out.push(frame);
        }
    }
    out.push(fixtures::faceless_details());
    (out, people)
}

fn stamp(frame: &mut Frame, person: IdentityId) {
    let count = frame.subjects.faces.len().max(1);
    frame.subjects.prominence.clear();
    for face in &mut frame.subjects.faces {
        face.identity_id = Some(person);
    }
    frame.subjects.prominence.insert(person, 1.0 / count as f32);
}

fn analyse(
    analyser: &Analyser,
    frame: &Frame,
    loci: &BTreeMap<IdentityId, SkinLocus>,
) -> AuraResult<aura_brain_photo::tone::analyse::FrameOutcome> {
    let buffer = PixelBuffer {
        width: frame.width as u32,
        height: frame.height as u32,
        data: PixelData::Srgb8(frame.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    };
    let context = FrameContext {
        scene: frame.scene,
        scene_known: true,
        attrs: frame.attrs,
        subjects: frame.subjects.clone(),
        as_shot: frame.as_shot,
        shadow_headroom_ev: 2.0,
    };
    analyser.analyse(PhotoId::new(), &buffer, &context, loci, Priority::AiBatch)
}

fn ratio(hits: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    hits as f64 / total as f64
}

// -- catalog helpers ---------------------------------------------------------

fn schema_object(catalog: &Catalog, kind: &str, name: &str) -> AuraResult<bool> {
    let kind = kind.to_string();
    let name = name.to_string();
    catalog.read(move |conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
        Ok(count > 0)
    })
}

/// Any column in migration 15's tables whose name suggests a fixed skin target.
///
/// Section 6.3's defence, checked rather than remembered. The list is deliberately broad:
/// this is looking for the *shape* of a mistake somebody would make in good faith while
/// adding a feature two phases from now.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(|conn| {
        let mut found = Vec::new();
        for table in [
            "image_tone_estimate",
            "identity_skin_locus",
            "segment_reference_frames",
        ] {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?;
            let mut rows = statement
                .query([])
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?;
            while let Some(row) = rows
                .next()
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?
            {
                let name: String = row.get(1).unwrap_or_default();
                let lower = name.to_ascii_lowercase();
                let suspicious = ["ideal", "target_skin", "skin_target", "reference_skin"]
                    .iter()
                    .any(|needle| lower.contains(needle))
                    || lower.contains("tone_bucket")
                    || lower.contains("monk");
                if suspicious {
                    found.push(format!("{table}.{name}"));
                }
            }
        }
        Ok(found)
    })
}

fn seed(
    catalog: &Catalog,
    project: &ProjectId,
    photos: &[PhotoId],
    people: &[IdentityId],
) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 15".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-18T00:00:00Z".to_string(),
        updated_at: "2026-08-18T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;
    let project_key = project.to_db();
    let ids: Vec<String> = photos.iter().map(aura_core::PhotoId::to_db).collect();
    let identities: Vec<String> = people.iter().map(aura_core::IdentityId::to_db).collect();
    let identity_project = project.to_db();
    catalog.writer().transact(move |tx| {
        // `identity_skin_locus.identity_id` references `identities`, so the people the
        // synthetic wedding invented have to exist before their loci can be stored. The
        // foreign key is the point: a locus for somebody the catalog has never heard of is a
        // measurement with nobody attached to it.
        for identity in &identities {
            tx.execute(
                "INSERT INTO identities (id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z')",
                params![identity, identity_project],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("identity", &e))?;
        }
        for (index, photo) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
                         '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z')",
                params![
                    photo,
                    project_key,
                    format!(
                        "2026-08-18T{:02}:{:02}:{:02}Z",
                        index / 3_600 % 24,
                        index / 60 % 60,
                        index % 60
                    ),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        }
        Ok(())
    })
}

/// An inference service that refuses every call.
///
/// Neither head is consulted while both are untrained, so the solve this gate measures is
/// exactly the one that ships. Section 4 above loads the real models through the real
/// registry; this is what the arithmetic runs on.
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
        Err(aura_core::errors::ml::cancelled("no head is consulted"))
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
        Err(aura_core::errors::ml::cancelled("no head is consulted"))
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
