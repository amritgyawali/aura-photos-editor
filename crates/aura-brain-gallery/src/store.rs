//! The five tables migration 25 adds, and the rules that live in the SQL rather than around them.
//!
//! ## Node membership is the delta table
//!
//! There is no join table. Every frame placed in a node gets a `gallery_delta` row - including the
//! frames that moved by nothing - so `WHERE node_id = ?` *is* the membership query. A separate
//! table would have cost about 85 B per image, a fifth of section 11's whole budget, to store a
//! fact already on the row beside it.
//!
//! That has one consequence worth stating: a node with no deltas is a node nothing was placed in,
//! which cannot happen through [`GalleryStore::write_pass`] and is therefore a corrupt catalog
//! rather than an empty chapter.
//!
//! ## A pinned anchor survives automation, and it takes two statements
//!
//! Phase 18 learned that guarding the `DELETE` is not enough when the table has a unique key: an
//! `INSERT OR REPLACE` deletes the row it conflicts with, whatever the `DELETE` excluded. So
//! [`GalleryStore::replace_anchors`] both excludes pinned and rejected rows from its `DELETE` *and*
//! skips those images on re-insert, and a trigger refuses the UPDATE that would unpin one.
//!
//! ## A reason set is one integer
//!
//! `reasons` is a bitmask over `GalleryCode::ALL`. Phase 09's rule was that a reason stores its
//! code rather than its sentence; this is that rule taken one step, because a list of slugs on the
//! one table with a row per photograph costs about sixty bytes and this costs eight. The weight is
//! a property of the code - `GalleryCode::default_weight` - so it is rendered on read like the
//! sentence is.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::gallery::{
    Bound, GalleryCode, GalleryOutline, GalleryReason, ImageId, NodeTarget, NormalisationDelta,
    Outlier, SceneNode, SkinCorrection, SkinTarget,
};
use aura_core::contract::ids::NodeId;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, IdentityId, ProjectId, SceneId, SegmentId};
use rusqlite::{params, OptionalExtension};

use crate::errors;

/// Bytes one photograph costs in `gallery_delta`, `gallery_outlier` and their indexes.
///
/// Section 11 budgets 500 B per image for the whole of this phase, the second tightest figure in
/// the product after phase 09's 1 KB. `crates/aura-perf/tests/gallery_budgets.rs` prints the
/// per-object breakdown on every run, and the figure in `perf/budgets.toml` carries the measurement
/// plus headroom rather than being pinned at it - phase 19's correction, after phase 21 shipped a
/// figure that had been written before it was measured and was wrong by a factor of two and a half.
pub const BUDGET_BYTES_PER_IMAGE: u64 = 500;

/// One catalog, wrapped.
#[derive(Debug, Clone)]
pub struct GalleryStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

/// One node, its anchors and its frames' deltas, as one unit to write.
///
/// The pass produces these and the store writes them in one transaction per node. Per node rather
/// than per project, because a 4,000-image wedding is one transaction that either holds a write
/// lock for a minute or is killed half way through - and per frame, because forty nodes is forty
/// round trips and four thousand frames is four thousand.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeWrite {
    /// The node.
    pub node: SceneNode,
    /// Its anchors, best first, with the quality that ranked them.
    pub anchors: Vec<(ImageId, f32)>,
    /// Its frames' deltas, in capture order.
    pub deltas: Vec<NormalisationDelta>,
    /// The frames of it that are still out of line.
    pub outliers: Vec<Outlier>,
    /// When its first frame was taken, as the project's own timeline text.
    pub first_ts: String,
}

impl GalleryStore {
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

