//! Phase 06 budgets, asserted against the real pipeline and the real store.
//!
//! Three of section 11's five rows are here. The two that are not - 4,000 images in
//! 240 s on an RTX 4070 and 480 s on an M3 Pro - describe hardware this build has no
//! backend for, and a green test that measures nothing is worse than an honest waiver.
//! They are waived in
//! `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` section 6, and the
//! per-image processor row below stands in for them.
//!
//! The faces here are synthetic and the models are the shipped placeholders, so these
//! numbers prove the *shape* of the cost - that the detector runs once per frame and not
//! once per face, that clustering is bounded by the skeleton cap rather than by the
//! project, that a face row is kilobytes rather than megabytes - and they catch a
//! regression the moment one lands. They do not predict what a trained SCRFD will cost,
//! and `docs/progress/PHASE-06-EXIT.md` says so.

use std::path::Path;
use std::sync::Arc;

use aura_core::clock::{Clock, SystemClock};
use aura_core::Priority;
use aura_infer::contract::infer::{BatchDefaults, HardwarePlan, ModelClass, Precision};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures as onnx_fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_people::store::BYTES_PER_FACE;
use aura_perf::{Budgets, Measurement, StageTimer};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use aura_vision::face::cluster::{self, ClusterConfig, ClusterFace, MAX_SKELETON};
use aura_vision::face::fixtures::{render_frame, synthetic_template, SynthFace};
use aura_vision::face::{FacePipeline, FACE_LEVEL};
use tempfile::TempDir;

/// Frames measured per run. A handful: the budget is per image, and the detector pass is
/// the dominant cost, so eight frames is enough to average out one slow scheduler
/// admission without adding a minute to `just budgets`.
const FRAMES: usize = if cfg!(debug_assertions) { 2 } else { 8 };

/// The skeleton cap, which is the size section 11's twelve-second clustering budget
/// actually constrains. A quarter of it in debug, where nothing is asserted.
const SKELETON: usize = if cfg!(debug_assertions) {
    MAX_SKELETON / 8
} else {
    MAX_SKELETON
};

/// Assert a timing budget, or explain why it was only reported.
///
/// The same shape phase 04 and phase 05 use, and for the same reason: an unoptimised
/// build is three to ten times slower than the binary a photographer runs, so asserting a
/// release budget against a debug build would either fail constantly or force the budget
/// up to a number that means nothing.
fn assert_timing(budgets: &Budgets, measurement: &Measurement) {
    let per_unit = if measurement.units == 0 {
        0.0
    } else {
        measurement.elapsed_ms as f64 / measurement.units as f64
    };
    println!(
        "{}: {} ms over {} units ({per_unit:.2} ms per unit)",
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

/// The three face models over the real inference service.
fn pipeline(dir: &TempDir) -> FacePipeline {
    let detect = dir.path().join("face_detect.onnx");
    let embed = dir.path().join("face_embed.onnx");
    let quality = dir.path().join("face_quality.onnx");
    std::fs::write(&detect, serialise(&onnx_fixtures::face_detect())).expect("detector");
    std::fs::write(&embed, serialise(&onnx_fixtures::face_embed())).expect("recogniser");
    std::fs::write(&quality, serialise(&onnx_fixtures::face_quality())).expect("head");

    let source = Arc::new(
        StaticModelSource::new()
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::detect::DETECT_MODEL,
                    aura_vision::face::detect::DETECT_MODEL_VERSION,
                ),
                &detect,
                Precision::Fp32,
                ModelClass::Segmentation,
                32,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::embed::EMBED_MODEL,
                    aura_vision::face::embed::EMBED_MODEL_VERSION,
                ),
                &embed,
                Precision::Fp32,
                ModelClass::Embedding,
                16,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::quality::QUALITY_MODEL,
                    aura_vision::face::quality::QUALITY_MODEL_VERSION,
                ),
                &quality,
                Precision::Fp32,
                ModelClass::Embedding,
                16,
            ),
    );
    let mut plan = HardwarePlan::conservative(4, "2026-08-13T00:00:00Z".to_string());
    plan.default_batch = BatchDefaults {
        embedding: 8,
        segmentation: 2,
        retouch: 1,
    };
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    FacePipeline::new(Arc::new(InferEngine::new(
        BackendRegistry::with_reference(),
        source,
        plan,
        clock,
    )))
    // The shipped detector is a placeholder, so it proposes anchors everywhere and the
    // suppression keeps `max_faces` of them - sixty-four, which is not what a wedding
    // frame costs. Capping at three makes the measurement representative: one 640 px
    // detector pass, which is the dominant cost and is real, plus the per-face work at
    // roughly the fixture set's rate. Capping is honest here precisely because the
    // *count* comes from weights that carry no meaning; the *cost per face* does not.
    .with_detect_config(aura_vision::face::detect::DetectConfig {
        max_faces: 3,
        ..aura_vision::face::detect::DetectConfig::default()
    })
}

fn frame(width: u32, height: u32, faces: &[SynthFace]) -> PixelBuffer {
    PixelBuffer {
        width,
        height,
        data: PixelData::Srgb8(render_frame(width as usize, height as usize, faces)),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 1,
    }
}

