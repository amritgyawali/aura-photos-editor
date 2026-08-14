//! One decoded frame in, everything phase 06 needs to know about the people in it
//! out.
//!
//! The module is arranged the way the phase document names it: [`detect`] for
//! finding faces, [`align`] for normalising them, [`embed`] for the recognition
//! template, [`quality`] for whether that template is worth trusting, [`person`] for
//! bodies, [`cluster`] for turning templates into identities, [`roles`] for who
//! those identities are, [`prominence`] for how much a frame is about each of them.
//! [`fixtures`] is the ground truth the gates are measured against.
//!
//! What ties the per-frame half together is [`FacePipeline::analyse`], and its shape
//! is the same argument phase 05 made: **the pixels are decoded once**. A frame is
//! read, and from that one buffer come the boxes, the landmarks, the aligned crops,
//! the pose estimates, the sharpness, the occlusion, the templates and the bodies.
//! Nothing in this crate opens a file twice.
//!
//! Two batches per frame rather than two per face, for the same reason: a group
//! photograph has thirty faces, and thirty separate calls into the runtime would
//! discard the memory ledger and the batch scheduler phase 03 built.
//!
//! ## Face ids are derived, not generated
//!
//! [`derive_face_id`] hashes the photograph, the box and the detector version into a
//! v8-shaped UUID rather than calling `FaceId::new`. Three things follow, and all
//! three are requirements rather than conveniences:
//!
//! * **Determinism (invariant 4).** The same pixels through the same detector
//!   produce the same face ids, so two machines analysing one wedding agree, and the
//!   gate can assert byte equality rather than a similarity.
//! * **Resumability (invariant 5).** A killed pass that is restarted re-derives the
//!   same ids, so the write is an `INSERT OR REPLACE` of the same row rather than a
//!   duplicate face beside the original.
//! * **Referential stability.** A sealed crop on disk is named by its face id. A
//!   generated id would orphan every crop on every re-scan.
//!
//! ## Nothing here is stored
//!
//! This crate produces observations and forgets them. The encryption, the catalog
//! rows, the erasure and the project scoping all live in `aura-people`, which is the
//! only crate that may hold a biometric template in a durable form. The split is
//! structural: `aura-vision` has no catalog dependency, so it cannot write one.

pub mod align;
pub mod cluster;
pub mod detect;
pub mod embed;
pub mod fixtures;
pub mod person;
pub mod prominence;
pub mod quality;
pub mod redact;
pub mod roles;

use std::sync::Arc;

use aura_core::{AuraResult, FaceId, PhotoId, Priority};
use aura_infer::contract::infer::InferService;
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel, PixelSource};
use uuid::Uuid;

use crate::face::align::{Landmarks, Pose};
use crate::face::detect::{Detection, FaceDetector, FoundBy, FrameHint, NormBox};
use crate::face::embed::FaceEmbedder;
use crate::face::person::{PeopleCount, PersonBox};
use crate::face::quality::{FaceQuality, FaceQualityModel, QualityGate};

/// One observed face, with everything a store or a ranking needs.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceObservation {
    /// Derived from the photograph, the box and the detector version.
    pub face_id: FaceId,
    /// The photograph.
    pub image_id: PhotoId,
    /// The face box, normalised to the frame.
    pub bbox: NormBox,
    /// The body box, when one was predicted for the same anchor.
    pub person_bbox: Option<NormBox>,
    /// Five landmarks, normalised to the frame.
    pub landmarks: Landmarks,
    /// Detector objectness, `0..1`.
    pub det_score: f32,
    /// Head pose, estimated from the landmarks.
    pub pose: Pose,
    /// The quality verdict, with its reasons.
    pub quality: FaceQuality,
    /// Fraction of the frame the face covers.
    pub area_frac: f32,
    /// How central it is, `0..1`.
    pub centrality: f32,
    /// The face's own sharpness, `0..1`. Not the frame's.
    pub sharpness: f32,
    /// The L2-normalised recognition template, or `None` when the recogniser
    /// returned something unusable.
    pub template: Option<Vec<f32>>,
    /// The aligned 112x112 sRGB crop, for the sealed crop store.
    ///
    /// Held in memory only until `aura-people` seals it. A caller that does not want
    /// crops passes `keep_crops = false` and gets `None`, which is what the phase
    /// gate and the benchmark do: a 4,000-image wedding is 37 KB of crop per face
    /// and there is no reason to hold ten thousand of them at once.
    pub crop: Option<Vec<u8>>,
    /// Which pass found it.
    pub found_by: FoundBy,
    /// Height of the face in source pixels.
    pub px_height: u32,
    /// Evidence that this identity is a child, from face-to-body proportion.
    ///
    /// `0.0` when there is no body box to measure against, which is not the same as
    /// "adult" - it is "unknown", and [`roles`] treats it as such.
    pub child_score: f32,
}

