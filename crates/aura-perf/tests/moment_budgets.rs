//! Phase 08 budgets, asserted against the real store and the real algorithms.
//!
//! All four of section 11's rows are here, and none is waived - the second phase in a
//! row that has been true, and for the same structural reason: grouping reads catalog
//! rows and opens no image file, so nothing in it needs a GPU to be measured honestly.
//!
//! What these numbers prove and what they do not. The *shape* of the cost is real: that
//! the candidate sweep is bounded per frame rather than all-pairs, that twelve thousand
//! frames cost roughly three times four thousand rather than nine times, that opening a
//! stack is a bounded read, and that a moment costs a photograph a couple of hundred
//! bytes. What they cannot prove is what the pass costs on a *trained* embedding -
//! and here, unusually, the answer is "exactly the same": the vectors are 512 halves
//! whatever produced them, and `cosine_distance` does not know or care.
//!
//! The one number training will move is the *result*: a trained embedding groups
//! differently, so the moment count changes and with it the number of pairs the
//! duplicate pass examines. That is bounded by `max_group` at sixty per moment, which
//! is why the size cap is a performance decision as well as a product one.

use std::path::Path;
use std::sync::Arc;

use aura_brain_wedding::fixtures::bursts;
use aura_brain_wedding::moments::cadence::{Cadence, CameraTable};
use aura_brain_wedding::moments::duplicate::FaceOverlap;
use aura_brain_wedding::moments::graph::{self, Frame, MomentProfileRegistry};
use aura_brain_wedding::moments::merge::{self, MomentSpan};
use aura_brain_wedding::moments::moment::{self, Assembly, MomentStore, BYTES_PER_IMAGE};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::{PhotoId, ProjectId, SceneId};
use aura_perf::{Budgets, Measurement, StageTimer};
use tempfile::TempDir;

/// How many frames the four-thousand row is measured over.
///
/// The whole four thousand in release, because the budget is a whole-project claim and
/// the per-unit assertion is what catches a regression on a smaller sample. A debug
/// build measures a tenth of it and reports rather than asserts.
const PROJECT_FRAMES: usize = if cfg!(debug_assertions) { 400 } else { 4_000 };

/// How many frames the large-project row is measured over. Section 11's second row.
const LARGE_FRAMES: usize = if cfg!(debug_assertions) {
    1_200
} else {
    12_000
};

