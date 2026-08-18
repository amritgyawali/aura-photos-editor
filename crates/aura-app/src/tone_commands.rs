//! The tone command surface. PHASE-15.
//!
//! Seven commands. Four read - the project's coverage, one photograph's decision, the
//! low-confidence review queue and a chapter's anchors - one runs the resumable pass, and two
//! record what the photographer decided about a frame the pass had already decided.
//!
//! # The one thing this module does that no other command module does
//!
//! **It writes a recipe on behalf of an automated pass.** `aura-brain-photo` deliberately does
//! not depend on `aura-recipe`: phase 14's rule is that `aura_recipe::schema::merge` is the
//! only function in the workspace that writes one recipe into another, and that it is where
//! `user_edited_fields` is honoured. So the tone pass produces estimates, and this module -
//! which can see both crates - carries the three numbers across, through the merge, with
//! `EditSource::Ai`. Every refusal the merge makes is counted into
//! `TonePassDto::recipes_protected` rather than swallowed, because a pass that quietly did
//! nothing to four hundred frames is the failure mode section 12's fourth row names.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command returns a skin locus.** `ToneService::skin_locus` and `skin_loci` exist and are
//! used by the pass; neither is on the wire. A locus is a per-person measurement of what
//! somebody's skin looks like, accumulated across a wedding, and ADR-0032 section 4 has the
//! argument for why it stays behind the service. What the panel gets instead is a *count* -
//! `constrainedIdentities` per frame and `skinConstrained` over the project - which is what a
//! photographer needs to know and carries nothing about any individual.
//!
//! **No command grades.** There is no curve, contrast, saturation or HSL anywhere on this
//! surface. Section 2.2 puts those in phase 16, and the boundary is structural: the only
//! recipe paths this module can write are `global.exposure`, `global.temperature` and
//! `global.tint`, and they are written out one by one rather than by copying a document.
//!
//! **No command normalises a gallery.** `reference_frames` reads anchors. Nothing here changes
//! one frame to match another; that is phase 25.

use aura_core::contract::tone::{
    Illuminant, IlluminantKind, ReferenceFrame, ToneAlternative, ToneEstimate, ToneOverride,
    ToneReason, ToneService,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId, SegmentId};
use aura_recipe::{schema, EditSource, Recipe};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptToneInput, CropRectDto, EstimateToneInput, IlluminantDto, IpcError, ReferenceFrameDto,
    ReferenceFramesInput, SetToneOverrideDto, SetToneOverrideInput, ToneAlternativeDto, ToneDto,
    TonePassDto, ToneReasonDto, ToneReviewInput, ToneStatusDto,
};
use crate::develop_commands::{load_or_neutral, recipe_dto};
use crate::state::AppState;

/// What the Basic panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored estimates cannot be read.
pub fn tone_status(state: &AppState, project_id: &str) -> IpcResult<ToneStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.tone().outline(project)?;
    Ok(ToneStatusDto {
        photos: outline.photos,
        estimated: outline.estimated,
        coverage: outline.coverage,
        face_anchored: outline.face_anchored,
        skin_constrained: outline.skin_constrained,
        mixed_light: outline.mixed_light,
        coloured_light: outline.coloured_light,
        needs_review: outline.needs_review,
        user_edited: outline.user_edited,
        mean_ev: outline.mean_ev,
        mean_cct: outline.mean_cct,
        illuminant_counts: outline.illuminant_histogram.to_vec(),
        illuminant_names: IlluminantKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        segments_anchored: outline.segments_anchored,
        segments: outline.segments,
        loci: outline.loci,
        untargeted_scenes: outline.untargeted_scenes,
        model_ver: u32::from(outline.model_ver),
        analysis_ver: u32::from(outline.analysis_ver),
        targets_ver: u32::from(outline.targets_ver),
    })
}

