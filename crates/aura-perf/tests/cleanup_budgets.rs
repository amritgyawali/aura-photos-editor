//! Phase 24 performance budgets, measured against the current engine and store.
//!
//! Section 11 has five rows. **Two are waived and three are re-based**, and the distinction
//! matters:
//!
//! | Row | Budget | This build |
//! |---|---|---|
//! | Detection per image | <= 45 ms | re-based on the proxy; this pass opens no pixels to detect |
//! | Classical fill per region (45 MP) | <= 400 ms | re-based on a proxy region |
//! | Sibling borrow per region | <= 700 ms | re-based on a proxy region |
//! | Diffusion inpaint per region | <= 3 s | **waived**: no tier exists (exit report C3) |
//! | Cleanup share of a 1,000-image export | <= 12 min | **waived**: nothing is applied |
//!
//! A *re-based* row is measured on the size the decision path actually runs at, with the ratio to
//! the section 11 size stated so a future full-resolution number has something to be compared
//! against. A *waived* row has nothing to measure at all. Phase 23 drew the same line and this
//! file follows it.
//!
//! Section 11 names no storage row. One is measured anyway, and it comes out above a kilobyte per
//! image for a structural reason rather than a lucky one - see `store::BYTES_PER_IMAGE`.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::cleanup::{
    CleanupCode, CleanupProposal, CleanupReason, DistractionClass, SafetyVerdict,
};
use aura_core::contract::ids::ProposalId;
use aura_core::{PhotoId, ProjectId, SceneId};
use aura_generative::queue::{Blocked, Plan, Prepared};
use aura_generative::selfcheck::ArtefactReport;
use aura_generative::store::{CleanupStore, BYTES_PER_IMAGE};
use aura_generative::{borrow, detect, fill, fixtures, selfcheck, Image};
use aura_perf::{Budgets, Measurement, StageTimer};
use rusqlite::params;
use tempfile::TempDir;

/// How many regions the timing runs work on.
///
/// Small in debug, where the figure is reported rather than asserted, and large enough in release
/// that one slow first iteration does not decide the answer.
const REGIONS: usize = if cfg!(debug_assertions) { 3 } else { 24 };

/// How many photographs the storage run stores.
///
/// A thousand, because SQLite allocates in pages and a per-image figure taken over ten rows is a
/// measurement of page granularity rather than of the schema.
const STORAGE_FRAMES: usize = 1_000;

/// The area ratio between a 45 MP frame and the 200 px fixture, for the re-based rows.
///
/// Stated rather than applied: the fill and the borrow are not linear in area - the exemplar
/// search is quadratic in the source window and the block match is quadratic in the search
/// radius - so multiplying a proxy figure by this number would be a worse estimate than saying
/// what the ratio is and leaving the extrapolation to whoever has the hardware.
const FULL_FRAME_AREA_RATIO: f64 = 45_000_000.0 / (200.0 * 200.0);

fn budgets() -> Budgets {
    Budgets::load(std::path::Path::new("../../perf/budgets.toml"))
        .unwrap_or_else(|err| panic!("budgets: {err}"))
}

fn assert_timing(budgets: &Budgets, measurement: &Measurement) {
    let per_unit = if measurement.units == 0 {
        0.0
    } else {
        measurement.elapsed_ms as f64 / measurement.units as f64
    };
    println!(
        "{}: {} ms over {} units ({per_unit:.2} ms each)",
        measurement.stage, measurement.elapsed_ms, measurement.units
    );
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    if let Err(reason) = budgets.check(measurement) {
        panic!("{reason}");
    }
}

#[test]
fn detection_is_far_inside_its_budget_because_it_opens_no_pixels() {
    // Section 11 budgets 45 ms per image. This pass opens no pixels for detection at all - phase 11
    // measured the salience field, phase 06 found the subjects - so what is being timed is the
    // clearance test, the removability product, the sort and the cap.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let frame = detect::Frame {
        salient: (0..12)
            .map(|index| {
                (
                    aura_core::contract::cleanup::Box2 {
                        x: 0.02 + (index as f32) * 0.07,
                        y: 0.88,
                        w: 0.04,
                        h: 0.04,
                    },
                    0.5 + (index as f32) * 0.03,
                )
            })
            .collect(),
        subjects: vec![aura_core::contract::cleanup::Box2 {
            x: 0.40,
            y: 0.30,
            w: 0.20,
            h: 0.40,
        }],
        ..detect::Frame::default()
    };

    let timer = StageTimer::start("cleanup_detect", clock.as_ref());
    let mut done = 0u64;
    let iterations = if cfg!(debug_assertions) { 20 } else { 400 };
    for _ in 0..iterations {
        let found = detect::candidates(&frame);
        assert!(found.len() <= aura_core::contract::cleanup::MAX_PROPOSALS_PER_IMAGE);
        done += 1;
    }
    assert_timing(&budgets(), &timer.finish(done));
}

