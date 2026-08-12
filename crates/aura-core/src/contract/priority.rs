//! FROZEN CONTRACT. The product-wide scheduling class.
//!
//! Phase 02 froze the same four classes inside `aura-preview`'s `PreviewService`
//! contract. Phase 03 needs them again for inference, and `aura-infer` must not
//! depend on `aura-preview`: previews are infrastructure, and Article II rule A2
//! points dependencies inward, toward the domain. So the vocabulary moves here,
//! where every crate can reach it without reaching through another crate's
//! infrastructure.
//!
//! The phase 02 copy is deliberately left alone. Editing a frozen contract to
//! deduplicate a four-variant enum would invalidate `contracts.lock` for a
//! cosmetic gain, and a test in `aura-app` - which already depends on both -
//! asserts the two stay in lockstep, so drift is a build failure rather than a
//! surprise.
//!
//! Changing anything here requires an ADR.

use serde::{Deserialize, Serialize};

/// Why the caller wants this work done, which decides what runs first.
///
/// The order matters and is deliberate: a photographer who clicks a photograph
/// must never wait behind a 4,000-image batch. `Visible` preempts everything
/// below it, and every pool in the product keeps a worker free so that
/// preemption is real rather than theoretical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// The user is looking at this right now.
    Visible,
    /// The user asked for it - opened the loupe, started an export.
    Interactive,
    /// A model needs it as part of a batch.
    AiBatch,
    /// Speculative work with no one waiting.
    Background,
}

impl Priority {
    /// Stable text for telemetry and for the plan file.
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

    /// True when a request of this class may push another aside.
    ///
    /// Only the two classes with a human waiting may preempt. An AI batch that
    /// could preempt background work would let a 4,000-image analysis starve the
    /// speculative decode that makes the next screen instant.
    #[must_use]
    pub const fn preempts(self) -> bool {
        matches!(self, Self::Visible | Self::Interactive)
    }
}
