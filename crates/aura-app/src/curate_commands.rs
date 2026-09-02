//! The curation command surface. PHASE-29.
//!
//! Eleven commands. Seven read - the header, the whole pass, the monochrome list, the hero grid,
//! the album, one spread and the sets - one runs the pass, two record what a photographer decided,
//! and one produces a specification. ADR-0060 records the shape and what is deliberately absent
//! from it.
//!
//! # What this surface does that no earlier command surface does
//!
//! **Its primary object is a deliverable.** Every earlier panel answers "what did AURA decide about
//! this photograph", and the reader is checking a measurement. Here the reader is making a decision
//! about their own portfolio, their own album and their own feed - so what travels beside every pick
//! is enough to *disagree quickly*, and the panel's cheapest action is rejection rather than
//! acceptance.
//!
//! # The field, and why it lives here
//!
//! `aura-curate` depends on none of the deciding crates: it takes its readings through
//! [`CurateField`], which is this module's implementation of `aura_curate::Field`. That indirection
//! is what stops `aura-cull` - the crate that decided what is in the gallery - from being visible to
//! the crate that curates it, and it is where every `None` in a `Frame` comes from.
//!
//! In this build most of them are absent. Phase 06's detector finds no faces, so `faces` is empty,
//! shot scale is `Unknown` on nearly every frame and `facing` is `Unknown` everywhere; phase 15 has
//! no skin loci to hand out, so no monochrome mix is offered on a frame with people in it. Those are
//! honest answers rather than defects, and `CurateStatusDto.rhythmMeasurable` is where a
//! photographer sees the consequence.
//!
//! # Why `curate_set_order` runs the pass
//!
//! Because "reordering is instant and remembered" is two things. `CurateService::set_order` records
//! the sequence and stops - which two images share a spread is still AURA's decision, and the
//! service has no readings to make it with. This command records and then re-composes, so the panel
//! that re-fetches sees what the photographer dragged. ADR-0060 section 4.
//!
//! # What is not here
//!
//! No `curate_apply`, no threshold read or write, no B&W strength, no bulk decide, and no command
//! that changes a photograph or the delivered gallery. ADR-0060 sections 5 and 6.

#![allow(clippy::cast_possible_truncation)]

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::contract::cull::{CoverageReport, CullService as _, MustHave};
use aura_core::contract::curate::{
    album_summary, AlbumPlan, AspectVariant, BwPick, CurateOverride, CurateReason, CurateService,
    CurationOutline, ExportFormat, ExportSubject, HeroPick, ImageId, PickKind, SocialPick,
    SocialSets, Spread, TeaserPick,
};
use aura_core::contract::ids::{IdentityId, SpreadId};
use aura_core::{AuraResult, ProjectId};
use aura_curate::api::{Curate, CuratePass};
use aura_curate::read::{Descriptor, FaceRead, Field, Frame};
use aura_curate::store::CurateStore;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    CurateAlbumDto, CurateBwDto, CurateCaptionDto, CurateChapterDto, CurateDecideInput,
    CurateExportDto, CurateExportInput, CurateHeroDto, CurateOrderInput, CuratePickDto,
    CurateProjectInput, CurateReasonDto, CurateSocialDto, CurateSpreadDto, CurateStatusDto,
    IpcError,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// The field
// ---------------------------------------------------------------------------

/// One project's readings, gathered once.
///
/// Gathered rather than fetched per call, for the reason phase 27's `AppField` gathers its
/// set-scoped readings: the hero selector asks every candidate about every chosen hero, and a
/// service round trip per question would be twenty thousand catalog reads inside a twenty-second
/// budget.
#[derive(Debug)]
pub struct CurateField {
    frames: Vec<Frame>,
    photos: u32,
    coverage: CoverageReport,
    loci: BTreeMap<IdentityId, u8>,
    rituals: Vec<String>,
    close_family: (Vec<IdentityId>, u32),
    /// The phase 05 vectors, keyed by image. Loaded once; the distance is the index's own.
    vectors: BTreeMap<ImageId, Vec<f32>>,
}

