//! Ingest is judged by one sentence: importing the same folder twice must insert
//! zero rows the second time.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, FixedClock};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{FileKind, ImportMode, ImportPlan, ImportReport};
use aura_ingest::{fixtures, pair, scan};
use time::OffsetDateTime;

fn test_clock() -> Arc<dyn Clock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000))
}

struct Harness {
    _dir: tempfile::TempDir,
    catalog: Catalog,
    project_id: ProjectId,
    root: PathBuf,
}

fn plan_for(harness: &Harness) -> ImportPlan {
    ImportPlan {
        import_id: ImportId::new(),
        project_id: harness.project_id,
        roots: vec![harness.root.clone()],
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: false,
        settle_window_ms: 0,
    }
}

fn harness_with_files(files: &[(&str, &[u8])]) -> Harness {
    let dir = tempfile::tempdir().expect("tmp");
    let root = dir.path().join("cards");
    for (rel, bytes) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&path, bytes).expect("write fixture");
    }

    let catalog = Catalog::open(&dir.path().join("c.sqlite"), test_clock(), "0.1.0-test")
        .expect("open catalog");
    let project_id = ProjectId::new();
    let now = rfc3339(catalog.clock().now_utc());
    let row = ProjectRow {
        project_id: project_id.to_db(),
        name: "Fixture wedding".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
        .expect("create project");

    Harness {
        _dir: dir,
        catalog,
        project_id,
        root,
    }
}

fn harness_with_generated_wedding() -> Harness {
    let dir = tempfile::tempdir().expect("tmp");
    let out = dir.path().join("fixtures");
    let wedding = fixtures::reference_weddings()
        .into_iter()
        .next()
        .expect("at least one reference wedding");
    let generated = fixtures::generate(&out, &wedding).expect("generate fixtures");

    let catalog = Catalog::open(&dir.path().join("c.sqlite"), test_clock(), "0.1.0-test")
        .expect("open catalog");
    let project_id = ProjectId::new();
    let now = rfc3339(catalog.clock().now_utc());
    let row = ProjectRow {
        project_id: project_id.to_db(),
        name: "Generated wedding".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
        .expect("create project");

    Harness {
        _dir: dir,
        catalog,
        project_id,
        root: generated.root,
    }
}

fn run(harness: &Harness) -> ImportReport {
    aura_ingest::run(
        &harness.catalog,
        &plan_for(harness),
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import")
}

fn digest(harness: &Harness) -> String {
    harness
        .catalog
        .read(|conn| repo::catalog_digest(conn, &harness.project_id.to_db()))
        .expect("digest")
}

fn scan_files(root: &Path) -> Vec<aura_ingest::scan::ScannedFile> {
    let plan = ImportPlan {
        import_id: ImportId::new(),
        project_id: ProjectId::new(),
        roots: vec![root.to_path_buf()],
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: false,
        settle_window_ms: 0,
    };
    scan::scan_root(root, &plan, i64::MAX).expect("scan").files
}

// ------------------------------------------------------------------ idempotence

#[test]
fn second_import_of_identical_folder_inserts_nothing() {
    let harness = harness_with_generated_wedding();

    let first = run(&harness);
    let digest_a = digest(&harness);
    let second = run(&harness);
    let digest_b = digest(&harness);

    assert!(first.files_imported > 0, "the first import must do work");
    assert_eq!(second.files_imported, 0, "re-import must insert zero files");
    assert_eq!(second.photos_created, 0);
    assert_eq!(
        second.bytes_hashed, 0,
        "fast_key must prevent all rehashing"
    );
    assert_eq!(
        digest_a, digest_b,
        "catalog must be identical after re-import"
    );
}

#[test]
fn renaming_a_file_does_not_duplicate_the_photo() {
    let harness = harness_with_files(&[("day1/IMG_1.JPG", b"identical bytes for one photo")]);
    run(&harness);

    let photos_before = harness.catalog.count("photo").expect("count");
    std::fs::rename(
        harness.root.join("day1/IMG_1.JPG"),
        harness.root.join("day1/RENAMED.JPG"),
    )
    .expect("rename");

    run(&harness);

    assert_eq!(
        harness.catalog.count("photo").expect("count"),
        photos_before,
        "the same bytes at a new path must attach to the same photograph"
    );
    assert_eq!(
        harness.catalog.count("photo_file").expect("count"),
        2,
        "the new path is a second file row"
    );
}

// ----------------------------------------------------------------------- pairing

#[test]
fn raw_and_jpeg_pair_become_one_photo() {
    let dir = tempfile::tempdir().expect("tmp");
    for name in ["IMG_1.CR2", "IMG_1.JPG", "IMG_1.xmp"] {
        std::fs::write(dir.path().join(name), b"x").expect("write");
    }
    let files = scan_files(dir.path());
    let groups = pair::group(&files);

    assert_eq!(groups.len(), 1);
    let primary = files.get(groups[0].primary).expect("primary");
    assert_eq!(primary.ext, "cr2", "a RAW always wins as primary");
    assert_eq!(groups[0].companions.len(), 2);
}

#[test]
fn same_stem_in_different_folders_are_two_photos() {
    let dir = tempfile::tempdir().expect("tmp");
    for rel in ["day1/IMG_1.CR2", "day2/IMG_1.CR2"] {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"x").expect("write");
    }
    assert_eq!(pair::group(&scan_files(dir.path())).len(), 2);
}

#[test]
fn adobe_double_extension_sidecars_group_with_their_raw() {
    let dir = tempfile::tempdir().expect("tmp");
    for name in ["IMG_9.CR2", "IMG_9.CR2.xmp"] {
        std::fs::write(dir.path().join(name), b"x").expect("write");
    }
    let groups = pair::group(&scan_files(dir.path()));
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].companions.len(), 1);
}

