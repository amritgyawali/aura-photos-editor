//! Phase 15 performance budgets, measured against the current analyser and store.
//!
//! Section 11 has three rows. All three are reported here and **one of them is not met**:
//!
//! | Row | Budget | This build |
//! |---|---|---|
//! | Estimation per image (GPU) | <= 25 ms | waived - there is no GPU backend (ADR-0007) |
//! | 4,000 images total | <= 100 s | waived, and extrapolated from the processor path |
//! | Extra storage per image | <= 600 B | **not met**; measured and recorded instead |
//!
//! The storage row is the interesting one and it is the reason this file exists rather than
//! a constant somewhere. `perf/budgets.toml` carries the measured figure and the argument
//! for it; this test is what produced that figure and what will fail if it moves.
//!
//! The two runtime rows name an RTX 4070 while this build exposes only the deterministic
//! processor backend, so they are waived rather than silently relabelled. What is measured
//! and guarded is the processor path: one 2048 px proxy per frame, the statistics pass, the
//! neutral detector, four hypotheses, the constrained solve and the exposure clamp. Neither
//! learned head is consulted - both are untrained (condition C1) - and measuring the
//! reference ONNX interpreter here would make this a phase 03 runtime benchmark rather than
//! a phase 15 regression guard.

use std::sync::Arc;

use aura_brain_photo::tone::analyse::{Analyser, FrameContext};
use aura_brain_photo::tone::fixtures::{self, Frame};
use aura_brain_photo::tone::store::ToneStore;
use aura_brain_photo::tone::BYTES_PER_IMAGE;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::tone::ToneEstimate;
use aura_core::{PhotoId, Priority, ProjectId};
use aura_infer::contract::infer::{
    HardwarePlan, InferError, InferRequest, InferResult, InferService, ModelRef, TensorView,
};
use aura_perf::{Budgets, Measurement, StageTimer};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use tempfile::TempDir;

