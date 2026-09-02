//! Migration 29, read and written.
//!
//! # What the pass may touch and what it may not
//!
//! [`CurateStore::write`] replaces the whole curation of one project in a single transaction: a
//! failure leaves the previous result exactly as it was rather than half-replaced. It touches
//! `curate_run`, `curate_bw`, `curate_hero`, `curate_album`, `album_spread`, `album_chapter`,
//! `social_pick`, `curate_caption` and `curate_reason`.
//!
//! It does **not** touch `curate_override` or `album_order`. Those are the photographer's, and the
//! guarantee that they survive a re-run is three separate statements: this module has no SQL naming
//! `album_order` outside [`CurateStore::set_order`], `album_order.source` may only be `'user'`, and
//! `curate_album_no_reorder` refuses a delete while `user_ordered` is set. Phase 18's protocol,
//! and `tests/no_outputs.rs` is what keeps the first of the three true.
//!
//! # Why the reasons are one table
//!
//! A reason is the same shape for a monochrome pick, a hero, a spread, a social pick and a teaser -
//! a code, a weight and an optional specific sentence - so `curate_reason` carries a `subject_kind`
//! discriminator. Six tables would be six places to forget the `MAX_REASONS` bound, and migration
//! 29's `curate_reason_bounded` trigger reads the discriminator.

use std::collections::BTreeMap;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::cull::{Coverage, CoverageReport, MustHave};
use aura_core::contract::curate::{
    AlbumPlan, AspectVariant, BwMix, BwPick, BwTerms, Caption, CaptionSource, ChapterSpan,
    CurateCode, CurateOverride, CurateReason, CurationOutline, CurationResult, HeroBinding,
    HeroPick, HeroTerms, ImageId, PickKind, ShotScale, SocialPick, SocialSets, SocialSlot, Spread,
    SpreadPair, TeaserPick,
};
use aura_core::contract::ids::{IdentityId, MomentId, SpreadId};
use aura_core::contract::scene::ChapterId;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult, ProjectId};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::errors::decision_refused;

/// Everything one pass produced, ready to be stored.
#[derive(Debug, Clone)]
pub struct StoredRun {
    /// The project.
    pub project: ProjectId,
    /// Photographs in the project, selected or not.
    pub photos: u32,
    /// Photographs phase 12 selected. The denominator.
    pub selected: u32,
    /// Selected photographs curation could read at all.
    pub curated: u32,
    /// What the pass produced.
    pub result: CurationResult,
    /// Whether the cloud sequencing task was reached.
    pub cloud_used: bool,
    /// Cloud moves the local objective agreed with.
    pub cloud_applied: u32,
    /// Cloud moves it refused.
    pub cloud_refused: u32,
    /// Captions a model drafted and the grounding check refused.
    pub captions_refused: u32,
    /// Which `curation.toml`.
    pub policy_ver: u16,
    /// Which build's arithmetic.
    pub analysis_ver: u16,
    /// Which phase 05 embedding.
    pub embed_ver: u16,
}

/// One `curate_run` row, in `SELECT` order.
///
/// A named alias rather than a bare tuple, because twenty-two columns positionally indexed is a
/// shape nobody can review: `row.14` beside `row.15` is exactly where a `captions_refused` and a
/// `cloud_used` get swapped without a compiler ever noticing.
type RunRow = (
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    f32,
    f32,
    f32,
    u32,
    u32,
    u32,
    u32,
    i64,
    u32,
    u32,
    u16,
    u16,
    u16,
    i64,
);

/// Migration 29's rows.
#[derive(Debug)]
pub struct CurateStore {
    catalog: std::sync::Arc<Catalog>,
    clock: std::sync::Arc<dyn Clock>,
}

impl CurateStore {
    /// Open the store over a catalog.
    #[must_use]
    pub fn new(catalog: std::sync::Arc<Catalog>, clock: std::sync::Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// Replace one project's curation, in one transaction.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be written. A failure rolls back, so the previous
    /// curation survives intact.
    pub fn write(&self, run: &StoredRun) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let run = run.clone();
        self.catalog.writer().transact(move |tx| {
            write_run(tx, &run, &now)?;
            write_bw(tx, &run)?;
            write_heroes(tx, &run)?;
            write_album(tx, &run, &now)?;
            write_social(tx, &run)?;
            Ok(())
        })
    }

