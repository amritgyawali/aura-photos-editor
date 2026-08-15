//! The two tables migration 9 adds, and the three rules that live in the SQL rather
//! than around it.
//!
//! ## `user_reviewed` is checked inside the statement, not before it
//!
//! A re-analysis overwrites every verdict it recomputes. That upsert re-applies the
//! photographer's dismissals from the row it is replacing, in the same statement, so a
//! dismissal cannot be lost by a background pass that read the flag a moment earlier and
//! wrote a moment later. Phase 06 wrote this rule for `identities`, phase 07 for
//! `segments`, phase 08 for `moments`; this is the fourth time and the window it closes
//! is the same one every time.
//!
//! The phase 09 variant is *re-application* rather than *exclusion*, and that difference
//! is deliberate. A locked moment is subtracted from phase 08's pass because a
//! photographer's grouping replaces the machine's. A dismissed flag is not a replacement
//! verdict - the frame still has to be re-measured when the calibration table moves - so
//! the dismissal is carried forward onto the new measurement instead. See
//! [`IntegrityStore::put`].
//!
//! ## Why the store reads `moment_images` directly
//!
//! [`IntegrityStore::relative_sharpness`] needs to know which frames were shot together,
//! and phase 08's rule is that `MomentService` is the only way to ask what was shot once.
//! This does not ask that: it reads back the grouping `MomentService` already made, as
//! opaque ids, to answer "of the frames in this group, where does this one rank". It
//! computes no cadence, builds no graph and cannot produce a grouping of its own.
//!
//! The alternative - taking `Arc<dyn MomentService>` and calling `moment_of` once per
//! frame - is four thousand round trips to answer a question one query answers, and
//! phase 08's own exit report (condition C4) names exactly that cost as the reason its
//! face signals are unwired. It is the same argument
//! `aura_brain_wedding::moments::moment` makes for reading `faces.identity_id`, in the
//! other direction. ADR-0019 section 6.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::integrity::{
    CropRect, ExposureVerdict, EyeOpenness, EyeState, ImageId, IntegrityFlags, IntegrityOutline,
    IntegrityResult, MotionKind, Reason, ReasonCode,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult, FaceId, MomentId, PhotoId, ProjectId, SceneId};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::errors;
use crate::integrity::analyse::FrameExif;

/// Bytes one photograph costs across the integrity tables and their indexes.
///
/// Section 11 budgets **1 KB per image including per-face eye states**, which is the
/// most generous storage budget in the product so far and is spent almost entirely on
/// one field: `reasons`, with an evidence rectangle per entry. That is section 13's last
/// acceptance criterion - the card shows the exact crop that caused each penalty - and
/// it is the difference between a technical verdict a photographer can argue with and
/// one they can only accept or turn off.
///
/// **Measured, not estimated.** `crates/aura-perf/tests/integrity_budgets.rs` reads
/// `PRAGMA page_count` on a real catalog before and after, exactly as phases 07 and 08
/// do.
pub const BYTES_PER_IMAGE: usize = 1_024;

/// How the `reasons` column is shaped.
///
/// A JSON array of these. Serde rather than a hand-rolled format because the same shape
/// crosses the IPC boundary to the Integrity card, and two encoders for one shape is two
/// things to keep in step.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredReason {
    #[serde(rename = "c")]
    code: String,
    /// Present **only** when it differs from `ReasonCode::user_text`.
    ///
    /// See that method's doc comment for the argument. In practice one reason in the
    /// product carries text - the camera-shake sentence that names a shutter speed - and
    /// storing the other twenty would be most of section 11's per-image budget spent on
    /// copy that a release can change.
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "w")]
    weight: f32,
    #[serde(rename = "e", skip_serializing_if = "Option::is_none")]
    evidence: Option<[f32; 4]>,
}

