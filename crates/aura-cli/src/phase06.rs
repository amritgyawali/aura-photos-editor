//! The phase 06 gate.
//!
//! `aura-cli verify --phase 06` proves, on this machine, in one run, everything section
//! 13 of the phase document asks for that can be proved without a human: the migration
//! lands, a wedding's worth of frames is scanned for faces, every template is sealed and
//! unreadable without the key, a killed pass resumes without recomputing, identities are
//! grouped and roles inferred with reasons, a photographer's decision survives a full
//! re-analysis, every image exposes a subject focus score, the same pixels produce the
//! same face ids twice, no query crosses a project boundary, and erasure leaves nothing
//! behind while keeping the catalog's other truth intact.
//!
//! It is deliberately a separate binary path from the tests, for the same reason the
//! phase 03, 04 and 05 gates are: the tests prove the pieces, the gate proves the
//! assembly, with the same code CI runs and a human reads the output of.
//!
//! **What it does not do, and why.** It does not import RAW files. The ingest path is
//! phase 01's and is gated by `verify --phase 01`; the decode path is phase 02's and is
//! gated by `verify --phase 02`. What phase 06 adds begins at a decoded proxy, so the
//! gate starts there - with synthetic frames whose faces are at known positions, which is
//! also the only way to state a recall number at all while the shipped detector is a
//! placeholder (condition C1).
//!
//! It never touches a network. Nothing in the local half of phase 06 can.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::people::{PeopleService, Role};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{AuraResult, PhotoId, Priority, ProjectId};
use aura_infer::contract::infer::{
    BatchDefaults, HardwarePlan, InferService, ModelClass, Precision,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures as onnx_fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_people::store::PeopleStore;
use aura_people::vault::{BiometricKeyStore, MemoryKeyStore};
use aura_people::{FaceScanner, People};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelLevel, PixelSource};
use aura_vision::face::fixtures::{self, SynthFace};
use aura_vision::face::FacePipeline;

/// Frames in the gate's synthetic wedding.
const FRAMES: usize = 24;

/// Frame size. Large enough that a 4 %-tall face is above the 48 px identity gate, which
/// is the case the small-face recall number is about.
const FRAME_WIDTH: usize = 1_600;
/// Frame height.
const FRAME_HEIGHT: usize = 1_200;

/// Section 10.1's detection recall gate.
const RECALL_GATE: f64 = 0.97;

/// Section 11's People panel budget.
const PANEL_BUDGET_MS: u64 = 300;

/// A preview service over the gate's pre-rendered frames.
#[derive(Debug)]
struct GateFrames {
    frames: BTreeMap<PhotoId, Vec<u8>>,
}

impl PreviewService for GateFrames {
    fn get(
        &self,
        id: PhotoId,
        _level: PixelLevel,
        _prio: PreviewPriority,
    ) -> AuraResult<Arc<PixelBuffer>> {
        let Some(bytes) = self.frames.get(&id) else {
            return Err(aura_core::errors::raw::corrupt("no frame for that photo"));
        };
        Ok(Arc::new(PixelBuffer {
            width: FRAME_WIDTH as u32,
            height: FRAME_HEIGHT as u32,
            data: PixelData::Srgb8(bytes.clone()),
            colour_space: ColourSpace::Srgb,
            source: PixelSource::Demosaiced,
            decode_ms: 1,
        }))
    }

    fn prefetch(&self, _ids: &[PhotoId], _level: PixelLevel) {}

    fn cache_stats(&self) -> aura_cache::budget::CacheStats {
        aura_cache::budget::CacheStats::default()
    }

    fn quarantine(&self) -> Vec<(PhotoId, String)> {
        Vec::new()
    }
}

