//! The export pass end to end: render, resize, sharpen, encode, write, read back, seal.
//!
//! Driven by authored plates rather than by a renderer, which is what makes it a test of the
//! *export* and not of phase 14. What it proves is the loop's contract: every file is read back,
//! every name is unique, a frame that will not render is skipped rather than fatal, and a manifest
//! is sealed only when nothing was skipped.

use std::path::PathBuf;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::SystemClock;
use aura_core::contract::delivery::{
    DeliveryCode, DeliveryColour, Destination, ExportJob, ExportService, ExportSet, FileFormat,
    NamingTemplate, OutputSharpen, Resize,
};
use aura_core::ProjectId;
use aura_export::api::{Export, ExportPass};
use aura_export::fixtures::{wedding, Plate, ScriptedField, ScriptedSource};
use aura_export::read::Frame;
use aura_export::store::ExportStore;
use aura_export::verify::hash_file;

/// A catalog with one project in it, in a temporary directory.
fn catalog(dir: &std::path::Path) -> (Arc<Catalog>, ProjectId) {
    let clock: Arc<dyn aura_core::clock::Clock> = Arc::new(SystemClock::default());
    let catalog =
        Arc::new(Catalog::open(&dir.join("catalog.db"), clock, "0.1.0").expect("open catalog"));
    let project = ProjectId::new();
    let key = project.to_db();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'test wedding', '2026-05-16T00:00:00Z', '2026-05-16T00:00:00Z')",
                rusqlite::params![key],
            )
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))
        })
        .expect("insert project");
    (catalog, project)
}

/// Photographs have to exist for `export_file`'s foreign key. Phase 26's lesson: a fixture that
/// seeds a project but not its rows passes every unit test and fails the first gate.
fn seed_photos(catalog: &Arc<Catalog>, project: ProjectId, images: &[aura_core::PhotoId]) {
    let key = project.to_db();
    let ids: Vec<String> = images.iter().map(|i| i.to_db()).collect();
    catalog
        .writer()
        .with(move |conn| {
            for (i, id) in ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                         created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, ?4, ?4)",
                    rusqlite::params![
                        id,
                        key,
                        1_760_000_000_000_i64 + i as i64,
                        "2026-05-16T00:00:00Z"
                    ],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .expect("insert photos");
}

fn one_set(images: Vec<aura_core::PhotoId>, root: PathBuf, template: &str) -> ExportJob {
    ExportJob::new(
        vec![ExportSet {
            name: "gallery".to_owned(),
            images,
            format: FileFormat::Jpeg,
            quality: 92,
            resize: Resize::Full,
            sharpen: OutputSharpen::None,
            naming: NamingTemplate::parse(template).expect("template"),
            colour: DeliveryColour::Srgb,
            bit_depth: 8,
            sidecar: false,
        }],
        Destination::Folder { path: root },
    )
}

#[test]
fn a_job_writes_every_file_reads_each_one_back_and_seals_a_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(6);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(Some("Alex & Sam"), 200, 6);
    let source = ScriptedSource::new(Plate::Gradient, 64, 48);
    let out = dir.path().join("delivery");
    let job = one_set(images.clone(), out.clone(), "{couple}_{seq}");

    let pass = ExportPass::new(&store, &field, &source, "0.1.0");
    let result = pass.run(project, &job).expect("run");

    assert_eq!(result.files.len(), 6);
    assert!(result.skipped.is_empty());
    let manifest = result.manifest.expect("sealed");
    assert_eq!(manifest.files.len(), 6);
    assert!(manifest.fully_hashed(), "every file carries a real digest");

    // Every file is on disk, and its stored digest is the digest of what is on disk.
    for file in &result.files {
        let path = out.join(&file.path);
        assert!(path.exists(), "{} missing", path.display());
        assert!(file.verified);
        assert_eq!(
            file.hash,
            hash_file(&path).expect("hash"),
            "{:?}",
            file.path
        );
        assert!(file
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::WrittenAndVerified));
    }

    // The travelling manifest is beside them and is itself readable.
    let doc = out.join(aura_core::contract::delivery::MANIFEST_NAME);
    assert!(doc.exists());
    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&doc).expect("read")).expect("json");
    assert_eq!(parsed["file_count"], 6);
    assert_eq!(parsed["verified"], true);

    // And the catalog agrees with the disk.
    let service = Export::new(store.clone(), 200, 6);
    let outline = service.outline(project).expect("outline");
    assert_eq!(outline.written, 6);
    assert_eq!(outline.verified, 6);
    assert_eq!(outline.unverified, 0);
    assert_eq!(outline.requested, 6);
    assert_eq!(outline.photos, 200);
    assert_eq!(outline.selected, 6);
    assert!(outline.manifest_sealed);
    assert!((outline.verified_share() - 1.0).abs() < 1e-6);
    assert_eq!(service.files(project).expect("files").len(), 6);
    assert!(service.manifest(project).expect("manifest").is_some());
}

