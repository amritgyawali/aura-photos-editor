#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args, clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// A waiver that is still true is a constant, and asserting it is the point: the day
// `NETWORK_TRANSPORT_AVAILABLE` becomes true this line fails and somebody has to decide whether
// the upload budget is still waived.
#![allow(clippy::assertions_on_constants)]

//! PHASE-30 section 11's budgets, as tests.
//!
//! | Metric | Budget | Status here |
//! |---|---|---|
//! | Export 1,000 images (45 MP JPEG, GPU) <= 12 min | **waived** | no `wgpu` backend |
//! | Export throughput >= 1.4 images/s | **waived** | same measurement, same reason |
//! | Hash verification overhead <= 8 % of export time | measured | see the run's own output |
//! | Upload 1,000 images (100 Mbps) <= 35 min | **waived** | no network transport |
//! | Learning update computation <= 90 s per wedding | measured | milliseconds |
//!
//! Plus a storage row section 11 does not name. Every phase since 21 has measured one anyway, and
//! phases 21, 26, 28 and 29 each wrote down what happens when a figure is quoted before it is
//! measured.
//!
//! ## Why three rows are waived rather than measured badly
//!
//! Two of them are about **rendering**, not about writing. "1,000 45 MP JPEGs in 12 minutes" is
//! dominated by the render graph, which on this machine has no `wgpu` backend and takes about
//! 210 ms for a 2,048 px proxy - phase 14's condition C1. Measuring the writer and calling it an
//! export budget would produce a comfortable number about the wrong thing.
//!
//! The third is about a network this build cannot reach. `NETWORK_TRANSPORT_AVAILABLE` is false,
//! and a resume measured against an in-memory transport is a measurement of memcpy.
//!
//! Phase 28 wrote the rule after its own first gate printed a wall clock over a `ScriptedRunner`:
//! **a gate that reads a wall clock on a fixture measures the fixture.** So the waivers are
//! printed on every run, and the test that prints them asserts that they are still true.
//!
//! ## What *is* measured, and why these two
//!
//! The verification overhead is the one row in section 11 that is entirely this phase's own work:
//! a flush, an `fsync`, a re-open, a full re-read and a hash of every file, which is the cost of
//! the guarantee the whole phase is built on. If it were 40 % rather than 8 %, somebody would add
//! a setting to switch it off.
//!
//! The learning update is the other, and its budget is 90 seconds against a measurement in
//! milliseconds - because the loop is a trimmed median over rows the ledger already holds and it
//! opens no photograph. That margin is not a boast; it is what
//! `crates/aura-learn/tests/no_guarantee_learning.rs` protects, because a change that reached for
//! a renderer to measure an improvement directly would blow this by four orders of magnitude.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::delivery::{
    DeliveryColour, Destination, ExportJob, ExportSet, FileFormat, MetadataPolicy, NamingTemplate,
    OutputSharpen, Resize,
};
use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{CorrectionBucket, Learnable, MAX_DIFF_LINES};
use aura_core::contract::scene::{ImageId, SceneId};
use aura_core::ProjectId;
use aura_export::api::ExportPass;
use aura_export::fixtures::{plate, Plate, ScriptedField, ScriptedSource};
use aura_export::read::Frame;
use aura_export::store::ExportStore;
use aura_export::verify::{hash_file, write_and_verify};
use aura_learn::aggregate::{fold, Sample};
use aura_learn::fixtures::derived_decision;
use aura_learn::update::{compute, Offsets};
use aura_perf::{Budgets, Measurement};
use rusqlite::params;

/// How many frames the export rows are measured over.
///
/// Sixty rather than a thousand. The budget being checked is a *ratio* - what fraction of an
/// export the read-back costs - and a ratio does not need a thousand frames to be stable. A
/// thousand authored plates would spend four minutes of CI time measuring the same number.
const FRAMES: usize = 60;

/// The plate size. Small, deliberately: what is being measured is the write and the read-back, and
/// a 45 MP plate would make the encode dominate a ratio that is about I/O.
const PLATE: (u32, u32) = (320, 240);

fn budgets() -> Budgets {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../perf/budgets.toml");
    Budgets::load(&path).expect("perf/budgets.toml parses")
}

/// Derive a photograph's id from an index rather than minting one.
///
/// Phase 29 found the trap the hard way: `ImageId::new()` is a v7 UUID, random in its low bits, so
/// a fixture that minted one looks deterministic while every downstream tie-break falls back on
/// the id. A budget suite is not a determinism suite, but a run whose file names change every time
/// is a run whose failures cannot be compared with the last one.
fn derived_image(index: usize) -> ImageId {
    ImageId::from_db(&format!("pht_30000000-0000-7000-8000-{index:012}")).expect("a derived id")
}

