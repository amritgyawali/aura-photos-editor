//! Full-quality editing previews, cached so each one is rendered once. ADR-0097.
//!
//! The editing views show the photograph at its own resolution, decoded from the original
//! file - never the 2048-pixel JPEG proxy the library uses for speed. Rendering a retouched
//! photograph at full resolution is slow, so every finished preview is kept twice:
//!
//! - **in memory**, for the session, so moving between sections and coming back is instant;
//! - **on disk**, under the cache root, so it is still instant after a restart.
//!
//! Both are keyed by the photograph, the original's content hash, the resolution asked for
//! and the renderer's own hash of the request (canonical recipe, engine and output). A
//! changed edit, a changed original or a new engine is a different key, so a stale preview is
//! never shown; old entries simply age out of the size budgets. Delivery and analysis never
//! come through here: they render the original directly, every time.
//!
//! A full-quality preview runs every stage, exactly as an export does - the interactive
//! path's permission to skip heavy stages until zoom is not used - so what is shown is what
//! will be delivered, apart from the crop the retouch view leaves off.
//!
//! [`Quality::Fast`] is the screen-sized first look from the proxy, for the moment before the
//! full-quality preview is ready and for live drafts while a brush is moving.
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_core::AuraResult;
use aura_render::{
    OutputColour, RenderLevel, RenderNote, RenderPurpose, RenderRequest, RenderService,
    RenderedData, RenderedImage,
};
use serde::{Deserialize, Serialize};

/// The size of the fast first look.
const FAST_EDGE: u32 = 1600;
/// Finished previews compete with active rendering, model inference and the `WebView`.
/// Keep a modest memory budget; larger and older previews remain available on disk.
const MEMORY_BYTES: usize = 128 * 1024 * 1024;
const MEMORY_ENTRIES: usize = 48;
/// Disk kept for finished previews. The oldest are removed first.
const DISK_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// The folder under the cache root; the version is bumped when the file format changes.
const DISK_DIR: &str = "edited-previews-v1";
const MAGIC: &[u8; 8] = b"AURAEP01";

/// How sharp an editing preview is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// The original's own resolution, decoded from the original file.
    Full,
    /// A screen-sized first look from the proxy.
    Fast,
}

impl Quality {
    /// `"fast"` is the first look; anything else, including nothing, is full quality.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("fast") => Self::Fast,
            _ => Self::Full,
        }
    }

    /// The render level this quality asks for.
    #[must_use]
    pub const fn level(self) -> RenderLevel {
        match self {
            Self::Full => RenderLevel::Full,
            Self::Fast => RenderLevel::Screen(FAST_EDGE, FAST_EDGE),
        }
    }
}

/// Finished previews held in memory, least recently used first.
#[derive(Debug, Default)]
pub(crate) struct Memory {
    entries: VecDeque<(String, Arc<RenderedImage>)>,
    bytes: usize,
}

fn size(image: &RenderedImage) -> usize {
    match &image.data {
        RenderedData::Eight(bytes) => bytes.len(),
        RenderedData::Sixteen(words) => words.len().saturating_mul(2),
    }
}

impl Memory {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    fn get(&mut self, key: &str) -> Option<Arc<RenderedImage>> {
        let index = self.entries.iter().position(|(saved, _)| saved == key)?;
        let entry = self.entries.remove(index)?;
        let image = Arc::clone(&entry.1);
        self.entries.push_back(entry);
        Some(image)
    }

    fn insert(&mut self, key: String, image: Arc<RenderedImage>) {
        let bytes = size(&image);
        if bytes > MEMORY_BYTES {
            return;
        }
        if let Some(index) = self.entries.iter().position(|(saved, _)| *saved == key) {
            if let Some((_, previous)) = self.entries.remove(index) {
                self.bytes = self.bytes.saturating_sub(size(&previous));
            }
        }
        while self.bytes.saturating_add(bytes) > MEMORY_BYTES
            || self.entries.len() >= MEMORY_ENTRIES
        {
            let Some((_, previous)) = self.entries.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(size(&previous));
        }
        self.bytes += bytes;
        self.entries.push_back((key, image));
    }
}

/// Everything about a finished preview except its pixels, as stored on disk.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    width: u32,
    height: u32,
    colour_space: OutputColour,
    render_hash: String,
    backend: String,
    notes: Vec<RenderNote>,
    stages_run: Vec<String>,
}

fn disk_dir(state: &crate::AppState) -> PathBuf {
    state.cache_root().join(DISK_DIR)
}

fn disk_path(state: &crate::AppState, key: &str) -> PathBuf {
    disk_dir(state).join(format!("{key}.bin"))
}

