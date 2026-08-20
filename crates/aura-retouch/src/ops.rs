//! One decoded frame in, one retouch plan out.
//!
//! PHASE-20 section 3 data flow, as one function, and section 8 implementation order, in that
//! order.
//!
//! ## The order, and why it is this order
//!
//! 1. **Refuse early.** Retouching switched off, no skin mask, no faces: each of those produces
//!    a plan that does nothing and *says why*, rather than no plan at all. A frame nobody
//!    retouched and a frame the photographer excluded look identical in a coverage report
//!    otherwise.
//! 2. **The protect set first, before anything is detected.** Section 6.1 default is
//!    conservative, and the cheapest way to be conservative is to know what may not be touched
//!    before deciding what to touch.
//! 3. **Blemishes, per face.** Detected on the face own skin statistics, vetoed against the
//!    protect set, and filtered by the temporary floor.
//! 4. **Under-eye, then evening.** Both are measurements against the skin around them, and both
//!    are refused on a face phase 19 has already evened - `RetouchCode::AlreadyEvenedByLocal`,
//!    which is the query `idx_local_evened` exists for.
//! 5. **The texture guard, over the whole frame.** Last, because it measures the *plan* rather
//!    than any one operation, and because a re-solve has to weaken everything together.
//! 6. **Reasons and confidence.** Assembled from what happened rather than from what was
//!    intended, which is why they are last.
//!
//! ## The placeholder, said once and plainly
//!
//! [`BLEMISH_HEAD_TRAINED`] and [`PERMANENT_HEAD_TRAINED`] are both false, so neither shipped
//! head is consulted. What runs is the measured detector in [`crate::blemish`], and
//! `docs/adr/ADR-0041-portrait-retouch-and-texture-protection.md` section 7 records why that is
//! a decision rather than a fallback. Every plan carries
//! [`RetouchCode::HeadUntrained`] so that nothing anywhere describes this output as learned.

use std::collections::BTreeMap;

use aura_core::contract::composition::Box2;
use aura_core::contract::error::AuraError;
use aura_core::contract::local::{MaskField, MaskKind};
use aura_core::contract::people::{FaceRef, Role};
use aura_core::contract::retouch::{
    FreqBand, ImageId, InpaintMethod, ProtectedFeature, RetouchCode, RetouchOp, RetouchPlan,
    RetouchPreset, RetouchReason, TextureReport, MAX_OPS, MIN_RETOUCHABLE_FACE, TEMPORARY_FLOOR,
};
use aura_core::{IdentityId, MaskId, SceneId};
use aura_raw::contract::pixels::{PixelBuffer, PixelData, PixelLevel};
use aura_render::retouch::RetouchContext;

use crate::blemish::{self, FaceCrop};
use crate::errors;
use crate::evening;
use crate::permanent::{self, Observation};
use crate::presets::PresetTable;
use crate::strength::{self, IdentityStats};
use crate::texture_guard::{self, Frame};
use crate::undereye;

/// Which build arithmetic produced a plan.
///
/// Bumped on any change to a detector threshold, a cap, a measurement or the way confidence is
/// combined. Written into `retouch_plan.analysis_ver`, and two plans made under different values
/// of it are not comparable: `AURA-ML-5090` exists so that comparison never happens silently.
pub const ANALYSIS_VER: u16 = 1;

/// The version stamped on every `retouch_plan.model_ver`.
///
/// Two heads, one number, for the reason phases 09, 11, 15 and 19 all fold theirs: no consumer
/// of a plan cares *which* head moved, only that the numbers are not comparable across the move.
pub const MODEL_VER: u16 = 100;

/// The pixel rung the retouch pass reads.
///
/// Tier 2, the 2048 px proxy. Section 2.1 says full-resolution execution happens at export and
/// the preview uses "an identical algorithm at proxy scale so what the user approves is what
/// ships" - which is exactly what this rung is for: the *decision* is made here and the same
/// operations are applied at whatever resolution the export asks for, because every constant in
/// `aura_render::retouch` is a fraction of the thing it measures rather than a number of pixels.
pub const RETOUCH_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// Whether the pinned blemish weights have passed the section 10.1 gate.
///
/// A compile-time release assertion, exactly as phase 11 `KEYPOINT_HEAD_TRAINED`, phase 15
/// `WB_HEAD_TRAINED` and phase 18 `SEG_HEAD_TRAINED` are. While this is false the head is never
/// consulted and the measured detector runs instead.
pub const BLEMISH_HEAD_TRAINED: bool = false;