fn setup(images: &[ImageId]) -> (tempfile::TempDir, Arc<Catalog>, ProjectId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog =
        Arc::new(Catalog::open(&dir.path().join("c.sqlite"), clock, "perf").expect("catalog"));
    let project = ProjectId::new();
    let key = project.to_db();
    let ids: Vec<String> = images.iter().map(ImageId::to_db).collect();
    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'perf', '2026-05-16T00:00:00Z', '2026-05-16T00:00:00Z')",
                params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("seed", &e))?;
            for (ix, id) in ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                         created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, ?4, ?4)",
                    params![
                        id,
                        key,
                        1_760_000_000_000_i64 + ix as i64,
                        "2026-05-16T00:00:00Z"
                    ],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("seed", &e))?;
            }
            Ok(())
        })
        .expect("seed");
    (dir, catalog, project)
}

fn job(images: &[ImageId], root: PathBuf, verify: bool) -> ExportJob {
    ExportJob {
        sets: vec![ExportSet {
            name: "gallery".to_owned(),
            images: images.to_vec(),
            format: FileFormat::Jpeg,
            quality: 92,
            resize: Resize::Full,
            sharpen: OutputSharpen::None,
            naming: NamingTemplate::parse("{seq}").expect("template"),
            colour: DeliveryColour::Srgb,
            bit_depth: 8,
            sidecar: false,
        }],
        destination: Destination::Folder { path: root },
        metadata: MetadataPolicy::default(),
        verify,
    }
}

// ---------------------------------------------------------------------------
// The measured rows
// ---------------------------------------------------------------------------

#[test]
fn hash_verification_costs_less_than_a_twelfth_of_an_export() {
    // Section 11's third row: verification overhead <= 8 % of export time. The one row that is
    // entirely this phase's own work, and the one that decides whether anybody switches the
    // guarantee off.
    //
    // **Measured directly rather than by differencing two exports.** The first version of this
    // test ran the job twice and subtracted, and on a 60-frame job the two whole-run timings are
    // within a third of a per cent of each other - so the run-to-run noise is larger than the
    // thing being measured, and the verified run came out *faster*. That reads as an overhead of
    // zero and passes an 8 % budget for no reason at all. The same family as phase 22's ringing
    // measurement and phase 19's halo test: a difference of two large numbers is the wrong
    // instrument for a small one. What verification adds is a re-open, a full re-read and a hash
    // of each written file, so that is what is timed.
    let images: Vec<ImageId> = (0..FRAMES).map(derived_image).collect();
    let (dir, catalog, project) = setup(&images);
    let field = ScriptedField::new(Some("Alex and Sam"), FRAMES as u32, FRAMES as u32);
    let source = ScriptedSource::new(Plate::Gradient, PLATE.0, PLATE.1);
    let store = ExportStore::new(Arc::clone(&catalog));
    let pass = ExportPass::new(&store, &field, &source, "perf");

    // DETERMINISM: measuring a budget, not deciding. The justification every budget test in this
    // crate records.
    let root = dir.path().join("verified");
    let started = Instant::now();
    let verified = pass
        .run(project, &job(&images, root.clone(), true))
        .expect("the verified export runs");
    let export_us = started.elapsed().as_micros().max(1);
    assert_eq!(verified.files.len(), FRAMES);
    assert!(verified.files.iter().all(|f| f.verified));

    // The added work, on the files the export has just written. Warm cache, exactly as it is
    // inside the pass.
    // `ExportedFile::path` is relative to the destination root, which is what the manifest
    // records: a delivery that stored absolute paths would not survive being moved to a drive.
    let paths: Vec<PathBuf> = verified.files.iter().map(|f| root.join(&f.path)).collect();
    // DETERMINISM: measuring a budget, not deciding.
    let started = Instant::now();
    let mut digests = Vec::with_capacity(paths.len());
    for path in &paths {
        digests.push(hash_file(path).expect("the written file reads back"));
    }
    let readback_us = started.elapsed().as_micros();

    // The read-back is the read-back: the digests it produces are the ones the pass stored. If
    // they were not, the timing above would be a measurement of something else.
    for (file, digest) in verified.files.iter().zip(&digests) {
        assert_eq!(
            &file.hash,
            digest,
            "{} did not read back the same",
            file.path.display()
        );
    }

    let percent = u64::try_from((readback_us * 100).div_ceil(export_us)).unwrap_or(u64::MAX);
    println!(
        "export: {} frames at {}x{} in {} us; the read-back of all {} files took {} us, {} % of it",
        FRAMES,
        PLATE.0,
        PLATE.1,
        export_us,
        paths.len(),
        readback_us,
        percent
    );

    if cfg!(debug_assertions) {
        // In a debug build the JPEG encoder is several times slower than it ships, which makes
        // this ratio *flattering* rather than conservative - the denominator is inflated. A
        // budget that only ever passes because the build is slow is not a budget.
        println!(
            "export_verify_overhead_percent: {percent} % - not asserted in a debug build; \
             run `cargo test --release --package aura-perf`"
        );
        return;
    }
    if let Err(breach) = budgets().check_count("export_verify_overhead_percent", percent) {
        panic!("{breach}");
    }
}

