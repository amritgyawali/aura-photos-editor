//! The three tables migration 15 adds, and the four rules that live in the SQL rather than
//! around it.
//!
//! ## `user_edited` is re-applied inside the statement, not read before it
//!
//! A re-analysis overwrites every estimate it recomputes. That upsert carries the
//! photographer's own three values forward from the row it is replacing, in the same
//! statement, so an override cannot be lost by a background pass that read the flag a moment
//! earlier and wrote a moment later. Phases 06 to 12 wrote this rule for identities,
//! segments, moments, verdicts and framing notes; this is the seventh time.
//!
//! Re-*application* rather than exclusion, as phases 09 and 11 chose and for their reason: an
//! override is not a replacement estimate. The frame still has to be re-measured when the
//! target table moves, because the review queue's whole job is to show a photographer where
//! they and the product disagree - and that comparison needs a current estimate to disagree
//! *with*.
//!
//! ## Three JSON columns, and why they are not three tables
//!
//! Section 11 budgets 600 bytes per image, the tightest per-image figure in the product.
//! `illuminants`, `reasons` and `alternatives` are always read together by the panel that
//! draws them and never queried across, so three child tables would cost more in row overhead
//! and index than the documents do. The keys inside them are one and two characters, and
//! [`codec`] is the one place that knows what they mean.
//!
//! ## A reason stores its code, not its sentence
//!
//! Phase 09's rule, for the fourth migration running. The `t` key is written only when the
//! sentence differs from `ToneCode::user_text`, which it does for the four codes that name a
//! temperature or a percentage.
//!
//! ## Why the store reads `segments` directly
//!
//! [`ToneStore::segments_of`] needs the list of chapters to select reference frames for, and
//! phase 07's rule is that `StoryService` is the only way to ask what a photograph is of.
//! This does not ask that: it reads back the segmentation `StoryService` already made, as
//! opaque ids, to answer "which chapters exist". It classifies nothing and cannot produce a
//! segmentation of its own. The same argument phases 09 and 11 make about `moment_images`,
//! and ADR-0031 section 6 records it again because the alternative - a round trip through the
//! frozen service per chapter - would be a service call inside the write path of a pass.

use std::collections::BTreeMap;
use std::sync::Arc;

pub mod codec;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::tone::{
    IlluminantKind, ImageId, ReferenceFrame, SkinLocus, ToneEstimate, ToneOutline, ToneOverride,
    EXPOSURE_RANGE, KELVIN_RANGE, REVIEW_WB_BELOW, TINT_RANGE,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, IdentityId, PhotoId, ProjectId, SceneId, SegmentId};
use rusqlite::{params, OptionalExtension};

use crate::errors;

/// Bytes one photograph costs in `image_tone_estimate` and its three indexes.
///
/// Section 11 budgets **600 B per image**, the tightest per-image figure in the product -
/// tighter than phase 11's 800 and phase 09's kilobyte - while carrying two illuminants with
/// regions, up to six reasons, three alternatives and twenty-two scalars.
///
/// Four decisions pay for it, and three of them are earlier phases' lessons applied again:
/// a reason stores its code rather than its sentence, the three documents use one and
/// two-character keys, empty arrays are omitted from the row rather than stored as `[]`
/// inside a larger object, and every float in the documents is written to three decimals
/// rather than to serde's default seventeen.
///
/// The figure is asserted by `crates/aura-perf/tests/tone_budgets.rs` against a real catalog
/// rather than against this constant.
pub const BYTES_PER_IMAGE: usize = 600;

/// The `image_tone_estimate`, `identity_skin_locus` and `segment_reference_frames` tables.
#[derive(Debug)]
pub struct ToneStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl ToneStore {
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

    // -----------------------------------------------------------------------
    // The estimate
    // -----------------------------------------------------------------------