impl FaceObservation {
    /// True when this face may vote on identity.
    #[must_use]
    pub const fn votes(&self) -> bool {
        self.quality.votes
    }
}

/// Everything one frame's people pass produced.
#[derive(Debug, Clone, PartialEq)]
pub struct FramePeople {
    /// The photograph.
    pub image_id: PhotoId,
    /// Faces, strongest first.
    pub faces: Vec<FaceObservation>,
    /// Bodies, with their bindings. Indices in [`PersonBox::face`] point into
    /// [`FramePeople::faces`].
    pub persons: Vec<PersonBox>,
    /// How many people are in the frame, and why.
    pub count: PeopleCount,
    /// True when the tiled pass ran.
    pub tiled: bool,
    /// Why the tiling decision went the way it did.
    pub tile_reasons: Vec<String>,
    /// Forward passes this frame cost, across all three models.
    pub passes: usize,
    /// Which pixel tier the frame was read from.
    pub pixel_tier: u8,
    /// Whether those pixels were the camera's JPEG or AURA's render.
    pub pixel_source: &'static str,
    /// Milliseconds inside the runtime for this frame.
    pub infer_ms: f32,
}

impl FramePeople {
    /// Faces that passed the quality gate.
    #[must_use]
    pub fn voting(&self) -> usize {
        self.faces.iter().filter(|face| face.votes()).count()
    }

    /// Faces small enough to count towards the tiling decision.
    #[must_use]
    pub fn small_faces(&self, fraction: f32) -> usize {
        self.faces
            .iter()
            .filter(|face| face.bbox.h < fraction)
            .count()
    }
}

/// Derive a face id from what produced the face.
///
/// The box is quantised to a ten-thousandth of the frame before hashing, which is a
/// deliberate loss of precision: a detector whose last floating-point bit moved
/// between two builds of the same version would otherwise produce a different id for
/// the same face, and every sealed crop would be orphaned. A ten-thousandth of a
/// 6,000 px frame is 0.6 px, far below the precision any detection has.
///
/// The UUID's version and variant nibbles are set to 8 and RFC 4122, so the value is
/// a well-formed UUID that no random generator will ever collide with.
#[must_use]
pub fn derive_face_id(photo: PhotoId, bbox: &NormBox, model_ver: u16) -> FaceId {
    let quantise = |value: f32| (value * 10_000.0).round() as i32;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aura.face.v1");
    hasher.update(photo.to_db().as_bytes());
    hasher.update(&quantise(bbox.x).to_le_bytes());
    hasher.update(&quantise(bbox.y).to_le_bytes());
    hasher.update(&quantise(bbox.w).to_le_bytes());
    hasher.update(&quantise(bbox.h).to_le_bytes());
    hasher.update(&model_ver.to_le_bytes());

    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(digest.as_bytes().get(..16).unwrap_or(&[0u8; 16]));
    if let Some(slot) = bytes.get_mut(6) {
        *slot = (*slot & 0x0f) | 0x80; // version 8: custom
    }
    if let Some(slot) = bytes.get_mut(8) {
        *slot = (*slot & 0x3f) | 0x80; // RFC 4122 variant
    }
    FaceId::from_uuid(Uuid::from_bytes(bytes))
}

/// Derive a person-box id the same way, from the body box.
#[must_use]
pub fn derive_person_id(photo: PhotoId, bbox: &NormBox, model_ver: u16) -> Uuid {
    let quantise = |value: f32| (value * 10_000.0).round() as i32;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aura.person.v1");
    hasher.update(photo.to_db().as_bytes());
    hasher.update(&quantise(bbox.x).to_le_bytes());
    hasher.update(&quantise(bbox.y).to_le_bytes());
    hasher.update(&quantise(bbox.w).to_le_bytes());
    hasher.update(&quantise(bbox.h).to_le_bytes());
    hasher.update(&model_ver.to_le_bytes());

    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(digest.as_bytes().get(..16).unwrap_or(&[0u8; 16]));
    if let Some(slot) = bytes.get_mut(6) {
        *slot = (*slot & 0x0f) | 0x80;
    }
    if let Some(slot) = bytes.get_mut(8) {
        *slot = (*slot & 0x3f) | 0x80;
    }
    Uuid::from_bytes(bytes)
}

/// The three models, and the pass that uses all of them.
#[derive(Debug, Clone)]
pub struct FacePipeline {
    detector: FaceDetector,
    embedder: FaceEmbedder,
    quality_model: FaceQualityModel,
    gate: QualityGate,
    keep_crops: bool,
    /// When set, the fixture detector replaces the model. The tests and the phase
    /// gate use it; nothing in the product does.
    reference_detector: Option<fixtures::ReferenceDetector>,
}

