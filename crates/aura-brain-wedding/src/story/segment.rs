//! The four tables migration 7 adds, and the rules that live in the statements.
//!
//! Two of those rules are enforced *inside* the SQL rather than around it, and both are
//! copied from phase 06's biometric store because the failure they prevent is the same.
//!
//! **`source = 'user'` is checked in the `WHERE` clause of the statement that would
//! overwrite it.** Not read first and then branched on: a read-then-write leaves a
//! window in which a photographer's label and a background re-classification race, and
//! the photographer loses. `AND source <> 'user'` closes it, and the affected-row count
//! tells the caller what happened.
//!
//! **`user_locked` is checked the same way on `segments`.** Section 6.4: "anything the
//! user touches is `user_locked` and re-analysis never overwrites it."
//!
//! ## Why `segment_images` is a join table
//!
//! Because a scene label and a chapter membership have different lifetimes. Re-running
//! segmentation rewrites every membership in the project and touches no scene label;
//! re-running classification does the opposite. A `segment_id` column on `image_scenes`
//! would couple them, and re-segmenting a wedding would mean four thousand `UPDATE`s
//! instead of one `DELETE` and one bulk insert.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::scene::{
    AttrFlags, ChapterId, SceneId, SceneProfile, SceneResult, SceneScore, Segment, Source,
    Timestamp,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, PhotoId, ProjectId, SegmentId};
use rusqlite::params;

/// The four version numbers migration 7 records on every scene row.
///
/// Model, preprocess, taxonomy, trunk - in that order, which is the order
/// `docs/runbooks/AURA-ML-5022.md` lists them and the order they are logged in.
pub type SceneVersions = (u16, u16, u16, u16);

/// Bytes one photograph costs in `image_scenes` and `segment_images` together.
///
/// Section 11 budgets 400 and the measured figure is 362, on the Christian daylight
/// fixture. **Measured, not estimated**: the first draft of this constant claimed 326,
/// the catalog said 410, and `crates/aura-perf/tests/scene_budgets.rs` is what found the
/// difference. The 48 bytes between the two came from the top-3 encoding, which is why
/// [`top3_json`] writes pairs rather than objects.
///
/// Roughly where it goes:
///
/// | Part | Bytes |
/// |---|---|
/// | two prefixed UUIDs on the scene row (`photo_id`, `project_id`) | 80 |
/// | the scene slug, ritual slug, source, tradition and timestamp | 66 |
/// | the padded top-3, as pairs rather than objects | 48 |
/// | nine numeric columns and `SQLite`'s row overhead | 40 |
/// | three prefixed UUIDs and a position on the membership row | 128 |
///
/// The dominant cost is **text ids**, five of them per photograph across the two
/// tables, at 40 characters each. That is phase 01's decision and not this phase's to
/// undo; what this phase controls is the top-3 encoding, which is why it is compact.
pub const BYTES_PER_IMAGE: usize = 80 + 66 + 48 + 40 + 128;

/// One classified frame, with the three version columns the contract does not carry.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenePassRow {
    /// The frozen result.
    pub result: SceneResult,
    /// The ritual's slug, when one was named. The catalog stores text, not an id.
    pub ritual_slug: Option<String>,
    /// The tradition that declared the rite.
    pub tradition: Option<String>,
    /// Which feature assembly produced the context numbers.
    pub preprocess_ver: u16,
    /// Which taxonomy named the rite.
    pub taxonomy_ver: u16,
    /// Which phase 05 trunk produced the vector the head read.
    pub embed_ver: u16,
    /// Wall clock for this frame.
    pub elapsed_ms: f32,
}

/// One frame as the segmenter needs to read it back.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRow {
    /// The photograph.
    pub photo_id: PhotoId,
    /// Timeline time, milliseconds since the Unix epoch.
    pub ts: Timestamp,
    /// The stored label.
    pub scene: SceneId,
    /// Its confidence.
    pub scene_conf: f32,
    /// The stored top-3.
    pub top3: [SceneScore; 3],
    /// The stored attribute bits.
    pub attrs: AttrFlags,
    /// Where the label came from.
    pub source: Source,
    /// The rite's slug, when one was named.
    pub ritual: Option<String>,
}

