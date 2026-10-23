//! Reversible, absolute strengths for saved automatic retouch operations. ADR-0130.
use std::collections::BTreeMap;

use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_recipe::{
    retouch_tools::{self, Edit, Tool},
    schema, EditSource, Recipe,
};
use serde::{Deserialize, Serialize};

use crate::{commands::IpcResult, AppState};

const KEY: &str = "automatic_strength_v1";
pub(crate) const MANAGED: &str = "manual-strength-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Blemishes,
    Smoothing,
    Tone,
    Wrinkles,
    Eyes,
    Teeth,
    Shine,
    Hair,
    Clothing,
    Background,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Strengths {
    pub overall: f32,
    pub categories: BTreeMap<Category, f32>,
}
impl Default for Strengths {
    fn default() -> Self {
        Self {
            overall: 1.0,
            categories: BTreeMap::new(),
        }
    }
}
impl Strengths {
    fn check(&self) -> AuraResult<()> {
        if std::iter::once(&self.overall)
            .chain(self.categories.values())
            .all(|v| v.is_finite() && (0.0..=2.0).contains(v))
        {
            Ok(())
        } else {
            Err(invalid("Finishing strengths must be between 0 and 200%."))
        }
    }
    fn factor(&self, category: Category) -> f32 {
        self.overall * self.categories.get(&category).copied().unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Original {
    amount: f32,
    shape: String,
    applied: f32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Saved {
    strengths: Strengths,
    originals: BTreeMap<String, Original>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    pub category: Category,
    pub operations: usize,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub recipe_hash: String,
    pub strengths: Strengths,
    pub categories: Vec<CategorySummary>,
    pub new_operations: usize,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub photo_id: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInput {
    pub project_id: String,
    pub photo_id: String,
    pub recipe_hash: String,
    pub strengths: Strengths,
}

fn invalid(message: &str) -> aura_core::AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("automatic strengths", message);
    message.clone_into(&mut error.user_message);
    error
}

fn category(edit: &Edit) -> Option<Category> {
    let id = edit.id.strip_prefix(MANAGED).unwrap_or(&edit.id);
    if !edit.enabled || edit.tool == Tool::Reshape || crate::portrait_auto::group_of(id).is_none() {
        return None;
    }
    if id.starts_with(crate::portrait_auto::SCENE_PREFIX)
        || id.ends_with("backdrop")
        || id.contains("-background-")
        || id.contains("-dust-")
    {
        return Some(Category::Background);
    }
    if id.contains("-hair-") || id.contains("-stray-") {
        return Some(Category::Hair);
    }
    if id.contains("-cloth-") || id.contains("-fabric") {
        return Some(Category::Clothing);
    }
    if id.contains("-spot-") || id.ends_with("-body-spots") || id.ends_with("-body-marks") {
        return Some(Category::Blemishes);
    }
    if id.contains("-lines-") || id.contains("-fold-") {
        return Some(Category::Wrinkles);
    }
    if id.contains("-eye-") || id.contains("-undereye-") {
        return Some(Category::Eyes);
    }
    Some(match edit.tool {
        Tool::AcneClear | Tool::AutoBlemish | Tool::PatchHeal | Tool::FrequencyHeal => {
            Category::Blemishes
        }
        Tool::Frequency | Tool::SkinSmooth | Tool::TextureGraft => Category::Smoothing,
        Tool::SkinColor
        | Tool::ColorMatch
        | Tool::SkinUniformity
        | Tool::Dodge
        | Tool::Burn
        | Tool::PortraitDodgeBurn
        | Tool::MicroDodgeBurn
        | Tool::Makeup => Category::Tone,
        Tool::Wrinkle | Tool::StretchMarks => Category::Wrinkles,
        Tool::EyeClean | Tool::EyeDetail | Tool::UnderEye | Tool::RedEye => Category::Eyes,
        Tool::Teeth => Category::Teeth,
        Tool::Mattify | Tool::Glare => Category::Shine,
        Tool::Fabric | Tool::FabricMoire | Tool::FabricStain => Category::Clothing,
        Tool::Reshape => return None,
        _ => Category::Background,
    })
}

fn fingerprint(edit: &Edit) -> AuraResult<String> {
    let mut copy = edit.clone();
    copy.id.clear();
    copy.amount = 0.0;
    serde_json::to_vec(&copy)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|_| invalid("Could not read the saved automatic operation."))
}

fn snapshot(recipe: &Recipe, edits: &[Edit]) -> AuraResult<(Saved, usize)> {
    let existing = recipe.extra.get(KEY);
    let mut saved: Saved = existing.map(|v| serde_json::from_value(v.clone())
        .map_err(|_| invalid("Saved finishing strengths are unreadable. Use Undo to restore the earlier edit.")))
        .transpose()?.unwrap_or_default();
    saved.strengths.check()?;
    let mut originals = BTreeMap::new();
    let mut new_operations = 0;
    for edit in edits.iter().filter(|e| category(e).is_some()) {
        let shape = fingerprint(edit)?;
        let original = saved.originals.get(&edit.id).filter(|old| {
            old.shape == shape
                && (old.applied - edit.amount).abs() < 1e-6
                && old.amount.is_finite()
                && (0.0..=1.0).contains(&old.amount)
        });
        if existing.is_some() && original.is_none() {
            new_operations += 1;
        }
        originals.insert(
            edit.id.clone(),
            original.cloned().unwrap_or(Original {
                amount: edit.amount,
                shape,
                applied: edit.amount,
            }),
        );
    }
    saved.originals = originals;
    Ok((saved, new_operations))
}

fn report(recipe: &Recipe) -> AuraResult<Report> {
    let edits = retouch_tools::read(recipe)?;
    let (saved, new_operations) = snapshot(recipe, &edits)?;
    let mut counts = BTreeMap::new();
    for edit in &edits {
        if let Some(group) = category(edit) {
            *counts.entry(group).or_insert(0) += 1;
        }
    }
    Ok(Report {
        recipe_hash: aura_recipe::hash::recipe_hash(recipe)?,
        strengths: saved.strengths,
        categories: counts
            .into_iter()
            .map(|(category, operations)| CategorySummary {
                category,
                operations,
            })
            .collect(),
        new_operations,
    })
}

/// Read saved finishing controls without changing any recipe or pixels.
/// # Errors
/// Invalid collection membership, corrupt recipe or saved finishing metadata.
pub fn read(state: &AppState, input: &Input) -> IpcResult<Report> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
    Ok(report(&crate::develop_commands::load_or_neutral(
        state, photo,
    )?)?)
}

fn adjusted(base: &Recipe, strengths: &Strengths) -> AuraResult<Recipe> {
    strengths.check()?;
    let mut edits = retouch_tools::read(base)?;
    let (mut saved, _) = snapshot(base, &edits)?;
    if saved.originals.is_empty() {
        return Err(invalid(
            "Run automatic retouch before adjusting its finish.",
        ));
    }
    let mut mattes = retouch_tools::read_mattes(base)?;
    let mut originals = BTreeMap::new();
    for edit in &mut edits {
        let Some(group) = category(edit) else {
            continue;
        };
        let mut original = saved.originals.get(&edit.id).cloned().ok_or_else(|| {
            invalid("The automatic operation changed. Reload finishing controls.")
        })?;
        let amount = (original.amount * strengths.factor(group)).clamp(0.0, 1.0);
        if (edit.amount - amount).abs() > 1e-6 {
            edit.amount = amount;
            if !edit.id.starts_with(MANAGED) {
                edit.id = format!("{MANAGED}{}", edit.id);
            }
            crate::native_retouch::preserve_manual_matte(edit, &mut mattes)?;
        }
        original.shape = fingerprint(edit)?;
        original.applied = edit.amount;
        originals.insert(edit.id.clone(), original);
    }
    saved.originals = originals;
    saved.strengths = strengths.clone();
    let mut proposal = base.clone();
    retouch_tools::write_with_mattes(&mut proposal, &edits, &mattes)?;
    proposal.extra.insert(
        KEY.into(),
        serde_json::to_value(saved).map_err(|_| invalid("Could not save finishing strengths."))?,
    );
    Ok(proposal)
}

/// Rescale saved automatic operations atomically, retaining zero-strength originals.
/// # Errors
/// Invalid strengths, stale edits, missing masks, invalid recipe or history-write failure.
pub fn apply(state: &AppState, input: &ApplyInput) -> IpcResult<Report> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection."))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let hash = aura_recipe::hash::recipe_hash(&base)?;
    if hash != input.recipe_hash {
        return Err(
            invalid("This photo changed. Reload finishing controls before applying.").into(),
        );
    }
    let proposal = adjusted(&base, &input.strengths)?;
    let (mut merged, mut changes) = schema::merge(&base, &proposal, EditSource::User)?;
    // Baseline maps are complete snapshots: removed operation IDs must not survive
    // the generic leaf merge, which retains absent object keys.
    if base.extra.get(KEY) != proposal.extra.get(KEY) {
        if let Some(value) = proposal.extra.get(KEY) {
            merged.extra.insert(KEY.into(), value.clone());
        }
        changes.changed.push(format!("extra.{KEY}"));
    }
    schema::Validation::check(&merged)?;
    if !changes.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &merged,
            &changes.changed,
            "Adjust automatic finish",
            &hash,
        )?;
    }
    Ok(report(&merged)?)
}

#[cfg(test)]
mod tests;
