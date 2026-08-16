//! The ten failures this crate can have, and what each one falls back to.
//!
//! Two phases live here now - phase 09's frame integrity and phase 11's composition -
//! and each has the same five shapes, which is the point rather than a coincidence. Both
//! are sets of decisions over stored numbers and measured pixels judged against a
//! versioned table a product manager owns, so both fail in the same five ways: version
//! drift, a refused edit, one item that could not be done, a refused config file, and a
//! config file that is missing a row rather than broken.
//!
//! The two halves are `AURA-ML-5033` to `AURA-ML-5037` and `AURA-ML-5043` to
//! `AURA-ML-5047`, in that order, and the parallel between the two blocks is exact.
//!
//! ## The phase 09 half
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
//!
//! ## The phase 11 half
//!
//! **`AURA-ML-5046` halts**, for `AURA-ML-5036`'s argument moved one phase along. A
//! half-loaded rule table judges the ceremony against measured headroom bands and the
//! reception against neutral ones, and the resulting review queue is sorted by which half
//! of the day a frame came from.
//!
//! `AURA-ML-5047` is the one that is *expected*, and it is the counterpart of
//! `AURA-ML-5037` rather than of `AURA-ML-5023`: a scene arrives in the taxonomy before
//! anybody has written its framing bands, exactly as a camera body ships before anybody
//! has measured its MTF50. The difference from `AURA-ML-5037` is what is substituted -
//! neutral bands rather than a cautious baseline - and it is why the confidence cost is a
//! flat 0.08 here and a per-row penalty there.

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

// ---------------------------------------------------------------------------
// PHASE-11. Composition and aesthetics.
// ---------------------------------------------------------------------------

/// Stored judgements came from different heads, different arithmetic or a different rule
/// table.
pub const ML_COMPOSITION_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5043");
/// A composition dismissal was refused.
pub const ML_COMPOSITION_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5044");
/// One photograph's framing could not be judged.
pub const ML_COMPOSITION_FAILED: ErrorCode = ErrorCode("AURA-ML-5045");
/// The composition rule table was refused.
pub const ML_RULES_REFUSED: ErrorCode = ErrorCode("AURA-ML-5046");
/// A scene has no composition rule row.
pub const ML_SCENE_UNRULED: ErrorCode = ErrorCode("AURA-ML-5047");

/// Stored rows disagree with the running build about a version.
///
/// Degraded rather than fatal, exactly as `AURA-ML-5033` is: the stale judgements keep
/// working while the affected rows are re-analysed in the background, and
/// `CompositionOutline` reports the lowest version present so a caller about to draw a
/// conclusion finds out that the set is mixed.
///
/// All three numbers are in the message because the support engineer's first question is
/// *which* one moved, and the answer changes what has to be redone. A `rules_ver` bump
/// re-compares numbers that are already measured - the headroom is still the headroom,
/// only the band it is judged against moved - whereas a `model_ver` bump re-runs the
/// keypoint pass over every frame.
#[must_use]
pub fn composition_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_COMPOSITION_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} judgements are model {}/analysis {}/rules {}; this build is model \
             {}/analysis {}/rules {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it judges framing, so it is re-checking this wedding in the \
         background. Anything you have already reviewed is kept.",
    )
    .with_context("stale_rows", rows.to_string())
    .with_context("stored_model_ver", stored.0.to_string())
    .with_context("stored_analysis_ver", stored.1.to_string())
    .with_context("stored_rules_ver", stored.2.to_string())
}

/// A dismissal was refused. Nothing was changed.
///
/// `ask_user` rather than a retry, for `AURA-ML-5034`'s reason: every refusal case is
/// answered by re-reading the judgement and redrawing the panel, which is a thing the
/// interface does and not a thing the code can do on its own.
#[must_use]
pub fn composition_edit_refused(what: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_COMPOSITION_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{what}: {why}"),
        "AURA could not change that framing note. Nothing was changed.",
    )
    .with_context("target", what)
}

/// One photograph could not be judged. The pass continues.
///
/// **No row is written**, which is the whole point of the code and the same rule
/// `AURA-ML-5035` states. A frame that could not be analysed must never be stored as a
/// frame that is framed well, because phase 12 and phase 29 both read a clean judgement
/// as evidence. The absence of a row means nobody looked, and the next pass tries it
/// again.
#[must_use]
pub fn composition_failed(photo: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_COMPOSITION_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {detail}"),
        "AURA could not check the framing of one photograph, and has left it unmarked rather \
         than guessing. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// The rule table was refused. Nothing was loaded and nothing was changed.
///
/// **This halts**, and it is the second code in this crate that does. The argument is
/// `AURA-ML-5036`'s, moved one phase along: a half-loaded rule table would judge the
/// ceremony against measured bands and the reception against neutral ones, producing a
/// review queue sorted by which half of the day a frame came from. That looks like "the
/// product hates the reception" and nothing like a config error, which is precisely the
/// class of failure invariant 9 forbids. The loader refuses and leaves the previous table
/// in place.
///
/// The message names the file, the key and the rule, in that order, because that is the
/// order somebody fixes them in. The same shape `AURA-ML-5024`, `AURA-ML-5031`,
/// `AURA-ML-5036` and `AURA-ML-5039` use.
#[must_use]
pub fn rules_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_RULES_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the framing rules that decide what counts as well composed, so it \
         has not marked anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A scene with no rule row. The neutral bands were used and the judgement says so.
///
/// Warning rather than degraded, and the distinction from `AURA-ML-5037` - a camera body
/// with no calibration row - is the same one phase 09 drew against `AURA-ML-5023`: the
/// substitute here is a *neutral* rule row rather than a cautious one, so the wedding is
/// fully judged and the confidence drops by a fixed amount rather than by a per-row
/// penalty.
///
/// It is expected rather than exceptional. `scene_profiles.toml` grows a row whenever a
/// tradition is added, and a scene that reaches this analyser before the product manager
/// has written its framing bands should say so in the panel rather than quietly inherit
/// a `couple_portrait`'s headroom band.
#[must_use]
pub fn scene_unruled(scene: &str) -> AuraError {
    AuraError::new(
        ML_SCENE_UNRULED,
        Severity::Warning,
        Recovery::Fallback,
        format!("no composition rule row for `{scene}`; the neutral bands were used"),
        "AURA has no framing rules recorded for this kind of photograph yet, so it is judging \
         those ones cautiously against neutral rules. They are all still usable.",
    )
    .with_context("scene", scene)
}