impl CurateField {
    /// Assemble one project's field.
    ///
    /// # Errors
    ///
    /// Whatever the frozen services raise, most often `AURA-DB-3006`.
    pub fn new(state: &AppState, project: ProjectId) -> AuraResult<Self> {
        let selection = state.cull()?.selection(project)?;
        let coverage = selection
            .as_ref()
            .map(|s| s.coverage.clone())
            .unwrap_or_default();
        let selected: Vec<ImageId> = selection
            .map(|s| s.selected.into_iter().map(|k| k.image_id).collect())
            .unwrap_or_default();

        let mut frames = Vec::with_capacity(selected.len());
        for (order, image) in selected.iter().enumerate() {
            frames.push(state.curate_frame(project, *image, order as u32));
        }
        // Timeline order is what phase 12 already delivers; sorting again on the id keeps the pass
        // deterministic when two frames share a capture second.
        frames.sort_by_key(|f| (f.order, f.image_id.to_db()));

        let photos = u32::try_from(state.catalog().count("photo").unwrap_or(0)).unwrap_or(0);
        let loci = state.curate_skin_bands(project)?;
        let rituals = state.curate_rituals(project)?;
        let close_family = state.curate_close_family(project)?;
        let vectors = state.curate_vectors(project, &selected);

        Ok(Self {
            frames,
            photos,
            coverage,
            loci,
            rituals,
            close_family,
            vectors,
        })
    }

    /// How many frames the gallery holds. What the gate reports.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// True when phase 12 selected nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

impl Field for CurateField {
    fn frames(&self, _project: ProjectId) -> AuraResult<Vec<Frame>> {
        Ok(self.frames.clone())
    }

    fn photo_count(&self, _project: ProjectId) -> AuraResult<u32> {
        Ok(self.photos)
    }

    fn gallery_coverage(&self, _project: ProjectId) -> AuraResult<CoverageReport> {
        Ok(self.coverage.clone())
    }

    fn skin_bands(&self, _project: ProjectId) -> AuraResult<BTreeMap<IdentityId, u8>> {
        Ok(self.loci.clone())
    }

    fn similarity(&self, from: ImageId, others: &[ImageId]) -> Vec<Option<f32>> {
        let Some(query) = self.vectors.get(&from) else {
            // No vector is a **skipped** term, not a similarity of zero. Phase 24's rule.
            return vec![None; others.len()];
        };
        others
            .iter()
            .map(|other| {
                let candidate = self.vectors.get(other)?;
                // `aura_index::contract::index::cosine_distance` is the function the phase 05 index
                // itself searches with, not a second implementation of it - which is what phase
                // 05's rule is actually about. A cosine distance in `0..2` becomes a similarity in
                // `-1..1`.
                let distance = aura_index::contract::index::cosine_distance(query, candidate);
                Some(1.0 - distance)
            })
            .collect()
    }

    fn rituals(&self, _project: ProjectId) -> AuraResult<Vec<String>> {
        Ok(self.rituals.clone())
    }

