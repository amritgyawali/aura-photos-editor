//! Assembling a moment, and the four tables migration 8 adds.
//!
//! Two halves. The first turns a group of frame indices into the frozen [`Moment`] -
//! its diversity, its confidence and the sentences that explain it. The second reads
//! and writes them, and holds the two rules that live in the SQL rather than around it.
//!
//! ## `user_locked` is checked inside the statement, not before it
//!
//! A re-grouping pass starts by deleting the moments it is about to replace. That
//! delete carries `AND user_locked = 0`, so a photographer's split cannot be undone by
//! a background pass that read the flag a moment earlier and acted on it a moment
//! later. Phase 06 wrote this rule for `identities`, phase 07 for `segments`, and the
//! window it closes is the same one in all three.
//!
//! The frames inside a locked moment are then excluded from the pass entirely - see
//! [`MomentStore::locked_frames`] - because a frame that is in a locked moment and also
//! in a freshly grouped one would violate `idx_moment_images_photo` and take the whole
//! transaction down. The photographer's grouping is not merely preserved; it is
//! subtracted from the problem.
//!
//! ## Why the store reads `faces.identity_id` directly
//!
//! Phase 06's rule is that `PeopleService` is the only way to ask **who** is in a
//! photograph, and this does not ask that. It reads back the assignment
//! `PeopleService` already made, as opaque ids, to answer "do these two frames contain
//! the same people" - and it holds no template, opens no vault, computes no identity
//! and cannot name anybody. Depending on `aura-people` to fetch two integers would put
//! the biometric crate in the dependency graph of the crate that groups photographs,
//! which is the arrangement phase 06 built its two crates specifically to avoid.
//!
//! It is the mirror image of what phase 07 already does in the other direction:
//! `aura_people::store::PeopleStore::scene_labels` reads `image_scenes` by SQL rather
//! than depending on this crate. See ADR-0017 section 6.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::moment::{
    CameraId, DuplicateKind, DuplicateSet, ImageId, Moment, MomentEdit, Timestamp,
};
use aura_core::errors::db::statement_failed;
use aura_core::{
    AuraError, AuraResult, IdentityId, MomentId, PhotoId, ProjectId, SceneId, SegmentId,
};
use rusqlite::{params, Connection};

use crate::errors;
use crate::moments::cadence::{CameraTable, Drive};
use crate::moments::graph::{Frame, MomentProfileRegistry};
use crate::moments::{burst, duplicate};

/// The mean pairwise distance at which a moment counts as fully diverse.
///
/// Diversity is `mean_pairwise_distance / DIVERSITY_FULL_SCALE`, clamped to `0..1`.
///
/// 0.30 rather than the theoretical maximum of 2.0, and the reason is that the maximum
/// is unreachable *by construction*: every frame in a moment got there through edges
/// that cleared a scene threshold, so a moment whose frames average 0.30 apart is
/// already at the loose end of what this phase will group. Scaling against 2.0 would
/// compress every real moment into the bottom sixth of the range and make
/// `Moment::suggested_keepers` return one for everything.
pub const DIVERSITY_FULL_SCALE: f32 = 0.30;

/// The confidence a moment of one frame is given.
///
/// A singleton is a real decision - every neighbour inside the window was compared and
/// none was close enough - but it is the decision this phase is least able to defend,
/// because a frame that grouped with nothing may equally be a frame whose neighbours
/// have no embedding yet. 0.75 is deliberately at `NEEDS_REVIEW_BELOW`, so a UI that
/// flags uncertain groupings the way phase 07's timeline flags uncertain chapters will
/// draw the line in the same place.
pub const SINGLETON_CONFIDENCE: f32 = 0.75;

/// Bytes one photograph costs across the moment tables and their indexes.
///
/// Section 11 budgets **200** and the measured figure is **319**, which is a recorded
/// waiver rather than a met budget - ADR-0017 section 8, and the row in
/// `perf/budgets.toml` carries the same decomposition.
///
/// | Part | Bytes |
/// |---|---|
/// | the membership row: two prefixed UUIDs and two integers | 90 |
/// | its unique index, `photo_id` to `moment_id` | 85 |
/// | the `moments` row, over the 3.3 frames per moment the fixtures produce | 88 |
/// | its three indexes: project and time, segment, locked | 55 |
///
/// **Measured, not estimated.** `crates/aura-perf/tests/moment_budgets.rs` reads
/// `PRAGMA page_count` on a real catalog before and after, exactly as phase 07's
/// equivalent does and for the reason phase 07 learned: its first draft claimed 330
/// bytes and the catalog said 410. This phase's first draft claimed 189 and the catalog
/// said 720; four schema decisions took that to 319, and the last 119 is 40-character
/// text ids plus the reasons invariant 2 requires.
pub const BYTES_PER_IMAGE: usize = 90 + 85 + 88 + 55;

/// The `SegmentId` a moment carries when the wedding has not been segmented.
///
/// The nil UUID. `Moment::segment_id` is non-optional in the frozen contract - PHASE-08
/// section 5 writes it that way - and migration 8's column is nullable, so one of the
/// two has to carry "not placed". A sentinel in the type and a NULL in the column is
/// the pairing that keeps the contract as section 5 froze it while letting grouping run
/// on a wedding phase 07 has not read yet.
pub const UNPLACED: SegmentId = SegmentId::from_uuid(uuid::Uuid::nil());

/// One assembled moment, with the columns the contract does not carry.
#[derive(Debug, Clone, PartialEq)]
pub struct MomentRow {
    /// The frozen moment.
    pub moment: Moment,
    /// Which burst each frame is in, parallel to `moment.image_ids`.
    pub burst_of: Vec<u32>,
    /// Which embedding produced the distances.
    pub embed_ver: u16,
    /// Which grouping implementation built it.
    pub group_ver: u16,
    /// Which threshold table it read.
    pub profile_ver: u16,
}