impl FacePipeline {
    /// Assemble the pipeline over one inference service.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            detector: FaceDetector::new(Arc::clone(&infer)),
            embedder: FaceEmbedder::new(Arc::clone(&infer)),
            quality_model: FaceQualityModel::new(infer),
            gate: QualityGate::default(),
            keep_crops: true,
            reference_detector: None,
        }
    }

    /// Override the quality gate. QA's sweeps use this.
    #[must_use]
    pub const fn with_gate(mut self, gate: QualityGate) -> Self {
        self.gate = gate;
        self
    }

    /// Override the detection policy.
    #[must_use]
    pub fn with_detect_config(mut self, config: detect::DetectConfig) -> Self {
        self.detector = self.detector.with_config(config);
        self
    }

    /// Stop carrying aligned crops out of the pass.
    #[must_use]
    pub const fn without_crops(mut self) -> Self {
        self.keep_crops = false;
        self
    }

    /// Detect with the deterministic fixture detector instead of the model.
    ///
    /// This is how section 10.1's recall and clustering gates are measured at all;
    /// see [`fixtures`]. It is deliberately explicit at the call site rather than a
    /// feature flag, so that no product path can reach it by accident.
    #[must_use]
    pub const fn with_reference_detector(mut self) -> Self {
        self.reference_detector = Some(fixtures::ReferenceDetector::new());
        self
    }

    /// The detection policy in force.
    #[must_use]
    pub const fn detect_config(&self) -> &detect::DetectConfig {
        self.detector.config()
    }

    /// The quality gate in force.
    #[must_use]
    pub const fn gate(&self) -> QualityGate {
        self.gate
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.detector.ep()
    }

    /// Load all three models before a photographer is waiting on them.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised. A warmup failure is a real failure: it is how the
    /// product learns a model pack is broken before a wedding depends on it.
    pub fn warmup(&self) -> AuraResult<()> {
        if self.reference_detector.is_none() {
            self.detector.warmup()?;
        }
        self.embedder.warmup()?;
        self.quality_model.warmup()
    }

    /// Find, align, measure and embed every face in one frame.
    ///
    /// # Errors
    ///
    /// * `AURA-ML-5007` when the buffer is not 8-bit sRGB, or a model's tensors
    ///   disagree with the manifest.
    /// * `AURA-ML-5011` when the work was cancelled.
    /// * Whatever `InferService` raised on first use.
    ///
    /// A face whose template comes back degenerate is **not** an error: it is stored
    /// with `template: None`, it does not vote, and the reason is on its quality
    /// verdict. One unreadable face must not end a frame, let alone a wedding.
    #[allow(clippy::too_many_lines)]
    pub fn analyse(
        &self,
        image_id: PhotoId,
        buffer: &PixelBuffer,
        level: PixelLevel,
        hint: &FrameHint,
        prio: Priority,
    ) -> AuraResult<FramePeople> {
        let Some(rgb) = buffer.as_srgb8() else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "an 8-bit sRGB buffer",
                buffer.data.as_str(),
            ));
        };
        let width = buffer.width as usize;
        let height = buffer.height as usize;
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{width}x{height}x3 samples"),
                &format!("{} samples", rgb.len()),
            ));
        }

        // 1. Detect.
        let detected = if let Some(reference) = &self.reference_detector {
            let found = reference.detect(rgb, width, height);
            let persons: Vec<NormBox> = found.iter().filter_map(|d| d.person).collect();
            let suppressed = detect::nms(found, self.detector.config());
            detect::FrameDetections {
                faces: suppressed,
                persons,
                tiled: false,
                tile_reasons: vec![
                    "the deterministic fixture detector does not tile; it sees the whole frame at \
                     full resolution"
                        .to_string(),
                ],
                passes: 1,
            }
        } else {
            self.detector.detect(rgb, width, height, hint, prio)?
        };

        // 2. Align every detection, and measure its crop. One warp per face, and the
        //    crop is reused by the quality head, the recogniser and the sealed store,
        //    so the bilinear resample happens exactly once per face.
        let mut crops: Vec<Vec<u8>> = Vec::with_capacity(detected.faces.len());
        let mut tensors: Vec<Vec<f32>> = Vec::with_capacity(detected.faces.len());
        let mut measurements = Vec::with_capacity(detected.faces.len());
        let mut poses = Vec::with_capacity(detected.faces.len());

        for detection in &detected.faces {
            let pixels = align::to_pixels(&detection.landmarks, width, height);
            let transform = align::alignment_for(&pixels);
            let crop = align::warp_crop(rgb, width, height, &transform);
            measurements.push(quality::measure(&crop));
            poses.push(align::pose_from_landmarks(&pixels));
            tensors.push(embed::preprocess_crop(&crop));
            crops.push(crop);
        }

        // 3. Two batches, not two per face.
        let head = self.quality_model.run_batch(&tensors, prio)?;
        let templates = self.embedder.run_batch(&tensors, prio)?;
        let passes = detected.passes + usize::from(!tensors.is_empty()) * 2;

        // 4. Bodies, so the child-proportion evidence has something to measure.
        let bound = person::associate(&detected.faces, &detected.persons);

        // 5. Assemble.
        let mut faces = Vec::with_capacity(detected.faces.len());
        for (index, detection) in detected.faces.iter().enumerate() {
            let measured = measurements.get(index).copied().unwrap_or_default();
            let pose = poses.get(index).copied().unwrap_or_default();
            let px_height = detection.px_height(height);
            let model_usable = head.get(index).and_then(|row| row.first().copied());
            let verdict = quality::fuse(&measured, pose, px_height, model_usable, self.gate);

            let template = templates.get(index).and_then(|template| {
                if embed::is_degenerate(template) {
                    None
                } else {
                    Some(template.clone())
                }
            });

            let mut verdict = verdict;
            if template.is_none() {
                verdict.votes = false;
                verdict.reasons.push(
                    "the recogniser returned an unusable template, so this face cannot vote on \
                     identity"
                        .to_string(),
                );
            }

            faces.push(FaceObservation {
                face_id: derive_face_id(image_id, &detection.face, detect::MODEL_VER),
                image_id,
                bbox: detection.face,
                person_bbox: detection.person,
                landmarks: detection.landmarks,
                det_score: detection.face_score,
                pose,
                quality: verdict,
                area_frac: detection.face.area_frac(),
                centrality: detection.face.centrality(),
                sharpness: measured.sharpness,
                template,
                crop: if self.keep_crops {
                    crops.get(index).cloned()
                } else {
                    None
                },
                found_by: detection.found_by,
                px_height,
                child_score: child_score(detection, &bound, index),
            });
        }

        let count = person::people_count(&detected.faces, &bound);

        Ok(FramePeople {
            image_id,
            faces,
            persons: bound,
            count,
            tiled: detected.tiled,
            tile_reasons: detected.tile_reasons,
            passes,
            pixel_tier: level.tier().clamp(1, 3) as u8,
            pixel_source: buffer.source.as_str(),
            infer_ms: 0.0,
        })
    }
}