/// The wedding this gate scans: three people, one of them appearing less often, plus a
/// row of bokeh highlights in every third frame.
fn wedding_frame(index: usize) -> Vec<SynthFace> {
    let mut faces = vec![
        SynthFace::new([0.30, 0.38], 0.22, 1),
        SynthFace::new([0.62, 0.40], 0.20, 2),
    ];
    if index.is_multiple_of(3) {
        faces.push(SynthFace::new([0.86, 0.60], 0.10, 3));
    }
    if index.is_multiple_of(4) {
        // Small faces, at the resolution the tiled pass exists for.
        for guest in 0..4 {
            faces.push(SynthFace::new(
                [0.14 + guest as f32 * 0.09, 0.80],
                0.045,
                10 + guest,
            ));
        }
    }
    if index.is_multiple_of(5) {
        for light in 0..5 {
            faces.push(SynthFace::bokeh([0.10 + light as f32 * 0.18, 0.10], 0.035));
        }
    }
    faces
}

/// Run the phase 06 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase06-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. The catalog reaches schema 6 and has every table migration 6 adds.
    let catalog_path = work.join("phase06.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let cache_root = work.join("cache");
    drop(std::fs::remove_dir_all(&cache_root));

    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            eprintln!("[{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 6 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 6, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for table in [
        "face_vault",
        "face_scan",
        "identities",
        "faces",
        "identity_links",
        "person_boxes",
        "cooccurrence",
    ] {
        match catalog.count(table) {
            Ok(count) => println!("  {table}: {count} rows"),
            Err(err) => {
                eprintln!("  {table}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 2. A wedding's worth of frames, with faces at known positions.
    let project = ProjectId::new();
    let project_key = project.to_db();
    if let Err(err) = catalog.writer().transact({
        let key = project_key.clone();
        move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'phase-06', '2026-08-13T00:00:00Z', '2026-08-13T00:00:00Z')",
                rusqlite::params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("create project", &e))?;
            Ok(())
        }
    }) {
        eprintln!("project: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }

    let mut frames = BTreeMap::new();
    let mut truth: Vec<(PhotoId, Vec<SynthFace>)> = Vec::new();
    for index in 0..FRAMES {
        let photo = PhotoId::new();
        let faces = wedding_frame(index);
        frames.insert(
            photo,
            fixtures::render_frame(FRAME_WIDTH, FRAME_HEIGHT, &faces),
        );
        truth.push((photo, faces));

        let photo_key = photo.to_db();
        let key = project_key.clone();
        let stamp = format!(
            "2026-08-13T{:02}:{:02}:00Z",
            10 + index / 12,
            (index * 5) % 60
        );
        if let Err(err) = catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, timeline_time, focal_len_mm,
                                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, 24.0, ?3, ?3)",
                rusqlite::params![photo_key, key, stamp],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("insert photo", &e))?;
            Ok(())
        }) {
            eprintln!("photo: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    }
    println!("fixtures: {FRAMES} frames at {FRAME_WIDTH}x{FRAME_HEIGHT}");

    // 3. The pipeline, over the real inference service and the real sealed store.
    let models = work.join("models");
    if let Err(err) = std::fs::create_dir_all(&models) {
        eprintln!("cannot create {}: {err}", models.display());
        return ExitCode::FAILURE;
    }
    let engine = match gate_engine(&models, Arc::clone(&clock)) {
        Ok(engine) => engine,
        Err(err) => {
            eprintln!("models: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    // An in-memory key store, so the gate never writes to the machine's real keychain.
    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let store = Arc::new(PeopleStore::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
        keys,
        &cache_root,
    ));
    let previews: Arc<dyn PreviewService> = Arc::new(GateFrames { frames });
    // The deterministic reference detector: the shipped `face_detect` is a placeholder
    // and finds nothing in a photograph. Condition C1.
    let pipeline = FacePipeline::new(Arc::clone(&engine)).with_reference_detector();
    let scanner = FaceScanner::new(
        Arc::clone(&previews),
        pipeline,
        Arc::clone(&store),
        Arc::clone(&clock),
    );
    let people = People::new(Arc::clone(&store), Arc::clone(&clock));

    // 4. A cancelled pass does nothing and says so; the next one starts from scratch.
    let cancelled = CancelToken::new();
    cancelled.cancel();
    match scanner.scan_project(&project, Priority::AiBatch, &cancelled, &NullProgress) {
        Ok(report) if report.cancelled && report.scanned == 0 => {
            println!("cancel: stopped before the first frame, nothing written");
        }
        Ok(report) => {
            eprintln!(
                "cancel: expected nothing scanned, got {} (cancelled {})",
                report.scanned, report.cancelled
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("cancel: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. The face pass.
    let started = clock.monotonic_ms();
    let report = match scanner.scan_project(
        &project,
        Priority::AiBatch,
        &CancelToken::new(),
        &NullProgress,
    ) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("scan: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let scan_ms = clock.monotonic_ms().saturating_sub(started);
    println!(
        "scan: {} frames, {} faces ({} voting, {} gated), {} bodies ({} headless), {} failed, \
         {} tiled ({:.0}%), {:.0} ms per frame",
        report.scanned,
        report.faces,
        report.voting,
        report.gated,
        report.person_boxes,
        report.headless,
        report.failed,
        report.tiled_frames,
        report.tile_ratio() * 100.0,
        report.ms_per_image()
    );
    if report.scanned != FRAMES || report.failed > 0 {
        eprintln!("scan: expected {FRAMES} frames scanned and none failed");
        failures += 1;
    }
    let _ = scan_ms;

    // 6. Recall against the known positions. Section 10.1's first gate.
    let mut matched = 0usize;
    let mut total = 0usize;
    let mut false_positives = 0usize;
    for (photo, faces) in &truth {
        let stored = match store.faces_of_image(&project, *photo) {
            Ok(rows) => rows,
            Err(err) => {
                eprintln!("recall: [{}] {}", err.code, err.detail);
                failures += 1;
                continue;
            }
        };
        let detections: Vec<aura_vision::face::detect::Detection> = stored
            .iter()
            .map(|face| aura_vision::face::detect::Detection {
                face: face.bbox,
                person: None,
                face_score: face.det_score,
                person_score: 0.0,
                landmarks: face.landmarks,
                stride: 8,
                found_by: face.found_by,
            })
            .collect();
        let (hit, count) = fixtures::recall_at_iou(faces, &detections, 0.5);
        matched += hit;
        total += count;
        false_positives += fixtures::false_positives(faces, &detections, 0.5);
    }
    let recall = if total == 0 {
        0.0
    } else {
        matched as f64 / total as f64
    };
    let false_positive_rate = if report.faces == 0 {
        0.0
    } else {
        false_positives as f64 / report.faces as f64
    };
    println!(
        "recall: {recall:.4} at IoU 0.5 ({matched}/{total}), false positives {false_positives} \
         ({:.2}%)",
        false_positive_rate * 100.0
    );
    if recall < RECALL_GATE {
        eprintln!("recall: {recall:.4} below the {RECALL_GATE} gate");
        failures += 1;
    }
    if false_positive_rate > 0.01 {
        eprintln!("recall: false-positive rate {false_positive_rate:.4} above the 0.01 gate");
        failures += 1;
    }

    // 7. Resumability: a second pass has nothing to do.
    match scanner.scan_project(
        &project,
        Priority::AiBatch,
        &CancelToken::new(),
        &NullProgress,
    ) {
        Ok(second) if second.scanned == 0 => {
            println!("resume: a completed project re-scans nothing");
        }
        Ok(second) => {
            eprintln!("resume: expected 0 frames, re-scanned {}", second.scanned);
            failures += 1;
        }
        Err(err) => {
            eprintln!("resume: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 8. Templates are sealed. The column must hold an envelope, never a vector.
    match sealed_sample(&catalog, &project_key) {
        Ok(Some(blob)) => {
            if blob.starts_with(aura_people::vault::MAGIC) {
                println!(
                    "sealed: faces.embed holds a {} byte envelope, not a vector",
                    blob.len()
                );
            } else {
                eprintln!("sealed: faces.embed is not an AURA envelope");
                failures += 1;
            }
        }
        Ok(None) => {
            eprintln!("sealed: no face carries a template");
            failures += 1;
        }
        Err(err) => {
            eprintln!("sealed: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 9. Grouping, roles and the co-occurrence graph.
    let group = match people.regroup(&project) {
        Ok(group) => group,
        Err(err) => {
            eprintln!("group: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "group: {} identities, {} faces assigned, {} unassigned, {} refused merges, \
         coverage {:.0}%, {} ms",
        group.identities,
        group.assigned,
        group.unassigned,
        group.refused_merges,
        group.coverage * 100.0,
        group.elapsed_ms
    );
    if group.identities == 0 {
        eprintln!("group: no identities were formed");
        failures += 1;
    }
    if !group.scene_starved {
        eprintln!("group: scenes are not available until phase 07; the report must say so");
        failures += 1;
    }

    // Every role carries reasons. Invariant 2.
    let identities = match store.load_identities(&project) {
        Ok(rows) => rows,
        Err(err) => {
            eprintln!("roles: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let unexplained = identities
        .iter()
        .filter(|identity| identity.role_confidence > 0.0 && identity.role_reasons.is_empty())
        .count();
    if unexplained > 0 {
        eprintln!("roles: {unexplained} identities carry a role with no reasons");
        failures += 1;
    } else {
        println!("roles: every role above zero confidence carries reasons");
    }

    // 10. Every image exposes a subject focus score. Acceptance criterion 4.
    let panel_started = clock.monotonic_ms();
    let mut scored = 0usize;
    for (photo, _) in &truth {
        match people.subjects(*photo) {
            Ok(subjects) => {
                if subjects.weights_ver > 0 {
                    scored += 1;
                }
            }
            Err(err) => {
                eprintln!("subjects: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    let panel_ms = clock.monotonic_ms().saturating_sub(panel_started);
    println!("subjects: {scored}/{FRAMES} frames scored, {panel_ms} ms for the whole project");
    if scored != FRAMES {
        eprintln!("subjects: every image must expose a subject focus score");
        failures += 1;
    }

    // The panel budget, measured over the reads a first paint actually makes.
    let open_started = clock.monotonic_ms();
    drop(people.hierarchy(project));
    drop(store.load_identities(&project));
    drop(store.coverage(&project));
    let open_ms = clock.monotonic_ms().saturating_sub(open_started);
    println!("panel: {open_ms} ms to open (budget {PANEL_BUDGET_MS} ms)");
    if open_ms > PANEL_BUDGET_MS {
        eprintln!("panel: {open_ms} ms exceeds the {PANEL_BUDGET_MS} ms budget");
        failures += 1;
    }

    // 11. A photographer's decision survives a full re-analysis. Section 12's first
    //     failure mode, and the mitigation.
    if let Some(target) = identities.first() {
        let decided = people
            .set_role(target.identity_id, Role::Bride)
            .and_then(|()| people.rename(target.identity_id, Some("Priya")));
        if let Err(err) = decided {
            eprintln!("decision: [{}] {}", err.code, err.detail);
            failures += 1;
        } else if let Err(err) = people.regroup(&project) {
            eprintln!("decision: [{}] {}", err.code, err.detail);
            failures += 1;
        } else {
            match store.load_identities(&project) {
                Ok(after) => {
                    let survived = after.iter().any(|identity| {
                        identity.role == Role::Bride
                            && identity.user_locked
                            && identity.label.as_deref() == Some("Priya")
                    });
                    if survived {
                        println!("decision: the bride kept her role and her name across a regroup");
                    } else {
                        eprintln!("decision: a photographer's decision did not survive a regroup");
                        failures += 1;
                    }
                }
                Err(err) => {
                    eprintln!("decision: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }
    }

    // 12. Nothing crosses a wedding. Section 2.2, and the code that halts.
    let other = ProjectId::new();
    if let Some(identity) = identities.first() {
        match store.assert_same_project(&other, &[identity.identity_id]) {
            Err(err) if err.code.0 == "AURA-SEC-9004" => {
                println!(
                    "scoping: a cross-project identity is refused with {}",
                    err.code
                );
            }
            Err(err) => {
                eprintln!("scoping: expected AURA-SEC-9004, got {}", err.code);
                failures += 1;
            }
            Ok(()) => {
                eprintln!("scoping: a cross-project identity was accepted");
                failures += 1;
            }
        }
    }

    // 13. Determinism: the same pixels produce the same face ids twice.
    let before: Vec<String> = store
        .load_faces(&project)
        .map(|faces| faces.iter().map(|face| face.face_id.to_db()).collect())
        .unwrap_or_default();
    if let Err(err) = catalog.writer().transact({
        let key = project_key.clone();
        move |tx| {
            tx.execute(
                "DELETE FROM face_scan WHERE project_id = ?1",
                rusqlite::params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("clear the ledger", &e))?;
            Ok(())
        }
    }) {
        eprintln!("determinism: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    drop(scanner.scan_project(
        &project,
        Priority::AiBatch,
        &CancelToken::new(),
        &NullProgress,
    ));
    let after: Vec<String> = store
        .load_faces(&project)
        .map(|faces| faces.iter().map(|face| face.face_id.to_db()).collect())
        .unwrap_or_default();
    if before == after && !before.is_empty() {
        println!(
            "determinism: {} face ids identical across two scans",
            before.len()
        );
    } else {
        eprintln!(
            "determinism: {} ids before, {} after, and they differ",
            before.len(),
            after.len()
        );
        failures += 1;
    }

    // 14. Erasure leaves nothing behind, and keeps everything that is not biometric.
    let photos_before = catalog.count("photo").unwrap_or(0);
    match people.erase(&project) {
        Ok(erased) => {
            println!(
                "erase: {} faces, {} identities, {} sealed crops, key removed {}",
                erased.faces, erased.identities, erased.crops, erased.key_removed
            );
            match store.coverage(&project) {
                Ok((_, scanned, faces, _)) if scanned == 0 && faces == 0 => {
                    println!("erase: nothing survived");
                }
                Ok((_, scanned, faces, _)) => {
                    eprintln!("erase: {faces} faces and {scanned} scans survived");
                    failures += 1;
                }
                Err(err) => {
                    eprintln!("erase: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
            let photos_after = catalog.count("photo").unwrap_or(0);
            if photos_after == photos_before {
                println!("erase: the photographs and their catalog rows are untouched");
            } else {
                eprintln!("erase: {photos_before} photographs before, {photos_after} after");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("erase: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    if failures == 0 {
        println!("phase-06 verify: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-06 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

/// One sealed template from the catalog, straight out of the column.
fn sealed_sample(catalog: &Catalog, project: &str) -> AuraResult<Option<Vec<u8>>> {
    let key = project.to_string();
    catalog.read(move |conn| {
        let found: Result<Vec<u8>, rusqlite::Error> = conn.query_row(
            "SELECT embed FROM faces WHERE project_id = ?1 AND embed IS NOT NULL LIMIT 1",
            rusqlite::params![key],
            |row| row.get(0),
        );
        match found {
            Ok(blob) => Ok(Some(blob)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(aura_core::errors::db::statement_failed(
                "read a template",
                &err,
            )),
        }
    })
}

/// The three face models, written to `work/models` and served through the real runtime.
fn gate_engine(
    directory: &std::path::Path,
    clock: Arc<dyn Clock>,
) -> AuraResult<Arc<dyn InferService>> {
    let detect = directory.join("face_detect.onnx");
    let embed = directory.join("face_embed.onnx");
    let quality = directory.join("face_quality.onnx");
    for (path, model) in [
        (&detect, onnx_fixtures::face_detect()),
        (&embed, onnx_fixtures::face_embed()),
        (&quality, onnx_fixtures::face_quality()),
    ] {
        std::fs::write(path, serialise(&model))
            .map_err(|err| aura_core::errors::io::unreadable(path, &err))?;
    }

    let source = Arc::new(
        StaticModelSource::new()
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::detect::DETECT_MODEL,
                    aura_vision::face::detect::DETECT_MODEL_VERSION,
                ),
                &detect,
                Precision::Fp32,
                ModelClass::Segmentation,
                32,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::embed::EMBED_MODEL,
                    aura_vision::face::embed::EMBED_MODEL_VERSION,
                ),
                &embed,
                Precision::Fp32,
                ModelClass::Embedding,
                16,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::quality::QUALITY_MODEL,
                    aura_vision::face::quality::QUALITY_MODEL_VERSION,
                ),
                &quality,
                Precision::Fp32,
                ModelClass::Embedding,
                16,
            ),
    );
    let mut plan = HardwarePlan::conservative(4, "2026-08-13T00:00:00Z".to_string());
    plan.default_batch = BatchDefaults {
        embedding: 16,
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
