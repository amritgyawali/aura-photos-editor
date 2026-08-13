//! The similarity half of the command surface.
//!
//! Five commands, and the same three rules the preview and hardware commands
//! follow.
//!
//! **Nothing here blocks for long, except the one command that says it does.**
//! `find_similar`, `index_status` and `image_descriptors` read memory or one
//! catalog row. `build_index` costs the build budget - under 400 ms for a
//! 4,000-image project - and `embed_project` is a batch pass that returns what it
//! did; the UI calls it from a job, not from a click handler, and it is resumable
//! so a cancel costs nothing.
//!
//! **The panel tells the truth about what is missing.** An empty neighbour list on
//! a project that was never embedded is a support ticket, so `IndexStatusDto`
//! carries coverage and the stale model versions rather than making the UI infer
//! them from silence.
//!
//! **No vector crosses the boundary.** There is no `get_embedding`: 512 halves per
//! image across a 4,000-cell grid is megabytes of JSON per screen, and a web view
//! has no use for a number it cannot compare. The comparison happens here.

use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority};
use aura_index::contract::index::{IndexFilter, SimilarityIndex};
use aura_index::query::find_similar as run_query;
use aura_vision::embed::model::MODEL_VER;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    DescriptorsDto, EmbedProgressDto, EmbedProjectInput, FindSimilarInput, IndexStatusDto,
    SimilarNeighbourDto, SimilarResultDto,
};
use crate::state::AppState;