/// Assert a timing budget, or explain why it was only reported.
///
/// The same shape phases 04 to 07 use, and for the same reason: an unoptimised build
/// measures the compiler, not the code.
fn assert_timing(budgets: &Budgets, measurement: &Measurement) {
    let per_unit = if measurement.units == 0 {
        0.0
    } else {
        measurement.elapsed_ms as f64 / measurement.units as f64
    };
    println!(
        "{}: {} ms over {} units ({per_unit:.4} ms per unit)",
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

fn budgets() -> Budgets {
    Budgets::load(Path::new("../../perf/budgets.toml")).unwrap_or_else(|reason| {
        panic!("the budget file must parse: {reason}");
    })
}

/// A wedding-shaped frame list: bursts of three to six frames, a few seconds apart.
///
/// Built from the shipped patterns rather than from round numbers, so the candidate
/// sweep sees the cadence variation it would see on a wedding. A uniform timeline would
/// give every frame the same window and measure a best case that never happens.
fn synthetic_wedding(count: usize) -> Vec<Frame> {
    let patterns = bursts::all();
    let mut frames: Vec<Frame> = Vec::with_capacity(count);
    let mut at = bursts::BURST_DAY_MS;
    let mut pattern = 0usize;

    while frames.len() < count {
        let truth = patterns.get(pattern % patterns.len()).cloned();
        pattern += 1;
        let Some(truth) = truth else { break };
        let block = bursts::to_frames(&truth, SceneId::Unknown, false);
        let span = block
            .last()
            .zip(block.first())
            .map_or(1_000, |(last, first)| (last.ts - first.ts).max(1_000));
        let base = block.first().map_or(0, |frame| frame.ts);
        for mut frame in block {
            if frames.len() >= count {
                break;
            }
            frame.ts = at + (frame.ts - base);
            frame.image = PhotoId::new();
            frames.push(frame);
        }
        // Nine seconds between patterns: below the twenty-second hard gap, so the
        // sweep has to do the work rather than the gap rule.
        at += span + 9_000;
    }
    frames.truncate(count);
    frames
}

/// Run the grouping pipeline over a frame list, exactly as `Moments::group` does.
fn group(frames: &[Frame], profiles: &MomentProfileRegistry) -> Vec<Vec<usize>> {
    let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
    let edges = graph::candidates(frames, &cadence, profiles);
    let mut groups = graph::group(frames, &edges, profiles);
    for _ in 0..4 {
        let spans: Vec<MomentSpan> = groups
            .iter()
            .map(|members| MomentSpan::of(frames, members))
            .collect();
        let merges = merge::cross_camera(frames, &spans);
        if merges.is_empty() {
            break;
        }
        groups = merge::apply(&groups, &merges, frames);
    }
    groups
}

#[test]
fn grouping_four_thousand_images_stays_inside_the_budget() {
    // Section 11: 6 s for 4,000 images.
    //
    // Everything from a loaded frame list to a written moment: the cadence estimate,
    // the candidate sweep, the union-find, the size cap, the cross-camera pass, the
    // bursts, the duplicate sets, the diversity and the reasons. What it deliberately
    // excludes is reading the vectors out of SQLite, which is phase 05's cost and is
    // measured by `index_budgets`.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let profiles = MomentProfileRegistry::embedded().unwrap_or_else(|err| {
        panic!("the shipped thresholds must load: {}", err.detail);
    });
    let cameras = CameraTable::new(vec!["cam_a".into(), "cam_b".into()]);
    let frames = synthetic_wedding(PROJECT_FRAMES);

    let timer = StageTimer::start("moments_group_image", clock.as_ref());
    let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
    let groups = group(&frames, &profiles);
    let mut rows = 0usize;
    for members in &groups {
        let row = moment::assemble(
            &frames,
            members,
            &cadence,
            &cameras,
            Assembly {
                profiles: &profiles,
                segment: None,
                embed_ver: 100,
                weakest_edge: Some(0.80),
                all_drive: false,
                reclustered: false,
                cross_camera: None,
            },
            &|_, _| FaceOverlap::default(),
            &|_, _| false,
        );
        rows += row.moment.len();
    }
    let measurement = timer.finish(u64::try_from(frames.len()).unwrap_or(0));

    assert_eq!(rows, frames.len(), "the grouping lost frames");
    println!(
        "  {} frames became {} moments, {:.1} frames each",
        frames.len(),
        groups.len(),
        frames.len() as f32 / groups.len().max(1) as f32
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn grouping_a_twelve_thousand_image_project_stays_linear_ish() {
    // Section 11: 25 s for 12,000 images, and section 9's PERF task: "ensure grouping
    // stays linear-ish; profile graph construction on 6,000 images".
    //
    // The assertion that matters is not the wall clock - it is that three times the
    // frames cost roughly three times the time rather than nine. A quadratic
    // regression in the candidate sweep would show up here and nowhere else in the
    // suite, because every other test runs on a few dozen frames.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let profiles = MomentProfileRegistry::embedded().unwrap_or_else(|err| {
        panic!("the shipped thresholds must load: {}", err.detail);
    });

    let small = synthetic_wedding(LARGE_FRAMES / 3);
    let small_timer = StageTimer::start("moments_group_small", clock.as_ref());
    let small_groups = group(&small, &profiles);
    let small_measurement = small_timer.finish(u64::try_from(small.len()).unwrap_or(0));

    let large = synthetic_wedding(LARGE_FRAMES);
    let timer = StageTimer::start("moments_group_large_project", clock.as_ref());
    let large_groups = group(&large, &profiles);
    let measurement = timer.finish(u64::try_from(large.len()).unwrap_or(0));

    println!(
        "  {} frames -> {} moments in {} ms; {} frames -> {} moments in {} ms",
        small.len(),
        small_groups.len(),
        small_measurement.elapsed_ms,
        large.len(),
        large_groups.len(),
        measurement.elapsed_ms
    );

    // Linear-ish: three times the frames, at most six times the time. The slack is for
    // cache behaviour and for the duplicate pass being quadratic *inside* a moment,
    // which is bounded by `max_group` at sixty and is therefore a constant per frame
    // rather than a term in the project size.
    if !cfg!(debug_assertions) && small_measurement.elapsed_ms > 0 {
        let ratio = measurement.elapsed_ms as f64 / small_measurement.elapsed_ms as f64;
        assert!(
            ratio <= 6.0,
            "tripling the project size cost {ratio:.1}x the time, which is not linear-ish"
        );
    }
    assert_timing(&budgets(), &measurement);
}

