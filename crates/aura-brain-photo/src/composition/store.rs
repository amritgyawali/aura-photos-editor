//! The one table migration 11 adds, and the three rules that live in the SQL rather than
//! around it.
//!
//! ## `user_reviewed` is checked inside the statement, not before it
//!
//! A re-analysis overwrites every judgement it recomputes. That upsert re-applies the
//! photographer's dismissals from the row it is replacing, in the same statement, so a
//! dismissal cannot be lost by a background pass that read the flag a moment earlier and
//! wrote a moment later. Phases 06, 07, 08 and 09 wrote this rule for identities,
//! segments, moments and technical verdicts; this is the fifth time and the window it
//! closes is the same one every time.
//!
//! Re-*application* rather than exclusion, as phase 09 chose and for phase 09's reason: a
//! dismissed framing note is not a replacement judgement. The frame still has to be
//! re-measured when the rule table moves, and the disagreement is carried onto the new
//! measurement rather than freezing the old one.
//!
//! ## Three JSON columns, and why they are not eleven
//!
//! `violations`, `reasons` and `crop_hint`. Section 11 budgets 800 bytes per image, a
//! fifth tighter than phase 09's, and the cuts, the intrusions and the bright blobs are
//! always read together by the overlay that draws them - so they share one column, and
//! the arrays that are empty are omitted from the object entirely. A frame with clean
//! framing stores `{}`.
//!
//! ## Why the store reads `moment_images` directly
//!
//! [`CompositionStore::relative_composition`] needs to know which frames were shot
//! together, and phase 08's rule is that `MomentService` is the only way to ask what was
//! shot once. This does not ask that: it reads back the grouping `MomentService` already
//! made, as opaque ids, to answer "of the frames in this group, where does this one rank".
//! It computes no cadence, builds no graph and cannot produce a grouping of its own. The
//! same argument phase 09's store makes, and ADR-0023 section 6 records it again because
//! the alternative - four thousand round trips through the frozen service - is the cost
//! phase 08's own exit report names.

use std::sync::Arc;

mod codec;
#[cfg(test)]
mod tests;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::composition::{
    CompositionFlags, CompositionOutline, CompositionResult, CropHint, HorizonSource, ImageId,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, MomentId, PhotoId, ProjectId, SceneId};
use rusqlite::{params, OptionalExtension};

use crate::errors;

use self::codec::{
    decode_json, decode_reasons, encode_hint, encode_reasons, encode_violations, visible_reasons,
    OwnedRow, StoredHint, StoredViolations,
};

/// Bytes one photograph costs in `image_composition` and its index.
///
/// Section 11 budgets **800 B per image**, which is tighter than phase 09's kilobyte while
/// carrying a comparable amount of evidence: up to six reasons with rectangles, up to
/// seven cuts with rectangles, four bright blobs, two intrusions and a crop hint.
///
/// Three decisions pay for it, and all three are phase 09's lessons applied a phase later:
/// a reason stores its code rather than its sentence, the three evidence arrays share one
/// column with empty arrays omitted, and there is one index rather than three.
///
/// The figure is asserted by `crates/aura-perf/tests/composition_budgets.rs` against a real
/// catalog rather than against this constant.
pub const BYTES_PER_IMAGE: usize = 800;

/// What the store knows that the analyser does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Provenance {
    /// True when this frame's scene had no rule row.
    pub unruled: bool,
}

/// The `image_composition` table.
#[derive(Debug)]
pub struct CompositionStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl CompositionStore {
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

