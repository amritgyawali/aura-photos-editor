use std::collections::BTreeMap;

use aura_core::{contract::style::StyleDelta, progress::CancelToken, AuraResult, PhotoId};
use aura_look::solve;
use aura_recipe::{
    local_masks::{self, Component, LocalMask, Mode, Source},
    MaskParams, Recipe,
};
use aura_render::{FrameSource, RenderLevel};

use super::super::{asset_key, check_cancel, fitting_frame, Reference};
use super::{
    appearance, current_hash, invalid, read, rendered, selections, Controls, FittedRegion, Kind,
    PrepareInput, Session, Unavailable, ENGINE,
};
use crate::AppState;

fn supported(mut delta: StyleDelta) -> StyleDelta {
    // Local masks have no tone-curve, vibrance or HSL operator. Never silently pretend those
    // unsupported global axes are regional corrections.
    delta.curve_shift = aura_core::contract::style::CurveShift::default();
    delta.vibrance = 0.0;
    delta.hsl = aura_core::contract::colour::HslAdjustments::default();
    delta
}

fn params(delta: &StyleDelta, controls: Controls) -> MaskParams {
    let tone = controls.amount * controls.tone;
    let color = controls.amount * controls.color;
    let white = controls.amount * controls.white_balance;
    let percentage = |value: f32| Some(value.round().clamp(-100.0, 100.0) as i16);
    MaskParams {
        exposure: Some((delta.exposure * tone).clamp(-5.0, 5.0)),
        contrast: percentage(delta.contrast * tone),
        highlights: percentage(delta.highlights * tone),
        shadows: percentage(delta.shadows * tone),
        whites: percentage(delta.whites * tone),
        blacks: percentage(delta.blacks * tone),
        saturation: percentage(delta.saturation * color),
        temperature: Some((delta.temperature_k * white).round().clamp(-5000.0, 5000.0) as i32),
        tint: percentage(delta.tint * white),
        ..MaskParams::default()
    }
}

pub(super) fn candidate(
    session: &Session,
    strengths: &BTreeMap<Kind, Controls>,
) -> AuraResult<Recipe> {
    let mut recipe = session.baseline.clone();
    let mut masks = local_masks::read(&recipe)?;
    let mut mattes = local_masks::read_mattes(&recipe)?;
    let mut added = false;
    // The disposable analysis handle must not make the saved recipe nondeterministic.
    let identity = blake3::hash(
        format!(
            "{ENGINE}|{}|{}",
            session.reference.id, session.baseline_hash
        )
        .as_bytes(),
    )
    .to_hex();
    for region in &session.regions {
        let controls = strengths
            .get(&region.kind)
            .copied()
            .ok_or_else(|| invalid("Missing strengths for a supported region."))?;
        controls.check()?;
        if controls.amount == 0.0 || region.fitted.is_neutral() {
            continue;
        }
        selections::validate(&region.matte)?;
        let id = format!("reference-{identity}-{}", region.kind.id());
        if mattes.contains_key(&id) || masks.iter().any(|mask| mask.id == id) {
            return Err(invalid(
                "A regional selection conflicts with an existing mask. Analyse again.",
            ));
        }
        mattes.insert(id.clone(), region.matte.clone());
        masks.push(LocalMask {
            id: id.clone(),
            name: format!("Reference: {}", region.kind.name()),
            enabled: true,
            amount: 1.0,
            components: vec![Component {
                mode: Mode::Add,
                invert: false,
                source: Source::Matte {
                    matte: id,
                    what: region.kind.id().into(),
                },
            }],
            params: params(&region.fitted, controls),
        });
        added = true;
    }
    // Zero is an exact recipe restoration, including absent extension keys and unused mattes.
    if added {
        local_masks::validate(&masks, &mattes)?;
        if mattes.len() > local_masks::MAX_MATTES {
            return Err(invalid(
                "Too many selections. Remove a local mask before matching.",
            ));
        }
        recipe.extra.insert(
            local_masks::KEY.into(),
            serde_json::to_value(&masks).map_err(|_| invalid("Cannot save regional masks."))?,
        );
        recipe.extra.insert(
            local_masks::MATTE_KEY.into(),
            serde_json::to_value(&mattes)
                .map_err(|_| invalid("Cannot save regional selections."))?,
        );
    }
    aura_recipe::schema::Validation::check(&recipe)?;
    Ok(recipe)
}