/// Reads and writes the four tables migration 7 adds.
#[derive(Debug)]
pub struct StoryStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl StoryStore {
    /// Use one catalog's scene and segment tables.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// Write one project's classified frames.
    ///
    /// `INSERT ... ON CONFLICT DO UPDATE` with `WHERE image_scenes.source <> 'user'`, so
    /// a re-classification is idempotent and a photographer's own label is untouchable.
    /// Returns how many rows actually changed, which is how the caller learns that a
    /// wedding is mostly hand-labelled.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn put_scenes(&self, project: &ProjectId, rows: &[ScenePassRow]) -> AuraResult<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let payload: Vec<(String, String, ScenePassRow)> = rows
            .iter()
            .map(|row| {
                (
                    row.result.image_id.to_db(),
                    top3_json(&row.result.top3),
                    row.clone(),
                )
            })
            .collect();

        self.catalog.writer().transact(move |tx| {
            let mut changed = 0usize;
            {
                let mut statement = tx
                    .prepare(
                        "INSERT INTO image_scenes (photo_id, project_id, scene, scene_conf,
                                                   top3_json, attrs, attrs_measured, ritual,
                                                   ritual_conf, tradition, source, model_ver,
                                                   preprocess_ver, taxonomy_ver, embed_ver,
                                                   elapsed_ms, classified_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
                         ON CONFLICT(photo_id) DO UPDATE SET
                            scene = excluded.scene,
                            scene_conf = excluded.scene_conf,
                            top3_json = excluded.top3_json,
                            attrs = excluded.attrs,
                            attrs_measured = excluded.attrs_measured,
                            ritual = excluded.ritual,
                            ritual_conf = excluded.ritual_conf,
                            tradition = excluded.tradition,
                            source = excluded.source,
                            model_ver = excluded.model_ver,
                            preprocess_ver = excluded.preprocess_ver,
                            taxonomy_ver = excluded.taxonomy_ver,
                            embed_ver = excluded.embed_ver,
                            elapsed_ms = excluded.elapsed_ms,
                            classified_at = excluded.classified_at
                         WHERE image_scenes.source <> 'user'",
                    )
                    .map_err(|e| statement_failed("could not prepare the scene insert", &e))?;

                for (photo, top3, row) in &payload {
                    changed += statement
                        .execute(params![
                            photo,
                            key,
                            row.result.scene.as_str(),
                            row.result.scene_conf,
                            top3,
                            i64::from(row.result.attributes.bits()),
                            i64::from(row.result.attributes_measured),
                            row.ritual_slug,
                            row.result.ritual_conf,
                            row.tradition,
                            row.result.source.as_str(),
                            i64::from(row.result.model_ver),
                            i64::from(row.preprocess_ver),
                            i64::from(row.taxonomy_ver),
                            i64::from(row.embed_ver),
                            f64::from(row.elapsed_ms),
                            now,
                        ])
                        .map_err(|e| statement_failed("could not write a scene row", &e))?;
                }
            }
            Ok(changed)
        })
    }

    /// Set one frame's scene by hand, and lock it against automation.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn set_scene_by_user(
        &self,
        project: &ProjectId,
        photo: PhotoId,
        scene: SceneId,
    ) -> AuraResult<usize> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let id = photo.to_db();
        let label = scene.as_str().to_string();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE image_scenes
                    SET scene = ?1, scene_conf = 1.0, source = 'user', classified_at = ?2
                  WHERE project_id = ?3 AND photo_id = ?4",
                params![label, now, key, id],
            )
            .map_err(|e| statement_failed("could not set a scene by hand", &e))
        })
    }

    /// Every classified frame in a project, in timeline order.
    ///
    /// The order is `timeline_time, photo_id` - the same order `aura_index` walks
    /// embeddings in - so a segmentation and a similarity query can never disagree about
    /// which frame came first. Two shooters whose clocks are aligned interleave here
    /// exactly as they did on the day, which is what section 10.1's two-shooter
    /// regression fixture is checking.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_scenes(&self, project: &ProjectId, model_ver: u16) -> AuraResult<Vec<SceneRow>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT s.photo_id, p.timeline_time, s.scene, s.scene_conf, s.top3_json,
                            s.attrs, s.source, s.ritual
                       FROM image_scenes s
                       JOIN photo p ON p.photo_id = s.photo_id
                      WHERE s.project_id = ?1 AND s.model_ver = ?2
                      ORDER BY p.timeline_time, s.photo_id",
                )
                .map_err(|e| statement_failed("could not read scenes", &e))?;
            let mut rows = statement
                .query(params![key, i64::from(model_ver)])
                .map_err(|e| statement_failed("could not read scenes", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a scene row", &e))?
            {
                let Ok(id) = row.get::<_, String>(0) else {
                    continue;
                };
                let Ok(photo_id) = PhotoId::from_db(&id) else {
                    continue;
                };
                let stamp: Option<String> = row.get(1).ok();
                let ts = stamp
                    .as_deref()
                    .and_then(aura_index::store::parse_rfc3339_ms)
                    .unwrap_or(0);
                let scene: String = row.get(2).unwrap_or_default();
                let attrs: i64 = row.get(5).unwrap_or(0);
                let source: String = row.get(6).unwrap_or_default();

                out.push(SceneRow {
                    photo_id,
                    ts,
                    scene: SceneId::from_str_or_unknown(&scene),
                    scene_conf: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                    top3: parse_top3(&row.get::<_, String>(4).unwrap_or_default()),
                    attrs: AttrFlags::from_bits(u16::try_from(attrs).unwrap_or(0)),
                    source: Source::from_str_or_local(&source),
                    ritual: row.get::<_, Option<String>>(7).unwrap_or(None),
                });
            }
            Ok(out)
        })
    }

    /// One frame's stored scene, for the Explain panel.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_scene(&self, photo: PhotoId) -> AuraResult<Option<SceneResult>> {
        let id = photo.to_db();
        self.catalog.read(move |conn| {
            let found: Result<SceneResult, rusqlite::Error> = conn.query_row(
                "SELECT scene, scene_conf, top3_json, attrs, attrs_measured, ritual_conf,
                        source, model_ver
                   FROM image_scenes WHERE photo_id = ?1",
                params![id],
                |row| {
                    let scene: String = row.get(0)?;
                    let top3: String = row.get(2)?;
                    let attrs: i64 = row.get(3)?;
                    let measured: i64 = row.get(4)?;
                    let source: String = row.get(6)?;
                    let model_ver: i64 = row.get(7)?;
                    Ok(SceneResult {
                        image_id: photo,
                        scene: SceneId::from_str_or_unknown(&scene),
                        scene_conf: row.get::<_, f64>(1)? as f32,
                        top3: parse_top3(&top3),
                        attributes: AttrFlags::from_bits(u16::try_from(attrs).unwrap_or(0)),
                        attributes_measured: measured != 0,
                        // The id is not stored - the slug is - so a caller that needs
                        // the id resolves it through the taxonomy. See ADR-0015
                        // section 3: a catalog whose meaning depends on a config file
                        // that has since been edited is a catalog that lies.
                        ritual: None,
                        ritual_conf: row.get::<_, f64>(5)? as f32,
                        source: Source::from_str_or_local(&source),
                        model_ver: u16::try_from(model_ver).unwrap_or(0),
                    })
                },
            );
            match found {
                Ok(result) => Ok(Some(result)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed("could not read a scene", &err)),
            }
        })
    }

    /// Photographs that still need classifying, in timeline order.
    ///
    /// Resumability, as one query. Invariant 5: a killed pass restarts and repeats no
    /// finished work, and "finished" means "has a row at the current version" - which
    /// is also why a version bump makes every frame pending again without a migration.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(&self, project: &ProjectId, model_ver: u16) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       JOIN embeddings e ON e.photo_id = p.photo_id
                       LEFT JOIN image_scenes s
                              ON s.photo_id = p.photo_id AND s.model_ver = ?2
                      WHERE p.project_id = ?1 AND s.photo_id IS NULL
                      ORDER BY p.timeline_time, p.photo_id",
                )
                .map_err(|e| statement_failed("could not read pending scenes", &e))?;
            let mut rows = statement
                .query(params![key, i64::from(model_ver)])
                .map_err(|e| statement_failed("could not read pending scenes", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a pending row", &e))?
            {
                if let Ok(id) = row.get::<_, String>(0) {
                    if let Ok(photo) = PhotoId::from_db(&id) {
                        out.push(photo);
                    }
                }
            }
            Ok(out)
        })
    }

    /// How many photographs a project has, and how many carry a scene.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the view cannot be read.
    pub fn coverage(&self, project: &ProjectId) -> AuraResult<(i64, i64)> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let found: Result<(i64, i64), rusqlite::Error> = conn.query_row(
                "SELECT photos, classified FROM v_scene_coverage WHERE project_id = ?1",
                params![key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            // A project with no photographs has no row in the view. Zero is truthful.
            Ok(found.unwrap_or((0, 0)))
        })
    }

    /// The distinct version tuples in a project's scene rows.
    ///
    /// What `AURA-ML-5022` is raised from. Four numbers per tuple, because four
    /// different things can go stale; see the runbook.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn scene_versions(&self, project: &ProjectId) -> AuraResult<Vec<(SceneVersions, usize)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT model_ver, preprocess_ver, taxonomy_ver, embed_ver, COUNT(*)
                       FROM image_scenes WHERE project_id = ?1
                      GROUP BY model_ver, preprocess_ver, taxonomy_ver, embed_ver
                      ORDER BY model_ver, preprocess_ver, taxonomy_ver, embed_ver",
                )
                .map_err(|e| statement_failed("could not read scene versions", &e))?;
            let mut rows = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read scene versions", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a version row", &e))?
            {
                let read = |index: usize| -> u16 {
                    u16::try_from(row.get::<_, i64>(index).unwrap_or(0)).unwrap_or(0)
                };
                let count = usize::try_from(row.get::<_, i64>(4).unwrap_or(0)).unwrap_or(0);
                out.push(((read(0), read(1), read(2), read(3)), count));
            }
            Ok(out)
        })
    }

    /// Replace a project's chapters, keeping every locked one.
    ///
    /// The whole re-segmentation, as one transaction. Locked chapters are read first,
    /// deleted last, and re-inserted unchanged - which is the only way to make "a
    /// photographer's boundary survives a re-analysis" true without asking the
    /// segmenter to reason about locks it cannot see.
    ///
    /// Returns how many chapters were written and how many locked ones were preserved.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn put_segments(
        &self,
        project: &ProjectId,
        segments: &[Segment],
        members: &BTreeMap<SegmentId, Vec<PhotoId>>,
        penalty: f32,
        scene_ver: u16,
    ) -> AuraResult<(usize, usize)> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let payload: Vec<(Segment, Vec<String>)> = segments
            .iter()
            .map(|segment| {
                (
                    segment.clone(),
                    members
                        .get(&segment.id)
                        .map(|photos| photos.iter().map(PhotoId::to_db).collect())
                        .unwrap_or_default(),
                )
            })
            .collect();

        self.catalog.writer().transact(move |tx| {
            let locked: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM segments WHERE project_id = ?1 AND user_locked = 1",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            // `segment_images` cascades from `segments`, so one delete clears both.
            tx.execute(
                "DELETE FROM segments WHERE project_id = ?1 AND user_locked = 0",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear old chapters", &e))?;

            let mut written = 0usize;
            {
                let mut insert = tx
                    .prepare(
                        "INSERT INTO segments (id, project_id, ordinal, chapter, label, start_ts,
                                               end_ts, dominant_scene, confidence, key_frame,
                                               image_count, reasons, user_locked, cloud_named,
                                               tradition, penalty, scene_ver, created_at,
                                               updated_at)
                         VALUES (?1,?2,?3,?4,NULL,?5,?6,?7,?8,?9,?10,?11,?12,0,NULL,?13,?14,?15,?15)
                         ON CONFLICT(id) DO NOTHING",
                    )
                    .map_err(|e| statement_failed("could not prepare the chapter insert", &e))?;
                let mut member = tx
                    .prepare(
                        "INSERT INTO segment_images (segment_id, photo_id, project_id, position)
                         VALUES (?1,?2,?3,?4) ON CONFLICT DO NOTHING",
                    )
                    .map_err(|e| statement_failed("could not prepare the membership insert", &e))?;

                for (ordinal, (segment, photos)) in payload.iter().enumerate() {
                    let id = segment.id.to_db();
                    let reasons = serde_json::to_string(&segment.reasons)
                        .unwrap_or_else(|_| "[]".to_string());
                    written += insert
                        .execute(params![
                            id,
                            key,
                            i64::try_from(ordinal).unwrap_or(0),
                            segment.chapter.as_str(),
                            segment.start_ts,
                            segment.end_ts,
                            segment.dominant_scene.as_str(),
                            segment.confidence,
                            segment.key_frame.to_db(),
                            i64::from(segment.image_count),
                            reasons,
                            i64::from(segment.user_locked),
                            f64::from(penalty),
                            i64::from(scene_ver),
                            now,
                        ])
                        .map_err(|e| statement_failed("could not write a chapter", &e))?;

                    for (position, photo) in photos.iter().enumerate() {
                        member
                            .execute(params![
                                id,
                                photo,
                                key,
                                i64::try_from(position).unwrap_or(0)
                            ])
                            .map_err(|e| statement_failed("could not write a membership", &e))?;
                    }
                }
            }
            Ok((written, usize::try_from(locked).unwrap_or(0)))
        })
    }

    /// A project's chapters, in order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the view cannot be read.
    pub fn load_segments(&self, project: &ProjectId) -> AuraResult<Vec<Segment>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT id, chapter, start_ts, end_ts, dominant_scene, confidence,
                            key_frame, image_count, reasons, user_locked
                       FROM segments WHERE project_id = ?1
                      ORDER BY ordinal, start_ts",
                )
                .map_err(|e| statement_failed("could not read chapters", &e))?;
            let mut rows = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read chapters", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a chapter row", &e))?
            {
                if let Some(segment) = read_segment(row) {
                    out.push(segment);
                }
            }
            Ok(out)
        })
    }

    /// The chapter one photograph belongs to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn segment_of(&self, photo: PhotoId) -> AuraResult<Option<Segment>> {
        let id = photo.to_db();
        self.catalog.read(move |conn| {
            let found = conn.query_row(
                "SELECT s.id, s.chapter, s.start_ts, s.end_ts, s.dominant_scene, s.confidence,
                        s.key_frame, s.image_count, s.reasons, s.user_locked
                   FROM segments s
                   JOIN segment_images si ON si.segment_id = s.id
                  WHERE si.photo_id = ?1",
                params![id],
                |row| Ok(read_segment(row)),
            );
            match found {
                Ok(segment) => Ok(segment),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed("could not read a chapter", &err)),
            }
        })
    }

    /// Which project a chapter belongs to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn project_of(&self, segment: SegmentId) -> AuraResult<Option<ProjectId>> {
        let id = segment.to_db();
        self.catalog.read(move |conn| {
            let found: Result<String, rusqlite::Error> = conn.query_row(
                "SELECT project_id FROM segments WHERE id = ?1",
                params![id],
                |row| row.get(0),
            );
            match found {
                Ok(text) => Ok(ProjectId::from_db(&text).ok()),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed("could not read a chapter's project", &err)),
            }
        })
    }

    /// Rename a chapter and lock it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn set_chapter(
        &self,
        segment: SegmentId,
        chapter: ChapterId,
        label: Option<&str>,
    ) -> AuraResult<usize> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let id = segment.to_db();
        let name = chapter.as_str().to_string();
        let title = label.map(ToString::to_string);
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE segments
                    SET chapter = ?1, label = ?2, user_locked = 1, confidence = 1.0,
                        reasons = json_array('renamed by the photographer'), updated_at = ?3
                  WHERE id = ?4",
                params![name, title, now, id],
            )
            .map_err(|e| statement_failed("could not rename a chapter", &e))
        })
    }

    /// Lock a chapter without changing it.
    ///
    /// Used by every boundary edit: a moved boundary belongs to two chapters and both
    /// must survive the next re-analysis, or the one that was not locked moves it back.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn lock(&self, segment: SegmentId) -> AuraResult<usize> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let id = segment.to_db();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE segments SET user_locked = 1, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| statement_failed("could not lock a chapter", &e))
        })
    }

    /// Move frames between two chapters and restate their bounds.
    ///
    /// One transaction, because a membership that has moved and a bound that has not is
    /// a chapter whose `image_count` lies - and `image_count` is what the strip draws.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn reassign(
        &self,
        project: &ProjectId,
        moves: &[(PhotoId, SegmentId)],
        bounds: &[(SegmentId, Timestamp, Timestamp, u32)],
    ) -> AuraResult<usize> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let rehomed: Vec<(String, String)> = moves
            .iter()
            .map(|(photo, segment)| (photo.to_db(), segment.to_db()))
            .collect();
        let bounded: Vec<(String, Timestamp, Timestamp, u32)> = bounds
            .iter()
            .map(|(segment, start, end, count)| (segment.to_db(), *start, *end, *count))
            .collect();

        self.catalog.writer().transact(move |tx| {
            let mut changed = 0usize;
            for (photo, segment) in &rehomed {
                tx.execute(
                    "DELETE FROM segment_images WHERE photo_id = ?1 AND project_id = ?2",
                    params![photo, key],
                )
                .map_err(|e| statement_failed("could not move a photograph", &e))?;
                changed += tx
                    .execute(
                        "INSERT INTO segment_images (segment_id, photo_id, project_id, position)
                         VALUES (?1,?2,?3,0) ON CONFLICT DO NOTHING",
                        params![segment, photo, key],
                    )
                    .map_err(|e| statement_failed("could not move a photograph", &e))?;
            }
            for (segment, start, end, count) in &bounded {
                tx.execute(
                    "UPDATE segments
                        SET start_ts = ?1, end_ts = ?2, image_count = ?3, user_locked = 1,
                            updated_at = ?4
                      WHERE id = ?5",
                    params![start, end, i64::from(*count), now, segment],
                )
                .map_err(|e| statement_failed("could not restate a chapter's bounds", &e))?;
            }
            Ok(changed)
        })
    }

    /// Delete one chapter, moving its frames to the survivor.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn absorb(&self, victim: SegmentId, survivor: SegmentId) -> AuraResult<usize> {
        let from = victim.to_db();
        let into = survivor.to_db();
        self.catalog.writer().transact(move |tx| {
            let moved = tx
                .execute(
                    "UPDATE segment_images SET segment_id = ?1 WHERE segment_id = ?2",
                    params![into, from],
                )
                .map_err(|e| statement_failed("could not move a chapter's photographs", &e))?;
            tx.execute("DELETE FROM segments WHERE id = ?1", params![from])
                .map_err(|e| statement_failed("could not delete a chapter", &e))?;
            Ok(moved)
        })
    }

    /// Insert one chapter, for a split.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn insert_segment(
        &self,
        project: &ProjectId,
        segment: &Segment,
        ordinal: usize,
        scene_ver: u16,
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let id = segment.id.to_db();
        let chapter = segment.chapter.as_str().to_string();
        let scene = segment.dominant_scene.as_str().to_string();
        let frame = segment.key_frame.to_db();
        let reasons = serde_json::to_string(&segment.reasons).unwrap_or_else(|_| "[]".to_string());
        let owned = segment.clone();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO segments (id, project_id, ordinal, chapter, label, start_ts, end_ts,
                                       dominant_scene, confidence, key_frame, image_count,
                                       reasons, user_locked, cloud_named, tradition, penalty,
                                       scene_ver, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,NULL,?5,?6,?7,?8,?9,?10,?11,1,0,NULL,0.0,?12,?13,?13)",
                params![
                    id,
                    key,
                    i64::try_from(ordinal).unwrap_or(0),
                    chapter,
                    owned.start_ts,
                    owned.end_ts,
                    scene,
                    owned.confidence,
                    frame,
                    i64::from(owned.image_count),
                    reasons,
                    i64::from(scene_ver),
                    now,
                ],
            )
            .map(|_| ())
            .map_err(|e| statement_failed("could not insert a chapter", &e))
        })
    }

    /// Record a cloud-named chapter.
    ///
    /// Separate from [`StoryStore::set_chapter`] because a cloud answer is **not** a
    /// user decision: it sets `cloud_named` and leaves `user_locked` alone, so the next
    /// re-analysis may still improve it. Phase 04's rule, applied: cloud proposes,
    /// deterministic code decides, and the photographer outranks both.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn set_cloud_label(
        &self,
        segment: SegmentId,
        chapter: ChapterId,
        confidence: f32,
        reasons: &[String],
        tradition: Option<&str>,
    ) -> AuraResult<usize> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let id = segment.to_db();
        let name = chapter.as_str().to_string();
        let text = serde_json::to_string(reasons).unwrap_or_else(|_| "[]".to_string());
        let culture = tradition.map(ToString::to_string);
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE segments
                    SET chapter = ?1, confidence = ?2, reasons = ?3, tradition = ?4,
                        cloud_named = 1, updated_at = ?5
                  WHERE id = ?6 AND user_locked = 0",
                params![name, confidence, text, culture, now, id],
            )
            .map_err(|e| statement_failed("could not record a cloud chapter label", &e))
        })
    }

    /// Write a project's scene profiles.
    ///
    /// `WHERE user_edited = 0` on the update path, so a photographer's own tolerances
    /// survive every reload - the same rule as `source <> 'user'` on a scene label and
    /// `user_locked` on a chapter, for the third time in this file.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn put_profiles(
        &self,
        project: &ProjectId,
        profiles: &[(SceneProfile, String)],
        profile_ver: u16,
    ) -> AuraResult<usize> {
        if profiles.is_empty() {
            return Ok(0);
        }
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let payload: Vec<(SceneProfile, String)> = profiles.to_vec();

        self.catalog.writer().transact(move |tx| {
            let mut written = 0usize;
            {
                let mut statement = tx
                    .prepare(
                        "INSERT INTO scene_profiles (project_id, scene, keeper_min, keeper_max,
                                                     max_acceptable_noise, max_acceptable_blur,
                                                     subject_focus_weight, emotion_weight,
                                                     composition_weight, editing_intent,
                                                     must_cover, rationale, user_edited,
                                                     profile_ver, updated_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0,?13,?14)
                         ON CONFLICT(project_id, scene) DO UPDATE SET
                            keeper_min = excluded.keeper_min,
                            keeper_max = excluded.keeper_max,
                            max_acceptable_noise = excluded.max_acceptable_noise,
                            max_acceptable_blur = excluded.max_acceptable_blur,
                            subject_focus_weight = excluded.subject_focus_weight,
                            emotion_weight = excluded.emotion_weight,
                            composition_weight = excluded.composition_weight,
                            editing_intent = excluded.editing_intent,
                            must_cover = excluded.must_cover,
                            rationale = excluded.rationale,
                            profile_ver = excluded.profile_ver,
                            updated_at = excluded.updated_at
                         WHERE scene_profiles.user_edited = 0",
                    )
                    .map_err(|e| statement_failed("could not prepare the profile insert", &e))?;

                for (profile, rationale) in &payload {
                    written += statement
                        .execute(params![
                            key,
                            profile.scene.as_str(),
                            profile.keeper_rate.0,
                            profile.keeper_rate.1,
                            profile.max_acceptable_noise,
                            profile.max_acceptable_blur,
                            profile.subject_focus_weight,
                            profile.emotion_weight,
                            profile.composition_weight,
                            profile.editing_intent.as_str(),
                            i64::from(profile.must_cover),
                            rationale,
                            i64::from(profile_ver),
                            now,
                        ])
                        .map_err(|e| statement_failed("could not write a scene profile", &e))?;
                }
            }
            Ok(written)
        })
    }

    /// A project's stored profiles, for the override layer.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_profiles(&self, project: &ProjectId) -> AuraResult<Vec<(SceneProfile, String)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT scene, keeper_min, keeper_max, max_acceptable_noise,
                            max_acceptable_blur, subject_focus_weight, emotion_weight,
                            composition_weight, editing_intent, must_cover, rationale
                       FROM scene_profiles WHERE project_id = ?1 ORDER BY scene",
                )
                .map_err(|e| statement_failed("could not read scene profiles", &e))?;
            let mut rows = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read scene profiles", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a profile row", &e))?
            {
                let scene: String = row.get(0).unwrap_or_default();
                let intent: String = row.get(8).unwrap_or_default();
                let read = |index: usize| -> f32 { row.get::<_, f64>(index).unwrap_or(0.0) as f32 };
                out.push((
                    SceneProfile {
                        scene: SceneId::from_str_or_unknown(&scene),
                        keeper_rate: (read(1), read(2)),
                        max_acceptable_noise: read(3),
                        max_acceptable_blur: read(4),
                        subject_focus_weight: read(5),
                        emotion_weight: read(6),
                        composition_weight: read(7),
                        editing_intent: aura_core::contract::scene::EditIntent::from_str_or_neutral(
                            &intent,
                        ),
                        must_cover: row.get::<_, i64>(9).unwrap_or(0) != 0,
                    },
                    row.get::<_, String>(10).unwrap_or_default(),
                ));
            }
            Ok(out)
        })
    }

    /// Every classified frame's coarse scene label, for `aura-people`.
    ///
    /// The one query that closes half of phase 06's condition C3. `SceneId::role_label`
    /// is the mapping, and frames whose scene says nothing about who somebody is -
    /// details, venue, candids - are deliberately absent rather than present with an
    /// empty label.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn role_labels(&self, project: &ProjectId) -> AuraResult<BTreeMap<PhotoId, String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id, scene FROM image_scenes WHERE project_id = ?1")
                .map_err(|e| statement_failed("could not read scene labels", &e))?;
            let mut rows = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read scene labels", &e))?;

            let mut out = BTreeMap::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a scene label", &e))?
            {
                let (Ok(id), Ok(scene)) = (row.get::<_, String>(0), row.get::<_, String>(1)) else {
                    continue;
                };
                let Ok(photo) = PhotoId::from_db(&id) else {
                    continue;
                };
                if let Some(label) = SceneId::from_str_or_unknown(&scene).role_label() {
                    out.insert(photo, label.to_string());
                }
            }
            Ok(out)
        })
    }
}

