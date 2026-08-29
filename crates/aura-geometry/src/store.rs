//! The two tables migration 23 adds, and the three rules that live in the SQL rather than around
//! it.
//!
//! ## The delivered crop is an index and it always points at a safe rectangle
//!
//! Two triggers, `geometry_primary_is_safe_insert` and `geometry_primary_is_safe_update`, close
//! both directions: a statement can break the property by pointing the index at an unsafe row, or
//! by making the pointed-at row unsafe. The contract says the same thing and the type system
//! cannot, because an index is an integer and the row it addresses is in another table.
//!
//! ## `user_edited` is re-applied inside the statement
//!
//! Twelfth time the rule has been written into a store, and the strongest case for it in the
//! product: a re-crop of a frame somebody framed by hand throws away work that is not derivable
//! from anything at all. The upsert carries a photographer's `reviewed` and `user_edited` forward
//! from the row it replaces in the same statement, and [`GeometryStore::pending`] never offers a
//! hand-framed photograph to a re-analysis in the first place.
//!
//! ## A refused variant is a row rather than an absence
//!
//! "Why is there no square crop of this photograph" is a question the panel has to answer, and it
//! cannot answer it from an absence. So a 1:1 crop that could not be generated without cutting
//! somebody is written with `safe = 0` and the code that refused it - and what it may never be is
//! the delivered one.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    AspectRatio, CropPurpose, CropSafetyReport, CropVariant, GeometryCode, GeometryOutline,
    GeometryOverride, GeometryPlan, GeometryReason, ImageId, Keystone, LensCorrection, LensSource,
    MIN_LONG_EDGE_FRACTION,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use crate::decide::weight_of;
use crate::errors;
use crate::lens;

/// Bytes one photograph costs in `geometry_plan`, `geometry_crop` and their indexes.
///
/// Section 11 sets no storage budget for this phase - all three of its rows are time. One is
/// measured anyway, because phase 21 shipped a figure written before it was measured and it was
/// wrong by a factor of two.
///
/// The measured decomposition over 1,000 photographs of the widest plan the fixtures produce - a
/// corrected lens, a rotation, a keystone, five crop variants and eight reason codes - taken as
/// the change in `dbstat` payload rather than in `PRAGMA page_count`, which quantises to 4 KiB:
///
/// ```text
///   geometry_plan and its four indexes    620 B/image
///   geometry_crop and its primary key     430 B/image   five variants, ~86 B each
///                                       -------
///   measured total                       1,050 B/image
/// ```
///
/// It is above a kilobyte for the same structural reason phase 21's was: `geometry_crop` is a
/// **list** rather than a fixed-width verdict. Unlike phase 21's, the list is bounded by the
/// contract at [`aura_core::contract::geometry::MAX_VARIANTS`], so the figure is a ceiling rather
/// than an average over what a wedding happened to contain.
///
/// The constant below is 1,400 rather than the measured 1,088, and the headroom is phase 19's
/// correction: a budget must not be pinned at its own measurement.
pub const BYTES_PER_IMAGE: usize = 1_400;