/// One photograph's exposure and white-balance decision.
///
/// `None` means nobody has estimated the frame. That is not the same as a neutral decision and
/// the panel renders it separately: an unestimated frame is developed as the camera recorded
/// it, and saying so is what stops a gap in coverage from looking like a decision.
///
/// # Errors
///
/// `AURA-DB-3006` when the estimate cannot be read.
pub fn image_tone(state: &AppState, photo_id: &str) -> IpcResult<Option<ToneDto>> {
    let image = parse_photo(photo_id)?;
    let Some(estimate) = state.tone().of_image(image)? else {
        return Ok(None);
    };
    let own = state.tone_store().override_of(image)?;
    Ok(Some(dto(&estimate, own)))
}

/// The frames whose white balance is worth a photographer's attention, weakest first.
///
/// Section 9's MFE deliverable. A queue, not a cull: nothing here rejects a photograph and
/// there is no field on this surface that could.
///
/// # Errors
///
/// `AURA-DB-3006` when the queue cannot be read.
pub fn tone_review_queue(state: &AppState, input: &ToneReviewInput) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .tone()
        .needs_review(project, limit)?
        .into_iter()
        .map(|id| id.to_db())
        .collect())
}

/// One chapter's anchors, best first.
///
/// What phase 25 will normalise toward. An empty list is a real answer rather than a failure:
/// a chapter shot under three different lights has no representative frame, and normalising it
/// toward one that is not representative is worse than leaving it alone.
///
/// # Errors
///
/// `AURA-DB-3006` when the anchors cannot be read.
pub fn reference_frames(
    state: &AppState,
    input: &ReferenceFramesInput,
) -> IpcResult<Vec<ReferenceFrameDto>> {
    let segment = parse_segment(&input.segment_id)?;
    Ok(state
        .tone()
        .reference_frames(segment)?
        .iter()
        .map(reference_dto)
        .collect())
}

/// Record that the photographer has looked at one estimate and agrees.
///
/// It does not set `userEdited`. Accepting a suggestion is not authoring one, and phase 30's
/// learning loop needs to be able to tell those apart.
///
/// # Errors
///
/// `AURA-ML-5061` when the photograph has no estimate.
pub fn accept_tone(state: &AppState, input: &AcceptToneInput) -> IpcResult<ToneDto> {
    let image = parse_photo(&input.photo_id)?;
    let tone = state.tone();
    tone.accept(image)?;
    let own = state.tone_store().override_of(image)?;
    tone.of_image(image)?
        .as_ref()
        .map(|estimate| dto(estimate, own))
        .ok_or_else(|| {
            IpcError::from(aura_brain_photo::errors::tone_edit_refused(
                "estimate",
                "the estimate disappeared between the write and the read",
            ))
        })
}

/// Record what the photographer set instead, and write it into the recipe.
///
/// **Two writes, in this order, and the order is the point.** The estimate row is written
/// first, so that a crash between the two leaves a catalog that says the photographer
/// disagreed and a recipe that has not caught up - which the next pass reconciles - rather than
/// a recipe nobody can explain. Then the same values go through
/// `aura_recipe::schema::merge` with `EditSource::User`, which is the only function permitted
/// to add to `user_edited_fields` and therefore the only way those three paths become
/// protected from every later automated pass.
///
/// # Errors
///
/// `AURA-ML-5061` when the photograph has no estimate, the override is empty, or a value is
/// outside its documented range; `AURA-RENDER-8002` when the merged recipe will not validate.
pub fn set_tone_override(
    state: &AppState,
    input: &SetToneOverrideInput,
) -> IpcResult<SetToneOverrideDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let values = ToneOverride {
        exposure_ev: input.exposure_ev,
        temperature_k: input.temperature_k,
        tint: input.tint,
    };

    let tone = state.tone();
    tone.set_override(photo, values)?;

    let base = load_or_neutral(state, photo)?;
    let proposal = with_tone(&base, values);
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;
    state.recipe_store().save(
        &project,
        &photo,
        &merged,
        &report.changed,
        "Exposure and colour",
    )?;

    let own = state.tone_store().override_of(photo)?;
    let estimate = tone
        .of_image(photo)?
        .as_ref()
        .map(|estimate| dto(estimate, own))
        .ok_or_else(|| {
            IpcError::from(aura_brain_photo::errors::tone_edit_refused(
                "estimate",
                "the estimate disappeared between the write and the read",
            ))
        })?;

    Ok(SetToneOverrideDto {
        estimate,
        recipe: recipe_dto(&input.photo_id, &merged),
        changed: report.changed.clone(),
        protected: merged.provenance.user_edited_fields.clone(),
    })
}