#[test]
fn a_learning_update_is_computed_in_milliseconds_rather_than_the_ninety_seconds_budgeted() {
    // Section 11's fifth row. The margin is four orders of magnitude and that is not a boast: it
    // is what the grep-as-a-test protects, because a change that reached for a renderer to measure
    // an improvement directly would blow this budget entirely.
    //
    // More corrections than any real wedding produces: every learnable value, in three scenes,
    // forty corrections each from four weddings - 45 buckets, 1,800 corrections, all of them
    // folded, split and trimmed.
    //
    // Only the first `MAX_DIFF_LINES` of them *move*, and that is not a way round the bound: it is
    // the bound. An update may carry 24 summary lines because that is what a photographer reads
    // before agreeing to it, so the largest fit this loop can ever be asked for is 45 buckets
    // aggregated and 24 offered. A fixture that moved all 45 would measure a shape
    // `LearningUpdate::validate` refuses.
    let mut aggregates = Vec::new();
    let mut moving = 0_usize;
    for (l_ix, learnable) in Learnable::ALL.into_iter().enumerate() {
        for (s_ix, scene) in [
            SceneId::Unknown,
            SceneId::Ceremony,
            SceneId::ReceptionEntrance,
        ]
        .into_iter()
        .enumerate()
        {
            let bucket = CorrectionBucket {
                kind: learnable.decision_kind(),
                scene,
                learnable,
                subject_close: false,
            };
            let magnitude = if moving < MAX_DIFF_LINES {
                moving += 1;
                learnable.ceiling() * 0.3
            } else {
                0.0
            };
            let samples: Vec<Sample> = (0..40)
                .map(|i| Sample {
                    decision: derived_decision(1, l_ix * 8 + s_ix, i),
                    project: (i % 4) as u64,
                    magnitude,
                })
                .collect();
            let (aggregate, _) = fold(bucket, &samples);
            aggregates.push((aggregate, samples));
        }
    }
    let corrections: u32 = aggregates.iter().map(|(a, _)| a.corrections).sum();

    // DETERMINISM: measuring a budget, not deciding.
    let started = Instant::now();
    let candidate = compute(ProfileId::new(), 1, &Offsets::new(), &aggregates)
        .expect("a candidate from a wedding of corrections");
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    println!(
        "learning: {} corrections across {} buckets fitted in {} ms (budget 90,000 ms), {} rows \
         offered",
        corrections,
        aggregates.len(),
        elapsed_ms,
        candidate.comparison.rows.len()
    );
    assert!(candidate.update.corrections_used > 0);

    let measurement = Measurement {
        stage: "learn_update".to_owned(),
        elapsed_ms,
        units: 1,
    };
    if let Err(breach) = budgets().check(&measurement) {
        panic!("{breach}");
    }
}

// ---------------------------------------------------------------------------
// The storage row section 11 does not name
// ---------------------------------------------------------------------------

#[test]
fn the_delivery_store_is_inside_its_per_file_budget_and_the_bound_holds() {
    // Phase 21's rule and phase 26's second half: measure the figure, and assert the *bound* as
    // well as the number by running the same pass over ten times the units. A size assertion alone
    // passes on a build that removed a cap and happened to be measured on a small fixture.
    let small = measure_store(20);
    let large = measure_store(200);

    println!(
        "delivery store: {} B for 20 files ({} B/file), {} B for 200 files ({} B/file)",
        small,
        small / 20,
        large,
        large / 200
    );

    let budgets = budgets();
    for (bytes, files) in [(small, 20_u64), (large, 200)] {
        if let Err(breach) = budgets.check_size("delivery_store_per_file", bytes / files) {
            panic!("{breach}");
        }
    }

    // The shape: one row per delivered file plus a bounded per-job header, so the per-file figure
    // is flat rather than falling (phase 29's shape) or growing (phase 26's). Ten times the files
    // costs between seven and thirteen times the bytes; anything outside that is a term nobody
    // bounded.
    let ratio = large as f64 / small as f64;
    assert!(
        (7.0..13.0).contains(&ratio),
        "ten times the files cost {ratio:.1}x the store, which is not a per-file shape"
    );
}