    /// What a project's curation covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn outline(&self, project: ProjectId) -> AuraResult<CurationOutline> {
        self.catalog.read(|conn| {
            let mut outline = CurationOutline {
                heads_trained: crate::heads_trained(),
                ..CurationOutline::default()
            };
            let row: Option<RunRow> = conn
                .query_row(
                    "SELECT photos, selected, curated, bw_offered, heroes, chapters_covered,
                            spreads, album_size, rhythm_score, rhythm_measurable, pairing_score,
                            facing_unknown, duplicates_refused, slots_unfilled, captions_refused,
                            cloud_used, cloud_applied, cloud_refused, policy_ver, analysis_ver,
                            embed_ver, heads_trained
                       FROM curate_run WHERE project_id = ?1",
                    params![project.to_db()],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                            r.get(8)?,
                            r.get(9)?,
                            r.get(10)?,
                            r.get(11)?,
                            r.get(12)?,
                            r.get(13)?,
                            r.get(14)?,
                            r.get(15)?,
                            r.get(16)?,
                            r.get(17)?,
                            r.get(18)?,
                            r.get(19)?,
                            r.get(20)?,
                            r.get(21)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("curate: outline", &e))?;

            let Some(row) = row else {
                return Ok(outline);
            };
            outline.photos = row.0;
            outline.selected = row.1;
            outline.curated = row.2;
            outline.bw_offered = row.3;
            outline.heroes = row.4;
            outline.chapters_covered = row.5;
            outline.spreads = row.6;
            outline.album_size = row.7;
            outline.rhythm_score = row.8;
            outline.rhythm_measurable = row.9;
            outline.pairing_score = row.10;
            outline.facing_unknown = row.11;
            outline.duplicates_refused = row.12;
            outline.slots_unfilled = row.13;
            outline.captions_refused = row.14;
            outline.cloud_used = row.15 != 0;
            outline.cloud_moves_applied = row.16;
            outline.cloud_moves_refused = row.17;
            outline.policy_ver = row.18;
            outline.analysis_ver = row.19;
            outline.embed_ver = row.20;
            outline.heads_trained = row.21 != 0;

            let count = |sql: &str| -> AuraResult<u32> {
                conn.query_row(sql, params![project.to_db()], |r| r.get::<_, u32>(0))
                    .map_err(|e| statement_failed("curate: count", &e))
            };
            outline.bw_accepted = count(
                "SELECT COUNT(*) FROM curate_override
                  WHERE project_id = ?1 AND kind = 'bw' AND accepted = 1",
            )?;
            outline.bw_rejected = count(
                "SELECT COUNT(*) FROM curate_override
                  WHERE project_id = ?1 AND kind = 'bw' AND accepted = 0",
            )?;
            outline.heroes_accepted = count(
                "SELECT COUNT(*) FROM curate_override
                  WHERE project_id = ?1 AND kind = 'hero' AND accepted = 1",
            )?;
            outline.captions = count("SELECT COUNT(*) FROM curate_caption WHERE project_id = ?1")?;
            outline.reorders = conn
                .query_row(
                    "SELECT reorders FROM curate_album WHERE project_id = ?1",
                    params![project.to_db()],
                    |r| r.get::<_, u32>(0),
                )
                .optional()
                .map_err(|e| statement_failed("curate: reorders", &e))?
                .unwrap_or(0);
            let (covered, missing) = conn
                .query_row(
                    "SELECT covered, missing FROM curate_album WHERE project_id = ?1",
                    params![project.to_db()],
                    |r| Ok((r.get::<_, u32>(0)?, r.get::<_, u32>(1)?)),
                )
                .optional()
                .map_err(|e| statement_failed("curate: coverage", &e))?
                .unwrap_or((0, 0));
            outline.album_covered = covered;
            outline.album_missing = missing;
            outline.bytes = bytes_for(conn)?;
            Ok(outline)
        })
    }

    /// The monochrome candidates, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn bw(&self, project: ProjectId) -> AuraResult<Vec<BwPick>> {
        self.catalog.read(|conn| {
            let decisions = decisions_of(conn, project, PickKind::Bw)?;
            let reasons = reasons_of(conn, project, "bw")?;
            let mut stmt = conn
                .prepare(
                    "SELECT image_id, score, confidence, tonal_separation, colour_distraction,
                            gesture, emotion, grain, mix_red, mix_orange, mix_yellow, mix_green,
                            mix_aqua, mix_blue, mix_purple, mix_magenta, skin_bands
                       FROM curate_bw WHERE project_id = ?1
                      ORDER BY score DESC, image_id",
                )
                .map_err(|e| statement_failed("curate: bw", &e))?;
            let rows = stmt
                .query_map(params![project.to_db()], |r| {
                    let image: String = r.get(0)?;
                    let mut bands = [0i16; 8];
                    for (ix, slot) in bands.iter_mut().enumerate() {
                        *slot = r.get(8 + ix)?;
                    }
                    let skin: String = r.get(16)?;
                    Ok((
                        image,
                        r.get::<_, f32>(1)?,
                        r.get::<_, f32>(2)?,
                        BwTerms {
                            tonal_separation: r.get(3)?,
                            colour_distraction: r.get(4)?,
                            gesture: r.get(5)?,
                            emotion: r.get(6)?,
                            grain: r.get(7)?,
                        },
                        bands,
                        skin,
                    ))
                })
                .map_err(|e| statement_failed("curate: bw rows", &e))?;

            let mut out = Vec::new();
            for row in rows {
                let (image, score, confidence, terms, bands, skin) =
                    row.map_err(|e| statement_failed("curate: bw row", &e))?;
                let image_id = parse_image(&image)?;
                out.push(BwPick {
                    image_id,
                    mix: BwMix { bands },
                    score,
                    terms,
                    skin_bands: skin
                        .split(',')
                        .filter_map(|s| s.trim().parse::<u8>().ok())
                        .collect(),
                    reasons: reasons.get(&image).cloned().unwrap_or_default(),
                    confidence,
                    accepted: decisions.get(&image).copied(),
                });
            }
            Ok(out)
        })
    }

    /// The portfolio, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn heroes(&self, project: ProjectId) -> AuraResult<Vec<HeroPick>> {
        self.catalog.read(|conn| {
            let decisions = decisions_of(conn, project, PickKind::Hero)?;
            let reasons = reasons_of(conn, project, "hero")?;
            let mut stmt = conn
                .prepare(
                    "SELECT image_id, rank, score, confidence, technical, emotion, composition,
                            uniqueness, story, chapter, moment_id, scale, binding
                       FROM curate_hero WHERE project_id = ?1 ORDER BY rank",
                )
                .map_err(|e| statement_failed("curate: heroes", &e))?;
            let rows = stmt
                .query_map(params![project.to_db()], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, f32>(2)?,
                        r.get::<_, f32>(3)?,
                        HeroTerms {
                            technical: r.get(4)?,
                            emotion: r.get(5)?,
                            composition: r.get(6)?,
                            uniqueness: r.get(7)?,
                            story: r.get(8)?,
                        },
                        r.get::<_, String>(9)?,
                        r.get::<_, Option<String>>(10)?,
                        r.get::<_, String>(11)?,
                        r.get::<_, String>(12)?,
                    ))
                })
                .map_err(|e| statement_failed("curate: hero rows", &e))?;

            let mut out = Vec::new();
            for row in rows {
                let (image, rank, score, confidence, terms, chapter, moment, scale, binding) =
                    row.map_err(|e| statement_failed("curate: hero row", &e))?;
                out.push(HeroPick {
                    image_id: parse_image(&image)?,
                    rank,
                    score,
                    terms,
                    chapter: parse_chapter(&chapter),
                    moment: moment.as_deref().and_then(|m| MomentId::from_db(m).ok()),
                    scale: ShotScale::parse(&scale),
                    binding: HeroBinding::parse(&binding),
                    reasons: reasons.get(&image).cloned().unwrap_or_default(),
                    confidence,
                    accepted: decisions.get(&image).copied(),
                });
            }
            Ok(out)
        })
    }

    /// The album, or `None` when the project has not been curated.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn album(&self, project: ProjectId) -> AuraResult<Option<AlbumPlan>> {
        self.catalog.read(|conn| read_album(conn, project))
    }

    /// One spread, or `None` when it is unknown.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn spread(&self, spread: SpreadId) -> AuraResult<Option<Spread>> {
        self.catalog.read(|conn| {
            let row = conn
                .query_row(
                    "SELECT project_id, ix, left_image, right_image, single, chapter, tonal_gap,
                            warmth_gap_k, facing_score, facing_known, similarity, pair_score
                       FROM album_spread WHERE spread_id = ?1",
                    params![spread.to_db()],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, u32>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, String>(5)?,
                            SpreadPair {
                                tonal_gap: r.get(6)?,
                                warmth_gap_k: r.get(7)?,
                                facing_score: r.get(8)?,
                                facing_known: r.get::<_, i64>(9)? != 0,
                                similarity: r.get(10)?,
                                score: r.get(11)?,
                            },
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("curate: spread", &e))?;
            let Some((project, ix, left, right, single, chapter, pair)) = row else {
                return Ok(None);
            };
            let project = ProjectId::from_db(&project)
                .map_err(|e| statement_failed("curate: spread project", &e))?;
            let reasons = reasons_of(conn, project, "spread")?;
            Ok(Some(Spread {
                id: spread,
                index: ix,
                left: left.as_deref().map(parse_image).transpose()?,
                right: right.as_deref().map(parse_image).transpose()?,
                single: single != 0,
                chapter: parse_chapter(&chapter),
                pair,
                reasons: reasons.get(&spread.to_db()).cloned().unwrap_or_default(),
            }))
        })
    }

    /// The social sets.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn social(&self, project: ProjectId) -> AuraResult<SocialSets> {
        self.catalog.read(|conn| {
            let reasons = reasons_of(conn, project, "social")?;
            let mut sets = SocialSets::default();
            let mut stmt = conn
                .prepare(
                    "SELECT set_kind, image_id, rank, slot, aspect, legibility
                       FROM social_pick WHERE project_id = ?1 ORDER BY set_kind, rank",
                )
                .map_err(|e| statement_failed("curate: social", &e))?;
            let rows = stmt
                .query_map(params![project.to_db()], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, f32>(5)?,
                    ))
                })
                .map_err(|e| statement_failed("curate: social rows", &e))?;
            for row in rows {
                let (kind, image, slot, aspect, legibility) =
                    row.map_err(|e| statement_failed("curate: social row", &e))?;
                let pick = SocialPick {
                    image_id: parse_image(&image)?,
                    aspect: AspectVariant::from_str_or_original(&aspect),
                    slot: SocialSlot::parse(&slot),
                    legibility,
                    reasons: reasons.get(&image).cloned().unwrap_or_default(),
                    accepted: None,
                };
                match kind.as_str() {
                    "grid" => sets.grid.push(pick),
                    "story" => sets.story.push(pick),
                    "hero" => sets.hero = Some(pick),
                    _ => {}
                }
            }

            let mut stmt = conn
                .prepare(
                    "SELECT image_id, chapter, text, source
                       FROM curate_caption WHERE project_id = ?1 ORDER BY chapter, image_id",
                )
                .map_err(|e| statement_failed("curate: captions", &e))?;
            let rows = stmt
                .query_map(params![project.to_db()], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .map_err(|e| statement_failed("curate: caption rows", &e))?;
            for row in rows {
                let (image, chapter, text, source) =
                    row.map_err(|e| statement_failed("curate: caption row", &e))?;
                sets.captions.push(Caption {
                    image_id: if image.is_empty() {
                        None
                    } else {
                        Some(parse_image(&image)?)
                    },
                    chapter: parse_chapter(&chapter),
                    text,
                    source: CaptionSource::parse(&source),
                    grounded: true,
                });
            }
            Ok(sets)
        })
    }

    /// The teaser, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn teaser(&self, project: ProjectId) -> AuraResult<Vec<TeaserPick>> {
        self.catalog.read(|conn| {
            let decisions = decisions_of(conn, project, PickKind::Teaser)?;
            let reasons = reasons_of(conn, project, "teaser")?;
            let mut stmt = conn
                .prepare(
                    "SELECT image_id, rank, slot FROM social_pick
                      WHERE project_id = ?1 AND set_kind = 'teaser' ORDER BY rank",
                )
                .map_err(|e| statement_failed("curate: teaser", &e))?;
            let rows = stmt
                .query_map(params![project.to_db()], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| statement_failed("curate: teaser rows", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let (image, rank, slot) =
                    row.map_err(|e| statement_failed("curate: teaser row", &e))?;
                out.push(TeaserPick {
                    image_id: parse_image(&image)?,
                    slot: SocialSlot::parse(&slot),
                    rank,
                    reasons: reasons.get(&image).cloned().unwrap_or_default(),
                    accepted: decisions.get(&image).copied(),
                });
            }
            Ok(out)
        })
    }

    /// A photographer's stored album order, when they have set one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn order(&self, project: ProjectId) -> AuraResult<Option<Vec<ImageId>>> {
        self.catalog.read(|conn| {
            let mut stmt = conn
                .prepare("SELECT image_id FROM album_order WHERE project_id = ?1 ORDER BY ix")
                .map_err(|e| statement_failed("curate: order", &e))?;
            let rows = stmt
                .query_map(params![project.to_db()], |r| r.get::<_, String>(0))
                .map_err(|e| statement_failed("curate: order rows", &e))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(parse_image(
                    &row.map_err(|e| statement_failed("curate: order row", &e))?,
                )?);
            }
            Ok((!out.is_empty()).then_some(out))
        })
    }

    /// Record a photographer's album order.
    ///
    /// The one place in this crate with SQL naming `album_order`. It clears `user_ordered`, replaces
    /// the rows and sets the flag again, all inside one transaction - which is what the
    /// `curate_album_no_reorder` trigger's protocol requires and what makes a *pass* that reached
    /// this table fail loudly.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5143` when the project has no album. `AURA-DB-3006` when the rows cannot be written.
    pub fn set_order(&self, project: ProjectId, order: &[ImageId]) -> Result<(), AuraError> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let order: Vec<ImageId> = order.to_vec();
        self.catalog.writer().transact(move |tx| {
            let exists: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM curate_album WHERE project_id = ?1",
                    params![project.to_db()],
                    |r| r.get(0),
                )
                .map_err(|e| statement_failed("curate: order check", &e))?;
            if exists == 0 {
                return Err(decision_refused(
                    "this wedding has no album to reorder; curate it first",
                ));
            }
            tx.execute(
                "UPDATE curate_album SET user_ordered = 0 WHERE project_id = ?1",
                params![project.to_db()],
            )
            .map_err(|e| statement_failed("curate: order unlock", &e))?;
            tx.execute(
                "DELETE FROM album_order WHERE project_id = ?1",
                params![project.to_db()],
            )
            .map_err(|e| statement_failed("curate: order clear", &e))?;
            for (ix, image) in order.iter().enumerate() {
                tx.execute(
                    "INSERT INTO album_order (project_id, ix, image_id, source, set_at)
                     VALUES (?1, ?2, ?3, 'user', ?4)",
                    params![project.to_db(), ix as i64, image.to_db(), now],
                )
                .map_err(|e| statement_failed("curate: order insert", &e))?;
            }
            tx.execute(
                "UPDATE curate_album
                    SET user_ordered = 1, reorders = reorders + 1, updated_at = ?2
                  WHERE project_id = ?1",
                params![project.to_db(), now],
            )
            .map_err(|e| statement_failed("curate: order lock", &e))?;
            Ok(())
        })
    }

    /// Record what a photographer decided about one pick.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5143` when the note is too long or the pick is not in this project.
    /// `AURA-DB-3006` when the row cannot be written.
    pub fn decide(
        &self,
        project: ProjectId,
        image: ImageId,
        decision: &CurateOverride,
    ) -> Result<(), AuraError> {
        if !decision.within_bounds() {
            return Err(decision_refused("that note is longer than AURA can store"));
        }
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let decision = decision.clone();
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO curate_override (project_id, kind, image_id, accepted, note, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(project_id, kind, image_id)
                 DO UPDATE SET accepted = excluded.accepted, note = excluded.note,
                               decided_at = excluded.decided_at",
                params![
                    project.to_db(),
                    decision.kind.as_str(),
                    image.to_db(),
                    i64::from(decision.accepted),
                    decision.note.clone().unwrap_or_default(),
                    now
                ],
            )
            .map_err(|e| statement_failed("curate: decide", &e))?;
            Ok(())
        })
    }

    /// Everything the last pass produced, or `None` when the project has not been curated.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn result(&self, project: ProjectId) -> AuraResult<Option<CurationResult>> {
        let Some(album) = self.album(project)? else {
            return Ok(None);
        };
        Ok(Some(CurationResult {
            bw: self.bw(project)?,
            heroes: self.heroes(project)?,
            album,
            social: self.social(project)?,
            teaser: self.teaser(project)?,
        }))
    }
}

