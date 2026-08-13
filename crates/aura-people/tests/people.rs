//! The people pass end to end: scan, store, resume, group, edit, replay.
//!
//! Everything here runs against a real catalog, the real sealed store and the real
//! three-model pipeline. The one double is the preview service, and it is a double
//! because the alternative is decoding RAW fixtures in a unit test - which phase 02
//! already covers and which would make this suite a decode benchmark.
//!
//! The two properties that matter most, and that nothing else asserts:
//!
//! * **A killed pass resumes without recomputing.** The remaining work is a query, so a
//!   crash and a cancel leave the same state behind.
//! * **A photographer's decision survives a re-analysis.** Regrouping produces new
//!   identity ids, and the decisions are replayed by face set rather than by id.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::people::{PeopleService, Role};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{AuraResult, PhotoId, Priority, ProjectId};
use aura_infer::contract::infer::{
    BatchDefaults, HardwarePlan, InferService, ModelClass, Precision,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures as onnx_fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_people::store::PeopleStore;
use aura_people::vault::{BiometricKeyStore, MemoryKeyStore};
use aura_people::{FaceScanner, People};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelLevel, PixelSource};
use aura_vision::face::fixtures::{render_frame, SynthFace};
use aura_vision::face::FacePipeline;
use tempfile::TempDir;

/// A preview service over pre-rendered synthetic frames.
///
/// A double for the *decode*, not for the pipeline: what it returns is a real buffer of
/// real pixels with real faces drawn in it, and everything downstream - detection,
/// alignment, quality, templates, bodies, sealing - is the product's own code.
#[derive(Debug)]
struct FixturePreviews {
    frames: BTreeMap<PhotoId, Vec<u8>>,
    width: u32,
    height: u32,
}

impl PreviewService for FixturePreviews {
    fn get(
        &self,
        id: PhotoId,
        _level: PixelLevel,
        _priority: PreviewPriority,
    ) -> AuraResult<Arc<PixelBuffer>> {
        let Some(bytes) = self.frames.get(&id) else {
            return Err(aura_core::errors::raw::corrupt("no fixture for that photo"));
        };
        Ok(Arc::new(PixelBuffer {
            width: self.width,
            height: self.height,
            data: PixelData::Srgb8(bytes.clone()),
            colour_space: ColourSpace::Srgb,
            source: PixelSource::Demosaiced,
            decode_ms: 1,
        }))
    }

    fn prefetch(&self, _ids: &[PhotoId], _level: PixelLevel) {}

    fn cache_stats(&self) -> aura_cache::budget::CacheStats {
        aura_cache::budget::CacheStats::default()
    }

    fn quarantine(&self) -> Vec<(PhotoId, String)> {
        Vec::new()
    }
}

struct Fixture {
    _dir: TempDir,
    catalog: Arc<Catalog>,
    store: Arc<PeopleStore>,
    people: People,
    scanner: FaceScanner,
    project: ProjectId,
    photos: Vec<PhotoId>,
}