    /// True when a project's stored rows were produced by this build's arithmetic and policy.
    ///
    /// **The work remaining is a query, not a journal.** Invariant 5: kill the process at 10 %,
    /// 50 % or 90 % and the next run asks the catalog whether what is there is current. A
    /// `policy_ver` bump therefore heals itself, because every row made under the old table
    /// answers `false`.
    ///
    /// Whole-project rather than per-photograph, unlike phases 09 to 24, and that is this phase's
    /// own shape rather than a departure: a delta is a statement about a *node*, and a node whose
    /// half was solved under one policy and half under another has a target that describes neither.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn is_current(&self, project: ProjectId, versions: (u16, u16)) -> AuraResult<bool> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let stale: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM gallery_node
                      WHERE project_id = ?1 AND (analysis_ver <> ?2 OR policy_ver <> ?3)",
                    params![key, i64::from(versions.0), i64::from(versions.1)],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("gallery_node version check", &err))?;
            let present: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM gallery_node WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("gallery_node count", &err))?;
            Ok(present > 0 && stale == 0)
        })
    }

    /// The versions a project's rows carry, or `None` when it has none.
    ///
    /// What `AURA-ML-5127` is raised from: a caller that finds a project at a different pair knows
    /// which way the comparison would have been wrong.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn stored_versions(&self, project: ProjectId) -> AuraResult<Option<(u16, u16)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT analysis_ver, policy_ver FROM gallery_node
                  WHERE project_id = ?1 ORDER BY first_ts LIMIT 1",
                params![key],
                |row| Ok((row.get::<_, i64>(0)? as u16, row.get::<_, i64>(1)? as u16)),
            )
            .optional()
            .map_err(|err| statement_failed("gallery_node versions", &err))
        })
    }

    /// Clear a project's tree, keeping every decision a photographer made.
    ///
    /// The pins, the rejections, the overrides and the per-frame switches are read out first and
    /// handed back so the next pass can re-apply them. Reading them out rather than leaving them in
    /// place is what makes this safe against a re-shaped tree: a node that no longer exists cannot
    /// carry a pin forward, and a pin whose photograph is now in a different node still belongs to
    /// the photographer.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a statement fails.
    pub fn take_decisions(&self, project: ProjectId) -> AuraResult<Decisions> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut pinned = BTreeSet::new();
            let mut rejected = BTreeSet::new();
            let mut statement = conn
                .prepare(
                    "SELECT a.photo_id, a.user_pinned, a.user_rejected
                       FROM gallery_anchor a
                       JOIN gallery_node n ON n.node_id = a.node_id
                      WHERE n.project_id = ?1 AND (a.user_pinned = 1 OR a.user_rejected = 1)",
                )
                .map_err(|err| statement_failed("gallery_anchor decisions", &err))?;
            let rows = statement
                .query_map(params![key.clone()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|err| statement_failed("gallery_anchor decisions", &err))?;
            for row in rows {
                let (id, pin, reject) =
                    row.map_err(|err| statement_failed("gallery_anchor row", &err))?;
                let Ok(image) = ImageId::from_db(&id) else {
                    continue;
                };
                if pin == 1 {
                    pinned.insert(image);
                }
                if reject == 1 {
                    rejected.insert(image);
                }
            }

            let mut overrides = BTreeMap::new();
            let mut disabled = BTreeSet::new();
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, user_edited, enabled, d_exposure, d_cct, d_tint,
                            d_contrast, d_saturation
                       FROM gallery_delta
                      WHERE project_id = ?1 AND (user_edited = 1 OR enabled = 0)",
                )
                .map_err(|err| statement_failed("gallery_delta decisions", &err))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        [
                            row.get::<_, f64>(3)? as f32,
                            row.get::<_, f64>(4)? as f32,
                            row.get::<_, f64>(5)? as f32,
                            row.get::<_, f64>(6)? as f32,
                            row.get::<_, f64>(7)? as f32,
                        ],
                    ))
                })
                .map_err(|err| statement_failed("gallery_delta decisions", &err))?;
            for row in rows {
                let (id, edited, enabled, values) =
                    row.map_err(|err| statement_failed("gallery_delta row", &err))?;
                let Ok(image) = ImageId::from_db(&id) else {
                    continue;
                };
                if edited == 1 {
                    overrides.insert(image, values);
                }
                if enabled == 0 {
                    disabled.insert(image);
                }
            }

            Ok(Decisions {
                pinned,
                rejected,
                overrides,
                disabled,
            })
        })
    }

    /// Replace a project's whole tree with a freshly solved one.
    ///
    /// One transaction. A tree half from one pass and half from another is a tree whose nodes
    /// overlap, and there is no partial state of this table a reader could make sense of - unlike
    /// phases 09 to 24, whose rows are independent per photograph. Resumability is at the level of
    /// the pass rather than the row here, and [`GalleryStore::is_current`] is what a resumed run
    /// asks.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a statement fails.
    // Four INSERTs and four DELETEs whose *order* is the correctness argument - see the comment
    // inside. Extracting them would put that order in four places.
    #[allow(clippy::too_many_lines)]
    pub fn write_pass(
        &self,
        project: ProjectId,
        nodes: &[NodeWrite],
        skin: &BTreeMap<IdentityId, SkinTarget>,
    ) -> AuraResult<()> {
        let key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let nodes = nodes.to_vec();
        let skin: Vec<SkinTarget> = skin.values().copied().collect();
        self.catalog.writer().transact(move |tx| {
            // The order matters: outliers and deltas point at nodes, so nodes go last on the way in
            // and first on the way out. `ON DELETE CASCADE` would do it, and being explicit means a
            // future table added to this migration does not silently start surviving a re-pass.
            tx.execute(
                "DELETE FROM gallery_outlier WHERE project_id = ?1",
                params![key],
            )
            .map_err(|err| statement_failed("gallery_outlier clear", &err))?;
            tx.execute(
                "DELETE FROM gallery_delta WHERE project_id = ?1",
                params![key],
            )
            .map_err(|err| statement_failed("gallery_delta clear", &err))?;
            tx.execute(
                "DELETE FROM gallery_node WHERE project_id = ?1",
                params![key],
            )
            .map_err(|err| statement_failed("gallery_node clear", &err))?;
            tx.execute(
                "DELETE FROM gallery_skin_target WHERE project_id = ?1",
                params![key],
            )
            .map_err(|err| statement_failed("gallery_skin_target clear", &err))?;

            for write in &nodes {
                let node = &write.node;
                let target = node.target;
                let signature =
                    target.map(|t| serde_json::to_string(&t.grade_signature).unwrap_or_default());
                tx.execute(
                    "INSERT INTO gallery_node
                       (node_id, project_id, segment_id, parent_id, label, ordinal, scene,
                        image_count, first_ts, cct_k, cct_tol, tint, tint_tol, subject_luma,
                        luma_tol, contrast, saturation, grade_signature, anchor_count, cohesion,
                        reasons, analysis_ver, policy_ver, created_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,
                             ?20,?21,?22,?23,?24)",
                    params![
                        node.id.to_db(),
                        key,
                        node.segment_id.to_db(),
                        node.parent.map(|p| p.to_db()),
                        node.label,
                        i64::try_from(write.deltas.len()).unwrap_or(0),
                        node.scene.as_str(),
                        i64::try_from(node.image_ids.len()).unwrap_or(0),
                        write.first_ts,
                        target.map(|t| f64::from(t.cct_k)),
                        target.map(|t| f64::from(t.cct_tol)),
                        target.map(|t| f64::from(t.tint)),
                        target.map(|t| f64::from(t.tint_tol)),
                        target.map(|t| f64::from(t.subject_luma)),
                        target.map(|t| f64::from(t.luma_tol)),
                        target.map(|t| f64::from(t.contrast)),
                        target.map(|t| f64::from(t.saturation)),
                        signature,
                        i64::from(target.map_or(0, |t| t.anchor_count)),
                        f64::from(target.map_or(0.0, |t| t.cohesion)),
                        i64::from(GalleryReason::to_bits(&node.reasons)),
                        i64::from(node.analysis_ver),
                        i64::from(node.policy_ver),
                        now,
                    ],
                )
                .map_err(|err| statement_failed("gallery_node insert", &err))?;

                for (rank, (image, quality)) in write.anchors.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO gallery_anchor (node_id, photo_id, rank, quality,
                                                     user_pinned, user_rejected)
                         VALUES (?1,?2,?3,?4,0,0)",
                        params![
                            node.id.to_db(),
                            image.to_db(),
                            i64::try_from(rank).unwrap_or(0),
                            f64::from(*quality),
                        ],
                    )
                    .map_err(|err| statement_failed("gallery_anchor insert", &err))?;
                }

                for delta in &write.deltas {
                    insert_delta(tx, &key, delta, &now)?;
                }

                for outlier in &write.outliers {
                    tx.execute(
                        "INSERT INTO gallery_outlier
                           (photo_id, project_id, node_id, residual_cct, residual_tint,
                            residual_exposure, residual_skin_de00, worst_identity, deviation,
                            reasons, analysis_ver, created_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                        params![
                            outlier.image_id.to_db(),
                            key,
                            outlier.node_id.to_db(),
                            f64::from(outlier.residual_cct),
                            f64::from(outlier.residual_tint),
                            f64::from(outlier.residual_exposure),
                            f64::from(outlier.residual_skin_de00),
                            outlier.worst_identity.map(|i| i.to_db()),
                            f64::from(outlier.deviation.clamp(0.0, 1.0)),
                            i64::from(GalleryReason::to_bits(&outlier.reasons)),
                            i64::from(outlier.analysis_ver),
                            now,
                        ],
                    )
                    .map_err(|err| statement_failed("gallery_outlier insert", &err))?;
                }
            }

            for target in &skin {
                tx.execute(
                    "INSERT INTO gallery_skin_target
                       (identity_id, project_id, u, v, luma, frames, spread_before, spread_after,
                        analysis_ver, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![
                        target.identity.to_db(),
                        key,
                        f64::from(target.uv[0]),
                        f64::from(target.uv[1]),
                        f64::from(target.luma.clamp(0.0, 1.0)),
                        i64::from(target.frames),
                        f64::from(target.spread_before.max(0.0)),
                        f64::from(target.spread_after.max(0.0)),
                        i64::from(target.analysis_ver),
                        now,
                    ],
                )
                .map_err(|err| statement_failed("gallery_skin_target insert", &err))?;
            }
            Ok(())
        })
    }

    /// Re-apply a photographer's pins and rejections to a freshly written tree.
    ///
    /// Run after [`GalleryStore::write_pass`], because a pin belongs to a photograph rather than to
    /// a node and the node it lands in may be a different one from last time. A pin whose
    /// photograph is no longer an anchor of anything is inserted as a rejected-quality anchor of
    /// whichever node the photograph is now in, which is what makes a pin survive a re-shaped tree.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a statement fails.
    pub fn restore_decisions(&self, project: ProjectId, decisions: &Decisions) -> AuraResult<()> {
        let key = project.to_db();
        let pinned: Vec<String> = decisions.pinned.iter().map(ImageId::to_db).collect();
        let rejected: Vec<String> = decisions.rejected.iter().map(ImageId::to_db).collect();
        let disabled: Vec<String> = decisions.disabled.iter().map(ImageId::to_db).collect();
        let overrides: Vec<(String, [f32; 5])> = decisions
            .overrides
            .iter()
            .map(|(image, values)| (image.to_db(), *values))
            .collect();
        self.catalog.writer().transact(move |tx| {
            for image in &pinned {
                // The photograph's current node, which is the delta row's own. A pin that pointed
                // at a node that no longer exists would be a pin nobody could see.
                let node: Option<String> = tx
                    .query_row(
                        "SELECT node_id FROM gallery_delta WHERE photo_id = ?1 AND project_id = ?2",
                        params![image, key],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|err| statement_failed("gallery_delta node", &err))?;
                let Some(node) = node else { continue };
                tx.execute(
                    "INSERT INTO gallery_anchor (node_id, photo_id, rank, quality, user_pinned,
                                                 user_rejected)
                     VALUES (?1, ?2, 0, 1.0, 1, 0)
                     ON CONFLICT(node_id, photo_id) DO UPDATE SET user_pinned = 1, rank = 0",
                    params![node, image],
                )
                .map_err(|err| statement_failed("gallery_anchor pin", &err))?;
            }
            for image in &rejected {
                let node: Option<String> = tx
                    .query_row(
                        "SELECT node_id FROM gallery_delta WHERE photo_id = ?1 AND project_id = ?2",
                        params![image, key],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|err| statement_failed("gallery_delta node", &err))?;
                let Some(node) = node else { continue };
                tx.execute(
                    "INSERT INTO gallery_anchor (node_id, photo_id, rank, quality, user_pinned,
                                                 user_rejected)
                     VALUES (?1, ?2, 999, 0.0, 0, 1)
                     ON CONFLICT(node_id, photo_id) DO UPDATE SET user_rejected = 1,
                                                                 user_pinned = 0",
                    params![node, image],
                )
                .map_err(|err| statement_failed("gallery_anchor reject", &err))?;
            }
            for image in &disabled {
                tx.execute(
                    "UPDATE gallery_delta
                        SET enabled = 0, d_exposure = 0.0, d_cct = 0.0, d_tint = 0.0,
                            d_contrast = 0.0, d_saturation = 0.0, skin_identity = NULL,
                            skin_du = NULL, skin_dv = NULL, skin_dluma = NULL,
                            skin_de00_before = NULL, skin_de00_after = NULL, skin_cap = NULL,
                            skin_capped = NULL
                      WHERE photo_id = ?1 AND project_id = ?2",
                    params![image, key],
                )
                .map_err(|err| statement_failed("gallery_delta disable", &err))?;
            }
            for (image, values) in &overrides {
                tx.execute(
                    "UPDATE gallery_delta
                        SET user_edited = 1, d_exposure = ?3, d_cct = ?4, d_tint = ?5,
                            d_contrast = ?6, d_saturation = ?7
                      WHERE photo_id = ?1 AND project_id = ?2",
                    params![
                        image,
                        key,
                        f64::from(values[0]),
                        f64::from(values[1]),
                        f64::from(values[2]),
                        f64::from(values[3]),
                        f64::from(values[4]),
                    ],
                )
                .map_err(|err| statement_failed("gallery_delta override", &err))?;
            }
            Ok(())
        })
    }

    /// Every node of a project, in capture order of their first frame.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn nodes(&self, project: ProjectId) -> AuraResult<Vec<SceneNode>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT node_id, parent_id, segment_id, label, scene, cct_k, cct_tol, tint,
                            tint_tol, subject_luma, luma_tol, contrast, saturation,
                            grade_signature, anchor_count, cohesion, reasons, analysis_ver,
                            policy_ver
                       FROM gallery_node WHERE project_id = ?1 ORDER BY first_ts, node_id",
                )
                .map_err(|err| statement_failed("gallery_node select", &err))?;
            let rows = statement
                .query_map(params![key], read_node)
                .map_err(|err| statement_failed("gallery_node select", &err))?;
            let mut out = Vec::new();
            for row in rows {
                let mut node = row.map_err(|err| statement_failed("gallery_node row", &err))?;
                node.image_ids = Self::images_in_conn(conn, node.id)?;
                node.anchors = Self::anchors_in_conn(conn, node.id)?;
                out.push(node);
            }
            Ok(out)
        })
    }

    /// One node, or `None`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn node(&self, node: NodeId) -> AuraResult<Option<SceneNode>> {
        let key = node.to_db();
        self.catalog.read(move |conn| {
            let found: Option<SceneNode> = conn
                .query_row(
                    "SELECT node_id, parent_id, segment_id, label, scene, cct_k, cct_tol, tint,
                            tint_tol, subject_luma, luma_tol, contrast, saturation,
                            grade_signature, anchor_count, cohesion, reasons, analysis_ver,
                            policy_ver
                       FROM gallery_node WHERE node_id = ?1",
                    params![key],
                    read_node,
                )
                .optional()
                .map_err(|err| statement_failed("gallery_node one", &err))?;
            let Some(mut node) = found else {
                return Ok(None);
            };
            node.image_ids = Self::images_in_conn(conn, node.id)?;
            node.anchors = Self::anchors_in_conn(conn, node.id)?;
            Ok(Some(node))
        })
    }

    /// One photograph's delta, or `None`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn delta(&self, image: ImageId) -> AuraResult<Option<NormalisationDelta>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(DELTA_COLUMNS, params![key], read_delta)
                .optional()
                .map_err(|err| statement_failed("gallery_delta one", &err))
        })
    }

    /// Every delta inside one node, in capture order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn deltas_in(&self, node: NodeId) -> AuraResult<Vec<NormalisationDelta>> {
        let key = node.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT d.photo_id, d.node_id, d.d_exposure, d.d_cct, d.d_tint, d.d_contrast,
                            d.d_saturation, d.from_exposure_ev, d.from_cct_k, d.from_tint,
                            d.damping, d.bounded_by, d.confidence, d.user_edited, d.reasons,
                            d.analysis_ver, d.policy_ver, d.skin_identity, d.skin_du, d.skin_dv,
                            d.skin_dluma, d.skin_de00_before, d.skin_de00_after, d.skin_cap,
                            d.skin_capped
                       FROM gallery_delta d
                       JOIN photo p ON p.photo_id = d.photo_id
                      WHERE d.node_id = ?1
                      ORDER BY p.timeline_time, d.photo_id",
                )
                .map_err(|err| statement_failed("gallery_delta in node", &err))?;
            let rows = statement
                .query_map(params![key], read_delta)
                .map_err(|err| statement_failed("gallery_delta in node", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("gallery_delta row", &err))?);
            }
            Ok(out)
        })
    }

    /// Every frame still out of line, worst first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn outliers(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<Outlier>> {
        let key = project.to_db();
        let limit = i64::try_from(limit.min(10_000)).unwrap_or(100);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, node_id, residual_cct, residual_tint, residual_exposure,
                            residual_skin_de00, worst_identity, deviation, reasons, analysis_ver
                       FROM gallery_outlier
                      WHERE project_id = ?1
                      ORDER BY deviation DESC, photo_id
                      LIMIT ?2",
                )
                .map_err(|err| statement_failed("gallery_outlier select", &err))?;
            let rows = statement
                .query_map(params![key, limit], |row| {
                    Ok(Outlier {
                        image_id: parse_image(&row.get::<_, String>(0)?),
                        node_id: parse_node(&row.get::<_, String>(1)?),
                        residual_cct: row.get::<_, f64>(2)? as f32,
                        residual_tint: row.get::<_, f64>(3)? as f32,
                        residual_exposure: row.get::<_, f64>(4)? as f32,
                        residual_skin_de00: row.get::<_, f64>(5)? as f32,
                        worst_identity: row
                            .get::<_, Option<String>>(6)?
                            .and_then(|text| IdentityId::from_db(&text).ok()),
                        deviation: row.get::<_, f64>(7)? as f32,
                        reasons: GalleryReason::from_bits(row.get::<_, i64>(8)? as u32),
                        analysis_ver: row.get::<_, i64>(9)? as u16,
                    })
                })
                .map_err(|err| statement_failed("gallery_outlier select", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("gallery_outlier row", &err))?);
            }
            Ok(out)
        })
    }

    /// One identity's gallery skin target, or `None`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn skin_target(&self, identity: IdentityId) -> AuraResult<Option<SkinTarget>> {
        let key = identity.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT identity_id, u, v, luma, frames, spread_before, spread_after, analysis_ver
                   FROM gallery_skin_target WHERE identity_id = ?1",
                params![key],
                read_skin,
            )
            .optional()
            .map_err(|err| statement_failed("gallery_skin_target one", &err))
        })
    }

    /// Every usable skin target in a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn skin_targets(&self, project: ProjectId) -> AuraResult<Vec<SkinTarget>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT identity_id, u, v, luma, frames, spread_before, spread_after,
                            analysis_ver
                       FROM gallery_skin_target WHERE project_id = ?1 ORDER BY identity_id",
                )
                .map_err(|err| statement_failed("gallery_skin_target select", &err))?;
            let rows = statement
                .query_map(params![key], read_skin)
                .map_err(|err| statement_failed("gallery_skin_target select", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("gallery_skin_target row", &err))?);
            }
            Ok(out)
        })
    }

    /// Which node a photograph is in.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn node_of(&self, image: ImageId) -> AuraResult<Option<NodeId>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let found: Option<String> = conn
                .query_row(
                    "SELECT node_id FROM gallery_delta WHERE photo_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|err| statement_failed("gallery_delta node_of", &err))?;
            Ok(found.and_then(|text| NodeId::from_db(&text).ok()))
        })
    }

    /// Pin or reject one photograph as an anchor of its node.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5124` when the photograph is not in the node.
    pub fn set_anchor(&self, node: NodeId, image: ImageId, pinned: bool) -> AuraResult<()> {
        let node_key = node.to_db();
        let image_key = image.to_db();
        self.catalog.writer().transact(move |tx| {
            let belongs: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM gallery_delta WHERE photo_id = ?1 AND node_id = ?2",
                    params![image_key, node_key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("gallery_delta membership", &err))?;
            if belongs == 0 {
                return Err(aura_core::errors::ml::gallery_anchor_refused(format!(
                    "photograph {image_key} is not in node {node_key}"
                )));
            }
            let (pin, reject) = if pinned { (1, 0) } else { (0, 1) };
            tx.execute(
                "INSERT INTO gallery_anchor (node_id, photo_id, rank, quality, user_pinned,
                                             user_rejected)
                 VALUES (?1, ?2, 0, 1.0, ?3, ?4)
                 ON CONFLICT(node_id, photo_id)
                 DO UPDATE SET user_pinned = ?3, user_rejected = ?4",
                params![node_key, image_key, pin, reject],
            )
            .map_err(|err| statement_failed("gallery_anchor set", &err))?;
            Ok(())
        })
    }

    /// Record what the photographer set instead, on one frame.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5125` when the photograph has no delta.
    pub fn set_override(&self, image: ImageId, values: [Option<f32>; 5]) -> AuraResult<()> {
        let key = image.to_db();
        self.catalog.writer().transact(move |tx| {
            let changed = tx
                .execute(
                    "UPDATE gallery_delta
                        SET user_edited = 1,
                            d_exposure   = COALESCE(?2, d_exposure),
                            d_cct        = COALESCE(?3, d_cct),
                            d_tint       = COALESCE(?4, d_tint),
                            d_contrast   = COALESCE(?5, d_contrast),
                            d_saturation = COALESCE(?6, d_saturation)
                      WHERE photo_id = ?1 AND enabled = 1",
                    params![
                        key,
                        values[0].map(f64::from),
                        values[1].map(f64::from),
                        values[2].map(f64::from),
                        values[3].map(f64::from),
                        values[4].map(f64::from),
                    ],
                )
                .map_err(|err| statement_failed("gallery_delta override", &err))?;
            if changed == 0 {
                return Err(aura_core::errors::ml::gallery_override_refused(format!(
                    "photograph {key} has no gallery delta, or the pass is switched off for it"
                )));
            }
            Ok(())
        })
    }

    /// Switch the consistency pass off for one photograph, or back on.
    ///
    /// Switching off zeroes the movement in the same statement, because migration 25 refuses a row
    /// that is disabled and still carries one - the panel cannot produce that state and a raw
    /// UPDATE that set only the flag would be refused rather than silently leaving a stale delta.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5125` when the photograph has no delta.
    pub fn set_enabled(&self, image: ImageId, enabled: bool) -> AuraResult<()> {
        let key = image.to_db();
        self.catalog.writer().transact(move |tx| {
            let changed = if enabled {
                tx.execute(
                    "UPDATE gallery_delta SET enabled = 1 WHERE photo_id = ?1",
                    params![key],
                )
            } else {
                tx.execute(
                    "UPDATE gallery_delta
                        SET enabled = 0, d_exposure = 0.0, d_cct = 0.0, d_tint = 0.0,
                            d_contrast = 0.0, d_saturation = 0.0, skin_identity = NULL,
                            skin_du = NULL, skin_dv = NULL, skin_dluma = NULL,
                            skin_de00_before = NULL, skin_de00_after = NULL, skin_cap = NULL,
                            skin_capped = NULL
                      WHERE photo_id = ?1",
                    params![key],
                )
            }
            .map_err(|err| statement_failed("gallery_delta enable", &err))?;
            if changed == 0 {
                return Err(aura_core::errors::ml::gallery_override_refused(format!(
                    "photograph {key} has no gallery delta"
                )));
            }
            Ok(())
        })
    }

    /// What the panel's project header shows.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a query fails.
    pub fn outline(
        &self,
        project: ProjectId,
        untargeted: Vec<String>,
    ) -> AuraResult<GalleryOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut outline = GalleryOutline {
                untargeted_scenes: untargeted.clone(),
                ..GalleryOutline::default()
            };
            let row = conn
                .query_row(
                    "SELECT photos, normalised, nodes, anchored_nodes, outliers, pinned_anchors,
                            skin_targets
                       FROM v_gallery_coverage WHERE project_id = ?1",
                    params![key.clone()],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    },
                )
                .optional()
                .map_err(|err| statement_failed("v_gallery_coverage", &err))?;
            let Some((photos, normalised, nodes, anchored, outliers, pinned, skin)) = row else {
                return Ok(outline);
            };
            outline.photos = photos.max(0) as u32;
            outline.normalised = normalised.max(0) as u32;
            outline.placed = outline.normalised;
            outline.nodes = nodes.max(0) as u32;
            outline.anchored_nodes = anchored.max(0) as u32;
            outline.outliers = outliers.max(0) as u32;
            outline.pinned_anchors = pinned.max(0) as u32;
            outline.skin_targeted = skin.max(0) as u32;
            outline.coverage = if outline.photos == 0 {
                0.0
            } else {
                outline.normalised as f32 / outline.photos as f32
            };

            let (bounded, mood, edited, mean_cct, mean_ev) = conn
                .query_row(
                    "SELECT
                       SUM(CASE WHEN bounded_by IS NOT NULL THEN 1 ELSE 0 END),
                       SUM(CASE WHEN (reasons & ?2) <> 0 THEN 1 ELSE 0 END),
                       SUM(user_edited),
                       AVG(ABS(d_cct)),
                       AVG(ABS(d_exposure))
                     FROM gallery_delta WHERE project_id = ?1",
                    params![key.clone(), i64::from(GalleryCode::MoodPreserved.bit())],
                    |row| {
                        Ok((
                            row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                            row.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                            row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                        ))
                    },
                )
                .map_err(|err| statement_failed("gallery_delta aggregate", &err))?;
            outline.bounded = bounded.max(0) as u32;
            outline.mood_preserved = mood.max(0) as u32;
            outline.user_edited = edited.max(0) as u32;
            outline.mean_d_cct = mean_cct as f32;
            outline.mean_d_ev = mean_ev as f32;

            let split: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM gallery_node
                      WHERE project_id = ?1 AND parent_id IS NOT NULL",
                    params![key.clone()],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("gallery_node split count", &err))?;
            outline.split_nodes = split.max(0) as u32;

            let (identities, worst) = conn
                .query_row(
                    "SELECT COUNT(*), COALESCE(MAX(spread_after), 0.0)
                       FROM gallery_skin_target WHERE project_id = ?1",
                    params![key.clone()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?)),
                )
                .map_err(|err| statement_failed("gallery_skin_target stats", &err))?;
            outline.identities = identities.max(0) as u32;
            outline.worst_skin_spread = worst as f32;

            let (analysis, policy) = conn
                .query_row(
                    "SELECT COALESCE(MAX(analysis_ver), 0), COALESCE(MAX(policy_ver), 0)
                       FROM gallery_node WHERE project_id = ?1",
                    params![key],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .map_err(|err| statement_failed("gallery_node versions", &err))?;
            outline.analysis_ver = analysis as u16;
            outline.policy_ver = policy as u16;

            Ok(outline)
        })
    }

    /// Record the before-and-after spreads the headline gate is measured from.
    ///
    /// Written by the pass rather than derived, because "the spread before" is a property of the
    /// frames the pass saw and cannot be recovered from a table of deltas: a frame that was
    /// disabled contributes to the before and not to the after, and a node that could not be
    /// anchored contributes to neither.
    pub fn set_spreads(&self, outline: &mut GalleryOutline, before: (f32, f32), after: (f32, f32)) {
        outline.spread_before_cct = before.0;
        outline.spread_before_ev = before.1;
        outline.spread_after_cct = after.0;
        outline.spread_after_ev = after.1;
        let _ = &self.clock;
    }

    fn images_in_conn(conn: &rusqlite::Connection, node: NodeId) -> AuraResult<Vec<ImageId>> {
        let mut statement = conn
            .prepare(
                "SELECT d.photo_id FROM gallery_delta d
                   JOIN photo p ON p.photo_id = d.photo_id
                  WHERE d.node_id = ?1 ORDER BY p.timeline_time, d.photo_id",
            )
            .map_err(|err| statement_failed("gallery_delta images", &err))?;
        let rows = statement
            .query_map(params![node.to_db()], |row| row.get::<_, String>(0))
            .map_err(|err| statement_failed("gallery_delta images", &err))?;
        let mut out = Vec::new();
        for row in rows {
            let text = row.map_err(|err| statement_failed("photo_id", &err))?;
            if let Ok(id) = ImageId::from_db(&text) {
                out.push(id);
            }
        }
        Ok(out)
    }

    fn anchors_in_conn(conn: &rusqlite::Connection, node: NodeId) -> AuraResult<Vec<ImageId>> {
        let mut statement = conn
            .prepare(
                "SELECT photo_id FROM gallery_anchor
                  WHERE node_id = ?1 AND user_rejected = 0
                  ORDER BY user_pinned DESC, rank, photo_id",
            )
            .map_err(|err| statement_failed("gallery_anchor select", &err))?;
        let rows = statement
            .query_map(params![node.to_db()], |row| row.get::<_, String>(0))
            .map_err(|err| statement_failed("gallery_anchor select", &err))?;
        let mut out = Vec::new();
        for row in rows {
            let text = row.map_err(|err| statement_failed("photo_id", &err))?;
            if let Ok(id) = ImageId::from_db(&text) {
                out.push(id);
            }
        }
        Ok(out)
    }
}

