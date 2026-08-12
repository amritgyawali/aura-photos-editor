//! The error vocabulary this crate raises, gathered in one place.
//!
//! Every constructor lives in `aura-core::errors`, because the registry, the
//! runbooks and the user-facing sentences are owned there and must not fork. What
//! this module adds is the *policy*: which code answers which situation inside
//! the runtime, written down once so two modules cannot answer the same situation
//! differently.

pub use aura_core::errors::gpu::{
    memory_downshift, no_accelerator, plan_unreadable, probe_failed, provider_set_aside,
};
pub use aura_core::errors::ml::{
    cancelled, deadline_exceeded, model_unknown, op_unsupported, parse_failed, shape_mismatch,
};

use aura_core::AuraError;

/// A malformed model file: verified bytes the parser cannot read.
///
/// Kept separate from [`op_unsupported`] on purpose. "I do not implement `LSTM`"
/// is a capability gap with a documented fallback; "field 4 of the graph is
/// truncated" is a broken artefact, and the two need different runbooks and
/// different people.
#[must_use]
pub fn malformed(detail: impl Into<String>) -> AuraError {
    parse_failed(detail)
}

/// The graph is well-formed but says something impossible - a convolution whose
/// kernel is larger than its padded input, a reshape to a different element
/// count. Reported as a parse failure because the artefact is still the thing at
/// fault.
#[must_use]
pub fn invalid_graph(detail: impl Into<String>) -> AuraError {
    parse_failed(detail)
}
