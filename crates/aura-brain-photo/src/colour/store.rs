//! Migration 16's one table, and the six statements that use it.
//!
//! The shape phases 09, 11 and 15 all settled on: a `pending` query rather than a journal, an
//! upsert that re-applies the photographer's own answer inside the same statement, an outline
//! built from the coverage view, and a decoder that is total.
//!
//! ## The one query that is not like the others
//!
//! [`ColourStore::outline`] reports `worst_skin_hue_shift`, which is a `MAX` over a column
//! rather than an average over a set. It is the number that falsifies this phase's headline
//! guarantee, and a project whose *worst* frame moved skin three degrees has broken the
//! promise however good its mean is. Migration 16 denormalises the column out of the guard
//! document for exactly this query.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::colour::{
    ColourDecision, ColourOutline, ColourOverride, ColourReason, ContentBand, HslAdjustments,
    ImageId, SkinGuardReport, ToneCurve, VariantKind, REVIEW_BELOW,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use crate::colour::codec;
use crate::errors;

/// What one decision is budgeted at on disk.
///
/// 1,600 bytes. Larger than any phase before it and deliberately so: three complete
/// alternative parameter sets are most of the row, and ADR-0033 decision 6 argues that a
/// delta would be smaller and wrong. Section 11 sets no storage budget for this phase, so
/// this is the phase's own, asserted by `aura-cli verify --phase 16`.
pub const BYTES_PER_IMAGE: usize = 1_600;

/// Migration 16's table.
pub struct ColourStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ColourStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColourStore").finish_non_exhaustive()
    }
}