#[test]
fn one_frame_stays_inside_the_processor_budget() {
    let dir = TempDir::new().expect("a temporary directory");
    let pipeline = pipeline(&dir).without_crops();
    let clock = SystemClock::default();

    // Two faces and a body each: the fixture set's 2.4 faces per frame, rounded to a
    // shape the renderer can draw.
    let faces = vec![
        SynthFace::new([0.32, 0.40], 0.24, 1),
        SynthFace::new([0.66, 0.42], 0.22, 2),
    ];
    let buffer = frame(1600, 1200, &faces);
    let hint = aura_vision::face::detect::FrameHint {
        focal_len_mm: Some(85.0),
        person_boxes: 0,
    };

    // One warm frame first: the first call through `InferService` loads and compiles
    // three models, and a budget that included the cold load would be measuring phase 03.
    drop(pipeline.analyse(
        aura_core::PhotoId::new(),
        &buffer,
        FACE_LEVEL,
        &hint,
        Priority::AiBatch,
    ));

    let timer = StageTimer::start("face_scan_image_cpu", &clock);
    let mut faces_found = 0usize;
    for _ in 0..FRAMES {
        let result = pipeline
            .analyse(
                aura_core::PhotoId::new(),
                &buffer,
                FACE_LEVEL,
                &hint,
                Priority::AiBatch,
            )
            .expect("the pass must run");
        faces_found += result.faces.len();
    }
    let measurement = timer.finish(FRAMES as u64);
    println!("  {faces_found} faces over {FRAMES} frames");
    assert_timing(&budgets(), &measurement);
}

#[test]
fn clustering_a_full_skeleton_stays_inside_the_budget() {
    // Section 11: 25,000 faces in 12 s. The quadratic part is capped at `MAX_SKELETON`,
    // so this measures the cap - past it the work is linear and the cap is what the
    // budget constrains. If this passes and a 25,000-face project is slow, the second
    // pass has become quadratic and that is the regression worth catching.
    let clock = SystemClock::default();
    let faces: Vec<ClusterFace> = (0..SKELETON)
        .map(|index| ClusterFace {
            key: index as u64,
            template: synthetic_template((index / 64) as u32, (index % 64) as u32, 0.40, None),
            quality: 0.8,
            votes: true,
        })
        .collect();

    let timer = StageTimer::start("identity_cluster_skeleton", &clock);
    let outcome = cluster::cluster(&faces, &ClusterConfig::default());
    let measurement = timer.finish(1);
    println!(
        "  {} faces into {} identities, {} refused merges",
        faces.len(),
        outcome.clusters.len(),
        outcome.refused_merges
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_face_store_stays_inside_its_size_budget() {
    // Section 11: 25 MB per 1,000 images. At the fixture set's 2.4 faces per frame that
    // is 2,400 faces, and each one costs its catalog row plus its sealed crop.
    let faces_per_1000_images = 2_400u64;
    let crop_bytes = aura_people::store::BYTES_PER_SEALED_CROP as u64;
    let per_face = BYTES_PER_FACE as u64 + crop_bytes;
    let total = per_face * faces_per_1000_images;

    println!(
        "face store: {per_face} bytes per face ({} row + {crop_bytes} sealed crop), \
         {total} bytes per 1,000 images",
        BYTES_PER_FACE
    );
    if let Err(reason) = budgets().check_size("face_store_per_1000_images", total) {
        panic!("{reason}");
    }
}

#[test]
fn the_detector_runs_once_per_frame_and_not_once_per_face() {
    // The shape of the cost, asserted rather than assumed. A frame with six faces must
    // cost one detector pass and six crops - not six detector passes - and the difference
    // is the whole reason the pipeline batches inside a frame.
    let dir = TempDir::new().expect("a temporary directory");
    let pipeline = pipeline(&dir).without_crops();
    let hint = aura_vision::face::detect::FrameHint {
        focal_len_mm: Some(85.0),
        person_boxes: 0,
    };

    let one = frame(1200, 900, &[SynthFace::new([0.5, 0.4], 0.22, 1)]);
    let many = frame(
        1200,
        900,
        &(0..6)
            .map(|index| SynthFace::new([0.14 + index as f32 * 0.14, 0.40], 0.14, index as u32 + 1))
            .collect::<Vec<_>>(),
    );

    for buffer in [&one, &many] {
        drop(pipeline.analyse(
            aura_core::PhotoId::new(),
            buffer,
            FACE_LEVEL,
            &hint,
            Priority::AiBatch,
        ));
    }

    let clock = SystemClock::default();
    let time = |buffer: &PixelBuffer| -> u64 {
        let started = clock.monotonic_us();
        let result = pipeline
            .analyse(
                aura_core::PhotoId::new(),
                buffer,
                FACE_LEVEL,
                &hint,
                Priority::AiBatch,
            )
            .expect("the pass must run");
        let elapsed = clock.monotonic_us().saturating_sub(started);
        println!("  {} faces in {elapsed} us", result.faces.len());
        elapsed
    };

    let single = time(&one);
    let crowd = time(&many);
    if cfg!(debug_assertions) {
        return;
    }
    assert!(
        crowd < single * 3,
        "six faces cost {crowd} us against one face at {single} us; the detector is being \
         run per face"
    );
}
