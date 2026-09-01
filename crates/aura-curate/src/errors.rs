//! Curation error constructors. Codes ML 5142-5145, registered in `crates/aura-core/errors.toml`.
//!
//! Three of the four live in `aura-core` rather than here, because the frozen `CurateService`
//! documents them on its own methods and a contract cannot depend on the crate that implements it.
//! The same split phases 16, 22, 23, 24, 25, 26 and 27 made. This module is the thin layer that
//! gives them a sentence naming what in *this* crate went wrong.

use aura_core::errors::ml::{curate_decision_refused, curate_pass_failed, curate_policy_refused};
use aura_core::AuraError;

/// The curation pass could not finish, naming the stage that failed.
///
/// The stage rather than the row, because curation's subject is a *set*: there is no partial album
/// worth storing, and "the hero selector failed" is what a support case needs while "image 412
/// failed" is not a thing that can happen here.
#[must_use]
pub fn pass_failed(stage: &str, detail: impl AsRef<str>) -> AuraError {
    curate_pass_failed(format!("curation stage `{stage}`: {}", detail.as_ref()))
}

/// `curation.toml` asked for something the contract does not permit.
///
/// The message names the key and both numbers, because "the file is wrong" is not something a
/// studio can act on and "album_max is 200 and the ceiling is 120" is.
#[must_use]
pub fn policy_refused(key: &str, detail: impl AsRef<str>) -> AuraError {
    curate_policy_refused(format!("curation.toml `{key}`: {}", detail.as_ref()))
}

/// A photographer's choice could not be recorded.
#[must_use]
pub fn decision_refused(detail: impl AsRef<str>) -> AuraError {
    curate_decision_refused(detail.as_ref().to_string())
}

/// An album order that puts one chapter before another.
///
/// Its own constructor rather than a string at each call site, because three separate paths refuse
/// this - the drag handler, the optimiser's own assertion and the cloud validator - and a sentence
/// written three times is a sentence that says three different things by the third release.
#[must_use]
pub fn chapters_reordered() -> AuraError {
    decision_refused(
        "an album's chapters stay in the order the wedding happened in; frames may be reordered \
         inside a chapter",
    )
}
