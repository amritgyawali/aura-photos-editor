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
#![allow(clippy::module_name_repetitions)]

//! The vocabulary every other AURA crate speaks: the error taxonomy, typed ids,
//! the consent contract, the clock, path safety and log redaction.
//!
//! `aura-core` depends on no other workspace crate. That rule keeps the error
//! taxonomy usable everywhere without a dependency cycle.

pub mod clock;

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod consent;
    pub mod error;
    pub mod ids;
    pub mod people;
    pub mod priority;
    pub mod scene;
}

/// Named error constructors, one module per domain in the code registry.
pub mod errors {
    pub mod cloud;
    pub mod db;
    pub mod gpu;
    pub mod io;
    pub mod job;
    pub mod ml;
    pub mod raw;
}

pub mod paths;
pub mod progress;
pub mod redact;

pub use contract::error::{AuraError, AuraResult, ErrorCode, Recovery, Severity};
pub use contract::ids::{
    ContentHash, FaceId, FileId, IdentityId, ImportId, PhotoId, ProjectId, RunId, SegmentId,
};
pub use contract::people::{FaceRef, ImageSubjects, PeopleService, Role, SubjectHierarchy};
pub use contract::priority::Priority;
pub use contract::scene::{
    AttrFlags, ChapterId, EditIntent, RitualId, SceneId, SceneProfile, SceneResult, SceneScore,
    Segment, Source, StoryOutline, StoryService,
};