/// Everything [`assemble`] needs that is not in the frames.
#[derive(Debug, Clone, Copy)]
pub struct Assembly<'a> {
    /// The threshold table in force.
    pub profiles: &'a MomentProfileRegistry,
    /// The chapter this moment sits in, when the wedding has been segmented.
    pub segment: Option<SegmentId>,
    /// Which phase 05 embedding produced the distances.
    pub embed_ver: u16,
    /// The weakest edge score that holds the group together, when it has any edges.
    pub weakest_edge: Option<f32>,
    /// True when every internal link carried camera drive evidence.
    pub all_drive: bool,
    /// True when the group survived the over-large re-cluster pass.
    pub reclustered: bool,
    /// Overlap and medoid distance, when a cross-camera merge produced this moment.
    pub cross_camera: Option<(f32, f32)>,
}

/// Turn a group of frame indices into a moment.
///
/// `members` must be in timeline order. The bursts, the duplicate sets, the diversity
/// and the reasons are all computed here so that a moment is a complete answer by the
/// time it leaves this function - a half-built moment written to the catalog and
/// completed later would be a row whose `confidence` did not describe its contents.
#[must_use]
pub fn assemble(
    frames: &[Frame],
    members: &[usize],
    cadence: &crate::moments::cadence::Cadence,
    cameras: &CameraTable,
    context: Assembly<'_>,
    faces: &dyn Fn(usize, usize) -> duplicate::FaceOverlap,
    same_file: &dyn Fn(usize, usize) -> bool,
) -> MomentRow {
    let bursts = burst::partition(frames, members, cadence);
    let duplicate_sets = duplicate::sets_of_moment(frames, members, &bursts, faces, same_file);

    let image_ids: Vec<ImageId> = members
        .iter()
        .filter_map(|index| frames.get(*index).map(|frame| frame.image))
        .collect();

    // Which burst each frame is in, in `image_ids` order, for `moment_images.burst_ix`.
    let mut burst_of = vec![0u32; image_ids.len()];
    for (slot, burst_group) in bursts.iter().enumerate() {
        for global in burst_group {
            if let Some(position) = members.iter().position(|index| index == global) {
                if let Some(cell) = burst_of.get_mut(position) {
                    *cell = slot as u32;
                }
            }
        }
    }

    let times: Vec<Timestamp> = members
        .iter()
        .filter_map(|index| frames.get(*index).map(|frame| frame.ts))
        .collect();
    let mut camera_ids: Vec<CameraId> = members
        .iter()
        .filter_map(|index| frames.get(*index).map(|frame| cameras.label(frame.camera)))
        .collect();
    camera_ids.sort();
    camera_ids.dedup();

    let mut identities: Vec<IdentityId> = members
        .iter()
        .filter_map(|index| frames.get(*index))
        .flat_map(|frame| frame.identities.iter().copied())
        .collect();
    identities.sort();
    identities.dedup();

    let diversity = diversity_of(frames, members);
    let (confidence, reasons) = explain(frames, members, &bursts, diversity, cameras, context);

    MomentRow {
        moment: Moment {
            id: MomentId::new(),
            // The nil UUID is the sentinel for "not placed in a chapter yet", and it is
            // what `segment_param` turns into a NULL column. The frozen shape has a
            // non-optional `SegmentId`, and inventing a fresh random one here would
            // write a dangling foreign key that looks like a real chapter.
            segment_id: context.segment.unwrap_or(UNPLACED),
            image_ids,
            bursts: burst::to_image_ids(frames, &bursts),
            start_ts: times.iter().copied().min().unwrap_or(0),
            end_ts: times.iter().copied().max().unwrap_or(0),
            cameras: camera_ids,
            identities,
            diversity,
            duplicate_sets,
            user_locked: false,
            confidence,
            reasons,
        },
        burst_of,
        embed_ver: context.embed_ver,
        group_ver: crate::moments::graph::GROUP_VER,
        profile_ver: context.profiles.version(),
    }
}

/// Mean pairwise embedding distance inside a group, normalised to `0..1`.
///
/// Every pair rather than a sample, because a moment is bounded by `max_group` at 60
/// frames and 1,770 distance computations is microseconds. Sampling would make the
/// number non-deterministic for no measurable saving, and invariant 4 forbids that.
#[must_use]
pub fn diversity_of(frames: &[Frame], members: &[usize]) -> f32 {
    if members.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0f32;
    let mut pairs = 0usize;
    for (offset, left) in members.iter().enumerate() {
        for right in members.iter().skip(offset + 1) {
            if let (Some(a), Some(b)) = (frames.get(*left), frames.get(*right)) {
                total += aura_index::contract::index::cosine_distance(&a.embedding, &b.embedding);
                pairs += 1;
            }
        }
    }
    if pairs == 0 {
        return 0.0;
    }
    ((total / pairs as f32) / DIVERSITY_FULL_SCALE).clamp(0.0, 1.0)
}

