//! The two tables migration 20 adds, and the two rules that live in the SQL rather than
//! around it.
//!
//! ## `user_edited` is re-applied inside the statement, not read before it
//!
//! A re-analysis overwrites every plan it recomputes. The upsert carries the photographer's
//! own rectangle forward from the row it is replacing, in the same statement, so an override
//! cannot be lost by a background pass that read the flag a moment earlier and wrote a moment
//! later. The ninth time this rule has been written into a store, and here the thing being
//! protected is somebody's own crop.
//!
//! ## The original framing is a row, and nothing deletes it
//!
//! `geometry_crop` ordinal zero is always `purpose = "original"`. The write path takes it from
//! `GeometryPlan::crops`, which `GeometryPlan::new` seeded and which `broken_guarantee`
//! refuses a plan without. Section 13's promise that the original framing is always one click
//! away survives a
//! round trip through `SQLite` because it is data rather than a code path.
//!
//! ## Refusals are counts
//!
//! Four columns rather than a child table. A wedding's crop search refuses on the order of two
//! hundred rectangles per photograph; storing each is 800,000 rows on a 4,000-image wedding
//! for information nobody queries across. What a photographer needs is "AURA tried and could
//! not, because of faces", and that is a number.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::geometry::{
    Aspect, CropPurpose, CropSafetyReport, CropVariant, GeometryCode, GeometryOutline,
    GeometryOverride, GeometryPlan, GeometryReason, ImageId, Keystone, LensCorrection, LensSource,
};
use aura_core::contract::integrity::CropRect;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use crate::errors;

/// Below this confidence a plan is worth a photographer's attention.
pub const REVIEW_BELOW: f32 = 0.50;

/// Measured storage per photograph, in bytes.
///
/// The plan row, its three indexes, up to eleven reasons as a document, and up to four crop
/// rows. Asserted by `crates/aura-perf/tests/geometry_budgets.rs` against a real catalog
/// rather than against this constant.
pub const BYTES_PER_IMAGE: usize = 900;

/// The `geometry_plan` and `geometry_crop` tables.
#[derive(Debug)]
pub struct GeometryStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl GeometryStore {
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
    /// 10 %, 50 % or 90 % and the next run asks the catalog what is left. A `profile_ver` or
    /// `rules_ver` bump therefore heals itself.
    ///
    /// `user_edited` rows are excluded outright rather than filtered afterwards: a
    /// photographer's own framing is not stale work, whatever version produced the row beside
    /// it.
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
                       LEFT JOIN geometry_plan g ON g.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND COALESCE(g.user_edited, 0) = 0
                        AND (g.photo_id IS NULL
                             OR g.profile_ver  <> ?2
                             OR g.analysis_ver <> ?3
                             OR g.rules_ver    <> ?4)
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

    /// Write one plan, carrying any override forward inside the statement.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)] // One upsert with thirty-seven columns, spelled out.
    pub fn put(&self, plan: &GeometryPlan) -> AuraResult<()> {
        let key = plan.image_id.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let reasons = serde_json::to_string(&plan.reasons).unwrap_or_else(|_| "[]".to_string());
        let plan = plan.clone();
        self.catalog.writer().transact(move |conn| {
            // The photographer's own rectangle and the two bits that protect it, read and
            // rewritten inside the same statement. See the module header.
            let existing: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT user_edited, reviewed FROM geometry_plan WHERE photo_id = ?1",
                    params![key],
                    |row| Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0))),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the existing plan", &e))?;
            let (user_edited, reviewed) = existing.unwrap_or((0, 0));
            if user_edited == 1 {
                // Somebody has framed this themselves. Nothing here overwrites it, and the
                // check is inside the transaction rather than before it.
                return Ok(());
            }

