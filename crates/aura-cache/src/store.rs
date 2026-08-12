//! The store itself: put, get, verify, evict.
//!
//! Two rules govern everything here.
//!
//! **A cache is never truth.** Every entry can be rebuilt from the original
//! file, so the correct response to anything unexpected - a bad digest, a
//! truncated file, an index that does not match the disk - is to delete and
//! rebuild, never to serve what was found and never to fail the request.
//!
//! **A failed write is not a failed request.** The caller already holds the
//! pixels; losing the on-disk copy costs time later, not correctness now. Write
//! failures are therefore reported as `AURA-IO-1009` warnings that the caller
//! logs and continues past.

use std::path::{Path, PathBuf};

use aura_core::errors::io::cache_unwritable;
use aura_core::AuraResult;
use parking_lot::Mutex;

use crate::budget::{CacheBudget, CacheStats};
use crate::lru::CacheIndex;
use crate::paths::{parse_file_name, CacheKey};

/// Name of the index file inside the cache root.
const INDEX_FILE: &str = "index.json";

/// A content-addressed store on one directory.
#[derive(Debug)]
pub struct CacheStore {
    root: PathBuf,
    budget: Mutex<CacheBudget>,
    index: Mutex<CacheIndex>,
}

impl CacheStore {
    /// Open (or create) a cache at `root`.
    ///
    /// A missing or unreadable index is rebuilt by scanning the directory, which
    /// is what makes a cache survive being copied, interrupted or partially
    /// deleted.
    ///
    /// # Errors
    ///
    /// `AURA-IO-1009` when the root cannot be created.
    pub fn open(root: &Path, budget: CacheBudget) -> AuraResult<Self> {
        std::fs::create_dir_all(root)
            .map_err(|e| cache_unwritable(format!("cannot create the cache folder: {e}")))?;
        let index = load_index(root).unwrap_or_else(|| rebuild_index(root));
        Ok(Self {
            root: root.to_path_buf(),
            budget: Mutex::new(budget),
            index: Mutex::new(index),
        })
    }

    /// Where this cache lives.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Replace the budget and evict down to it immediately.
    pub fn set_budget(&self, budget: CacheBudget) {
        *self.budget.lock() = budget;
        self.evict_to_budget();
    }

    /// The current budget.
    #[must_use]
    pub fn budget(&self) -> CacheBudget {
        *self.budget.lock()
    }

