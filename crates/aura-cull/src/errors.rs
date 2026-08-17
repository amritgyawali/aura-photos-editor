//! The six codes phase 12 adds, `AURA-ML-5048` to `AURA-ML-5053`.
//!
//! The block has the same shape as phase 09's `5033..5037` and phase 11's `5043..5047` -
//! a version-drift code, an edit-refused code, a per-item failure, a config refusal and a
//! warning about a gap - with one addition, because this phase has two config files rather
//! than one and losing either of them fails differently.
//!
//! ## Two halting codes, and they are not interchangeable
//!
//! `AURA-ML-5051` and `AURA-ML-5052` both halt, and both would be a single "config
//! refused" code in a smaller product. They are separate because the *question a support
//! engineer asks next* is different. A refused weight table means nothing has been scored
//! and the fix is a file restore. A refused rule table means the guarantees are not loaded,
//! and the correct response is to refuse to deliver anything at all until they are -
//! because a gallery culled with weights but without must-haves is a gallery that looks
//! finished and is missing the vows.
//!
//! ## `AURA-ML-5050` is a warning about a denominator, not about a photograph
//!
//! It is the only code in this crate raised about frames that were never *considered*.
//! Phase 11's `AURA-ML-5045` says one photograph could not be judged; this says a number
//! of photographs were not eligible, and the number is the point. A cull over 60 % of a
//! wedding produces a gallery that looks exactly like a cull over 100 % of it, and this is
//! the only place the difference is announced.
//!
//! ## `AURA-ML-5053` is expected, and it is still worth a code
//!
//! Not every wedding has a cake. The counterpart of phase 11's `AURA-ML-5047`: a code that
//! fires in normal operation, is not a defect, and exists so that the *absence* is
//! reported as a fact rather than absorbed into silence. Section 6.3: the product cannot
//! invent coverage and must not imply that it did.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// A stored selection was made from different scores, passes, calibration or config.
pub const ML_SELECTION_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5048");
/// A culling override or a re-selection was refused.
pub const ML_CULL_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5049");
/// Photographs had no analysis and could not be considered.
pub const ML_CULL_NOT_ELIGIBLE: ErrorCode = ErrorCode("AURA-ML-5050");
/// The culling weight table was refused.
pub const ML_WEIGHTS_REFUSED: ErrorCode = ErrorCode("AURA-ML-5051");
/// The coverage rule table was refused.
pub const ML_COVERAGE_RULES_REFUSED: ErrorCode = ErrorCode("AURA-ML-5052");
/// A must-have has no candidate photographs.
pub const ML_MUST_HAVE_MISSING: ErrorCode = ErrorCode("AURA-ML-5053");

/// A stored selection no longer answers the question this build would ask.
///
/// Degraded rather than fatal, exactly as `AURA-ML-5033` and `AURA-ML-5043` are: the
/// stored gallery keeps working while the project is re-selected in the background, and
/// every hand-made override survives because it lives in its own table.
///
/// The message carries both triples *and* both config digests, because the support
/// engineer's first question is which of the four moved and the answer changes what has to
/// be redone. A calibration bump re-fuses numbers that are already measured; a `model_ver`
/// bump means the sub-scores themselves are stale and phases 09 to 11 have to run first.
#[must_use]
pub fn selection_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    stored_digest: &str,
    current_digest: &str,
) -> AuraError {
    AuraError::new(
        ML_SELECTION_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "stored selection is model {}/analysis {}/calibration {} with config {}; this \
             build is model {}/analysis {}/calibration {} with config {}",
            stored.0,
            stored.1,
            stored.2,
            short(stored_digest),
            current.0,
            current.1,
            current.2,
            short(current_digest),
        ),
        "AURA has improved how it chooses photographs, so this gallery is being worked out \
         again. Anything you kept or removed by hand is kept.",
    )
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("stored_analysis_ver", stored.1.to_string())
    .with_context("stored_calibration_ver", stored.2.to_string())
    .with_context("stored_config", short(stored_digest))
    .with_context("current_config", short(current_digest))
}