/// Evidence that a face belongs to a child, from face-to-body proportion.
///
/// An adult's head is about one-seventh of their standing height; a small child's is
/// closer to one-quarter. The body boxes at a wedding are almost never whole
/// standing bodies, so this is a weak signal and it is treated as one: the ratio is
/// mapped onto `0..1` with a wide dead band, and [`roles`] needs 0.6 before it will
/// say anything.
///
/// Zero when there is no body box, and zero means *unknown* rather than *adult*.
/// The distinction is why `roles` tests against a threshold rather than sorting.
fn child_score(detection: &Detection, bound: &[PersonBox], face_index: usize) -> f32 {
    let Some(body) = bound
        .iter()
        .find(|body| body.face == Some(face_index))
        .map(|body| body.bbox)
        .or(detection.person)
    else {
        return 0.0;
    };
    if body.h <= f32::EPSILON {
        return 0.0;
    }
    let ratio = detection.face.h / body.h;
    // One-seventh (0.143) is an adult; one-quarter (0.25) is a young child. Below
    // 0.16 and above 0.30 the signal saturates, because a clipped torso can produce
    // any ratio at all and a confident answer from a clipped torso is a guess.
    ((ratio - 0.16) / (0.30 - 0.16)).clamp(0.0, 1.0)
}

/// Side of the aligned crop, re-exported so a caller does not have to reach into
/// `aura-infer` for a number that is really this crate's alignment contract.
pub const CROP_SIDE: usize = aura_infer::onnx::fixtures::FACE_CROP_SIDE;

/// The pixel rung the face pass is computed from.
///
/// Tier 2, the 2048 px proxy - and this is the one place phase 06 spends more than
/// phase 05 did, deliberately. Invariant 3 puts cheap analysis on embedded previews,
/// and a 384 px thumbnail is the right rung for a perceptual embedding. It is the
/// wrong rung for faces: a guest whose face is 3 % of the frame is eleven pixels
/// tall on a 384 px thumbnail and sixty on a 2048 px proxy, and the whole small-face
/// recall requirement in section 10.1 lives in the difference. The proxy is already
/// cached by phase 02, so the cost is a cache read rather than a decode.
pub const FACE_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// The provenance string for a buffer, for the `face_scan` row.
#[must_use]
pub const fn source_label(source: PixelSource) -> &'static str {
    source.as_str()
}