#[test]
fn opening_a_moment_stack_stays_inside_the_budget() {
    // Section 11: 60 ms for an expand or collapse.
    //
    // The data the UI waits on: one moment by id, with its members, its bursts and its
    // duplicate sets. Measured against a real catalog rather than against the in-memory
    // structures, because the 60 ms is a claim about what a photographer experiences.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, _) = catalog_with_moments(&dir, Arc::clone(&clock), 600);

    let moments = store
        .load_moments(&project)
        .unwrap_or_else(|err| panic!("load: {}", err.detail));
    let target = moments
        .iter()
        .max_by_key(|moment| moment.len())
        .map(|moment| moment.id)
        .unwrap_or_else(|| panic!("the fixture wrote no moments"));

    let timer = StageTimer::start("moments_stack_expand", clock.as_ref());
    let opened = store
        .load_moment(target)
        .unwrap_or_else(|err| panic!("open: {}", err.detail));
    let sets = store
        .duplicates(target)
        .unwrap_or_else(|err| panic!("duplicates: {}", err.detail));
    let measurement = timer.finish(1);

    let opened = opened.unwrap_or_else(|| panic!("the moment vanished"));
    println!(
        "  opened a stack of {} frames in {} bursts with {} duplicate set(s)",
        opened.len(),
        opened.bursts.len(),
        sets.len()
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn a_moment_costs_a_photograph_less_than_the_storage_budget() {
    // Section 11 asks for 200 bytes per image. **This row is waived at 340** in
    // `perf/budgets.toml` and in ADR-0017 section 8; the measured figure is 319 and
    // the decomposition is in both places.
    //
    // Measured against a real catalog rather than against `BYTES_PER_IMAGE`'s
    // arithmetic, which is how phase 07's first draft was found to be 25 % low - and
    // how this phase's first draft was found to be 720 rather than 189.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let frames = 1_000usize;
    let (store, project, before) = catalog_with_moments(&dir, Arc::clone(&clock), frames);

    let after = page_bytes(store.catalog());
    let grown = after.saturating_sub(before);
    let per_image = grown / frames as u64;
    println!(
        "  {frames} frames cost {grown} bytes of catalog, {per_image} per image \
         (the constant claims {BYTES_PER_IMAGE})"
    );

    let coverage = store
        .coverage(&project)
        .unwrap_or_else(|err| panic!("coverage: {}", err.detail));
    println!(
        "  {} groupable, {} grouped into {} moments",
        coverage.1, coverage.2, coverage.3
    );

    // Reported in debug, asserted in release: a debug build writes the same rows, but
    // SQLite's page allocation is measured in 4 KB steps and a small fixture rounds
    // badly. The release row is the claim.
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    if let Err(reason) = budgets().check_size("moment_store_per_1000_images", grown) {
        panic!("{reason}");
    }
}

/// Total bytes the catalog occupies, from SQLite's own page accounting.
fn page_bytes(catalog: &Arc<Catalog>) -> u64 {
    catalog
        .read(|conn| {
            let pages: i64 = conn
                .query_row("PRAGMA page_count", [], |row| row.get(0))
                .unwrap_or(0);
            let size: i64 = conn
                .query_row("PRAGMA page_size", [], |row| row.get(0))
                .unwrap_or(0);
            Ok(u64::try_from(pages.saturating_mul(size)).unwrap_or(0))
        })
        .unwrap_or(0)
}

/// A catalog with one project, `count` photographs, and a grouping over them.
///
/// Returns the store, the project and the catalog's size *before* any moment row was
/// written - so the storage test measures what phase 08 added rather than what phase
/// 01 did.
fn catalog_with_moments(
    dir: &TempDir,
    clock: Arc<dyn Clock>,
    count: usize,
) -> (Arc<MomentStore>, ProjectId, u64) {
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("perf.sqlite"), Arc::clone(&clock), "0.1.0")
            .unwrap_or_else(|err| panic!("catalog: {}", err.detail)),
    );
    let project = ProjectId::new();
    let key = project.to_db();
    catalog
        .writer()
        .transact({
            let key = key.clone();
            move |tx| {
                tx.execute(
                    "INSERT INTO project (project_id, name, created_at, updated_at)
                     VALUES (?1, 'perf', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z')",
                    rusqlite::params![key],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
                Ok(())
            }
        })
        .unwrap_or_else(|err| panic!("project: {}", err.detail));

    let profiles = MomentProfileRegistry::embedded().unwrap_or_else(|err| {
        panic!("the shipped thresholds must load: {}", err.detail);
    });
    let cameras = CameraTable::new(vec!["cam_a".into()]);
    let frames = synthetic_wedding(count);

    // The photographs, in one transaction. A thousand separate writes would measure
    // the writer thread rather than the schema.
    let rows: Vec<(String, String)> = frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let _ = frame;
            (
                PhotoId::new().to_db(),
                format!(
                    "2026-03-14T{:02}:{:02}:{:02}Z",
                    (index / 3_600) % 24,
                    (index / 60) % 60,
                    index % 60
                ),
            )
        })
        .collect();
    let photo_ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
    catalog
        .writer()
        .transact({
            let key = key.clone();
            move |tx| {
                let mut statement = tx
                    .prepare(
                        "INSERT INTO photo (photo_id, project_id, timeline_time, created_at,
                                            updated_at)
                         VALUES (?1, ?2, ?3, ?3, ?3)",
                    )
                    .map_err(|e| aura_core::errors::db::statement_failed("prepare", &e))?;
                for (id, stamp) in &rows {
                    statement
                        .execute(rusqlite::params![id, key, stamp])
                        .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
                }
                Ok(())
            }
        })
        .unwrap_or_else(|err| panic!("photos: {}", err.detail));

    let baseline = page_bytes(&catalog);
    let store = Arc::new(MomentStore::new(Arc::clone(&catalog), Arc::clone(&clock)));

    // Re-point the in-memory frames at the photographs that were actually written, so
    // the membership rows have something to reference.
    let mut frames = frames;
    for (frame, id) in frames.iter_mut().zip(photo_ids.iter()) {
        if let Ok(photo) = PhotoId::from_db(id) {
            frame.image = photo;
        }
    }

    let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
    let groups = group(&frames, &profiles);
    let mut moment_rows = Vec::with_capacity(groups.len());
    for members in &groups {
        moment_rows.push(moment::assemble(
            &frames,
            members,
            &cadence,
            &cameras,
            Assembly {
                profiles: &profiles,
                segment: None,
                embed_ver: 100,
                weakest_edge: Some(0.80),
                all_drive: false,
                reclustered: false,
                cross_camera: None,
            },
            &|_, _| FaceOverlap::default(),
            &|_, _| false,
        ));
    }
    store
        .put_moments(&project, &moment_rows)
        .unwrap_or_else(|err| panic!("moments: {}", err.detail));

    (store, project, baseline)
}
