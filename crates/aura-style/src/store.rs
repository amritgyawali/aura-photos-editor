//! Migration 17's four tables, and the statements that use them.
//!
//! The shape phases 09, 11, 15 and 16 all settled on - a `pending` query rather than a journal,
//! an outline built from a coverage view, a decoder that is total - with two differences that
//! are worth reading before the code.
//!
//! ## The resume unit is a pair, not a photograph
//!
//! Every phase since 09 resumes by asking "which photographs have no row at this version". This
//! one asks "which **pairs** have no fit at this version", because the expensive thing here is
//! not analysing a frame in the catalog but reproducing a frame in somebody's archive - about a
//! second and a half each, two thousand of them, twenty-five minutes. A photographer who cancels
//! at 60 % and restarts must re-fit none of what they already paid for.
//!
//! The key is the **original's content hash**, so a re-scan of an archive that was renamed
//! between runs also re-uses the fits. Invariant 5 across a rename as well as across a cancel.
//!
//! ## A profile is never updated in place, and never deleted
//!
//! Training writes a new row with the next version for that name; adopting it retires whatever
//! was adopted under the same name. Nothing here has an `UPDATE profiles SET` that touches a
//! delta, and there is no delete. A gallery delivered under version 3 has to stay reproducible
//! after version 4 exists - invariant 4 reaching further than one render - and migration 17's
//! partial unique index is what makes "at most one adopted version per name" a property of the
//! database rather than of the code that writes it.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::colour::{HslAdjustments, HslBand, HslShift};
use aura_core::contract::scene::ChapterId;
use aura_core::contract::style::{
    BucketDiagnostic, BucketModel, CurveShift, ExtractSource, FallbackLevel, MatchMethod,
    ProfileDiagnostics, ProfileStatus, SceneGroup, SkinBias, StyleBucket, StyleDelta, StyleOutline,
    StyleProfile,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, ProfileId, ProjectId};
use rusqlite::{params, OptionalExtension};

use crate::errors;
use crate::tree::Tree;

/// The chapter key that means "the project's default".
///
/// `'*'` rather than a null, because `SQLite` does not enforce uniqueness over a null inside
/// a primary key, and the whole point of the row is that there is one default per project.
pub const PROJECT_DEFAULT: &str = "*";

/// One pair, as the catalog stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct PairRow {
    /// BLAKE3 of the original's bytes. The key.
    pub original_hash: String,
    /// Which profile name this pair was scanned for.
    pub profile_name: String,
    /// The original's file name.
    pub original_name: String,
    /// The final's file name.
    pub final_name: String,
    /// How the two were matched.
    pub matched_by: MatchMethod,
    /// Where the parameters came from.
    pub extracted_from: ExtractSource,
    /// Which leaf it landed in.
    pub bucket: StyleBucket,
    /// The photographer's own numbers.
    pub params: crate::extract::TheirParams,
    /// The six features the regression conditions on.
    pub features: PairFeatures,
    /// What the fit could not explain.
    pub residual_de00: f32,
    /// True when the pair was used.
    pub accepted: bool,
    /// Why it was not, when it was not.
    pub rejection: Option<aura_core::contract::style::StyleCode>,
    /// True when it was held out of the fit.
    pub held_out: bool,
    /// Which wedding it came from.
    pub archive: String,
    /// Which build's fitter produced it.
    pub analysis_ver: u16,
}

/// The six numbers migration 17 stores on a pair.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PairFeatures {
    /// The subject's own luminance before the exposure moved.
    pub subject_luma: f32,
    /// The colour temperature phase 15 settled on.
    pub cct_k: f32,
    /// The frame's dynamic range in stops.
    pub dynamic_range_ev: f32,
    /// The ISO EXIF recorded.
    pub iso: u32,
    /// True when the flash fired.
    pub flash: bool,
    /// True when phase 15 found two illuminants disagreeing spatially.
    pub mixed_light: bool,
}

/// Migration 17's tables.
pub struct StyleStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for StyleStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StyleStore").finish_non_exhaustive()
    }
}

impl StyleStore {
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

    // -----------------------------------------------------------------
    // Pairs
    // -----------------------------------------------------------------