fn distance(
    reference: &[appearance::Region],
    readings: &[appearance::Region],
    kind: Kind,
) -> AuraResult<f32> {
    let target = readings
        .iter()
        .find(|r| r.name == kind.name())
        .ok_or_else(|| invalid("The selected region could not be measured. No edit was saved."))?;
    let reference = reference
        .iter()
        .find(|r| r.name == kind.name())
        .ok_or_else(|| invalid("The reference has no matching semantic region."))?;
    let value = solve::distance(&reference.appearance, &target.appearance);
    if !value.is_finite() {
        return Err(invalid("Regional colour measurement was unavailable."));
    }
    Ok(value)
}

pub(super) fn prepare(
    state: &AppState,
    input: &PrepareInput,
    cancel: &CancelToken,
) -> AuraResult<Session> {
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
    let reference: Reference = read(state, &asset_key(&input.reference_id)?)?
        .filter(|r: &Reference| r.engine == super::super::ENGINE)
        .ok_or_else(|| invalid("Choose a saved reference image first."))?;
    let baseline = state
        .recipe_store()
        .load(&photo)?
        .ok_or_else(|| invalid("Save an initial correction before regional matching."))?;
    let hash = aura_recipe::hash::recipe_hash(&baseline)?;
    let frame = fitting_frame(state, &photo)?;
    let before = rendered(state, &frame, &baseline)?;
    let target_regions = appearance::analyse(&before)?;
    // Native local masks run before perspective/crop/rotation, after lens and portrait
    // corrections. Segment that coordinate system while measuring the real cropped output.
    let mut untransformed = baseline.clone();
    untransformed.geometry = aura_recipe::Geometry::default();
    let mask_image = if baseline.geometry.is_identity() {
        before.clone()
    } else {
        rendered(state, &frame, &untransformed)?
    };
    let mattes = selections::analyse(&mask_image)?;
    check_cancel(cancel)?;
    let mut session = Session {
        engine: ENGINE.into(),
        id: uuid::Uuid::new_v4().to_string(),
        photo_id: input.photo_id.clone(),
        reference: reference.summary,
        baseline,
        baseline_hash: hash.clone(),
        last_hash: hash,
        regions: Vec::new(),
        unavailable: Vec::new(),
        reference_regions: reference.regions,
        target_regions,
        checked_hash: None,
    };
    for kind in Kind::ALL {
        let region = session
            .target_regions
            .iter()
            .find(|r| r.name == kind.name());
        let reference = session
            .reference_regions
            .iter()
            .find(|r| r.name == kind.name());
        let (Some(target), Some(reference), Some(matte)) = (region, reference, mattes.get(&kind))
        else {
            session.unavailable.push(Unavailable {
                name: kind,
                reason: "A confident matching region was not found in both photos.".into(),
            });
            continue;
        };
        let before_distance = solve::distance(&reference.appearance, &target.appearance);
        if !before_distance.is_finite() {
            continue;
        }
        let seed = supported(solve::initial(&reference.appearance, &target.appearance));
        session.regions.push(FittedRegion {
            kind,
            matte: matte.clone(),
            fitted: StyleDelta::neutral(),
            controls: Controls::default(),
            before: before_distance,
            after: before_distance,
        });
        fit_region(state, &frame, &before, &mut session, kind, &seed, cancel)?;
    }
    if session.regions.is_empty() {
        return Err(invalid("No confident matching semantic region was found in both photos. Try whole-image matching or another reference."));
    }
    if current_hash(state, &input.photo_id)? != session.baseline_hash {
        return Err(invalid(
            "The photo changed during regional analysis. Your newer edit was kept. Analyse again.",
        ));
    }
    Ok(session)
}

