//! Native selection between scene-preserving and full-palette look transforms.
use std::collections::BTreeMap;

use aura_core::contract::style::StyleDelta;
use aura_core::AuraResult;
use aura_raw::codec::Rgb8;
use aura_recipe::{reference_grade, schema, EditSource, Recipe};

use super::super::{
    appearance, check_cancel, content, native_checks, rendered, skin_guard, transfer, MatchIntent,
    MatchQuality,
};
use super::correspondence::SpatialEvidence;
use super::{candidate, objective::Objective, reduced, AppState, Fit, ReferenceEvidence, Request};

// Authored stopping tolerance for a registered native pixel fit, in OKLab x100.
// Avoid full palette/slider searches after a checked result already meets this target.
const REGISTERED_TOLERANCE: f32 = 0.1;

fn canonical(request: &Request<'_>, proposal: &Recipe) -> AuraResult<Recipe> {
    let recipe = schema::merge(request.current, proposal, EditSource::Ai)?.0;
    schema::Validation::check(&recipe)?;
    serde_json::from_str(&aura_recipe::canonical(&recipe)?)
        .map_err(|_| super::super::invalid("The adaptive reference recipe could not be encoded."))
}

fn measured(reference: &transfer::GradeStats, image: &Rgb8) -> transfer::Distance {
    transfer::distance(reference, &transfer::measure_image(image))
}

struct NativeReview<'a> {
    reference: &'a transfer::GradeStats,
    regions: Vec<appearance::Region>,
    clipped: f32,
    contours: super::super::contours::Contours,
    before: transfer::Distance,
    objective: &'a Objective<'a>,
    spatial: Option<&'a SpatialEvidence>,
    aligned_before: f32,
}

impl NativeReview<'_> {
    fn score(&self, image: &Rgb8, monochrome: bool) -> Option<f32> {
        if appearance::clipped(image) > self.clipped + 0.01
            || !self.contours.accepts(image)
            || !skin_guard(
                &self.regions,
                &appearance::readings(image, &self.regions),
                monochrome,
            )
        {
            return None;
        }
        let score = if let Some(spatial) = self.spatial {
            spatial.distance(image)
        } else if self.objective.corresponding() {
            self.objective.distance(image)
        } else {
            let after = measured(self.reference, image);
            if !native_checks::non_regressing(self.before, after) {
                return None;
            }
            after.total
        };
        (score.is_finite() && score + 1e-4 < self.aligned_before).then_some(score)
    }
}

fn qualified_endpoint(
    state: &AppState,
    request: &Request<'_>,
    review: &NativeReview<'_>,
    full: Recipe,
    requested: Recipe,
) -> AuraResult<Option<(Recipe, f32)>> {
    check_cancel(request.cancel)?;
    let score = {
        let image = rendered(state, request.full, &full)?;
        check_cancel(request.cancel)?;
        review.score(&image, request.monochrome)
    };
    let Some(score) = score else {
        return Ok(None);
    };
    if full == requested {
        return Ok(Some((requested, score)));
    }
    // A shared safe full endpoint makes 50% mean half of that fit, rather than
    // independently backtracking 100% and 50% onto the same saved operation.
    let image = rendered(state, request.full, &requested)?;
    check_cancel(request.cancel)?;
    Ok(review
        .score(&image, request.monochrome)
        .map(|score| (requested, score)))
}

