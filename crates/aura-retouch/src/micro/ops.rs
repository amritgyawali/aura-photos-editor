//! One decoded frame in, one micro-retouch plan out.
//!
//! PHASE-21 section 3's data flow as one function, and section 8's implementation order in that
//! order.
//!
//! ## The order, and why it is this order
//!
//! 1. **Refuse early.** Micro-retouch switched off, a scene that gets none, no regions at all:
//!    each of those produces a plan that does nothing and *says why*, rather than no plan. A
//!    frame nobody cleaned and a frame the studio excluded look identical in a coverage report
//!    otherwise.
//! 2. **Glare first**, because a specular sheet lying across an eye would otherwise set the
//!    specular exclusion the eye measurements read, and an eye under a repaired sheet can then be
//!    measured normally. It is also the only family that can consult another photograph, and
//!    deciding that first keeps the sibling decode off the path of every frame that does not need
//!    one.
//! 3. **Hair**, which needs only the hair region and the background behind it.
//! 4. **Clothing**, which needs only the garment.
//! 5. **Teeth and eyes last**, because both are statistics over small regions and both read the
//!    face's own skin as a reference.
//! 6. **The guard, over the whole frame.** Last, because it measures the *plan* rather than any
//!    one operation, and because a re-solve has to weaken a whole family together.
//! 7. **Reasons and confidence**, assembled from what happened rather than from what was
//!    intended - which is why they are last.
//!
//! ## Where the strength of an operation comes from
//!
//! Three multiplied numbers and no fourth. The **scene limit** from the matrix file, the
//! **region's own quality** through `MicroField::strength_scale`, and the **contract's ceiling**
//! for that operator. There is no per-photographer strength here and deliberately no
//! per-identity one: phase 20's gallery-constant strength exists because skin retouching varies
//! by how much of a person is in frame, and none of these five operations does. A piece of lint
//! is a piece of lint.
//!
//! ## The placeholder, said once and plainly
//!
//! [`FLYAWAY_HEAD_TRAINED`], [`GLARE_HEAD_TRAINED`] and [`LINT_HEAD_TRAINED`] are all false, so
//! none of the three shipped heads is consulted. What runs is the measured detection in
//! [`super::hair`], [`super::glare`] and [`super::clothing`], and ADR-0043 section 6 records why
//! that is a decision rather than a fallback. Every plan carries
//! [`MicroCode::HeadUntrained`] so nothing downstream can describe this output as learned.

use std::collections::BTreeMap;

use aura_core::contract::error::AuraError;
use aura_core::contract::micro::{
    ClothingIssue, GlareMethod, ImageId, MicroCode, MicroField, MicroOp, MicroPlan, MicroReason,
    MicroRegion, NaturalnessReport, MAX_CLOTHING_STRENGTH, MAX_FLYAWAY_AREA, MAX_FLYAWAY_STRENGTH,
    MAX_OPS,
};
use aura_core::contract::people::FaceRef;
use aura_core::SceneId;
use aura_raw::contract::pixels::{PixelBuffer, PixelData, PixelLevel};
use aura_render::micro::{FaceGeometry, MicroContext};

use crate::errors;
use crate::micro::borrow::{self, Refusal, SiblingFrame};
use crate::micro::clothing;
use crate::micro::eyes;
use crate::micro::glare;
use crate::micro::guard;
use crate::micro::hair;
use crate::micro::matrix::MicroTable;
use crate::micro::teeth;
use crate::texture_guard::Frame;

/// Which build's arithmetic produced a plan.
///
/// Bumped on any change to a detector threshold, a cap, a measurement or the way confidence is
/// combined. Written into `micro_plan.analysis_ver`; two plans made under different values of it
/// are not comparable, and `AURA-ML-5096` exists so that comparison never happens silently.
pub const ANALYSIS_VER: u16 = 1;

/// The version stamped on every `micro_plan.model_ver`.
///
/// Three heads, one number, for the reason phases 09, 11, 15, 19 and 20 all fold theirs: no
/// consumer of a plan cares *which* head moved, only that the numbers are not comparable across
/// the move.
pub const MODEL_VER: u16 = 100;

/// The pixel rung the micro pass reads.
///
/// Tier 2, the 2048 px proxy, as phase 20's is. The *decision* is made here and the same
/// operations are applied at whatever resolution the export asks for, because every constant in
/// `aura_render::micro` is a fraction of the thing it measures rather than a number of pixels.
pub const MICRO_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// Whether the pinned flyaway weights have passed the section 10.1 gate.
pub const FLYAWAY_HEAD_TRAINED: bool = false;

/// Whether the pinned glare weights have passed the section 10.1 gate.
pub const GLARE_HEAD_TRAINED: bool = false;