/// How sure the grouping is, and why - invariant 2, for a moment.
///
/// The confidence is driven by the **weakest** internal link rather than by the mean,
/// because a moment is only as defensible as the edge a photographer would argue with
/// first. A chain of six strong edges and one marginal one is a moment that might be
/// two, and averaging would hide exactly that.
fn explain(
    frames: &[Frame],
    members: &[usize],
    bursts: &[Vec<usize>],
    diversity: f32,
    cameras: &CameraTable,
    context: Assembly<'_>,
) -> (f32, Vec<String>) {
    let mut reasons: Vec<String> = Vec::new();

    if members.len() < 2 {
        reasons.push("no neighbouring frame was close enough to group with".to_string());
        return (SINGLETON_CONFIDENCE, reasons);
    }

    let scene = members
        .first()
        .and_then(|index| frames.get(*index))
        .map_or(SceneId::Unknown, |frame| frame.scene);
    let threshold = context.profiles.get(scene).edge_threshold;

    let span_ms = {
        let times: Vec<Timestamp> = members
            .iter()
            .filter_map(|index| frames.get(*index).map(|frame| frame.ts))
            .collect();
        let (low, high) = (
            times.iter().copied().min().unwrap_or(0),
            times.iter().copied().max().unwrap_or(0),
        );
        high - low
    };
    reasons.push(format!(
        "{} frames over {:.1} s",
        members.len(),
        span_ms as f32 / 1_000.0
    ));

    if context.all_drive {
        reasons.push("the camera recorded one continuous release".to_string());
    } else if bursts.len() > 1 {
        reasons.push(format!("{} separate takes of the same thing", bursts.len()));
    }

    let mut bodies: BTreeSet<u32> = BTreeSet::new();
    for index in members {
        if let Some(frame) = frames.get(*index) {
            bodies.insert(frame.camera);
        }
    }
    if let Some((overlap, medoid_distance)) = context.cross_camera {
        let names: Vec<String> = bodies
            .iter()
            .map(|handle| cameras.label(*handle).to_string())
            .collect();
        reasons.push(format!(
            "two photographers overlapping {:.0}% of the time, {medoid_distance:.2} apart in \
             appearance ({})",
            overlap * 100.0,
            names.join(", ")
        ));
    }

    if context.reclustered {
        reasons.push("split out of a larger run that exceeded this scene's size cap".to_string());
    }

    if scene != SceneId::Unknown {
        reasons.push(format!("scene {scene}, grouped at {threshold:.2}"));
    }
    if diversity >= 0.55 {
        reasons.push("the action changed across these frames".to_string());
    }
    reasons.truncate(Moment::MAX_REASONS);

    // The confidence: how far past the threshold the weakest link is, mapped onto
    // 0.5..1.0. A group held together by an edge exactly at the bar is a coin flip and
    // says so; one held by edges at 0.95 is not.
    let base = match context.weakest_edge {
        Some(weakest) if threshold < 1.0 => {
            0.5 + 0.5 * ((weakest - threshold) / (1.0 - threshold)).clamp(0.0, 1.0)
        }
        // No recorded edge and more than one frame: a cross-camera merge, which is
        // held together by the two conditions in `merge`, not by an edge score.
        _ => 0.70,
    };
    let confidence = if context.all_drive {
        // The camera said the shutter was held down. Nothing this code computes is
        // better evidence than that.
        base.max(0.95)
    } else if context.reclustered {
        // A group that had to be broken up is a group the thresholds did not describe
        // well, and its pieces inherit that doubt.
        base * 0.85
    } else {
        base
    };

    (confidence.clamp(0.0, 1.0), reasons)
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

/// Reads and writes the four tables migration 8 adds.
#[derive(Debug)]
pub struct MomentStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl MomentStore {
    /// Use one catalog's moment tables.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The catalog underneath.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// Everything the grouping pass needs about one project's photographs.
    ///
    /// One query for the frames, one for the identities, one for the camera ids. Three
    /// round trips rather than one join, because the identity rows are one-to-many and
    /// a join would return a frame once per face - which on a 60-guest group shot is
    /// sixty copies of a 1,024-byte vector.
    ///
    /// Frames with no embedding are **absent** rather than present-and-empty:
    /// `v_moment_coverage` reports them, `AURA-ML-5032` explains them, and grouping a
    /// frame that has never been compared with anything would be a claim this phase has
    /// not earned.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when any of the three queries fails.
    pub fn load_frames(&self, project: &ProjectId) -> AuraResult<(Vec<Frame>, CameraTable)> {
        let key = project.to_db();
        let identities = self.identities_by_photo(project)?;
        let camera_ids = self.camera_ids(project)?;
        let mut table = CameraTable::new(camera_ids);

        let rows = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    // `COALESCE(camera_id, camera_serial, '')`, in that order, and the
                    // middle term is load-bearing. Phase 01 populates the `camera`
                    // table during import, but a project part-way through one - or a
                    // body whose serial appeared after the camera pass - has photographs
                    // with a serial and no camera row. Falling through to `''` there
                    // would make every body look like the same body, and the
                    // cross-camera merge would have nothing to merge.
                    "SELECT e.photo_id, p.timeline_time, e.vec, e.dhash, e.model_ver,
                            COALESCE(c.camera_id, p.camera_serial, ''), p.sub_sec,
                            COALESCE(d.edge_energy, 0.0),
                            COALESCE(s.scene, ''), COALESCE(s.scene_conf, 0.0)
                       FROM embeddings e
                       JOIN photo p             ON p.photo_id = e.photo_id
                       LEFT JOIN descriptors d  ON d.photo_id = e.photo_id
                       LEFT JOIN image_scenes s ON s.photo_id = e.photo_id
                       LEFT JOIN camera c       ON c.project_id = p.project_id
                                               AND c.body_serial = COALESCE(p.camera_serial, '')
                      WHERE e.project_id = ?1 AND p.timeline_time IS NOT NULL
                      ORDER BY p.timeline_time, e.photo_id",
                )
                .map_err(|e| statement_failed("could not read frames for grouping", &e))?;

            let mut out: Vec<RawFrame> = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read frames for grouping", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a frame row", &e))?
            {
                if let Some(raw) = read_raw_frame(row) {
                    out.push(raw);
                }
            }
            Ok(out)
        })?;

        let frames = rows
            .into_iter()
            .map(|raw| {
                let camera = table.handle(&raw.camera_id);
                Frame {
                    image: raw.image,
                    ts: raw.ts,
                    camera,
                    drive: Drive {
                        sub_sec: raw.sub_sec,
                        // Phase 01's EXIF pass does not resolve maker-note drive mode,
                        // so this is `false` for every real catalog today. It is read
                        // from the catalog rather than hard-coded so that the day the
                        // ingest pass learns the tag, nothing here changes. Section
                        // 6.1's inferred path carries the case meanwhile.
                        continuous: false,
                    },
                    embedding: raw.embedding,
                    dhash: raw.dhash,
                    identities: identities.get(&raw.image).cloned().unwrap_or_default(),
                    scene: raw.scene,
                    classified: raw.classified,
                    edge_energy: raw.edge_energy,
                    // Filled by the caller from `PeopleService` when a face pass has
                    // run. Zero here, and the keep-hint says so in its reasons.
                    subject_focus: 0.0,
                }
            })
            .collect();

        Ok((frames, table))
    }

    /// Which identities `PeopleService` assigned to each photograph.
    ///
    /// Ids only. See this module's header for why this is not a second `PeopleService`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn identities_by_photo(
        &self,
        project: &ProjectId,
    ) -> AuraResult<BTreeMap<PhotoId, BTreeSet<IdentityId>>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT image_id, identity_id FROM faces
                      WHERE project_id = ?1 AND identity_id IS NOT NULL
                      ORDER BY image_id, identity_id",
                )
                .map_err(|e| statement_failed("could not read face assignments", &e))?;
            let mut out: BTreeMap<PhotoId, BTreeSet<IdentityId>> = BTreeMap::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read face assignments", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a face row", &e))?
            {
                let (Ok(photo), Ok(identity)) = (row.get::<_, String>(0), row.get::<_, String>(1))
                else {
                    continue;
                };
                let (Ok(photo), Ok(identity)) =
                    (PhotoId::from_db(&photo), IdentityId::from_db(&identity))
                else {
                    continue;
                };
                out.entry(photo).or_default().insert(identity);
            }
            Ok(out)
        })
    }

    /// Every camera body id in a project, sorted.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn camera_ids(&self, project: &ProjectId) -> AuraResult<Vec<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT camera_id FROM camera WHERE project_id = ?1 ORDER BY camera_id")
                .map_err(|e| statement_failed("could not read cameras", &e))?;
            let rows = statement
                .query_map(params![key], |row| row.get::<_, String>(0))
                .map_err(|e| statement_failed("could not read cameras", &e))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|e| statement_failed("bad camera id", &e))?);
            }
            Ok(out)
        })
    }

    /// Photographs the photographer has locked into a moment.
    ///
    /// Subtracted from the grouping pass's input, so a locked moment is not merely
    /// preserved - it is removed from the problem. See this module's header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn locked_frames(&self, project: &ProjectId) -> AuraResult<BTreeSet<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT mi.photo_id FROM moment_images mi
                       JOIN moments m ON m.id = mi.moment_id
                      WHERE m.project_id = ?1 AND m.user_locked = 1",
                )
                .map_err(|e| statement_failed("could not read locked moments", &e))?;
            let rows = statement
                .query_map(params![key], |row| row.get::<_, String>(0))
                .map_err(|e| statement_failed("could not read locked moments", &e))?;
            let mut out = BTreeSet::new();
            for row in rows {
                let text = row.map_err(|e| statement_failed("bad photo id", &e))?;
                if let Ok(photo) = PhotoId::from_db(&text) {
                    out.insert(photo);
                }
            }
            Ok(out)
        })
    }

    /// Replace one project's unlocked moments with a fresh grouping.
    ///
    /// One transaction: the delete and every insert, so a killed pass leaves either the
    /// old grouping or the new one and never half of each. Invariant 5.
    ///
    /// Returns how many moments were written.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn put_moments(&self, project: &ProjectId, rows: &[MomentRow]) -> AuraResult<usize> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let payload: Vec<MomentRow> = rows.to_vec();

        self.catalog.writer().transact(move |tx| {
            // The rule that is in the statement rather than around it. `moment_images`
            // and `duplicates` cascade from here, so one delete clears all three.
            tx.execute(
                "DELETE FROM moments WHERE project_id = ?1 AND user_locked = 0",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear the previous grouping", &e))?;

            let mut written = 0usize;
            for row in &payload {
                let moment = &row.moment;
                let reasons =
                    serde_json::to_string(&moment.reasons).unwrap_or_else(|_| "[]".into());
                let cameras = i64::try_from(moment.cameras.len()).unwrap_or(0);
                tx.execute(
                    "INSERT INTO moments (id, project_id, segment_id, start_ts, end_ts,
                                          frame_count, burst_count, camera_count, diversity,
                                          confidence, reasons, user_locked, embed_ver,
                                          group_ver, profile_ver, created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0,?12,?13,?14,?15,?15)",
                    params![
                        moment.id.to_db(),
                        key,
                        // A moment whose chapter is not known writes NULL rather than a
                        // fabricated segment id: the column is nullable precisely so
                        // that grouping does not depend on segmentation having run.
                        segment_param(moment),
                        moment.start_ts,
                        moment.end_ts,
                        i64::try_from(moment.image_ids.len()).unwrap_or(0),
                        i64::try_from(moment.bursts.len()).unwrap_or(0),
                        cameras,
                        f64::from(moment.diversity),
                        f64::from(moment.confidence),
                        reasons,
                        i64::from(row.embed_ver),
                        i64::from(row.group_ver),
                        i64::from(row.profile_ver),
                        now,
                    ],
                )
                .map_err(|e| statement_failed("could not write a moment", &e))?;

                {
                    let mut membership = tx
                        .prepare(
                            "INSERT INTO moment_images (moment_id, photo_id, position, burst_ix)
                             VALUES (?1,?2,?3,?4)",
                        )
                        .map_err(|e| statement_failed("could not prepare a membership", &e))?;
                    for (position, image) in moment.image_ids.iter().enumerate() {
                        membership
                            .execute(params![
                                moment.id.to_db(),
                                image.to_db(),
                                i64::try_from(position).unwrap_or(0),
                                i64::from(row.burst_of.get(position).copied().unwrap_or(0)),
                            ])
                            .map_err(|e| statement_failed("could not write a membership", &e))?;
                    }
                }

                {
                    let mut sets = tx
                        .prepare(
                            "INSERT INTO duplicates (moment_id, set_ix, photo_id, kind, keep_hint,
                                                     user_chosen, confidence, reasons,
                                                     dhash_dist, embed_dist)
                             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                        )
                        .map_err(|e| statement_failed("could not prepare a duplicate", &e))?;
                    for (set_ix, set) in moment.duplicate_sets.iter().enumerate() {
                        let reasons =
                            serde_json::to_string(&set.reasons).unwrap_or_else(|_| "[]".into());
                        for image in &set.image_ids {
                            let is_hint = *image == set.keep_hint;
                            sets.execute(params![
                                moment.id.to_db(),
                                i64::try_from(set_ix).unwrap_or(0),
                                image.to_db(),
                                set.kind.as_str(),
                                i64::from(is_hint),
                                i64::from(set.user_chosen),
                                f64::from(set.confidence),
                                // The reasons ride on the hint row only. Repeating them
                                // on every frame of a forty-frame set would cost more
                                // than the whole per-image storage budget.
                                if is_hint {
                                    reasons.clone()
                                } else {
                                    "[]".to_string()
                                },
                                0i64,
                                0.0f64,
                            ])
                            .map_err(|e| statement_failed("could not write a duplicate", &e))?;
                        }
                    }
                }
                written += 1;
            }
            Ok(written)
        })
    }

    /// Every moment in a project, earliest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_moments(&self, project: &ProjectId) -> AuraResult<Vec<Moment>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT id, segment_id, start_ts, end_ts, diversity, confidence, reasons,
                            user_locked
                       FROM moments WHERE project_id = ?1 ORDER BY start_ts, id",
                )
                .map_err(|e| statement_failed("could not read moments", &e))?;
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not read moments", &e))?;

            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not read a moment row", &e))?
            {
                if let Some(moment) = read_moment(conn, row) {
                    out.push(moment);
                }
            }
            Ok(out)
        })
    }

    /// The moment one photograph is in.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn moment_of(&self, image: ImageId) -> AuraResult<Option<Moment>> {
        let key = image.to_db();
        self.catalog.read(move |conn| {
            let found: Result<String, rusqlite::Error> = conn.query_row(
                "SELECT moment_id FROM moment_images WHERE photo_id = ?1",
                params![key],
                |row| row.get(0),
            );
            let id = match found {
                Ok(id) => id,
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(err) => return Err(statement_failed("could not find a moment", &err)),
            };
            load_one(conn, &id)
        })
    }

    /// One moment by id.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_moment(&self, moment: MomentId) -> AuraResult<Option<Moment>> {
        let key = moment.to_db();
        self.catalog.read(move |conn| load_one(conn, &key))
    }

    /// The duplicate sets of one moment.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn duplicates(&self, moment: MomentId) -> AuraResult<Vec<DuplicateSet>> {
        let key = moment.to_db();
        self.catalog.read(move |conn| duplicate_sets(conn, &key))
    }

    /// Photographs, groupable frames, grouped frames, moments and locked moments.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the view cannot be read.
    pub fn coverage(&self, project: &ProjectId) -> AuraResult<(i64, i64, i64, i64, i64)> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let found: Result<(i64, i64, i64, i64, i64), rusqlite::Error> = conn.query_row(
                "SELECT photos, groupable, grouped, moments, COALESCE(locked, 0)
                   FROM v_moment_coverage WHERE project_id = ?1",
                params![key],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            );
            // A project with no photographs has no row in the view. Zero is the
            // truthful answer, not an error.
            Ok(found.unwrap_or((0, 0, 0, 0, 0)))
        })
    }

    /// Version tuples present in a project's moments, with their counts.
    ///
    /// More than one tuple means the project is mid-regroup. The caller reports
    /// `AURA-ML-5028` and keeps going; nothing here decides policy.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn versions(&self, project: &ProjectId) -> AuraResult<Vec<(u16, u16, u16, usize)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT embed_ver, group_ver, profile_ver, COUNT(*)
                       FROM moments WHERE project_id = ?1
                      GROUP BY embed_ver, group_ver, profile_ver
                      ORDER BY embed_ver, group_ver, profile_ver",
                )
                .map_err(|e| statement_failed("could not read moment versions", &e))?;
            let rows = statement
                .query_map(params![key], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .map_err(|e| statement_failed("could not read moment versions", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let (embed, group, profile, count) =
                    row.map_err(|e| statement_failed("bad version row", &e))?;
                out.push((
                    u16::try_from(embed).unwrap_or(0),
                    u16::try_from(group).unwrap_or(0),
                    u16::try_from(profile).unwrap_or(0),
                    usize::try_from(count).unwrap_or(0),
                ));
            }
            Ok(out)
        })
    }

    /// Set or clear a moment's lock.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5029` when the moment is unknown.
    pub fn set_locked(&self, moment: MomentId, locked: bool) -> Result<(), AuraError> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = moment.to_db();
        let changed = self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE moments SET user_locked = ?1, updated_at = ?2 WHERE id = ?3",
                params![i64::from(locked), now, key],
            )
            .map_err(|e| statement_failed("could not lock a moment", &e))
        })?;
        if changed == 0 {
            return Err(errors::moment_edit_refused(
                if locked { "lock" } else { "unlock" },
                "that moment is not in this catalog",
            ));
        }
        Ok(())
    }

    /// Move a duplicate set's keep hint to a photograph the photographer chose.
    ///
    /// Two statements in one transaction: clear the old hint, set the new one. Both
    /// scoped to the set the photograph is in, so a moment with three sets keeps the
    /// other two untouched.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5029` when the photograph is not in any of the moment's sets.
    pub fn set_keep_hint(&self, moment: MomentId, keep: ImageId) -> Result<(), AuraError> {
        let key = moment.to_db();
        let photo = keep.to_db();
        let changed = self.catalog.writer().transact(move |tx| {
            let set_ix: Option<i64> = tx
                .query_row(
                    "SELECT set_ix FROM duplicates WHERE moment_id = ?1 AND photo_id = ?2",
                    params![key, photo],
                    |row| row.get(0),
                )
                .ok();
            let Some(set_ix) = set_ix else {
                return Ok(0usize);
            };
            tx.execute(
                "UPDATE duplicates SET keep_hint = 0
                  WHERE moment_id = ?1 AND set_ix = ?2",
                params![key, set_ix],
            )
            .map_err(|e| statement_failed("could not clear the previous hint", &e))?;
            tx.execute(
                "UPDATE duplicates SET keep_hint = 1, user_chosen = 1
                  WHERE moment_id = ?1 AND set_ix = ?2 AND photo_id = ?3",
                params![key, set_ix, photo],
            )
            .map_err(|e| statement_failed("could not set the hint", &e))
        })?;
        if changed == 0 {
            return Err(errors::moment_edit_refused(
                "keep_hint",
                "that photograph is not in any duplicate set of this moment",
            ));
        }
        Ok(())
    }

    /// Move frames out of one moment into a new one, and lock both.
    ///
    /// Both, for the reason `StoryService::move_boundary` locks both sides of a chapter
    /// boundary: a split is one statement about two moments, and locking one would let
    /// the next re-grouping put them back together from the other side.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn apply_split(
        &self,
        original: MomentId,
        moved: &[ImageId],
        keep_span: (Timestamp, Timestamp),
        moved_span: (Timestamp, Timestamp),
    ) -> AuraResult<MomentId> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let source = original.to_db();
        let created = MomentId::new();
        let fresh = created.to_db();
        let images: Vec<String> = moved.iter().map(PhotoId::to_db).collect();

        self.catalog.writer().transact(move |tx| {
            // The new half inherits the original's chapter and versions: a split does
            // not re-derive anything, it partitions what is already there.
            tx.execute(
                "INSERT INTO moments (id, project_id, segment_id, start_ts, end_ts, frame_count,
                                      burst_count, camera_count, diversity, confidence, reasons,
                                      user_locked, embed_ver, group_ver, profile_ver,
                                      created_at, updated_at)
                 SELECT ?1, project_id, segment_id, ?4, ?5, 0, 0, camera_count,
                        diversity, 1.0, '[\"the photographer split this moment\"]', 1,
                        embed_ver, group_ver, profile_ver, ?3, ?3
                   FROM moments WHERE id = ?2",
                params![fresh, source, now, moved_span.0, moved_span.1],
            )
            .map_err(|e| statement_failed("could not create the new moment", &e))?;

            {
                let mut moving = tx
                    .prepare(
                        "UPDATE moment_images SET moment_id = ?1, burst_ix = 0
                          WHERE moment_id = ?2 AND photo_id = ?3",
                    )
                    .map_err(|e| statement_failed("could not prepare the move", &e))?;
                for image in &images {
                    moving
                        .execute(params![fresh, source, image])
                        .map_err(|e| statement_failed("could not move a frame", &e))?;
                }
            }

            // Duplicate sets do not survive a split: a set that spanned the cut is no
            // longer a set, and recomputing one needs embeddings this statement does
            // not have. The next grouping pass rebuilds them for the unlocked side, and
            // a locked moment keeps its frames without a duplicate claim - which is
            // honest, because the claim was about a group that no longer exists.
            tx.execute(
                "DELETE FROM duplicates WHERE moment_id = ?1",
                params![source],
            )
            .map_err(|e| statement_failed("could not clear duplicate sets", &e))?;

            renumber(tx, &source)?;
            renumber(tx, &fresh)?;

            tx.execute(
                "UPDATE moments SET user_locked = 1, confidence = 1.0,
                        reasons = '[\"the photographer split this moment\"]',
                        start_ts = ?3, end_ts = ?4, updated_at = ?2
                  WHERE id = ?1",
                params![source, now, keep_span.0, keep_span.1],
            )
            .map_err(|e| statement_failed("could not lock the original", &e))?;

            // A split that emptied the original leaves no row behind: an empty moment
            // is not a moment, and `frame_count = 0` would appear in the grid as a
            // stack with nothing in it. `plan_split` refuses this case, so reaching it
            // means the catalog disagreed with what was read a moment earlier.
            tx.execute(
                "DELETE FROM moments WHERE id IN (?1, ?2) AND frame_count = 0",
                params![source, fresh],
            )
            .map_err(|e| statement_failed("could not clean up an empty moment", &e))?;
            Ok(())
        })?;
        Ok(created)
    }

    /// Fold one moment's frames into another, and lock the survivor.
    ///
    /// `span` is the merged `(start_ts, end_ts)`, computed by the caller from the two
    /// moments it already read. Recomputing it in SQL would mean parsing RFC 3339 text
    /// with `strftime`, which truncates to whole seconds - and a moment's start is
    /// compared against a chapter's boundary to the millisecond.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transaction fails.
    pub fn apply_merge(
        &self,
        survivor: MomentId,
        absorbed: MomentId,
        span: (Timestamp, Timestamp),
    ) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let keep = survivor.to_db();
        let gone = absorbed.to_db();

        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE moment_images SET moment_id = ?1 WHERE moment_id = ?2",
                params![keep, gone],
            )
            .map_err(|e| statement_failed("could not move the frames", &e))?;
            tx.execute(
                "DELETE FROM duplicates WHERE moment_id IN (?1, ?2)",
                params![keep, gone],
            )
            .map_err(|e| statement_failed("could not clear duplicate sets", &e))?;
            tx.execute("DELETE FROM moments WHERE id = ?1", params![gone])
                .map_err(|e| statement_failed("could not remove the absorbed moment", &e))?;

            renumber(tx, &keep)?;
            tx.execute(
                "UPDATE moments
                    SET user_locked = 1, confidence = 1.0,
                        reasons = '[\"the photographer merged these moments\"]',
                        start_ts = ?3, end_ts = ?4, updated_at = ?2
                  WHERE id = ?1",
                params![keep, now, span.0, span.1],
            )
            .map_err(|e| statement_failed("could not lock the survivor", &e))?;
            Ok(())
        })
    }

    /// Record an edit in the undo journal.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the insert fails.
    pub fn record_edit(&self, project: &ProjectId, edit: &MomentEdit) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = project.to_db();
        let action = edit.action.clone();
        let moment = edit.moment.to_db();
        let other = edit.other.map(|id| id.to_db());
        let photo = edit.image.map(|id| id.to_db());
        let size = i64::from(edit.moment_size);
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO moment_edits (edit_id, project_id, action, moment_id, other_id,
                                           photo_id, moment_size, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    MomentId::new().to_db(),
                    key,
                    action,
                    moment,
                    other,
                    photo,
                    size,
                    now
                ],
            )
            .map_err(|e| statement_failed("could not record a grouping edit", &e))?;
            Ok(())
        })
    }

    /// The most recent edit that has not been undone.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn last_edit(&self, project: &ProjectId) -> AuraResult<Option<(String, MomentEdit)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let found: Result<(String, String, String, Option<String>, Option<String>, i64), _> =
                conn.query_row(
                    "SELECT edit_id, action, moment_id, other_id, photo_id, moment_size
                       FROM moment_edits
                      WHERE project_id = ?1 AND undone_at IS NULL
                      ORDER BY created_at DESC, edit_id DESC LIMIT 1",
                    params![key],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                );
            match found {
                Ok((edit_id, action, moment, other, photo, size)) => {
                    let Ok(moment) = MomentId::from_db(&moment) else {
                        return Ok(None);
                    };
                    Ok(Some((
                        edit_id,
                        MomentEdit {
                            action,
                            moment,
                            other: other.and_then(|id| MomentId::from_db(&id).ok()),
                            image: photo.and_then(|id| PhotoId::from_db(&id).ok()),
                            moment_size: u32::try_from(size).unwrap_or(0),
                        },
                    )))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed("could not read the edit journal", &err)),
            }
        })
    }

    /// Mark an edit undone. The row stays, so the journal never forgets.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the update fails.
    pub fn mark_undone(&self, edit_id: &str) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let key = edit_id.to_string();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE moment_edits SET undone_at = ?1 WHERE edit_id = ?2",
                params![now, key],
            )
            .map_err(|e| statement_failed("could not mark an edit undone", &e))?;
            Ok(())
        })
    }

    /// Whether two moments belong to the same project and chapter.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn same_home(&self, a: MomentId, b: MomentId) -> AuraResult<bool> {
        let left = a.to_db();
        let right = b.to_db();
        self.catalog.read(move |conn| {
            let read = |id: &str| -> Option<(String, Option<String>)> {
                conn.query_row(
                    "SELECT project_id, segment_id FROM moments WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok()
            };
            Ok(match (read(&left), read(&right)) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            })
        })
    }
}

