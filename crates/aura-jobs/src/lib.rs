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
// The panic family and slice indexing are banned in library code and are how a test asserts. An
// inline `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can
// reach a photographer; the lints stay denied everywhere else in the crate. The same exemption
// phases 14, 19, 23, 24, 25, 26 and 27 took, for the same reason - this crate is only acquiring it
// now because phase 01 wrote it before the idiom existed and phase 28 is the first to add inline
// tests here.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::disallowed_methods,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args,
        clippy::too_many_lines
    )
)]

//! The job graph and the zero-touch autopilot: tasks, dependencies, leases, and the orchestrator
//! that turns twenty-seven phases into one button.
//!
//! Resumability is a product invariant, so task state lives in the catalog and is committed in the
//! same transaction as the work it describes. A worker that dies loses its lease, not its
//! progress.
//!
//! # What is here, in two halves
//!
//! **Phase 01's half** is [`graph`] and [`lease`]: a task, its dependencies and the lease a worker
//! holds while it runs one. That is the unit-of-work layer, and it has not changed.
//!
//! **Phase 28's half** is everything else: the twenty-five-stage DAG, the checkpoints, the
//! resume, the governor, the pre-flight, the ETA, the run summary and the loop in [`api`] that
//! joins them. It sits *above* the task layer - a stage is a phase's whole pass rather than one
//! photograph - and it is the first code in the product whose subject is a run.
//!
//! # What this crate cannot do
//!
//! It depends on `aura-core` and `aura-catalog` and on none of the twenty-two deciding crates. A
//! stage is executed through [`contract::autopilot::StageRunner`], a band comes from
//! [`contract::autopilot::AutonomyGate`], and the machine is read through
//! [`contract::autopilot::MachineProbe`] - three ports `aura-app` implements over `AppState`.
//!
//! That is not a layering preference. A scheduler that depended on every phase would be a crate
//! every phase could reach back into, and the thing it would eventually reach for is the one thing
//! an orchestrator must never have: an opinion about a photograph.
//! `crates/aura-jobs/tests/no_decisions.rs` is the grep that says so on every build.

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod autopilot;
}

pub mod api;
pub mod cancel;
pub mod checkpoint;
pub mod dag;
pub mod errors;
pub mod fixtures;
pub mod governor;
pub mod graph;
pub mod lease;
pub mod policy;
pub mod preflight;
pub mod progress;
pub mod resume;
pub mod retry;
pub mod stages;
pub mod store;
pub mod summary;

pub use contract::autopilot::{
    AutonomyGate, AutopilotCode, AutopilotOutline, AutopilotOverride, AutopilotReason,
    AutopilotService, Checkpoint, CheckpointKind, Eta, GovernorAction, Invalidation, MachineProbe,
    MachineState, PreflightCheck, PreflightReport, PreflightRow, PreflightVerdict, ResourceEvent,
    ResourceKind, ResourceNeeds, RunHandle, RunProgress, RunStatus, RunSummary, RunWatch,
    SkipCause, StageDecl, StageId, StageOutcome, StageReport, StageRequest, StageRunner,
    StageScope, StageVerdict,
};
pub use graph::{JobGraph, Task, TaskState};
pub use lease::{Lease, LeaseConfig};

pub use api::{Autopilot, Ports, Tally};
pub use dag::{Dag, DagError};
pub use policy::{Budgets, Policy};
pub use store::AutopilotStore;
