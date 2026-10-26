//! Prepare and apply for the distribution-transfer engine (ADR-0160).
//!
//! The photograph is measured with any earlier reference grade removed, so matching a second
//! reference replaces the first instead of stacking on it, and the strengths move from that
//! earlier look - or from none - to the new one. Zero is therefore the photograph exactly as it
//! was. Both steps report what the renderer actually drew; Apply refuses a measured regression.
use aura_core::contract::style::StyleDelta;
use aura_core::progress::CancelToken;
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_raw::codec::Rgb8;
use aura_recipe::reference_grade::{self as grade_model, ReferenceGrade};
use aura_recipe::{schema, Recipe};
use aura_render::Frame;
use serde::{Deserialize, Serialize};

use super::{
    appearance, baseline, check_cancel, content, fitting_frame_quality, invalid, rendered, report,
    session_key, store, transfer, AnalysisBaseline, AppState, ApplyInput, Controls, FitDiagnostics,
    MatchIntent, MatchReport, PrepareInput, Reference, Session, ENGINE,
};

mod native;

const ENGINE_NAME: &str = "reference-grade-transfer-v3";

/// What a transfer session keeps between analysis and every later strength change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TransferSession {
    /// The reference grade the photograph already had, which strength zero returns to.
    pub prior: Option<ReferenceGrade>,
    pub fitted: ReferenceGrade,
    /// The reference's measurement, kept so the session works after the reference is removed
    /// from the library.
    pub reference: transfer::GradeStats,
}

/// How the transfer fit went, for the report and the panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferDiagnostics {
    pub content_similarity: f32,
    pub iterations: usize,
    pub predicted_before: transfer::Distance,
    pub predicted_after: transfer::Distance,
    pub rendered_before: transfer::Distance,
    pub rendered_after: transfer::Distance,
    pub skin: transfer::SkinReview,
    pub replaced_earlier_match: bool,
}

fn measured(reference: &transfer::GradeStats, image: &Rgb8) -> transfer::Distance {
    transfer::distance(reference, &transfer::measure_image(image))
}

fn preview_distances(
    state: &AppState,
    frame: &Frame,
    baseline: &Recipe,
    plain: &Rgb8,
    reference: &transfer::GradeStats,
    grade: &ReferenceGrade,
) -> AuraResult<(transfer::Distance, transfer::Distance)> {
    let earlier;
    let shown = if grade_model::read(baseline)?.is_some() {
        earlier = rendered(state, frame, baseline)?;
        &earlier
    } else {
        plain
    };
    let before = measured(reference, shown);
    let mut candidate = baseline.clone();
    candidate.extra.remove(grade_model::KEY);
    grade_model::write(&mut candidate, grade)?;
    Ok((
        before,
        measured(reference, &rendered(state, frame, &candidate)?),
    ))
}

struct Observations {
    native: Option<(transfer::Distance, transfer::Distance, f32, usize)>,
    before: transfer::Distance,
    after: transfer::Distance,
    skin: transfer::SkinReview,
    similarity: Option<f32>,
    prior: bool,
    samples: usize,
}

fn diagnostics(
    input: &PrepareInput,
    fit: &transfer::Fit,
    observation: Observations,
) -> FitDiagnostics {
    FitDiagnostics {
        engine: if input.intent == MatchIntent::Palette {
            "reference-palette-transport-v8"
        } else {
            ENGINE_NAME
        }
        .into(),
        intent: input.intent,
        quality: input.quality,
        target_samples: observation.samples,
        max_iterations: input.quality.iterations(),
        refinement_candidates: fit.iterations,
        unique_renders: observation
            .native
            .map_or(if observation.prior { 3 } else { 2 }, |v| v.3 + 1),
        cached_proposals: 0,
        grade_candidates: 0,
        native_candidate_checked: observation.native.is_some(),
        native_backtrack_factor: observation.native.map_or(1.0, |v| v.2),
        structure_similarity: observation.similarity,
        regularization_weight: 0.0,
        transfer: Some(TransferDiagnostics {
            content_similarity: fit.content,
            iterations: fit.iterations,
            predicted_before: fit.before,
            predicted_after: fit.after,
            rendered_before: observation.before,
            rendered_after: observation.after,
            skin: observation.skin,
            replaced_earlier_match: observation.prior,
        }),
    }
}

