//! The colour command surface. PHASE-16.
//!
//! Seven commands, matching phase 15's shape: three read - the project's coverage, one
//! photograph's grade and the low-confidence review queue - one runs the resumable pass, and
//! three record what the photographer decided about a frame the pass had already decided.
//!
//! # The thing this module does that `tone_commands` also does, and why it is here twice
//!
//! **It writes a recipe on behalf of an automated pass.** `aura-brain-photo` does not write
//! recipes - phase 14's rule is that `aura_recipe::schema::merge` is the only function in the
//! workspace that writes one into another, and that it is where `user_edited_fields` is
//! honoured. So the colour pass produces decisions, and this module - which can see both
//! crates - carries **nine** values across, through the merge, with `EditSource::Ai`. Every
//! refusal the merge makes is counted into `ColourPassDto::recipes_protected` rather than
//! swallowed.
//!
//! Nine rather than phase 15's three, and the two sets do not overlap: phase 15 owns
//! `global.exposure`, `global.temperature` and `global.tint`, and phase 16 owns the five tone
//! parameters, the vibrance, the saturation, the curve and the HSL block. Nothing here can
//! reach a white balance, which is the boundary section 2.2 draws and the reason the "warmer"
//! alternative is a band shift rather than a temperature.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command returns a mask.** The content bands are *inferred* from colour statistics and
//! what reaches the panel is a reading - an area, a hue, a saturation and a confidence -
//! rather than a region. Masks are phase 18's and inventing one here would be inventing the
//! evidence for an adjustment that was made without it.
//!
//! **No command normalises a gallery.** Every value here is about one photograph. Phase 25
//! owns making a chapter agree with itself.
//!
//! **No command sets a skin target.** There is nothing on this surface that could: the
//! guarantee is a measurement of movement, and `SkinGuardDto` reports what happened rather
//! than what was wanted.

use aura_core::contract::colour::{
    BandReading, ColourDecision, ColourOverride, ColourReason, ColourService, ColourVariant,
    CurvePoint, HslAdjustments, HslBand, HslShift, SkinGuardReport, ToneCurve, VariantKind,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId};
use aura_recipe::{schema, EditSource, HslShift as RecipeHslShift, Recipe};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AcceptColourInput, BandReadingDto, ColourDto, ColourPassDto, ColourReasonDto,
    ColourReviewInput, ColourStatusDto, ColourVariantDto, CropRectDto, CurvePointDto,
    EstimateColourInput, HslShiftDto, IpcError, SelectVariantInput, SetColourOverrideDto,
    SetColourOverrideInput, SkinGuardDto,
};
use crate::develop_commands::{load_or_neutral, recipe_dto};
use crate::state::AppState;

/// What the Develop panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored decisions cannot be read.
pub fn colour_status(state: &AppState, project_id: &str) -> IpcResult<ColourStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.colour().outline(project)?;
    Ok(ColourStatusDto {
        photos: outline.photos,
        decided: outline.decided,
        coverage: outline.coverage,
        skin_measured: outline.skin_measured,
        skin_guard_triggered: outline.skin_guard_triggered,
        clip_guard_resolved: outline.clip_guard_resolved,
        subtlety_capped: outline.subtlety_capped,
        needs_review: outline.needs_review,
        user_edited: outline.user_edited,
        mean_contrast: outline.mean_contrast,
        mean_shadow_lift: outline.mean_shadow_lift,
        mean_subtlety: outline.mean_subtlety,
        worst_skin_hue_shift: outline.worst_skin_hue_shift,
        guarantee_held: outline.guarantee_held(),
        untargeted_scenes: outline.untargeted_scenes.clone(),
        model_ver: u32::from(outline.model_ver),
        analysis_ver: u32::from(outline.analysis_ver),
        intent_ver: u32::from(outline.intent_ver),
    })
}

/// One photograph's tone and colour decision.
///
/// `None` means nobody has graded the frame. That is not the same as a neutral grade and the
/// panel renders it separately: an ungraded frame is developed as phase 15 left it, and saying
/// so is what stops a gap in coverage from looking like a decision.
///
/// # Errors
///
/// `AURA-DB-3006` when the decision cannot be read.
pub fn image_colour(state: &AppState, photo_id: &str) -> IpcResult<Option<ColourDto>> {
    let image = parse_photo(photo_id)?;
    Ok(state.colour().of_image(image)?.as_ref().map(dto))
}