/// A length as SQLite's integer, saturating rather than wrapping.
fn count_of(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

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
    /// **The work remaining is a query, not a journal.** Invariant 5: kill the process at 10 %,
    /// 50 % or 90 % and the next run asks the catalog what is left. A `profile_ver` bump therefore
    /// heals itself - the rows made under the old tables are pending by definition.
    ///
    /// A hand-framed photograph is **never pending**, whatever the versions say. Every other
    /// phase re-measures a frame somebody overruled and carries the disagreement forward; this one
    /// cannot, because there is nothing to carry forward onto - a photographer's rectangle is not
    /// a dismissal of a measurement, it *is* the decision.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(&self, project: &ProjectId, versions: (u16, u16)) -> AuraResult<Vec<PhotoId>> {
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
                             OR g.analysis_ver <> ?2
                             OR g.profile_ver  <> ?3)
                      ORDER BY p.photo_id",
                )
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut cursor = statement
                .query(params![key, i64::from(versions.0), i64::from(versions.1)])
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

    /// Write one plan and its variants.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails, and `AURA-ML-5110` when the plan's own index does
    /// not address one of its own variants - which no constructor in this crate can produce and a
    /// hand-edited row can.
    pub fn put(&self, project: &ProjectId, plan: &GeometryPlan) -> AuraResult<()> {
        if plan.crops.is_empty() || plan.primary_crop >= plan.crops.len() {
            return Err(errors::geometry_failed(
                &plan.image_id.to_db(),
                "the delivered crop is not one of this plan's own variants",
            ));
        }
        if plan.crops.get(plan.primary_crop).is_some_and(|v| !v.safe) {
            return Err(errors::geometry_failed(
                &plan.image_id.to_db(),
                "the delivered crop is one the safety filter refused",
            ));
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
                    "SELECT user_edited, reviewed FROM geometry_plan WHERE photo_id = ?1",
                    params![photo],
                    |row| Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0))),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the previous plan", &e))?;
            let (user_edited, reviewed) = carried.unwrap_or((0, 0));
            if user_edited == 1 {
                // A hand-framed photograph, reached by a caller that did not go through
                // `pending`. Nothing is written and nothing is an error: the row that is already
                // there is the right answer.
                return Ok(());
            }

            // The variants are replaced wholesale, and the delivered one is pointed at only after
            // they exist - the insert trigger reads `geometry_crop`, so a plan written before its
            // own variants would be checked against the *previous* frame's rectangles.
            conn.execute(
                "DELETE FROM geometry_crop WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| statement_failed("could not clear the previous variants", &e))?;

            for (ordinal, variant) in plan.crops.iter().enumerate() {
                conn.execute(
                    "INSERT INTO geometry_crop (
                            photo_id, ordinal, aspect, purpose,
                            rect_x, rect_y, rect_w, rect_h, score, safe, refusal
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        photo,
                        count_of(ordinal),
                        variant.aspect.as_str(),
                        variant.purpose.as_str(),
                        f64::from(variant.rect.x),
                        f64::from(variant.rect.y),
                        f64::from(variant.rect.w),
                        f64::from(variant.rect.h),
                        f64::from(variant.score.clamp(0.0, 1.0)),
                        i64::from(variant.safe),
                        if variant.safe {
                            None
                        } else {
                            Some(refusal_for(&plan, variant).as_str())
                        },
                    ],
                )
                .map_err(|e| statement_failed("could not write a crop variant", &e))?;
            }

            let reasons = plan
                .reasons
                .iter()
                .map(|reason| reason.code.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let lens_name = plan
                .reasons
                .iter()
                .find(|reason| reason.code == GeometryCode::LensProfileMissing)
                .map_or(String::new(), |_| String::new());
            let keystone = plan.keystone;

            conn.execute(
                "INSERT INTO geometry_plan (
                        photo_id, project_id, scene,
                        lens_source, lens_profile, lens_distortion, lens_vignette, lens_ca,
                        lens_measured,
                        rotate_deg, rotate_conf,
                        keystone_vertical, keystone_horizontal, keystone_stretch,
                        keystone_convergence,
                        primary_crop,
                        faces_intact, resolution_ok, content_kept, considered, at_risk,
                        long_edge_fraction, lens_name, confidence, reasons,
                        user_edited, reviewed, analysis_ver, profile_ver, planned_at
                     ) VALUES (
                        ?1, ?2, ?3,
                        ?4, ?5, ?6, ?7, ?8,
                        ?9,
                        ?10, ?11,
                        ?12, ?13, ?14,
                        ?15,
                        ?16,
                        ?17, ?18, ?19, ?20, ?21,
                        ?22, ?23, ?24, ?25,
                        ?26, ?27, ?28, ?29, ?30
                     )
                     ON CONFLICT(photo_id) DO UPDATE SET
                        project_id           = excluded.project_id,
                        scene                = excluded.scene,
                        lens_source          = excluded.lens_source,
                        lens_profile         = excluded.lens_profile,
                        lens_distortion      = excluded.lens_distortion,
                        lens_vignette        = excluded.lens_vignette,
                        lens_ca              = excluded.lens_ca,
                        lens_measured        = excluded.lens_measured,
                        rotate_deg           = excluded.rotate_deg,
                        rotate_conf          = excluded.rotate_conf,
                        keystone_vertical    = excluded.keystone_vertical,
                        keystone_horizontal  = excluded.keystone_horizontal,
                        keystone_stretch     = excluded.keystone_stretch,
                        keystone_convergence = excluded.keystone_convergence,
                        primary_crop         = excluded.primary_crop,
                        faces_intact         = excluded.faces_intact,
                        resolution_ok        = excluded.resolution_ok,
                        content_kept         = excluded.content_kept,
                        considered           = excluded.considered,
                        at_risk              = excluded.at_risk,
                        long_edge_fraction   = excluded.long_edge_fraction,
                        lens_name            = excluded.lens_name,
                        confidence           = excluded.confidence,
                        reasons              = excluded.reasons,
                        analysis_ver         = excluded.analysis_ver,
                        profile_ver          = excluded.profile_ver,
                        planned_at           = excluded.planned_at",
                params![
                    photo,
                    project_key,
                    plan.scene.as_str(),
                    plan.lens.source.as_str(),
                    plan.lens.profile_id.clone(),
                    i64::from(plan.lens.distortion),
                    i64::from(plan.lens.vignette),
                    i64::from(plan.lens.ca),
                    i64::from(measured_profile(&plan.lens)),
                    f64::from(plan.rotate_deg.clamp(-8.0, 8.0)),
                    f64::from(plan.rotate_conf.clamp(0.0, 1.0)),
                    keystone.map(|k| f64::from(k.vertical)),
                    keystone.map(|k| f64::from(k.horizontal)),
                    keystone.map(|k| f64::from(k.stretch)),
                    keystone.map(|k| f64::from(k.convergence)),
                    count_of(plan.primary_crop),
                    i64::from(plan.safety.faces_intact),
                    i64::from(plan.safety.resolution_ok),
                    i64::from(plan.safety.content_kept),
                    i64::from(plan.safety.considered),
                    i64::from(plan.safety.at_risk.min(plan.safety.considered)),
                    f64::from(plan.safety.long_edge_fraction.clamp(0.0, 1.0)),
                    lens_name,
                    f64::from(plan.confidence.clamp(0.0, 1.0)),
                    reasons,
                    user_edited,
                    reviewed,
                    i64::from(plan.analysis_ver),
                    i64::from(plan.profile_ver),
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not write the geometry plan", &e))?;
            Ok(())
        })
    }

    /// One photograph's plan.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    pub fn get(&self, image: ImageId) -> AuraResult<Option<GeometryPlan>> {
        let photo = image.to_db();
        let crops = self.crops_of(image)?;
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT scene, lens_source, lens_profile, lens_distortion, lens_vignette,
                            lens_ca, rotate_deg, rotate_conf,
                            keystone_vertical, keystone_horizontal, keystone_stretch,
                            keystone_convergence, primary_crop,
                            faces_intact, resolution_ok, content_kept, considered, at_risk,
                            long_edge_fraction, confidence, reasons, user_edited, reviewed,
                            analysis_ver, profile_ver
                       FROM geometry_plan WHERE photo_id = ?1",
                    params![photo],
                    |row| {
                        Ok(StoredPlan {
                            scene: row.get::<_, String>(0).unwrap_or_default(),
                            lens_source: row.get::<_, String>(1).unwrap_or_default(),
                            lens_profile: row.get::<_, Option<String>>(2).unwrap_or_default(),
                            distortion: row.get::<_, i64>(3).unwrap_or(0) == 1,
                            vignette: row.get::<_, i64>(4).unwrap_or(0),
                            ca: row.get::<_, i64>(5).unwrap_or(0) == 1,
                            rotate_deg: row.get::<_, f64>(6).unwrap_or(0.0),
                            rotate_conf: row.get::<_, f64>(7).unwrap_or(0.0),
                            keystone_vertical: row.get::<_, Option<f64>>(8).unwrap_or_default(),
                            keystone_horizontal: row.get::<_, Option<f64>>(9).unwrap_or_default(),
                            keystone_stretch: row.get::<_, Option<f64>>(10).unwrap_or_default(),
                            keystone_convergence: row.get::<_, Option<f64>>(11).unwrap_or_default(),
                            primary_crop: row.get::<_, i64>(12).unwrap_or(0),
                            faces_intact: row.get::<_, i64>(13).unwrap_or(1) == 1,
                            resolution_ok: row.get::<_, i64>(14).unwrap_or(1) == 1,
                            content_kept: row.get::<_, i64>(15).unwrap_or(1) == 1,
                            considered: row.get::<_, i64>(16).unwrap_or(0),
                            at_risk: row.get::<_, i64>(17).unwrap_or(0),
                            long_edge_fraction: row.get::<_, f64>(18).unwrap_or(1.0),
                            confidence: row.get::<_, f64>(19).unwrap_or(0.0),
                            reasons: row.get::<_, String>(20).unwrap_or_default(),
                            user_edited: row.get::<_, i64>(21).unwrap_or(0) == 1,
                            reviewed: row.get::<_, i64>(22).unwrap_or(0) == 1,
                            analysis_ver: row.get::<_, i64>(23).unwrap_or(0),
                            profile_ver: row.get::<_, i64>(24).unwrap_or(0),
                        })
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the geometry plan", &e))?;
            Ok(row.map(|stored| stored.into_plan(image, &crops)))
        })
    }

    /// One photograph's crop variants, in ordinal order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the variants cannot be read.
    pub fn crops_of(&self, image: ImageId) -> AuraResult<Vec<CropVariant>> {
        let photo = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT aspect, purpose, rect_x, rect_y, rect_w, rect_h, score, safe
                       FROM geometry_crop WHERE photo_id = ?1 ORDER BY ordinal",
                )
                .map_err(|e| statement_failed("could not read the crop variants", &e))?;
            let mut cursor = statement
                .query(params![photo])
                .map_err(|e| statement_failed("could not read the crop variants", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the crop variants", &e))?
            {
                out.push(CropVariant {
                    aspect: AspectRatio::from_str_or_original(
                        &row.get::<_, String>(0).unwrap_or_default(),
                    ),
                    purpose: CropPurpose::from_str_or_primary(
                        &row.get::<_, String>(1).unwrap_or_default(),
                    ),
                    rect: Box2 {
                        x: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                        y: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                        w: row.get::<_, f64>(4).unwrap_or(1.0) as f32,
                        h: row.get::<_, f64>(5).unwrap_or(1.0) as f32,
                    },
                    score: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                    safe: row.get::<_, i64>(7).unwrap_or(1) == 1,
                });
            }
            Ok(out)
        })
    }

    /// What a project's pass covered and did.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    #[allow(clippy::too_many_lines)]
    pub fn outline(&self, project: ProjectId) -> AuraResult<GeometryOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let photos: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let mut outline = GeometryOutline {
                photos: photos.max(0) as u64,
                ..GeometryOutline::default()
            };

            // The coverage view carries every count that is a property of the delivered variant,
            // which is a join this query would otherwise have to repeat.
            if let Some((planned, kept, straightened, keystoned, user_edited, pending)) = conn
                .query_row(
                    "SELECT planned, kept_original, straightened, keystoned, user_edited,
                            pending_review
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
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the coverage view", &e))?
            {
                outline.planned = planned.max(0) as u64;
                outline.kept_original = kept.max(0) as u64;
                outline.cropped = (planned - kept).max(0) as u64;
                outline.straightened = straightened.max(0) as u64;
                outline.keystoned = keystoned.max(0) as u64;
                outline.user_edited = user_edited.max(0) as u64;
                outline.pending_review = pending.max(0) as u64;
            }
            outline.coverage = if outline.photos == 0 {
                0.0
            } else {
                (outline.planned as f64 / outline.photos as f64) as f32
            };

            outline.mean_rotation_deg = conn
                .query_row(
                    "SELECT AVG(ABS(rotate_deg)) FROM geometry_plan
                      WHERE project_id = ?1 AND rotate_deg <> 0.0",
                    params![key],
                    |row| row.get::<_, Option<f64>>(0),
                )
                .ok()
                .flatten()
                .unwrap_or(0.0) as f32;

            outline.acted_on = conn
                .query_row(
                    "SELECT COUNT(*) FROM v_geometry_coverage v
                       JOIN geometry_plan p ON p.project_id = v.project_id
                      WHERE v.project_id = ?1",
                    params![key],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
                .map_or(0, |_| 0);
            // The view's `untouched` is what "acted on" is the complement of, and it is read
            // directly rather than derived from the join above.
            if let Some(untouched) = conn
                .query_row(
                    "SELECT untouched FROM v_geometry_coverage WHERE project_id = ?1",
                    params![key],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the coverage view", &e))?
            {
                outline.acted_on = (outline.planned as i64 - untouched).max(0) as u64;
            }

            outline.variants = conn
                .query_row(
                    "SELECT COUNT(*) FROM geometry_crop c
                       JOIN geometry_plan p ON p.photo_id = c.photo_id
                      WHERE p.project_id = ?1 AND c.ordinal > 0 AND c.safe = 1",
                    params![key],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0)
                .max(0) as u64;

            if let Some((checked, cut)) = conn
                .query_row(
                    "SELECT faces_checked, faces_cut FROM v_geometry_safety WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0).unwrap_or(0),
                            row.get::<_, i64>(1).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the safety view", &e))?
            {
                outline.faces_checked = checked.max(0) as u64;
                outline.faces_cut = cut.max(0) as u64;
            }

            // The refusal histogram, by code. Read from the variants rather than from the reason
            // strings, because a variant's refusal is one code in one column and a reason string
            // would have to be split.
            let mut statement = conn
                .prepare(
                    "SELECT c.refusal, COUNT(*) FROM geometry_crop c
                       JOIN geometry_plan p ON p.photo_id = c.photo_id
                      WHERE p.project_id = ?1 AND c.refusal IS NOT NULL
                      GROUP BY c.refusal ORDER BY COUNT(*) DESC, c.refusal",
                )
                .map_err(|e| statement_failed("could not read the refusals", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the refusals", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the refusals", &e))?
            {
                let slug: String = row.get(0).unwrap_or_default();
                if let Some(code) = GeometryCode::parse(&slug) {
                    outline
                        .crop_refusals
                        .push((code, row.get::<_, i64>(1).unwrap_or(0).max(0) as u64));
                }
            }

            let mut statement = conn
                .prepare(
                    "SELECT lens_source, COUNT(*) FROM geometry_plan
                      WHERE project_id = ?1 GROUP BY lens_source",
                )
                .map_err(|e| statement_failed("could not read the lens sources", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the lens sources", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the lens sources", &e))?
            {
                let source = LensSource::from_str_or_none(&row.get::<_, String>(0).unwrap_or_default());
                let index = LensSource::ALL
                    .iter()
                    .position(|candidate| *candidate == source)
                    .unwrap_or(3);
                if let Some(slot) = outline.lens_sources.get_mut(index) {
                    *slot = row.get::<_, i64>(1).unwrap_or(0).max(0) as u64;
                }
            }

            let mut statement = conn
                .prepare(
                    "SELECT lens_name, COUNT(*) FROM geometry_plan
                      WHERE project_id = ?1 AND lens_source = 'none' AND lens_name <> ''
                      GROUP BY lens_name ORDER BY COUNT(*) DESC, lens_name LIMIT 16",
                )
                .map_err(|e| statement_failed("could not read the missing lenses", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the missing lenses", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the missing lenses", &e))?
            {
                outline.lenses_missing.push((
                    row.get::<_, String>(0).unwrap_or_default(),
                    row.get::<_, i64>(1).unwrap_or(0).max(0) as u64,
                ));
            }

            Ok(outline)
        })
    }

    /// The frames worth a look, least confident first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    pub fn review_queue(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        let key = project.to_db();
        let limit = count_of(limit.min(4_000));
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM geometry_plan
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                      ORDER BY confidence ASC, photo_id ASC LIMIT ?2",
                )
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut cursor = statement
                .query(params![key, limit])
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the review queue", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Record that the photographer looked and agrees.
    ///
    /// Does **not** set `user_edited`: agreeing with a proposal is not the same as making one, and
    /// a later re-analysis with a better lens profile should still improve a frame somebody merely
    /// approved. Phase 15's distinction, inherited.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5111` when the image has no plan to accept.
    pub fn accept(&self, image: ImageId) -> Result<GeometryPlan, AuraError> {
        let photo = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE geometry_plan SET reviewed = 1 WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| statement_failed("could not record the acceptance", &e))
        })?;
        if changed == 0 {
            return Err(errors::geometry_edit_refused(
                "there is no geometry plan for that photograph to accept",
            ));
        }
        self.get(image)?
            .ok_or_else(|| errors::geometry_edit_refused("the accepted plan could not be read back"))
    }

    /// Record the photographer's own geometry, or revert to what was shot.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5111` when the image has no plan and when the override names a rectangle that is
    /// not inside the frame.
    ///
    /// **Both are `AURA-ML-5111` rather than one of them being `AURA-ML-5112`.** A refused edit
    /// and an unreadable profile table are different failures with different runbooks: 5112 says
    /// AURA cannot read its own settings and will not straighten or crop anything, which is a
    /// project-wide halt, and a photographer who dragged a crop handle past the edge of their
    /// photograph has not broken their installation.
    pub fn set_override(
        &self,
        image: ImageId,
        change: &GeometryOverride,
    ) -> Result<GeometryPlan, AuraError> {
        let Some(current) = self.get(image)? else {
            return Err(errors::geometry_edit_refused(
                "there is no geometry plan for that photograph to change",
            ));
        };
        if change.is_empty() {
            return Ok(current);
        }
        if let Some(rect) = change.crop {
            // The one thing a photographer may not do through this shape is name a rectangle that
            // is not a rectangle. They *may* crop tighter than AURA proposed and tighter than the
            // resolution floor, because it is their photograph and one frame is not four hundred.
            if rect.is_empty()
                || rect.x < -1e-4
                || rect.y < -1e-4
                || rect.x + rect.w > 1.0 + 1e-4
                || rect.y + rect.h > 1.0 + 1e-4
            {
                return Err(errors::geometry_edit_refused(
                    "that crop is not a rectangle inside the photograph",
                ));
            }
        }

        let photo = image.to_db();
        let revert = change.revert;
        let crop = change.crop;
        let aspect = change.aspect;
        let rotate = change.rotate_deg;
        let distortion = change.distortion;
        let vignette = change.vignette;
        let ca = change.ca;

        self.catalog.writer().transact(move |conn| {
            if revert {
                // Everything back to what was shot, and `user_edited` cleared - a photographer who
                // asked for the original framing back has asked automation to resume rather than
                // to stop. Section 13's "original framing is always one click away", as one
                // statement.
                conn.execute(
                    "DELETE FROM geometry_crop WHERE photo_id = ?1 AND ordinal > 0",
                    params![photo],
                )
                .map_err(|e| statement_failed("could not clear the variants", &e))?;
                conn.execute(
                    "UPDATE geometry_crop
                        SET rect_x = 0.0, rect_y = 0.0, rect_w = 1.0, rect_h = 1.0,
                            aspect = 'original', purpose = 'primary', safe = 1, refusal = NULL
                      WHERE photo_id = ?1 AND ordinal = 0",
                    params![photo],
                )
                .map_err(|e| statement_failed("could not restore the original framing", &e))?;
                conn.execute(
                    "UPDATE geometry_plan
                        SET primary_crop = 0, rotate_deg = 0.0,
                            keystone_vertical = NULL, keystone_horizontal = NULL,
                            keystone_stretch = NULL, keystone_convergence = NULL,
                            lens_source = 'none', lens_profile = NULL, lens_distortion = 0,
                            lens_vignette = 0, lens_ca = 0, lens_measured = 0,
                            user_edited = 0, reviewed = 1
                      WHERE photo_id = ?1",
                    params![photo],
                )
                .map_err(|e| statement_failed("could not revert the geometry plan", &e))?;
                return Ok(());
            }

            if let Some(rect) = crop {
                // The photographer's rectangle becomes ordinal zero, which is the row every
                // consumer of this table reads first. A new ordinal would leave the machine's
                // proposal as the delivered one until something else moved the index.
                conn.execute(
                    "UPDATE geometry_crop
                        SET rect_x = ?2, rect_y = ?3, rect_w = ?4, rect_h = ?5,
                            aspect = 'original', purpose = 'primary', safe = 1, refusal = NULL
                      WHERE photo_id = ?1 AND ordinal = 0",
                    params![
                        photo,
                        f64::from(rect.x),
                        f64::from(rect.y),
                        f64::from(rect.w),
                        f64::from(rect.h),
                    ],
                )
                .map_err(|e| statement_failed("could not write the framing", &e))?;
                conn.execute(
                    "UPDATE geometry_plan SET primary_crop = 0 WHERE photo_id = ?1",
                    params![photo],
                )
                .map_err(|e| statement_failed("could not write the framing", &e))?;
            }
            if let Some(aspect) = aspect {
                // Choosing an aspect is choosing one of the variants that already exists, so it
                // moves the index rather than writing a rectangle. An aspect with no safe variant
                // leaves the index where it was, which is the honest outcome: there is no square
                // crop of this photograph and saying so is `VariantUnsafe`.
                conn.execute(
                    "UPDATE geometry_plan
                        SET primary_crop = COALESCE((
                                SELECT ordinal FROM geometry_crop
                                 WHERE photo_id = ?1 AND aspect = ?2 AND safe = 1
                                 LIMIT 1), primary_crop)
                      WHERE photo_id = ?1",
                    params![photo, aspect.as_str()],
                )
                .map_err(|e| statement_failed("could not choose the aspect", &e))?;
            }
            if let Some(degrees) = rotate {
                conn.execute(
                    "UPDATE geometry_plan SET rotate_deg = ?2 WHERE photo_id = ?1",
                    params![photo, f64::from(degrees.clamp(-8.0, 8.0))],
                )
                .map_err(|e| statement_failed("could not write the rotation", &e))?;
            }
            if let Some(on) = distortion {
                conn.execute(
                    "UPDATE geometry_plan SET lens_distortion = ?2 WHERE photo_id = ?1",
                    params![photo, i64::from(on)],
                )
                .map_err(|e| statement_failed("could not write the lens correction", &e))?;
            }
            if let Some(amount) = vignette {
                conn.execute(
                    "UPDATE geometry_plan SET lens_vignette = ?2 WHERE photo_id = ?1",
                    params![photo, i64::from(amount.min(100))],
                )
                .map_err(|e| statement_failed("could not write the lens correction", &e))?;
            }
            if let Some(on) = ca {
                conn.execute(
                    "UPDATE geometry_plan SET lens_ca = ?2 WHERE photo_id = ?1",
                    params![photo, i64::from(on)],
                )
                .map_err(|e| statement_failed("could not write the lens correction", &e))?;
            }
            conn.execute(
                "UPDATE geometry_plan SET user_edited = 1, reviewed = 1 WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| statement_failed("could not record the override", &e))?;
            Ok(())
        })?;

        self.get(image)?
            .ok_or_else(|| errors::geometry_edit_refused("the changed plan could not be read back"))
    }
}