/// One chapter row.
fn read_segment(row: &rusqlite::Row<'_>) -> Option<Segment> {
    let id = SegmentId::from_db(&row.get::<_, String>(0).ok()?).ok()?;
    let chapter: String = row.get(1).ok()?;
    let scene: String = row.get(4).ok()?;
    let frame: Option<String> = row.get(6).ok()?;
    let reasons: String = row.get(8).unwrap_or_else(|_| "[]".to_string());

    Some(Segment {
        id,
        chapter: ChapterId::from_str_or_other(&chapter),
        start_ts: row.get(2).unwrap_or(0),
        end_ts: row.get(3).unwrap_or(0),
        dominant_scene: SceneId::from_str_or_unknown(&scene),
        confidence: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
        // A chapter whose key frame was deleted still exists; the panel shows a
        // placeholder. Refusing to read the row would hide a whole chapter over one
        // missing thumbnail.
        key_frame: frame
            .and_then(|text| PhotoId::from_db(&text).ok())
            .unwrap_or_default(),
        image_count: u32::try_from(row.get::<_, i64>(7).unwrap_or(0)).unwrap_or(0),
        reasons: serde_json::from_str(&reasons).unwrap_or_default(),
        user_locked: row.get::<_, i64>(9).unwrap_or(0) != 0,
    })
}

