#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args)]

//! PHASE-27 section 11's budgets, as tests.
//!
//! | Metric | Budget |
//! |---|---|
//! | QC pass for 1,000 images | <= 90 s |
//! | Single remediation round per image | <= 1.2 s |
//! | Report generation | <= 3 s |
//! | Cloud calls per wedding (default) | <= 40 |
//!
//! Plus a storage row section 11 does not name. Every phase since 21 has measured one anyway.
//!
//! ## Why this phase's pass is fast, and why that is not a boast
//!
//! Ten checks over a thousand frames inside 90 s sounds tight and is not, because **no check in
//! this phase opens a photograph**. Every inspection is a comparison between numbers phases 08 to 26
//! already measured and stored, so the pass is arithmetic over rows plus one catalog write.
//!
//! That is worth stating plainly rather than celebrating: the 90 s budget was written for a phase
//! that might have needed pixels, and this implementation does not. `crates/aura-qc/tests/
//! no_pixel_ops.rs` is what keeps it true - a future change that reached for the renderer to measure
//! something directly would blow this budget by two orders of magnitude, and the grep is what stops
//! it landing quietly.
//!
//! ## The storage row has phase 21's shape, not phase 09's
//!
//! Every migration from 09 to 26 stores **one fixed-width verdict** per photograph. This one stores
//! a **list** whose length is the number of things that were wrong with the frame, which is phase
//! 21's shape - and phase 21 learned that a per-image figure written before it is measured is wrong
//! by a factor of two.
//!
//! So the figure below is measured, and two structural bounds keep it bounded rather than open:
//! `MAX_TICKETS_PER_IMAGE` is eight and `MAX_ROUNDS` is two, so one photograph can own at most eight
//! tickets and sixteen rounds however badly it went wrong.
//!
//! **The denominator is selected frames**, not photographs. A QC check over a frame nobody is
//! delivering is not an inspection anybody asked for; phase 18 established that denominator and this
//! phase inherits it.
//!
//! ## The clean case is the common one and is measured separately
//!
//! A gallery with nothing wrong stores one `qc_run` row and nothing else - about 0.4 B/image at a
//! thousand frames. The budget is set against a **deliberately terrible** gallery where every frame
//! carries a finding, because a budget measured on the happy path is a budget that is met.

use std::sync::Arc;
use std::time::Instant;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::qc::{ImageId, QcCategory, MAX_ROUNDS, MAX_TICKETS_PER_IMAGE};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{AuraResult, ProjectId};
use aura_qc::api::{Field, QcPass};
use aura_qc::checks::{Frame, SetContext};
use aura_qc::policy::Thresholds;
use aura_qc::replace::{CandidateMetric, CoverageEffect};
use aura_qc::store::QcStore;
use aura_qc::{fixtures, report};
use rusqlite::params;

/// Section 11's whole-pass row, for a thousand images.
const PASS_MS_PER_1K: u128 = 90_000;

/// Section 11's per-round row.
const ROUND_MS: u128 = 1_200;

/// Section 11's report row.
const REPORT_MS: u128 = 3_000;

/// Not in section 11. Measured at 421 B/image on a gallery where **every** frame carries a finding.
///
/// The budget is 1,500 rather than 421 plus a little, and the headroom is named rather than
/// generous: that measurement is of an inspection with no remediation, and a ticket that went
/// through the loop owns up to two round rows and possibly a replacement row as well. One ticket
/// per image with both rounds and a swap measures about 1,100 B/image, which is the worst case a
/// real gallery could reach.
///
/// The structural ceiling is far higher and is bounded rather than open: eight tickets, sixteen
/// rounds and eight replacements is about 7.8 kB for one photograph. No gallery reaches it - a
/// frame with eight findings escalates whole as `MultiSymptom` - and the bound existing at all is
/// what the second assertion in the store test checks.
const BUDGET_BYTES_PER_IMAGE: u64 = 1_500;

/// How many frames the pass budget is measured over.
///
/// Two hundred rather than a thousand, because the pass is linear in frames and a thousand-frame run
/// in a debug build is thirty seconds of CI time to prove a bound that scales. The budget is scaled
/// to match, so the assertion is against the same rate section 11 states.
const PASS_FRAMES: usize = 200;

// ---------------------------------------------------------------------------
// A field over authored readings
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Bench {
    order: Vec<ImageId>,
    frames: std::collections::BTreeMap<ImageId, Frame>,
}

impl Field for Bench {
    fn selected(&self, _project: ProjectId) -> AuraResult<Vec<ImageId>> {
        Ok(self.order.clone())
    }

    fn frame(&self, image: ImageId) -> AuraResult<Frame> {
        Ok(self.frames.get(&image).cloned().unwrap_or_default())
    }

    fn coverage(&self, _project: ProjectId) -> AuraResult<SetContext> {
        Ok(fixtures::healthy_coverage())
    }

