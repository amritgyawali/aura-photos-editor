#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::cast_precision_loss)]

//! The preview service: one door, three tiers, four priorities.
//!
//! Later phases never call a decoder. They call [`contract::service::PreviewService`],
//! which decides whether the answer comes from memory, from the content-addressed
//! cache or from a fresh decode - and which makes sure that a photographer
//! scrolling a grid is served before a batch of four thousand images that no one
//! is waiting for.
//!
//! The pieces:
//!
//! * [`contract::service`] - the frozen trait and the priority classes.
//! * [`priority`] - strict-priority queueing with de-duplication and promotion.
//! * [`pool`] - workers, sized to leave one core free for the person.
//! * [`source`] - the catalog boundary: where a file is, where results go.
//! * [`service`] - the wiring, the recording and the telemetry.

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod service;
}

pub mod pool;
pub mod priority;
pub mod request;
pub mod service;
pub mod source;

pub use contract::service::{ImageId, PreviewService, Priority};
pub use service::{PreviewConfig, Previews};
pub use source::{CatalogSource, PreviewRecord, PreviewSource, SourceFile};
