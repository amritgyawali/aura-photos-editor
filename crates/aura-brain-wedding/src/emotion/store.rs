//! The five tables migration 10 adds, and the three rules that live in the SQL rather
//! than around it.
//!
//! ## `user_chosen` is checked inside the statement, not before it
//!
//! A re-score overwrites every peak it recomputes. The upsert re-applies the
//! photographer's choice from the row it is replacing, in the same statement, so a
//! choice cannot be lost by a background pass that read the flag a moment earlier and
//! wrote a moment later. Phase 06 wrote this rule for `identities`, phase 07 for
//! `segments`, phase 08 for `moments`, phase 09 for `image_integrity`; this is the fifth
//! time and the window it closes is the same one every time.
//!
//! ## Per-mille integers, in one place
//!
//! The eight expression channels and the confidence are `INTEGER` columns holding
//! thousandths. That is one of the three decisions section 11's 900-byte budget forced -
//! eight REALs is 64 bytes a face and 141 a photograph at this product's 2.2 faces a
//! frame - and the conversion lives in [`milli`] and [`unmilli`] so that no call site
//! divides by a thousand.
//!
//! ## Why the store reads `moment_images` and `faces` directly
//!
//! For `IntegrityStore`'s reason, one phase on. The peak pass needs to know which frames
//! were shot together and the expression pass needs to know where the faces are, and
//! both are already stored. Calling `MomentService::moment_of` once per frame is four
//! thousand round trips to answer a question one query answers; what this must never do -
//! and cannot, because it computes no cadence and builds no graph - is produce a grouping
//! of its own. ADR-0021 section 6.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::emotion::{
    EmotionCode, EmotionOutline, EmotionReason, FaceExpression, GazeTarget, ImageEmotion, ImageId,
    Interaction, MomentPeak, PeakKind, Preference, ReactionLink,
};
use aura_core::contract::integrity::CropRect;
use aura_core::contract::scene::Source;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult, FaceId, MomentId, PhotoId, ProjectId, SceneId};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::errors;

/// Bytes one photograph costs across the emotion tables and their index.
///
/// Section 11 budgets **900 B per image**, which is less than phase 09's kilobyte for a
/// row that carries per-face readings as well. Three decisions in migration 10 exist
/// because of it: per-mille integers for the eight channels, no identity or timestamp on
/// `face_expression`, and codes rather than sentences in `reasons`.
///
/// **Measured, not estimated.** `crates/aura-perf/tests/emotion_budgets.rs` reads
/// `PRAGMA page_count` on a real catalog before and after, exactly as phases 07, 08 and
/// 09 do.
pub const BYTES_PER_IMAGE: usize = 900;

/// A `0..1` reading as thousandths, for storage.
#[must_use]
pub fn milli(value: f32) -> i64 {
    i64::from((value.clamp(0.0, 1.0) * 1000.0).round() as u16).clamp(0, 1000)
}

/// Thousandths back into a `0..1` reading.
#[must_use]
pub fn unmilli(value: i64) -> f32 {
    f32::from(u16::try_from(value.clamp(0, 1000)).unwrap_or(0)) / 1000.0
}

/// How the `reasons` column is shaped.
///
/// A JSON array of these. Short keys because section 11's budget is 900 bytes and six
/// reasons with three-letter keys would be a quarter of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredReason {
    #[serde(rename = "c")]
    code: String,
    /// Present **only** when it differs from `EmotionCode::user_text`.
    ///
    /// Phase 09's decision, inherited without change. In practice three reasons in this
    /// phase carry text - the ones that name an interaction, a margin or a gap in
    /// seconds - and storing the other seventeen would be most of the per-image budget
    /// spent on copy a release can change.
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "w")]
    weight: f32,
    #[serde(rename = "e", skip_serializing_if = "Option::is_none")]
    evidence: Option<[f32; 4]>,
}

/// Encode a reason list for storage.
#[must_use]
pub fn encode_reasons(reasons: &[EmotionReason]) -> String {
    let stored: Vec<StoredReason> = reasons
        .iter()
        .map(|reason| StoredReason {
            code: reason.code.as_str().to_string(),
            text: (reason.text != reason.code.user_text()).then(|| reason.text.clone()),
            weight: reason.weight,
            evidence: reason.evidence.map(|crop| [crop.x, crop.y, crop.w, crop.h]),
        })
        .collect();
    serde_json::to_string(&stored).unwrap_or_else(|_| "[]".to_string())
}

