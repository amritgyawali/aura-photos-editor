//! Vectors as catalog rows: the migration, the round trip, resumability, the
//! camera join, the storage budget and the purge.
//!
//! Everything here goes through a real SQLite catalog rather than a double. The
//! reason is the same reason the store exists at all: the properties that matter -
//! that a killed run resumes, that deleting a project takes its vectors with it,
//! that the payload fits its budget - are properties of the schema, and a double
//! would assert them about itself.

mod support;

use std::sync::Arc;

use aura_catalog::model::{CameraRow, CaptureTimeSource, PhotoRow, ProjectRow};
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::{PhotoId, ProjectId};
use aura_index::contract::index::EMBED_DIM;
use aura_index::hnsw::{HnswIndex, HnswParams};
use aura_index::store::{
    Descriptors, EmbeddingStore, StoredEmbedding, BYTES_PER_IMAGE, HISTOGRAM_BYTES, VECTOR_BYTES,
};
use tempfile::TempDir;

use support::{embedding_from, id_for};

/// Section 11's storage budget: 1.6 KB per image, where a kilobyte is 1,024 bytes.
const STORAGE_BUDGET_BYTES: usize = 1_638;

struct Fixture {
    _dir: TempDir,
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
    project: ProjectId,
    store: EmbeddingStore,
}

fn fixture(photos: usize) -> Fixture {
    let dir = TempDir::new().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(
            &dir.path().join("phase05.sqlite"),
            Arc::clone(&clock),
            "0.1.0",
        )
        .expect("open catalog"),
    );

    let project = ProjectId::new();
    let now = rfc3339(clock.now_utc());
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase-05".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
        .expect("create project");

    // Two bodies, so the camera join and the camera filter have something to do.
    for (serial, label) in [("BODY-A", "primary"), ("BODY-B", "second")] {
        let camera = CameraRow {
            camera_id: format!("cam_{}", blake3::hash(serial.as_bytes()).to_hex()),
            project_id: project.to_db(),
            make: Some("AURA".to_string()),
            model: Some("Fixture".to_string()),
            body_serial: serial.to_string(),
            shooter_label: label.to_string(),
            clock_offset_ms: 0,
            offset_confidence: 1.0,
            offset_source: "manual".to_string(),
            photo_count: 0,
        };
        let stamp = now.clone();
        catalog
            .writer()
            .transact(move |tx| repo::upsert_camera(tx, &camera, &stamp).map(|_| ()))
            .expect("upsert camera");
    }

    for n in 0..photos {
        let photo = PhotoRow {
            photo_id: id_for(n as u64 + 1).to_db(),
            project_id: project.to_db(),
            capture_time: Some(format!("2026-06-0{}T12:00:0{}Z", 1 + n / 10 % 9, n % 10)),
            capture_time_source: CaptureTimeSource::ExifOriginal,
            timeline_time: Some(format!("2026-06-0{}T12:00:0{}Z", 1 + n / 10 % 9, n % 10)),
            camera_make: Some("AURA".to_string()),
            camera_model: Some("Fixture".to_string()),
            camera_serial: Some(if n % 2 == 0 { "BODY-A" } else { "BODY-B" }.to_string()),
            lens_model: None,
            iso: Some(800),
            aperture: Some(1.8),
            shutter_us: Some(4_000),
            focal_len_mm: Some(35.0),
            flash_fired: Some(false),
            orientation: 1,
            width_px: Some(6_000),
            height_px: Some(4_000),
            sub_sec: None,
        };
        let stamp = now.clone();
        catalog
            .writer()
            .transact(move |tx| repo::insert_photo(tx, &photo, &stamp))
            .expect("insert photo");
    }

    let store = EmbeddingStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    Fixture {
        _dir: dir,
        catalog,
        clock,
        project,
        store,
    }
}

fn stored(id: PhotoId, seed: f32) -> StoredEmbedding {
    let vector: Vec<f32> = (0..EMBED_DIM)
        .map(|n| (n as f32 * 0.01 + seed).sin())
        .collect();
    let mut descriptors = Descriptors::empty(1);
    for (n, slot) in descriptors.hsv_hist.iter_mut().enumerate() {
        *slot = ((n as u32 * 7 + 3) % 256) as u8;
    }
    descriptors.luma.mean = 0.42;
    descriptors.luma.p1 = 0.03;
    descriptors.luma.p50 = 0.40;
    descriptors.luma.p99 = 0.95;
    descriptors.edge_energy = 0.11;
    descriptors.edge_p90 = 0.34;
    descriptors.palette = [
        [200, 180, 160],
        [40, 40, 48],
        [120, 90, 70],
        [10, 10, 12],
        [250, 250, 250],
    ];

    // The runner copies the histogram and the luminance statistics onto the
    // embedding as well as into the descriptors, because the frozen
    // `ImageEmbedding` carries both. The fixture does the same, so the round-trip
    // assertion compares like with like.
    let mut embedding = embedding_from(
        id,
        vector,
        0xDEAD_BEEF_0000_0000 ^ u64::from(seed.to_bits()),
    );
    embedding.hsv_hist = descriptors.hsv_hist;
    embedding.luma = descriptors.luma;

    StoredEmbedding {
        embedding,
        descriptors,
        preprocess_ver: 1,
        pixel_tier: 1,
        pixel_source: "embedded",
    }
}

