//! Migration 14's four tables.
//!
//! # The two rules that live in the SQL rather than around it
//!
//! **There is no path column and no file operation.** A recipe describes an edit; where the
//! original lives is phase 01's business and where the export goes is phase 30's. Invariant
//! 1 is a property of the schema here rather than a rule people remember.
//!
//! **A save is one transaction over four tables.** The recipe row, the history row, the
//! trim of anything past `history::MAX_ENTRIES` and the snapshot list move together, because
//! a recipe whose history says something else is a photograph whose undo button lies.
//!
//! # What the catalog is for when a sidecar exists
//!
//! Both. The sidecar survives a lost catalog and travels with the card; the catalog survives
//! a re-copied folder, answers "which photographs has a person touched" without opening four
//! thousand files, and is what the develop filmstrip reads. [`RecipeStore::load`] prefers
//! the catalog and `aura_recipe::sidecar::load` prefers the sidecar, and the two are
//! reconciled at import - which is where a disagreement means somebody edited elsewhere.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::errors::db::statement_failed;
use aura_core::errors::render::sidecar_unreadable;
use aura_core::{AuraResult, PhotoId, ProjectId};
use rusqlite::{params, OptionalExtension};

use crate::contract::recipe::{EditSource, Recipe};
use crate::hash::{canonical, recipe_hash};
use crate::history::{History, HistoryEntry, Snapshot, MAX_ENTRIES};

/// How much of a wedding has an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DevelopCoverage {
    /// Photographs in the project. **The denominator**, and not the delivered gallery.
    pub images: u32,
    /// Photographs carrying a recipe.
    pub with_recipe: u32,
    /// Recipes an automated pass wrote.
    pub from_ai: u32,
    /// Recipes a person wrote.
    pub from_user: u32,
    /// Photographs where a person has touched at least one parameter.
    pub touched_by_hand: u32,
    /// Photographs whose sidecar on disk is behind the catalog.
    pub sidecar_behind: u32,
}

impl DevelopCoverage {
    /// Fraction of the project carrying an edit, `0..1`.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.images == 0 {
            0.0
        } else {
            f32::from(u16::try_from(self.with_recipe.min(65_535)).unwrap_or(0))
                / f32::from(u16::try_from(self.images.min(65_535)).unwrap_or(1))
        }
    }
}

/// Read and write edits.
#[derive(Debug)]
pub struct RecipeStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl RecipeStore {
    /// A store over one catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// Write the current recipe and append one history step.
    ///
    /// `changed` is the list [`crate::schema::merge`] or [`crate::history::changed_paths`]
    /// produced. An empty list writes the recipe row and no history row, which is what a
    /// first save is.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when the recipe cannot be serialised, `AURA-DB-3006` when a
    /// statement fails; the transaction rolls back and the previous edit is still there.
    pub fn save(
        &self,
        project: &ProjectId,
        photo: &PhotoId,
        recipe: &Recipe,
        changed: &[String],
        label: &str,
    ) -> AuraResult<()> {
        let body = canonical(recipe)?;
        let hash = recipe_hash(recipe)?;
        let now = self.clock.now_utc();
        let at = aura_catalog::rfc3339(now);
        let at_ts = (now.unix_timestamp_nanos() / 1_000_000) as i64;

        let project_key = project.to_db();
        let photo_key = photo.to_db();
        let source = recipe.provenance.source.as_str().to_string();
        let confidence = f64::from(recipe.provenance.confidence);
        let decision = recipe.provenance.decision_id.clone();
        let scene = recipe.provenance.scene.clone();
        let style = recipe.provenance.style_profile.clone();
        let user_edited = recipe.provenance.user_edited_fields.len() as i64;
        let schema_ver = i64::from(recipe.schema);
        let engine = recipe.engine.clone();
        let changed_json = serde_json::to_string(changed).unwrap_or_else(|_| "[]".to_string());
        let label = label.to_string();
        let record_step = !changed.is_empty();

        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO edit_recipes (
                     photo_id, project_id, schema_ver, engine, body, recipe_hash,
                     source, confidence, decision_id, scene, style_profile,
                     user_edited_count, sidecar_written, updated_ts, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, ?14)
                 ON CONFLICT(photo_id) DO UPDATE SET
                     schema_ver        = excluded.schema_ver,
                     engine            = excluded.engine,
                     body              = excluded.body,
                     recipe_hash       = excluded.recipe_hash,
                     source            = excluded.source,
                     confidence        = excluded.confidence,
                     decision_id       = excluded.decision_id,
                     scene             = excluded.scene,
                     style_profile     = excluded.style_profile,
                     user_edited_count = excluded.user_edited_count,
                     sidecar_written   = 0,
                     updated_ts        = excluded.updated_ts,
                     updated_at        = excluded.updated_at",
                params![
                    photo_key,
                    project_key,
                    schema_ver,
                    engine,
                    body,
                    hash,
                    source,
                    confidence,
                    decision,
                    scene,
                    style,
                    user_edited,
                    at_ts,
                    at,
                ],
            )
            .map_err(|e| statement_failed("could not save the edit", &e))?;

