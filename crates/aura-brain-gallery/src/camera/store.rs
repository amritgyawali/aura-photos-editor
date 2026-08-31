//! The five tables migration 26 adds, and the rules that live in the SQL rather than around them.
//!
//! ## The row count does not grow with the wedding
//!
//! Four of the five tables are per body, per shooter or per project: a three-camera wedding has six
//! fingerprints and six transforms whether it is four hundred photographs or six thousand. Only
//! `camera_pair` scales, and it is bounded twice - a pair must be inside one scene node and inside
//! the policy's own gap, and [`MAX_PAIRS_PER_CAMERA`] caps what is kept per body. That is why this
//! phase has no per-image storage budget while phases 09 to 25 all do.
//!
//! ## A decision a photographer made is read out before the project is cleared
//!
//! [`CameraStore::take_decisions`] pulls the reference choice, the switched-off bodies and the
//! hand-set transforms out before [`CameraStore::write_pass`] clears anything, and the pass puts
//! them back. Phase 25's mechanism, and phase 18's lesson about why the DELETE guard alone is not
//! enough: these tables are written with `INSERT OR REPLACE` against their primary keys, and an
//! `INSERT OR REPLACE` deletes the row it conflicts with whatever the DELETE excluded.
//!
//! ## Every JSON column is read as a unit and never queried into
//!
//! `sat_response`, `contrast_response`, `grade_signature`, `channel_gain`, `contrast_shape` and the
//! four appearance distances are stored as JSON. None of them is ever filtered on, and expanding
//! them into columns would be thirty-one columns nobody queries - while the two things a CHECK
//! constraint has to be able to see, the worst channel-gain and contrast-shape departures, are
//! denormalised beside them precisely so the constraint can. Two columns to make one promise
//! enforceable is cheaper than a promise that is only tested.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::camera::{
    AppearanceDistance, Brand, CameraFingerprint, CameraOutline, CameraReason, CameraTransform,
    FlashState, MatchedPair, Reference, ReferenceSource, ShooterBias, SkinCorrection,
    TransformBound, TransformSource,
};
use aura_core::contract::gallery::ImageId;
use aura_core::contract::ids::{NodeId, PairId};
use aura_core::contract::moment::CameraId;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use super::MAX_PAIRS_PER_CAMERA;

/// One catalog, wrapped.
#[derive(Debug, Clone)]
pub struct CameraStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

/// Everything one matching pass produced, as one unit to write.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PassWrite {
    /// Which body everything was matched to.
    pub reference: Option<Reference>,
    /// Every body's colour response, in both flash states.
    pub fingerprints: Vec<CameraFingerprint>,
    /// Every body's correction, in both flash states.
    pub transforms: Vec<CameraTransform>,
    /// The pairs the corrections rest on, verified and rejected alike.
    pub pairs: Vec<MatchedPair>,
    /// Every measured exposure habit.
    pub shooter_bias: Vec<ShooterBias>,
}

/// What a photographer decided, read out before a re-pass clears the project.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Decisions {
    /// The reference body they chose, when they chose one.
    pub reference: Option<CameraId>,
    /// The bodies they switched matching off for.
    pub disabled: BTreeSet<String>,
    /// The transforms they set by hand, keyed by body and flash state.
    pub overrides: BTreeMap<(String, FlashState), [f32; 4]>,
}

impl Decisions {
    /// True when the photographer decided nothing about this project.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.reference.is_none() && self.disabled.is_empty() && self.overrides.is_empty()
    }
}

impl CameraStore {
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