/// The segment id to write, or NULL for a moment no chapter has claimed.
fn segment_param(moment: &Moment) -> Option<String> {
    if moment.segment_id == UNPLACED {
        None
    } else {
        Some(moment.segment_id.to_db())
    }
}

/// Renumber a moment's positions and refresh its counts after an edit.
fn renumber(tx: &Connection, moment: &str) -> AuraResult<()> {
    tx.execute(
        "UPDATE moment_images
            SET position = (SELECT COUNT(*) FROM moment_images other
                             JOIN photo op ON op.photo_id = other.photo_id
                             JOIN photo tp ON tp.photo_id = moment_images.photo_id
                            WHERE other.moment_id = moment_images.moment_id
                              AND (op.timeline_time < tp.timeline_time
                                   OR (op.timeline_time = tp.timeline_time
                                       AND other.photo_id < moment_images.photo_id)))
          WHERE moment_id = ?1",
        params![moment],
    )
    .map_err(|e| statement_failed("could not renumber a moment", &e))?;
    tx.execute(
        "UPDATE moments
            SET frame_count = (SELECT COUNT(*) FROM moment_images WHERE moment_id = ?1),
                burst_count = (SELECT COUNT(DISTINCT burst_ix) FROM moment_images
                                WHERE moment_id = ?1)
          WHERE id = ?1",
        params![moment],
    )
    .map_err(|e| statement_failed("could not refresh a moment's counts", &e))?;
    Ok(())
}

