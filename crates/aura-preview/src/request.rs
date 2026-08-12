//! One unit of preview work.

use aura_raw::contract::pixels::PixelLevel;

use crate::contract::service::{ImageId, Priority};

/// A request for one image at one level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Which photograph.
    pub id: ImageId,
    /// Which rung of the pyramid.
    pub level: PixelLevel,
    /// How urgent.
    pub priority: Priority,
    /// Submission order, used to break ties inside a class so the queue is
    /// first-in-first-out among equals and therefore predictable.
    pub sequence: u64,
}

impl Request {
    /// The identity used for de-duplication: two requests for the same image at
    /// the same level are the same work, whatever priority they arrived with.
    #[must_use]
    pub fn dedupe_key(&self) -> (ImageId, PixelLevel) {
        (self.id, self.level)
    }
}

/// Where the pixels for a request came from, recorded for telemetry and for the
/// benchmark table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Served {
    /// The in-memory buffer cache.
    Memory,
    /// The on-disk content-addressed cache.
    Disk,
    /// A fresh decode.
    Decoded,
}

impl Served {
    /// Stable text for telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Decoded => "decoded",
        }
    }

    /// True when this request cost a decode.
    #[must_use]
    pub const fn is_miss(self) -> bool {
        matches!(self, Self::Decoded)
    }
}
