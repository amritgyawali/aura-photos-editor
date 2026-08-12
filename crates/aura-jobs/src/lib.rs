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

//! The job graph: tasks, dependencies, leases and heartbeats.
//!
//! Resumability is a product invariant, so task state lives in the catalog and is
//! committed in the same transaction as the work it describes. A worker that dies
//! loses its lease, not its progress.

pub mod graph;
pub mod lease;

pub use graph::{JobGraph, Task, TaskState};
pub use lease::{Lease, LeaseConfig};
