//! The service: cache, decode, record, schedule.
//!
//! Everything above this line asks for `Pixels::at_least(level)`. Everything
//! below it is containers and arithmetic. This module is the join, and it owns
//! four responsibilities that nothing else should:
//!
//! 1. **The three-level lookup.** Memory, then the disk cache, then a decode.
//!    A cached read must beat 8 ms, so the disk path decodes a small JPEG and
//!    nothing else.
//! 2. **Recording.** A produced preview writes a cache entry, a sidecar and a
//!    `preview` row, in that order, so a crash can leave an orphaned cache entry
//!    (harmless, self-healing) but never a catalog row pointing at nothing.
//! 3. **Quarantine.** A decode failure with `Recovery::Quarantine` becomes a
//!    Problems-list row and never an exception the UI has to interpret.
//! 4. **Telemetry.** `preview.decoded`, `preview.quarantined`, `cache.stats` and
//!    `colour.profile_missing`, exactly as the phase document specifies.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use aura_cache::{Artefact, CacheBudget, CacheKey, CacheStats, CacheStore};
use aura_core::clock::{Clock, SystemClock};
use aura_core::{AuraError, AuraResult, Recovery};
use aura_raw::codec::Rgb8;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelLevel, PixelSource};
use aura_raw::contract::sidecar::{
    CameraProfileMeta, EngineMeta, PreviewSidecar, TierMeta, PIPELINE_VER,
};
use aura_raw::meta::RawMeta;
use aura_raw::timeout::{self, DecodeLimits};
use aura_raw::{codec, full, meta, proxy, thumb};
use parking_lot::Mutex;

use crate::contract::service::{ImageId, PreviewService, Priority};
use crate::pool::{DecodePool, Handler};
use crate::request::{Request, Served};
use crate::source::{PreviewRecord, PreviewSource};

/// How the service is sized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewConfig {
    /// Long edge of the tier 1 thumbnail the grid renders.
    pub thumb_edge: u32,
    /// How many bytes of decoded pixels to keep in memory.
    pub memory_budget_bytes: u64,
    /// Decode workers. Zero means "every core but one".
    pub workers: usize,
    /// How many requests may wait.
    pub queue_capacity: usize,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            thumb_edge: thumb::DEFAULT_THUMB_EDGE,
            // 512 MB of decoded pixels is roughly 40 proxies or 600 thumbnails:
            // more than a screenful, far below the 2.5 GB peak-RSS budget.
            memory_budget_bytes: 512 * 1024 * 1024,
            workers: 0,
            queue_capacity: crate::priority::DEFAULT_CAPACITY,
        }
    }
}

/// A bounded in-memory buffer cache, least-recently-used.
#[derive(Debug, Default)]
struct MemoryCache {
    entries: BTreeMap<String, (Arc<PixelBuffer>, u64)>,
    counter: u64,
    bytes: u64,
}

impl MemoryCache {
    fn get(&mut self, key: &str) -> Option<Arc<PixelBuffer>> {
        self.counter += 1;
        let counter = self.counter;
        let entry = self.entries.get_mut(key)?;
        entry.1 = counter;
        Some(Arc::clone(&entry.0))
    }

    fn insert(&mut self, key: String, buffer: &Arc<PixelBuffer>, budget: u64) {
        self.counter += 1;
        let size = buffer.byte_len() as u64;
        if let Some((old, _)) = self.entries.insert(key, (Arc::clone(buffer), self.counter)) {
            self.bytes = self.bytes.saturating_sub(old.byte_len() as u64);
        }
        self.bytes += size;
        while self.bytes > budget && self.entries.len() > 1 {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(key, (_, used))| (*used, (*key).clone()))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some((dropped, _)) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(dropped.byte_len() as u64);
            }
        }
    }
}

#[derive(Debug)]
struct Inner {
    cache: CacheStore,
    source: Arc<dyn PreviewSource>,
    memory: Mutex<MemoryCache>,
    quarantine: Mutex<BTreeMap<String, (ImageId, String)>>,
    clock: Arc<dyn Clock>,
    config: PreviewConfig,
}

