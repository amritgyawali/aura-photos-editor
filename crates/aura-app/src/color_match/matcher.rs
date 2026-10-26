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
use serde::{Deserialize, Serialize};

use super::{
    appearance, baseline, check_cancel, content, fitting_frame_quality, invalid, rendered, report,
    session_key, store, transfer, AnalysisBaseline, AppState, ApplyInput, Controls, FitDiagnostics,
    MatchIntent, MatchReport, PrepareInput, Reference, Session, ENGINE,
};

mod native;

const ENGINE_NAME: &str = "reference-grade-transfer-v4-scene-tone";
/// The engine of a look learned from the reference's own original (ADR-0161).
const LEARNED_ENGINE: &str = "reference-original-calibrated-v1";
const EXACT_ENGINE: &str = "reference-exact-colours-v1";
/// How far the exact colours may move skin, in multiples of the native check's limits: they are
/// the reference's colours, which is a larger change than a look, and still never an unchecked one.
const EXACT_SKIN: f32 = 2.0;

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
    /// The look learned from the reference's own original in the collection (ADR-0161), whose
    /// `model.amount` is the strongest share of it that passed this photograph's native checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learned: Option<super::learned::LearnedLook>,
    /// How far the reference's exact colours may move skin and clipping (ADR-0182), so Apply
    /// checks a strength by the same limits analysis did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowance: Option<super::gallery_fit::Allowance>,
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
    /// The scene-prior tone estimate, present when the reference's scene differs (ADR-0180).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone_estimate: Option<transfer::ToneReading>,
    /// The reference's own original, found in the collection, that the look was learned from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learned_from: Option<String>,
    /// Why a photograph that looked like the reference's original could not be learned from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_refused: Option<String>,
    /// How far the fitted look stretched nearby colours before it was limited to avoid bands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stretch_limited_from: Option<f32>,
    /// The share of the photograph the look would newly have clipped before its highlights were
    /// held back, when they had to be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlights_limited_from: Option<f32>,
}

fn measured(reference: &transfer::GradeStats, image: &Rgb8) -> transfer::Distance {
    transfer::distance(reference, &transfer::measure_image(image))
}

struct Observations {
    native: Option<(
        transfer::Distance,
        transfer::Distance,
        f32,
        usize,
        Option<f32>,
    )>,
    before: transfer::Distance,
    after: transfer::Distance,
    skin: transfer::SkinReview,
    similarity: Option<f32>,
    prior: bool,
    samples: usize,
    refused: Option<String>,
}

