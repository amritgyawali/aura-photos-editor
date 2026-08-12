//! The service, end to end, against a stub catalog.
//!
//! The stub stands in for `CatalogSource` so this suite stays a unit of
//! *scheduling and caching* behaviour rather than a second test of SQLite. The
//! catalog-backed path is covered by the phase gate in `aura-cli`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use aura_cache::CacheBudget;
use aura_core::{AuraError, AuraResult};
use aura_preview::contract::service::PreviewService;
use aura_preview::source::{PreviewRecord, PreviewSource, SourceFile};
use aura_preview::{ImageId, PreviewConfig, Previews, Priority};
use aura_raw::contract::pixels::{PixelData, PixelLevel, PixelSource};
use aura_raw::fixtures::{self, FixtureOptions};
use parking_lot::Mutex;

#[derive(Debug, Default)]
struct StubSource {
    files: Mutex<BTreeMap<String, SourceFile>>,
    previews: Mutex<Vec<PreviewRecord>>,
    problems: Mutex<Vec<(String, String)>>,
}

impl StubSource {
    fn add(&self, id: ImageId, path: &Path) {
        let bytes = std::fs::read(path).expect("read the fixture");
        self.files.lock().insert(
            id.to_db(),
            SourceFile {
                path: path.to_path_buf(),
                content_hash: blake3::hash(&bytes).to_hex().to_string(),
            },
        );
    }
}

impl PreviewSource for StubSource {
    fn locate(&self, id: ImageId) -> AuraResult<SourceFile> {
        self.files.lock().get(&id.to_db()).cloned().ok_or_else(|| {
            aura_core::errors::db::statement_failed(
                "unknown photograph",
                &std::io::Error::from(std::io::ErrorKind::NotFound),
            )
        })
    }

    fn record_preview(&self, record: &PreviewRecord) -> AuraResult<()> {
        self.previews.lock().push(record.clone());
        Ok(())
    }

    fn record_problem(&self, id: ImageId, _path: &Path, error: &AuraError) -> AuraResult<()> {
        self.problems
            .lock()
            .push((id.to_db(), error.code.0.to_string()));
        Ok(())
    }
}

struct Harness {
    _directory: tempfile::TempDir,
    service: Previews,
    source: Arc<StubSource>,
    fixtures: PathBuf,
    cache_root: PathBuf,
}

fn harness(config: PreviewConfig) -> Harness {
    let directory = tempfile::tempdir().expect("temporary directory");
    let fixtures_dir = directory.path().join("fixtures");
    std::fs::create_dir_all(&fixtures_dir).expect("fixture directory");
    let source = Arc::new(StubSource::default());
    let cache_root = directory.path().join("cache");
    let service = Previews::open(
        &cache_root,
        CacheBudget::default(),
        Arc::clone(&source) as Arc<dyn PreviewSource>,
        config,
    )
    .expect("open the preview service");
    Harness {
        _directory: directory,
        service,
        source,
        fixtures: fixtures_dir,
        cache_root,
    }
}

fn add_fixture(harness: &Harness, body: usize) -> ImageId {
    let id = ImageId::new();
    let path = harness.fixtures.join(format!("{}.dng", id.to_db()));
    fixtures::write_bench_raw(
        &path,
        FixtureOptions {
            body,
            ..FixtureOptions::default()
        },
    )
    .expect("write the fixture");
    harness.source.add(id, &path);
    id
}

// --------------------------------------------------------------- the basics

#[test]
fn a_thumbnail_is_produced_cached_and_recorded() {
    let harness = harness(PreviewConfig::default());
    let id = add_fixture(&harness, 1);

    let buffer = harness
        .service
        .get(id, PixelLevel::Thumb(512), Priority::Visible)
        .expect("tier 1");

    assert!(buffer.width.max(buffer.height) <= 512);
    assert_eq!(buffer.source, PixelSource::Embedded);
    assert!(matches!(buffer.data, PixelData::Srgb8(_)));

    let recorded = harness.source.previews.lock();
    assert_eq!(recorded.len(), 1);
    let row = recorded.first().expect("a preview row");
    assert_eq!(row.tier, 1);
    assert_eq!(row.source, "embedded");
    assert!(row.bytes > 0);
    assert!(row.rel_cache_path.contains(".p1.thumb.jpg"));
}