// ---------------------------------------------------------------------------
// Writers
// ---------------------------------------------------------------------------

fn write_run(tx: &Transaction<'_>, run: &StoredRun, now: &str) -> AuraResult<()> {
    let album = &run.result.album;
    let facing_unknown = album
        .spreads
        .iter()
        .filter(|s| !s.single && !s.pair.facing_known)
        .count() as u32;
    let duplicates_refused = album.spreads.iter().filter(|s| s.single).count() as u32;
    let slots_unfilled: u32 = run
        .result
        .social
        .unfilled_slots()
        .iter()
        .map(|(_, short)| *short)
        .sum();
    tx.execute(
        "INSERT INTO curate_run (project_id, selected, curated, photos, bw_offered, heroes,
                                 chapters_covered, spreads, album_size, rhythm_score,
                                 rhythm_measurable, pairing_score, facing_unknown,
                                 duplicates_refused, slots_unfilled, captions_refused, cloud_used,
                                 cloud_applied, cloud_refused, policy_ver, analysis_ver, embed_ver,
                                 heads_trained, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24, ?24)
         ON CONFLICT(project_id) DO UPDATE SET
            selected = excluded.selected, curated = excluded.curated, photos = excluded.photos,
            bw_offered = excluded.bw_offered, heroes = excluded.heroes,
            chapters_covered = excluded.chapters_covered, spreads = excluded.spreads,
            album_size = excluded.album_size, rhythm_score = excluded.rhythm_score,
            rhythm_measurable = excluded.rhythm_measurable, pairing_score = excluded.pairing_score,
            facing_unknown = excluded.facing_unknown,
            duplicates_refused = excluded.duplicates_refused,
            slots_unfilled = excluded.slots_unfilled, captions_refused = excluded.captions_refused,
            cloud_used = excluded.cloud_used, cloud_applied = excluded.cloud_applied,
            cloud_refused = excluded.cloud_refused, policy_ver = excluded.policy_ver,
            analysis_ver = excluded.analysis_ver, embed_ver = excluded.embed_ver,
            heads_trained = excluded.heads_trained, updated_at = excluded.updated_at",
        params![
            run.project.to_db(),
            run.selected,
            run.curated,
            run.photos,
            run.result.bw.len() as u32,
            run.result.heroes.len() as u32,
            crate::hero::chapters_covered(&run.result.heroes),
            album.spreads.len() as u32,
            album.size,
            album.rhythm_score,
            album.rhythm_measurable,
            album.pairing_score,
            facing_unknown,
            duplicates_refused,
            slots_unfilled,
            run.captions_refused,
            i64::from(run.cloud_used),
            run.cloud_applied,
            run.cloud_refused,
            run.policy_ver,
            run.analysis_ver,
            run.embed_ver,
            i64::from(crate::heads_trained()),
            now
        ],
    )
    .map_err(|e| statement_failed("curate: run", &e))?;
    Ok(())
}