/// The frames whose grade is worth a photographer's attention, weakest first.
///
/// A queue, not a cull: nothing here rejects a photograph and there is no field on this
/// surface that could.
///
/// # Errors
///
/// `AURA-DB-3006` when the queue cannot be read.
pub fn colour_review_queue(state: &AppState, input: &ColourReviewInput) -> IpcResult<Vec<String>> {
    let project = parse_project(&input.project_id)?;
    let limit = input.limit.unwrap_or(200).clamp(1, 5_000) as usize;
    Ok(state
        .colour()
        .needs_review(project, limit)?
        .into_iter()
        .map(|id| id.to_db())
        .collect())
}

/// Record that the photographer has looked at one grade and agrees.
///
/// It does not set `userEdited`. Accepting a suggestion is not authoring one.
///
/// # Errors
///
/// `AURA-ML-5067` when the photograph has no decision.
pub fn accept_colour(state: &AppState, input: &AcceptColourInput) -> IpcResult<ColourDto> {
    let image = parse_photo(&input.photo_id)?;
    let colour = state.colour();
    colour.accept(image)?;
    colour
        .of_image(image)?
        .as_ref()
        .map(dto)
        .ok_or_else(|| missing("the decision disappeared between the write and the read"))
}

/// Record what the photographer set instead, and write it into the recipe.
///
/// **Two writes, in this order, and the order is the point.** The decision row is written
/// first, so that a crash between the two leaves a catalog that says the photographer
/// disagreed and a recipe that has not caught up - which the next pass reconciles - rather
/// than a recipe nobody can explain.
///
/// # Errors
///
/// `AURA-ML-5067` when the photograph has no decision, the override is empty, or a supplied
/// curve is not monotone; `AURA-RENDER-8002` when the merged recipe will not validate.
pub fn set_colour_override(
    state: &AppState,
    input: &SetColourOverrideInput,
) -> IpcResult<SetColourOverrideDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let values = override_of(input)?;

    let colour = state.colour();
    colour.set_override(photo, values.clone())?;

    let base = load_or_neutral(state, photo)?;
    let proposal = with_colour(&base, &values);
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;
    state.recipe_store().save(
        &project,
        &photo,
        &merged,
        &report.changed,
        "Tone and colour",
    )?;

    let decision = colour
        .of_image(photo)?
        .as_ref()
        .map(dto)
        .ok_or_else(|| missing("the decision disappeared between the write and the read"))?;

    Ok(SetColourOverrideDto {
        decision,
        recipe: recipe_dto(&input.photo_id, &merged),
        changed: report.changed.clone(),
        protected: merged.provenance.user_edited_fields.clone(),
    })
}

/// Promote one stored alternative to the primary grade.
///
/// Section 6.4's "switch instantly without recomputation". It records as the photographer's
/// own, because it is: they looked at up to three answers and chose one.
///
/// # Errors
///
/// `AURA-ML-5067` when the photograph has no decision or no variant of that kind.
pub fn select_colour_variant(
    state: &AppState,
    input: &SelectVariantInput,
) -> IpcResult<SetColourOverrideDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let Some(kind) = VariantKind::parse(&input.kind) else {
        return Err(IpcError::from(
            aura_brain_photo::errors::colour_edit_refused(
                "variant",
                "must be one of flatter, punchier or warmer",
            ),
        ));
    };

    let colour = state.colour();
    colour.select_variant(photo, kind)?;

    // The promoted set becomes the recipe by the same route an override does, so a variant and
    // a hand-set value are protected identically. There is no second path.
    let Some(decision) = colour.of_image(photo)? else {
        return Err(missing("the decision disappeared after the promotion"));
    };
    let values = state.colour_store().override_of(photo)?.unwrap_or_default();
    let base = load_or_neutral(state, photo)?;
    let proposal = with_colour(&base, &values);
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;
    state.recipe_store().save(
        &project,
        &photo,
        &merged,
        &report.changed,
        "Tone and colour",
    )?;

    Ok(SetColourOverrideDto {
        decision: dto(&decision),
        recipe: recipe_dto(&input.photo_id, &merged),
        changed: report.changed.clone(),
        protected: merged.provenance.user_edited_fields.clone(),
    })
}

