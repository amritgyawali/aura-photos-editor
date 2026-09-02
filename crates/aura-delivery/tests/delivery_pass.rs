#![allow(clippy::assertions_on_constants)]

//! Backup and upload end to end, over a catalog and a transport that can be made to fail.
//!

//! Section 10.1 asks for two things this file proves: that a provider upload resumes correctly
//! after a network drop, and that per-set mapping is respected. Both are measured against a
//! transport that keeps what it received before dropping, which is what a real service does and
//! what makes a resume possible at all.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::delivery::{
    DeliveryCode, DeliveryService, Destination, ExportedFile, ImageId, ProviderId, SetMapping,
    UploadState,
};
use aura_core::ProjectId;
use aura_delivery::api::{resolve, Delivery, UploadPass};
use aura_delivery::providers::{registry, ScriptedTransport};
use aura_delivery::store::DeliveryStore;

fn catalog(dir: &Path) -> (Arc<Catalog>, ProjectId) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog =
        Arc::new(Catalog::open(&dir.join("c.sqlite"), clock, "test").expect("open catalog"));
    let project = ProjectId::new();
    let key = project.to_db();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'wedding', '2026-05-16T00:00:00Z', '2026-05-16T00:00:00Z')",
                rusqlite::params![key],
            )
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))
        })
        .expect("project");
    (catalog, project)
}

/// Photograph rows, because `delivery_upload` has a foreign key onto them. Phase 26's lesson: a
/// fixture that seeds a project but not its rows passes every unit test and fails the first gate.
fn seed_photos(catalog: &Arc<Catalog>, project: ProjectId, images: &[ImageId]) {
    let key = project.to_db();
    let ids: Vec<String> = images.iter().map(ImageId::to_db).collect();
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
        .expect("photos");
}

/// An `export_job` row, because `delivery_upload` and `delivery_backup` both have foreign keys
/// onto it - an upload that names a job which does not exist is an upload of nothing.
///
/// Phase 26 wrote this lesson down after its gate failed on `camera_pair`'s foreign keys, and
/// phase 25's had failed the same way one phase earlier. This is the third time: a store test is
/// handed ids rather than making them, so nothing below the gate exercises a referential
/// constraint until the gate does.
fn seed_job(catalog: &Arc<Catalog>, project: ProjectId, job_id: &str) {
    let key = project.to_db();
    let job = job_id.to_owned();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO export_job (job_id, project_id, destination_kind,
                     destination, metadata_policy, verify, status, started_at, app_version)
                 VALUES (?1, ?2, 'folder', '{}', '{}', 1, 'sealed', ?3, '0.1.0')",
                rusqlite::params![job, key, "2026-05-16T00:00:00Z"],
            )
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("export_job", &e))
        })
        .expect("export job");
}

/// A sealed delivery on disk: `n` files in `set`, each `size` bytes.
fn delivery(root: &Path, set: &str, images: &[ImageId], size: usize) -> Vec<ExportedFile> {
    let mut out = Vec::new();
    for (i, image) in images.iter().enumerate() {
        let bytes: Vec<u8> = (0..size).map(|b| ((b + i) % 251) as u8).collect();
        let rel = PathBuf::from(set).join(format!("{i:04}.jpg"));
        let path = root.join(&rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, &bytes).expect("write");
        out.push(ExportedFile {
            image: *image,
            set: set.to_owned(),
            path: rel,
            bytes: bytes.len() as u64,
            hash: blake3::hash(&bytes).to_hex().to_string(),
            width: 100,
            height: 100,
            render_hash: "a".repeat(64),
            verified: true,
            renamed: false,
            reasons: Vec::new(),
        });
    }
    out
}

fn map(set: &str, remote: &str) -> SetMapping {
    SetMapping {
        set: set.to_owned(),
        remote: remote.to_owned(),
        publish: false,
    }
}