fn write_bw(tx: &Transaction<'_>, run: &StoredRun) -> AuraResult<()> {
    tx.execute(
        "DELETE FROM curate_bw WHERE project_id = ?1",
        params![run.project.to_db()],
    )
    .map_err(|e| statement_failed("curate: bw clear", &e))?;
    clear_reasons(tx, run.project, "bw")?;
    for pick in &run.result.bw {
        let bands = pick.mix.bands;
        tx.execute(
            "INSERT INTO curate_bw (project_id, image_id, score, confidence, tonal_separation,
                                    colour_distraction, gesture, emotion, grain, mix_red,
                                    mix_orange, mix_yellow, mix_green, mix_aqua, mix_blue,
                                    mix_purple, mix_magenta, skin_bands)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18)",
            params![
                run.project.to_db(),
                pick.image_id.to_db(),
                pick.score,
                pick.confidence,
                pick.terms.tonal_separation,
                pick.terms.colour_distraction,
                pick.terms.gesture,
                pick.terms.emotion,
                pick.terms.grain,
                bands[0],
                bands[1],
                bands[2],
                bands[3],
                bands[4],
                bands[5],
                bands[6],
                bands[7],
                pick.skin_bands
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ],
        )
        .map_err(|e| statement_failed("curate: bw insert", &e))?;
        write_reasons(tx, run.project, "bw", &pick.image_id.to_db(), &pick.reasons)?;
    }
    Ok(())
}