/// Milliseconds a stored `photo.sub_sec` represents.
///
/// **This function is why phase 08 can see a burst at all**, and the reason it exists is
/// a defect the phase 08 gate found end to end after every unit test had passed.
///
/// EXIF's `DateTimeOriginal` has **whole-second resolution**. Phase 01 parses it into
/// `photo.timeline_time`, so every frame of a 10 fps burst carries the same timestamp
/// as the nine around it, and a cadence estimator reading that column alone sees a
/// photographer who took fourteen photographs in one second and then stopped. The
/// fraction lives in `SubSecTimeOriginal`, which phase 01 stores separately in
/// `photo.sub_sec` - and section 6.1's "sub-second EXIF ... where present" is that
/// column.
///
/// ## The digit-count problem, and the documented reading of it
///
/// `SubSecTimeOriginal` is a *string of fractional digits*: `"12"` is 0.12 s and
/// `"012"` is 0.012 s. The catalog column is an `INTEGER`, so the digit count is gone
/// by the time this code sees it, and 12 could mean either.
///
/// The reading below is by magnitude: one or two digits are **centiseconds**, three are
/// **milliseconds**. That is right for the overwhelming majority of bodies - two digits
/// is what Canon, Nikon and Sony write - and it is wrong only for a body that writes
/// three digits and happens to produce a value under 100, where it reads 0.012 s as
/// 0.12 s. The consequence of that error is a frame placed up to 108 ms from where it
/// belongs, which is inside the 700 ms window floor and therefore changes no grouping.
///
/// The alternative - storing the digit count - is a phase 01 schema change, and this
/// note is the argument for making it if a camera is ever found where the heuristic
/// costs a real grouping.
#[must_use]
pub fn sub_sec_ms(sub_sec: u16) -> i64 {
    if sub_sec >= 100 {
        i64::from(sub_sec.min(999))
    } else {
        i64::from(sub_sec) * 10
    }
}