            if record_step {
                let next: i64 = conn
                    .query_row(
                        "SELECT COALESCE(MAX(seq), 0) + 1 FROM edit_history WHERE photo_id = ?1",
                        params![photo_key],
                        |row| row.get(0),
                    )
                    .map_err(|e| statement_failed("could not read the history", &e))?;

                conn.execute(
                    "INSERT INTO edit_history
                         (photo_id, seq, source, changed, label, body, at_ts, at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        photo_key,
                        next,
                        source,
                        changed_json,
                        label,
                        body,
                        at_ts,
                        at
                    ],
                )
                .map_err(|e| statement_failed("could not record the edit step", &e))?;

                // Trimmed on write rather than by a background pass: the bound is what the
                // storage budget is measured against, and a table that only gets trimmed
                // when somebody remembers is a table that is never trimmed.
                conn.execute(
                    "DELETE FROM edit_history
                      WHERE photo_id = ?1
                        AND seq <= (SELECT MAX(seq) - ?2 FROM edit_history WHERE photo_id = ?1)",
                    params![photo_key, MAX_ENTRIES as i64],
                )
                .map_err(|e| statement_failed("could not trim the history", &e))?;
            }

            Ok(())
        })
    }

    /// Mark a photograph's sidecar as in step with the catalog.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn mark_sidecar_written(&self, photo: &PhotoId) -> AuraResult<()> {
        let key = photo.to_db();
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE edit_recipes SET sidecar_written = 1 WHERE photo_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not mark the sidecar written", &e))?;
            Ok(())
        })
    }

    /// The current recipe for a photograph, or `None`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails, `AURA-RENDER-8005` when the stored body is not a
    /// recipe - which means the row was written by something other than [`Self::save`].
    pub fn load(&self, photo: &PhotoId) -> AuraResult<Option<Recipe>> {
        let key = photo.to_db();
        let body: Option<String> = self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT body FROM edit_recipes WHERE photo_id = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| statement_failed("could not read the edit", &e))
        })?;

        match body {
            None => Ok(None),
            Some(text) => crate::sidecar::parse(&text).map(Some),
        }
    }

    /// Rebuild a photograph's [`History`] from the catalog.
    ///
    /// `original` is the neutral recipe the history starts from; the store does not know
    /// what a camera's own starting point is, and inventing one would be a colour decision
    /// taken in a database module.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a read fails, `AURA-RENDER-8005` when a stored body is not a
    /// recipe.
    pub fn history(&self, photo: &PhotoId, original: Recipe) -> AuraResult<History> {
        let key = photo.to_db();
        let rows: Vec<(i64, String, String, String, String, i64)> =
            self.catalog.read(move |conn| {
                let mut statement = conn
                    .prepare(
                        "SELECT seq, source, changed, label, body, at_ts
                           FROM edit_history WHERE photo_id = ?1 ORDER BY seq",
                    )
                    .map_err(|e| statement_failed("could not prepare the history read", &e))?;
                let mapped = statement
                    .query_map(params![key], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    })
                    .map_err(|e| statement_failed("could not read the history", &e))?;
                let mut out = Vec::new();
                for row in mapped {
                    out.push(row.map_err(|e| statement_failed("could not read a step", &e))?);
                }
                Ok(out)
            })?;

        let mut entries = Vec::with_capacity(rows.len());
        for (seq, source, changed, label, body, at_ts) in rows {
            entries.push(HistoryEntry {
                seq: u64::try_from(seq).unwrap_or(0),
                at_ms: at_ts,
                source: source_from_db(&source),
                changed: serde_json::from_str(&changed).unwrap_or_default(),
                label,
                recipe: crate::sidecar::parse(&body)?,
            });
        }

        let snapshot_key = photo.to_db();
        let snapshot_rows: Vec<(String, String, i64)> = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT name, body, at_ts FROM edit_snapshots
                      WHERE photo_id = ?1 ORDER BY at_ts, name",
                )
                .map_err(|e| statement_failed("could not prepare the snapshot read", &e))?;
            let mapped = statement
                .query_map(params![snapshot_key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|e| statement_failed("could not read the snapshots", &e))?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row.map_err(|e| statement_failed("could not read a snapshot", &e))?);
            }
            Ok(out)
        })?;

        let mut snapshots = Vec::with_capacity(snapshot_rows.len());
        for (name, body, at_ts) in snapshot_rows {
            snapshots.push(Snapshot {
                name,
                at_ms: at_ts,
                recipe: crate::sidecar::parse(&body)?,
            });
        }

        Ok(History::rehydrate(original, entries, snapshots))
    }

    /// Store a named snapshot.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when the recipe cannot be serialised, `AURA-DB-3006` when the
    /// statement fails - which includes a name that is already taken, because the primary
    /// key is what enforces that rather than a read-then-write.
    pub fn put_snapshot(&self, photo: &PhotoId, name: &str, recipe: &Recipe) -> AuraResult<()> {
        let body = canonical(recipe)?;
        let now = self.clock.now_utc();
        let at = aura_catalog::rfc3339(now);
        let at_ts = (now.unix_timestamp_nanos() / 1_000_000) as i64;
        let key = photo.to_db();
        let name = name.to_string();

        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO edit_snapshots (photo_id, name, body, at_ts, at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![key, name, body, at_ts, at],
            )
            .map_err(|e| statement_failed("could not save the snapshot", &e))?;
            Ok(())
        })
    }

    /// Record what an export produced, with the four inputs of its hash.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    #[allow(clippy::too_many_arguments)]
    pub fn record_export(
        &self,
        project: &ProjectId,
        photo: &PhotoId,
        render_hash: &str,
        inputs: ExportInputs,
    ) -> AuraResult<()> {
        let now = self.clock.now_utc();
        let at = aura_catalog::rfc3339(now);
        let at_ts = (now.unix_timestamp_nanos() / 1_000_000) as i64;

        let project_key = project.to_db();
        let photo_key = photo.to_db();
        let render_hash = render_hash.to_string();

        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO export_renders (
                     render_hash, photo_id, project_id,
                     content_hash, recipe_hash, engine, output_spec,
                     level, colour_space, bit_depth, backend,
                     width, height, ms, at_ts, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT(render_hash) DO UPDATE SET
                     at_ts = excluded.at_ts,
                     at    = excluded.at,
                     ms    = excluded.ms",
                params![
                    render_hash,
                    photo_key,
                    project_key,
                    inputs.content_hash,
                    inputs.recipe_hash,
                    inputs.engine,
                    inputs.output_spec,
                    inputs.level,
                    inputs.colour_space,
                    i64::from(inputs.bit_depth),
                    inputs.backend,
                    i64::from(inputs.width),
                    i64::from(inputs.height),
                    i64::from(inputs.ms),
                    at_ts,
                    at,
                ],
            )
            .map_err(|e| statement_failed("could not record the export", &e))?;
            Ok(())
        })
    }

    /// What a stored export was made of, for `AURA-RENDER-8007`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn export_inputs(&self, render_hash: &str) -> AuraResult<Option<ExportInputs>> {
        let key = render_hash.to_string();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT content_hash, recipe_hash, engine, output_spec, level,
                        colour_space, bit_depth, backend, width, height, ms
                   FROM export_renders WHERE render_hash = ?1",
                params![key],
                |row| {
                    Ok(ExportInputs {
                        content_hash: row.get(0)?,
                        recipe_hash: row.get(1)?,
                        engine: row.get(2)?,
                        output_spec: row.get(3)?,
                        level: row.get(4)?,
                        colour_space: row.get(5)?,
                        bit_depth: row.get::<_, i64>(6)? as u8,
                        backend: row.get(7)?,
                        width: row.get::<_, i64>(8)? as u32,
                        height: row.get::<_, i64>(9)? as u32,
                        ms: row.get::<_, i64>(10)? as u32,
                    })
                },
            )
            .optional()
            .map_err(|e| statement_failed("could not read the export", &e))
        })
    }

    /// How much of a project carries an edit.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn coverage(&self, project: &ProjectId) -> AuraResult<DevelopCoverage> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT images, with_recipe, from_ai, from_user, touched_by_hand, sidecar_behind
                   FROM v_develop_coverage WHERE project_id = ?1",
                params![key],
                |row| {
                    Ok(DevelopCoverage {
                        images: row.get::<_, i64>(0)? as u32,
                        with_recipe: row.get::<_, i64>(1)? as u32,
                        from_ai: row.get::<_, Option<i64>>(2)?.unwrap_or(0) as u32,
                        from_user: row.get::<_, Option<i64>>(3)?.unwrap_or(0) as u32,
                        touched_by_hand: row.get::<_, Option<i64>>(4)?.unwrap_or(0) as u32,
                        sidecar_behind: row.get::<_, Option<i64>>(5)?.unwrap_or(0) as u32,
                    })
                },
            )
            .optional()
            .map(Option::unwrap_or_default)
            .map_err(|e| statement_failed("could not read the develop coverage", &e))
        })
    }

    /// Every recipe in a project, keyed by photograph.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails, `AURA-RENDER-8005` when a body is not a recipe.
    pub fn all(&self, project: &ProjectId) -> AuraResult<BTreeMap<String, Recipe>> {
        let key = project.to_db();
        let rows: Vec<(String, String)> = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, body FROM edit_recipes
                      WHERE project_id = ?1 ORDER BY photo_id",
                )
                .map_err(|e| statement_failed("could not prepare the edit read", &e))?;
            let mapped = statement
                .query_map(params![key], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| statement_failed("could not read the edits", &e))?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row.map_err(|e| statement_failed("could not read an edit", &e))?);
            }
            Ok(out)
        })?;

        let mut out = BTreeMap::new();
        for (photo, body) in rows {
            out.insert(photo, crate::sidecar::parse(&body)?);
        }
        Ok(out)
    }
}

