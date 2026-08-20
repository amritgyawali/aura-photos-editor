//! The four tables migration 19 adds, and the three rules that live in the SQL rather than
//! around it.
//!
//! ## `user_edited` is re-applied inside the statement, not read before it
//!
//! A re-analysis overwrites every plan it recomputes. That upsert carries the photographer's
//! own six strengths forward from the row it is replacing, in the same statement, so an
//! override cannot be lost by a background pass that read the flag a moment earlier and wrote
//! a moment later. This is the eighth time the rule has been written into a store.
//!
//! ## A gated operation is a row, not an absence
//!
//! `local_light_gate` records which operation was reduced or skipped and which mask kind
//! caused it. Storing only the operations that *ran* would make "phase 18 is not installed"
//! and "there was nothing to do here" the same query, and those are the two states this phase
//! most needs to be able to tell apart.
//!
//! ## The shaping is stored as moves, and the grid is re-derived
//!
//! Phase 13's rule - evidence can never be a pixel - applied to a decision. The `shaping`
//! document holds a handful of named zones per face; [`dodgeburn::grid`] regenerates the
//! 32x32 map from them at read time. That is what keeps [`BYTES_PER_IMAGE`] reachable, and it
//! is why `shaping_ver` exists: a change to the derivation moves delivered pixels without
//! moving one stored number.
//!
//! It is a **document** rather than a child table, which migration 15 argued for its own
//! three and which is sharper here: a shaping zone is read only by the panel that lists it
//! and by the renderer that regenerates the grid from it, and nothing selects "every frame
//! whose jaw was burned". A `WITHOUT ROWID` child table costs about 75 bytes a row once the
//! primary key is counted twice, and ten of those per face is most of a kilobyte for rows
//! nobody queries across.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::{
    BackgroundBalanceDelta, DodgeBurnMaps, FaceLightDelta, FaceShaping, ImageId, LocalCode,
    LocalLightPlan, LocalOp, LocalOutline, LocalOverride, LocalReason, MaskKind, ShineReduction,
    SubjectEnhanceDelta, REVIEW_BELOW, SHAPING_SIDE,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, IdentityId, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use crate::errors;
use crate::local::dodgeburn;

/// Bytes one photograph costs in `local_light_plan`, its two child tables and their indexes.
///
/// Section 11 names **no storage row for this phase** - its three budgets are all time. One is
/// measured anyway, against the 1 KB per image every phase since 09 has aimed at, because a
/// decision table that quietly costs four kilobytes a frame is a catalog nobody can back up
/// and section 11 not asking is not a reason not to know.
///
/// The figure below is the measurement rather than the aim. The measured decomposition on
/// 1,000 photographs, from `PRAGMA page_count`, over the widest rows the fixtures produce:
///
/// ```text
///   the plan row and its three indexes   503 B/image
///   the lit faces                        249 B/image   one row per face, ~180 B each
///   the reasons document                 143 B/image   up to eight codes with evidence
///   the shaping document                 114 B/image   four numbers per shaped face
///   the gates                              0 B/image   inside the pages above
///                                      -----
///   measured total                     1,064 B/image
/// ```
///
/// Two reductions were made and are why it is not far worse. It started at **2,236 B**:
///
/// * the shaping was a `WITHOUT ROWID` child table of one row per zone, and ten zones on each
///   of four faces cost 1,286 B/image on its own. The zones are a pure function of the face
///   region, the light direction and the strength, so the catalog stores those and
///   `dodgeburn::zones_for` reproduces the list - 114 B/image, and the panel still shows every
///   zone by name because they are regenerated on read;
/// * everything derivable is derived rather than stored, which is the same rule migration 15
///   wrote for its illuminants.
///
/// Two more were considered and rejected:
///
/// * **folding the lit faces into a document** saves about 200 B/image and removes
///   `idx_local_face_identity`, which is the index phase 20 joins on to avoid retouching a
///   face this phase has already lifted. A document cannot be joined, and the alternative is
///   phase 20 scanning every plan in a wedding.
/// * **storing reason codes as integers** saves about 60 B/image and creates a second reason
///   vocabulary that a catalog written by a newer build cannot be read with. Phase 09's rule
///   runs the other way, for the fifth migration running.
///
/// At the measured figure a 4,000-image wedding costs about 4.1 MB, against about 3.2 MB for
/// phase 15's tone estimates and 48 MB for phase 14's recipes. That is the trade, stated
/// rather than hidden behind a smaller constant.
///
/// The figure is asserted by `crates/aura-perf/tests/local_budgets.rs` against a real catalog
/// rather than against this constant.
pub const BYTES_PER_IMAGE: usize = 1_100;