/// One row of the frame query, before the camera handle is assigned.
struct RawFrame {
    image: PhotoId,
    ts: Timestamp,
    embedding: Vec<f32>,
    dhash: u64,
    camera_id: String,
    sub_sec: Option<u16>,
    edge_energy: f32,
    scene: SceneId,
    classified: bool,
}

/// Read one joined frame row, or `None` when it cannot be trusted.
fn read_raw_frame(row: &rusqlite::Row<'_>) -> Option<RawFrame> {
    let photo: String = row.get(0).ok()?;
    let image = PhotoId::from_db(&photo).ok()?;
    let timeline: String = row.get(1).ok()?;
    let whole_seconds = aura_index::store::parse_rfc3339_ms(&timeline)?;

    let bytes: Vec<u8> = row.get(2).ok()?;
    let mut vector =
        [aura_index::contract::index::F16::ZERO; aura_index::contract::index::EMBED_DIM];
    aura_index::store::decode_vector(&bytes, &mut vector)?;
    let embedding: Vec<f32> = vector.iter().map(|half| half.to_f32()).collect();

    let dhash: i64 = row.get(3).ok()?;
    let camera_id: String = row.get(5).ok().unwrap_or_default();
    let sub_sec: Option<i64> = row.get(6).ok().flatten();
    let edge_energy: f64 = row.get(7).ok().unwrap_or(0.0);
    let scene_text: String = row.get(8).ok().unwrap_or_default();
    let sub_sec = sub_sec.and_then(|value| u16::try_from(value).ok());

    // The whole second from `timeline_time`, the fraction from `sub_sec`. See
    // `sub_sec_ms`: without this, every frame of a burst carries the same timestamp and
    // the cadence estimator cannot see a burst at all.
    let ts = whole_seconds + sub_sec.map_or(0, sub_sec_ms);

    Some(RawFrame {
        image,
        ts,
        embedding,
        dhash: dhash as u64,
        camera_id,
        sub_sec,
        edge_energy: edge_energy as f32,
        classified: !scene_text.is_empty(),
        scene: if scene_text.is_empty() {
            SceneId::Unknown
        } else {
            SceneId::from_str_or_unknown(&scene_text)
        },
    })
}