/// Reads and writes the two integrity tables.
#[derive(Debug)]
pub struct IntegrityStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl IntegrityStore {
    /// Use one catalog's integrity tables.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// Photographs in this project that have not been analysed at this version.
    ///
    /// **The work remaining is a query, not a journal** - invariant 5, the same way
    /// phase 05's embedding pass and phase 06's face pass do it. Kill the process at 10 %,
    /// 50 % or 90 % and the next run asks the catalog what is left.
    ///
    /// A frame whose stored verdict is a version behind is returned too, which is what
    /// makes a `calib_ver` bump self-healing rather than something an operator has to
    /// trigger.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(
        &self,
        project: &ProjectId,
        model_ver: u16,
        analysis_ver: u16,
        calib_ver: u16,
    ) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN image_integrity i ON i.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (i.photo_id IS NULL
                             OR i.model_ver <> ?2
                             OR i.analysis_ver <> ?3
                             OR i.calib_ver <> ?4)
                      ORDER BY p.timeline_time, p.photo_id",
                )
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![
                    key,
                    i64::from(model_ver),
                    i64::from(analysis_ver),
                    i64::from(calib_ver)
                ])
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the pending set", &e))?
            {
                let text: String = row
                    .get(0)
                    .map_err(|e| statement_failed("could not read a photo id", &e))?;
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// What EXIF says about one project's photographs, keyed by photograph.
    ///
    /// One query for the whole project rather than one per frame: the pass is already
    /// per-frame and one extra round trip per frame would be four thousand of them for
    /// six numbers each.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn exif_table(&self, project: &ProjectId) -> AuraResult<BTreeMap<PhotoId, FrameExif>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    // Every one of these is a column on `photo`, written by phase 01's
                    // ingest. The `exif` table is a tag/value store for everything phase
                    // 01 did *not* promote to a column, and reading six numbers out of it
                    // would be six self-joins for data that is already here.
                    "SELECT p.photo_id,
                            COALESCE(p.camera_make, ''), COALESCE(p.camera_model, ''),
                            COALESCE(p.iso, 0),
                            p.shutter_us, p.focal_len_mm,
                            COALESCE(p.width_px, 0), COALESCE(p.height_px, 0)
                       FROM photo p
                      WHERE p.project_id = ?1",
                )
                .map_err(|e| statement_failed("could not read EXIF for the integrity pass", &e))?;
            let mut out = BTreeMap::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read EXIF", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read EXIF", &e))?
            {
                let text: String = row.get(0).map_err(|e| statement_failed("photo id", &e))?;
                let Ok(id) = PhotoId::from_db(&text) else {
                    continue;
                };
                let width: i64 = row.get(6).unwrap_or(0);
                let height: i64 = row.get(7).unwrap_or(0);
                out.insert(
                    id,
                    FrameExif {
                        make: row.get(1).unwrap_or_default(),
                        model: row.get(2).unwrap_or_default(),
                        iso: u32::try_from(row.get::<_, i64>(3).unwrap_or(0)).unwrap_or(0),
                        // Microseconds in the catalog, because phase 01 stores an
                        // integer for exactness; seconds here, because the reciprocal
                        // rule is expressed in seconds and converting at every call site
                        // is how a factor of a million goes unnoticed.
                        shutter_s: row
                            .get::<_, Option<i64>>(4)
                            .ok()
                            .flatten()
                            .filter(|micros| *micros > 0)
                            .map(|micros| micros as f32 / 1_000_000.0),
                        focal_mm: row.get::<_, Option<f64>>(5).ok().flatten().map(|v| v as f32),
                        // EXIF's stabilisation tag is not in phase 01's schema. Absent
                        // rather than guessed: `MotionContext::safe_shutter_s` treats
                        // "not stabilised" as the conservative reading, which makes the
                        // reciprocal rule stricter and therefore makes shake *harder*
                        // to claim - the safe direction. Condition C6.
                        stabilised: false,
                        megapixels: (width.max(0) as f32 * height.max(0) as f32) / 1_000_000.0,
                    },
                );
            }
            Ok(out)
        })
    }

    /// Store one verdict, carrying forward whatever the photographer dismissed.
    ///
    /// The dismissal is re-applied **inside** the upsert. See this module's header for
    /// why re-application rather than exclusion is the right shape here.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)]
    pub fn put(
        &self,
        project: &ProjectId,
        result: &IntegrityResult,
        provenance: &Provenance,
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let project_key = project.to_db();
        let photo_key = result.image_id.to_db();
        let reasons = encode_reasons(&result.reasons);
        let row = OwnedRow::from((result, provenance));
        let eyes: Vec<OwnedEye> = result.eyes.iter().map(OwnedEye::from).collect();
        let eye_model_ver = i64::from(result.model_ver);

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("could not open the integrity transaction", &e))?;

            // `dismissed` and `user_reviewed` come from the row being replaced, not from
            // the verdict being written. `excluded` is the incoming row; the bare column
            // name is the existing one. A freshly computed flag set has the dismissed
            // bits cleared out of it in the same statement that stores it, so there is
            // no window in which the photographer's decision is not in force.
            tx.execute(
                "INSERT INTO image_integrity (
                     photo_id, project_id,
                     subject_sharpness, bg_sharpness, focus_offset, relative_sharpness,
                     motion, motion_severity,
                     exposure, clip_hi, clip_lo, ev_offset,
                     noise_sigma_rel,
                     closed_eye_ratio, gating_faces,
                     technical_score, flags, reasons, confidence,
                     scene, uncalibrated,
                     user_reviewed, dismissed,
                     model_ver, analysis_ver, calib_ver,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20, ?21, 0, 0, ?22, ?23, ?24, ?25, ?25)
                 ON CONFLICT(photo_id) DO UPDATE SET
                     subject_sharpness = excluded.subject_sharpness,
                     bg_sharpness      = excluded.bg_sharpness,
                     focus_offset      = excluded.focus_offset,
                     relative_sharpness= excluded.relative_sharpness,
                     motion            = excluded.motion,
                     motion_severity   = excluded.motion_severity,
                     exposure          = excluded.exposure,
                     clip_hi           = excluded.clip_hi,
                     clip_lo           = excluded.clip_lo,
                     ev_offset         = excluded.ev_offset,
                     noise_sigma_rel   = excluded.noise_sigma_rel,
                     closed_eye_ratio  = excluded.closed_eye_ratio,
                     gating_faces      = excluded.gating_faces,
                     technical_score   = excluded.technical_score,
                     flags             = excluded.flags & ~image_integrity.dismissed,
                     reasons           = excluded.reasons,
                     confidence        = excluded.confidence,
                     scene             = excluded.scene,
                     uncalibrated      = excluded.uncalibrated,
                     model_ver         = excluded.model_ver,
                     analysis_ver      = excluded.analysis_ver,
                     calib_ver         = excluded.calib_ver,
                     updated_at        = excluded.updated_at",
                params![
                    photo_key,
                    project_key,
                    row.subject_sharpness,
                    row.bg_sharpness,
                    row.focus_offset,
                    row.relative_sharpness,
                    row.motion,
                    row.motion_severity,
                    row.exposure,
                    row.clip_hi,
                    row.clip_lo,
                    row.ev_offset,
                    row.noise_sigma_rel,
                    row.closed_eye_ratio,
                    row.gating_faces,
                    row.technical_score,
                    row.flags,
                    reasons,
                    row.confidence,
                    row.scene,
                    row.uncalibrated,
                    row.model_ver,
                    row.analysis_ver,
                    row.calib_ver,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store an integrity verdict", &e))?;

            // The eye rows are replaced wholesale for this photograph. A face that has
            // gone - because the face pass re-ran at a new detector version and derived
            // different ids - must not leave an orphan eye state behind claiming
            // somebody blinked.
            tx.execute(
                "DELETE FROM face_eye_state
                  WHERE face_id IN (SELECT id FROM faces WHERE image_id = ?1)",
                params![photo_key],
            )
            .map_err(|e| statement_failed("could not clear stale eye states", &e))?;

            for eye in &eyes {
                tx.execute(
                    "INSERT OR REPLACE INTO face_eye_state (
                         face_id, identity_id, state, confidence,
                         intentional, gates, model_ver, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        eye.face_id,
                        eye.identity_id,
                        eye.state,
                        eye.confidence,
                        eye.intentional,
                        eye.gates,
                        eye_model_ver,
                        now,
                    ],
                )
                .map_err(|e| statement_failed("could not store an eye state", &e))?;
            }

            tx.commit()
                .map_err(|e| statement_failed("could not commit the integrity verdict", &e))
        })
    }

    /// One photograph's verdict.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn get(&self, image: ImageId) -> AuraResult<Option<IntegrityResult>> {
        let key = image.to_db();
        let eyes = self.eyes_of(image)?;
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT subject_sharpness, bg_sharpness, focus_offset, relative_sharpness,
                            motion, motion_severity, exposure, clip_hi, clip_lo, ev_offset,
                            noise_sigma_rel, closed_eye_ratio, gating_faces,
                            technical_score, flags, reasons, confidence, scene,
                            user_reviewed, model_ver, analysis_ver, calib_ver
                       FROM image_integrity WHERE photo_id = ?1",
                )
                .map_err(|e| statement_failed("could not read an integrity verdict", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read an integrity verdict", &e))?;
            let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read an integrity verdict", &e))?
            else {
                return Ok(None);
            };
            Ok(Some(IntegrityResult {
                image_id: image,
                subject_sharpness: row.get::<_, f64>(0).unwrap_or(0.0) as f32,
                bg_sharpness: row.get::<_, f64>(1).unwrap_or(0.0) as f32,
                focus_offset: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                relative_sharpness: row.get::<_, f64>(3).unwrap_or(0.5) as f32,
                motion: MotionKind::from_str_or_none(
                    &row.get::<_, String>(4).unwrap_or_default(),
                ),
                motion_severity: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                exposure: ExposureVerdict::from_str_or_good(
                    &row.get::<_, String>(6).unwrap_or_default(),
                ),
                clip_hi: row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                clip_lo: row.get::<_, f64>(8).unwrap_or(0.0) as f32,
                ev_offset: row.get::<_, f64>(9).unwrap_or(0.0) as f32,
                noise_sigma_rel: row.get::<_, f64>(10).unwrap_or(0.0) as f32,
                eyes,
                closed_eye_ratio: row.get::<_, f64>(11).unwrap_or(0.0) as f32,
                gating_faces: u32::try_from(row.get::<_, i64>(12).unwrap_or(0)).unwrap_or(0),
                technical_score: row.get::<_, f64>(13).unwrap_or(0.0) as f32,
                flags: IntegrityFlags::from_bits(
                    u32::try_from(row.get::<_, i64>(14).unwrap_or(0)).unwrap_or(0),
                ),
                reasons: decode_reasons(&row.get::<_, String>(15).unwrap_or_default()),
                confidence: row.get::<_, f64>(16).unwrap_or(0.0) as f32,
                scene: SceneId::from_str_or_unknown(
                    &row.get::<_, String>(17).unwrap_or_default(),
                ),
                user_reviewed: row.get::<_, i64>(18).unwrap_or(0) == 1,
                model_ver: u16::try_from(row.get::<_, i64>(19).unwrap_or(0)).unwrap_or(0),
                analysis_ver: u16::try_from(row.get::<_, i64>(20).unwrap_or(0)).unwrap_or(0),
                calib_ver: u16::try_from(row.get::<_, i64>(21).unwrap_or(0)).unwrap_or(0),
            }))
        })
    }

    /// One photograph's eye states, in prominence order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn eyes_of(&self, image: ImageId) -> AuraResult<Vec<EyeState>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    // The area and the evidence rectangle come from `faces`, which is
                    // the one place phase 06's geometry lives. See migration 9.
                    "SELECT e.face_id, e.identity_id, e.state, e.confidence, e.intentional,
                            e.gates, f.area_frac, f.x, f.y, f.w, f.h, f.landmarks_json
                       FROM face_eye_state e
                       JOIN faces f ON f.id = e.face_id
                      WHERE f.image_id = ?1
                      ORDER BY f.area_frac DESC, e.face_id",
                )
                .map_err(|e| statement_failed("could not read eye states", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read eye states", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read eye states", &e))?
            {
                let face_text: String = row.get(0).unwrap_or_default();
                let Ok(face_id) = FaceId::from_db(&face_text) else {
                    continue;
                };
                out.push(EyeState {
                    face_id,
                    identity: row
                        .get::<_, Option<String>>(1)
                        .ok()
                        .flatten()
                        .and_then(|text| aura_core::IdentityId::from_db(&text).ok()),
                    state: EyeOpenness::from_str_or_open(
                        &row.get::<_, String>(2).unwrap_or_default(),
                    ),
                    confidence: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                    intentional: row.get::<_, i64>(4).unwrap_or(0) == 1,
                    gates: row.get::<_, i64>(5).unwrap_or(0) == 1,
                    area_frac: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                    crop: crop_from_face(
                        CropRect {
                            x: row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                            y: row.get::<_, f64>(8).unwrap_or(0.0) as f32,
                            w: row.get::<_, f64>(9).unwrap_or(0.0) as f32,
                            h: row.get::<_, f64>(10).unwrap_or(0.0) as f32,
                        },
                        &row.get::<_, String>(11).unwrap_or_default(),
                    ),
                });
            }
            Ok(out)
        })
    }

    /// The project's coverage, histogram and versions, in two queries.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when either read fails.
    pub fn outline(&self, project: ProjectId) -> AuraResult<IntegrityOutline> {
        let key = project.to_db();
        let uncalibrated = self.uncalibrated_bodies(project)?;
        self.catalog.read(move |conn| {
            let mut outline = IntegrityOutline {
                uncalibrated,
                ..IntegrityOutline::default()
            };

            let mut coverage = conn
                .prepare(
                    "SELECT photos, scored, subject_aware, reviewed, mean_score,
                            model_ver, analysis_ver, calib_ver
                       FROM v_integrity_coverage WHERE project_id = ?1",
                )
                .map_err(|e| statement_failed("could not read integrity coverage", &e))?;
            let mut cursor = coverage
                .query(params![key])
                .map_err(|e| statement_failed("could not read integrity coverage", &e))?;
            if let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read integrity coverage", &e))?
            {
                let photos = row.get::<_, i64>(0).unwrap_or(0).max(0);
                let scored = row.get::<_, i64>(1).unwrap_or(0).max(0);
                let aware = row.get::<_, i64>(2).unwrap_or(0).max(0);
                outline.photos = u32::try_from(photos).unwrap_or(u32::MAX);
                outline.scored = u32::try_from(scored).unwrap_or(u32::MAX);
                outline.coverage = ratio(scored, photos);
                outline.subject_aware = ratio(aware, scored);
                outline.reviewed =
                    u32::try_from(row.get::<_, i64>(3).unwrap_or(0).max(0)).unwrap_or(0);
                outline.mean_score = row.get::<_, f64>(4).unwrap_or(0.0) as f32;
                outline.model_ver =
                    u16::try_from(row.get::<_, i64>(5).unwrap_or(0).max(0)).unwrap_or(0);
                outline.analysis_ver =
                    u16::try_from(row.get::<_, i64>(6).unwrap_or(0).max(0)).unwrap_or(0);
                outline.calib_ver =
                    u16::try_from(row.get::<_, i64>(7).unwrap_or(0).max(0)).unwrap_or(0);
            }

            let mut flags = conn
                .prepare(
                    "SELECT subject_soft, camera_shake, subject_motion, intentional_motion,
                            back_focus, front_focus, highlight_lost, shadow_lost, heavy_noise,
                            eyes_closed, eyes_closed_ok, squint, no_subject_detected,
                            mixed_light_risk
                       FROM v_integrity_flags WHERE project_id = ?1",
                )
                .map_err(|e| statement_failed("could not read the flag histogram", &e))?;
            let mut cursor = flags
                .query(params![key])
                .map_err(|e| statement_failed("could not read the flag histogram", &e))?;
            if let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the flag histogram", &e))?
            {
                for (index, slot) in outline.flag_histogram.iter_mut().enumerate() {
                    *slot = u32::try_from(row.get::<_, i64>(index).unwrap_or(0).max(0))
                        .unwrap_or(0);
                }
            }
            Ok(outline)
        })
    }

    /// Camera bodies in this project that were judged by the fallback.
    ///
    /// Section 11's `integrity.camera_uncalibrated {make, model}` event, and section 12's
    /// fourth failure mode. `DISTINCT` on the make and model rather than on the body
    /// serial: a photographer with two identical uncalibrated bodies has one calibration
    /// gap, not two.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn uncalibrated_bodies(&self, project: ProjectId) -> AuraResult<Vec<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT DISTINCT TRIM(COALESCE(p.camera_make, '') || ' '
                                          || COALESCE(p.camera_model, ''))
                       FROM image_integrity i
                       JOIN photo p ON p.photo_id = i.photo_id
                      WHERE i.project_id = ?1 AND i.uncalibrated = 1
                      ORDER BY 1",
                )
                .map_err(|e| statement_failed("could not read uncalibrated bodies", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read uncalibrated bodies", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read uncalibrated bodies", &e))?
            {
                let name: String = row.get(0).unwrap_or_default();
                if !name.trim().is_empty() {
                    out.push(name);
                }
            }
            Ok(out)
        })
    }

    /// Photographs carrying any of these flags, worst score first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn flagged(
        &self,
        project: ProjectId,
        flags: IntegrityFlags,
        limit: usize,
    ) -> AuraResult<Vec<ImageId>> {
        if flags.is_empty() {
            return Ok(Vec::new());
        }
        let key = project.to_db();
        let mask = i64::from(flags.bits());
        let bound = i64::try_from(limit.clamp(1, 5_000)).unwrap_or(500);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM image_integrity
                      WHERE project_id = ?1 AND (flags & ?2) <> 0
                      ORDER BY technical_score, photo_id
                      LIMIT ?3",
                )
                .map_err(|e| statement_failed("could not read the flagged set", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key, mask, bound])
                .map_err(|e| statement_failed("could not read the flagged set", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the flagged set", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// One moment's frames ranked by subject sharpness, sharpest first.
    ///
    /// Section 6.1's within-moment answer, which is the question phase 12 asks most
    /// often. The second element is the rank normalised to `0..1`: one for the sharpest
    /// and zero for the softest, with `0.5` when the moment holds a single frame -
    /// neither the best nor the worst of no siblings.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn relative_sharpness(&self, moment: MomentId) -> AuraResult<Vec<(ImageId, f32)>> {
        let key = moment.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT mi.photo_id, COALESCE(i.subject_sharpness, 0.0)
                       FROM moment_images mi
                       LEFT JOIN image_integrity i ON i.photo_id = mi.photo_id
                      WHERE mi.moment_id = ?1
                      ORDER BY 2 DESC, 1",
                )
                .map_err(|e| statement_failed("could not rank a moment's frames", &e))?;
            let mut rows: Vec<(PhotoId, f32)> = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not rank a moment's frames", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not rank a moment's frames", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    rows.push((id, row.get::<_, f64>(1).unwrap_or(0.0) as f32));
                }
            }
            Ok(rank(&rows))
        })
    }

    /// Write the within-moment ranks back onto the verdicts.
    ///
    /// Run once at the end of a pass rather than per frame, because a frame's rank
    /// depends on its siblings and half a moment's siblings have not been analysed yet
    /// while the pass is running.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn refresh_relative_sharpness(&self, project: &ProjectId) -> AuraResult<usize> {
        let key = project.to_db();
        self.catalog.writer().with(move |conn| {
            // One statement, computed in SQL, because pulling four thousand rows into
            // Rust to divide a rank by a count and put them back is three round trips
            // and a transaction for arithmetic SQLite does in the query planner.
            //
            // The `CASE` is the single-frame moment: `COUNT(*) - 1` is zero there, and
            // 0.5 is the documented neutral.
            let changed = conn
                .execute(
                    "WITH ranked AS (
                         SELECT mi.photo_id AS photo_id,
                                RANK() OVER (PARTITION BY mi.moment_id
                                             ORDER BY i.subject_sharpness ASC) - 1 AS position,
                                COUNT(*) OVER (PARTITION BY mi.moment_id) - 1 AS span
                           FROM moment_images mi
                           JOIN image_integrity i ON i.photo_id = mi.photo_id
                          WHERE i.project_id = ?1
                     )
                     UPDATE image_integrity
                        SET relative_sharpness = (
                            SELECT CASE WHEN r.span <= 0 THEN 0.5
                                        ELSE CAST(r.position AS REAL) / r.span END
                              FROM ranked r WHERE r.photo_id = image_integrity.photo_id)
                      WHERE photo_id IN (SELECT photo_id FROM ranked)",
                    params![key],
                )
                .map_err(|e| statement_failed("could not refresh within-moment ranks", &e))?;
            Ok(changed)
        })
    }

    /// Record that the photographer disagrees with one flag.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5034` when the photograph has no verdict, when `flag` is not a single
    /// flag, or when it is not currently set.
    pub fn dismiss(&self, image: ImageId, flag: IntegrityFlags) -> Result<(), AuraError> {
        if flag.count() != 1 {
            return Err(errors::integrity_edit_refused(
                &image.to_db(),
                "a dismissal names exactly one flag; clearing two judgements in one statement \
                 would record one decision for two disagreements",
            ));
        }
        if flag.intersects(exonerations()) {
            return Err(errors::integrity_edit_refused(
                &image.to_db(),
                "that mark is not a fault, so there is nothing to dismiss",
            ));
        }
        let Some(stored) = self.get(image)? else {
            return Err(errors::integrity_edit_refused(
                &image.to_db(),
                "this photograph has not been checked yet, so it carries no marks",
            ));
        };
        if !stored.flags.contains(flag) {
            return Err(errors::integrity_edit_refused(
                &image.to_db(),
                "that mark is not on this photograph; the panel may be showing an older reading",
            ));
        }

        let key = image.to_db();
        let bit = i64::from(flag.bits());
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE image_integrity
                    SET flags = flags & ~?2,
                        dismissed = dismissed | ?2,
                        user_reviewed = 1,
                        updated_at = ?3
                  WHERE photo_id = ?1",
                params![key, bit, now],
            )
            .map_err(|e| statement_failed("could not dismiss a technical mark", &e))?;
            Ok(())
        })?;
        Ok(())
    }
}