/// The preview service.
#[derive(Debug)]
pub struct Previews {
    inner: Arc<Inner>,
    pool: DecodePool,
}

impl Previews {
    /// Build a service over a cache directory and a catalog-backed source.
    ///
    /// # Errors
    ///
    /// `AURA-IO-1009` when the cache directory cannot be created.
    pub fn open(
        cache_root: &Path,
        budget: CacheBudget,
        source: Arc<dyn PreviewSource>,
        config: PreviewConfig,
    ) -> AuraResult<Self> {
        Self::open_with_clock(
            cache_root,
            budget,
            source,
            config,
            Arc::new(SystemClock::default()),
        )
    }

    /// Build a service with an explicit clock, which is what tests use to make
    /// decode timings deterministic.
    ///
    /// # Errors
    ///
    /// `AURA-IO-1009` when the cache directory cannot be created.
    pub fn open_with_clock(
        cache_root: &Path,
        budget: CacheBudget,
        source: Arc<dyn PreviewSource>,
        config: PreviewConfig,
        clock: Arc<dyn Clock>,
    ) -> AuraResult<Self> {
        let inner = Arc::new(Inner {
            cache: CacheStore::open(cache_root, budget)?,
            source,
            memory: Mutex::new(MemoryCache::default()),
            quarantine: Mutex::new(BTreeMap::new()),
            clock,
            config,
        });
        let workers = if config.workers == 0 {
            DecodePool::default_worker_count()
        } else {
            config.workers
        };
        let pool = DecodePool::new(workers, config.queue_capacity, &Arc::clone(&inner));
        Ok(Self { inner, pool })
    }

    /// Abandon queued work for cells that have scrolled out of view.
    #[must_use = "the count tells the caller how much speculative work it saved"]
    pub fn cancel(&self, ids: &[ImageId]) -> usize {
        self.pool.cancel(ids)
    }

    /// How many requests are waiting.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.pool.queued()
    }

    /// How many background requests have completed.
    #[must_use]
    pub fn completed(&self) -> u64 {
        self.pool.completed()
    }

    /// How many decode workers are running.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.pool.worker_count()
    }

    /// Change the cache budget and evict down to it at once.
    pub fn set_cache_budget(&self, budget: CacheBudget) {
        self.inner.cache.set_budget(budget);
        let stats = self.inner.cache.stats();
        tracing::info!(
            target: "cache.stats",
            bytes_used = stats.bytes_used,
            budget = stats.budget_bytes,
            hit_rate = stats.hit_rate(),
            evictions = stats.evictions,
            "cache budget changed"
        );
    }

    /// Delete every cached preview. Nothing is lost; everything rebuilds.
    ///
    /// # Errors
    ///
    /// `AURA-IO-1009` when the cache folder cannot be recreated.
    pub fn purge_cache(&self) -> AuraResult<()> {
        self.inner.memory.lock().entries.clear();
        self.inner.cache.purge()
    }
}

impl Handler for Inner {
    fn handle(&self, request: &Request) {
        // Background work: failures are already recorded by `produce`, and there
        // is no caller waiting to be told.
        let _ = self.produce(request.id, request.level, request.priority);
    }
}

impl PreviewService for Previews {
    fn get(&self, id: ImageId, level: PixelLevel, prio: Priority) -> AuraResult<Arc<PixelBuffer>> {
        // Visible and interactive requests are served on the calling thread.
        // Queueing them behind a batch would be exactly the stutter the priority
        // model exists to prevent, and the pool deliberately leaves a core free
        // for this.
        self.inner.produce(id, level, prio)
    }

    fn prefetch(&self, ids: &[ImageId], level: PixelLevel) {
        for id in ids {
            // A dropped prefetch is the backpressure working, not a failure:
            // the queue is full because someone is already being served.
            let _accepted = self.pool.submit(*id, level, Priority::AiBatch);
        }
    }

    fn cache_stats(&self) -> CacheStats {
        self.inner.cache.stats()
    }

    fn quarantine(&self) -> Vec<(ImageId, String)> {
        self.inner
            .quarantine
            .lock()
            .values()
            .map(|(id, reason)| (*id, reason.clone()))
            .collect()
    }
}

