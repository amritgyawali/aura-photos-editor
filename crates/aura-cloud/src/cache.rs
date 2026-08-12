//! The response cache: what makes re-running a wedding nearly free.
//!
//! Section 11 budgets a cache hit rate of at least 70 % on a re-run, and section
//! 10.1 requires that the re-run produce *identical decisions*. Both come from
//! the same property: the key is a function of everything that could change the
//! answer, and of nothing else.
//!
//! ```text
//! key = blake3( task | task_version | prompt_hash | sorted image hashes | model )
//! ```
//!
//! Each element earns its place:
//!
//! * `task` and `task_version` mean a prompt edit invalidates its own answers.
//!   Nobody has to remember to purge, because there is nothing to purge - the old
//!   entries simply stop being looked up, and the age index sweeps them later.
//! * `prompt_hash` covers the system prompt and the rendered user turn, so a
//!   different question is a different entry even at the same version.
//! * The image hashes are **sorted** before hashing, and separately from the
//!   prompt hash which keeps them in order. That is deliberate: the prompt hash
//!   distinguishes "the same twelve frames in a different order", which is a
//!   different question; the sorted list in the key is what lets a future
//!   content-addressed lookup find "have we ever seen this set".
//! * `model` because the same question to a different model is a different
//!   answer, and a cache that ignored it would serve a cheap model's guess after
//!   the user upgraded their tier.
//!
//! What is deliberately **not** in the key is anything about the machine, the
//! run, the project or the time. Two photographers with the same fixtures get
//! the same key, which is what makes the CI cassette suite reproducible.
//!
//! And what is not in the key from the frozen contract: `CloudTask::Input`'s
//! `Hash` implementation. `DefaultHasher` is not guaranteed stable across
//! compiler releases, and a cache key that moved when the toolchain moved would
//! silently re-bill every user after an upgrade. See
//! `docs/adr/ADR-0009-cloud-ai-policy.md`.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use rusqlite::params;

use crate::contract::cloud::PromptSpec;