    /// True when a project's stored rows came from this build's arithmetic and this policy table.
    ///
    /// Whole-project rather than per-photograph, exactly as phase 25's is and for a stronger version
    /// of the same reason: a transform is a statement about a **body**, and a project whose Sony was
    /// solved under one policy and whose Canon was solved under another has been matched to two
    /// different promises. Invariant 5: the work remaining is a query rather than a journal.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn is_current(&self, project: ProjectId, versions: (u16, u16)) -> AuraResult<bool> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let stale: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM camera_transform
                      WHERE project_id = ?1 AND (analysis_ver <> ?2 OR policy_ver <> ?3)",
                    params![key, i64::from(versions.0), i64::from(versions.1)],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("camera_transform version check", &err))?;
            let present: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM camera_transform WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("camera_transform count", &err))?;
            Ok(present > 0 && stale == 0)
        })
    }

    /// The versions a project's rows carry, or `None` when it has none.
    ///
    /// What `AURA-ML-5132` is raised from.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn stored_versions(&self, project: ProjectId) -> AuraResult<Option<(u16, u16)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT analysis_ver, policy_ver FROM camera_transform
                  WHERE project_id = ?1 LIMIT 1",
                params![key],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|err| statement_failed("camera_transform versions", &err))
            .map(|found| {
                found.map(|(analysis, policy)| {
                    (
                        u16::try_from(analysis).unwrap_or(0),
                        u16::try_from(policy).unwrap_or(0),
                    )
                })
            })
        })
    }

    /// Read out everything a photographer decided, before a re-pass clears the project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a statement fails.
    pub fn take_decisions(&self, project: ProjectId) -> AuraResult<Decisions> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let reference: Option<String> = conn
                .query_row(
                    "SELECT camera_id FROM camera_reference
                      WHERE project_id = ?1 AND source = 'user'",
                    params![key.clone()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|err| statement_failed("camera_reference decision", &err))?;

            let mut disabled = BTreeSet::new();
            let mut overrides = BTreeMap::new();
            let mut statement = conn
                .prepare(
                    "SELECT camera_id, flash, enabled, user_edited,
                            d_cct, d_tint, d_exposure, d_saturation
                       FROM camera_transform
                      WHERE project_id = ?1 AND (enabled = 0 OR user_edited = 1)",
                )
                .map_err(|err| statement_failed("camera_transform decisions", &err))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        [
                            row.get::<_, f64>(4)? as f32,
                            row.get::<_, f64>(5)? as f32,
                            row.get::<_, f64>(6)? as f32,
                            row.get::<_, f64>(7)? as f32,
                        ],
                    ))
                })
                .map_err(|err| statement_failed("camera_transform decisions", &err))?;
            for row in rows {
                let (camera, flash, enabled, edited, values) =
                    row.map_err(|err| statement_failed("camera_transform row", &err))?;
                if enabled == 0 {
                    disabled.insert(camera.clone());
                }
                if edited == 1 {
                    overrides.insert((camera, FlashState::from_str_or_ambient(&flash)), values);
                }
            }

            Ok(Decisions {
                reference: reference.map(CameraId::new),
                disabled,
                overrides,
            })
        })
    }

    /// Put a photographer's decisions back onto a freshly solved project, in place.
    ///
    /// Applied to the pass's own rows **before** they are written rather than to the catalog
    /// afterwards, which is the only ordering that is safe against a crash: a pass that wrote its
    /// own answers and then re-applied the overrides would leave a window in which a photographer's
    /// decision had been lost from disk.
    pub fn restore_decisions(write: &mut PassWrite, decisions: &Decisions) {
        if let (Some(chosen), Some(reference)) =
            (decisions.reference.as_ref(), write.reference.as_mut())
        {
            if &reference.camera_id != chosen {
                reference.camera_id = chosen.clone();
            }
            reference.source = ReferenceSource::User;
        }
        for transform in &mut write.transforms {
            let key = transform.camera_id.as_str().to_string();
            if decisions.disabled.contains(&key) {
                transform.enabled = false;
                transform.reasons.push(CameraReason::of(
                    aura_core::contract::camera::CameraCode::Disabled,
                ));
            }
            if let Some(values) = decisions.overrides.get(&(key, transform.flash)) {
                transform.d_cct = values[0];
                transform.d_tint = values[1];
                transform.d_exposure = values[2];
                transform.d_saturation = values[3];
                transform.user_edited = true;
                transform.reasons.push(CameraReason::of(
                    aura_core::contract::camera::CameraCode::UserEdited,
                ));
            }
        }
    }

    /// Replace a project's whole matching result with a freshly solved one.
    ///
    /// One transaction. A project half solved against one reference and half against another has
    /// been matched to nothing, and there is no partial state of these tables a reader could make
    /// sense of - unlike phases 09 to 24, whose rows are independent per photograph.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a statement fails, including when a CHECK refuses a movement outside a
    /// contract ceiling - which is the second layer under `CameraTransform::within_bounds` and is
    /// meant to be unreachable.
    pub fn write_pass(&self, project: ProjectId, write: &PassWrite) -> AuraResult<()> {
        let key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let write = write.clone();
        self.catalog.writer().transact(move |tx| {
            for table in [
                "camera_pair",
                "camera_shooter_bias",
                "camera_transform",
                "camera_fingerprint",
                "camera_reference",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE project_id = ?1"),
                    params![key],
                )
                .map_err(|err| statement_failed("camera table clear", &err))?;
            }

            if let Some(reference) = &write.reference {
                tx.execute(
                    "INSERT INTO camera_reference
                       (project_id, camera_id, source, frames, shooter, analysis_ver,
                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    params![
                        key,
                        reference.camera_id.as_str(),
                        reference.source.as_str(),
                        i64::from(reference.frames),
                        reference.shooter.as_deref(),
                        i64::from(super::ANALYSIS_VER),
                        now
                    ],
                )
                .map_err(|err| statement_failed("camera_reference insert", &err))?;
            }

            for print in &write.fingerprints {
                tx.execute(
                    "INSERT INTO camera_fingerprint
                       (project_id, camera_id, flash, brand, skin_u, skin_v, white_u, white_v,
                        sat_response, contrast_response, highlight_rolloff, grade_signature,
                        subject_luma, samples, confidence, reasons, analysis_ver, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                             ?16, ?17, ?18)",
                    params![
                        key,
                        print.camera_id.as_str(),
                        print.flash.as_str(),
                        print.brand.as_str(),
                        f64::from(print.skin_chroma[0]),
                        f64::from(print.skin_chroma[1]),
                        f64::from(print.white_point[0]),
                        f64::from(print.white_point[1]),
                        encode(&print.sat_response),
                        encode(&print.contrast_response),
                        f64::from(print.highlight_rolloff),
                        encode(&print.grade_signature),
                        f64::from(print.subject_luma),
                        i64::from(print.samples),
                        f64::from(print.confidence),
                        i64::from(CameraReason::to_bits(&print.reasons)),
                        i64::from(print.analysis_ver),
                        now
                    ],
                )
                .map_err(|err| statement_failed("camera_fingerprint insert", &err))?;
            }

            for transform in &write.transforms {
                let skin = transform.skin_correction;
                let max_gain = transform
                    .channel_gain
                    .iter()
                    .map(|g| (g - 1.0).abs())
                    .fold(0.0_f32, f32::max);
                let max_shape = transform
                    .contrast_shape
                    .iter()
                    .map(|c| (c - 1.0).abs())
                    .fold(0.0_f32, f32::max);
                tx.execute(
                    "INSERT INTO camera_transform
                       (project_id, camera_id, flash, reference_id, d_cct, d_tint, d_exposure,
                        d_saturation, channel_gain, contrast_shape, max_gain_dev, max_shape_dev,
                        skin_du, skin_dv, skin_dluma, skin_de00_before, skin_de00_after,
                        skin_locus_valid, skin_capped, source, blend, evidence_pairs,
                        distance_before, distance_after, heldout_before, heldout_after,
                        heldout_pairs, bounded_by, confidence, reasons, enabled, user_edited,
                        analysis_ver, policy_ver, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                             ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28,
                             ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?35)",
                    params![
                        key,
                        transform.camera_id.as_str(),
                        transform.flash.as_str(),
                        transform.reference.as_str(),
                        f64::from(transform.d_cct),
                        f64::from(transform.d_tint),
                        f64::from(transform.d_exposure),
                        f64::from(transform.d_saturation),
                        encode(&transform.channel_gain),
                        encode(&transform.contrast_shape),
                        f64::from(max_gain),
                        f64::from(max_shape),
                        f64::from(skin.d_uv[0]),
                        f64::from(skin.d_uv[1]),
                        f64::from(skin.d_luma),
                        f64::from(skin.de00_before),
                        f64::from(skin.de00_after),
                        i64::from(skin.locus_valid),
                        i64::from(skin.capped),
                        transform.source.as_str(),
                        f64::from(transform.blend),
                        i64::from(transform.evidence_pairs),
                        encode_distance(&transform.distance_before),
                        encode_distance(&transform.distance_after),
                        encode_distance(&transform.heldout_before),
                        encode_distance(&transform.heldout_after),
                        i64::from(transform.heldout_pairs),
                        transform.bounded.map(|bound| bound.as_str()),
                        f64::from(transform.confidence),
                        i64::from(CameraReason::to_bits(&transform.reasons)),
                        i64::from(transform.enabled),
                        i64::from(transform.user_edited),
                        i64::from(transform.analysis_ver),
                        i64::from(transform.policy_ver),
                        now
                    ],
                )
                .map_err(|err| statement_failed("camera_transform insert", &err))?;
            }

            for pair in write.pairs.iter().take(MAX_PAIRS_PER_CAMERA * 8) {
                tx.execute(
                    "INSERT OR IGNORE INTO camera_pair
                       (pair_id, project_id, node_id, left_image, right_image, left_camera,
                        right_camera, flash, gap_ms, subject_similarity, background_agreement,
                        verified, held_out, analysis_ver, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        pair.id.to_db(),
                        key,
                        pair.node.to_db(),
                        pair.left.to_db(),
                        pair.right.to_db(),
                        pair.left_camera.as_str(),
                        pair.right_camera.as_str(),
                        pair.flash.as_str(),
                        pair.gap_ms,
                        f64::from(pair.subject_similarity),
                        f64::from(pair.background_agreement),
                        i64::from(pair.verified),
                        i64::from(pair.held_out),
                        i64::from(pair.analysis_ver),
                        now
                    ],
                )
                .map_err(|err| statement_failed("camera_pair insert", &err))?;
            }

            for row in &write.shooter_bias {
                tx.execute(
                    "INSERT OR REPLACE INTO camera_shooter_bias
                       (project_id, camera_id, scene, shooter, measured_ev, applied_ev, frames,
                        capped, reasons, analysis_ver, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        key,
                        row.camera_id.as_str(),
                        row.scene.as_str(),
                        row.shooter,
                        f64::from(row.measured_ev),
                        f64::from(row.applied_ev),
                        i64::from(row.frames),
                        i64::from(row.capped),
                        i64::from(CameraReason::to_bits(&row.reasons)),
                        i64::from(row.analysis_ver),
                        now
                    ],
                )
                .map_err(|err| statement_failed("camera_shooter_bias insert", &err))?;
            }

            Ok(())
        })
    }

    /// Every fingerprint in a project, by body and then by flash state.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn fingerprints(&self, project: ProjectId) -> AuraResult<Vec<CameraFingerprint>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT camera_id, flash, brand, skin_u, skin_v, white_u, white_v,
                            sat_response, contrast_response, highlight_rolloff, grade_signature,
                            subject_luma, samples, confidence, reasons, analysis_ver
                       FROM camera_fingerprint WHERE project_id = ?1
                      ORDER BY camera_id, flash",
                )
                .map_err(|err| statement_failed("camera_fingerprint select", &err))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok(CameraFingerprint {
                        camera_id: CameraId::new(row.get::<_, String>(0)?),
                        flash: FlashState::from_str_or_ambient(&row.get::<_, String>(1)?),
                        brand: Brand::from_str_or_other(&row.get::<_, String>(2)?),
                        skin_chroma: [row.get::<_, f64>(3)? as f32, row.get::<_, f64>(4)? as f32],
                        white_point: [row.get::<_, f64>(5)? as f32, row.get::<_, f64>(6)? as f32],
                        sat_response: decode4(&row.get::<_, String>(7)?),
                        contrast_response: decode4(&row.get::<_, String>(8)?),
                        highlight_rolloff: row.get::<_, f64>(9)? as f32,
                        grade_signature: decode8(&row.get::<_, String>(10)?),
                        subject_luma: row.get::<_, f64>(11)? as f32,
                        samples: u32::try_from(row.get::<_, i64>(12)?).unwrap_or(0),
                        confidence: row.get::<_, f64>(13)? as f32,
                        reasons: CameraReason::from_bits(
                            u32::try_from(row.get::<_, i64>(14)?).unwrap_or(0),
                        ),
                        analysis_ver: u16::try_from(row.get::<_, i64>(15)?).unwrap_or(0),
                    })
                })
                .map_err(|err| statement_failed("camera_fingerprint rows", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("camera_fingerprint row", &err))?);
            }
            Ok(out)
        })
    }

    /// Every transform in a project, by body and then by flash state.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn transforms(&self, project: ProjectId) -> AuraResult<Vec<CameraTransform>> {
        let key = project.to_db();
        self.catalog
            .read(move |conn| read_transforms(conn, &key, None, None))
    }

    /// One body's transform in one flash state.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn transform(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
    ) -> AuraResult<Option<CameraTransform>> {
        let key = project.to_db();
        let camera = camera.as_str().to_string();
        self.catalog.read(move |conn| {
            Ok(read_transforms(conn, &key, Some(&camera), Some(flash))?
                .into_iter()
                .next())
        })
    }

    /// The transform that applies to one photograph, by resolving its body and flash state.
    ///
    /// **What phase 25 reads.** A disabled body returns `None` rather than an identity, so a caller
    /// cannot confuse "matching is off here" with "matching found nothing to do".
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn transform_for_image(&self, image: ImageId) -> AuraResult<Option<CameraTransform>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let found: Option<(String, String, i64)> = conn
                .query_row(
                    "SELECT p.project_id, c.camera_id, COALESCE(p.flash_fired, 0)
                       FROM photo p
                       JOIN camera c ON c.project_id = p.project_id
                                    AND c.body_serial = COALESCE(p.camera_serial, '')
                      WHERE p.photo_id = ?1",
                    params![key],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(|err| statement_failed("photo camera lookup", &err))?;
            let Some((project, camera, flash)) = found else {
                return Ok(None);
            };
            let flash = FlashState::of(Some(flash == 1));
            let transform = read_transforms(conn, &project, Some(&camera), Some(flash))?
                .into_iter()
                .next();
            Ok(transform.filter(|t| t.enabled))
        })
    }

    /// The verified pairs behind one body's transform, best agreement first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pairs(
        &self,
        project: ProjectId,
        camera: &CameraId,
        limit: usize,
    ) -> AuraResult<Vec<MatchedPair>> {
        let key = project.to_db();
        let camera = camera.as_str().to_string();
        let limit = i64::try_from(limit.min(MAX_PAIRS_PER_CAMERA * 2)).unwrap_or(64);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT pair_id, node_id, left_image, right_image, left_camera, right_camera,
                            flash, gap_ms, subject_similarity, background_agreement, verified,
                            held_out, analysis_ver
                       FROM camera_pair
                      WHERE project_id = ?1 AND right_camera = ?2
                      ORDER BY verified DESC, background_agreement DESC, gap_ms ASC
                      LIMIT ?3",
                )
                .map_err(|err| statement_failed("camera_pair select", &err))?;
            let rows = statement
                .query_map(params![key, camera, limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, f64>(8)?,
                        row.get::<_, f64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, i64>(12)?,
                    ))
                })
                .map_err(|err| statement_failed("camera_pair rows", &err))?;
            let mut out = Vec::new();
            for row in rows {
                let row = row.map_err(|err| statement_failed("camera_pair row", &err))?;
                let (Ok(id), Ok(node), Ok(left), Ok(right)) = (
                    PairId::from_db(&row.0),
                    NodeId::from_db(&row.1),
                    ImageId::from_db(&row.2),
                    ImageId::from_db(&row.3),
                ) else {
                    continue;
                };
                out.push(MatchedPair {
                    id,
                    node,
                    left,
                    right,
                    left_camera: CameraId::new(row.4),
                    right_camera: CameraId::new(row.5),
                    flash: FlashState::from_str_or_ambient(&row.6),
                    gap_ms: row.7,
                    subject_similarity: row.8 as f32,
                    background_agreement: row.9 as f32,
                    verified: row.10 == 1,
                    held_out: row.11 == 1,
                    analysis_ver: u16::try_from(row.12).unwrap_or(0),
                });
            }
            Ok(out)
        })
    }

    /// Every measured exposure habit in a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn shooter_bias(&self, project: ProjectId) -> AuraResult<Vec<ShooterBias>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT camera_id, scene, shooter, measured_ev, applied_ev, frames, capped,
                            reasons, analysis_ver
                       FROM camera_shooter_bias WHERE project_id = ?1
                      ORDER BY camera_id, scene",
                )
                .map_err(|err| statement_failed("camera_shooter_bias select", &err))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok(ShooterBias {
                        camera_id: CameraId::new(row.get::<_, String>(0)?),
                        scene: SceneId::from_str_or_unknown(&row.get::<_, String>(1)?),
                        shooter: row.get::<_, String>(2)?,
                        measured_ev: row.get::<_, f64>(3)? as f32,
                        applied_ev: row.get::<_, f64>(4)? as f32,
                        frames: u32::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
                        capped: row.get::<_, i64>(6)? == 1,
                        reasons: CameraReason::from_bits(
                            u32::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
                        ),
                        analysis_ver: u16::try_from(row.get::<_, i64>(8)?).unwrap_or(0),
                    })
                })
                .map_err(|err| statement_failed("camera_shooter_bias rows", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("camera_shooter_bias row", &err))?);
            }
            Ok(out)
        })
    }

    /// Which body a project is matched to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn reference(&self, project: ProjectId) -> AuraResult<Option<Reference>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT camera_id, source, frames, shooter FROM camera_reference
                  WHERE project_id = ?1",
                params![key],
                |row| {
                    Ok(Reference {
                        project,
                        camera_id: CameraId::new(row.get::<_, String>(0)?),
                        source: ReferenceSource::from_str_or_shooter(&row.get::<_, String>(1)?),
                        frames: u32::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        shooter: row.get::<_, Option<String>>(3)?,
                    })
                },
            )
            .optional()
            .map_err(|err| statement_failed("camera_reference select", &err))
        })
    }

    /// Record a photographer's reference choice.
    ///
    /// Writes `source = 'user'`, which the `camera_reference_keep_user` trigger then protects from
    /// every automatic re-solve.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn set_reference(
        &self,
        project: ProjectId,
        camera: &CameraId,
        frames: u32,
        shooter: Option<String>,
    ) -> AuraResult<()> {
        let key = project.to_db();
        let camera = camera.as_str().to_string();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |tx| {
            // A DELETE and an INSERT rather than an UPDATE, because the trigger that protects a
            // user choice fires on UPDATE and would refuse a photographer changing their own mind
            // from one body to another. The trigger's job is automation, not people.
            tx.execute(
                "DELETE FROM camera_reference WHERE project_id = ?1",
                params![key],
            )
            .map_err(|err| statement_failed("camera_reference clear", &err))?;
            tx.execute(
                "INSERT INTO camera_reference
                   (project_id, camera_id, source, frames, shooter, analysis_ver,
                    created_at, updated_at)
                 VALUES (?1, ?2, 'user', ?3, ?4, ?5, ?6, ?6)",
                params![
                    key,
                    camera,
                    i64::from(frames),
                    shooter,
                    i64::from(super::ANALYSIS_VER),
                    now
                ],
            )
            .map_err(|err| statement_failed("camera_reference insert", &err))?;
            Ok(())
        })
    }

    /// Switch matching off for one body, or back on. Both flash states move together.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn set_enabled(
        &self,
        project: ProjectId,
        camera: &CameraId,
        enabled: bool,
    ) -> AuraResult<usize> {
        let key = project.to_db();
        let camera = camera.as_str().to_string();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |tx| {
            let changed = tx
                .execute(
                    "UPDATE camera_transform SET enabled = ?3, updated_at = ?4
                      WHERE project_id = ?1 AND camera_id = ?2",
                    params![key, camera, i64::from(enabled), now],
                )
                .map_err(|err| statement_failed("camera_transform enable", &err))?;
            Ok(changed)
        })
    }

    /// Record what a photographer set instead, for one body in one flash state.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn set_override(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
        values: [Option<f32>; 4],
    ) -> AuraResult<usize> {
        let key = project.to_db();
        let camera = camera.as_str().to_string();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |tx| {
            let changed = tx
                .execute(
                    "UPDATE camera_transform
                        SET d_cct        = COALESCE(?4, d_cct),
                            d_tint       = COALESCE(?5, d_tint),
                            d_exposure   = COALESCE(?6, d_exposure),
                            d_saturation = COALESCE(?7, d_saturation),
                            user_edited  = 1,
                            updated_at   = ?8
                      WHERE project_id = ?1 AND camera_id = ?2 AND flash = ?3",
                    params![
                        key,
                        camera,
                        flash.as_str(),
                        values[0].map(f64::from),
                        values[1].map(f64::from),
                        values[2].map(f64::from),
                        values[3].map(f64::from),
                        now
                    ],
                )
                .map_err(|err| statement_failed("camera_transform override", &err))?;
            Ok(changed)
        })
    }

    /// What a project's matching pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a query fails.
    pub fn outline(
        &self,
        project: ProjectId,
        unknown_brands: Vec<String>,
    ) -> AuraResult<CameraOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let photos: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
                    params![key.clone()],
                    |row| row.get(0),
                )
                .map_err(|err| statement_failed("photo count", &err))?;

            // The denominator is every photograph, as phases 09, 10, 15 and 25 count it. A
            // photograph whose body could not be identified is a gap in this pass whatever caused
            // it, and counting against matched bodies would hide the largest failure the pass has.
            let matched: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM photo p
                       JOIN camera c ON c.project_id = p.project_id
                                    AND c.body_serial = COALESCE(p.camera_serial, '')
                       JOIN camera_transform t ON t.project_id = p.project_id
                                              AND t.camera_id = c.camera_id
                      WHERE p.project_id = ?1",
                    params![key.clone()],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let mut outline = CameraOutline {
                photos: u32::try_from(photos).unwrap_or(0),
                matched: u32::try_from(matched).unwrap_or(0),
                unknown_brands,
                analysis_ver: super::ANALYSIS_VER,
                ..CameraOutline::default()
            };
            if photos > 0 {
                #[allow(clippy::cast_precision_loss)]
                {
                    outline.coverage = (matched as f32 / photos as f32).clamp(0.0, 1.0);
                }
            }

            let counts = |sql: &str| -> i64 {
                conn.query_row(sql, params![key.clone()], |row| row.get::<_, i64>(0))
                    .unwrap_or(0)
            };
            outline.cameras = u32::try_from(counts(
                "SELECT COUNT(DISTINCT camera_id) FROM camera_transform WHERE project_id = ?1",
            ))
            .unwrap_or(0);
            outline.fingerprinted = u32::try_from(counts(
                "SELECT COUNT(DISTINCT camera_id) FROM camera_fingerprint WHERE project_id = ?1",
            ))
            .unwrap_or(0);
            outline.solved_from_pairs = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_transform
                  WHERE project_id = ?1 AND source = 'matched_pairs'",
            ))
            .unwrap_or(0);
            outline.blended = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_transform WHERE project_id = ?1 AND source = 'blended'",
            ))
            .unwrap_or(0);
            outline.baseline_only = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_transform
                  WHERE project_id = ?1 AND source = 'brand_baseline'",
            ))
            .unwrap_or(0);
            outline.pairs = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_pair WHERE project_id = ?1 AND verified = 1",
            ))
            .unwrap_or(0);
            outline.pairs_rejected = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_pair WHERE project_id = ?1 AND verified = 0",
            ))
            .unwrap_or(0);
            outline.heldout_pairs = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_pair WHERE project_id = ?1 AND held_out = 1",
            ))
            .unwrap_or(0);
            outline.flash_separated = u32::try_from(counts(
                "SELECT COUNT(*) FROM (SELECT camera_id FROM camera_fingerprint
                   WHERE project_id = ?1 GROUP BY camera_id HAVING COUNT(DISTINCT flash) = 2)",
            ))
            .unwrap_or(0);
            outline.shooters_measured = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_shooter_bias WHERE project_id = ?1 AND frames >= 20",
            ))
            .unwrap_or(0);
            outline.shooters_capped = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_shooter_bias WHERE project_id = ?1 AND capped = 1",
            ))
            .unwrap_or(0);
            outline.disabled = u32::try_from(counts(
                "SELECT COUNT(DISTINCT camera_id) FROM camera_transform
                  WHERE project_id = ?1 AND enabled = 0",
            ))
            .unwrap_or(0);
            outline.user_edited = u32::try_from(counts(
                "SELECT COUNT(*) FROM camera_transform WHERE project_id = ?1 AND user_edited = 1",
            ))
            .unwrap_or(0);

            // The reference body's own rows are excluded from every skin figure. Its transform is
            // the identity by construction, so leaving it in would average a guaranteed zero into
            // the number the phase's headline promise is measured on - which would make a project
            // with one badly-matched body and one reference look half as bad as it is.
            let skin: Option<(f64, f64, f64)> = conn
                .query_row(
                    "SELECT AVG(skin_de00_before), AVG(skin_de00_after), MAX(skin_de00_after)
                       FROM camera_transform
                      WHERE project_id = ?1 AND camera_id <> reference_id",
                    params![key.clone()],
                    |row| {
                        Ok((
                            row.get::<_, Option<f64>>(0)?.unwrap_or(0.0),
                            row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                            row.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                        ))
                    },
                )
                .optional()
                .map_err(|err| statement_failed("camera skin summary", &err))?;
            if let Some((before, after, worst)) = skin {
                outline.skin_de00_before = before as f32;
                outline.skin_de00_after = after as f32;
                outline.worst_skin_de00 = worst as f32;
            }

            let reference: Option<(String, String)> = conn
                .query_row(
                    "SELECT camera_id, source FROM camera_reference WHERE project_id = ?1",
                    params![key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|err| statement_failed("camera_reference summary", &err))?;
            if let Some((camera, source)) = reference {
                outline.reference = Some(CameraId::new(camera));
                outline.reference_source = ReferenceSource::from_str_or_shooter(&source);
            }

            Ok(outline)
        })
    }
}

