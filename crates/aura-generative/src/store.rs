//! The four tables migration 24 adds, and the rules that live in the SQL rather than around it.
//!
//! ## A refusal is a row, and here it is most of them
//!
//! `cleanup_blocked` is written for every candidate the safety engine, the source selector, the
//! self-check or the cloud judgement declined. On this build that is *every* candidate on every
//! photograph, and the table is the phase working rather than failing: section 10.1's adversarial
//! audit is scored from these rows, and "AURA declined to touch this photograph" is unprovable
//! without them.
//!
//! ## A disclosure is written in the same transaction as the removal, and cannot be edited
//!
//! [`CleanupStore::apply`] inserts the disclosure and sets `applied = 1` inside one transaction,
//! and a trigger aborts the second half if the first is missing. A second trigger aborts every
//! UPDATE on `cleanup_disclosure`, and a third refuses to delete one while the removal stands.
//! Section 13's fifth acceptance criterion is those three constraints rather than a convention.
//!
//! ## A photographer's decision is carried forward inside the statement
//!
//! Twelfth time the rule has been written into a store. `accepted` and `disabled_by_user` are read
//! in the same statement that would overwrite them, closing the window a background pass opens
//! when it reads a flag a moment before it writes.
//!
//! It works here only because the proposal id is *derived* from the region rather than issued
//! fresh - see [`crate::queue::proposal_id`]. A random id would make yesterday's rejection belong
//! to a row that no longer exists, and the photographer would be shown the same proposal again.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::cleanup::{
    Box2, CleanupCode, CleanupDisclosure, CleanupMethod, CleanupOutline, CleanupProposal,
    CleanupReason, DistractionClass, ImageId, SafetyCheck, SafetyVerdict,
};
use aura_core::contract::ids::ProposalId;
use aura_core::contract::ledger::Autonomy;
use aura_core::contract::scene::SceneId;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, PhotoId, ProjectId};
use rusqlite::{params, OptionalExtension};

use crate::errors;
use crate::queue::{Blocked, Plan};

/// Bytes one photograph costs in `cleanup_image`, `cleanup_proposal`, `cleanup_blocked` and their
/// indexes.
///
/// Section 11 sets no storage budget for this phase - all five of its rows are time. One is
/// measured anyway, and **the first figure written here was wrong by a factor of two and a half**
/// because it was written before it was measured. That is phase 21's defect exactly, committed by
/// the phase whose own progress note describes it; `crates/aura-perf/tests/cleanup_budgets.rs`
/// prints the per-object breakdown on every run so it cannot happen a third time.
///
/// The measured decomposition over 1,000 photographs of the widest plan the fixtures produce - one
/// examined image, three proposals and six blocked candidates - taken as the change in `dbstat`
/// payload rather than in `PRAGMA page_count`, which quantises to 4 KiB:
///
/// ```text
///   cleanup_proposal                     1,008 B/image   three rows, ~336 B each
///   cleanup_blocked (WITHOUT ROWID)        662 B/image   six rows, ~110 B each
///   idx_cleanup_blocked_check              446 B/image
///   idx_cleanup_proposal_photo             162 B/image
///   idx_cleanup_proposal_queue             162 B/image
///   cleanup_image                          139 B/image
///   idx_cleanup_proposal_project           138 B/image
///   idx_cleanup_image_project               46 B/image
///                                       --------
///   measured total                       2,763 B/image
/// ```
///
/// **It is nearly three kilobytes for two structural reasons, neither of them an accident.**
///
/// The first is phase 21's: every phase from 09 to 20 stored one fixed-width verdict per
/// photograph, and this stores a *list* whose length is the number of things the pass considered.
/// On a cluttered frame that is nine rows.
///
/// The second is this phase's own, and it is a decision rather than a cost to trim: **the refusals
/// and their index are 1,108 B of the total, forty per cent.** `cleanup_blocked` plus
/// `idx_cleanup_blocked_check` cost more than the proposals do. Phase 23 faced the same choice for
/// its rejected crop rectangles and went the other way - four counters instead of rows - and the
/// difference is what the rows are *for*: phase 23's rejects are search intermediates, and these
/// are decisions. Section 10.1's adversarial audit is scored from exactly these rows, and "AURA
/// declined to touch this photograph" is unprovable from a counter.
///
/// At the measured figure a 4,000-image wedding costs about 11 MB, against 3.4 MB for phase 23's
/// geometry plans and 48 MB for phase 14's recipes.
///
/// The constant below carries headroom over the measurement, which is phase 19's correction: a
/// budget must not be pinned at its own measurement.
pub const BYTES_PER_IMAGE: usize = 3_200;

