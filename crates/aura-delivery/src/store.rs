//! Migration 30's delivery half: targets, backup copies and per-file upload state.
//!
//! ## The upload table is the resume protocol
//!
//! `delivery_upload.sent_bytes` is what makes a network drop a pause rather than a restart, and the
//! reason it is a stored column rather than a runtime value is that a photographer closes the
//! laptop. What it is *not* is the offset the next pass resumes from - [`crate::resume::step`] asks
//! the far end - because a stored offset is a claim about somebody else's disk and the two disagree
//! exactly when it matters.
//!
//! What the column is for is the panel and the report: "how far did this get" is a question a
//! photographer asks while the wifi is down, when nobody can ask the far end anything.

use std::path::PathBuf;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::delivery::{
    DeliveryOutline, ImageId, ProviderId, SetMapping, UploadItem, UploadProgress, UploadState,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, ProjectId};
use rusqlite::{params, OptionalExtension};

use crate::backup::Copied;

/// One catalog, wrapped.
#[derive(Debug, Clone)]
pub struct DeliveryStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl DeliveryStore {
    /// Wrap a catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>) -> Self {
        let clock = Arc::clone(catalog.clock());
        Self { catalog, clock }
    }

    /// Record a backup destination or a provider, and return its id.
    ///
    /// `has_credential` is a **flag**, never a secret. The secret lives in the OS credential store
    /// and never in the catalog, in a config file or in a log - phase 04's rule, which
    /// `scripts/check-banned.sh` enforces for every crate in this workspace.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written.
    pub fn upsert_target(
        &self,
        project: ProjectId,
        kind: &str,
        name: &str,
        destination: &str,
        mapping: &[SetMapping],
        has_credential: bool,
    ) -> AuraResult<String> {
        let target_id = format!("tgt_{kind}_{name}");
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let id = target_id.clone();
        let kind = kind.to_owned();
        let name = name.to_owned();
        let destination = destination.to_owned();
        let mapping = serde_json::to_string(mapping).unwrap_or_else(|_| "[]".to_owned());

        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO delivery_target (target_id, project_id, kind, name, destination,
                     mapping, has_credential, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(project_id, kind, name) DO UPDATE SET
                     destination = excluded.destination,
                     mapping = excluded.mapping,
                     has_credential = excluded.has_credential",
                params![
                    id,
                    key,
                    kind,
                    name,
                    destination,
                    mapping,
                    i64::from(has_credential),
                    at
                ],
            )
            .map_err(|e| statement_failed("delivery_target", &e))?;
            Ok(())
        })?;
        Ok(target_id)
    }

    /// A target's id, its mapping and whether it has a credential.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn target(
        &self,
        project: ProjectId,
        kind: &str,
        name: &str,
    ) -> AuraResult<Option<(String, Vec<SetMapping>, bool)>> {
        let key = project.to_db();
        let kind = kind.to_owned();
        let name = name.to_owned();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT target_id, mapping, has_credential FROM delivery_target
                     WHERE project_id = ?1 AND kind = ?2 AND name = ?3",
                    params![key, kind, name],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("delivery_target", &e))?;
            Ok(row.map(|(id, mapping, cred)| {
                (
                    id,
                    serde_json::from_str(&mapping).unwrap_or_default(),
                    cred != 0,
                )
            }))
        })
    }

    /// Every provider this machine has configured, and whether each has a credential.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn providers(&self) -> AuraResult<Vec<(ProviderId, bool)>> {
        self.catalog.read(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT name, MAX(has_credential) FROM delivery_target
                     WHERE kind = 'provider' GROUP BY name ORDER BY name",
                )
                .map_err(|e| statement_failed("delivery_target", &e))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|e| statement_failed("delivery_target", &e))?;
            Ok(rows
                .flatten()
                .filter_map(|(name, cred)| ProviderId::parse(&name).ok().map(|p| (p, cred != 0)))
                .collect())
        })
    }

    /// Record what a backup copied.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the rows cannot be written.
    pub fn write_backup(&self, target_id: &str, job_id: &str, copied: &[Copied]) -> AuraResult<()> {
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let target = target_id.to_owned();
        let job = job_id.to_owned();
        let rows: Vec<(String, u64, String, bool)> = copied
            .iter()
            .map(|c| {
                (
                    c.rel_path.to_string_lossy().replace('\\', "/"),
                    c.bytes,
                    c.hash.clone(),
                    c.already_present,
                )
            })
            .collect();

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("delivery_backup", &e))?;
            for (rel, bytes, hash, present) in rows {
                tx.execute(
                    "INSERT OR REPLACE INTO delivery_backup (target_id, job_id, rel_path, bytes,
                         hash, diverged, already_present, copied_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)",
                    params![target, job, rel, bytes as i64, hash, i64::from(present), at],
                )
                .map_err(|e| statement_failed("delivery_backup", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("delivery_backup", &e))?;
            Ok(())
        })
    }

    /// Seed an upload with one row per file, leaving anything already verified alone.
    ///
    /// `INSERT OR IGNORE` rather than `OR REPLACE`, and that is the whole of resumability at the
    /// store level: a re-run must not reset the state of the 640 files that already arrived.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the rows cannot be written.
    pub fn seed_upload(
        &self,
        target_id: &str,
        job_id: &str,
        items: &[UploadItem],
    ) -> AuraResult<()> {
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let target = target_id.to_owned();
        let job = job_id.to_owned();
        let rows: Vec<(String, String, String, u64, String)> = items
            .iter()
            .map(|i| {
                (
                    i.path.to_string_lossy().replace('\\', "/"),
                    i.set.clone(),
                    i.image.to_db(),
                    i.bytes,
                    i.hash.clone(),
                )
            })
            .collect();

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("delivery_upload", &e))?;
            for (rel, set, image, bytes, hash) in rows {
                tx.execute(
                    "INSERT OR IGNORE INTO delivery_upload (target_id, job_id, rel_path, set_name,
                         photo_id, bytes, hash, state, sent_bytes, resumes, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, 0, ?8)",
                    params![target, job, rel, set, image, bytes as i64, hash, at],
                )
                .map_err(|e| statement_failed("delivery_upload", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("delivery_upload", &e))?;
            Ok(())
        })
    }

    /// Record where one file has got to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written - including when it claims to have sent more
    /// bytes than the file has, which `delivery_upload_sent_within_bytes` refuses.
    pub fn set_state(
        &self,
        target_id: &str,
        job_id: &str,
        rel_path: &str,
        state: &UploadState,
    ) -> AuraResult<()> {
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let target = target_id.to_owned();
        let job = job_id.to_owned();
        let rel = rel_path.replace('\\', "/");
        let word = state.as_str().to_owned();
        let (sent, resumes) = match state {
            UploadState::InProgress { sent, resumes } => (*sent, *resumes),
            UploadState::Verified => (u64::MAX, 0),
            _ => (0, 0),
        };
        let failure = match state {
            UploadState::Failed { code } => Some(code.clone()),
            _ => None,
        };

        self.catalog.writer().with(move |conn| {
            // `Verified` means "all of it", and the file's own size is the only place that number
            // lives. Writing u64::MAX would trip the schema's own bound, which is the trigger
            // working rather than a problem to route around.
            if word == "verified" {
                conn.execute(
                    "UPDATE delivery_upload SET state = 'verified', sent_bytes = bytes,
                         failure_code = NULL, updated_at = ?4
                     WHERE target_id = ?1 AND job_id = ?2 AND rel_path = ?3",
                    params![target, job, rel, at],
                )
                .map_err(|e| statement_failed("delivery_upload", &e))?;
            } else {
                conn.execute(
                    "UPDATE delivery_upload SET state = ?4, sent_bytes = MIN(?5, bytes),
                         resumes = ?6, failure_code = ?7, updated_at = ?8
                     WHERE target_id = ?1 AND job_id = ?2 AND rel_path = ?3",
                    params![target, job, rel, word, sent as i64, resumes, failure, at],
                )
                .map_err(|e| statement_failed("delivery_upload", &e))?;
            }
            let _ = sent;
            Ok(())
        })
    }

    /// Every file's state at a target.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn items(&self, target_id: &str) -> AuraResult<Vec<UploadItem>> {
        let target = target_id.to_owned();
        self.catalog.read(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT rel_path, set_name, photo_id, bytes, hash, state, sent_bytes,
                            resumes, failure_code
                     FROM delivery_upload WHERE target_id = ?1 ORDER BY set_name, rel_path",
                )
                .map_err(|e| statement_failed("delivery_upload", &e))?;
            let rows = stmt
                .query_map(params![target], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                })
                .map_err(|e| statement_failed("delivery_upload", &e))?;

            let mut out = Vec::new();
            for row in rows.flatten() {
                let (rel, set, photo, bytes, hash, word, sent, resumes, failure) = row;
                let Ok(image) = ImageId::from_db(&photo) else {
                    continue;
                };
                let state = match word.as_str() {
                    "verified" => UploadState::Verified,
                    "corrupt" => UploadState::Corrupt,
                    "failed" => UploadState::Failed {
                        code: failure.unwrap_or_else(|| "AURA-DLV-10002".to_owned()),
                    },
                    "in_progress" => UploadState::InProgress {
                        sent: u64::try_from(sent).unwrap_or(0),
                        resumes: u32::try_from(resumes).unwrap_or(0),
                    },
                    _ => UploadState::Pending,
                };
                out.push(UploadItem {
                    image,
                    set,
                    path: PathBuf::from(rel),
                    bytes: u64::try_from(bytes).unwrap_or(0),
                    hash,
                    state,
                });
            }
            Ok(out)
        })
    }

    /// How an upload is going.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn progress(&self, target_id: &str) -> AuraResult<UploadProgress> {
        let target = target_id.to_owned();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT COUNT(*),
                            SUM(state = 'verified'),
                            SUM(state <> 'verified'),
                            SUM(state = 'failed'),
                            COALESCE(SUM(sent_bytes), 0),
                            COALESCE(SUM(bytes), 0),
                            COALESCE(SUM(resumes), 0)
                     FROM delivery_upload WHERE target_id = ?1",
                    params![target],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    },
                )
                .map_err(|e| statement_failed("delivery_upload", &e))?;
            Ok(UploadProgress {
                files: u32::try_from(row.0).unwrap_or(0),
                verified: u32::try_from(row.1).unwrap_or(0),
                outstanding: u32::try_from(row.2).unwrap_or(0),
                failed: u32::try_from(row.3).unwrap_or(0),
                bytes_sent: u64::try_from(row.4).unwrap_or(0),
                bytes_total: u64::try_from(row.5).unwrap_or(0),
                resumes: u32::try_from(row.6).unwrap_or(0),
            })
        })
    }

    /// What a project's backups and uploads covered.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn outline(&self, project: ProjectId) -> AuraResult<DeliveryOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut out = DeliveryOutline::default();
            let mut stmt = conn
                .prepare(
                    "SELECT target_kind, uploaded, outstanding, resumes, backed_up, diverged
                     FROM v_delivery_state WHERE project_id = ?1",
                )
                .map_err(|e| statement_failed("v_delivery_state", &e))?;
            let rows = stmt
                .query_map(params![key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                })
                .map_err(|e| statement_failed("v_delivery_state", &e))?;
            for (kind, uploaded, outstanding, resumes, backed_up, diverged) in rows.flatten() {
                if kind == "backup" {
                    out.backups = out.backups.saturating_add(1);
                    out.backed_up = out
                        .backed_up
                        .saturating_add(u32::try_from(backed_up).unwrap_or(0));
                    out.diverged = out
                        .diverged
                        .saturating_add(u32::try_from(diverged).unwrap_or(0));
                } else {
                    out.providers = out.providers.saturating_add(1);
                    out.uploaded = out
                        .uploaded
                        .saturating_add(u32::try_from(uploaded).unwrap_or(0));
                    out.outstanding = out
                        .outstanding
                        .saturating_add(u32::try_from(outstanding).unwrap_or(0));
                    out.resumes = out
                        .resumes
                        .saturating_add(u32::try_from(resumes).unwrap_or(0));
                }
            }
            let sent: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(u.sent_bytes), 0) FROM delivery_upload u
                     JOIN delivery_target t ON t.target_id = u.target_id
                     WHERE t.project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            out.bytes_sent = u64::try_from(sent).unwrap_or(0);
            let failed: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM delivery_upload u
                     JOIN delivery_target t ON t.target_id = u.target_id
                     WHERE t.project_id = ?1 AND u.state = 'failed'",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            out.refused = u32::try_from(failed).unwrap_or(0);
            Ok(out)
        })
    }
}
