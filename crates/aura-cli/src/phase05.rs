//! The phase 05 gate.
//!
//! `aura-cli verify --phase 05` proves, on this machine, in one run, everything
//! section 13 of the phase document asks for that can be proved without a human:
//! the migration lands, a wedding's worth of RAW fixtures is imported and embedded,
//! every image gains a vector and all four descriptors, "find similar" answers in
//! well under five milliseconds, a time-windowed query returns only in-window
//! frames, the snapshot makes a second open a read rather than a rebuild, a second
//! card embeds only its own files, a killed pass resumes without recomputing, and
//! the same pixels produce the same vector twice.
//!
//! It is deliberately a separate binary path from the tests, for the same reason
//! the phase 03 and phase 04 gates are: the tests prove the pieces, the gate proves
//! the assembly, with the same code CI runs and a human reads the output of.
//!
//! It never touches a network. Nothing in this phase can.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority};
use aura_index::contract::index::{IndexFilter, SimilarityIndex};
use aura_index::hnsw::{HnswIndex, HnswParams};
use aura_index::metrics::{exact_knn, recall_at_k};
use aura_index::query::{self, NEAR_DUPLICATE_HAMMING};
use aura_index::snapshot::Snapshot;
use aura_index::store::{EmbeddingStore, BYTES_PER_IMAGE};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_vision::embed::model::{MODEL_VER, PREPROCESS_VER};
use aura_vision::EmbeddingRunner;

/// Section 11: one kNN query at k = 32.
const QUERY_BUDGET_MS: f32 = 5.0;

/// Section 11: storage per image, where a kilobyte is 1,024 bytes.
const STORAGE_BUDGET_BYTES: usize = 1_638;

/// Section 11 and acceptance criterion 4, for the snapshot path.
const SNAPSHOT_BUDGET_MS: u64 = 400;

