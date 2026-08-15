//! The five failures this crate can have, and what each one falls back to.
//!
//! The same five shapes phases 07 and 08 each had, a third time, and the parallel is
//! not laziness: a technical verdict is a set of decisions over stored numbers and
//! measured pixels, exactly as a story and a grouping are, so it fails in the same five
//! ways. Version drift, a refused edit, one item that could not be done, a refused
//! config file, and - new here - a config file that is *missing a row* rather than
//! broken.
//!
//! **`AURA-ML-5036` halts.** It is the only phase 09 code that stops anything, and it
//! fires when the camera calibration table is refused. Every other failure degrades
//! into a wedding that is still usable: a frame with no verdict, a body judged by the
//! fallback, a dismissal that did not take. A **half-loaded calibration table** is
//! different in kind. Sharpness is normalised by the expected MTF50 for the body, noise
//! by the read noise, and the exposure verdict decided against the measured headroom -
//! so a table that loaded nine bodies out of twenty judges a two-camera wedding by two
//! standards and produces a review queue sorted by which camera took the frame. That
//! looks like "the product hates this camera" and nothing like a config error, which is
//! precisely the class of failure invariant 9 exists to forbid. The loader refuses and
//! leaves the previous table in place.
//!
//! `AURA-ML-5037` is the one that is *expected*. A new body ships every few months and
//! this product cannot have measured it in advance; the code exists so that a wedding
//! judged against a guessed baseline says so, in the panel and in the telemetry, rather
//! than looking like a wedding judged against a measured one.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored verdicts came from different heads, different arithmetic or a different
/// calibration table.
pub const ML_INTEGRITY_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5033");
/// A dismissal was refused.
pub const ML_INTEGRITY_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5034");
/// One photograph could not be checked.
pub const ML_INTEGRITY_FAILED: ErrorCode = ErrorCode("AURA-ML-5035");
/// The camera calibration table was refused.
pub const ML_CALIBRATION_REFUSED: ErrorCode = ErrorCode("AURA-ML-5036");
/// A camera body has no calibration row.
pub const ML_CAMERA_UNCALIBRATED: ErrorCode = ErrorCode("AURA-ML-5037");

/// Stored rows disagree with the running build about a version.
///
/// Degraded rather than fatal: the stale verdicts keep working while the affected rows
/// are re-analysed in the background, and `IntegrityOutline` reports the lowest version
/// present so a caller about to draw a conclusion finds out that the set is mixed.
///
/// All three numbers are in the message because the support engineer's first question
/// is *which* one moved, and the answer changes what has to be redone: a `calib_ver`
/// bump re-normalises numbers already measured, a `model_ver` bump re-runs the pass.
#[must_use]
pub fn integrity_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_INTEGRITY_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} verdicts are model {}/analysis {}/calibration {}; this build is model \
             {}/analysis {}/calibration {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it judges focus, motion and exposure, so it is re-checking this \
         wedding in the background. Anything you have already reviewed is kept.",
    )
    .with_context("stale_rows", rows.to_string())
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("stored_analysis_ver", stored.1.to_string())
    .with_context("stored_calib_ver", stored.2.to_string())
}

/// A dismissal was refused. Nothing was changed.
///
/// `ask_user` rather than a retry: all three refusal cases are answered by re-reading
/// the verdict and redrawing the panel, which is a thing the interface does and not a
/// thing the code can do on its own.
#[must_use]
pub fn integrity_edit_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_INTEGRITY_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not change that technical note. Nothing was changed.",
    )
    .with_context("target", what)
}

/// One photograph could not be checked. The pass continues.
///
/// **No row is written**, which is the whole point of the code. A frame that could not
/// be analysed must never be stored as a frame with nothing wrong, because phase 12
/// reads "nothing wrong" as evidence. The absence of a row means nobody looked, and the
/// next pass tries it again - right for a transient decode failure and harmless for a
/// permanent one.
#[must_use]
pub fn integrity_failed(photo: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_INTEGRITY_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {detail}"),
        "AURA could not check one photograph for sharpness and exposure, and has left it \
         unmarked rather than guessing. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// The calibration table was refused. Nothing was loaded and nothing was changed.
///
/// The message names the file, the key and the rule, in that order, because that is the
/// order somebody fixes them in. The same shape `AURA-ML-5024` and `AURA-ML-5031` use.
#[must_use]
pub fn calibration_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_CALIBRATION_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the measurements that decide what counts as sharp on your camera, \
         so it has not marked anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A body with no row. The fallback was used and the verdict says so.
///
/// Warning rather than degraded, and the distinction from `AURA-ML-5023` - a scene with
/// no profile - is worth stating: a missing scene profile substitutes a *neutral*
/// judgement, whereas a missing calibration row substitutes a *cautious* one and lowers
/// the confidence of every verdict it produces. The wedding is fully analysed either
/// way.
#[must_use]
pub fn camera_uncalibrated(make: &str, model: &str) -> AuraError {
    AuraError::new(
        ML_CAMERA_UNCALIBRATED,
        Severity::Warning,
        Recovery::Fallback,
        format!("no calibration row for {make} {model}; normalised by sensor resolution alone"),
        "AURA has not measured this camera model yet, so it is judging sharpness and noise more \
         cautiously on those photographs. They are all still usable.",
    )
    .with_context("make", make)
    .with_context("model", model)
}