/// Rebuild an eye reason's evidence rectangle from phase 06's stored geometry.
///
/// The eye landmarks' bounding box grown for context, or the face box when the landmarks
/// are missing - the same rule `eyes::eye_crop` applies at analysis time, expressed here
/// against the catalog's own JSON so the two cannot drift apart into two rectangles for
/// one eye.
fn crop_from_face(bbox: CropRect, landmarks_json: &str) -> CropRect {
    let Ok(points) = serde_json::from_str::<Vec<[f32; 2]>>(landmarks_json) else {
        return bbox.expanded(0.15);
    };
    let (Some(left), Some(right)) = (points.first(), points.get(1)) else {
        return bbox.expanded(0.15);
    };
    if left[0].abs() <= f32::EPSILON && right[0].abs() <= f32::EPSILON {
        return bbox.expanded(0.15);
    }
    let x0 = left[0].min(right[0]);
    let x1 = left[0].max(right[0]);
    let y0 = left[1].min(right[1]);
    let y1 = left[1].max(right[1]);
    CropRect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.01),
        h: (y1 - y0).max(0.01),
    }
    .expanded(1.4)
}

/// The three flags that describe something right with a photograph.
fn exonerations() -> IntegrityFlags {
    IntegrityFlags::INTENTIONAL_MOTION
        .union(IntegrityFlags::EYES_CLOSED_OK)
        .union(IntegrityFlags::NO_SUBJECT_DETECTED)
}