/// What a photographer decided, carried across a re-analysis.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decisions {
    /// Frames pinned as anchors.
    pub pinned: BTreeSet<ImageId>,
    /// Frames rejected as anchors.
    pub rejected: BTreeSet<ImageId>,
    /// Frames whose delta the photographer set, and what they set.
    pub overrides: BTreeMap<ImageId, [f32; 5]>,
    /// Frames the pass is switched off for.
    pub disabled: BTreeSet<ImageId>,
}

impl Decisions {
    /// True when the photographer has said nothing about this project.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pinned.is_empty()
            && self.rejected.is_empty()
            && self.overrides.is_empty()
            && self.disabled.is_empty()
    }

    /// How many decisions there are, for the pass's log line.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pinned.len() + self.rejected.len() + self.overrides.len() + self.disabled.len()
    }
}

const DELTA_COLUMNS: &str = "SELECT photo_id, node_id, d_exposure, d_cct, d_tint, d_contrast,
        d_saturation, from_exposure_ev, from_cct_k, from_tint, damping, bounded_by, confidence,
        user_edited, reasons, analysis_ver, policy_ver, skin_identity, skin_du, skin_dv,
        skin_dluma, skin_de00_before, skin_de00_after, skin_cap, skin_capped
   FROM gallery_delta WHERE photo_id = ?1";