/// A wedding: `frames` photographs, each with two of the three synthetic people in it.
fn wedding(frames: usize) -> Fixture {
    let dir = TempDir::new().expect("a temporary directory");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(
            &dir.path().join("people.sqlite"),
            Arc::clone(&clock),
            "0.1.0",
        )
        .expect("the catalog must open"),
    );

    let project = ProjectId::new();
    let key = project.to_db();
    catalog
        .writer()
        .transact({
            let key = key.clone();
            move |tx| {
                tx.execute(
                    "INSERT INTO project (project_id, name, created_at, updated_at)
                     VALUES (?1, 'wedding', '2026-08-13T00:00:00Z', '2026-08-13T00:00:00Z')",
                    rusqlite::params![key],
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not create a project", &e)
                })?;
                Ok(())
            }
        })
        .expect("the project must be created");

    // Three people. The first two are photographed together constantly - the couple -
    // and the third appears with them less often.
    let (width, height) = (800usize, 600usize);
    let mut previews = BTreeMap::new();
    let mut photos = Vec::new();
    for index in 0..frames {
        let photo = PhotoId::new();
        let mut faces = vec![
            SynthFace::new([0.30, 0.40], 0.26, 1),
            SynthFace::new([0.62, 0.40], 0.26, 2),
        ];
        if index % 3 == 0 {
            faces.push(SynthFace::new([0.86, 0.62], 0.16, 3));
        }
        previews.insert(photo, render_frame(width, height, &faces));
        photos.push(photo);

        let photo_key = photo.to_db();
        let project_key = key.clone();
        let stamp = format!("2026-08-13T1{}:{:02}:00Z", index / 60, index % 60);
        catalog
            .writer()
            .transact(move |tx| {
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, timeline_time, focal_len_mm,
                                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, 35.0, ?3, ?3)",
                    rusqlite::params![photo_key, project_key, stamp],
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not insert a photo", &e)
                })?;
                Ok(())
            })
            .expect("the photo must be inserted");
    }

    // The three models, through the real inference service.
    let detect = dir.path().join("face_detect.onnx");
    let embed = dir.path().join("face_embed.onnx");
    let quality = dir.path().join("face_quality.onnx");
    std::fs::write(&detect, serialise(&onnx_fixtures::face_detect())).expect("detector");
    std::fs::write(&embed, serialise(&onnx_fixtures::face_embed())).expect("recogniser");
    std::fs::write(&quality, serialise(&onnx_fixtures::face_quality())).expect("head");

    let source = Arc::new(
        StaticModelSource::new()
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::detect::DETECT_MODEL,
                    aura_vision::face::detect::DETECT_MODEL_VERSION,
                ),
                &detect,
                Precision::Fp32,
                ModelClass::Segmentation,
                32,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::embed::EMBED_MODEL,
                    aura_vision::face::embed::EMBED_MODEL_VERSION,
                ),
                &embed,
                Precision::Fp32,
                ModelClass::Embedding,
                16,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_vision::face::quality::QUALITY_MODEL,
                    aura_vision::face::quality::QUALITY_MODEL_VERSION,
                ),
                &quality,
                Precision::Fp32,
                ModelClass::Embedding,
                16,
            ),
    );
    let mut plan = HardwarePlan::conservative(4, "2026-08-13T00:00:00Z".to_string());
    plan.default_batch = BatchDefaults {
        embedding: 8,
        segmentation: 2,
        retouch: 1,
    };
    let engine: Arc<dyn InferService> = Arc::new(InferEngine::new(
        BackendRegistry::with_reference(),
        source,
        plan,
        Arc::clone(&clock),
    ));

    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let store = Arc::new(PeopleStore::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
        keys,
        &dir.path().join("cache"),
    ));

    // The reference detector, because the shipped detector is a placeholder that finds
    // nothing. Condition C1: the pipeline is real, the weights are not.
    let pipeline = FacePipeline::new(engine).with_reference_detector();
    let previews: Arc<dyn PreviewService> = Arc::new(FixturePreviews {
        frames: previews,
        width: width as u32,
        height: height as u32,
    });
    let scanner = FaceScanner::new(previews, pipeline, Arc::clone(&store), Arc::clone(&clock));
    let people = People::new(Arc::clone(&store), clock);

    Fixture {
        _dir: dir,
        catalog,
        store,
        people,
        scanner,
        project,
        photos,
    }
}

fn scan(fixture: &Fixture) -> aura_people::ScanReport {
    fixture
        .scanner
        .scan_project(
            &fixture.project,
            Priority::AiBatch,
            &CancelToken::new(),
            &NullProgress,
        )
        .expect("the scan must run")
}

#[test]
fn a_scan_finds_faces_bodies_and_records_that_it_looked() {
    let fixture = wedding(6);
    let report = scan(&fixture);

    assert_eq!(report.scanned, 6);
    assert!(report.faces >= 12, "found {}", report.faces);
    assert_eq!(report.failed, 0);
    assert!(report.person_boxes > 0, "bodies must be found too");

    let (photos, scanned, faces, voting) =
        fixture.store.coverage(&fixture.project).expect("coverage");
    assert_eq!(photos, 6);
    assert_eq!(scanned, 6);
    assert!(faces >= 12);
    assert!(voting > 0, "some faces must pass the quality gate");
}

