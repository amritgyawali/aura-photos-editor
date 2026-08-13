#![allow(dead_code)]

//! A real runtime over the real model, and synthetic photographs with known
//! content.
//!
//! Nothing here is a double of the inference service. The point of phase 03 was
//! that `InferService` is the one way to run a model, and a phase 05 test that
//! stubbed it out would be testing its own stub. The model is the signed
//! placeholder from `models/`, generated on the fly so the test does not depend
//! on `cargo xtask models --generate` having been run.

use std::sync::Arc;

use aura_core::clock::{Clock, SystemClock};
use aura_infer::contract::infer::{
    BatchDefaults, HardwarePlan, InferService, ModelClass, Precision,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use aura_vision::embed::model::model_ref;
use tempfile::TempDir;

/// An inference service running the phase 05 embedding model.
#[must_use]
pub fn engine(dir: &TempDir, batch: u16) -> Arc<dyn InferService> {
    let path = dir.path().join("wedding_embedding.onnx");
    std::fs::write(&path, serialise(&fixtures::wedding_embedding())).expect("write the model");

    let source = Arc::new(StaticModelSource::new().with(
        model_ref(),
        &path,
        Precision::Fp32,
        ModelClass::Embedding,
        16,
    ));

    let mut plan = HardwarePlan::conservative(4, "2026-08-13T00:00:00Z".to_string());
    plan.default_batch = BatchDefaults {
        embedding: batch,
        segmentation: 1,
        retouch: 1,
    };

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    Arc::new(InferEngine::new(
        BackendRegistry::with_reference(),
        source,
        plan,
        clock,
    ))
}

/// A synthetic photograph with structure a descriptor can find.
///
/// `hue_shift` moves the colour, `brightness` scales the whole frame, and
/// `stripes` puts hard vertical edges in it. Three knobs, so a test can assert
/// that the histogram follows the hue, the luminance statistics follow the
/// brightness and the edge energy follows the stripes - rather than asserting only
/// that some number came back.
#[must_use]
pub fn synthetic(
    width: u32,
    height: u32,
    hue_shift: f32,
    brightness: f32,
    stripes: u32,
) -> PixelBuffer {
    let mut bytes = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let across = x as f32 / width.max(1) as f32;
            let down = y as f32 / height.max(1) as f32;
            let stripe = if stripes > 0 && (x / stripes.max(1)).is_multiple_of(2) {
                1.0
            } else {
                0.55
            };
            let base = (across * 0.6 + down * 0.4) * stripe * brightness;
            let red = ((base + hue_shift).clamp(0.0, 1.0) * 255.0) as u8;
            let green = ((base * 0.8 + hue_shift * 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            let blue = ((base * 0.6).clamp(0.0, 1.0) * 255.0) as u8;
            bytes.push(red);
            bytes.push(green);
            bytes.push(blue);
        }
    }
    PixelBuffer {
        width,
        height,
        data: PixelData::Srgb8(bytes),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Embedded,
        decode_ms: 1,
    }
}

/// An inference service running all three PHASE-06 face models.
///
/// The same argument as [`engine`]: nothing here is a double. The three signed
/// placeholders are generated on the fly and run through the real `InferService`, so a
/// test that passes has exercised the batch scheduler, the memory ledger and the
/// deterministic interpreter rather than a stub of them.
///
/// The detector is registered as `Segmentation` because that is the class `models.lock`
/// gives it, and the class decides which batch-size column the scheduler reads.
#[must_use]
pub fn face_engine(dir: &TempDir, batch: u16) -> Arc<dyn InferService> {
    let detect = dir.path().join("face_detect.onnx");
    let embed = dir.path().join("face_embed.onnx");
    let quality = dir.path().join("face_quality.onnx");
    std::fs::write(&detect, serialise(&fixtures::face_detect())).expect("write the detector");
    std::fs::write(&embed, serialise(&fixtures::face_embed())).expect("write the recogniser");
    std::fs::write(&quality, serialise(&fixtures::face_quality())).expect("write the head");

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
        embedding: batch,
        segmentation: 2,
        retouch: 1,
    };

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    Arc::new(InferEngine::new(
        BackendRegistry::with_reference(),
        source,
        plan,
        clock,
    ))
}

/// A frame with synthetic faces in it, as a `PixelBuffer`.
///
/// Wraps `aura_vision::face::fixtures::render_frame` so a test can hand it straight to
/// the pipeline. The provenance is `Decoded` rather than `Embedded`, because the face
/// pass runs on the tier 2 proxy and a test whose buffer claimed to be a camera JPEG
/// would store the wrong `pixel_source`.
#[must_use]
pub fn face_frame(
    width: u32,
    height: u32,
    faces: &[aura_vision::face::fixtures::SynthFace],
) -> PixelBuffer {
    PixelBuffer {
        width,
        height,
        data: PixelData::Srgb8(aura_vision::face::fixtures::render_frame(
            width as usize,
            height as usize,
            faces,
        )),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 1,
    }
}

/// A frame of one flat colour.
#[must_use]
pub fn flat(width: u32, height: u32, level: u8) -> PixelBuffer {
    PixelBuffer {
        width,
        height,
        data: PixelData::Srgb8(vec![level; (width * height * 3) as usize]),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Embedded,
        decode_ms: 1,
    }
}
