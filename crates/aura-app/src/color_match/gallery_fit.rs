//! Native, per-photo fitting for the reference-first Match a look screen.
use std::collections::BTreeMap;

use aura_core::contract::look::LookAggregate;
use aura_core::contract::style::StyleDelta;
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use aura_raw::codec::Rgb8;
use aura_recipe::{schema, EditSource, Recipe};
use aura_render::Frame;
use serde::{Deserialize, Serialize};

use super::calibration;
use super::{appearance, check_cancel, content, grade, refine, rendered, skin_guard, AppState};

mod adaptive;
mod correspondence;
mod evidence;
mod objective;
pub(crate) use evidence::{EvidenceBuilder, ReferenceEvidence};

pub(crate) fn calibrate(
    state: &AppState,
    original_path: &std::path::Path,
    edited_path: &std::path::Path,
    cancel: &CancelToken,
) -> AuraResult<(
    aura_recipe::calibrated_look::CalibratedLook,
    super::CalibrationReport,
)> {
    check_cancel(cancel)?;
    let original = super::reference_image::decode_native(original_path)?;
    let edited = super::reference_image::decode_native(edited_path)?;
    if (original.width, original.height) != (edited.width, edited.height) {
        return Err(super::invalid(
            "The original and edited reference must have identical dimensions and framing.",
        ));
    }
    // Learn the edit beyond the same automatic baseline that every batch target receives.
    // Otherwise the target's auto exposure would be counted twice by the learned operation.
    let frame = Frame::working(
        crate::photo_frames::srgb8_to_working(&original.image.data),
        original.image.width,
        original.image.height,
        "",
    );
    let neutral = aura_recipe::fixtures::neutral(&original.hash, "");
    let source = rendered(state, &reduced(&frame)?, &neutral)?;
    let baseline = crate::edit_profiles::build(
        &neutral,
        None,
        1.0,
        crate::edit_profiles::AutoCorrection::measure(&source.data)?,
        crate::edit_profiles::SceneStats::measure(&source.data)?,
    )?
    .0;
    check_cancel(cancel)?;
    let baseline = rendered(state, &frame, &baseline)?;
    calibration::fit(&baseline, &edited.image, cancel)
}

/// How much a calibrated look may change people's skin and the clipped area. The defaults are the
/// pilot limits every estimate is held to. A look learned from the photographer's own edit of a
/// frame in the collection may move them as far as that edit moved them on its original, because
/// the change is the photographer's choice rather than an inference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Allowance {
    /// The largest skin change, in multiples of the pilot limits.
    pub skin: f32,
    /// The largest increase in the fraction of clipped pixels.
    pub clipping: f32,
}