/// Whether the pinned lint weights have passed the section 10.1 gate.
pub const LINT_HEAD_TRAINED: bool = false;

/// The smallest share of the frame a face may be and still have its teeth or eyes worked on.
///
/// Phase 20's `MIN_RETOUCHABLE_FACE` is about skin over a whole face; this is about an iris and a
/// row of teeth, which are perhaps a fortieth of it, so the floor is higher. Below this the
/// correction is smaller than the proxy's own pixel and would only ever be visible as noise.
pub const MIN_DETAILED_FACE: f32 = 0.075;

/// One sibling frame offered to the borrow search.
///
/// Handed in decoded rather than fetched, because this crate has no route to a preview service of
/// its own and because the pass already holds one. `aura_retouch::micro::api` fills these from
/// phase 08's moment grouping and nothing else may.
#[derive(Debug, Clone)]
pub struct Sibling {
    /// Which photograph. **This id is the disclosure** - it reaches the operation, the row and
    /// the panel.
    pub image: ImageId,
    /// Its proxy, at [`MICRO_LEVEL`].
    pub pixels: PixelBuffer,
    /// The faces phase 06 found on it.
    pub faces: Vec<FaceRef>,
}

/// Everything known about a frame before its pixels are read.
///
/// All of it arrives through a frozen service: `PeopleService` for the faces, `StoryService` for
/// the scene, `MaskService` for the regions - through the [`MicroField`] port - `ToneService` for
/// the neutral, and `MomentService` for the siblings.
#[derive(Debug, Clone)]
pub struct MicroFrame {
    /// What the photograph is of. Invariant 7.
    pub scene: SceneId,
    /// The faces phase 06 found, in prominence order.
    pub faces: Vec<FaceRef>,
    /// The regions phase 18 generated.
    ///
    /// **Empty on a build with no mask generator wired in**, which is the honest state of this
    /// repository. Every operation is then skipped and the plan says
    /// [`MicroCode::RegionUnavailable`]; there is no geometric fallback, for the reason phase 19
    /// gives - a rectangle's edge does not follow a person, and a teeth correction through one
    /// whitens a lip.
    pub regions: BTreeMap<MicroRegion, MicroField>,
    /// The frame's own neutral in CIE `u'v'`, from phase 15.
    ///
    /// `None` skips every colour move and records [`MicroCode::NoIlluminant`]: a locus with no
    /// origin describes nothing.
    pub neutral: Option<[f32; 2]>,
    /// Which of the five operations this project permits, in `MicroOp::NAMES` order.
    pub allowed: [bool; 5],
    /// Which clothing issues this project permits, in `ClothingIssue::ALL` order.
    pub clothing: [bool; ClothingIssue::COUNT],
    /// Whether this project permits cross-frame borrowing at all.
    pub borrowing: bool,
    /// Frames from the same moment carrying the same people, for the borrow search.
    pub siblings: Vec<Sibling>,
    /// Whether micro-retouch is switched on for this project. The kill switch hard rule 8 wants.
    pub enabled: bool,
}

impl MicroFrame {
    /// A frame with the defaults a photograph nobody has analysed would have.
    #[must_use]
    pub fn new(scene: SceneId) -> Self {
        Self {
            scene,
            faces: Vec::new(),
            regions: BTreeMap::new(),
            neutral: None,
            allowed: [true; 5],
            clothing: [true, true, true, false, false],
            borrowing: true,
            siblings: Vec::new(),
            enabled: true,
        }
    }

    /// The usable field for one region, or `None` when it is absent or too doubtful.
    ///
    /// The two states are separated by the caller, which records a different code for each -
    /// `RegionUnavailable` against `RegionDoubtful` - because they send a support engineer to two
    /// different places.
    #[must_use]
    pub fn field(&self, region: MicroRegion) -> Option<&MicroField> {
        self.regions.get(&region)
    }

    /// True when at least one region arrived and could be used.
    #[must_use]
    pub fn any_region(&self) -> bool {
        self.regions.values().any(MicroField::is_usable)
    }
}

/// One frame's answer.
#[derive(Debug, Clone)]
pub struct MicroOutcome {
    /// The plan.
    pub plan: MicroPlan,
    /// The edited pixels, for a caller that wants to show them.
    pub rendered: Option<Vec<f32>>,
    /// Anything worth telling a support engineer. Never fatal.
    pub warnings: Vec<AuraError>,
}

/// One frame in, one plan out.
#[derive(Debug, Clone)]
pub struct Analyser {
    table: MicroTable,
}

impl Analyser {
    /// Build an analyser over the embedded matrix.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5099` when the table will not load.
    pub fn new() -> Result<Self, AuraError> {
        Ok(Self {
            table: MicroTable::embedded()?,
        })
    }