    /// Record one pair, replacing whatever was there for the same original.
    ///
    /// A rejected pair is stored exactly as an accepted one is. That is migration 17's note 3
    /// and it is the difference between a report that can say "158 left out" and one that can
    /// only ever say a hundred percent.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_pair(&self, row: &PairRow) -> AuraResult<()> {
        let now = epoch_ms(self.clock.now_utc());
        let row = row.clone();
        self.catalog.writer().transact(move |conn| {
            conn.execute(
                "INSERT INTO style_pairs (
                     original_hash, profile_name, original_name, final_name,
                     matched_by, extracted_from, bucket,
                     their_exposure, their_temperature_k, their_tint, their_contrast,
                     their_highlights, their_shadows, their_whites, their_blacks,
                     their_vibrance, their_saturation, their_curve, their_hsl,
                     subject_luma, cct_k, dynamic_range_ev, iso, flash, mixed_light,
                     residual_de00, accepted, rejection, held_out, archive,
                     analysis_ver, at_ts
                 ) VALUES (
                     ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                     ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                     ?20, ?21, ?22, ?23, ?24, ?25,
                     ?26, ?27, ?28, ?29, ?30, ?31, ?32
                 )
                 ON CONFLICT(profile_name, original_hash) DO UPDATE SET
                     original_name = excluded.original_name,
                     final_name = excluded.final_name,
                     matched_by = excluded.matched_by,
                     extracted_from = excluded.extracted_from,
                     bucket = excluded.bucket,
                     their_exposure = excluded.their_exposure,
                     their_temperature_k = excluded.their_temperature_k,
                     their_tint = excluded.their_tint,
                     their_contrast = excluded.their_contrast,
                     their_highlights = excluded.their_highlights,
                     their_shadows = excluded.their_shadows,
                     their_whites = excluded.their_whites,
                     their_blacks = excluded.their_blacks,
                     their_vibrance = excluded.their_vibrance,
                     their_saturation = excluded.their_saturation,
                     their_curve = excluded.their_curve,
                     their_hsl = excluded.their_hsl,
                     subject_luma = excluded.subject_luma,
                     cct_k = excluded.cct_k,
                     dynamic_range_ev = excluded.dynamic_range_ev,
                     iso = excluded.iso,
                     flash = excluded.flash,
                     mixed_light = excluded.mixed_light,
                     residual_de00 = excluded.residual_de00,
                     accepted = excluded.accepted,
                     rejection = excluded.rejection,
                     held_out = excluded.held_out,
                     archive = excluded.archive,
                     analysis_ver = excluded.analysis_ver,
                     at_ts = excluded.at_ts",
                params![
                    row.original_hash,
                    row.profile_name,
                    row.original_name,
                    row.final_name,
                    row.matched_by.as_str(),
                    row.extracted_from.as_str(),
                    row.bucket.as_key(),
                    f64::from(row.params.exposure),
                    f64::from(row.params.temperature_k),
                    f64::from(row.params.tint),
                    f64::from(row.params.contrast),
                    f64::from(row.params.highlights),
                    f64::from(row.params.shadows),
                    f64::from(row.params.whites),
                    f64::from(row.params.blacks),
                    f64::from(row.params.vibrance),
                    f64::from(row.params.saturation),
                    encode_curve(&row.params.curve),
                    encode_hsl(&row.params.hsl),
                    f64::from(row.features.subject_luma),
                    f64::from(row.features.cct_k),
                    f64::from(row.features.dynamic_range_ev),
                    i64::from(row.features.iso),
                    i64::from(row.features.flash),
                    i64::from(row.features.mixed_light),
                    f64::from(row.residual_de00),
                    i64::from(row.accepted),
                    row.rejection.map(|code| code.as_str().to_string()),
                    i64::from(row.held_out),
                    row.archive,
                    i64::from(row.analysis_ver),
                    now,
                ],
            )
            .map_err(|err| statement_failed("insert style_pairs", &err))?;
            Ok(())
        })
    }

    /// Which original hashes already have a fit at this version.
    ///
    /// Invariant 5, and the reason the resume unit is a pair. See this module's header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn fitted(&self, profile_name: &str, analysis_ver: u16) -> AuraResult<Vec<String>> {
        let name = profile_name.to_string();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT original_hash FROM style_pairs
                      WHERE profile_name = ?1 AND analysis_ver = ?2
                      ORDER BY original_hash",
                )
                .map_err(|err| statement_failed("prepare fitted", &err))?;
            let rows = statement
                .query_map(params![name, i64::from(analysis_ver)], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|err| statement_failed("query fitted", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("read fitted", &err))?);
            }
            Ok(out)
        })
    }

    /// Every accepted pair for one profile name, in hash order.
    ///
    /// Hash order rather than insertion order, so the deterministic held-out split
    /// (`crate::tree::HOLDOUT_EVERY`) picks the same pairs on every re-train. A split that
    /// depended on the order files were walked in would make the headline accuracy figure change
    /// when somebody renamed a folder.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pairs(&self, profile_name: &str, accepted_only: bool) -> AuraResult<Vec<PairRow>> {
        let name = profile_name.to_string();
        self.catalog.read(move |conn| {
            let sql = if accepted_only {
                "SELECT * FROM style_pairs WHERE profile_name = ?1 AND accepted = 1
                  ORDER BY original_hash"
            } else {
                "SELECT * FROM style_pairs WHERE profile_name = ?1 ORDER BY original_hash"
            };
            let mut statement = conn
                .prepare(sql)
                .map_err(|err| statement_failed("prepare pairs", &err))?;
            let rows = statement
                .query_map(params![name], |row| {
                    Ok(PairRow {
                        original_hash: row.get("original_hash")?,
                        profile_name: row.get("profile_name")?,
                        original_name: row.get("original_name")?,
                        final_name: row.get("final_name")?,
                        matched_by: MatchMethod::from_str_or_unmatched(
                            &row.get::<_, String>("matched_by")?,
                        ),
                        extracted_from: ExtractSource::from_str_or_none(
                            &row.get::<_, String>("extracted_from")?,
                        ),
                        bucket: StyleBucket::from_key(&row.get::<_, String>("bucket")?),
                        params: crate::extract::TheirParams {
                            exposure: row.get::<_, f64>("their_exposure")? as f32,
                            temperature_k: row.get::<_, f64>("their_temperature_k")? as f32,
                            tint: row.get::<_, f64>("their_tint")? as f32,
                            contrast: row.get::<_, f64>("their_contrast")? as f32,
                            highlights: row.get::<_, f64>("their_highlights")? as f32,
                            shadows: row.get::<_, f64>("their_shadows")? as f32,
                            whites: row.get::<_, f64>("their_whites")? as f32,
                            blacks: row.get::<_, f64>("their_blacks")? as f32,
                            vibrance: row.get::<_, f64>("their_vibrance")? as f32,
                            saturation: row.get::<_, f64>("their_saturation")? as f32,
                            curve: decode_curve(&row.get::<_, String>("their_curve")?),
                            hsl: decode_hsl(&row.get::<_, String>("their_hsl")?),
                        },
                        features: PairFeatures {
                            subject_luma: row.get::<_, f64>("subject_luma")? as f32,
                            cct_k: row.get::<_, f64>("cct_k")? as f32,
                            dynamic_range_ev: row.get::<_, f64>("dynamic_range_ev")? as f32,
                            iso: row.get::<_, i64>("iso")?.max(0) as u32,
                            flash: row.get::<_, i64>("flash")? != 0,
                            mixed_light: row.get::<_, i64>("mixed_light")? != 0,
                        },
                        residual_de00: row.get::<_, f64>("residual_de00")? as f32,
                        accepted: row.get::<_, i64>("accepted")? != 0,
                        rejection: row.get::<_, Option<String>>("rejection")?.map(|text| {
                            aura_core::contract::style::StyleCode::from_str_or_none(&text)
                        }),
                        held_out: row.get::<_, i64>("held_out")? != 0,
                        archive: row.get("archive")?,
                        analysis_ver: row.get::<_, i64>("analysis_ver")?.max(0) as u16,
                    })
                })
                .map_err(|err| statement_failed("query pairs", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("read pairs", &err))?);
            }
            Ok(out)
        })
    }

    // -----------------------------------------------------------------
    // Profiles
    // -----------------------------------------------------------------

    /// Write one profile and its buckets. Never an update - see this module's header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the write fails.
    pub fn put_profile(&self, profile: &StyleProfile, tree: &Tree) -> AuraResult<()> {
        let now = epoch_ms(self.clock.now_utc());
        let stamp = aura_catalog::rfc3339(self.clock.now_utc());
        // A profile that never recorded when it was trained is stamped now rather than in 1970:
        // `trained_at_ts` is what the profile list is ordered by, and an epoch zero row would
        // sort below every other profile forever.
        let trained_at = if profile.trained_at == 0 {
            now
        } else {
            profile.trained_at
        };
        let profile = profile.clone();
        let groups = tree.groups.clone();
        self.catalog.writer().transact(move |conn| {
            let global = &profile.global;
            conn.execute(
                "INSERT OR REPLACE INTO profiles (
                     profile_id, name, version, status,
                     exposure, temperature_k, tint, contrast, highlights, shadows,
                     whites, blacks, vibrance, saturation, curve_shift, hsl,
                     skin_warmth_deg, skin_chroma, confidence,
                     trained_pairs, accepted_pairs, rejected_pairs, overall_de00,
                     weak_buckets, recommendation,
                     engine_ver, analysis_ver, trained_at_ts, trained_at
                 ) VALUES (
                     ?1, ?2, ?3, ?4,
                     ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19,
                     ?20, ?21, ?22, ?23, ?24, ?25,
                     ?26, ?27, ?28, ?29
                 )",
                params![
                    profile.id.to_db(),
                    profile.name,
                    i64::from(profile.version),
                    profile.status.as_str(),
                    f64::from(global.exposure),
                    f64::from(global.temperature_k),
                    f64::from(global.tint),
                    f64::from(global.contrast),
                    f64::from(global.highlights),
                    f64::from(global.shadows),
                    f64::from(global.whites),
                    f64::from(global.blacks),
                    f64::from(global.vibrance),
                    f64::from(global.saturation),
                    encode_shift(&global.curve_shift),
                    encode_hsl(&global.hsl),
                    f64::from(global.skin_bias.warmth_deg),
                    f64::from(global.skin_bias.chroma),
                    f64::from(global.confidence),
                    i64::from(profile.trained_pairs),
                    i64::from(profile.diagnostics.accepted_pairs),
                    i64::from(profile.diagnostics.rejected_pairs),
                    f64::from(profile.diagnostics.overall_de00),
                    encode_buckets(&profile.diagnostics.weak_buckets),
                    profile.diagnostics.recommendation,
                    profile.engine_ver,
                    i64::from(profile.analysis_ver),
                    trained_at,
                    stamp,
                ],
            )
            .map_err(|err| statement_failed("insert profiles", &err))?;

            for (group, delta) in &groups {
                write_bucket_row(
                    conn,
                    &profile.id.to_db(),
                    &format!("{}/*", group.as_str()),
                    group.as_str(),
                    "*",
                    delta,
                    true,
                    delta.samples,
                    0,
                    None,
                    delta.confidence,
                )?;
            }
            for (bucket, model) in &profile.buckets {
                write_bucket_row(
                    conn,
                    &profile.id.to_db(),
                    &bucket.as_key(),
                    bucket.group.as_str(),
                    bucket.lighting.as_str(),
                    &model.delta,
                    false,
                    model.samples,
                    model.held_out,
                    model.match_de00,
                    model.shrink,
                )?;
            }
            Ok(())
        })
    }

    /// Every profile, newest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn profiles(&self) -> AuraResult<Vec<StyleProfile>> {
        let ids = self.catalog.read(|conn| {
            let mut statement = conn
                .prepare("SELECT profile_id FROM profiles ORDER BY trained_at_ts DESC, profile_id")
                .map_err(|err| statement_failed("prepare profiles", &err))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|err| statement_failed("query profiles", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| statement_failed("read profiles", &err))?);
            }
            Ok(out)
        })?;
        let mut out = Vec::new();
        for id in ids {
            if let Ok(parsed) = ProfileId::from_db(&id) {
                if let Some(profile) = self.profile(parsed)? {
                    out.push(profile);
                }
            }
        }
        Ok(out)
    }

    /// One profile and its buckets.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    #[allow(clippy::too_many_lines)]
    pub fn profile(&self, id: ProfileId) -> AuraResult<Option<StyleProfile>> {
        let key = id.to_db();
        self.catalog.read(move |conn| {
            let head: Option<StyleProfile> = conn
                .query_row(
                    "SELECT * FROM profiles WHERE profile_id = ?1",
                    params![key],
                    |row| {
                        let mut profile = StyleProfile::empty(
                            id,
                            row.get::<_, String>("name")?,
                            row.get::<_, String>("engine_ver")?,
                        );
                        profile.version = row.get::<_, i64>("version")?.max(1) as u16;
                        profile.status =
                            ProfileStatus::from_str_or_candidate(&row.get::<_, String>("status")?);
                        profile.global = StyleDelta {
                            exposure: row.get::<_, f64>("exposure")? as f32,
                            temperature_k: row.get::<_, f64>("temperature_k")? as f32,
                            tint: row.get::<_, f64>("tint")? as f32,
                            contrast: row.get::<_, f64>("contrast")? as f32,
                            highlights: row.get::<_, f64>("highlights")? as f32,
                            shadows: row.get::<_, f64>("shadows")? as f32,
                            whites: row.get::<_, f64>("whites")? as f32,
                            blacks: row.get::<_, f64>("blacks")? as f32,
                            curve_shift: decode_shift(&row.get::<_, String>("curve_shift")?),
                            hsl: decode_hsl(&row.get::<_, String>("hsl")?),
                            vibrance: row.get::<_, f64>("vibrance")? as f32,
                            saturation: row.get::<_, f64>("saturation")? as f32,
                            skin_bias: SkinBias {
                                warmth_deg: row.get::<_, f64>("skin_warmth_deg")? as f32,
                                chroma: row.get::<_, f64>("skin_chroma")? as f32,
                            },
                            confidence: row.get::<_, f64>("confidence")? as f32,
                            samples: row.get::<_, i64>("trained_pairs")?.max(0) as u32,
                        };
                        profile.trained_pairs = row.get::<_, i64>("trained_pairs")?.max(0) as u32;
                        profile.trained_at = row.get::<_, i64>("trained_at_ts")?;
                        profile.analysis_ver = row.get::<_, i64>("analysis_ver")?.max(0) as u16;
                        profile.diagnostics = ProfileDiagnostics {
                            per_bucket: Vec::new(),
                            weak_buckets: decode_buckets(&row.get::<_, String>("weak_buckets")?),
                            overall_de00: row.get::<_, f64>("overall_de00")? as f32,
                            accepted_pairs: row.get::<_, i64>("accepted_pairs")?.max(0) as u32,
                            rejected_pairs: row.get::<_, i64>("rejected_pairs")?.max(0) as u32,
                            recommendation: row.get::<_, String>("recommendation")?,
                        };
                        Ok(profile)
                    },
                )
                .optional()
                .map_err(|err| statement_failed("select profile", &err))?;

            let Some(mut profile) = head else {
                return Ok(None);
            };

            let mut statement = conn
                .prepare(
                    "SELECT * FROM profile_buckets WHERE profile_id = ?1
                      ORDER BY scene_group, lighting",
                )
                .map_err(|err| statement_failed("prepare buckets", &err))?;
            let rows = statement
                .query_map(params![profile.id.to_db()], |row| {
                    let delta = StyleDelta {
                        exposure: row.get::<_, f64>("exposure")? as f32,
                        temperature_k: row.get::<_, f64>("temperature_k")? as f32,
                        tint: row.get::<_, f64>("tint")? as f32,
                        contrast: row.get::<_, f64>("contrast")? as f32,
                        highlights: row.get::<_, f64>("highlights")? as f32,
                        shadows: row.get::<_, f64>("shadows")? as f32,
                        whites: row.get::<_, f64>("whites")? as f32,
                        blacks: row.get::<_, f64>("blacks")? as f32,
                        curve_shift: decode_shift(&row.get::<_, String>("curve_shift")?),
                        hsl: decode_hsl(&row.get::<_, String>("hsl")?),
                        vibrance: row.get::<_, f64>("vibrance")? as f32,
                        saturation: row.get::<_, f64>("saturation")? as f32,
                        skin_bias: SkinBias {
                            warmth_deg: row.get::<_, f64>("skin_warmth_deg")? as f32,
                            chroma: row.get::<_, f64>("skin_chroma")? as f32,
                        },
                        confidence: row.get::<_, f64>("confidence")? as f32,
                        samples: row.get::<_, i64>("samples")?.max(0) as u32,
                    };
                    Ok((
                        row.get::<_, String>("bucket")?,
                        row.get::<_, i64>("is_group")? != 0,
                        delta,
                        row.get::<_, i64>("samples")?.max(0) as u32,
                        row.get::<_, i64>("held_out")?.max(0) as u32,
                        row.get::<_, Option<f64>>("match_de00")?
                            .map(|value| value as f32),
                        row.get::<_, f64>("shrink")? as f32,
                    ))
                })
                .map_err(|err| statement_failed("query buckets", &err))?;

            let mut per_bucket = Vec::new();
            for row in rows {
                let (key, is_group, delta, samples, held_out, match_de00, shrink) =
                    row.map_err(|err| statement_failed("read buckets", &err))?;
                if is_group {
                    let group = key.split_once('/').map_or(SceneGroup::Other, |(head, _)| {
                        SceneGroup::from_str_or_other(head)
                    });
                    profile.groups.insert(group, delta);
                } else {
                    let bucket = StyleBucket::from_key(&key);
                    per_bucket.push(BucketDiagnostic {
                        bucket,
                        samples,
                        match_de00,
                        level: FallbackLevel::Bucket,
                    });
                    profile.buckets.insert(
                        bucket,
                        BucketModel {
                            bucket,
                            delta,
                            samples,
                            held_out,
                            match_de00,
                            shrink,
                        },
                    );
                }
            }
            // The level is filled in after every row is present, because resolving a bucket
            // needs the group and the global that a partially loaded profile does not have yet.
            for row in &mut per_bucket {
                let (_, level) = profile.resolve(row.bucket);
                row.level = level;
            }
            profile.diagnostics.per_bucket = per_bucket;
            Ok(Some(profile))
        })
    }

    /// Adopt one profile, retiring whatever was adopted under the same name.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5075` when the profile does not exist.
    pub fn adopt(&self, id: ProfileId) -> AuraResult<()> {
        let key = id.to_db();
        let changed = self.catalog.writer().transact(move |conn| {
            let name: Option<String> = conn
                .query_row(
                    "SELECT name FROM profiles WHERE profile_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|err| statement_failed("select profile name", &err))?;
            let Some(name) = name else {
                return Ok(0_usize);
            };
            // Retire first, then adopt: the partial unique index refuses two adopted rows of one
            // name, so doing it the other way round is a constraint violation rather than an
            // update. The order is the constraint, made visible.
            conn.execute(
                "UPDATE profiles SET status = 'retired'
                  WHERE name = ?1 AND status = 'adopted'",
                params![name],
            )
            .map_err(|err| statement_failed("retire profiles", &err))?;
            let n = conn
                .execute(
                    "UPDATE profiles SET status = 'adopted' WHERE profile_id = ?1",
                    params![key],
                )
                .map_err(|err| statement_failed("adopt profile", &err))?;
            Ok(n)
        })?;
        if changed == 0 {
            return Err(errors::profile_refused(
                &id.to_db(),
                "no profile with that id exists",
            ));
        }
        Ok(())
    }

    // -----------------------------------------------------------------
    // Selection
    // -----------------------------------------------------------------

    /// Which profile one project, and optionally one chapter of it, uses.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5075` when the profile does not exist or has not been adopted.
    pub fn select(
        &self,
        project: ProjectId,
        chapter: Option<ChapterId>,
        profile: Option<ProfileId>,
    ) -> AuraResult<()> {
        let key = project.to_db();
        let chapter_key =
            chapter.map_or_else(|| PROJECT_DEFAULT.to_string(), |c| c.as_str().to_string());
        let now = epoch_ms(self.clock.now_utc());
        match profile {
            None => {
                self.catalog.writer().transact(move |conn| {
                    conn.execute(
                        "DELETE FROM project_style WHERE project_id = ?1 AND chapter = ?2",
                        params![key, chapter_key],
                    )
                    .map_err(|err| statement_failed("clear project_style", &err))?;
                    Ok(())
                })?;
                Ok(())
            }
            Some(id) => {
                let profile_key = id.to_db();
                let adopted = self.catalog.read({
                    let profile_key = profile_key.clone();
                    move |conn| {
                        let status: Option<String> = conn
                            .query_row(
                                "SELECT status FROM profiles WHERE profile_id = ?1",
                                params![profile_key],
                                |row| row.get(0),
                            )
                            .optional()
                            .map_err(|err| statement_failed("select profile status", &err))?;
                        Ok(status)
                    }
                })?;
                match adopted.as_deref() {
                    None => {
                        return Err(errors::profile_refused(
                            &id.to_db(),
                            "no profile with that id exists",
                        ))
                    }
                    Some("adopted") => {}
                    Some(other) => {
                        return Err(errors::profile_refused(
                            &id.to_db(),
                            &format!(
                                "the profile is {other} and only an adopted profile can be \
                                 selected - adoption is a deliberate act"
                            ),
                        ))
                    }
                }
                self.catalog.writer().transact(move |conn| {
                    conn.execute(
                        "INSERT INTO project_style (project_id, chapter, profile_id, at_ts)
                         VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(project_id, chapter) DO UPDATE SET
                             profile_id = excluded.profile_id, at_ts = excluded.at_ts",
                        params![key, chapter_key, profile_key, now],
                    )
                    .map_err(|err| statement_failed("insert project_style", &err))?;
                    Ok(())
                })?;
                Ok(())
            }
        }
    }

    /// Which profile applies to one project, and to one chapter of it.
    ///
    /// The chapter override wins over the project default, which is section 6.4's whole point -
    /// "a moody reception with an airy ceremony".
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn selected(
        &self,
        project: ProjectId,
        chapter: Option<ChapterId>,
    ) -> AuraResult<Option<ProfileId>> {
        let key = project.to_db();
        let chapter_key = chapter.map(|c| c.as_str().to_string());
        self.catalog.read(move |conn| {
            if let Some(chapter_key) = chapter_key {
                let found: Option<String> = conn
                    .query_row(
                        "SELECT profile_id FROM project_style
                          WHERE project_id = ?1 AND chapter = ?2",
                        params![key, chapter_key],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|err| statement_failed("select chapter profile", &err))?;
                if let Some(found) = found {
                    return Ok(ProfileId::from_db(&found).ok());
                }
            }
            let found: Option<String> = conn
                .query_row(
                    "SELECT profile_id FROM project_style
                      WHERE project_id = ?1 AND chapter = ?2",
                    params![key, PROJECT_DEFAULT],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|err| statement_failed("select project profile", &err))?;
            Ok(found.and_then(|text| ProfileId::from_db(&text).ok()))
        })
    }

    /// What one project knows about style.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn outline(&self, project: ProjectId) -> AuraResult<StyleOutline> {
        let profiles = self.catalog.read(|conn| {
            conn.query_row("SELECT COUNT(*) FROM profiles", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|err| statement_failed("count profiles", &err))
        })?;
        let overrides = self.catalog.read({
            let key = project.to_db();
            move |conn| {
                let mut statement = conn
                    .prepare(
                        "SELECT chapter FROM project_style
                          WHERE project_id = ?1 AND chapter <> '*' ORDER BY chapter",
                    )
                    .map_err(|err| statement_failed("prepare overrides", &err))?;
                let rows = statement
                    .query_map(params![key], |row| row.get::<_, String>(0))
                    .map_err(|err| statement_failed("query overrides", &err))?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row.map_err(|err| statement_failed("read overrides", &err))?);
                }
                Ok(out)
            }
        })?;

        let active = self.selected(project, None)?;
        let mut outline = StyleOutline {
            profiles: profiles.max(0) as u32,
            active,
            chapter_overrides: overrides,
            level_counts: BTreeMap::new(),
            ..StyleOutline::default()
        };
        if let Some(id) = active {
            if let Some(profile) = self.profile(id)? {
                outline.active_name.clone_from(&profile.name);
                outline.active_version = profile.version;
                outline.trained_pairs = profile.trained_pairs;
                outline.strength = profile.strength();
                outline.overall_de00 = profile.diagnostics.overall_de00;
                outline.analysis_ver = profile.analysis_ver;
                for row in &profile.diagnostics.per_bucket {
                    *outline
                        .level_counts
                        .entry(row.level.as_str().to_string())
                        .or_insert(0) += 1;
                }
            }
        }
        Ok(outline)
    }
}