impl Default for Allowance {
    fn default() -> Self {
        Self {
            skin: 1.0,
            clipping: 0.01,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Request<'a> {
    pub full: &'a Frame,
    pub baseline: &'a Recipe,
    pub current: &'a Recipe,
    pub target: &'a LookAggregate,
    pub evidence: Option<&'a ReferenceEvidence>,
    pub lighting: aura_core::contract::style::LightingBucket,
    pub lighting_confidence: f32,
    pub thumbnail: &'a [u8],
    pub monochrome: bool,
    pub strength: f32,
    pub cancel: &'a CancelToken,
    /// Applies to a calibrated look only; parameter fits keep the pilot limits.
    pub allowance: Allowance,
}

#[derive(Debug)]
pub(crate) struct Fit {
    pub recipe: Recipe,
    pub changed: Vec<String>,
    pub before: f32,
    pub after: f32,
}

/// The fitting domain is the native decode, including RAW camera colour, rather than an
/// embedded camera JPEG. Only this reduced working-space frame is cloned for the search.
pub(crate) fn reduced(full: &Frame) -> AuraResult<Frame> {
    super::reduction::reduce(full, 256)
}

fn candidate(
    request: &Request<'_>,
    delta: &StyleDelta,
    wheels: &[f32; 6],
    amount: f32,
) -> AuraResult<Recipe> {
    let mut proposal = aura_look::verify::shift(request.baseline, &delta.scaled(amount));
    grade::apply(&mut proposal.global.colour_grade, wheels, amount);
    if request.monochrome {
        let original = f32::from(request.baseline.global.saturation);
        proposal.global.saturation = (original + amount * (-100.0 - original)).round() as i16;
        let grading = &mut proposal.global.colour_grade;
        for wheel in [
            &mut grading.shadows,
            &mut grading.midtones,
            &mut grading.highlights,
            &mut grading.global,
        ] {
            wheel.saturation = (f32::from(wheel.saturation) * (1.0 - amount)).round() as i16;
        }
        if amount >= 1.0 {
            if let Some(bw) = &mut proposal.bw {
                bw.grade = None;
            }
        }
    }
    let merged = schema::merge(request.current, &proposal, EditSource::Ai)?.0;
    schema::Validation::check(&merged)?;
    // Search, preview and history must use the exact precision written to the recipe store.
    // Otherwise a sub-millionth exposure difference creates a new history step on every run.
    serde_json::from_str(&aura_recipe::canonical(&merged)?)
        .map_err(|_| super::invalid("The fitted reference recipe could not be encoded."))
}

fn wheel_effort(wheels: &[f32; 6]) -> f32 {
    wheels
        .chunks_exact(2)
        .filter_map(|pair| match pair {
            &[x, y] => Some(x.hypot(y) / grade::MAX_CAST),
            _ => None,
        })
        .sum()
}

/// Fit at full look strength first, then apply the photographer's strength. Optimising an
/// already-scaled proposal would let the optimiser undo the meaning of the strength slider.
#[allow(clippy::too_many_lines)]
pub(crate) fn fit(state: &AppState, request: &Request<'_>) -> AuraResult<Fit> {
    if let Some(evidence) = request.evidence {
        return adaptive::fit(state, request, evidence);
    }
    fit_legacy(state, request)
}

#[allow(clippy::too_many_lines)]
fn fit_legacy(state: &AppState, request: &Request<'_>) -> AuraResult<Fit> {
    check_cancel(request.cancel)?;
    let small = reduced(request.full)?;
    let baseline = candidate(request, &StyleDelta::neutral(), &[0.0; 6], 0.0)?;
    let before = rendered(state, &small, &baseline)?;
    let regions = appearance::analyse(&before)?;
    let clipped = appearance::clipped(&before);
    let reading = aura_look::measure::read("reference-baseline", &before);
    let measured = aura_look::aggregate::fold(&[&reading]);
    let similarity = content::similarity(request.thumbnail, &before);
    let objective = objective::Objective::new(request.target, request.thumbnail, &before);
    let paired = objective.corresponding();
    let regularization = if paired {
        0.0
    } else {
        content::regularization(similarity)
    };
    let neutral = StyleDelta::neutral();
    let supported = std::array::from_fn(|index| {
        measured
            .bands
            .get(index)
            .zip(request.target.bands.get(index))
            .is_some_and(|(a, b)| {
                a.share >= aura_look::solve::BAND_MIN_SHARE
                    && b.share >= aura_look::solve::BAND_MIN_SHARE
            })
    });
    let before_score = objective.distance(&before);
    let mut scores = BTreeMap::new();
    let mut evaluate = |delta: &StyleDelta, wheels: &[f32; 6]| -> AuraResult<f32> {
        check_cancel(request.cancel)?;
        let proposal = candidate(request, delta, wheels, 1.0)?;
        let hash = aura_recipe::hash::recipe_hash(&proposal)?;
        let raw = if let Some(&score) = scores.get(&hash) {
            score
        } else {
            let image = rendered(state, &small, &proposal)?;
            let after = appearance::readings(&image, &regions);
            let score = if appearance::clipped(&image) > clipped + 0.01
                || !skin_guard(&regions, &after, request.monochrome)
            {
                f32::INFINITY
            } else {
                objective.distance(&image)
            };
            scores.insert(hash, score);
            score
        };
        Ok(raw + regularization * (refine::effort(delta, &neutral) + wheel_effort(wheels)))
    };
    let mut fitted = neutral.clone();
    let mut score = before_score;
    if request.strength > 0.0 {
        // Monochrome intent changes the neutral proposal too. Score it before the search
        // so an already exact desaturation is not replaced by a needless tone correction.
        score = evaluate(&neutral, &[0.0; 6])?.min(score);
        if score > 1e-4 {
            let seed = aura_look::solve::initial(request.target, &measured);
            let seeded = evaluate(&seed, &[0.0; 6])?;
            if seeded < score {
                fitted = seed;
                score = seeded;
            }
            let refined = refine::search(&fitted, score, supported, |delta| {
                evaluate(delta, &[0.0; 6])
            })?;
            fitted = refined.delta;
            score = refined.distance;
        }
    }
    let mut wheels = if request.strength > 0.0 && !request.monochrome {
        let (wheels, next_score, _) = grade::search(score, |wheels| evaluate(&fitted, wheels))?;
        score = next_score;
        wheels
    } else {
        [0.0; 6]
    };
    // WB, tone, saturation and split toning are coupled. Revisit the ordinary controls with
    // the fitted casts present, then refine those casts around their incumbent. A single
    // sequential pass cannot undo a WB correction that was compensating for missing toning.
    if request.strength > 0.0 && !request.monochrome && score > 1e-4 {
        let joint = refine::search(&fitted, score, supported, |delta| evaluate(delta, &wheels))?;
        if joint.distance + 1e-4 < score {
            fitted = joint.delta;
            wheels = grade::search_from(&wheels, joint.distance, |offsets| {
                evaluate(&fitted, offsets)
            })?
            .0;
        }
    }
    check_cancel(request.cancel)?;

    // Measure the *merged* recipe at native resolution. Full-resolution safeguards and the
    // same actual objective decide acceptance, including small faces/highlights missed above.
    let image = rendered(state, request.full, &baseline)?;
    let native_regions = appearance::analyse(&image)?;
    let contours = super::contours::Contours::new(&image);
    let native_clip = appearance::clipped(&image);
    let native_before = objective.distance(&image);
    drop(image);
    let mut best = baseline;
    let mut native_after = native_before;
    for factor in [1.0, 0.66, 0.33] {
        if request.strength <= 0.0 {
            break;
        }
        check_cancel(request.cancel)?;
        let proposal = candidate(request, &fitted, &wheels, request.strength * factor)?;
        if proposal == best {
            continue;
        }
        let image = rendered(state, request.full, &proposal)?;
        let after = appearance::readings(&image, &native_regions);
        let measured = objective.distance(&image);
        if measured.is_finite()
            && measured + 1e-4 < native_after
            && appearance::clipped(&image) <= native_clip + 0.01
            && skin_guard(&native_regions, &after, request.monochrome)
            && contours.accepts(&image)
        {
            native_after = measured;
            best = proposal;
            if measured <= 1e-4 {
                break;
            }
        }
    }
    check_cancel(request.cancel)?;
    // Keep the final provenance alongside the canonical values already used by the renderer.
    best.provenance.source = EditSource::Ai;
    best.provenance.confidence = 0.4;
    let (recipe, report) = schema::merge(request.current, &best, EditSource::Ai)?;
    Ok(Fit {
        recipe,
        changed: report.changed,
        before: native_before,
        after: native_after,
    })
}

pub(crate) fn decode(path: &std::path::Path) -> AuraResult<Rgb8> {
    Ok(super::reference_image::decode_native(path)?.image)
}

pub(crate) fn thumbnail(image: &Rgb8) -> Vec<u8> {
    content::thumbnail(image)
}
pub(crate) fn monochrome(image: &Rgb8) -> bool {
    content::achromatic(image)
}
pub(crate) fn render(state: &AppState, frame: &Frame, recipe: &Recipe) -> AuraResult<Rgb8> {
    rendered(state, frame, recipe)
}

/// Compare corresponding target pixels to the *learned operation*, not the reference scene's
/// histogram. A green target can remain green when the calibration photo contains a blue sky.
fn calibrated_distance(
    baseline: &Rgb8,
    image: &Rgb8,
    evaluator: &aura_render::calibrated_look::Evaluator,
) -> f32 {
    if image.width != baseline.width
        || image.height != baseline.height
        || image.data.len() != baseline.data.len()
    {
        return f32::INFINITY;
    }
    let count = baseline.data.len() / 3;
    let mut total = 0.0;
    let mut measured = 0;
    for i in calibration::sample_indices(count, 40_000) {
        let read = |bytes: &[u8]| {
            std::array::from_fn(|c| f32::from(bytes.get(3 * i + c).copied().unwrap_or(0)) / 255.0)
        };
        total += calibration::pixel_error(
            read(&image.data),
            evaluator.encoded_srgb(read(&baseline.data)),
        );
        measured += 1;
    }
    if measured == 0 {
        f32::INFINITY
    } else {
        total / measured as f32
    }
}

/// Apply a single calibrated colour mapping consistently across scenes; target-specific
/// quality checks may attenuate it. Explicit strengths never trigger a histogram refit.
pub(crate) fn apply_calibrated(
    state: &AppState,
    request: &Request<'_>,
    model: &aura_recipe::calibrated_look::CalibratedLook,
) -> AuraResult<Fit> {
    check_cancel(request.cancel)?;
    let baseline = candidate(request, &StyleDelta::neutral(), &[0.0; 6], 0.0)?;
    let native = rendered(state, request.full, &baseline)?;
    check_cancel(request.cancel)?;
    let before_regions = appearance::analyse(&native)?;
    let contours = super::contours::Contours::new(&native);
    let clipped = appearance::clipped(&native);
    let mut full_model = model.clone();
    full_model.amount = 1.0;
    let evaluator = aura_render::calibrated_look::Evaluator::new(&full_model);
    let before = calibrated_distance(&native, &native, &evaluator);
    let mut after = before;
    let mut best = baseline.clone();
    for factor in [1.0, 0.75, 0.5, 0.25] {
        if request.strength <= 0.0 {
            break;
        }
        check_cancel(request.cancel)?;
        let mut proposal = baseline.clone();
        let mut selected = model.clone();
        selected.amount = request.strength * factor;
        aura_recipe::calibrated_look::write(&mut proposal, &selected)?;
        let proposal = schema::merge(request.current, &proposal, EditSource::Ai)?.0;
        let proposal: Recipe = serde_json::from_str(&aura_recipe::canonical(&proposal)?)
            .map_err(|_| super::invalid("Cannot encode the calibrated reference recipe."))?;
        schema::Validation::check(&proposal)?;
        let image = rendered(state, request.full, &proposal)?;
        let distance = calibrated_distance(&native, &image, &evaluator);
        let readings = appearance::readings(&image, &before_regions);
        // A look learned from the photographer's own edit may move skin and clipping as far as
        // that edit did on its original (ADR-0161); everything else keeps the pilot limits.
        let skin_ok = if request.monochrome {
            skin_guard(&before_regions, &readings, true)
        } else {
            appearance::skin_change(&before_regions, &readings) <= request.allowance.skin
        };
        if distance.is_finite()
            && distance + 1e-4 < after
            && appearance::clipped(&image) <= clipped + request.allowance.clipping
            && skin_ok
            && contours.accepts(&image)
        {
            best = proposal;
            after = distance;
            // The requested strength passed at native resolution. Keep that explicit amount.
            break;
        }
    }
    check_cancel(request.cancel)?;
    best.provenance.source = EditSource::Ai;
    best.provenance.confidence = 0.4;
    let (recipe, report) = schema::merge(request.current, &best, EditSource::Ai)?;
    Ok(Fit {
        recipe,
        changed: report.changed,
        before,
        after,
    })
}