fn write_heroes(tx: &Transaction<'_>, run: &StoredRun) -> AuraResult<()> {
    tx.execute(
        "DELETE FROM curate_hero WHERE project_id = ?1",
        params![run.project.to_db()],
    )
    .map_err(|e| statement_failed("curate: hero clear", &e))?;
    clear_reasons(tx, run.project, "hero")?;
    for hero in &run.result.heroes {
        // The reasons go in first, because `curate_hero_reason_count` is an AFTER INSERT trigger on
        // the hero row and can only see what is already there.
        write_reasons(
            tx,
            run.project,
            "hero",
            &hero.image_id.to_db(),
            &hero.reasons,
        )?;
        tx.execute(
            "INSERT INTO curate_hero (project_id, image_id, rank, score, confidence, technical,
                                      emotion, composition, uniqueness, story, chapter, moment_id,
                                      scale, binding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                run.project.to_db(),
                hero.image_id.to_db(),
                hero.rank,
                hero.score,
                hero.confidence,
                hero.terms.technical,
                hero.terms.emotion,
                hero.terms.composition,
                hero.terms.uniqueness,
                hero.terms.story,
                hero.chapter.as_str(),
                hero.moment.map(|m| m.to_db()),
                hero.scale.as_str(),
                hero.binding.as_str()
            ],
        )
        .map_err(|e| statement_failed("curate: hero insert", &e))?;
    }
    Ok(())
}