            let keystone = plan.keystone;
            conn.execute(
                "INSERT INTO geometry_plan (
                     photo_id, scene, rules_row, frame_aspect,
                     lens_source, lens_id, lens_profile, k1, k2, k3, vignette, ca_red, ca_blue,
                     rotate_deg, rotate_conf,
                     keystone_v, keystone_h, keystone_scale, keystone_stretch,
                     keystone_verticals,
                     primary_ordinal,
                     faces_intact, resolution_ok, content_kept, faces_checked, hands_checked,
                     refused_face, refused_hands, refused_small, refused_content,
                     reasons, confidence,
                     profile_ver, analysis_ver, rules_ver, user_edited, reviewed, planned_at
                 ) VALUES (
                     ?1, ?2, ?3, ?4,
                     ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?15,
                     ?16, ?17, ?18, ?19, ?20,
                     ?21,
                     ?22, ?23, ?24, ?25, ?26,
                     ?27, ?28, ?29, ?30,
                     ?31, ?32,
                     ?33, ?34, ?35, 0, ?36, ?37
                 )
                 ON CONFLICT(photo_id) DO UPDATE SET
                     scene = excluded.scene, rules_row = excluded.rules_row,
                     frame_aspect = excluded.frame_aspect,
                     lens_source = excluded.lens_source, lens_id = excluded.lens_id,
                     lens_profile = excluded.lens_profile,
                     k1 = excluded.k1, k2 = excluded.k2, k3 = excluded.k3,
                     vignette = excluded.vignette,
                     ca_red = excluded.ca_red, ca_blue = excluded.ca_blue,
                     rotate_deg = excluded.rotate_deg, rotate_conf = excluded.rotate_conf,
                     keystone_v = excluded.keystone_v, keystone_h = excluded.keystone_h,
                     keystone_scale = excluded.keystone_scale,
                     keystone_stretch = excluded.keystone_stretch,
                     keystone_verticals = excluded.keystone_verticals,
                     primary_ordinal = excluded.primary_ordinal,
                     faces_intact = excluded.faces_intact,
                     resolution_ok = excluded.resolution_ok,
                     content_kept = excluded.content_kept,
                     faces_checked = excluded.faces_checked,
                     hands_checked = excluded.hands_checked,
                     refused_face = excluded.refused_face,
                     refused_hands = excluded.refused_hands,
                     refused_small = excluded.refused_small,
                     refused_content = excluded.refused_content,
                     reasons = excluded.reasons, confidence = excluded.confidence,
                     profile_ver = excluded.profile_ver,
                     analysis_ver = excluded.analysis_ver,
                     rules_ver = excluded.rules_ver,
                     planned_at = excluded.planned_at
                 WHERE geometry_plan.user_edited = 0",
                params![
                    key,
                    plan.scene.as_str(),
                    1_i64,
                    1.5_f64,
                    plan.lens.source.as_str(),
                    plan.lens.lens_id.clone(),
                    plan.lens.profile_id.clone(),
                    f64::from(plan.lens.distortion[0]),
                    f64::from(plan.lens.distortion[1]),
                    f64::from(plan.lens.distortion[2]),
                    f64::from(plan.lens.vignette),
                    f64::from(plan.lens.ca[0]),
                    f64::from(plan.lens.ca[1]),
                    f64::from(plan.rotate_deg),
                    f64::from(plan.rotate_conf),
                    keystone.map(|k| f64::from(k.vertical)),
                    keystone.map(|k| f64::from(k.horizontal)),
                    keystone.map(|k| f64::from(k.scale)),
                    keystone.map(|k| f64::from(k.stretch)),
                    i64::from(keystone.map_or(0, |k| k.verticals)),
                    i64::try_from(plan.primary_crop).unwrap_or(0),
                    i64::from(plan.safety.faces_intact),
                    i64::from(plan.safety.resolution_ok),
                    i64::from(plan.safety.content_kept),
                    i64::from(plan.safety.faces_checked),
                    i64::from(plan.safety.hands_checked),
                    i64::from(plan.safety.refused[0]),
                    i64::from(plan.safety.refused[1]),
                    i64::from(plan.safety.refused[2]),
                    i64::from(plan.safety.refused[3]),
                    reasons,
                    f64::from(plan.confidence),
                    i64::from(plan.profile_ver),
                    i64::from(plan.analysis_ver),
                    i64::from(plan.rules_ver),
                    reviewed,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not write the geometry plan", &e))?;

            conn.execute(
                "DELETE FROM geometry_crop WHERE photo_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear the crops", &e))?;
            for (ordinal, variant) in plan.crops.iter().enumerate() {
                conn.execute(
                    "INSERT INTO geometry_crop
                         (photo_id, ordinal, purpose, aspect, x, y, w, h, score, safe)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        key,
                        i64::try_from(ordinal).unwrap_or(0),
                        variant.purpose.as_str(),
                        variant.aspect.as_str(),
                        f64::from(variant.rect.x),
                        f64::from(variant.rect.y),
                        f64::from(variant.rect.w.max(1e-4)),
                        f64::from(variant.rect.h.max(1e-4)),
                        f64::from(variant.score.clamp(0.0, 1.0)),
                        i64::from(variant.safe),
                    ],
                )
                .map_err(|e| statement_failed("could not write a crop variant", &e))?;
            }
            Ok(())
        })
    }

    /// Read one plan.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn get(&self, image: ImageId) -> AuraResult<Option<GeometryPlan>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let row: Option<GeometryPlan> = conn
                .query_row(
                    "SELECT scene, lens_source, lens_id, lens_profile, k1, k2, k3, vignette,
                            ca_red, ca_blue, rotate_deg, rotate_conf,
                            keystone_v, keystone_h, keystone_scale, keystone_stretch,
                            keystone_verticals, primary_ordinal,
                            faces_intact, resolution_ok, content_kept, faces_checked,
                            hands_checked, refused_face, refused_hands, refused_small,
                            refused_content, reasons, confidence,
                            profile_ver, analysis_ver, rules_ver, user_edited
                       FROM geometry_plan WHERE photo_id = ?1",
                    params![key],
                    |row| {
                        let scene = SceneId::from_str_or_unknown(
                            &row.get::<_, String>(0).unwrap_or_default(),
                        );
                        let mut plan = GeometryPlan::new(image, scene);
                        plan.lens = LensCorrection {
                            source: LensSource::from_str_or_none(
                                &row.get::<_, String>(1).unwrap_or_default(),
                            ),
                            lens_id: row.get(2).ok().flatten(),
                            profile_id: row.get(3).ok().flatten(),
                            distortion: [
                                row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                            ],
                            vignette: row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                            ca: [
                                row.get::<_, f64>(8).unwrap_or(1.0) as f32,
                                row.get::<_, f64>(9).unwrap_or(1.0) as f32,
                            ],
                        };
                        plan.rotate_deg = row.get::<_, f64>(10).unwrap_or(0.0) as f32;
                        plan.rotate_conf = row.get::<_, f64>(11).unwrap_or(0.0) as f32;
                        let vertical: Option<f64> = row.get(12).ok().flatten();
                        plan.keystone = vertical.and_then(|v| {
                            Keystone::new(
                                v as f32,
                                row.get::<_, f64>(13).unwrap_or(0.0) as f32,
                                row.get::<_, f64>(14).unwrap_or(1.0) as f32,
                                row.get::<_, f64>(15).unwrap_or(1.0) as f32,
                                row.get::<_, i64>(16).unwrap_or(0) as u16,
                            )
                            .ok()
                        });
                        plan.primary_crop = row.get::<_, i64>(17).unwrap_or(0).max(0) as usize;
                        plan.safety = CropSafetyReport {
                            faces_intact: row.get::<_, i64>(18).unwrap_or(1) == 1,
                            resolution_ok: row.get::<_, i64>(19).unwrap_or(1) == 1,
                            content_kept: row.get::<_, i64>(20).unwrap_or(1) == 1,
                            faces_checked: row.get::<_, i64>(21).unwrap_or(0).max(0) as u32,
                            hands_checked: row.get::<_, i64>(22).unwrap_or(0).max(0) as u32,
                            refused: [
                                row.get::<_, i64>(23).unwrap_or(0).max(0) as u32,
                                row.get::<_, i64>(24).unwrap_or(0).max(0) as u32,
                                row.get::<_, i64>(25).unwrap_or(0).max(0) as u32,
                                row.get::<_, i64>(26).unwrap_or(0).max(0) as u32,
                            ],
                        };
                        plan.reasons = serde_json::from_str::<Vec<GeometryReason>>(
                            &row.get::<_, String>(27).unwrap_or_default(),
                        )
                        .unwrap_or_default();
                        plan.confidence = row.get::<_, f64>(28).unwrap_or(0.0) as f32;
                        plan.profile_ver = row.get::<_, i64>(29).unwrap_or(0).max(0) as u16;
                        plan.analysis_ver = row.get::<_, i64>(30).unwrap_or(0).max(0) as u16;
                        plan.rules_ver = row.get::<_, i64>(31).unwrap_or(0).max(0) as u16;
                        plan.user_edited = row.get::<_, i64>(32).unwrap_or(0) == 1;
                        Ok(plan)
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the geometry plan", &e))?;
            let Some(mut plan) = row else {
                return Ok(None);
            };
            plan.crops = read_crops(conn, &key)?;
            if plan.crops.is_empty() {
                // A plan whose crops did not survive is a plan whose original framing is gone,
                // which the contract's own constructor makes impossible. Rebuild it rather
                // than hand a caller a plan with nothing to deliver.
                plan.crops = vec![CropVariant::original()];
                plan.primary_crop = 0;
            }
            if plan.primary_crop >= plan.crops.len() {
                plan.primary_crop = 0;
            }
            if plan.reasons.is_empty() {
                plan.reasons = vec![GeometryReason::plain(GeometryCode::Clean, 0.0)];
            }
            Ok(Some(plan))
        })
    }

    /// One purpose's rectangle, without decoding the whole plan.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn variant(
        &self,
        image: ImageId,
        purpose: CropPurpose,
    ) -> AuraResult<Option<CropVariant>> {
        let key = image.to_db();
        let want = purpose.as_str();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT purpose, aspect, x, y, w, h, score, safe
                   FROM geometry_crop WHERE photo_id = ?1 AND purpose = ?2",
                params![key, want],
                |row| Ok(crop_from_row(row)),
            )
            .optional()
            .map_err(|e| statement_failed("could not read the crop variant", &e))
        })
    }

    /// The project's outline.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    #[allow(clippy::too_many_lines)] // Five aggregates over two tables, each named.
    pub fn outline(
        &self,
        project: &ProjectId,
        versions: (u16, u16, u16),
    ) -> AuraResult<GeometryOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut outline = GeometryOutline {
                profile_ver: versions.0,
                analysis_ver: versions.1,
                rules_ver: versions.2,
                ..GeometryOutline::default()
            };
            let row = conn
                .query_row(
                    "SELECT images, planned, kept_original, profile_covered, levelled,
                            keystoned, user_edited, needs_review
                       FROM v_geometry_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0).unwrap_or(0),
                            row.get::<_, i64>(1).unwrap_or(0),
                            row.get::<_, i64>(2).unwrap_or(0),
                            row.get::<_, i64>(3).unwrap_or(0),
                            row.get::<_, i64>(4).unwrap_or(0),
                            row.get::<_, i64>(5).unwrap_or(0),
                            row.get::<_, i64>(6).unwrap_or(0),
                            row.get::<_, i64>(7).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the geometry outline", &e))?;
            let Some((images, planned, kept, profiled, levelled, keystoned, edited, review)) = row
            else {
                return Ok(outline);
            };
            outline.photos = images.max(0) as u32;
            outline.planned = planned.max(0) as u32;
            outline.coverage = ratio(planned, images);
            outline.kept_original = ratio(kept, planned);
            outline.profile_covered = ratio(profiled, planned);
            outline.levelled = levelled.max(0) as u32;
            outline.keystoned = keystoned.max(0) as u32;
            outline.user_edited = edited.max(0) as u32;
            outline.needs_review = review.max(0) as u32;

            outline.mean_rotate_deg = conn
                .query_row(
                    "SELECT AVG(ABS(g.rotate_deg)) FROM geometry_plan g
                       JOIN photo p ON p.photo_id = g.photo_id
                      WHERE p.project_id = ?1 AND g.rotate_deg <> 0.0",
                    params![key],
                    |row| Ok(row.get::<_, Option<f64>>(0).unwrap_or(None).unwrap_or(0.0)),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the mean rotation", &e))?
                .unwrap_or(0.0) as f32;

            let refused = conn
                .query_row(
                    "SELECT SUM(g.refused_face), SUM(g.refused_hands),
                            SUM(g.refused_small), SUM(g.refused_content)
                       FROM geometry_plan g
                       JOIN photo p ON p.photo_id = g.photo_id
                      WHERE p.project_id = ?1",
                    params![key],
                    |row| {
                        Ok([
                            row.get::<_, Option<i64>>(0).unwrap_or(None).unwrap_or(0),
                            row.get::<_, Option<i64>>(1).unwrap_or(None).unwrap_or(0),
                            row.get::<_, Option<i64>>(2).unwrap_or(None).unwrap_or(0),
                            row.get::<_, Option<i64>>(3).unwrap_or(None).unwrap_or(0),
                        ])
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the refusal histogram", &e))?
                .unwrap_or([0; 4]);
            for (slot, value) in outline.refused_histogram.iter_mut().zip(refused.iter()) {
                *slot = (*value).max(0) as u32;
            }

            let mut variants = conn
                .prepare(
                    "SELECT c.purpose, COUNT(*) FROM geometry_crop c
                       JOIN photo p ON p.photo_id = c.photo_id
                      WHERE p.project_id = ?1 GROUP BY c.purpose",
                )
                .map_err(|e| statement_failed("could not read the variant histogram", &e))?;
            let mut cursor = variants
                .query(params![key])
                .map_err(|e| statement_failed("could not read the variant histogram", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the variant histogram", &e))?
            {
                let purpose =
                    CropPurpose::from_str_or_original(&row.get::<_, String>(0).unwrap_or_default());
                let count = row.get::<_, i64>(1).unwrap_or(0).max(0) as u32;
                if let Some(index) = CropPurpose::ALL.iter().position(|p| *p == purpose) {
                    if let Some(slot) = outline.variant_histogram.get_mut(index) {
                        *slot = count;
                    }
                }
            }
            drop(cursor);
            drop(variants);

            let mut missing = conn
                .prepare(
                    "SELECT g.lens_id, COUNT(*) AS n FROM geometry_plan g
                       JOIN photo p ON p.photo_id = g.photo_id
                      WHERE p.project_id = ?1 AND g.lens_source = 'none'
                        AND g.lens_id IS NOT NULL
                      GROUP BY g.lens_id ORDER BY n DESC, g.lens_id LIMIT ?2",
                )
                .map_err(|e| statement_failed("could not read the missing profiles", &e))?;
            let mut cursor = missing
                .query(params![key, i64::try_from(GeometryOutline::MAX_MISSING).unwrap_or(20)])
                .map_err(|e| statement_failed("could not read the missing profiles", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the missing profiles", &e))?
            {
                outline.missing_profiles.push(row.get(0).unwrap_or_default());
            }
            Ok(outline)
        })
    }

    /// Frames whose plan is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn needs_review(&self, project: &ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        let key = project.to_db();
        let limit = i64::try_from(limit.clamp(1, 5_000)).unwrap_or(200);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT g.photo_id FROM geometry_plan g
                       JOIN photo p ON p.photo_id = g.photo_id
                      WHERE p.project_id = ?1 AND g.reviewed = 0 AND g.user_edited = 0
                        AND g.confidence < ?2
                      ORDER BY g.confidence, g.photo_id LIMIT ?3",
                )
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key, f64::from(REVIEW_BELOW), limit])
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the review queue", &e))?
            {
                if let Ok(id) = PhotoId::from_db(&row.get::<_, String>(0).unwrap_or_default()) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Record the framing a photographer chose.
    ///
    /// **The override replaces the primary crop and never the original.** Ordinal zero stays
    /// exactly where it is, which is what makes "revert to original" a click rather than a
    /// re-analysis, and what makes reverting itself an ordinary override rather than a delete.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5091` when the photograph has no plan.
    pub fn set_framing(&self, over: GeometryOverride) -> AuraResult<()> {
        let key = over.image_id.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM geometry_plan WHERE photo_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if exists == 0 {
                return Ok(0usize);
            }
            conn.execute(
                "UPDATE geometry_plan
                    SET user_edited = 1, reviewed = 1, rotate_deg = ?2,
                        rotate_conf = MAX(rotate_conf, ?3)
                  WHERE photo_id = ?1",
                params![key, f64::from(over.rotate_deg), f64::from(1.0_f32)],
            )
            .map_err(|e| statement_failed("could not record the framing", &e))?;
            // The photographer's rectangle becomes the primary. Ordinal zero is untouched.
            conn.execute(
                "DELETE FROM geometry_crop WHERE photo_id = ?1 AND purpose = 'primary'",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear the primary crop", &e))?;
            let ordinal: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM geometry_crop WHERE photo_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(1);
            conn.execute(
                "INSERT INTO geometry_crop
                     (photo_id, ordinal, purpose, aspect, x, y, w, h, score, safe)
                 VALUES (?1, ?2, 'primary', ?3, ?4, ?5, ?6, ?7, 0.0, 1)",
                params![
                    key,
                    ordinal.min(5),
                    over.aspect.as_str(),
                    f64::from(over.rect.x),
                    f64::from(over.rect.y),
                    f64::from(over.rect.w.max(1e-4)),
                    f64::from(over.rect.h.max(1e-4)),
                ],
            )
            .map_err(|e| statement_failed("could not record the framing", &e))?;
            conn.execute(
                "UPDATE geometry_plan SET primary_ordinal = ?2 WHERE photo_id = ?1",
                params![key, ordinal.min(5)],
            )
            .map_err(|e| statement_failed("could not point at the framing", &e))?;
            Ok(1usize)
        })?;
        if changed == 0 {
            return Err(errors::framing_refused(
                "the photograph has no geometry plan to override",
            ));
        }
        Ok(())
    }

    /// Record that the photographer has looked at one plan and agrees.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5091` when the photograph has no plan.
    pub fn accept(&self, image: ImageId) -> AuraResult<()> {
        let key = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE geometry_plan SET reviewed = 1 WHERE photo_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not accept the plan", &e))
        })?;
        if changed == 0 {
            return Err(errors::framing_refused("the photograph has no geometry plan"));
        }
        Ok(())
    }

    /// Scenes in a project that were planned with no rules row.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn unpolicied(&self, project: &ProjectId) -> AuraResult<Vec<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT DISTINCT g.scene FROM geometry_plan g
                       JOIN photo p ON p.photo_id = g.photo_id
                      WHERE p.project_id = ?1 AND g.rules_row = 0 ORDER BY g.scene",
                )
                .map_err(|e| statement_failed("could not read the unpolicied scenes", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the unpolicied scenes", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the unpolicied scenes", &e))?
            {
                out.push(row.get::<_, String>(0).unwrap_or_default());
            }
            Ok(out)
        })
    }

    /// Report a version boundary a comparison would have crossed.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5090` when stored plans span more than one version triple.
    pub fn check_versions(
        &self,
        project: &ProjectId,
        current: (u16, u16, u16),
    ) -> AuraResult<()> {
        let key = project.to_db();
        let stale: Option<(i64, i64, i64)> = self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT g.profile_ver, g.analysis_ver, g.rules_ver FROM geometry_plan g
                   JOIN photo p ON p.photo_id = g.photo_id
                  WHERE p.project_id = ?1
                    AND (g.profile_ver <> ?2 OR g.analysis_ver <> ?3 OR g.rules_ver <> ?4)
                  LIMIT 1",
                params![
                    key,
                    i64::from(current.0),
                    i64::from(current.1),
                    i64::from(current.2)
                ],
                |row| {
                    Ok((
                        row.get(0).unwrap_or(0),
                        row.get(1).unwrap_or(0),
                        row.get(2).unwrap_or(0),
                    ))
                },
            )
            .optional()
            .map_err(|e| statement_failed("could not check the geometry versions", &e))
        })?;
        match stale {
            None => Ok(()),
            Some((a, b, c)) => Err(errors::versions_moved(
                (a.max(0) as u16, b.max(0) as u16, c.max(0) as u16),
                current,
            )),
        }
    }
}

