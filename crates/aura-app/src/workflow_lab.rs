//! Bounded, durable studio review workspaces and revision-bound multi-photo plans.
//! Source originals and sidecars are never opened for writing by this module.
use std::collections::{BTreeMap, BTreeSet};

use aura_core::{AuraError, AuraResult, PhotoId, ProjectId};
use aura_recipe::{schema, EditSource, Recipe};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{commands::IpcResult, AppState};

const LIMIT: usize = 32;
const MAX_DOCUMENT: usize = 4 * 1024 * 1024;
const SECTIONS: [&str; 7] = [
    "tone",
    "white_balance",
    "curves",
    "colour",
    "detail",
    "effects",
    "geometry",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub action: String,
    pub expected_revision: Option<String>,
    #[serde(default)]
    pub photo_ids: Vec<String>,
    #[serde(default)]
    pub recipe_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub sections: Vec<String>,
    pub name: Option<String>,
    pub checkpoint_id: Option<String>,
    pub source_photo_id: Option<String>,
    pub issue_id: Option<String>,
    pub text: Option<String>,
    pub reviewer: Option<String>,
    pub due_date: Option<String>,
    pub review_date: Option<String>,
    pub winner: Option<String>,
    pub criterion: Option<String>,
    #[serde(default)]
    pub passed: bool,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub point: Option<[f32; 2]>,
    pub filter: Option<SavedFilter>,
    pub document: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub recipe_hash: String,
    pub passed: BTreeSet<String>,
    pub reviewer: String,
    pub due_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Issue {
    pub id: String,
    pub photo_id: String,
    pub recipe_hash: String,
    pub text: String,
    pub point: Option<[f32; 2]>,
    pub resolved: bool,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comparison {
    pub left: String,
    pub right: String,
    pub left_hash: String,
    pub right_hash: String,
    pub winner: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedFilter {
    pub name: String,
    pub reviewer: String,
    pub state: String,
    pub overdue_only: bool,
    pub issues_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Event {
    pub at: String,
    pub action: String,
    pub photos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    id: String,
    name: String,
    at: String,
    recipes: BTreeMap<String, Recipe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlanEntry {
    photo_id: String,
    before_hash: String,
    proposal: Recipe,
    changed: Vec<String>,
    conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Plan {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checkpoint_id: Option<String>,
    source: Option<(String, String)>,
    entries: Vec<PlanEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Workspace {
    queue: Vec<String>,
    criteria: Vec<String>,
    reviews: BTreeMap<String, Review>,
    issues: Vec<Issue>,
    comparisons: Vec<Comparison>,
    filters: Vec<SavedFilter>,
    checkpoints: Vec<Checkpoint>,
    plan: Option<Plan>,
    audit: Vec<Event>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoState {
    pub photo_id: String,
    pub recipe_hash: String,
    pub status: String,
    pub open_issues: usize,
    pub overdue: bool,
    pub fingerprint: String,
    pub sections: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointSummary {
    pub id: String,
    pub name: String,
    pub at: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub kind: String,
    pub entries: Vec<PlanRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRow {
    pub photo_id: String,
    pub changed: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: String,
    pub queue: Vec<String>,
    pub criteria: Vec<String>,
    pub reviews: BTreeMap<String, Review>,
    pub issues: Vec<Issue>,
    pub comparisons: Vec<Comparison>,
    pub filters: Vec<SavedFilter>,
    pub checkpoints: Vec<CheckpointSummary>,
    pub plan: Option<PlanSummary>,
    pub audit: Vec<Event>,
    pub photos: Vec<PhotoState>,
    pub rows: Vec<PlanRow>,
    pub selected: Vec<String>,
    pub document: Option<String>,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Handoff<'a> {
    format: &'static str,
    created_at: &'a str,
    ready: bool,
    criteria: &'a [String],
    photos: &'a [PhotoState],
}

fn invalid(text: &str) -> AuraError {
    let mut error = aura_recipe::errors::recipe_invalid("workflow lab", text);
    text.clone_into(&mut error.user_message);
    error
}
fn db(error: &rusqlite::Error) -> AuraError {
    aura_core::errors::db::statement_failed("studio review workspace", error)
}
fn label(value: Option<&str>, max: usize) -> AuraResult<String> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(invalid(
            "Enter a nonempty label within the displayed limit.",
        ))
    } else {
        Ok(value.into())
    }
}
fn encode<T: Serialize>(value: &T) -> AuraResult<String> {
    let text =
        serde_json::to_string(value).map_err(|_| invalid("Workspace could not be encoded."))?;
    if text.len() > MAX_DOCUMENT {
        return Err(invalid(
            "The review workspace exceeds its 4 MiB limit. Remove a checkpoint first.",
        ));
    }
    Ok(text)
}
fn revision(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}
fn key(project: &str) -> String {
    format!("studio_workflow_v1:{project}")
}
fn load(conn: &Connection, project: &str) -> AuraResult<(Workspace, String)> {
    let body: Option<String> = conn
        .query_row(
            "SELECT value_json FROM setting WHERE key=?1",
            [key(project)],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| db(&e))?;
    let text = body.unwrap_or(encode(&Workspace::default())?);
    if text.len() > MAX_DOCUMENT {
        return Err(invalid("The saved workspace is too large."));
    }
    let workspace =
        serde_json::from_str(&text).map_err(|_| invalid("The saved workspace is unreadable."))?;
    Ok((workspace, revision(&text)))
}
fn today(state: &AppState) -> String {
    aura_catalog::rfc3339(state.clock().now_utc())
        .chars()
        .take(10)
        .collect()
}
fn valid_date(text: &str) -> bool {
    let parts: Vec<_> = text.split('-').collect();
    let mut parts = parts.into_iter();
    let year = parts.next().and_then(|v| v.parse::<i32>().ok());
    let month = parts
        .next()
        .and_then(|v| v.parse::<u8>().ok())
        .and_then(|v| time::Month::try_from(v).ok());
    let day = parts.next().and_then(|v| v.parse::<u8>().ok());
    text.len() == 10
        && parts.next().is_none()
        && year
            .zip(month)
            .zip(day)
            .is_some_and(|((y, m), d)| time::Date::from_calendar_date(y, m, d).is_ok())
}
fn ids(input: &Input) -> AuraResult<BTreeSet<String>> {
    let ids: BTreeSet<_> = input.photo_ids.iter().cloned().collect();
    if ids.is_empty() || ids.len() > LIMIT || ids.len() != input.photo_ids.len() {
        return Err(invalid("Select one to 32 different photographs."));
    }
    Ok(ids)
}
fn checked_recipes(
    state: &AppState,
    project: &str,
    selected: &BTreeSet<String>,
) -> AuraResult<BTreeMap<String, Recipe>> {
    selected
        .iter()
        .map(|id| {
            crate::studio_tools::require_member(state, project, id)?;
            let photo =
                PhotoId::from_db(id).map_err(|_| invalid("Invalid photograph identifier."))?;
            Ok((
                id.clone(),
                crate::develop_commands::load_or_neutral(state, photo)?,
            ))
        })
        .collect()
}
fn require_hashes(input: &Input, recipes: &BTreeMap<String, Recipe>) -> AuraResult<()> {
    for (id, recipe) in recipes {
        if input.recipe_hashes.get(id) != Some(&aura_recipe::recipe_hash(recipe)?) {
            return Err(invalid(
                "A selected photo changed. Refresh the review workspace.",
            ));
        }
    }
    Ok(())
}
fn status(workspace: &Workspace, photo: &str, hash: &str) -> &'static str {
    if workspace
        .issues
        .iter()
        .any(|v| v.photo_id == photo && !v.resolved)
    {
        return "changes_requested";
    }
    let Some(review) = workspace.reviews.get(photo) else {
        return "unreviewed";
    };
    if review.recipe_hash.is_empty() {
        return "unreviewed";
    }
    if review.recipe_hash != hash {
        return "changed_since_review";
    }
    if !workspace.criteria.is_empty()
        && workspace
            .criteria
            .iter()
            .all(|criterion| review.passed.contains(criterion))
    {
        "approved"
    } else {
        "in_review"
    }
}

fn section(recipe: &Recipe, name: &str) -> AuraResult<serde_json::Value> {
    if name == "geometry" {
        return serde_json::to_value(&recipe.geometry)
            .map_err(|_| invalid("Could not read geometry."));
    }
    let fields: &[&str] = match name {
        "tone" => &[
            "exposure",
            "contrast",
            "highlights",
            "shadows",
            "whites",
            "blacks",
            "clarity",
            "texture",
            "dehaze",
        ],
        "white_balance" => &["temperature", "tint"],
        "curves" => &["curve", "parametric", "channel_curves"],
        "colour" => &["saturation", "vibrance", "hsl", "colour_grade"],
        "detail" => &["sharpen", "noise"],
        "effects" => &["effects"],
        _ => return Err(invalid("Choose a supported settings section.")),
    };
    let global = serde_json::to_value(&recipe.global)
        .map_err(|_| invalid("Could not read global settings."))?;
    let mut values = serde_json::Map::new();
    for field in fields {
        // Neutral optional blocks are deliberately omitted by Global's serializer.
        // A checkpoint still needs their typed defaults for comparison and restore.
        let value = match global.get(*field) {
            Some(value) => value.clone(),
            None => match *field {
                "parametric" => serde_json::to_value(recipe.global.parametric),
                "channel_curves" => serde_json::to_value(&recipe.global.channel_curves),
                "colour_grade" => serde_json::to_value(recipe.global.colour_grade),
                "effects" => serde_json::to_value(recipe.global.effects),
                _ => return Err(invalid("A settings section is unavailable.")),
            }
            .map_err(|_| invalid("Could not read optional global settings."))?,
        };
        values.insert((*field).to_owned(), value);
    }
    if name == "colour" {
        values.insert(
            "bw".into(),
            serde_json::to_value(&recipe.bw)
                .map_err(|_| invalid("Could not read monochrome settings."))?,
        );
    }
    Ok(serde_json::Value::Object(values))
}
fn copy_section(target: &mut Recipe, source: &Recipe, name: &str) -> AuraResult<()> {
    let src = &source.global;
    let dst = &mut target.global;
    match name {
        "tone" => {
            dst.exposure = src.exposure;
            dst.contrast = src.contrast;
            dst.highlights = src.highlights;
            dst.shadows = src.shadows;
            dst.whites = src.whites;
            dst.blacks = src.blacks;
            dst.clarity = src.clarity;
            dst.texture = src.texture;
            dst.dehaze = src.dehaze;
        }
        "white_balance" => {
            dst.temperature = src.temperature;
            dst.tint = src.tint;
        }
        "curves" => {
            dst.curve.clone_from(&src.curve);
            dst.parametric = src.parametric;
            dst.channel_curves.clone_from(&src.channel_curves);
        }
        "colour" => {
            dst.saturation = src.saturation;
            dst.vibrance = src.vibrance;
            dst.hsl.clone_from(&src.hsl);
            dst.colour_grade = src.colour_grade;
            target.bw.clone_from(&source.bw);
        }
        "detail" => {
            dst.sharpen = src.sharpen;
            dst.noise = src.noise.clone();
        }
        "effects" => dst.effects = src.effects,
        "geometry" => target.geometry = source.geometry.clone(),
        _ => return Err(invalid("Choose a supported settings section.")),
    }
    Ok(())
}
fn selected_sections(input: &Input) -> AuraResult<Vec<String>> {
    let set: BTreeSet<_> = input.sections.iter().collect();
    if set.is_empty()
        || set.len() != input.sections.len()
        || set.iter().any(|s| !SECTIONS.contains(&s.as_str()))
    {
        return Err(invalid("Select one or more different settings sections."));
    }
    Ok(input.sections.clone())
}
fn checkpoint<'a>(workspace: &'a Workspace, input: &Input) -> AuraResult<&'a Checkpoint> {
    workspace
        .checkpoints
        .iter()
        .find(|c| Some(&c.id) == input.checkpoint_id.as_ref())
        .ok_or_else(|| invalid("Select an available checkpoint."))
}
fn sections_changed(left: &Recipe, right: &Recipe) -> AuraResult<Vec<String>> {
    SECTIONS
        .into_iter()
        .filter_map(|name| {
            match section(left, name).and_then(|a| section(right, name).map(|b| a != b)) {
                Ok(true) => Some(Ok(name.into())),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            }
        })
        .collect()
}
fn validate_plan_request(
    workspace: &Workspace,
    input: &Input,
    recipes: &BTreeMap<String, Recipe>,
    sections: &[String],
    checkpoint_id: &str,
) -> AuraResult<()> {
    if input.action == "rollback_plan"
        && workspace
            .plan
            .as_ref()
            .is_none_or(|plan| plan.checkpoint_id.as_deref() != Some(checkpoint_id))
    {
        return Err(invalid(
            "Rollback must use the prepared plan's original checkpoint.",
        ));
    }
    if matches!(input.action.as_str(), "delta_plan" | "merge_plan") {
        if recipes.len() < 2
            || input
                .source_photo_id
                .as_ref()
                .is_none_or(|id| !recipes.contains_key(id))
        {
            return Err(invalid(
                "Include a source and at least one target photograph.",
            ));
        }
        if input.action == "delta_plan"
            && sections
                .iter()
                .any(|name| name != "tone" && name != "colour")
        {
            return Err(invalid(
                "Relative transfer supports tone and colour scalar controls.",
            ));
        }
        if input.action == "merge_plan" && sections.iter().any(|name| name == "geometry") {
            return Err(invalid(
                "Three-way transfer keeps each photograph's geometry.",
            ));
        }
    }
    Ok(())
}

fn plan_entry(
    id: &str,
    current: &Recipe,
    proposal: &Recipe,
    conflicts: Vec<String>,
) -> AuraResult<PlanEntry> {
    let (merged, _) = schema::merge(current, proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;
    let changed = aura_recipe::history::changed_paths(current, &merged);
    Ok(PlanEntry {
        photo_id: id.to_owned(),
        before_hash: aura_recipe::recipe_hash(current)?,
        proposal: merged,
        changed,
        conflicts,
    })
}

fn plan(
    workspace: &Workspace,
    input: &Input,
    recipes: &BTreeMap<String, Recipe>,
) -> AuraResult<Plan> {
    let checkpoint = checkpoint(workspace, input)?;
    let sections = selected_sections(input)?;
    validate_plan_request(workspace, input, recipes, &sections, &checkpoint.id)?;
    let source_id = input.source_photo_id.as_ref();
    let source = source_id.and_then(|id| recipes.get(id));
    let mut entries = Vec::new();
    for (id, current) in recipes {
        let mut proposal = current.clone();
        let mut conflicts = Vec::new();
        let prior = checkpoint
            .recipes
            .get(id)
            .ok_or_else(|| invalid("The checkpoint does not contain every selected photograph."))?;
        if input.action == "delta_plan" {
            let source_id = source_id.ok_or_else(|| invalid("Select a source photograph."))?;
            if id == source_id {
                continue;
            }
            let source =
                source.ok_or_else(|| invalid("Include the source in the selected cohort."))?;
            let original = checkpoint
                .recipes
                .get(source_id)
                .ok_or_else(|| invalid("The checkpoint does not contain the source."))?;
            proposal = relative_proposal(current, source, original, &sections);
        } else if input.action == "merge_plan" {
            let source_id = source_id.ok_or_else(|| invalid("Select a source photograph."))?;
            if id == source_id {
                continue;
            }
            let source = source.ok_or_else(|| invalid("Include the source in the cohort."))?;
            let ancestor = checkpoint
                .recipes
                .get(source_id)
                .ok_or_else(|| invalid("The checkpoint does not contain the source."))?;
            for name in &sections {
                let from = section(ancestor, name)?;
                let theirs = section(source, name)?;
                let ours = section(current, name)?;
                if theirs == from || ours == theirs {
                    continue;
                }
                if ours == section(prior, name)? {
                    copy_section(&mut proposal, source, name)?;
                } else {
                    conflicts.push(name.clone());
                }
            }
        } else {
            for name in &sections {
                // Rollback preview only offers sections unchanged since this session's
                // prepared plan, preserving subsequent authoring in conflicted sections.
                if input.action == "rollback_plan" {
                    let old = workspace
                        .plan
                        .as_ref()
                        .and_then(|p| p.entries.iter().find(|e| e.photo_id == *id));
                    if old.is_none_or(|e| {
                        section(current, name).ok() != section(&e.proposal, name).ok()
                    }) {
                        conflicts.push(name.clone());
                        continue;
                    }
                }
                copy_section(&mut proposal, prior, name)?;
            }
        }
        entries.push(plan_entry(id, current, &proposal, conflicts)?);
    }
    Ok(Plan {
        kind: input.action.clone(),
        checkpoint_id: Some(checkpoint.id.clone()),
        source: source_id
            .and_then(|id| {
                recipes
                    .get(id)
                    .map(|recipe| aura_recipe::recipe_hash(recipe).map(|hash| (id.clone(), hash)))
            })
            .transpose()?,
        entries,
    })
}
fn add_i16(base: i16, new: i16, old: i16) -> i16 {
    base.saturating_add(new.saturating_sub(old))
        .clamp(-100, 100)
}

fn relative_proposal(
    current: &Recipe,
    source: &Recipe,
    original: &Recipe,
    sections: &[String],
) -> Recipe {
    let mut proposal = current.clone();
    for name in sections {
        if name == "tone" {
            proposal.global.exposure += source.global.exposure - original.global.exposure;
            proposal.global.contrast = add_i16(
                current.global.contrast,
                source.global.contrast,
                original.global.contrast,
            );
            proposal.global.highlights = add_i16(
                current.global.highlights,
                source.global.highlights,
                original.global.highlights,
            );
            proposal.global.shadows = add_i16(
                current.global.shadows,
                source.global.shadows,
                original.global.shadows,
            );
            proposal.global.whites = add_i16(
                current.global.whites,
                source.global.whites,
                original.global.whites,
            );
            proposal.global.blacks = add_i16(
                current.global.blacks,
                source.global.blacks,
                original.global.blacks,
            );
            proposal.global.clarity = add_i16(
                current.global.clarity,
                source.global.clarity,
                original.global.clarity,
            );
            proposal.global.texture = add_i16(
                current.global.texture,
                source.global.texture,
                original.global.texture,
            );
            proposal.global.dehaze = add_i16(
                current.global.dehaze,
                source.global.dehaze,
                original.global.dehaze,
            );
        } else {
            proposal.global.saturation = add_i16(
                current.global.saturation,
                source.global.saturation,
                original.global.saturation,
            );
            proposal.global.vibrance = add_i16(
                current.global.vibrance,
                source.global.vibrance,
                original.global.vibrance,
            );
        }
    }
    proposal
}

fn matches_filter(filter: &SavedFilter, photo: &PhotoState, review: Option<&Review>) -> bool {
    (filter.reviewer.is_empty() || review.is_some_and(|r| r.reviewer == filter.reviewer))
        && (filter.state.is_empty() || photo.status == filter.state)
        && (!filter.overdue_only || photo.overdue)
        && (!filter.issues_only || photo.open_issues > 0)
}
fn photo_states(
    workspace: &Workspace,
    recipes: &BTreeMap<String, Recipe>,
    date: &str,
) -> AuraResult<Vec<PhotoState>> {
    recipes
        .iter()
        .map(|(id, recipe)| {
            let hash = aura_recipe::recipe_hash(recipe)?;
            let sections = SECTIONS
                .iter()
                .map(|name| Ok(((*name).into(), revision(&encode(&section(recipe, name)?)?))))
                .collect::<AuraResult<BTreeMap<_, _>>>()?;
            let fingerprint = revision(&encode(&sections)?);
            let state = status(workspace, id, &hash);
            Ok(PhotoState {
                photo_id: id.clone(),
                recipe_hash: hash,
                status: state.into(),
                open_issues: workspace
                    .issues
                    .iter()
                    .filter(|i| i.photo_id == *id && !i.resolved)
                    .count(),
                overdue: state != "approved"
                    && workspace
                        .reviews
                        .get(id)
                        .is_some_and(|r| !r.due_date.is_empty() && r.due_date.as_str() < date),
                fingerprint,
                sections,
            })
        })
        .collect()
}

/// Execute a bounded workflow. Metadata changes use revision-checked transactions;
/// edit plans apply individually with catalog compare-and-save and explicit partial results.
/// # Errors
/// Invalid inputs, foreign photos, stale revisions, size budgets or failed storage.
#[allow(clippy::too_many_lines)]
pub fn run(state: &AppState, input: &Input) -> IpcResult<Snapshot> {
    if input.recipe_hashes.len() > LIMIT || input.depends_on.len() > 8 || input.sections.len() > 7 {
        return Err(invalid("The workflow request exceeds its selection budget.").into());
    }
    ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection."))?;
    let selected = ids(input)?;
    let recipes = checked_recipes(state, &input.project_id, &selected)?;
    let project = input.project_id.clone();
    let (mut workspace, previous) = state.catalog().read(move |conn| load(conn, &project))?;
    let readonly = matches!(
        input.action.as_str(),
        "read"
            | "drift"
            | "verify"
            | "fingerprints"
            | "matrix"
            | "undecided"
            | "handoff"
            | "export"
            | "audit"
            | "apply_filter"
    );
    if !readonly {
        if input.expected_revision.as_ref() != Some(&previous) {
            return Err(invalid("The review workspace changed. Refresh before continuing.").into());
        }
        require_hashes(input, &recipes)?;
    }
    let mut rows = Vec::new();
    let mut document = None;
    let mut chosen = Vec::new();
    let mut message = String::new();
    let at = aura_catalog::rfc3339(state.clock().now_utc());
    let date = match input.review_date.as_deref() {
        Some(date) if valid_date(date) => date.to_owned(),
        Some(_) => return Err(invalid("Use a valid review calendar date.").into()),
        None => today(state),
    };
    match input.action.as_str() {
        "read" | "audit" => {}
        "fingerprints" => {
            let photos = photo_states(&workspace, &recipes, &date)?;
            let count = photos
                .iter()
                .map(|photo| &photo.fingerprint)
                .collect::<BTreeSet<_>>()
                .len();
            message = format!(
                "Found {count} settings cohorts across {} photographs.",
                photos.len()
            );
        }
        "matrix" => {
            message =
                "Section signatures identify which settings differ between photographs.".into();
        }
        "checkpoint" => {
            if workspace.checkpoints.len() >= 8 {
                return Err(invalid("Keep at most eight cohort checkpoints.").into());
            }
            workspace.checkpoints.push(Checkpoint {
                id: uuid::Uuid::new_v4().to_string(),
                name: label(input.name.as_deref(), 80)?,
                at: at.clone(),
                recipes: recipes.clone(),
            });
            message = "Cohort checkpoint saved.".into();
        }
        "remove_checkpoint" => {
            let id = checkpoint(&workspace, input)?.id.clone();
            workspace.checkpoints.retain(|v| v.id != id);
        }
        "drift" => {
            let prior = checkpoint(&workspace, input)?;
            for (id, current) in &recipes {
                let old = prior
                    .recipes
                    .get(id)
                    .ok_or_else(|| invalid("The checkpoint does not contain this cohort."))?;
                rows.push(PlanRow {
                    photo_id: id.clone(),
                    changed: sections_changed(old, current)?,
                    conflicts: Vec::new(),
                });
            }
        }
        "restore_plan" | "rollback_plan" | "delta_plan" | "merge_plan" => {
            workspace.plan = Some(plan(&workspace, input, &recipes)?);
            message = "Plan prepared. Review the changes and conflicts before applying.".into();
        }
        "apply_plan" => {
            let plan = workspace
                .plan
                .as_ref()
                .ok_or_else(|| invalid("Prepare a plan first."))?;
            if plan.entries.iter().any(|e| !selected.contains(&e.photo_id)) {
                return Err(invalid("Select every photo in the prepared plan.").into());
            }
            if let Some((source, hash)) = &plan.source {
                let source = recipes
                    .get(source)
                    .ok_or_else(|| invalid("Include the source photograph."))?;
                if aura_recipe::recipe_hash(source)? != *hash {
                    return Err(invalid("The source changed. Prepare a new plan.").into());
                }
            }
            let project = ProjectId::from_db(&input.project_id)
                .map_err(|_| invalid("Invalid collection."))?;
            for entry in &plan.entries {
                let current = recipes
                    .get(&entry.photo_id)
                    .ok_or_else(|| invalid("A target is unavailable."))?;
                if aura_recipe::recipe_hash(current)? != entry.before_hash {
                    return Err(invalid("A planned target changed. Prepare a fresh plan.").into());
                }
                if !entry.conflicts.is_empty() {
                    return Err(invalid(
                        "Resolve conflicts by preparing a narrower section plan before applying.",
                    )
                    .into());
                }
            }
            for entry in &plan.entries {
                let photo =
                    PhotoId::from_db(&entry.photo_id).map_err(|_| invalid("Invalid target."))?;
                if entry.changed.is_empty() {
                    continue;
                }
                let saved = state.recipe_store().load(&photo).and_then(|stored| {
                    if stored.is_some() {
                        state.recipe_store().save_if_current(
                            &project,
                            &photo,
                            &entry.proposal,
                            &entry.changed,
                            "Workflow lab plan",
                            &entry.before_hash,
                        )
                    } else if aura_recipe::recipe_hash(&crate::develop_commands::neutral_recipe(
                        state, photo,
                    ))? == entry.before_hash
                    {
                        state.recipe_store().save_first(
                            &project,
                            &photo,
                            &entry.proposal,
                            &entry.changed,
                            "Workflow lab plan",
                        )
                    } else {
                        Err(invalid(
                            "The saved target was removed. Prepare a fresh plan.",
                        ))
                    }
                });
                rows.push(PlanRow {
                    photo_id: entry.photo_id.clone(),
                    changed: if saved.is_ok() {
                        entry.changed.clone()
                    } else {
                        Vec::new()
                    },
                    conflicts: saved
                        .err()
                        .map(|e| vec![e.user_message])
                        .unwrap_or_default(),
                });
            }
            message="Plan finished. Inspect each target result; successful saves have individual Undo history.".into();
        }
        "verify" => {
            require_hashes(input, &recipes)?;
            message = "Every selected saved revision still matches the loaded workspace.".into();
        }
        "queue" => {
            workspace.queue.clone_from(&input.photo_ids);
        }
        "criteria" => {
            let text = input.text.as_deref().unwrap_or_default();
            let criteria = text
                .lines()
                .map(|line| label(Some(line), 80))
                .collect::<AuraResult<Vec<_>>>()?;
            if criteria.is_empty()
                || criteria.len() > 16
                || criteria.iter().collect::<BTreeSet<_>>().len() != criteria.len()
            {
                return Err(
                    invalid("Use one to 16 different checklist criteria, one per line.").into(),
                );
            }
            workspace.criteria = criteria;
            for review in workspace.reviews.values_mut() {
                review.passed.clear();
            }
        }
        "signoff" => {
            let criterion = label(input.criterion.as_deref(), 80)?;
            if !workspace.criteria.contains(&criterion) {
                return Err(invalid("Choose an existing review criterion.").into());
            }
            for (id, recipe) in &recipes {
                if input.passed
                    && workspace
                        .issues
                        .iter()
                        .any(|issue| issue.photo_id == *id && !issue.resolved)
                {
                    return Err(
                        invalid("Resolve open correction issues before signing off.").into(),
                    );
                }
                let hash = aura_recipe::recipe_hash(recipe)?;
                let review = workspace.reviews.entry(id.clone()).or_default();
                if review.recipe_hash != hash {
                    review.passed.clear();
                }
                review.recipe_hash = hash;
                if input.passed {
                    review.passed.insert(criterion.clone());
                } else {
                    review.passed.remove(&criterion);
                }
            }
        }
        "issue" => {
            let text = label(input.text.as_deref(), 500)?;
            if input
                .point
                .is_some_and(|p| p.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
            {
                return Err(invalid("Place a correction marker inside the photograph.").into());
            }
            if workspace.issues.len() + selected.len() > 128 {
                return Err(invalid("Keep at most 128 correction issues.").into());
            }
            for id in &selected {
                if input.depends_on.len() > 8
                    || input.depends_on.iter().collect::<BTreeSet<_>>().len()
                        != input.depends_on.len()
                    || input.depends_on.iter().any(|dep| {
                        !workspace
                            .issues
                            .iter()
                            .any(|i| i.id == *dep && i.photo_id == *id)
                    })
                {
                    return Err(invalid(
                        "Dependencies must be up to eight existing issues on the same photograph.",
                    )
                    .into());
                }
                workspace.issues.push(Issue {
                    id: uuid::Uuid::new_v4().to_string(),
                    photo_id: id.clone(),
                    recipe_hash: aura_recipe::recipe_hash(
                        recipes
                            .get(id)
                            .ok_or_else(|| invalid("Unavailable photo."))?,
                    )?,
                    text: text.clone(),
                    point: input.point,
                    resolved: false,
                    depends_on: input.depends_on.clone(),
                });
                if let Some(review) = workspace.reviews.get_mut(id) {
                    review.passed.clear();
                }
            }
        }
        "resolve_issue" => {
            let issue = workspace
                .issues
                .iter()
                .find(|i| Some(&i.id) == input.issue_id.as_ref())
                .cloned()
                .ok_or_else(|| invalid("Select a correction issue."))?;
            if !selected.contains(&issue.photo_id) {
                return Err(invalid("Include the issue's photograph.").into());
            }
            if input.passed
                && issue
                    .depends_on
                    .iter()
                    .any(|id| workspace.issues.iter().any(|i| i.id == *id && !i.resolved))
            {
                return Err(invalid("Resolve prerequisite issues first.").into());
            }
            if let Some(saved) = workspace.issues.iter_mut().find(|i| i.id == issue.id) {
                saved.resolved = input.passed;
            }
            if !input.passed {
                // Reopening an earlier correction invalidates every downstream completion.
                let mut reopened = BTreeSet::from([issue.id.clone()]);
                for dependent in &mut workspace.issues {
                    if dependent.depends_on.iter().any(|id| reopened.contains(id)) {
                        dependent.resolved = false;
                        reopened.insert(dependent.id.clone());
                    }
                }
                if let Some(review) = workspace.reviews.get_mut(&issue.photo_id) {
                    review.passed.clear();
                }
            }
        }
        "propagate_issue" => {
            let issue = workspace
                .issues
                .iter()
                .find(|i| Some(&i.id) == input.issue_id.as_ref())
                .cloned()
                .ok_or_else(|| invalid("Choose a correction to propagate."))?;
            if workspace.issues.len() + selected.len() > 128 {
                return Err(invalid("Keep at most 128 issues.").into());
            }
            for (id, recipe) in &recipes {
                if *id == issue.photo_id {
                    continue;
                }
                workspace.issues.push(Issue {
                    id: uuid::Uuid::new_v4().to_string(),
                    photo_id: id.clone(),
                    recipe_hash: aura_recipe::recipe_hash(recipe)?,
                    text: issue.text.clone(),
                    point: None,
                    resolved: false,
                    depends_on: Vec::new(),
                });
                if let Some(review) = workspace.reviews.get_mut(id) {
                    review.passed.clear();
                }
            }
        }
        "assign" => {
            let reviewer = label(input.reviewer.as_deref(), 80)?;
            for id in &selected {
                workspace
                    .reviews
                    .entry(id.clone())
                    .or_default()
                    .reviewer
                    .clone_from(&reviewer);
            }
        }
        "schedule" => {
            let due = input.due_date.clone().unwrap_or_default();
            if !due.is_empty() && !valid_date(&due) {
                return Err(invalid("Choose a valid YYYY-MM-DD due date.").into());
            }
            for id in &selected {
                workspace
                    .reviews
                    .entry(id.clone())
                    .or_default()
                    .due_date
                    .clone_from(&due);
            }
            workspace.queue.sort_by_key(|id| {
                workspace.reviews.get(id).map_or_else(
                    || "9999-12-31".into(),
                    |r| {
                        if r.due_date.is_empty() {
                            "9999-12-31".into()
                        } else {
                            r.due_date.clone()
                        }
                    },
                )
            });
        }
        "compare" => {
            if input.photo_ids.len() != 2 {
                return Err(
                    invalid("Select exactly two photographs for a pair comparison.").into(),
                );
            }
            let left = input
                .photo_ids
                .first()
                .cloned()
                .ok_or_else(|| invalid("Choose the left photo."))?;
            let right = input
                .photo_ids
                .last()
                .cloned()
                .ok_or_else(|| invalid("Choose the right photo."))?;
            let winner = input.winner.clone().unwrap_or_default();
            if !winner.is_empty() && winner != "tie" && winner != left && winner != right {
                return Err(invalid("Choose left, right, tied or undecided.").into());
            }
            let note = input.text.clone().unwrap_or_default();
            if note.len() > 500 {
                return Err(invalid("Comparison notes are limited to 500 bytes.").into());
            }
            workspace.comparisons.retain(|c| {
                !((c.left == left && c.right == right) || (c.right == left && c.left == right))
            });
            if workspace.comparisons.len() >= 64 {
                return Err(invalid("Keep at most 64 comparison pairs.").into());
            }
            workspace.comparisons.push(Comparison {
                left_hash: input.recipe_hashes.get(&left).cloned().unwrap_or_default(),
                right_hash: input.recipe_hashes.get(&right).cloned().unwrap_or_default(),
                left,
                right,
                winner,
                note,
            });
        }
        "undecided" => {
            for comparison in &workspace.comparisons {
                let changed_revision = recipes.get(&comparison.left).is_some_and(|r| {
                    aura_recipe::recipe_hash(r).ok().as_ref() != Some(&comparison.left_hash)
                }) || recipes.get(&comparison.right).is_some_and(|r| {
                    aura_recipe::recipe_hash(r).ok().as_ref() != Some(&comparison.right_hash)
                });
                if comparison.winner.is_empty() || comparison.winner == "tie" || changed_revision {
                    for id in [&comparison.left, &comparison.right] {
                        if selected.contains(id) && !chosen.contains(id) {
                            chosen.push(id.clone());
                        }
                    }
                }
            }
        }
        "filter" => {
            let filter = input
                .filter
                .clone()
                .ok_or_else(|| invalid("Enter a review filter."))?;
            validate_filter(&filter)?;
            workspace.filters.retain(|f| f.name != filter.name);
            if workspace.filters.len() >= 16 {
                return Err(invalid("Keep at most 16 saved review filters.").into());
            }
            workspace.filters.push(filter);
        }
        "apply_filter" => {
            let filter = workspace
                .filters
                .iter()
                .find(|f| Some(&f.name) == input.name.as_ref())
                .ok_or_else(|| invalid("Choose a saved filter."))?;
            chosen = photo_states(&workspace, &recipes, &date)?
                .iter()
                .filter(|p| matches_filter(filter, p, workspace.reviews.get(&p.photo_id)))
                .map(|p| p.photo_id.clone())
                .collect();
        }
        "handoff" => {
            let photos = photo_states(&workspace, &recipes, &date)?;
            let ready = photos
                .iter()
                .all(|p| p.status == "approved" && p.open_issues == 0);
            document = Some(encode(&Handoff {
                format: "aura-review-handoff-v1",
                created_at: &at,
                ready,
                criteria: &workspace.criteria,
                photos: &photos,
            })?);
            message = if ready {
                "Every selected saved revision passed the checklist."
            } else {
                "Handoff blocked: review incomplete, changed revisions, or open corrections."
            }
            .into();
        }
        "export" => {
            document = Some(export_review(&workspace)?);
        }
        "import" => {
            import_review(&mut workspace, input, &selected)?;
        }
        _ => return Err(invalid("Unknown studio workflow.").into()),
    }
    if !readonly {
        workspace.audit.push(Event {
            at: at.clone(),
            action: input.action.clone(),
            photos: input.photo_ids.clone(),
        });
        if workspace.audit.len() > 128 {
            workspace.audit.remove(0);
        }
        let text = encode(&workspace)?;
        let project = input.project_id.clone();
        let expected = previous.clone();
        let members = selected.clone();
        let neutral_hashes = recipes
            .keys()
            .map(|id| {
                let photo =
                    PhotoId::from_db(id).map_err(|_| invalid("Invalid collection photograph."))?;
                Ok((
                    id.clone(),
                    aura_recipe::recipe_hash(&crate::develop_commands::neutral_recipe(
                        state, photo,
                    ))?,
                ))
            })
            .collect::<AuraResult<BTreeMap<_, _>>>()?;
        let checked = if input.action == "apply_plan" {
            BTreeMap::new()
        } else {
            input.recipe_hashes.clone()
        };
        state.catalog().writer().transact(move |conn| {
            let (_,actual)=load(conn,&project)?;
            if actual!=expected { return Err(invalid("The workspace changed. Refresh before continuing.")); }
            for id in &members {
                let member: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM photo WHERE photo_id=?1 AND project_id=?2)", [id, &project], |row| row.get(0)).map_err(|e| db(&e))?;
                if !member { return Err(invalid("A selected photograph no longer belongs to this collection.")); }
            }
            for (id,hash) in &checked {
                let actual:Option<String>=conn.query_row("SELECT recipe_hash FROM edit_recipes WHERE photo_id=?1 AND project_id=?2",[id,&project],|r| r.get(0)).optional().map_err(|e| db(&e))?;
                if actual.as_ref()!=Some(hash) && !(actual.is_none() && neutral_hashes.get(id)==Some(hash)) { return Err(invalid("A selected photo changed before the workspace was saved.")); }
            }
            conn.execute("INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",[key(&project),text,at]).map_err(|e| db(&e))?;
            Ok(())
        })?;
    }
    let current = checked_recipes(state, &input.project_id, &selected)?;
    if readonly
        && current.iter().any(|(id, recipe)| {
            recipes.get(id).is_none_or(|old| {
                aura_recipe::recipe_hash(recipe).ok() != aura_recipe::recipe_hash(old).ok()
            })
        })
    {
        return Err(
            invalid("A selected photo changed during review. Refresh the workspace.").into(),
        );
    }
    let rev = revision(&encode(&workspace)?);
    let plan = workspace.plan.as_ref().map(|p| PlanSummary {
        kind: p.kind.clone(),
        entries: p
            .entries
            .iter()
            .map(|e| PlanRow {
                photo_id: e.photo_id.clone(),
                changed: e.changed.clone(),
                conflicts: e.conflicts.clone(),
            })
            .collect(),
    });
    let photos = photo_states(&workspace, &current, &date)?;
    Ok(Snapshot {
        revision: rev,
        queue: workspace.queue,
        criteria: workspace.criteria,
        reviews: workspace.reviews,
        issues: workspace.issues,
        comparisons: workspace.comparisons,
        filters: workspace.filters,
        checkpoints: workspace
            .checkpoints
            .iter()
            .map(|c| CheckpointSummary {
                id: c.id.clone(),
                name: c.name.clone(),
                at: c.at.clone(),
                count: c.recipes.len(),
            })
            .collect(),
        plan,
        audit: workspace.audit,
        photos,
        rows,
        selected: chosen,
        document,
        message,
    })
}

fn validate_filter(filter: &SavedFilter) -> AuraResult<()> {
    label(Some(&filter.name), 80)?;
    if filter.reviewer.len() > 80
        || ![
            "",
            "approved",
            "in_review",
            "unreviewed",
            "changed_since_review",
            "changes_requested",
        ]
        .contains(&filter.state.as_str())
    {
        return Err(invalid("Choose a supported review state and reviewer."));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Portable {
    format: String,
    queue: Vec<String>,
    criteria: Vec<String>,
    reviews: BTreeMap<String, Review>,
    issues: Vec<Issue>,
    comparisons: Vec<Comparison>,
    filters: Vec<SavedFilter>,
}
fn export_review(workspace: &Workspace) -> AuraResult<String> {
    encode(&Portable {
        format: "aura-review-v1".into(),
        queue: workspace.queue.clone(),
        criteria: workspace.criteria.clone(),
        reviews: workspace.reviews.clone(),
        issues: workspace.issues.clone(),
        comparisons: workspace.comparisons.clone(),
        filters: workspace.filters.clone(),
    })
}
fn import_review(
    workspace: &mut Workspace,
    input: &Input,
    selected: &BTreeSet<String>,
) -> AuraResult<()> {
    let text = input.document.as_deref().unwrap_or_default();
    if text.len() > 256 * 1024 {
        return Err(invalid("Import at most 256 KiB of review metadata."));
    }
    let mut portable: Portable =
        serde_json::from_str(text).map_err(|_| invalid("Choose a valid AURA review document."))?;
    if portable.format != "aura-review-v1"
        || portable.queue.len() > LIMIT
        || portable.reviews.len() > LIMIT
        || portable.criteria.len() > 16
        || portable.issues.len() > 128
        || portable.comparisons.len() > 64
        || portable.filters.len() > 16
    {
        return Err(invalid("Review interchange format or limits do not match."));
    }
    for name in &portable.criteria {
        label(Some(name), 80)?;
    }
    if portable.queue.iter().collect::<BTreeSet<_>>().len() != portable.queue.len()
        || portable.criteria.iter().collect::<BTreeSet<_>>().len() != portable.criteria.len()
        || portable
            .filters
            .iter()
            .map(|filter| &filter.name)
            .collect::<BTreeSet<_>>()
            .len()
            != portable.filters.len()
    {
        return Err(invalid(
            "Queue photographs, checklist criteria and saved filter names must be unique.",
        ));
    }
    let mut references: BTreeSet<_> = portable
        .queue
        .iter()
        .cloned()
        .chain(portable.reviews.keys().cloned())
        .collect();
    let issue_ids: BTreeSet<_> = portable.issues.iter().map(|i| i.id.clone()).collect();
    if issue_ids.len() != portable.issues.len() {
        return Err(invalid("Issue identifiers must be unique."));
    }
    for issue in &mut portable.issues {
        label(Some(&issue.id), 80)?;
        label(Some(&issue.text), 500)?;
        references.insert(issue.photo_id.clone());
        if issue
            .point
            .is_some_and(|p| p.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
            || issue.depends_on.len() > 8
            || issue.depends_on.iter().collect::<BTreeSet<_>>().len() != issue.depends_on.len()
            || issue
                .depends_on
                .iter()
                .any(|d| !issue_ids.contains(d) || d == &issue.id)
        {
            return Err(invalid("Invalid issue markers or prerequisites."));
        }
        // Imported metadata cannot manufacture trusted approval or completed corrections.
        issue.resolved = false;
    }
    for comparison in &portable.comparisons {
        references.insert(comparison.left.clone());
        references.insert(comparison.right.clone());
        if comparison.left == comparison.right
            || comparison.note.len() > 500
            || (!comparison.winner.is_empty()
                && comparison.winner != "tie"
                && comparison.winner != comparison.left
                && comparison.winner != comparison.right)
        {
            return Err(invalid("Invalid comparison pair."));
        }
    }
    for filter in &portable.filters {
        validate_filter(filter)?;
    }
    for review in portable.reviews.values_mut() {
        if review.reviewer.len() > 80
            || (!review.due_date.is_empty() && !valid_date(&review.due_date))
        {
            return Err(invalid("Invalid reviewer or due date."));
        }
        review.passed.clear();
        review.recipe_hash.clear();
    }
    if references.iter().any(|id| !selected.contains(id)) {
        return Err(invalid(
            "Select every referenced collection photo before importing.",
        ));
    }
    validate_dependencies(&portable.issues)?;
    workspace.queue = portable.queue;
    workspace.criteria = portable.criteria;
    workspace.reviews = portable.reviews;
    workspace.issues = portable.issues;
    workspace.comparisons = portable.comparisons;
    workspace.filters = portable.filters;
    Ok(())
}

fn validate_dependencies(issues: &[Issue]) -> AuraResult<()> {
    // A predecessor must occur earlier in the document, which proves acyclic dependencies.
    let mut seen = BTreeSet::new();
    for issue in issues {
        if issue.depends_on.iter().any(|id| !seen.contains(id)) {
            return Err(invalid(
                "Import correction prerequisites before their dependents.",
            ));
        }
        if issue.depends_on.iter().any(|id| {
            issues
                .iter()
                .any(|parent| parent.id == *id && parent.photo_id != issue.photo_id)
        }) {
            return Err(invalid(
                "Correction prerequisites must belong to the same photograph.",
            ));
        }
        seen.insert(issue.id.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