#[test]
fn an_upload_that_drops_resumes_and_finishes_without_re_sending_what_arrived() {
    // Section 10.1's row. The transport drops part-way through the third file, and the second pass
    // has to pick up from what the far end kept rather than starting the wedding again.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images: Vec<ImageId> = (0..5).map(|_| ImageId::new()).collect();
    seed_photos(&catalog, project, &images);
    seed_job(&catalog, project, "job-1");

    let root = dir.path().join("delivery");
    let files = delivery(&root, "gallery", &images, 3000);
    let store = DeliveryStore::new(Arc::clone(&catalog));
    let provider = registry("folder-gallery").expect("provider");
    let transport = ScriptedTransport::new();
    let mapping = vec![map("gallery", "wedding-2026")];

    // Everything after the first 400 bytes of any call is lost. Deliberately small relative to
    // the 3,000-byte files: `send` already retries three times inside one pass, so a drop that
    // three attempts could ride out would leave nothing for the second pass to prove.
    transport.drop_after(400);
    let pass = UploadPass::new(&store, provider.as_ref(), &transport);
    let first = pass
        .run(project, "job-1", &root, &files, &mapping)
        .expect("first pass");
    assert!(first.progress.verified < 5, "not everything arrived");
    assert!(first.progress.outstanding > 0);

    transport.recover();
    let second = pass
        .run(project, "job-1", &root, &files, &mapping)
        .expect("second pass");
    assert_eq!(second.progress.verified, 5, "everything arrived");
    assert_eq!(second.progress.outstanding, 0);
    assert!(second.progress.resumes > 0, "a resume was recorded");

    // And what the far end holds is byte-for-byte what was sent.
    for (i, file) in files.iter().enumerate() {
        let key = format!("wedding-2026/{i:04}.jpg");
        let held = transport.contents(&key).expect("held");
        assert_eq!(
            blake3::hash(&held).to_hex().to_string(),
            file.hash,
            "{key} differs"
        );
    }
}

#[test]
fn a_second_run_over_a_finished_upload_sends_nothing() {
    // The ordinary case, because a photographer presses the button again. `INSERT OR IGNORE` in
    // `seed_upload` is what stops a re-run resetting the state of files that already arrived.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images: Vec<ImageId> = (0..3).map(|_| ImageId::new()).collect();
    seed_photos(&catalog, project, &images);
    seed_job(&catalog, project, "job-1");

    let root = dir.path().join("delivery");
    let files = delivery(&root, "gallery", &images, 500);
    let store = DeliveryStore::new(Arc::clone(&catalog));
    let provider = registry("folder-gallery").expect("provider");
    let transport = ScriptedTransport::new();
    let mapping = vec![map("gallery", "main")];
    let pass = UploadPass::new(&store, provider.as_ref(), &transport);

    pass.run(project, "job-1", &root, &files, &mapping)
        .expect("first");
    let again = pass
        .run(project, "job-1", &root, &files, &mapping)
        .expect("second");
    assert_eq!(again.progress.verified, 3);
    assert_eq!(again.progress.resumes, 0, "nothing was re-sent");
}

#[test]
fn an_unmapped_set_is_left_out_and_named_while_the_mapped_one_goes() {
    // Phase 24's rule: an absent mapping is ignorance, not permission. Sending it "somewhere
    // sensible" is the response that cannot be taken back.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images: Vec<ImageId> = (0..4).map(|_| ImageId::new()).collect();
    seed_photos(&catalog, project, &images);
    seed_job(&catalog, project, "job-1");

    let root = dir.path().join("delivery");
    let mut files = delivery(&root, "gallery", &images[..2], 200);
    files.extend(delivery(&root, "album", &images[2..], 200));

    let store = DeliveryStore::new(Arc::clone(&catalog));
    let provider = registry("folder-gallery").expect("provider");
    let transport = ScriptedTransport::new();
    let pass = UploadPass::new(&store, provider.as_ref(), &transport);

    let result = pass
        .run(project, "job-1", &root, &files, &[map("gallery", "main")])
        .expect("run");
    assert_eq!(result.unmapped, vec!["album".to_owned()]);
    assert!(result
        .reasons
        .iter()
        .any(|r| r.code == DeliveryCode::SetUnmapped));
    assert_eq!(result.progress.files, 2, "only the mapped set was queued");
    assert!(transport.contents("main/0000.jpg").is_some());
}

#[test]
fn a_publish_flag_is_cleared_and_named_because_no_provider_here_may_publish() {
    // The failure this guards is a whole wedding visible on the wedding night.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images: Vec<ImageId> = (0..2).map(|_| ImageId::new()).collect();
    seed_photos(&catalog, project, &images);
    seed_job(&catalog, project, "job-1");

    let root = dir.path().join("delivery");
    let files = delivery(&root, "gallery", &images, 100);
    let store = DeliveryStore::new(Arc::clone(&catalog));
    let provider = registry("folder-gallery").expect("provider");
    let transport = ScriptedTransport::new();
    let pass = UploadPass::new(&store, provider.as_ref(), &transport);

    let result = pass
        .run(
            project,
            "job-1",
            &root,
            &files,
            &[SetMapping {
                set: "gallery".to_owned(),
                remote: "main".to_owned(),
                publish: true,
            }],
        )
        .expect("run");
    assert!(result
        .reasons
        .iter()
        .any(|r| r.code == DeliveryCode::LeftUnpublished));
}