fn write_album(tx: &Transaction<'_>, run: &StoredRun, now: &str) -> AuraResult<()> {
    let album = &run.result.album;
    let covered = album
        .coverage
        .must_haves
        .iter()
        .filter(|(_, state)| state.is_satisfied())
        .count() as u32;
    let missing = album.coverage.missing().len() as u32;
    let must_haves = album
        .coverage
        .must_haves
        .iter()
        .map(|(rule, state)| format!("\"{}\":\"{}\"", rule.as_str(), state.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let warnings = album
        .coverage
        .warnings
        .iter()
        .map(|w| serde_json::Value::String(w.clone()))
        .collect::<Vec<_>>();
    let warnings = serde_json::Value::Array(warnings).to_string();

    tx.execute(
        "INSERT INTO curate_album (project_id, target_size, size, rhythm_score, rhythm_measurable,
                                   pairing_score, user_ordered, must_haves, warnings, covered,
                                   missing, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(project_id) DO UPDATE SET
            target_size = excluded.target_size, size = excluded.size,
            rhythm_score = excluded.rhythm_score, rhythm_measurable = excluded.rhythm_measurable,
            pairing_score = excluded.pairing_score, must_haves = excluded.must_haves,
            warnings = excluded.warnings, covered = excluded.covered, missing = excluded.missing,
            updated_at = excluded.updated_at",
        params![
            run.project.to_db(),
            album.target_size,
            album.size,
            album.rhythm_score,
            album.rhythm_measurable,
            album.pairing_score,
            i64::from(album.user_ordered),
            format!("{{{must_haves}}}"),
            warnings,
            covered,
            missing,
            now
        ],
    )
    .map_err(|e| statement_failed("curate: album", &e))?;

    tx.execute(
        "DELETE FROM album_spread WHERE project_id = ?1",
        params![run.project.to_db()],
    )
    .map_err(|e| statement_failed("curate: spread clear", &e))?;
    tx.execute(
        "DELETE FROM album_chapter WHERE project_id = ?1",
        params![run.project.to_db()],
    )
    .map_err(|e| statement_failed("curate: chapter clear", &e))?;
    clear_reasons(tx, run.project, "spread")?;
    clear_reasons(tx, run.project, "album")?;

    for spread in &album.spreads {
        tx.execute(
            "INSERT INTO album_spread (spread_id, project_id, ix, left_image, right_image, single,
                                       chapter, tonal_gap, warmth_gap_k, facing_score,
                                       facing_known, similarity, pair_score)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                spread.id.to_db(),
                run.project.to_db(),
                spread.index,
                spread.left.map(|i| i.to_db()),
                spread.right.map(|i| i.to_db()),
                i64::from(spread.single),
                spread.chapter.as_str(),
                spread.pair.tonal_gap,
                spread.pair.warmth_gap_k,
                spread.pair.facing_score,
                i64::from(spread.pair.facing_known),
                spread.pair.similarity,
                spread.pair.score
            ],
        )
        .map_err(|e| statement_failed("curate: spread insert", &e))?;
        write_reasons(
            tx,
            run.project,
            "spread",
            &spread.id.to_db(),
            &spread.reasons,
        )?;
    }
    for span in &album.chapter_map {
        tx.execute(
            "INSERT INTO album_chapter (project_id, chapter, first_ix, len, target)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run.project.to_db(),
                span.chapter.as_str(),
                span.first,
                span.len,
                span.target
            ],
        )
        .map_err(|e| statement_failed("curate: chapter insert", &e))?;
    }
    write_reasons(tx, run.project, "album", "", &album.reasons)?;
    Ok(())
}

