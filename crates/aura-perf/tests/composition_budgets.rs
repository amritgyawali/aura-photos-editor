//! Phase 11 performance budgets, measured against the current analyser and store.
//!
//! Section 11 has three rows. Storage is asserted exactly. The two runtime rows name an
//! RTX 4070, while this build exposes only the deterministic processor backend (ADR-0007),
//! so they are waived rather than silently relabelled: this suite measures and guards the
//! processor path, prints its 4,000-image extrapolation, and states why neither number is
//! evidence for the absent GPU path.
//!
//! The current product path deliberately withholds keypoint and learned-aesthetic claims
//! because both pinned heads are untrained placeholders. The timed path therefore measures
//! exactly what ships today: one 2048 px luminance plane, face-derived placement, horizon,
//! background, reference taste, flags, reasons, score and crop hint, plus the explicit
//! degradation. Real architecture-fixture execution is covered by
//! `aura-cli verify --phase 11`; measuring the reference interpreter here would make this
//! a phase 03 runtime benchmark rather than a phase 11 regression guard.

use std::sync::Arc;

use aura_brain_photo::composition::fixtures::{self, Canvas};
use aura_brain_photo::composition::keypoints::{AESTHETIC_MODEL, KEYPOINT_MODEL};
use aura_brain_photo::composition::store::{CompositionStore, Provenance};
use aura_brain_photo::composition::{Analyser, FrameContext, BYTES_PER_IMAGE};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::composition::{Box2, CompositionCode, CompositionResult};
use aura_core::contract::people::ImageSubjects;
use aura_core::{PhotoId, Priority, ProjectId};
use aura_infer::contract::infer::{
    ExecutionProvider, HardwarePlan, InferError, InferRequest, InferResult, InferService, ModelRef,
    Tensor, TensorView,
};
use aura_infer::onnx::fixtures::POSE_OUTPUTS;
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
fn analysing_a_proxy_has_a_measured_processor_guardrail() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut canvas = fixtures::bright_blob();
    canvas.add_face(
        Box2 {
            x: 0.50,
            y: 0.40,
            w: 0.12,
            h: 0.15,
        },
        52,
    );
    canvas.add_face(
        Box2 {
            x: 0.60,
            y: 0.42,
            w: 0.10,
            h: 0.13,
        },
        53,
    );
    let buffer = proxy_sized(&canvas);
    let context = context_of(&canvas);
    let analyser = Analyser::new(Arc::new(PlantedInfer::new()))
        .unwrap_or_else(|err| panic!("rules: {}", err.detail));

    // Warm allocations outside the timer. Placeholder trust guards intentionally prevent
    // either planted result from becoming a product observation.
    let warm = analyser
        .analyse(PhotoId::new(), &buffer, &context, Priority::AiBatch)
        .unwrap_or_else(|err| panic!("warm analysis: {}", err.detail));
    assert_eq!(warm.keypoint_subjects, 0);
    assert!(warm
        .reasons
        .iter()
        .any(|reason| reason.code == CompositionCode::KeypointsUnavailable));
    assert!(warm
        .reasons
        .iter()
        .any(|reason| reason.code == CompositionCode::AestheticUnavailable));

    let timer = StageTimer::start("composition_analyse_image_cpu", clock.as_ref());
    for _ in 0..FRAMES {
        let result = analyser
            .analyse(PhotoId::new(), &buffer, &context, Priority::AiBatch)
            .unwrap_or_else(|err| panic!("analysis: {}", err.detail));
        assert!(!result.reasons.is_empty());
        assert_eq!(result.keypoint_subjects, 0);
    }
    let measurement = timer.finish(u64::try_from(FRAMES).unwrap_or(0));
    let full_wedding_seconds = measurement.elapsed_ms as f64 / FRAMES as f64 * 4_000.0 / 1_000.0;
    println!(
        "  processor extrapolation: {full_wedding_seconds:.0} s for 4,000 images with three faces"
    );
    println!(
        "  section 11 GPU <=30 ms and RTX 4070 <=120 s rows are waived: no GPU backend \
         is available in this build (ADR-0007)"
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn a_composition_judgement_stays_under_eight_hundred_bytes() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, before) = catalog_with_photos(&dir, Arc::clone(&clock), STORAGE_FRAMES);
    let templates = representative_results();
    assert!(templates
        .iter()
        .any(|result| !result.bright_blobs.is_empty()));
    assert!(templates
        .iter()
        .any(|result| result.crop_suggestion_hint.is_some()));

    let photos = photo_ids(store.catalog(), &project);
    assert_eq!(photos.len(), STORAGE_FRAMES);
    for (index, photo) in photos.into_iter().enumerate() {
        let result = CompositionResult {
            image_id: photo,
            ..templates[index % templates.len()].clone()
        };
        store
            .put(&project, &result, &Provenance::default())
            .unwrap_or_else(|err| panic!("store: {}", err.detail));
    }

    let after = page_bytes(store.catalog());
    let grown = after.saturating_sub(before);
    let per_image = grown as f64 / STORAGE_FRAMES as f64;
    let mean_cuts = templates
        .iter()
        .map(|result| result.joint_cuts.len())
        .sum::<usize>() as f64
        / templates.len() as f64;
    let mean_reasons = templates
        .iter()
        .map(|result| result.reasons.len())
        .sum::<usize>() as f64
        / templates.len() as f64;
    println!(
        "composition store: {grown} bytes over {STORAGE_FRAMES} images \
         ({per_image:.1} B/image), fixture mean {mean_cuts:.1} cuts and \
         {mean_reasons:.1} reasons/image"
    );

    assert_eq!(BYTES_PER_IMAGE, 800, "store and phase budget must agree");
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    if let Err(reason) = budgets().check_size("composition_store_per_1000_images", grown) {
        panic!("{reason}");
    }
}

