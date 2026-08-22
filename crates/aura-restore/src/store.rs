//! The two tables migration 22 adds, and the three rules that live in the SQL rather than around
//! it.
//!
//! ## A delivered face never moved further than the ceiling
//!
//! `restore_face` refuses a row with `skipped = 0` and `identity_drift` above
//! [`MAX_IDENTITY_DRIFT`], and a trigger aborts the UPDATE that would un-skip one. Two layers,
//! because this is the guarantee of the phase and a promise enforced in one layer lasts until
//! somebody writes a second caller.
//!
//! ## `user_edited` is re-applied inside the statement
//!
//! A re-analysis overwrites every plan it recomputes, and that upsert carries a photographer's
//! `reviewed` and `user_edited` forward from the row it replaces, in the same statement. Eleventh
//! time the rule has been written into a store, and the window it closes is the same one every
//! time: a background pass that read the flag a moment earlier and wrote a moment later.
//!
//! ## A refusal is a row rather than an absence
//!
//! Every face this phase considered gets a `restore_face` row, whether it was recovered or
//! skipped, and a skipped one keeps its measured distance and its reason code. Phase 17
//! established that a rejection is written rather than dropped when the failure *is* the
//! evidence; here the refusal is the product working, and a schema that recorded only successes
//! would make "AURA declined to change what somebody looks like" unprovable.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::composition::Box2;
use aura_core::contract::restore::{
    ArtefactReport, DenoiseSpec, DenoiseTier, ImageId, RecoveredFace, RestoreCode, RestoreOutline,
    RestoreOverride, RestorePlan, RestoreReason, RestoreRegion, RestoreWhen, RunWhere, SharpenMask,
    SharpenSpec, MAX_IDENTITY_DRIFT, MAX_RECOVERED_FACES, REVIEW_BELOW,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult, IdentityId, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use crate::errors;

/// Bytes one photograph costs in `restore_plan`, `restore_face` and their indexes.
///
/// Section 11 sets no storage budget for this phase - all five of its rows are time. One is
/// measured anyway, and it is back under a kilobyte after phase 21's was not.
///
/// The measured decomposition over 1,000 photographs of the widest frame the fixtures produce - a
/// denoised, sharpened frame with two faces considered and eight reason codes - taken as the
/// change in `dbstat` payload rather than in `PRAGMA page_count`, which quantises to 4 KiB:
///
/// ```text
///   restore_plan and its five indexes     540 B/image
///   restore_face and its primary key      190 B/image   two faces, ~95 B each
///                                       -------
///   measured total                        730 B/image
/// ```
///
/// It is smaller than phase 21's 1,633 B for a structural reason rather than a lucky one:
/// `restore_plan` is **one fixed-width verdict** per photograph, as phases 09 to 20 all were, and
/// the only list is one row per face rather than one row per thing that was wrong with the frame.
/// A wedding is a few faces per photograph and can be dozens of blemishes.
///
/// The constant below is 1,000 rather than the measured 730, and the headroom is phase 19's
/// correction: a budget must not be pinned at its own measurement.
pub const BYTES_PER_IMAGE: usize = 1_000;

/// A length as SQLite's integer, saturating rather than wrapping.
fn count_of(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// The `restore_plan` and `restore_face` tables.
#[derive(Debug)]
pub struct RestoreStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl RestoreStore {
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

    /// Photographs with no plan at these versions.
    ///
    /// **The work remaining is a query, not a journal.** Invariant 5: kill the process at 10 %,
    /// 50 % or 90 % and the next run asks the catalog what is left. A `profile_ver` bump therefore
    /// heals itself - the rows made under the old tables are pending by definition.
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
                       LEFT JOIN restore_plan r ON r.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (r.photo_id IS NULL
                             OR r.model_ver    <> ?2
                             OR r.analysis_ver <> ?3
                             OR r.profile_ver  <> ?4)
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

    /// Every photograph in a project that has a plan, in id order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn planned(&self, project: &ProjectId) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM restore_plan WHERE project_id = ?1 ORDER BY photo_id",
                )
                .map_err(|e| statement_failed("could not list the planned frames", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not list the planned frames", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a planned frame", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Store one plan, carrying forward whatever the photographer settled.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails, and `AURA-ML-5103` when the plan breaks a guarantee -
    /// checked here as well as in the solver, because the store is the last place a bad row can be
    /// stopped and a caller nobody has written yet will reach this function.
    #[allow(clippy::too_many_lines)]
    pub fn put(&self, project: &ProjectId, plan: &RestorePlan) -> AuraResult<()> {
        if let Some(problem) = plan.broken_guarantee() {
            return Err(errors::restore_failed(&plan.image_id.to_db(), problem));
        }
        let photo = plan.image_id.to_db();
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let plan = plan.clone();

        self.catalog.writer().transact(move |conn| {
            // Both flags carried forward inside the statement rather than read before it. See the
            // module header.
            let carried: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT user_edited, reviewed FROM restore_plan WHERE photo_id = ?1",
                    params![photo],
                    |row| Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0))),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the previous plan", &e))?;
            let (user_edited, reviewed) = carried.unwrap_or((0, 0));

            let spec = plan.denoise_spec.clone().unwrap_or(DenoiseSpec {
                luminance: 0.0,
                colour: 0.0,
                detail: 0.0,
                sigma: 0.0,
                camera: String::new(),
                measured_model: false,
            });
            let report = plan.selfcheck.unwrap_or(ArtefactReport::UNTOUCHED);
            let reasons = plan
                .reasons
                .iter()
                .map(|reason| reason.code.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let sharpen = plan.sharpen.clone();

            conn.execute(
                "INSERT INTO restore_plan (
                        photo_id, project_id, scene,
                        denoise_tier, denoise_luminance, denoise_colour, denoise_detail,
                        denoise_sigma, denoise_camera, denoise_measured,
                        sharpen_kernel, sharpen_amount, sharpen_skin_atten, sharpen_coverage,
                        sharpen_iterations, face_recovery,
                        texture_retention, ringing, measured_on, resolves,
                        denoise_reduced, sharpen_reduced,
                        run_where, run_when, region_covered, confidence, reasons,
                        user_edited, reviewed, model_ver, analysis_ver, profile_ver, planned_at
                     ) VALUES (
                        ?1, ?2, ?3,
                        ?4, ?5, ?6, ?7,
                        ?8, ?9, ?10,
                        ?11, ?12, ?13, ?14,
                        ?15, ?16,
                        ?17, ?18, ?19, ?20,
                        ?21, ?22,
                        ?23, ?24, ?25, ?26, ?27,
                        ?28, ?29, ?30, ?31, ?32, ?33
                     )
                     ON CONFLICT(photo_id) DO UPDATE SET
                        project_id         = excluded.project_id,
                        scene              = excluded.scene,
                        denoise_tier       = excluded.denoise_tier,
                        denoise_luminance  = excluded.denoise_luminance,
                        denoise_colour     = excluded.denoise_colour,
                        denoise_detail     = excluded.denoise_detail,
                        denoise_sigma      = excluded.denoise_sigma,
                        denoise_camera     = excluded.denoise_camera,
                        denoise_measured   = excluded.denoise_measured,
                        sharpen_kernel     = excluded.sharpen_kernel,
                        sharpen_amount     = excluded.sharpen_amount,
                        sharpen_skin_atten = excluded.sharpen_skin_atten,
                        sharpen_coverage   = excluded.sharpen_coverage,
                        sharpen_iterations = excluded.sharpen_iterations,
                        face_recovery      = excluded.face_recovery,
                        texture_retention  = excluded.texture_retention,
                        ringing            = excluded.ringing,
                        measured_on        = excluded.measured_on,
                        resolves           = excluded.resolves,
                        denoise_reduced    = excluded.denoise_reduced,
                        sharpen_reduced    = excluded.sharpen_reduced,
                        run_where          = excluded.run_where,
                        run_when           = excluded.run_when,
                        region_covered     = excluded.region_covered,
                        confidence         = excluded.confidence,
                        reasons            = excluded.reasons,
                        model_ver          = excluded.model_ver,
                        analysis_ver       = excluded.analysis_ver,
                        profile_ver        = excluded.profile_ver,
                        planned_at         = excluded.planned_at",
                params![
                    photo,
                    project_key,
                    plan.scene.as_str(),
                    plan.denoise.as_str(),
                    f64::from(spec.luminance),
                    f64::from(spec.colour),
                    f64::from(spec.detail),
                    f64::from(spec.sigma),
                    spec.camera,
                    i64::from(spec.measured_model),
                    sharpen.as_ref().map(|s| f64::from(s.kernel_sigma)),
                    sharpen.as_ref().map_or(0.0, |s| f64::from(s.amount)),
                    sharpen
                        .as_ref()
                        .map_or(0.0, |s| f64::from(s.skin_attenuation)),
                    sharpen.as_ref().map_or(0.0, |s| f64::from(s.mask.coverage)),
                    sharpen.as_ref().map_or(0, |s| i64::from(s.iterations)),
                    f64::from(plan.face_recovery.unwrap_or(0.0)),
                    f64::from(report.texture_retention),
                    f64::from(report.ringing),
                    i64::from(report.measured_on),
                    i64::from(report.resolves),
                    i64::from(report.denoise_reduced),
                    i64::from(report.sharpen_reduced),
                    plan.run_where.as_str(),
                    plan.when.as_str(),
                    i64::from(plan.region_covered),
                    f64::from(plan.confidence),
                    reasons,
                    user_edited,
                    reviewed,
                    i64::from(plan.model_ver),
                    i64::from(plan.analysis_ver),
                    i64::from(plan.profile_ver),
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not write the restoration plan", &e))?;

            conn.execute(
                "DELETE FROM restore_face WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| statement_failed("could not clear the previous faces", &e))?;

            for (seq, face) in plan.recovered.iter().take(MAX_RECOVERED_FACES).enumerate() {
                conn.execute(
                    "INSERT INTO restore_face (
                            photo_id, seq, identity_id, x, y, w, h,
                            sharpness, strength, identity_drift, resolves,
                            skipped, skipped_because
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        photo,
                        count_of(seq),
                        face.identity.map(|id| id.to_db()),
                        f64::from(face.bounds.x.clamp(0.0, 1.0)),
                        f64::from(face.bounds.y.clamp(0.0, 1.0)),
                        f64::from(face.bounds.w.clamp(1e-4, 1.0)),
                        f64::from(face.bounds.h.clamp(1e-4, 1.0)),
                        f64::from(face.sharpness),
                        f64::from(face.strength),
                        f64::from(face.identity_drift),
                        i64::from(face.resolves),
                        i64::from(face.skipped),
                        face.skipped_because.map(RestoreCode::as_str),
                    ],
                )
                .map_err(|e| statement_failed("could not write a considered face", &e))?;
            }
            Ok(())
        })
    }

    /// One photograph's plan.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    #[allow(clippy::too_many_lines)]
    pub fn get(&self, image: ImageId) -> AuraResult<Option<RestorePlan>> {
        let photo = image.to_db();
        let faces = self.faces_of(image)?;
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT scene,
                            denoise_tier, denoise_luminance, denoise_colour, denoise_detail,
                            denoise_sigma, denoise_camera, denoise_measured,
                            sharpen_kernel, sharpen_amount, sharpen_skin_atten, sharpen_coverage,
                            sharpen_iterations, face_recovery,
                            texture_retention, ringing, measured_on, resolves,
                            denoise_reduced, sharpen_reduced,
                            run_where, run_when, region_covered, confidence, reasons,
                            user_edited, reviewed, model_ver, analysis_ver, profile_ver
                       FROM restore_plan WHERE photo_id = ?1",
                    params![photo],
                    |row| {
                        Ok(StoredPlan {
                            scene: row.get::<_, String>(0).unwrap_or_default(),
                            tier: row.get::<_, String>(1).unwrap_or_default(),
                            luminance: row.get::<_, f64>(2).unwrap_or(0.0),
                            colour: row.get::<_, f64>(3).unwrap_or(0.0),
                            detail: row.get::<_, f64>(4).unwrap_or(0.0),
                            sigma: row.get::<_, f64>(5).unwrap_or(0.0),
                            camera: row.get::<_, String>(6).unwrap_or_default(),
                            measured: row.get::<_, i64>(7).unwrap_or(0),
                            kernel: row.get::<_, Option<f64>>(8).unwrap_or(None),
                            amount: row.get::<_, f64>(9).unwrap_or(0.0),
                            skin_atten: row.get::<_, f64>(10).unwrap_or(0.0),
                            coverage: row.get::<_, f64>(11).unwrap_or(0.0),
                            iterations: row.get::<_, i64>(12).unwrap_or(0),
                            face_recovery: row.get::<_, f64>(13).unwrap_or(0.0),
                            retention: row.get::<_, f64>(14).unwrap_or(1.0),
                            ringing: row.get::<_, f64>(15).unwrap_or(0.0),
                            measured_on: row.get::<_, i64>(16).unwrap_or(0),
                            resolves: row.get::<_, i64>(17).unwrap_or(0),
                            denoise_reduced: row.get::<_, i64>(18).unwrap_or(0),
                            sharpen_reduced: row.get::<_, i64>(19).unwrap_or(0),
                            run_where: row.get::<_, String>(20).unwrap_or_default(),
                            run_when: row.get::<_, String>(21).unwrap_or_default(),
                            region_covered: row.get::<_, i64>(22).unwrap_or(0),
                            confidence: row.get::<_, f64>(23).unwrap_or(0.0),
                            reasons: row.get::<_, String>(24).unwrap_or_default(),
                            user_edited: row.get::<_, i64>(25).unwrap_or(0),
                            reviewed: row.get::<_, i64>(26).unwrap_or(0),
                            model_ver: row.get::<_, i64>(27).unwrap_or(0),
                            analysis_ver: row.get::<_, i64>(28).unwrap_or(0),
                            profile_ver: row.get::<_, i64>(29).unwrap_or(0),
                        })
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the restoration plan", &e))?;

            let Some(stored) = row else {
                return Ok(None);
            };
            Ok(Some(stored.into_plan(image, faces.clone())))
        })
    }

    /// Every face considered for one photograph, in stored order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn faces_of(&self, image: ImageId) -> AuraResult<Vec<RecoveredFace>> {
        let photo = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT identity_id, x, y, w, h, sharpness, strength, identity_drift,
                            resolves, skipped, skipped_because
                       FROM restore_face WHERE photo_id = ?1 ORDER BY seq",
                )
                .map_err(|e| statement_failed("could not read the considered faces", &e))?;
            let mut cursor = statement
                .query(params![photo])
                .map_err(|e| statement_failed("could not read the considered faces", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a considered face", &e))?
            {
                let identity: Option<String> = row.get(0).unwrap_or(None);
                let skipped: i64 = row.get(9).unwrap_or(0);
                let because: Option<String> = row.get(10).unwrap_or(None);
                out.push(RecoveredFace {
                    identity: identity.and_then(|text| IdentityId::from_db(&text).ok()),
                    bounds: Box2 {
                        x: row.get::<_, f64>(1).unwrap_or(0.0) as f32,
                        y: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                        w: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                        h: row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                    },
                    sharpness: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                    strength: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                    identity_drift: row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                    resolves: row.get::<_, i64>(8).unwrap_or(0) as u8,
                    skipped: skipped != 0,
                    skipped_because: because.as_deref().and_then(RestoreCode::parse),
                });
            }
            Ok(out)
        })
    }

    /// What a project's pass covered and did.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    #[allow(clippy::too_many_lines)]
    pub fn outline(
        &self,
        project: &ProjectId,
        versions: (u16, u16, u16),
    ) -> AuraResult<RestoreOutline> {
        let key = project.to_db();
        let refusals = self.identity_summary(project)?;
        let unmeasured = self.unmeasured_cameras(project)?;
        self.catalog.read(move |conn| {
            let mut outline = conn
                .query_row(
                    "SELECT photos, planned, acted_on, region_covered,
                            tier_off, tier_light, tier_standard, tier_strong,
                            sharpened, reduced, unmeasured_camera,
                            COALESCE(mean_texture_retention, 1.0),
                            COALESCE(mean_ringing, 0.0)
                       FROM v_restore_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok(RestoreOutline {
                            photos: row.get::<_, i64>(0).unwrap_or(0) as u32,
                            planned: row.get::<_, i64>(1).unwrap_or(0) as u32,
                            acted_on: row.get::<_, i64>(2).unwrap_or(0) as u32,
                            region_covered: row.get::<_, i64>(3).unwrap_or(0) as u32,
                            tier_histogram: [
                                row.get::<_, i64>(4).unwrap_or(0) as u32,
                                row.get::<_, i64>(5).unwrap_or(0) as u32,
                                row.get::<_, i64>(6).unwrap_or(0) as u32,
                                row.get::<_, i64>(7).unwrap_or(0) as u32,
                            ],
                            sharpened: row.get::<_, i64>(8).unwrap_or(0) as u32,
                            reduced: row.get::<_, i64>(9).unwrap_or(0) as u32,
                            mean_texture_retention: row.get::<_, f64>(11).unwrap_or(1.0) as f32,
                            mean_ringing: row.get::<_, f64>(12).unwrap_or(0.0) as f32,
                            ..RestoreOutline::default()
                        })
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the restoration coverage", &e))?
                .unwrap_or_default();

            outline.coverage = if outline.photos == 0 {
                0.0
            } else {
                f64::from(outline.planned) as f32 / f64::from(outline.photos) as f32
            };

            outline.needs_review = conn
                .query_row(
                    "SELECT COUNT(*) FROM restore_plan
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                        AND confidence < ?2",
                    params![key, f64::from(REVIEW_BELOW)],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u32;
            outline.user_edited = conn
                .query_row(
                    "SELECT COUNT(*) FROM restore_plan WHERE project_id = ?1 AND user_edited = 1",
                    params![key],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u32;

            for (index, destination) in RunWhere::ALL.iter().enumerate() {
                let count = conn
                    .query_row(
                        "SELECT COUNT(*) FROM restore_plan WHERE project_id = ?1 AND run_where = ?2",
                        params![key, destination.as_str()],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap_or(0) as u32;
                if let Some(slot) = outline.run_where_histogram.get_mut(index) {
                    *slot = count;
                }
            }

            // The sharpening refusals, as a histogram rather than a count. "AURA sharpened
            // nothing in this wedding" has six causes and five of them are somebody else's bug.
            let mut statement = conn
                .prepare("SELECT reasons FROM restore_plan WHERE project_id = ?1")
                .map_err(|e| statement_failed("could not read the reasons", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the reasons", &e))?;
            let mut histogram: Vec<(RestoreCode, u32)> = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a reason list", &e))?
            {
                let blob: String = row.get(0).unwrap_or_default();
                for slug in blob.split(',').filter(|s| !s.is_empty()) {
                    let Some(code) = RestoreCode::parse(slug) else {
                        continue;
                    };
                    if code.subject() != aura_core::contract::restore::RestoreSubject::Sharpen
                        || !code.is_restraint()
                    {
                        continue;
                    }
                    match histogram.iter_mut().find(|(existing, _)| *existing == code) {
                        Some((_, count)) => *count += 1,
                        None => histogram.push((code, 1)),
                    }
                }
            }
            histogram.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.as_str().cmp(b.0.as_str())));
            outline.sharpen_refusals = histogram;

            outline.faces_recovered = refusals.0;
            outline.faces_skipped_identity = refusals.1;
            outline.worst_identity_drift = refusals.2;
            outline.unmeasured_cameras.clone_from(&unmeasured);
            outline.model_ver = versions.0;
            outline.analysis_ver = versions.1;
            outline.profile_ver = versions.2;
            Ok(outline)
        })
    }

    /// Faces recovered, faces refused for identity, and the worst kept drift.
    ///
    /// **The guarantee's own query**, read from `v_restore_identity` so that the panel, the
    /// delivery report, phase 27 and the gate all get the same number. Four callers deriving it
    /// separately is a number they will eventually disagree about.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn identity_summary(&self, project: &ProjectId) -> AuraResult<(u32, u32, f32)> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT recovered, refused_for_identity, worst_kept_drift
                       FROM v_restore_identity WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0).unwrap_or(0) as u32,
                            row.get::<_, i64>(1).unwrap_or(0) as u32,
                            row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the identity summary", &e))?;
            Ok(row.unwrap_or((0, 0, 0.0)))
        })
    }

    /// Frames where face recovery was refused to keep somebody looking like themselves.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn identity_refusals(&self, project: &ProjectId, limit: usize) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        let cap = count_of(limit.max(1));
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT f.photo_id
                       FROM restore_face f
                       JOIN restore_plan p ON p.photo_id = f.photo_id
                      WHERE p.project_id = ?1
                        AND f.skipped_because = 'restore_identity_drift_skipped'
                      GROUP BY f.photo_id
                      ORDER BY MAX(f.identity_drift) DESC, f.photo_id
                      LIMIT ?2",
                )
                .map_err(|e| statement_failed("could not read the identity refusals", &e))?;
            let mut cursor = statement
                .query(params![key, cap])
                .map_err(|e| statement_failed("could not read the identity refusals", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a refusal", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// The camera bodies in this project that were denoised against a synthetic model.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn unmeasured_cameras(&self, project: &ProjectId) -> AuraResult<Vec<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT DISTINCT denoise_camera FROM restore_plan
                      WHERE project_id = ?1 AND denoise_measured = 0 AND denoise_tier <> 'off'
                      ORDER BY denoise_camera",
                )
                .map_err(|e| statement_failed("could not read the camera list", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the camera list", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a camera", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if !text.is_empty() {
                    out.push(text);
                }
            }
            Ok(out)
        })
    }

    /// The frames worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn needs_review(&self, project: &ProjectId, limit: usize) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        let cap = count_of(limit.max(1));
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM restore_plan
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                        AND confidence < ?2
                      ORDER BY confidence ASC, photo_id
                      LIMIT ?3",
                )
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut cursor = statement
                .query(params![key, f64::from(REVIEW_BELOW), cap])
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a review row", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Record that a photographer has looked at one plan and agrees.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5104` when the photograph has no plan.
    pub fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        let photo = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE restore_plan SET reviewed = 1 WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| statement_failed("could not record the acceptance", &e))
        })?;
        if changed == 0 {
            return Err(errors::restore_edit_refused(
                "that photograph has no restoration plan yet",
            ));
        }
        Ok(())
    }

    /// Record what a photographer chose for one photograph.
    ///
    /// Sets `user_edited`, which a re-analysis carries forward rather than overwriting.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5104` when the override sets nothing or the photograph has no plan.
    pub fn set_override(&self, image: ImageId, values: &RestoreOverride) -> Result<(), AuraError> {
        if let Some(problem) = values.problem() {
            return Err(errors::restore_edit_refused(problem));
        }
        let photo = image.to_db();
        let tier = values.denoise.map(|tier| tier.as_str().to_string());
        let sharpen = values.sharpen;
        let face = values.face_recovery;
        let changed = self.catalog.writer().transact(move |conn| {
            let mut total = 0usize;
            if let Some(tier) = tier {
                // A tier the photographer chose is also a set of amounts they did not: the row
                // keeps AURA's own numbers so the review queue can show the disagreement and
                // phase 30's learning loop can read one - phase 15's rule. The renderer reads
                // the recipe, which the caller rewrites from this row.
                total += conn
                    .execute(
                        "UPDATE restore_plan
                            SET denoise_tier = ?2, user_edited = 1
                          WHERE photo_id = ?1",
                        params![photo, tier],
                    )
                    .map_err(|e| statement_failed("could not record the tier", &e))?;
            }
            if sharpen == Some(false) {
                total += conn
                    .execute(
                        "UPDATE restore_plan
                            SET sharpen_amount = 0.0, sharpen_kernel = NULL,
                                sharpen_iterations = 0, sharpen_coverage = 0.0,
                                ringing = 0.0, user_edited = 1
                          WHERE photo_id = ?1",
                        params![photo],
                    )
                    .map_err(|e| statement_failed("could not switch sharpening off", &e))?;
            } else if sharpen == Some(true) {
                total += conn
                    .execute(
                        "UPDATE restore_plan SET user_edited = 1 WHERE photo_id = ?1",
                        params![photo],
                    )
                    .map_err(|e| statement_failed("could not record the choice", &e))?;
            }
            if face == Some(false) {
                total += conn
                    .execute(
                        "UPDATE restore_plan SET face_recovery = 0.0, user_edited = 1
                          WHERE photo_id = ?1",
                        params![photo],
                    )
                    .map_err(|e| statement_failed("could not switch face recovery off", &e))?;
                conn.execute(
                    "UPDATE restore_face
                        SET strength = 0.0, skipped = 1,
                            skipped_because = 'restore_recovery_head_untrained'
                      WHERE photo_id = ?1 AND skipped = 0",
                    params![photo],
                )
                .map_err(|e| statement_failed("could not clear the recovered faces", &e))?;
            } else if face == Some(true) {
                total += conn
                    .execute(
                        "UPDATE restore_plan SET user_edited = 1 WHERE photo_id = ?1",
                        params![photo],
                    )
                    .map_err(|e| statement_failed("could not record the choice", &e))?;
            }
            Ok(total)
        })?;
        if changed == 0 {
            return Err(errors::restore_edit_refused(
                "that photograph has no restoration plan yet",
            ));
        }
        Ok(())
    }

    /// Which versions the stored plans were made under, and how many there are.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn stored_versions(
        &self,
        project: &ProjectId,
    ) -> AuraResult<Option<((u16, u16, u16), usize)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT model_ver, analysis_ver, profile_ver, COUNT(*)
                       FROM restore_plan WHERE project_id = ?1
                      GROUP BY model_ver, analysis_ver, profile_ver
                      ORDER BY COUNT(*) DESC LIMIT 1",
                    params![key],
                    |row| {
                        Ok((
                            (
                                row.get::<_, i64>(0).unwrap_or(0) as u16,
                                row.get::<_, i64>(1).unwrap_or(0) as u16,
                                row.get::<_, i64>(2).unwrap_or(0) as u16,
                            ),
                            row.get::<_, i64>(3).unwrap_or(0) as usize,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the stored versions", &e))?;
            Ok(row)
        })
    }
}