    fn close_family(&self, _project: ProjectId) -> AuraResult<(Vec<IdentityId>, u32)> {
        Ok(self.close_family.clone())
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// What the Curate panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn curate_status(state: &AppState, project_id: &str) -> IpcResult<CurateStatusDto> {
    let project = parse_project(project_id)?;
    let outline = service(state).outline(project)?;
    Ok(status_dto(&outline))
}

/// Run the whole curation pass for one project.
///
/// Returns the outline rather than the whole result: a 120-spread album with its coverage report,
/// twenty heroes, two hundred monochrome candidates and three social sets is a large payload to send
/// when the panel that asked is about to render a header. ADR-0060 section 2.
///
/// # Errors
///
/// `AURA-ML-5144` when a stage cannot run, `AURA-ML-5145` when the policy table is refused, and
/// `AURA-DB-3006` when the result cannot be stored.
pub fn curate_project(state: &AppState, input: CurateProjectInput) -> IpcResult<CurateStatusDto> {
    let project = parse_project(&input.project_id)?;
    let outline = run_pass(state, project, input.album_size)?;
    Ok(status_dto(&outline))
}

/// The monochrome candidates, best first.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn curate_bw(state: &AppState, project_id: &str) -> IpcResult<Vec<CurateBwDto>> {
    let project = parse_project(project_id)?;
    Ok(service(state).bw(project)?.iter().map(bw_dto).collect())
}

/// The portfolio, best first.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn curate_heroes(state: &AppState, project_id: &str) -> IpcResult<Vec<CurateHeroDto>> {
    let project = parse_project(project_id)?;
    Ok(service(state)
        .heroes(project)?
        .iter()
        .map(hero_dto)
        .collect())
}

/// The album draft, or `None` when the project has not been curated.
///
/// `None` is not an empty album. A project nobody has curated and a wedding whose gallery had
/// nothing worth an album are different answers.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn curate_album(state: &AppState, project_id: &str) -> IpcResult<Option<CurateAlbumDto>> {
    let project = parse_project(project_id)?;
    Ok(service(state).album(project)?.as_ref().map(album_dto))
}

/// One spread, or `None` when it is unknown.
///
/// # Errors
///
/// `AURA-ML-5143` when the id is malformed. `AURA-DB-3006` when the row cannot be read.
pub fn curate_spread(state: &AppState, spread_id: &str) -> IpcResult<Option<CurateSpreadDto>> {
    let spread = SpreadId::from_db(spread_id).map_err(|_| {
        IpcError::from(aura_core::errors::ml::curate_decision_refused(format!(
            "`{spread_id}` is not a spread id"
        )))
    })?;
    Ok(service(state).spread(spread)?.as_ref().map(spread_dto))
}

/// The three sets a photographer posts from.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn curate_social(state: &AppState, project_id: &str) -> IpcResult<CurateSocialDto> {
    let project = parse_project(project_id)?;
    Ok(social_dto(&service(state).social(project)?))
}

/// The wedding-night teaser, best first.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn curate_teaser(state: &AppState, project_id: &str) -> IpcResult<Vec<CuratePickDto>> {
    let project = parse_project(project_id)?;
    Ok(service(state)
        .teaser(project)?
        .iter()
        .map(teaser_dto)
        .collect())
}

/// Record a photographer's album order, and re-compose the album around it.
///
/// Two steps in one command, and the split is deliberate: `set_order` stores the sequence, and the
/// pass decides which two images share a spread - because a photographer chose an order and the
/// near-duplicate refusal and the tonal ceiling still apply to whatever ends up adjacent.
///
/// # Errors
///
/// `AURA-ML-5143` when the order reorders chapters, names an image the album does not carry, or
/// repeats one. `AURA-ML-5144` when the re-composition fails.
pub fn curate_set_order(state: &AppState, input: CurateOrderInput) -> IpcResult<CurateAlbumDto> {
    let project = parse_project(&input.project_id)?;
    let mut order = Vec::with_capacity(input.order.len());
    for id in &input.order {
        order.push(parse_image(id)?);
    }
    service(state).set_order(project, &order)?;
    // The album's own target is what it was composed with; a reorder is not a resize.
    let target = service(state).album(project)?.map(|plan| plan.target_size);
    run_pass(state, project, target)?;
    let plan = service(state).album(project)?;
    plan.as_ref().map(album_dto).ok_or_else(|| {
        IpcError::from(aura_core::errors::ml::curate_pass_failed(
            "the album disappeared while its order was being recorded",
        ))
    })
}