impl ColourStore {
    /// Wrap one catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// Which photographs have no decision at this version.
    ///
    /// Invariant 5: the work remaining is a query rather than a journal, so a pass killed at
    /// any point resumes by asking the same question again. An `intent_ver` bump therefore
    /// heals itself - the rows made under the old table are pending by definition.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(
        &self,
        project: &ProjectId,
        model_ver: u16,
        analysis_ver: u16,
        intent_ver: u16,
    ) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN image_colour_decision c ON c.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (c.photo_id IS NULL
                             OR c.model_ver    <> ?2
                             OR c.analysis_ver <> ?3
                             OR c.intent_ver   <> ?4)
                      ORDER BY p.photo_id",
                )
                .map_err(|e| statement_failed("could not prepare the colour pending query", &e))?;
            let rows = statement
                .query_map(
                    params![
                        key,
                        i64::from(model_ver),
                        i64::from(analysis_ver),
                        i64::from(intent_ver)
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| statement_failed("could not read the colour pending set", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let id = row.map_err(|e| statement_failed("could not read a photo id", &e))?;
                if let Ok(parsed) = PhotoId::from_db(&id) {
                    out.push(parsed);
                }
            }
            Ok(out)
        })
    }

    /// Every photograph in a project, in a stable order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn all_photos(&self, project: &ProjectId) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id FROM photo WHERE project_id = ?1 ORDER BY photo_id")
                .map_err(|e| statement_failed("could not prepare the project photo query", &e))?;
            let rows = statement
                .query_map(params![key], |row| row.get::<_, String>(0))
                .map_err(|e| statement_failed("could not read the project's photographs", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let id = row.map_err(|e| statement_failed("could not read a photo id", &e))?;
                if let Ok(parsed) = PhotoId::from_db(&id) {
                    out.push(parsed);
                }
            }
            Ok(out)
        })
    }

    /// Store one decision, carrying forward whatever the photographer set.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)]
    pub fn put(
        &self,
        project: &ProjectId,
        decision: &ColourDecision,
        reason_numbers: &[Vec<f32>],
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let at_ts = epoch_ms(self.clock.now_utc());
        let project_key = project.to_db();
        let photo_key = decision.image_id.to_db();

        let curve = codec::encode_curve(&decision.curve);
        let hsl = codec::encode_hsl(&decision.hsl);
        let bands = codec::encode_bands(&decision.bands);
        let alternatives = codec::encode_variants(&decision.alternatives);
        let reasons = codec::encode_reasons(&decision.reasons, reason_numbers);
        let guard = codec::encode_guard(&decision.skin_guard);
        let untargeted = decision
            .reasons
            .iter()
            .any(|reason| reason.code == aura_core::contract::colour::ColourCode::NoIntentRow);
        let capped = decision
            .reasons
            .iter()
            .any(|reason| reason.code == aura_core::contract::colour::ColourCode::SubtletyCapped);
        let clip_resolved = decision.reasons.iter().any(|reason| {
            reason.code == aura_core::contract::colour::ColourCode::ClippingGuardResolved
        });
        let reason_count =
            i64::try_from(decision.reasons.len().clamp(1, ColourDecision::MAX_REASONS))
                .unwrap_or(1);

        let row = OwnedRow {
            contrast: decision.contrast,
            highlights: decision.highlights,
            shadows: decision.shadows,
            whites: decision.whites,
            blacks: decision.blacks,
            vibrance: decision.vibrance,
            saturation: decision.saturation,
            curve,
            hsl,
            bands,
            alternatives,
            reasons,
            guard,
            skin_measured: decision.skin_measured,
            skin_hue_shift: decision.skin_guard.max_hue_shift_deg,
            skin_chroma_change: decision.skin_guard.max_chroma_change,
            skin_attenuation: decision.skin_guard.attenuation,
            skin_resolves: decision.skin_guard.resolves,
            clip_before_high: decision.clipping_before.0,
            clip_before_low: decision.clipping_before.1,
            clip_after_high: decision.clipping_after.0,
            clip_after_low: decision.clipping_after.1,
            confidence: decision.confidence,
            subtlety: decision.subtlety,
            scene: decision.scene.as_str().to_string(),
            untargeted,
            guard_triggered: decision.skin_guard.intervened(),
            clip_resolved,
            subtlety_capped: capped,
            reason_count,
            model_ver: i64::from(decision.model_ver),
            analysis_ver: i64::from(decision.analysis_ver),
            intent_ver: i64::from(decision.intent_ver),
        };

        self.catalog.writer().transact(move |conn| {
            // The photographer's own answer and the two bits that protect it, read and
            // rewritten inside the same statement. See migration 16's note 4.
            let existing: Option<(i64, i64, Option<String>)> = conn
                .query_row(
                    "SELECT user_edited, reviewed, user_values
                       FROM image_colour_decision WHERE photo_id = ?1",
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
                .map_err(|e| statement_failed("could not read the previous grade", &e))?;
            let (user_edited, reviewed, user_values) = existing.unwrap_or((0, 0, None));

            conn.execute(
                "INSERT INTO image_colour_decision (
                     photo_id, project_id, contrast, highlights, shadows, whites, blacks,
                     vibrance, saturation, curve, hsl, bands, alternatives, reasons,
                     skin_guard, skin_measured, skin_hue_shift, skin_chroma_change,
                     skin_attenuation, skin_resolves, clip_before_high, clip_before_low,
                     clip_after_high, clip_after_low, confidence, subtlety, scene, untargeted,
                     guard_triggered, clip_resolved, subtlety_capped, reason_count,
                     user_edited, reviewed, user_values, model_ver, analysis_ver, intent_ver,
                     at_ts, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28,
                         ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40)
                 ON CONFLICT(photo_id) DO UPDATE SET
                     contrast = excluded.contrast,
                     highlights = excluded.highlights,
                     shadows = excluded.shadows,
                     whites = excluded.whites,
                     blacks = excluded.blacks,
                     vibrance = excluded.vibrance,
                     saturation = excluded.saturation,
                     curve = excluded.curve,
                     hsl = excluded.hsl,
                     bands = excluded.bands,
                     alternatives = excluded.alternatives,
                     reasons = excluded.reasons,
                     skin_guard = excluded.skin_guard,
                     skin_measured = excluded.skin_measured,
                     skin_hue_shift = excluded.skin_hue_shift,
                     skin_chroma_change = excluded.skin_chroma_change,
                     skin_attenuation = excluded.skin_attenuation,
                     skin_resolves = excluded.skin_resolves,
                     clip_before_high = excluded.clip_before_high,
                     clip_before_low = excluded.clip_before_low,
                     clip_after_high = excluded.clip_after_high,
                     clip_after_low = excluded.clip_after_low,
                     confidence = excluded.confidence,
                     subtlety = excluded.subtlety,
                     scene = excluded.scene,
                     untargeted = excluded.untargeted,
                     guard_triggered = excluded.guard_triggered,
                     clip_resolved = excluded.clip_resolved,
                     subtlety_capped = excluded.subtlety_capped,
                     reason_count = excluded.reason_count,
                     model_ver = excluded.model_ver,
                     analysis_ver = excluded.analysis_ver,
                     intent_ver = excluded.intent_ver,
                     at_ts = excluded.at_ts,
                     at = excluded.at",
                params![
                    photo_key,
                    project_key,
                    f64::from(row.contrast),
                    f64::from(row.highlights),
                    f64::from(row.shadows),
                    f64::from(row.whites),
                    f64::from(row.blacks),
                    f64::from(row.vibrance),
                    f64::from(row.saturation),
                    row.curve,
                    row.hsl,
                    row.bands,
                    row.alternatives,
                    row.reasons,
                    row.guard,
                    i64::from(row.skin_measured),
                    f64::from(row.skin_hue_shift),
                    f64::from(row.skin_chroma_change),
                    f64::from(row.skin_attenuation),
                    i64::from(row.skin_resolves),
                    f64::from(row.clip_before_high),
                    f64::from(row.clip_before_low),
                    f64::from(row.clip_after_high),
                    f64::from(row.clip_after_low),
                    f64::from(row.confidence),
                    f64::from(row.subtlety),
                    row.scene,
                    i64::from(row.untargeted),
                    i64::from(row.guard_triggered),
                    i64::from(row.clip_resolved),
                    i64::from(row.subtlety_capped),
                    row.reason_count,
                    user_edited,
                    reviewed,
                    user_values,
                    row.model_ver,
                    row.analysis_ver,
                    row.intent_ver,
                    at_ts,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store a colour decision", &e))?;
            Ok(())
        })
    }

    /// One photograph's decision.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn get(&self, image: ImageId) -> AuraResult<Option<ColourDecision>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let row = conn
                .query_row(
                    "SELECT contrast, highlights, shadows, whites, blacks, vibrance, saturation,
                            curve, hsl, bands, alternatives, reasons, skin_guard, skin_measured,
                            clip_before_high, clip_before_low, clip_after_high, clip_after_low,
                            confidence, subtlety, scene, user_edited, reviewed,
                            model_ver, analysis_ver, intent_ver
                       FROM image_colour_decision WHERE photo_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, f64>(0)?,
                            row.get::<_, f64>(1)?,
                            row.get::<_, f64>(2)?,
                            row.get::<_, f64>(3)?,
                            row.get::<_, f64>(4)?,
                            row.get::<_, f64>(5)?,
                            row.get::<_, f64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, String>(9)?,
                            row.get::<_, String>(10)?,
                            row.get::<_, String>(11)?,
                            row.get::<_, String>(12)?,
                            row.get::<_, i64>(13)?,
                            row.get::<_, f64>(14)?,
                            row.get::<_, f64>(15)?,
                            row.get::<_, f64>(16)?,
                            row.get::<_, f64>(17)?,
                            row.get::<_, f64>(18)?,
                            row.get::<_, f64>(19)?,
                            row.get::<_, String>(20)?,
                            row.get::<_, i64>(21)?,
                            row.get::<_, i64>(22)?,
                            row.get::<_, i64>(23)?,
                            row.get::<_, i64>(24)?,
                            row.get::<_, i64>(25)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read a colour decision", &e))?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(ColourDecision {
                image_id: image,
                contrast: row.0 as f32,
                highlights: row.1 as f32,
                shadows: row.2 as f32,
                whites: row.3 as f32,
                blacks: row.4 as f32,
                vibrance: row.5 as f32,
                saturation: row.6 as f32,
                curve: codec::decode_curve(&row.7),
                hsl: codec::decode_hsl(&row.8),
                bands: codec::decode_bands(&row.9),
                alternatives: codec::decode_variants(&row.10),
                reasons: codec::decode_reasons(&row.11),
                skin_guard: codec::decode_guard(&row.12),
                skin_measured: row.13 != 0,
                clipping_before: (row.14 as f32, row.15 as f32),
                clipping_after: (row.16 as f32, row.17 as f32),
                confidence: row.18 as f32,
                subtlety: row.19 as f32,
                scene: SceneId::from_str_or_unknown(&row.20),
                user_edited: row.21 != 0,
                reviewed: row.22 != 0,
                model_ver: u16::try_from(row.23).unwrap_or(0),
                analysis_ver: u16::try_from(row.24).unwrap_or(0),
                intent_ver: u16::try_from(row.25).unwrap_or(0),
            }))
        })
    }

    /// The frames whose grade is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        let key = project.to_db();
        let bound = i64::try_from(limit.clamp(1, 5_000)).unwrap_or(200);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id FROM image_colour_decision
                      WHERE project_id = ?1 AND reviewed = 0 AND user_edited = 0
                        AND confidence < ?2
                      ORDER BY confidence ASC, photo_id ASC
                      LIMIT ?3",
                )
                .map_err(|e| statement_failed("could not prepare the colour review query", &e))?;
            let rows = statement
                .query_map(params![key, f64::from(REVIEW_BELOW), bound], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|e| statement_failed("could not read the colour review queue", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let id = row.map_err(|e| statement_failed("could not read a photo id", &e))?;
                if let Ok(parsed) = PhotoId::from_db(&id) {
                    out.push(parsed);
                }
            }
            Ok(out)
        })
    }

    /// Record that the photographer has looked at one decision and agrees.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5067` when the photograph has no decision.
    pub fn accept(&self, image: ImageId) -> Result<(), aura_core::AuraError> {
        let key = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE image_colour_decision SET reviewed = 1 WHERE photo_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not record the review", &e))
        })?;
        if changed == 0 {
            return Err(errors::colour_edit_refused(
                "accept",
                "the photograph has no grade to accept",
            ));
        }
        Ok(())
    }

    /// Record what the photographer set instead.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5067` when the photograph has no decision or the override is empty.
    pub fn set_override(
        &self,
        image: ImageId,
        values: &ColourOverride,
    ) -> Result<(), aura_core::AuraError> {
        if values.is_empty() {
            return Err(errors::colour_edit_refused(
                "override",
                "no value was set; an empty override would mark the frame as yours and stop \
                 every later pass touching it, which is a trap rather than a no-op",
            ));
        }
        let encoded = serde_json::to_string(values).map_err(|err| {
            errors::colour_edit_refused("override", &format!("could not be encoded: {err}"))
        })?;
        let key = image.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            conn.execute(
                "UPDATE image_colour_decision
                    SET user_edited = 1, reviewed = 1, user_values = ?2
                  WHERE photo_id = ?1",
                params![key, encoded],
            )
            .map_err(|e| statement_failed("could not record the override", &e))
        })?;
        if changed == 0 {
            return Err(errors::colour_edit_refused(
                "override",
                "the photograph has no grade to override",
            ));
        }
        Ok(())
    }

    /// What the photographer set, when they set anything.
    ///
    /// Beside the frozen service rather than on it, exactly as phase 15's `override_of` is:
    /// a stored decision carries **both** answers, and the review queue exists to show the
    /// difference between them.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn override_of(&self, image: ImageId) -> AuraResult<Option<ColourOverride>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let stored: Option<Option<String>> = conn
                .query_row(
                    "SELECT user_values FROM image_colour_decision WHERE photo_id = ?1",
                    params![key],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(|e| statement_failed("could not read the override", &e))?;
            Ok(stored
                .flatten()
                .and_then(|text| serde_json::from_str::<ColourOverride>(&text).ok()))
        })
    }

    /// Promote one stored alternative to the primary grade.
    ///
    /// Safe because every variant has already been through both guards - ADR-0033 decision 6 -
    /// so the promoted set is one somebody checked. The promotion is recorded as the
    /// photographer's own, because it is: they looked at three answers and chose.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5067` when the photograph has no decision or no variant of that kind.
    pub fn select_variant(
        &self,
        image: ImageId,
        kind: VariantKind,
    ) -> Result<(), aura_core::AuraError> {
        let Some(decision) = self.get(image)? else {
            return Err(errors::colour_edit_refused(
                "variant",
                "the photograph has no grade to switch",
            ));
        };
        let Some(variant) = decision.variant(kind) else {
            return Err(errors::colour_edit_refused(
                "variant",
                &format!(
                    "this photograph has no `{}` alternative; one that would have looked the \
                     same as the main grade is dropped rather than stored",
                    kind.as_str()
                ),
            ));
        };
        self.set_override(
            image,
            &ColourOverride {
                contrast: Some(variant.contrast),
                highlights: Some(variant.highlights),
                shadows: Some(variant.shadows),
                whites: Some(variant.whites),
                blacks: Some(variant.blacks),
                vibrance: Some(variant.vibrance),
                saturation: Some(variant.saturation),
                curve: Some(variant.curve.clone()),
                hsl: Some(variant.hsl),
            },
        )
    }

    /// How much of a project has been graded.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    #[allow(clippy::too_many_lines)]
    pub fn outline(&self, project: ProjectId) -> AuraResult<ColourOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let coverage = conn
                .query_row(
                    "SELECT images, decided, skin_measured, skin_guard_triggered,
                            clip_guard_resolved, subtlety_capped, user_edited, needs_review,
                            worst_skin_hue_shift
                       FROM v_colour_coverage WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                            row.get::<_, Option<f64>>(8)?.unwrap_or(0.0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the colour coverage", &e))?
                .unwrap_or((0, 0, 0, 0, 0, 0, 0, 0, 0.0));

            let means = conn
                .query_row(
                    "SELECT AVG(contrast), AVG(shadows), AVG(subtlety),
                            MIN(model_ver), MIN(analysis_ver), MIN(intent_ver)
                       FROM image_colour_decision WHERE project_id = ?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, Option<f64>>(0)?.unwrap_or(0.0),
                            row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                            row.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                            row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                            row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("could not read the colour means", &e))?
                .unwrap_or((0.0, 0.0, 0.0, 0, 0, 0));

            let photos = u32::try_from(coverage.0).unwrap_or(0);
            let decided = u32::try_from(coverage.1).unwrap_or(0);
            Ok(ColourOutline {
                photos,
                decided,
                coverage: if photos == 0 {
                    0.0
                } else {
                    f64::from(decided) / f64::from(photos)
                },
                skin_measured: u32::try_from(coverage.2).unwrap_or(0),
                skin_guard_triggered: u32::try_from(coverage.3).unwrap_or(0),
                clip_guard_resolved: u32::try_from(coverage.4).unwrap_or(0),
                subtlety_capped: u32::try_from(coverage.5).unwrap_or(0),
                user_edited: u32::try_from(coverage.6).unwrap_or(0),
                needs_review: u32::try_from(coverage.7).unwrap_or(0),
                worst_skin_hue_shift: coverage.8 as f32,
                mean_contrast: means.0 as f32,
                mean_shadow_lift: means.1 as f32,
                mean_subtlety: means.2 as f32,
                untargeted_scenes: Vec::new(),
                model_ver: u16::try_from(means.3).unwrap_or(0),
                analysis_ver: u16::try_from(means.4).unwrap_or(0),
                intent_ver: u16::try_from(means.5).unwrap_or(0),
            })
        })
    }

    /// How many bytes one project's decisions occupy, for the budget assertion.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn bytes_for(&self, project: &ProjectId) -> AuraResult<(u64, u64)> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(LENGTH(curve) + LENGTH(hsl) + LENGTH(bands)
                                     + LENGTH(alternatives) + LENGTH(reasons)
                                     + LENGTH(skin_guard) + LENGTH(scene)
                                     + LENGTH(COALESCE(user_values, '')) + 160), 0)
                   FROM image_colour_decision WHERE project_id = ?1",
                params![key],
                |row| {
                    Ok((
                        u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                        u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    ))
                },
            )
            .map_err(|e| statement_failed("could not measure the colour rows", &e))
        })
    }
}