    /// Build an analyser over a supplied table, for the gate and the eval harness.
    #[must_use]
    pub const fn with_table(table: MicroTable) -> Self {
        Self { table }
    }

    /// The table underneath.
    #[must_use]
    pub const fn table(&self) -> &MicroTable {
        &self.table
    }

    /// Plan one photograph.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5097` when the buffer cannot be read as pixels.
    #[allow(clippy::too_many_lines)]
    pub fn analyse(
        &self,
        image: ImageId,
        pixels: &PixelBuffer,
        context: &MicroFrame,
    ) -> Result<MicroOutcome, AuraError> {
        let mut reasons: Vec<MicroReason> = Vec::new();
        let mut warnings: Vec<AuraError> = Vec::new();

        // First on every plan, so nothing downstream can read this output as learned.
        reasons.push(MicroReason::plain(MicroCode::HeadUntrained, -0.05));

        if !context.enabled {
            reasons.push(MicroReason::plain(MicroCode::Disabled, 0.0));
            return Ok(nothing(
                image,
                context,
                reasons,
                warnings,
                self.table.version(),
            ));
        }

        let (scene_row, scene_found) = self.table.scene(context.scene);
        if !scene_found {
            warnings.push(errors::micro_region_unusable(
                &image.to_db(),
                "scene",
                format!(
                    "`{}` has no row, so the neutral row was used",
                    context.scene
                ),
            ));
            reasons.push(MicroReason::plain(MicroCode::SceneLimited, -0.10));
        }
        if scene_row.limit <= 0.0 {
            reasons.push(MicroReason::plain(MicroCode::SceneLimited, 0.0));
            return Ok(nothing(
                image,
                context,
                reasons,
                warnings,
                self.table.version(),
            ));
        }
        let scene_limit = scene_row.limit;
        // The matrix and the scene both have to say yes. Two independent switches rather than
        // one: a studio's policy is about the wedding and a scene row is about the kind of
        // photograph, and collapsing them would make "no teeth work in the ceremony" and "no
        // teeth work at all" the same setting.
        let mut allowed = [false; 5];
        for index in 0..5 {
            if let Some(slot) = allowed.get_mut(index) {
                *slot =
                    context.allowed.get(index).copied().unwrap_or(false) && scene_row.allows(index);
            }
        }
        if context.allowed.iter().any(|flag| !*flag) {
            reasons.push(MicroReason::plain(MicroCode::OptedOut, 0.0));
        }

        if !context.any_region() {
            let code = if context.regions.is_empty() {
                MicroCode::RegionUnavailable
            } else {
                MicroCode::RegionDoubtful
            };
            reasons.push(MicroReason::plain(code, -0.25));
            warnings.push(errors::micro_region_unusable(
                &image.to_db(),
                "any",
                "did not arrive, or was too doubtful to act through",
            ));
            let mut plan = nothing(image, context, reasons, warnings, self.table.version());
            plan.plan.allowed = allowed;
            return Ok(plan);
        }

        let Some(frame) = to_linear(pixels) else {
            return Err(errors::micro_failed(
                &image.to_db(),
                "the proxy carries no readable pixels",
            ));
        };

        let planes = Self::planes(context, frame.width, frame.height);
        let mut ops: Vec<MicroOp> = Vec::new();
        let mut geometry: Vec<FaceGeometry> = Vec::new();
        let mut borrows: Vec<aura_render::micro::BorrowPatch> = Vec::new();

        // --- glare, first --------------------------------------------------------------------
        if allowed.get(4).copied().unwrap_or(false) {
            let eye_plane = planes.get(&MicroRegion::Eyes).cloned().unwrap_or_default();
            let scale = region_scale(context, MicroRegion::Eyes);
            let sheets = glare::detect(&frame, &eye_plane, &context.faces);
            if sheets.is_empty() {
                // Not a reason on its own: a frame with no glasses in it has nothing to say
                // about glare, and a code per absence would drown the panel.
            }
            for sheet in &sheets {
                if ops.len() >= MAX_OPS {
                    break;
                }
                let Some(face) = context.faces.get(sheet.face) else {
                    continue;
                };
                if sheet.may_borrow() && context.borrowing {
                    let siblings = decode_siblings(&context.siblings);
                    match borrow::choose(&frame, sheet.region, face, &siblings) {
                        Ok(candidate) => {
                            borrows.push(candidate.patch);
                            ops.push(MicroOp::Glare {
                                region: sheet.region,
                                method: GlareMethod::BorrowFrom {
                                    source: candidate.source,
                                    alignment: candidate.alignment,
                                },
                            });
                            // Disclosure, first class. The reason carries the evidence rectangle
                            // so the panel can draw exactly which pixels came from elsewhere.
                            reasons.push(MicroReason::at(
                                MicroCode::BorrowedFromSibling,
                                format!(
                                    "{} - from {}, aligned {:.2}",
                                    MicroCode::BorrowedFromSibling.user_text(),
                                    candidate.source,
                                    candidate.alignment
                                ),
                                0.05,
                                sheet.region,
                            ));
                            continue;
                        }
                        Err(
                            Refusal::NoSibling | Refusal::NoAlignment | Refusal::SiblingAlsoGlared,
                        ) => {
                            reasons.push(MicroReason::plain_at(
                                MicroCode::BorrowNoAlignedSibling,
                                -0.05,
                                sheet.region,
                            ));
                        }
                        Err(Refusal::StillInformative) => {
                            reasons.push(MicroReason::plain_at(
                                MicroCode::BorrowRefusedInformative,
                                0.0,
                                sheet.region,
                            ));
                        }
                    }
                } else if context.borrowing {
                    // Two different refusals, and they are recorded apart because a photographer
                    // reads them differently. The first says the photograph still holds the eye,
                    // so a reduction is the *better* repair. The second says it does not, and
                    // that rebuilding this much of a face would be a composite rather than a
                    // patch - which is the line `docs/retouch-ethics.md` section 5 draws.
                    let code = if sheet.clipped_fraction
                        < aura_core::contract::micro::MIN_SPECULAR_FRACTION
                    {
                        MicroCode::BorrowRefusedInformative
                    } else {
                        MicroCode::BorrowRefusedTooLarge
                    };
                    reasons.push(MicroReason::plain_at(code, 0.0, sheet.region));
                }
                let strength = glare::reduce_strength(sheet, scale * scene_limit);
                if strength > 0.0 {
                    ops.push(MicroOp::Glare {
                        region: sheet.region,
                        method: GlareMethod::Reduce { strength },
                    });
                    reasons.push(MicroReason::plain_at(
                        MicroCode::GlareReduced,
                        0.05,
                        sheet.region,
                    ));
                }
            }
        }

        // --- hair ---------------------------------------------------------------------------
        if allowed.first().copied().unwrap_or(false) {
            match planes.get(&MicroRegion::Hair) {
                None => reasons.push(MicroReason::plain(MicroCode::RegionUnavailable, -0.05)),
                Some(plane) => {
                    let scale = region_scale(context, MicroRegion::Hair);
                    let candidates = hair::detect(&frame, plane);
                    let mut area = 0.0f32;
                    let mut calmed = 0u32;
                    let mut busy = 0u32;
                    for candidate in &candidates {
                        if candidate.background_busy {
                            busy += 1;
                            continue;
                        }
                        if !candidate.is_actionable() {
                            continue;
                        }
                        let next = area + candidate.region.w * candidate.region.h;
                        if next > MAX_FLYAWAY_AREA {
                            reasons.push(MicroReason::plain(MicroCode::FlyawayAreaCapped, 0.0));
                            break;
                        }
                        if ops.len() >= MAX_OPS {
                            break;
                        }
                        area = next;
                        ops.push(MicroOp::Flyaway {
                            region: candidate.region,
                            strength: (candidate.contrast * scale * scene_limit)
                                .clamp(0.0, MAX_FLYAWAY_STRENGTH),
                        });
                        calmed += 1;
                    }
                    if calmed > 0 {
                        reasons.push(MicroReason::plain(MicroCode::FlyawayCalmed, 0.10));
                    }
                    if busy > 0 {
                        reasons.push(MicroReason::plain(MicroCode::BackgroundBusy, 0.0));
                    }
                    if candidates.is_empty() {
                        reasons.push(MicroReason::plain(MicroCode::NoFlyawayFound, 0.0));
                    }
                }
            }
        }

        // --- clothing -----------------------------------------------------------------------
        if allowed.get(3).copied().unwrap_or(false) {
            let garment = union(
                &planes,
                &[MicroRegion::Clothing, MicroRegion::Dress],
                frame.width * frame.height,
            );
            if garment.is_empty() {
                reasons.push(MicroReason::plain(MicroCode::RegionUnavailable, -0.05));
            } else {
                let scale = region_scale(context, MicroRegion::Clothing)
                    .max(region_scale(context, MicroRegion::Dress));
                let mut cleaned = 0u32;
                let mut large = 0u32;
                let mut textured = 0u32;
                for mark in clothing::detect(&frame, &garment) {
                    if mark.too_large {
                        large += 1;
                        continue;
                    }
                    if mark.fabric_busy {
                        textured += 1;
                        continue;
                    }
                    if !mark.is_actionable() {
                        continue;
                    }
                    let index = ClothingIssue::ALL
                        .iter()
                        .position(|kind| *kind == mark.kind)
                        .unwrap_or(usize::MAX);
                    if !context.clothing.get(index).copied().unwrap_or(false) {
                        reasons.push(MicroReason::plain_at(MicroCode::OptedOut, 0.0, mark.region));
                        continue;
                    }
                    if ops.len() >= MAX_OPS {
                        break;
                    }
                    ops.push(MicroOp::Clothing {
                        region: mark.region,
                        kind: mark.kind,
                        strength: (mark.departure * 4.0 * scale * scene_limit)
                            .clamp(0.0, MAX_CLOTHING_STRENGTH),
                    });
                    cleaned += 1;
                }
                if cleaned > 0 {
                    reasons.push(MicroReason::plain(MicroCode::ClothingCleaned, 0.10));
                }
                if large > 0 {
                    reasons.push(MicroReason::plain(MicroCode::ClothingTooLarge, 0.0));
                }
                if textured > 0 {
                    reasons.push(MicroReason::plain(MicroCode::FabricTooTextured, 0.0));
                }
            }
        }

        // --- teeth and eyes, per face ---------------------------------------------------------
        if context.faces.is_empty() {
            reasons.push(MicroReason::plain(MicroCode::NoFaces, -0.10));
        }
        let skin_plane = planes.get(&MicroRegion::Skin).cloned().unwrap_or_default();
        for face in &context.faces {
            if face.area_frac.sqrt() < MIN_DETAILED_FACE {
                reasons.push(MicroReason::plain_at(
                    MicroCode::FaceTooSmall,
                    0.0,
                    face.bbox,
                ));
                continue;
            }
            let Some(identity) = face.identity_id else {
                // Both remaining operators name a person, and an operation that names nobody
                // cannot be reviewed, overridden or explained. Phase 20's rule.
                continue;
            };
            if !face.has_eyes() {
                reasons.push(MicroReason::plain(MicroCode::NoEyeLandmarks, -0.05));
                continue;
            }
            geometry.push(FaceGeometry {
                left_eye: [
                    face.eyes[0][0] * frame.width as f32,
                    face.eyes[0][1] * frame.height as f32,
                ],
                right_eye: [
                    face.eyes[1][0] * frame.width as f32,
                    face.eyes[1][1] * frame.height as f32,
                ],
                bbox: face.bbox,
            });

            // --- teeth ------------------------------------------------------------------------
            if allowed.get(1).copied().unwrap_or(false) {
                match planes.get(&MicroRegion::Teeth) {
                    None => reasons.push(MicroReason::plain(MicroCode::RegionUnavailable, -0.05)),
                    Some(plane) => {
                        let scale = region_scale(context, MicroRegion::Teeth);
                        match teeth::measure(&frame, plane) {
                            None => {
                                reasons.push(MicroReason::plain(MicroCode::MouthTooSmall, 0.0));
                            }
                            Some(reading) => {
                                if context.neutral.is_none() {
                                    reasons
                                        .push(MicroReason::plain(MicroCode::NoIlluminant, -0.10));
                                }
                                let peak = teeth::skin_peak(&frame, &skin_plane, face);
                                if let Some(decision) = teeth::solve(
                                    &reading,
                                    context.neutral,
                                    self.table.guard().teeth_locus,
                                    peak,
                                    scale * scene_limit,
                                ) {
                                    if decision.already_natural && decision.luma_ev <= 0.0 {
                                        reasons.push(MicroReason::plain(
                                            MicroCode::TeethAlreadyNatural,
                                            0.0,
                                        ));
                                    } else if !decision.is_noop() && ops.len() < MAX_OPS {
                                        ops.push(MicroOp::Teeth {
                                            identity,
                                            luma: decision.luma_ev,
                                            yellow_reduce: decision.yellow_reduce,
                                        });
                                        reasons.push(MicroReason::plain(
                                            MicroCode::TeethCorrected,
                                            0.05,
                                        ));
                                        if decision.skin_bound {
                                            reasons.push(MicroReason::plain(
                                                MicroCode::TeethWouldOutshineSkin,
                                                0.0,
                                            ));
                                        } else if decision.capped {
                                            reasons.push(MicroReason::plain(
                                                MicroCode::TeethCapped,
                                                0.0,
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // --- eyes --------------------------------------------------------------------------
            if allowed.get(2).copied().unwrap_or(false) {
                let sclera = planes
                    .get(&MicroRegion::Sclera)
                    .cloned()
                    .unwrap_or_default();
                let iris = planes.get(&MicroRegion::Iris).cloned().unwrap_or_default();
                if sclera.is_empty() && iris.is_empty() {
                    reasons.push(MicroReason::plain(MicroCode::RegionUnavailable, -0.05));
                } else {
                    let scale = region_scale(context, MicroRegion::Sclera)
                        .max(region_scale(context, MicroRegion::Iris));
                    let reading = eyes::measure(&frame, &sclera, &iris);
                    match eyes::solve(
                        &reading,
                        context.neutral,
                        self.table.sclera_locus(),
                        scale * scene_limit,
                    ) {
                        None => reasons.push(MicroReason::plain(MicroCode::EyesAlreadyClear, 0.0)),
                        Some(decision) if decision.is_noop() => {
                            reasons.push(MicroReason::plain(MicroCode::EyesAlreadyClear, 0.0));
                        }
                        Some(decision) => {
                            if ops.len() < MAX_OPS {
                                ops.push(MicroOp::Eyes {
                                    identity,
                                    sclera: decision.sclera,
                                    iris_clarity: decision.iris_clarity,
                                });
                                if decision.sclera > 0.0 {
                                    reasons
                                        .push(MicroReason::plain(MicroCode::ScleraCleared, 0.05));
                                }
                                if decision.iris_clarity > 0.0 {
                                    reasons
                                        .push(MicroReason::plain(MicroCode::IrisClarified, 0.05));
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- the guarantee ---------------------------------------------------------------------
        let render_context = MicroContext {
            regions: planes,
            faces: geometry,
            neutral: context.neutral,
            teeth_locus: self.table.guard().teeth_locus,
            borrows,
        };
        let guarded = guard::enforce(&frame, &ops, &render_context);

        if guarded
            .report
            .is_withdrawn(aura_core::contract::micro::OpFamily::Hair)
        {
            reasons.push(MicroReason::plain(MicroCode::HairEnergyLost, -0.20));
            warnings.push(errors::naturalness_guard(
                &image.to_db(),
                "hair",
                guarded.report.hair_energy_ratio,
                aura_core::contract::micro::HAIR_ENERGY_FLOOR,
                true,
            ));
        }
        if guarded
            .report
            .is_withdrawn(aura_core::contract::micro::OpFamily::Eyes)
        {
            reasons.push(MicroReason::plain(MicroCode::CatchlightAtRisk, -0.20));
            warnings.push(errors::naturalness_guard(
                &image.to_db(),
                "eyes",
                guarded.report.catchlight_ratio,
                aura_core::contract::micro::CATCHLIGHT_FLOOR,
                true,
            ));
        }
        if guarded
            .report
            .is_withdrawn(aura_core::contract::micro::OpFamily::Teeth)
        {
            reasons.push(MicroReason::plain(MicroCode::TeethCapped, -0.15));
            warnings.push(errors::naturalness_guard(
                &image.to_db(),
                "teeth",
                guarded.report.teeth_excursion,
                aura_core::contract::micro::TEETH_EXCURSION_CEILING,
                true,
            ));
        }
        if guarded.report.resolves > 0 && !guarded.report.any_withdrawn() {
            warnings.push(errors::naturalness_guard(
                &image.to_db(),
                "plan",
                guarded.report.catchlight_ratio,
                aura_core::contract::micro::CATCHLIGHT_FLOOR,
                false,
            ));
        }

        // Reasons that named an operation the guard then withdrew have to go, or the plan claims
        // to have done something it did not. The codes below are exactly the ones a withdrawn
        // family emits.
        prune_withdrawn(&mut reasons, &guarded.report);

        let budget_used = budget(&frame, &guarded.rendered);
        let confidence = confidence(&reasons);

        let plan = MicroPlan {
            image_id: image,
            ops: guarded.ops,
            naturalness: guarded.report,
            allowed,
            reasons,
            confidence,
            scene: context.scene,
            budget_used,
            user_edited: false,
            reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            matrix_ver: self.table.version(),
        };

        Ok(MicroOutcome {
            plan,
            rendered: Some(guarded.rendered),
            warnings,
        })
    }

    /// Turn every arrived region into a per-pixel plane at the frame's resolution.
    ///
    /// A region that is absent or too doubtful is simply not in the map, which is what makes
    /// `planes.get(..)` returning `None` mean "may not act here" everywhere above.
    fn planes(
        context: &MicroFrame,
        width: usize,
        height: usize,
    ) -> BTreeMap<MicroRegion, Vec<f32>> {
        let mut out = BTreeMap::new();
        for region in MicroRegion::ALL {
            let Some(field) = context.field(region) else {
                continue;
            };
            if !field.is_usable() || field.problem().is_some() {
                continue;
            }
            out.insert(region, upsample(field, width, height));
        }
        out
    }
}

/// A plan that does nothing, carrying the reasons that say why.
fn nothing(
    image: ImageId,
    context: &MicroFrame,
    reasons: Vec<MicroReason>,
    warnings: Vec<AuraError>,
    matrix_ver: u16,
) -> MicroOutcome {
    let confidence = confidence(&reasons);
    let plan = MicroPlan {
        image_id: image,
        ops: Vec::new(),
        naturalness: NaturalnessReport::UNTOUCHED,
        allowed: [false; 5],
        reasons,
        confidence,
        scene: context.scene,
        budget_used: 0.0,
        user_edited: false,
        reviewed: false,
        model_ver: MODEL_VER,
        analysis_ver: ANALYSIS_VER,
        matrix_ver,
    };
    MicroOutcome {
        plan,
        rendered: None,
        warnings,
    }
}

/// Drop the reasons that claim an operation a withdrawn family no longer carries.
fn prune_withdrawn(reasons: &mut Vec<MicroReason>, report: &NaturalnessReport) {
    use aura_core::contract::micro::OpFamily;
    let hair = report.is_withdrawn(OpFamily::Hair);
    let teeth = report.is_withdrawn(OpFamily::Teeth);
    let eyes = report.is_withdrawn(OpFamily::Eyes);
    reasons.retain(|reason| match reason.code {
        MicroCode::FlyawayCalmed | MicroCode::FlyawayAreaCapped => !hair,
        MicroCode::TeethCorrected | MicroCode::TeethAlreadyNatural => !teeth,
        MicroCode::ScleraCleared
        | MicroCode::IrisClarified
        | MicroCode::GlareReduced
        | MicroCode::BorrowedFromSibling => !eyes,
        _ => true,
    });
}

/// How much of the shared per-image allowance a micro pass spent.
///
/// **Phase 19's allowance, shared for the third time.** Measured the way phases 19 and 20 measure
/// it: mean absolute change in a perceptual space, over the whole frame - which is the right
/// denominator here, unlike phase 20's skin-weighted one, because these five operations act in
/// five different regions and there is no single one they share.
#[must_use]
pub fn budget(before: &Frame, after: &[f32]) -> f32 {
    let mut total = 0.0f64;
    let mut count = 0u64;
    for slot in 0..before.width * before.height * 3 {
        let a = before.rgb.get(slot).copied().unwrap_or(0.0);
        let b = after.get(slot).copied().unwrap_or(a);
        total += f64::from((encode(a) - encode(b)).abs());
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    let change = (total / count as f64) as f32;
    (change / aura_core::contract::local::PERCEPTUAL_BUDGET).clamp(0.0, 1.0)
}

/// The perceptual position of a linear value, for the budget measurement only.
///
/// Not an output transform: nothing encoded here reaches a pixel, a file or a buffer. It exists
/// so the number the governor spends is in the space phases 19 and 20 spend it in.
fn encode(linear: f32) -> f32 {
    linear.max(0.0).powf(1.0 / 2.2)
}

/// Combine the reason weights into a confidence.
///
/// Starts at three quarters rather than at one, as phase 20's does: on this build no plan is ever
/// fully trusted, and the first reason on every plan says why.
#[must_use]
pub fn confidence(reasons: &[MicroReason]) -> f32 {
    let total: f32 = reasons.iter().map(|reason| reason.weight).sum();
    (0.75 + total).clamp(0.0, 1.0)
}

/// One region's own strength multiplier, or zero when it did not arrive.
#[must_use]
pub fn region_scale(context: &MicroFrame, region: MicroRegion) -> f32 {
    context
        .field(region)
        .map_or(0.0, MicroField::strength_scale)
        .max(0.0)
}

/// The union of several regions as one plane.
fn union(
    planes: &BTreeMap<MicroRegion, Vec<f32>>,
    regions: &[MicroRegion],
    pixels: usize,
) -> Vec<f32> {
    let present: Vec<&Vec<f32>> = regions.iter().filter_map(|r| planes.get(r)).collect();
    if present.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0.0f32; pixels];
    for plane in present {
        for index in 0..pixels {
            let value = plane.get(index).copied().unwrap_or(0.0);
            if let Some(slot) = out.get_mut(index) {
                *slot = slot.max(value);
            }
        }
    }
    out
}

/// Turn a coarse field into a per-pixel weight.
///
/// Bilinear and **clamped at the edges rather than zero-padded** - the defect phase 18 found in
/// its own resampler, inherited as a rule: reading zero outside the plane darkens the outermost
/// half-pixel of every upsampled region, which here would leave a rim of untouched hair around
/// every head.
#[must_use]
pub fn upsample(field: &MicroField, width: usize, height: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; width * height];
    if field.width == 0 || field.height == 0 || width == 0 || height == 0 {
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

/// Decode the sibling proxies the pass offered.
///
/// Siblings whose pixels will not read are dropped rather than erroring: a borrow that cannot
/// happen is an ordinary outcome and the frame is repaired from itself instead.
fn decode_siblings(siblings: &[Sibling]) -> Vec<SiblingFrame> {
    let mut out = Vec::new();
    for sibling in siblings {
        let Some(frame) = to_linear(&sibling.pixels) else {
            continue;
        };
        // The same person, and the face that carries landmarks. A sibling whose faces phase 06
        // could not land on is no use for an alignment seeded by two eyes.
        let Some(face) = sibling.faces.iter().find(|f| f.has_eyes()).copied() else {
            continue;
        };
        out.push(SiblingFrame {
            image: sibling.image,
            frame,
            face,
        });
    }
    out
}

/// Read a proxy buffer as linear RGB.
///
/// `None` when the buffer carries tiles, which this pass never asks for: tiling is an export path
/// and this is a decision path.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::micro::fixtures;

    #[test]
    fn switching_the_stage_off_produces_a_plan_rather_than_no_plan() {
        let (image, pixels, mut context) = fixtures::planned_frame();
        context.enabled = false;
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome.plan.is_noop());
        assert!(outcome
            .plan
            .reasons
            .iter()
            .any(|r| r.code == MicroCode::Disabled));
        assert!(outcome.plan.is_sound());
    }

    #[test]
    fn a_frame_with_no_regions_is_planned_and_says_why() {
        let (image, pixels, mut context) = fixtures::planned_frame();
        context.regions.clear();
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome.plan.is_noop());
        assert!(outcome
            .plan
            .reasons
            .iter()
            .any(|r| r.code == MicroCode::RegionUnavailable));
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
            .any(|r| r.code == MicroCode::HeadUntrained));
        // Read through a binding so this checks the constants rather than something the compiler
        // folds away. When a head is trained, this is what fails - and it fails in the right
        // place, because the plan must stop saying `head_untrained` on the same day.
        let trained = [FLYAWAY_HEAD_TRAINED, GLARE_HEAD_TRAINED, LINT_HEAD_TRAINED];
        assert!(trained.iter().all(|flag| !flag));
    }

    #[test]
    fn an_operation_the_matrix_forbids_never_appears_in_a_plan() {
        let (image, pixels, mut context) = fixtures::planned_frame();
        context.allowed = [false; 5];
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(outcome.plan.is_noop(), "{:?}", outcome.plan.ops);
        assert!(outcome.plan.is_sound());
    }

    #[test]
    fn the_end_to_end_fixture_actually_produces_operations() {
        // The test that keeps every other test in this module honest. Four of the five refusal
        // tests below assert that a plan is empty, and all four would pass on a pass that could
        // never produce anything at all.
        let (image, pixels, context) = fixtures::planned_frame();
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert!(
            !outcome.plan.is_noop(),
            "the end-to-end fixture produced nothing: {:?}",
            outcome
                .plan
                .reasons
                .iter()
                .map(|r| r.code.as_str())
                .collect::<Vec<_>>()
        );
        assert!(outcome.plan.is_sound());
        assert!(outcome.plan.budget_used <= 1.0);
    }

    #[test]
    fn a_borrow_is_disclosed_on_the_plan_and_in_the_reasons() {
        let (image, pixels, context) = fixtures::glare_frame();
        let analyser = Analyser::new().expect("an analyser");
        let outcome = analyser.analyse(image, &pixels, &context).expect("a plan");
        if outcome.plan.is_composite() {
            assert!(
                !outcome.plan.borrowed_from().is_empty(),
                "a composite plan named no source"
            );
            assert!(outcome
                .plan
                .reasons
                .iter()
                .any(|r| r.code == MicroCode::BorrowedFromSibling));
        } else {
            // A borrow that did not happen is also a valid outcome, and it must have said why.
            assert!(
                outcome.plan.reasons.iter().any(|r| {
                    r.code == MicroCode::BorrowNoAlignedSibling
                        || r.code == MicroCode::BorrowRefusedInformative
                        || r.code == MicroCode::RegionUnavailable
                        || r.code == MicroCode::CatchlightAtRisk
                }),
                "no borrow and no reason: {:?}",
                outcome
                    .plan
                    .reasons
                    .iter()
                    .map(|r| r.code.as_str())
                    .collect::<Vec<_>>()
            );
        }
        assert!(outcome.plan.is_sound());
    }

    #[test]
    fn planning_is_deterministic() {
        let (image, pixels, context) = fixtures::planned_frame();
        let analyser = Analyser::new().expect("an analyser");
        let first = analyser.analyse(image, &pixels, &context).expect("a plan");
        let second = analyser.analyse(image, &pixels, &context).expect("a plan");
        assert_eq!(first.plan, second.plan);
    }
}