#[allow(clippy::too_many_arguments)]
fn write_bucket_row(
    conn: &rusqlite::Connection,
    profile_id: &str,
    bucket: &str,
    group: &str,
    lighting: &str,
    delta: &StyleDelta,
    is_group: bool,
    samples: u32,
    held_out: u32,
    match_de00: Option<f32>,
    shrink: f32,
) -> AuraResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO profile_buckets (
             profile_id, bucket, scene_group, lighting,
             exposure, temperature_k, tint, contrast, highlights, shadows,
             whites, blacks, vibrance, saturation, curve_shift, hsl,
             skin_warmth_deg, skin_chroma, confidence, is_group,
             samples, held_out, match_de00, shrink
         ) VALUES (
             ?1, ?2, ?3, ?4,
             ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
             ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24
         )",
        params![
            profile_id,
            bucket,
            group,
            lighting,
            f64::from(delta.exposure),
            f64::from(delta.temperature_k),
            f64::from(delta.tint),
            f64::from(delta.contrast),
            f64::from(delta.highlights),
            f64::from(delta.shadows),
            f64::from(delta.whites),
            f64::from(delta.blacks),
            f64::from(delta.vibrance),
            f64::from(delta.saturation),
            encode_shift(&delta.curve_shift),
            encode_hsl(&delta.hsl),
            f64::from(delta.skin_bias.warmth_deg),
            f64::from(delta.skin_bias.chroma),
            f64::from(delta.confidence),
            i64::from(is_group),
            i64::from(samples),
            i64::from(held_out),
            match_de00.map(f64::from),
            f64::from(shrink),
        ],
    )
    .map_err(|err| statement_failed("insert profile_buckets", &err))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The four documents this store encodes