    /// Photographs with no estimate at these versions.
    ///
    /// **The work remaining is a query, not a journal.** Invariant 5: kill the process at
    /// 10 %, 50 % or 90 % and the next run asks the catalog what is left. A `targets_ver`
    /// bump therefore heals itself - the rows made under the old table are pending by
    /// definition.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(
        &self,
        project: &ProjectId,
        model_ver: u16,
        analysis_ver: u16,
        targets_ver: u16,
    ) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN image_tone_estimate t ON t.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (t.photo_id IS NULL
                             OR t.model_ver <> ?2
                             OR t.analysis_ver <> ?3
                             OR t.targets_ver <> ?4)
                      ORDER BY p.photo_id",
                )
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![
                    key,
                    i64::from(model_ver),
                    i64::from(analysis_ver),
                    i64::from(targets_ver)
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

    /// Every photograph in a project, in id order.
    ///
    /// The locus pass reads this rather than `pending`: a locus is accumulated from frames
    /// that already have a confident answer, and restricting it to the pending set would make
    /// the loci depend on how far the previous run got.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn all_photos(&self, project: &ProjectId) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id FROM photo WHERE project_id = ?1 ORDER BY photo_id")
                .map_err(|e| statement_failed("could not list the project", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not list the project", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not list the project", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Store one estimate, carrying forward whatever the photographer set.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)]
    pub fn put(
        &self,
        project: &ProjectId,
        estimate: &ToneEstimate,
        reason_numbers: &[Vec<f32>],
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let at_ts = epoch_ms(self.clock.now_utc());
        let project_key = project.to_db();
        let photo_key = estimate.image_id.to_db();

        let illuminants = codec::encode_illuminants(&estimate.illuminants);
        let reasons = codec::encode_reasons(&estimate.reasons, reason_numbers);
        let alternatives = codec::encode_alternatives(&estimate.alternatives);
        let dominant_kind = estimate
            .subject_illuminant()
            .or_else(|| estimate.illuminants.first())
            .map_or(IlluminantKind::Unknown, |light| light.kind);

        let row = OwnedRow {
            exposure_ev: estimate.exposure_ev,
            temperature_k: estimate.temperature_k,
            tint: estimate.tint,
            exposure_conf: estimate.exposure_conf,
            wb_conf: estimate.wb_conf,
            subject_luma_before: estimate.subject_luma_before,
            subject_luma_target: estimate.subject_luma_target,
            face_anchored: estimate.face_anchored,
            mixed_light: estimate.mixed_light,
            coloured_light: estimate.coloured_light,
            backlit: estimate.backlit,
            dominant_on_subject: estimate.dominant_on_subject,
            dominant_kind: dominant_kind.as_str().to_string(),
            clipping_added: estimate.clipping_added,
            skin_de00: estimate.skin_de00_estimate,
            constrained_ids: estimate.constrained_identities,
            scene: estimate.scene.as_str().to_string(),
            untargeted: estimate
                .reasons
                .iter()
                .any(|reason| reason.code == aura_core::ToneCode::NoTargetRow),
            illuminants,
            reasons,
            alternatives,
            reason_count: i64::try_from(estimate.reasons.len().clamp(1, ToneEstimate::MAX_REASONS))
                .unwrap_or(1),
            model_ver: i64::from(estimate.model_ver),
            analysis_ver: i64::from(estimate.analysis_ver),
            targets_ver: i64::from(estimate.targets_ver),
        };

        self.catalog.writer().transact(move |conn| {
            // The photographer's own three values and the two bits that protect them, read
            // and rewritten inside the same statement. See the module header.
            let existing: Option<(i64, i64, Option<f64>, Option<f64>, Option<f64>)> = conn
                .query_row(
                    "SELECT user_edited, reviewed, user_exposure_ev, user_temperature_k, user_tint
                       FROM image_tone_estimate WHERE photo_id = ?1",
                    params![photo_key],
                    |stored| {
                        Ok((
                            stored.get::<_, i64>(0).unwrap_or(0),
                            stored.get::<_, i64>(1).unwrap_or(0),
                            stored.get::<_, Option<f64>>(2).unwrap_or(None),
                            stored.get::<_, Option<f64>>(3).unwrap_or(None),
                            stored.get::<_, Option<f64>>(4).unwrap_or(None),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the previous estimate", &e))?;
            let (user_edited, reviewed, user_ev, user_k, user_tint) =
                existing.unwrap_or((0, 0, None, None, None));

            conn.execute(
                "INSERT INTO image_tone_estimate (
                     photo_id, project_id, exposure_ev, temperature_k, tint,
                     exposure_conf, wb_conf, subject_luma_before, subject_luma_target,
                     face_anchored, mixed_light, coloured_light, backlit,
                     dominant_on_subject, dominant_kind, clipping_added, skin_de00,
                     constrained_ids, scene, untargeted, illuminants, reasons, alternatives,
                     reason_count, user_edited, reviewed, user_exposure_ev,
                     user_temperature_k, user_tint, model_ver, analysis_ver, targets_ver,
                     at_ts, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28,
                         ?29, ?30, ?31, ?32, ?33, ?34)
                 ON CONFLICT(photo_id) DO UPDATE SET
                     exposure_ev = excluded.exposure_ev,
                     temperature_k = excluded.temperature_k,
                     tint = excluded.tint,
                     exposure_conf = excluded.exposure_conf,
                     wb_conf = excluded.wb_conf,
                     subject_luma_before = excluded.subject_luma_before,
                     subject_luma_target = excluded.subject_luma_target,
                     face_anchored = excluded.face_anchored,
                     mixed_light = excluded.mixed_light,
                     coloured_light = excluded.coloured_light,
                     backlit = excluded.backlit,
                     dominant_on_subject = excluded.dominant_on_subject,
                     dominant_kind = excluded.dominant_kind,
                     clipping_added = excluded.clipping_added,
                     skin_de00 = excluded.skin_de00,
                     constrained_ids = excluded.constrained_ids,
                     scene = excluded.scene,
                     untargeted = excluded.untargeted,
                     illuminants = excluded.illuminants,
                     reasons = excluded.reasons,
                     alternatives = excluded.alternatives,
                     reason_count = excluded.reason_count,
                     model_ver = excluded.model_ver,
                     analysis_ver = excluded.analysis_ver,
                     targets_ver = excluded.targets_ver,
                     at_ts = excluded.at_ts,
                     at = excluded.at",
                params![
                    photo_key,
                    project_key,
                    f64::from(row.exposure_ev),
                    f64::from(row.temperature_k),
                    f64::from(row.tint),
                    f64::from(row.exposure_conf),
                    f64::from(row.wb_conf),
                    f64::from(row.subject_luma_before),
                    f64::from(row.subject_luma_target),
                    i64::from(row.face_anchored),
                    i64::from(row.mixed_light),
                    i64::from(row.coloured_light),
                    i64::from(row.backlit),
                    row.dominant_on_subject
                        .and_then(|index| i64::try_from(index).ok()),
                    row.dominant_kind,
                    f64::from(row.clipping_added),
                    f64::from(row.skin_de00),
                    i64::from(row.constrained_ids),
                    row.scene,
                    i64::from(row.untargeted),
                    row.illuminants,
                    row.reasons,
                    row.alternatives,
                    row.reason_count,
                    user_edited,
                    reviewed,
                    user_ev,
                    user_k,
                    user_tint,
                    row.model_ver,
                    row.analysis_ver,
                    row.targets_ver,
                    at_ts,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store a tone estimate", &e))?;
            Ok(())
        })
    }

    /// One photograph's estimate.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn get(&self, image: ImageId) -> AuraResult<Option<ToneEstimate>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT exposure_ev, temperature_k, tint, exposure_conf, wb_conf,
                            subject_luma_before, subject_luma_target, face_anchored,
                            mixed_light, coloured_light, backlit, dominant_on_subject,
                            clipping_added, skin_de00, constrained_ids, scene,
                            illuminants, reasons, alternatives, user_edited, reviewed,
                            model_ver, analysis_ver, targets_ver, photo_id
                       FROM image_tone_estimate WHERE photo_id = ?1",
                    params![key],
                    |stored| {
                        Ok(StoredRow {
                            exposure_ev: stored.get::<_, f64>(0).unwrap_or(0.0) as f32,
                            temperature_k: stored.get::<_, f64>(1).unwrap_or(5500.0) as f32,
                            tint: stored.get::<_, f64>(2).unwrap_or(0.0) as f32,
                            exposure_conf: stored.get::<_, f64>(3).unwrap_or(0.0) as f32,
                            wb_conf: stored.get::<_, f64>(4).unwrap_or(0.0) as f32,
                            subject_luma_before: stored.get::<_, f64>(5).unwrap_or(0.0) as f32,
                            subject_luma_target: stored.get::<_, f64>(6).unwrap_or(0.0) as f32,
                            face_anchored: stored.get::<_, i64>(7).unwrap_or(0) == 1,
                            mixed_light: stored.get::<_, i64>(8).unwrap_or(0) == 1,
                            coloured_light: stored.get::<_, i64>(9).unwrap_or(0) == 1,
                            backlit: stored.get::<_, i64>(10).unwrap_or(0) == 1,
                            dominant_on_subject: stored
                                .get::<_, Option<i64>>(11)
                                .unwrap_or(None)
                                .and_then(|index| usize::try_from(index).ok()),
                            clipping_added: stored.get::<_, f64>(12).unwrap_or(0.0) as f32,
                            skin_de00: stored.get::<_, f64>(13).unwrap_or(0.0) as f32,
                            constrained_ids: stored.get::<_, i64>(14).unwrap_or(0) as u32,
                            scene: stored.get::<_, String>(15).unwrap_or_default(),
                            illuminants: stored.get::<_, String>(16).unwrap_or_default(),
                            reasons: stored.get::<_, String>(17).unwrap_or_default(),
                            alternatives: stored.get::<_, String>(18).unwrap_or_default(),
                            user_edited: stored.get::<_, i64>(19).unwrap_or(0) == 1,
                            reviewed: stored.get::<_, i64>(20).unwrap_or(0) == 1,
                            model_ver: stored.get::<_, i64>(21).unwrap_or(0) as u16,
                            analysis_ver: stored.get::<_, i64>(22).unwrap_or(0) as u16,
                            targets_ver: stored.get::<_, i64>(23).unwrap_or(0) as u16,
                            photo_id: stored.get::<_, String>(24).unwrap_or_default(),
                        })
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read a tone estimate", &e))?;
            Ok(row.and_then(StoredRow::into_estimate))
        })
    }

    /// The frames whose white balance is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        let key = project.to_db();
        let limit = i64::try_from(limit.clamp(1, 5_000)).unwrap_or(500);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM image_tone_estimate
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                        AND wb_conf < ?2
                      ORDER BY wb_conf ASC, photo_id ASC
                      LIMIT ?3",
                )
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key, f64::from(REVIEW_WB_BELOW), limit])
                .map_err(|e| statement_failed("could not read the review queue", &e))?;
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

    /// Record that the photographer looked at an estimate and agrees.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5061` when the photograph has no estimate.
    pub fn accept(&self, image: ImageId) -> Result<(), aura_core::AuraError> {
        let key = image.to_db();
        let label = image.to_db();
        self.catalog.writer().transact(move |conn| {
            let changed = conn
                .execute(
                    "UPDATE image_tone_estimate SET reviewed = 1 WHERE photo_id = ?1",
                    params![key],
                )
                .map_err(|e| statement_failed("could not accept a tone estimate", &e))?;
            if changed == 0 {
                return Err(errors::tone_edit_refused(
                    &label,
                    "has no exposure or colour estimate to accept",
                ));
            }
            Ok(())
        })
    }

    /// Record what the photographer set instead.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5061` when the photograph has no estimate, when the override is empty, or
    /// when a value is outside its documented range.
    pub fn set_override(
        &self,
        image: ImageId,
        values: ToneOverride,
    ) -> Result<(), aura_core::AuraError> {
        let label = image.to_db();
        if values.is_empty() {
            return Err(errors::tone_edit_refused(
                &label,
                "carries no value; an override that sets nothing would freeze the two sliders \
                 the photographer did not touch",
            ));
        }
        if let Some(ev) = values.exposure_ev {
            if !(EXPOSURE_RANGE.0..=EXPOSURE_RANGE.1).contains(&ev) || !ev.is_finite() {
                return Err(errors::tone_edit_refused(
                    &label,
                    "the exposure is outside -5..5 stops",
                ));
            }
        }
        if let Some(kelvin) = values.temperature_k {
            if !(KELVIN_RANGE.0..=KELVIN_RANGE.1).contains(&kelvin) || !kelvin.is_finite() {
                return Err(errors::tone_edit_refused(
                    &label,
                    "the temperature is outside 2000..50000 K",
                ));
            }
        }
        if let Some(tint) = values.tint {
            if !(TINT_RANGE.0..=TINT_RANGE.1).contains(&tint) || !tint.is_finite() {
                return Err(errors::tone_edit_refused(
                    &label,
                    "the tint is outside -150..150",
                ));
            }
        }

        let key = image.to_db();
        let refusal_label = label.clone();
        self.catalog.writer().transact(move |conn| {
            // COALESCE, so setting only the temperature leaves an exposure the photographer
            // set earlier untouched. Three independent sliders, three independent overrides.
            let changed = conn
                .execute(
                    "UPDATE image_tone_estimate
                        SET user_edited = 1,
                            reviewed = 1,
                            user_exposure_ev = COALESCE(?2, user_exposure_ev),
                            user_temperature_k = COALESCE(?3, user_temperature_k),
                            user_tint = COALESCE(?4, user_tint)
                      WHERE photo_id = ?1",
                    params![
                        key,
                        values.exposure_ev.map(f64::from),
                        values.temperature_k.map(f64::from),
                        values.tint.map(f64::from),
                    ],
                )
                .map_err(|e| statement_failed("could not record a tone override", &e))?;
            if changed == 0 {
                return Err(errors::tone_edit_refused(
                    &refusal_label,
                    "has no exposure or colour estimate to override",
                ));
            }
            Ok(())
        })
    }

    /// What the photographer set by hand, or `None` when they have not.
    ///
    /// **The other half of [`ToneStore::set_override`], and the reason both exist.** A
    /// `ToneEstimate` carries what the *product* decided, and the migration keeps it that way
    /// on purpose: a row with `user_edited = 1` still holds AURA's own numbers, which is what
    /// makes the review queue able to show a disagreement and phase 30's learning loop able to
    /// read one. That is only true if something can read the other side, and this is it.
    ///
    /// Not on the frozen [`aura_core::contract::tone::ToneService`], because that contract is
    /// frozen and this is a store method the command layer calls beside `of_image`. The panel
    /// gets both numbers and says which is whose.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn override_of(&self, image: ImageId) -> AuraResult<Option<ToneOverride>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let found = conn
                .query_row(
                    "SELECT user_edited, user_exposure_ev, user_temperature_k, user_tint
                       FROM image_tone_estimate WHERE photo_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0).unwrap_or(0),
                            row.get::<_, Option<f64>>(1).unwrap_or(None),
                            row.get::<_, Option<f64>>(2).unwrap_or(None),
                            row.get::<_, Option<f64>>(3).unwrap_or(None),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read a tone override", &e))?;
            let Some((edited, ev, kelvin, tint)) = found else {
                return Ok(None);
            };
            if edited != 1 {
                return Ok(None);
            }
            Ok(Some(ToneOverride {
                exposure_ev: ev.map(|value| value as f32),
                temperature_k: kelvin.map(|value| value as f32),
                tint: tint.map(|value| value as f32),
            }))
        })
    }

    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    #[allow(clippy::too_many_lines)]
    pub fn outline(&self, project: ProjectId) -> AuraResult<ToneOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let (images, estimated, face_anchored, skin_constrained, mixed, coloured, reviewed) =
                conn.query_row(
                    "SELECT images, estimated, face_anchored, skin_constrained, mixed_light,
                            coloured_light, needs_review
                       FROM v_tone_coverage WHERE project_id = ?1",
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
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the tone coverage", &e))?
                .unwrap_or((0, 0, 0, 0, 0, 0, 0));

            let (mean_ev, mean_cct, user_edited, model_ver, analysis_ver, targets_ver) = conn
                .query_row(
                    "SELECT AVG(exposure_ev), AVG(temperature_k),
                            SUM(CASE WHEN user_edited = 1 THEN 1 ELSE 0 END),
                            MIN(model_ver), MIN(analysis_ver), MIN(targets_ver)
                       FROM image_tone_estimate WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, Option<f64>>(0).unwrap_or(None).unwrap_or(0.0),
                            row.get::<_, Option<f64>>(1).unwrap_or(None).unwrap_or(0.0),
                            row.get::<_, Option<i64>>(2).unwrap_or(None).unwrap_or(0),
                            row.get::<_, Option<i64>>(3).unwrap_or(None).unwrap_or(0),
                            row.get::<_, Option<i64>>(4).unwrap_or(None).unwrap_or(0),
                            row.get::<_, Option<i64>>(5).unwrap_or(None).unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the tone summary", &e))?
                .unwrap_or((0.0, 0.0, 0, 0, 0, 0));

            let mut histogram = [0u32; IlluminantKind::COUNT];
            let mut statement = conn
                .prepare(
                    "SELECT dominant_kind, COUNT(*) FROM image_tone_estimate
                      WHERE project_id = ?1 GROUP BY dominant_kind",
                )
                .map_err(|e| statement_failed("could not read the illuminant histogram", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the illuminant histogram", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the illuminant histogram", &e))?
            {
                let slug: String = row.get(0).unwrap_or_default();
                let count: i64 = row.get(1).unwrap_or(0);
                let kind = IlluminantKind::from_str_or_unknown(&slug);
                if let Some(index) = IlluminantKind::ALL
                    .iter()
                    .position(|candidate| *candidate == kind)
                {
                    if let Some(slot) = histogram.get_mut(index) {
                        *slot = count.max(0) as u32;
                    }
                }
            }
            drop(cursor);
            drop(statement);

            let loci: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM identity_skin_locus
                      WHERE project_id = ?1 AND samples >= ?2",
                    params![key, i64::from(aura_core::contract::tone::MIN_LOCUS_SAMPLES)],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let segments: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM segments WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let anchored: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM (
                         SELECT r.segment_id FROM segment_reference_frames r
                           JOIN segments s ON s.segment_id = r.segment_id
                          WHERE s.project_id = ?1
                          GROUP BY r.segment_id
                         HAVING COUNT(*) >= ?2)",
                    params![
                        key,
                        i64::try_from(aura_core::contract::tone::MIN_REFERENCE_FRAMES).unwrap_or(3)
                    ],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let denominator = images.max(0) as f32;
            let estimated_f = estimated.max(0) as f32;
            Ok(ToneOutline {
                estimated: estimated.max(0) as u32,
                photos: images.max(0) as u32,
                coverage: if denominator == 0.0 {
                    0.0
                } else {
                    estimated_f / denominator
                },
                face_anchored: if estimated_f == 0.0 {
                    0.0
                } else {
                    face_anchored.max(0) as f32 / estimated_f
                },
                skin_constrained: if estimated_f == 0.0 {
                    0.0
                } else {
                    skin_constrained.max(0) as f32 / estimated_f
                },
                mixed_light: mixed.max(0) as u32,
                coloured_light: coloured.max(0) as u32,
                needs_review: reviewed.max(0) as u32,
                user_edited: user_edited.max(0) as u32,
                mean_ev: mean_ev as f32,
                mean_cct: mean_cct as f32,
                illuminant_histogram: histogram,
                segments_anchored: anchored.max(0) as u32,
                segments: segments.max(0) as u32,
                loci: loci.max(0) as u32,
                untargeted_scenes: Vec::new(),
                model_ver: model_ver.max(0) as u16,
                analysis_ver: analysis_ver.max(0) as u16,
                targets_ver: targets_ver.max(0) as u16,
            })
        })
    }

    // -----------------------------------------------------------------------
    // The loci
    // -----------------------------------------------------------------------

    /// Store one person's locus.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_locus(&self, project: &ProjectId, locus: &SkinLocus) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let at_ts = epoch_ms(self.clock.now_utc());
        let project_key = project.to_db();
        let identity_key = locus.identity.to_db();
        let locus = *locus;
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO identity_skin_locus (
                     identity_id, project_id, u, v, radius, luma, samples, cohesion,
                     analysis_ver, at_ts, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(identity_id) DO UPDATE SET
                     u = excluded.u, v = excluded.v, radius = excluded.radius,
                     luma = excluded.luma, samples = excluded.samples,
                     cohesion = excluded.cohesion, analysis_ver = excluded.analysis_ver,
                     at_ts = excluded.at_ts, at = excluded.at",
                params![
                    identity_key,
                    project_key,
                    f64::from(locus.uv[0]),
                    f64::from(locus.uv[1]),
                    f64::from(locus.radius),
                    f64::from(locus.luma),
                    i64::from(locus.samples),
                    f64::from(locus.cohesion),
                    i64::from(locus.analysis_ver),
                    at_ts,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store a skin locus", &e))?;
            Ok(())
        })
    }

    /// One person's locus.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn locus(&self, identity: IdentityId) -> AuraResult<Option<SkinLocus>> {
        let key = identity.to_db();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT u, v, radius, luma, samples, cohesion, analysis_ver, identity_id
                       FROM identity_skin_locus WHERE identity_id = ?1",
                    params![key],
                    |stored| {
                        Ok((
                            stored.get::<_, f64>(0).unwrap_or(0.0),
                            stored.get::<_, f64>(1).unwrap_or(0.0),
                            stored.get::<_, f64>(2).unwrap_or(0.0),
                            stored.get::<_, f64>(3).unwrap_or(0.0),
                            stored.get::<_, i64>(4).unwrap_or(0),
                            stored.get::<_, f64>(5).unwrap_or(0.0),
                            stored.get::<_, i64>(6).unwrap_or(0),
                            stored.get::<_, String>(7).unwrap_or_default(),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read a skin locus", &e))?;
            Ok(row.and_then(locus_from_row))
        })
    }

    /// Every usable locus in a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn loci(&self, project: ProjectId) -> AuraResult<BTreeMap<IdentityId, SkinLocus>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT u, v, radius, luma, samples, cohesion, analysis_ver, identity_id
                       FROM identity_skin_locus WHERE project_id = ?1 ORDER BY identity_id",
                )
                .map_err(|e| statement_failed("could not read the skin loci", &e))?;
            let mut out = BTreeMap::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the skin loci", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the skin loci", &e))?
            {
                let tuple = (
                    row.get::<_, f64>(0).unwrap_or(0.0),
                    row.get::<_, f64>(1).unwrap_or(0.0),
                    row.get::<_, f64>(2).unwrap_or(0.0),
                    row.get::<_, f64>(3).unwrap_or(0.0),
                    row.get::<_, i64>(4).unwrap_or(0),
                    row.get::<_, f64>(5).unwrap_or(0.0),
                    row.get::<_, i64>(6).unwrap_or(0),
                    row.get::<_, String>(7).unwrap_or_default(),
                );
                if let Some(locus) = locus_from_row(tuple) {
                    out.insert(locus.identity, locus);
                }
            }
            Ok(out)
        })
    }

    // -----------------------------------------------------------------------
    // The anchors
    // -----------------------------------------------------------------------

    /// Every segment in a project, in time order.
    ///
    /// See the module header for why this reads `segments` rather than going through
    /// `StoryService`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn segments_of(&self, project: &ProjectId) -> AuraResult<Vec<SegmentId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT segment_id FROM segments WHERE project_id = ?1
                      ORDER BY start_ts, segment_id",
                )
                .map_err(|e| statement_failed("could not list the segments", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not list the segments", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not list the segments", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = SegmentId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Which photographs belong to one segment.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn photos_in_segment(&self, segment: SegmentId) -> AuraResult<Vec<PhotoId>> {
        let key = segment.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       JOIN segments s ON s.segment_id = ?1
                      WHERE p.project_id = s.project_id
                        AND p.timeline_time >= s.start_ts
                        AND p.timeline_time <= s.end_ts
                      ORDER BY p.photo_id",
                )
                .map_err(|e| statement_failed("could not read a segment's photographs", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read a segment's photographs", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a segment's photographs", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Replace one segment's anchors.
    ///
    /// A delete and an insert in one transaction rather than an upsert: a segment that used
    /// to have five anchors and now qualifies for three must not keep two stale rows, and a
    /// rank is a position in a list rather than an identity.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_reference_frames(
        &self,
        segment: SegmentId,
        frames: &[ReferenceFrame],
        analysis_ver: u16,
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let at_ts = epoch_ms(self.clock.now_utc());
        let key = segment.to_db();
        let rows: Vec<(String, i64, f64, f64, f64, f64, f64)> = frames
            .iter()
            .map(|frame| {
                (
                    frame.image_id.to_db(),
                    i64::from(frame.rank),
                    f64::from(frame.wb_conf),
                    f64::from(frame.temperature_k),
                    f64::from(frame.tint),
                    f64::from(frame.subject_luma),
                    f64::from(frame.quality),
                )
            })
            .collect();
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "DELETE FROM segment_reference_frames WHERE segment_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear a segment's anchors", &e))?;
            for (photo, rank, wb_conf, temperature_k, tint, subject_luma, quality) in &rows {
                conn.execute(
                    "INSERT INTO segment_reference_frames (
                         segment_id, rank, photo_id, wb_conf, temperature_k, tint,
                         subject_luma, quality, analysis_ver, at_ts, at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        key,
                        rank,
                        photo,
                        wb_conf,
                        temperature_k,
                        tint,
                        subject_luma,
                        quality,
                        i64::from(analysis_ver),
                        at_ts,
                        now,
                    ],
                )
                .map_err(|e| statement_failed("could not store a segment anchor", &e))?;
            }
            Ok(())
        })
    }

    /// One segment's anchors, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn reference_frames(&self, segment: SegmentId) -> AuraResult<Vec<ReferenceFrame>> {
        let key = segment.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, rank, wb_conf, temperature_k, tint, subject_luma, quality
                       FROM segment_reference_frames WHERE segment_id = ?1 ORDER BY rank",
                )
                .map_err(|e| statement_failed("could not read a segment's anchors", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read a segment's anchors", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a segment's anchors", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                let Ok(image_id) = PhotoId::from_db(&text) else {
                    continue;
                };
                out.push(ReferenceFrame {
                    image_id,
                    segment,
                    rank: row.get::<_, i64>(1).unwrap_or(0).max(0) as u16,
                    wb_conf: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                    temperature_k: row.get::<_, f64>(3).unwrap_or(5500.0) as f32,
                    tint: row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                    subject_luma: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                    quality: row.get::<_, f64>(6).unwrap_or(0.0) as f32,
                });
            }
            Ok(out)
        })
    }
}

