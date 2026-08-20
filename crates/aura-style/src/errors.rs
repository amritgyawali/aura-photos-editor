//! The six codes phase 17 raises, `AURA-ML-5072` to `AURA-ML-5077`.
//!
//! The same five shapes every phase since 09 has - one item that could not be done, a refused
//! run, a warning about a sparse case, a refused edit and version drift - plus one that is new
//! in this phase.
//!
//! `AURA-ML-5076` is **the first code in the product that refuses an artefact a photographer
//! chose to import** rather than one AURA shipped. Every previous refusal of a signed thing -
//! `AURA-ML-5002` for a manifest, `AURA-ML-5003` for a model file - was about the release
//! chain. A style profile arrives from a colleague, over a chat client, on a stick, and the
//! only thing the signature can prove is that the bytes have not changed since somebody signed
//! them. The runbook says so in those words and the panel never says "verified".

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// One archive pair could not be matched, read or fitted.
pub const ML_PAIR_FAILED: ErrorCode = ErrorCode("AURA-ML-5072");
/// A style training run was refused.
pub const ML_TRAINING_REFUSED: ErrorCode = ErrorCode("AURA-ML-5073");
/// A style bucket had too few pairs and borrowed from its parent.
pub const ML_BUCKET_SPARSE: ErrorCode = ErrorCode("AURA-ML-5074");
/// A profile could not be adopted or selected.
pub const ML_PROFILE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5075");
/// A style profile bundle was refused.
pub const ML_BUNDLE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5076");
/// A profile was learned with a different render engine or fitter.
pub const ML_STYLE_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5077");

/// One pair could not be used. The scan continues.
///
/// **The pair is still written**, with `accepted = 0` and the reason code - which is the one
/// place this phase differs from every "one item failed" code before it. Phases 09, 15 and 16
/// write no row on a failure, because a stored neutral decision reads to later phases as a
/// decision. Here the opposite is true: a rejected pair *is* the evidence, and a scan that
/// dropped its rejections could report a hundred percent acceptance on any archive.
#[must_use]
pub fn pair_failed(original: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_PAIR_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{original}: {detail}"),
        "AURA could not learn from one of your finished photographs, so it left that one out. \
         Everything else in the archive is still being used.",
    )
    .with_context("original", original)
}

/// A training run was refused before it started. Nothing is written.
///
/// A refusal rather than a warning because the alternative is a profile fitted on a handful of
/// frames, which is section 12's fourth failure mode - "users expect one-click magic from 20
/// photos" - shipped as a feature instead of arriving as a support ticket.
#[must_use]
pub fn training_refused(accepted: usize, needed: usize, detail: &str) -> AuraError {
    AuraError::new(
        ML_TRAINING_REFUSED,
        Severity::RunBlocking,
        Recovery::AskUser,
        format!("{accepted} usable pairs against a minimum of {needed}: {detail}"),
        "AURA could not find enough matching originals and finished photographs in that folder \
         to learn anything, so it has not made a profile. Check that both are there and try \
         again.",
    )
    .with_context("accepted", accepted.to_string())
    .with_context("needed", needed.to_string())
}

/// A bucket did not have enough evidence to answer for itself.
///
/// A warning rather than a failure, and the *design working* rather than the design failing: a
/// bucket with eight pairs that decided on its own would produce a visibly different look from
/// the bucket beside it on evidence nobody would call sufficient. Section 6.2's shrinkage is
/// what makes the transition continuous, and this is how the photographer finds out which
/// wedding to add.
#[must_use]
pub fn bucket_sparse(bucket: &str, samples: u32, fell_back_to: &str) -> AuraError {
    AuraError::new(
        ML_BUCKET_SPARSE,
        Severity::Warning,
        Recovery::Fallback,
        format!("{bucket}: {samples} pairs, answering from {fell_back_to}"),
        "AURA has not seen enough of your photographs of this kind in this light, so it is \
         using your overall look for them. The profile report says which weddings would help.",
    )
    .with_context("bucket", bucket)
    .with_context("samples", samples.to_string())
    .with_context("fallback", fell_back_to)
}

/// A profile could not be adopted or selected. Nothing was changed.
///
/// `ask_user` for `AURA-ML-5061`'s reason: every case is answered by re-reading the profile
/// list and redrawing the panel.
#[must_use]
pub fn profile_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_PROFILE_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not switch to that style profile. Nothing was changed.",
    )
    .with_context("target", what)
}

/// A bundle was refused before it was parsed into a profile.
///
/// The order of the five checks matters and is part of the defence: size, then magic, then
/// schema, then digest, then signature. Nothing large is ever parsed and nothing from a future
/// build is ever partially read - a partially read profile is a look nobody chose.
#[must_use]
pub fn bundle_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_BUNDLE_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "That style profile file has been altered since it was signed, or it is not a style \
         profile, so AURA did not open it. Ask whoever sent it for a fresh copy.",
    )
}

/// A profile was fitted against a different render engine or a different fitter.
///
/// The ninth version-drift code in the product, and it exists for the reason all of them do: a
/// comparison across versions returns a plausible number that means nothing.
///
/// Degraded rather than refused, and the distinction is real. The profile's *measured* dE00 is
/// about a build that no longer exists, so the number is stale; the delta itself is still the
/// photographer's own lean and is still worth applying, more gently. Adoption of a mismatched
/// profile is refused outright - that is `AURA-ML-5075` - and this fires for one that was
/// already adopted when the engine moved underneath it.
#[must_use]
pub fn style_version_mismatch(
    profile: &str,
    have_engine: &str,
    want_engine: &str,
    have_analysis: u16,
    want_analysis: u16,
) -> AuraError {
    AuraError::new(
        ML_STYLE_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{profile}: fitted against engine {have_engine} at analysis {have_analysis}, this \
             build is engine {want_engine} at analysis {want_analysis}"
        ),
        "This style profile was learned with an older version of AURA's editing engine, so it \
         is being applied cautiously. Re-training it on the same weddings will restore full \
         accuracy.",
    )
    .with_context("profile", profile)
    .with_context("have_engine", have_engine)
    .with_context("want_engine", want_engine)
    .with_context("have_analysis", have_analysis.to_string())
    .with_context("want_analysis", want_analysis.to_string())
}