/// Turn a sharpness-ordered list into ranks in `0..1`, sharpest first.
fn rank(rows: &[(PhotoId, f32)]) -> Vec<(ImageId, f32)> {
    if rows.len() <= 1 {
        return rows.iter().map(|(id, _)| (*id, 0.5)).collect();
    }
    let span = (rows.len() - 1) as f32;
    rows.iter()
        .enumerate()
        .map(|(index, (id, _))| (*id, 1.0 - index as f32 / span))
        .collect()
}

fn ratio(numerator: i64, denominator: i64) -> f32 {
    if denominator <= 0 {
        return 0.0;
    }
    (numerator.max(0) as f64 / denominator as f64) as f32
}

fn encode_reasons(reasons: &[Reason]) -> String {
    let stored: Vec<StoredReason> = reasons
        .iter()
        .map(|reason| StoredReason {
            code: reason.code.as_str().to_string(),
            text: if reason.text == reason.code.user_text() {
                None
            } else {
                Some(reason.text.clone())
            },
            weight: reason.weight,
            evidence: reason
                .evidence
                .map(|crop| [crop.x, crop.y, crop.w, crop.h]),
        })
        .collect();
    serde_json::to_string(&stored).unwrap_or_else(|_| "[]".to_string())
}

fn decode_reasons(text: &str) -> Vec<Reason> {
    let Ok(stored) = serde_json::from_str::<Vec<StoredReason>>(text) else {
        return Vec::new();
    };
    stored
        .into_iter()
        .map(|reason| {
            let code = ReasonCode::from_str_or_clean(&reason.code);
            Reason {
                code,
                text: reason
                    .text
                    .unwrap_or_else(|| code.user_text().to_string()),
                weight: reason.weight,
                evidence: reason.evidence.map(|values| CropRect {
                    x: values[0],
                    y: values[1],
                    w: values[2],
                    h: values[3],
                }),
            }
        })
        .collect()
}

