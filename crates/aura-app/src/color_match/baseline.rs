//! Stable reference-session baselines, separately guarded against the observed current edit.
use aura_core::{AuraResult, PhotoId};
use aura_recipe::Recipe;

use super::{invalid, read, session_key, AppState, Session, ENGINE};

#[derive(Debug)]
pub(super) struct AnalysisBaseline {
    pub(super) recipe: Recipe,
    pub(super) observed_hash: String,
}

impl AnalysisBaseline {
    pub(super) fn select(state: &AppState, photo: PhotoId, reference: &str) -> AuraResult<Self> {
        let current = crate::develop_commands::load_or_neutral(state, photo)?;
        let observed_hash = aura_recipe::hash::recipe_hash(&current)?;
        let previous: Option<Session> = read(state, &session_key(&photo.to_db())?)?;
        let recipe = previous
            .filter(|session| {
                session.engine == ENGINE
                    && session.photo_id == photo.to_db()
                    && session.reference.id == reference
                    && (session.last_hash == observed_hash
                        || session.baseline_hash == observed_hash)
                    && aura_recipe::hash::recipe_hash(&session.baseline)
                        .is_ok_and(|hash| hash == session.baseline_hash)
            })
            .map_or(current, |session| session.baseline);
        Ok(Self {
            recipe,
            observed_hash,
        })
    }
}

pub(super) fn verify_current(
    state: &AppState,
    photo: PhotoId,
    expected_hash: &str,
) -> AuraResult<()> {
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::hash::recipe_hash(&recipe)? != expected_hash {
        return Err(invalid(
            "The photo changed during analysis. Your newer edit was kept. Analyse it again.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{new_summary, store, Controls};
    use super::*;
    use aura_core::contract::style::StyleDelta;
    use aura_core::ProjectId;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn a_manual_edit_between_baseline_selection_and_verification_is_never_adopted() {
        let directory = tempfile::tempdir().expect("baseline race catalogue");
        let state = AppState::open(&directory.path().join("baseline.aura")).expect("state");
        let project = crate::create_project(
            &state,
            crate::contract::ipc::CreateProjectInput {
                name: "Baseline race".into(),
                couple_names: None,
                event_date: None,
            },
        )
        .expect("project");
        let project = ProjectId::from_db(&project.id).expect("project id");
        let photo = PhotoId::new();
        let photo_key = photo.to_db();
        let project_key = project.to_db();
        state.catalog().writer().with(move |conn| {
            conn.execute("INSERT INTO photo (photo_id, project_id, created_at, updated_at) VALUES (?1, ?2, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')", [&photo_key, &project_key])
                .map_err(|e| aura_core::errors::db::statement_failed("baseline race photo", &e))?;
            Ok(())
        }).expect("photo");
        let baseline = crate::develop_commands::neutral_recipe(&state, photo);
        let mut shown = baseline.clone();
        shown.global.saturation = 12;
        state
            .recipe_store()
            .save_first(
                &project,
                &photo,
                &shown,
                &["global.saturation".into()],
                "Earlier reference",
            )
            .expect("shown edit");
        let shown_hash = aura_recipe::hash::recipe_hash(&shown).expect("shown hash");
        let reference = "a".repeat(64);
        let session = Session {
            engine: ENGINE.into(),
            id: "baseline-race".into(),
            photo_id: photo.to_db(),
            reference: new_summary(reference.clone(), "Reference".into(), 64, 64, false),
            baseline_hash: aura_recipe::hash::recipe_hash(&baseline).expect("baseline hash"),
            baseline: baseline.clone(),
            last_hash: shown_hash.clone(),
            fitted: StyleDelta::neutral(),
            reference_regions: Vec::new(),
            target_regions: Vec::new(),
            before_distance: 1.0,
            fitted_distance: 0.5,
            controls: Controls::default(),
            checked_hash: None,
            measured_before: None,
            measured_after: None,
            fitting: None,
            reference_monochrome: false,
            grade_offsets: [0.0; 6],
            transfer: None,
        };
        store(&state, &session_key(&photo.to_db()).expect("key"), &session).expect("session");
        let selected =
            AnalysisBaseline::select(&state, photo, &reference).expect("select stable baseline");
        assert_eq!(selected.recipe, baseline);
        assert_eq!(selected.observed_hash, shown_hash);
        let different =
            AnalysisBaseline::select(&state, photo, &"b".repeat(64)).expect("other reference");
        assert_eq!(
            different.recipe, shown,
            "different references retain their earlier-look crossfade"
        );
        let mut manual = shown;
        manual.global.exposure = 0.25;
        state
            .recipe_store()
            .save_if_current(
                &project,
                &photo,
                &manual,
                &["global.exposure".into()],
                "Concurrent manual edit",
                &selected.observed_hash,
            )
            .expect("manual write during analysis");
        assert!(verify_current(&state, photo, &selected.observed_hash).is_err());
        assert_eq!(
            state
                .recipe_store()
                .load(&photo)
                .expect("manual kept")
                .expect("saved"),
            manual
        );
        let refreshed =
            AnalysisBaseline::select(&state, photo, &reference).expect("explicit fresh analysis");
        assert_eq!(
            refreshed.recipe, manual,
            "a stale session must not discard a newer manual baseline"
        );
    }
}
