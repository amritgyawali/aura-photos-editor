//! The three tables migration 21 adds, and the three rules that live in the SQL rather than
//! around it.
//!
//! ## A borrow always names its source
//!
//! `micro_op` refuses a `borrow` row with no `borrowed_from`, a trigger aborts an update that
//! would clear one, and `ON DELETE RESTRICT` refuses to delete the source photograph of a live
//! borrow. Three layers, because an undisclosed composite is the one failure in this phase that a
//! photographer cannot discover by looking at the photograph.
//!
//! ## `user_edited` is re-applied inside the statement
//!
//! A re-analysis overwrites every plan it recomputes, and that upsert carries a photographer's
//! `reviewed` and `user_edited` forward from the row it replaces, in the same statement. Tenth
//! time the rule has been written into a store, and the window it closes is the same one every
//! time: a background pass that read the flag a moment earlier and wrote a moment later.
//!
//! ## A withdrawn family is a row with no operations of that family
//!
//! Three CHECKs, one per family, rather than one flag. `withdrawn_hair = 1` with
//! `flyaway_count = 0` means AURA tried and could not do it safely; an absent row means nobody has
//! looked. Phases 25, 27 and 28 all read this table and all three act differently on those two.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{
    ClothingIssue, GlareMethod, ImageId, MicroCode, MicroOp, MicroOutline, MicroOverride,
    MicroPlan, MicroReason, NaturalnessReport, OpFamily, REVIEW_BELOW,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult, IdentityId, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use crate::errors;

/// Bytes one photograph costs in `micro_plan`, `micro_op` and their indexes.
///
/// Section 11 sets no storage budget for this phase - all four of its rows are time. One is
/// measured anyway, and **this is the first phase since 09 whose figure is above a kilobyte per
/// image**, which is a decision rather than an overrun.
///
/// The measured figure over 1,000 photographs of the widest frame the fixtures produce - a
/// portrait with a flyaway calmed, four marks taken off a lapel, teeth and eyes corrected, and
/// seven reason codes - taken as the change in `dbstat` payload rather than in `PRAGMA
/// page_count`, which quantises to 4 KiB:
///
/// ```text
///   micro_plan and its four indexes       524 B/image
///   micro_op  and its primary key       1,108 B/image   seven operations, ~158 B each
///                                     -------
///   measured total                      1,633 B/image
/// ```
///
/// Every phase from 09 to 20 stored **one verdict** per photograph, which is a fixed-width row.
/// This one stores a *list*: up to [`aura_core::contract::micro::MAX_OPS`] operations, each
/// carrying its own rectangle and its own magnitudes. The alternative - packing the five
/// operators' magnitudes into one shared column - was rejected in the schema for the reason
/// ADR-0044 section 5 gives about the wire: somebody whose complaint is that the teeth look wrong
/// needs to find out whether it was the lift or the colour, and a shared column cannot say.
///
/// A typical frame carries three or four operations rather than seven and costs about 1.1 KB.
/// The constant below is 2,000 rather than the measured 1,633, and the headroom is phase 19's
/// correction: a budget must not be pinned at its own measurement.
///
/// `micro_matrix` is deliberately outside the figure. It is one row per *project*, so including
/// it would put a constant into a per-image number and make it look smaller on a larger wedding.
pub const BYTES_PER_IMAGE: usize = 2_000;

/// One `micro_plan` row as SQLite hands it back.
///
/// A named alias rather than the tuple written out at the call site: twenty-two columns in a
/// `let` binding is a shape nobody can check against the `SELECT` above it, and the alias is the
/// one place the two are compared.
///
/// The order is the `SELECT`'s: scene, the three guard measurements, the sample count, the
/// resolve count, three withdrawal flags, five permission flags, the budget, the confidence, the
/// reasons blob, the two photographer flags, and the three versions.
type PlanRow = (
    String,
    f64,
    f64,
    f64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    f64,
    f64,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
);

