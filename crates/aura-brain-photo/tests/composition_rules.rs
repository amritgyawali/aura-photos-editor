use std::sync::Arc;

use aura_brain_photo::composition::store::Provenance;
use aura_brain_photo::CompositionStore;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::composition::{
    CompositionCode, CompositionFlags, CompositionReason, CompositionResult, HorizonSource,
};
use aura_core::errors::db::statement_failed;
use aura_core::{MomentId, PhotoId, ProjectId, SceneId};
use rusqlite::params;
use tempfile::TempDir;

struct Harness {
    _dir: TempDir,
    catalog: Arc<Catalog>,
    store: CompositionStore,
    project: ProjectId,
    moment: MomentId,
    photos: [PhotoId; 4],
}

fn harness() -> Result<Harness, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(Catalog::open(
        &dir.path().join("composition.sqlite"),
        Arc::clone(&clock),
        "0.1.0-test",
    )?);
    let project = ProjectId::new();
    let moment = MomentId::new();
    let photos = [
        PhotoId::new(),
        PhotoId::new(),
        PhotoId::new(),
        PhotoId::new(),
    ];
    let project_key = project.to_db();
    let moment_key = moment.to_db();
    let photo_keys = photos.map(|photo| photo.to_db());

    catalog.writer().transact(move |conn| {
        conn.execute(
            "INSERT INTO project (project_id, name, created_at, updated_at)
             VALUES (?1, 'Composition test', ?2, ?2)",
            params![project_key, "2026-08-15T00:00:00Z"],
        )
        .map_err(|error| statement_failed("could not seed a composition project", &error))?;
        for key in &photo_keys {
            conn.execute(
                "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)",
                params![key, project_key, "2026-08-15T00:00:00Z"],
            )
            .map_err(|error| statement_failed("could not seed a composition photo", &error))?;
        }
        conn.execute(
            "INSERT INTO moments
                (id, project_id, start_ts, end_ts, frame_count, created_at, updated_at)
             VALUES (?1, ?2, 0, 3, 4, ?3, ?3)",
            params![moment_key, project_key, "2026-08-15T00:00:00Z"],
        )
        .map_err(|error| statement_failed("could not seed a composition moment", &error))?;
        for (position, key) in photo_keys.iter().enumerate() {
            conn.execute(
                "INSERT INTO moment_images (moment_id, photo_id, position, burst_ix)
                 VALUES (?1, ?2, ?3, 0)",
                params![moment_key, key, i64::try_from(position).unwrap_or(0)],
            )
            .map_err(|error| {
                statement_failed("could not seed a composition moment member", &error)
            })?;
        }
        Ok(())
    })?;

    Ok(Harness {
        _dir: dir,
        store: CompositionStore::new(Arc::clone(&catalog), clock),
        catalog,
        project,
        moment,
        photos,
    })
}

fn result(image_id: PhotoId, score: f32) -> CompositionResult {
    CompositionResult {
        image_id,
        tilt_deg: 3.0,
        tilt_intentional: false,
        horizon_conf: 0.9,
        horizon_source: HorizonSource::Gradient,
        headroom: 0.1,
        thirds_offset: 0.1,
        balance: 0.8,
        negative_space: 0.1,
        joint_cuts: Vec::new(),
        head_crop: false,
        edge_intrusions: Vec::new(),
        clutter: 0.2,
        bright_blobs: Vec::new(),
        head_merge: false,
        colour_competition: 0.1,
        aesthetic: 0.5,
        composition_score: score,
        crop_suggestion_hint: None,
        scene: SceneId::CouplePortrait,
        relative_composition: 0.5,
        keypoint_subjects: 1,
        flags: CompositionFlags::HORIZON_TILTED,
        reasons: vec![CompositionReason::frame(
            CompositionCode::HorizonTilted,
            CompositionCode::HorizonTilted.user_text(),
            -0.2,
        )],
        confidence: 0.9,
        user_reviewed: false,
        model_ver: 100,
        analysis_ver: 1,
        rules_ver: 1,
    }
}

#[test]
fn persistence_review_and_relative_ranking_are_consistent() -> Result<(), Box<dyn std::error::Error>>
{
    let harness = harness()?;
    let provenance = Provenance::default();
    harness.store.put(
        &harness.project,
        &result(harness.photos[0], 0.9),
        &provenance,
    )?;
    harness.store.put(
        &harness.project,
        &result(harness.photos[1], 0.9),
        &provenance,
    )?;
    harness.store.put(
        &harness.project,
        &result(harness.photos[2], 0.2),
        &provenance,
    )?;

    let ranked = harness.store.relative_composition(harness.moment)?;
    assert_eq!(
        ranked.len(),
        3,
        "an unscored moment member must be excluded"
    );
    let first = ranked
        .iter()
        .find_map(|(photo, rank)| (*photo == harness.photos[0]).then_some(*rank));
    let tied = ranked
        .iter()
        .find_map(|(photo, rank)| (*photo == harness.photos[1]).then_some(*rank));
    assert_eq!(
        first, tied,
        "equal composition scores must have equal ranks"
    );

    harness
        .store
        .dismiss(harness.photos[0], CompositionFlags::HORIZON_TILTED)?;
    let reviewed = harness.store.get(harness.photos[0])?;
    assert!(reviewed.as_ref().is_some_and(|stored| {
        stored.user_reviewed
            && !stored.flags.contains(CompositionFlags::HORIZON_TILTED)
            && stored
                .reasons
                .iter()
                .all(|reason| reason.code != CompositionCode::HorizonTilted)
    }));

    harness.store.put(
        &harness.project,
        &result(harness.photos[0], 0.8),
        &provenance,
    )?;
    let reanalysed = harness.store.get(harness.photos[0])?;
    assert!(reanalysed.as_ref().is_some_and(|stored| {
        stored.user_reviewed
            && !stored.flags.contains(CompositionFlags::HORIZON_TILTED)
            && stored
                .reasons
                .iter()
                .all(|reason| reason.code != CompositionCode::HorizonTilted)
    }));

    let corrupt = harness.photos[1].to_db();
    harness.catalog.writer().transact(move |conn| {
        conn.execute(
            "UPDATE image_composition SET reasons = '{bad' WHERE photo_id = ?1",
            params![corrupt],
        )
        .map_err(|error| statement_failed("could not inject corrupt test JSON", &error))?;
        Ok(())
    })?;
    assert!(
        harness.store.get(harness.photos[1]).is_err(),
        "corrupt stored JSON must return a typed error"
    );
    Ok(())
}