fn diagnostics(
    input: &PrepareInput,
    fit: &transfer::Fit,
    observation: Observations,
) -> FitDiagnostics {
    FitDiagnostics {
        engine: match input.intent {
            MatchIntent::Palette => "reference-palette-transport-v8",
            MatchIntent::Exact => EXACT_ENGINE,
            MatchIntent::Natural => ENGINE_NAME,
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
            tone_estimate: fit.tone,
            learned_from: None,
            original_refused: observation.refused,
            stretch_limited_from: fit.stretch_limited_from,
            highlights_limited_from: observation.native.and_then(|v| v.4),
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
#[allow(clippy::too_many_lines)]
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
    let monochrome = reference_measurements.achromatic || reference.achromatic;
    // The reference's own original in this collection gives the photographer's actual edit,
    // learned pixel for pixel; it is used whenever it is there, for the Natural and Palette goals.
    // The Exact goal asks for the reference's colours themselves (ADR-0182).
    let (learned, refused) = if input.intent == MatchIntent::Exact {
        (None, None)
    } else {
        let project = photo_project(state, &photo)?;
        super::learned::find(state, cancel, &reference.summary.id, &project)?
    };
    if let Some(learned) = learned {
        return prepare_learned(
            state,
            cancel,
            input,
            photo,
            reference,
            Learning {
                baseline,
                baseline_hash,
                observed_hash,
                prior,
                regions: target_regions,
                measurements: reference_measurements,
                monochrome,
                learned,
            },
        );
    }
    let similarity = content::similarity(&reference.small_rgb, &plain);
    let target_samples = transfer::samples(&plain, input.quality.target_samples());
    // The Exact goal gives the photograph the reference's colours themselves (ADR-0182); skin
    // goes to the reference's skin inside the transfer, so the look-wide skin review does not
    // apply, and the native checks allow it a larger move.
    let mut allowance = None;
    let (mut fit, skin) = if input.intent == MatchIntent::Exact {
        let (colours, reference_clipped) =
            super::exact::reference_colours(state, &reference.summary.id)?.ok_or_else(|| {
                invalid("This reference has no stored colours. Upload its edited photo again, then match. No edit was saved.")
            })?;
        // The reference's own clipped highlights are part of its colours; more than that is not.
        allowance = Some(super::gallery_fit::Allowance {
            skin: EXACT_SKIN,
            clipping: (reference_clipped - appearance::clipped(&plain)).max(0.0) + 0.01,
        });
        let fit = transfer::fit_exact(&reference_measurements, &target_samples, &colours, cancel)?;
        (fit, transfer::SkinReview::default())
    } else {
        let mut fit = transfer::fit_with_intent_checkpoint(
            &reference_measurements,
            &target_samples,
            similarity,
            input.quality,
            input.intent,
            || check_cancel(cancel),
        )?;
        let skin = transfer::review_skin(&mut fit.grade, &plain, &target_regions, monochrome);
        (fit, skin)
    };
    check_cancel(cancel)?;
    // Both intents can move tone across an unrelated scene (Palette by distribution, Natural by
    // the scene-prior estimate), so find a native-safe endpoint before offering it at 100%: a
    // match the panel offers should be one Apply accepts. The safeguards are identical to Apply,
    // and every attempted render shares one baseline.
    let native_fit = native::fit(
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
            allowance,
        },
    )?;
    // The renderer is the instrument: report what the stage drew, not what the fit predicted.
    let (rendered_before, rendered_after) = (native_fit.0, native_fit.1);
    check_cancel(cancel)?;
    baseline::verify_current(state, photo, &observed_hash)?;
    let fitting = diagnostics(
        input,
        &fit,
        Observations {
            native: Some(native_fit),
            before: rendered_before,
            after: rendered_after,
            skin,
            similarity,
            prior: prior.is_some(),
            samples: target_samples.len(),
            refused,
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
        measured_visible: None,
        fitting: Some(fitting),
        reference_monochrome: monochrome,
        grade_offsets: [0.0; 6],
        transfer: Some(TransferSession {
            prior,
            fitted: fit.grade,
            reference: reference_measurements,
            learned: None,
            allowance,
        }),
    };
    save_analysis(state, &session)
}

fn photo_project(state: &AppState, photo: &PhotoId) -> AuraResult<ProjectId> {
    let key = photo.to_db();
    let project: String = state.catalog().read(move |conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&key],
            |r| r.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("colour match photo", &e))
    })?;
    ProjectId::from_db(&project).map_err(|_| invalid("Invalid collection."))
}

/// What an analysis with a learned look works from.
struct Learning {
    baseline: Recipe,
    baseline_hash: String,
    observed_hash: String,
    prior: Option<ReferenceGrade>,
    regions: Vec<appearance::Region>,
    measurements: transfer::GradeStats,
    monochrome: bool,
    learned: super::learned::LearnedLook,
}

/// The recipe a learned look gives at `amount` of the photographer's edit. Zero is the
/// photograph exactly as it was; above it, any earlier reference look fades out as the learned
/// one fades in, so two looks never stack.
fn learned_recipe(
    baseline: &Recipe,
    prior: Option<&ReferenceGrade>,
    learned: &super::learned::LearnedLook,
    amount: f32,
) -> AuraResult<Recipe> {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return canonical(baseline);
    }
    let mut recipe = baseline.clone();
    recipe.extra.remove(grade_model::KEY);
    recipe.extra.remove(aura_recipe::calibrated_look::KEY);
    if let Some(prior) = prior {
        let faded = transfer::toward(&ReferenceGrade::identity(), prior, 1.0 - amount);
        grade_model::write(&mut recipe, &faded)?;
    }
    let mut model = learned.model.clone();
    model.amount = amount;
    aura_recipe::calibrated_look::write(&mut recipe, &model)?;
    schema::Validation::check(&recipe)?;
    canonical(&recipe)
}