/// The four inputs of a render hash, plus what the file turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportInputs {
    /// The RAW's content address.
    pub content_hash: String,
    /// The recipe's hash.
    pub recipe_hash: String,
    /// The engine string.
    pub engine: String,
    /// The output spec's canonical form.
    pub output_spec: String,
    /// The `RenderLevel` slug.
    pub level: String,
    /// The output colour space slug.
    pub colour_space: String,
    /// 8 or 16.
    pub bit_depth: u8,
    /// `cpu`, or the accelerator's name.
    pub backend: String,
    /// Output width.
    pub width: u32,
    /// Output height.
    pub height: u32,
    /// How long it took.
    pub ms: u32,
}

fn source_from_db(text: &str) -> EditSource {
    match text {
        "ai" => EditSource::Ai,
        "user" => EditSource::User,
        "qc" => EditSource::Qc,
        "preset" => EditSource::Preset,
        _ => EditSource::Default,
    }
}

/// A recipe that could not be parsed out of the catalog.
///
/// Not a variant of anything: a body that does not parse means the row was written by
/// something other than [`RecipeStore::save`], and the only honest answer is the sidecar's
/// own code.
#[must_use]
pub fn unreadable_body(detail: &str) -> aura_core::AuraError {
    sidecar_unreadable(format!("stored edit is not a recipe: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use aura_catalog::model::ProjectRow;
    use aura_core::clock::SystemClock;

    fn catalog() -> (tempfile::TempDir, Arc<Catalog>, Arc<dyn Clock>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
        let catalog =
            Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "test").expect("open");
        (dir, Arc::new(catalog), clock)
    }

    fn seed(catalog: &Catalog, project: &ProjectId, photo: &PhotoId) {
        let row = ProjectRow {
            project_id: project.to_db(),
            name: "wedding".to_string(),
            couple_label: None,
            event_date: None,
            timezone: "UTC".to_string(),
            status: "active".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        catalog
            .writer()
            .transact(move |conn| aura_catalog::repo::create_project(conn, &row))
            .expect("project");
        let project_key = project.to_db();
        let photo_key = photo.to_db();
        catalog
            .writer()
            .transact(move |conn| {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                     VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![photo_key, project_key],
                )
                .map_err(|e| statement_failed("seed photo", &e))?;
                Ok(())
            })
            .expect("photo");
    }

    #[test]
    fn a_saved_recipe_reads_back_identically() {
        let (_dir, catalog, clock) = catalog();
        let project = ProjectId::new();
        let photo = PhotoId::new();
        seed(&catalog, &project, &photo);

        let store = RecipeStore::new(Arc::clone(&catalog), clock);
        let recipe = fixtures::reference();
        store
            .save(&project, &photo, &recipe, &[], "first")
            .expect("save");

        let back = store.load(&photo).expect("load").expect("some");
        assert_eq!(
            recipe_hash(&back).expect("back"),
            recipe_hash(&recipe).expect("original")
        );
    }

    #[test]
    fn a_history_step_is_recorded_and_rebuilt() {
        let (_dir, catalog, clock) = catalog();
        let project = ProjectId::new();
        let photo = PhotoId::new();
        seed(&catalog, &project, &photo);
        let store = RecipeStore::new(Arc::clone(&catalog), clock);

        let original = fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4");
        store
            .save(&project, &photo, &original, &[], "first")
            .expect("save");

        let mut edited = original.clone();
        edited.global.exposure = 1.25;
        store
            .save(
                &project,
                &photo,
                &edited,
                &["global.exposure".to_string()],
                "Exposure",
            )
            .expect("save");

        let history = store.history(&photo, original).expect("history");
        assert_eq!(history.entries().len(), 1);
        assert!(history.can_undo());
        assert!((history.current().global.exposure - 1.25).abs() < 1e-6);
    }

    #[test]
    fn the_history_is_trimmed_to_the_bound_on_write() {
        let (_dir, catalog, clock) = catalog();
        let project = ProjectId::new();
        let photo = PhotoId::new();
        seed(&catalog, &project, &photo);
        let store = RecipeStore::new(Arc::clone(&catalog), clock);

        let mut recipe = fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4");
        for i in 0..(MAX_ENTRIES + 10) {
            recipe.global.exposure = i as f32 * 0.001;
            store
                .save(
                    &project,
                    &photo,
                    &recipe,
                    &["global.exposure".to_string()],
                    "Exposure",
                )
                .expect("save");
        }

        let count = catalog.count("edit_history").expect("count");
        assert!(
            count <= MAX_ENTRIES as i64,
            "history is bounded on write, found {count}"
        );
    }

    #[test]
    fn coverage_counts_every_photograph_not_only_the_edited_ones() {
        let (_dir, catalog, clock) = catalog();
        let project = ProjectId::new();
        let a = PhotoId::new();
        let b = PhotoId::new();
        seed(&catalog, &project, &a);
        let project_key = project.to_db();
        let b_key = b.to_db();
        catalog
            .writer()
            .transact(move |conn| {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                     VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![b_key, project_key],
                )
                .map_err(|e| statement_failed("seed photo", &e))?;
                Ok(())
            })
            .expect("second photo");

        let store = RecipeStore::new(Arc::clone(&catalog), clock);
        let mut recipe = fixtures::reference();
        recipe.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        store
            .save(&project, &a, &recipe, &[], "first")
            .expect("save");

        let coverage = store.coverage(&project).expect("coverage");
        assert_eq!(coverage.images, 2);
        assert_eq!(coverage.with_recipe, 1);
        assert_eq!(coverage.touched_by_hand, 1);
        assert_eq!(coverage.sidecar_behind, 1);
        assert!((coverage.fraction() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_export_records_the_four_inputs_of_its_hash() {
        let (_dir, catalog, clock) = catalog();
        let project = ProjectId::new();
        let photo = PhotoId::new();
        seed(&catalog, &project, &photo);
        let store = RecipeStore::new(Arc::clone(&catalog), clock);

        let inputs = ExportInputs {
            content_hash: fixtures::FIXTURE_HASH.to_string(),
            recipe_hash: "c".repeat(64),
            engine: crate::contract::recipe::ENGINE.to_string(),
            output_spec: "srgb/8".to_string(),
            level: "full".to_string(),
            colour_space: "srgb".to_string(),
            bit_depth: 8,
            backend: "cpu".to_string(),
            width: 6000,
            height: 4000,
            ms: 812,
        };
        let render_hash = "d".repeat(64);
        store
            .record_export(&project, &photo, &render_hash, inputs.clone())
            .expect("record");

        let back = store
            .export_inputs(&render_hash)
            .expect("read")
            .expect("some");
        assert_eq!(back, inputs);
    }
}