/// The `local_light_plan`, `local_light_face` and `local_light_gate` tables.
#[derive(Debug)]
pub struct LocalStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl LocalStore {
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
    /// **The work remaining is a query, not a journal.** Invariant 5: kill the process at
    /// 10 %, 50 % or 90 % and the next run asks the catalog what is left. A `policy_ver` or
    /// `shaping_ver` bump therefore heals itself.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(
        &self,
        project: &ProjectId,
        versions: (u16, u16, u16, u16),
    ) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN local_light_plan l ON l.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (l.photo_id IS NULL
                             OR l.model_ver    <> ?2
                             OR l.analysis_ver <> ?3
                             OR l.policy_ver   <> ?4
                             OR l.shaping_ver  <> ?5)
                      ORDER BY p.photo_id",
                )
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![
                    key,
                    i64::from(versions.0),
                    i64::from(versions.1),
                    i64::from(versions.2),
                    i64::from(versions.3),
                ])
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
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
    /// What the IPC layer walks when it carries plans into recipes. Separate from
    /// [`LocalStore::pending`], which answers the opposite question and would return every
    /// photograph in the project if it were asked this one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn planned(&self, project: &ProjectId) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM local_light_plan
                      WHERE project_id = ?1 ORDER BY photo_id",
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

    /// Store one plan, carrying forward whatever the photographer set.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5086` when the plan breaks one of the phase's own guarantees, and
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)]
    pub fn put(&self, project: &ProjectId, plan: &LocalLightPlan) -> AuraResult<()> {
        crate::local::guard::check_plan(plan)?;
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let at_ts = epoch_ms(self.clock.now_utc());
        let project_key = project.to_db();
        let photo_key = plan.image_id.to_db();

        let reasons = encode_reasons(&plan.reasons);
        let shine_boxes = encode_boxes(
            plan.shine
                .as_ref()
                .map_or(&[] as &[CropRect], |s| s.regions.as_slice()),
        );
        let shine = plan.shine.clone().unwrap_or(ShineReduction {
            regions: Vec::new(),
            identities: Vec::new(),
            reduction_ev: 0.0,
            area_fraction: 0.0,
            peak_before: 0.0,
            peak_after: 0.0,
            mask_scale: 0.0,
        });
        let faces: Vec<(Option<IdentityId>, FaceLightDelta)> = plan.face_light.clone();
        let gates = plan.gated_by_mask_quality.clone();
        let row = Row {
            strengths: plan.strengths,
            budget_used: plan.total_budget_used,
            subject: plan.subject,
            background: plan.background,
            shine_regions: i64::try_from(shine.regions.len()).unwrap_or(0),
            shine_ev: shine.reduction_ev,
            shine_area: shine.area_fraction,
            shine_boxes,
            shaping: encode_shaping(plan.dodge_burn.as_ref()),
            face_spread: plan.inter_face_spread(),
            faces_lit: i64::try_from(plan.face_light.len()).unwrap_or(0),
            group_solved: plan
                .reasons
                .iter()
                .any(|r| r.code == LocalCode::GroupSolvedJointly),
            scene: plan.scene.as_str().to_string(),
            unpolicied: plan
                .reasons
                .iter()
                .any(|r| r.code == LocalCode::SceneStrengthLimited),
            confidence: plan.confidence,
            reasons,
            reason_count: i64::try_from(plan.reasons.len().clamp(1, 8)).unwrap_or(1),
            model_ver: i64::from(plan.model_ver),
            analysis_ver: i64::from(plan.analysis_ver),
            policy_ver: i64::from(plan.policy_ver),
            shaping_ver: i64::from(plan.shaping_ver),
        };

        self.catalog.writer().transact(move |conn| {
            // The photographer's own strengths and the two bits that protect them, read and
            // rewritten inside the same statement. See the module header.
            let existing: Option<(i64, i64, Option<String>)> = conn
                .query_row(
                    "SELECT user_edited, reviewed, user_strengths
                       FROM local_light_plan WHERE photo_id = ?1",
                    params![photo_key],
                    |stored| {
                        Ok((
                            stored.get::<_, i64>(0).unwrap_or(0),
                            stored.get::<_, i64>(1).unwrap_or(0),
                            stored.get::<_, Option<String>>(2).unwrap_or(None),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the previous plan", &e))?;
            let (user_edited, reviewed, user_strengths) = existing.unwrap_or((0, 0, None));

            conn.execute(
                "INSERT INTO local_light_plan (
                     photo_id, project_id,
                     s_face_light, s_subject, s_background, s_shine,
                     s_dodge_burn_low, s_dodge_burn_mid, budget_used,
                     subject_clarity, subject_texture, subject_contrast,
                     bg_exposure_ev, bg_highlights, bg_saturation, bg_feather,
                     competition_ratio, chroma_energy, bright_blobs,
                     mean_luma_before, mean_luma_after,
                     shine_regions, shine_ev, shine_area, shine_boxes, shaping,
                     face_spread, faces_lit, group_solved,
                     scene, unpolicied, confidence, reasons, reason_count,
                     user_edited, reviewed, user_strengths,
                     model_ver, analysis_ver, policy_ver, shaping_ver, at_ts, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28,
                         ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42,
                         ?43)
                 ON CONFLICT(photo_id) DO UPDATE SET
                     s_face_light      = excluded.s_face_light,
                     s_subject         = excluded.s_subject,
                     s_background      = excluded.s_background,
                     s_shine           = excluded.s_shine,
                     s_dodge_burn_low  = excluded.s_dodge_burn_low,
                     s_dodge_burn_mid  = excluded.s_dodge_burn_mid,
                     budget_used       = excluded.budget_used,
                     subject_clarity   = excluded.subject_clarity,
                     subject_texture   = excluded.subject_texture,
                     subject_contrast  = excluded.subject_contrast,
                     bg_exposure_ev    = excluded.bg_exposure_ev,
                     bg_highlights     = excluded.bg_highlights,
                     bg_saturation     = excluded.bg_saturation,
                     bg_feather        = excluded.bg_feather,
                     competition_ratio = excluded.competition_ratio,
                     chroma_energy     = excluded.chroma_energy,
                     bright_blobs      = excluded.bright_blobs,
                     mean_luma_before  = excluded.mean_luma_before,
                     mean_luma_after   = excluded.mean_luma_after,
                     shine_regions     = excluded.shine_regions,
                     shine_ev          = excluded.shine_ev,
                     shine_area        = excluded.shine_area,
                     shine_boxes       = excluded.shine_boxes,
                     shaping           = excluded.shaping,
                     face_spread       = excluded.face_spread,
                     faces_lit         = excluded.faces_lit,
                     group_solved      = excluded.group_solved,
                     scene             = excluded.scene,
                     unpolicied        = excluded.unpolicied,
                     confidence        = excluded.confidence,
                     reasons           = excluded.reasons,
                     reason_count      = excluded.reason_count,
                     model_ver         = excluded.model_ver,
                     analysis_ver      = excluded.analysis_ver,
                     policy_ver        = excluded.policy_ver,
                     shaping_ver       = excluded.shaping_ver,
                     at_ts             = excluded.at_ts,
                     at                = excluded.at",
                params![
                    photo_key,
                    project_key,
                    f64::from(row.strengths[0]),
                    f64::from(row.strengths[1]),
                    f64::from(row.strengths[2]),
                    f64::from(row.strengths[3]),
                    f64::from(row.strengths[4]),
                    f64::from(row.strengths[5]),
                    f64::from(row.budget_used),
                    i64::from(row.subject.clarity),
                    i64::from(row.subject.texture),
                    i64::from(row.subject.contrast),
                    f64::from(row.background.exposure_ev.clamp(-3.0, 0.0)),
                    i64::from(row.background.highlights.clamp(-100, 0)),
                    i64::from(row.background.saturation.clamp(-100, 0)),
                    f64::from(row.background.feather),
                    f64::from(row.background.competition_ratio.max(0.0)),
                    f64::from(row.background.chroma_energy.max(0.0)),
                    i64::from(row.background.bright_blobs),
                    f64::from(row.background.mean_luma_before.clamp(0.0, 1.0)),
                    f64::from(row.background.mean_luma_after.clamp(0.0, 1.0)),
                    row.shine_regions,
                    f64::from(row.shine_ev.clamp(-1.0, 0.0)),
                    f64::from(row.shine_area.clamp(0.0, 1.0)),
                    row.shine_boxes,
                    row.shaping,
                    f64::from(row.face_spread.max(0.0)),
                    row.faces_lit,
                    i64::from(row.group_solved),
                    row.scene,
                    i64::from(row.unpolicied),
                    f64::from(row.confidence.clamp(0.0, 1.0)),
                    row.reasons,
                    row.reason_count,
                    user_edited,
                    reviewed,
                    user_strengths,
                    row.model_ver,
                    row.analysis_ver,
                    row.policy_ver,
                    row.shaping_ver,
                    at_ts,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store the local light plan", &e))?;

            // The children are replaced wholesale: a plan is one decision and half of an old
            // one beside half of a new one is not a state anything could read.
            for table in ["local_light_face", "local_light_gate"] {
                conn.execute(
                    &format!("DELETE FROM {table} WHERE photo_id = ?1"),
                    params![photo_key],
                )
                .map_err(|e| statement_failed("could not clear the previous plan", &e))?;
            }

            for (ordinal, (identity, delta)) in faces.iter().enumerate().take(64) {
                conn.execute(
                    "INSERT INTO local_light_face (
                         photo_id, ordinal, identity_id, exposure_ev, shadows, highlights,
                         feather, luma_before, luma_target, luma_after, noise_cap_ev, mask_scale)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        photo_key,
                        i64::try_from(ordinal).unwrap_or(0),
                        identity.as_ref().map(IdentityId::to_db),
                        f64::from(delta.exposure_ev.clamp(-1.0, 1.5)),
                        i64::from(delta.shadows),
                        i64::from(delta.highlights.clamp(-100, 0)),
                        f64::from(delta.feather.clamp(0.0, 1.0)),
                        f64::from(delta.luma_before.clamp(0.0, 1.0)),
                        f64::from(delta.luma_target.clamp(0.0, 1.0)),
                        f64::from(delta.luma_after.clamp(0.0, 1.0)),
                        f64::from(delta.noise_cap_ev.max(0.0)),
                        f64::from(delta.mask_scale.clamp(0.0, 1.0)),
                    ],
                )
                .map_err(|e| statement_failed("could not store a lit face", &e))?;
            }

            for (op, kind) in &gates {
                conn.execute(
                    "INSERT OR REPLACE INTO local_light_gate (photo_id, op, mask_kind, mask_conf)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![photo_key, op.as_str(), kind.as_str(), 0.0f64],
                )
                .map_err(|e| statement_failed("could not store a gate", &e))?;
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
    pub fn get(&self, image: ImageId) -> AuraResult<Option<LocalLightPlan>> {
        let key = image.to_db();
        let faces = self.faces_of(image)?;
        let gates = self.gates_of(image)?;
        self.catalog.read(move |conn| {
            let stored = conn
                .query_row(
                    "SELECT s_face_light, s_subject, s_background, s_shine, s_dodge_burn_low,
                            s_dodge_burn_mid, budget_used, subject_clarity, subject_texture,
                            subject_contrast, bg_exposure_ev, bg_highlights, bg_saturation,
                            bg_feather, competition_ratio, chroma_energy, bright_blobs,
                            mean_luma_before, mean_luma_after, shine_ev, shine_area,
                            shine_boxes, shaping, scene, confidence, reasons, user_edited,
                            reviewed, model_ver, analysis_ver, policy_ver, shaping_ver
                       FROM local_light_plan WHERE photo_id = ?1",
                    params![key],
                    |row| {
                        Ok(Stored {
                            strengths: [
                                row.get::<_, f64>(0).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(1).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                            ],
                            budget_used: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                            clarity: row.get::<_, i64>(7).unwrap_or(0) as i16,
                            texture: row.get::<_, i64>(8).unwrap_or(0) as i16,
                            contrast: row.get::<_, i64>(9).unwrap_or(0) as i16,
                            bg_ev: row.get::<_, f64>(10).unwrap_or(0.0) as f32,
                            bg_highlights: row.get::<_, i64>(11).unwrap_or(0) as i16,
                            bg_saturation: row.get::<_, i64>(12).unwrap_or(0) as i16,
                            bg_feather: row.get::<_, f64>(13).unwrap_or(0.0) as f32,
                            competition: row.get::<_, f64>(14).unwrap_or(1.0) as f32,
                            chroma: row.get::<_, f64>(15).unwrap_or(0.0) as f32,
                            blobs: row.get::<_, i64>(16).unwrap_or(0) as u8,
                            mean_before: row.get::<_, f64>(17).unwrap_or(0.0) as f32,
                            mean_after: row.get::<_, f64>(18).unwrap_or(0.0) as f32,
                            shine_ev: row.get::<_, f64>(19).unwrap_or(0.0) as f32,
                            shine_area: row.get::<_, f64>(20).unwrap_or(0.0) as f32,
                            shine_boxes: row.get::<_, String>(21).unwrap_or_default(),
                            shaping: row.get::<_, String>(22).unwrap_or_default(),
                            scene: row.get::<_, String>(23).unwrap_or_default(),
                            confidence: row.get::<_, f64>(24).unwrap_or(0.0) as f32,
                            reasons: row.get::<_, String>(25).unwrap_or_default(),
                            user_edited: row.get::<_, i64>(26).unwrap_or(0) == 1,
                            reviewed: row.get::<_, i64>(27).unwrap_or(0) == 1,
                            model_ver: row.get::<_, i64>(28).unwrap_or(0) as u16,
                            analysis_ver: row.get::<_, i64>(29).unwrap_or(0) as u16,
                            policy_ver: row.get::<_, i64>(30).unwrap_or(0) as u16,
                            shaping_ver: row.get::<_, i64>(31).unwrap_or(0) as u16,
                        })
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the plan", &e))?;
            let Some(stored) = stored else {
                return Ok(None);
            };
            let boxes = decode_boxes(&stored.shine_boxes);
            let shine = if boxes.is_empty() || stored.shine_ev >= 0.0 {
                None
            } else {
                Some(ShineReduction {
                    identities: vec![None; boxes.len()],
                    regions: boxes,
                    reduction_ev: stored.shine_ev,
                    area_fraction: stored.shine_area,
                    peak_before: 0.0,
                    peak_after: 0.0,
                    mask_scale: 0.0,
                })
            };
            let dodge_burn = rebuild_shaping(&decode_shaping(&stored.shaping), stored.shaping_ver);
            Ok(Some(LocalLightPlan {
                image_id: image,
                face_light: faces,
                subject: SubjectEnhanceDelta {
                    clarity: stored.clarity,
                    texture: stored.texture,
                    contrast: stored.contrast,
                    paired_background_ev: stored.bg_ev,
                    competition_ratio: stored.competition,
                    mask_scale: 0.0,
                },
                background: BackgroundBalanceDelta {
                    exposure_ev: stored.bg_ev,
                    highlights: stored.bg_highlights,
                    saturation: stored.bg_saturation,
                    feather: stored.bg_feather,
                    competition_ratio: stored.competition,
                    chroma_energy: stored.chroma,
                    bright_blobs: stored.blobs,
                    mean_luma_before: stored.mean_before,
                    mean_luma_after: stored.mean_after,
                    mask_scale: 0.0,
                },
                dodge_burn,
                shine,
                total_budget_used: stored.budget_used,
                gated_by_mask_quality: gates,
                reasons: decode_reasons(&stored.reasons),
                confidence: stored.confidence,
                scene: SceneId::from_str_or_unknown(&stored.scene),
                strengths: stored.strengths,
                user_edited: stored.user_edited,
                reviewed: stored.reviewed,
                model_ver: stored.model_ver,
                analysis_ver: stored.analysis_ver,
                policy_ver: stored.policy_ver,
                shaping_ver: stored.shaping_ver,
            }))
        })
    }

    /// The lit faces of one photograph, in solver order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn faces_of(
        &self,
        image: ImageId,
    ) -> AuraResult<Vec<(Option<IdentityId>, FaceLightDelta)>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT identity_id, exposure_ev, shadows, highlights, feather,
                            luma_before, luma_target, luma_after, noise_cap_ev, mask_scale
                       FROM local_light_face WHERE photo_id = ?1 ORDER BY ordinal",
                )
                .map_err(|e| statement_failed("could not read the lit faces", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the lit faces", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a lit face", &e))?
            {
                let identity = row
                    .get::<_, Option<String>>(0)
                    .ok()
                    .flatten()
                    .and_then(|text| IdentityId::from_db(&text).ok());
                out.push((
                    identity,
                    FaceLightDelta {
                        exposure_ev: row.get::<_, f64>(1).unwrap_or(0.0) as f32,
                        shadows: row.get::<_, i64>(2).unwrap_or(0) as i16,
                        highlights: row.get::<_, i64>(3).unwrap_or(0) as i16,
                        feather: row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                        luma_before: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                        luma_target: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                        luma_after: row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                        noise_cap_ev: row.get::<_, f64>(8).unwrap_or(0.0) as f32,
                        mask_scale: row.get::<_, f64>(9).unwrap_or(0.0) as f32,
                    },
                ));
            }
            Ok(out)
        })
    }

    /// Which operations were gated on one photograph, and by what.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn gates_of(&self, image: ImageId) -> AuraResult<Vec<(LocalOp, MaskKind)>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT op, mask_kind FROM local_light_gate WHERE photo_id = ?1 ORDER BY op",
                )
                .map_err(|e| statement_failed("could not read the gates", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the gates", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a gate", &e))?
            {
                let op = row
                    .get::<_, String>(0)
                    .ok()
                    .and_then(|t| LocalOp::parse(&t));
                let kind = row
                    .get::<_, String>(1)
                    .ok()
                    .and_then(|t| MaskKind::parse(&t));
                if let (Some(op), Some(kind)) = (op, kind) {
                    out.push((op, kind));
                }
            }
            Ok(out)
        })
    }

    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    #[allow(clippy::too_many_lines)]
    pub fn outline(
        &self,
        project: &ProjectId,
        unpolicied: Vec<String>,
    ) -> AuraResult<LocalOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let (photos, planned, acted_on, group_solved, shine_reduced, user_edited, needs_review): (
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
            ) = conn
                .query_row(
                    "SELECT images, planned, acted_on, group_solved, shine_reduced, user_edited,
                            needs_review
                       FROM v_local_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get(0).unwrap_or(0),
                            row.get(1).unwrap_or(0),
                            row.get(2).unwrap_or(0),
                            row.get(3).unwrap_or(0),
                            row.get(4).unwrap_or(0),
                            row.get(5).unwrap_or(0),
                            row.get(6).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the coverage view", &e))?
                .unwrap_or((0, 0, 0, 0, 0, 0, 0));

            let mut outline = LocalOutline {
                planned: u32::try_from(planned).unwrap_or(0),
                photos: u32::try_from(photos).unwrap_or(0),
                coverage: ratio(planned, photos),
                acted_on: ratio(acted_on, planned),
                mask_covered: 0.0,
                op_histogram: [0; LocalOp::COUNT],
                gated_histogram: [0; MaskKind::COUNT],
                mean_budget_used: 0.0,
                shine_reduced: u32::try_from(shine_reduced).unwrap_or(0),
                mean_shine_ev: 0.0,
                group_solved: u32::try_from(group_solved).unwrap_or(0),
                needs_review: u32::try_from(needs_review).unwrap_or(0),
                user_edited: u32::try_from(user_edited).unwrap_or(0),
                unpolicied_scenes: unpolicied,
                model_ver: 0,
                analysis_ver: 0,
                policy_ver: 0,
                shaping_ver: 0,
            };

            // The six operation counts, one aggregate rather than six queries.
            let columns = [
                "s_face_light",
                "s_subject",
                "s_background",
                "s_shine",
                "s_dodge_burn_low",
                "s_dodge_burn_mid",
            ];
            for (index, column) in columns.iter().enumerate() {
                let count: i64 = conn
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM local_light_plan
                              WHERE project_id = ?1 AND {column} > 0.0"
                        ),
                        params![key],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                if let Some(slot) = outline.op_histogram.get_mut(index) {
                    *slot = u32::try_from(count).unwrap_or(0);
                }
            }

            let (mean_budget, mean_shine): (f64, f64) = conn
                .query_row(
                    "SELECT AVG(budget_used), AVG(CASE WHEN shine_regions > 0 THEN shine_ev END)
                       FROM local_light_plan WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, Option<f64>>(0).unwrap_or(None).unwrap_or(0.0),
                            row.get::<_, Option<f64>>(1).unwrap_or(None).unwrap_or(0.0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the budget average", &e))?
                .unwrap_or((0.0, 0.0));
            outline.mean_budget_used = mean_budget as f32;
            outline.mean_shine_ev = mean_shine as f32;

            // Section 11's `local.gated {mask_kind, count}`.
            let mut statement = conn
                .prepare(
                    "SELECT g.mask_kind, COUNT(*)
                       FROM local_light_gate g
                       JOIN local_light_plan l ON l.photo_id = g.photo_id
                      WHERE l.project_id = ?1
                      GROUP BY g.mask_kind",
                )
                .map_err(|e| statement_failed("could not read the gate histogram", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the gate histogram", &e))?;
            let mut gated_frames = 0i64;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a gate count", &e))?
            {
                let kind = row.get::<_, String>(0).ok().and_then(|t| MaskKind::parse(&t));
                let count = row.get::<_, i64>(1).unwrap_or(0);
                gated_frames += count;
                if let Some(kind) = kind {
                    let index = MaskKind::ALL
                        .iter()
                        .position(|k| *k == kind)
                        .unwrap_or(MaskKind::COUNT);
                    if let Some(slot) = outline.gated_histogram.get_mut(index) {
                        *slot = u32::try_from(count).unwrap_or(0);
                    }
                }
            }

            // A frame is mask-covered when nothing on it was gated.
            let ungated: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM local_light_plan l
                      WHERE l.project_id = ?1
                        AND NOT EXISTS (SELECT 1 FROM local_light_gate g
                                         WHERE g.photo_id = l.photo_id)",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let _ = gated_frames;
            outline.mask_covered = ratio(ungated, planned);

            let versions: Option<(i64, i64, i64, i64)> = conn
                .query_row(
                    "SELECT model_ver, analysis_ver, policy_ver, shaping_ver
                       FROM local_light_plan WHERE project_id = ?1 LIMIT 1",
                    params![key],
                    |row| {
                        Ok((
                            row.get(0).unwrap_or(0),
                            row.get(1).unwrap_or(0),
                            row.get(2).unwrap_or(0),
                            row.get(3).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the versions", &e))?;
            if let Some((model, analysis, policy, shaping)) = versions {
                outline.model_ver = u16::try_from(model).unwrap_or(0);
                outline.analysis_ver = u16::try_from(analysis).unwrap_or(0);
                outline.policy_ver = u16::try_from(policy).unwrap_or(0);
                outline.shaping_ver = u16::try_from(shaping).unwrap_or(0);
            }
            Ok(outline)
        })
    }

    /// The frames worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn needs_review(&self, project: &ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        let key = project.to_db();
        let limit = i64::try_from(limit.clamp(1, 5000)).unwrap_or(200);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM local_light_plan
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                        AND confidence < ?2
                      ORDER BY confidence ASC, photo_id ASC
                      LIMIT ?3",
                )
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut cursor = statement
                .query(params![key, f64::from(REVIEW_BELOW), limit])
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a queued frame", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Record that the photographer has looked at one plan and agrees.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5085` when the photograph has no plan.
    pub fn accept(&self, image: ImageId) -> Result<(), aura_core::AuraError> {
        let key = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE local_light_plan SET reviewed = 1 WHERE photo_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not accept the plan", &e))
        })?;
        if changed == 0 {
            return Err(errors::local_edit_refused(format!(
                "{} has no local light plan to accept",
                image.to_db()
            )));
        }
        Ok(())
    }

    /// Record what the photographer set instead.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5085` when the photograph has no plan, the override is empty, or a strength is
    /// outside `0..1`.
    pub fn set_override(
        &self,
        image: ImageId,
        values: LocalOverride,
    ) -> Result<(), aura_core::AuraError> {
        crate::local::guard::check_override(&values)?;
        let key = image.to_db();
        let encoded = encode_override(&values);
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE local_light_plan
                    SET user_edited = 1, reviewed = 1, user_strengths = ?2
                  WHERE photo_id = ?1",
                params![key, encoded],
            )
            .map_err(|e| statement_failed("could not record the override", &e))
        })?;
        if changed == 0 {
            return Err(errors::local_edit_refused(format!(
                "{} has no local light plan to override",
                image.to_db()
            )));
        }
        Ok(())
    }

    /// The photographer's own strengths for one photograph, when they set any.
    ///
    /// Beside the frozen service rather than on it, exactly as `ToneStore::override_of` is,
    /// and for the same reason: the review panel has to be able to show both sides of a
    /// disagreement and phase 30's learning loop has to be able to read one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn override_of(&self, image: ImageId) -> AuraResult<Option<LocalOverride>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let stored: Option<Option<String>> = conn
                .query_row(
                    "SELECT user_strengths FROM local_light_plan WHERE photo_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the override", &e))?;
            Ok(stored.flatten().as_deref().map(decode_override))
        })
    }

    /// How many frames each operation ran on.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn op_counts(&self, project: &ProjectId) -> AuraResult<BTreeMap<LocalOp, u32>> {
        let outline = self.outline(project, Vec::new())?;
        Ok(LocalOp::PRIORITY
            .into_iter()
            .enumerate()
            .map(|(index, op)| (op, outline.op_histogram.get(index).copied().unwrap_or(0)))
            .collect())
    }
}