/// Whether the pinned permanent-feature weights have passed the section 10.1 gate.
pub const PERMANENT_HEAD_TRAINED: bool = false;

/// How much a face box is grown before it is measured.
///
/// A tenth. The skin a donor patch is borrowed from is often just outside the detector box -
/// the side of a jaw, the temple - and a heal that could only borrow from inside the box would
/// refuse marks at the edge of every face.
pub const CROP_MARGIN: f32 = 0.10;

/// Everything known about a frame before its pixels are read.
///
/// All of it arrives through a frozen service: `PeopleService` for the faces and the roles,
/// `StoryService` for the scene, `MaskService` for the skin - through phase 19
/// [`MaskField`] port, deliberately, because a second mask port would be a second answer to
/// "where is the skin" - and `LocalService` for whether phase 19 has already evened this face.
#[derive(Debug, Clone)]
pub struct FrameContext {
    /// What the photograph is of. Invariant 7.
    pub scene: SceneId,
    /// The faces phase 06 found, in prominence order.
    pub faces: Vec<FaceRef>,
    /// The skin region phase 18 generated, and its id.
    ///
    /// **`None` on a build with no mask generator wired in**, which is the honest state of this
    /// repository. Every operation is then withdrawn and the plan says
    /// [`RetouchCode::MaskUnavailable`]; there is no geometric fallback here for the reason
    /// phase 19 gives - a rectangle edge does not follow a person, and a retouch through one
    /// smooths the wall behind somebody ear.
    pub skin: Option<(MaskId, MaskField)>,
    /// The gallery-constant strength for each identity, from [`crate::strength`].
    pub identity_strength: BTreeMap<IdentityId, f32>,
    /// What may never be touched on the people in this frame, in face-frame coordinates.
    pub protected: Vec<ProtectedFeature>,
    /// Faces phase 19 has already evened, by identity.
    ///
    /// Phase 19 wrote the rule and this is the phase that keeps it: "phase 20 retouches skin
    /// this phase has already evened and must not do it twice".
    pub evened_by_local: Vec<Option<IdentityId>>,
    /// Which preset this photograph is retouched under.
    pub preset: RetouchPreset,
    /// Whether retouching is switched on for this project. The kill switch hard rule 8 requires.
    pub enabled: bool,
}

impl FrameContext {
    /// A context with the defaults a frame nobody has analysed would have.
    #[must_use]
    pub fn new(scene: SceneId) -> Self {
        Self {
            scene,
            faces: Vec::new(),
            skin: None,
            identity_strength: BTreeMap::new(),
            protected: Vec::new(),
            evened_by_local: Vec::new(),
            preset: RetouchPreset::default(),
            enabled: true,
        }
    }

    /// True when phase 19 already evened this face.
    #[must_use]
    pub fn already_evened(&self, identity: Option<IdentityId>) -> bool {
        self.evened_by_local.contains(&identity)
    }

    /// The strength for one person, or the conservative default for somebody unidentified.
    #[must_use]
    pub fn strength_for(&self, face: &FaceRef, table: &PresetTable, preset: RetouchPreset) -> f32 {
        if let Some(value) = face
            .identity_id
            .and_then(|id| self.identity_strength.get(&id).copied())
        {
            value
        } else {
            {
                // Nobody knows who this is - the state every face is in on this build. The
                // fallback is the `unknown` role weight and this frame own face size, which is
                // the most conservative reading available rather than a guess at a person.
                let mut stats = IdentityStats::unknown(IdentityId::new());
                stats.role = Role::Unknown;
                stats.median_face_frac = face.area_frac.sqrt();
                stats.dominant_scene = self.scene;
                strength::assign(&stats, table, preset)
            }
        }
    }
}

/// One frame answer.
#[derive(Debug, Clone)]
pub struct FrameOutcome {
    /// The plan.
    pub plan: RetouchPlan,
    /// Marks that looked permanent, for the cross-frame accumulation.
    ///
    /// Returned rather than stored here, because permanence is a property of a *person across a
    /// gallery* and one frame cannot decide it. [`crate::permanent::accumulate`] turns a
    /// project worth of these into protect rows.
    pub observations: Vec<Observation>,
    /// The retouched pixels, for a caller that wants to show them.
    pub rendered: Option<Vec<f32>>,
    /// Anything worth telling a support engineer. Never fatal.
    pub warnings: Vec<AuraError>,
}