    fn coverage_effect(&self, _image: ImageId) -> AuraResult<CoverageEffect> {
        Ok(CoverageEffect::unprotected())
    }

    fn candidate(
        &self,
        _runner_up: ImageId,
        _category: QcCategory,
    ) -> AuraResult<Option<CandidateMetric>> {
        Ok(None)
    }
}

/// A gallery of `count` frames, every `every`th one carrying a defect.
fn gallery(count: usize, every: usize) -> Vec<Frame> {
    let defects = fixtures::defects();
    (0..count)
        .map(|index| {
            if every > 0 && index % every == 0 {
                let mut frame = defects[index % defects.len()].frame.clone();
                frame.image_id = ImageId::new();
                frame
            } else {
                fixtures::healthy(aura_core::contract::scene::SceneId::Ceremony)
            }
        })
        .collect()
}

fn setup(frames: &[Frame]) -> (tempfile::TempDir, QcStore, ProjectId, Bench) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("qc.sqlite"), Arc::clone(&clock), "perf")
            .expect("catalog opens at 27"),
    );
    let project = ProjectId::new();
    let key = project.to_db();
    let ids: Vec<String> = frames.iter().map(|frame| frame.image_id.to_db()).collect();

    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'perf', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for photo in &ids {
                conn.execute(
                    "INSERT OR IGNORE INTO photo (photo_id, project_id, created_at, updated_at)
                     VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![photo, key],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .expect("the fixture seeds");

    let mut bench = Bench {
        order: Vec::new(),
        frames: std::collections::BTreeMap::new(),
    };
    for frame in frames {
        bench.order.push(frame.image_id);
        bench.frames.insert(frame.image_id, frame.clone());
    }
    (dir, QcStore::new(catalog, clock), project, bench)
}

// ---------------------------------------------------------------------------
// The budgets
// ---------------------------------------------------------------------------

#[test]
fn a_qc_pass_over_a_wedding_is_inside_its_budget() {
    // Every fifth frame carries a defect, which is far worse than any real gallery and is the point:
    // a budget measured on the happy path is a budget that is met.
    let frames = gallery(PASS_FRAMES, 5);
    let (_dir, store, project, bench) = setup(&frames);
    let pass = QcPass::new(
        store,
        Thresholds::shipped().expect("the shipped table loads"),
    );

    // DETERMINISM: measuring a budget, not deciding. The same justification phases 12, 13 and 26
    // record for their own timers - nothing here reaches a stored row or a photograph.
    let started = Instant::now();
    let result = pass
        .inspect_only(project, &bench, 0, &CancelToken::new(), &NullProgress)
        .expect("the pass runs");
    let elapsed = started.elapsed().as_millis();

    let scaled = PASS_MS_PER_1K * PASS_FRAMES as u128 / 1_000;
    println!(
        "qc pass: {} frames in {} ms (budget {} ms at this size, {} ms per 1,000)",
        PASS_FRAMES, elapsed, scaled, PASS_MS_PER_1K
    );
    println!(
        "         {} tickets, {} checks run, {} skipped",
        result.tickets.len(),
        result.report.checks_run,
        result.report.skipped
    );
    assert!(
        elapsed <= scaled,
        "the pass took {elapsed} ms against a {scaled} ms budget at {PASS_FRAMES} frames"
    );
    assert!(result.report.complete(), "the pass reached every frame");
}

#[test]
fn one_remediation_round_is_inside_its_budget() {
    use aura_core::contract::qc::Remedy;
    use aura_qc::reedit::{Loop, Remediator};

    #[derive(Debug)]
    struct Instant2(Frame);

    impl Remediator for Instant2 {
        fn apply(
            &mut self,
            _image: ImageId,
            _remedy: &Remedy,
        ) -> Result<Frame, aura_core::AuraError> {
            Ok(self.0.clone())
        }

        fn revert(
            &mut self,
            _image: ImageId,
            _remedy: &Remedy,
        ) -> Result<Frame, aura_core::AuraError> {
            Ok(self.0.clone())
        }
    }

    let thresholds = Thresholds::reference();
    let reedit = Loop::new(&thresholds);
    let defect = fixtures::defects()
        .into_iter()
        .find(|d| d.category == QcCategory::Consistency)
        .expect("a consistency defect");
    let healthy = fixtures::healthy(defect.frame.scene);
    let finding = aura_qc::checks::findings_for(&defect.frame, &thresholds)
        .into_iter()
        .next()
        .expect("a finding");
    let ticket = aura_qc::ticket::from_finding(
        ProjectId::new(),
        &defect.frame,
        finding,
        Remedy::ResolveParam {
            target: aura_core::contract::qc::SolveTarget::Normalisation,
            constraint: "x".into(),
        },
        0,
    );
    let mut remediator = Instant2(healthy);

    // DETERMINISM: measuring a budget, not deciding.
    let started = Instant::now();
    reedit
        .run(
            &ticket,
            &defect.frame,
            &ticket.remedy,
            &mut remediator,
            0,
            0,
        )
        .expect("the round runs");
    let elapsed = started.elapsed().as_millis();

    println!("qc round: {elapsed} ms (budget {ROUND_MS} ms)");
    // What this measures is the loop's own arithmetic - re-inspection plus the collateral re-check -
    // with the remedy itself instantaneous. The real cost of a round is the deciding phase's
    // re-solve, which is that phase's budget rather than this one's.
    assert!(elapsed <= ROUND_MS, "one round took {elapsed} ms");
}

#[test]
fn report_generation_is_inside_its_budget() {
    let frames = gallery(PASS_FRAMES, 2);
    let (_dir, store, project, bench) = setup(&frames);
    let pass = QcPass::new(
        store.clone(),
        Thresholds::shipped().expect("the shipped table loads"),
    );
    let result = pass
        .inspect_only(project, &bench, 0, &CancelToken::new(), &NullProgress)
        .expect("the pass runs");

    // DETERMINISM: measuring a budget, not deciding.
    let started = Instant::now();
    let markdown = report::to_markdown(&result.report, &result.replacements);
    let elapsed = started.elapsed().as_millis();

    println!(
        "qc report: {elapsed} ms for {} bytes of markdown (budget {REPORT_MS} ms)",
        markdown.len()
    );
    assert!(elapsed <= REPORT_MS, "the report took {elapsed} ms");
    assert!(markdown.contains("What was checked"));
}

#[test]
fn the_cloud_ceiling_is_forty_calls_per_wedding() {
    // Section 11's fourth row is a bound rather than a measurement, and it is a constant rather than
    // a configuration - `qc_thresholds.toml` has no field for it, so a studio cannot raise it.
    assert_eq!(aura_core::contract::qc::MAX_PLANNER_CALLS, 40);
    // And migration 27 CHECKs the same number, so a row above it cannot be stored either.
    println!("qc cloud ceiling: {} calls per wedding", 40);
}

#[test]
fn the_store_is_inside_its_budget_and_is_bounded_by_the_ticket_cap() {
    let frames = gallery(400, 1);
    let (_dir, store, project, bench) = setup(&frames);
    let pass = QcPass::new(
        store.clone(),
        Thresholds::shipped().expect("the shipped table loads"),
    );
    pass.inspect_only(project, &bench, 0, &CancelToken::new(), &NullProgress)
        .expect("the pass runs");

    let outline = store.outline(project, 400).expect("an outline");
    let per_image = outline.bytes / 400;
    println!(
        "qc store: {} B over 400 frames = {} B/image (budget {} B/image)",
        outline.bytes, per_image, BUDGET_BYTES_PER_IMAGE
    );
    println!(
        "          {} tickets, {} rounds, {} replaced",
        outline.by_status.iter().sum::<u32>(),
        outline.rounds,
        outline.replaced
    );
    assert!(
        per_image <= BUDGET_BYTES_PER_IMAGE,
        "{per_image} B/image against a {BUDGET_BYTES_PER_IMAGE} B/image budget"
    );

    // The bound as well as the number. Phase 26 had to add this assertion after realising a size
    // check alone would pass on a build that had quietly removed its cap and happened to be
    // measured on a small fixture: one photograph can own at most `MAX_TICKETS_PER_IMAGE` tickets
    // and `MAX_TICKETS_PER_IMAGE * MAX_ROUNDS` rounds, whatever is wrong with it.
    let tickets: u32 = outline.by_status.iter().sum();
    let ceiling = u32::try_from(400 * MAX_TICKETS_PER_IMAGE).unwrap_or(u32::MAX);
    assert!(
        tickets <= ceiling,
        "{tickets} tickets over 400 frames exceeds the per-image cap of {MAX_TICKETS_PER_IMAGE}"
    );
    assert!(
        outline.rounds <= tickets * u32::from(MAX_ROUNDS),
        "more rounds than the two-per-ticket bound permits"
    );
}

#[test]
fn a_clean_gallery_costs_almost_nothing_to_store() {
    // The common case, measured separately so the budget above is visibly a ceiling on the bad case
    // rather than a description of the normal one.
    let frames: Vec<Frame> = fixtures::clean_gallery(400);
    let (_dir, store, project, bench) = setup(&frames);
    let pass = QcPass::new(
        store.clone(),
        Thresholds::shipped().expect("the shipped table loads"),
    );
    pass.inspect_only(project, &bench, 0, &CancelToken::new(), &NullProgress)
        .expect("the pass runs");

    let outline = store.outline(project, 400).expect("an outline");
    println!(
        "qc store, clean gallery: {} B over 400 frames = {} B/image",
        outline.bytes,
        outline.bytes / 400
    );
    assert_eq!(outline.by_status.iter().sum::<u32>(), 0, "nothing to file");
    assert!(
        outline.bytes / 400 < 10,
        "a clean gallery stores one run row"
    );
}