/// Decode a stored reason list.
#[must_use]
pub fn decode_reasons(text: &str) -> Vec<EmotionReason> {
    let Ok(stored) = serde_json::from_str::<Vec<StoredReason>>(text) else {
        return Vec::new();
    };
    stored
        .into_iter()
        .map(|row| {
            let code = EmotionCode::from_str_or_unremarkable(&row.code);
            EmotionReason {
                code,
                text: row.text.unwrap_or_else(|| code.user_text().to_string()),
                weight: row.weight,
                evidence: row.evidence.map(|values| CropRect {
                    x: values[0],
                    y: values[1],
                    w: values[2],
                    h: values[3],
                }),
            }
        })
        .collect()
}

/// Encode the interactions column: `[[slug, strength_milli], ...]`.
#[must_use]
pub fn encode_interactions(interactions: &[(Interaction, f32)]) -> String {
    let stored: Vec<(String, i64)> = interactions
        .iter()
        .map(|(kind, strength)| (kind.as_str().to_string(), milli(*strength)))
        .collect();
    serde_json::to_string(&stored).unwrap_or_else(|_| "[]".to_string())
}

/// Decode the interactions column.
///
/// An unrecognised slug is dropped rather than defaulted, for the reason
/// `Interaction::from_slug` returns an `Option`: there is no interaction that claims less
/// than the others, so a slug a newer build wrote must disappear rather than become a
/// hug.
#[must_use]
pub fn decode_interactions(text: &str) -> Vec<(Interaction, f32)> {
    let Ok(stored) = serde_json::from_str::<Vec<(String, i64)>>(text) else {
        return Vec::new();
    };
    stored
        .into_iter()
        .filter_map(|(slug, strength)| {
            Interaction::from_slug(&slug).map(|kind| (kind, unmilli(strength)))
        })
        .collect()
}

/// Reads and writes the emotion tables.
#[derive(Debug)]
pub struct EmotionStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

/// One face row, ready for the writer thread.
#[derive(Debug, Clone)]
struct OwnedFace {
    face_id: String,
    channels: [i64; FaceExpression::CHANNELS],
    gaze: String,
    confidence: i64,
}

impl From<&FaceExpression> for OwnedFace {
    fn from(face: &FaceExpression) -> Self {
        let raw = face.channels();
        let mut channels = [0i64; FaceExpression::CHANNELS];
        for (slot, value) in channels.iter_mut().zip(raw.iter()) {
            *slot = milli(*value);
        }
        Self {
            face_id: face.face_id.to_db(),
            channels,
            gaze: face.gaze.as_str().to_string(),
            confidence: milli(face.confidence),
        }
    }
}