fn upgrade_palette_measurements(
    reference: &Reference,
    stats: &mut transfer::GradeStats,
) -> AuraResult<()> {
    // Older v3 descriptors have a small stored preview but no joint colour evidence.
    if stats.palette_samples.is_empty() {
        if reference.small_rgb.len() != content::BYTES {
            return Err(invalid("This reference needs joint colour measurements. Upload its edited photo again. No edit was saved."));
        }
        stats.palette_samples = transfer::measure_image(&Rgb8 {
            width: content::EDGE,
            height: content::EDGE,
            data: reference.small_rgb.clone(),
        })
        .palette_samples;
    }
    stats.palette_matching = true;
    Ok(())
}

/// Measure the photograph without any earlier reference grade and fit the reference onto it.
pub(super) fn prepare(
    state: &AppState,
    cancel: &CancelToken,
    input: &PrepareInput,
    photo: PhotoId,
    reference: Reference,
    baseline: AnalysisBaseline,
    mut reference_measurements: transfer::GradeStats,
) -> AuraResult<MatchReport> {
    if input.intent == MatchIntent::Palette {
        upgrade_palette_measurements(&reference, &mut reference_measurements)?;
    }
    let observed_hash = baseline.observed_hash;
    let baseline = baseline.recipe;
    let baseline_hash = aura_recipe::hash::recipe_hash(&baseline)?;
    let prior = grade_model::read(&baseline).ok().flatten();
    let mut ungraded = baseline.clone();
    ungraded.extra.remove(grade_model::KEY);
    let frame = fitting_frame_quality(state, &photo, input.quality)?;
    let plain = rendered(state, &frame, &ungraded)?;
    check_cancel(cancel)?;
    let target_regions = appearance::analyse(&plain)?;
    let similarity = content::similarity(&reference.small_rgb, &plain);
    let target_samples = transfer::samples(&plain, input.quality.target_samples());
    let mut fit = transfer::fit_with_intent_checkpoint(
        &reference_measurements,
        &target_samples,
        similarity,
        input.quality,
        input.intent,
        || check_cancel(cancel),
    )?;
    let monochrome = reference_measurements.achromatic || reference.achromatic;
    let skin = transfer::review_skin(&mut fit.grade, &plain, &target_regions, monochrome);
    check_cancel(cancel)?;
    // Palette transfer explicitly allows the reference's tone distribution to guide an
    // unrelated scene. Find a native-safe fitted endpoint before offering it at 100%.
    // The safeguards are identical to Apply, and every attempted render shares one baseline.
    let native_fit = if input.intent == MatchIntent::Palette {
        Some(native::fit(
            state,
            cancel,
            native::FitInput {
                photo,
                baseline: &baseline,
                ungraded: &ungraded,
                regions: &target_regions,
                reference: &reference_measurements,
                monochrome,
                grade: &mut fit.grade,
            },
        )?)
    } else {
        None
    };
    // The renderer is the instrument: report what the stage drew, not what the fit predicted.
    let (rendered_before, rendered_after) = if let Some(native) = native_fit {
        (native.0, native.1)
    } else {
        preview_distances(
            state,
            &frame,
            &baseline,
            &plain,
            &reference_measurements,
            &fit.grade,
        )?
    };
    check_cancel(cancel)?;
    baseline::verify_current(state, photo, &observed_hash)?;
    let fitting = diagnostics(
        input,
        &fit,
        Observations {
            native: native_fit,
            before: rendered_before,
            after: rendered_after,
            skin,
            similarity,
            prior: prior.is_some(),
            samples: target_samples.len(),
        },
    );
    let session = Session {
        engine: ENGINE.into(),
        id: uuid::Uuid::new_v4().to_string(),
        photo_id: input.photo_id.clone(),
        reference: reference.summary.clone(),
        baseline,
        baseline_hash: baseline_hash.clone(),
        last_hash: observed_hash,
        fitted: StyleDelta::neutral(),
        reference_regions: reference.regions,
        target_regions,
        before_distance: rendered_before.total,
        fitted_distance: rendered_after.total,
        controls: reference.summary.default_controls.unwrap_or_default(),
        checked_hash: None,
        measured_before: None,
        measured_after: None,
        fitting: Some(fitting),
        reference_monochrome: monochrome,
        grade_offsets: [0.0; 6],
        transfer: Some(TransferSession {
            prior,
            fitted: fit.grade,
            reference: reference_measurements,
        }),
    };
    save_analysis(state, &session)
}