/// One `restore_plan` row as SQLite hands it back.
///
/// A named struct rather than a thirty-element tuple: a `let` binding that wide is a shape nobody
/// can check against the `SELECT` above it, and this is the one place the two are compared.
#[derive(Debug, Clone)]
struct StoredPlan {
    scene: String,
    tier: String,
    luminance: f64,
    colour: f64,
    detail: f64,
    sigma: f64,
    camera: String,
    measured: i64,
    kernel: Option<f64>,
    amount: f64,
    skin_atten: f64,
    coverage: f64,
    iterations: i64,
    face_recovery: f64,
    retention: f64,
    ringing: f64,
    measured_on: i64,
    resolves: i64,
    denoise_reduced: i64,
    sharpen_reduced: i64,
    run_where: String,
    run_when: String,
    region_covered: i64,
    confidence: f64,
    reasons: String,
    user_edited: i64,
    reviewed: i64,
    model_ver: i64,
    analysis_ver: i64,
    profile_ver: i64,
}

impl StoredPlan {
    fn into_plan(self, image: ImageId, faces: Vec<RecoveredFace>) -> RestorePlan {
        let tier = DenoiseTier::parse(&self.tier).unwrap_or_default();
        let spec = if tier == DenoiseTier::Off {
            None
        } else {
            Some(DenoiseSpec {
                luminance: self.luminance as f32,
                colour: self.colour as f32,
                detail: self.detail as f32,
                sigma: self.sigma as f32,
                camera: self.camera,
                measured_model: self.measured != 0,
            })
        };
        let sharpen = (self.amount > 0.0)
            .then(|| {
                self.kernel.map(|kernel| {
                    let mut excluded = [false; RestoreRegion::COUNT];
                    for (index, region) in RestoreRegion::ALL.iter().enumerate() {
                        if region.excluded_from_sharpen() {
                            if let Some(slot) = excluded.get_mut(index) {
                                *slot = true;
                            }
                        }
                    }
                    SharpenSpec {
                        kernel_sigma: kernel as f32,
                        amount: self.amount as f32,
                        mask: SharpenMask {
                            excluded,
                            coverage: self.coverage as f32,
                            from_regions: true,
                        },
                        skin_attenuation: self.skin_atten as f32,
                        iterations: self.iterations.clamp(0, 255) as u8,
                    }
                })
            })
            .flatten();
        let report = ArtefactReport {
            texture_retention: self.retention as f32,
            ringing: self.ringing as f32,
            identity_drift: faces
                .iter()
                .filter(|face| !face.skipped)
                .map(|face| face.identity_drift)
                .fold(0.0_f32, f32::max),
            measured_on: self.measured_on.clamp(0, i64::from(u32::MAX)) as u32,
            resolves: self.resolves.clamp(0, 255) as u8,
            denoise_reduced: self.denoise_reduced != 0,
            sharpen_reduced: self.sharpen_reduced != 0,
            face_skipped: faces
                .iter()
                .any(|face| face.skipped_because == Some(RestoreCode::IdentityDriftSkipped)),
        };
        let acted = tier != DenoiseTier::Off || sharpen.is_some() || self.face_recovery > 0.0;
        RestorePlan {
            image_id: image,
            denoise: tier,
            denoise_spec: spec,
            sharpen,
            face_recovery: (self.face_recovery > 0.0).then_some(self.face_recovery as f32),
            recovered: faces,
            run_where: RunWhere::parse(&self.run_where).unwrap_or_default(),
            when: RestoreWhen::parse(&self.run_when).unwrap_or_default(),
            selfcheck: acted.then_some(report),
            reasons: self
                .reasons
                .split(',')
                .filter_map(RestoreCode::parse)
                .map(|code| RestoreReason::plain(code, 0.0))
                .collect(),
            confidence: self.confidence as f32,
            scene: SceneId::from_str_or_unknown(&self.scene),
            region_covered: self.region_covered != 0,
            user_edited: self.user_edited != 0,
            reviewed: self.reviewed != 0,
            model_ver: self.model_ver.clamp(0, i64::from(u16::MAX)) as u16,
            analysis_ver: self.analysis_ver.clamp(0, i64::from(u16::MAX)) as u16,
            profile_ver: self.profile_ver.clamp(0, i64::from(u16::MAX)) as u16,
        }
    }
}

/// The identity ceiling, restated for callers that want to assert against it without importing
/// the contract.
#[must_use]
pub const fn identity_ceiling() -> f32 {
    MAX_IDENTITY_DRIFT
}