/// Run the resumable exposure and white-balance pass, then write what it decided into the
/// recipes.
///
/// # Errors
///
/// `AURA-ML-5063` when the target table will not load, or whatever building the preview
/// service and the inference engine raised. Per-photograph failures are counted rather than
/// returned: one unreadable frame must not end a 4,000-image pass.
pub fn estimate_tone(state: &AppState, input: &EstimateToneInput) -> IpcResult<TonePassDto> {
    let project = parse_project(&input.project_id)?;
    let pass = state.tone_pass(&input.project_id)?;
    let cancel = CancelToken::new();
    if let Some(id) = input.cancel_id.as_deref() {
        state.register_job(id, cancel.clone());
    }
    let report = pass.run(&project, Priority::AiBatch, &cancel, &NullProgress);
    if let Some(id) = input.cancel_id.as_deref() {
        state.finish_job(id);
    }
    let report = report?;

    let (written, protected) = write_recipes(state, &project, &cancel)?;

    Ok(TonePassDto {
        estimated: saturating_u32(report.estimated),
        failed: saturating_u32(report.failed),
        face_anchored: saturating_u32(report.face_anchored),
        mixed_light: saturating_u32(report.mixed_light),
        coloured_light: saturating_u32(report.coloured_light),
        low_confidence: saturating_u32(report.low_confidence),
        loci: saturating_u32(report.loci),
        segments_anchored: saturating_u32(report.segments_anchored),
        mean_ev: report.mean_ev(),
        mean_cct: report.mean_cct(),
        untargeted_scenes: report.untargeted.clone(),
        recipes_written: written,
        recipes_protected: protected,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- carrying the decision into the recipe ----------------------------------

/// Write every current estimate into its photograph's recipe, as the product.
///
/// Returns how many recipes moved and how many paths the merge refused because a person had
/// already set them. A refusal is not an error and is not retried: it is the protection working,
/// and phase 14's `AURA-RENDER-8006` already exists to make one visible when a caller wants to
/// tell somebody about it.
fn write_recipes(
    state: &AppState,
    project: &ProjectId,
    cancel: &CancelToken,
) -> Result<(u32, u32), IpcError> {
    let tone = state.tone();
    let store = state.recipe_store();
    let mut written = 0_u32;
    let mut protected = 0_u32;

    for photo in state.tone_store().all_photos(project)? {
        if cancel.is_cancelled() {
            break;
        }
        let Some(estimate) = tone.of_image(photo)? else {
            continue;
        };
        // A frame the photographer has taken over keeps their numbers. The merge would refuse
        // the paths anyway; skipping the whole row saves a load, a serialise and a save per
        // frame on a wedding somebody has already worked through.
        if estimate.user_edited {
            continue;
        }
        let base = load_or_neutral(state, photo)?;
        let proposal = proposed(&base, &estimate);
        let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
        protected = protected.saturating_add(saturating_u32(report.refused.len()));
        if report.changed.is_empty() {
            continue;
        }
        let merged = merged.clamped();
        schema::Validation::check(&merged)?;
        store.save(
            project,
            &photo,
            &merged,
            &report.changed,
            "Exposure and colour",
        )?;
        written = written.saturating_add(1);
    }
    Ok((written, protected))
}

/// The base recipe with this estimate's three numbers and its provenance.
///
/// The scene, the confidence and the source travel with the values because invariant 2 is about
/// the *edit*, not only about the row that decided it: a recipe exported to a sidecar and read
/// back by phase 27 has to be able to say how sure the product was.
fn proposed(base: &Recipe, estimate: &ToneEstimate) -> Recipe {
    let mut proposal = with_tone(
        base,
        ToneOverride {
            exposure_ev: Some(estimate.exposure_ev),
            temperature_k: Some(estimate.temperature_k),
            tint: Some(estimate.tint),
        },
    );
    proposal.provenance.scene = Some(estimate.scene.as_str().to_string());
    proposal.provenance.confidence = estimate.confidence();
    proposal.provenance.source = EditSource::Ai;
    proposal
}

/// The base recipe with whichever of the three values were set.
///
/// The only function in this module that touches a recipe's parameters, and it can reach
/// exactly three of them. `temperature` and `tint` are integers in the recipe, which is why
/// `store::codec` stores them as integers too: a fractional kelvin is precision the renderer
/// rounds away.
fn with_tone(base: &Recipe, values: ToneOverride) -> Recipe {
    let mut out = base.clone();
    if let Some(exposure) = values.exposure_ev {
        out.global.exposure = exposure;
    }
    if let Some(kelvin) = values.temperature_k {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            out.global.temperature = kelvin.round().clamp(2_000.0, 50_000.0) as u32;
        }
    }
    if let Some(tint) = values.tint {
        #[allow(clippy::cast_possible_truncation)]
        {
            out.global.tint = tint.round().clamp(-150.0, 150.0) as i16;
        }
    }
    out
}

// -- wire shapes ------------------------------------------------------------

/// The wire shape, carrying **both** answers.
///
/// `own` is what the photographer set, read from the three columns `set_override` fills. The
/// estimate's own three numbers are never replaced by it: a panel that showed only the
/// photographer's values could not say what AURA thought, and the review queue exists to show
/// exactly that difference.
fn dto(estimate: &ToneEstimate, own: Option<ToneOverride>) -> ToneDto {
    let own = own.unwrap_or_default();
    ToneDto {
        photo_id: estimate.image_id.to_db(),
        exposure_ev: estimate.exposure_ev,
        exposure_conf: estimate.exposure_conf,
        temperature_k: estimate.temperature_k,
        tint: estimate.tint,
        wb_conf: estimate.wb_conf,
        confidence: estimate.confidence(),
        illuminants: estimate.illuminants.iter().map(illuminant_dto).collect(),
        mixed_light: estimate.mixed_light,
        dominant_on_subject: estimate.dominant_on_subject.map(saturating_u32),
        subject_luma_before: estimate.subject_luma_before,
        subject_luma_target: estimate.subject_luma_target,
        skin_de00_estimate: estimate.skin_de00_estimate,
        alternatives: estimate.alternatives.iter().map(alternative_dto).collect(),
        reasons: estimate.reasons.iter().map(reason_dto).collect(),
        scene: estimate.scene.as_str().to_string(),
        face_anchored: estimate.face_anchored,
        backlit: estimate.backlit,
        coloured_light: estimate.coloured_light,
        clipping_added: estimate.clipping_added,
        constrained_identities: estimate.constrained_identities,
        user_edited: estimate.user_edited,
        user_exposure_ev: own.exposure_ev,
        user_temperature_k: own.temperature_k,
        user_tint: own.tint,
        reviewed: estimate.reviewed,
        needs_review: estimate.needs_review(),
        model_ver: u32::from(estimate.model_ver),
        analysis_ver: u32::from(estimate.analysis_ver),
        targets_ver: u32::from(estimate.targets_ver),
    }
}

fn illuminant_dto(light: &Illuminant) -> IlluminantDto {
    IlluminantDto {
        kind: light.kind.as_str().to_string(),
        cct_k: light.cct_k,
        tint: light.tint,
        weight: light.weight,
        chroma: light.chroma,
        source: light.source.as_str().to_string(),
        region: light.region.map(|rect| CropRectDto {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }),
    }
}

fn alternative_dto(alternative: &ToneAlternative) -> ToneAlternativeDto {
    ToneAlternativeDto {
        exposure_ev: alternative.exposure_ev,
        temperature_k: alternative.temperature_k,
        tint: alternative.tint,
        cost: alternative.cost,
        why: alternative.why.as_str().to_string(),
    }
}

fn reason_dto(reason: &ToneReason) -> ToneReasonDto {
    ToneReasonDto {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        evidence: reason.evidence.map(|rect| CropRectDto {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }),
    }
}

fn reference_dto(frame: &ReferenceFrame) -> ReferenceFrameDto {
    ReferenceFrameDto {
        photo_id: frame.image_id.to_db(),
        segment_id: frame.segment.to_db(),
        rank: u32::from(frame.rank),
        wb_conf: frame.wb_conf,
        temperature_k: frame.temperature_k,
        tint: frame.tint,
        subject_luma: frame.subject_luma,
        quality: frame.quality,
    }
}

// -- parsing ----------------------------------------------------------------

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| bad_id("project", id))
}