//
// Flat arrays rather than objects, for phase 16's reason: a key is a name that can be typo'd
// and carried silently through a hash, and every one of these has a fixed length and a frozen
// order. Every decoder is **total** - a malformed document decodes to the neutral value rather
// than raising - because a row written by a future build must still open. Phase 07's rule.
// ---------------------------------------------------------------------------

fn encode_shift(shift: &CurveShift) -> String {
    let values = shift.as_array();
    let parts: Vec<String> = values.iter().map(|value| format!("{value:.4}")).collect();
    format!("[{}]", parts.join(","))
}

fn decode_shift(text: &str) -> CurveShift {
    let values = numbers(text);
    CurveShift::from_array([
        values.first().copied().unwrap_or(0.0),
        values.get(1).copied().unwrap_or(0.0),
        values.get(2).copied().unwrap_or(0.0),
        values.get(3).copied().unwrap_or(0.0),
        values.get(4).copied().unwrap_or(0.0),
    ])
}

fn encode_hsl(hsl: &HslAdjustments) -> String {
    let mut parts = Vec::with_capacity(HslBand::COUNT * 3);
    for band in HslBand::ALL {
        let shift = hsl.get(band);
        parts.push(format!("{:.4}", shift.h));
        parts.push(format!("{:.4}", shift.s));
        parts.push(format!("{:.4}", shift.l));
    }
    format!("[{}]", parts.join(","))
}