/// Record what a photographer decided about one pick.
///
/// One pick per call. There is no bulk decide on this surface: phase 27 established that agreeing
/// with forty findings and authorising forty actions are different judgements, and here the
/// equivalent is that accepting twenty heroes one at a time is the photographer *looking at* twenty
/// photographs. ADR-0060 section 4.
///
/// # Errors
///
/// `AURA-ML-5143` when the kind is unknown or the note is too long.
pub fn curate_decide(state: &AppState, input: CurateDecideInput) -> IpcResult<()> {
    let project = parse_project(&input.project_id)?;
    let image = parse_image(&input.image_id)?;
    let kind = PickKind::parse(&input.kind)?;
    service(state).decide(
        project,
        image,
        CurateOverride {
            kind,
            accepted: input.accepted,
            note: input.note,
        },
    )?;
    Ok(())
}

/// One set as a specification another tool can read.
///
/// Text, never a file. Nothing in phase 29 opens one, and phase 30 owns delivery.
///
/// # Errors
///
/// `AURA-ML-5143` when the subject or the format is unknown. `AURA-DB-3006` when the rows cannot be
/// read.
pub fn curate_export(state: &AppState, input: CurateExportInput) -> IpcResult<CurateExportDto> {
    let project = parse_project(&input.project_id)?;
    let subject = ExportSubject::parse(&input.subject)?;
    let format = ExportFormat::parse(&input.format)?;
    Ok(CurateExportDto {
        text: service(state).export(project, subject, format)?,
        extension: format.extension().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn service(state: &AppState) -> Curate {
    Curate::new(Arc::clone(state.catalog()), Arc::clone(state.clock()))
}

fn run_pass(
    state: &AppState,
    project: ProjectId,
    album_size: Option<u32>,
) -> Result<CurationOutline, IpcError> {
    let field = CurateField::new(state, project)?;
    let policy = state.curate_policy()?;
    let store = CurateStore::new(Arc::clone(state.catalog()), Arc::clone(state.clock()));
    let embed_ver = state.curate_embed_ver(project);
    let pass = CuratePass::new(&field, &policy, &store, embed_ver);
    // No cloud answer. Section 7's offline fallback is the deterministic optimiser, which has
    // already run by this point - so an unreachable provider produces exactly the album a reached
    // one would have unless it could improve on it. ADR-0059 section 11.
    Ok(pass.run(project, album_size, None)?)
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_core::errors::ml::curate_decision_refused(format!(
            "`{id}` is not a project id"
        )))
    })
}

fn parse_image(id: &str) -> Result<ImageId, IpcError> {
    ImageId::from_db(id).map_err(|_| {
        IpcError::from(aura_core::errors::ml::curate_decision_refused(format!(
            "`{id}` is not a photograph id"
        )))
    })
}

fn status_dto(outline: &CurationOutline) -> CurateStatusDto {
    CurateStatusDto {
        photos: outline.photos,
        selected: outline.selected,
        curated: outline.curated,
        coverage: outline.coverage(),
        bw_offered: outline.bw_offered,
        bw_accepted: outline.bw_accepted,
        bw_rejected: outline.bw_rejected,
        heroes: outline.heroes,
        chapters_covered: outline.chapters_covered,
        spreads: outline.spreads,
        album_size: outline.album_size,
        rhythm_score: outline.rhythm_score,
        rhythm_measurable: outline.rhythm_measurable,
        pairing_score: outline.pairing_score,
        album_covered: outline.album_covered,
        album_missing: outline.album_missing,
        reorders: outline.reorders,
        slots_unfilled: outline.slots_unfilled,
        cloud_used: outline.cloud_used,
        cloud_moves_applied: outline.cloud_moves_applied,
        cloud_moves_refused: outline.cloud_moves_refused,
        bytes: outline.bytes,
        policy_ver: outline.policy_ver,
        analysis_ver: outline.analysis_ver,
        embed_ver: outline.embed_ver,
        heads_trained: outline.heads_trained,
    }
}