#[test]
fn the_catalog_reaches_the_current_schema_and_has_both_tables() {
    // The version is the build's, not phase 05's: every later phase adds a migration and
    // this test is about *these two tables existing*, not about the catalog stopping at
    // five. Pinning the literal here would make every future migration a phase 05 failure.
    let fixture = fixture(2);
    assert_eq!(
        fixture.catalog.schema_version().expect("version"),
        aura_catalog::migrate::APP_SCHEMA_VERSION
    );
    assert_eq!(fixture.catalog.count("embeddings").expect("count"), 0);
    assert_eq!(fixture.catalog.count("descriptors").expect("count"), 0);
}

#[test]
fn the_migration_records_its_own_rollback() {
    // The rollback path is part of the Definition of Done, and a rollback that
    // lives only in an exit report is a rollback nobody finds at three in the
    // morning. Same assertion the phase 04 gate makes about migration 4.
    let sql = include_str!("../../aura-catalog/migrations/0005_embeddings.sql");
    for statement in [
        "DROP TABLE IF EXISTS descriptors;",
        "DROP TABLE IF EXISTS embeddings;",
        "DELETE FROM schema_version WHERE version = 5;",
    ] {
        assert!(
            sql.contains(statement),
            "the migration does not record `{statement}`"
        );
    }
}

#[test]
fn a_vector_survives_the_round_trip_bit_for_bit() {
    let fixture = fixture(1);
    let id = id_for(1);
    let row = stored(id, 0.25);
    fixture
        .store
        .put(&fixture.project.to_db(), &row)
        .expect("put");

    let (entries, _) = fixture
        .store
        .load_entries(&fixture.project.to_db())
        .expect("load");
    let Some(read) = entries.first() else {
        panic!("nothing came back");
    };

    assert_eq!(read.embedding.image_id, id);
    assert_eq!(read.embedding.vec, row.embedding.vec, "the vector changed");
    assert_eq!(
        read.embedding.dhash, row.embedding.dhash,
        "the hash changed"
    );
    assert_eq!(read.embedding.hsv_hist, row.embedding.hsv_hist);
    assert_eq!(read.embedding.model_ver, row.embedding.model_ver);
}

#[test]
fn a_negative_looking_hash_survives_sqlites_signed_integers() {
    // SQLite has no unsigned type. A hash with its top bit set round-trips as a
    // negative integer and must come back as the same 64 bits, not as a number.
    let fixture = fixture(1);
    let id = id_for(1);
    let mut row = stored(id, 0.5);
    row.embedding.dhash = 0xFFFF_FFFF_FFFF_FFF0;
    fixture
        .store
        .put(&fixture.project.to_db(), &row)
        .expect("put");

    let (entries, _) = fixture
        .store
        .load_entries(&fixture.project.to_db())
        .expect("load");
    assert_eq!(
        entries.first().map(|entry| entry.embedding.dhash),
        Some(0xFFFF_FFFF_FFFF_FFF0)
    );
}

#[test]
fn the_pending_set_is_what_is_left_to_do_and_shrinks_as_work_lands() {
    let fixture = fixture(10);
    let project = fixture.project.to_db();
    assert_eq!(
        fixture.store.pending(&project, 100).expect("pending").len(),
        10
    );

    for n in 0..4 {
        fixture
            .store
            .put(&project, &stored(id_for(n + 1), n as f32 * 0.1))
            .expect("put");
    }
    assert_eq!(
        fixture.store.pending(&project, 100).expect("pending").len(),
        6
    );

    // Resumability: writing the same rows again is harmless and does not change
    // what is left.
    for n in 0..4 {
        fixture
            .store
            .put(&project, &stored(id_for(n + 1), n as f32 * 0.1))
            .expect("put");
    }
    assert_eq!(
        fixture.store.pending(&project, 100).expect("pending").len(),
        6
    );
    assert_eq!(fixture.catalog.count("embeddings").expect("count"), 4);
}