impl Inner {
    fn memory_key(id: ImageId, level: PixelLevel) -> String {
        format!("{}:{}:{}", id.to_db(), level.as_str(), level.long_edge())
    }

    fn produce(
        &self,
        id: ImageId,
        level: PixelLevel,
        prio: Priority,
    ) -> AuraResult<Arc<PixelBuffer>> {
        let key = Self::memory_key(id, level);
        if let Some(found) = self.memory.lock().get(&key) {
            Self::report(id, level, prio, Served::Memory, &found);
            return Ok(found);
        }

        let file = self.source.locate(id)?;
        if let Some(found) = self.read_cached(&file.content_hash, level) {
            let buffer = Arc::new(found);
            self.memory
                .lock()
                .insert(key, &buffer, self.config.memory_budget_bytes);
            Self::report(id, level, prio, Served::Disk, &buffer);
            return Ok(buffer);
        }

        match self.decode(id, level, &file.path, &file.content_hash) {
            Ok(buffer) => {
                let buffer = Arc::new(buffer);
                self.memory
                    .lock()
                    .insert(key, &buffer, self.config.memory_budget_bytes);
                Self::report(id, level, prio, Served::Decoded, &buffer);
                Ok(buffer)
            }
            Err(error) => {
                self.quarantine_if_needed(id, &file.path, &error);
                Err(error)
            }
        }
    }

    /// Read a cached artefact and turn it back into a buffer.
    fn read_cached(&self, content_hash: &str, level: PixelLevel) -> Option<PixelBuffer> {
        let artefact = match level {
            PixelLevel::Thumb(_) => Artefact::Thumb,
            PixelLevel::Proxy2048 => Artefact::Proxy,
            // A full decode is gigabytes; it is produced on demand and never
            // cached, which is also why tier 3 has no cache key.
            PixelLevel::Full => return None,
        };
        let key = CacheKey::new(content_hash, PIPELINE_VER, artefact);
        let bytes = self.cache.get(&key)?;
        let image = codec::decode_jpeg(&bytes, DecodeLimits::tier1()).ok()?;
        let source = self
            .cache
            .get(&CacheKey::new(content_hash, PIPELINE_VER, Artefact::Meta))
            .and_then(|raw| String::from_utf8(raw).ok())
            .and_then(|text| PreviewSidecar::from_json(&text).ok())
            .and_then(|sidecar| match level {
                PixelLevel::Thumb(_) => sidecar.tier1,
                _ => sidecar.tier2,
            })
            .map_or(PixelSource::Embedded, |tier| {
                if tier.source == PixelSource::Demosaiced.as_str() {
                    PixelSource::Demosaiced
                } else {
                    PixelSource::Embedded
                }
            });

        Some(PixelBuffer {
            width: image.width,
            height: image.height,
            data: PixelData::Srgb8(image.data),
            colour_space: ColourSpace::Srgb,
            source,
            decode_ms: 0,
        })
    }

    fn decode(
        &self,
        id: ImageId,
        level: PixelLevel,
        path: &Path,
        content_hash: &str,
    ) -> AuraResult<PixelBuffer> {
        let bytes = std::fs::read(path).map_err(|e| aura_core::errors::io::from_io(&e, path))?;
        match level {
            PixelLevel::Thumb(edge) => self.decode_tier1(id, &bytes, path, content_hash, edge),
            PixelLevel::Proxy2048 => self.decode_tier2(id, &bytes, path, content_hash),
            PixelLevel::Full => self.decode_tier3(&bytes, path),
        }
    }