fn read_disk(path: &Path, now: std::time::SystemTime) -> Option<RenderedImage> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic).ok()?;
    if &magic != MAGIC {
        return None;
    }
    let mut length = [0_u8; 4];
    file.read_exact(&mut length).ok()?;
    let mut header = vec![0_u8; u32::from_le_bytes(length) as usize];
    file.read_exact(&mut header).ok()?;
    let header: Header = serde_json::from_slice(&header).ok()?;
    let expected = (header.width as usize)
        .checked_mul(header.height as usize)?
        .checked_mul(3)?;
    let mut pixels = Vec::with_capacity(expected);
    file.read_to_end(&mut pixels).ok()?;
    if pixels.len() != expected {
        return None;
    }
    // Reading counts as use, so the disk budget removes what has not been looked at longest.
    if let Ok(touch) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = touch.set_modified(now);
    }
    Some(RenderedImage {
        width: header.width,
        height: header.height,
        data: RenderedData::Eight(pixels),
        colour_space: header.colour_space,
        render_hash: header.render_hash,
        backend: header.backend,
        notes: header.notes,
        stages_run: header.stages_run,
        ms: 0,
        cache_hit: true,
    })
}

fn write_disk(dir: &Path, path: &Path, image: &RenderedImage) -> std::io::Result<()> {
    let RenderedData::Eight(pixels) = &image.data else {
        return Ok(());
    };
    std::fs::create_dir_all(dir)?;
    let header = serde_json::to_vec(&Header {
        width: image.width,
        height: image.height,
        colour_space: image.colour_space,
        render_hash: image.render_hash.clone(),
        backend: image.backend.clone(),
        notes: image.notes.clone(),
        stages_run: image.stages_run.clone(),
    })
    .map_err(std::io::Error::other)?;
    let length = u32::try_from(header.len()).map_err(std::io::Error::other)?;
    // Written beside its final name and renamed into place, so a reader never sees half a file.
    let partial = path.with_extension("part");
    {
        let mut file = std::io::BufWriter::new(std::fs::File::create(&partial)?);
        file.write_all(MAGIC)?;
        file.write_all(&length.to_le_bytes())?;
        file.write_all(&header)?;
        file.write_all(pixels)?;
        file.flush()?;
    }
    std::fs::rename(&partial, path)
}

/// Free space the preview cache never takes from the disk it lives on: a full system disk is
/// a far worse problem than a preview rendered again.
const DISK_RESERVE: u64 = 2 * 1024 * 1024 * 1024;

/// Make room for a file of `incoming` bytes: the cache stays inside [`DISK_BYTES`] and leaves
/// [`DISK_RESERVE`] free on its disk, removing the least recently used files first. False when
/// there is no room even with the cache empty, and the preview is then kept in memory only.
fn make_room(dir: &Path, incoming: u64) -> bool {
    make_room_with(dir, incoming, fs4::available_space(dir).unwrap_or(0))
}