/// A length as SQLite's integer, saturating rather than wrapping.
///
/// Every caller is a count of things in one photograph's plan - operations, reason codes, a page
/// limit - so the saturation is unreachable. It is written as a saturation anyway because the
/// alternative is a cast that wraps a plan with four billion operations into a negative one, and
/// a negative count in a CHECK constraint is a refusal nobody can read.
fn count_of(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// The `micro_plan`, `micro_matrix` and `micro_op` tables.
#[derive(Debug)]
pub struct MicroStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl MicroStore {
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
    /// 50 % or 90 % and the next run asks the catalog what is left. A `matrix_ver` bump therefore
    /// heals itself - the rows made under the old table are pending by definition.
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
                       LEFT JOIN micro_plan m ON m.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (m.photo_id IS NULL
                             OR m.model_ver    <> ?2
                             OR m.analysis_ver <> ?3
                             OR m.matrix_ver   <> ?4)
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
    /// What the IPC layer walks when it carries plans into recipes. The opposite question to
    /// [`MicroStore::pending`], which would return every photograph in the project if it were
    /// asked this one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn planned(&self, project: &ProjectId) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id FROM micro_plan WHERE project_id = ?1 ORDER BY photo_id")
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
    /// `AURA-DB-3006` when the write fails, and `AURA-ML-5097` when the plan breaks a guarantee -
    /// checked here as well as in the pass, because the store is the last place a bad row can be
    /// stopped and a caller nobody has written yet will reach this function.
    #[allow(clippy::too_many_lines)]
    pub fn put(&self, project: &ProjectId, plan: &MicroPlan) -> AuraResult<()> {
        if let Some(problem) = plan.broken_guarantee() {
            return Err(errors::micro_failed(&plan.image_id.to_db(), problem));
        }
        let photo = plan.image_id.to_db();
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let plan = plan.clone();

        // `transact` already opens and commits one transaction, so the plan row and its
        // operations are written atomically without a second one nested inside it.
        self.catalog.writer().transact(move |conn| {
            // Both flags carried forward inside the statement rather than read before it. See the
            // module header.
            let carried: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT user_edited, reviewed FROM micro_plan WHERE photo_id = ?1",
                    params![photo],
                    |row| Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0))),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the previous plan", &e))?;
            let (user_edited, reviewed) = carried.unwrap_or((0, 0));

            let counts = histogram(&plan);
            let borrows = count_of(
                plan.ops
                    .iter()
                    .filter(|op| op.borrowed_from().is_some())
                    .count(),
            );
            let reasons = plan
                .reasons
                .iter()
                .map(|reason| reason.code.as_str())
                .collect::<Vec<_>>()
                .join(",");

            conn.execute(
                "INSERT INTO micro_plan (
                        photo_id, project_id, scene,
                        catchlight_ratio, hair_energy_ratio, teeth_excursion, measured_on, resolves,
                        withdrawn_hair, withdrawn_teeth, withdrawn_eyes,
                        allow_flyaway, allow_teeth, allow_eyes, allow_clothing, allow_glare,
                        op_count, flyaway_count, teeth_count, eyes_count, clothing_count,
                        glare_count, borrow_count,
                        budget_used, region_covered, confidence, reasons,
                        user_edited, reviewed, model_ver, analysis_ver, matrix_ver, planned_at
                     ) VALUES (
                        ?1, ?2, ?3,
                        ?4, ?5, ?6, ?7, ?8,
                        ?9, ?10, ?11,
                        ?12, ?13, ?14, ?15, ?16,
                        ?17, ?18, ?19, ?20, ?21,
                        ?22, ?23,
                        ?24, ?25, ?26, ?27,
                        ?28, ?29, ?30, ?31, ?32, ?33
                     )
                     ON CONFLICT(photo_id) DO UPDATE SET
                        project_id        = excluded.project_id,
                        scene             = excluded.scene,
                        catchlight_ratio  = excluded.catchlight_ratio,
                        hair_energy_ratio = excluded.hair_energy_ratio,
                        teeth_excursion   = excluded.teeth_excursion,
                        measured_on       = excluded.measured_on,
                        resolves          = excluded.resolves,
                        withdrawn_hair    = excluded.withdrawn_hair,
                        withdrawn_teeth   = excluded.withdrawn_teeth,
                        withdrawn_eyes    = excluded.withdrawn_eyes,
                        allow_flyaway     = excluded.allow_flyaway,
                        allow_teeth       = excluded.allow_teeth,
                        allow_eyes        = excluded.allow_eyes,
                        allow_clothing    = excluded.allow_clothing,
                        allow_glare       = excluded.allow_glare,
                        op_count          = excluded.op_count,
                        flyaway_count     = excluded.flyaway_count,
                        teeth_count       = excluded.teeth_count,
                        eyes_count        = excluded.eyes_count,
                        clothing_count    = excluded.clothing_count,
                        glare_count       = excluded.glare_count,
                        borrow_count      = excluded.borrow_count,
                        budget_used       = excluded.budget_used,
                        region_covered    = excluded.region_covered,
                        confidence        = excluded.confidence,
                        reasons           = excluded.reasons,
                        model_ver         = excluded.model_ver,
                        analysis_ver      = excluded.analysis_ver,
                        matrix_ver        = excluded.matrix_ver,
                        planned_at        = excluded.planned_at",
                params![
                    photo,
                    project_key,
                    plan.scene.as_str(),
                    f64::from(plan.naturalness.catchlight_ratio),
                    f64::from(plan.naturalness.hair_energy_ratio),
                    f64::from(plan.naturalness.teeth_excursion),
                    i64::from(plan.naturalness.measured_on),
                    i64::from(plan.naturalness.resolves),
                    i64::from(plan.naturalness.is_withdrawn(OpFamily::Hair)),
                    i64::from(plan.naturalness.is_withdrawn(OpFamily::Teeth)),
                    i64::from(plan.naturalness.is_withdrawn(OpFamily::Eyes)),
                    i64::from(plan.allowed.first().copied().unwrap_or(false)),
                    i64::from(plan.allowed.get(1).copied().unwrap_or(false)),
                    i64::from(plan.allowed.get(2).copied().unwrap_or(false)),
                    i64::from(plan.allowed.get(3).copied().unwrap_or(false)),
                    i64::from(plan.allowed.get(4).copied().unwrap_or(false)),
                    count_of(plan.ops.len()),
                    counts[0],
                    counts[1],
                    counts[2],
                    counts[3],
                    counts[4],
                    borrows,
                    f64::from(plan.budget_used),
                    i64::from(!plan.reasons.iter().any(|r| {
                        r.code == MicroCode::RegionUnavailable
                            || r.code == MicroCode::RegionDoubtful
                    })),
                    f64::from(plan.confidence),
                    reasons,
                    user_edited,
                    reviewed,
                    i64::from(plan.model_ver),
                    i64::from(plan.analysis_ver),
                    i64::from(plan.matrix_ver),
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store the micro plan", &e))?;

            conn.execute("DELETE FROM micro_op WHERE photo_id = ?1", params![photo])
                .map_err(|e| statement_failed("could not clear the previous operations", &e))?;

            for (seq, op) in plan.ops.iter().enumerate() {
                let region = op.region();
                let (x, y, w, h) = region.map_or((None, None, None, None), |r| {
                    (
                        Some(f64::from(r.x)),
                        Some(f64::from(r.y)),
                        Some(f64::from(r.w)),
                        Some(f64::from(r.h)),
                    )
                });
                let identity = match op {
                    MicroOp::Teeth { identity, .. } | MicroOp::Eyes { identity, .. } => {
                        Some(identity.to_db())
                    }
                    _ => None,
                };
                let (luma, yellow) = match op {
                    MicroOp::Teeth {
                        luma,
                        yellow_reduce,
                        ..
                    } => (f64::from(*luma), f64::from(*yellow_reduce)),
                    _ => (0.0, 0.0),
                };
                let (sclera, iris) = match op {
                    MicroOp::Eyes {
                        sclera,
                        iris_clarity,
                        ..
                    } => (f64::from(*sclera), f64::from(*iris_clarity)),
                    _ => (0.0, 0.0),
                };
                let clothing_kind = match op {
                    MicroOp::Clothing { kind, .. } => Some(kind.as_str()),
                    _ => None,
                };
                let (method, borrowed, alignment) = match op {
                    MicroOp::Glare { method, .. } => match method {
                        GlareMethod::Reduce { .. } => (Some("reduce"), None, 0.0),
                        GlareMethod::BorrowFrom { source, alignment } => {
                            (Some("borrow"), Some(source.to_db()), f64::from(*alignment))
                        }
                    },
                    _ => (None, None, 0.0),
                };
                let strength = match op {
                    MicroOp::Flyaway { strength, .. } | MicroOp::Clothing { strength, .. } => {
                        f64::from(*strength)
                    }
                    MicroOp::Glare {
                        method: GlareMethod::Reduce { strength },
                        ..
                    } => f64::from(*strength),
                    _ => 0.0,
                };

                conn.execute(
                    "INSERT INTO micro_op (
                            photo_id, seq, kind, x, y, w, h, identity_id, strength,
                            luma_ev, yellow_reduce, sclera, iris_clarity,
                            clothing_kind, method, borrowed_from, alignment
                         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                    params![
                        photo,
                        count_of(seq),
                        op.as_str(),
                        x,
                        y,
                        w,
                        h,
                        identity,
                        strength,
                        luma,
                        yellow,
                        sclera,
                        iris,
                        clothing_kind,
                        method,
                        borrowed,
                        alignment,
                    ],
                )
                .map_err(|e| statement_failed("could not store a micro operation", &e))?;
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
    pub fn of_image(&self, image: ImageId) -> AuraResult<Option<MicroPlan>> {
        let photo = image.to_db();
        self.catalog.read(move |conn| {
            let row: Option<PlanRow> = conn
                .query_row(
                    "SELECT scene, catchlight_ratio, hair_energy_ratio, teeth_excursion,
                            measured_on, resolves, withdrawn_hair, withdrawn_teeth, withdrawn_eyes,
                            allow_flyaway, allow_teeth, allow_eyes, allow_clothing, allow_glare,
                            budget_used, confidence, reasons, user_edited, reviewed,
                            model_ver, analysis_ver, matrix_ver
                       FROM micro_plan WHERE photo_id = ?1",
                    params![photo],
                    |row| {
                        Ok((
                            row.get(0).unwrap_or_default(),
                            row.get(1).unwrap_or(1.0),
                            row.get(2).unwrap_or(1.0),
                            row.get(3).unwrap_or(0.0),
                            row.get(4).unwrap_or(0),
                            row.get(5).unwrap_or(0),
                            row.get(6).unwrap_or(0),
                            row.get(7).unwrap_or(0),
                            row.get(8).unwrap_or(0),
                            row.get(9).unwrap_or(0),
                            row.get(10).unwrap_or(0),
                            row.get(11).unwrap_or(0),
                            row.get(12).unwrap_or(0),
                            row.get(13).unwrap_or(0),
                            row.get(14).unwrap_or(0.0),
                            row.get(15).unwrap_or(0.0),
                            row.get(16).unwrap_or_default(),
                            row.get(17).unwrap_or(0),
                            row.get(18).unwrap_or(0),
                            row.get(19).unwrap_or(0),
                            row.get(20).unwrap_or(0),
                            row.get(21).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the micro plan", &e))?;

            let Some(row) = row else {
                return Ok(None);
            };

            let mut statement = conn
                .prepare(
                    "SELECT kind, x, y, w, h, identity_id, strength, luma_ev, yellow_reduce,
                            sclera, iris_clarity, clothing_kind, method, borrowed_from, alignment
                       FROM micro_op WHERE photo_id = ?1 ORDER BY seq",
                )
                .map_err(|e| statement_failed("could not read the micro operations", &e))?;
            let mut cursor = statement
                .query(params![photo])
                .map_err(|e| statement_failed("could not read the micro operations", &e))?;
            let mut ops = Vec::new();
            while let Some(op_row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a micro operation", &e))?
            {
                let kind: String = op_row.get(0).unwrap_or_default();
                let region = Box2 {
                    x: op_row.get::<_, f64>(1).unwrap_or(0.0) as f32,
                    y: op_row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                    w: op_row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                    h: op_row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                };
                let identity = op_row
                    .get::<_, Option<String>>(5)
                    .ok()
                    .flatten()
                    .and_then(|text| IdentityId::from_db(&text).ok());
                let strength = op_row.get::<_, f64>(6).unwrap_or(0.0) as f32;
                match kind.as_str() {
                    "flyaway" => ops.push(MicroOp::Flyaway { region, strength }),
                    "teeth" => {
                        if let Some(identity) = identity {
                            ops.push(MicroOp::Teeth {
                                identity,
                                luma: op_row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                                yellow_reduce: op_row.get::<_, f64>(8).unwrap_or(0.0) as f32,
                            });
                        }
                    }
                    "eyes" => {
                        if let Some(identity) = identity {
                            ops.push(MicroOp::Eyes {
                                identity,
                                sclera: op_row.get::<_, f64>(9).unwrap_or(0.0) as f32,
                                iris_clarity: op_row.get::<_, f64>(10).unwrap_or(0.0) as f32,
                            });
                        }
                    }
                    "clothing" => {
                        let text: String = op_row.get(11).unwrap_or_default();
                        ops.push(MicroOp::Clothing {
                            region,
                            kind: ClothingIssue::parse(&text).unwrap_or_default(),
                            strength,
                        });
                    }
                    "glare" => {
                        let method: String = op_row.get(12).unwrap_or_default();
                        let source = op_row
                            .get::<_, Option<String>>(13)
                            .ok()
                            .flatten()
                            .and_then(|text| PhotoId::from_db(&text).ok());
                        let alignment = op_row.get::<_, f64>(14).unwrap_or(0.0) as f32;
                        // A `borrow` row with no source cannot exist - the schema refuses it -
                        // so a `None` here is a corrupt catalog rather than a repairable state,
                        // and the operation is dropped rather than rewritten as a reduction. A
                        // silent downgrade would turn an undisclosed composite into a plausible
                        // row, which is exactly backwards.
                        match (method.as_str(), source) {
                            ("borrow", Some(source)) => ops.push(MicroOp::Glare {
                                region,
                                method: GlareMethod::BorrowFrom { source, alignment },
                            }),
                            ("reduce", _) => ops.push(MicroOp::Glare {
                                region,
                                method: GlareMethod::Reduce { strength },
                            }),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            let reasons: Vec<MicroReason> = row
                .16
                .split(',')
                .filter(|slug| !slug.is_empty())
                .filter_map(MicroCode::parse)
                .map(|code| MicroReason::plain(code, 0.0))
                .collect();

            Ok(Some(MicroPlan {
                image_id: image,
                ops,
                naturalness: NaturalnessReport {
                    catchlight_ratio: row.1 as f32,
                    hair_energy_ratio: row.2 as f32,
                    teeth_excursion: row.3 as f32,
                    measured_on: row.4 as u32,
                    resolves: row.5 as u8,
                    withdrawn: [row.6 != 0, row.7 != 0, row.8 != 0],
                },
                allowed: [
                    row.9 != 0,
                    row.10 != 0,
                    row.11 != 0,
                    row.12 != 0,
                    row.13 != 0,
                ],
                reasons,
                confidence: row.15 as f32,
                scene: SceneId::from_str_or_unknown(&row.0),
                budget_used: row.14 as f32,
                user_edited: row.17 != 0,
                reviewed: row.18 != 0,
                model_ver: row.19 as u16,
                analysis_ver: row.20 as u16,
                matrix_ver: row.21 as u16,
            }))
        })
    }

    /// Every frame in the project that borrowed pixels, with its sources.
    ///
    /// **The disclosure query**, served by `v_micro_composites` so that the panel, the delivery
    /// report and the QC agent read the same rows through the same view.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn composites(&self, project: &ProjectId) -> AuraResult<BTreeMap<ImageId, Vec<ImageId>>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, source_photo_id FROM v_micro_composites
                      WHERE project_id = ?1",
                )
                .map_err(|e| statement_failed("could not read the composites", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the composites", &e))?;
            let mut out: BTreeMap<ImageId, Vec<ImageId>> = BTreeMap::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a composite", &e))?
            {
                let photo: String = row.get(0).unwrap_or_default();
                let source: String = row.get(1).unwrap_or_default();
                if let (Ok(photo), Ok(source)) =
                    (PhotoId::from_db(&photo), PhotoId::from_db(&source))
                {
                    let entry = out.entry(photo).or_default();
                    if !entry.contains(&source) {
                        entry.push(source);
                    }
                }
            }
            Ok(out)
        })
    }

    /// The frames worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn needs_review(&self, project: &ProjectId, limit: usize) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        let cap = count_of(limit);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM micro_plan
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                        AND confidence < ?2
                      ORDER BY confidence ASC, photo_id ASC
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

    /// Mark one plan as reviewed.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5098` when the photograph has no plan.
    pub fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        let photo = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE micro_plan SET reviewed = 1 WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| statement_failed("could not accept the micro plan", &e))
        })?;
        if changed == 0 {
            return Err(errors::micro_edit_refused(
                "that photograph has no micro-retouch plan",
            ));
        }
        Ok(())
    }

    /// Which operations a project permits, falling back to the supplied defaults.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn matrix(
        &self,
        project: &ProjectId,
        defaults: ([bool; 5], [bool; ClothingIssue::COUNT], bool),
    ) -> AuraResult<MicroOverride> {
        let key = project.to_db();
        let stored = self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT allow_flyaway, allow_teeth, allow_eyes, allow_clothing, allow_glare,
                        allow_lint, allow_thread, allow_stain, allow_strap, allow_crease,
                        allow_borrow
                   FROM micro_matrix WHERE project_id = ?1",
                params![key],
                |row| {
                    let mut flags = [0i64; 11];
                    for (index, slot) in flags.iter_mut().enumerate() {
                        *slot = row.get(index).unwrap_or(0);
                    }
                    Ok(flags)
                },
            )
            .optional()
            .map_err(|e| statement_failed("could not read the micro matrix", &e))
        })?;

        Ok(match stored {
            None => MicroOverride {
                allowed: Some(defaults.0),
                clothing: Some(defaults.1),
                borrowing: Some(defaults.2),
            },
            Some(flags) => MicroOverride {
                allowed: Some([
                    flags[0] != 0,
                    flags[1] != 0,
                    flags[2] != 0,
                    flags[3] != 0,
                    flags[4] != 0,
                ]),
                clothing: Some([
                    flags[5] != 0,
                    flags[6] != 0,
                    flags[7] != 0,
                    flags[8] != 0,
                    flags[9] != 0,
                ]),
                borrowing: Some(flags[10] != 0),
            },
        })
    }

    /// Record which operations a project permits.
    ///
    /// Sets `user_edited`, which a re-analysis never clears. Fields the override leaves absent
    /// keep whatever the row already had - the shape phases 15, 19 and 20 all use, because
    /// somebody who switched glare off has made no claim about teeth.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5098` when the override sets nothing.
    pub fn set_matrix(
        &self,
        project: &ProjectId,
        values: &MicroOverride,
        defaults: ([bool; 5], [bool; ClothingIssue::COUNT], bool),
        matrix_ver: u16,
    ) -> Result<(), AuraError> {
        crate::micro::guard::check_override(values)?;
        let current = self.matrix(project, defaults)?;
        let allowed = values.allowed.or(current.allowed).unwrap_or(defaults.0);
        let clothing = values.clothing.or(current.clothing).unwrap_or(defaults.1);
        let borrowing = values.borrowing.or(current.borrowing).unwrap_or(defaults.2);
        let key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());

        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO micro_matrix (
                    project_id, allow_flyaway, allow_teeth, allow_eyes, allow_clothing,
                    allow_glare, allow_lint, allow_thread, allow_stain, allow_strap,
                    allow_crease, allow_borrow, user_edited, matrix_ver, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,1,?13,?14)
                 ON CONFLICT(project_id) DO UPDATE SET
                    allow_flyaway  = excluded.allow_flyaway,
                    allow_teeth    = excluded.allow_teeth,
                    allow_eyes     = excluded.allow_eyes,
                    allow_clothing = excluded.allow_clothing,
                    allow_glare    = excluded.allow_glare,
                    allow_lint     = excluded.allow_lint,
                    allow_thread   = excluded.allow_thread,
                    allow_stain    = excluded.allow_stain,
                    allow_strap    = excluded.allow_strap,
                    allow_crease   = excluded.allow_crease,
                    allow_borrow   = excluded.allow_borrow,
                    user_edited    = 1,
                    matrix_ver     = excluded.matrix_ver,
                    updated_at     = excluded.updated_at",
                params![
                    key,
                    i64::from(allowed[0]),
                    i64::from(allowed[1]),
                    i64::from(allowed[2]),
                    i64::from(allowed[3]),
                    i64::from(allowed[4]),
                    i64::from(clothing[0]),
                    i64::from(clothing[1]),
                    i64::from(clothing[2]),
                    i64::from(clothing[3]),
                    i64::from(clothing[4]),
                    i64::from(borrowing),
                    i64::from(matrix_ver),
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store the micro matrix", &e))
        })?;
        Ok(())
    }

    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    #[allow(clippy::too_many_lines)]
    pub fn outline(&self, project: &ProjectId, unlisted: Vec<String>) -> AuraResult<MicroOutline> {
        let key = project.to_db();
        let mut outline = self.catalog.read(move |conn| {
            let mut outline = MicroOutline {
                unlisted_scenes: Vec::new(),
                ..MicroOutline::default()
            };

            if let Some(row) = conn
                .query_row(
                    "SELECT photos, planned, acted_on, region_covered, borrowed, resolved,
                            withdrawn_hair, withdrawn_teeth, withdrawn_eyes,
                            COALESCE(mean_catchlight_ratio, 0.0),
                            COALESCE(mean_hair_energy_ratio, 0.0)
                       FROM v_micro_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        let mut values = [0i64; 9];
                        for (index, slot) in values.iter_mut().enumerate() {
                            *slot = row.get(index).unwrap_or(0);
                        }
                        Ok((
                            values,
                            row.get::<_, f64>(9).unwrap_or(0.0),
                            row.get::<_, f64>(10).unwrap_or(0.0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the micro coverage", &e))?
            {
                let (values, catchlight, hair) = row;
                outline.photos = values[0] as u32;
                outline.planned = values[1] as u32;
                outline.acted_on = values[2] as u32;
                outline.region_covered = values[3] as u32;
                outline.borrows = values[4] as u32;
                outline.resolved = values[5] as u32;
                outline.withdrawn_histogram =
                    [values[6] as u32, values[7] as u32, values[8] as u32];
                outline.mean_catchlight_ratio = catchlight as f32;
                outline.mean_hair_energy_ratio = hair as f32;
            }

            if let Some(row) = conn
                .query_row(
                    "SELECT SUM(flyaway_count), SUM(teeth_count), SUM(eyes_count),
                            SUM(clothing_count), SUM(glare_count),
                            SUM(CASE WHEN reviewed = 0 AND user_edited = 0 AND confidence < ?2
                                     THEN 1 ELSE 0 END),
                            SUM(user_edited),
                            MAX(model_ver), MAX(analysis_ver), MAX(matrix_ver)
                       FROM micro_plan WHERE project_id = ?1",
                    params![key, f64::from(REVIEW_BELOW)],
                    |row| {
                        let mut values = [0i64; 10];
                        for (index, slot) in values.iter_mut().enumerate() {
                            *slot = row.get(index).unwrap_or(0);
                        }
                        Ok(values)
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the micro histogram", &e))?
            {
                outline.op_histogram = [
                    row[0] as u32,
                    row[1] as u32,
                    row[2] as u32,
                    row[3] as u32,
                    row[4] as u32,
                ];
                outline.needs_review = row[5] as u32;
                outline.user_edited = row[6] as u32;
                outline.model_ver = row[7] as u16;
                outline.analysis_ver = row[8] as u16;
                outline.matrix_ver = row[9] as u16;
            }

            Ok(outline)
        })?;

        outline.coverage = if outline.photos == 0 {
            0.0
        } else {
            f64::from(outline.planned) as f32 / f64::from(outline.photos) as f32
        };
        outline.unlisted_scenes = unlisted;
        Ok(outline)
    }
}

/// How many operations of each kind a plan carries, in `MicroOp::NAMES` order.
fn histogram(plan: &MicroPlan) -> [i64; 5] {
    let mut out = [0i64; 5];
    for op in &plan.ops {
        if let Some(index) = MicroOp::NAMES.iter().position(|name| *name == op.as_str()) {
            if let Some(slot) = out.get_mut(index) {
                *slot += 1;
            }
        }
    }
    out
}