/// One frame in, one plan out.
#[derive(Debug, Clone)]
pub struct Analyser {
    presets: PresetTable,
}

impl Analyser {
    /// Build an analyser over the embedded preset table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when the table will not load.
    pub fn new() -> Result<Self, AuraError> {
        Ok(Self {
            presets: PresetTable::embedded()?,
        })
    }

    /// Build an analyser over a supplied table, for the gate and the eval harness.
    #[must_use]
    pub const fn with_presets(presets: PresetTable) -> Self {
        Self { presets }
    }

    /// The table underneath.
    #[must_use]
    pub const fn presets(&self) -> &PresetTable {
        &self.presets
    }

    /// Plan one photograph.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5092` when the buffer cannot be read as pixels.
    #[allow(clippy::too_many_lines)]
    pub fn analyse(
        &self,
        image: ImageId,
        pixels: &PixelBuffer,
        context: &FrameContext,
    ) -> Result<FrameOutcome, AuraError> {
        let mut reasons: Vec<RetouchReason> = Vec::new();
        let mut warnings: Vec<AuraError> = Vec::new();

        // Every plan in this build carries it, and it is first so that nothing downstream can
        // read this output as learned.
        reasons.push(RetouchReason::plain(RetouchCode::HeadUntrained, -0.05));

        if !context.enabled || context.preset.is_off() {
            return Ok(self.nothing(image, context, reasons, warnings));
        }

        let (scene_row, scene_found) = self.presets.scene(context.scene);
        if !scene_found {
            warnings.push(errors::scene_unpreset(context.scene.as_str()));
            reasons.push(RetouchReason::plain(RetouchCode::SceneLimited, -0.10));
        } else if scene_row.limit <= 0.0 {
            reasons.push(RetouchReason::plain(RetouchCode::SceneLimited, 0.0));
            return Ok(self.nothing(image, context, reasons, warnings));
        }

        let Some((mask_id, field)) = context.skin.as_ref() else {
            reasons.push(RetouchReason::plain(RetouchCode::MaskUnavailable, -0.25));
            return Ok(self.nothing(image, context, reasons, warnings));
        };
        if field.kind != MaskKind::Skin || !field.is_usable() {
            reasons.push(RetouchReason::plain(RetouchCode::MaskUnavailable, -0.25));
            return Ok(self.nothing(image, context, reasons, warnings));
        }

        let Some(frame) = to_linear(pixels) else {
            return Err(errors::retouch_failed(
                &image.to_db(),
                "the proxy carries no readable pixels",
            ));
        };
        let skin = skin_weights(field, frame.width, frame.height);
        let mask_scale = field.strength_scale();

        let mut ops: Vec<RetouchOp> = Vec::new();
        let mut eyes: Vec<[[f32; 2]; 2]> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        let mut per_identity: BTreeMap<IdentityId, f32> = BTreeMap::new();
        let mut faces_retouched = 0u32;
        let mut anomalies_left = 0u32;

        for face in &context.faces {
            if face.area_frac.sqrt() < MIN_RETOUCHABLE_FACE {
                reasons.push(RetouchReason::plain_at(
                    RetouchCode::FaceTooSmall,
                    0.0,
                    face.bbox,
                ));
                continue;
            }
            let identity_strength =
                context.strength_for(face, &self.presets, context.preset) * mask_scale;
            if identity_strength <= 0.0 {
                continue;
            }
            if let Some(identity) = face.identity_id {
                per_identity.insert(identity, identity_strength);
                reasons.push(RetouchReason::plain(RetouchCode::IdentityStrength, 0.05));
            } else {
                reasons.push(RetouchReason::plain(RetouchCode::IdentityUnknown, -0.10));
            }
            faces_retouched += 1;

            let Some(crop) = crop_face(&frame, &skin, face) else {
                continue;
            };
            let preset_row = self.presets.preset(context.preset);

            // --- blemishes -------------------------------------------------------------
            let candidates = blemish::detect(&crop);
            let mut removed = 0u32;
            for candidate in &candidates {
                if let Some(feature) = Self::veto(context, face, candidate.area) {
                    reasons.push(RetouchReason::at(
                        RetouchCode::VetoedByProtection,
                        format!(
                            "{} - {}",
                            RetouchCode::VetoedByProtection.user_text(),
                            feature.kind.as_str()
                        ),
                        0.0,
                        candidate.area,
                    ));
                    if feature.kind.is_absolute() {
                        reasons.push(RetouchReason::plain(RetouchCode::TattooProtected, 0.0));
                    }
                    continue;
                }
                if candidate.too_large {
                    reasons.push(RetouchReason::plain_at(
                        RetouchCode::AnomalyTooLarge,
                        0.0,
                        candidate.area,
                    ));
                    anomalies_left += 1;
                    continue;
                }
                if candidate.is_permanent() {
                    if let Some(area) = permanent::to_face_frame(candidate.area, face) {
                        if let Some(identity) = face.identity_id {
                            let (score, kind) = permanent::classify(candidate);
                            observations.push(Observation {
                                identity,
                                image,
                                area,
                                minute: 0.0,
                                permanent: score,
                                kind,
                            });
                        }
                    }
                    reasons.push(RetouchReason::plain_at(
                        RetouchCode::FeatureProtected,
                        0.0,
                        candidate.area,
                    ));
                    anomalies_left += 1;
                    continue;
                }
                if candidate.temporary < TEMPORARY_FLOOR {
                    reasons.push(RetouchReason::plain_at(
                        RetouchCode::AnomalyUncertain,
                        -0.02,
                        candidate.area,
                    ));
                    anomalies_left += 1;
                    continue;
                }
                if ops.len() >= MAX_OPS {
                    break;
                }
                ops.push(RetouchOp::Blemish {
                    area: candidate.area,
                    method: InpaintMethod::Patch,
                    strength: (identity_strength * preset_row.blemish * scene_row.limit)
                        .clamp(0.0, 1.0),
                });
                removed += 1;
            }
            if removed > 0 {
                reasons.push(RetouchReason::plain(RetouchCode::BlemishRemoved, 0.10));
            } else if candidates.is_empty() {
                reasons.push(RetouchReason::plain(RetouchCode::NoBlemishFound, 0.0));
            }

            // --- under-eye --------------------------------------------------------------
            if scene_row.allow_undereye {
                if !face.has_eyes() {
                    reasons.push(RetouchReason::plain(RetouchCode::NoEyeLandmarks, -0.05));
                } else if let Some(decision) = undereye::solve(
                    &crop,
                    face,
                    (identity_strength * preset_row.undereye * scene_row.limit).clamp(0.0, 1.0),
                ) {
                    if let Some(identity) = face.identity_id {
                        ops.push(RetouchOp::UnderEye {
                            identity,
                            luma: decision.luma_ev,
                            chroma: decision.chroma,
                        });
                        eyes.push(face.eyes);
                        reasons.push(RetouchReason::plain(RetouchCode::UnderEyeCorrected, 0.05));
                        if decision.capped {
                            reasons.push(RetouchReason::plain(RetouchCode::UnderEyeCapped, 0.0));
                        }
                    }
                }
            }

            // --- tone evening ------------------------------------------------------------
            if scene_row.allow_evening {
                if context.already_evened(face.identity_id) {
                    reasons.push(RetouchReason::plain(RetouchCode::AlreadyEvenedByLocal, 0.0));
                } else if let Some(decision) = evening::solve(
                    &crop,
                    (identity_strength * preset_row.evening * scene_row.limit).clamp(0.0, 1.0),
                ) {
                    ops.push(RetouchOp::ToneEvening {
                        mask: *mask_id,
                        strength: decision.strength,
                        band: FreqBand::Mid,
                    });
                    reasons.push(RetouchReason::plain(RetouchCode::ToneEvened, 0.05));
                } else {
                    reasons.push(RetouchReason::plain(RetouchCode::SkinAlreadyEven, 0.0));
                }
            }
        }

        // --- the guarantee -----------------------------------------------------------------
        let context_for_render = RetouchContext {
            skin: skin.clone(),
            eyes,
        };
        let floor = self.presets.preset(context.preset).texture_floor;
        let guarded = texture_guard::enforce(&frame, &ops, &context_for_render, floor);

        if guarded.report.withdrawn {
            reasons.push(RetouchReason::plain(
                RetouchCode::TextureFloorUnreachable,
                -0.20,
            ));
            warnings.push(errors::texture_guard(
                &image.to_db(),
                guarded.report.band_ratio,
                floor,
                true,
            ));
        } else if guarded.report.resolves > 0 {
            reasons.push(RetouchReason::plain(RetouchCode::TextureResolved, -0.05));
            warnings.push(errors::texture_guard(
                &image.to_db(),
                guarded.report.band_ratio,
                floor,
                false,
            ));
        } else if !guarded.ops.is_empty() {
            reasons.push(RetouchReason::plain(RetouchCode::TextureHeld, 0.10));
        }
        if !guarded.report.is_well_measured() && !guarded.ops.is_empty() {
            reasons.push(RetouchReason::plain(
                RetouchCode::TextureUnmeasurable,
                -0.15,
            ));
        }

        let budget_used = budget(&frame, &guarded.rendered, &skin);
        let confidence = confidence(&reasons);

        let plan = RetouchPlan {
            image_id: image,
            ops: guarded.ops,
            per_identity_strength: per_identity,
            protected: context.protected.clone(),
            texture_report: guarded.report,
            preset: context.preset,
            reasons,
            confidence,
            scene: context.scene,
            budget_used,
            user_edited: false,
            reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            preset_ver: self.presets.version(),
        };

        let _ = (faces_retouched, anomalies_left);

        Ok(FrameOutcome {
            plan,
            observations,
            rendered: Some(guarded.rendered),
            warnings,
        })
    }