/// The scalar half of a row, so the transaction closure moves one value.
#[derive(Debug, Clone)]
struct Row {
    strengths: [f32; LocalOp::COUNT],
    budget_used: f32,
    subject: SubjectEnhanceDelta,
    background: BackgroundBalanceDelta,
    shine_regions: i64,
    shine_ev: f32,
    shine_area: f32,
    shine_boxes: String,
    shaping: String,
    face_spread: f32,
    faces_lit: i64,
    group_solved: bool,
    scene: String,
    unpolicied: bool,
    confidence: f32,
    reasons: String,
    reason_count: i64,
    model_ver: i64,
    analysis_ver: i64,
    policy_ver: i64,
    shaping_ver: i64,
}

/// What one stored row reads back as.
#[derive(Debug, Clone)]
struct Stored {
    strengths: [f32; LocalOp::COUNT],
    budget_used: f32,
    clarity: i16,
    texture: i16,
    contrast: i16,
    bg_ev: f32,
    bg_highlights: i16,
    bg_saturation: i16,
    bg_feather: f32,
    competition: f32,
    chroma: f32,
    blobs: u8,
    mean_before: f32,
    mean_after: f32,
    shine_ev: f32,
    shine_area: f32,
    shine_boxes: String,
    shaping: String,
    scene: String,
    confidence: f32,
    reasons: String,
    user_edited: bool,
    reviewed: bool,
    model_ver: u16,
    analysis_ver: u16,
    policy_ver: u16,
    shaping_ver: u16,
}