/// Run the resumable grading pass, then write what it decided into the recipes.
///
/// # Errors
///
/// `AURA-ML-5070` when the intent table will not load, or whatever building the preview
/// service and the inference engine raised. Per-photograph failures are counted rather than
/// returned: one unreadable frame must not end a 4,000-image pass.
pub fn estimate_colour(state: &AppState, input: &EstimateColourInput) -> IpcResult<ColourPassDto> {
    let project = parse_project(&input.project_id)?;
    let pass = state.colour_pass(&input.project_id)?;
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

    Ok(ColourPassDto {
        decided: saturating_u32(report.decided),
        failed: saturating_u32(report.failed),
        skin_measured: saturating_u32(report.skin_measured),
        skin_guard_triggered: saturating_u32(report.skin_guard_triggered),
        skin_guard_withdrew: saturating_u32(report.skin_guard_withdrew),
        clip_guard_resolved: saturating_u32(report.clip_guard_resolved),
        subtlety_capped: saturating_u32(report.subtlety_capped),
        low_confidence: saturating_u32(report.low_confidence),
        mean_contrast: report.mean_contrast(),
        mean_shadow_lift: report.mean_shadow_lift(),
        mean_subtlety: report.mean_subtlety(),
        worst_skin_hue_shift: report.worst_skin_hue_shift,
        untargeted_scenes: report.untargeted.clone(),
        recipes_written: written,
        recipes_protected: protected,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

// -- carrying the decision into the recipe ----------------------------------

/// Write every current decision into its photograph's recipe, as the product.
///
/// Returns how many recipes moved and how many paths the merge refused because a person had
/// already set them. A refusal is not an error and is not retried: it is the protection
/// working.
fn write_recipes(
    state: &AppState,
    project: &ProjectId,
    cancel: &CancelToken,
) -> Result<(u32, u32), IpcError> {
    let colour = state.colour();
    let store = state.recipe_store();
    let mut written = 0_u32;
    let mut protected = 0_u32;

    for photo in state.colour_store().all_photos(project)? {
        if cancel.is_cancelled() {
            break;
        }
        let Some(decision) = colour.of_image(photo)? else {
            continue;
        };
        // A frame the photographer has taken over keeps their numbers. The merge would refuse
        // the paths anyway; skipping the row saves a load, a serialise and a save per frame on
        // a wedding somebody has already worked through.
        if decision.user_edited {
            continue;
        }
        let base = load_or_neutral(state, photo)?;
        let proposal = proposed(&base, &decision);
        let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
        protected = protected.saturating_add(saturating_u32(report.refused.len()));
        if report.changed.is_empty() {
            continue;
        }
        let merged = merged.clamped();
        schema::Validation::check(&merged)?;
        store.save(project, &photo, &merged, &report.changed, "Tone and colour")?;
        written = written.saturating_add(1);
    }
    Ok((written, protected))
}

/// The base recipe with this decision's nine values and its provenance.
fn proposed(base: &Recipe, decision: &ColourDecision) -> Recipe {
    let mut proposal = with_colour(
        base,
        &ColourOverride {
            contrast: Some(decision.contrast),
            highlights: Some(decision.highlights),
            shadows: Some(decision.shadows),
            whites: Some(decision.whites),
            blacks: Some(decision.blacks),
            vibrance: Some(decision.vibrance),
            saturation: Some(decision.saturation),
            curve: Some(decision.curve.clone()),
            hsl: Some(decision.hsl),
        },
    );
    proposal.provenance.scene = Some(decision.scene.as_str().to_string());
    proposal.provenance.confidence = decision.confidence;
    proposal.provenance.source = EditSource::Ai;
    proposal
}

/// The base recipe with whichever of the nine values were set.
///
/// The only function in this module that touches a recipe's parameters, and it can reach
/// exactly nine of them. The five tone parameters, the vibrance and the saturation are
/// integers in the recipe, which is why they are rounded here: a fractional contrast is
/// precision the renderer discards.
fn with_colour(base: &Recipe, values: &ColourOverride) -> Recipe {
    let mut out = base.clone();
    let unit = |value: f32| -> i16 {
        #[allow(clippy::cast_possible_truncation)]
        {
            value.round().clamp(-100.0, 100.0) as i16
        }
    };
    if let Some(value) = values.contrast {
        out.global.contrast = unit(value);
    }
    if let Some(value) = values.highlights {
        out.global.highlights = unit(value);
    }
    if let Some(value) = values.shadows {
        out.global.shadows = unit(value);
    }
    if let Some(value) = values.whites {
        out.global.whites = unit(value);
    }
    if let Some(value) = values.blacks {
        out.global.blacks = unit(value);
    }
    if let Some(value) = values.vibrance {
        out.global.vibrance = unit(value);
    }
    if let Some(value) = values.saturation {
        out.global.saturation = unit(value);
    }
    if let Some(curve) = &values.curve {
        out.global.curve = aura_recipe::Curve {
            points: curve.recipe_points(),
        };
    }
    if let Some(hsl) = &values.hsl {
        out.global.hsl.clear();
        for band in HslBand::ALL {
            let shift = hsl.get(band);
            if shift.is_neutral() {
                continue;
            }
            out.global.hsl.insert(
                band.as_str().to_string(),
                RecipeHslShift {
                    h: unit(shift.h),
                    s: unit(shift.s),
                    l: unit(shift.l),
                },
            );
        }
    }
    out
}

// -- wire shapes ------------------------------------------------------------

fn dto(decision: &ColourDecision) -> ColourDto {
    ColourDto {
        photo_id: decision.image_id.to_db(),
        contrast: decision.contrast,
        highlights: decision.highlights,
        shadows: decision.shadows,
        whites: decision.whites,
        blacks: decision.blacks,
        vibrance: decision.vibrance,
        saturation: decision.saturation,
        curve: curve_dto(&decision.curve),
        hsl: hsl_dto(&decision.hsl),
        bands: decision.bands.iter().map(band_dto).collect(),
        skin_guard: guard_dto(&decision.skin_guard),
        clipping_before: decision.clipping_before.0,
        clipping_after: decision.clipping_after.0,
        clipping_added: decision.highlight_clipping_added(),
        alternatives: decision.alternatives.iter().map(variant_dto).collect(),
        reasons: decision.reasons.iter().map(reason_dto).collect(),
        confidence: decision.confidence,
        subtlety: decision.subtlety,
        scene: decision.scene.as_str().to_string(),
        skin_measured: decision.skin_measured,
        user_edited: decision.user_edited,
        reviewed: decision.reviewed,
        needs_review: decision.needs_review(),
        model_ver: u32::from(decision.model_ver),
        analysis_ver: u32::from(decision.analysis_ver),
        intent_ver: u32::from(decision.intent_ver),
    }
}

fn curve_dto(curve: &ToneCurve) -> Vec<CurvePointDto> {
    curve
        .points()
        .iter()
        .map(|point| CurvePointDto {
            x: point.x,
            y: point.y,
        })
        .collect()
}

/// The eight bands, **all of them**, in the recipe's order.
///
/// Neutral bands included rather than filtered out, because the HSL panel draws eight sliders
/// and a panel that had to guess which of them were missing would guess wrong the first time
/// a band was set to exactly zero by hand.
fn hsl_dto(hsl: &HslAdjustments) -> Vec<HslShiftDto> {
    HslBand::ALL
        .into_iter()
        .map(|band| {
            let shift = hsl.get(band);
            HslShiftDto {
                band: band.as_str().to_string(),
                h: shift.h,
                s: shift.s,
                l: shift.l,
            }
        })
        .collect()
}

fn band_dto(reading: &BandReading) -> BandReadingDto {
    BandReadingDto {
        band: reading.band.as_str().to_string(),
        area: reading.area,
        hue_deg: reading.hue_deg,
        saturation: reading.saturation,
        luma: reading.luma,
        confidence: reading.confidence,
    }
}

fn guard_dto(guard: &SkinGuardReport) -> SkinGuardDto {
    SkinGuardDto {
        mask_area: guard.mask_area,
        max_hue_shift_deg: guard.max_hue_shift_deg,
        max_chroma_change: guard.max_chroma_change,
        attenuation: guard.attenuation,
        resolves: u32::from(guard.resolves),
        measured: guard.measured,
        within_ceilings: guard.within_ceilings(),
    }
}

fn variant_dto(variant: &ColourVariant) -> ColourVariantDto {
    ColourVariantDto {
        kind: variant.kind.as_str().to_string(),
        contrast: variant.contrast,
        highlights: variant.highlights,
        shadows: variant.shadows,
        whites: variant.whites,
        blacks: variant.blacks,
        vibrance: variant.vibrance,
        saturation: variant.saturation,
        curve: curve_dto(&variant.curve),
        hsl: hsl_dto(&variant.hsl),
        skin_guard: guard_dto(&variant.skin_guard),
    }
}

fn reason_dto(reason: &ColourReason) -> ColourReasonDto {
    ColourReasonDto {
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

// -- parsing ----------------------------------------------------------------

/// One override off the wire, with the curve checked before it can reach a pixel.
fn override_of(input: &SetColourOverrideInput) -> Result<ColourOverride, IpcError> {
    let curve = match &input.curve {
        Some(points) => {
            let parsed = ToneCurve::new(
                points
                    .iter()
                    .map(|point| CurvePoint::new(point.x, point.y))
                    .collect(),
            )
            .map_err(IpcError::from)?;
            Some(parsed)
        }
        None => None,
    };
    let hsl = input.hsl.as_ref().map(|bands| {
        let mut out = HslAdjustments::default();
        for entry in bands {
            out.set(
                HslBand::from_str_or_red(&entry.band),
                HslShift {
                    h: entry.h,
                    s: entry.s,
                    l: entry.l,
                }
                .clamped(),
            );
        }
        out
    });
    Ok(ColourOverride {
        contrast: input.contrast,
        highlights: input.highlights,
        shadows: input.shadows,
        whites: input.whites,
        blacks: input.blacks,
        vibrance: input.vibrance,
        saturation: input.saturation,
        curve,
        hsl,
    })
}

fn missing(detail: &str) -> IpcError {
    IpcError::from(aura_brain_photo::errors::colour_edit_refused(
        "decision", detail,
    ))
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| bad_id("project", id))
}

fn parse_photo(id: &str) -> Result<PhotoId, IpcError> {
    PhotoId::from_db(id).map_err(|_| bad_id("photo", id))
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
    use aura_recipe::HSL_BANDS;

    fn neutral() -> Recipe {
        recipe_fixtures::neutral(&"a".repeat(64), "Test Camera")
    }

    fn s_curve() -> ToneCurve {
        ToneCurve::new(vec![
            CurvePoint::new(0, 0),
            CurvePoint::new(64, 54),
            CurvePoint::new(192, 202),
            CurvePoint::new(255, 255),
        ])
        .expect("a monotone curve")
    }

    #[test]
    fn the_eight_bands_agree_with_the_recipes() {
        // ADR-0033 decision 3. `aura-core` depends on no workspace crate and cannot read the
        // recipe's array, so this crate - which can see both - is where the two are kept one
        // thing. A ninth band added to either side fails here.
        let contract: Vec<&str> = HslBand::ALL.into_iter().map(HslBand::as_str).collect();
        assert_eq!(contract, HSL_BANDS.to_vec());
    }

    #[test]
    fn an_override_touches_only_the_values_it_names() {
        let base = neutral();
        let out = with_colour(
            &base,
            &ColourOverride {
                contrast: Some(24.4),
                ..ColourOverride::default()
            },
        );
        assert_eq!(out.global.contrast, 24);
        assert_eq!(out.global.shadows, base.global.shadows);
        assert!(out.global.curve.is_identity());
    }

    #[test]
    fn an_override_cannot_leave_the_recipes_own_range() {
        let out = with_colour(
            &neutral(),
            &ColourOverride {
                contrast: Some(900.0),
                blacks: Some(-900.0),
                ..ColourOverride::default()
            },
        );
        assert_eq!(out.global.contrast, 100);
        assert_eq!(out.global.blacks, -100);
    }

    #[test]
    fn a_curve_reaches_the_recipe_as_its_own_control_points() {
        let out = with_colour(
            &neutral(),
            &ColourOverride {
                curve: Some(s_curve()),
                ..ColourOverride::default()
            },
        );
        assert_eq!(out.global.curve.points, s_curve().recipe_points());
    }

    #[test]
    fn a_non_monotone_curve_is_refused_before_it_reaches_a_recipe() {
        // The guarantee applied to the wire. A script calling this command directly cannot
        // deliver a posterising curve to the renderer.
        let input = SetColourOverrideInput {
            project_id: String::new(),
            photo_id: String::new(),
            contrast: None,
            highlights: None,
            shadows: None,
            whites: None,
            blacks: None,
            vibrance: None,
            saturation: None,
            curve: Some(vec![
                CurvePointDto { x: 0, y: 0 },
                CurvePointDto { x: 128, y: 200 },
                CurvePointDto { x: 64, y: 20 },
                CurvePointDto { x: 255, y: 255 },
            ]),
            hsl: None,
        };
        assert!(override_of(&input).is_err());
    }

    #[test]
    fn only_the_bands_that_moved_reach_the_recipe() {
        let mut hsl = HslAdjustments::default();
        hsl.set(
            HslBand::Green,
            HslShift {
                h: 6.0,
                s: -8.0,
                l: 0.0,
            },
        );
        let out = with_colour(
            &neutral(),
            &ColourOverride {
                hsl: Some(hsl),
                ..ColourOverride::default()
            },
        );
        assert_eq!(out.global.hsl.len(), 1);
        assert!(out.global.hsl.contains_key("green"));
    }

    #[test]
    fn a_persons_value_survives_an_automated_pass() {
        // The property this whole module exists to keep, and the nine-path version of phase
        // 15's three-path test. A person sets the contrast; the pass then proposes its own,
        // and the merge refuses that one path and takes the rest.
        let base = neutral();
        let mine = with_colour(
            &base,
            &ColourOverride {
                contrast: Some(30.0),
                ..ColourOverride::default()
            },
        );
        let (base, _) = schema::merge(&base, &mine, EditSource::User).expect("user merge");
        assert!(base
            .provenance
            .user_edited_fields
            .contains(&"global.contrast".to_string()));

        let theirs = with_colour(
            &base,
            &ColourOverride {
                contrast: Some(-20.0),
                shadows: Some(14.0),
                ..ColourOverride::default()
            },
        );
        let (merged, report) = schema::merge(&base, &theirs, EditSource::Ai).expect("ai merge");
        assert_eq!(merged.global.contrast, 30, "the person's contrast stood");
        assert_eq!(merged.global.shadows, 14, "the rest applied");
        assert!(report.refused.contains(&"global.contrast".to_string()));
    }

    #[test]
    fn this_surface_cannot_reach_a_white_balance() {
        // The boundary section 2.2 draws between phases 15 and 16, checked rather than
        // remembered: nine paths, and none of them is a temperature.
        let out = with_colour(
            &neutral(),
            &ColourOverride {
                contrast: Some(20.0),
                highlights: Some(-20.0),
                shadows: Some(20.0),
                whites: Some(10.0),
                blacks: Some(-10.0),
                vibrance: Some(8.0),
                saturation: Some(0.0),
                curve: Some(s_curve()),
                hsl: Some(HslAdjustments::default()),
            },
        );
        let base = neutral();
        assert_eq!(out.global.temperature, base.global.temperature);
        assert_eq!(out.global.tint, base.global.tint);
        assert!((out.global.exposure - base.global.exposure).abs() < f32::EPSILON);
    }

    #[test]
    fn the_recipe_paths_an_override_owns_are_the_nine_this_module_writes() {
        let all = ColourOverride {
            contrast: Some(0.0),
            highlights: Some(0.0),
            shadows: Some(0.0),
            whites: Some(0.0),
            blacks: Some(0.0),
            vibrance: Some(0.0),
            saturation: Some(0.0),
            curve: Some(s_curve()),
            hsl: Some(HslAdjustments::default()),
        };
        assert_eq!(all.recipe_paths().len(), 9);
    }
}