// -------------------------------------------------------------------- scan order

#[test]
fn scan_order_is_stable_across_runs() {
    let dir = tempfile::tempdir().expect("tmp");
    for name in ["b.JPG", "a.JPG", "c.JPG", "A.JPG"] {
        std::fs::write(dir.path().join(name), b"x").expect("write");
    }
    let first: Vec<String> = scan_files(dir.path())
        .iter()
        .map(|f| f.rel_path.clone())
        .collect();
    let second: Vec<String> = scan_files(dir.path())
        .iter()
        .map(|f| f.rel_path.clone())
        .collect();
    assert_eq!(first, second, "scan order must not vary between runs");
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(
        first, sorted,
        "scan order must be sorted, not filesystem order"
    );
}

#[test]
fn unknown_extensions_are_ignored_not_guessed() {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join("notes.txt"), b"x").expect("write");
    std::fs::write(dir.path().join("IMG_1.CR2"), b"x").expect("write");
    let files = scan_files(dir.path());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].kind, FileKind::Raw);
}

// -------------------------------------------------------------------- quarantine

#[test]
fn zero_byte_files_are_quarantined_and_the_import_completes() {
    let harness = harness_with_files(&[("good.JPG", b"real bytes"), ("empty.JPG", b"")]);
    let report = run(&harness);

    assert_eq!(report.files_quarantined, 1);
    assert!(report.files_imported > 0, "the good file still imports");
    assert_eq!(harness.catalog.count("quarantine").expect("count"), 1);

    let problems = harness
        .catalog
        .read(|conn| repo::list_quarantine(conn, &harness.project_id.to_db()))
        .expect("problems");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].1, "AURA-IO-1010");
}