/// [`make_room`] with the disk's free space given.
fn make_room_with(dir: &Path, incoming: u64, free: u64) -> bool {
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let meta = entry.metadata().ok()?;
                    if !meta.is_file() {
                        return None;
                    }
                    Some((meta.modified().ok()?, meta.len(), entry.path()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    let limit = DISK_BYTES.min((total + free).saturating_sub(DISK_RESERVE));
    if incoming > limit {
        return false;
    }
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
    for (_, len, path) in files {
        if total + incoming <= limit {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
    total + incoming <= limit
}

/// Remove every cached editing preview, in memory and on disk.
pub(crate) fn clear(state: &crate::AppState) {
    state.edited_previews().lock().clear();
    state.clear_retouch_checkpoints();
    let _ = std::fs::remove_dir_all(disk_dir(state));
}

/// Render an editing preview at `level`, or return the one already rendered for exactly this
/// request. The full level runs every stage, as an export does; smaller levels are the
/// interactive path. Eight-bit output only: that is what a preview displays. `persist` also
/// keeps it on disk; an unsaved draft is held for the session only.
///
/// # Errors
/// Invalid recipe, missing original, or failed rendering.
pub(crate) fn render(
    state: &crate::AppState,
    image_id: aura_core::PhotoId,
    recipe: aura_recipe::Recipe,
    level: RenderLevel,
    colour_space: OutputColour,
    persist: bool,
) -> AuraResult<RenderedImage> {
    let engine = state.render()?;
    let request = RenderRequest {
        image_id,
        recipe,
        level,
        output: aura_render::OutputSpec {
            colour_space,
            bit_depth: 8,
            icc: None,
        },
        purpose: if level == RenderLevel::Full {
            RenderPurpose::Export
        } else {
            RenderPurpose::Interactive
        },
    };
    aura_recipe::schema::Validation::check(&request.recipe)?;
    let level = match request.level {
        RenderLevel::Screen(w, h) => format!("screen-{w}x{h}"),
        other => other.as_str().to_owned(),
    };
    let purpose = request.purpose.as_str();
    // The original's own hash, not only the recipe's copy of it: a photograph whose recipe
    // was never saved carries a placeholder there.
    let content = state
        .photo_content_hash(request.image_id)
        .unwrap_or_default();
    let key = blake3::hash(
        format!(
            "{}|{content}|{level}|{purpose}|{}",
            request.image_id.to_db(),
            engine.render_hash(&request)?
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    if let Some(image) = state.edited_previews().lock().get(&key) {
        let mut image = (*image).clone();
        image.cache_hit = true;
        // No time was spent rendering it this time.
        image.ms = 0;
        return Ok(image);
    }
    let path = disk_path(state, &key);
    if let Some(image) = persist
        .then(|| read_disk(&path, std::time::SystemTime::from(state.clock().now_utc())))
        .flatten()
    {
        state
            .edited_previews()
            .lock()
            .insert(key, Arc::new(image.clone()));
        return Ok(image);
    }
    // The cache lock is never held while rendering.
    let image = engine.render(request)?;
    state
        .edited_previews()
        .lock()
        .insert(key, Arc::new(image.clone()));
    if !persist {
        return Ok(image);
    }
    let dir = disk_dir(state);
    let room = std::fs::create_dir_all(&dir).is_ok()
        && make_room(&dir, u64::try_from(size(&image)).unwrap_or(u64::MAX));
    if room {
        if let Err(error) = write_disk(&dir, &path, &image) {
            tracing::warn!(target: "cache.stats", detail = %error, "editing preview cache write failed");
        }
    }
    Ok(image)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn image(bytes: usize) -> RenderedImage {
        RenderedImage {
            width: 1,
            height: 1,
            data: RenderedData::Eight(vec![7; bytes]),
            colour_space: OutputColour::Srgb,
            render_hash: "hash".into(),
            backend: "cpu".into(),
            notes: Vec::new(),
            stages_run: vec!["retouch".into()],
            ms: 12,
            cache_hit: false,
        }
    }

    #[test]
    fn memory_is_bounded_and_keeps_what_was_used_last() {
        let mut memory = Memory::default();
        for n in 0..MEMORY_ENTRIES {
            memory.insert(n.to_string(), Arc::new(image(3)));
        }
        assert!(memory.get("0").is_some());
        memory.insert("new".into(), Arc::new(image(3)));
        assert!(
            memory.get("1").is_none(),
            "the least recently used goes first"
        );
        assert!(memory.get("0").is_some());
        memory.insert("huge".into(), Arc::new(image(MEMORY_BYTES + 1)));
        assert!(memory.get("huge").is_none());
        assert!(memory.bytes <= MEMORY_BYTES);
    }

    #[test]
    fn a_preview_survives_the_round_trip_to_disk_and_a_damaged_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.bin");
        let mut original = image(3);
        original.notes.push(RenderNote {
            stage: "lens".into(),
            reason: aura_render::SkipReason::NotRequested,
            detail: Some("no profile".into()),
        });
        write_disk(dir.path(), &path, &original).unwrap();
        let read = read_disk(&path, std::time::SystemTime::UNIX_EPOCH).unwrap();
        assert!(read.cache_hit);
        assert_eq!(read.data, original.data);
        assert_eq!(read.notes, original.notes);
        assert_eq!(read.render_hash, original.render_hash);
        // A truncated file is a miss, never a wrong picture.
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
        assert!(read_disk(&path, std::time::SystemTime::UNIX_EPOCH).is_none());
    }

    #[test]
    fn the_disk_cache_never_takes_more_than_its_budget_or_the_reserve() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.bin"), [0_u8; 16]).unwrap();
        let plenty = 100 * 1024 * 1024 * 1024;
        assert!(
            make_room_with(dir.path(), 16, plenty),
            "a small preview fits"
        );
        assert!(
            !make_room_with(dir.path(), DISK_BYTES + 1, plenty),
            "nothing above the budget is written"
        );
        // A nearly full disk: the reserve stays free, and what the cache already holds is
        // given up first.
        assert!(!make_room_with(dir.path(), 64, DISK_RESERVE));
        assert!(make_room_with(dir.path(), 16, DISK_RESERVE));
        assert!(
            !dir.path().join("old.bin").exists(),
            "the oldest preview made room"
        );
    }

    #[test]
    fn quality_defaults_to_full() {
        assert_eq!(Quality::parse(None), Quality::Full);
        assert_eq!(Quality::parse(Some("full")), Quality::Full);
        assert_eq!(Quality::parse(Some("fast")), Quality::Fast);
        assert_eq!(Quality::Full.level(), RenderLevel::Full);
    }
}