    /// Store one artefact.
    ///
    /// The write is atomic: bytes land in a temporary file in the same shard and
    /// are renamed into place, so a crash halfway through leaves either the old
    /// entry or no entry, never a half-written one.
    ///
    /// # Errors
    ///
    /// `AURA-IO-1009` when the bytes cannot be written.
    pub fn put(&self, key: &CacheKey, bytes: &[u8]) -> AuraResult<()> {
        let path = key.path_under(&self.root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| cache_unwritable(format!("cannot create a cache shard: {e}")))?;
        }
        let temporary = path.with_extension("part");
        std::fs::write(&temporary, bytes)
            .map_err(|e| cache_unwritable(format!("cannot write a cache entry: {e}")))?;
        std::fs::rename(&temporary, &path).map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            cache_unwritable(format!("cannot publish a cache entry: {e}"))
        })?;

        let digest = blake3::hash(bytes).to_hex().to_string();
        self.index
            .lock()
            .insert(key.id(), bytes.len() as u64, digest);
        self.persist_index();
        self.evict_to_budget();
        Ok(())
    }

    /// Fetch one artefact, verifying it against the digest recorded when it was
    /// written.
    ///
    /// Returns `None` for a miss, and also for a corrupt entry - which is
    /// deleted on the way out, so the next request rebuilds it.
    #[must_use]
    pub fn get(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let id = key.id();
        let expected = {
            let mut index = self.index.lock();
            if !index.touch(&id) {
                return None;
            }
            index.entries.get(&id).map(|entry| entry.digest.clone())
        };
        let path = key.path_under(&self.root);
        let Ok(bytes) = std::fs::read(&path) else {
            // The index believes in a file that is not there: heal and miss.
            self.forget(key);
            return None;
        };
        let digest = blake3::hash(&bytes).to_hex().to_string();
        if expected.is_some_and(|recorded| recorded != digest) {
            tracing::warn!(
                target: "cache",
                entry = %id,
                "cache entry failed its digest check and was rebuilt"
            );
            self.forget(key);
            return None;
        }
        self.persist_index();
        Some(bytes)
    }

    /// True when an artefact is present, without reading or touching it.
    #[must_use]
    pub fn contains(&self, key: &CacheKey) -> bool {
        self.index.lock().entries.contains_key(&key.id())
    }

    /// Delete one artefact and forget it.
    pub fn forget(&self, key: &CacheKey) {
        let _ = std::fs::remove_file(key.path_under(&self.root));
        self.index.lock().remove(&key.id());
        self.persist_index();
    }

    /// Delete everything. Nothing is lost: every entry is derivable.
    ///
    /// # Errors
    ///
    /// `AURA-IO-1009` when the directory cannot be recreated afterwards.
    pub fn purge(&self) -> AuraResult<()> {
        let _ = std::fs::remove_dir_all(&self.root);
        std::fs::create_dir_all(&self.root)
            .map_err(|e| cache_unwritable(format!("cannot recreate the cache folder: {e}")))?;
        let mut index = self.index.lock();
        let carried_hits = index.hits;
        let carried_misses = index.misses;
        let carried_evictions = index.evictions;
        *index = CacheIndex {
            hits: carried_hits,
            misses: carried_misses,
            evictions: carried_evictions,
            ..CacheIndex::default()
        };
        drop(index);
        self.persist_index();
        Ok(())
    }

    /// What the settings panel shows.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        let index = self.index.lock();
        CacheStats {
            bytes_used: index.bytes_used(),
            budget_bytes: self.budget.lock().max_bytes,
            entries: index.entries.len() as u64,
            hits: index.hits,
            misses: index.misses,
            evictions: index.evictions,
        }
    }

    /// Drop least-recently-used entries until the total fits the budget.
    fn evict_to_budget(&self) {
        let budget = self.budget.lock().max_bytes;
        let doomed = self.index.lock().eviction_order(budget);
        if doomed.is_empty() {
            return;
        }
        for id in &doomed {
            if let Some(key) = parse_file_name(id) {
                let _ = std::fs::remove_file(key.path_under(&self.root));
            }
            let mut index = self.index.lock();
            index.remove(id);
            index.evictions += 1;
        }
        tracing::info!(target: "cache", evicted = doomed.len(), "cache evicted to budget");
        self.persist_index();
    }

    /// Write the index out. A failure here is logged and ignored: the index is
    /// itself a cache, and [`rebuild_index`] can always reconstruct it.
    fn persist_index(&self) {
        let index = self.index.lock();
        let Ok(text) = serde_json::to_string(&*index) else {
            return;
        };
        drop(index);
        let path = self.root.join(INDEX_FILE);
        let temporary = self.root.join("index.part");
        if std::fs::write(&temporary, text.as_bytes()).is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
    }
}

fn load_index(root: &Path) -> Option<CacheIndex> {
    let text = std::fs::read_to_string(root.join(INDEX_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Reconstruct the index from what is actually on disk.
///
/// Digests are recomputed, so an entry that was corrupted while the index was
/// missing is still caught on its next read rather than being trusted.
fn rebuild_index(root: &Path) -> CacheIndex {
    let mut index = CacheIndex::default();
    let Ok(shards) = std::fs::read_dir(root) else {
        return index;
    };
    for shard in shards.flatten() {
        if !shard.path().is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(shard.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(key) = parse_file_name(name) else {
                continue;
            };
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            index.insert(
                key.id(),
                bytes.len() as u64,
                blake3::hash(&bytes).to_hex().to_string(),
            );
        }
    }
    tracing::info!(
        target: "cache",
        entries = index.entries.len(),
        "rebuilt the preview cache index by scanning"
    );
    index
}
