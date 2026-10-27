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
use aura_render::{FrameSource, RenderLevel};
use serde::{Deserialize, Serialize};

use super::{
    appearance, check_cancel, content, fitting_frame, invalid, rendered, report, session_key,
    skin_guard, store, transfer, AppState, ApplyInput, Controls, FitDiagnostics, MatchReport,
    PrepareInput, Reference, Session, ENGINE,
};

const ENGINE_NAME: &str = "reference-grade-transfer-v3";
/// New clipping a look fit may add, measured on a fitting-size render. Below the one per cent
/// the full-resolution check allows, so the check confirms rather than cuts.
const LOOK_CLIP_ALLOWANCE: f32 = 0.007;

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
    /// Fitted to follow the reference's look (one-click matching, ADR-0170) rather than the
    /// conservative estimate. Checked with the look skin guard.
    #[serde(default)]
    pub look: bool,
}

/// What a session's checks compare against. A one-click look replaces any earlier reference
/// look, so it is judged against the photograph without one; judging it against the earlier
/// look would refuse a new reference for not improving on the old one's highlights.
pub(super) fn guard_baseline(session: &Session) -> Recipe {
    let mut baseline = session.baseline.clone();
    if session.transfer.as_ref().is_some_and(|t| t.look) {
        baseline.extra.remove(grade_model::KEY);
    }
    baseline
}

/// The skin check for this session: the look guard for a look fit, the strict guard otherwise.
pub(super) fn skin_ok(
    session: &Session,
    before: &[appearance::Region],
    after: &[appearance::Region],
) -> bool {
    if !session.reference_monochrome && session.transfer.as_ref().is_some_and(|t| t.look) {
        appearance::skin_natural(before, after)
    } else {
        skin_guard(before, after, session.reference_monochrome)
    }
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

pub(super) fn measured(reference: &transfer::GradeStats, image: &Rgb8) -> transfer::Distance {
    transfer::distance(reference, &transfer::measure_image(image))
}

/// Measure the photograph without any earlier reference grade and fit the reference onto it.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // One analysis, two fits, one store.
pub(super) fn prepare(
    state: &AppState,
    cancel: &CancelToken,
    input: &PrepareInput,
    photo: PhotoId,
    reference: Reference,
    baseline: Recipe,
    reference_measurements: transfer::GradeStats,
    look: bool,
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
    // One-click matching needs a different photograph; refuse before anything is stored.
    if look && similarity.is_some_and(|s| s >= super::one_click::DUPLICATE_SIMILARITY) {
        return Err(invalid(format!(
            "{}. Choose a different photo as the reference; nothing was changed.",
            super::one_click::DUPLICATE
        )));
    }
    let samples = transfer::samples(&plain, transfer::TARGET_SAMPLES);
    let mut fit = if look {
        transfer::fit_look(
            &reference_measurements,
            &samples,
            &transfer::skin_samples(&plain, &target_regions),
            &transfer::palette(&reference.small_rgb),
            &transfer::LookOptions::default(),
            || check_cancel(cancel),
        )?
    } else {
        transfer::fit_with_checkpoint(&reference_measurements, &samples, similarity, || {
            check_cancel(cancel)
        })?
    };
    let monochrome = reference_measurements.achromatic || reference.achromatic;
    let skin = transfer::review_skin(&mut fit.grade, &plain, &target_regions, monochrome);
    if look && !monochrome {
        // Hold each person's skin to the look guard with the guard's own readings, so the full
        // look can be kept rather than turned down for one face.
        transfer::settle(&mut fit.grade, &samples, &plain, &target_regions);
        // The fit estimates clipping on finished pixels; the renderer applies the look inside
        // its pipeline, where near-white sky behaves differently. Check on real renders at
        // fitting size, the same instrument the one-click prediction uses, and ease until the
        // look adds less than the full check's allowance.
        let mut probe = ungraded.clone();
        for _ in 0..10 {
            check_cancel(cancel)?;
            probe.extra.remove(grade_model::KEY);
            grade_model::write(&mut probe, &fit.grade)?;
            let (fresh, white, sectors) =
                transfer::new_clipping(&fit.grade, &plain, &rendered(state, &frame, &probe)?);
            if fresh <= LOOK_CLIP_ALLOWANCE {
                break;
            }
            transfer::ease_highlights(&mut fit.grade, white, &sectors);
            transfer::settle_skin(&mut fit.grade, &plain, &target_regions);
        }
    }
    check_cancel(cancel)?;
    // The renderer is the instrument: report what the stage drew, not what the fit predicted.
    let earlier;
    let shown = if prior.is_some() {
        earlier = rendered(state, &frame, &baseline)?;
        &earlier
    } else {
        &plain
    };
    let rendered_before = measured(&reference_measurements, shown);
    let mut candidate = ungraded;
    grade_model::write(&mut candidate, &fit.grade)?;
    let rendered_after = measured(
        &reference_measurements,
        &rendered(state, &frame, &candidate)?,
    );
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
            rendered_before,
            rendered_after,
            skin,
            replaced_earlier_match: prior.is_some(),
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
            reference: reference_measurements,
            look,
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
pub(super) fn candidate(
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

/// Native quality checks use the exact candidate, including extrapolated strength controls.
fn verified_distances(
    state: &AppState,
    cancel: &CancelToken,
    session: &Session,
    reference: &transfer::GradeStats,
    photo: &PhotoId,
    candidate: &Recipe,
) -> AuraResult<(transfer::Distance, transfer::Distance)> {
    let frame =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(photo, RenderLevel::Full)?;
    check_cancel(cancel)?;
    let baseline = rendered(state, &frame, &guard_baseline(session))?;
    let before = measured(reference, &baseline);
    let before_regions = appearance::readings(&baseline, &session.target_regions);
    let before_clip = appearance::clipped(&baseline);
    drop(baseline);
    check_cancel(cancel)?;
    let image = rendered(state, &frame, candidate)?;
    let after = measured(reference, &image);
    if appearance::clipped(&image) > before_clip + 0.01
        || !skin_ok(
            session,
            &before_regions,
            &appearance::readings(&image, &session.target_regions),
        )
    {
        return Err(invalid(
            "This strength failed the full-resolution highlight or skin-colour check. Reduce the strengths and apply again. No edit was saved.",
        ));
    }
    drop(image);
    if !before.total.is_finite() || !after.total.is_finite() {
        return Err(invalid(
            "The final colour measurement was unavailable. No edit was saved.",
        ));
    }
    if after.total > before.total + 0.0001 {
        return Err(invalid(
            "These strengths moved the native-resolution colour farther from the reference. Reduce the strengths or analyse again. No edit was saved.",
        ));
    }
    Ok((before, after))
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
    if !transfer.reference.usable() {
        return Err(invalid(
            "This analysis needs fresh colour measurements. Upload its edited photo again, then analyse the match. No edit was saved.",
        ));
    }
    let candidate = candidate(&session, &transfer, &input.controls)?;
    let (before, after) = verified_distances(
        state,
        cancel,
        &session,
        &transfer.reference,
        &photo,
        &candidate,
    )?;
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
