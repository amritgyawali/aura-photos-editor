//! The four tables migration 21 adds, and the three rules that live in the SQL rather than
//! around it.
//!
//! ## `user_edited` is re-applied inside the statement
//!
//! A re-analysis overwrites every plan it recomputes, and that upsert carries the preset a
//! photographer chose forward from the row it is replacing, in the same statement. Ninth time
//! the rule has been written into a store, and the window it closes is the same one every time:
//! a background pass that read the flag a moment earlier and wrote a moment later.
//!
//! ## A protect row belongs to a person, and a photographer one is never touched
//!
//! `retouch_protected` is keyed by identity rather than by photograph, and a re-analysis deletes
//! only the rows it wrote itself - `source <> 'user'` inside the `DELETE`. A tattoo cannot be
//! deleted at all, and that is enforced by a trigger in the migration rather than here: a
//! promise that holds in one layer holds until somebody writes a second caller.
//!
//! ## A withdrawn retouch is a row with no operations
//!
//! `withdrawn = 1` with `op_count = 0` means AURA tried and could not do it safely. An absent
//! row means nobody has looked. Phases 21, 25 and 27 all read this table and all three act
//! differently on those two states, so the schema keeps them apart with a CHECK.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::composition::Box2;
use aura_core::contract::people::Role;
use aura_core::contract::retouch::{
    FreqBand, ImageId, InpaintMethod, ProtectedFeature, ProtectedKind, ProtectedSource,
    RetouchCode, RetouchOp, RetouchOutline, RetouchOverride, RetouchPlan, RetouchPreset,
    RetouchReason, TextureReport, REVIEW_BELOW,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult, IdentityId, MaskId, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use crate::errors;
use crate::strength::IdentityStats;

/// Bytes one photograph costs in `retouch_plan`, `retouch_op` and their indexes.
///
/// Section 11 sets no storage budget for this phase - all four of its rows are time. One is
/// measured anyway, against the kilobyte per image every phase since 09 has aimed at, because a
/// decision table that quietly costs four kilobytes a frame is a catalog nobody can back up.
///
/// The measured figure over 1,000 photographs of the widest row the fixtures produce - a
/// portrait with a mark removed, a texture measurement and five reason codes - taken as the
/// change in `PRAGMA page_count`:
///
/// ```text
///   measured total                        659 B/image
/// ```
///
/// The constant below is 900 rather than 659, and the headroom is deliberate: phase 19
/// correction was that a budget measured with a quantised instrument must not be pinned at its
/// own measurement, and `page_count` moves in 4 KiB steps. A busier frame - a family formal with
/// six faces, three marks each and a dozen reasons - costs more than the fixture does, and the
/// gap is where that lives.
///
/// `retouch_protected` and `retouch_identity` are deliberately not in the figure: they are per
/// *person* rather than per photograph, and a wedding with sixty identified people and four
/// protect rows each costs about 20 KB in total - which does not scale with the size of the
/// import and would make a per-image number meaningless.
pub const BYTES_PER_IMAGE: usize = 900;

/// The `retouch_plan`, `retouch_identity`, `retouch_protected` and `retouch_op` tables.
#[derive(Debug)]
pub struct RetouchStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl RetouchStore {
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
    /// 50 % or 90 % and the next run asks the catalog what is left. A `preset_ver` bump
    /// therefore heals itself - the rows made under the old table are pending by definition.
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
                       LEFT JOIN retouch_plan r ON r.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (r.photo_id IS NULL
                             OR r.model_ver    <> ?2
                             OR r.analysis_ver <> ?3
                             OR r.preset_ver   <> ?4)
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
    /// What the IPC layer walks when it carries plans into recipes. Separate from
    /// [`RetouchStore::pending`], which answers the opposite question and would return every
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
                    "SELECT photo_id FROM retouch_plan WHERE project_id = ?1 ORDER BY photo_id",
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
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)]
    pub fn put(&self, project: &ProjectId, plan: &RetouchPlan) -> AuraResult<()> {
        let photo = plan.image_id.to_db();
        let project_key = project.to_db();
        let stamped = aura_catalog::rfc3339(self.clock.now_utc());
        let reasons = encode_reasons(&plan.reasons);
        let ops = plan.ops.clone();
        let plan = plan.clone();

        self.catalog.writer().transact(move |conn| {
            // The preset and the two bits that protect it, read and rewritten inside the same
            // statement. See the module header.
            let existing: Option<(i64, i64, String)> = conn
                .query_row(
                    "SELECT user_edited, reviewed, preset FROM retouch_plan WHERE photo_id = ?1",
                    params![photo],
                    |row| {
                        Ok((
                            row.get(0).unwrap_or(0),
                            row.get(1).unwrap_or(0),
                            row.get(2).unwrap_or_default(),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the existing plan", &e))?;

            let (user_edited, reviewed, preset) = match existing {
                Some((1, reviewed, preset)) => (1, reviewed, preset),
                Some((_, reviewed, _)) => (0, reviewed, plan.preset.as_str().to_string()),
                None => (0, 0, plan.preset.as_str().to_string()),
            };

            let blemishes = count(plan.count_of("blemish"));
            conn.execute(
                "INSERT INTO retouch_plan (
                     photo_id, project_id, preset, scene,
                     band_ratio, texture_floor, texture_passed, texture_samples,
                     texture_resolves, withdrawn,
                     op_count, blemish_count, faces_seen, faces_retouched, anomalies_left,
                     budget_used, mask_covered, confidence, reasons,
                     user_edited, reviewed, model_ver, analysis_ver, preset_ver, planned_at
                 ) VALUES (
                     ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                     ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
                 )
                 ON CONFLICT(photo_id) DO UPDATE SET
                     project_id = excluded.project_id,
                     preset = excluded.preset,
                     scene = excluded.scene,
                     band_ratio = excluded.band_ratio,
                     texture_floor = excluded.texture_floor,
                     texture_passed = excluded.texture_passed,
                     texture_samples = excluded.texture_samples,
                     texture_resolves = excluded.texture_resolves,
                     withdrawn = excluded.withdrawn,
                     op_count = excluded.op_count,
                     blemish_count = excluded.blemish_count,
                     faces_seen = excluded.faces_seen,
                     faces_retouched = excluded.faces_retouched,
                     anomalies_left = excluded.anomalies_left,
                     budget_used = excluded.budget_used,
                     mask_covered = excluded.mask_covered,
                     confidence = excluded.confidence,
                     reasons = excluded.reasons,
                     model_ver = excluded.model_ver,
                     analysis_ver = excluded.analysis_ver,
                     preset_ver = excluded.preset_ver,
                     planned_at = excluded.planned_at",
                params![
                    photo,
                    project_key,
                    preset,
                    plan.scene.as_str(),
                    f64::from(plan.texture_report.band_ratio),
                    f64::from(plan.texture_report.floor),
                    i64::from(plan.texture_report.passed),
                    i64::from(plan.texture_report.measured_on),
                    i64::from(plan.texture_report.resolves),
                    i64::from(plan.texture_report.withdrawn),
                    count(plan.ops.len()),
                    blemishes,
                    i64::from(plan.per_identity_strength.len() as u32),
                    i64::from(plan.per_identity_strength.len() as u32),
                    plan.reasons
                        .iter()
                        .filter(|r| {
                            matches!(
                                r.code,
                                RetouchCode::AnomalyUncertain
                                    | RetouchCode::AnomalyTooLarge
                                    | RetouchCode::FeatureProtected
                                    | RetouchCode::VetoedByProtection
                            )
                        })
                        .count()
                        .try_into()
                        .unwrap_or(i64::MAX),
                    f64::from(plan.budget_used),
                    i64::from(
                        !plan.texture_report.withdrawn && plan.texture_report.measured_on > 0
                    ),
                    f64::from(plan.confidence),
                    reasons,
                    user_edited,
                    reviewed,
                    i64::from(plan.model_ver),
                    i64::from(plan.analysis_ver),
                    i64::from(plan.preset_ver),
                    stamped,
                ],
            )
            .map_err(|e| statement_failed("could not store the retouch plan", &e))?;

            conn.execute("DELETE FROM retouch_op WHERE photo_id = ?1", params![photo])
                .map_err(|e| statement_failed("could not clear the operations", &e))?;

            for (seq, op) in ops.iter().enumerate() {
                let (x, y, w, h) = match op.area() {
                    Some(area) => (
                        Some(f64::from(area.x)),
                        Some(f64::from(area.y)),
                        Some(f64::from(area.w)),
                        Some(f64::from(area.h)),
                    ),
                    None => (None, None, None, None),
                };
                let (method, identity, mask, luma, chroma, band) = match op {
                    RetouchOp::Blemish { method, .. } => (
                        Some(method.as_str().to_string()),
                        None,
                        None,
                        0.0f64,
                        0.0f64,
                        None,
                    ),
                    RetouchOp::UnderEye {
                        identity,
                        luma,
                        chroma,
                    } => (
                        None,
                        Some(identity.to_db()),
                        None,
                        f64::from(*luma),
                        f64::from(*chroma),
                        None,
                    ),
                    RetouchOp::ToneEvening { mask, band, .. } => (
                        None,
                        None,
                        Some(mask.to_db()),
                        0.0,
                        0.0,
                        Some(band.as_str().to_string()),
                    ),
                    RetouchOp::ShineReduce { .. } => (None, None, None, 0.0, 0.0, None),
                };
                conn.execute(
                    "INSERT INTO retouch_op (
                         photo_id, seq, kind, x, y, w, h, strength, method,
                         identity_id, mask_id, luma_ev, chroma, band
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        photo,
                        count(seq),
                        op.as_str(),
                        x,
                        y,
                        w,
                        h,
                        f64::from(op.strength()),
                        method,
                        identity,
                        mask,
                        luma,
                        chroma,
                        band,
                    ],
                )
                .map_err(|e| statement_failed("could not store a retouch operation", &e))?;
            }
            Ok(())
        })
    }

    /// One photograph plan.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    #[allow(clippy::too_many_lines)]
    pub fn get(&self, image: ImageId) -> AuraResult<Option<RetouchPlan>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let row: Option<StoredPlan> = conn
                .query_row(
                    "SELECT preset, scene, band_ratio, texture_floor, texture_passed,
                            texture_samples, texture_resolves, withdrawn, budget_used,
                            confidence, reasons, user_edited, reviewed,
                            model_ver, analysis_ver, preset_ver
                       FROM retouch_plan WHERE photo_id = ?1",
                    params![key],
                    |row| {
                        Ok(StoredPlan {
                            preset: row.get(0).unwrap_or_default(),
                            scene: row.get(1).unwrap_or_default(),
                            band_ratio: row.get(2).unwrap_or(1.0),
                            texture_floor: row.get(3).unwrap_or(0.9),
                            texture_passed: row.get(4).unwrap_or(1),
                            texture_samples: row.get(5).unwrap_or(0),
                            texture_resolves: row.get(6).unwrap_or(0),
                            withdrawn: row.get(7).unwrap_or(0),
                            budget_used: row.get(8).unwrap_or(0.0),
                            confidence: row.get(9).unwrap_or(0.0),
                            reasons: row.get(10).unwrap_or_default(),
                            user_edited: row.get(11).unwrap_or(0),
                            reviewed: row.get(12).unwrap_or(0),
                            model_ver: row.get(13).unwrap_or(0),
                            analysis_ver: row.get(14).unwrap_or(0),
                            preset_ver: row.get(15).unwrap_or(0),
                        })
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the retouch plan", &e))?;

            let Some(row) = row else {
                return Ok(None);
            };

            let mut statement = conn
                .prepare(
                    "SELECT kind, x, y, w, h, strength, method, identity_id, mask_id,
                            luma_ev, chroma, band
                       FROM retouch_op WHERE photo_id = ?1 ORDER BY seq",
                )
                .map_err(|e| statement_failed("could not read the operations", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the operations", &e))?;
            let mut ops = Vec::new();
            while let Some(op) = cursor
                .next()
                .map_err(|e| statement_failed("could not read an operation", &e))?
            {
                let kind: String = op.get(0).unwrap_or_default();
                let area = Box2 {
                    x: op.get::<_, Option<f64>>(1).ok().flatten().unwrap_or(0.0) as f32,
                    y: op.get::<_, Option<f64>>(2).ok().flatten().unwrap_or(0.0) as f32,
                    w: op.get::<_, Option<f64>>(3).ok().flatten().unwrap_or(0.0) as f32,
                    h: op.get::<_, Option<f64>>(4).ok().flatten().unwrap_or(0.0) as f32,
                };
                let strength = op.get::<_, f64>(5).unwrap_or(0.0) as f32;
                match kind.as_str() {
                    "blemish" => ops.push(RetouchOp::Blemish {
                        area,
                        method: op
                            .get::<_, Option<String>>(6)
                            .ok()
                            .flatten()
                            .and_then(|text| InpaintMethod::parse(&text))
                            .unwrap_or_default(),
                        strength,
                    }),
                    "under_eye" => {
                        let identity = op
                            .get::<_, Option<String>>(7)
                            .ok()
                            .flatten()
                            .and_then(|text| IdentityId::from_db(&text).ok());
                        if let Some(identity) = identity {
                            ops.push(RetouchOp::UnderEye {
                                identity,
                                luma: op.get::<_, f64>(9).unwrap_or(0.0) as f32,
                                chroma: op.get::<_, f64>(10).unwrap_or(0.0) as f32,
                            });
                        }
                    }
                    "tone_evening" => {
                        let mask = op
                            .get::<_, Option<String>>(8)
                            .ok()
                            .flatten()
                            .and_then(|text| MaskId::from_db(&text).ok());
                        if let Some(mask) = mask {
                            ops.push(RetouchOp::ToneEvening {
                                mask,
                                strength,
                                band: op
                                    .get::<_, Option<String>>(11)
                                    .ok()
                                    .flatten()
                                    .and_then(|text| FreqBand::parse(&text))
                                    .unwrap_or_default(),
                            });
                        }
                    }
                    _ => {}
                }
            }

            let identity_strength = read_identity_strengths_for_photo(conn, &key)?;
            let protected = read_protected_for_photo(conn, &key)?;

            Ok(Some(RetouchPlan {
                image_id: image,
                ops,
                per_identity_strength: identity_strength,
                protected,
                texture_report: TextureReport {
                    band_ratio: row.band_ratio as f32,
                    floor: row.texture_floor as f32,
                    passed: row.texture_passed == 1,
                    measured_on: row.texture_samples as u32,
                    resolves: row.texture_resolves as u8,
                    withdrawn: row.withdrawn == 1,
                },
                preset: RetouchPreset::parse(&row.preset).unwrap_or_default(),
                reasons: decode_reasons(&row.reasons),
                confidence: row.confidence as f32,
                scene: SceneId::from_str_or_unknown(&row.scene),
                budget_used: row.budget_used as f32,
                user_edited: row.user_edited == 1,
                reviewed: row.reviewed == 1,
                model_ver: row.model_ver as u16,
                analysis_ver: row.analysis_ver as u16,
                preset_ver: row.preset_ver as u16,
            }))
        })
    }

    /// What a project pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    #[allow(clippy::too_many_lines)]
    pub fn outline(
        &self,
        project: &ProjectId,
        unpreset: Vec<String>,
    ) -> AuraResult<RetouchOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut outline = RetouchOutline {
                unpreset_scenes: unpreset,
                ..RetouchOutline::default()
            };

            let (photos, planned, acted, masked, withdrawn, resolved, ratio): (
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
                Option<f64>,
            ) = conn
                .query_row(
                    "SELECT photos, planned, acted_on, mask_covered, withdrawn, texture_resolved,
                            mean_band_ratio
                       FROM v_retouch_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get(0).unwrap_or(0),
                            row.get(1).unwrap_or(0),
                            row.get(2).unwrap_or(0),
                            row.get(3).unwrap_or(0),
                            row.get(4).unwrap_or(0),
                            row.get(5).unwrap_or(0),
                            row.get(6).ok(),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the retouch coverage", &e))?
                .unwrap_or((0, 0, 0, 0, 0, 0, None));

            outline.photos = photos as u32;
            outline.planned = planned as u32;
            outline.coverage = fraction(planned, photos);
            outline.acted_on = fraction(acted, planned);
            outline.mask_covered = fraction(masked, planned);
            outline.texture_withdrawn = withdrawn as u32;
            outline.texture_resolved = resolved as u32;
            outline.mean_band_ratio = ratio.unwrap_or(0.0) as f32;

            let (blemishes, anomalies, faces_seen, faces_retouched, needs_review, edited): (
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
            ) = conn
                .query_row(
                    "SELECT COALESCE(SUM(blemish_count), 0),
                            COALESCE(SUM(anomalies_left), 0),
                            COALESCE(SUM(faces_seen), 0),
                            COALESCE(SUM(faces_retouched), 0),
                            COALESCE(SUM(CASE WHEN reviewed = 0 AND user_edited = 0
                                               AND confidence < ?2 THEN 1 ELSE 0 END), 0),
                            COALESCE(SUM(user_edited), 0)
                       FROM retouch_plan WHERE project_id = ?1",
                    params![key, f64::from(REVIEW_BELOW)],
                    |row| {
                        Ok((
                            row.get(0).unwrap_or(0),
                            row.get(1).unwrap_or(0),
                            row.get(2).unwrap_or(0),
                            row.get(3).unwrap_or(0),
                            row.get(4).unwrap_or(0),
                            row.get(5).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not summarise the retouch plans", &e))?
                .unwrap_or((0, 0, 0, 0, 0, 0));

            outline.blemishes_removed = blemishes as u32;
            outline.anomalies_left = anomalies as u32;
            outline.faces_seen = faces_seen as u32;
            outline.faces_retouched = faces_retouched as u32;
            outline.needs_review = needs_review as u32;
            outline.user_edited = edited as u32;

            let mut presets = conn
                .prepare(
                    "SELECT preset, COUNT(*) FROM retouch_plan WHERE project_id = ?1
                      GROUP BY preset",
                )
                .map_err(|e| statement_failed("could not read the preset histogram", &e))?;
            let mut cursor = presets
                .query(params![key])
                .map_err(|e| statement_failed("could not read the preset histogram", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a preset count", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                let count: i64 = row.get(1).unwrap_or(0);
                if let Some(preset) = RetouchPreset::parse(&text) {
                    if let Some(slot) = outline.preset_histogram.get_mut(
                        RetouchPreset::ALL
                            .iter()
                            .position(|p| *p == preset)
                            .unwrap_or(0),
                    ) {
                        *slot = count as u32;
                    }
                }
            }

            let mut kinds = conn
                .prepare(
                    "SELECT kind, COUNT(*) FROM retouch_protected WHERE project_id = ?1
                      GROUP BY kind",
                )
                .map_err(|e| statement_failed("could not read the protected histogram", &e))?;
            let mut cursor = kinds
                .query(params![key])
                .map_err(|e| statement_failed("could not read the protected histogram", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a protected count", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                let count: i64 = row.get(1).unwrap_or(0);
                if let Some(kind) = ProtectedKind::parse(&text) {
                    if let Some(slot) = outline.protected_histogram.get_mut(
                        ProtectedKind::ALL
                            .iter()
                            .position(|k| *k == kind)
                            .unwrap_or(0),
                    ) {
                        *slot = count as u32;
                    }
                }
            }

            let strengths = read_identity_strengths(conn, &key)?;
            let values: Vec<f32> = strengths.values().copied().collect();
            outline.mean_strength = if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f32>() / values.len() as f32
            };
            outline.max_identity_spread = 0.0;

            let (model, analysis, preset_ver): (i64, i64, i64) = conn
                .query_row(
                    "SELECT COALESCE(MAX(model_ver), 0), COALESCE(MAX(analysis_ver), 0),
                            COALESCE(MAX(preset_ver), 0)
                       FROM retouch_plan WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get(0).unwrap_or(0),
                            row.get(1).unwrap_or(0),
                            row.get(2).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the stored versions", &e))?
                .unwrap_or((0, 0, 0));
            outline.model_ver = model as u16;
            outline.analysis_ver = analysis as u16;
            outline.preset_ver = preset_ver as u16;

            Ok(outline)
        })
    }

    /// The frames worth a photographer attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn needs_review(&self, project: &ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM retouch_plan
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                        AND confidence < ?2
                      ORDER BY confidence, photo_id LIMIT ?3",
                )
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut cursor = statement
                .query(params![key, f64::from(REVIEW_BELOW), count(limit)])
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

    /// Record that a photographer has looked at a plan and agrees.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5097` when the photograph has no plan.
    pub fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        let key = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE retouch_plan SET reviewed = 1 WHERE photo_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not accept the plan", &e))
        })?;
        if changed == 0 {
            return Err(errors::retouch_edit_refused(
                "that photograph has no retouch plan yet",
            ));
        }
        Ok(())
    }

    /// Record what the photographer set instead.
    ///
    /// A preset is per photograph; a strength is **gallery-wide**, because setting one person
    /// strength on one frame and not on the rest is how a gallery ends up with a bride whose
    /// skin changes character between the ceremony and the reception.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5097` when the photograph has no plan or the override is empty.
    pub fn set_override(
        &self,
        image: ImageId,
        project: &ProjectId,
        values: &RetouchOverride,
    ) -> Result<(), AuraError> {
        crate::guard::check_override(values)?;
        let key = image.to_db();
        let project_key = project.to_db();
        let stamped = aura_catalog::rfc3339(self.clock.now_utc());

        let preset_choice = values.preset;
        let identity_strength = values.identity_strength;
        let changed = self.catalog.writer().transact(move |conn| {
            let mut touched = 0usize;
            if let Some(preset) = preset_choice {
                touched += conn
                    .execute(
                        "UPDATE retouch_plan
                            SET preset = ?2, user_edited = 1, reviewed = 1
                          WHERE photo_id = ?1",
                        params![key, preset.as_str()],
                    )
                    .map_err(|e| statement_failed("could not record the preset", &e))?;
            }
            if let Some((identity, strength)) = identity_strength {
                touched += conn
                    .execute(
                        "INSERT INTO retouch_identity (
                             project_id, identity_id, strength, role, median_face_frac,
                             dominant_scene, frames, user_edited, preset_ver, updated_at
                         ) VALUES (?1, ?2, ?3, 'unknown', 0.0, '', 0, 1, 0, ?4)
                         ON CONFLICT(project_id, identity_id) DO UPDATE SET
                             strength = excluded.strength,
                             user_edited = 1,
                             updated_at = excluded.updated_at",
                        params![project_key, identity.to_db(), f64::from(strength), stamped],
                    )
                    .map_err(|e| statement_failed("could not record the strength", &e))?;
            }
            Ok(touched)
        })?;

        if changed == 0 {
            return Err(errors::retouch_edit_refused(
                "that photograph has no retouch plan yet",
            ));
        }
        Ok(())
    }

    /// Store the gallery-constant strength for one person.
    ///
    /// Never overwrites a row a photographer set: `user_edited = 0` is inside the statement.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_identity(
        &self,
        project: &ProjectId,
        stats: &IdentityStats,
        strength: f32,
        preset_ver: u16,
    ) -> AuraResult<()> {
        let project_key = project.to_db();
        let identity = stats.identity.to_db();
        let role = stats.role.as_str().to_string();
        let scene = stats.dominant_scene.as_str().to_string();
        let median = f64::from(stats.median_face_frac);
        let frames = i64::from(stats.frames);
        let strength = f64::from(strength.clamp(0.0, 1.0));
        let stamped = aura_catalog::rfc3339(self.clock.now_utc());

        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO retouch_identity (
                     project_id, identity_id, strength, role, median_face_frac,
                     dominant_scene, frames, user_edited, preset_ver, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9)
                 ON CONFLICT(project_id, identity_id) DO UPDATE SET
                     strength = excluded.strength,
                     role = excluded.role,
                     median_face_frac = excluded.median_face_frac,
                     dominant_scene = excluded.dominant_scene,
                     frames = excluded.frames,
                     preset_ver = excluded.preset_ver,
                     updated_at = excluded.updated_at
                 WHERE retouch_identity.user_edited = 0",
                params![
                    project_key,
                    identity,
                    strength,
                    role,
                    median,
                    scene,
                    frames,
                    i64::from(preset_ver),
                    stamped
                ],
            )
            .map_err(|e| statement_failed("could not store the identity strength", &e))?;
            Ok(())
        })
    }

    /// The gallery-constant strength for every person in a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn identity_strengths(&self, project: &ProjectId) -> AuraResult<BTreeMap<IdentityId, f32>> {
        let key = project.to_db();
        self.catalog
            .read(move |conn| read_identity_strengths(conn, &key))
    }

    /// Everything protected on one person.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn protected(&self, identity: IdentityId) -> AuraResult<Vec<ProtectedFeature>> {
        let key = identity.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT identity_id, kind, fx, fy, fw, fh, confidence, source, frames,
                            span_minutes, first_seen_photo
                       FROM retouch_protected WHERE identity_id = ?1
                      ORDER BY protected_id",
                )
                .map_err(|e| statement_failed("could not read the protect set", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the protect set", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a protected feature", &e))?
            {
                if let Some(feature) = decode_protected(row) {
                    out.push(feature);
                }
            }
            Ok(out)
        })
    }

    /// Add or clear one protected feature.
    ///
    /// **An absolute protection cannot be cleared**, and the refusal happens twice: here, and in
    /// the trigger migration 21 installs. A promise enforced in one layer is a promise until
    /// somebody writes a second caller.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5097` when the feature is absolute or the rectangle is empty.
    pub fn set_protection(
        &self,
        project: &ProjectId,
        feature: &ProtectedFeature,
        protect: bool,
    ) -> Result<(), AuraError> {
        crate::guard::check_protection(feature, protect)?;
        let project_key = project.to_db();
        let identity = feature.identity.to_db();
        let kind = feature.kind.as_str().to_string();
        let area = feature.area;
        let confidence = f64::from(feature.confidence.clamp(0.0, 1.0));
        let source = if protect {
            ProtectedSource::User
        } else {
            feature.source
        }
        .as_str()
        .to_string();
        let frames = i64::from(feature.frames);
        let span = f64::from(feature.span_minutes);
        let first_seen = feature.first_seen.to_db();
        let stamped = aura_catalog::rfc3339(self.clock.now_utc());
        let id = format!("prt_{}", uuid::Uuid::now_v7());

        self.catalog.writer().transact(move |conn| {
            if protect {
                conn.execute(
                    "INSERT INTO retouch_protected (
                         protected_id, identity_id, project_id, kind, fx, fy, fw, fh,
                         confidence, source, frames, span_minutes, first_seen_photo, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        id,
                        identity,
                        project_key,
                        kind,
                        f64::from(area.x),
                        f64::from(area.y),
                        f64::from(area.w),
                        f64::from(area.h),
                        confidence,
                        source,
                        frames,
                        span,
                        first_seen,
                        stamped
                    ],
                )
                .map_err(|e| statement_failed("could not protect that feature", &e))?;
            } else {
                conn.execute(
                    "DELETE FROM retouch_protected
                      WHERE identity_id = ?1 AND kind = ?2
                        AND ABS(fx - ?3) < 0.02 AND ABS(fy - ?4) < 0.02",
                    params![identity, kind, f64::from(area.x), f64::from(area.y)],
                )
                .map_err(|e| statement_failed("could not clear that protection", &e))?;
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Replace the protect rows this product wrote itself, leaving a photographer alone.
    ///
    /// `source <> 'user'` inside the `DELETE`, which is the ninth time this rule has been
    /// written into a store. A tattoo survives the delete whatever its source, because the
    /// trigger in migration 21 aborts it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn replace_protected(
        &self,
        project: &ProjectId,
        features: &[ProtectedFeature],
    ) -> AuraResult<()> {
        let project_key = project.to_db();
        let stamped = aura_catalog::rfc3339(self.clock.now_utc());
        let features = features.to_vec();
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "DELETE FROM retouch_protected
                  WHERE project_id = ?1 AND source <> 'user' AND kind <> 'tattoo'",
                params![project_key],
            )
            .map_err(|e| statement_failed("could not clear the machine protect rows", &e))?;

            for feature in &features {
                let id = format!("prt_{}", uuid::Uuid::now_v7());
                conn.execute(
                    "INSERT INTO retouch_protected (
                         protected_id, identity_id, project_id, kind, fx, fy, fw, fh,
                         confidence, source, frames, span_minutes, first_seen_photo, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        id,
                        feature.identity.to_db(),
                        project_key,
                        feature.kind.as_str(),
                        f64::from(feature.area.x),
                        f64::from(feature.area.y),
                        f64::from(feature.area.w),
                        f64::from(feature.area.h),
                        f64::from(feature.confidence),
                        feature.source.as_str(),
                        i64::from(feature.frames),
                        f64::from(feature.span_minutes),
                        feature.first_seen.to_db(),
                        stamped
                    ],
                )
                .map_err(|e| statement_failed("could not store a protected feature", &e))?;
            }
            Ok(())
        })
    }
}

