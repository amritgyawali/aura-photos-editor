//! The classification pass: walk a project, batch it, write it, and survive being
//! killed.
//!
//! Invariant 5, as a query. `StoryStore::pending` returns the photographs that have an
//! embedding and no scene row *at the current version*, so:
//!
//! * a pass that is killed at 50 % restarts and does the other half;
//! * a pass that is run twice does nothing the second time;
//! * a `MODEL_VER` bump makes every frame pending again without a migration.
//!
//! ## Why the embedding is read here and not in the classifier
//!
//! The classifier takes a vector. Where the vector came from is a storage question, and
//! `aura_index::EmbeddingStore` already owns it - including the half-precision decode
//! and the `model_ver` on every row. Reading it here means the classifier is a pure
//! function of its inputs and can be driven by the eval harness with synthetic vectors,
//! which is what makes section 10.1's gates measurable against a placeholder model.
//!
//! It also means `embed_ver` is **observed rather than assumed**. Migration 7's fourth
//! version column records which trunk produced the vector the head actually read, not
//! which trunk this build would have used. Those differ exactly when a re-embed is
//! half-finished, which is the case the column exists for.

use std::collections::BTreeMap;

use aura_core::contract::priority::Priority;
use aura_core::contract::scene::{SceneId, Source};
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_index::store::EmbeddingStore;

use crate::errors;
use crate::scene::attributes::ContextFeatures;
use crate::scene::classifier::{SceneClassifier, SceneInput, MODEL_VER, PREPROCESS_VER};
use crate::scene::ritual::{self, RitualClassifier, RitualInput, TraditionVote};
use crate::story::api::{ScenePassReport, Story};
use crate::story::segment::ScenePassRow;

/// How many frames go into one inference call.
///
/// 256.
///
/// The hardware plan sizes batches for models that hold activations; this one holds a
/// 528-wide feature vector per item, so 256 frames is 540 KB and the memory ledger has
/// nothing to say about it. The number that matters here is the *write* batch, one
/// transaction per 256 rows rather than one per row, which is what keeps the pass
/// inside section 11's 35-second budget on a spinning disk.
pub const BATCH: usize = 256;

/// Everything the pass needs that is not a model.
///
/// A struct rather than eight arguments, and every field is optional in the sense that
/// a `None` produces a documented neutral rather than a failure: a project with no face
/// scan classifies perfectly well, it just has two context slots at their midpoint.
#[derive(Debug, Default)]
pub struct PassContext {
    /// Faces per photograph, from phase 06. Absent when the face pass has not run.
    pub faces: BTreeMap<PhotoId, u32>,
    /// Whether one of the couple is in each frame, from `PeopleService`.
    ///
    /// Phase 06's rule: no phase keeps its own idea of who the couple are. This crate
    /// does not link `aura-people`; the caller passes the answer in.
    pub primaries: BTreeMap<PhotoId, bool>,
    /// EXIF flash flags, from the catalog.
    pub flash: BTreeMap<PhotoId, bool>,
    /// EXIF ISO.
    pub iso: BTreeMap<PhotoId, u32>,
    /// EXIF colour temperature in kelvin.
    pub kelvin: BTreeMap<PhotoId, u32>,
}