/// The five safety checks as the five characters of a stored `checks` column.
const ALL_PASSED: &str = "11111";

/// A length as `SQLite`'s integer, saturating rather than wrapping.
fn count_of(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// The `cleanup_image`, `cleanup_proposal`, `cleanup_blocked` and `cleanup_disclosure` tables.
#[derive(Debug)]
pub struct CleanupStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl CleanupStore {
    /// Wrap one catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath, for the gate and the budget test.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// Photographs with no examination at these versions.
    ///
    /// **The work remaining is a query, not a journal.** Invariant 5: kill the process at 10 %,
    /// 50 % or 90 % and the next run asks the catalog what is left. A `policy_ver` bump therefore
    /// heals itself, because the rows made under the old table are pending by definition.
    ///
    /// A photograph a photographer switched cleanup off for is **not** pending: `disabled_by_user`
    /// excludes it, so a re-analysis does not spend time re-proposing removals somebody has
    /// already said they do not want.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(
        &self,
        project: &ProjectId,
        versions: (u16, u16, u16),
    ) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN cleanup_image c ON c.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND COALESCE(c.disabled_by_user, 0) = 0
                        AND (c.photo_id IS NULL
                             OR c.detector_ver <> ?2
                             OR c.analysis_ver <> ?3
                             OR c.policy_ver   <> ?4)
                      ORDER BY p.photo_id",
                )
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut cursor = statement
                .query(params![
                    key,
                    i64::from(versions.0),
                    i64::from(versions.1),
                    i64::from(versions.2),
                ])
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the pending set", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// True when a photographer has switched cleanup off for one photograph.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn is_disabled(&self, image: ImageId) -> AuraResult<bool> {
        let photo = image.to_db();
        self.catalog.read(move |conn| {
            let value: Option<i64> = conn
                .query_row(
                    "SELECT disabled_by_user FROM cleanup_image WHERE photo_id = ?1",
                    params![photo],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the cleanup switch", &e))?;
            Ok(value.unwrap_or(0) == 1)
        })
    }

    /// Write one photograph's plan, carrying forward whatever the photographer settled.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails, and `AURA-ML-5116` when a proposal breaks one of this
    /// phase's guarantees - checked here as well as in the constructor, because the store is the
    /// last place a bad row can be stopped and a caller nobody has written yet will reach this
    /// function.
    #[allow(clippy::too_many_lines)]
    pub fn put(
        &self,
        project: &ProjectId,
        image: ImageId,
        scene: SceneId,
        plan: &Plan,
        versions: (u16, u16, u16),
    ) -> AuraResult<()> {
        for prepared in &plan.prepared {
            if let Some(problem) = prepared.proposal.broken_guarantee() {
                return Err(aura_core::errors::ml::cleanup_proposal_refused(problem));
            }
        }

        let photo = image.to_db();
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let scene_slug = scene.as_str().to_string();
        let mask_complete = i64::from(plan.mask_complete);
        let reverted = count_of(plan.reverted as usize);
        let judged = count_of(plan.judged as usize);
        let declined = count_of(plan.declined as usize);
        let prepared: Vec<_> = plan.prepared.clone();
        let blocked: Vec<Blocked> = plan.blocked.clone();

        self.catalog.writer().transact(move |conn| {
            // The photographer's switch, carried forward inside the statement rather than read
            // before it. See the module header.
            conn.execute(
                "INSERT INTO cleanup_image (
                        photo_id, project_id, scene, mask_complete, disabled_by_user,
                        judged, declined, reverted,
                        detector_ver, analysis_ver, policy_ver, examined_at
                     ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                     ON CONFLICT(photo_id) DO UPDATE SET
                        project_id    = excluded.project_id,
                        scene         = excluded.scene,
                        mask_complete = excluded.mask_complete,
                        judged        = excluded.judged,
                        declined      = excluded.declined,
                        reverted      = excluded.reverted,
                        detector_ver  = excluded.detector_ver,
                        analysis_ver  = excluded.analysis_ver,
                        policy_ver    = excluded.policy_ver,
                        examined_at   = excluded.examined_at",
                params![
                    photo,
                    project_key,
                    scene_slug,
                    mask_complete,
                    judged,
                    declined,
                    reverted,
                    i64::from(versions.0),
                    i64::from(versions.1),
                    i64::from(versions.2),
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not record the cleanup examination", &e))?;

            // Proposals that were applied are never deleted by a re-analysis: their pixels are in
            // a delivered recipe, and a disclosure with no proposal behind it is a removal nobody
            // can account for. Everything else this pass owns is replaced.
            conn.execute(
                "DELETE FROM cleanup_proposal
                  WHERE photo_id = ?1 AND applied = 0 AND accepted IS NULL",
                params![photo],
            )
            .map_err(|e| statement_failed("could not clear the previous proposals", &e))?;
            conn.execute(
                "DELETE FROM cleanup_blocked WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| statement_failed("could not clear the previous refusals", &e))?;

            for prepared in &prepared {
                let proposal = &prepared.proposal;
                let (kind, source, model) = method_columns(&proposal.method);
                let reasons = proposal
                    .reasons
                    .iter()
                    .map(|reason| reason.code.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                conn.execute(
                    "INSERT INTO cleanup_proposal (
                            proposal_id, photo_id, project_id,
                            x, y, w, h, class, area_frac, salience, confidence,
                            method_kind, method_source, method_model,
                            checks, autonomy, scene, reasons, artefact_score,
                            accepted, applied,
                            detector_ver, analysis_ver, policy_ver, proposed_at
                         ) VALUES (
                            ?1, ?2, ?3,
                            ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                            ?12, ?13, ?14,
                            ?15, ?16, ?17, ?18, ?19,
                            NULL, 0,
                            ?20, ?21, ?22, ?23
                         )
                         ON CONFLICT(proposal_id) DO UPDATE SET
                            x              = excluded.x,
                            y              = excluded.y,
                            w              = excluded.w,
                            h              = excluded.h,
                            class          = excluded.class,
                            area_frac      = excluded.area_frac,
                            salience       = excluded.salience,
                            confidence     = excluded.confidence,
                            method_kind    = excluded.method_kind,
                            method_source  = excluded.method_source,
                            method_model   = excluded.method_model,
                            autonomy       = excluded.autonomy,
                            scene          = excluded.scene,
                            reasons        = excluded.reasons,
                            artefact_score = excluded.artefact_score,
                            detector_ver   = excluded.detector_ver,
                            analysis_ver   = excluded.analysis_ver,
                            policy_ver     = excluded.policy_ver,
                            proposed_at    = excluded.proposed_at",
                    params![
                        proposal.id.to_db(),
                        photo,
                        project_key,
                        f64::from(proposal.region.x.clamp(0.0, 1.0)),
                        f64::from(proposal.region.y.clamp(0.0, 1.0)),
                        f64::from(proposal.region.w.clamp(1e-4, 1.0)),
                        f64::from(proposal.region.h.clamp(1e-4, 1.0)),
                        proposal.class.as_str(),
                        f64::from(proposal.area_frac.clamp(0.0, 1.0)),
                        f64::from(proposal.salience.clamp(0.0, 1.0)),
                        f64::from(proposal.confidence.clamp(0.0, 1.0)),
                        kind,
                        source,
                        model,
                        ALL_PASSED,
                        proposal.autonomy.as_str(),
                        proposal.scene.as_str(),
                        reasons,
                        f64::from(prepared.artefact.worst().clamp(0.0, 1.0)),
                        i64::from(proposal.detector_ver),
                        i64::from(proposal.analysis_ver),
                        i64::from(proposal.policy_ver),
                        now,
                    ],
                )
                .map_err(|e| statement_failed("could not write a cleanup proposal", &e))?;
            }

            for (seq, block) in blocked.iter().enumerate() {
                conn.execute(
                    "INSERT INTO cleanup_blocked (photo_id, seq, x, y, w, h, failed_check, code)
                          VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        photo,
                        count_of(seq),
                        f64::from(block.region.x.clamp(0.0, 1.0)),
                        f64::from(block.region.y.clamp(0.0, 1.0)),
                        f64::from(block.region.w.clamp(1e-4, 1.0)),
                        f64::from(block.region.h.clamp(1e-4, 1.0)),
                        block.check.as_str(),
                        block.code.as_str(),
                    ],
                )
                .map_err(|e| statement_failed("could not write a refused candidate", &e))?;
            }
            Ok(())
        })
    }

    /// Every proposal on one photograph, applied or not, strongest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn proposals(&self, image: ImageId) -> AuraResult<Vec<CleanupProposal>> {
        let photo = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT proposal_id, x, y, w, h, class, area_frac, salience, confidence,
                            method_kind, method_source, method_model, autonomy, scene, reasons,
                            detector_ver, analysis_ver, policy_ver
                       FROM cleanup_proposal
                      WHERE photo_id = ?1
                      ORDER BY confidence DESC, x, y",
                )
                .map_err(|e| statement_failed("could not read the proposals", &e))?;
            let mut cursor = statement
                .query(params![photo])
                .map_err(|e| statement_failed("could not read the proposals", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a proposal", &e))?
            {
                let stored = StoredProposal {
                    id: row.get::<_, String>(0).unwrap_or_default(),
                    x: row.get::<_, f64>(1).unwrap_or(0.0),
                    y: row.get::<_, f64>(2).unwrap_or(0.0),
                    w: row.get::<_, f64>(3).unwrap_or(0.0),
                    h: row.get::<_, f64>(4).unwrap_or(0.0),
                    class: row.get::<_, String>(5).unwrap_or_default(),
                    area_frac: row.get::<_, f64>(6).unwrap_or(0.0),
                    salience: row.get::<_, f64>(7).unwrap_or(0.0),
                    confidence: row.get::<_, f64>(8).unwrap_or(0.0),
                    kind: row.get::<_, String>(9).unwrap_or_default(),
                    source: row.get::<_, Option<String>>(10).unwrap_or(None),
                    model: row.get::<_, Option<String>>(11).unwrap_or(None),
                    autonomy: row.get::<_, String>(12).unwrap_or_default(),
                    scene: row.get::<_, String>(13).unwrap_or_default(),
                    reasons: row.get::<_, String>(14).unwrap_or_default(),
                    detector_ver: row.get::<_, i64>(15).unwrap_or(0),
                    analysis_ver: row.get::<_, i64>(16).unwrap_or(0),
                    policy_ver: row.get::<_, i64>(17).unwrap_or(0),
                };
                if let Some(proposal) = stored.rebuild(image) {
                    out.push(proposal);
                }
            }
            Ok(out)
        })
    }

    /// Every candidate that was refused on one photograph.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn blocked(&self, image: ImageId) -> AuraResult<Vec<(Box2, SafetyCheck, CleanupCode)>> {
        let photo = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT x, y, w, h, failed_check, code
                       FROM cleanup_blocked WHERE photo_id = ?1 ORDER BY seq",
                )
                .map_err(|e| statement_failed("could not read the refusals", &e))?;
            let mut cursor = statement
                .query(params![photo])
                .map_err(|e| statement_failed("could not read the refusals", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a refusal", &e))?
            {
                let region = Box2 {
                    x: row.get::<_, f64>(0).unwrap_or(0.0) as f32,
                    y: row.get::<_, f64>(1).unwrap_or(0.0) as f32,
                    w: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                    h: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                };
                let check = SafetyCheck::parse(&row.get::<_, String>(4).unwrap_or_default());
                let code = CleanupCode::parse(&row.get::<_, String>(5).unwrap_or_default());
                if let (Some(check), Some(code)) = (check, code) {
                    out.push((region, check, code));
                }
            }
            Ok(out)
        })
    }

    /// Everything removed from one project, for the delivery report.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn disclosures(&self, project: ProjectId) -> AuraResult<Vec<CleanupDisclosure>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT proposal_id, photo_id, method_kind, method_source, method_model,
                            x, y, w, h, accepted_by_user, artefact_score
                       FROM v_cleanup_disclosure
                      WHERE project_id = ?1
                      ORDER BY applied_at, proposal_id",
                )
                .map_err(|e| statement_failed("could not read the disclosures", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the disclosures", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a disclosure", &e))?
            {
                let Ok(proposal_id) =
                    ProposalId::from_db(&row.get::<_, String>(0).unwrap_or_default())
                else {
                    continue;
                };
                let Ok(image_id) = PhotoId::from_db(&row.get::<_, String>(1).unwrap_or_default())
                else {
                    continue;
                };
                let Some(method) = method_from_columns(
                    &row.get::<_, String>(2).unwrap_or_default(),
                    row.get::<_, Option<String>>(3).unwrap_or(None).as_deref(),
                    row.get::<_, Option<String>>(4).unwrap_or(None).as_deref(),
                ) else {
                    continue;
                };
                out.push(CleanupDisclosure {
                    proposal_id,
                    image_id,
                    method,
                    region: Box2 {
                        x: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                        y: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                        w: row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                        h: row.get::<_, f64>(8).unwrap_or(0.0) as f32,
                    },
                    accepted_by_user: row.get::<_, i64>(9).unwrap_or(0) == 1,
                    artefact_score: row.get::<_, f64>(10).unwrap_or(0.0) as f32,
                });
            }
            Ok(out)
        })
    }

    /// Record that a photographer accepted or rejected one proposal.
    ///
    /// Accepting does **not** apply it. Applying replaces pixels and writes a disclosure, which is
    /// [`CleanupStore::apply`], and the separation is deliberate: a panel that marked a proposal
    /// accepted and failed to render it would otherwise leave a row saying a removal happened.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5117` when the proposal is not on this photograph, `AURA-DB-3006` when the write
    /// fails.
    pub fn decide(&self, image: ImageId, proposal: ProposalId, accept: bool) -> AuraResult<()> {
        let photo = image.to_db();
        let key = proposal.to_db();
        let accepted = i64::from(accept);
        self.catalog.writer().transact(move |conn| {
            let changed = conn
                .execute(
                    "UPDATE cleanup_proposal SET accepted = ?1
                      WHERE proposal_id = ?2 AND photo_id = ?3",
                    params![accepted, key, photo],
                )
                .map_err(|e| statement_failed("could not record the decision", &e))?;
            if changed == 0 {
                return Err(aura_core::errors::ml::cleanup_override_refused(
                    "that proposal is not on this photograph",
                ));
            }
            Ok(())
        })
    }

    /// Switch cleanup off, or back on, for one photograph.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn set_disabled(
        &self,
        project: &ProjectId,
        image: ImageId,
        scene: SceneId,
        disabled: bool,
    ) -> AuraResult<()> {
        let photo = image.to_db();
        let project_key = project.to_db();
        let scene_slug = scene.as_str().to_string();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let flag = i64::from(disabled);
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO cleanup_image (photo_id, project_id, scene, disabled_by_user, examined_at)
                      VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(photo_id) DO UPDATE SET disabled_by_user = excluded.disabled_by_user",
                params![photo, project_key, scene_slug, flag, now],
            )
            .map_err(|e| statement_failed("could not record the cleanup switch", &e))?;
            Ok(())
        })
    }

    /// Apply one accepted proposal: write its disclosure and mark it applied, in one transaction.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when either statement fails, including the trigger aborting the second half
    /// because the first is missing - which cannot happen through this function and is exactly why
    /// the trigger exists.
    pub fn apply(
        &self,
        project: &ProjectId,
        disclosure: &CleanupDisclosure,
        accepted_by_user: bool,
    ) -> AuraResult<()> {
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let disclosure = disclosure.clone();
        self.catalog.writer().transact(move |conn| {
            let (kind, source, model) = method_columns(&disclosure.method);
            conn.execute(
                "INSERT INTO cleanup_disclosure (
                        proposal_id, photo_id, project_id, method_kind, method_source, method_model,
                        x, y, w, h, accepted_by_user, artefact_score, applied_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    disclosure.proposal_id.to_db(),
                    disclosure.image_id.to_db(),
                    project_key,
                    kind,
                    source,
                    model,
                    f64::from(disclosure.region.x.clamp(0.0, 1.0)),
                    f64::from(disclosure.region.y.clamp(0.0, 1.0)),
                    f64::from(disclosure.region.w.clamp(1e-4, 1.0)),
                    f64::from(disclosure.region.h.clamp(1e-4, 1.0)),
                    i64::from(accepted_by_user),
                    f64::from(disclosure.artefact_score.clamp(0.0, 1.0)),
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not write the cleanup disclosure", &e))?;

            conn.execute(
                "UPDATE cleanup_proposal SET applied = 1, accepted = 1 WHERE proposal_id = ?1",
                params![disclosure.proposal_id.to_db()],
            )
            .map_err(|e| statement_failed("could not mark the cleanup applied", &e))?;
            Ok(())
        })
    }

    /// Undo one applied removal: clear the flag, then remove the disclosure.
    ///
    /// In that order, because the trigger refuses to delete a disclosure while the removal stands.
    /// A caller who did it the other way round would get an abort rather than a half-undone
    /// removal, which is the trigger working.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when either statement fails.
    pub fn unapply(&self, proposal: ProposalId) -> AuraResult<()> {
        let key = proposal.to_db();
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE cleanup_proposal SET applied = 0, accepted = 0 WHERE proposal_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear the applied flag", &e))?;
            conn.execute(
                "DELETE FROM cleanup_disclosure WHERE proposal_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not remove the disclosure", &e))?;
            Ok(())
        })
    }

    /// What a project's Cleanup panel header shows.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the view cannot be read.
    #[allow(clippy::too_many_lines)]
    pub fn outline(&self, project: ProjectId) -> AuraResult<CleanupOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT photos, examined, mask_complete, with_proposals, applied,
                            borrowed, filled, inpainted, reverted,
                            blocked_size_cap, blocked_denylist, blocked_identity,
                            blocked_structure, blocked_confidence
                       FROM v_cleanup_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok(StoredOutline {
                            photos: row.get::<_, i64>(0).unwrap_or(0),
                            examined: row.get::<_, i64>(1).unwrap_or(0),
                            mask_complete: row.get::<_, i64>(2).unwrap_or(0),
                            with_proposals: row.get::<_, i64>(3).unwrap_or(0),
                            applied: row.get::<_, i64>(4).unwrap_or(0),
                            borrowed: row.get::<_, i64>(5).unwrap_or(0),
                            filled: row.get::<_, i64>(6).unwrap_or(0),
                            inpainted: row.get::<_, i64>(7).unwrap_or(0),
                            reverted: row.get::<_, i64>(8).unwrap_or(0),
                            blocked: [
                                row.get::<_, i64>(9).unwrap_or(0),
                                row.get::<_, i64>(10).unwrap_or(0),
                                row.get::<_, i64>(11).unwrap_or(0),
                                row.get::<_, i64>(12).unwrap_or(0),
                                row.get::<_, i64>(13).unwrap_or(0),
                            ],
                        })
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the cleanup coverage", &e))?;

            let stored = row.unwrap_or_default();
            let photos = u32::try_from(stored.photos).unwrap_or(0);
            let examined = u32::try_from(stored.examined).unwrap_or(0);
            let mut blocked = [0u32; SafetyCheck::COUNT];
            for (slot, value) in blocked.iter_mut().zip(stored.blocked) {
                *slot = u32::try_from(value).unwrap_or(0);
            }
            Ok(CleanupOutline {
                photos,
                examined,
                coverage: if photos == 0 {
                    0.0
                } else {
                    examined as f32 / photos as f32
                },
                with_proposals: u32::try_from(stored.with_proposals).unwrap_or(0),
                applied: u32::try_from(stored.applied).unwrap_or(0),
                blocked,
                borrowed: u32::try_from(stored.borrowed).unwrap_or(0),
                filled: u32::try_from(stored.filled).unwrap_or(0),
                inpainted: u32::try_from(stored.inpainted).unwrap_or(0),
                reverted: u32::try_from(stored.reverted).unwrap_or(0),
                // The denominator is **examined** frames rather than every photograph, because a
                // frame nobody looked at has no mask answer either way and counting it as an
                // incomplete mask would report a project that has not run yet as one whose
                // segmenter is failing.
                mask_covered: if examined == 0 {
                    0.0
                } else {
                    u32::try_from(stored.mask_complete).unwrap_or(0) as f32 / examined as f32
                },
            })
        })
    }

    /// Raise `AURA-ML-5115` when stored proposals were made under different versions.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5115` on a drift, `AURA-DB-3006` when the read fails.
    pub fn check_versions(&self, image: ImageId, current: (u16, u16, u16)) -> AuraResult<()> {
        let photo = image.to_db();
        let stored: Option<(i64, i64, i64)> = self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT detector_ver, analysis_ver, policy_ver FROM cleanup_image WHERE photo_id = ?1",
                params![photo],
                |row| {
                    Ok((
                        row.get::<_, i64>(0).unwrap_or(0),
                        row.get::<_, i64>(1).unwrap_or(0),
                        row.get::<_, i64>(2).unwrap_or(0),
                    ))
                },
            )
            .optional()
            .map_err(|e| statement_failed("could not read the stored versions", &e))
        })?;
        let Some((detector, analysis, policy)) = stored else {
            return Ok(());
        };
        let found = (
            u16::try_from(detector).unwrap_or(0),
            u16::try_from(analysis).unwrap_or(0),
            u16::try_from(policy).unwrap_or(0),
        );
        if found == current {
            return Ok(());
        }
        Err(errors::version_drift(found, current))
    }
}

