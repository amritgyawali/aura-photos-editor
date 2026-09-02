#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args)]

//! PHASE-29 section 11's budgets, as tests.
//!
//! | Metric | Budget |
//! |---|---|
//! | Full curation for a 1,000-image gallery | <= 20 s |
//! | Album re-composition after a swap | <= 1.5 s |
//! | B&W mix generation per image | <= 25 ms |
//!
//! Plus a storage row section 11 does not name. Every phase since 21 has measured one anyway.
//!
//! ## Why this phase's pass is fast, and why that is not a boast
//!
//! A whole wedding curated inside twenty seconds sounds tight and is not, because **nothing in this
//! phase opens a photograph**. Every reading is a number some earlier phase measured and stored, so
//! the pass is arithmetic over rows plus one catalog write. `crates/aura-curate/tests/no_outputs.rs`
//! is what keeps it true: a future change that reached for the renderer to measure a mix directly
//! would blow this budget by two orders of magnitude, and the grep is what stops it landing quietly.
//!
//! The one term that could have broken it is uniqueness, which asks phase 05's index how unlike the
//! already-chosen heroes a frame is. That is `candidates x target^2 / 2` readings - linear in the
//! gallery with a constant near two hundred, not quadratic in it - and
//! `tests/eval/curate_eval.rs::the_uniqueness_term_grows_with_the_gallery_rather_than_with_its_square`
//! asserts the shape by doubling the wedding rather than trusting the constant.
//!
//! ## The re-composition row is the one a photographer feels
//!
//! Twenty seconds happens once, in the background, after a cull. A re-composition happens every time
//! somebody drags a spread, in front of them, and 1.5 s is already at the edge of what reads as
//! instant. It is measured here as what the IPC command actually does - record the order, then
//! re-compose the album in that order - because that is one command and one wait. ADR-0060 section
//! 4.
//!
//! ## The storage row has phase 21's shape, not phase 09's
//!
//! Every migration from 09 to 20 stores one fixed-width verdict per photograph. This one stores a
//! **selection**: eighty album images out of a gallery of hundreds, twenty heroes, a bounded
//! monochrome list, and the reasons under each of them. So the per-image figure *falls* as a gallery
//! grows, because the numerator is capped by the contract and the denominator is not.
//!
//! **The denominator is selected frames**, not photographs - phase 18's rule, which this phase
//! inherits because a monochrome suggestion about a frame nobody is delivering is not a gap.
//!
//! **The bound is asserted as well as the number**, by running the same pass over a doubled gallery
//! and checking the store did not double with it. Phase 26 learned that from the other side, when a
//! note about a table growing with the square of a wedding's overlap turned out to describe a table
//! that was capped. A size assertion alone would pass on a build that had removed the cap and
//! happened to be measured on a small fixture.

use std::sync::Arc;
use std::time::Instant;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::curate::{ImageId, ALBUM_DEFAULT};
use aura_core::ProjectId;
use aura_curate::api::CuratePass;
use aura_curate::fixtures::{self, FixtureField, Shape};
use aura_curate::policy::{Policy, DEFAULT_TOML};
use aura_curate::read::Field;
use aura_curate::store::CurateStore;
use aura_curate::bw;
use rusqlite::params;

/// Section 11's whole-pass row, for a thousand images.
const PASS_MS_PER_1K: u128 = 20_000;

/// Section 11's re-composition row.
const RECOMPOSE_MS: u128 = 1_500;

/// Section 11's per-image monochrome row.
const MIX_MS: u128 = 25;

/// How many frames the pass budget is measured over.
///
/// Six hundred rather than a thousand, because the pass is linear in frames and the budget is
/// scaled to match, so the assertion is against the same rate section 11 states. Six hundred is
/// also what every gate in this phase runs on, so a regression shows up in the same shape twice.
const PASS_FRAMES: u32 = 600;

/// Not in section 11. Measured at **211 B per selected image over a 600-frame gallery**, and at
/// 2,440 B/image over the smallest gallery this fixture can build.
///
/// Two numbers because the figure *falls* as a wedding grows, which is the opposite of every
/// migration from 09 to 20 and is why the budget is set at the small end. Three things here are
/// capped by the contract rather than by the gallery - the album at `ALBUM_MAX` images, the
/// portfolio at `HERO_TARGET`, the captions at `MAX_CAPTIONS` - so on a ten-frame gallery the album
/// *is* the gallery and every frame carries album rows, while on a six-hundred-frame one the same
/// bounded set is spread over sixty times as many images.
///
/// The only row count that grows with a wedding is `curate_bw`, one row per offered conversion, and
/// `BW_CANDIDATE_FLOOR` is what bounds that: 90 offers at 600 frames and 191 at 1,200. The second
/// assertion doubles the wedding and checks the store grew by 1.62x rather than 2x, which is that
/// one unbounded term and nothing else.
const BUDGET_BYTES_PER_IMAGE: u64 = 4_000;

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

