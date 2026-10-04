//! Migration 31 and the rows.
//!
//! Serialisation is JSON in a text column for the three aggregate shapes and the delta, and
//! typed columns for everything a query needs to answer. The split is the one phases 17 and 25
//! made: a column exists when something asks "how many looks are above a strength", and a JSON
//! blob when the value is only ever read back whole. Thirty-two columns of `ToneLandmarks`
//! would be thirty-two columns nothing ever selects one of.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::error::AuraResult;
use aura_core::contract::ids::{ProfileId, ProjectId};
use aura_core::contract::look::{
    BucketResidual, LookAggregate, LookBucket, LookCode, LookDiagnostics, LookMatchReport,
    LookOutline, LookProfile, LookReason, MediaSource, ReferenceOrigin, ReferenceReading,
};
use aura_core::contract::style::{LightingBucket, StyleDelta};
use aura_core::errors::db::statement_failed;
use aura_core::errors::ml::look_refused;

/// Migration 31's tables.
pub struct LookStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for LookStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LookStore").finish_non_exhaustive()
    }
}

/// Milliseconds since the Unix epoch.
fn epoch_ms(now: time::OffsetDateTime) -> i64 {
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

/// Anything serde can write, as text, or `null` when it cannot.
///
/// A look whose aggregate will not serialise is a bug rather than a state, and the row is still
/// written - with an empty object - so that the profile is visible in the panel and can be
/// deleted. A refusal here would leave a half-written profile nobody can see or remove.
fn json_of<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

/// Anything serde can read, or its default.
fn from_json<T: serde::de::DeserializeOwned + Default>(text: &str) -> T {
    serde_json::from_str(text).unwrap_or_default()
}

impl LookStore {
    /// Wrap one catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath, for the panel and the gate.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    // -----------------------------------------------------------------
    // Writing a look
    // -----------------------------------------------------------------

    /// Store one look, its buckets, its reference rows and its reasons, in one transaction.
    ///
    /// Replaces whatever was there under the same id. A look is re-measured rather than
    /// amended - the reference folder may have gained photographs, the measurer may have moved -
    /// and a partial replacement would leave buckets from one measurement beside buckets from
    /// another, which is the comparison `AURA-ML-5148` exists to prevent.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put(&self, look: &LookProfile, readings: &[ReferenceReading]) -> AuraResult<()> {
        let look = look.clone();
        let readings = readings.to_vec();
        self.catalog.writer().transact(move |conn| {
            let id = look.id.to_db();
            conn.execute("DELETE FROM look_profile WHERE id = ?1", [&id])
                .map_err(|err| statement_failed("clear look_profile", &err))?;

            conn.execute(
                "INSERT INTO look_profile (
                     id, name, origin_key, source, global_delta,
                     references_used, references_found, references_refused, baseline_frames,
                     strength, measured_at, engine_ver, analysis_ver
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    &id,
                    &look.name,
                    look.origin.as_key(),
                    look.source.as_str(),
                    json_of(&look.global),
                    i64::from(look.references),
                    i64::from(look.diagnostics.found),
                    i64::from(look.diagnostics.refused),
                    i64::from(look.diagnostics.baseline_frames),
                    f64::from(look.diagnostics.strength),
                    look.measured_at,
                    &look.engine_ver,
                    i64::from(look.analysis_ver),
                ],
            )
            .map_err(|err| statement_failed("insert look_profile", &err))?;

            for (lighting, bucket) in &look.buckets {
                conn.execute(
                    "INSERT INTO look_bucket (
                         profile_id, lighting, reference_json, baseline_json, delta_json,
                         samples, confidence
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        &id,
                        lighting.as_str(),
                        json_of(&bucket.reference),
                        json_of(&bucket.baseline),
                        json_of(&bucket.delta),
                        i64::from(bucket.reference.samples),
                        f64::from(bucket.confidence),
                    ],
                )
                .map_err(|err| statement_failed("insert look_bucket", &err))?;
            }

            for reading in &readings {
                conn.execute(
                    "INSERT OR REPLACE INTO look_reference (
                         profile_id, content_hash, file_name, lighting,
                         lighting_confidence, reading_json
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        &id,
                        &reading.key,
                        &reading.key,
                        reading.lighting.as_str(),
                        f64::from(reading.lighting_confidence),
                        json_of(reading),
                    ],
                )
                .map_err(|err| statement_failed("insert look_reference", &err))?;
            }

            for (ordinal, reason) in look.diagnostics.reasons.iter().enumerate() {
                conn.execute(
                    "INSERT INTO look_reason (profile_id, ordinal, code, value, threshold)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        &id,
                        i64::try_from(ordinal).unwrap_or(i64::MAX),
                        reason.code.as_str(),
                        reason.value.map(f64::from),
                        reason.threshold.map(f64::from),
                    ],
                )
                .map_err(|err| statement_failed("insert look_reason", &err))?;
            }

            Ok(())
        })
    }

    // -----------------------------------------------------------------
    // Reading one back
    // -----------------------------------------------------------------

    /// Every stored look, newest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn profiles(&self) -> AuraResult<Vec<LookProfile>> {
        let ids = self.catalog.read(|conn| {
            let mut statement = conn
                .prepare("SELECT id FROM look_profile ORDER BY measured_at DESC, id")
                .map_err(|err| statement_failed("prepare look_profile", &err))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|err| statement_failed("query look_profile", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("read look_profile", &err))?);
            }
            Ok(out)
        })?;

        let mut out = Vec::new();
        for id in ids {
            if let Ok(parsed) = ProfileId::from_db(&id) {
                if let Some(profile) = self.profile(parsed)? {
                    out.push(profile);
                }
            }
        }
        Ok(out)
    }

    /// One look and its buckets.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn profile(&self, id: ProfileId) -> AuraResult<Option<LookProfile>> {
        let key = id.to_db();
        let row = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT name, origin_key, source, global_delta, references_used,
                            references_found, references_refused, baseline_frames, strength,
                            measured_at, engine_ver, analysis_ver
                     FROM look_profile WHERE id = ?1",
                )
                .map_err(|err| statement_failed("prepare look_profile row", &err))?;
            let mut rows = statement
                .query_map([&key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, f64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, i64>(11)?,
                    ))
                })
                .map_err(|err| statement_failed("query look_profile row", &err))?;
            match rows.next() {
                Some(row) => {
                    Ok(Some(row.map_err(|err| {
                        statement_failed("read look_profile row", &err)
                    })?))
                }
                None => Ok(None),
            }
        })?;

        let Some((
            name,
            origin_key,
            source,
            global,
            used,
            found,
            refused,
            baseline_frames,
            strength,
            measured_at,
            engine_ver,
            analysis_ver,
        )) = row
        else {
            return Ok(None);
        };

        let buckets = self.buckets_of(id)?;
        let reasons = self.reasons_of(id)?;
        let weak = buckets
            .values()
            .filter(|bucket| bucket.reference.is_weak())
            .count();

        Ok(Some(LookProfile {
            id,
            name,
            origin: ReferenceOrigin::from_key(&origin_key),
            source: MediaSource::from_str_or_folder(&source),
            global: from_json::<StyleDelta>(&global),
            diagnostics: LookDiagnostics {
                found: u32::try_from(found).unwrap_or(0),
                measured: u32::try_from(used).unwrap_or(0),
                refused: u32::try_from(refused).unwrap_or(0),
                baseline_frames: u32::try_from(baseline_frames).unwrap_or(0),
                buckets_populated: u32::try_from(buckets.len()).unwrap_or(0),
                buckets_weak: u32::try_from(weak).unwrap_or(0),
                strength: strength as f32,
                summary: crate::api::summarise(&reasons),
                reasons,
            },
            buckets,
            references: u32::try_from(used).unwrap_or(0),
            measured_at,
            engine_ver,
            analysis_ver: u16::try_from(analysis_ver).unwrap_or(0),
        }))
    }

    fn buckets_of(&self, id: ProfileId) -> AuraResult<BTreeMap<LightingBucket, LookBucket>> {
        let key = id.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT lighting, reference_json, baseline_json, delta_json, confidence
                     FROM look_bucket WHERE profile_id = ?1 ORDER BY lighting",
                )
                .map_err(|err| statement_failed("prepare look_bucket", &err))?;
            let rows = statement
                .query_map([&key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, f64>(4)?,
                    ))
                })
                .map_err(|err| statement_failed("query look_bucket", &err))?;

            let mut out = BTreeMap::new();
            for row in rows {
                let (lighting, reference, baseline, delta, confidence) =
                    row.map_err(|err| statement_failed("read look_bucket", &err))?;
                let lighting = LightingBucket::from_str_or_unknown(&lighting);
                out.insert(
                    lighting,
                    LookBucket {
                        lighting,
                        reference: from_json::<LookAggregate>(&reference),
                        baseline: from_json::<LookAggregate>(&baseline),
                        delta: from_json::<StyleDelta>(&delta),
                        confidence: confidence as f32,
                        reasons: Vec::new(),
                    },
                );
            }
            Ok(out)
        })
    }

    fn reasons_of(&self, id: ProfileId) -> AuraResult<Vec<LookReason>> {
        let key = id.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT code, value, threshold FROM look_reason
                     WHERE profile_id = ?1 ORDER BY ordinal",
                )
                .map_err(|err| statement_failed("prepare look_reason", &err))?;
            let rows = statement
                .query_map([&key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<f64>>(1)?,
                        row.get::<_, Option<f64>>(2)?,
                    ))
                })
                .map_err(|err| statement_failed("query look_reason", &err))?;
            let mut out = Vec::new();
            for row in rows {
                let (code, value, threshold) =
                    row.map_err(|err| statement_failed("read look_reason", &err))?;
                out.push(LookReason {
                    code: LookCode::from_str_or_empty(&code),
                    value: value.map(|v| v as f32),
                    threshold: threshold.map(|v| v as f32),
                });
            }
            Ok(out)
        })
    }

    /// Delete one look. A project that had it selected falls back to none.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn forget(&self, id: ProfileId) -> AuraResult<()> {
        let key = id.to_db();
        self.catalog.writer().transact(move |conn| {
            conn.execute("DELETE FROM look_profile WHERE id = ?1", [&key])
                .map_err(|err| statement_failed("delete look_profile", &err))?;
            Ok(())
        })
    }

    // -----------------------------------------------------------------
    // Matches
    // -----------------------------------------------------------------

    /// Record what a look did to a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_match(&self, report: &LookMatchReport, engine: &str) -> AuraResult<()> {
        let report = report.clone();
        let engine = engine.to_string();
        let now = epoch_ms(self.clock.now_utc());
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO look_match (
                     project_id, profile_id, before_de00, after_de00, frames, user_edited,
                     buckets_json, measured_at, engine_ver
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    report.project.to_db(),
                    report.profile.to_db(),
                    f64::from(report.before_de00),
                    f64::from(report.after_de00),
                    i64::from(report.frames),
                    i64::from(report.user_edited),
                    json_of(&report.buckets),
                    now,
                    &engine,
                ],
            )
            .map_err(|err| statement_failed("insert look_match", &err))?;
            Ok(())
        })
    }

    /// The most recent match measured on one project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn last_match(&self, project: ProjectId) -> AuraResult<Option<LookMatchReport>> {
        let key = project.to_db();
        let row = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT profile_id, before_de00, after_de00, frames, user_edited, buckets_json
                     FROM look_match WHERE project_id = ?1
                     ORDER BY measured_at DESC, id DESC LIMIT 1",
                )
                .map_err(|err| statement_failed("prepare look_match", &err))?;
            let mut rows = statement
                .query_map([&key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })
                .map_err(|err| statement_failed("query look_match", &err))?;
            match rows.next() {
                Some(row) => {
                    Ok(Some(row.map_err(|err| {
                        statement_failed("read look_match", &err)
                    })?))
                }
                None => Ok(None),
            }
        })?;

        let Some((profile, before, after, frames, user_edited, buckets)) = row else {
            return Ok(None);
        };
        let Ok(profile) = ProfileId::from_db(&profile) else {
            return Ok(None);
        };

        let buckets: Vec<BucketResidual> = serde_json::from_str(&buckets).unwrap_or_default();
        let user_edited = u32::try_from(user_edited).unwrap_or(0);
        let applied = u32::try_from(frames).unwrap_or(0);
        // Derived from the stored bucket rows rather than kept in a column of its own. It is a
        // sum of numbers already on the row, and a second copy of a derived value is a second
        // copy that can disagree with the first - which is the defect phase 26 found in its own
        // storage note and phase 31's `shrink_of` avoids the same way.
        let measured = crate::verify::measured_frames(&buckets);
        Ok(Some(LookMatchReport {
            profile,
            project,
            reasons: crate::verify::reasons_for(&buckets, applied, user_edited),
            buckets,
            before_de00: before as f32,
            after_de00: after as f32,
            frames: applied,
            measured_frames: measured,
            user_edited,
        }))
    }

    // -----------------------------------------------------------------
    // Selection
    // -----------------------------------------------------------------

    /// Point a project at a look, or at none.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5147` when the database refuses the selection, which is what
    /// `project_look_needs_a_match` raises for a look nobody has measured.
    pub fn select(&self, project: ProjectId, profile: Option<ProfileId>) -> AuraResult<()> {
        let key = project.to_db();
        let chosen = profile.map(|id| id.to_db());
        let now = epoch_ms(self.clock.now_utc());
        self.catalog
            .writer()
            .transact(move |conn| {
                conn.execute(
                    "INSERT INTO project_look (project_id, profile_id, strength, selected_at)
                     VALUES (?1, ?2, 1.0, ?3)
                     ON CONFLICT(project_id) DO UPDATE SET
                         profile_id = excluded.profile_id,
                         selected_at = excluded.selected_at",
                    rusqlite::params![&key, &chosen, now],
                )
                .map_err(|err| statement_failed("select look", &err))?;
                Ok(())
            })
            .map_err(|_| {
                look_refused(
                    "that look has not been measured on any project yet, so it cannot be applied",
                )
            })
    }

    /// Apply a look at less than its measured strength.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn set_strength(&self, project: ProjectId, fraction: f32) -> AuraResult<()> {
        let key = project.to_db();
        let fraction = f64::from(fraction.clamp(0.0, 1.0));
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE project_look SET strength = ?2 WHERE project_id = ?1",
                rusqlite::params![&key, fraction],
            )
            .map_err(|err| statement_failed("set look strength", &err))?;
            Ok(())
        })
    }

    /// Rename a look.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn rename(&self, id: ProfileId, name: &str) -> AuraResult<()> {
        let key = id.to_db();
        let name = name.to_string();
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE look_profile SET name = ?2 WHERE id = ?1",
                rusqlite::params![&key, &name],
            )
            .map_err(|err| statement_failed("rename look", &err))?;
            Ok(())
        })
    }

    /// Which look a project uses, and how strongly.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn selected(&self, project: ProjectId) -> AuraResult<Option<(ProfileId, f32)>> {
        let key = project.to_db();
        let row = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT profile_id, strength FROM project_look
                     WHERE project_id = ?1 AND profile_id IS NOT NULL",
                )
                .map_err(|err| statement_failed("prepare project_look", &err))?;
            let mut rows = statement
                .query_map([&key], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })
                .map_err(|err| statement_failed("query project_look", &err))?;
            match rows.next() {
                Some(row) => {
                    Ok(Some(row.map_err(|err| {
                        statement_failed("read project_look", &err)
                    })?))
                }
                None => Ok(None),
            }
        })?;

        let Some((id, strength)) = row else {
            return Ok(None);
        };
        Ok(ProfileId::from_db(&id)
            .ok()
            .map(|parsed| (parsed, strength as f32)))
    }

    /// What a caller needs to know about looks in one project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn outline(&self, project: ProjectId) -> AuraResult<LookOutline> {
        let key = project.to_db();
        let (profiles, photographs, appliable) = self.catalog.read(move |conn| {
            let profiles: i64 = conn
                .query_row("SELECT COUNT(*) FROM look_profile", [], |row| row.get(0))
                .unwrap_or(0);
            let photographs: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
                    [&key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            // The denominator is frames phases 15 and 16 have decided, because a frame with no
            // baseline has nothing for a look to be a residual from. Both numbers go on the
            // wire; phase 18's rule about saying what the denominator is.
            let appliable: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM image_tone_estimate WHERE project_id = ?1",
                    [&key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok((profiles, photographs, appliable))
        })?;

        let selected = self.selected(project)?;
        let (selected_name, selected_origin) = match selected {
            Some((id, _)) => match self.profile(id)? {
                Some(profile) => (Some(profile.name), Some(profile.origin)),
                None => (None, None),
            },
            None => (None, None),
        };

        Ok(LookOutline {
            profiles: u32::try_from(profiles).unwrap_or(0),
            selected: selected.map(|(id, _)| id),
            selected_name,
            selected_origin,
            appliable: u32::try_from(appliable).unwrap_or(0),
            photographs: u32::try_from(photographs).unwrap_or(0),
            baseline_coverage: if photographs > 0 {
                appliable as f32 / photographs as f32
            } else {
                0.0
            },
            // A const on the contract, read here so there is one answer to it in the product.
            network_transport_available: MediaSource::PublicUrl.can_fetch(),
        })
    }
}