/// True when the profile behind a correction was measured by somebody.
///
/// **False on every row this build writes.** The lookup is by profile id against the bundled
/// table, so the day a measured row arrives the column starts telling the truth without anything
/// else changing.
fn measured_profile(lens: &LensCorrection) -> bool {
    lens.profile_id
        .as_deref()
        .and_then(|id| aura_render::geometry::database().get(id))
        .is_some_and(|model| model.measured)
}

/// The code that refused a variant, recovered from the plan's own reasons.
///
/// The most specific safety refusal the plan carries, and `VariantUnsafe` when it carries none.
/// A variant that is unsafe for a reason nobody recorded is a row the schema refuses, which is
/// what makes this function total rather than optional.
fn refusal_for(plan: &GeometryPlan, variant: &CropVariant) -> GeometryCode {
    let _ = variant;
    plan.reasons
        .iter()
        .map(|reason| reason.code)
        .find(|code| code.is_safety_refusal() && *code != GeometryCode::VariantUnsafe)
        .unwrap_or(GeometryCode::VariantUnsafe)
}

/// One row, before it becomes a plan.
struct StoredPlan {
    scene: String,
    lens_source: String,
    lens_profile: Option<String>,
    distortion: bool,
    vignette: i64,
    ca: bool,
    rotate_deg: f64,
    rotate_conf: f64,
    keystone_vertical: Option<f64>,
    keystone_horizontal: Option<f64>,
    keystone_stretch: Option<f64>,
    keystone_convergence: Option<f64>,
    primary_crop: i64,
    faces_intact: bool,
    resolution_ok: bool,
    content_kept: bool,
    considered: i64,
    at_risk: i64,
    long_edge_fraction: f64,
    confidence: f64,
    reasons: String,
    user_edited: bool,
    reviewed: bool,
    analysis_ver: i64,
    profile_ver: i64,
}