fn read_crops(conn: &rusqlite::Connection, key: &str) -> AuraResult<Vec<CropVariant>> {
    let mut statement = conn
        .prepare(
            "SELECT purpose, aspect, x, y, w, h, score, safe FROM geometry_crop
              WHERE photo_id = ?1 ORDER BY ordinal",
        )
        .map_err(|e| statement_failed("could not read the crops", &e))?;
    let mut out = Vec::new();
    let mut cursor = statement
        .query(params![key])
        .map_err(|e| statement_failed("could not read the crops", &e))?;
    while let Some(row) = cursor
        .next()
        .map_err(|e| statement_failed("could not read the crops", &e))?
    {
        out.push(crop_from_row(row));
    }
    Ok(out)
}

fn crop_from_row(row: &rusqlite::Row<'_>) -> CropVariant {
    CropVariant {
        purpose: CropPurpose::from_str_or_original(&row.get::<_, String>(0).unwrap_or_default()),
        aspect: Aspect::from_str_or_original(&row.get::<_, String>(1).unwrap_or_default()),
        rect: CropRect {
            x: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
            y: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
            w: row.get::<_, f64>(4).unwrap_or(1.0) as f32,
            h: row.get::<_, f64>(5).unwrap_or(1.0) as f32,
        },
        score: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
        safe: row.get::<_, i64>(7).unwrap_or(1) == 1,
    }
}