fn parse_photo(id: &str) -> Result<PhotoId, IpcError> {
    PhotoId::from_db(id).map_err(|_| bad_id("photo", id))
}

fn parse_segment(id: &str) -> Result<SegmentId, IpcError> {
    SegmentId::from_db(id).map_err(|_| bad_id("segment", id))
}

fn bad_id(kind: &str, id: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("not a {kind} id: {id}"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::fixtures as recipe_fixtures;

    fn neutral() -> Recipe {
        recipe_fixtures::neutral(&"a".repeat(64), "Test Camera")
    }

    #[test]
    fn an_override_touches_only_the_values_it_names() {
        let base = neutral();
        let out = with_tone(
            &base,
            ToneOverride {
                exposure_ev: None,
                temperature_k: Some(3_200.4),
                tint: None,
            },
        );
        assert_eq!(out.global.temperature, 3_200);
        assert!((out.global.exposure - base.global.exposure).abs() < f32::EPSILON);
        assert_eq!(out.global.tint, base.global.tint);
    }

    #[test]
    fn an_override_cannot_leave_the_recipes_own_range() {
        let out = with_tone(
            &neutral(),
            ToneOverride {
                exposure_ev: None,
                temperature_k: Some(90_000.0),
                tint: Some(-900.0),
            },
        );
        assert_eq!(out.global.temperature, 50_000);
        assert_eq!(out.global.tint, -150);
    }

    #[test]
    fn a_persons_value_survives_an_automated_pass() {
        // The property this whole module exists to keep. A person sets the temperature; the
        // pass then proposes its own, and the merge refuses that one path and takes the rest.
        let base = neutral();
        let mine = with_tone(
            &base,
            ToneOverride {
                exposure_ev: None,
                temperature_k: Some(3_000.0),
                tint: None,
            },
        );
        let (base, _) = schema::merge(&base, &mine, EditSource::User).expect("user merge");
        assert!(base
            .provenance
            .user_edited_fields
            .contains(&"global.temperature".to_string()));

        let theirs = with_tone(
            &base,
            ToneOverride {
                exposure_ev: Some(0.75),
                temperature_k: Some(5_500.0),
                tint: None,
            },
        );
        let (merged, report) = schema::merge(&base, &theirs, EditSource::Ai).expect("ai merge");
        assert_eq!(
            merged.global.temperature, 3_000,
            "the person's kelvin stood"
        );
        assert!(
            (merged.global.exposure - 0.75).abs() < 1e-6,
            "the rest applied"
        );
        assert!(report.refused.contains(&"global.temperature".to_string()));
    }
}