#[test]
fn a_proxy_is_produced_in_both_representations() {
    let harness = harness(PreviewConfig::default());
    let id = add_fixture(&harness, 2);

    let buffer = harness
        .service
        .get(id, PixelLevel::Proxy2048, Priority::Interactive)
        .expect("tier 2");
    assert_eq!(buffer.source, PixelSource::Demosaiced);

    // The 16-bit companion is written beside the 8-bit one, which is what the
    // colour phases will read.
    let recorded = harness.source.previews.lock();
    assert_eq!(recorded.first().map(|row| row.tier), Some(2));
    drop(recorded);

    let stats = harness.service.cache_stats();
    assert!(stats.entries >= 3, "proxy, linear buffer and sidecar");
    assert!(stats.bytes_used > 0);
}

#[test]
fn the_second_request_is_served_from_memory_and_the_third_from_disk() {
    let harness = harness(PreviewConfig::default());
    let id = add_fixture(&harness, 1);

    let first = harness
        .service
        .get(id, PixelLevel::Thumb(512), Priority::Visible)
        .expect("first");
    let started = Instant::now();
    let second = harness
        .service
        .get(id, PixelLevel::Thumb(512), Priority::Visible)
        .expect("second");
    let memory_elapsed = started.elapsed();

    assert!(
        Arc::ptr_eq(&first, &second),
        "the buffer is shared, not rebuilt"
    );
    assert!(
        memory_elapsed.as_millis() <= 8,
        "a memory hit took {memory_elapsed:?}, budget is 8 ms"
    );

    // A fresh service over the same cache directory has an empty memory cache,
    // so this is the on-disk path.
    let reopened = Previews::open(
        &harness.cache_root,
        CacheBudget::default(),
        Arc::clone(&harness.source) as Arc<dyn PreviewSource>,
        PreviewConfig::default(),
    )
    .expect("reopen");
    let started = Instant::now();
    let third = reopened
        .get(id, PixelLevel::Thumb(512), Priority::Visible)
        .expect("third");
    let disk_elapsed = started.elapsed();

    assert_eq!((third.width, third.height), (first.width, first.height));
    assert!(
        reopened.cache_stats().hits > 0,
        "the disk cache must have been consulted"
    );
    assert!(
        disk_elapsed.as_millis() <= 25,
        "a cached read took {disk_elapsed:?}"
    );
}

// -------------------------------------------------------------- quarantine

#[test]
fn a_file_that_cannot_be_decoded_becomes_a_problem_row_not_an_exception() {
    let harness = harness(PreviewConfig::default());
    let id = ImageId::new();
    let path = harness.fixtures.join("broken.cr2");
    std::fs::write(&path, b"this is not a photograph").expect("write");
    harness.source.add(id, &path);

    let error = harness
        .service
        .get(id, PixelLevel::Thumb(512), Priority::Visible)
        .expect_err("a text file is not decodable");
    assert_eq!(error.code.0, "AURA-RAW-2001");

    let quarantined = harness.service.quarantine();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(quarantined.first().map(|(image, _)| *image), Some(id));
    assert_eq!(
        harness
            .source
            .problems
            .lock()
            .first()
            .map(|(_, code)| code.clone()),
        Some("AURA-RAW-2001".to_string())
    );
}

