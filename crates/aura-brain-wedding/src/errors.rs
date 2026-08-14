//! The eleven failures this crate can have, and what each one falls back to.
//!
//! Six are phase 07's, five are phase 08's, and the two halves are the same shape:
//! everything degrades except a refused config file.
//!
//! Of phase 07's six, five are soft and the sixth is the interesting one.
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

// -- PHASE-08 ----------------------------------------------------------------
//
// The same five shapes, one phase later, and the parallel is not an accident: a
// grouping is a set of decisions over stored numbers, exactly as a story is, so it
// fails in the same five ways. Version drift, a refused edit, an implausible result,
// a refused config file and one item that could not be done.
//
// `AURA-ML-5031` is the only one that halts, for `AURA-ML-5024`'s reason: a
// half-loaded threshold table silently changes how a whole wedding is grouped.

/// Stored moments were built from a different embedding, grouper or threshold table.
pub const ML_MOMENT_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5028");
/// A split, merge, lock or keep-hint change was refused.
pub const ML_MOMENT_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5029");
/// The grouping produced a moment count nobody would recognise as this wedding.
pub const ML_GROUPING_IMPLAUSIBLE: ErrorCode = ErrorCode("AURA-ML-5030");
/// A moment profile file was refused.
pub const ML_MOMENT_CONFIG_REFUSED: ErrorCode = ErrorCode("AURA-ML-5031");
/// One photograph could not be placed in a moment.
pub const ML_MOMENT_FAILED: ErrorCode = ErrorCode("AURA-ML-5032");

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

// -- PHASE-08 ----------------------------------------------------------------

/// Stored moments were built under different versions than this build uses.
///
/// Degraded rather than fatal: the stored moments keep working - a stack in the grid is
/// still a stack - while the project is re-grouped in the background.
///
/// All three numbers are in the message because a support engineer's first question is
/// *which* one moved, and the answer changes what has to be redone. An `embed_ver` bump
/// invalidates every distance and therefore the whole graph; a `profile_ver` bump only
/// invalidates the comparison the edges were put through, which is cheaper; a
/// `group_ver` bump is a code change and re-runs everything.
#[must_use]
pub fn moment_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    moments: usize,
) -> AuraError {
    AuraError::new(
        ML_MOMENT_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{moments} moments were grouped with embed {}/group {}/profile {}; this build is \
             embed {}/group {}/profile {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it groups repeated frames, so it is regrouping this wedding in the \
         background. Your own splits and merges are kept.",
    )
    .with_context("stale_moments", moments.to_string())
    .with_context("stored_embed_ver", stored.0.to_string())
    .with_context("current_embed_ver", current.0.to_string())
}

/// A grouping edit was refused. Nothing was written.
///
/// `AskUser` rather than `Retry`, because every refusal here is a statement about what
/// the photographer asked for - a moment that is not there, a split that would leave a
/// side empty, two moments in different weddings - and repeating it changes nothing.
#[must_use]
pub fn moment_edit_refused(action: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_MOMENT_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{action} refused: {why}"),
        "AURA could not make that change to the grouping. Nothing was changed.",
    )
    .with_context("action", action)
}

/// The grouping produced a moment count nobody would recognise as this wedding.
///
/// Section 0's headline: 3,000 files become roughly 700 to 1,100 moments, which is
/// between 2.7 and 4.3 frames each. This fires outside a much wider band than that -
/// see `moments::graph::PLAUSIBLE_MEAN_SIZE` - because a photographer who shot a whole
/// wedding on single frames really does have one frame per moment, and a sports-style
/// shooter really does have twelve.
///
/// Degraded rather than fatal, and the moments are still written: a grouping nobody
/// trusts is still better than no grouping, because every stack can be split by hand
/// and the alternative is a grid of three thousand loose files. What the code carries
/// is the number, so the Problems panel can say what happened rather than leaving the
/// photographer to notice.
#[must_use]
pub fn grouping_implausible(images: usize, moments: usize, mean_size: f32) -> AuraError {
    AuraError::new(
        ML_GROUPING_IMPLAUSIBLE,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{images} frames became {moments} moments, {mean_size:.1} frames each, which is \
             outside the plausible band"
        ),
        "AURA grouped this wedding into an unusual number of moments. The grouping is still \
         available and every stack can be split or merged by hand; the runbook explains what \
         usually causes this.",
    )
    .with_context("images", images.to_string())
    .with_context("moments", moments.to_string())
}

/// A moment profile file was refused. Nothing was loaded and nothing was changed.
///
/// The one phase 08 code that halts, and it halts for `AURA-ML-5024`'s reason: a
/// half-loaded threshold table silently changes how an entire wedding is grouped, and
/// invariant 9 exists to forbid exactly that class of failure. The previous table stays
/// in place.
///
/// The message names the file, the key and the rule, in the order somebody fixes them.
#[must_use]
pub fn moment_config_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_MOMENT_CONFIG_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide which frames belong to the same moment, so \
         it has not changed anything. Restore the file or reinstall; the runbook explains what is \
         wrong with it.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// One photograph could not be placed in a moment.
///
/// Almost always a frame with no embedding or no timeline time: grouping needs both,
/// and phase 05 or phase 01 is where it is missing from. The frame is left ungrouped
/// rather than made a moment of one, because a moment of one that was never compared
/// with anything is a claim this phase has not earned - and `v_moment_coverage` reports
/// it honestly instead.
#[must_use]
pub fn moment_failed(photo: &str, why: &str) -> AuraError {
    AuraError::new(
        ML_MOMENT_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {why}"),
        "AURA could not work out which moment one photograph belongs to and has left it on its \
         own. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}