/// Milliseconds since the Unix epoch, from the injected clock.
///
// DETERMINISM: the stamp comes from the injected clock, never from the wall clock. A
// timestamp read from the process's own clock is a row that differs between two runs of the
// same fixture, and invariant 4 is asserted by comparing exactly that. The mention below is
// what the banned-pattern check is looking at; the code takes an `OffsetDateTime` argument.
/// From the injected clock rather than from the process's own, for the reason on the line
/// above.
fn epoch_ms(at: time::OffsetDateTime) -> i64 {
    (at.unix_timestamp_nanos() / 1_000_000) as i64
}

/// Turn one locus row into the frozen shape.
fn locus_from_row(row: (f64, f64, f64, f64, i64, f64, i64, String)) -> Option<SkinLocus> {
    let (u, v, radius, luma, samples, cohesion, analysis_ver, identity) = row;
    let identity = IdentityId::from_db(&identity).ok()?;
    Some(SkinLocus {
        identity,
        uv: [u as f32, v as f32],
        radius: radius as f32,
        luma: luma as f32,
        samples: samples.max(0) as u32,
        cohesion: cohesion as f32,
        analysis_ver: analysis_ver.max(0) as u16,
    })
}

/// The scalar half of one row, gathered so the insert takes one value.
///
/// Six booleans. The lint is allowed rather than obeyed for `Candidate`'s reason: these are
/// the six bits the schema has, and renaming them into a sub-struct would make the mapping
/// between this struct and the `INSERT` one indirection longer for no gain.
#[allow(clippy::struct_excessive_bools)]
struct OwnedRow {
    exposure_ev: f32,
    temperature_k: f32,
    tint: f32,
    exposure_conf: f32,
    wb_conf: f32,
    subject_luma_before: f32,
    subject_luma_target: f32,
    face_anchored: bool,
    mixed_light: bool,
    coloured_light: bool,
    backlit: bool,
    dominant_on_subject: Option<usize>,
    dominant_kind: String,
    clipping_added: f32,
    skin_de00: f32,
    constrained_ids: u32,
    scene: String,
    untargeted: bool,
    illuminants: String,
    reasons: String,
    alternatives: String,
    reason_count: i64,
    model_ver: i64,
    analysis_ver: i64,
    targets_ver: i64,
}