struct StoredPlan {
    preset: String,
    scene: String,
    band_ratio: f64,
    texture_floor: f64,
    texture_passed: i64,
    texture_samples: i64,
    texture_resolves: i64,
    withdrawn: i64,
    budget_used: f64,
    confidence: f64,
    reasons: String,
    user_edited: i64,
    reviewed: i64,
    model_ver: i64,
    analysis_ver: i64,
    preset_ver: i64,
}

/// A count as the integer SQLite stores, saturating rather than wrapping.
fn count(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn fraction(part: i64, whole: i64) -> f32 {
    if whole <= 0 {
        return 0.0;
    }
    (part as f64 / whole as f64) as f32
}

/// Reason codes, comma separated, in emission order.
///
/// Codes rather than sentences: phase 09 rule, seventh migration running. A stored sentence is
/// copy a release can change, and a catalog full of English cannot be translated.
fn encode_reasons(reasons: &[RetouchReason]) -> String {
    reasons
        .iter()
        .map(|reason| reason.code.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_reasons(text: &str) -> Vec<RetouchReason> {
    let mut out: Vec<RetouchReason> = text
        .split(',')
        .filter(|slug| !slug.is_empty())
        .filter_map(RetouchCode::parse)
        .map(|code| RetouchReason::plain(code, 0.0))
        .collect();
    if out.is_empty() {
        // Invariant 2: a plan with no reason is a bug, and a row that lost its reasons must not
        // become one on the way back in.
        out.push(RetouchReason::plain(RetouchCode::HeadUntrained, 0.0));
    }
    out
}

fn read_identity_strengths(
    conn: &rusqlite::Connection,
    project: &str,
) -> AuraResult<BTreeMap<IdentityId, f32>> {
    let mut statement = conn
        .prepare(
            "SELECT identity_id, strength FROM retouch_identity WHERE project_id = ?1
              ORDER BY identity_id",
        )
        .map_err(|e| statement_failed("could not read the identity strengths", &e))?;
    let mut cursor = statement
        .query(params![project])
        .map_err(|e| statement_failed("could not read the identity strengths", &e))?;
    let mut out = BTreeMap::new();
    while let Some(row) = cursor
        .next()
        .map_err(|e| statement_failed("could not read an identity strength", &e))?
    {
        let text: String = row.get(0).unwrap_or_default();
        let value: f64 = row.get(1).unwrap_or(0.0);
        if let Ok(id) = IdentityId::from_db(&text) {
            out.insert(id, value as f32);
        }
    }
    Ok(out)
}

fn read_identity_strengths_for_photo(
    conn: &rusqlite::Connection,
    photo: &str,
) -> AuraResult<BTreeMap<IdentityId, f32>> {
    let project: Option<String> = conn
        .query_row(
            "SELECT project_id FROM retouch_plan WHERE photo_id = ?1",
            params![photo],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| statement_failed("could not read the project", &e))?;
    match project {
        Some(project) => read_identity_strengths(conn, &project),
        None => Ok(BTreeMap::new()),
    }
}

fn read_protected_for_photo(
    conn: &rusqlite::Connection,
    photo: &str,
) -> AuraResult<Vec<ProtectedFeature>> {
    let mut statement = conn
        .prepare(
            "SELECT identity_id, kind, fx, fy, fw, fh, confidence, source, frames,
                    span_minutes, first_seen_photo
               FROM retouch_protected
              WHERE project_id = (SELECT project_id FROM retouch_plan WHERE photo_id = ?1)
              ORDER BY protected_id",
        )
        .map_err(|e| statement_failed("could not read the protect set", &e))?;
    let mut cursor = statement
        .query(params![photo])
        .map_err(|e| statement_failed("could not read the protect set", &e))?;
    let mut out = Vec::new();
    while let Some(row) = cursor
        .next()
        .map_err(|e| statement_failed("could not read a protected feature", &e))?
    {
        if let Some(feature) = decode_protected(row) {
            out.push(feature);
        }
    }
    Ok(out)
}

fn decode_protected(row: &rusqlite::Row<'_>) -> Option<ProtectedFeature> {
    let identity: String = row.get(0).ok()?;
    let kind: String = row.get(1).ok()?;
    let first_seen: Option<String> = row.get(10).ok().flatten();
    Some(ProtectedFeature {
        identity: IdentityId::from_db(&identity).ok()?,
        kind: ProtectedKind::parse(&kind)?,
        area: Box2 {
            x: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
            y: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
            w: row.get::<_, f64>(4).unwrap_or(0.0) as f32,
            h: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
        },
        confidence: row.get::<_, f64>(6).unwrap_or(1.0) as f32,
        source: row
            .get::<_, String>(7)
            .ok()
            .and_then(|text| ProtectedSource::parse(&text))
            .unwrap_or_default(),
        frames: row.get::<_, i64>(8).unwrap_or(0) as u32,
        span_minutes: row.get::<_, f64>(9).unwrap_or(0.0) as f32,
        first_seen: first_seen
            .and_then(|text| PhotoId::from_db(&text).ok())
            .unwrap_or_default(),
    })
}

/// The role a stored strength row was computed for.
///
/// Read back so a re-computation under a new preset table can be compared with the old one, and
/// so the panel can say *why* somebody strength is what it is rather than only what it is.
#[must_use]
pub fn role_of(text: &str) -> Role {
    Role::from_str_or_unknown(text)
}
