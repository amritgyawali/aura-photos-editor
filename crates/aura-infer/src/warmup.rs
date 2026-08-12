//! Paying the one-time cost where the user can see it.
//!
//! Loading a model is expensive and the expense is unavoidable; what is avoidable
//! is paying it at the moment a photographer clicks something. Warmup loads the
//! models the next stage needs, in a fixed order, reporting progress as it goes -
//! section 6.5 asks for the progress specifically, because a five-second pause
//! nobody explained is a bug report and a five-second progress bar is not.
//!
//! Warmup failing is a real failure, not a warning. It is how the product learns
//! that a model pack is broken *before* a photographer is waiting on it.

use aura_core::AuraResult;

use crate::contract::infer::ModelRef;
use crate::service::InferEngine;

/// How far warmup has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarmupProgress {
    /// Models finished so far.
    pub done: usize,
    /// Models in this warmup.
    pub total: usize,
    /// The model that just finished, or is about to start.
    pub model: String,
}

/// Load each model once, reporting progress.
///
/// # Errors
///
/// The first failure stops the warmup and is returned: a pack with one broken
/// model is a broken pack, and continuing would only postpone the same error to a
/// worse moment.
pub fn warmup(
    engine: &InferEngine,
    models: &[ModelRef],
    progress: &dyn Fn(WarmupProgress),
) -> AuraResult<()> {
    let total = models.len();
    for (index, model) in models.iter().enumerate() {
        progress(WarmupProgress {
            done: index,
            total,
            model: model.key(),
        });
        let session = engine.load(*model)?;
        tracing::debug!(
            target: "infer.run",
            model = model.key().as_str(),
            working_set_bytes = session.working_set_bytes(),
            precision = session.precision().as_str(),
            "model warmed"
        );
    }
    progress(WarmupProgress {
        done: total,
        total,
        model: String::new(),
    });
    Ok(())
}