fn decode_hsl(text: &str) -> HslAdjustments {
    let values = numbers(text);
    let mut out = HslAdjustments::default();
    for (index, band) in HslBand::ALL.into_iter().enumerate() {
        out.set(
            band,
            HslShift {
                h: values.get(index * 3).copied().unwrap_or(0.0),
                s: values.get(index * 3 + 1).copied().unwrap_or(0.0),
                l: values.get(index * 3 + 2).copied().unwrap_or(0.0),
            },
        );
    }
    out
}

fn encode_curve(curve: &aura_core::contract::colour::ToneCurve) -> String {
    let parts: Vec<String> = curve
        .points()
        .iter()
        .flat_map(|point| [point.x.to_string(), point.y.to_string()])
        .collect();
    format!("[{}]", parts.join(","))
}

fn decode_curve(text: &str) -> aura_core::contract::colour::ToneCurve {
    use aura_core::contract::colour::{CurvePoint, ToneCurve};
    let values = numbers(text);
    let points: Vec<CurvePoint> = values
        .chunks_exact(2)
        .filter_map(|pair| {
            let (Some(x), Some(y)) = (pair.first(), pair.get(1)) else {
                return None;
            };
            Some(CurvePoint::new(
                x.clamp(0.0, 255.0) as u16,
                y.clamp(0.0, 255.0) as u16,
            ))
        })
        .collect();
    // A stored curve that is not monotone reads as the identity. `ToneCurve::new` refuses it on
    // the way in exactly as it refuses it on the way out, so a row written by a future build
    // cannot deliver a posterising curve to anything. Phase 16's codec makes the same choice.
    ToneCurve::new(points).unwrap_or_else(|_| ToneCurve::identity())
}