impl StoredPlan {
    /// Reassemble a plan from its row and its variants.
    ///
    /// **The reasons come back as codes with their weights recomputed** and their evidence
    /// rectangles resolved from the crops table and from [`lens::evidence_box`]. Storing a copy of
    /// each rectangle on each reason would be storing the same numbers twice, and the second copy
    /// is the one that goes stale when a photographer moves a crop.
    fn into_plan(self, image: ImageId, crops: &[CropVariant]) -> GeometryPlan {
        let primary_crop = self.primary_crop.max(0) as usize;
        let delivered = crops
            .get(primary_crop)
            .map_or(Box2::FULL, |variant| variant.rect);
        let reasons = self
            .reasons
            .split(',')
            .filter(|slug| !slug.is_empty())
            .filter_map(GeometryCode::parse)
            .map(|code| {
                let evidence = lens::evidence_box(code).or_else(|| {
                    matches!(
                        code,
                        GeometryCode::CropProposed
                            | GeometryCode::CropKeptOriginal
                            | GeometryCode::CropNoImprovement
                    )
                    .then_some(delivered)
                });
                match evidence {
                    Some(area) => GeometryReason::at(code, weight_of(code), area),
                    None => GeometryReason::plain(code, weight_of(code)),
                }
            })
            .collect();

        let keystone = match (
            self.keystone_vertical,
            self.keystone_horizontal,
            self.keystone_stretch,
            self.keystone_convergence,
        ) {
            (Some(v), Some(h), Some(s), Some(c)) => Some(Keystone {
                vertical: v as f32,
                horizontal: h as f32,
                stretch: s as f32,
                convergence: c as f32,
            }),
            _ => None,
        };

        let considered = self.considered.max(0) as u32;
        let safety = if considered == 0 {
            CropSafetyReport::nothing_protected(self.long_edge_fraction as f32)
        } else {
            CropSafetyReport {
                faces_intact: self.faces_intact,
                resolution_ok: self.resolution_ok,
                content_kept: self.content_kept,
                considered,
                at_risk: self.at_risk.max(0) as u32,
                long_edge_fraction: self.long_edge_fraction as f32,
                // The regions themselves are not stored: they are phase 06's faces and phase 08's
                // moment key, both of which live in their own tables and both of which can move.
                // A copy here would be a rectangle that disagrees with the detector it came from.
                regions: Vec::new(),
            }
        };

        GeometryPlan {
            image_id: image,
            scene: SceneId::from_str_or_unknown(&self.scene),
            lens: LensCorrection {
                distortion: self.distortion,
                vignette: u8::try_from(self.vignette.clamp(0, 100)).unwrap_or(0),
                ca: self.ca,
                profile_id: self.lens_profile,
                source: LensSource::from_str_or_none(&self.lens_source),
            },
            rotate_deg: self.rotate_deg as f32,
            rotate_conf: self.rotate_conf as f32,
            keystone,
            crops: if crops.is_empty() {
                vec![CropVariant::original(0.0)]
            } else {
                crops.to_vec()
            },
            primary_crop: primary_crop.min(crops.len().saturating_sub(1)),
            safety,
            reasons,
            confidence: self.confidence as f32,
            user_edited: self.user_edited,
            reviewed: self.reviewed,
            analysis_ver: u16::try_from(self.analysis_ver.max(0)).unwrap_or(0),
            profile_ver: u16::try_from(self.profile_ver.max(0)).unwrap_or(0),
        }
    }
}

/// The resolution floor, restated where the SQL can be compared against it.
///
/// The schema cannot name a Rust constant, so the CHECK on `long_edge_fraction` is a range and
/// this is what a test compares the two against.
pub const SCHEMA_LONG_EDGE_FLOOR: f32 = MIN_LONG_EDGE_FRACTION;