fn insert_delta(
    tx: &rusqlite::Transaction<'_>,
    project: &str,
    delta: &NormalisationDelta,
    now: &str,
) -> AuraResult<()> {
    let skin = delta.skin_correction;
    tx.execute(
        "INSERT INTO gallery_delta
           (photo_id, project_id, node_id, d_exposure, d_cct, d_tint, d_contrast, d_saturation,
            from_exposure_ev, from_cct_k, from_tint, damping, bounded_by, confidence,
            skin_identity, skin_du, skin_dv, skin_dluma, skin_de00_before, skin_de00_after,
            skin_cap, skin_capped, user_edited, enabled, reasons, analysis_ver, policy_ver,
            updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,
                 ?23,1,?24,?25,?26,?27)",
        params![
            delta.image_id.to_db(),
            project,
            delta.node_id.to_db(),
            f64::from(delta.d_exposure),
            f64::from(delta.d_cct),
            f64::from(delta.d_tint),
            f64::from(delta.d_contrast),
            f64::from(delta.d_saturation),
            f64::from(delta.from_exposure_ev),
            f64::from(delta.from_cct_k),
            f64::from(delta.from_tint),
            f64::from(delta.damping),
            delta.bounded_by.map(Bound::as_str),
            f64::from(delta.confidence.clamp(0.0, 1.0)),
            skin.map(|s| s.identity.to_db()),
            skin.map(|s| f64::from(s.d_uv[0])),
            skin.map(|s| f64::from(s.d_uv[1])),
            skin.map(|s| f64::from(s.d_luma)),
            skin.map(|s| f64::from(s.de00_before)),
            skin.map(|s| f64::from(s.de00_after)),
            skin.map(|s| f64::from(s.cap)),
            skin.map(|s| i64::from(s.capped)),
            i64::from(delta.user_edited),
            i64::from(GalleryReason::to_bits(&delta.reasons)),
            i64::from(delta.analysis_ver),
            i64::from(delta.policy_ver),
            now,
        ],
    )
    .map_err(|err| statement_failed("gallery_delta insert", &err))?;
    Ok(())
}