/// Rebuild the shaping maps from the four numbers each face stored.
///
/// The zones and the grids are both regenerated, which is the whole point of storing the
/// inputs. The band energies are not recoverable and read back as zero - they are a
/// *measurement of the pass*, not of the plan, and the texture gate that uses them runs
/// against a live analysis rather than against a stored row.
fn rebuild_shaping(rows: &[ShapingRow], shaping_ver: u16) -> Option<DodgeBurnMaps> {
    if rows.is_empty() {
        return None;
    }
    let faces = rows
        .iter()
        .map(|row| {
            let zones = dodgeburn::zones_for(row.region, row.direction, row.low_strength, 1.0);
            FaceShaping {
                identity: None,
                region: row.region,
                side: SHAPING_SIDE,
                low_freq: dodgeburn::grid(row.region, &zones),
                mid_freq: Vec::new(),
                zones,
                light_direction: row.direction,
                low_strength: row.low_strength,
                evening: row.evening,
                band_energy_before: 0.0,
                band_energy_after: 0.0,
            }
        })
        .collect();
    Some(DodgeBurnMaps { faces, shaping_ver })
}

/// The shaping, as the four numbers each face's zones are derived from.
///
/// `[[x, y, w, h, direction, low_strength, evening], ...]` - one array per face. Every zone's
/// centre and radius is a fixed proportion of the region and its gain is a fixed base scaled
/// by the direction and the strength, so `dodgeburn::zones_for` reproduces the list exactly.
///
/// Writing the zones out instead costs about 450 bytes a face, and a group formal has four of
/// them: measured, that was 1,286 of the 2,236 bytes this table used per image. This is the
/// same argument as storing zones rather than grids, taken one step further - and it is why
/// `shaping_ver` exists, because a change to the derivation moves delivered pixels without
/// moving a stored number.
fn encode_shaping(maps: Option<&DodgeBurnMaps>) -> String {
    let Some(maps) = maps else {
        return "[]".to_string();
    };
    let faces: Vec<Value> = maps
        .faces
        .iter()
        .map(|face| {
            Value::from(vec![
                round3(face.region.x),
                round3(face.region.y),
                round3(face.region.w),
                round3(face.region.h),
                round3(face.light_direction),
                round3(face.low_strength),
                round3(face.evening),
            ])
        })
        .collect();
    serde_json::to_string(&faces).unwrap_or_else(|_| "[]".to_string())
}