const FRAMES: usize = if cfg!(debug_assertions) { 2 } else { 12 };
const STORAGE_FRAMES: usize = 1_000;

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
        "{}: {} ms over {} frames ({per_unit:.2} ms per image)",
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
fn one_frame_is_estimated_inside_the_processor_budget() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let analyser = Analyser::new(Arc::new(NullInfer::new()))
        .unwrap_or_else(|err| panic!("targets: {}", err.detail));
    let cases: Vec<Frame> = (0..fixtures::SKIN_TONES.len())
        .flat_map(|index| {
            [
                fixtures::daylight_ceremony(index),
                fixtures::tungsten_reception(index),
                fixtures::mixed_daylight_led(index),
            ]
        })
        .collect();

    let timer = StageTimer::start("tone_estimate_frame", clock.as_ref());
    let mut done = 0;
    for index in 0..FRAMES {
        let Some(frame) = cases.get(index % cases.len()) else {
            continue;
        };
        let buffer = proxy_sized(frame);
        analyser
            .analyse(
                PhotoId::new(),
                &buffer,
                &context_of(frame),
                &std::collections::BTreeMap::new(),
                Priority::AiBatch,
            )
            .unwrap_or_else(|err| panic!("analyse: {}", err.detail));
        done += 1;
    }
    let measurement = timer.finish(done as u64);
    let per_image = if done == 0 {
        0.0
    } else {
        measurement.elapsed_ms as f64 / done as f64
    };
    println!(
        "  extrapolated 4,000 images: {:.1} s on the processor path",
        per_image * 4_000.0 / 1_000.0
    );
    println!(
        "  section 11's 25 ms GPU row and 100 s 4,000-image row are waived: no GPU backend \
         is available in this build (ADR-0007)"
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_storage_cost_of_one_estimate_is_measured_rather_than_assumed() {
    // **Section 11's 600 B row, and the one number in this phase that does not meet its
    // budget.** The figure below is what `perf/budgets.toml` records and argues for; this
    // test is what produced it.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, before) = catalog_with_photos(&dir, Arc::clone(&clock), STORAGE_FRAMES);

    let templates = representative_estimates();
    assert!(
        templates.iter().any(|(estimate, _)| estimate.mixed_light),
        "the fixture set must include the widest row this table stores"
    );
    assert!(
        templates
            .iter()
            .any(|(estimate, _)| !estimate.alternatives.is_empty()),
        "and one that carries runner-up answers"
    );

    let photos = photo_ids(store.catalog(), &project);
    assert_eq!(photos.len(), STORAGE_FRAMES);
    for (index, photo) in photos.into_iter().enumerate() {
        let (template, numbers) = &templates[index % templates.len()];
        let estimate = ToneEstimate {
            image_id: photo,
            ..template.clone()
        };
        store
            .put(&project, &estimate, numbers)
            .unwrap_or_else(|err| panic!("store: {}", err.detail));
    }

    let after = page_bytes(store.catalog());
    let grown = after.saturating_sub(before);
    let per_image = grown as f64 / STORAGE_FRAMES as f64;
    let mean_reasons = templates
        .iter()
        .map(|(estimate, _)| estimate.reasons.len())
        .sum::<usize>() as f64
        / templates.len() as f64;
    let mean_lights = templates
        .iter()
        .map(|(estimate, _)| estimate.illuminants.len())
        .sum::<usize>() as f64
        / templates.len() as f64;
    println!(
        "tone store: {grown} bytes over {STORAGE_FRAMES} images ({per_image:.1} B/image), \
         fixture mean {mean_lights:.1} lights and {mean_reasons:.1} reasons/image"
    );
    println!(
        "  section 11 budgets 600 B/image. See perf/budgets.toml for the decomposition and \
         the waiver."
    );

    assert_eq!(
        BYTES_PER_IMAGE, 600,
        "the constant must keep naming the phase document's figure, met or not"
    );
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    if let Err(reason) = budgets().check_size("tone_store_per_1000_images", grown) {
        panic!("{reason}");
    }
}

// -- fixtures ----------------------------------------------------------------

/// One estimate per shape the table has to hold, with the numbers its reasons name.
fn representative_estimates() -> Vec<(ToneEstimate, Vec<Vec<f32>>)> {
    let analyser = Analyser::new(Arc::new(NullInfer::new()))
        .unwrap_or_else(|err| panic!("targets: {}", err.detail));
    let cases = [
        fixtures::daylight_ceremony(0),
        fixtures::tungsten_reception(2),
        fixtures::mixed_daylight_led(2),
        fixtures::backlit_exit(3),
        fixtures::purple_dance_floor(1),
        fixtures::red_mandap(4),
        fixtures::faceless_details(),
    ];
    cases
        .iter()
        .map(|frame| {
            let outcome = analyser
                .analyse(
                    PhotoId::new(),
                    &proxy_sized(frame),
                    &context_of(frame),
                    &std::collections::BTreeMap::new(),
                    Priority::AiBatch,
                )
                .unwrap_or_else(|err| panic!("analyse: {}", err.detail));
            (outcome.estimate, outcome.reason_numbers)
        })
        .collect()
}

/// The fixture, resampled to the rung the pass actually reads.
///
/// A 384 px fixture measured as though it were a proxy would report a statistics pass that
/// is thirty times cheaper than the one that ships.
fn proxy_sized(frame: &Frame) -> PixelBuffer {
    const SIDE: usize = 2_048;
    let height = SIDE * frame.height / frame.width.max(1);
    let mut out = vec![0u8; SIDE * height * 3];
    for y in 0..height {
        let source_y = y * frame.height / height.max(1);
        for x in 0..SIDE {
            let source_x = x * frame.width / SIDE;
            let from = (source_y * frame.width + source_x) * 3;
            let to = (y * SIDE + x) * 3;
            for channel in 0..3 {
                if let (Some(slot), Some(value)) =
                    (out.get_mut(to + channel), frame.rgb.get(from + channel))
                {
                    *slot = *value;
                }
            }
        }
    }
    PixelBuffer {
        width: SIDE as u32,
        height: height as u32,
        data: PixelData::Srgb8(out),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_of(frame: &Frame) -> FrameContext {
    FrameContext {
        scene: frame.scene,
        scene_known: true,
        attrs: frame.attrs,
        subjects: frame.subjects.clone(),
        as_shot: frame.as_shot,
        shadow_headroom_ev: 2.0,
    }
}

// -- catalog helpers ---------------------------------------------------------

fn catalog_with_photos(
    dir: &TempDir,
    clock: Arc<dyn Clock>,
    count: usize,
) -> (Arc<ToneStore>, ProjectId, u64) {
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("perf.sqlite"), Arc::clone(&clock), "0.1.0")
            .unwrap_or_else(|err| panic!("catalog: {}", err.detail)),
    );
    let project = ProjectId::new();
    let project_key = project.to_db();
    let ids: Vec<String> = (0..count).map(|_| PhotoId::new().to_db()).collect();
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'tone-perf', '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z')",
                rusqlite::params![project_key],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("project", &err))?;
            for (index, photo) in ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        camera_make, camera_model, iso, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
                             '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z')",
                    rusqlite::params![
                        photo,
                        project_key,
                        format!(
                            "2026-08-18T{:02}:{:02}:{:02}Z",
                            index / 3_600 % 24,
                            index / 60 % 60,
                            index % 60
                        ),
                    ],
                )
                .map_err(|err| aura_core::errors::db::statement_failed("photo", &err))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("seed: {}", err.detail));

    let before = page_bytes(&catalog);
    let store = Arc::new(ToneStore::new(catalog, clock));
    (store, project, before)
}

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

fn photo_ids(catalog: &Arc<Catalog>, project: &ProjectId) -> Vec<PhotoId> {
    let project_key = project.to_db();
    catalog
        .read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id FROM photo WHERE project_id = ?1 ORDER BY photo_id")
                .map_err(|err| aura_core::errors::db::statement_failed("photos", &err))?;
            let mut cursor = statement
                .query(rusqlite::params![project_key])
                .map_err(|err| aura_core::errors::db::statement_failed("photos", &err))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|err| aura_core::errors::db::statement_failed("photos", &err))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
        .unwrap_or_default()
}

/// An inference service that refuses every call.
///
/// Neither phase 15 head is consulted while both are untrained, so this changes nothing
/// about what is timed. See this file's header.
#[derive(Debug)]
struct NullInfer {
    plan: HardwarePlan,
}

impl NullInfer {
    fn new() -> Self {
        Self {
            plan: HardwarePlan::conservative(1, "1970-01-01T00:00:00Z".to_string()),
        }
    }
}

impl InferService for NullInfer {
    fn run(&self, _req: InferRequest<'_>) -> Result<InferResult, InferError> {
        Err(aura_core::errors::ml::cancelled("no head is consulted"))
    }

    fn run_batch(
        &self,
        _model: ModelRef,
        _inputs: Vec<Vec<TensorView<'_>>>,
        _prio: Priority,
    ) -> Result<Vec<InferResult>, InferError> {
        Err(aura_core::errors::ml::cancelled("no head is consulted"))
    }

    fn plan(&self) -> &HardwarePlan {
        &self.plan
    }

    fn warmup(&self, _models: &[ModelRef]) -> Result<(), InferError> {
        Ok(())
    }
}