fn read_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<SceneNode> {
    let cct: Option<f64> = row.get(5)?;
    let signature: Option<String> = row.get(13)?;
    let target = match (cct, signature) {
        (Some(cct), Some(signature)) => {
            let values: [f32; 8] = serde_json::from_str(&signature).unwrap_or([0.0; 8]);
            Some(NodeTarget {
                cct_k: cct as f32,
                cct_tol: row.get::<_, Option<f64>>(6)?.unwrap_or(0.0) as f32,
                tint: row.get::<_, Option<f64>>(7)?.unwrap_or(0.0) as f32,
                tint_tol: row.get::<_, Option<f64>>(8)?.unwrap_or(0.0) as f32,
                subject_luma: row.get::<_, Option<f64>>(9)?.unwrap_or(0.0) as f32,
                luma_tol: row.get::<_, Option<f64>>(10)?.unwrap_or(0.0) as f32,
                contrast: row.get::<_, Option<f64>>(11)?.unwrap_or(0.0) as f32,
                saturation: row.get::<_, Option<f64>>(12)?.unwrap_or(0.0) as f32,
                grade_signature: values,
                anchor_count: row.get::<_, i64>(14)? as u16,
                cohesion: row.get::<_, f64>(15)? as f32,
            })
        }
        _ => None,
    };
    Ok(SceneNode {
        id: parse_node(&row.get::<_, String>(0)?),
        parent: row
            .get::<_, Option<String>>(1)?
            .and_then(|text| NodeId::from_db(&text).ok()),
        segment_id: SegmentId::from_db(&row.get::<_, String>(2)?).unwrap_or_default(),
        label: row.get(3)?,
        image_ids: Vec::new(),
        anchors: Vec::new(),
        target,
        scene: SceneId::from_str_or_unknown(&row.get::<_, String>(4)?),
        reasons: GalleryReason::from_bits(row.get::<_, i64>(16)? as u32),
        analysis_ver: row.get::<_, i64>(17)? as u16,
        policy_ver: row.get::<_, i64>(18)? as u16,
    })
}

