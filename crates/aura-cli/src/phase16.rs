//! The phase 16 mechanical gate.
//!
//! This is the assembly proof for tone, curves, HSL and skin protection: migration 16 and its
//! objects, the intent table, the content classifier, the curve fitter under all three of
//! section 6.1's constraints, the harmony objective, the two guards, the skin guarantee
//! measured through the real renderer, the store and its override protection, and the one real
//! model loaded through the real registry.
//!
//! **Nothing here proves a photograph looks good.** There are no camera files and no expert
//! edits in this repository - phase 02's exit conditions and this phase's condition C1, both
//! open - so every number below is measured against synthetic frames whose foliage hue, dress
//! luminance and subject spread were chosen and painted in. The distinction is printed at the
//! end of every run rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/colour_eval.rs` is the
//! other half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_photo::colour::analyse::{Analyser, FrameContext, ANALYSIS_VER, MODEL_VER};
use aura_brain_photo::colour::fixtures::{self, Frame};
use aura_brain_photo::colour::intent::IntentTable;
use aura_brain_photo::colour::store::ColourStore;
use aura_brain_photo::colour::{clip_guard, curve, skin_guard, BYTES_PER_IMAGE};
use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::colour::{
    ColourCode, ColourDecision, ColourOverride, ContentBand, HslBand, VariantKind,
    SKIN_CHROMA_CEILING, SKIN_HUE_CEILING_DEG, SUBTLETY_CEILING,
};
use aura_core::{AuraResult, PhotoId, Priority, ProjectId, SceneId};
use aura_infer::contract::infer::{
    BatchDefaults, HardwarePlan, InferService, ModelClass, ModelRef, Precision,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures as onnx_fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use rusqlite::params;

/// Run the phase 16 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase16-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 16 and every object it owns.
    let catalog_path = work.join("phase16.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 16 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 16, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "image_colour_decision"),
        ("view", "v_colour_coverage"),
        ("index", "idx_colour_project"),
        ("index", "idx_colour_review"),
        ("index", "idx_colour_guard"),
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

    // 2. There is nowhere in this schema, and nowhere in the intent table, to put an ideal
    //    skin value.
    //
    //    Section 6.3's fairness argument is that a fixed target is how an editor lightens dark
    //    skin while believing it is correcting a cast, and the defence is structural rather
    //    than remembered. Phase 15's gate checks its own schema; this checks phase 16's, and
    //    the config file as well.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no ideal-skin column anywhere in migration 16");
        }
        Ok(found) => {
            eprintln!(
                "  migration 16 grew a skin target column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  forbidden column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match forbidden_config() {
        Ok(found) if found.is_empty() => println!("  no ideal-skin row in tone_intent.toml"),
        Ok(found) => {
            eprintln!(
                "  tone_intent.toml grew a skin target: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(detail) => {
            eprintln!("  config scan: {detail}");
            failures += 1;
        }
    }

    // 3. The intent table, loaded and argued over.
    let intents = match IntentTable::embedded() {
        Ok(table) => {
            println!(
                "intents: version {}, {} scene rows, {} untargeted",
                table.version(),
                table.rows(),
                table.untargeted().len()
            );
            if !table.untargeted().is_empty() {
                eprintln!("  every scene in the taxonomy needs an intent row");
                failures += 1;
            }
            let mut over = Vec::new();
            for scene in SceneId::ALL {
                if table.get(scene).subtlety_cap > SUBTLETY_CEILING {
                    over.push(scene.as_str());
                }
            }
            if over.is_empty() {
                println!("  no scene asks to grade past the subtlety ceiling");
            } else {
                eprintln!("  scenes above the ceiling: {}", over.join(", "));
                failures += 1;
            }
            Some(table)
        }
        Err(err) => {
            eprintln!("intents: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };

    // 4. The one real model, through the real registry.
    match gate_engine(&work, Arc::clone(&clock)) {
        Ok(engine) => {
            let model = ModelRef::new(
                onnx_fixtures::TONE_MODEL,
                aura_brain_photo::colour::analyse::TONE_MODEL_VERSION,
            );
            match engine.warmup(&[model]) {
                Ok(()) => println!("models: tone_model loaded and warmed through the registry"),
                Err(err) => {
                    eprintln!("models: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
            if aura_brain_photo::colour::TONE_HEAD_TRAINED {
                eprintln!(
                    "models: TONE_HEAD_TRAINED is true but no training has been recorded; \
                     close condition C1 before flipping it"
                );
                failures += 1;
            } else {
                println!("  the head is untrained and is never consulted (condition C1)");
            }
        }
        Err(err) => {
            eprintln!("models: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. One frame in, one grade out, through the assembled analyser.
    let analyser = match Analyser::new(Arc::new(NullInfer::new())) {
        Ok(analyser) => analyser,
        Err(err) => {
            eprintln!("analyser: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let cases = fixtures::every_case();
    println!("fixtures: {} synthetic frames", cases.len());

    let mut decisions: Vec<(String, ColourDecision)> = Vec::new();
    for frame in &cases {
        match analyser.analyse(
            PhotoId::new(),
            &buffer_of(frame),
            &context_of(frame),
            Priority::AiBatch,
        ) {
            Ok(outcome) => decisions.push((frame.name.to_string(), outcome.decision)),
            Err(err) => {
                eprintln!("  {}: [{}] {}", frame.name, err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 6. The guarantee, on every frame and every skin tone.
    let mut worst_hue = 0.0_f32;
    let mut worst_chroma = 0.0_f32;
    let mut measured = 0usize;
    let mut withdrew = 0usize;
    for (name, decision) in &decisions {
        if decision.skin_measured {
            measured += 1;
            worst_hue = worst_hue.max(decision.skin_guard.max_hue_shift_deg);
            worst_chroma = worst_chroma.max(decision.skin_guard.max_chroma_change);
        }
        if decision
            .reasons
            .iter()
            .any(|reason| reason.code == ColourCode::SkinGuardWithdrew)
        {
            withdrew += 1;
        }
        if !decision.skin_within_ceilings() {
            eprintln!(
                "  {name}: skin moved {:.2} deg and {:.1} % after grading",
                decision.skin_guard.max_hue_shift_deg,
                decision.skin_guard.max_chroma_change * 100.0
            );
            failures += 1;
        }
    }
    println!(
        "skin: {measured} of {} frames measured, worst {worst_hue:.2} deg (ceiling \
         {SKIN_HUE_CEILING_DEG}) and {:.1} % chroma (ceiling {:.0} %), {withdrew} withdrawn",
        decisions.len(),
        worst_chroma * 100.0,
        SKIN_CHROMA_CEILING * 100.0
    );

    // 7. Every curve monotone, through the renderer's own interpolation.
    let mut worst_run = 0usize;
    for (name, decision) in &decisions {
        let mut previous = f32::NEG_INFINITY;
        let mut run = 0usize;
        let mut last = f32::NAN;
        for step in 0..=4096 {
            let x = step as f32 / 4096.0;
            let y = curve::evaluate(&decision.curve, x);
            if y < previous - 1e-5 {
                eprintln!("  {name}: the curve goes backwards at x = {x}");
                failures += 1;
                break;
            }
            previous = y;
            if (y - last).abs() < 1e-6 {
                run += 1;
                worst_run = worst_run.max(run);
            } else {
                run = 0;
            }
            last = y;
        }
    }
    if worst_run > 12 {
        eprintln!("  a curve produced a flat band {worst_run} steps long");
        failures += 1;
    } else {
        println!("curves: every fitted curve monotone, longest flat run {worst_run} of 4,096");
    }

    // 8. The clipping guard, against each scene's own tolerance.
    if let Some(table) = &intents {
        let mut outside = Vec::new();
        for (name, decision) in &decisions {
            let tolerance =
                aura_brain_photo::colour::intent::clip_tolerance(&table.get(decision.scene));
            if decision.highlight_clipping_added() > tolerance + 1e-4 {
                outside.push(name.clone());
            }
        }
        if outside.is_empty() {
            println!("clipping: no frame gained clipping beyond its scene tolerance");
        } else {
            eprintln!(
                "clipping: {} frame(s) outside: {}",
                outside.len(),
                outside.join(", ")
            );
            failures += 1;
        }

        // 9. The subtlety cap, per scene.
        let mut over = Vec::new();
        for (name, decision) in &decisions {
            if decision.subtlety > table.get(decision.scene).subtlety_cap + 1e-3 {
                over.push(name.clone());
            }
        }
        if over.is_empty() {
            println!("subtlety: every grade inside its scene's cap");
        } else {
            eprintln!(
                "subtlety: {} frame(s) over: {}",
                over.len(),
                over.join(", ")
            );
            failures += 1;
        }
    }

    // 10. The content classifier and the harmony objective, on the two frames that name them.
    let outdoor = fixtures::outdoor_portrait(1);
    match analyser.analyse(
        PhotoId::new(),
        &buffer_of(&outdoor),
        &context_of(&outdoor),
        Priority::AiBatch,
    ) {
        Ok(outcome) => {
            let found = outcome
                .reading
                .band(ContentBand::Greenery)
                .map_or(0.0, |band| band.area);
            let shift = outcome.decision.hsl.get(HslBand::of_hue(
                outdoor.truth.greenery_hue_deg.unwrap_or(95.0),
            ));
            if found > 0.20 && shift.h > 0.0 && shift.s < 0.0 {
                println!(
                    "content: foliage found over {:.0} % of the frame and pulled {:+.1} in hue, \
                     {:+.1} in saturation",
                    found * 100.0,
                    shift.h,
                    shift.s
                );
            } else {
                eprintln!("content: the greenery was not found or was not adjusted");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("content: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let dance = fixtures::purple_dance_floor(2);
    match analyser.analyse(
        PhotoId::new(),
        &buffer_of(&dance),
        &context_of(&dance),
        Priority::AiBatch,
    ) {
        Ok(outcome) => {
            let tamed = outcome
                .decision
                .reasons
                .iter()
                .any(|reason| reason.code == ColourCode::DistractorTamed);
            if tamed {
                eprintln!("harmony: a dance floor's own colour was treated as a distraction");
                failures += 1;
            } else {
                println!("harmony: a purple dance floor keeps its purple");
            }
        }
        Err(err) => {
            eprintln!("harmony: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 11. The store: an upsert that carries a photographer's answer forward, and the budget.
    let store = Arc::new(ColourStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let project = ProjectId::new();
    let photo = PhotoId::new();
    match seed(&catalog, &project, &photo) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("seed: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    }
    if let Some((_, first)) = decisions.first() {
        let mut row = first.clone();
        row.image_id = photo;
        match store.put(&project, &row, &[]) {
            Ok(()) => println!("store: one decision written"),
            Err(err) => {
                eprintln!("store: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }

        let mine = ColourOverride {
            contrast: Some(-11.0),
            ..ColourOverride::default()
        };
        if let Err(err) = store.set_override(photo, &mine) {
            eprintln!("store: override refused: [{}] {}", err.code, err.detail);
            failures += 1;
        }
        // The re-analysis, which must not overwrite what the photographer decided.
        if let Err(err) = store.put(&project, &row, &[]) {
            eprintln!("store: re-analysis failed: [{}] {}", err.code, err.detail);
            failures += 1;
        }
        match store.override_of(photo) {
            Ok(Some(kept)) if kept.contrast == Some(-11.0) => {
                println!("  a photographer's contrast survived a re-analysis");
            }
            Ok(_) => {
                eprintln!("  a re-analysis overwrote a photographer's own value");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  override read: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
        match store.get(photo) {
            Ok(Some(read)) => {
                if read.user_edited {
                    println!("  the row still carries AURA's own numbers beside the person's");
                } else {
                    eprintln!("  the override did not mark the row");
                    failures += 1;
                }
                if read.curve == row.curve && read.hsl == row.hsl {
                    println!("  the curve and the eight bands round-tripped exactly");
                } else {
                    eprintln!("  the curve or the bands changed on the way through the catalog");
                    failures += 1;
                }
            }
            Ok(None) => {
                eprintln!("  the stored decision disappeared");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  decision read: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }

        match store.bytes_for(&project) {
            Ok((rows, bytes)) if rows > 0 => {
                let per = bytes / rows;
                if per <= BYTES_PER_IMAGE as u64 {
                    println!("storage: {per} B per image against a {BYTES_PER_IMAGE} B budget");
                } else {
                    eprintln!("storage: {per} B per image, over the {BYTES_PER_IMAGE} B budget");
                    failures += 1;
                }
            }
            Ok(_) => {
                eprintln!("storage: nothing was measured");
                failures += 1;
            }
            Err(err) => {
                eprintln!("storage: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }

        match store.outline(project) {
            Ok(outline) => println!(
                "outline: {} of {} graded, worst skin shift {:.2} deg, guarantee {}",
                outline.decided,
                outline.photos,
                outline.worst_skin_hue_shift,
                if outline.guarantee_held() {
                    "held"
                } else {
                    "BROKEN"
                }
            ),
            Err(err) => {
                eprintln!("outline: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 12. Determinism, on the assembled path.
    if let Some(frame) = cases.first() {
        let first = analyser.analyse(
            PhotoId::new(),
            &buffer_of(frame),
            &context_of(frame),
            Priority::AiBatch,
        );
        let second = analyser.analyse(
            PhotoId::new(),
            &buffer_of(frame),
            &context_of(frame),
            Priority::AiBatch,
        );
        match (first, second) {
            (Ok(a), Ok(b)) => {
                let same = a.decision.contrast == b.decision.contrast
                    && a.decision.curve == b.decision.curve
                    && a.decision.hsl == b.decision.hsl
                    && a.decision.skin_guard == b.decision.skin_guard;
                if same {
                    println!("determinism: two runs agree exactly");
                } else {
                    eprintln!("determinism: two runs of the same frame disagreed");
                    failures += 1;
                }
            }
            _ => {
                eprintln!("determinism: a run failed");
                failures += 1;
            }
        }
    }
    println!(
        "versions: model {MODEL_VER}, analysis {ANALYSIS_VER}, storage budget \
         {BYTES_PER_IMAGE} B"
    );

    // 13. The vocabulary a panel renders.
    let mut missing = Vec::new();
    for code in ColourCode::ALL {
        if code.user_text().is_empty() {
            missing.push(code.as_str());
        }
    }
    for kind in VariantKind::ALL {
        if kind.user_text().is_empty() {
            missing.push("a variant kind");
        }
    }
    for band in ContentBand::ALL {
        if band.as_str().is_empty() {
            missing.push("a content band");
        }
    }
    if missing.is_empty() {
        println!(
            "vocabulary: every one of the {} codes a panel can show has a sentence",
            ColourCode::COUNT
        );
    } else {
        eprintln!("vocabulary: {} has no sentence", missing.join(", "));
        failures += 1;
    }

    // 14. The guards only ever reduce, which is what lets them run in this order.
    if let Some((_, decision)) = decisions.first() {
        let identity = skin_guard::Grade {
            tone: aura_brain_photo::colour::ToneParams::default(),
            curve: aura_core::contract::colour::ToneCurve::identity(),
            colour: aura_brain_photo::colour::hsl::Solved::default(),
        };
        if clip_guard::subtlety(&identity) <= decision.subtlety + 1e-6 {
            println!("guards: an ungraded frame is the subtlest a frame can be");
        } else {
            eprintln!("guards: the subtlety metric is not zero at the identity");
            failures += 1;
        }
    }

    println!();
    println!("What this gate proves: migration 16 and its objects exist and cannot hold an");
    println!("ideal-skin value, the intent table is loadable and argued-over, the tone model");
    println!("loads through the real registry and is never consulted, every fitted curve is");
    println!("monotone through the renderer's own interpolation, the greenery is found and");
    println!("tamed, a purple dance floor keeps its purple, no frame gains clipping past its");
    println!("scene's tolerance, no grade passes its scene's subtlety cap, skin stays inside");
    println!("both ceilings on every skin tone, a photographer's value beats a re-analysis,");
    println!("and the same frame twice gives the same grade.");
    println!();
    println!("What it does not prove: that any grade LOOKS right on a photograph. Every frame");
    println!("here is synthetic, with its foliage hue, dress luminance and subject contrast");
    println!("painted in. There are no camera files and no expert edits in this repository,");
    println!("and the learned head is an untrained placeholder that is never consulted.");
    println!("Condition C1 in docs/progress/PHASE-16-EXIT.md. The skin fairness measurement is");
    println!("five reflectances rather than five people - condition C2, and");
    println!("docs/skin-fairness.md says so in the product's own words.");

    if failures == 0 {
        println!();
        println!("phase 16: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 16: {failures} failure(s)");
        ExitCode::FAILURE
    }
}

/// A real inference engine over the phase 16 graph, through the real registry.
///
/// Section 4 of the gate. The model loads and warms up; nothing asks it a question, because it
/// is untrained and `TONE_HEAD_TRAINED` is false. What this proves is that the manifest, the
/// shape and the loader agree - which is the half of a model contract that has no test
/// anywhere else.
fn gate_engine(
    directory: &std::path::Path,
    clock: Arc<dyn Clock>,
) -> AuraResult<Arc<dyn InferService>> {
    let path = directory.join("tone_model.onnx");
    std::fs::write(&path, serialise(&onnx_fixtures::tone_model()))
        .map_err(|err| aura_core::errors::io::unreadable(&path, &err))?;

    let source = Arc::new(StaticModelSource::new().with(
        ModelRef::new(
            onnx_fixtures::TONE_MODEL,
            aura_brain_photo::colour::analyse::TONE_MODEL_VERSION,
        ),
        &path,
        Precision::Fp32,
        ModelClass::Embedding,
        8,
    ));
    let mut plan = HardwarePlan::conservative(4, "2026-08-19T00:00:00Z".to_string());
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

/// Whether one schema object exists.
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
            .map_err(|e| {
                aura_core::errors::db::statement_failed("could not read sqlite_master", &e)
            })?;
        Ok(count > 0)
    })
}

/// Any column in migration 16 that looks like an ideal-skin target.
///
/// The structural half of section 6.3's fairness argument, checked rather than remembered.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(move |conn| {
        let mut statement = conn
            .prepare("PRAGMA table_info(image_colour_decision)")
            .map_err(|e| {
                aura_core::errors::db::statement_failed("could not read the table info", &e)
            })?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| {
                aura_core::errors::db::statement_failed("could not read the column names", &e)
            })?;
        let mut found = Vec::new();
        for row in rows.flatten() {
            let lower = row.to_ascii_lowercase();
            for needle in [
                "skin_target",
                "ideal_skin",
                "skin_hue_target",
                "skin_bucket",
                "skin_tone",
                "monk",
                "fitzpatrick",
            ] {
                if lower.contains(needle) {
                    found.push(row.clone());
                }
            }
        }
        Ok(found)
    })
}

/// Any row in the shipped intent table that looks like an ideal-skin target.
fn forbidden_config() -> Result<Vec<String>, String> {
    let path = std::path::Path::new("crates/aura-brain-photo/config/tone_intent.toml");
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("{}: {err}", path.display()))?
        .to_ascii_lowercase();
    let mut found = Vec::new();
    for needle in [
        "skin_hue",
        "skin_saturation",
        "skin_luma",
        "skin_target",
        "ideal_skin",
        "skin_tone",
        "monk",
        "fitzpatrick",
    ] {
        if text.contains(needle) {
            found.push(needle.to_string());
        }
    }
    Ok(found)
}

/// A project and one photograph, so the store has something to write against.
fn seed(catalog: &Catalog, project: &ProjectId, photo: &PhotoId) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 16".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-19T00:00:00Z".to_string(),
        updated_at: "2026-08-19T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;

    let project_key = project.to_db();
    let photo_key = photo.to_db();
    catalog.writer().transact(move |tx| {
        tx.execute(
            "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                camera_make, camera_model, iso, created_at, updated_at)
             VALUES (?1, ?2, '2026-08-19T12:00:00Z', '2026-08-19T12:00:00Z',
                     'SONY', 'ILCE-7M3', 800, '2026-08-19T00:00:00Z',
                     '2026-08-19T00:00:00Z')",
            params![photo_key, project_key],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        Ok(())
    })
}

fn buffer_of(frame: &Frame) -> PixelBuffer {
    PixelBuffer {
        width: frame.width as u32,
        height: frame.height as u32,
        data: PixelData::Srgb8(frame.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_of(frame: &Frame) -> FrameContext {
    FrameContext {
        scene: frame.scene,
        scene_known: true,
        attrs: frame.attrs,
        subjects: frame.subjects.clone(),
        shadow_headroom_ev: 2.0,
    }
}

/// An inference service that refuses every call.
///
/// Correct rather than a shortcut: `TONE_HEAD_TRAINED` is false, so nothing in this phase asks
/// the head a question. Section 4 above loads the real model through the real registry, which
/// is the part that has no test anywhere else.
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
        _req: aura_infer::contract::infer::InferRequest<'_>,
    ) -> Result<aura_infer::contract::infer::InferResult, aura_infer::contract::infer::InferError>
    {
        Err(aura_core::errors::ml::cancelled(
            "the null inference service is never called",
        ))
    }

    fn run_batch(
        &self,
        _model: ModelRef,
        _inputs: Vec<Vec<aura_infer::contract::infer::TensorView<'_>>>,
        _prio: Priority,
    ) -> Result<
        Vec<aura_infer::contract::infer::InferResult>,
        aura_infer::contract::infer::InferError,
    > {
        Err(aura_core::errors::ml::cancelled(
            "the null inference service is never called",
        ))
    }

    fn plan(&self) -> &HardwarePlan {
        &self.plan
    }

    fn warmup(&self, _models: &[ModelRef]) -> Result<(), aura_infer::contract::infer::InferError> {
        Ok(())
    }
}
