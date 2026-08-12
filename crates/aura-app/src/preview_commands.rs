//! The preview half of the command surface.
//!
//! Three rules shape these commands.
//!
//! **No command blocks the UI thread for long.** `get_preview` is allowed to
//! decode inline because a visible cell is exactly the work that must not be
//! queued behind a batch, and the preview service reserves a core for it. Every
//! other path - the 4,000-image proxy pass - goes through `prefetch_previews`
//! and returns immediately.
//!
//! **Pixels cross the boundary as a `data:` URL.** A JSON array of 800,000
//! numbers costs eight times the bytes and a great deal of garbage collection;
//! a base64 JPEG can be assigned straight to `img.src`.
//!
//! **A failure is a Problems row, not a dialog.** Anything the decoder
//! quarantines is already recorded in the catalog by the service, so these
//! commands only have to translate the error.

use aura_cache::CacheBudget;
use aura_preview::contract::service::PreviewService;
use aura_preview::{ImageId, Priority};
use aura_raw::contract::pixels::PixelLevel;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    CacheStatsDto, GetPreviewInput, IpcError, PrefetchInput, PreviewPayload, SetCacheBudgetInput,
};
use crate::state::AppState;

/// The long edge of the thumbnail the grid asks for.
const GRID_THUMB_EDGE: u32 = 512;

fn parse_level(text: &str) -> PixelLevel {
    match text {
        "proxy" | "proxy2048" => PixelLevel::Proxy2048,
        // A full-resolution render is gigabytes and never crosses IPC; asking
        // for one from the UI gets the proxy, which is what the loupe wants.
        _ => PixelLevel::Thumb(GRID_THUMB_EDGE),
    }
}

fn parse_priority(text: &str) -> Priority {
    match text {
        "interactive" => Priority::Interactive,
        "aiBatch" | "ai_batch" => Priority::AiBatch,
        "background" => Priority::Background,
        _ => Priority::Visible,
    }
}

fn parse_photo(photo_id: &str) -> Result<ImageId, IpcError> {
    ImageId::from_db(photo_id).map_err(|_| {
        IpcError::from(aura_core::errors::db::statement_failed(
            "photo id was not a valid AURA id",
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}

/// Get the pixels of one photograph, decoding them if necessary.
///
/// # Errors
///
/// The decoder's code (`AURA-RAW-2001` to `AURA-RAW-2008`) when the photograph
/// cannot be rendered, or the catalog error when it cannot be located.
pub fn get_preview(state: &AppState, input: &GetPreviewInput) -> IpcResult<PreviewPayload> {
    let id = parse_photo(&input.photo_id)?;
    let level = parse_level(&input.level);
    let service = state.previews(&input.project_id)?;
    let buffer = service.get(id, level, parse_priority(&input.priority))?;

    let jpeg = match buffer.as_srgb8() {
        Some(pixels) => aura_raw::codec::encode_jpeg(
            &aura_raw::codec::Rgb8 {
                width: buffer.width,
                height: buffer.height,
                data: pixels.to_vec(),
            },
            85,
        )?,
        None => {
            return Err(IpcError::from(aura_core::errors::raw::corrupt(
                "preview buffer is not 8-bit sRGB",
            )))
        }
    };

    Ok(PreviewPayload {
        photo_id: input.photo_id.clone(),
        tier: level.tier(),
        width: i64::from(buffer.width),
        height: i64::from(buffer.height),
        source: buffer.source.as_str().to_string(),
        data_url: format!("data:image/jpeg;base64,{}", base64(&jpeg)),
    })
}

/// Ask for a batch of photographs to be produced in the background.
///
/// Returns the number of requests actually queued: a full queue drops
/// speculative work rather than growing, and the UI is told so it can ask again
/// as the user scrolls.
///
/// # Errors
///
/// `AURA-IO-1009` when the cache cannot be opened.
pub fn prefetch_previews(state: &AppState, input: &PrefetchInput) -> IpcResult<i64> {
    let service = state.previews(&input.project_id)?;
    let level = parse_level(&input.level);
    let ids: Vec<ImageId> = input
        .photo_ids
        .iter()
        .filter_map(|id| ImageId::from_db(id).ok())
        .collect();
    let before = service.queued();
    service.prefetch(&ids, level);
    Ok(i64::try_from(service.queued().saturating_sub(before)).unwrap_or(0))
}

/// Abandon queued work for cells that have scrolled out of view.
///
/// # Errors
///
/// `AURA-IO-1009` when the cache cannot be opened.
pub fn cancel_previews(state: &AppState, project_id: &str, photo_ids: &[String]) -> IpcResult<i64> {
    let service = state.previews(project_id)?;
    let ids: Vec<ImageId> = photo_ids
        .iter()
        .filter_map(|id| ImageId::from_db(id).ok())
        .collect();
    Ok(i64::try_from(service.cancel(&ids)).unwrap_or(0))
}

/// What the cache is doing, for the settings panel.
///
/// # Errors
///
/// `AURA-IO-1009` when the cache cannot be opened.
pub fn preview_stats(state: &AppState, project_id: &str) -> IpcResult<CacheStatsDto> {
    let previews = state.previews(project_id)?;
    let counters = previews.cache_stats();
    Ok(CacheStatsDto {
        bytes_used: counters.bytes_used,
        budget_bytes: counters.budget_bytes,
        entries: counters.entries,
        hits: counters.hits,
        misses: counters.misses,
        evictions: counters.evictions,
        hit_rate: counters.hit_rate(),
    })
}

/// Change the cache ceiling and evict down to it immediately.
///
/// # Errors
///
/// `AURA-IO-1009` when the cache cannot be opened.
pub fn set_cache_budget(state: &AppState, input: &SetCacheBudgetInput) -> IpcResult<CacheStatsDto> {
    let service = state.previews(&input.project_id)?;
    service.set_cache_budget(CacheBudget::clamped(input.budget_bytes));
    preview_stats(state, &input.project_id)
}

/// Delete every cached preview for a project. Nothing is lost.
///
/// # Errors
///
/// `AURA-IO-1009` when the cache folder cannot be recreated.
pub fn purge_cache(state: &AppState, project_id: &str) -> IpcResult<CacheStatsDto> {
    let service = state.previews(project_id)?;
    service.purge_cache()?;
    preview_stats(state, project_id)
}

/// Photographs that could not be decoded, with a photographer-facing reason.
///
/// # Errors
///
/// `AURA-IO-1009` when the cache cannot be opened.
pub fn preview_problems(state: &AppState, project_id: &str) -> IpcResult<Vec<(String, String)>> {
    let service = state.previews(project_id)?;
    Ok(service
        .quarantine()
        .into_iter()
        .map(|(id, reason)| (id.to_db(), reason))
        .collect())
}

/// Standard base64, written here rather than pulled in as a dependency: it is
/// twenty lines, it has no configuration, and a supply-chain review of a crate
/// that does this much is not a good use of anyone's afternoon.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let symbol = |index: u32| -> char {
        ALPHABET
            .get(index as usize & 0x3F)
            .map_or('=', |byte| char::from(*byte))
    };

    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk.first().copied().unwrap_or(0));
        let second = u32::from(chunk.get(1).copied().unwrap_or(0));
        let third = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (first << 16) | (second << 8) | third;

        out.push(symbol(triple >> 18));
        out.push(symbol(triple >> 12));
        out.push(if chunk.len() > 1 {
            symbol(triple >> 6)
        } else {
            '='
        });
        out.push(if chunk.len() > 2 { symbol(triple) } else { '=' });
    }
    out
}