#[test]
fn a_frame_that_will_not_render_is_skipped_and_the_manifest_is_not_sealed() {
    // Item-level, not fatal: a wedding with one unreadable original delivers the other frames.
    // And the manifest is *not* sealed, because a partial manifest is a document that says a
    // wedding was delivered when part of it was not.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(4);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(Some("Alex"), 4, 4);
    let source = ScriptedSource::new(Plate::Flat, 32, 32).failing(images[2]);
    let out = dir.path().join("delivery");
    let job = one_set(images.clone(), out, "{seq}");

    let result = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect("run");

    assert_eq!(result.files.len(), 3);
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].0, images[2]);
    assert!(
        result.manifest.is_none(),
        "a partial delivery seals nothing"
    );
}

#[test]
fn two_cameras_with_the_same_original_name_both_arrive() {
    // Section 10.1's naming row, end to end rather than in the planner.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(2);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let mut field = ScriptedField::new(None, 2, 2);
    for id in &images {
        field = field.with_frame(
            *id,
            Frame {
                image: Some(*id),
                original_stem: Some("DSC_0431".to_owned()),
                ..Frame::default()
            },
        );
    }
    let source = ScriptedSource::new(Plate::Flat, 16, 16);
    let out = dir.path().join("delivery");
    let job = one_set(images, out.clone(), "{original}");

    let result = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect("run");

    assert_eq!(result.files.len(), 2);
    assert!(out.join("gallery/DSC_0431.jpg").exists());
    assert!(out.join("gallery/DSC_0431_2.jpg").exists());
    assert!(result.files.iter().any(|f| f.renamed));
    assert!(result.files.iter().any(|f| f
        .reasons
        .iter()
        .any(|r| r.code == DeliveryCode::NameCollisionResolved)));
}

#[test]
fn a_resized_set_writes_smaller_files_and_never_larger_ones() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(2);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(None, 2, 2);
    let source = ScriptedSource::new(Plate::Gradient, 1200, 900);
    let out = dir.path().join("delivery");

    // 240 is MIN_LONG_EDGE: the smallest output the contract will write. Asking for less is
    // refused at validation, which is the bound working rather than a limitation to route around.
    let mut job = one_set(images.clone(), out, "{seq}");
    job.sets[0].resize = Resize::LongEdge { pixels: 240 };
    job.sets[0].sharpen = OutputSharpen::Screen;

    let result = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect("run");
    for f in &result.files {
        assert_eq!((f.width, f.height), (240, 180));
        assert!(f
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::SharpenedForOutput));
    }

    // Asking for more than the frame has writes the frame at its own size and says so.
    let mut job = one_set(images, dir.path().join("delivery2"), "{seq}");
    job.sets[0].resize = Resize::LongEdge { pixels: 8000 };
    let result = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect("run");
    for f in &result.files {
        assert_eq!((f.width, f.height), (1200, 900));
        assert!(f
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::ResizeIgnoredUpscale));
    }
}

#[test]
fn an_unverified_job_says_so_on_every_file_and_in_the_manifest() {
    // ADR-0061 decision 2: the rule is not "verification cannot be switched off", it is that a
    // delivery which was not verified can never look like one that was.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(3);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(None, 3, 3);
    let source = ScriptedSource::new(Plate::Flat, 16, 16);
    let out = dir.path().join("delivery");
    let mut job = one_set(images, out.clone(), "{seq}");
    job.verify = false;

    let result = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect("run");
    for f in &result.files {
        assert!(!f.verified);
        assert!(f.hash.is_empty(), "no digest without a read-back");
        assert!(f
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::WrittenUnverified));
    }

    let outline = Export::new(store, 3, 3).outline(project).expect("outline");
    assert_eq!(outline.verified, 0);
    assert_eq!(outline.unverified, 3);

    let doc = out.join(aura_core::contract::delivery::MANIFEST_NAME);
    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&doc).expect("read")).expect("json");
    assert_eq!(parsed["verified"], false);
}

