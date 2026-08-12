//! A documented subset of ONNX opset 13, read, written and executed here.
//!
//! Four layers, each testable on its own:
//!
//! * [`wire`] - the protobuf encoding, reader and writer.
//! * [`model`] - the nine messages we care about, parsed and serialised.
//! * [`graph`] - validation, ordering and execution.
//! * [`ops`] - the nineteen operators, each with a fixed accumulation order.
//!
//! [`fixtures`] builds small models in memory so the runtime, the registry, the
//! benchmarks and the phase gate all have something real to run without a
//! training pipeline or a download.

pub mod fixtures;
pub mod graph;
pub mod model;
pub mod ops;
pub mod wire;

pub use graph::{Executable, Value};
pub use model::{parse, serialise, OnnxModel, OPSET};