fn write_social(tx: &Transaction<'_>, run: &StoredRun) -> AuraResult<()> {
    tx.execute(
        "DELETE FROM social_pick WHERE project_id = ?1",
        params![run.project.to_db()],
    )
    .map_err(|e| statement_failed("curate: social clear", &e))?;
    tx.execute(
        "DELETE FROM curate_caption WHERE project_id = ?1",
        params![run.project.to_db()],
    )
    .map_err(|e| statement_failed("curate: caption clear", &e))?;
    clear_reasons(tx, run.project, "social")?;
    clear_reasons(tx, run.project, "teaser")?;

    let sets = &run.result.social;
    let write_pick = |kind: &str, rank: u32, pick: &SocialPick| -> AuraResult<()> {
        tx.execute(
            "INSERT INTO social_pick (project_id, set_kind, image_id, rank, slot, aspect,
                                      legibility)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run.project.to_db(),
                kind,
                pick.image_id.to_db(),
                rank,
                pick.slot.as_str(),
                pick.aspect.as_str(),
                pick.legibility
            ],
        )
        .map_err(|e| statement_failed("curate: social insert", &e))?;
        write_reasons(
            tx,
            run.project,
            "social",
            &pick.image_id.to_db(),
            &pick.reasons,
        )
    };
    for (rank, pick) in sets.grid.iter().enumerate() {
        write_pick("grid", rank as u32, pick)?;
    }
    for (rank, pick) in sets.story.iter().enumerate() {
        write_pick("story", rank as u32, pick)?;
    }
    if let Some(hero) = &sets.hero {
        write_pick("hero", 0, hero)?;
    }
    for pick in &run.result.teaser {
        tx.execute(
            "INSERT INTO social_pick (project_id, set_kind, image_id, rank, slot, aspect,
                                      legibility)
             VALUES (?1, 'teaser', ?2, ?3, ?4, 'original', 0.0)",
            params![
                run.project.to_db(),
                pick.image_id.to_db(),
                pick.rank,
                pick.slot.as_str()
            ],
        )
        .map_err(|e| statement_failed("curate: teaser insert", &e))?;
        write_reasons(
            tx,
            run.project,
            "teaser",
            &pick.image_id.to_db(),
            &pick.reasons,
        )?;
    }

    for caption in &sets.captions {
        tx.execute(
            "INSERT OR REPLACE INTO curate_caption (project_id, image_id, chapter, text, source,
                                                    grounded)
             VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            params![
                run.project.to_db(),
                caption.image_id.map(|i| i.to_db()).unwrap_or_default(),
                caption.chapter.as_str(),
                caption.text,
                caption.source.as_str()
            ],
        )
        .map_err(|e| statement_failed("curate: caption insert", &e))?;
    }
    Ok(())
}

fn write_reasons(
    tx: &Transaction<'_>,
    project: ProjectId,
    kind: &str,
    subject: &str,
    reasons: &[CurateReason],
) -> AuraResult<()> {
    for (ix, reason) in reasons.iter().enumerate() {
        // An album-level note is unbounded by design - it is one line per finding about the whole
        // album - and every per-pick list is bounded by `curate_reason_bounded`.
        tx.execute(
            "INSERT OR REPLACE INTO curate_reason (project_id, subject_kind, subject_id, ix, code,
                                                   weight, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                project.to_db(),
                kind,
                subject,
                ix as i64,
                reason.code.as_str(),
                reason.weight,
                if reason.text == reason.code.user_text() {
                    String::new()
                } else {
                    reason.text.clone()
                }
            ],
        )
        .map_err(|e| statement_failed("curate: reason insert", &e))?;
    }
    Ok(())
}