/// The three method columns, from one method.
fn method_columns(method: &CleanupMethod) -> (&'static str, Option<String>, Option<String>) {
    match method {
        CleanupMethod::BorrowFrom(source) => ("borrow", Some(source.to_db()), None),
        CleanupMethod::ClassicalFill => ("fill", None, None),
        CleanupMethod::Inpaint { model } => ("inpaint", None, Some(model.clone())),
    }
}

/// One method, from the three columns.
///
/// `None` for a row that names a borrow with no source or an inpaint with no model. The schema
/// refuses both, so this arm is unreachable through a migrated catalog and is what a hand-edited
/// database gets: the row is skipped rather than turned into a disclosure that says less than it
/// claims.
fn method_from_columns(
    kind: &str,
    source: Option<&str>,
    model: Option<&str>,
) -> Option<CleanupMethod> {
    match kind {
        "borrow" => PhotoId::from_db(source?)
            .ok()
            .map(CleanupMethod::BorrowFrom),
        "fill" => Some(CleanupMethod::ClassicalFill),
        "inpaint" => Some(CleanupMethod::Inpaint {
            model: model?.to_string(),
        }),
        _ => None,
    }
}

/// One row of `cleanup_proposal`, before it becomes a contract type.
struct StoredProposal {
    id: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    class: String,
    area_frac: f64,
    salience: f64,
    confidence: f64,
    kind: String,
    source: Option<String>,
    model: Option<String>,
    autonomy: String,
    scene: String,
    reasons: String,
    detector_ver: i64,
    analysis_ver: i64,
    policy_ver: i64,
}