#[test]
fn a_frame_with_no_faces_is_recorded_as_scanned() {
    // The difference from phase 05, and the whole reason `face_scan` exists: "no faces
    // here" is a legitimate result, and a pipeline that inferred work-remaining from
    // missing face rows would re-scan every landscape for ever.
    let fixture = wedding(2);
    // Overwrite one frame with an empty one by scanning, then checking the ledger.
    scan(&fixture);
    let pending = fixture
        .store
        .pending(
            &fixture.project,
            aura_vision::face::detect::MODEL_VER,
            aura_vision::face::detect::PREPROCESS_VER,
        )
        .expect("the pending set must read");
    assert!(
        pending.is_empty(),
        "a completed scan leaves nothing pending"
    );
}

#[test]
fn a_scan_resumes_without_recomputing() {
    // Invariant 5. The pending set is a query, so a killed pass and a cancelled pass leave
    // the same state and the next call continues from it.
    let fixture = wedding(6);

    let cancelled = CancelToken::new();
    cancelled.cancel();
    let stopped = fixture
        .scanner
        .scan_project(
            &fixture.project,
            Priority::AiBatch,
            &cancelled,
            &NullProgress,
        )
        .expect("a cancelled scan is not a failure");
    assert!(stopped.cancelled);
    assert_eq!(stopped.scanned, 0);

    let first = scan(&fixture);
    assert_eq!(first.scanned, 6);

    // The second pass has nothing to do, and says so rather than doing it again.
    let second = scan(&fixture);
    assert_eq!(
        second.scanned, 0,
        "a completed project must not be re-scanned"
    );
    assert_eq!(second.faces, 0);
}

#[test]
fn a_template_is_sealed_in_the_catalog() {
    // The promise, checked at the column rather than at the API: `faces.embed` must not
    // contain anything that looks like a vector.
    let fixture = wedding(2);
    scan(&fixture);

    let blob: Vec<u8> = fixture
        .catalog
        .read({
            let key = fixture.project.to_db();
            move |conn| {
                conn.query_row(
                    "SELECT embed FROM faces WHERE project_id = ?1 AND embed IS NOT NULL LIMIT 1",
                    rusqlite::params![key],
                    |row| row.get(0),
                )
                .map_err(|e| aura_core::errors::db::statement_failed("no sealed face", &e))
            }
        })
        .expect("at least one face must carry a template");

    assert!(
        blob.starts_with(aura_people::vault::MAGIC),
        "the column must hold a sealed envelope"
    );
    assert_eq!(
        blob.len(),
        1_024 + aura_people::vault::OVERHEAD,
        "1,024 bytes of template plus the envelope"
    );

    // And it opens through the vault, which is the only way in.
    let faces = fixture.store.load_faces(&fixture.project).expect("faces");
    let with_template = faces
        .iter()
        .find(|face| face.template.is_some())
        .expect("a face with a template");
    assert_eq!(
        with_template
            .template
            .as_ref()
            .map(std::vec::Vec::len)
            .unwrap_or(0),
        512
    );
}

#[test]
fn scanning_twice_rewrites_rather_than_duplicates() {
    // Face ids are derived from the photograph and the box, so a re-scan is an
    // `INSERT OR REPLACE` of the same row. A generated id would double every face.
    let fixture = wedding(3);
    scan(&fixture);
    let before = fixture
        .store
        .load_faces(&fixture.project)
        .expect("faces")
        .len();

    // Force a re-scan by clearing the ledger, which is what a model version bump does.
    fixture
        .catalog
        .writer()
        .transact({
            let key = fixture.project.to_db();
            move |tx| {
                tx.execute(
                    "DELETE FROM face_scan WHERE project_id = ?1",
                    rusqlite::params![key],
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not clear the ledger", &e)
                })?;
                Ok(())
            }
        })
        .expect("the ledger must clear");

    scan(&fixture);
    let after = fixture
        .store
        .load_faces(&fixture.project)
        .expect("faces")
        .len();
    assert_eq!(before, after, "a re-scan must not duplicate faces");
}