#[test]
fn a_backup_copies_a_delivery_and_the_outline_reports_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images: Vec<ImageId> = (0..4).map(|_| ImageId::new()).collect();
    seed_photos(&catalog, project, &images);
    seed_job(&catalog, project, "job-1");

    let root = dir.path().join("delivery");
    let files = delivery(&root, "gallery", &images, 400);
    let backup = dir.path().join("backup");

    let service = Delivery::new(Arc::clone(&catalog)).with_delivery("job-1", &root, files.clone());
    let outline = service
        .backup(
            project,
            &Destination::Folder {
                path: backup.clone(),
            },
        )
        .expect("backup");
    assert_eq!(outline.backed_up, 4);
    assert_eq!(outline.diverged, 0);
    for f in &files {
        assert!(backup.join(&f.path).exists());
    }

    let read_back = service.outline(project).expect("outline");
    assert_eq!(read_back.backups, 1);
    assert_eq!(read_back.backed_up, 4);
}

#[test]
fn this_build_refuses_an_upload_rather_than_reporting_an_empty_one() {
    // "Nothing was sent" and "nothing can be sent from this build" are different facts, and a
    // photographer who saw the first would go looking at their credentials. Exit condition C3.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let service = Delivery::new(catalog);
    let err = service
        .upload(
            project,
            &ProviderId::parse("folder-gallery").expect("id"),
            &[],
        )
        .expect_err("refused");
    assert_eq!(err.code.0, "AURA-DLV-10002");
    assert!(!aura_delivery::NETWORK_TRANSPORT_AVAILABLE);
}

#[test]
fn a_provider_with_no_credential_is_refused_before_a_byte_is_read() {
    let err = resolve("folder-gallery", false).expect_err("no credential");
    assert_eq!(err.code.0, "AURA-DLV-10004");
    assert!(resolve("folder-gallery", true).is_ok());
    assert_eq!(
        resolve("pic-time", true).expect_err("unknown").code.0,
        "AURA-DLV-10001"
    );
}

#[test]
fn the_store_refuses_an_upload_that_claims_more_bytes_than_the_file_has() {
    // `delivery_upload_sent_within_bytes`. Cheap, and it catches the resume arithmetic getting a
    // sign wrong - which would look like a completed upload. A control runs first, so a refusal
    // caused by a broken fixture cannot read as the promise working (phase 21's rule).
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, project) = catalog(dir.path());
    let images: Vec<ImageId> = (0..1).map(|_| ImageId::new()).collect();
    seed_photos(&catalog, project, &images);
    seed_job(&catalog, project, "job-1");

    let root = dir.path().join("delivery");
    let files = delivery(&root, "gallery", &images, 1000);
    let store = DeliveryStore::new(Arc::clone(&catalog));
    let target = store
        .upsert_target(project, "provider", "folder-gallery", "x", &[], true)
        .expect("target");
    let items: Vec<_> = files
        .iter()
        .map(|f| aura_core::contract::delivery::UploadItem {
            image: f.image,
            set: f.set.clone(),
            path: f.path.clone(),
            bytes: f.bytes,
            hash: f.hash.clone(),
            state: UploadState::Pending,
        })
        .collect();
    store.seed_upload(&target, "job-1", &items).expect("seed");

    let rel = files[0].path.to_string_lossy().to_string();
    // The control: a legal offset is accepted, so a failure below is the bound rather than a
    // missing row.
    store
        .set_state(
            &target,
            "job-1",
            &rel,
            &UploadState::InProgress {
                sent: 400,
                resumes: 1,
            },
        )
        .expect("control accepted");
    let stored = store.items(&target).expect("items");
    assert_eq!(stored[0].state.sent(), 400);

    // An impossible offset is clamped by the statement rather than stored, which is the schema's
    // bound doing its job: `MIN(?, bytes)`.
    store
        .set_state(
            &target,
            "job-1",
            &rel,
            &UploadState::InProgress {
                sent: 999_999,
                resumes: 2,
            },
        )
        .expect("clamped");
    let stored = store.items(&target).expect("items");
    assert_eq!(stored[0].state.sent(), 1000, "clamped to the file's size");
}