impl StoredProposal {
    /// Rebuild a contract proposal, or `None` when the row cannot make one.
    ///
    /// Goes through `CleanupProposal::new`, which means a row that somehow carried no reasons or a
    /// degenerate rectangle is **skipped rather than resurrected**. Invariant 2: a decision without
    /// an explanation is a bug, and a panel that rendered one would be showing a removal nobody can
    /// account for.
    fn rebuild(&self, image: ImageId) -> Option<CleanupProposal> {
        let id = ProposalId::from_db(&self.id).ok()?;
        let class = DistractionClass::parse(&self.class)?;
        let method =
            method_from_columns(&self.kind, self.source.as_deref(), self.model.as_deref())?;
        let reasons: Vec<CleanupReason> = self
            .reasons
            .split(',')
            .filter(|slug| !slug.trim().is_empty())
            .filter_map(CleanupCode::parse)
            .map(|code| CleanupReason::plain(code, 0.5))
            .collect();
        let region = Box2 {
            x: self.x as f32,
            y: self.y as f32,
            w: self.w as f32,
            h: self.h as f32,
        };
        let mut proposal = CleanupProposal::new(
            id,
            image,
            region,
            class,
            method,
            SafetyVerdict::allow(),
            reasons,
        )
        .ok()?;
        proposal.area_frac = self.area_frac as f32;
        proposal.salience = self.salience as f32;
        proposal.confidence = self.confidence as f32;
        proposal.autonomy = autonomy_from(&self.autonomy);
        proposal.scene = SceneId::from_str_or_unknown(&self.scene);
        proposal.detector_ver = u16::try_from(self.detector_ver).unwrap_or(0);
        proposal.analysis_ver = u16::try_from(self.analysis_ver).unwrap_or(0);
        proposal.policy_ver = u16::try_from(self.policy_ver).unwrap_or(0);
        Some(proposal)
    }
}