impl EmotionStore {
    /// Use one catalog's emotion tables.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// Photographs in this project that have not been scored at this version.
    ///
    /// **The work remaining is a query, not a journal** - invariant 5, the same way every
    /// pass since phase 05 does it. A `weights_ver` bump therefore heals itself: the rows
    /// scored under the old table are pending by definition.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(
        &self,
        project: &ProjectId,
        model_ver: u16,
        analysis_ver: u16,
        weights_ver: u16,
    ) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN image_interaction e ON e.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (e.photo_id IS NULL
                             OR e.model_ver <> ?2
                             OR e.analysis_ver <> ?3
                             OR e.weights_ver <> ?4)
                      ORDER BY p.timeline_time, p.photo_id",
                )
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![
                    key,
                    i64::from(model_ver),
                    i64::from(analysis_ver),
                    i64::from(weights_ver)
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

    /// Store one frame's reading and its faces.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)]
    pub fn put(&self, project: &ProjectId, result: &ImageEmotion) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let project_key = project.to_db();
        let photo_key = result.image_id.to_db();
        let interactions = encode_interactions(&result.interactions);
        let reasons = encode_reasons(&result.reasons);
        let faces: Vec<OwnedFace> = result.faces.iter().map(OwnedFace::from).collect();
        let reaction_of = result.reaction_of.map(|id| id.to_db());
        let scene = result.scene.as_str().to_string();
        let source = result.source.as_str().to_string();
        let mutual = i64::from(result.mutual_gaze);
        let peak_proximity = f64::from(result.peak_proximity);
        let emotion_score = f64::from(result.emotion_score);
        let narrative = f64::from(result.narrative_weight);
        let confidence = f64::from(result.confidence);
        let model_ver = i64::from(result.model_ver);
        let analysis_ver = i64::from(result.analysis_ver);
        let weights_ver = i64::from(result.weights_ver);

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("could not open the emotion transaction", &e))?;

            // `source` is carried forward when the stored row is a user override.
            // Phase 07's `image_scenes.source = 'user'` rule, applied to a score: a
            // photographer who has said something about a frame has ended that argument,
            // and a background re-score must not quietly reopen it.
            tx.execute(
                "INSERT INTO image_interaction (
                     photo_id, project_id, interactions, mutual_gaze,
                     peak_proximity, reaction_of,
                     emotion_score, narrative_weight,
                     scene, reasons, confidence, source,
                     model_ver, analysis_ver, weights_ver,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?16)
                 ON CONFLICT(photo_id) DO UPDATE SET
                     interactions     = excluded.interactions,
                     mutual_gaze      = excluded.mutual_gaze,
                     peak_proximity   = excluded.peak_proximity,
                     reaction_of      = excluded.reaction_of,
                     emotion_score    = CASE WHEN image_interaction.source = 'user'
                                             THEN image_interaction.emotion_score
                                             ELSE excluded.emotion_score END,
                     narrative_weight = excluded.narrative_weight,
                     scene            = excluded.scene,
                     reasons          = excluded.reasons,
                     confidence       = excluded.confidence,
                     source           = CASE WHEN image_interaction.source = 'user'
                                             THEN 'user' ELSE excluded.source END,
                     model_ver        = excluded.model_ver,
                     analysis_ver     = excluded.analysis_ver,
                     weights_ver      = excluded.weights_ver,
                     updated_at       = excluded.updated_at",
                params![
                    photo_key,
                    project_key,
                    interactions,
                    mutual,
                    peak_proximity,
                    reaction_of,
                    emotion_score,
                    narrative,
                    scene,
                    reasons,
                    confidence,
                    source,
                    model_ver,
                    analysis_ver,
                    weights_ver,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store an emotion reading", &e))?;

            // The face rows are replaced wholesale for this photograph. A face that has
            // gone - because the face pass re-ran at a new detector version and derived
            // different ids - must not leave an orphan expression behind claiming
            // somebody was laughing.
            tx.execute(
                "DELETE FROM face_expression
                  WHERE face_id IN (SELECT id FROM faces WHERE image_id = ?1)",
                params![photo_key],
            )
            .map_err(|e| statement_failed("could not clear stale expressions", &e))?;

            for face in &faces {
                tx.execute(
                    "INSERT OR REPLACE INTO face_expression (
                         face_id, smile, genuineness, laughter, tears,
                         surprise, tenderness, composed, discomfort,
                         gaze, confidence, model_ver)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        face.face_id,
                        face.channels[0],
                        face.channels[1],
                        face.channels[2],
                        face.channels[3],
                        face.channels[4],
                        face.channels[5],
                        face.channels[6],
                        face.channels[7],
                        face.gaze,
                        face.confidence,
                        model_ver,
                    ],
                )
                .map_err(|e| statement_failed("could not store an expression", &e))?;
            }

            tx.commit()
                .map_err(|e| statement_failed("could not commit the emotion reading", &e))
        })
    }

    /// One photograph's reading.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn get(&self, image: ImageId) -> AuraResult<Option<ImageEmotion>> {
        let key = image.to_db();
        let faces = self.faces_of(image)?;
        self.catalog.read(move |conn| {
            let found = conn.query_row(
                "SELECT interactions, mutual_gaze, peak_proximity, reaction_of,
                        emotion_score, narrative_weight, scene, reasons, confidence, source,
                        model_ver, analysis_ver, weights_ver
                   FROM image_interaction WHERE photo_id = ?1",
                params![key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, f64>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, i64>(12)?,
                    ))
                },
            );
            match found {
                Ok(row) => Ok(Some(ImageEmotion {
                    image_id: image,
                    faces,
                    interactions: decode_interactions(&row.0),
                    mutual_gaze: row.1 != 0,
                    peak_proximity: row.2 as f32,
                    reaction_of: row
                        .3
                        .as_deref()
                        .and_then(|text| PhotoId::from_db(text).ok()),
                    emotion_score: row.4 as f32,
                    narrative_weight: row.5 as f32,
                    scene: SceneId::from_str_or_unknown(&row.6),
                    reasons: decode_reasons(&row.7),
                    confidence: row.8 as f32,
                    source: Source::from_str_or_local(&row.9),
                    model_ver: u16::try_from(row.10).unwrap_or(0),
                    analysis_ver: u16::try_from(row.11).unwrap_or(0),
                    weights_ver: u16::try_from(row.12).unwrap_or(0),
                })),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed("could not read an emotion reading", &err)),
            }
        })
    }

    /// One photograph's face readings, in phase 06's detection order.
    ///
    /// The geometry and the identity come from `faces`, which is why
    /// `face_expression` stores neither. Migration 10's argument, as a join.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn faces_of(&self, image: ImageId) -> AuraResult<Vec<FaceExpression>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT f.id, f.identity_id, f.x, f.y, f.w, f.h,
                            x.smile, x.genuineness, x.laughter, x.tears,
                            x.surprise, x.tenderness, x.composed, x.discomfort,
                            x.gaze, x.confidence
                       FROM face_expression x
                       JOIN faces f ON f.id = x.face_id
                      WHERE f.image_id = ?1
                      ORDER BY f.area_frac DESC, f.id",
                )
                .map_err(|e| statement_failed("could not read expressions", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read expressions", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read expressions", &e))?
            {
                let face_text: String = row.get(0).map_err(|e| statement_failed("face id", &e))?;
                let Ok(face_id) = FaceId::from_db(&face_text) else {
                    continue;
                };
                let identity = row
                    .get::<_, Option<String>>(1)
                    .ok()
                    .flatten()
                    .and_then(|text| aura_core::IdentityId::from_db(&text).ok());
                let crop = CropRect {
                    x: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                    y: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                    w: row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                    h: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                }
                .expanded(0.20);
                out.push(FaceExpression {
                    face_id,
                    identity,
                    smile: unmilli(row.get(6).unwrap_or(0)),
                    genuineness: unmilli(row.get(7).unwrap_or(0)),
                    laughter: unmilli(row.get(8).unwrap_or(0)),
                    tears: unmilli(row.get(9).unwrap_or(0)),
                    surprise: unmilli(row.get(10).unwrap_or(0)),
                    tenderness: unmilli(row.get(11).unwrap_or(0)),
                    composed: unmilli(row.get(12).unwrap_or(0)),
                    discomfort: unmilli(row.get(13).unwrap_or(0)),
                    gaze: GazeTarget::from_str_or_unknown(
                        &row.get::<_, String>(14).unwrap_or_default(),
                    ),
                    confidence: unmilli(row.get(15).unwrap_or(0)),
                    crop,
                });
            }
            Ok(out)
        })
    }

    /// Store one moment's peak, preserving a photographer's choice.
    ///
    /// The choice is re-applied **inside** the upsert; see this module's header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_peak(&self, peak: &MomentPeak) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let moment_key = peak.moment_id.to_db();
        let photo_key = peak.image_id.to_db();
        let reasons = encode_reasons(&peak.reasons);
        let kind = peak.kind.as_str().to_string();
        let index = i64::from(peak.index);
        let frames = i64::from(peak.frames);
        let margin = f64::from(peak.margin);
        let confidence = f64::from(peak.confidence);

        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO moment_peak (
                     moment_id, photo_id, peak_index, frames, margin, kind,
                     confidence, reasons, user_chosen,
                     model_ver, analysis_ver, weights_ver, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, 0, 0, ?9, ?9)
                 ON CONFLICT(moment_id) DO UPDATE SET
                     photo_id   = CASE WHEN moment_peak.user_chosen = 1
                                       THEN moment_peak.photo_id ELSE excluded.photo_id END,
                     peak_index = CASE WHEN moment_peak.user_chosen = 1
                                       THEN moment_peak.peak_index ELSE excluded.peak_index END,
                     frames     = excluded.frames,
                     margin     = excluded.margin,
                     kind       = CASE WHEN moment_peak.user_chosen = 1
                                       THEN moment_peak.kind ELSE excluded.kind END,
                     confidence = excluded.confidence,
                     reasons    = excluded.reasons,
                     updated_at = excluded.updated_at",
                params![
                    moment_key, photo_key, index, frames, margin, kind, confidence, reasons, now,
                ],
            )
            .map_err(|e| statement_failed("could not store a moment peak", &e))?;
            Ok(())
        })
    }

    /// One moment's peak.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn peak_of(&self, moment: MomentId) -> AuraResult<Option<MomentPeak>> {
        let key = moment.to_db();
        self.catalog.read(move |conn| {
            let found = conn.query_row(
                "SELECT photo_id, peak_index, frames, margin, kind, confidence, reasons,
                        user_chosen
                   FROM moment_peak WHERE moment_id = ?1",
                params![key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, f64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            );
            match found {
                Ok(row) => {
                    let Ok(image_id) = PhotoId::from_db(&row.0) else {
                        return Ok(None);
                    };
                    Ok(Some(MomentPeak {
                        moment_id: moment,
                        image_id,
                        index: u32::try_from(row.1).unwrap_or(0),
                        frames: u32::try_from(row.2).unwrap_or(0),
                        margin: row.3 as f32,
                        kind: PeakKind::from_str_or_expression(&row.4),
                        confidence: row.5 as f32,
                        reasons: decode_reasons(&row.6),
                        user_chosen: row.7 != 0,
                    }))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed("could not read a moment peak", &err)),
            }
        })
    }

    /// Record that the photographer chose a moment's peak frame.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5041` when the moment is unknown or the photograph is not in it.
    pub fn set_peak(&self, moment: MomentId, image: ImageId) -> Result<(), AuraError> {
        let moment_key = moment.to_db();
        let photo_key = image.to_db();
        let members = self.moment_frames(moment)?;
        let Some(index) = members.iter().position(|candidate| *candidate == image) else {
            return Err(errors::preference_refused(
                "set_peak",
                "that photograph is not one of that moment's frames",
            ));
        };
        let index = i64::try_from(index).unwrap_or(0);
        let frames = i64::try_from(members.len()).unwrap_or(0);
        let now = aura_catalog::rfc3339(self.clock.now_utc());

        let changed = self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO moment_peak (
                     moment_id, photo_id, peak_index, frames, margin, kind, confidence,
                     reasons, user_chosen, model_ver, analysis_ver, weights_ver,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1.0, 'expression', 1.0, '[]', 1, 0, 0, 0, ?5, ?5)
                 ON CONFLICT(moment_id) DO UPDATE SET
                     photo_id    = excluded.photo_id,
                     peak_index  = excluded.peak_index,
                     frames      = excluded.frames,
                     confidence  = 1.0,
                     user_chosen = 1,
                     updated_at  = excluded.updated_at",
                params![moment_key, photo_key, index, frames, now],
            )
            .map_err(|e| statement_failed("could not record a peak choice", &e))
        })?;
        if changed == 0 {
            return Err(errors::preference_refused(
                "set_peak",
                "that moment is not in this catalog",
            ));
        }
        Ok(())
    }

    /// The frames of one moment, in timeline order.
    ///
    /// Read back from the grouping `MomentService` already made, as opaque ids. This
    /// computes no cadence and builds no graph; see this module's header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn moment_frames(&self, moment: MomentId) -> AuraResult<Vec<ImageId>> {
        let key = moment.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT mi.photo_id
                       FROM moment_images mi
                       JOIN photo p ON p.photo_id = mi.photo_id
                      WHERE mi.moment_id = ?1
                      ORDER BY p.timeline_time, mi.photo_id",
                )
                .map_err(|e| statement_failed("could not read a moment's frames", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read a moment's frames", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a moment's frames", &e))?
            {
                let text: String = row.get(0).map_err(|e| statement_failed("photo id", &e))?;
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Every moment in a project, with its frames in timeline order.
    ///
    /// One query for the whole project rather than one per moment: a wedding has eight
    /// hundred moments and eight hundred round trips is a second of latency for data one
    /// scan produces.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn moments_of(&self, project: &ProjectId) -> AuraResult<Vec<(MomentId, Vec<ImageId>)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT m.id, mi.photo_id
                       FROM moments m
                       JOIN moment_images mi ON mi.moment_id = m.id
                       JOIN photo p ON p.photo_id = mi.photo_id
                      WHERE m.project_id = ?1
                      ORDER BY m.start_ts, m.id, p.timeline_time, mi.photo_id",
                )
                .map_err(|e| statement_failed("could not read the project's moments", &e))?;
            let mut out: Vec<(MomentId, Vec<ImageId>)> = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the project's moments", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the project's moments", &e))?
            {
                let moment_text: String = row.get(0).map_err(|e| statement_failed("id", &e))?;
                let photo_text: String = row.get(1).map_err(|e| statement_failed("id", &e))?;
                let (Ok(moment), Ok(photo)) = (
                    MomentId::from_db(&moment_text),
                    PhotoId::from_db(&photo_text),
                ) else {
                    continue;
                };
                match out.last_mut() {
                    Some((current, frames)) if *current == moment => frames.push(photo),
                    _ => out.push((moment, vec![photo])),
                }
            }
            Ok(out)
        })
    }

    /// Replace this project's reaction links.
    ///
    /// Wholesale rather than incrementally, for the reason the face rows are replaced
    /// wholesale: a link whose action frame no longer scores above the floor must not
    /// survive as an orphan claiming somebody reacted to nothing.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_links(
        &self,
        project: &ProjectId,
        links: &[ReactionLink],
        analysis_ver: u16,
    ) -> AuraResult<usize> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let analysis_ver = i64::from(analysis_ver);
        let rows: Vec<(String, String, i64, f64, f64, String)> = links
            .iter()
            .map(|link| {
                (
                    link.action.to_db(),
                    link.reaction.to_db(),
                    link.gap_ms,
                    f64::from(link.bonus),
                    f64::from(link.confidence),
                    encode_reasons(&link.reasons),
                )
            })
            .collect();
        let pointers: Vec<(String, String)> = links
            .iter()
            .map(|link| (link.reaction.to_db(), link.action.to_db()))
            .collect();

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("could not open the reaction transaction", &e))?;
            tx.execute(
                "DELETE FROM reaction_links
                  WHERE action_id IN (SELECT photo_id FROM photo WHERE project_id = ?1)",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear the reaction links", &e))?;
            tx.execute(
                "UPDATE image_interaction SET reaction_of = NULL WHERE project_id = ?1",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear the reaction pointers", &e))?;

            let mut written = 0usize;
            for row in &rows {
                tx.execute(
                    "INSERT OR REPLACE INTO reaction_links (
                         action_id, reaction_id, gap_ms, bonus, confidence, reasons,
                         analysis_ver, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![row.0, row.1, row.2, row.3, row.4, row.5, analysis_ver, now],
                )
                .map_err(|e| statement_failed("could not store a reaction link", &e))?;
                written += 1;
            }
            for (reaction, action) in &pointers {
                tx.execute(
                    "UPDATE image_interaction SET reaction_of = ?2 WHERE photo_id = ?1",
                    params![reaction, action],
                )
                .map_err(|e| statement_failed("could not store a reaction pointer", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("could not commit the reaction links", &e))?;
            Ok(written)
        })
    }

    /// Every frame that reacts to this one, earliest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn reactions_to(&self, action: ImageId) -> AuraResult<Vec<ReactionLink>> {
        let key = action.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT reaction_id, gap_ms, bonus, confidence, reasons
                       FROM reaction_links WHERE action_id = ?1
                      ORDER BY gap_ms, reaction_id",
                )
                .map_err(|e| statement_failed("could not read reaction links", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read reaction links", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read reaction links", &e))?
            {
                let text: String = row.get(0).map_err(|e| statement_failed("photo id", &e))?;
                let Ok(reaction) = PhotoId::from_db(&text) else {
                    continue;
                };
                out.push(ReactionLink {
                    action,
                    reaction,
                    gap_ms: row.get(1).unwrap_or(0),
                    bonus: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                    confidence: row.get::<_, f64>(3).unwrap_or(0.0) as f32,
                    reasons: decode_reasons(&row.get::<_, String>(4).unwrap_or_default()),
                });
            }
            Ok(out)
        })
    }

    /// A project's frames ordered by emotion score, strongest first.
    ///
    /// The one query `idx_emotion_rank` exists for.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn ranked(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<(ImageId, f32)>> {
        let key = project.to_db();
        let limit = i64::try_from(limit.clamp(1, 20_000)).unwrap_or(200);
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, emotion_score
                       FROM image_interaction
                      WHERE project_id = ?1
                      ORDER BY emotion_score DESC, photo_id
                      LIMIT ?2",
                )
                .map_err(|e| statement_failed("could not read the ranking", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key, limit])
                .map_err(|e| statement_failed("could not read the ranking", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the ranking", &e))?
            {
                let text: String = row.get(0).map_err(|e| statement_failed("photo id", &e))?;
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push((id, row.get::<_, f64>(1).unwrap_or(0.0) as f32));
                }
            }
            Ok(out)
        })
    }

    /// Record one pairwise preference.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5041` when either photograph is unknown, when they are the same
    /// photograph, or when they belong to different projects.
    pub fn record_preference(&self, choice: Preference) -> Result<(), AuraError> {
        if choice.winner == choice.loser {
            return Err(errors::preference_refused(
                "prefer",
                "a photograph cannot be preferred to itself",
            ));
        }
        let winner = choice.winner.to_db();
        let loser = choice.loser.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let (winner_check, loser_check) = (winner.clone(), loser.clone());

        let same_project: Option<bool> = self.catalog.read(move |conn| {
            let read = |id: &str| -> Option<String> {
                conn.query_row(
                    "SELECT project_id FROM photo WHERE photo_id = ?1",
                    params![id],
                    |row| row.get::<_, String>(0),
                )
                .ok()
            };
            Ok(match (read(&winner_check), read(&loser_check)) {
                (Some(left), Some(right)) => Some(left == right),
                _ => None,
            })
        })?;
        match same_project {
            None => {
                return Err(errors::preference_refused(
                    "prefer",
                    "one of those photographs is not in this catalog",
                ))
            }
            Some(false) => {
                return Err(errors::preference_refused(
                    "prefer",
                    "those two photographs are in different weddings; a comparison across two \
                     weddings is not a comparison",
                ))
            }
            Some(true) => {}
        }

        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO emotion_preferences (winner_id, loser_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![winner, loser, now],
            )
            .map_err(|e| statement_failed("could not record a preference", &e))?;
            Ok(())
        })
    }

    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    #[allow(clippy::too_many_lines)]
    pub fn outline(&self, project: ProjectId) -> AuraResult<EmotionOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut outline = EmotionOutline::default();

            let coverage = conn.query_row(
                "SELECT photos, scored, face_aware, COALESCE(mean_score, 0.0),
                        COALESCE(model_ver, 0), COALESCE(analysis_ver, 0), COALESCE(weights_ver, 0)
                   FROM v_emotion_coverage WHERE project_id = ?1",
                params![key],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            );
            if let Ok(row) = coverage {
                outline.photos = u32::try_from(row.0).unwrap_or(0);
                outline.scored = u32::try_from(row.1).unwrap_or(0);
                outline.coverage = fraction(row.1, row.0);
                outline.face_aware = fraction(row.2, row.1);
                outline.mean_score = row.3 as f32;
                outline.model_ver = u16::try_from(row.4).unwrap_or(0);
                outline.analysis_ver = u16::try_from(row.5).unwrap_or(0);
                outline.weights_ver = u16::try_from(row.6).unwrap_or(0);
            }

            let histogram = conn.query_row(
                "SELECT hug, kiss, hand_hold, dance_hold, ring_exchange, blessing, toast,
                        tears_wiped, group_cheer
                   FROM v_emotion_interactions WHERE project_id = ?1",
                params![key],
                |row| {
                    let mut out = [0u32; Interaction::COUNT];
                    for (slot, value) in out.iter_mut().enumerate() {
                        *value = u32::try_from(row.get::<_, i64>(slot).unwrap_or(0)).unwrap_or(0);
                    }
                    Ok(out)
                },
            );
            if let Ok(values) = histogram {
                outline.interaction_histogram = values;
            }

            let peaks = conn.query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN mp.margin >= ?2 AND mp.kind <> 'flat' THEN 1 ELSE 0 END),
                        COALESCE(AVG(CASE WHEN mp.margin >= ?2 THEN mp.margin END), 0.0)
                   FROM moments m
                   LEFT JOIN moment_peak mp ON mp.moment_id = m.id
                  WHERE m.project_id = ?1",
                params![key, f64::from(MomentPeak::MIN_MARGIN)],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        row.get::<_, f64>(2)?,
                    ))
                },
            );
            if let Ok(row) = peaks {
                outline.moments = u32::try_from(row.0).unwrap_or(0);
                outline.peaked = u32::try_from(row.1).unwrap_or(0);
                outline.mean_margin = row.2 as f32;
            }

            outline.links = conn
                .query_row(
                    "SELECT COUNT(*) FROM reaction_links r
                       JOIN photo p ON p.photo_id = r.action_id
                      WHERE p.project_id = ?1",
                    params![key],
                    |row| row.get::<_, i64>(0),
                )
                .map_or(0, |count| u32::try_from(count).unwrap_or(0));

            outline.preferences = conn
                .query_row(
                    "SELECT COUNT(*) FROM emotion_preferences e
                       JOIN photo p ON p.photo_id = e.winner_id
                      WHERE p.project_id = ?1",
                    params![key],
                    |row| row.get::<_, i64>(0),
                )
                .map_or(0, |count| u32::try_from(count).unwrap_or(0));

            Ok(outline)
        })
    }

    /// The distinct version tuples present, with how many rows carry each.
    ///
    /// Lowest first, so a caller reporting "the oldest thing in this project" takes the
    /// head. The same shape `MomentStore::versions` has.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn versions(&self, project: &ProjectId) -> AuraResult<Vec<(u16, u16, u16, usize)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT model_ver, analysis_ver, weights_ver, COUNT(*)
                       FROM image_interaction WHERE project_id = ?1
                      GROUP BY model_ver, analysis_ver, weights_ver
                      ORDER BY model_ver, analysis_ver, weights_ver",
                )
                .map_err(|e| statement_failed("could not read the emotion versions", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read the emotion versions", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the emotion versions", &e))?
            {
                out.push((
                    u16::try_from(row.get::<_, i64>(0).unwrap_or(0)).unwrap_or(0),
                    u16::try_from(row.get::<_, i64>(1).unwrap_or(0)).unwrap_or(0),
                    u16::try_from(row.get::<_, i64>(2).unwrap_or(0)).unwrap_or(0),
                    usize::try_from(row.get::<_, i64>(3).unwrap_or(0)).unwrap_or(0),
                ));
            }
            Ok(out)
        })
    }

    /// Set a frame's peak proximity after the moment curves are built.
    ///
    /// A second pass over the project rather than a field written during the per-frame
    /// walk, for `IntegrityStore::refresh_relative_sharpness`'s reason: a frame's place on
    /// its moment's curve depends on its siblings, and half a moment's siblings have not
    /// been scored yet while the pass is running.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn set_proximities(&self, values: &BTreeMap<ImageId, f32>) -> AuraResult<usize> {
        let rows: Vec<(String, f64)> = values
            .iter()
            .map(|(image, proximity)| (image.to_db(), f64::from(*proximity)))
            .collect();
        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("could not open the proximity transaction", &e))?;
            let mut written = 0usize;
            for (photo, proximity) in &rows {
                written += tx
                    .execute(
                        "UPDATE image_interaction SET peak_proximity = ?2 WHERE photo_id = ?1",
                        params![photo, proximity],
                    )
                    .map_err(|e| statement_failed("could not store a peak proximity", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("could not commit the proximities", &e))?;
            Ok(written)
        })
    }
}

/// A count as a fraction, guarding the zero case.
///
/// Per-mille integer arithmetic then one lossless widening, the same shape every status
/// in this workspace uses.
fn fraction(part: i64, whole: i64) -> f32 {
    if whole <= 0 {
        return 0.0;
    }
    let per_mille = (part.saturating_mul(1_000) / whole).clamp(0, 1_000);
    f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
}