/// The reasons, bands and guard a decision carries, ready for one statement.
///
/// Six bools, and they stay six: each one is a separate column with a separate `CHECK` and a
/// separate meaning, and collapsing them into a bitfield would make the row unreadable in a
/// `SELECT` - which is the one place a support engineer looks first.
#[allow(clippy::struct_excessive_bools)]
struct OwnedRow {
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    vibrance: f32,
    saturation: f32,
    curve: String,
    hsl: String,
    bands: String,
    alternatives: String,
    reasons: String,
    guard: String,
    skin_measured: bool,
    skin_hue_shift: f32,
    skin_chroma_change: f32,
    skin_attenuation: f32,
    skin_resolves: u8,
    clip_before_high: f32,
    clip_before_low: f32,
    clip_after_high: f32,
    clip_after_low: f32,
    confidence: f32,
    subtlety: f32,
    scene: String,
    untargeted: bool,
    guard_triggered: bool,
    clip_resolved: bool,
    subtlety_capped: bool,
    reason_count: i64,
    model_ver: i64,
    analysis_ver: i64,
    intent_ver: i64,
}

/// Milliseconds since the epoch, for the sortable column beside the RFC 3339 one.
fn epoch_ms(now: time::OffsetDateTime) -> i64 {
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

/// Whether a decision names one content band, for the panel's caveat line.
#[must_use]
pub fn mentions(decision: &ColourDecision, band: ContentBand) -> bool {
    decision.bands.iter().any(|reading| reading.band == band)
}

/// A decision's curve and bands as the recipe would hold them, for the command module.
///
/// Here rather than in `aura-app` so that the one place that knows how a stored decision
/// becomes a recipe document is beside the one that knows how it becomes a row.
#[must_use]
pub fn recipe_shapes(decision: &ColourDecision) -> (ToneCurve, HslAdjustments) {
    (decision.curve.clone(), decision.hsl)
}

/// The guard report a decision carries, for the gate.
#[must_use]
pub fn guard_of(decision: &ColourDecision) -> SkinGuardReport {
    decision.skin_guard
}

/// The reasons a decision carries, for the gate.
#[must_use]
pub fn reasons_of(decision: &ColourDecision) -> &[ColourReason] {
    &decision.reasons
}