#[test]
fn a_classical_fill_of_one_region_is_inside_its_budget() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let (frame, region) = fixtures::with_object(fixtures::Background::Grass, fixtures::CORNER);

    let timer = StageTimer::start("cleanup_fill", clock.as_ref());
    let mut done = 0u64;
    for _ in 0..REGIONS {
        let filled = fill::fill(&frame, &region).unwrap_or_else(|code| {
            panic!("the fixture must be fillable, got {code:?}");
        });
        // Read something off the result so the whole synthesis cannot be optimised away.
        assert!(filled.patches > 0);
        done += 1;
    }
    println!(
        "  section 11's row is a 45 MP region, {FULL_FRAME_AREA_RATIO:.0}x this fixture's area. \
         The exemplar search is quadratic in the source window, so that ratio is stated rather \
         than multiplied through."
    );
    assert_timing(&budgets(), &timer.finish(done));
}

#[test]
fn a_sibling_borrow_of_one_region_is_inside_its_budget() {
    // The most expensive operation in the phase: twelve block matches over a 49-by-49 search
    // window, then 495 four-subset homography solves. Exhaustive rather than sampled because
    // invariant 4 needs the same recipe to produce the same pixels.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let clean = fixtures::clean(fixtures::Background::Busy);
    let (frame, region) = fixtures::with_object(fixtures::Background::Busy, fixtures::CORNER);
    let source = PhotoId::new();

    let timer = StageTimer::start("cleanup_borrow", clock.as_ref());
    let mut done = 0u64;
    for _ in 0..REGIONS {
        let borrowed = borrow::borrow(&frame, &clean, source, &region)
            .unwrap_or_else(|code| panic!("the fixture must be borrowable, got {code:?}"));
        assert!(borrowed.alignment.inliers >= 4);
        done += 1;
    }
    assert_timing(&budgets(), &timer.finish(done));
}

#[test]
fn the_self_check_of_one_result_is_inside_its_budget() {
    // Three measurements. The expensive one is the frame-wide step percentile, which is bucketed
    // rather than sorted and strided rather than exhaustive for exactly this reason.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let (frame, region) = fixtures::with_object(fixtures::Background::Busy, fixtures::CENTRE);

    let timer = StageTimer::start("cleanup_selfcheck", clock.as_ref());
    let mut done = 0u64;
    for _ in 0..REGIONS {
        let report = selfcheck::inspect(&frame, &region);
        assert!(report.worst() >= 0.0);
        done += 1;
    }
    assert_timing(&budgets(), &timer.finish(done));
}

#[test]
fn the_two_waived_rows_are_named_rather_than_silently_absent() {
    // A waiver with no test is a waiver nobody reads. These two have nothing to measure, and
    // saying so here means a build that gained either would fail this test rather than quietly
    // keep the waiver.
    assert!(
        !aura_generative::INPAINT_PACK_INSTALLED,
        "a diffusion pack is installed, so section 11's 3 s inpaint row is no longer waived: \
         measure it and delete this test"
    );
    println!("waived: diffusion inpaint per region (<= 3 s) - no tier exists, exit report C3");
    println!(
        "waived: cleanup share of a 1,000-image export (<= 12 min) - nothing is applied on this \
         build, exit report C1 and C2"
    );
}

#[test]
fn the_store_stays_inside_its_per_image_budget() {
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "perf")
        .unwrap_or_else(|err| panic!("catalog: {}", err.detail));
    let catalog = Arc::new(catalog);

    let project = ProjectId::new();
    let photos: Vec<PhotoId> = (0..STORAGE_FRAMES).map(|_| PhotoId::new()).collect();
    seed(&catalog, &project, &photos);

    let store = CleanupStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let before = payload_bytes(&catalog);

    for photo in &photos {
        let plan = widest_plan(*photo);
        store
            .put(&project, *photo, SceneId::ReceptionEntrance, &plan, (1, 1, 1))
            .unwrap_or_else(|err| panic!("put: {}", err.detail));
    }

    // The per-object breakdown, printed so the figure in `store::BYTES_PER_IMAGE` and in
    // `perf/budgets.toml` is a measurement rather than an estimate. Phase 21 wrote a storage
    // figure before measuring it and was wrong by a factor of two; this phase did the same thing
    // and this is what caught it.
    for (name, bytes) in payload_by_object(&catalog) {
        println!("  {name:<34} {:.0} B/image", bytes as f64 / STORAGE_FRAMES as f64);
    }

    let after = payload_bytes(&catalog);
    let per_image = (after - before) as f64 / STORAGE_FRAMES as f64;
    println!(
        "cleanup store: {per_image:.0} B per image over {STORAGE_FRAMES} photographs, against a \
         budget of {BYTES_PER_IMAGE} B"
    );
    println!(
        "  above a kilobyte for phase 21's structural reason: this stores a *list* whose length \
         is the number of things the pass considered, where phases 09 to 20 stored one \
         fixed-width verdict"
    );
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    assert!(
        per_image <= BYTES_PER_IMAGE as f64,
        "the cleanup store costs {per_image:.0} B per image against a budget of {BYTES_PER_IMAGE}"
    );
}