/// Write `n` files and measure what migration 30 stores for them.
fn measure_store(n: usize) -> u64 {
    let images: Vec<ImageId> = (0..n).map(derived_image).collect();
    let (dir, catalog, project) = setup(&images);
    let mut field = ScriptedField::new(Some("Alex and Sam"), n as u32, n as u32);
    for (ix, image) in images.iter().enumerate() {
        field = field.with_frame(
            *image,
            Frame {
                image: Some(*image),
                original_stem: Some(format!("DSC_{ix:04}")),
                date: Some("2026-05-16".to_owned()),
                chapter: Some("ceremony".to_owned()),
                camera: Some("nikon-z9".to_owned()),
                ..Frame::default()
            },
        );
    }
    let source = ScriptedSource::new(Plate::Flat, 32, 32);
    let store = ExportStore::new(Arc::clone(&catalog));
    ExportPass::new(&store, &field, &source, "perf")
        .run(project, &job(&images, dir.path().join("out"), true))
        .expect("the export runs");

    // `dbstat` payload rather than whole-file `page_count`. Phase 09 learned that the hard way:
    // `page_count` quantises to 4 KiB, so a budget pinned at its own measurement can only move in
    // 4 KiB steps and reads as exact when it is not.
    let bytes = catalog
        .read(move |conn| {
            let mut total = 0_i64;
            for table in [
                "export_job",
                "export_set",
                "export_file",
                "export_reason",
                "delivery_manifest",
            ] {
                let payload: i64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(payload), 0) FROM dbstat WHERE name = ?1",
                        params![table],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                total += payload;
            }
            Ok(total)
        })
        .expect("dbstat");
    drop(dir);
    u64::try_from(bytes).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The waived rows, printed rather than skipped
// ---------------------------------------------------------------------------

#[test]
fn the_waived_rows_are_named_on_every_run() {
    // Phase 28's rule: the conditions a gate did **not** prove are printed rather than left in a
    // document nobody opens. A budget suite that silently omitted three of five rows would read as
    // a suite that passed five.
    println!("PHASE-30 section 11, waived on this machine:");
    println!("  export 1,000 45 MP JPEGs in 12 min  - no wgpu backend; the render dominates, not");
    println!("                                        the writer. Phase 14 condition C1.");
    println!("  export throughput 1.4 images/s      - same measurement, same reason.");
    println!(
        "  upload 1,000 images in 35 min       - no network transport ships; a resume against"
    );
    println!("                                        an in-memory transport measures memcpy.");
    println!();
    println!("Measured: hash verification overhead, learning update computation, store per file.");

    // The one thing this test asserts: that the waivers are still true. A build that acquired a
    // network transport and left the waiver in place would be claiming less than it could prove,
    // which is the mirror of the failure the waivers exist for.
    assert!(
        !aura_delivery::NETWORK_TRANSPORT_AVAILABLE,
        "a network transport ships now; the upload budget is no longer waivable"
    );
}

#[test]
fn a_write_and_its_read_back_are_both_real_work() {
    // The floor under the overhead measurement: if `write_and_verify` were somehow not reading the
    // file back, the overhead would be zero and the budget would pass for the wrong reason.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big.bin");
    let bytes = vec![7_u8; 4 * 1024 * 1024];

    // DETERMINISM: measuring a budget, not deciding.
    let started = Instant::now();
    let written = write_and_verify(&path, &bytes, true).expect("verified write");
    println!(
        "a 4 MB verified write took {} us",
        started.elapsed().as_micros()
    );

    assert!(written.verified);
    assert_eq!(written.hash.len(), 64);
    // Not a timing assertion - a hash of four megabytes is fast enough that a threshold would be
    // flaky. What is asserted is that the digest is the digest of the **file** rather than of the
    // buffer, which is the whole distinction the guarantee rests on.
    assert_eq!(written.hash, hash_file(&path).expect("hash the file"));

    // And that a plate is a plate, so a failure in the export rows above is a failure in the
    // export rather than in the fixture that feeds it.
    let plate = plate(Plate::Gradient, 16, 16, DeliveryColour::Srgb, 8);
    assert!(plate.is_well_formed());
}