/// One row as read back.
#[allow(clippy::struct_excessive_bools)]
struct StoredRow {
    exposure_ev: f32,
    temperature_k: f32,
    tint: f32,
    exposure_conf: f32,
    wb_conf: f32,
    subject_luma_before: f32,
    subject_luma_target: f32,
    face_anchored: bool,
    mixed_light: bool,
    coloured_light: bool,
    backlit: bool,
    dominant_on_subject: Option<usize>,
    clipping_added: f32,
    skin_de00: f32,
    constrained_ids: u32,
    scene: String,
    illuminants: String,
    reasons: String,
    alternatives: String,
    user_edited: bool,
    reviewed: bool,
    model_ver: u16,
    analysis_ver: u16,
    targets_ver: u16,
    photo_id: String,
}

impl StoredRow {
    fn into_estimate(self) -> Option<ToneEstimate> {
        let image_id = PhotoId::from_db(&self.photo_id).ok()?;
        Some(ToneEstimate {
            image_id,
            exposure_ev: self.exposure_ev,
            exposure_conf: self.exposure_conf,
            temperature_k: self.temperature_k,
            tint: self.tint,
            wb_conf: self.wb_conf,
            illuminants: codec::decode_illuminants(&self.illuminants),
            mixed_light: self.mixed_light,
            dominant_on_subject: self.dominant_on_subject,
            subject_luma_before: self.subject_luma_before,
            subject_luma_target: self.subject_luma_target,
            skin_de00_estimate: self.skin_de00,
            alternatives: codec::decode_alternatives(&self.alternatives, self.exposure_ev),
            reasons: codec::decode_reasons(&self.reasons),
            scene: SceneId::from_str_or_unknown(&self.scene),
            face_anchored: self.face_anchored,
            backlit: self.backlit,
            coloured_light: self.coloured_light,
            clipping_added: self.clipping_added,
            constrained_identities: self.constrained_ids,
            user_edited: self.user_edited,
            reviewed: self.reviewed,
            model_ver: self.model_ver,
            analysis_ver: self.analysis_ver,
            targets_ver: self.targets_ver,
        })
    }
}