fn clear_reasons(tx: &Transaction<'_>, project: ProjectId, kind: &str) -> AuraResult<()> {
    tx.execute(
        "DELETE FROM curate_reason WHERE project_id = ?1 AND subject_kind = ?2",
        params![project.to_db(), kind],
    )
    .map_err(|e| statement_failed("curate: reason clear", &e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Readers
// ---------------------------------------------------------------------------

fn read_album(conn: &Connection, project: ProjectId) -> AuraResult<Option<AlbumPlan>> {
    let row = conn
        .query_row(
            "SELECT target_size, size, rhythm_score, rhythm_measurable, pairing_score,
                    user_ordered, must_haves, warnings
               FROM curate_album WHERE project_id = ?1",
            params![project.to_db()],
            |r| {
                Ok((
                    r.get::<_, u32>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, f32>(2)?,
                    r.get::<_, f32>(3)?,
                    r.get::<_, f32>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|e| statement_failed("curate: album", &e))?;
    let Some((target_size, size, rhythm, measurable, pairing, user_ordered, must_haves, warnings)) =
        row
    else {
        return Ok(None);
    };
    let (must_haves, warnings): (String, String) = (must_haves, warnings);

    let reasons = reasons_of(conn, project, "spread")?;
    let album_reasons = reasons_of(conn, project, "album")?;

    let mut stmt = conn
        .prepare(
            "SELECT spread_id, ix, left_image, right_image, single, chapter, tonal_gap,
                    warmth_gap_k, facing_score, facing_known, similarity, pair_score
               FROM album_spread WHERE project_id = ?1 ORDER BY ix",
        )
        .map_err(|e| statement_failed("curate: spreads", &e))?;
    let rows = stmt
        .query_map(params![project.to_db()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                SpreadPair {
                    tonal_gap: r.get(6)?,
                    warmth_gap_k: r.get(7)?,
                    facing_score: r.get(8)?,
                    facing_known: r.get::<_, i64>(9)? != 0,
                    similarity: r.get(10)?,
                    score: r.get(11)?,
                },
            ))
        })
        .map_err(|e| statement_failed("curate: spread rows", &e))?;

    let mut spreads = Vec::new();
    for row in rows {
        let (id, ix, left, right, single, chapter, pair) =
            row.map_err(|e| statement_failed("curate: spread row", &e))?;
        spreads.push(Spread {
            id: SpreadId::from_db(&id).map_err(|e| statement_failed("curate: spread id", &e))?,
            index: ix,
            left: left.as_deref().map(parse_image).transpose()?,
            right: right.as_deref().map(parse_image).transpose()?,
            single: single != 0,
            chapter: parse_chapter(&chapter),
            pair,
            reasons: reasons.get(&id).cloned().unwrap_or_default(),
        });
    }

    let mut stmt = conn
        .prepare(
            "SELECT chapter, first_ix, len, target FROM album_chapter
              WHERE project_id = ?1 ORDER BY first_ix",
        )
        .map_err(|e| statement_failed("curate: chapters", &e))?;
    let rows = stmt
        .query_map(params![project.to_db()], |r| {
            Ok(ChapterSpan {
                chapter: parse_chapter(&r.get::<_, String>(0)?),
                first: r.get(1)?,
                len: r.get(2)?,
                target: r.get(3)?,
            })
        })
        .map_err(|e| statement_failed("curate: chapter rows", &e))?;
    let mut chapter_map = Vec::new();
    for row in rows {
        chapter_map.push(row.map_err(|e| statement_failed("curate: chapter row", &e))?);
    }

    let mut coverage = CoverageReport::default();
    if let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(&must_haves)
    {
        for rule in MustHave::ALL {
            if let Some(serde_json::Value::String(state)) = map.get(rule.as_str()) {
                coverage
                    .must_haves
                    .push((rule, parse_coverage(state.as_str())));
            }
        }
    }
    if let Ok(serde_json::Value::Array(items)) =
        serde_json::from_str::<serde_json::Value>(&warnings)
    {
        for item in items {
            if let serde_json::Value::String(text) = item {
                coverage.warnings.push(text);
            }
        }
    }
    let identity_coverage: Vec<(IdentityId, u32)> = Vec::new();
    coverage.identity_coverage = identity_coverage;

    Ok(Some(AlbumPlan {
        spreads,
        chapter_map,
        coverage,
        rhythm_score: rhythm,
        rhythm_measurable: measurable,
        pairing_score: pairing,
        size,
        target_size,
        user_ordered: user_ordered != 0,
        reasons: album_reasons.get("").cloned().unwrap_or_default(),
    }))
}

fn reasons_of(
    conn: &Connection,
    project: ProjectId,
    kind: &str,
) -> AuraResult<BTreeMap<String, Vec<CurateReason>>> {
    let mut stmt = conn
        .prepare(
            "SELECT subject_id, code, weight, detail FROM curate_reason
              WHERE project_id = ?1 AND subject_kind = ?2 ORDER BY subject_id, ix",
        )
        .map_err(|e| statement_failed("curate: reasons", &e))?;
    let rows = stmt
        .query_map(params![project.to_db(), kind], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f32>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| statement_failed("curate: reason rows", &e))?;
    let mut out: BTreeMap<String, Vec<CurateReason>> = BTreeMap::new();
    for row in rows {
        let (subject, slug, weight, detail) =
            row.map_err(|e| statement_failed("curate: reason row", &e))?;
        // A slug this build does not know is `AURA-ML-5142`: degraded, so the panel still draws.
        let Ok(code) = CurateCode::parse(&slug) else {
            continue;
        };
        let reason = if detail.is_empty() {
            CurateReason::plain(code, weight)
        } else {
            CurateReason::detailed(code, detail, weight)
        };
        out.entry(subject).or_default().push(reason);
    }
    Ok(out)
}

fn decisions_of(
    conn: &Connection,
    project: ProjectId,
    kind: PickKind,
) -> AuraResult<BTreeMap<String, bool>> {
    let mut stmt = conn
        .prepare(
            "SELECT image_id, accepted FROM curate_override
              WHERE project_id = ?1 AND kind = ?2",
        )
        .map_err(|e| statement_failed("curate: decisions", &e))?;
    let rows = stmt
        .query_map(params![project.to_db(), kind.as_str()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))
        })
        .map_err(|e| statement_failed("curate: decision rows", &e))?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (image, accepted) = row.map_err(|e| statement_failed("curate: decision row", &e))?;
        out.insert(image, accepted);
    }
    Ok(out)
}

/// How many bytes migration 29's tables and indexes occupy.
///
/// `dbstat` payload rather than whole-file `page_count`, which quantises to 4 KiB. Phase 09's
/// lesson: a budget measured with a quantised instrument must not be set at its own measurement.
fn bytes_for(conn: &Connection) -> AuraResult<u64> {
    let sql = "SELECT COALESCE(SUM(payload), 0) FROM dbstat
                WHERE name LIKE 'curate%' OR name LIKE 'album%' OR name LIKE 'social_pick%'
                   OR name LIKE 'idx_curate%' OR name LIKE 'idx_album%'
                   OR name LIKE 'idx_social%'";
    conn.query_row(sql, [], |r| r.get::<_, i64>(0))
        .map(|v| v.max(0) as u64)
        // `dbstat` is a compile-time option. A build without it reports zero rather than failing,
        // because a storage figure is a diagnostic and a missing one must not stop a panel drawing.
        .or(Ok(0))
}

fn parse_image(text: &str) -> AuraResult<ImageId> {
    ImageId::from_db(text).map_err(|e| statement_failed("curate: image id", &e))
}

fn parse_chapter(text: &str) -> ChapterId {
    ChapterId::ALL
        .into_iter()
        .find(|c| c.as_str() == text)
        .unwrap_or(ChapterId::Other)
}

fn parse_coverage(text: &str) -> Coverage {
    Coverage::ALL
        .into_iter()
        .find(|c| c.as_str() == text)
        .unwrap_or(Coverage::Missing)
}