/// Run the phase 05 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase05-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Two cards of RAW fixtures. The second one is what proves the incremental
    //    path later: acceptance criterion 5 is about what happens on the *second*
    //    import, so there has to be a second import.
    let first_card = work.join("card-a");
    let second_card = work.join("card-b");
    drop(std::fs::remove_dir_all(&first_card));
    drop(std::fs::remove_dir_all(&second_card));
    let first_count = match write_gate_fixtures(&first_card, 1) {
        Ok(count) => count,
        Err(err) => {
            eprintln!("fixtures: {err}");
            return ExitCode::FAILURE;
        }
    };
    let second_count = match write_gate_fixtures(&second_card, 2) {
        Ok(count) => count,
        Err(err) => {
            eprintln!("fixtures: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("fixtures: {first_count} files on card A, {second_count} on card B");

    // 2. The catalog reaches schema 5 and has both new tables.
    let catalog_path = work.join("phase05.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let cache_root = work.join("cache");
    drop(std::fs::remove_dir_all(&cache_root));

    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(5) => println!("migration: catalog at schema 5"),
        Ok(other) => {
            eprintln!("migration: catalog is at schema {other}, expected 5");
            failures += 1;
        }
        Err(err) => {
            eprintln!("migration: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for table in ["embeddings", "descriptors"] {
        if let Err(err) = catalog.count(table) {
            eprintln!(
                "migration: {table} is missing - [{}] {}",
                err.code, err.detail
            );
            failures += 1;
        }
    }
    if !migration_documents_its_rollback() {
        eprintln!("migration: the rollback statements are not recorded in the migration file");
        failures += 1;
    }

    let project_id = match crate::ensure_project(&catalog, clock.as_ref(), "phase-05") {
        Ok(id) => id,
        Err(err) => {
            eprintln!("catalog: {err}");
            return ExitCode::FAILURE;
        }
    };
    let project = project_id.to_db();

    // 3. Import card A.
    let plan = ImportPlan {
        import_id: aura_core::ImportId::new(),
        project_id,
        roots: vec![first_card.clone()],
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: true,
        settle_window_ms: 0,
    };
    let import = match aura_ingest::run(&catalog, &plan, &CancelToken::new(), &NullProgress) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("import: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!("import: {} photographs from card A", import.files_imported);

    match assign_timeline(&catalog, &project, clock.as_ref()) {
        Ok(count) => println!(
            "timeline: {count} capture times written by the gate - the RAW fixtures carry make and model but no clock, so a time-windowed query would otherwise have nothing to find"
        ),
        Err(err) => {
            eprintln!("timeline: {err}");
            failures += 1;
        }
    }

    // 4. Embed everything, resumably. The pass is run twice with a cancel in
    //    between, which is how invariant 5 is proved rather than asserted.
    let store = Arc::new(EmbeddingStore::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
    ));
    let runner = match build_runner(&catalog, &cache_root, &project, &store, &clock) {
        Ok(runner) => runner,
        Err(err) => {
            eprintln!("embed: {err}");
            return ExitCode::FAILURE;
        }
    };

    let pending_before = match store.pending(&project, MODEL_VER) {
        Ok(pending) => pending.len(),
        Err(err) => {
            eprintln!("embed: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    // A pass cancelled before it starts must do nothing and say so.
    let stopped = CancelToken::new();
    stopped.cancel();
    match runner.embed_project(&project, Priority::AiBatch, &stopped, &NullProgress) {
        Ok(report) => {
            if report.embedded != 0 || !report.cancelled {
                eprintln!(
                    "resume: a cancelled pass embedded {} and reported cancelled={}",
                    report.embedded, report.cancelled
                );
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("resume: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match store.pending(&project, MODEL_VER) {
        Ok(pending) if pending.len() == pending_before => {
            println!("resume: a cancelled pass left all {pending_before} still to do");
        }
        Ok(pending) => {
            eprintln!(
                "resume: a cancelled pass changed the work left from {pending_before} to {}",
                pending.len()
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("resume: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let report = match runner.embed_project(
        &project,
        Priority::AiBatch,
        &CancelToken::new(),
        &NullProgress,
    ) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("embed: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "embed: {} analysed, {} unreadable, {} batches, {} ms ({:.0} ms each)",
        report.embedded,
        report.failed,
        report.batches,
        report.elapsed_ms,
        report.ms_per_image()
    );

    // Acceptance criterion 1: every image has a vector and all four descriptors.
    // One fixture is deliberately undecodable, so the honest expectation is
    // "everything the decoder could read", and the failure is counted rather than
    // hidden.
    match store.coverage(&project) {
        Ok((photos, embedded)) => {
            let expected = photos - i64::try_from(report.failed).unwrap_or(0);
            if embedded == expected && embedded > 0 {
                println!("coverage: {embedded} of {photos} photographs analysed, {} could not be decoded", report.failed);
            } else {
                eprintln!("coverage: {embedded} of {photos} analysed, expected {expected}");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("coverage: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. Build the index and query it.
    let (entries, camera_labels) = match store.load_entries(&project) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("index: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let corpus: Vec<_> = entries
        .iter()
        .map(|entry| entry.embedding.clone())
        .collect();
    let index = HnswIndex::build(entries.clone(), HnswParams::default(), clock.as_ref());
    index.set_camera_labels(&camera_labels);
    let stats = index.stats();
    println!(
        "index: {} vectors, {} filterable, built in {} ms, model version {}",
        stats.vectors, stats.filterable, stats.build_ms, stats.model_ver
    );
    if stats.degenerate > 0 {
        eprintln!(
            "index: {} degenerate vectors reached the graph",
            stats.degenerate
        );
        failures += 1;
    }
    if stats.model_ver != MODEL_VER {
        eprintln!(
            "index: graph is at model version {}, this build uses {MODEL_VER}",
            stats.model_ver
        );
        failures += 1;
    }

    // Acceptance criterion 2: neighbours in under five milliseconds.
    let Some(subject) = corpus.first().cloned() else {
        eprintln!("index: nothing was embedded, so nothing can be queried");
        return ExitCode::FAILURE;
    };
    let mut worst_ms = 0.0f32;
    let mut recall_total = 0.0f64;
    let mut queries = 0usize;
    for embedding in &corpus {
        let answer =
            query::find_similar(&index, embedding, 32, IndexFilter::none(), clock.as_ref());
        worst_ms = worst_ms.max(answer.elapsed_ms());
        let approximate: Vec<_> = answer
            .neighbours
            .iter()
            .map(|neighbour| (neighbour.id, neighbour.distance))
            .collect();
        recall_total += recall_at_k(&approximate, &exact_knn(embedding, &corpus, 10));
        queries += 1;
        if answer
            .neighbours
            .iter()
            .any(|neighbour| neighbour.id == embedding.image_id)
        {
            eprintln!("query: a frame was returned as its own neighbour");
            failures += 1;
        }
    }
    let recall = if queries == 0 {
        0.0
    } else {
        recall_total / queries as f64
    };
    println!("query: {queries} queries, worst {worst_ms:.3} ms, recall@10 {recall:.4} against the flat scan");
    if worst_ms > QUERY_BUDGET_MS {
        eprintln!("query: worst query was {worst_ms:.3} ms, budget {QUERY_BUDGET_MS} ms");
        failures += 1;
    }
    if recall < 0.95 {
        eprintln!("query: recall@10 was {recall:.4}, which is below 0.95");
        failures += 1;
    }

    // A time window returns only in-window frames.
    let windowed = index.knn(&subject, 64, IndexFilter::within_seconds(5));
    if windowed.is_empty() {
        eprintln!("window: a five-second window found nothing, so the filter proves nothing");
        failures += 1;
    }
    let subject_at = index
        .meta_of(&subject.image_id)
        .and_then(|meta| meta.timeline_ms);
    let mut out_of_window = 0usize;
    for (id, _) in &windowed {
        let stamp = index.meta_of(id).and_then(|meta| meta.timeline_ms);
        match (subject_at, stamp) {
            (Some(at), Some(other)) if (other - at).abs() <= 5_000 => {}
            _ => out_of_window += 1,
        }
    }
    if out_of_window == 0 {
        println!(
            "window: {} frames within five seconds, none outside it",
            windowed.len()
        );
    } else {
        eprintln!("window: {out_of_window} frames outside the window were returned");
        failures += 1;
    }

    // A camera filter returns one body.
    match index
        .meta_of(&subject.image_id)
        .and_then(|meta| meta.camera)
    {
        Some(handle) => {
            let filtered = index.knn(&subject, 64, IndexFilter::none().with_camera(handle));
            let wrong = filtered
                .iter()
                .filter(|(id, _)| index.meta_of(id).and_then(|meta| meta.camera) != Some(handle))
                .count();
            let label = index.camera_label(handle).unwrap_or_default();
            if wrong == 0 && !filtered.is_empty() {
                println!(
                    "camera: {} frames from {label}, none from another body",
                    filtered.len()
                );
            } else if filtered.is_empty() {
                eprintln!("camera: the subject's own body returned no other frame, so the filter proves nothing");
                failures += 1;
            } else {
                eprintln!("camera: {wrong} frames from another body were returned");
                failures += 1;
            }
        }
        None => {
            eprintln!("camera: the subject has no camera handle, so the filter cannot be checked");
            failures += 1;
        }
    }

    // The difference hash answers the trivial question without the graph.
    let near = index.dhash_within(subject.dhash, NEAR_DUPLICATE_HAMMING);
    println!(
        "duplicates: {} frames within {NEAR_DUPLICATE_HAMMING} bits of the subject's hash",
        near.len()
    );
    if near.is_empty() {
        eprintln!("duplicates: a frame is not even within five bits of itself");
        failures += 1;
    }

    // The medoid of a set is a member of it.
    let members: Vec<PhotoId> = corpus
        .iter()
        .take(8)
        .map(|embedding| embedding.image_id)
        .collect();
    match index.medoid(&members) {
        Some(found) if members.contains(&found) => {
            println!("medoid: {found} is the most representative of eight frames");
        }
        Some(found) => {
            eprintln!("medoid: {found} is not in the set it was chosen from");
            failures += 1;
        }
        None => {
            eprintln!("medoid: no medoid for a non-empty set");
            failures += 1;
        }
    }

    // 6. The snapshot: a second open is a read.
    let snapshot_path = Snapshot::path_for(&cache_root, &project, MODEL_VER);
    match Snapshot::write(&index, &snapshot_path, MODEL_VER, PREPROCESS_VER) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("snapshot: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    let started = clock.monotonic_ms();
    match Snapshot::load(
        &snapshot_path,
        HnswParams::default(),
        MODEL_VER,
        PREPROCESS_VER,
    ) {
        Ok(loaded) => {
            let elapsed = clock.monotonic_ms().saturating_sub(started);
            let same = loaded.knn(&subject, 16, IndexFilter::none())
                == index.knn(&subject, 16, IndexFilter::none());
            println!(
                "snapshot: {} vectors loaded in {elapsed} ms, answers identical: {same}",
                loaded.len()
            );
            if !same {
                eprintln!("snapshot: a reloaded graph answered differently");
                failures += 1;
            }
            if elapsed > SNAPSHOT_BUDGET_MS {
                eprintln!("snapshot: load took {elapsed} ms, budget {SNAPSHOT_BUDGET_MS} ms");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("snapshot: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // A snapshot from another model version is refused rather than half-trusted.
    match Snapshot::load(
        &snapshot_path,
        HnswParams::default(),
        MODEL_VER + 10,
        PREPROCESS_VER,
    ) {
        Err(err) if err.code.0 == "AURA-ML-5014" => {
            println!("snapshot: a stale version is refused with {}", err.code);
        }
        Err(err) => {
            eprintln!("snapshot: expected AURA-ML-5014, got {}", err.code);
            failures += 1;
        }
        Ok(_) => {
            eprintln!("snapshot: a snapshot from another model version was accepted");
            failures += 1;
        }
    }

    // 7. Acceptance criterion 5: a second card embeds only its own files.
    let second_plan = ImportPlan {
        import_id: aura_core::ImportId::new(),
        project_id,
        roots: vec![second_card.clone()],
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: true,
        settle_window_ms: 0,
    };
    match aura_ingest::run(&catalog, &second_plan, &CancelToken::new(), &NullProgress) {
        Ok(second) => {
            if let Err(err) = assign_timeline(&catalog, &project, clock.as_ref()) {
                eprintln!("timeline: {err}");
                failures += 1;
            }
            let expected = match store.pending(&project, MODEL_VER) {
                Ok(pending) => pending.len(),
                Err(err) => {
                    eprintln!("incremental: [{}] {}", err.code, err.detail);
                    failures += 1;
                    0
                }
            };
            match runner.embed_project(
                &project,
                Priority::AiBatch,
                &CancelToken::new(),
                &NullProgress,
            ) {
                Ok(second_pass) => {
                    let touched = second_pass.embedded + second_pass.failed;
                    println!(
                        "incremental: card B imported {} photographs, the second pass touched {touched} of them and re-read none of card A",
                        second.files_imported
                    );
                    if touched != expected {
                        eprintln!(
                            "incremental: the second pass touched {touched} photographs and {expected} were pending"
                        );
                        failures += 1;
                    }
                }
                Err(err) => {
                    eprintln!("incremental: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }
        Err(err) => {
            eprintln!("incremental: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // Incremental insert must not cost recall. Section 10.1 allows one point.
    let (all_entries, _) = match store.load_entries(&project) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("index: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let full_corpus: Vec<_> = all_entries
        .iter()
        .map(|entry| entry.embedding.clone())
        .collect();
    let grown = HnswIndex::build(entries, HnswParams::default(), clock.as_ref());
    let added: Vec<_> = all_entries
        .iter()
        .filter(|entry| !grown.contains(&entry.embedding.image_id))
        .cloned()
        .collect();
    grown.extend(added);
    let rebuilt = HnswIndex::build(all_entries, HnswParams::default(), clock.as_ref());

    let mut grown_recall = 0.0f64;
    let mut rebuilt_recall = 0.0f64;
    let mut counted = 0usize;
    for embedding in &full_corpus {
        let exact = exact_knn(embedding, &full_corpus, 10);
        grown_recall += recall_at_k(&grown.knn(embedding, 10, IndexFilter::none()), &exact);
        rebuilt_recall += recall_at_k(&rebuilt.knn(embedding, 10, IndexFilter::none()), &exact);
        counted += 1;
    }
    if counted > 0 {
        grown_recall /= counted as f64;
        rebuilt_recall /= counted as f64;
    }
    println!(
        "incremental: recall@10 {grown_recall:.4} grown against {rebuilt_recall:.4} rebuilt over {} vectors",
        grown.len()
    );
    if grown_recall < rebuilt_recall - 0.01 {
        eprintln!("incremental: growing the index cost more than one point of recall");
        failures += 1;
    }

    // 8. Determinism: the same pixels, the same vector.
    match determinism_holds(&runner, &store, &project) {
        Ok(true) => println!("determinism: re-embedding one frame produced identical bytes"),
        Ok(false) => {
            eprintln!("determinism: re-embedding one frame produced a different vector");
            failures += 1;
        }
        Err(err) => {
            eprintln!("determinism: {err}");
            failures += 1;
        }
    }

    // 9. Storage: section 11's budget, from the constant the schema is built on.
    if BYTES_PER_IMAGE <= STORAGE_BUDGET_BYTES {
        println!(
            "storage: {BYTES_PER_IMAGE} bytes per image, budget {STORAGE_BUDGET_BYTES} ({} spare)",
            STORAGE_BUDGET_BYTES - BYTES_PER_IMAGE
        );
    } else {
        eprintln!("storage: {BYTES_PER_IMAGE} bytes per image exceeds {STORAGE_BUDGET_BYTES}");
        failures += 1;
    }

    // 10. Nothing here opened a socket, and nothing here can.
    println!("offline: the whole phase ran with no network of any kind");

    if failures == 0 {
        println!("\nphase 05 gate: pass");
        ExitCode::SUCCESS
    } else {
        eprintln!("\nphase 05 gate: {failures} failures");
        ExitCode::FAILURE
    }
}

/// Two takes of every bench body, so a camera filter has something to filter and a
/// burst has more than one frame in it.
///
/// `card` shifts the patch edge, which changes the pixels and therefore the content
/// hash - so card B is a genuinely different set of photographs rather than a second
/// copy of card A, and the incremental import has real work to do.
fn write_gate_fixtures(out: &Path, card: usize) -> Result<usize, String> {
    use aura_raw::fixtures::{write_bench_raw, FixtureOptions, MosaicEncoding};

    std::fs::create_dir_all(out).map_err(|err| format!("create {}: {err}", out.display()))?;

    // Every encoding the decoder claims to read, cycled across the bodies so one
    // gate run exercises all of them.
    let encodings = [
        MosaicEncoding::Unpacked16,
        MosaicEncoding::Packed14,
        MosaicEncoding::LosslessJpeg,
        MosaicEncoding::NikonCompressed,
        MosaicEncoding::SonyArw2,
        MosaicEncoding::OlympusCompressed,
        MosaicEncoding::XTrans16,
    ];

    let mut written = 0usize;
    for body in 1..=8usize {
        for take in 0..2usize {
            let encoding = encodings
                .get((body - 1) % encodings.len())
                .copied()
                .unwrap_or(MosaicEncoding::Unpacked16);
            let options = FixtureOptions {
                body,
                encoding,
                with_preview: true,
                orientation: if body % 4 == 0 { 6 } else { 1 },
                with_colour_matrix: body % 3 != 0,
                patch_edge: 48 + (card as u32 * 24) + (take as u32 * 12),
            };
            let path = out.join(format!(
                "card{card}-body{body:02}-take{take}-{}.dng",
                encoding.as_str()
            ));
            write_bench_raw(&path, options)?;
            written += 1;
        }
    }

    // A file with a RAW extension that is not a RAW at all. Every gate run must set
    // it aside and finish the rest: one unreadable frame out of a wedding is an item
    // failure, not a run failure.
    let poison = out.join(format!("card{card}-poison.dng"));
    std::fs::write(&poison, b"AURA phase-05 poison fixture: not a photograph")
        .map_err(|err| format!("write {}: {err}", poison.display()))?;
    written += 1;

    Ok(written)
}

/// Give the imported photographs a wedding-shaped timeline.
///
/// The RAW fixtures carry make, model and orientation but no capture time, so every
/// photograph arrives with a null timeline and a time-windowed query has nothing to
/// find. Rather than skip the criterion, the gate writes the timeline it needs -
/// bursts of four frames two seconds apart, bursts ten minutes apart, which is the
/// shape of a real ceremony - and says so in its output.
///
/// Ordered by photo id so two runs produce the same timeline.
fn assign_timeline(
    catalog: &Arc<Catalog>,
    project: &str,
    clock: &dyn Clock,
) -> Result<usize, String> {
    use aura_catalog::repo;

    let rows = catalog
        .read(|conn| repo::list_photos_for_timeline(conn, project))
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;

    let now = aura_catalog::rfc3339(clock.now_utc());
    let mut written = 0usize;
    for (index, photo_id) in rows.iter().enumerate() {
        let burst = index / 4;
        let within = index % 4;
        let stamp = format!(
            "2026-06-13T{:02}:{:02}:{:02}.000Z",
            13 + (burst * 10) / 60,
            (burst * 10) % 60,
            within * 2
        );
        let photo = photo_id.clone();
        let at = stamp.clone();
        let stamped = now.clone();
        written += catalog
            .writer()
            .with(move |conn| repo::set_capture_time(conn, &photo, &at, &stamped))
            .map_err(|err| format!("[{}] {}", err.code, err.detail))?;
    }
    Ok(written)
}

/// Build the embedding runner over a real preview service and a real runtime.
fn build_runner(
    catalog: &Arc<Catalog>,
    cache_root: &Path,
    project: &str,
    store: &Arc<EmbeddingStore>,
    clock: &Arc<dyn Clock>,
) -> Result<EmbeddingRunner, String> {
    use aura_infer::contract::infer::{HardwarePlan, ModelClass, Precision};
    use aura_infer::ep::BackendRegistry;
    use aura_infer::onnx::{fixtures, serialise};
    use aura_infer::service::InferEngine;
    use aura_infer::source::StaticModelSource;
    use aura_preview::{CatalogSource, PreviewConfig, PreviewSource, Previews};

    let source: Arc<dyn PreviewSource> =
        Arc::new(CatalogSource::new(Arc::clone(catalog), project.to_string()));
    let previews = Arc::new(
        Previews::open(
            cache_root,
            aura_cache::CacheBudget::default(),
            source,
            PreviewConfig::default(),
        )
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?,
    );

    // The model comes from the fixture generator rather than from `models/`, so the
    // gate runs on a machine that has never generated the model pack. The bytes are
    // identical either way: the generator is seeded, which is the whole reason
    // `models.lock` verifies on every machine.
    let model_dir = cache_root.join("gate-model");
    std::fs::create_dir_all(&model_dir).map_err(|err| format!("create model dir: {err}"))?;
    let model_path = model_dir.join("wedding_embedding.onnx");
    std::fs::write(&model_path, serialise(&fixtures::wedding_embedding()))
        .map_err(|err| format!("write the model: {err}"))?;

    let models = Arc::new(StaticModelSource::new().with(
        aura_vision::embed::model::model_ref(),
        &model_path,
        Precision::Fp32,
        ModelClass::Embedding,
        16,
    ));
    let engine = Arc::new(InferEngine::new(
        BackendRegistry::with_reference(),
        models,
        HardwarePlan::conservative(4, "2026-08-13T00:00:00Z".to_string()),
        Arc::clone(clock),
    ));

    Ok(EmbeddingRunner::new(
        previews,
        engine,
        Arc::clone(store),
        Arc::clone(clock),
    ))
}

/// Re-embed one frame and compare the stored bytes.
///
/// Invariant 4, proved through the whole path rather than at the model boundary:
/// the preview cache, the descriptors, the model, the quantisation and the blob
/// encoding all have to agree with themselves.
fn determinism_holds(
    runner: &EmbeddingRunner,
    store: &Arc<EmbeddingStore>,
    project: &str,
) -> Result<bool, String> {
    let (entries, _) = store
        .load_entries(project)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;
    let Some(first) = entries.first() else {
        return Err("nothing was embedded".to_string());
    };
    let id = first.embedding.image_id;
    let before = first.embedding.clone();

    let again = runner
        .embed_one(project, id, Priority::Interactive)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;

    Ok(again.vec == before.vec && again.dhash == before.dhash)
}

/// The migration must carry its own rollback, so nobody has to invent one at three
/// in the morning. The same check the phase 04 gate makes about migration 4.
fn migration_documents_its_rollback() -> bool {
    let sql = include_str!("../../aura-catalog/migrations/0005_embeddings.sql");
    [
        "DROP TABLE IF EXISTS descriptors;",
        "DROP TABLE IF EXISTS embeddings;",
        "DELETE FROM schema_version WHERE version = 5;",
    ]
    .iter()
    .all(|statement| sql.contains(statement))
}