#[test]
fn a_model_version_bump_makes_every_row_pending_again() {
    let fixture = fixture(6);
    let project = fixture.project.to_db();
    for n in 0..6 {
        fixture
            .store
            .put(&project, &stored(id_for(n + 1), n as f32 * 0.1))
            .expect("put");
    }
    assert!(fixture
        .store
        .pending(&project, 100)
        .expect("pending")
        .is_empty());
    assert_eq!(
        fixture.store.pending(&project, 110).expect("pending").len(),
        6
    );
    assert_eq!(
        fixture.store.model_versions(&project).expect("versions"),
        vec![100]
    );
}

#[test]
fn coverage_answers_the_question_the_debug_panel_asks() {
    let fixture = fixture(8);
    let project = fixture.project.to_db();
    assert_eq!(fixture.store.coverage(&project).expect("coverage"), (8, 0));
    for n in 0..3 {
        fixture
            .store
            .put(&project, &stored(id_for(n + 1), n as f32 * 0.2))
            .expect("put");
    }
    assert_eq!(fixture.store.coverage(&project).expect("coverage"), (8, 3));
}

#[test]
fn loading_assigns_camera_handles_from_the_catalog_and_the_index_resolves_them() {
    let fixture = fixture(6);
    let project = fixture.project.to_db();
    for n in 0..6 {
        fixture
            .store
            .put(&project, &stored(id_for(n + 1), n as f32 * 0.15))
            .expect("put");
    }

    let (entries, labels) = fixture.store.load_entries(&project).expect("load");
    assert_eq!(entries.len(), 6);
    assert_eq!(labels.len(), 2, "two bodies were inserted");
    assert!(
        labels.windows(2).all(|pair| pair.first() < pair.get(1)),
        "labels must be sorted, so a handle is stable across runs"
    );

    // Both handles are used, and every frame has one.
    let mut seen = std::collections::BTreeSet::new();
    for entry in &entries {
        let Some(camera) = entry.meta.camera else {
            panic!("{} has no camera handle", entry.embedding.image_id);
        };
        seen.insert(camera.0);
        assert!(entry.meta.timeline_ms.is_some(), "no timeline stamp");
    }
    assert_eq!(seen.len(), 2);

    let index = HnswIndex::build(entries, HnswParams::default(), fixture.clock.as_ref());
    index.set_camera_labels(&labels);
    let Some(first) = labels.first() else {
        panic!("no labels");
    };
    assert_eq!(
        index.camera_handle(first),
        Some(aura_index::CameraId(0)),
        "the first sorted label is handle zero"
    );
    assert_eq!(index.camera_handle("cam_not_in_this_project"), None);
}

#[test]
fn one_image_costs_less_than_the_section_11_storage_budget() {
    assert_eq!(VECTOR_BYTES, 1_024);
    assert_eq!(HISTOGRAM_BYTES, 512);
    // Read through a local, so the assertion is about the constants rather than a
    // comparison the compiler folds away and clippy rightly calls out.
    let measured = BYTES_PER_IMAGE;
    let budget = STORAGE_BUDGET_BYTES;
    assert!(
        measured <= budget,
        "an image costs {measured} bytes and the budget is {budget}"
    );
}

#[test]
fn purging_a_version_takes_its_descriptors_with_it() {
    let fixture = fixture(5);
    let project = fixture.project.to_db();
    for n in 0..5 {
        fixture
            .store
            .put(&project, &stored(id_for(n + 1), n as f32 * 0.1))
            .expect("put");
    }
    assert_eq!(fixture.catalog.count("descriptors").expect("count"), 5);

    let removed = fixture.store.purge_version(&project, 100).expect("purge");
    assert_eq!(removed, 5);
    assert_eq!(fixture.catalog.count("embeddings").expect("count"), 0);
    assert_eq!(
        fixture.catalog.count("descriptors").expect("count"),
        0,
        "descriptors outlived their vectors"
    );
    assert_eq!(
        fixture.store.pending(&project, 100).expect("pending").len(),
        5
    );
}

#[test]
fn deleting_a_project_takes_its_vectors_with_it() {
    // Privacy, by foreign key rather than by a cleanup pass somebody has to
    // remember to run. ADR-0011 section 9.
    let fixture = fixture(4);
    let project = fixture.project.to_db();
    for n in 0..4 {
        fixture
            .store
            .put(&project, &stored(id_for(n + 1), n as f32 * 0.1))
            .expect("put");
    }
    assert_eq!(fixture.catalog.count("embeddings").expect("count"), 4);

    let id = project.clone();
    fixture
        .catalog
        .writer()
        .transact(move |tx| {
            tx.execute("DELETE FROM project WHERE project_id = ?1", [&id])
                .map(|_| ())
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not delete the project", &e)
                })
        })
        .expect("delete project");

    assert_eq!(fixture.catalog.count("embeddings").expect("count"), 0);
    assert_eq!(fixture.catalog.count("descriptors").expect("count"), 0);
    fixture.catalog.integrity_check().expect("integrity");
}