#[test]
fn a_destination_that_cannot_be_written_to_fails_before_any_pixel_is_rendered() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(2);
    seed_photos(&catalog, project, &images);

    // A file where the destination directory should be.
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, b"x").expect("write");

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(None, 2, 2);
    let source = ScriptedSource::new(Plate::Flat, 16, 16);
    let job = one_set(images, blocked, "{seq}");

    let err = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect_err("refused");
    assert_eq!(err.code.0, "AURA-RENDER-8023");
    // Nothing was opened: no job row exists for this project.
    assert!(store.latest_job(project).expect("latest").is_none());
}

#[test]
fn a_sidecar_is_written_beside_the_file_when_the_set_asks_for_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(1);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(None, 1, 1).with_frame(
        images[0],
        Frame {
            image: Some(images[0]),
            recipe_json: Some("<x:xmpmeta>the edit</x:xmpmeta>".to_owned()),
            ..Frame::default()
        },
    );
    let source = ScriptedSource::new(Plate::Flat, 16, 16);
    let out = dir.path().join("delivery");
    let mut job = one_set(images, out.clone(), "{seq}");
    job.sets[0].sidecar = true;

    let result = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect("run");
    assert!(out.join("gallery/0001.xmp").exists());
    assert!(result.files[0]
        .reasons
        .iter()
        .any(|r| r.code == DeliveryCode::SidecarWritten));
}

#[test]
fn a_cleanup_disclosure_reaches_the_manifest_the_client_receives() {
    // A removal that is not disclosed in the thing handed to the client is a removal nobody can
    // audit, and this is the thing handed to the client.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(1);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(None, 1, 1).with_frame(
        images[0],
        Frame {
            image: Some(images[0]),
            cleanup_disclosures: vec!["an exit sign was removed from the background".to_owned()],
            ..Frame::default()
        },
    );
    let source = ScriptedSource::new(Plate::Flat, 16, 16);
    let out = dir.path().join("delivery");
    let job = one_set(images, out.clone(), "{seq}");

    let result = ExportPass::new(&store, &field, &source, "0.1.0")
        .run(project, &job)
        .expect("run");
    let manifest = result.manifest.expect("sealed");
    assert_eq!(manifest.cleanup_disclosures.len(), 1);

    let doc = std::fs::read_to_string(out.join(aura_core::contract::delivery::MANIFEST_NAME))
        .expect("read");
    assert!(doc.contains("an exit sign was removed"));
}

#[test]
fn the_store_refuses_a_file_that_claims_to_be_verified_with_no_digest() {
    // `export_file_verified_needs_a_hash`. The two columns can express "verified with no hash",
    // and that is the one combination that would make a manifest a lie. A control runs first, so
    // a refusal caused by a broken fixture cannot read as the promise working - phase 21's rule.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images = wedding(1);
    seed_photos(&catalog, project, &images);

    let store = ExportStore::new(Arc::clone(&catalog));
    let field = ScriptedField::new(None, 1, 1);
    let job = one_set(images.clone(), dir.path().join("d"), "{seq}");
    let job_id = store
        .open_job(project, &job, "0.1.0", &[])
        .expect("open job");
    let _ = field;

    let good = aura_core::contract::delivery::ExportedFile {
        image: images[0],
        set: "gallery".to_owned(),
        path: PathBuf::from("gallery/0001.jpg"),
        bytes: 10,
        hash: "a".repeat(64),
        width: 16,
        height: 16,
        render_hash: "b".repeat(64),
        verified: true,
        renamed: false,
        reasons: Vec::new(),
    };
    // The control: the same row with a real digest is accepted, so a failure below is the trigger
    // rather than a foreign key.
    store.write_file(&job_id, &good).expect("control accepted");

    let bad = aura_core::contract::delivery::ExportedFile {
        path: PathBuf::from("gallery/0002.jpg"),
        hash: String::new(),
        ..good
    };
    assert!(
        store.write_file(&job_id, &bad).is_err(),
        "a verified file with no digest must be refused"
    );
}