/// Analyse with the look learned from the reference's original: find the strongest share of it
/// that passes this photograph's native checks, so 100% is an endpoint Apply accepts.
#[allow(clippy::too_many_lines)]
fn prepare_learned(
    state: &AppState,
    cancel: &CancelToken,
    input: &PrepareInput,
    photo: PhotoId,
    reference: Reference,
    learning: Learning,
) -> AuraResult<MatchReport> {
    let Learning {
        baseline,
        baseline_hash,
        observed_hash,
        prior,
        regions,
        measurements,
        monochrome,
        mut learned,
    } = learning;
    let review = native::Review::new(
        state,
        cancel,
        photo,
        &baseline,
        &regions,
        &measurements,
        monochrome,
        Some(learned.allowance),
    )?;
    let endpoint = review
        .fit_learned(state, cancel, 1.0, |amount| {
            learned_recipe(&baseline, prior.as_ref(), &learned, amount)
        })?
        .ok_or_else(|| {
            invalid(format!(
                "Your own edit of {} could not be applied to this photo without banding, extra clipping or changing skin more than that edit did. No edit was saved.",
                learned.label
            ))
        })?;
    learned.model.amount = endpoint.factor;
    check_cancel(cancel)?;
    baseline::verify_current(state, photo, &observed_hash)?;
    let fitting = FitDiagnostics {
        engine: LEARNED_ENGINE.into(),
        intent: input.intent,
        quality: input.quality,
        target_samples: 0,
        max_iterations: 0,
        refinement_candidates: 0,
        unique_renders: endpoint.renders,
        cached_proposals: 0,
        grade_candidates: 0,
        native_candidate_checked: true,
        native_backtrack_factor: endpoint.factor,
        structure_similarity: None,
        regularization_weight: 0.0,
        transfer: Some(TransferDiagnostics {
            content_similarity: 0.0,
            iterations: 0,
            predicted_before: review.before,
            predicted_after: endpoint.after,
            rendered_before: review.before,
            rendered_after: endpoint.after,
            skin: transfer::SkinReview::default(),
            replaced_earlier_match: prior.is_some(),
            tone_estimate: None,
            learned_from: Some(learned.label.clone()),
            original_refused: None,
            stretch_limited_from: None,
            highlights_limited_from: None,
        }),
    };
    drop(endpoint.recipe);
    let mut controls = reference.summary.default_controls.unwrap_or_default();
    controls.amount = controls.amount.min(1.0);
    let session = Session {
        engine: ENGINE.into(),
        id: uuid::Uuid::new_v4().to_string(),
        photo_id: input.photo_id.clone(),
        reference: reference.summary.clone(),
        baseline,
        baseline_hash,
        last_hash: observed_hash,
        fitted: StyleDelta::neutral(),
        reference_regions: reference.regions,
        target_regions: regions,
        before_distance: review.before.total,
        fitted_distance: endpoint.after.total,
        controls,
        checked_hash: None,
        measured_before: None,
        measured_after: None,
        measured_visible: None,
        fitting: Some(fitting),
        reference_monochrome: monochrome,
        grade_offsets: [0.0; 6],
        transfer: Some(TransferSession {
            prior,
            fitted: ReferenceGrade::identity(),
            reference: measurements,
            learned: Some(learned),
            allowance: None,
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
    if let Some(learned) = &transfer.learned {
        // Only the amount scales a learned edit: tone, colour and warmth have no separate part
        // in one pixel-for-pixel operation, and 100% is the photographer's own edit.
        let amount = controls.amount.clamp(0.0, 1.0) * learned.model.amount;
        return learned_recipe(&session.baseline, transfer.prior.as_ref(), learned, amount);
    }
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

/// The refusal when no strength passes the native checks.
const REFUSED: &str = "This strength failed the full-resolution smoothness, highlight or skin-colour check, or moved colour farther from the reference. Reduce the strengths and apply again. No edit was saved.";

/// Shares of a requested amount tried, strongest first, when the amount itself fails the native
/// checks. Those checks are not monotone in strength on every photograph - a clipped share can sit
/// on its limit at 80 % and below it at 90 % - so a refusal at one amount says little about the
/// amount just under it.
const BACKOFF: [f32; 3] = [0.75, 0.5, 0.25];

/// Native quality checks use the exact candidate, including extrapolated strength controls.
/// `None` when the candidate fails them.
fn verified_distances(
    state: &AppState,
    cancel: &CancelToken,
    session: &Session,
    transfer: &TransferSession,
    photo: &PhotoId,
    candidate: &Recipe,
) -> AuraResult<Option<(transfer::Distance, transfer::Distance, bool, f32)>> {
    let reference = &transfer.reference;
    let review = native::Review::new(
        state,
        cancel,
        *photo,
        &session.baseline,
        &session.target_regions,
        reference,
        session.reference_monochrome,
        transfer
            .learned
            .as_ref()
            .map(|learned| learned.allowance)
            .or(transfer.allowance),
    )?
    .with_regression_check(transfer.learned.is_none());
    Ok(review.check(state, cancel, candidate)?.map(|checked| {
        (
            review.before,
            checked.after,
            checked.reused,
            checked.visible,
        )
    }))
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
    // The amount asked for, then shares of it, until one passes; the panel says when the saved
    // amount is less than the one asked for.
    let mut controls = input.controls;
    let mut found = None;
    for share in std::iter::once(1.0).chain(BACKOFF) {
        if share < 1.0 && input.controls.amount <= 0.0 {
            break;
        }
        controls.amount = input.controls.amount * share;
        let trial = candidate(&session, &transfer, &controls)?;
        if let Some(distances) =
            verified_distances(state, cancel, &session, &transfer, &photo, &trial)?
        {
            found = Some((trial, distances));
            break;
        }
    }
    let (candidate, (before, after, reused, visible)) = found.ok_or_else(|| invalid(REFUSED))?;
    let reduced_from = (controls.amount < input.controls.amount).then_some(input.controls.amount);
    if let Some(fitting) = session.fitting.as_mut() {
        fitting.native_candidate_checked = true;
        fitting.cached_proposals += usize::from(reused);
        if let Some(transfer) = fitting.transfer.as_mut() {
            transfer.rendered_before = before;
            transfer.rendered_after = after;
        }
    }
    let mut merged = current.clone();
    // A match owns exactly its reference looks: the transfer grade and a learned operation.
    for key in [grade_model::KEY, aura_recipe::calibrated_look::KEY] {
        match candidate.extra.get(key) {
            Some(value) => {
                merged.extra.insert(key.into(), value.clone());
            }
            None => {
                merged.extra.remove(key);
            }
        }
    }
    // Canonical identity is the renderer/store contract; JSON number representations may
    // compare differently in memory even when their stored values and pixels are identical.
    let candidate_hash = aura_recipe::hash::recipe_hash(&merged)?;
    let changed: Vec<String> = if candidate_hash == hash {
        Vec::new()
    } else {
        [grade_model::KEY, aura_recipe::calibrated_look::KEY]
            .into_iter()
            .filter(|key| current.extra.get(*key) != merged.extra.get(*key))
            .map(|key| format!("extra.{key}"))
            .collect()
    };
    check_cancel(cancel)?;
    let project = photo_project(state, &photo)?;
    if !changed.is_empty() {
        let c = &controls;
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
    session.controls = controls;
    session.checked_hash = Some(session.last_hash.clone());
    session.measured_before = Some(before.total);
    session.measured_after = Some(after.total);
    session.measured_visible = Some(visible);
    session.transfer = Some(transfer);
    store(state, key, &session)?;
    let mut result = report(&session, changed.len(), 0, after.total, true, true);
    result.before_distance = before.total;
    result.visible_change = Some(visible);
    result.reduced_from = reduced_from;
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