fn save_analysis(state: &AppState, session: &Session) -> AuraResult<MatchReport> {
    let result = report(
        session,
        0,
        session.baseline.provenance.user_edited_fields.len(),
        session.fitted_distance,
        false,
        true,
    );
    store(state, &session_key(&session.photo_id)?, session)?;
    Ok(result)
}

/// The strengths a photographer set, as how far each component moves.
fn scales(controls: &Controls) -> transfer::Scales {
    let d = &controls.detail;
    let amount = controls.amount;
    let tone = amount * controls.tone * d.contrast * d.curves;
    transfer::Scales {
        tone: [
            tone * d.blacks,
            tone * d.shadows,
            tone * d.exposure,
            tone * d.highlights,
            tone * d.whites,
        ],
        temperature: amount * controls.white_balance * d.temperature,
        tint: amount * controls.white_balance * d.tint,
        chroma: amount * controls.color * d.vibrance,
        saturation: amount * controls.color * d.saturation,
        hue: amount * controls.color * d.hue,
        hue_light: amount * controls.tone * d.curves,
        skin: amount.min(1.0),
    }
}

/// The recipe these strengths produce from the session's baseline.
fn candidate(
    session: &Session,
    transfer: &TransferSession,
    controls: &Controls,
) -> AuraResult<Recipe> {
    let identity = ReferenceGrade::identity();
    let from = transfer.prior.as_ref().unwrap_or(&identity);
    let model = transfer::mix(from, &transfer.fitted, &scales(controls));
    let mut candidate = session.baseline.clone();
    // Strength zero is the earlier look exactly - the stored value itself, not a re-encoding.
    if transfer.prior.as_ref() != Some(&model) {
        candidate.extra.remove(grade_model::KEY);
        grade_model::write(&mut candidate, &model)?;
    }
    schema::Validation::check(&candidate)?;
    // Verification and repeated-strength equality must use the precision actually saved.
    // Raw f32-derived extension values otherwise differ from the six-decimal stored recipe,
    // which can manufacture stale hashes or duplicate Undo entries on an identical request.
    canonical(&candidate)
}

fn canonical(recipe: &Recipe) -> AuraResult<Recipe> {
    serde_json::from_str(&aura_recipe::canonical(recipe)?)
        .map_err(|_| invalid("The fitted reference recipe could not be encoded."))
}

/// Native quality checks use the exact candidate, including extrapolated strength controls.
fn verified_distances(
    state: &AppState,
    cancel: &CancelToken,
    session: &Session,
    reference: &transfer::GradeStats,
    photo: &PhotoId,
    candidate: &Recipe,
) -> AuraResult<(transfer::Distance, transfer::Distance, bool)> {
    let review = native::Review::new(
        state,
        cancel,
        *photo,
        &session.baseline,
        &session.target_regions,
        reference,
        session.reference_monochrome,
    )?;
    let checked = review.check(state, cancel, candidate)?.ok_or_else(|| invalid(
        "This strength failed the full-resolution smoothness, highlight or skin-colour check, or moved colour farther from the reference. Reduce the strengths and apply again. No edit was saved."))?;
    Ok((review.before, checked.after, checked.reused))
}