/// An override, a resize or a mode change was refused. Nothing was changed.
///
/// `ask_user` rather than a retry, for `AURA-ML-5034`'s and `AURA-ML-5044`'s reason: every
/// refusal here is answered by re-reading the selection and redrawing the panel, which is
/// something the interface does and not something the code can do on its own.
#[must_use]
pub fn cull_edit_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_CULL_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not change that choice. The gallery is unchanged.",
    )
    .with_context("target", what)
}

/// Photographs were left out because nobody had analysed them.
///
/// **Not a rejection**, and the distinction is the reason the code exists. A frame with no
/// phase 09 verdict is `CullCode::NotAnalysed`: it carries a reason saying nobody looked,
/// it is counted in `CullOutline::eligible` rather than in the gallery, and no part of the
/// interface may render it as a judgement.
///
/// A warning rather than an item failure because it is about the *set*: one unanalysed
/// frame is noise and four hundred is a gallery of half a wedding, and only the count can
/// tell them apart.
#[must_use]
pub fn not_eligible(photos: usize, of: usize) -> AuraError {
    AuraError::new(
        ML_CULL_NOT_ELIGIBLE,
        Severity::Warning,
        Recovery::Fallback,
        format!("{photos} of {of} photographs have no technical verdict and were not considered"),
        "Some photographs have not been checked yet, so AURA has left them out of this \
         gallery rather than judging them. Run the analysis and choose again to include \
         them.",
    )
    .with_context("not_analysed", photos.to_string())
    .with_context("photos", of.to_string())
}

/// The weight table was refused. Nothing was loaded and nothing was scored.
///
/// **This halts.** A half-loaded weight table would fuse the ceremony with measured
/// weights and the reception with neutral ones, and the gallery that came out would be
/// culled differently by time of day - which reads as a product opinion about receptions
/// and nothing like a configuration error. `AURA-ML-5036`'s and `AURA-ML-5046`'s argument,
/// one phase later and with a gallery rather than a review queue at stake.
///
/// The message names the file, the key and the rule, in that order, because that is the
/// order somebody fixes them in.
#[must_use]
pub fn weights_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_WEIGHTS_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide which photographs are delivered, so \
         it has not chosen anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// The coverage rule table was refused. Nothing was loaded and nothing was chosen.
///
/// **This halts, and it is the stricter of the two.** `coverage_rules.toml` is the file
/// that guarantees the vows, the rings and the kiss are in the gallery. A partially loaded
/// rule table would drop a guarantee silently, and a dropped guarantee is invisible until
/// a customer finds it - which section 12 names as the failure that loses the customer
/// forever. So an unknown must-have slug is a refusal rather than a default: `MustHave` is
/// the one vocabulary in this workspace with no `from_str_or_*`, for exactly this reason.
#[must_use]
pub fn coverage_rules_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_COVERAGE_RULES_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the rules that guarantee the important parts of the day are in \
         the gallery, so it has not chosen anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A must-have found no candidates at all.
///
/// Expected, frequently correct, and still worth announcing. The counterpart of phase 11's
/// `AURA-ML-5047`. Its one job is to make the *absence* a reported fact: the coverage panel
/// shows `missing` with "no candidates found", and nothing anywhere fills the gap with a
/// frame from somewhere else.
#[must_use]
pub fn must_have_missing(rule: &str) -> AuraError {
    AuraError::new(
        ML_MUST_HAVE_MISSING,
        Severity::Warning,
        Recovery::AskUser,
        format!("`{rule}` has no candidate photographs in this project"),
        "AURA could not find any photographs of one of the moments a wedding gallery \
         normally contains. It has said so rather than filling the gap with something \
         else.",
    )
    .with_context("rule", rule)
}

/// The first eight characters of a digest, for a message a human reads.
fn short(digest: &str) -> String {
    digest.chars().take(8).collect()
}