fn ratio(numerator: i64, denominator: i64) -> f32 {
    if denominator <= 0 {
        0.0
    } else {
        (numerator.max(0) as f32 / denominator as f32).clamp(0.0, 1.0)
    }
}

/// Scene counts, for the gate's summary.
///
/// # Errors
///
/// `AURA-DB-3006` when the query fails.
pub fn scene_histogram(catalog: &Catalog, project: &ProjectId) -> AuraResult<BTreeMap<String, u32>> {
    let key = project.to_db();
    catalog.read(move |conn| {
        let mut statement = conn
            .prepare(
                "SELECT g.scene, COUNT(*) FROM geometry_plan g
                   JOIN photo p ON p.photo_id = g.photo_id
                  WHERE p.project_id = ?1 GROUP BY g.scene ORDER BY g.scene",
            )
            .map_err(|e| statement_failed("could not read the scene histogram", &e))?;
        let mut out = BTreeMap::new();
        let mut cursor = statement
            .query(params![key])
            .map_err(|e| statement_failed("could not read the scene histogram", &e))?;
        while let Some(row) = cursor
            .next()
            .map_err(|e| statement_failed("could not read the scene histogram", &e))?
        {
            out.insert(
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, i64>(1).unwrap_or(0).max(0) as u32,
            );
        }
        Ok(out)
    })
}