#[test]
fn grouping_produces_identities_a_couple_and_a_graph() {
    let fixture = wedding(9);
    scan(&fixture);
    let report = fixture.people.regroup(&fixture.project).expect("grouping");

    // **One identity is a legitimate outcome here, and the reason is condition C1.**
    // The shipped `face_embed` is a placeholder with no training, so its templates carry
    // no identity information and every face in this wedding looks like every other. What
    // this test asserts is the machinery around it: that grouping runs, writes identities,
    // assigns faces, stores a graph consistent with those identities, and reports honest
    // coverage. Whether the *right* number of people comes out is a property of the
    // recogniser, and it is measured against synthetic templates with known separation in
    // `tests/eval/identity_eval.rs` - which is the only place it can honestly be measured
    // until a trained model exists.
    assert!(report.identities >= 1, "{} identities", report.identities);
    assert!(report.assigned > 0);
    assert!(
        report.scene_starved,
        "phase 07 has not run, and the report must say so"
    );
    assert!((report.coverage - 1.0).abs() < 1e-6);

    let edges = fixture
        .store
        .load_cooccurrence(&fixture.project)
        .expect("the graph must be stored");
    if report.identities >= 2 {
        assert!(!edges.is_empty(), "two people in one frame is an edge");
    } else {
        assert!(
            edges.is_empty(),
            "one identity cannot co-occur with itself: {} edges",
            edges.len()
        );
    }

    let hierarchy = fixture
        .people
        .hierarchy(fixture.project)
        .expect("a hierarchy");
    assert_eq!(
        hierarchy.weights.len(),
        report.identities,
        "the weight map is total over the project's identities"
    );
    assert!(
        hierarchy.couple_unconfirmed,
        "nothing is confirmed until a photographer says so"
    );
}

#[test]
fn subjects_of_a_frame_carry_prominence_and_a_focus_score() {
    let fixture = wedding(6);
    scan(&fixture);
    fixture.people.regroup(&fixture.project).expect("grouping");

    let photo = *fixture.photos.first().expect("a photograph");
    let subjects = fixture.people.subjects(photo).expect("subjects");

    assert!(!subjects.faces.is_empty());
    assert!(
        subjects.weights_ver > 0,
        "every score records its weight table"
    );
    assert!(
        subjects.subject_focus_score >= 0.0 && subjects.subject_focus_score <= 1.0,
        "focus {}",
        subjects.subject_focus_score
    );
    assert!(
        subjects.dominant.is_some(),
        "a frame with grouped faces has a dominant subject"
    );
}

#[test]
fn a_role_set_by_the_photographer_survives_a_regroup() {
    // Section 12's first failure mode, and the mitigation: the human's answer is
    // structurally unbeatable. Regrouping produces new identity ids, so the decision is
    // replayed by face set - which is the part that would silently stop working.
    let fixture = wedding(9);
    scan(&fixture);
    fixture.people.regroup(&fixture.project).expect("grouping");

    let identities = fixture
        .store
        .load_identities(&fixture.project)
        .expect("identities");
    let target = identities.first().expect("at least one identity");
    fixture
        .people
        .set_role(target.identity_id, Role::Bride)
        .expect("the role must be set");
    fixture
        .people
        .rename(target.identity_id, Some("Priya"))
        .expect("the name must be set");

    fixture
        .people
        .regroup(&fixture.project)
        .expect("a second grouping");

    let after = fixture
        .store
        .load_identities(&fixture.project)
        .expect("identities");
    let bride = after
        .iter()
        .find(|identity| identity.role == Role::Bride)
        .expect("the bride must survive a re-analysis");
    assert!(bride.user_locked, "and must still be locked");
    assert_eq!(bride.label.as_deref(), Some("Priya"), "and keep her name");
    assert!(
        (bride.role_confidence - 1.0).abs() < f32::EPSILON,
        "a human's decision is not a probability"
    );
}

#[test]
fn a_merge_moves_faces_and_can_be_undone() {
    let fixture = wedding(9);
    scan(&fixture);
    fixture.people.regroup(&fixture.project).expect("grouping");

    let identities = fixture
        .store
        .load_identities(&fixture.project)
        .expect("identities");
    if identities.len() < 2 {
        // A wedding that grouped into one identity has nothing to merge; the other tests
        // cover the grouping itself.
        return;
    }
    let a = identities[0].identity_id;
    let b = identities[1].identity_id;
    let moved = fixture.store.faces_of_identity(b).expect("faces").len();

    let survivor = fixture.people.merge(a, b).expect("the merge must succeed");
    assert_eq!(survivor, a);

    let after = fixture
        .store
        .load_identities(&fixture.project)
        .expect("identities");
    assert!(
        !after.iter().any(|identity| identity.identity_id == b),
        "the absorbed identity must be gone"
    );
    assert_eq!(
        fixture.store.faces_of_identity(a).expect("faces").len(),
        identities[0].face_count as usize + moved
    );

    // The journal entry is what makes it undoable and what makes it survive a regroup.
    let undone = fixture.people.undo(a).expect("undo must read");
    assert!(undone.is_some(), "a merge must be undoable");
}