fn encode_buckets(buckets: &[StyleBucket]) -> String {
    let parts: Vec<String> = buckets
        .iter()
        .map(|bucket| format!("\"{}\"", bucket.as_key()))
        .collect();
    format!("[{}]", parts.join(","))
}

fn decode_buckets(text: &str) -> Vec<StyleBucket> {
    text.trim_matches(['[', ']'])
        .split(',')
        .map(|part| part.trim().trim_matches('"'))
        .filter(|part| !part.is_empty())
        .map(StyleBucket::from_key)
        .collect()
}

fn epoch_ms(now: time::OffsetDateTime) -> i64 {
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn numbers(text: &str) -> Vec<f32> {
    text.trim_matches(['[', ']'])
        .split(',')
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::style::LightingBucket;

    #[test]
    fn the_curve_shift_document_round_trips() {
        let shift = CurveShift::from_array([1.5, -2.25, 0.0, 3.75, -1.0]);
        let decoded = decode_shift(&encode_shift(&shift));
        assert!((decoded.black - 1.5).abs() < 1e-3);
        assert!((decoded.high - 3.75).abs() < 1e-3);
    }

    #[test]
    fn the_hsl_document_round_trips_in_band_order() {
        let mut hsl = HslAdjustments::default();
        hsl.set(
            HslBand::Green,
            HslShift {
                h: 4.0,
                s: -8.0,
                l: 1.0,
            },
        );
        let decoded = decode_hsl(&encode_hsl(&hsl));
        assert!((decoded.get(HslBand::Green).s + 8.0).abs() < 1e-3);
        assert!(decoded.get(HslBand::Blue).is_neutral());
    }

    #[test]
    fn every_decoder_is_total_on_rubbish() {
        assert!(decode_shift("not a document").is_neutral());
        assert!(decode_hsl("").is_neutral());
        assert!(decode_curve("[]").is_identity());
        assert!(decode_buckets("[]").is_empty());
    }

    #[test]
    fn a_non_monotone_stored_curve_decodes_as_the_identity() {
        assert!(decode_curve("[0,0,128,200,64,30,255,255]").is_identity());
    }

    #[test]
    fn the_weak_bucket_list_round_trips() {
        let buckets = vec![
            StyleBucket::new(SceneGroup::Dance, LightingBucket::Flash),
            StyleBucket::new(SceneGroup::Details, LightingBucket::Tungsten),
        ];
        assert_eq!(decode_buckets(&encode_buckets(&buckets)), buckets);
    }

    #[test]
    fn a_group_row_key_can_never_collide_with_a_leaf_key() {
        // `StyleBucket::from_key` maps the group row's `*` lighting to `Unknown`, and the key it
        // produces back is never `group/*` - so the two namespaces do not overlap.
        let group_key = format!("{}/*", SceneGroup::Dance.as_str());
        let parsed = StyleBucket::from_key(&group_key);
        assert_ne!(parsed.as_key(), group_key);
    }
}