/// Read one moment row plus its members and sets.
fn read_moment(conn: &Connection, row: &rusqlite::Row<'_>) -> Option<Moment> {
    let id: String = row.get(0).ok()?;
    let moment_id = MomentId::from_db(&id).ok()?;
    let segment: Option<String> = row.get(1).ok().flatten();
    let start_ts: i64 = row.get(2).ok()?;
    let end_ts: i64 = row.get(3).ok()?;
    let diversity: f64 = row.get(4).ok().unwrap_or(0.0);
    let confidence: f64 = row.get(5).ok().unwrap_or(0.0);
    let reasons: String = row.get(6).ok().unwrap_or_else(|| "[]".into());
    let locked: i64 = row.get(7).ok().unwrap_or(0);

    let (image_ids, bursts) = members_of(conn, &id)?;
    let duplicate_sets = duplicate_sets(conn, &id).ok().unwrap_or_default();
    let (cameras, identities) = context_of(conn, &id);

    Some(Moment {
        id: moment_id,
        segment_id: segment
            .and_then(|text| SegmentId::from_db(&text).ok())
            .unwrap_or_else(|| SegmentId::from_uuid(uuid::Uuid::nil())),
        image_ids,
        bursts,
        start_ts,
        end_ts,
        cameras,
        identities,
        diversity: diversity as f32,
        duplicate_sets,
        user_locked: locked == 1,
        confidence: confidence as f32,
        reasons: serde_json::from_str(&reasons).unwrap_or_default(),
    })
}