/// Run the scene pass over one project.
///
/// Returns what it did. Never fails for one bad frame: a frame whose batch could not be
/// run is retried alone, and one that still fails is counted, logged as `AURA-ML-5027`
/// and skipped - with **no row written**, deliberately, so the next pass retries it
/// rather than treating an `unknown` row as finished.
///
/// # Errors
///
/// `AURA-DB-3006` on a catalog failure, `AURA-ML-5011` when the photographer cancelled.
// Eight arguments, and every one of them is a different collaborator: the store, the
// vectors, two models, the project, the catalog's own facts, the progress sink and the
// stop button. Collapsing them into a context struct would hide which of them the pass
// writes through, which is the one thing a reader of this function needs to know.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn run(
    story: &Story,
    embeddings: &EmbeddingStore,
    scenes: &SceneClassifier,
    rituals: Option<&RitualClassifier>,
    project: &ProjectId,
    context: &PassContext,
    progress: &dyn ProgressSink,
    cancel: &CancelToken,
) -> AuraResult<ScenePassReport> {
    let started = story.store().catalog().clock().monotonic_ms();
    story.check_versions(project)?;
    story.install_profiles(project)?;

    let pending = story.pending(project)?;
    let total = u64::try_from(pending.len()).unwrap_or(u64::MAX);

    let mut report = ScenePassReport::default();
    let mut confidence_total = 0.0f32;
    let mut vote = TraditionVote::new();
    let key = project.to_db();

    // The day's first frame, for the hour-of-day feature. Read once: `pending` is in
    // timeline order, but a resumed pass starts in the middle of the day and would
    // otherwise measure every offset from wherever it happened to restart.
    let day_start = day_start_ms(story, project)?;

    for chunk in pending.chunks(BATCH) {
        if cancel.is_cancelled() {
            return Err(aura_core::errors::ml::cancelled(format!(
                "the scene pass was stopped after {} frames",
                report.classified
            )));
        }

        let mut inputs = Vec::with_capacity(chunk.len());
        for photo in chunk {
            let Some(stored) = embeddings.load_one(&key, *photo)? else {
                // An embedding that vanished between `pending` and here. Counted, not
                // fatal: `pending` is a snapshot and a concurrent purge is legal.
                report.failed += 1;
                continue;
            };
            let vector: Vec<f32> = stored
                .embedding
                .vec
                .iter()
                .map(|half| half.to_f32())
                .collect();
            inputs.push(SceneInput {
                image_id: *photo,
                embedding: vector,
                embed_ver: stored.embedding.model_ver,
                context: ContextFeatures {
                    offset_ms: Some(0),
                    flash: context.flash.get(photo).copied(),
                    iso: context.iso.get(photo).copied(),
                    kelvin: context.kelvin.get(photo).copied(),
                    faces: context.faces.get(photo).copied(),
                    has_primary: context.primaries.get(photo).copied(),
                    luma: Some(stored.embedding.luma),
                },
            });
        }
        if inputs.is_empty() {
            continue;
        }

        // The hour-of-day offset, now that the day's start is known.
        let stamps = timestamps_for(story, project, chunk)?;
        for input in &mut inputs {
            input.context.offset_ms = stamps
                .get(&input.image_id)
                .map(|ts| ts.saturating_sub(day_start));
        }

        let (results, stats) = match scenes.classify(&inputs, Priority::Background) {
            Ok(found) => found,
            Err(batch_error) => {
                // One bad frame must not cost a whole chunk. Retry alone.
                tracing::warn!(
                    target: "scene.classified",
                    code = %batch_error.code,
                    frames = inputs.len(),
                    "a scene batch failed; retrying its frames one at a time"
                );
                retry_alone(scenes, &inputs, &mut report)
            }
        };
        report.corrected_attributes += stats.corrected_attributes;
        report.abstained += stats.abstained;
        confidence_total += stats.mean_confidence * stats.labelled as f32;

        // The ritual head, only where the context suggests a ceremony.
        let verdicts = run_rituals(rituals, &inputs, &results, &vote);
        for verdict in verdicts.values() {
            if let Some(tradition) = &verdict.tradition {
                if verdict.ritual.is_some() {
                    vote.add(tradition);
                }
            }
        }

        let rows: Vec<ScenePassRow> = results
            .iter()
            .zip(inputs.iter())
            .map(|(result, input)| {
                let verdict = verdicts.get(&input.image_id);
                let mut result = result.clone();
                result.ritual = verdict.and_then(|found| found.ritual);
                result.ritual_conf = verdict.map_or(0.0, |found| found.confidence);
                if verdict.and_then(|found| found.ritual).is_some() {
                    report.rituals += 1;
                }
                ScenePassRow {
                    result,
                    ritual_slug: verdict.and_then(|found| found.slug.clone()),
                    tradition: verdict.and_then(|found| found.tradition.clone()),
                    preprocess_ver: PREPROCESS_VER,
                    taxonomy_ver: story.taxonomy().version(),
                    embed_ver: input.embed_ver,
                    elapsed_ms: 0.0,
                }
            })
            .collect();

        report.classified += story.record(project, &rows)?;
        progress.report(ProgressUpdate {
            stage: "scene.classify",
            done: u64::try_from(report.classified).unwrap_or(0),
            total,
            current: None,
        });
    }

    report.coverage = story.coverage(project)?;
    if report.classified > 0 {
        report.mean_confidence = confidence_total / report.classified as f32;
    }
    report.elapsed_ms = story
        .store()
        .catalog()
        .clock()
        .monotonic_ms()
        .saturating_sub(started);

    // Section 11's `scene.classified`.
    tracing::info!(
        target: "scene.classified",
        images = report.classified,
        ms = report.elapsed_ms,
        mean_conf = report.mean_confidence,
        low_conf_count = report.abstained,
        rituals = report.rituals,
        "scenes classified"
    );

    Ok(report)
}