/// One stored face's shaping inputs.
#[derive(Debug, Clone, Copy)]
struct ShapingRow {
    region: CropRect,
    direction: f32,
    low_strength: f32,
    evening: f32,
}

/// Read a shaping document back.
///
/// Total: a malformed document reads as no shaping at all. The rule phase 07's
/// `SceneId::from_str_or_unknown` established - a catalog written by a newer build must still
/// open, and an unreadable decision detail must never take a decision row down with it.
fn decode_shaping(text: &str) -> Vec<ShapingRow> {
    let Ok(Value::Array(faces)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    faces
        .iter()
        .filter_map(|face| {
            let row = face.as_array()?;
            let number =
                |index: usize| row.get(index).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            Some(ShapingRow {
                region: CropRect {
                    x: number(0),
                    y: number(1),
                    w: number(2),
                    h: number(3),
                },
                direction: number(4),
                low_strength: number(5),
                evening: number(6),
            })
        })
        .collect()
}

/// A reason stores its code, not its sentence. Phase 09's rule, fifth migration running.
fn encode_reasons(reasons: &[LocalReason]) -> String {
    let items: Vec<Value> = reasons
        .iter()
        .map(|reason| {
            let mut row = vec![
                Value::from(reason.code.as_str()),
                Value::from(f64::from((reason.weight * 1000.0).round() / 1000.0)),
            ];
            if let Some(evidence) = reason.evidence {
                row.push(Value::from(vec![
                    round3(evidence.x),
                    round3(evidence.y),
                    round3(evidence.w),
                    round3(evidence.h),
                ]));
            }
            Value::from(row)
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

/// Read a reason document back. Total: an unknown code reads as
/// [`LocalCode::FaceAlreadyInBand`], the code that claims the least.
fn decode_reasons(text: &str) -> Vec<LocalReason> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text) else {
        return vec![LocalReason::plain(LocalCode::FaceAlreadyInBand, 0.0)];
    };
    let mut out: Vec<LocalReason> = items
        .iter()
        .filter_map(|item| {
            let row = item.as_array()?;
            let code = row
                .first()
                .and_then(Value::as_str)
                .and_then(LocalCode::parse)
                .unwrap_or_default();
            let weight = row.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let evidence = row.get(2).and_then(Value::as_array).map(|rect| {
                let value =
                    |index: usize| rect.get(index).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                CropRect {
                    x: value(0),
                    y: value(1),
                    w: value(2),
                    h: value(3),
                }
            });
            Some(match evidence {
                Some(rect) => LocalReason::plain_at(code, weight, rect),
                None => LocalReason::plain(code, weight),
            })
        })
        .collect();
    if out.is_empty() {
        out.push(LocalReason::plain(LocalCode::FaceAlreadyInBand, 0.0));
    }
    out
}

/// The shine regions, as a compact positional document.
fn encode_boxes(boxes: &[CropRect]) -> String {
    let items: Vec<Value> = boxes
        .iter()
        .map(|rect| {
            Value::from(vec![
                round3(rect.x),
                round3(rect.y),
                round3(rect.w),
                round3(rect.h),
            ])
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

fn decode_boxes(text: &str) -> Vec<CropRect> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let row = item.as_array()?;
            let value = |index: usize| row.get(index).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            Some(CropRect {
                x: value(0),
                y: value(1),
                w: value(2),
                h: value(3),
            })
        })
        .collect()
}

/// The photographer's own strengths, sparse.
fn encode_override(values: &LocalOverride) -> String {
    let mut object = serde_json::Map::new();
    for (index, slot) in values.strengths.iter().enumerate() {
        if let (Some(value), Some(op)) = (slot, LocalOp::PRIORITY.get(index)) {
            object.insert(
                op.as_str().to_string(),
                Value::from(f64::from((value * 1000.0).round() / 1000.0)),
            );
        }
    }
    serde_json::to_string(&Value::Object(object)).unwrap_or_else(|_| "{}".to_string())
}

fn decode_override(text: &str) -> LocalOverride {
    let mut out = LocalOverride::default();
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(text) else {
        return out;
    };
    for (index, op) in LocalOp::PRIORITY.iter().enumerate() {
        if let Some(value) = object.get(op.as_str()).and_then(Value::as_f64) {
            if let Some(slot) = out.strengths.get_mut(index) {
                *slot = Some(value as f32);
            }
        }
    }
    out
}

fn round3(value: f32) -> f64 {
    f64::from((value * 1000.0).round() / 1000.0)
}

fn ratio(part: i64, whole: i64) -> f32 {
    if whole <= 0 {
        0.0
    } else {
        (part as f32 / whole as f32).clamp(0.0, 1.0)
    }
}

fn epoch_ms(now: time::OffsetDateTime) -> i64 {
    now.unix_timestamp() * 1000 + i64::from(now.millisecond())
}