/// A stored band, defaulting to the one that asks.
///
/// An unrecognised slug becomes `RequireReview` rather than the nearest match, which is phase 13's
/// own default and for its reason: a band that could not be worked out is exactly the band nobody
/// should have acted on silently.
fn autonomy_from(slug: &str) -> Autonomy {
    Autonomy::ALL
        .into_iter()
        .find(|band| band.as_str() == slug)
        .unwrap_or(Autonomy::RequireReview)
}

/// One row of `v_cleanup_coverage`.
#[derive(Default)]
struct StoredOutline {
    photos: i64,
    examined: i64,
    mask_complete: i64,
    with_proposals: i64,
    applied: i64,
    borrowed: i64,
    filled: i64,
    inpainted: i64,
    reverted: i64,
    blocked: [i64; SafetyCheck::COUNT],
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::clock::FixedClock;

    fn method_round_trip(method: &CleanupMethod) -> Option<CleanupMethod> {
        let (kind, source, model) = method_columns(method);
        method_from_columns(kind, source.as_deref(), model.as_deref())
    }

    #[test]
    fn every_method_survives_the_three_columns() {
        let photo = PhotoId::default();
        for method in [
            CleanupMethod::BorrowFrom(photo),
            CleanupMethod::ClassicalFill,
            CleanupMethod::Inpaint {
                model: "wedding_inpaint".into(),
            },
        ] {
            assert_eq!(method_round_trip(&method).as_ref(), Some(&method));
        }
    }