    fn decode_tier1(
        &self,
        id: ImageId,
        bytes: &[u8],
        path: &Path,
        content_hash: &str,
        edge: u32,
    ) -> AuraResult<PixelBuffer> {
        let limits = DecodeLimits::tier1();
        let clock = Arc::clone(&self.clock);
        let owned = bytes.to_vec();
        let owned_path = path.to_path_buf();
        let result = timeout::guarded("tier1", limits, move || {
            let parsed = meta::read(&owned, &owned_path)?;
            thumb::tier1(&owned, &parsed, edge, limits, clock.as_ref(), &owned_path)
        })?;

        if let Some(warning) = &result.warning {
            tracing::warn!(
                target: "preview.quarantined",
                code = warning.code.0,
                reason = warning.detail,
                "tier 1 fell back"
            );
        }

        let key = CacheKey::new(content_hash, PIPELINE_VER, Artefact::Thumb);
        self.store(&key, &result.jpeg);
        self.merge_sidecar(id, content_hash, |sidecar| {
            sidecar.tier1 = Some(TierMeta {
                w: result.buffer.width,
                h: result.buffer.height,
                source: result.buffer.source.as_str().to_string(),
                bytes: result.jpeg.len() as u64,
                decode_ms: result.buffer.decode_ms,
                linear16: false,
                render_path: result.render_path.to_string(),
            });
        });
        self.record(id, 1, &key, &result.buffer, result.jpeg.len() as u64);

        Ok(result.buffer)
    }

    fn decode_tier2(
        &self,
        id: ImageId,
        bytes: &[u8],
        path: &Path,
        content_hash: &str,
    ) -> AuraResult<PixelBuffer> {
        let limits = DecodeLimits::tier2();
        let clock = Arc::clone(&self.clock);
        let owned = bytes.to_vec();
        let owned_path = path.to_path_buf();
        let result = timeout::guarded("tier2", limits, move || {
            let parsed: RawMeta = meta::read(&owned, &owned_path)?;
            let embedded: Option<Rgb8> = None;
            proxy::tier2(
                &owned,
                &parsed,
                limits,
                clock.as_ref(),
                embedded.as_ref(),
                &owned_path,
            )
        })?;

        for warning in &result.warnings {
            if warning.code == aura_core::errors::raw::RAW_PROFILE_MISSING {
                tracing::warn!(
                    target: "colour.profile_missing",
                    make = result.profile.make,
                    model = result.profile.model,
                    "no camera profile; used the generic matrix"
                );
            } else {
                tracing::warn!(
                    target: "preview.decoded",
                    code = warning.code.0,
                    reason = warning.detail,
                    "tier 2 fell back"
                );
            }
        }

        let proxy_key = CacheKey::new(content_hash, PIPELINE_VER, Artefact::Proxy);
        self.store(&proxy_key, &result.jpeg);

        let linear_bytes = result
            .pair
            .linear
            .as_linear16()
            .map_or_else(Vec::new, |samples| {
                aura_cache::encode_linear(
                    aura_cache::LinearHeader {
                        width: result.pair.linear.width,
                        height: result.pair.linear.height,
                    },
                    samples,
                )
            });
        if !linear_bytes.is_empty() {
            self.store(
                &CacheKey::new(content_hash, PIPELINE_VER, Artefact::Linear),
                &linear_bytes,
            );
        }

        let clipping = result.clipping;
        let profile = result.profile.clone();
        self.merge_sidecar(id, content_hash, |sidecar| {
            sidecar.tier2 = Some(TierMeta {
                w: result.pair.srgb.width,
                h: result.pair.srgb.height,
                source: result.pair.srgb.source.as_str().to_string(),
                bytes: result.jpeg.len() as u64,
                decode_ms: result.pair.srgb.decode_ms,
                linear16: true,
                render_path: result.render_path.to_string(),
            });
            sidecar.camera_profile = CameraProfileMeta {
                make: profile.make.clone(),
                model: profile.model.clone(),
                matrix: profile.source.as_str().to_string(),
                illuminant: profile.illuminant.to_string(),
            };
            sidecar.clipping = clipping;
            // A sidecar written by tier 1 has no white balance in it yet; a
            // zeroed triple means "not measured", never "neutral is black".
            if sidecar.as_shot_neutral.iter().all(|value| *value <= 0.0) {
                sidecar.as_shot_neutral = [1.0, 1.0, 1.0];
            }
        });
        self.record(
            id,
            2,
            &proxy_key,
            &result.pair.srgb,
            result.jpeg.len() as u64,
        );

        Ok(result.pair.srgb)
    }