/// The stored form of a top-3 list.
///
/// Always exactly three entries, padded with `unknown` at zero. A constant arity means
/// the Explain panel never reasons about a short list, and the migration's
/// `length(top3_json) > 2` check is a check about content rather than about shape.
///
/// **A list of pairs, not a list of objects**, and that is a storage decision with a
/// measured cause. `[{"scene":"getting_ready_bride","score":0.55},...]` repeats twelve
/// bytes of key names three times per photograph, which is 77 bytes per row and 77 KB
/// per thousand images - a fifth of section 11's whole 400-byte budget spent on the
/// words "scene" and "score". `[["getting_ready_bride",0.55],...]` costs nothing in
/// readability, because the slug is still text a support engineer can read, and
/// `crates/aura-perf/tests/scene_budgets.rs` measures the difference against the
/// catalog rather than against this comment.
#[must_use]
pub fn top3_json(top3: &[SceneScore; 3]) -> String {
    // Built by hand rather than with `serde_json::json!`, which expands to an
    // `unwrap` that R1 bans. The rounding to three decimals is deliberate: a stored
    // top-3 is read by a panel and by a support engineer, and seventeen significant
    // figures of an f32 posterior is noise in both cases.
    let entries: Vec<serde_json::Value> = top3
        .iter()
        .map(|score| {
            let rounded = (f64::from(score.score) * 1000.0).round() / 1000.0;
            serde_json::Value::Array(vec![
                serde_json::Value::String(score.scene.as_str().to_string()),
                serde_json::Number::from_f64(rounded)
                    .map_or(serde_json::Value::Null, serde_json::Value::Number),
            ])
        })
        .collect();
    serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
}

/// Parse the stored top-3, padding anything short.
///
/// Reads both the pair form this build writes and the object form an earlier build
/// wrote. Two shapes rather than one because a catalog written by a previous build must
/// still open - which is the same reason `SceneId::from_str_or_unknown` never fails -
/// and because a top-3 that would not parse is a debug payload lost, not a wedding lost.
#[must_use]
pub fn parse_top3(text: &str) -> [SceneScore; 3] {
    let mut out = [SceneScore::NONE; 3];
    let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(text) else {
        return out;
    };
    for (slot, entry) in out.iter_mut().zip(parsed.iter()) {
        let (scene, score) = match entry {
            serde_json::Value::Array(pair) => (
                pair.first()
                    .and_then(serde_json::Value::as_str)
                    .map_or(SceneId::Unknown, SceneId::from_str_or_unknown),
                pair.get(1)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
            ),
            other => (
                other
                    .get("scene")
                    .and_then(serde_json::Value::as_str)
                    .map_or(SceneId::Unknown, SceneId::from_str_or_unknown),
                other
                    .get("score")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
            ),
        };
        *slot = SceneScore {
            scene,
            score: score as f32,
        };
    }
    out
}