fn read_delta(row: &rusqlite::Row<'_>) -> rusqlite::Result<NormalisationDelta> {
    let identity: Option<String> = row.get(17)?;
    let skin = match identity.and_then(|text| IdentityId::from_db(&text).ok()) {
        Some(identity) => Some(SkinCorrection {
            identity,
            d_uv: [
                row.get::<_, Option<f64>>(18)?.unwrap_or(0.0) as f32,
                row.get::<_, Option<f64>>(19)?.unwrap_or(0.0) as f32,
            ],
            d_luma: row.get::<_, Option<f64>>(20)?.unwrap_or(0.0) as f32,
            de00_before: row.get::<_, Option<f64>>(21)?.unwrap_or(0.0) as f32,
            de00_after: row.get::<_, Option<f64>>(22)?.unwrap_or(0.0) as f32,
            cap: row.get::<_, Option<f64>>(23)?.unwrap_or(0.0) as f32,
            capped: row.get::<_, Option<i64>>(24)?.unwrap_or(0) == 1,
        }),
        None => None,
    };
    Ok(NormalisationDelta {
        image_id: parse_image(&row.get::<_, String>(0)?),
        node_id: parse_node(&row.get::<_, String>(1)?),
        d_exposure: row.get::<_, f64>(2)? as f32,
        d_cct: row.get::<_, f64>(3)? as f32,
        d_tint: row.get::<_, f64>(4)? as f32,
        d_contrast: row.get::<_, f64>(5)? as f32,
        d_saturation: row.get::<_, f64>(6)? as f32,
        skin_correction: skin,
        from_exposure_ev: row.get::<_, f64>(7)? as f32,
        from_cct_k: row.get::<_, f64>(8)? as f32,
        from_tint: row.get::<_, f64>(9)? as f32,
        damping: row.get::<_, f64>(10)? as f32,
        bounded_by: row
            .get::<_, Option<String>>(11)?
            .as_deref()
            .map(Bound::from_str_or_cct),
        reasons: GalleryReason::from_bits(row.get::<_, i64>(14)? as u32),
        confidence: row.get::<_, f64>(12)? as f32,
        user_edited: row.get::<_, i64>(13)? == 1,
        analysis_ver: row.get::<_, i64>(15)? as u16,
        policy_ver: row.get::<_, i64>(16)? as u16,
    })
}