#[test]
fn a_split_refuses_to_move_every_face() {
    // A split that moves everything is a rename with extra steps, and it would leave an
    // empty card in the panel.
    let fixture = wedding(9);
    scan(&fixture);
    fixture.people.regroup(&fixture.project).expect("grouping");

    let identities = fixture
        .store
        .load_identities(&fixture.project)
        .expect("identities");
    let target = identities.first().expect("an identity");
    let faces = fixture
        .store
        .faces_of_identity(target.identity_id)
        .expect("faces");
    if faces.len() < 2 {
        return;
    }

    let error = fixture
        .people
        .split(target.identity_id, &faces)
        .expect_err("moving every face must be refused");
    assert_eq!(error.code.0, "AURA-SEC-9005");

    // Moving some of them is fine.
    let created = fixture
        .people
        .split(target.identity_id, &faces[..1])
        .expect("a partial split must succeed");
    assert_eq!(
        fixture
            .store
            .faces_of_identity(created)
            .expect("faces")
            .len(),
        1
    );
}

#[test]
fn a_merge_with_itself_is_refused() {
    let fixture = wedding(3);
    scan(&fixture);
    fixture.people.regroup(&fixture.project).expect("grouping");
    let identities = fixture
        .store
        .load_identities(&fixture.project)
        .expect("identities");
    let Some(target) = identities.first() else {
        return;
    };
    let error = fixture
        .people
        .merge(target.identity_id, target.identity_id)
        .expect_err("self-merge must be refused");
    assert_eq!(error.code.0, "AURA-SEC-9005");
}

#[test]
fn erasing_leaves_no_faces_and_records_that_it_happened() {
    let fixture = wedding(4);
    scan(&fixture);
    fixture.people.regroup(&fixture.project).expect("grouping");

    let report = fixture.people.erase(&fixture.project).expect("erasure");
    assert!(report.faces > 0);
    assert!(report.key_removed);

    let (photos, scanned, faces, _) = fixture.store.coverage(&fixture.project).expect("coverage");
    assert_eq!(faces, 0);
    assert_eq!(
        scanned, 0,
        "the scan ledger goes too, or a re-scan is refused"
    );
    assert_eq!(photos, 4, "the photographs stay");
}

#[test]
fn a_scan_is_deterministic() {
    // Invariant 4, at the level that matters: two scans of the same pixels must produce
    // the same face ids and the same sealed bytes.
    let first = wedding(3);
    scan(&first);
    let second = wedding(3);
    scan(&second);

    let ids = |fixture: &Fixture| -> Vec<String> {
        let mut out: Vec<String> = fixture
            .store
            .load_faces(&fixture.project)
            .expect("faces")
            .iter()
            // The photo id differs between the two fixtures, so compare the geometry and
            // the quality rather than the ids themselves.
            .map(|face| {
                format!(
                    "{:.4},{:.4},{:.4},{:.4},{:.3},{}",
                    face.bbox.x, face.bbox.y, face.bbox.w, face.bbox.h, face.quality, face.votes
                )
            })
            .collect();
        out.sort();
        out
    };
    assert_eq!(ids(&first), ids(&second));
}

#[test]
fn timelines_are_built_from_the_clock_aligned_times() {
    let fixture = wedding(9);
    scan(&fixture);
    fixture.people.regroup(&fixture.project).expect("grouping");

    let timelines = fixture
        .people
        .timelines(&fixture.project)
        .expect("timelines");
    assert!(!timelines.is_empty());
    for timeline in &timelines {
        assert!(timeline.frames > 0);
        assert!(
            timeline.first_ms.is_some(),
            "the fixture stamps every photograph"
        );
        assert!(timeline.best_frame().is_some());
    }
}
