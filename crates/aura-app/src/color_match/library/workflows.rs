//! Atomic reference-library organization and bounded, descriptor-only interchange.
use std::collections::{BTreeMap, BTreeSet};

use aura_core::{AuraError, AuraResult};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::super::{
    asset_key, invalid, AppState, Controls, Reference, ReferenceSummary, ASSET, ENGINE,
};
use super::label;

const KEY: &str = "colour_reference_library_v2";
const LIMIT: usize = 128;
const MAX_GROUPS: usize = 64;
const MAX_DOCUMENT: usize = 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowInput {
    pub action: String,
    pub expected_revision: Option<String>,
    pub group: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub reference_ids: Vec<String>,
    #[serde(default)]
    pub order: Vec<String>,
    pub favorite: Option<bool>,
    pub controls: Option<Controls>,
    pub document: Option<String>,
    #[serde(default)]
    pub replace_existing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSnapshot {
    /// The empty string is the fixed first group, displayed as Ungrouped.
    pub groups: Vec<String>,
    pub references: Vec<ReferenceSummary>,
    pub revision: String,
    pub document: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Organization {
    groups: Vec<String>,
    order: BTreeMap<String, Vec<String>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Portable {
    version: u32,
    engine: String,
    groups: Vec<String>,
    references: Vec<PortableReference>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableReference {
    id: String,
    name: String,
    width: u32,
    height: u32,
    converted_icc: bool,
    group: String,
    favorite: bool,
    default_controls: Option<Controls>,
    regions: Vec<PortableRegion>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortableRegion {
    name: String,
    appearance: aura_core::contract::look::LookAggregate,
}

fn db(error: &rusqlite::Error) -> AuraError {
    aura_core::errors::db::statement_failed("reference library workflow", error)
}

fn canonical_group(value: &str) -> AuraResult<String> {
    let value = value.trim();
    if value.is_empty() || value == "Ungrouped" {
        Ok(String::new())
    } else {
        label(value, 80)
    }
}

fn group_name(input: &WorkflowInput) -> AuraResult<String> {
    canonical_group(input.group.as_deref().unwrap_or_default())
}

fn load(conn: &Connection) -> AuraResult<(Organization, Vec<Reference>)> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value_json FROM setting WHERE key=?1",
            [KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| db(&e))?;
    let mut organization: Organization = stored
        .map(|text| {
            if text.len() > MAX_DOCUMENT {
                return Err(invalid("The reference organization is too large."));
            }
            serde_json::from_str(&text)
                .map_err(|_| invalid("The reference organization is unreadable."))
        })
        .transpose()?
        .unwrap_or_default();
    let mut query = conn.prepare("SELECT value_json FROM setting WHERE key LIKE 'colour_reference_v1:%' ORDER BY key LIMIT 129").map_err(|e| db(&e))?;
    let rows = query
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| db(&e))?;
    let mut references = Vec::new();
    let mut stored_bytes = 0;
    for (index, row) in rows.enumerate() {
        if index >= LIMIT {
            return Err(invalid("Organize at most 128 references at a time."));
        }
        let text = row.map_err(|e| db(&e))?;
        stored_bytes += text.len();
        if stored_bytes > 16 * MAX_DOCUMENT {
            return Err(invalid(
                "The reference library exceeds its 16 MB metadata budget.",
            ));
        }
        if text.len() > 8 * MAX_DOCUMENT {
            return Err(invalid("A saved reference exceeds its size limit."));
        }
        let mut reference: Reference =
            serde_json::from_str(&text).map_err(|_| invalid("A saved reference is unreadable."))?;
        if reference.engine != ENGINE {
            continue;
        }
        reference.summary.group = canonical_group(&reference.summary.group)?;
        references.push(reference);
    }
    if references.len() > LIMIT {
        return Err(invalid("Organize at most 128 references at a time."));
    }
    organization.groups = organization
        .groups
        .iter()
        .map(|group| canonical_group(group))
        .collect::<AuraResult<Vec<_>>>()?;
    organization.groups.retain(|group| !group.is_empty());
    let mut unique = BTreeSet::new();
    organization
        .groups
        .retain(|group| unique.insert(group.clone()));
    for reference in &references {
        if !reference.summary.group.is_empty() && unique.insert(reference.summary.group.clone()) {
            organization.groups.push(reference.summary.group.clone());
        }
    }
    if organization.groups.len() > MAX_GROUPS {
        return Err(invalid("Use at most 64 reference groups."));
    }
    let valid_ids: BTreeSet<_> = references
        .iter()
        .map(|reference| reference.summary.id.clone())
        .collect();
    organization
        .order
        .retain(|group, _| group.is_empty() || unique.contains(group));
    for ids in organization.order.values_mut() {
        let mut seen = BTreeSet::new();
        ids.retain(|id| valid_ids.contains(id) && seen.insert(id.clone()));
    }
    Ok((organization, references))
}

fn sorted<'a>(organization: &Organization, references: &'a [Reference]) -> Vec<&'a Reference> {
    let mut groups = vec![String::new()];
    groups.extend(organization.groups.clone());
    let mut out = Vec::new();
    for group in groups {
        let order = organization.order.get(&group);
        let mut members: Vec<_> = references
            .iter()
            .filter(|item| item.summary.group == group)
            .collect();
        members.sort_by(|a, b| {
            let position = |id: &str| {
                order
                    .and_then(|ids| ids.iter().position(|value| value == id))
                    .unwrap_or(usize::MAX)
            };
            position(&a.summary.id)
                .cmp(&position(&b.summary.id))
                .then_with(|| a.summary.name.cmp(&b.summary.name))
                .then_with(|| a.summary.id.cmp(&b.summary.id))
        });
        out.extend(members);
    }
    out
}

fn revision(organization: &Organization, references: &[Reference]) -> AuraResult<String> {
    let mut normalized = organization.clone();
    normalized
        .order
        .retain(|group, _| group.is_empty() || normalized.groups.contains(group));
    for ids in normalized.order.values_mut() {
        let mut seen = BTreeSet::new();
        ids.retain(|id| {
            references
                .iter()
                .any(|reference| reference.summary.id == *id)
                && seen.insert(id.clone())
        });
    }
    let mut references: Vec<_> = references.iter().collect();
    references.sort_by(|a, b| a.summary.id.cmp(&b.summary.id));
    let bytes = serde_json::to_vec(&(normalized, references))
        .map_err(|_| invalid("Cannot fingerprint the reference library."))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn exact_order(order: &[String], expected: &BTreeSet<String>) -> AuraResult<()> {
    if order.len() != expected.len()
        || order.iter().collect::<BTreeSet<_>>().len() != order.len()
        || order.iter().any(|value| !expected.contains(value))
    {
        Err(invalid(
            "The new order must contain every member exactly once. Refresh the library.",
        ))
    } else {
        Ok(())
    }
}

fn selected(input: &WorkflowInput, references: &[Reference]) -> AuraResult<BTreeSet<String>> {
    if input.reference_ids.is_empty() || input.reference_ids.len() > LIMIT {
        return Err(invalid("Select between one and 128 references."));
    }
    let ids: BTreeSet<_> = input.reference_ids.iter().cloned().collect();
    if ids.len() != input.reference_ids.len()
        || ids.iter().any(|id| {
            !references
                .iter()
                .any(|reference| reference.summary.id == *id)
        })
    {
        return Err(invalid(
            "A selected reference is missing or duplicated. Refresh the library.",
        ));
    }
    Ok(ids)
}

fn descriptor_ok(value: &aura_core::contract::look::LookAggregate) -> bool {
    let unit = |number: f32| number.is_finite() && (0.0..=1.0).contains(&number);
    // Lab chroma is normalized by 128 as a scale, not a ceiling; saturated blue exceeds one.
    let chroma = |number: f32| number.is_finite() && (0.0..=2.0).contains(&number);
    let tone = value.tone.as_array();
    value.samples > 0
        && value.samples <= 2_000
        && tone.iter().copied().all(unit)
        && tone
            .windows(2)
            .all(|pair| pair.first().zip(pair.last()).is_some_and(|(a, b)| a <= b))
        && unit(value.tone_spread)
        && chroma(value.chroma_p50)
        && chroma(value.chroma_p90)
        && value.chroma_p50 <= value.chroma_p90
        && value.rendered_cct_k.is_finite()
        && (0.0..=100_000.0).contains(&value.rendered_cct_k)
        && [value.shadow, value.mid, value.high].iter().all(|tint| {
            [tint.a, tint.b]
                .iter()
                .all(|number| number.is_finite() && (-160.0..=160.0).contains(number))
        })
        && value
            .bands
            .iter()
            .all(|band| unit(band.share) && chroma(band.chroma) && unit(band.luma))
}

impl PortableReference {
    fn from_reference(reference: &Reference) -> Self {
        let summary = &reference.summary;
        Self {
            id: summary.id.clone(),
            name: summary.name.clone(),
            width: summary.width,
            height: summary.height,
            converted_icc: summary.converted_icc,
            group: summary.group.clone(),
            favorite: summary.favorite,
            default_controls: summary.default_controls,
            regions: reference
                .regions
                .iter()
                .map(|region| PortableRegion {
                    name: region.name.clone(),
                    appearance: region.appearance.clone(),
                })
                .collect(),
        }
    }

    fn into_reference(self) -> AuraResult<Reference> {
        asset_key(&self.id)?;
        let name = label(&self.name, 120)?;
        let group = canonical_group(&self.group)?;
        if self.width < 8
            || self.height < 8
            || self.width > 16_000
            || self.height > 16_000
            || u64::from(self.width) * u64::from(self.height) > 64_000_000
        {
            return Err(invalid("Invalid portable reference dimensions."));
        }
        if let Some(controls) = self.default_controls {
            controls.check()?;
        }
        let allowed = [
            "whole image",
            "face skin",
            "body skin",
            "hair",
            "clothes",
            "background",
        ];
        let mut names = BTreeSet::new();
        if self.regions.is_empty()
            || self.regions.len() > allowed.len()
            || self.regions.iter().any(|region| {
                !allowed.contains(&region.name.as_str())
                    || !names.insert(region.name.clone())
                    || !descriptor_ok(&region.appearance)
            })
            || !names.contains("whole image")
        {
            return Err(invalid(
                "Invalid portable reference appearance descriptors.",
            ));
        }
        let regions: Vec<_> = self
            .regions
            .into_iter()
            .map(|region| super::super::appearance::Region {
                name: region.name,
                points: Vec::new(),
                appearance: region.appearance,
            })
            .collect();
        Ok(Reference {
            engine: ENGINE.into(),
            summary: ReferenceSummary {
                id: self.id,
                name,
                width: self.width,
                height: self.height,
                converted_icc: self.converted_icc,
                group,
                favorite: self.favorite,
                default_controls: self.default_controls,
                thumbnail: None,
                regions: regions.iter().map(|region| region.name.clone()).collect(),
            },
            regions,
        })
    }
}

fn portable(
    organization: &Organization,
    references: &[Reference],
    ids: &[String],
) -> AuraResult<String> {
    let selected_ids: BTreeSet<_> = ids.iter().collect();
    if !ids.is_empty()
        && (selected_ids.len() != ids.len()
            || ids.iter().any(|id| {
                !references
                    .iter()
                    .any(|reference| reference.summary.id == *id)
            }))
    {
        return Err(invalid("The export selection is missing or duplicated."));
    }
    let document = Portable {
        version: 1,
        engine: ENGINE.into(),
        groups: organization.groups.clone(),
        references: sorted(organization, references)
            .into_iter()
            .filter(|reference| ids.is_empty() || selected_ids.contains(&reference.summary.id))
            .map(PortableReference::from_reference)
            .collect(),
    };
    let text = serde_json::to_string_pretty(&document)
        .map_err(|_| invalid("Cannot export the reference library."))?;
    if text.len() > MAX_DOCUMENT {
        return Err(invalid(
            "The portable library exceeds 1 MB. Export a smaller selection.",
        ));
    }
    Ok(text)
}

fn import(
    input: &WorkflowInput,
    organization: &mut Organization,
    references: &mut Vec<Reference>,
) -> AuraResult<()> {
    let text = input
        .document
        .as_ref()
        .ok_or_else(|| invalid("Choose an AURA descriptor library."))?;
    if text.len() > MAX_DOCUMENT {
        return Err(invalid(
            "Portable reference libraries must be at most 1 MB.",
        ));
    }
    let document: Portable = serde_json::from_str(text)
        .map_err(|_| invalid("This is not a valid AURA descriptor library."))?;
    if document.version != 1
        || document.engine != ENGINE
        || document.references.len() > LIMIT
        || document.groups.len() > MAX_GROUPS
    {
        return Err(invalid(
            "The reference library version, engine or size is unsupported.",
        ));
    }
    let mut ids = BTreeSet::new();
    let incoming: Vec<_> = document
        .references
        .into_iter()
        .map(PortableReference::into_reference)
        .collect::<AuraResult<_>>()?;
    for reference in &incoming {
        if !ids.insert(reference.summary.id.clone()) {
            return Err(invalid(
                "A portable reference identifier appears more than once.",
            ));
        }
    }
    for group in document.groups.into_iter().chain(
        incoming
            .iter()
            .map(|reference| reference.summary.group.clone()),
    ) {
        let group = canonical_group(&group)?;
        if !group.is_empty() && !organization.groups.contains(&group) {
            organization.groups.push(group);
        }
    }
    for mut reference in incoming {
        if let Some(existing) = references
            .iter_mut()
            .find(|existing| existing.summary.id == reference.summary.id)
        {
            let descriptors = |reference: &Reference| {
                serde_json::to_value((
                    &reference.regions,
                    reference.summary.width,
                    reference.summary.height,
                    reference.summary.converted_icc,
                ))
                .map_err(|_| invalid("Cannot compare reference descriptors."))
            };
            if descriptors(existing)? != descriptors(&reference)? {
                return Err(invalid("A reference identifier conflicts with different appearance data; nothing was imported."));
            }
            if input.replace_existing {
                reference
                    .summary
                    .thumbnail
                    .clone_from(&existing.summary.thumbnail);
                *existing = reference;
            }
        } else {
            organization
                .order
                .entry(reference.summary.group.clone())
                .or_default()
                .push(reference.summary.id.clone());
            references.push(reference);
        }
    }
    if references.len() > LIMIT || organization.groups.len() > MAX_GROUPS {
        return Err(invalid(
            "The merged library exceeds 128 references or 64 groups.",
        ));
    }
    Ok(())
}

fn rename_or_delete_group(
    input: &WorkflowInput,
    organization: &mut Organization,
    references: &mut [Reference],
) -> AuraResult<()> {
    let group = group_name(input)?;
    let position = organization
        .groups
        .iter()
        .position(|existing| *existing == group)
        .ok_or_else(|| {
            invalid("Choose an existing named group. Ungrouped cannot be renamed or deleted.")
        })?;
    let next = if input.action == "delete_group" {
        String::new()
    } else {
        canonical_group(input.name.as_deref().unwrap_or_default())?
    };
    if input.action == "rename_group" && (next.is_empty() || organization.groups.contains(&next)) {
        return Err(invalid("Choose a different unused group name."));
    }
    if next.is_empty() {
        organization.groups.remove(position);
    } else {
        let name = organization
            .groups
            .get_mut(position)
            .ok_or_else(|| invalid("The selected group no longer exists."))?;
        name.clone_from(&next);
    }
    for reference in references
        .iter_mut()
        .filter(|reference| reference.summary.group == group)
    {
        reference.summary.group.clone_from(&next);
    }
    if let Some(ids) = organization.order.remove(&group) {
        organization.order.entry(next).or_default().extend(ids);
    }
    Ok(())
}

fn batch(
    input: &WorkflowInput,
    organization: &Organization,
    references: &mut Vec<Reference>,
) -> AuraResult<()> {
    let group = group_name(input)?;
    let ids = selected(input, references)?;
    if input.action == "move" && !group.is_empty() && !organization.groups.contains(&group) {
        return Err(invalid("Create the destination group first."));
    }
    if input.action == "defaults" {
        input
            .controls
            .ok_or_else(|| invalid("Choose matching defaults."))?
            .check()?;
    }
    if input.action == "favorite" && input.favorite.is_none() {
        return Err(invalid(
            "Choose whether to favorite the selected references.",
        ));
    }
    if input.action == "remove" {
        references.retain(|reference| !ids.contains(&reference.summary.id));
    } else {
        for reference in references
            .iter_mut()
            .filter(|reference| ids.contains(&reference.summary.id))
        {
            match input.action.as_str() {
                "move" => reference.summary.group.clone_from(&group),
                "favorite" => reference.summary.favorite = input.favorite.unwrap_or(false),
                "defaults" => reference.summary.default_controls = input.controls,
                "reset_defaults" => reference.summary.default_controls = None,
                _ => {}
            }
        }
    }
    Ok(())
}

fn apply(
    input: &WorkflowInput,
    organization: &mut Organization,
    references: &mut Vec<Reference>,
) -> AuraResult<()> {
    let group = group_name(input)?;
    match input.action.as_str() {
        "create_group" => {
            if group.is_empty()
                || organization.groups.contains(&group)
                || organization.groups.len() >= MAX_GROUPS
            {
                return Err(invalid(
                    "Choose a new group name; at most 64 groups are supported.",
                ));
            }
            organization.groups.push(group);
        }
        "rename_group" | "delete_group" => rename_or_delete_group(input, organization, references)?,
        "reorder_groups" => {
            exact_order(&input.order, &organization.groups.iter().cloned().collect())?;
            organization.groups.clone_from(&input.order);
        }
        "reorder_references" => {
            if !group.is_empty() && !organization.groups.contains(&group) {
                return Err(invalid("This group no longer exists."));
            }
            let ids = references
                .iter()
                .filter(|reference| reference.summary.group == group)
                .map(|reference| reference.summary.id.clone())
                .collect();
            exact_order(&input.order, &ids)?;
            organization.order.insert(group, input.order.clone());
        }
        "move" | "remove" | "favorite" | "defaults" | "reset_defaults" => {
            batch(input, organization, references)?;
        }
        "import" => import(input, organization, references)?,
        _ => return Err(invalid("Unknown reference-library workflow.")),
    }
    Ok(())
}

/// Read, atomically organize, export or import the descriptor library.
///
/// # Errors
/// Refuses stale revisions, invalid selections, unsupported documents and catalogue failures.
/// Source files, recipes, matching sessions and history are never modified.
pub fn run(
    state: &AppState,
    input: &WorkflowInput,
) -> crate::commands::IpcResult<WorkflowSnapshot> {
    if input.reference_ids.len() > LIMIT
        || input.order.len() > LIMIT
        || input
            .document
            .as_ref()
            .is_some_and(|text| text.len() > MAX_DOCUMENT)
    {
        return Err(invalid("This reference-library request exceeds its size limit.").into());
    }
    let input = input.clone();
    let at = aura_catalog::rfc3339(state.clock().now_utc());
    Ok(state.catalog().writer().with(move |conn| {
        let transaction = conn.unchecked_transaction().map_err(|e| db(&e))?;
        let (mut organization, mut references) = load(&transaction)?;
        let before = revision(&organization, &references)?;
        let previous_ids: BTreeSet<_> = references.iter().map(|reference| reference.summary.id.clone()).collect();
        let document = if input.action == "export" { Some(portable(&organization,&references,&input.reference_ids)?) } else { None };
        let mut changed = false;
        if !matches!(input.action.as_str(), "read" | "export") {
            if input.expected_revision.as_deref() != Some(before.as_str()) { return Err(invalid("The reference library changed. Refresh it before applying this action.")); }
            apply(&input,&mut organization,&mut references)?;
            changed = revision(&organization, &references)? != before;
            if changed {
                let count: i64 = transaction.query_row("SELECT COUNT(*) FROM setting WHERE key LIKE 'colour_reference_v1:%'",[],|row| row.get(0)).map_err(|e| db(&e))?;
                let count = usize::try_from(count).map_err(|_| invalid("The reference count is invalid."))?;
                let added = references.iter().filter(|reference| !previous_ids.contains(&reference.summary.id)).count();
                let removed = previous_ids.iter().filter(|id| !references.iter().any(|reference| reference.summary.id == **id)).count();
                if added > LIMIT.saturating_sub(count) + removed { return Err(invalid("The merged library exceeds 128 references, including references from older engines.")); }
                let text = serde_json::to_string(&organization).map_err(|_| invalid("Cannot save reference organization."))?;
                transaction.execute("INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",rusqlite::params![KEY,text,at]).map_err(|e| db(&e))?;
                for reference in &references {
                    if !previous_ids.contains(&reference.summary.id) {
                        let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM setting WHERE key=?1)",[asset_key(&reference.summary.id)?],|row| row.get(0)).map_err(|e| db(&e))?;
                        if exists { return Err(invalid("This identifier belongs to an older reference engine. Reimport the original photo before transferring it.")); }
                    }
                    let text = serde_json::to_string(reference).map_err(|_| invalid("Cannot save reference metadata."))?;
                    transaction.execute("INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",rusqlite::params![asset_key(&reference.summary.id)?,text,at]).map_err(|e| db(&e))?;
                }
                for id in previous_ids.into_iter().filter(|id| !references.iter().any(|reference| reference.summary.id == *id)) {
                    transaction.execute("DELETE FROM setting WHERE key=?1",[format!("{ASSET}{id}")]).map_err(|e| db(&e))?;
                }
            }
        }
        let revision = revision(&organization,&references)?;
        let summaries = sorted(&organization,&references).into_iter().map(|reference| reference.summary.clone()).collect();
        let mut groups = vec![String::new()]; groups.extend(organization.groups);
        transaction.commit().map_err(|e| db(&e))?;
        Ok(WorkflowSnapshot { groups, references: summaries, revision, document, changed })
    })?)
}

/// Read references in durable group and per-group order without mutating metadata.
pub(in crate::color_match) fn ordered_references(
    state: &AppState,
) -> crate::commands::IpcResult<Vec<ReferenceSummary>> {
    Ok(state.catalog().read(|conn| {
        let (organization, references) = load(conn)?;
        Ok(sorted(&organization, &references)
            .into_iter()
            .map(|reference| reference.summary.clone())
            .collect())
    })?)
}

#[cfg(test)]
#[path = "workflows/tests/mod.rs"]
mod tests;