fn read_skin(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkinTarget> {
    Ok(SkinTarget {
        identity: IdentityId::from_db(&row.get::<_, String>(0)?).unwrap_or_default(),
        uv: [row.get::<_, f64>(1)? as f32, row.get::<_, f64>(2)? as f32],
        luma: row.get::<_, f64>(3)? as f32,
        frames: row.get::<_, i64>(4)? as u32,
        spread_before: row.get::<_, f64>(5)? as f32,
        spread_after: row.get::<_, f64>(6)? as f32,
        analysis_ver: row.get::<_, i64>(7)? as u16,
    })
}

/// A stored id that will not parse is a corrupt row rather than a recoverable one.
///
/// A fresh id is returned so a read of four thousand rows is not lost to one of them, and the row
/// it belongs to will be re-solved on the next pass because nothing points at the id it was given.
/// The alternative - propagating an error - turns one bad byte into a panel that cannot open.
fn parse_image(text: &str) -> ImageId {
    ImageId::from_db(text).unwrap_or_default()
}

fn parse_node(text: &str) -> NodeId {
    NodeId::from_db(text).unwrap_or_default()
}

/// Raise `AURA-ML-5127` when a project's stored rows came from a different build.
///
/// Called by the pass before it reads anything, so a comparison across versions never happens
/// silently. Tenth phase, tenth version-drift code.
///
/// # Errors
///
/// `AURA-ML-5127` when the versions differ.
pub fn check_versions(stored: Option<(u16, u16)>, current: (u16, u16)) -> AuraResult<()> {
    match stored {
        Some(stored) if stored != current => Err(errors::version_drift(stored, current)),
        _ => Ok(()),
    }
}