fn representative_results() -> Vec<CompositionResult> {
    let analyser = Analyser::new(Arc::new(PlantedInfer::new()))
        .unwrap_or_else(|err| panic!("rules: {}", err.detail));
    let cases = [
        fixtures::intentional_dutch(),
        fixtures::accidental_dutch(),
        fixtures::cut_at_wrist(),
        fixtures::cut_mid_limb(),
        fixtures::tight_but_uncut(),
        fixtures::head_cropped(),
        fixtures::bright_blob(),
        fixtures::head_merge(),
        fixtures::colour_clash(),
        fixtures::flat_lay(),
        fixtures::tilted(0.2),
    ];
    cases
        .iter()
        .map(|canvas| {
            analyser
                .analyse(
                    PhotoId::new(),
                    &buffer_of(canvas),
                    &context_of(canvas),
                    Priority::AiBatch,
                )
                .unwrap_or_else(|err| panic!("fixture: {}", err.detail))
        })
        .collect()
}

fn proxy_sized(canvas: &Canvas) -> PixelBuffer {
    const SIDE: usize = 2_048;
    let mut rgb = vec![0u8; SIDE * SIDE * 3];
    for y in 0..SIDE {
        let source_y = y * canvas.height / SIDE;
        for x in 0..SIDE {
            let source_x = x * canvas.width / SIDE;
            let source = (source_y * canvas.width + source_x) * 3;
            let target = (y * SIDE + x) * 3;
            for channel in 0..3 {
                if let (Some(slot), Some(value)) = (
                    rgb.get_mut(target + channel),
                    canvas.rgb.get(source + channel).copied(),
                ) {
                    *slot = value;
                }
            }
        }
    }
    PixelBuffer {
        width: SIDE as u32,
        height: SIDE as u32,
        data: PixelData::Srgb8(rgb),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn buffer_of(canvas: &Canvas) -> PixelBuffer {
    PixelBuffer {
        width: canvas.width as u32,
        height: canvas.height as u32,
        data: PixelData::Srgb8(canvas.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_of(canvas: &Canvas) -> FrameContext {
    FrameContext {
        scene: canvas.truth.scene,
        subjects: ImageSubjects {
            faces: canvas.faces.clone(),
            people_count: u32::try_from(canvas.faces.len()).unwrap_or(0),
            weights_ver: 1,
            ..ImageSubjects::default()
        },
        scene_known: true,
        gravity_deg: None,
    }
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

fn catalog_with_photos(
    dir: &TempDir,
    clock: Arc<dyn Clock>,
    count: usize,
) -> (Arc<CompositionStore>, ProjectId, u64) {
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
                 VALUES (?1, 'composition-perf', '2026-08-15T00:00:00Z',
                         '2026-08-15T00:00:00Z')",
                rusqlite::params![project_key],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("project", &err))?;
            for (index, photo) in ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        camera_make, camera_model, iso, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
                             '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
                    rusqlite::params![
                        photo,
                        project_key,
                        format!(
                            "2026-08-15T{:02}:{:02}:{:02}Z",
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
    (
        Arc::new(CompositionStore::new(catalog, clock)),
        project,
        before,
    )
}

#[derive(Debug)]
struct PlantedInfer {
    plan: HardwarePlan,
}

impl PlantedInfer {
    fn new() -> Self {
        Self {
            plan: HardwarePlan::conservative(1, "1970-01-01T00:00:00Z".to_string()),
        }
    }

    fn result(model: ModelRef) -> InferResult {
        let (shape, values) = if model.name == KEYPOINT_MODEL {
            (vec![1, POSE_OUTPUTS], vec![0.5; POSE_OUTPUTS])
        } else {
            debug_assert_eq!(model.name, AESTHETIC_MODEL);
            (vec![1, 1], vec![0.62])
        };
        InferResult {
            outputs: vec![Tensor {
                shape,
                data: values,
            }],
            ep_used: ExecutionProvider::Cpu,
            latency_ms: 0.0,
            batch_size: 1,
        }
    }
}

impl InferService for PlantedInfer {
    fn run(&self, request: InferRequest<'_>) -> Result<InferResult, InferError> {
        Ok(Self::result(request.model))
    }

    fn run_batch(
        &self,
        model: ModelRef,
        inputs: Vec<Vec<TensorView<'_>>>,
        _priority: Priority,
    ) -> Result<Vec<InferResult>, InferError> {
        Ok(inputs.iter().map(|_| Self::result(model)).collect())
    }

    fn plan(&self) -> &HardwarePlan {
        &self.plan
    }

    fn warmup(&self, _models: &[ModelRef]) -> Result<(), InferError> {
        Ok(())
    }
}