/// Read transforms, optionally narrowed to one body and flash state.
fn read_transforms(
    conn: &rusqlite::Connection,
    project: &str,
    camera: Option<&str>,
    flash: Option<FlashState>,
) -> AuraResult<Vec<CameraTransform>> {
    let sql = "SELECT camera_id, flash, reference_id, d_cct, d_tint, d_exposure, d_saturation,
                      channel_gain, contrast_shape, skin_du, skin_dv, skin_dluma,
                      skin_de00_before, skin_de00_after, skin_locus_valid, skin_capped,
                      source, blend, evidence_pairs, distance_before, distance_after,
                      heldout_before, heldout_after, heldout_pairs, bounded_by, confidence,
                      reasons, enabled, user_edited, analysis_ver, policy_ver
                 FROM camera_transform
                WHERE project_id = ?1
                  AND (?2 IS NULL OR camera_id = ?2)
                  AND (?3 IS NULL OR flash = ?3)
                ORDER BY camera_id, flash";
    let mut statement = conn
        .prepare(sql)
        .map_err(|err| statement_failed("camera_transform select", &err))?;
    let rows = statement
        .query_map(params![project, camera, flash.map(|f| f.as_str())], |row| {
            Ok(CameraTransform {
                camera_id: CameraId::new(row.get::<_, String>(0)?),
                flash: FlashState::from_str_or_ambient(&row.get::<_, String>(1)?),
                reference: CameraId::new(row.get::<_, String>(2)?),
                d_cct: row.get::<_, f64>(3)? as f32,
                d_tint: row.get::<_, f64>(4)? as f32,
                d_exposure: row.get::<_, f64>(5)? as f32,
                d_saturation: row.get::<_, f64>(6)? as f32,
                channel_gain: decode3(&row.get::<_, String>(7)?),
                contrast_shape: decode3(&row.get::<_, String>(8)?),
                skin_correction: SkinCorrection {
                    d_uv: [row.get::<_, f64>(9)? as f32, row.get::<_, f64>(10)? as f32],
                    d_luma: row.get::<_, f64>(11)? as f32,
                    de00_before: row.get::<_, f64>(12)? as f32,
                    de00_after: row.get::<_, f64>(13)? as f32,
                    locus_valid: row.get::<_, i64>(14)? == 1,
                    capped: row.get::<_, i64>(15)? == 1,
                },
                source: TransformSource::from_str_or_baseline(&row.get::<_, String>(16)?),
                blend: row.get::<_, f64>(17)? as f32,
                evidence_pairs: u32::try_from(row.get::<_, i64>(18)?).unwrap_or(0),
                distance_before: decode_distance(&row.get::<_, String>(19)?),
                distance_after: decode_distance(&row.get::<_, String>(20)?),
                heldout_before: decode_distance(&row.get::<_, String>(21)?),
                heldout_after: decode_distance(&row.get::<_, String>(22)?),
                heldout_pairs: u32::try_from(row.get::<_, i64>(23)?).unwrap_or(0),
                bounded: row
                    .get::<_, Option<String>>(24)?
                    .map(|text| TransformBound::from_str_or_cct(&text)),
                confidence: row.get::<_, f64>(25)? as f32,
                reasons: CameraReason::from_bits(
                    u32::try_from(row.get::<_, i64>(26)?).unwrap_or(0),
                ),
                enabled: row.get::<_, i64>(27)? == 1,
                user_edited: row.get::<_, i64>(28)? == 1,
                analysis_ver: u16::try_from(row.get::<_, i64>(29)?).unwrap_or(0),
                policy_ver: u16::try_from(row.get::<_, i64>(30)?).unwrap_or(0),
            })
        })
        .map_err(|err| statement_failed("camera_transform rows", &err))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|err| statement_failed("camera_transform row", &err))?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The codec