/// Which body took the frame, and whether it had been measured.
///
/// Passed to [`IntegrityStore::put`] beside the verdict rather than carried on
/// `IntegrityResult`, because neither number is part of the technical judgement: the
/// calibration row has already been chosen and applied by the time a verdict exists.
/// They are stored so that a fairness audit and the uncalibrated telemetry event are one
/// query rather than a join through `photo` and a re-derivation of the lookup.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Provenance {
    /// True when the fallback calibration row was used.
    ///
    /// The body itself is **not** stored beside it. A camera id is 68 characters and the
    /// answer is one join through `photo` away, which is a trade section 11's 1 KB budget
    /// decided: the uncalibrated *flag* is asked per row by the coverage view, and the
    /// body's name is asked once per project by the telemetry event.
    pub uncalibrated: bool,
}

/// The verdict's columns, owned, so the closure that writes them is `'static`.
struct OwnedRow {
    subject_sharpness: f64,
    bg_sharpness: f64,
    focus_offset: f64,
    relative_sharpness: f64,
    motion: String,
    motion_severity: f64,
    exposure: String,
    clip_hi: f64,
    clip_lo: f64,
    ev_offset: f64,
    noise_sigma_rel: f64,
    closed_eye_ratio: f64,
    gating_faces: i64,
    technical_score: f64,
    flags: i64,
    confidence: f64,
    scene: String,
    uncalibrated: i64,
    model_ver: i64,
    analysis_ver: i64,
    calib_ver: i64,
}