fn reason_dtos(reasons: &[CurateReason]) -> Vec<CurateReasonDto> {
    reasons
        .iter()
        .map(|reason| CurateReasonDto {
            code: reason.code.as_str().to_string(),
            text: reason.text.clone(),
            weight: reason.weight,
            caveat: reason.code.is_caveat(),
        })
        .collect()
}

fn bw_dto(pick: &BwPick) -> CurateBwDto {
    CurateBwDto {
        image_id: pick.image_id.to_db(),
        score: pick.score,
        confidence: pick.confidence,
        mix: pick.mix.bands.to_vec(),
        terms: pick
            .terms
            .labelled()
            .iter()
            .map(|(name, value)| ((*name).to_string(), *value))
            .collect(),
        skin_bands: pick.skin_bands.clone(),
        reasons: reason_dtos(&pick.reasons),
        accepted: pick.accepted,
    }
}

fn hero_dto(hero: &HeroPick) -> CurateHeroDto {
    CurateHeroDto {
        image_id: hero.image_id.to_db(),
        rank: hero.rank,
        score: hero.score,
        confidence: hero.confidence,
        terms: hero
            .terms
            .labelled()
            .iter()
            .map(|(name, value)| ((*name).to_string(), *value))
            .collect(),
        chapter: hero.chapter.as_str().to_string(),
        scale: hero.scale.as_str().to_string(),
        binding: hero.binding.as_str().to_string(),
        binding_text: hero.binding.user_text().to_string(),
        reasons: reason_dtos(&hero.reasons),
        accepted: hero.accepted,
    }
}

fn spread_dto(spread: &Spread) -> CurateSpreadDto {
    CurateSpreadDto {
        spread_id: spread.id.to_db(),
        index: spread.index,
        left: spread.left.map(|i| i.to_db()),
        right: spread.right.map(|i| i.to_db()),
        single: spread.single,
        chapter: spread.chapter.as_str().to_string(),
        pair_score: spread.pair.score,
        tonal_gap: spread.pair.tonal_gap,
        warmth_gap_k: spread.pair.warmth_gap_k,
        facing_score: spread.pair.facing_score,
        facing_known: spread.pair.facing_known,
        similarity: spread.pair.similarity,
        reasons: reason_dtos(&spread.reasons),
    }
}

fn album_dto(plan: &AlbumPlan) -> CurateAlbumDto {
    CurateAlbumDto {
        spreads: plan.spreads.iter().map(spread_dto).collect(),
        chapters: plan
            .chapter_map
            .iter()
            .map(|span| CurateChapterDto {
                chapter: span.chapter.as_str().to_string(),
                first: span.first,
                len: span.len,
                target: span.target,
            })
            .collect(),
        size: plan.size,
        target_size: plan.target_size,
        rhythm_score: plan.rhythm_score,
        rhythm_measurable: plan.rhythm_measurable,
        pairing_score: plan.pairing_score,
        user_ordered: plan.user_ordered,
        coverage: plan
            .coverage
            .must_haves
            .iter()
            .map(|(rule, state)| (rule.as_str().to_string(), state.as_str().to_string()))
            .collect(),
        warnings: plan.coverage.warnings.clone(),
        reasons: reason_dtos(&plan.reasons),
        summary: album_summary(plan),
    }
}

fn pick_dto(pick: &SocialPick, rank: u32) -> CuratePickDto {
    CuratePickDto {
        image_id: pick.image_id.to_db(),
        aspect: pick.aspect.as_str().to_string(),
        slot: pick.slot.as_str().to_string(),
        rank,
        legibility: pick.legibility,
        reasons: reason_dtos(&pick.reasons),
        accepted: pick.accepted,
    }
}

fn teaser_dto(pick: &TeaserPick) -> CuratePickDto {
    CuratePickDto {
        image_id: pick.image_id.to_db(),
        aspect: AspectVariant::Original.as_str().to_string(),
        slot: pick.slot.as_str().to_string(),
        rank: pick.rank,
        legibility: 0.0,
        reasons: reason_dtos(&pick.reasons),
        accepted: pick.accepted,
    }
}