/// One moment by id, from an open connection.
fn load_one(conn: &Connection, id: &str) -> AuraResult<Option<Moment>> {
    let found = conn.query_row(
        "SELECT id, segment_id, start_ts, end_ts, diversity, confidence, reasons, user_locked
           FROM moments WHERE id = ?1",
        params![id],
        |row| Ok(read_moment(conn, row)),
    );
    match found {
        Ok(moment) => Ok(moment),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(statement_failed("could not read a moment", &err)),
    }
}

/// A moment's frames in order, and its bursts as a partition of them.
fn members_of(conn: &Connection, moment: &str) -> Option<(Vec<ImageId>, Vec<Vec<ImageId>>)> {
    let mut statement = conn
        .prepare(
            "SELECT photo_id, burst_ix FROM moment_images
              WHERE moment_id = ?1 ORDER BY position, photo_id",
        )
        .ok()?;
    let rows = statement
        .query_map(params![moment], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .ok()?;

    let mut images = Vec::new();
    let mut by_burst: BTreeMap<i64, Vec<ImageId>> = BTreeMap::new();
    for row in rows.flatten() {
        let (text, burst_ix) = row;
        let Ok(photo) = PhotoId::from_db(&text) else {
            continue;
        };
        images.push(photo);
        by_burst.entry(burst_ix).or_default().push(photo);
    }
    Some((images, by_burst.into_values().collect()))
}

/// A moment's distinct cameras and identities.
fn context_of(conn: &Connection, moment: &str) -> (Vec<CameraId>, Vec<IdentityId>) {
    let cameras = conn
        .prepare(
            // The same COALESCE the frame query uses; see the note there.
            "SELECT DISTINCT COALESCE(c.camera_id, p.camera_serial, '')
               FROM moment_images mi
               JOIN photo p       ON p.photo_id = mi.photo_id
               LEFT JOIN camera c ON c.project_id = p.project_id
                                 AND c.body_serial = COALESCE(p.camera_serial, '')
              WHERE mi.moment_id = ?1
              ORDER BY 1",
        )
        .ok()
        .and_then(|mut statement| {
            let rows = statement
                .query_map(params![moment], |row| row.get::<_, String>(0))
                .ok()?;
            Some(rows.flatten().map(CameraId::new).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let identities = conn
        .prepare(
            "SELECT DISTINCT f.identity_id
               FROM moment_images mi
               JOIN faces f ON f.image_id = mi.photo_id
              WHERE mi.moment_id = ?1 AND f.identity_id IS NOT NULL
              ORDER BY 1",
        )
        .ok()
        .and_then(|mut statement| {
            let rows = statement
                .query_map(params![moment], |row| row.get::<_, String>(0))
                .ok()?;
            Some(
                rows.flatten()
                    .filter_map(|text| IdentityId::from_db(&text).ok())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();

    (cameras, identities)
}

/// A moment's duplicate sets, reassembled from the per-frame rows.
fn duplicate_sets(conn: &Connection, moment: &str) -> AuraResult<Vec<DuplicateSet>> {
    let mut statement = conn
        .prepare(
            "SELECT set_ix, photo_id, kind, keep_hint, user_chosen, confidence, reasons
               FROM duplicates WHERE moment_id = ?1 ORDER BY set_ix, photo_id",
        )
        .map_err(|e| statement_failed("could not read duplicate sets", &e))?;
    let rows = statement
        .query_map(params![moment], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| statement_failed("could not read duplicate sets", &e))?;

    let mut by_set: BTreeMap<i64, DuplicateSet> = BTreeMap::new();
    for row in rows.flatten() {
        let (set_ix, photo, kind, is_hint, chosen, confidence, reasons) = row;
        let Ok(image) = PhotoId::from_db(&photo) else {
            continue;
        };
        let entry = by_set.entry(set_ix).or_insert_with(|| DuplicateSet {
            kind: DuplicateKind::from_str_or_variant(&kind),
            image_ids: Vec::new(),
            keep_hint: image,
            confidence: confidence as f32,
            reasons: Vec::new(),
            user_chosen: false,
        });
        entry.image_ids.push(image);
        if is_hint == 1 {
            entry.keep_hint = image;
            entry.user_chosen = chosen == 1;
            entry.reasons = serde_json::from_str(&reasons).unwrap_or_default();
        }
    }
    Ok(by_set.into_values().collect())
}
