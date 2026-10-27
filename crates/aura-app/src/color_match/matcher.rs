//! Prepare and apply for the distribution-transfer engine (ADR-0160).
//!
//! The photograph is measured with any earlier reference grade removed, so matching a second
//! reference replaces the first instead of stacking on it, and the strengths move from that
//! earlier look - or from none - to the new one. Zero is therefore the photograph exactly as it
//! was. Both steps report what the renderer actually drew. Neither refuses a strength for being
//! far from the reference's appearance: a learned edit is the right answer even where the scene
//! keeps the photograph unlike the reference, and the distance is reported, not enforced.
use aura_core::contract::style::StyleDelta;
use aura_core::progress::CancelToken;
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_raw::codec::Rgb8;
use aura_recipe::reference_grade::{self as grade_model, ReferenceGrade};
use aura_recipe::{schema, Recipe};
use aura_render::{FrameSource, RenderLevel};
use serde::{Deserialize, Serialize};

use super::{
    appearance, check_cancel, content, edit, fitting_frame, invalid, rendered, report, session_key,
    store, transfer, AppState, ApplyInput, Controls, FitDiagnostics, MatchReport, PrepareInput,
    Reference, Session, ENGINE,
};

const ENGINE_NAME: &str = "reference-grade-transfer-v2";

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
    /// Distance from what the fit aimed at, which for a different scene is only part of the
    /// reference's appearance.
    #[serde(default)]
    pub predicted_goal_after: transfer::Distance,
    pub rendered_before: transfer::Distance,
    pub rendered_after: transfer::Distance,
    pub skin: transfer::SkinReview,
    pub replaced_earlier_match: bool,
    #[serde(default)]
    pub method: Method,
    /// The file the edit was learned from, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learned_from: Option<String>,
}

/// How a match was made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    /// This photograph is the reference's own frame; the edit was fitted on it whole.
    SameFrame,
    /// The edit was learned from the reference's original and applied unchanged.
    LearnedEdit,
    /// No original was found; the reference's appearance was matched part of the way.
    #[default]
    Appearance,
}

/// The collection a photograph belongs to.
fn project_of(state: &AppState, photo_id: &str) -> AuraResult<ProjectId> {
    let project: String = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [photo_id],
            |r| r.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("colour match photo", &e))
    })?;
    ProjectId::from_db(&project).map_err(|_| invalid("Invalid collection."))
}

fn measured(reference: &transfer::GradeStats, image: &Rgb8) -> transfer::Distance {
    transfer::distance(reference, &transfer::measure_image(image))
}