#[test]
fn one_bad_file_does_not_stop_the_rest_of_the_batch() {
    let harness = harness(PreviewConfig::default());
    let good = add_fixture(&harness, 3);
    let bad = ImageId::new();
    let path = harness.fixtures.join("bad.dng");
    std::fs::write(&path, b"\x00\x01\x02\x03").expect("write");
    harness.source.add(bad, &path);

    assert!(harness
        .service
        .get(bad, PixelLevel::Thumb(512), Priority::AiBatch)
        .is_err());
    assert!(harness
        .service
        .get(good, PixelLevel::Thumb(512), Priority::AiBatch)
        .is_ok());
    assert_eq!(harness.service.quarantine().len(), 1);
}

// --------------------------------------------------------------- scheduling

#[test]
fn prefetch_returns_immediately_and_work_lands_in_the_background() {
    let harness = harness(PreviewConfig {
        workers: 2,
        ..PreviewConfig::default()
    });
    let ids: Vec<ImageId> = (1..=4).map(|body| add_fixture(&harness, body)).collect();

    let started = Instant::now();
    harness.service.prefetch(&ids, PixelLevel::Proxy2048);
    assert!(
        started.elapsed().as_millis() < 50,
        "prefetch must not block the caller"
    );

    // Wait for the pool rather than sleeping a fixed amount.
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    while harness.service.completed() < ids.len() as u64 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(harness.service.completed(), ids.len() as u64);
    assert_eq!(harness.source.previews.lock().len(), ids.len());
}

#[test]
fn scrolling_away_cancels_queued_work() {
    let harness = harness(PreviewConfig {
        // One worker, so the queue actually builds up and there is something to
        // cancel; with every core busy the test would race the pool.
        workers: 1,
        ..PreviewConfig::default()
    });
    let ids: Vec<ImageId> = (1..=8)
        .map(|body| add_fixture(&harness, (body % 8) + 1))
        .collect();
    harness.service.prefetch(&ids, PixelLevel::Proxy2048);

    let cancelled = harness.service.cancel(&ids);
    assert!(
        cancelled + harness.service.completed() as usize + harness.service.queued() <= ids.len(),
        "cancellation must not invent work"
    );
}

#[test]
fn a_visible_request_is_served_while_a_batch_is_running() {
    let harness = harness(PreviewConfig {
        workers: 1,
        ..PreviewConfig::default()
    });
    let visible = add_fixture(&harness, 1);
    // Warm the visible cell, the way the grid does when it first renders.
    harness
        .service
        .get(visible, PixelLevel::Thumb(512), Priority::Visible)
        .expect("warm");

    let batch: Vec<ImageId> = (1..=8).map(|body| add_fixture(&harness, body)).collect();
    harness.service.prefetch(&batch, PixelLevel::Proxy2048);

    // The photographer scrolls back to a cell they were already looking at.
    let started = Instant::now();
    let buffer = harness
        .service
        .get(visible, PixelLevel::Thumb(512), Priority::Visible)
        .expect("visible request");
    let elapsed = started.elapsed();

    assert!(buffer.width > 0);
    assert!(
        elapsed.as_millis() <= 50,
        "a visible request waited {elapsed:?} behind the batch; the budget is 50 ms"
    );
}

// --------------------------------------------------------------- the budget

#[test]
fn the_cache_budget_can_be_changed_and_purged_at_runtime() {
    let harness = harness(PreviewConfig::default());
    let id = add_fixture(&harness, 1);
    harness
        .service
        .get(id, PixelLevel::Proxy2048, Priority::Interactive)
        .expect("proxy");
    assert!(harness.service.cache_stats().bytes_used > 0);

    harness
        .service
        .set_cache_budget(CacheBudget { max_bytes: 1 });
    assert_eq!(harness.service.cache_stats().budget_bytes, 1);

    harness.service.purge_cache().expect("purge");
    assert_eq!(harness.service.cache_stats().entries, 0);

    // And everything still works afterwards, because nothing was truth.
    harness
        .service
        .get(id, PixelLevel::Thumb(512), Priority::Visible)
        .expect("rebuild after a purge");
}