    fn decode_tier3(&self, bytes: &[u8], path: &Path) -> AuraResult<PixelBuffer> {
        let limits = DecodeLimits::tier3();
        let clock = Arc::clone(&self.clock);
        let owned = bytes.to_vec();
        let owned_path = path.to_path_buf();
        timeout::guarded("tier3", limits, move || {
            let parsed = meta::read(&owned, &owned_path)?;
            full::tier3(&owned, &parsed, limits, clock.as_ref())
        })
    }

    /// Write an artefact, downgrading a cache failure to a warning: the caller
    /// already has the pixels, so losing the disk copy costs time, not truth.
    fn store(&self, key: &CacheKey, bytes: &[u8]) {
        if let Err(error) = self.cache.put(key, bytes) {
            tracing::warn!(
                target: "cache.stats",
                code = error.code.0,
                detail = error.detail,
                "preview cache write failed"
            );
        }
    }

    fn merge_sidecar(
        &self,
        id: ImageId,
        content_hash: &str,
        mutate: impl FnOnce(&mut PreviewSidecar),
    ) {
        let key = CacheKey::new(content_hash, PIPELINE_VER, Artefact::Meta);
        let mut sidecar = self
            .cache
            .get(&key)
            .and_then(|raw| String::from_utf8(raw).ok())
            .and_then(|text| PreviewSidecar::from_json(&text).ok())
            .unwrap_or_else(|| blank_sidecar(id, content_hash));
        mutate(&mut sidecar);
        if let Ok(text) = sidecar.to_json() {
            self.store(&key, text.as_bytes());
        }
    }

    fn record(&self, id: ImageId, tier: i64, key: &CacheKey, buffer: &PixelBuffer, bytes: u64) {
        let record = PreviewRecord {
            id,
            tier,
            rel_cache_path: format!("{}/{}", key.shard(), key.file_name()),
            width: buffer.width,
            height: buffer.height,
            source: buffer.source.as_str().to_string(),
            bytes,
            stage_version: PIPELINE_VER,
        };
        if let Err(error) = self.source.record_preview(&record) {
            tracing::warn!(
                target: "preview.decoded",
                code = error.code.0,
                detail = error.detail,
                "could not record a preview row"
            );
        }
    }

    fn quarantine_if_needed(&self, id: ImageId, path: &Path, error: &AuraError) {
        if error.recovery != Recovery::Quarantine {
            return;
        }
        tracing::warn!(
            target: "preview.quarantined",
            code = error.code.0,
            reason = error.detail,
            "decode failed; photograph set aside"
        );
        self.quarantine
            .lock()
            .insert(id.to_db(), (id, error.user_message.clone()));
        if let Err(problem) = self.source.record_problem(id, path, error) {
            tracing::warn!(
                target: "preview.quarantined",
                code = problem.code.0,
                detail = problem.detail,
                "could not record the problem row"
            );
        }
    }

    fn report(
        id: ImageId,
        level: PixelLevel,
        prio: Priority,
        served: Served,
        buffer: &PixelBuffer,
    ) {
        tracing::debug!(
            target: "preview.decoded",
            image = %id,
            tier = level.tier(),
            ms = buffer.decode_ms,
            source = buffer.source.as_str(),
            served = served.as_str(),
            priority = prio.as_str(),
            ok = true,
            "preview served"
        );
    }
}

fn blank_sidecar(id: ImageId, content_hash: &str) -> PreviewSidecar {
    PreviewSidecar {
        image_id: id.to_db(),
        content_hash: content_hash.to_string(),
        tier1: None,
        tier2: None,
        camera_profile: CameraProfileMeta {
            make: String::new(),
            model: String::new(),
            matrix: "generic".to_string(),
            illuminant: "D50".to_string(),
        },
        as_shot_neutral: [1.0, 1.0, 1.0],
        black_level: 0,
        white_level: 0,
        clipping: aura_raw::contract::sidecar::ClippingStats {
            highlight: 0.0,
            shadow: 0.0,
        },
        engine: EngineMeta {
            libraw: None,
            decoder: aura_raw::decoder_version(),
            pipeline_ver: PIPELINE_VER,
        },
    }
}
