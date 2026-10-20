//! Independent semantic-region fitting through the native local-adjustment renderer.
//! A session owns immutable target selections; no competing editor's code or weights are used.
use std::collections::BTreeMap;

use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_recipe::{local_masks, schema, EditSource, Recipe};
use serde::{Deserialize, Serialize};

use super::{appearance, invalid, read, rendered, store, ReferenceSummary};
use crate::{commands::IpcResult, AppState};

mod fit;
mod selections;

const ENGINE: &str = "semantic-local-reference-v1";
const SESSION: &str = "regional_colour_match_v1:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    FaceSkin,
    BodySkin,
    Hair,
    Clothes,
    Background,
}
impl Kind {
    const ALL: [Self; 5] = [
        Self::FaceSkin,
        Self::BodySkin,
        Self::Hair,
        Self::Clothes,
        Self::Background,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::FaceSkin => "face skin",
            Self::BodySkin => "body skin",
            Self::Hair => "hair",
            Self::Clothes => "clothes",
            Self::Background => "background",
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::FaceSkin => "face_skin",
            Self::BodySkin => "body_skin",
            Self::Hair => "hair",
            Self::Clothes => "clothes",
            Self::Background => "background",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Controls {
    pub amount: f32,
    pub tone: f32,
    pub color: f32,
    pub white_balance: f32,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            amount: 1.0,
            tone: 1.0,
            color: 1.0,
            white_balance: 1.0,
        }
    }
}
impl Controls {
    fn check(self) -> AuraResult<()> {
        if [self.amount, self.tone, self.color, self.white_balance]
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=2.0).contains(&v))
        {
            Ok(())
        } else {
            Err(invalid(
                "Regional matching strengths must be between 0 and 200%.",
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareInput {
    pub photo_id: String,
    pub reference_id: String,
    pub cancel_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInput {
    pub photo_id: String,
    pub session_id: String,
    pub recipe_hash: String,
    pub strengths: BTreeMap<Kind, Controls>,
    pub cancel_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unavailable {
    pub name: Kind,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionReport {
    pub name: Kind,
    pub controls: Controls,
    pub before_distance: f32,
    pub after_distance: f32,
    pub correction_available: bool,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchReport {
    pub session_id: String,
    pub photo_id: String,
    pub reference: ReferenceSummary,
    pub recipe_hash: String,
    pub current: bool,
    pub regions: Vec<RegionReport>,
    pub unavailable: Vec<Unavailable>,
    pub full_resolution_checked: bool,
    pub changed: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct FittedRegion {
    kind: Kind,
    matte: aura_recipe::retouch_tools::Matte,
    fitted: aura_core::contract::style::StyleDelta,
    controls: Controls,
    before: f32,
    after: f32,
}
#[derive(Debug, Serialize, Deserialize)]
struct Session {
    engine: String,
    id: String,
    photo_id: String,
    reference: ReferenceSummary,
    baseline: Recipe,
    baseline_hash: String,
    last_hash: String,
    regions: Vec<FittedRegion>,
    unavailable: Vec<Unavailable>,
    reference_regions: Vec<appearance::Region>,
    target_regions: Vec<appearance::Region>,
    checked_hash: Option<String>,
}

fn key(photo: &str) -> AuraResult<String> {
    PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo."))?;
    Ok(format!("{SESSION}{photo}"))
}
fn current_hash(state: &AppState, photo: &str) -> AuraResult<String> {
    let photo = PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo."))?;
    aura_recipe::hash::recipe_hash(&crate::develop_commands::load_or_neutral(state, photo)?)
}
fn validate_session(session: &Session, photo_id: &str) -> AuraResult<()> {
    let unique: std::collections::BTreeSet<_> = session.regions.iter().map(|r| r.kind).collect();
    if session.photo_id != photo_id
        || session.regions.is_empty()
        || session.regions.len() > Kind::ALL.len()
        || unique.len() != session.regions.len()
        || aura_recipe::hash::recipe_hash(&session.baseline)? != session.baseline_hash
    {
        return Err(invalid(
            "The saved regional analysis is invalid. Analyse again.",
        ));
    }
    for region in &session.regions {
        region.controls.check()?;
        selections::validate(&region.matte)?;
        if !region.before.is_finite()
            || !region.after.is_finite()
            || !session
                .reference_regions
                .iter()
                .any(|r| r.name == region.kind.name())
            || !session
                .target_regions
                .iter()
                .any(|r| r.name == region.kind.name() && r.points.len() >= 32)
        {
            return Err(invalid(
                "A saved regional measurement is invalid. Analyse again.",
            ));
        }
    }
    Ok(())
}
fn report(session: &Session, hash: String, changed: usize) -> MatchReport {
    let at_baseline = hash == session.baseline_hash && hash != session.last_hash;
    MatchReport {
        session_id: session.id.clone(),
        photo_id: session.photo_id.clone(),
        reference: session.reference.clone(),
        current: session.engine == ENGINE
            && (hash == session.last_hash || hash == session.baseline_hash),
        full_resolution_checked: session.checked_hash.as_deref() == Some(hash.as_str()),
        recipe_hash: hash,
        regions: session
            .regions
            .iter()
            .map(|r| RegionReport {
                name: r.kind,
                controls: if at_baseline {
                    Controls {
                        amount: 0.0,
                        ..r.controls
                    }
                } else {
                    r.controls
                },
                before_distance: r.before,
                after_distance: if at_baseline { r.before } else { r.after },
                correction_available: !r.fitted.is_neutral(),
                reason: r.fitted.is_neutral().then(|| "No bounded correction improved this region while passing the skin and highlight checks.".into()),
            })
            .collect(),
        unavailable: session.unavailable.clone(),
        changed,
    }
}

/// Restore independently fitted semantic-region strengths; later manual edits remain stale.
/// # Errors
/// Invalid photo or unreadable saved session.
pub fn status(state: &AppState, photo_id: &str) -> IpcResult<Option<MatchReport>> {
    let Some(session) = read::<Session>(state, &key(photo_id)?)? else {
        return Ok(None);
    };
    validate_session(&session, photo_id)?;
    Ok(Some(report(&session, current_hash(state, photo_id)?, 0)))
}

/// Fit local corrections only where both photos contain a supported, measured region.
/// # Errors
/// Missing reference/initial recipe/regions, rendering, cancellation, or a stale edit.
pub fn prepare(state: &AppState, input: &PrepareInput) -> IpcResult<MatchReport> {
    super::job(state, &input.cancel_id, |cancel| {
        let session = fit::prepare(state, input, cancel)?;
        let result = report(&session, session.baseline_hash.clone(), 0);
        store(state, &key(&input.photo_id)?, &session)?;
        Ok(result)
    })
}

fn validate_apply(session: &Session, input: &ApplyInput, hash: &str) -> AuraResult<()> {
    if session.engine != ENGINE
        || session.id != input.session_id
        || session.photo_id != input.photo_id
    {
        return Err(invalid(
            "This regional analysis is no longer current. Analyse again.",
        ));
    }
    if input.recipe_hash != hash || (hash != session.last_hash && hash != session.baseline_hash) {
        return Err(invalid(
            "The photo changed after regional analysis. Your newer edit was kept. Analyse again.",
        ));
    }
    if input.strengths.len() != session.regions.len()
        || session
            .regions
            .iter()
            .any(|r| !input.strengths.contains_key(&r.kind))
    {
        return Err(invalid(
            "Supply strengths for exactly the regions this analysis supports.",
        ));
    }
    for controls in input.strengths.values() {
        controls.check()?;
    }
    Ok(())
}

/// Save real native local masks from a frozen baseline, guarded on actual full-resolution output.
/// # Errors
/// Invalid/missing region strengths, stale edits, cancellation, or failed native-image guards.
pub fn apply(state: &AppState, input: &ApplyInput) -> IpcResult<MatchReport> {
    super::job(state, &input.cancel_id, |cancel| {
        let session_key = key(&input.photo_id)?;
        let mut session: Session = read(state, &session_key)?
            .ok_or_else(|| invalid("Analyse regional reference matching first."))?;
        validate_session(&session, &input.photo_id)?;
        let hash = current_hash(state, &input.photo_id)?;
        validate_apply(&session, input, &hash)?;
        let candidate = fit::candidate(&session, &input.strengths)?;
        let distances = fit::full_check(state, &session, &candidate, cancel)?;
        super::check_cancel(cancel)?;
        let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
        let current = crate::develop_commands::load_or_neutral(state, photo)?;
        // User-driven addition/removal of this session's masks. Every baseline/manual mask and
        // every non-mask field is carried verbatim; unrelated protected fields never move.
        let (mut merged, mut changes) = schema::merge(&current, &candidate, EditSource::User)?;
        for extension in [local_masks::KEY, local_masks::MATTE_KEY] {
            if merged.extra.get(extension) != candidate.extra.get(extension) {
                if let Some(value) = candidate.extra.get(extension) {
                    merged.extra.insert(extension.into(), value.clone());
                } else {
                    merged.extra.remove(extension);
                }
                let path = extension.to_owned();
                if !changes.changed.contains(&path) {
                    changes.changed.push(path);
                }
            }
        }
        schema::Validation::check(&merged)?;
        let project: String = state.catalog().read(|conn| {
            conn.query_row(
                "SELECT project_id FROM photo WHERE photo_id=?1",
                [&input.photo_id],
                |r| r.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("regional colour photo", &e))
        })?;
        let project = ProjectId::from_db(&project).map_err(|_| invalid("Invalid collection."))?;
        super::check_cancel(cancel)?;
        if !changes.changed.is_empty() {
            state.recipe_store().save_if_current(&project, &photo, &merged, &changes.changed,
                &format!("Regional reference colour: {}; native-resolution skin and highlight checks passed", session.reference.name), &hash)?;
        } else if current_hash(state, &input.photo_id)? != hash {
            return Err(invalid(
                "The photo changed during regional checks. Your newer edit was kept.",
            ));
        }
        session.last_hash = aura_recipe::hash::recipe_hash(&merged)?;
        for region in &mut session.regions {
            region.controls = input
                .strengths
                .get(&region.kind)
                .copied()
                .ok_or_else(|| invalid("Missing regional strengths."))?;
            if let Some((before, after)) = distances.get(&region.kind) {
                region.before = *before;
                region.after = *after;
            }
        }
        session.checked_hash = Some(session.last_hash.clone());
        store(state, &session_key, &session)?;
        Ok(report(
            &session,
            session.last_hash.clone(),
            changes.changed.len(),
        ))
    })
}

#[cfg(test)]
mod tests;