fn fit_region(
    state: &AppState,
    frame: &aura_render::Frame,
    before: &aura_raw::codec::Rgb8,
    session: &mut Session,
    kind: Kind,
    seed: &StyleDelta,
    cancel: &CancelToken,
) -> AuraResult<()> {
    let mut best = StyleDelta::neutral();
    let mut best_distance = session
        .regions
        .iter()
        .find(|r| r.kind == kind)
        .ok_or_else(|| invalid("Missing regional fit."))?
        .before;
    let strengths: BTreeMap<_, _> = session
        .regions
        .iter()
        .map(|r| {
            (
                r.kind,
                Controls {
                    amount: if r.kind == kind { 1.0 } else { 0.0 },
                    ..Controls::default()
                },
            )
        })
        .collect();
    let clipped = appearance::clipped(before);
    let mut proposals: Vec<StyleDelta> = [0.25, 0.5, 0.75, 1.0]
        .iter()
        .map(|v| seed.scaled(*v))
        .collect();
    for pass in 0..10 {
        if pass > 0 {
            proposals = [0.0, 0.5, 1.0]
                .into_iter()
                .map(|factor| {
                    let mut value = best.clone();
                    // The supported local axes share the existing bounded, independent seed fitter.
                    let axis = match pass {
                        1 => 0,
                        2 => 1,
                        3 => 2,
                        4 => 3,
                        5 => 4,
                        6 => 5,
                        7 => 6,
                        8 => 7,
                        _ => 9,
                    };
                    super::super::set_axis(&mut value, seed, axis, factor);
                    value
                })
                .collect();
        }
        for value in &proposals {
            check_cancel(cancel)?;
            let region = session
                .regions
                .iter_mut()
                .find(|r| r.kind == kind)
                .ok_or_else(|| invalid("Missing regional fit."))?;
            region.fitted = value.clone();
            let output = rendered(state, frame, &candidate(session, &strengths)?)?;
            let readings = appearance::readings(&output, &session.target_regions);
            if appearance::clipped(&output) > clipped + 0.01
                || !appearance::skin_preserved(&session.target_regions, &readings)
            {
                continue;
            }
            let measured = distance(&session.reference_regions, &readings, kind)?;
            if measured + 1e-4 < best_distance {
                best = value.clone();
                best_distance = measured;
            }
        }
    }
    let region = session
        .regions
        .iter_mut()
        .find(|r| r.kind == kind)
        .ok_or_else(|| invalid("Missing regional fit."))?;
    region.fitted = best;
    region.after = best_distance;
    Ok(())
}

pub(super) fn full_check(
    state: &AppState,
    session: &Session,
    candidate: &Recipe,
    cancel: &CancelToken,
) -> AuraResult<BTreeMap<Kind, (f32, f32)>> {
    let photo = PhotoId::from_db(&session.photo_id).map_err(|_| invalid("Invalid photo."))?;
    let frame =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(&photo, RenderLevel::Full)?;
    check_cancel(cancel)?;
    let before = rendered(state, &frame, &session.baseline)?;
    let before_readings = appearance::readings(&before, &session.target_regions);
    let clipped = appearance::clipped(&before);
    drop(before);
    check_cancel(cancel)?;
    let after = rendered(state, &frame, candidate)?;
    let after_readings = appearance::readings(&after, &session.target_regions);
    if appearance::clipped(&after) > clipped + 0.01
        || !appearance::skin_preserved(&before_readings, &after_readings)
    {
        return Err(invalid("Regional strength failed the native-resolution highlight or per-person skin-colour check. Reduce strengths; no edit was saved."));
    }
    session.regions.iter().map(|r| {
        let before = distance(&session.reference_regions, &before_readings, r.kind)?;
        let after = distance(&session.reference_regions, &after_readings, r.kind)?;
        if after > before + 0.25 {
            return Err(invalid(format!("The {} correction moved farther from the reference at native resolution. Reduce strengths; no edit was saved.", r.kind.name())));
        }
        Ok((r.kind, (before, after)))
    }).collect()
}