#[test]
fn re_running_bumps_attempts_instead_of_adding_quarantine_rows() {
    let harness = harness_with_files(&[("good.JPG", b"real bytes"), ("empty.JPG", b"")]);
    run(&harness);
    run(&harness);

    assert_eq!(
        harness.catalog.count("quarantine").expect("count"),
        1,
        "one broken file is one row, however many times we retry"
    );
    let attempts: i64 = harness
        .catalog
        .read(|conn| {
            conn.query_row("SELECT attempts FROM quarantine LIMIT 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| aura_core::errors::db::statement_failed("read attempts", &e))
        })
        .expect("attempts");
    assert_eq!(attempts, 2);
}

// ------------------------------------------------------------------ cancellation

#[test]
fn cancel_keeps_completed_work_and_marks_the_run_cancelled() {
    let harness = harness_with_generated_wedding();
    let cancel = CancelToken::new();
    cancel.cancel();

    let report = aura_ingest::run(
        &harness.catalog,
        &plan_for(&harness),
        &cancel,
        &NullProgress,
    )
    .expect("cancelled cleanly");

    assert_eq!(
        report.files_imported, 0,
        "nothing was inserted before the cancel"
    );
    harness.catalog.integrity_check().expect("catalog intact");

    let state: String = harness
        .catalog
        .read(|conn| {
            conn.query_row(
                "SELECT state FROM import_run ORDER BY started_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("read state", &e))
        })
        .expect("state");
    assert_eq!(state, "cancelled");
}

// ------------------------------------------------------------------------- EXIF

#[test]
fn generated_fixtures_carry_readable_exif() {
    let dir = tempfile::tempdir().expect("tmp");
    let wedding = fixtures::reference_weddings()
        .into_iter()
        .next()
        .expect("wedding");
    let generated = fixtures::generate(dir.path(), &wedding).expect("generate");

    let files = scan_files(&generated.root);
    let first = files.first().expect("at least one file");
    let facts = aura_ingest::exif::extract(&first.abs_path).expect("extract");

    assert!(
        facts.capture_time.is_some(),
        "fixtures must carry a capture time"
    );
    assert_eq!(facts.capture_time_source, "exif_original");
    assert!(
        facts.camera_serial.is_some(),
        "fixtures must carry a body serial"
    );
}

#[test]
fn a_file_with_no_exif_still_imports() {
    let harness = harness_with_files(&[("plain.JPG", b"not a real jpeg at all")]);
    let report = run(&harness);

    assert_eq!(report.files_quarantined, 0);
    assert_eq!(harness.catalog.count("photo").expect("count"), 1);

    let source: String = harness
        .catalog
        .read(|conn| {
            conn.query_row("SELECT capture_time_source FROM photo LIMIT 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| aura_core::errors::db::statement_failed("read source", &e))
        })
        .expect("source");
    assert_eq!(source, "unknown", "a missing time is never invented");
}

#[test]
fn exif_datetime_parsing_handles_offsets_and_junk() {
    use aura_ingest::exif::parse_exif_datetime;

    let utc = parse_exif_datetime("2026:05:02 14:30:00", None).expect("parse");
    assert_eq!(utc.hour(), 14);

    let shifted = parse_exif_datetime("2026:05:02 14:30:00", Some("+05:45")).expect("parse");
    assert_eq!(shifted.hour(), 8, "an offset is normalised to UTC");
    assert_eq!(shifted.minute(), 45);

    assert!(parse_exif_datetime("not a date", None).is_none());
    assert!(parse_exif_datetime("2026:13:45 99:99:99", None).is_none());
}

// -------------------------------------------------------------- clock alignment

#[test]
fn clock_alignment_recovers_synthetic_offsets() {
    use aura_ingest::clock_align::{estimate_offsets, CameraSamples};

    // Irregular moments, because a wedding is not a metronome and a metronome
    // makes every candidate offset look equally likely.
    let base = 1_700_000_000_000i64;
    let mut clock = base;
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let reference: Vec<i64> = (0..150)
        .map(|_| {
            let at = clock;
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            clock += 2_000 + i64::try_from((seed >> 33) % 9_000).unwrap_or(0);
            at
        })
        .collect();

    for skew in [-90_000i64, 12 * 60 * 1_000, 7 * 3_600_000] {
        // The second shooter covers the same moments, but not all of them, and its
        // clock is wrong by `skew`.
        let drifted: Vec<i64> = reference.iter().take(100).map(|t| t + skew).collect();
        let estimates = estimate_offsets(&[
            CameraSamples {
                body_serial: "REF".to_string(),
                times_ms: reference.clone(),
            },
            CameraSamples {
                body_serial: "DRIFT".to_string(),
                times_ms: drifted,
            },
        ]);

        let drift = estimates
            .iter()
            .find(|e| e.body_serial == "DRIFT")
            .expect("drift estimate");
        assert!(drift.applied, "a clean offset must be applied");
        assert!(
            (drift.offset_ms + skew).abs() <= 500,
            "recovered {} for skew {skew}",
            drift.offset_ms
        );
    }
}

#[test]
fn clock_alignment_refuses_to_guess_when_evidence_disagrees() {
    use aura_ingest::clock_align::{estimate_offsets, CameraSamples};

    let base = 1_700_000_000_000i64;
    let reference: Vec<i64> = (0..40).map(|i| base + i * 60_000).collect();
    // Scattered frames with no consistent relationship to the reference clock.
    let scattered: Vec<i64> = (0..40).map(|i| base + i * 137_000 + i * i * 911).collect();

    let estimates = estimate_offsets(&[
        CameraSamples {
            body_serial: "REF".to_string(),
            times_ms: reference,
        },
        CameraSamples {
            body_serial: "SCATTER".to_string(),
            times_ms: scattered,
        },
    ]);

    let scatter = estimates
        .iter()
        .find(|e| e.body_serial == "SCATTER")
        .expect("estimate");
    assert!(!scatter.applied, "low confidence must refuse to guess");
    assert_eq!(scatter.offset_ms, 0);
    assert!(scatter.confidence < aura_ingest::clock_align::MIN_CONFIDENCE);
}

#[test]
fn importing_two_bodies_aligns_the_timeline() {
    let harness = harness_with_generated_wedding();
    run(&harness);

    let cameras = harness
        .catalog
        .read(|conn| repo::list_cameras(conn, &harness.project_id.to_db()))
        .expect("cameras");
    assert!(cameras.len() >= 2, "the fixture wedding has two bodies");

    let drifted = cameras
        .iter()
        .find(|c| c.body_serial.contains("SONY"))
        .expect("second body");
    assert_eq!(drifted.offset_source, "estimated");
    // The fixture body is one hour and twelve seconds ahead, so the correction is
    // negative and close to that magnitude.
    assert!(
        (drifted.clock_offset_ms + 3_612_000).abs() <= 2_000,
        "unexpected offset {}",
        drifted.clock_offset_ms
    );

    let ordered = harness
        .catalog
        .read(|conn| {
            repo::list_photos(
                conn,
                &harness.project_id.to_db(),
                0,
                10,
                repo::PhotoOrder::Timeline,
            )
        })
        .expect("photos");
    let times: Vec<String> = ordered
        .iter()
        .filter_map(|row| row.timeline_ts.clone())
        .collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted, "the grid must come back in timeline order");
}

// ----------------------------------------------------------------------- journal

#[test]
fn the_journal_records_every_path_it_finished() {
    let harness = harness_with_files(&[("a.JPG", b"aaaa"), ("b.JPG", b"bbbb")]);
    run(&harness);

    let inserted: i64 = harness
        .catalog
        .read(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM ingest_journal WHERE phase = 'inserted'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("count journal", &e))
        })
        .expect("count");
    assert_eq!(inserted, 2);
}

#[test]
fn unplugging_a_drive_marks_files_absent_rather_than_deleting_them() {
    let harness = harness_with_files(&[("a.JPG", b"aaaa"), ("b.JPG", b"bbbb")]);
    run(&harness);

    std::fs::remove_file(harness.root.join("b.JPG")).expect("remove");
    run(&harness);

    assert_eq!(
        harness.catalog.count("photo_file").expect("count"),
        2,
        "rows survive so ratings and edits survive"
    );
    let absent: i64 = harness
        .catalog
        .read(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM photo_file WHERE is_present = 0",
                [],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("count absent", &e))
        })
        .expect("count");
    assert_eq!(absent, 1);
}