    /// A plan that does nothing, carrying the reasons that say why.
    fn nothing(
        &self,
        image: ImageId,
        context: &FrameContext,
        reasons: Vec<RetouchReason>,
        warnings: Vec<AuraError>,
    ) -> FrameOutcome {
        let confidence = confidence(&reasons);
        let plan = RetouchPlan {
            image_id: image,
            ops: Vec::new(),
            per_identity_strength: BTreeMap::new(),
            protected: context.protected.clone(),
            texture_report: TextureReport::UNTOUCHED,
            preset: context.preset,
            reasons,
            confidence,
            scene: context.scene,
            budget_used: 0.0,
            user_edited: false,
            reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            preset_ver: self.presets.version(),
        };
        FrameOutcome {
            plan,
            observations: Vec::new(),
            rendered: None,
            warnings,
        }
    }

    /// The protected feature that forbids touching this region, if any.
    ///
    /// The protect set is stored per person in face-frame coordinates and the candidates are
    /// found per frame, so one of the two has to move. The feature moves, because a feature
    /// projected onto this frame is a rectangle a photographer can be shown.
    fn veto(context: &FrameContext, face: &FaceRef, candidate: Box2) -> Option<ProtectedFeature> {
        for feature in &context.protected {
            if face.identity_id != Some(feature.identity) {
                continue;
            }
            let Some(area) = permanent::to_frame(feature.area, face) else {
                continue;
            };
            let projected = ProtectedFeature { area, ..*feature };
            if projected.vetoes(candidate) {
                return Some(projected);
            }
        }
        None
    }
}