/// Compute the cache key for one call.
#[must_use]
pub fn key(task: &str, task_version: u16, prompt: &PromptSpec, model: &str) -> String {
    let mut sorted: Vec<&str> = prompt
        .images
        .iter()
        .map(|image| image.content_hash.as_str())
        .collect();
    sorted.sort_unstable();

    let mut hasher = blake3::Hasher::new();
    hasher.update(task.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(task_version.to_string().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(prompt.prompt_hash().as_bytes());
    for hash in sorted {
        hasher.update(b"\x1f");
        hasher.update(hash.as_bytes());
    }
    hasher.update(b"\x1f");
    hasher.update(model.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// What the cache is holding, for the settings panel and the budget report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Entries stored.
    pub entries: u64,
    /// Total response bytes.
    pub bytes: u64,
    /// Lifetime hits across all entries.
    pub hits: u64,
}

/// Looking up and storing validated responses.
///
/// A port rather than a concrete type so the agent loop and the gate can run
/// against an in-memory cache without a catalog, and so a future phase can put
/// the cache somewhere other than the project database without touching the
/// gateway.
pub trait ResponseCache: Send + Sync + std::fmt::Debug {
    /// The stored response text for a key, if there is one. Records the hit.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be read. A miss is `Ok(None)`.
    fn get(&self, key: &str) -> AuraResult<Option<String>>;

    /// Store a validated response.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be written.
    fn put(&self, entry: &CacheEntry<'_>) -> AuraResult<()>;

    /// Counters for the panel.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be read.
    fn stats(&self) -> AuraResult<CacheStats>;

    /// Forget everything for one task version, after a prompt change.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be written.
    fn purge_task(&self, task: &str, task_version: u16) -> AuraResult<u64>;
}

/// One thing to store.
#[derive(Debug, Clone, Copy)]
pub struct CacheEntry<'a> {
    /// The key from [`key`].
    pub key: &'a str,
    /// Registry name of the task.
    pub task: &'a str,
    /// Task version the answer was made under.
    pub task_version: u16,
    /// The model that answered.
    pub model: &'a str,
    /// The validated response, as JSON text.
    pub response_json: &'a str,
}

/// The cache in the project's catalog.
#[derive(Debug)]
pub struct CatalogCache {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl CatalogCache {
    /// Use one catalog's `cloud_cache` table.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }
}

impl ResponseCache for CatalogCache {
    fn get(&self, key: &str) -> AuraResult<Option<String>> {
        let found: Option<String> = self.catalog.read(|conn| {
            conn.query_row(
                "SELECT response_json FROM cloud_cache WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(statement_failed("cloud cache lookup failed", &other)),
            })
        })?;

        if found.is_some() {
            // The hit counter is the only thing a read writes, and it is written
            // on the writer thread like everything else. A failure to record a
            // hit must never fail the lookup: the answer is already in hand.
            let key = key.to_string();
            let now = aura_catalog::rfc3339(self.clock.now_utc());
            let recorded = self.catalog.writer().with(move |conn| {
                conn.execute(
                    "UPDATE cloud_cache SET hits = hits + 1, last_hit_at = ?2 WHERE key = ?1",
                    params![key, now],
                )
                .map_err(|err| statement_failed("cloud cache hit counter failed", &err))
            });
            if let Err(err) = recorded {
                tracing::debug!(target: "cloud.cache", code = err.code.0, "hit counter not recorded");
            }
        }
        Ok(found)
    }

    fn put(&self, entry: &CacheEntry<'_>) -> AuraResult<()> {
        let key = entry.key.to_string();
        let task = entry.task.to_string();
        let version = i64::from(entry.task_version);
        let model = entry.model.to_string();
        let json = entry.response_json.to_string();
        let bytes = i64::try_from(json.len()).unwrap_or(i64::MAX);
        let now = aura_catalog::rfc3339(self.clock.now_utc());

        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO cloud_cache
                   (key, task, response_json, created_at, hits, task_version, model, bytes)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7)
                 ON CONFLICT(key) DO UPDATE SET response_json = excluded.response_json",
                params![key, task, json, now, version, model, bytes],
            )
            .map(|_| ())
            .map_err(|err| statement_failed("cloud cache write failed", &err))
        })
    }

    fn stats(&self) -> AuraResult<CacheStats> {
        self.catalog.read(|conn| {
            conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(bytes), 0), COALESCE(SUM(hits), 0) FROM cloud_cache",
                [],
                |row| {
                    Ok(CacheStats {
                        entries: row.get::<_, i64>(0)?.max(0) as u64,
                        bytes: row.get::<_, i64>(1)?.max(0) as u64,
                        hits: row.get::<_, i64>(2)?.max(0) as u64,
                    })
                },
            )
            .map_err(|err| statement_failed("cloud cache statistics failed", &err))
        })
    }

    fn purge_task(&self, task: &str, task_version: u16) -> AuraResult<u64> {
        let task = task.to_string();
        let version = i64::from(task_version);
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "DELETE FROM cloud_cache WHERE task = ?1 AND task_version = ?2",
                params![task, version],
            )
            .map(|rows| rows as u64)
            .map_err(|err| statement_failed("cloud cache purge failed", &err))
        })
    }
}

/// An in-process cache. The gate and the unit tests use it.
#[derive(Debug, Default)]
pub struct MemoryCache {
    entries: parking_lot::Mutex<std::collections::BTreeMap<String, (String, String, u16, u64)>>,
}

impl MemoryCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ResponseCache for MemoryCache {
    fn get(&self, key: &str) -> AuraResult<Option<String>> {
        let mut entries = self.entries.lock();
        match entries.get_mut(key) {
            Some(entry) => {
                entry.3 += 1;
                Ok(Some(entry.0.clone()))
            }
            None => Ok(None),
        }
    }

    fn put(&self, entry: &CacheEntry<'_>) -> AuraResult<()> {
        self.entries.lock().insert(
            entry.key.to_string(),
            (
                entry.response_json.to_string(),
                entry.task.to_string(),
                entry.task_version,
                0,
            ),
        );
        Ok(())
    }

    fn stats(&self) -> AuraResult<CacheStats> {
        let entries = self.entries.lock();
        Ok(CacheStats {
            entries: entries.len() as u64,
            bytes: entries.values().map(|entry| entry.0.len() as u64).sum(),
            hits: entries.values().map(|entry| entry.3).sum(),
        })
    }

    fn purge_task(&self, task: &str, task_version: u16) -> AuraResult<u64> {
        let mut entries = self.entries.lock();
        let doomed: Vec<String> = entries
            .iter()
            .filter(|(_, entry)| entry.1 == task && entry.2 == task_version)
            .map(|(key, _)| key.clone())
            .collect();
        let count = doomed.len() as u64;
        for key in doomed {
            entries.remove(&key);
        }
        Ok(count)
    }
}