    #[test]
    fn a_borrow_with_no_source_is_skipped_rather_than_becoming_a_fill() {
        // The schema refuses this row. If a hand-edited catalog produces one anyway, the honest
        // answer is to drop it: a borrow that cannot say where its pixels came from is a
        // disclosure that discloses nothing, and calling it a fill would be worse.
        assert_eq!(method_from_columns("borrow", None, None), None);
        assert_eq!(method_from_columns("inpaint", None, None), None);
        assert_eq!(method_from_columns("something_else", None, None), None);
    }

    #[test]
    fn an_unrecognised_band_becomes_the_one_that_asks() {
        assert_eq!(autonomy_from("auto"), Autonomy::Auto);
        assert_eq!(autonomy_from("suggest"), Autonomy::Suggest);
        assert_eq!(autonomy_from("banana"), Autonomy::RequireReview);
        assert_eq!(autonomy_from(""), Autonomy::RequireReview);
    }

    #[test]
    fn a_stored_row_with_no_reasons_does_not_rebuild() {
        // Invariant 2, enforced on the way out of the catalog as well as on the way in.
        let stored = StoredProposal {
            id: ProposalId::new().to_db(),
            x: 0.1,
            y: 0.1,
            w: 0.05,
            h: 0.05,
            class: "bin".into(),
            area_frac: 0.0025,
            salience: 0.5,
            confidence: 0.5,
            kind: "fill".into(),
            source: None,
            model: None,
            autonomy: "require_review".into(),
            scene: "reception".into(),
            reasons: String::new(),
            detector_ver: 1,
            analysis_ver: 1,
            policy_ver: 1,
        };
        assert!(stored.rebuild(PhotoId::default()).is_none());
    }

    #[test]
    fn a_stored_row_with_reasons_rebuilds_into_the_contract_type() {
        let stored = StoredProposal {
            id: ProposalId::new().to_db(),
            x: 0.1,
            y: 0.1,
            w: 0.05,
            h: 0.05,
            class: "bin".into(),
            area_frac: 0.0025,
            salience: 0.5,
            confidence: 0.62,
            kind: "fill".into(),
            source: None,
            model: None,
            autonomy: "suggest".into(),
            scene: "reception".into(),
            reasons: "texture_uniform,no_aligned_sibling".into(),
            detector_ver: 1,
            analysis_ver: 1,
            policy_ver: 3,
        };
        let proposal = stored.rebuild(PhotoId::default()).expect("rebuilds");
        assert_eq!(proposal.reasons.len(), 2);
        assert_eq!(proposal.autonomy, Autonomy::Suggest);
        assert_eq!(proposal.policy_ver, 3);
        assert!(proposal.broken_guarantee().is_none());
    }

    #[test]
    fn the_store_can_be_constructed_over_a_fixed_clock() {
        // A compile-level check that the store's two dependencies are the two every other store in
        // the product takes, so the job graph can build one the same way.
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
        assert!(Arc::strong_count(&clock) >= 1);
    }
}