#[allow(clippy::too_many_lines)]
pub(super) fn fit(
    state: &AppState,
    request: &Request<'_>,
    evidence: &ReferenceEvidence,
) -> AuraResult<Fit> {
    evidence.validate()?;
    check_cancel(request.cancel)?;
    let mut reference = evidence
        .select(request.lighting, request.lighting_confidence)
        .clone();
    let baseline = candidate(request, &StyleDelta::neutral(), &[0.0; 6], 0.0)?;
    let small_frame = reduced(request.full)?;
    let small = rendered(state, &small_frame, &baseline)?;
    let objective = Objective::new(request.target, request.thumbnail, &small);
    let native = rendered(state, request.full, &baseline)?;
    check_cancel(request.cancel)?;
    let spatial = (!request.thumbnail.is_empty())
        .then(|| evidence.corresponding(&native))
        .flatten();
    // An unrelated scene is matched as a look, not as a colour distribution. Scoring it by how
    // closely its colour proportions match the reference's rewards repainting the scene - a blue
    // sky turned the colour of the reference's fruit, a white floor the green of its forest - and
    // measured worse at reproducing a known look than leaving the photograph alone (ADR-0180,
    // deep assessment 2026-10-25). Only a registered or corresponding scene, where the reference
    // is the same picture, is scored by its whole palette.
    let unrelated = spatial.is_none() && !objective.corresponding();
    reference.palette_matching = !unrelated;
    let before = measured(&reference, &native);
    let aligned_before = if let Some(spatial) = spatial {
        spatial.distance(&native)
    } else if objective.corresponding() {
        objective.distance(&native)
    } else {
        before.total
    };
    let review = NativeReview {
        reference: &reference,
        regions: appearance::analyse(&native)?,
        clipped: appearance::clipped(&native),
        contours: super::super::contours::Contours::new(&native),
        before,
        objective: &objective,
        spatial,
        aligned_before,
    };
    let mut best = baseline.clone();
    let mut best_score = aligned_before;
    let aligned = spatial.is_some() || objective.corresponding();
    if request.strength > 0.0 {
        if let Some(spatial) = spatial {
            if let Some(model) = spatial.affine(&native, request.cancel)? {
                for factor in [1.0, 0.5, 0.25, 0.125] {
                    check_cancel(request.cancel)?;
                    let mut selected = model.clone();
                    selected.amount = factor;
                    let mut proposal = baseline.clone();
                    aura_recipe::calibrated_look::write(&mut proposal, &selected)?;
                    let proposal = canonical(request, &proposal)?;
                    selected.amount *= request.strength;
                    let mut requested = baseline.clone();
                    aura_recipe::calibrated_look::write(&mut requested, &selected)?;
                    let requested = canonical(request, &requested)?;
                    if let Some((recipe, score)) =
                        qualified_endpoint(state, request, &review, proposal, requested)?
                    {
                        best = recipe;
                        best_score = score;
                        break;
                    }
                }
            }
        }
    }
    if request.strength > 0.0 && request.monochrome {
        let legacy = super::fit_legacy(state, request)?;
        let native = rendered(state, request.full, &legacy.recipe)?;
        check_cancel(request.cancel)?;
        if let Some(score) = review.score(&native, true) {
            if score + 1e-4 < best_score {
                best = legacy.recipe;
                best_score = score;
            }
        }
    }
    if request.strength > 0.0
        && before.total > 1e-4
        && best_score > 1e-4
        && !(aligned && best_score <= REGISTERED_TOLERANCE)
    {
        let quality = MatchQuality::Precise;
        // Fit colour distributions from the actual native render. Area-reduced images are
        // useful for structure but blend striped fabric into colours the photo never had.
        let samples = transfer::samples(&native, quality.target_samples());
        // An unrelated composition never supplies corresponding pixels to the fitter.
        let similarity = if spatial.is_some() {
            Some(1.0)
        } else {
            objective
                .corresponding()
                .then(|| content::similarity(request.thumbnail, &small))
                .flatten()
        };
        let mut seen = BTreeMap::new();
        let intents: &[MatchIntent] = if unrelated {
            &[MatchIntent::Natural]
        } else {
            &[MatchIntent::Natural, MatchIntent::Palette]
        };
        for &intent in intents {
            if aligned && best_score <= REGISTERED_TOLERANCE {
                break;
            }
            check_cancel(request.cancel)?;
            let mut fit = transfer::fit_with_intent_checkpoint(
                &reference,
                &samples,
                similarity,
                quality,
                intent,
                || check_cancel(request.cancel),
            )?;
            transfer::review_skin(&mut fit.grade, &native, &review.regions, request.monochrome);
            for factor in [1.0, 0.5, 0.25, 0.125] {
                check_cancel(request.cancel)?;
                let grade = transfer::toward(
                    &reference_grade::ReferenceGrade::identity(),
                    &fit.grade,
                    factor,
                );
                let mut proposal = baseline.clone();
                reference_grade::write(&mut proposal, &grade)?;
                let proposal = canonical(request, &proposal)?;
                let hash = aura_recipe::hash::recipe_hash(&proposal)?;
                if let Some(&accepted) = seen.get(&hash) {
                    if accepted {
                        break;
                    }
                    continue;
                }
                seen.insert(hash.clone(), false);
                check_cancel(request.cancel)?;
                let grade = transfer::toward(
                    &reference_grade::ReferenceGrade::identity(),
                    &fit.grade,
                    request.strength * factor,
                );
                let mut requested = baseline.clone();
                reference_grade::write(&mut requested, &grade)?;
                let requested = canonical(request, &requested)?;
                if let Some((recipe, score)) =
                    qualified_endpoint(state, request, &review, proposal, requested)?
                {
                    seen.insert(hash, true);
                    if score + 1e-4 < best_score {
                        best = recipe;
                        best_score = score;
                    }
                    // Keep the strongest safe endpoint for this strategy. Strength is a
                    // user choice, not another variable to optimize away on this photo.
                    break;
                }
            }
        }
    }
    if request.strength > 0.0
        && !request.monochrome
        && (best == baseline || (aligned && best_score > REGISTERED_TOLERANCE))
    {
        let legacy = super::fit_legacy(state, request)?;
        let native = rendered(state, request.full, &legacy.recipe)?;
        check_cancel(request.cancel)?;
        if let Some(score) = review.score(&native, false) {
            if score + 1e-4 < best_score {
                best = legacy.recipe;
                best_score = score;
            }
        }
    }
    check_cancel(request.cancel)?;
    best.provenance.source = EditSource::Ai;
    best.provenance.confidence = 0.4;
    let (recipe, report) = schema::merge(request.current, &best, EditSource::Ai)?;
    Ok(Fit {
        recipe,
        changed: report.changed,
        before: aligned_before,
        after: best_score,
    })
}