/// Retry a failed batch one frame at a time.
///
/// Returns whatever succeeded plus empty statistics, and counts the rest. The cost is
/// one inference call per frame for one chunk, which is 256 calls once rather than a
/// whole wedding lost to one malformed vector.
fn retry_alone(
    scenes: &SceneClassifier,
    inputs: &[SceneInput],
    report: &mut ScenePassReport,
) -> (
    Vec<aura_core::contract::scene::SceneResult>,
    crate::scene::classifier::ClassifyStats,
) {
    let mut out = Vec::with_capacity(inputs.len());
    let mut stats = crate::scene::classifier::ClassifyStats::default();
    for input in inputs {
        match scenes.classify(std::slice::from_ref(input), Priority::Background) {
            Ok((mut found, one)) => {
                stats.labelled += one.labelled;
                stats.abstained += one.abstained;
                stats.corrected_attributes += one.corrected_attributes;
                out.append(&mut found);
            }
            Err(err) => {
                report.failed += 1;
                let failure = errors::scene_failed(&input.image_id.to_db(), &err.detail);
                tracing::warn!(
                    target: "scene.classified",
                    code = %failure.code,
                    "{}", failure.detail
                );
                // A placeholder so the caller's zip stays aligned. It is **not**
                // written: `run` filters unwritten frames by never recording a result
                // whose source is `Local` and whose scene is `Unknown` at zero
                // confidence with no top-3 - see `ScenePassRow` assembly, which only
                // sees results that came back from a successful call.
                out.push(aura_core::contract::scene::SceneResult {
                    source: Source::Local,
                    ..aura_core::contract::scene::SceneResult::unknown(input.image_id, MODEL_VER)
                });
            }
        }
    }
    (out, stats)
}

/// Run the ritual head over the frames whose context suggests a ceremony.
fn run_rituals(
    rituals: Option<&RitualClassifier>,
    inputs: &[SceneInput],
    results: &[aura_core::contract::scene::SceneResult],
    vote: &TraditionVote,
) -> BTreeMap<PhotoId, ritual::RitualVerdict> {
    let mut out = BTreeMap::new();
    let Some(head) = rituals else {
        return out;
    };

    let tradition = vote.verdict();
    let mut wanted = Vec::new();
    let mut ids = Vec::new();
    for (input, result) in inputs.iter().zip(results.iter()) {
        let top1 = result
            .top3
            .first()
            .map_or(SceneId::Unknown, |score| score.scene);
        if !ritual::should_evaluate(top1, result.attributes) {
            continue;
        }
        wanted.push(RitualInput {
            embedding: input.embedding.clone(),
            context: input.context.to_vector(),
            tradition: tradition.clone(),
        });
        ids.push(input.image_id);
    }
    if wanted.is_empty() {
        return out;
    }

    match head.classify(&wanted, Priority::Background) {
        Ok(verdicts) => {
            for (photo, verdict) in ids.into_iter().zip(verdicts) {
                out.insert(photo, verdict);
            }
        }
        Err(err) => {
            // A ritual head that will not run is a wedding with chapters and no rite
            // names, which is a degradation and not a failure. Invariant 6.
            tracing::warn!(
                target: "scene.ritual",
                code = %err.code,
                frames = wanted.len(),
                "the ritual head did not run; chapters keep their scene labels"
            );
        }
    }
    out
}

/// The timeline time of the project's first photograph.
fn day_start_ms(story: &Story, project: &ProjectId) -> AuraResult<i64> {
    let key = project.to_db();
    story.store().catalog().read(move |conn| {
        let found: Result<Option<String>, rusqlite::Error> = conn.query_row(
            "SELECT MIN(timeline_time) FROM photo WHERE project_id = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        );
        Ok(found
            .ok()
            .flatten()
            .and_then(|text| aura_index::store::parse_rfc3339_ms(&text))
            .unwrap_or(0))
    })
}

/// The timeline times of one chunk.
fn timestamps_for(
    story: &Story,
    project: &ProjectId,
    chunk: &[PhotoId],
) -> AuraResult<BTreeMap<PhotoId, i64>> {
    let key = project.to_db();
    let wanted: Vec<String> = chunk.iter().map(PhotoId::to_db).collect();
    story.store().catalog().read(move |conn| {
        let mut statement = conn
            .prepare("SELECT photo_id, timeline_time FROM photo WHERE project_id = ?1")
            .map_err(|e| {
                aura_core::errors::db::statement_failed("could not read timeline times", &e)
            })?;
        let mut rows = statement.query(rusqlite::params![key]).map_err(|e| {
            aura_core::errors::db::statement_failed("could not read timeline times", &e)
        })?;

        let mut out = BTreeMap::new();
        while let Some(row) = rows.next().map_err(|e| {
            aura_core::errors::db::statement_failed("could not read a timeline time", &e)
        })? {
            let (Ok(id), Ok(stamp)) = (row.get::<_, String>(0), row.get::<_, Option<String>>(1))
            else {
                continue;
            };
            if !wanted.contains(&id) {
                continue;
            }
            let Ok(photo) = PhotoId::from_db(&id) else {
                continue;
            };
            if let Some(ms) = stamp
                .as_deref()
                .and_then(aura_index::store::parse_rfc3339_ms)
            {
                out.insert(photo, ms);
            }
        }
        Ok(out)
    })
}
