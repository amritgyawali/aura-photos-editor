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
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

//! One local AI runtime, one hardware plan, one batch scheduler.
//!
//! Roughly twenty-five models will pass through this crate before the product
//! ships. Written the obvious way - each feature loading its own model, choosing
//! its own device, sizing its own batches - that becomes twenty-five different
//! answers to "why is it slow on this laptop", and no way to tell a driver bug
//! from a model bug. So there is exactly one entry point, [`InferService`], and
//! everything else in this crate exists to keep that promise:
//!
//! * [`probe`] measures the machine once and writes a [`HardwarePlan`];
//! * [`ep`] negotiates which execution provider answers, and sets aside any that
//!   crashes or disagrees with the processor;
//! * [`session`] pools loaded models and binds their input and output buffers;
//! * [`batch`] admits work against a memory ledger, halves batches rather than
//!   crashing, and lets an interactive request cut ahead of a 4,000-image run;
//! * [`warmup`] pays the one-time cost where the user can see it.
//!
//! The runtime underneath is a port, not a contract. `ReferenceBackend` is a
//! deterministic interpreter over a documented subset of ONNX opset 13, written
//! in safe Rust in [`onnx`]; an ONNX Runtime backend can be added behind the same
//! port without a caller noticing. The reasoning, including why ONNX Runtime is
//! not linked in this build, is in `docs/adr/ADR-0007-inference-runtime.md`.
//!
//! Determinism is the reason the interpreter exists at all. Accumulation order is
//! fixed, every parallel loop writes disjoint output rows, and no result depends
//! on how many cores answered - so two machines return bit-identical tensors, and
//! invariant 4 survives contact with the hardware.

pub mod backend;
pub mod batch;
pub mod ep;
pub mod errors;
pub mod onnx;
pub mod plan;
pub mod probe;
pub mod service;
pub mod session;
pub mod source;
pub mod tensor;
pub mod warmup;

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod infer;
}

pub use contract::infer::{
    BatchDefaults, ExecutionProvider, GpuInfo, HardwarePlan, InferError, InferRequest, InferResult,
    InferService, ModelClass, ModelRef, Precision, SetAsideProvider, Tensor, TensorView,
    UnavailableProvider, Version,
};
pub use service::InferEngine;
pub use source::{ModelSource, ResolvedModel, StaticModelSource};