/// Measure the photograph without any earlier reference grade and fit the reference onto it.
#[allow(clippy::too_many_lines, clippy::similar_names)]
pub(super) fn prepare(
    state: &AppState,
    cancel: &CancelToken,
    input: &PrepareInput,
    photo: PhotoId,
    reference: Reference,
    baseline: Recipe,
    stats: transfer::GradeStats,
) -> AuraResult<MatchReport> {
    let baseline_hash = aura_recipe::hash::recipe_hash(&baseline)?;
    let prior = grade_model::read(&baseline).ok().flatten();
    let mut ungraded = baseline.clone();
    ungraded.extra.remove(grade_model::KEY);
    let frame = fitting_frame(state, &photo)?;
    let plain = rendered(state, &frame, &ungraded)?;
    check_cancel(cancel)?;
    let target_regions = appearance::analyse(&plain)?;
    let similarity = content::similarity(&reference.small_rgb, &plain);
    let pixels = transfer::samples(&plain, transfer::TARGET_SAMPLES);
    // The edit itself when the reference's original is known - this photograph, or one named
    // like the reference in its collection - and the reference's appearance only otherwise.
    let learned = if similarity.is_some_and(|s| s >= edit::SAME_FRAME) {
        let learned = edit::learn(
            &stats,
            &reference,
            &plain,
            similarity.unwrap_or(1.0),
            &input.photo_id,
            &edit::file_name(state, &photo)?,
            || check_cancel(cancel),
        )?;
        edit::remember(state, &reference.summary.id, &learned)?;
        Some((Method::SameFrame, learned))
    } else {
        let known = match edit::recall(state, &reference.summary.id)? {
            Some(known) => Some(known),
            None => edit::search(
                state,
                &project_of(state, &input.photo_id)?,
                &reference,
                &stats,
                &photo,
                || check_cancel(cancel),
            )?,
        };
        known.map(|known| (Method::LearnedEdit, known))
    };
    let (method, learned_from, mut fit) = match learned {
        Some((method, learned)) => {
            // The same edit for every frame. Bringing each frame's greys to the original's first
            // was measured on a real shoot and left the batch farther from the right answer.
            let grade = learned.grade;
            let (before, after) = transfer::predicted(&stats, &grade, &pixels);
            let fit = transfer::Fit {
                grade,
                before,
                after,
                goal_after: after,
                iterations: 0,
                content: transfer::content_of(similarity),
            };
            (method, Some(learned.source), fit)
        }
        None => (
            Method::Appearance,
            None,
            transfer::fit_with_checkpoint(&stats, &pixels, similarity, || check_cancel(cancel))?,
        ),
    };
    let monochrome = stats.achromatic || reference.achromatic;
    let skin = transfer::review_skin(&mut fit.grade, &plain, &target_regions, monochrome);
    check_cancel(cancel)?;
    // The renderer is the instrument: report what the stage drew, not what the fit predicted.
    let earlier;
    let shown = if prior.is_some() {
        earlier = rendered(state, &frame, &baseline)?;
        &earlier
    } else {
        &plain
    };
    let rendered_before = measured(&stats, shown);
    let mut candidate = ungraded;
    grade_model::write(&mut candidate, &fit.grade)?;
    let rendered_after = measured(&stats, &rendered(state, &frame, &candidate)?);
    check_cancel(cancel)?;
    if aura_recipe::hash::recipe_hash(&crate::develop_commands::load_or_neutral(state, photo)?)?
        != baseline_hash
    {
        return Err(invalid(
            "The photo changed during analysis. Your newer edit was kept. Analyse it again.",
        ));
    }
    let fitting = FitDiagnostics {
        engine: ENGINE_NAME.into(),
        refinement_candidates: fit.iterations,
        unique_renders: if prior.is_some() { 3 } else { 2 },
        cached_proposals: 0,
        grade_candidates: 0,
        native_candidate_checked: true,
        native_backtrack_factor: 1.0,
        structure_similarity: similarity,
        regularization_weight: 0.0,
        transfer: Some(TransferDiagnostics {
            content_similarity: fit.content,
            iterations: fit.iterations,
            predicted_before: fit.before,
            predicted_after: fit.after,
            predicted_goal_after: fit.goal_after,
            rendered_before,
            rendered_after,
            skin,
            replaced_earlier_match: prior.is_some(),
            method,
            learned_from,
        }),
    };
    let session = Session {
        engine: ENGINE.into(),
        id: uuid::Uuid::new_v4().to_string(),
        photo_id: input.photo_id.clone(),
        reference: reference.summary.clone(),
        baseline,
        baseline_hash: baseline_hash.clone(),
        last_hash: baseline_hash,
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
            reference: stats,
        }),
    };
    let result = report(
        &session,
        0,
        session.baseline.provenance.user_edited_fields.len(),
        rendered_after.total,
        false,
        true,
    );
    store(state, &session_key(&input.photo_id)?, &session)?;
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
    Ok(candidate)
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
) -> AuraResult<MatchReport> {
    let candidate = candidate(&session, &transfer, &input.controls)?;
    let frame =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(&photo, RenderLevel::Full)?;
    check_cancel(cancel)?;
    let before = measured(
        &transfer.reference,
        &rendered(state, &frame, &session.baseline)?,
    );
    check_cancel(cancel)?;
    let after = measured(&transfer.reference, &rendered(state, &frame, &candidate)?);
    if !before.total.is_finite() || !after.total.is_finite() {
        return Err(invalid(
            "The final colour measurement was unavailable. No edit was saved.",
        ));
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
    let changed: Vec<String> = if merged == *current {
        Vec::new()
    } else {
        vec![format!("extra.{}", grade_model::KEY)]
    };
    check_cancel(cancel)?;
    let project = project_of(state, &input.photo_id)?;
    if !changed.is_empty() {
        let c = &input.controls;
        state.recipe_store().save_if_current(
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
            &hash,
        )?;
    }
    session.last_hash = if changed.is_empty() {
        hash
    } else {
        aura_recipe::hash::recipe_hash(&merged)?
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
    #[allow(clippy::field_reassign_with_default)] // one control changed from the default at a time
    fn the_strength_controls_reach_every_component_independently() {
        let full = scales(&Controls::default());
        assert!(full.tone.iter().all(|v| (v - 1.0).abs() < 1e-6));
        let mut controls = Controls::default();
        controls.white_balance = 0.0;
        let s = scales(&controls);
        assert!(s.temperature.abs() < 1e-6 && s.tint.abs() < 1e-6 && (s.chroma - 1.0).abs() < 1e-6);
        controls = Controls::default();
        controls.detail.shadows = 0.0;
        let s = scales(&controls);
        assert!(s.tone[1].abs() < 1e-6 && (s.tone[2] - 1.0).abs() < 1e-6);
        controls = Controls::default();
        controls.amount = 0.0;
        let s = scales(&controls);
        assert!(s
            .tone
            .iter()
            .chain([&s.temperature, &s.chroma, &s.hue, &s.skin])
            .all(|v| v.abs() < 1e-6));
    }
}
