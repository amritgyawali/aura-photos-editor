//! The six failures this crate can have, and what each one falls back to.
//!
//! Five of the six are soft, and the sixth is the interesting one.
//!
//! **`AURA-ML-5024` halts.** It is the only phase 07 code that stops anything,
//! and it fires when a config file is refused. Every other failure here degrades
//! into a wedding that is still usable: a frame with no scene, a chapter with no
//! profile, a timeline segmented by time gaps alone. A **half-loaded threshold
//! table** is different in kind. It silently changes every downstream number - what
//! may be noisy, what may be soft, what must be covered - and it does so without
//! anybody noticing, which is precisely the class of failure invariant 9 exists to
//! forbid. So the loader refuses and leaves the previous table in place.
//!
//! The other five follow phase 05's and phase 06's precedent for the `AURA-ML-50xx`
//! range: the code follows the concern rather than the crate, and these sit next to
//! the embedding and face codes because they mean the same kind of thing.
//!
//! `AURA-ML-5022` deserves one more sentence. It is the third version-drift code in
//! the product - `AURA-ML-5015` for embeddings, `AURA-ML-5018` for faces - and it
//! exists for the same reason as the other two: comparing a number produced under one
//! version with a number produced under another returns a plausible answer that means
//! nothing, and the only defence is to make the comparison impossible rather than
//! discouraged.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored scene labels came from a different classifier, taxonomy or trunk.
pub const ML_SCENE_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5022");
/// A scene has no profile, so neutral tolerances were substituted.
pub const ML_PROFILE_MISSING: ErrorCode = ErrorCode("AURA-ML-5023");
/// A scene profile or ritual taxonomy file was refused.
pub const ML_CONFIG_REFUSED: ErrorCode = ErrorCode("AURA-ML-5024");
/// A chapter rename, split, merge or boundary move was refused.
pub const ML_STORY_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5025");
/// The timeline could not be split into a plausible number of chapters.
pub const ML_SEGMENTATION_IMPLAUSIBLE: ErrorCode = ErrorCode("AURA-ML-5026");
/// One photograph could not be classified.
pub const ML_SCENE_FAILED: ErrorCode = ErrorCode("AURA-ML-5027");

/// Stored rows disagree with the running build about a version.
///
/// Degraded rather than fatal: the stale labels keep working while the affected rows
/// are re-classified in the background, and `StoryOutline::scene_ver` lets a caller
/// that is about to draw a conclusion find out that the set is mixed.
///
/// The four version numbers are all in the message because a support engineer's first
/// question is *which* one moved, and the answer changes what has to be redone: a
/// taxonomy bump re-reads a slug, an `embed_ver` bump re-runs the whole pass.
#[must_use]
pub fn scene_version_mismatch(
    stored: (u16, u16, u16, u16),
    current: (u16, u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_SCENE_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} scene rows are model {}/preprocess {}/taxonomy {}/embed {}; this build is \
             model {}/preprocess {}/taxonomy {}/embed {}",
            stored.0, stored.1, stored.2, stored.3, current.0, current.1, current.2, current.3
        ),
        "AURA has improved how it reads a wedding's story, so it is re-labelling this wedding in \
         the background. The timeline stays available while it works.",
    )
    .with_context("stale_rows", rows.to_string())
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("current_model_ver", current.0.to_string())
}

/// A scene with no profile row. Neutral tolerances were used.
///
/// Warning rather than degraded: the substitution is complete and documented, every
/// later phase reads the substituted profile like any other, and the wedding is judged
/// consistently - just not specifically.
#[must_use]
pub fn profile_missing(scene: &str) -> AuraError {
    AuraError::new(
        ML_PROFILE_MISSING,
        Severity::Warning,
        Recovery::Fallback,
        format!("no scene profile for `{scene}`; neutral tolerances substituted"),
        "AURA has no tuned settings for one kind of photograph in this wedding and is judging it \
         neutrally. Results are still usable; the runbook explains how to add the missing \
         settings.",
    )
    .with_context("scene", scene)
}

/// A config file was refused. Nothing was loaded and nothing was changed.
///
/// The message names the file, the key and the rule, in that order, because that is
/// the order somebody fixes them in.
#[must_use]
pub fn config_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_CONFIG_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide how each kind of wedding photograph is \
         judged, so it has not changed anything. Restore the file or reinstall; the runbook \
         explains what is wrong with it.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A chapter edit was refused. Nothing was written.
#[must_use]
pub fn edit_refused(action: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_STORY_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{action} refused: {why}"),
        "AURA could not make that change to the timeline. Nothing was changed.",
    )
    .with_context("action", action)
}

/// The penalty search never landed inside the chapter band.
///
/// Carries the bounds it searched and what each end produced, because two counts that
/// jump from 3 to 27 across one step mean the signal has one dominant break and no
/// structure - which is a different problem from a penalty that is merely mistuned.
#[must_use]
pub fn segmentation_implausible(
    low: (f32, usize),
    high: (f32, usize),
    band: (usize, usize),
) -> AuraError {
    AuraError::new(
        ML_SEGMENTATION_IMPLAUSIBLE,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "penalty search found {} chapters at {:.3} and {} at {:.3}, never inside {}..{}; fell \
             back to time gaps only",
            low.1, low.0, high.1, high.0, band.0, band.1
        ),
        "AURA could not divide this wedding into a sensible set of chapters, so it has made one \
         chapter per clear break in the day. Open the timeline and adjust the boundaries; your \
         edits are kept.",
    )
    .with_context("chapters_at_low_penalty", low.1.to_string())
    .with_context("chapters_at_high_penalty", high.1.to_string())
}

/// One photograph could not be classified.
///
/// Nothing is written for that frame, deliberately: an `unknown` row would look like a
/// completed classification and the next pass would skip it. A missing row is retried.
#[must_use]
pub fn scene_failed(photo: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_SCENE_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {why}"),
        "AURA could not tell what one photograph is of and has left it unlabelled. Everything \
         else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}
