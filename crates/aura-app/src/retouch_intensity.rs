//! Sliders over what the automatic retouch did: one strength per kind of retouch, set after
//! the pass and applied by the renderer. ADR-0130.
use std::collections::BTreeMap;

use aura_core::{PhotoId, ProjectId};
use aura_recipe::retouch_intensity::{self, Category};
use aura_recipe::{retouch_tools, schema, EditSource, Recipe};
use serde::{Deserialize, Serialize};

use crate::{commands::IpcResult, AppState};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntensityInput {
    pub project_id: String,
    pub photo_id: String,
    /// `get`, `set` (the given factors, others unchanged), `reset` (every slider to 100%) or
    /// `sync` (this photo's sliders onto every other retouched photo in the collection).
    pub action: String,
    #[serde(default)]
    pub factors: BTreeMap<Category, f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Slider {
    pub category: Category,
    /// Automatic operations this slider scales on this photo.
    pub operations: usize,
    /// 1 is exactly what the automatic pass decided.
    pub factor: f32,
    /// How far above 100% the slider moves anything: past this every operation in the
    /// category is already at full strength.
    pub headroom: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IntensityReport {
    pub sliders: Vec<Slider>,
    /// For `sync`: photos that took this photo's sliders.
    pub synced: usize,
    /// For `sync`: other photos left alone because the automatic pass had not retouched them.
    pub skipped: usize,
}

fn invalid(message: &str) -> aura_core::AuraError {
    let mut e = aura_core::errors::render::recipe_invalid("retouch intensity", message);
    message.clone_into(&mut e.user_message);
    e
}

fn sliders(recipe: &Recipe) -> aura_core::AuraResult<Vec<Slider>> {
    let edits = retouch_tools::read(recipe)?;
    let factors = retouch_intensity::read(recipe)?;
    let mut counts: BTreeMap<Category, usize> = BTreeMap::new();
    for edit in edits.iter().filter(|e| e.enabled) {
        if let Some(category) = retouch_intensity::category_of(edit) {
            *counts.entry(category).or_default() += 1;
        }
    }
    Ok(Category::ALL
        .into_iter()
        .filter_map(|category| {
            let operations = counts.get(&category).copied()?;
            Some(Slider {
                category,
                operations,
                factor: factors.get(&category).copied().unwrap_or(1.0),
                headroom: retouch_intensity::headroom(&edits, category),
            })
        })
        .collect())
}

/// Store `factors` over `base` as the photographer's edit. Returns false when nothing moved,
/// so an unchanged slider never adds a history step.
fn store(
    state: &AppState,
    project: &ProjectId,
    photo: &PhotoId,
    base: &Recipe,
    factors: &BTreeMap<Category, f32>,
    label: &str,
) -> aura_core::AuraResult<bool> {
    let mut proposal = base.clone();
    retouch_intensity::write(&mut proposal, factors)?;
    let (merged, changes) = schema::merge(base, &proposal, EditSource::User)?;
    if changes.changed.is_empty() {
        return Ok(false);
    }
    schema::Validation::check(&merged)?;
    state.recipe_store().save_if_current(
        project,
        photo,
        &merged,
        &changes.changed,
        label,
        &aura_recipe::recipe_hash(base)?,
    )?;
    Ok(true)
}

/// The stored factors with `changes` laid over them, every value written explicitly.
fn combined(
    base: &Recipe,
    changes: &BTreeMap<Category, f32>,
) -> aura_core::AuraResult<BTreeMap<Category, f32>> {
    let mut factors = retouch_intensity::read(base)?;
    for (category, factor) in changes {
        factors.insert(*category, *factor);
    }
    Ok(factors)
}

/// Read or change the strength of each kind of automatic retouch on a photo.
///
/// # Errors
/// Invalid membership, an unknown action, a factor outside 0..=200%, or a failed save.
pub fn intensity(state: &AppState, input: &IntensityInput) -> IpcResult<IntensityReport> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection"))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut synced = 0;
    let mut skipped = 0;
    match input.action.as_str() {
        "get" => {}
        "set" => {
            if input.factors.is_empty() {
                return Err(invalid("Choose a retouch strength to change").into());
            }
            store(
                state,
                &project,
                &photo,
                &base,
                &combined(&base, &input.factors)?,
                "Retouch strength",
            )?;
        }
        "reset" => {
            let reset: BTreeMap<Category, f32> = retouch_intensity::read(&base)?
                .into_keys()
                .map(|c| (c, 1.0))
                .collect();
            if !reset.is_empty() {
                store(
                    state,
                    &project,
                    &photo,
                    &base,
                    &reset,
                    "Reset retouch strength",
                )?;
            }
        }
        "sync" => {
            let factors = retouch_intensity::read(&base)?;
            for (id, recipe) in state.recipe_store().all(&project)? {
                if id == input.photo_id {
                    continue;
                }
                let other = PhotoId::from_db(&id).map_err(|_| invalid("Invalid photo"))?;
                let automatic = retouch_tools::read(&recipe)?
                    .iter()
                    .any(|e| retouch_intensity::category_of(e).is_some());
                if !automatic {
                    skipped += 1;
                    continue;
                }
                // Every slider this photo has, 100% included, so a slider moved back on this
                // photo is moved back on the others too.
                let mut onto = combined(&recipe, &factors)?;
                for category in sliders(&base)?.into_iter().map(|s| s.category) {
                    onto.entry(category).or_insert(1.0);
                }
                store(
                    state,
                    &project,
                    &other,
                    &recipe,
                    &onto,
                    "Synced retouch strength",
                )?;
                synced += 1;
            }
        }
        _ => return Err(invalid("Unknown retouch strength action").into()),
    }
    let current = crate::develop_commands::load_or_neutral(state, photo)?;
    Ok(IntensityReport {
        sliders: sliders(&current)?,
        synced,
        skipped,
    })
}
