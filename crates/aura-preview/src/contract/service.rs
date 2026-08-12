//! FROZEN CONTRACT. How the rest of the product asks for pixels.
//!
//! Phase 02 section 5 freezes this trait before any decoder exists, and that
//! ordering is the point: phases 05 to 30 are written against it, so none of
//! them can grow a dependency on *where* pixels came from. A caller asks for a
//! level and a priority; whether the answer arrives from memory, from the disk
//! cache, from an embedded JPEG or from a full demosaic is not its business.
//!
//! Changing anything here requires an ADR.

use std::sync::Arc;

use aura_cache::CacheStats;
use aura_core::AuraResult;
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel};
use serde::{Deserialize, Serialize};

/// The identity of a photograph in a preview request.
///
/// Phase 02 names this `ImageId`; the catalog calls the same thing a `PhotoId`.
/// They are one type, aliased rather than duplicated so no conversion can ever
/// disagree.
pub type ImageId = aura_core::PhotoId;

/// Why the caller wants these pixels, which decides what gets served first.
///
/// The order matters and is deliberate: a photographer scrolling a grid must
/// never wait behind a 4,000-image batch. `Visible` work preempts everything
/// below it, and the pool keeps a worker free so that preemption is real rather
/// than theoretical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// The user is looking at this cell right now.
    Visible,
    /// The user asked for it - opened the loupe, started an export.
    Interactive,
    /// A model needs it as part of a batch.
    AiBatch,
    /// Speculative work with no one waiting.
    Background,
}

impl Priority {
    /// Stable text for telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Interactive => "interactive",
            Self::AiBatch => "ai_batch",
            Self::Background => "background",
        }
    }

    /// Every class, most urgent first.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [
            Self::Visible,
            Self::Interactive,
            Self::AiBatch,
            Self::Background,
        ]
    }
}

/// The one way to obtain pixels.
pub trait PreviewService: Send + Sync + std::fmt::Debug {
    /// Get one image at one level, waiting for it.
    ///
    /// # Errors
    ///
    /// Whatever the decoder raised: `AURA-RAW-2001` through `AURA-RAW-2008`, or
    /// a catalog error when the photograph cannot be located.
    fn get(
        &self,
        id: ImageId,
        level: PixelLevel,
        prio: Priority,
    ) -> AuraResult<Arc<PixelBuffer>>;

    /// Ask for images to be produced in the background. Returns immediately.
    ///
    /// Prefetching is advisory: requests may be dropped when the queue is full,
    /// because a full queue means the user is already being served and
    /// speculative work is exactly what should give way.
    fn prefetch(&self, ids: &[ImageId], level: PixelLevel);

    /// What the cache is doing, for the settings panel.
    fn cache_stats(&self) -> CacheStats;

    /// Photographs that could not be decoded, with a photographer-facing reason.
    fn quarantine(&self) -> Vec<(ImageId, String)>;
}