/// `dbstat` payload rather than `PRAGMA page_count`.
///
/// Phase 19's correction and phase 09's: whole-file page count quantises to 4 KiB, so a per-image
/// figure taken from it measures page granularity rather than the schema.
fn payload_bytes(catalog: &Arc<Catalog>) -> u64 {
    catalog
        .read(|conn| {
            let total: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(payload), 0) FROM dbstat WHERE name LIKE 'cleanup%'
                       OR name LIKE 'idx_cleanup%'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok(u64::try_from(total).unwrap_or(0))
        })
        .unwrap_or(0)
}

/// Payload bytes for each cleanup table and index, largest first.
fn payload_by_object(catalog: &Arc<Catalog>) -> Vec<(String, u64)> {
    catalog
        .read(|conn| {
            let mut statement = conn
                .prepare(
                    "SELECT name, SUM(payload) FROM dbstat
                      WHERE name LIKE 'cleanup%' OR name LIKE 'idx_cleanup%'
                      GROUP BY name ORDER BY 2 DESC",
                )
                .map_err(|e| aura_core::errors::db::statement_failed("dbstat", &e))?;
            let mut rows = statement
                .query([])
                .map_err(|e| aura_core::errors::db::statement_failed("dbstat", &e))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| aura_core::errors::db::statement_failed("dbstat", &e))?
            {
                let name: String = row.get(0).unwrap_or_default();
                let bytes: i64 = row.get(1).unwrap_or(0);
                out.push((name, u64::try_from(bytes).unwrap_or(0)));
            }
            Ok(out)
        })
        .unwrap_or_default()
}

/// The widest plan the fixtures produce: three proposals and six refusals.
fn widest_plan(photo: PhotoId) -> Plan {
    let prepared: Vec<Prepared> = (0..3)
        .filter_map(|index| {
            let region = aura_core::contract::cleanup::Box2 {
                x: 0.02 + (index as f32) * 0.10,
                y: 0.85,
                w: 0.06,
                h: 0.06,
            };
            let mut proposal = CleanupProposal::new(
                ProposalId::new(),
                photo,
                region,
                DistractionClass::Bin,
                aura_core::contract::cleanup::CleanupMethod::ClassicalFill,
                SafetyVerdict::allow(),
                vec![
                    CleanupReason::plain(CleanupCode::TextureUniform, 1.0),
                    CleanupReason::plain(CleanupCode::NoAlignedSibling, 0.3),
                    CleanupReason::plain(CleanupCode::ReviewRequiredConfidence, 0.5),
                ],
            )
            .ok()?;
            proposal.confidence = 0.72;
            proposal.salience = 0.8;
            proposal.scene = SceneId::ReceptionEntrance;
            proposal.detector_ver = 1;
            proposal.analysis_ver = 1;
            proposal.policy_ver = 1;
            Some(Prepared {
                proposal,
                patch: Image::black(1, 1),
                artefact: ArtefactReport::CLEAN,
            })
        })
        .collect();

    let blocked: Vec<Blocked> = (0..6)
        .map(|index| {
            let check = aura_core::contract::cleanup::SafetyCheck::ALL
                .get(index % 5)
                .copied()
                .unwrap_or(aura_core::contract::cleanup::SafetyCheck::Confidence);
            Blocked {
                region: aura_core::contract::cleanup::Box2 {
                    x: 0.30 + (index as f32) * 0.05,
                    y: 0.40,
                    w: 0.05,
                    h: 0.05,
                },
                check,
                code: CleanupCode::ProtectionUnknown,
                verdict: SafetyVerdict::block(check, "a budget fixture"),
            }
        })
        .collect();

    Plan {
        prepared,
        blocked,
        reverted: 0,
        mask_complete: false,
        judged: 0,
        declined: 0,
    }
}

fn seed(catalog: &Arc<Catalog>, project: &ProjectId, photos: &[PhotoId]) {
    let key = project.to_db();
    let name = key.clone();
    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                      VALUES (?1, 'perf', '2026-08-29T00:00:00Z', '2026-08-29T00:00:00Z')",
                params![name],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            Ok(())
        })
        .unwrap_or_else(|err| panic!("project: {}", err.detail));

    let ids: Vec<String> = photos.iter().map(aura_core::PhotoId::to_db).collect();
    catalog
        .writer()
        .transact(move |conn| {
            for photo in &ids {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        created_at, updated_at)
                          VALUES (?1, ?2, '2026-08-29T00:00:00Z', '2026-08-29T00:00:00Z',
                                  '2026-08-29T00:00:00Z', '2026-08-29T00:00:00Z')",
                    params![photo, key],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("photos: {}", err.detail));
}