/// What looks like this photograph.
///
/// The index is built on first use and kept, so the first call after a project
/// opens pays the build and every call after it is a graph search.
///
/// # Errors
///
/// `AURA-DB-3006` when the project's vectors cannot be read, and
/// `AURA-ML-5013` when the query frame itself has no usable vector - which is the
/// honest answer to "what looks like this?" for a frame nothing could read.
pub fn find_similar(state: &AppState, input: &FindSimilarInput) -> IpcResult<SimilarResultDto> {
    let index = state.similarity_index(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;

    let Some(query) = state.embedding_of(&input.project_id, photo)? else {
        return Err(aura_index::errors::embed_degenerate(
            &input.photo_id,
            "this photograph has not been analysed yet",
        )
        .into());
    };

    // The exclusion set is built here rather than in the UI so that a bad id is a
    // skipped entry rather than a failed query: a panel that had just deleted a
    // frame should not get an error for mentioning it.
    let excluded: Vec<PhotoId> = input
        .exclude
        .iter()
        .filter_map(|text| PhotoId::from_db(text).ok())
        .collect();

    let mut filter = IndexFilter::none().excluding(&excluded);
    filter.time_window_s = input.time_window_s;
    if let Some(camera) = &input.camera_id {
        // A camera the index has never seen means "no frames from that camera",
        // which is an empty result rather than an unfiltered one.
        match index.camera_handle(camera) {
            Some(handle) => filter.camera_id = Some(handle),
            None => {
                return Ok(SimilarResultDto {
                    photo_id: input.photo_id.clone(),
                    neighbours: Vec::new(),
                    elapsed_ms: 0.0,
                    filter_kind: "camera".to_string(),
                })
            }
        }
    }

    let k = usize::try_from(input.k).unwrap_or(32).clamp(1, 500);
    let answer = run_query(&index, &query, k, filter, state.clock().as_ref());

    // Section 11: `index.query` {k, ms, filter_kind}.
    tracing::info!(
        target: "index.query",
        k = input.k,
        ms = answer.elapsed_ms(),
        filter_kind = answer.filter_kind,
        "similarity query"
    );

    Ok(SimilarResultDto {
        photo_id: input.photo_id.clone(),
        neighbours: answer
            .neighbours
            .iter()
            .map(|neighbour| SimilarNeighbourDto {
                photo_id: neighbour.id.to_db(),
                distance: neighbour.distance,
                similarity: neighbour.similarity,
                dhash_distance: neighbour.dhash_distance,
                near_duplicate: neighbour.near_duplicate,
            })
            .collect(),
        elapsed_ms: answer.elapsed_ms(),
        filter_kind: answer.filter_kind.to_string(),
    })
}

/// What the index is holding, and what it is missing.
///
/// # Errors
///
/// `AURA-DB-3006` when the coverage view or the vectors cannot be read.
pub fn index_status(state: &AppState, project_id: &str) -> IpcResult<IndexStatusDto> {
    let index = state.similarity_index(project_id)?;
    let store = state.embedding_store();
    let (photos, embedded) = store.coverage(project_id)?;
    let versions = store.model_versions(project_id)?;
    let holding = index.stats();

    Ok(IndexStatusDto {
        vectors: u32::try_from(holding.vectors).unwrap_or(u32::MAX),
        photos: u32::try_from(photos).unwrap_or(u32::MAX),
        // Per-mille integer arithmetic, then one lossless widening. Both counts are
        // `i64`, and every direct cast from one to a float is a precision loss the
        // compiler is right to object to - even though four thousand fits
        // comfortably, the next reader should not have to check that it does.
        coverage: if photos <= 0 {
            0.0
        } else {
            let per_mille = (embedded.saturating_mul(1_000) / photos).clamp(0, 1_000);
            f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
        },
        filterable: u32::try_from(holding.filterable).unwrap_or(u32::MAX),
        model_ver: u32::from(holding.model_ver),
        stale_model_versions: versions
            .into_iter()
            .filter(|version| *version != MODEL_VER)
            .map(u32::from)
            .collect(),
        build_ms: holding.build_ms,
        from_snapshot: holding.from_snapshot,
    })
}

/// Throw the graph away and build it again.
///
/// The button behind a model update: a new embedding version makes every stored
/// vector incomparable with a new one, and the honest response is to rebuild
/// rather than to compare across versions. Also the escape hatch when a snapshot
/// is suspected.
///
/// # Errors
///
/// `AURA-DB-3006` when the vectors cannot be read.
pub fn build_index(state: &AppState, project_id: &str) -> IpcResult<IndexStatusDto> {
    state.rebuild_similarity_index(project_id)?;
    index_status(state, project_id)
}

/// Embed every photograph in a project that has no current vector.
///
/// Resumable by construction: what is left to do is a query, so a cancelled pass
/// and a killed process leave the same state behind and the next call continues.
///
/// # Errors
///
/// `AURA-DB-3006` when the pending set cannot be read, and whatever the model
/// loader raised on first use. Per-photograph failures are counted in the result
/// rather than returned - one unreadable frame must not end a 4,000-image pass.
pub fn embed_project(state: &AppState, input: &EmbedProjectInput) -> IpcResult<EmbedProgressDto> {
    let runner = state.embedding_runner(&input.project_id)?;
    let report = runner.embed_project(
        &input.project_id,
        Priority::AiBatch,
        &CancelToken::new(),
        &NullProgress,
    )?;
    let remaining = state
        .embedding_store()
        .pending(&input.project_id, MODEL_VER)?
        .len();

    // A pass that embedded anything invalidates the graph in memory: the point of
    // embedding is to be searchable, and a caller that had to remember to rebuild
    // would sometimes forget.
    if report.embedded > 0 {
        state.rebuild_similarity_index(&input.project_id)?;
    }

    Ok(EmbedProgressDto {
        embedded: u32::try_from(report.embedded).unwrap_or(u32::MAX),
        failed: u32::try_from(report.failed).unwrap_or(u32::MAX),
        remaining: u32::try_from(remaining).unwrap_or(u32::MAX),
        elapsed_ms: report.elapsed_ms,
        batches: u32::try_from(report.batches).unwrap_or(u32::MAX),
        cancelled: report.cancelled,
    })
}

/// The cheap descriptors of one photograph, for the debug panel's readout.
///
/// # Errors
///
/// `AURA-DB-3006` when the row cannot be read, and `AURA-ML-5013` when the
/// photograph has not been analysed.
pub fn image_descriptors(
    state: &AppState,
    project_id: &str,
    photo_id: &str,
) -> IpcResult<DescriptorsDto> {
    let photo = parse_photo(photo_id)?;
    let Some(row) = state.stored_embedding(project_id, photo)? else {
        return Err(aura_index::errors::embed_degenerate(
            photo_id,
            "this photograph has not been analysed yet",
        )
        .into());
    };

    Ok(DescriptorsDto {
        photo_id: photo_id.to_string(),
        dhash_hex: format!("{:016x}", row.embedding.dhash),
        luma_mean: row.embedding.luma.mean,
        luma_p1: row.embedding.luma.p1,
        luma_p50: row.embedding.luma.p50,
        luma_p99: row.embedding.luma.p99,
        clip_lo: row.embedding.luma.clip_lo,
        clip_hi: row.embedding.luma.clip_hi,
        edge_energy: row.descriptors.edge_energy,
        palette: row
            .descriptors
            .palette
            .iter()
            .map(|colour| {
                format!(
                    "#{:02x}{:02x}{:02x}",
                    colour.first().copied().unwrap_or(0),
                    colour.get(1).copied().unwrap_or(0),
                    colour.get(2).copied().unwrap_or(0)
                )
            })
            .collect(),
        model_ver: u32::from(row.embedding.model_ver),
        pixel_source: format!("tier {} {}", row.pixel_tier, row.pixel_source),
    })
}

/// Parse a photograph id, or refuse with a coded error rather than a panic.
fn parse_photo(text: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(text).map_err(|_| {
        crate::contract::ipc::IpcError::from(aura_core::errors::db::statement_failed(
            "photo id was not a valid AURA id",
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}