/// A catalog at migration 29, seeded with one project and its photographs.
fn setup(field: &FixtureField) -> (tempfile::TempDir, CurateStore, ProjectId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
    let catalog = Arc::new(
        Catalog::open(
            &dir.path().join("curate.sqlite"),
            Arc::clone(&clock),
            "perf",
        )
        .expect("catalog opens at 29"),
    );
    let project = field.wedding().project;
    let key = project.to_db();
    let photos: Vec<String> = field
        .wedding()
        .frames
        .iter()
        .map(|frame| frame.image_id.to_db())
        .collect();

    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'perf', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for photo in &photos {
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

    (dir, CurateStore::new(catalog, clock), project)
}

fn policy() -> Policy {
    Policy::load_str(DEFAULT_TOML).expect("the shipped curation table parses")
}

// ---------------------------------------------------------------------------
// The budgets
// ---------------------------------------------------------------------------

#[test]
fn a_whole_curation_is_inside_its_budget() {
    // The `complete` shape rather than `as shipped`, because it is the slower one: faces mean shot
    // scales, which mean a rhythm measured over the whole album rather than over a seventh of it,
    // and skin loci mean a mix solved under a constraint on every frame.
    let field = FixtureField::new(fixtures::wedding(Shape::complete(PASS_FRAMES), 29));
    let (_dir, store, project) = setup(&field);
    let policy = policy();
    let pass = CuratePass::new(&field, &policy, &store, 1);

    // DETERMINISM: measuring a budget, not deciding. The same justification phases 12, 13, 26 and
    // 27 record for their own timers - nothing here reaches a stored row or a photograph.
    let started = Instant::now();
    let outline = pass.run(project, None, None).expect("the pass runs");
    let elapsed = started.elapsed().as_millis();

    let scaled = PASS_MS_PER_1K * u128::from(PASS_FRAMES) / 1_000;
    println!(
        "curation: {} frames in {} ms (budget {} ms at this size, {} ms per 1,000)",
        PASS_FRAMES, elapsed, scaled, PASS_MS_PER_1K
    );
    println!(
        "          {} heroes, {} spreads, {} album images, {} monochrome offers",
        outline.heroes, outline.spreads, outline.album_size, outline.bw_offered
    );
    assert!(
        elapsed <= scaled,
        "curation took {elapsed} ms against a {scaled} ms budget at {PASS_FRAMES} frames"
    );
}

#[test]
fn a_re_composition_after_a_drag_is_inside_its_budget() {
    // What the IPC command does, timed as one thing: record the photographer's order, then rebuild
    // the album in it. Splitting the two would measure a wait nobody has.
    let field = FixtureField::new(fixtures::wedding(Shape::complete(PASS_FRAMES), 30));
    let (_dir, store, project) = setup(&field);
    let policy = policy();
    let pass = CuratePass::new(&field, &policy, &store, 1);
    pass.run(project, None, None).expect("the first pass runs");

    let album = store
        .album(project)
        .expect("the album is stored")
        .expect("the pass wrote one");

    // One drag, **inside one chapter**. Not across: `check_order` refuses an order that reorders
    // chapters and the schema refuses the album it would produce, so a cross-chapter drag would be
    // timing a refusal rather than a re-composition. The first version of this test did exactly
    // that and the pass came back `AURA-ML-5144` from `album_chapter`'s own primary key - which is
    // the guarantee working, in the layer phase 21's rule says to put it in, and not a budget.
    let chapter = album
        .spreads
        .first()
        .map(|spread| spread.chapter)
        .expect("the album has a chapter");
    let mut order: Vec<ImageId> = Vec::new();
    let mut first_block: Vec<ImageId> = Vec::new();
    for spread in &album.spreads {
        if spread.chapter == chapter {
            first_block.extend(spread.images());
        } else {
            order.extend(spread.images());
        }
    }
    assert!(first_block.len() >= 2, "a chapter with something to drag");
    let moved = first_block.remove(first_block.len() - 1);
    first_block.insert(0, moved);
    let order = {
        let mut whole = first_block;
        whole.extend(order);
        whole
    };

    let started = Instant::now();
    store
        .set_order(project, &order)
        .expect("the order is recorded");
    pass.run(project, None, None)
        .expect("the album re-composes");
    let elapsed = started.elapsed().as_millis();

    println!(
        "re-composition: {} images in {} ms (budget {} ms)",
        order.len(),
        elapsed,
        RECOMPOSE_MS
    );
    assert!(
        elapsed <= RECOMPOSE_MS,
        "a re-composition took {elapsed} ms against a {RECOMPOSE_MS} ms budget"
    );
}

#[test]
fn one_monochrome_mix_is_inside_its_budget() {
    // Per image, so it is measured per image: the whole candidate list over a gallery divided by the
    // frames it read. A mean over a batch would hide a solver whose cost depended on how many bands
    // a frame has, which is exactly the shape `bw::solve` has.
    let field = FixtureField::new(fixtures::wedding(Shape::complete(PASS_FRAMES), 31));
    let frames = field.frames(field.wedding().project).expect("frames");
    let loci = field
        .skin_bands(field.wedding().project)
        .expect("skin bands");
    let policy = policy();

    let started = Instant::now();
    let picks = bw::candidates(&frames, &loci, &policy);
    let elapsed = started.elapsed().as_micros();

    let per_image = elapsed / u128::from(PASS_FRAMES);
    println!(
        "monochrome: {} frames read in {} us, {} us each (budget {} ms), {} offered",
        PASS_FRAMES,
        elapsed,
        per_image,
        MIX_MS,
        picks.len()
    );
    assert!(
        per_image <= MIX_MS * 1_000,
        "a mix took {per_image} us against a {} us budget",
        MIX_MS * 1_000
    );
}

#[test]
fn the_curation_store_is_inside_its_budget_and_does_not_grow_with_the_gallery() {
    let policy = policy();

    let mut measured: Vec<(u32, u64, u32)> = Vec::new();
    for frames_in in [PASS_FRAMES, PASS_FRAMES * 2] {
        let field = FixtureField::new(fixtures::wedding(Shape::complete(frames_in), 32));
        let (_dir, store, project) = setup(&field);
        let pass = CuratePass::new(&field, &policy, &store, 1);
        let outline = pass.run(project, None, None).expect("the pass runs");
        measured.push((frames_in, outline.bytes, outline.bw_offered));
    }

    let (frames_in, bytes, offers) = measured[0];
    let per_image = bytes / u64::from(frames_in);
    println!(
        "curation store: {} B over {} selected frames = {} B/image (budget {} B/image), {} offers",
        bytes, frames_in, per_image, BUDGET_BYTES_PER_IMAGE, offers
    );
    assert!(
        per_image <= BUDGET_BYTES_PER_IMAGE,
        "the store costs {per_image} B/image against a {BUDGET_BYTES_PER_IMAGE} B budget"
    );

    // The shape, not only the size. Doubling the gallery must not double the store: the album, the
    // portfolio and the captions are all capped by the contract, so the only term that may grow is
    // the monochrome list.
    let (big_frames, big_bytes, big_offers) = measured[1];
    let growth = big_bytes as f64 / bytes.max(1) as f64;
    println!(
        "curation store: {} B over {} frames = {:.2}x for twice the gallery ({} offers)",
        big_bytes, big_frames, growth, big_offers
    );
    assert!(
        growth < 1.9,
        "doubling the gallery multiplied the store by {growth:.2}, so something in it is \
         uncapped"
    );
    assert!(
        big_bytes / u64::from(big_frames) <= BUDGET_BYTES_PER_IMAGE,
        "the per-image figure did not fall as the gallery grew"
    );
}

/// The small end of the same measurement, and the end the budget is set at.
///
/// `Shape::complete(1)` asks for one frame and produces ten, because `fixtures::wedding` gives every
/// chapter of its plan at least one - which is the right floor to measure: a wedding with no
/// ceremony in it is not a small wedding, it is a broken fixture. Ten frames is a gallery whose
/// album is the whole gallery, so every frame carries album rows and the per-image figure is at its
/// worst.
#[test]
fn the_smallest_gallery_is_where_the_per_image_figure_is_worst() {
    let field = FixtureField::new(fixtures::wedding(Shape::complete(1), 33));
    let (_dir, store, project) = setup(&field);
    let policy = policy();
    let pass = CuratePass::new(&field, &policy, &store, 1);
    let outline = pass.run(project, None, None).expect("the pass runs");

    let frames = u64::from(outline.selected.max(1));
    let per_image = outline.bytes / frames;
    println!(
        "curation store: {} B over {} selected frames = {} B/image (budget {} B/image), {} album \
         images",
        outline.bytes, frames, per_image, BUDGET_BYTES_PER_IMAGE, outline.album_size
    );
    assert!(
        per_image <= BUDGET_BYTES_PER_IMAGE,
        "the smallest gallery costs {per_image} B/image against a {BUDGET_BYTES_PER_IMAGE} B budget"
    );
    assert!(
        outline.album_size <= ALBUM_DEFAULT,
        "an album larger than the default came out of a ten-frame gallery"
    );
}