fn social_dto(sets: &SocialSets) -> CurateSocialDto {
    CurateSocialDto {
        grid: sets
            .grid
            .iter()
            .enumerate()
            .map(|(rank, pick)| pick_dto(pick, rank as u32))
            .collect(),
        story: sets
            .story
            .iter()
            .enumerate()
            .map(|(rank, pick)| pick_dto(pick, rank as u32))
            .collect(),
        hero: sets.hero.as_ref().map(|pick| pick_dto(pick, 0)),
        captions: sets
            .captions
            .iter()
            .map(|caption| CurateCaptionDto {
                image_id: caption.image_id.map(|i| i.to_db()),
                chapter: caption.chapter.as_str().to_string(),
                text: caption.text.clone(),
                source: caption.source.as_str().to_string(),
            })
            .collect(),
        unfilled: sets
            .unfilled_slots()
            .iter()
            .map(|(slot, short)| (slot.as_str().to_string(), *short))
            .collect(),
    }
}

/// Assemble one frame's readings out of the frozen services.
///
/// Every `None` here is a **skipped** term rather than a zero, and on this build most of them are
/// `None`. Exposed on [`AppState`] rather than inlined so that the phase gate can build a field over
/// a real catalog without going through the IPC surface.
#[allow(clippy::too_many_lines)]
pub(crate) fn frame_from_services(
    state: &AppState,
    project: ProjectId,
    image: ImageId,
    order: u32,
) -> Frame {
    use aura_core::contract::composition::CompositionService as _;
    use aura_core::contract::emotion::EmotionService as _;
    use aura_core::contract::gallery::GalleryService as _;
    use aura_core::contract::geometry::GeometryService as _;
    use aura_core::contract::integrity::IntegrityService as _;
    use aura_core::contract::moment::MomentService as _;
    use aura_core::contract::people::PeopleService as _;
    use aura_core::contract::scene::StoryService as _;
    use aura_core::contract::tone::ToneService as _;

    let mut frame = Frame::bare(image, order);

    if let Ok(story) = state.story() {
        if let Ok(Some(scene)) = story.scene(image) {
            frame.scene = Some(scene.scene);
        }
        if let Ok(Some(segment)) = story.segment_of(image) {
            frame.chapter = Some(segment.chapter);
        }
    }
    if let Ok(moments) = state.moments() {
        if let Ok(Some(moment)) = moments.moment_of(image) {
            frame.moment = Some(moment.id);
        }
    }
    if let Ok(cull) = state.cull() {
        if let Ok(Some(aura_core::contract::cull::Decision::Keep(keep))) = cull.of_image(image) {
            frame.keep_score = keep.keep_score;
            // Phase 12 already decided which guarantee protects a frame it forced in. Read rather
            // than re-derived: there is no second coverage rule table in this product.
            if let Some(role) = keep.coverage_role {
                if !frame.satisfies.contains(&role) {
                    frame.satisfies.push(role);
                }
            }
        }
    }
    // Which guarantees this frame can satisfy, from phase 12's own vocabulary. Read off the scene
    // rather than decided again: there is no second rule table in this product.
    if let Some(scene) = frame.scene {
        frame.satisfies = must_haves_of(scene);
    }

    let integrity = state.integrity();
    if let Ok(Some(result)) = integrity.of_image(image) {
        frame.technical = Some(result.technical_score);
        frame.noise_sigma_rel = Some(result.noise_sigma_rel);
        frame.subject_sharpness = Some(result.subject_sharpness);
    }
    if let Ok(emotion) = state.emotion() {
        if let Ok(Some(reading)) = emotion.of_image(image) {
            frame.emotion = Some(reading.emotion_score);
            frame.narrative = Some(reading.narrative_weight);
            frame.interaction = reading.interactions.first().map(|(_, strength)| *strength);
        }
    }
    if let Ok(Some(composition)) = state.composition().of_image(image) {
        frame.composition = Some(composition.composition_score);
        frame.negative_space = Some(composition.negative_space);
        frame.clutter = Some(composition.clutter);
    }
    if let Ok(subjects) = state.people().subjects(image) {
        frame.identities = subjects
            .faces
            .iter()
            .filter_map(|f| f.identity_id)
            .collect();
        frame.identities.sort_by_key(IdentityId::to_db);
        frame.identities.dedup();
        frame.faces = subjects
            .faces
            .iter()
            .map(|face| {
                let bbox = face.bbox;
                FaceRead {
                    identity: face.identity_id,
                    area_frac: face.area_frac,
                    centre_x: bbox.x + bbox.w / 2.0,
                    width: bbox.w,
                    // Phase 06 stores `[[0,0],[0,0]]` for "no landmarks", and a caller that read
                    // that as a midpoint would be measuring the frame's top-left corner.
                    eye_mid_x: face
                        .has_eyes()
                        .then(|| f32::midpoint(face.eyes[0][0], face.eyes[1][0])),
                }
            })
            .collect();
    }

    // The warmth a frame will actually be delivered at: phase 15's estimate plus phase 25's
    // normalisation delta. One number rather than two, because a spread cares what the frame will
    // look like and not how it got there.
    if let Ok(Some(estimate)) = state.tone().of_image(image) {
        let delta = state
            .gallery()
            .delta(image)
            .ok()
            .flatten()
            .map_or(0.0, |d| d.d_cct);
        frame.warmth_k = Some(estimate.temperature_k + delta);
    }

    // Only aspects `GeometryService` called safe. A social set never asks for one that is not here.
    if let Ok(geometry) = state.geometry() {
        if let Ok(Some(plan)) = geometry.of_image(image) {
            for crop in &plan.crops {
                if crop.safe && !frame.aspects.contains(&crop.aspect) {
                    frame.aspects.push(crop.aspect);
                }
            }
        }
    }

    if let Some(descriptor) = state.curate_descriptor(project, image) {
        frame.descriptor = Some(descriptor);
    }
    frame
}