// ---------------------------------------------------------------------------

/// A fixed-length float array as a JSON array.
fn encode(values: &[f32]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

/// An appearance distance as a JSON object, in the contract's own field names.
fn encode_distance(distance: &AppearanceDistance) -> String {
    serde_json::to_string(distance).unwrap_or_else(|_| "{}".to_string())
}

/// Read one back, defaulting to zeroes: a row whose JSON is unreadable is a corrupt row, and a
/// distance of zero renders as "not measured" rather than as "perfectly matched" everywhere it is
/// used. See `CameraOutline::skin_reduction` and `report::summary`.
fn decode_distance(text: &str) -> AppearanceDistance {
    serde_json::from_str(text).unwrap_or_default()
}

fn decode3(text: &str) -> [f32; 3] {
    decode_n(text, [1.0; 3])
}

fn decode4(text: &str) -> [f32; 4] {
    decode_n(text, [1.0; 4])
}

fn decode8(text: &str) -> [f32; 8] {
    decode_n(text, [0.0; 8])
}

/// Read a JSON array back into a fixed array, keeping the fallback for anything missing.
///
/// The fallback differs by axis and that is deliberate: a channel gain and a contrast shape default
/// to **one**, because their identity is a ratio, and a grade signature defaults to **zero**,
/// because it is a description rather than a multiplier. A single default here would make one of
/// the two silently wrong on a corrupt row.
fn decode_n<const N: usize>(text: &str, fallback: [f32; N]) -> [f32; N] {
    let Ok(values) = serde_json::from_str::<Vec<f32>>(text) else {
        return fallback;
    };
    let mut out = fallback;
    for (slot, value) in out.iter_mut().zip(values.into_iter()) {
        if value.is_finite() {
            *slot = value;
        }
    }
    out
}