/// How much of the shared per-image allowance a retouch spent.
///
/// **Phase 19 allowance, not a second one.** Six local operations and a retouch that each
/// respect their own budget still add up to a photograph that looks worked on, and phase 19 own
/// rule said the seventh operation would inherit the allowance rather than get one.
///
/// Measured the way phase 19 measures it: mean absolute change in a perceptual space, over the
/// region the operation could act on.
#[must_use]
pub fn budget(before: &Frame, after: &[f32], skin: &[f32]) -> f32 {
    let mut total = 0.0f64;
    let mut weight = 0.0f64;
    for pixel in 0..before.width * before.height {
        let coverage = f64::from(skin.get(pixel).copied().unwrap_or(0.0));
        if coverage <= 0.0 {
            continue;
        }
        let slot = pixel * 3;
        for channel in 0..3 {
            let a = before.rgb.get(slot + channel).copied().unwrap_or(0.0);
            let b = after.get(slot + channel).copied().unwrap_or(0.0);
            // Perceptual rather than linear, for phase 19 reason: a mean absolute change
            // measured in linear light calls a shadow lift free and a highlight nudge enormous.
            total += f64::from((encode(a) - encode(b)).abs()) * coverage;
            weight += coverage;
        }
    }
    if weight <= f64::EPSILON {
        return 0.0;
    }
    let change = (total / weight) as f32;
    (change / aura_core::contract::local::PERCEPTUAL_BUDGET).clamp(0.0, 1.0)
}