/// Which coverage guarantees a scene can satisfy.
///
/// Phase 12's own mapping, read here rather than re-derived: `MustHave` is its vocabulary and its
/// rule table decides which frames a rule can be satisfied by. A second table would be a second
/// answer to "is the ring exchange in the gallery".
fn must_haves_of(scene: aura_core::contract::scene::SceneId) -> Vec<MustHave> {
    use aura_core::contract::scene::SceneId;
    match scene {
        SceneId::FirstLook => vec![MustHave::FirstLook],
        SceneId::CeremonyEntrance => vec![MustHave::CeremonyEntrance],
        SceneId::Vows => vec![MustHave::Vows],
        SceneId::Rings => vec![MustHave::Rings],
        SceneId::Kiss => vec![MustHave::Kiss],
        SceneId::FamilyPortrait => vec![MustHave::FamilyFormals],
        SceneId::ReceptionEntrance => vec![MustHave::ReceptionEntrance],
        SceneId::Cake => vec![MustHave::Cake],
        SceneId::FirstDance => vec![MustHave::FirstDance],
        SceneId::Venue => vec![MustHave::VenueEstablishing],
        SceneId::Exit => vec![MustHave::Exit],
        _ => Vec::new(),
    }
}

/// Phase 05's stored descriptors, as this phase reads them.
pub(crate) fn descriptor_from(stored: &aura_index::store::StoredEmbedding) -> Descriptor {
    Descriptor {
        hsv_hist: stored.descriptors.hsv_hist.to_vec(),
        luma: stored.descriptors.luma,
        edge_energy: stored.descriptors.edge_energy,
    }
}