    /// Photographs with no judgement at these versions.
    ///
    /// **The work remaining is a query, not a journal.** Invariant 5: kill the process at
    /// 10 %, 50 % or 90 % and the next run asks the catalog what is left. A `rules_ver`
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
        rules_ver: u16,
    ) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN image_composition c ON c.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (c.photo_id IS NULL
                             OR c.model_ver <> ?2
                             OR c.analysis_ver <> ?3
                             OR c.rules_ver <> ?4)
                      ORDER BY p.photo_id",
                )
                .map_err(|e| statement_failed("could not read the pending set", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![
                    key,
                    i64::from(model_ver),
                    i64::from(analysis_ver),
                    i64::from(rules_ver)
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

    /// Store one judgement, carrying forward whatever the photographer dismissed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    #[allow(clippy::too_many_lines)]
    pub fn put(
        &self,
        project: &ProjectId,
        result: &CompositionResult,
        provenance: &Provenance,
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let project_key = project.to_db();
        let photo_key = result.image_id.to_db();
        let source_reasons = result.reasons.clone();
        let violations = encode_violations(result)?;
        let hint = result.crop_suggestion_hint.map(encode_hint).transpose()?;
        let mut row = OwnedRow::from((result, provenance));

        self.catalog.writer().transact(move |conn| {
            let dismissed_raw = conn
                .query_row(
                    "SELECT dismissed FROM image_composition WHERE photo_id = ?1",
                    params![photo_key],
                    |stored| stored.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| {
                    statement_failed("could not read existing composition review state", &e)
                })?
                .unwrap_or(0)
                .max(0);
            let dismissed =
                CompositionFlags::from_bits(u32::try_from(dismissed_raw).map_err(|error| {
                    statement_failed(
                        "stored composition dismissals are outside their wire range",
                        &error,
                    )
                })?);
            let incoming_flags = u32::try_from(row.flags).map_err(|error| {
                statement_failed(
                    "computed composition flags are outside their wire range",
                    &error,
                )
            })?;
            row.flags = i64::from(
                CompositionFlags::from_bits(incoming_flags)
                    .without(dismissed)
                    .bits(),
            );
            let visible_reasons = visible_reasons(source_reasons, dismissed);
            let reasons = encode_reasons(&visible_reasons)?;

            // `dismissed` and `user_reviewed` come from the row being replaced, not from
            // the judgement being written. `excluded` is the incoming row; the bare column
            // name is the existing one. A freshly computed flag set has the dismissed bits
            // cleared out of it in the same statement that stores it, so there is no
            // window in which the photographer's decision is not in force.
            conn.execute(
                "INSERT INTO image_composition (
                     photo_id, project_id,
                     tilt_deg, tilt_intentional, horizon_conf, horizon_source,
                     headroom, thirds_offset, balance, negative_space,
                     head_crop, clutter, head_merge, colour_competition,
                     aesthetic, composition_score, relative_composition, keypoint_subjects,
                     flags, violations, reasons, crop_hint, confidence,
                     scene, unruled,
                     user_reviewed, dismissed,
                     model_ver, analysis_ver, rules_ver,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, 0, 0,
                         ?26, ?27, ?28, ?29, ?29)
                 ON CONFLICT(photo_id) DO UPDATE SET
                     tilt_deg             = excluded.tilt_deg,
                     tilt_intentional     = excluded.tilt_intentional,
                     horizon_conf         = excluded.horizon_conf,
                     horizon_source       = excluded.horizon_source,
                     headroom             = excluded.headroom,
                     thirds_offset        = excluded.thirds_offset,
                     balance              = excluded.balance,
                     negative_space       = excluded.negative_space,
                     head_crop            = excluded.head_crop,
                     clutter              = excluded.clutter,
                     head_merge           = excluded.head_merge,
                     colour_competition   = excluded.colour_competition,
                     aesthetic            = excluded.aesthetic,
                     composition_score    = excluded.composition_score,
                     relative_composition = excluded.relative_composition,
                     keypoint_subjects    = excluded.keypoint_subjects,
                     flags                = excluded.flags & ~image_composition.dismissed,
                     violations           = excluded.violations,
                     reasons              = excluded.reasons,
                     crop_hint            = excluded.crop_hint,
                     confidence           = excluded.confidence,
                     scene                = excluded.scene,
                     unruled              = excluded.unruled,
                     model_ver            = excluded.model_ver,
                     analysis_ver         = excluded.analysis_ver,
                     rules_ver            = excluded.rules_ver,
                     updated_at           = excluded.updated_at",
                params![
                    photo_key,
                    project_key,
                    row.tilt_deg,
                    row.tilt_intentional,
                    row.horizon_conf,
                    row.horizon_source,
                    row.headroom,
                    row.thirds_offset,
                    row.balance,
                    row.negative_space,
                    row.head_crop,
                    row.clutter,
                    row.head_merge,
                    row.colour_competition,
                    row.aesthetic,
                    row.composition_score,
                    row.relative_composition,
                    row.keypoint_subjects,
                    row.flags,
                    violations,
                    reasons,
                    hint,
                    row.confidence,
                    row.scene,
                    row.unruled,
                    row.model_ver,
                    row.analysis_ver,
                    row.rules_ver,
                    now,
                ],
            )
            .map_err(|e| statement_failed("could not store a framing judgement", &e))?;
            Ok(())
        })
    }

    /// One photograph's judgement.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn get(&self, image: ImageId) -> AuraResult<Option<CompositionResult>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT tilt_deg, tilt_intentional, horizon_conf, horizon_source,
                            headroom, thirds_offset, balance, negative_space,
                            head_crop, clutter, head_merge, colour_competition,
                            aesthetic, composition_score, relative_composition,
                            keypoint_subjects, flags, violations, reasons, crop_hint,
                            confidence, scene, user_reviewed,
                            model_ver, analysis_ver, rules_ver
                       FROM image_composition WHERE photo_id = ?1",
                )
                .map_err(|e| statement_failed("could not read a framing judgement", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read a framing judgement", &e))?;
            let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a framing judgement", &e))?
            else {
                return Ok(None);
            };

            let violations_text = row.get::<_, String>(17).map_err(|e| {
                statement_failed("could not read stored composition violations", &e)
            })?;
            let violations: StoredViolations =
                decode_json("composition violations", &violations_text)?;
            let reasons_text = row
                .get::<_, String>(18)
                .map_err(|e| statement_failed("could not read stored composition reasons", &e))?;
            let reasons = decode_reasons(&reasons_text)?;
            let hint_text = row
                .get::<_, Option<String>>(19)
                .map_err(|e| statement_failed("could not read a stored crop hint", &e))?;
            let hint = hint_text
                .map(|text| decode_json::<StoredHint>("composition crop hint", &text))
                .transpose()?
                .map(StoredHint::into_hint);

            Ok(Some(CompositionResult {
                image_id: image,
                tilt_deg: row.get::<_, f64>(0).unwrap_or(0.0) as f32,
                tilt_intentional: row.get::<_, i64>(1).unwrap_or(0) != 0,
                horizon_conf: row.get::<_, f64>(2).unwrap_or(0.0) as f32,
                horizon_source: HorizonSource::from_str_or_none(
                    &row.get::<_, String>(3).unwrap_or_default(),
                ),
                headroom: row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                thirds_offset: row.get::<_, f64>(5).unwrap_or(0.0) as f32,
                balance: row.get::<_, f64>(6).unwrap_or(1.0) as f32,
                negative_space: row.get::<_, f64>(7).unwrap_or(0.0) as f32,
                joint_cuts: violations.cuts(),
                head_crop: row.get::<_, i64>(8).unwrap_or(0) != 0,
                edge_intrusions: violations.intrusions(),
                clutter: row.get::<_, f64>(9).unwrap_or(0.0) as f32,
                bright_blobs: violations.blobs(),
                head_merge: row.get::<_, i64>(10).unwrap_or(0) != 0,
                colour_competition: row.get::<_, f64>(11).unwrap_or(0.0) as f32,
                aesthetic: row.get::<_, f64>(12).unwrap_or(0.5) as f32,
                composition_score: row.get::<_, f64>(13).unwrap_or(0.0) as f32,
                crop_suggestion_hint: hint,
                scene: SceneId::from_str_or_unknown(&row.get::<_, String>(21).unwrap_or_default()),
                relative_composition: row.get::<_, f64>(14).unwrap_or(0.5) as f32,
                keypoint_subjects: u32::try_from(row.get::<_, i64>(15).unwrap_or(0).max(0))
                    .unwrap_or(0),
                flags: CompositionFlags::from_bits(
                    u32::try_from(row.get::<_, i64>(16).unwrap_or(0).max(0)).unwrap_or(0),
                ),
                reasons,
                confidence: row.get::<_, f64>(20).unwrap_or(0.0) as f32,
                user_reviewed: row.get::<_, i64>(22).unwrap_or(0) != 0,
                model_ver: u16::try_from(row.get::<_, i64>(23).unwrap_or(0).max(0)).unwrap_or(0),
                analysis_ver: u16::try_from(row.get::<_, i64>(24).unwrap_or(0).max(0)).unwrap_or(0),
                rules_ver: u16::try_from(row.get::<_, i64>(25).unwrap_or(0).max(0)).unwrap_or(0),
            }))
        })
    }

    /// One photograph's crop hint and nothing else.
    ///
    /// Phase 23 wants this and not the rest, and reading a whole judgement to take one
    /// field out of it is how a crop pass ends up decoding reasons it never renders.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn crop_hint(&self, image: ImageId) -> AuraResult<Option<CropHint>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT crop_hint FROM image_composition WHERE photo_id = ?1")
                .map_err(|e| statement_failed("could not read a crop hint", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read a crop hint", &e))?;
            let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a crop hint", &e))?
            else {
                return Ok(None);
            };
            let hint = row
                .get::<_, Option<String>>(0)
                .map_err(|e| statement_failed("could not read a stored crop hint", &e))?;
            hint.map(|text| decode_json::<StoredHint>("composition crop hint", &text))
                .transpose()
                .map(|stored| stored.map(StoredHint::into_hint))
        })
    }

    /// The project's coverage, histogram, tilt telemetry and versions, in two queries.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when either read fails.
    pub fn outline(&self, project: ProjectId) -> AuraResult<CompositionOutline> {
        let key = project.to_db();
        let unruled_scenes = self.unruled_scenes(project)?;
        self.catalog.read(move |conn| {
            let mut outline = CompositionOutline {
                unruled_scenes,
                ..CompositionOutline::default()
            };

            let mut coverage = conn
                .prepare(
                    "SELECT photos, scored, keypoint_aware, reviewed, hinted, mean_score,
                            model_ver, analysis_ver, rules_ver
                       FROM v_composition_coverage WHERE project_id = ?1",
                )
                .map_err(|e| statement_failed("could not read composition coverage", &e))?;
            let mut cursor = coverage
                .query(params![key])
                .map_err(|e| statement_failed("could not read composition coverage", &e))?;
            if let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read composition coverage", &e))?
            {
                let photos = row.get::<_, i64>(0).unwrap_or(0).max(0);
                let scored = row.get::<_, i64>(1).unwrap_or(0).max(0);
                let aware = row.get::<_, i64>(2).unwrap_or(0).max(0);
                outline.photos = u32::try_from(photos).unwrap_or(u32::MAX);
                outline.scored = u32::try_from(scored).unwrap_or(u32::MAX);
                outline.coverage = ratio(scored, photos);
                outline.keypoint_aware = ratio(aware, scored);
                outline.reviewed =
                    u32::try_from(row.get::<_, i64>(3).unwrap_or(0).max(0)).unwrap_or(0);
                outline.hinted =
                    u32::try_from(row.get::<_, i64>(4).unwrap_or(0).max(0)).unwrap_or(0);
                outline.mean_score = row.get::<_, f64>(5).unwrap_or(0.0) as f32;
                outline.model_ver =
                    u16::try_from(row.get::<_, i64>(6).unwrap_or(0).max(0)).unwrap_or(0);
                outline.analysis_ver =
                    u16::try_from(row.get::<_, i64>(7).unwrap_or(0).max(0)).unwrap_or(0);
                outline.rules_ver =
                    u16::try_from(row.get::<_, i64>(8).unwrap_or(0).max(0)).unwrap_or(0);
            }

            let mut flags = conn
                .prepare(
                    "SELECT horizon_tilted, intentional_tilt, verticals_converging,
                            headroom_tight, headroom_excessive, head_cropped,
                            joint_cut, limb_cut, edge_intrusion, off_balance, centred,
                            cluttered_background, bright_blob, head_merge,
                            colour_competition, no_geometry,
                            mean_abs_tilt, tilted_intentional, tilted_any
                       FROM v_composition_flags WHERE project_id = ?1",
                )
                .map_err(|e| statement_failed("could not read the flag histogram", &e))?;
            let mut cursor = flags
                .query(params![key])
                .map_err(|e| statement_failed("could not read the flag histogram", &e))?;
            if let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read the flag histogram", &e))?
            {
                for index in 0..CompositionFlags::COUNT {
                    let value = row.get::<_, i64>(index).unwrap_or(0).max(0);
                    if let Some(slot) = outline.flag_histogram.get_mut(index) {
                        *slot = u32::try_from(value).unwrap_or(u32::MAX);
                    }
                }
                outline.mean_abs_tilt = row.get::<_, f64>(16).unwrap_or(0.0) as f32;
                let intentional = row.get::<_, i64>(17).unwrap_or(0).max(0);
                let tilted = row.get::<_, i64>(18).unwrap_or(0).max(0);
                outline.intentional_ratio = ratio(intentional, tilted);
            }

            Ok(outline)
        })
    }

    /// Scenes in this project that were judged on the neutral rule row.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn unruled_scenes(&self, project: ProjectId) -> AuraResult<Vec<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT DISTINCT scene FROM image_composition
                      WHERE project_id = ?1 AND unruled = 1
                      ORDER BY scene",
                )
                .map_err(|e| statement_failed("could not read unruled scenes", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read unruled scenes", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read unruled scenes", &e))?
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
        flags: CompositionFlags,
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
                    "SELECT photo_id FROM image_composition
                      WHERE project_id = ?1 AND (flags & ?2) <> 0
                      ORDER BY composition_score, photo_id
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

    /// One moment's frames ranked by composition, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the read fails.
    pub fn relative_composition(&self, moment: MomentId) -> AuraResult<Vec<(ImageId, f32)>> {
        let key = moment.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT mi.photo_id, c.composition_score
                       FROM moment_images mi
                       JOIN image_composition c ON c.photo_id = mi.photo_id
                      WHERE mi.moment_id = ?1
                      ORDER BY c.composition_score DESC, mi.photo_id",
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

    /// Write the within-moment ranks back onto the judgements.
    ///
    /// Run once at the end of a pass rather than per frame, because a frame's rank depends
    /// on its siblings and half a moment's siblings have not been judged yet while the
    /// pass is running.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn refresh_relative_composition(&self, project: &ProjectId) -> AuraResult<usize> {
        let key = project.to_db();
        self.catalog.writer().with(move |conn| {
            let changed = conn
                .execute(
                    "WITH ranked AS (
                         SELECT mi.photo_id AS photo_id,
                                RANK() OVER (PARTITION BY mi.moment_id
                                             ORDER BY c.composition_score DESC) - 1 AS position,
                                COUNT(*) OVER (PARTITION BY mi.moment_id) - 1 AS span
                           FROM moment_images mi
                           JOIN image_composition c ON c.photo_id = mi.photo_id
                          WHERE c.project_id = ?1
                     )
                     UPDATE image_composition
                        SET relative_composition = (
                            SELECT CASE WHEN r.span <= 0 THEN 0.5
                                        ELSE 1.0 - CAST(r.position AS REAL) / r.span END
                              FROM ranked r WHERE r.photo_id = image_composition.photo_id)
                      WHERE photo_id IN (SELECT photo_id FROM ranked)",
                    params![key],
                )
                .map_err(|e| statement_failed("could not refresh within-moment ranks", &e))?;
            Ok(changed)
        })
    }

    /// Record that the photographer disagrees with one flag.
    ///
    /// A dismissal changes the review projection, not the underlying measurement: the bit
    /// and its penalty reason disappear together, while scalar evidence and the original
    /// composite remain immutable and reproducible. Re-analysis reapplies the same review
    /// projection to fresh measurements inside [`CompositionStore::put`].
    ///
    /// # Errors
    ///
    /// `AURA-ML-5044` when the photograph has no judgement, when `flag` is not a single
    /// flag, or when it is not currently set.
    pub fn dismiss(&self, image: ImageId, flag: CompositionFlags) -> AuraResult<()> {
        if flag.count() != 1 {
            return Err(errors::composition_edit_refused(
                &image.to_db(),
                "a dismissal names exactly one flag; clearing two judgements in one statement \
                 would record one decision for two disagreements",
            ));
        }
        if !CompositionFlags::DEFECTS.contains(flag) {
            return Err(errors::composition_edit_refused(
                &image.to_db(),
                "that mark is not a fault, so there is nothing to dismiss",
            ));
        }
        let key = image.to_db();
        let bit = i64::from(flag.bits());
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |conn| {
            let stored = conn
                .query_row(
                    "SELECT flags, reasons FROM image_composition WHERE photo_id = ?1",
                    params![key],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(|e| {
                    statement_failed("could not read a framing mark for dismissal", &e)
                })?;
            let Some((stored_bits, reasons_text)) = stored else {
                return Err(errors::composition_edit_refused(
                    &image.to_db(),
                    "this photograph's framing has not been checked yet, so it carries no marks",
                ));
            };
            let stored_flags = CompositionFlags::from_bits(
                u32::try_from(stored_bits.max(0)).map_err(|e| {
                    statement_failed("stored composition flags are outside their wire range", &e)
                })?,
            );
            if !stored_flags.contains(flag) {
                return Err(errors::composition_edit_refused(
                    &image.to_db(),
                    "that mark is not on this photograph; the panel may be showing an older reading",
                ));
            }

            let visible_reasons = visible_reasons(decode_reasons(&reasons_text)?, flag);
            let reasons = encode_reasons(&visible_reasons)?;
            let changed = conn
                .execute(
                "UPDATE image_composition
                    SET flags = flags & ~?2,
                        dismissed = dismissed | ?2,
                        reasons = ?3,
                        user_reviewed = 1,
                        updated_at = ?4
                  WHERE photo_id = ?1 AND (flags & ?2) <> 0",
                    params![key, bit, reasons, now],
                )
                .map_err(|e| statement_failed("could not dismiss a framing mark", &e))?;
            if changed != 1 {
                return Err(errors::composition_edit_refused(
                    &image.to_db(),
                    "that framing mark changed while it was being reviewed; refresh the panel",
                ));
            }
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// Turn a rank order into `0..1`, best first.
///
/// One for the best-composed frame and zero for the worst, with `0.5` when the moment
/// holds a single frame - neither the best nor the worst of no siblings. Ties share the
/// higher value, because two frames that scored identically are equally good and an
/// arbitrary tiebreak would be an ordering the data does not support.
fn rank(rows: &[(PhotoId, f32)]) -> Vec<(ImageId, f32)> {
    if rows.len() <= 1 {
        return rows.iter().map(|(id, _)| (*id, 0.5)).collect();
    }
    let span = (rows.len() - 1) as f32;
    let mut rank_position = 0usize;
    let mut previous_score: Option<f32> = None;
    rows.iter()
        .enumerate()
        .map(|(position, (id, score))| {
            if previous_score
                .is_some_and(|previous| previous.total_cmp(score) != std::cmp::Ordering::Equal)
            {
                rank_position = position;
            }
            previous_score = Some(*score);
            (*id, 1.0 - rank_position as f32 / span)
        })
        .collect()
}

/// `numerator / denominator`, guarded.
fn ratio(numerator: i64, denominator: i64) -> f32 {
    if denominator <= 0 {
        return 0.0;
    }
    (numerator as f64 / denominator as f64) as f32
}