/// Apply explicit strengths from the saved baseline and measure the full-resolution result.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    state: &AppState,
    cancel: &CancelToken,
    input: &ApplyInput,
    key: &str,
    mut session: Session,
    transfer: TransferSession,
    photo: PhotoId,
    current: &Recipe,
    hash: String,
    has_saved_recipe: bool,
) -> AuraResult<MatchReport> {
    if !transfer.reference.usable() {
        return Err(invalid(
            "This analysis needs fresh colour measurements. Upload its edited photo again, then analyse the match. No edit was saved.",
        ));
    }
    let candidate = candidate(&session, &transfer, &input.controls)?;
    let (before, after, reused) = verified_distances(
        state,
        cancel,
        &session,
        &transfer.reference,
        &photo,
        &candidate,
    )?;
    if let Some(fitting) = session.fitting.as_mut() {
        fitting.native_candidate_checked = true;
        fitting.cached_proposals += usize::from(reused);
        if let Some(transfer) = fitting.transfer.as_mut() {
            transfer.rendered_before = before;
            transfer.rendered_after = after;
        }
    }
    let mut merged = current.clone();
    match candidate.extra.get(grade_model::KEY) {
        Some(value) => {
            merged.extra.insert(grade_model::KEY.into(), value.clone());
        }
        None => {
            merged.extra.remove(grade_model::KEY);
        }
    }
    // Canonical identity is the renderer/store contract; JSON number representations may
    // compare differently in memory even when their stored values and pixels are identical.
    let candidate_hash = aura_recipe::hash::recipe_hash(&merged)?;
    let changed: Vec<String> = if candidate_hash == hash {
        Vec::new()
    } else {
        vec![format!("extra.{}", grade_model::KEY)]
    };
    check_cancel(cancel)?;
    let project: String = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&input.photo_id],
            |r| r.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("colour match photo", &e))
    })?;
    let project = ProjectId::from_db(&project).map_err(|_| invalid("Invalid collection."))?;
    if !changed.is_empty() {
        let c = &input.controls;
        super::save_verified(
            state,
            &project,
            &photo,
            &merged,
            &changed,
            &format!(
                "Reference colour: {}; amount {:.0}%, tone {:.0}%, colour {:.0}%, warmth {:.0}%; colour and tone distance {:.2} -> {:.2}",
                session.reference.name,
                c.amount * 100.0,
                c.tone * 100.0,
                c.color * 100.0,
                c.white_balance * 100.0,
                before.total,
                after.total
            ),
            has_saved_recipe.then_some(hash.as_str()),
        )?;
    }
    session.last_hash = if changed.is_empty() {
        hash
    } else {
        candidate_hash
    };
    session.controls = input.controls;
    session.checked_hash = Some(session.last_hash.clone());
    session.measured_before = Some(before.total);
    session.measured_after = Some(after.total);
    session.transfer = Some(transfer);
    store(state, key, &session)?;
    let mut result = report(&session, changed.len(), 0, after.total, true, true);
    result.before_distance = before.total;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strength_controls_reach_every_component_independently() {
        let full = scales(&Controls::default());
        assert!(full.tone.iter().all(|v| (v - 1.0).abs() < 1e-6));
        let controls = Controls {
            white_balance: 0.0,
            ..Controls::default()
        };
        let s = scales(&controls);
        assert!(s.temperature.abs() < 1e-6 && s.tint.abs() < 1e-6 && (s.chroma - 1.0).abs() < 1e-6);
        let controls = Controls {
            detail: super::super::FineControls {
                shadows: 0.0,
                ..super::super::FineControls::default()
            },
            ..Controls::default()
        };
        let s = scales(&controls);
        assert!(s.tone[1].abs() < 1e-6 && (s.tone[2] - 1.0).abs() < 1e-6);
        let controls = Controls {
            amount: 0.0,
            ..Controls::default()
        };
        let s = scales(&controls);
        assert!(s
            .tone
            .iter()
            .chain([&s.temperature, &s.chroma, &s.hue, &s.skin])
            .all(|v| v.abs() < 1e-6));
    }
}