/// The perceptual position of a linear value, for the budget measurement only.
///
/// Not an output transform: nothing encoded here reaches a pixel, a file or a buffer. It exists
/// so that the number the governor spends is in the same space phase 19 spends it in.
fn encode(linear: f32) -> f32 {
    linear.max(0.0).powf(1.0 / 2.2)
}

/// Combine the reason weights into a confidence.
///
/// Starts at three quarters rather than at one, because on this build no plan is ever fully
/// trusted: the heads are untrained and the first reason on every plan says so.
#[must_use]
pub fn confidence(reasons: &[RetouchReason]) -> f32 {
    let total: f32 = reasons.iter().map(|reason| reason.weight).sum();
    (0.75 + total).clamp(0.0, 1.0)
}

/// Read a proxy buffer as linear RGB.
///
/// `None` when the buffer carries tiles, which the retouch pass never asks for: tiling is an
/// export path and this is a decision path.
#[must_use]
pub fn to_linear(buffer: &PixelBuffer) -> Option<Frame> {
    let width = buffer.width as usize;
    let height = buffer.height as usize;
    if width == 0 || height == 0 {
        return None;
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    match &buffer.data {
        PixelData::Srgb8(bytes) => {
            for value in bytes.iter().take(width * height * 3) {
                rgb.push(aura_raw::colour::curve::srgb_decode(
                    f32::from(*value) / 255.0,
                ));
            }
        }
        PixelData::Linear16(values) => {
            for value in values.iter().take(width * height * 3) {
                rgb.push(aura_raw::colour::curve::linear_u16_to_scene(*value));
            }
        }
        PixelData::Tiled(_) => return None,
    }
    if rgb.len() < width * height * 3 {
        rgb.resize(width * height * 3, 0.0);
    }
    Some(Frame { rgb, width, height })
}

/// Turn a coarse mask field into a per-pixel weight.
///
/// Bilinear rather than nearest, and clamped at the edges rather than zero-padded - which is the
/// defect phase 18 found in its own resampler: reading zero outside the plane darkens the
/// outermost half-pixel of every upsampled region, and here it would leave a rim of unretouched
/// skin around every face.
#[must_use]
pub fn skin_weights(field: &MaskField, width: usize, height: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; width * height];
    if field.width == 0 || field.height == 0 {
        return out;
    }
    let scale = field.strength_scale();
    for y in 0..height {
        let fy = ((y as f32 + 0.5) / height as f32 * f32::from(field.height) - 0.5)
            .clamp(0.0, f32::from(field.height) - 1.0);
        let y0 = fy.floor() as u16;
        let y1 = (y0 + 1).min(field.height.saturating_sub(1));
        let ty = fy - f32::from(y0);
        for x in 0..width {
            let fx = ((x as f32 + 0.5) / width as f32 * f32::from(field.width) - 0.5)
                .clamp(0.0, f32::from(field.width) - 1.0);
            let x0 = fx.floor() as u16;
            let x1 = (x0 + 1).min(field.width.saturating_sub(1));
            let tx = fx - f32::from(x0);

            let a = field.sample(x0, y0);
            let b = field.sample(x1, y0);
            let c = field.sample(x0, y1);
            let d = field.sample(x1, y1);
            let top = a + (b - a) * tx;
            let bottom = c + (d - c) * tx;
            if let Some(slot) = out.get_mut(y * width + x) {
                *slot = ((top + (bottom - top) * ty) * scale).clamp(0.0, 1.0);
            }
        }
    }
    out
}