impl From<(&IntegrityResult, &Provenance)> for OwnedRow {
    fn from((result, provenance): (&IntegrityResult, &Provenance)) -> Self {
        Self {
            subject_sharpness: f64::from(result.subject_sharpness.clamp(0.0, 1.0)),
            bg_sharpness: f64::from(result.bg_sharpness.clamp(0.0, 1.0)),
            focus_offset: f64::from(result.focus_offset.clamp(-1.0, 1.0)),
            relative_sharpness: f64::from(result.relative_sharpness.clamp(0.0, 1.0)),
            motion: result.motion.as_str().to_string(),
            motion_severity: f64::from(result.motion_severity.clamp(0.0, 1.0)),
            exposure: result.exposure.as_str().to_string(),
            clip_hi: f64::from(result.clip_hi.clamp(0.0, 1.0)),
            clip_lo: f64::from(result.clip_lo.clamp(0.0, 1.0)),
            ev_offset: f64::from(result.ev_offset.clamp(-6.0, 6.0)),
            noise_sigma_rel: f64::from(result.noise_sigma_rel.max(0.0)),
            closed_eye_ratio: f64::from(result.closed_eye_ratio.clamp(0.0, 1.0)),
            gating_faces: i64::from(result.gating_faces),
            technical_score: f64::from(result.technical_score.clamp(0.0, 1.0)),
            flags: i64::from(result.flags.bits()),
            confidence: f64::from(result.confidence.clamp(0.0, 1.0)),
            scene: result.scene.as_str().to_string(),
            uncalibrated: i64::from(provenance.uncalibrated),
            model_ver: i64::from(result.model_ver),
            analysis_ver: i64::from(result.analysis_ver),
            calib_ver: i64::from(result.calib_ver),
        }
    }
}

/// One eye row, owned.
struct OwnedEye {
    face_id: String,
    identity_id: Option<String>,
    state: String,
    confidence: f64,
    intentional: i64,
    gates: i64,
    area_frac: f64,
    crop: [f64; 4],
}

impl From<&EyeState> for OwnedEye {
    fn from(eye: &EyeState) -> Self {
        Self {
            face_id: eye.face_id.to_db(),
            identity_id: eye.identity.map(|id| id.to_db()),
            state: eye.state.as_str().to_string(),
            confidence: f64::from(eye.confidence.clamp(0.0, 1.0)),
            intentional: i64::from(eye.intentional),
            gates: i64::from(eye.gates),
            area_frac: f64::from(eye.area_frac.clamp(0.0, 1.0)),
            crop: [
                f64::from(eye.crop.x),
                f64::from(eye.crop.y),
                f64::from(eye.crop.w),
                f64::from(eye.crop.h),
            ],
        }
    }
}