/// Cut one face out of the frame, with a margin.
#[must_use]
pub fn crop_face(frame: &Frame, skin: &[f32], face: &FaceRef) -> Option<FaceCrop> {
    let bounds = Box2 {
        x: face.bbox.x - face.bbox.w * CROP_MARGIN,
        y: face.bbox.y - face.bbox.h * CROP_MARGIN,
        w: face.bbox.w * (1.0 + CROP_MARGIN * 2.0),
        h: face.bbox.h * (1.0 + CROP_MARGIN * 2.0),
    }
    .clamped();

    let x0 = (bounds.x * frame.width as f32).floor().max(0.0) as usize;
    let y0 = (bounds.y * frame.height as f32).floor().max(0.0) as usize;
    let w = ((bounds.w * frame.width as f32).ceil() as usize).min(frame.width.saturating_sub(x0));
    let h = ((bounds.h * frame.height as f32).ceil() as usize).min(frame.height.saturating_sub(y0));
    if w == 0 || h == 0 {
        return None;
    }

    let mut rgb = Vec::with_capacity(w * h * 3);
    let mut weights = Vec::with_capacity(w * h);
    for row in 0..h {
        for col in 0..w {
            let pixel = (y0 + row) * frame.width + (x0 + col);
            let slot = pixel * 3;
            match frame.rgb.get(slot..slot + 3) {
                Some(value) => rgb.extend_from_slice(value),
                None => rgb.extend_from_slice(&[0.0, 0.0, 0.0]),
            }
            weights.push(skin.get(pixel).copied().unwrap_or(0.0));
        }
    }

    Some(FaceCrop {
        rgb,
        width: w,
        height: h,
        skin: weights,
        bounds: Box2 {
            x: x0 as f32 / frame.width as f32,
            y: y0 as f32 / frame.height as f32,
            w: w as f32 / frame.width as f32,
            h: h as f32 / frame.height as f32,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn a_frame_with_no_skin_mask_is_planned_and_says_why() {
        let (image, pixels, mut context) = fixtures::planned_frame();
        context.skin = None;
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome.plan.is_noop());
        assert!(outcome
            .plan
            .reasons
            .iter()
            .any(|r| r.code == RetouchCode::MaskUnavailable));
        assert!(outcome.plan.is_sound());
    }

    #[test]
    fn switching_retouch_off_produces_a_plan_rather_than_no_plan() {
        let (image, pixels, mut context) = fixtures::planned_frame();
        context.enabled = false;
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome.plan.is_noop());
        assert!(!outcome.plan.reasons.is_empty());
    }

    #[test]
    fn a_blemish_is_removed_and_the_plan_holds_its_texture_floor() {
        let (image, pixels, context) = fixtures::planned_frame();
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(
            outcome.plan.count_of("blemish") > 0,
            "no blemish was removed: {:?}",
            outcome
                .plan
                .reasons
                .iter()
                .map(|r| r.code.as_str())
                .collect::<Vec<_>>()
        );
        assert!(outcome.plan.texture_report.passed);
        assert!(outcome.plan.is_sound());
        assert!(outcome.plan.budget_used <= 1.0);
    }

    #[test]
    fn a_protected_feature_vetoes_the_operation_over_it() {
        let (image, pixels, context) = fixtures::planned_frame_with_protected_mole();
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome
            .plan
            .reasons
            .iter()
            .any(|r| r.code == RetouchCode::VetoedByProtection));
        for op in &outcome.plan.ops {
            if let Some(area) = op.area() {
                assert!(!outcome.plan.is_protected(area));
            }
        }
        assert!(outcome.plan.is_sound());
    }

    #[test]
    fn every_plan_says_the_heads_are_untrained() {
        let (image, pixels, context) = fixtures::planned_frame();
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome
            .plan
            .reasons
            .iter()
            .any(|r| r.code == RetouchCode::HeadUntrained));
        // Read through a binding so this is a check of the constants rather than a constant
        // the compiler folds away. When either head is trained this test is what fails, and it
        // fails in the right place: the plan must stop saying `head_untrained` on the same day.
        let trained = [BLEMISH_HEAD_TRAINED, PERMANENT_HEAD_TRAINED];
        assert!(trained.iter().all(|flag| !flag));
    }

    #[test]
    fn planning_is_deterministic() {
        let (image, pixels, context) = fixtures::planned_frame();
        let analyser = Analyser::new().expect("an analyser");
        let first = analyser.analyse(image, &pixels, &context).expect("a plan");
        let second = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert_eq!(first.plan, second.plan);
    }

    #[test]
    fn a_scene_with_no_skin_in_it_is_never_retouched() {
        let (image, pixels, mut context) = fixtures::planned_frame();
        context.scene = SceneId::Details;
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome.plan.is_noop());
    }
}
