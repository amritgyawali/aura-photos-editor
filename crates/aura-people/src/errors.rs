//! The ten failures this crate can have, and what each one falls back to.
//!
//! The codes are split across two ranges, and the split is the point.
//!
//! **`AURA-SEC-90xx`** is everything about the biometric store: the key, the
//! envelope, erasure, project scoping and the edit surface. These are the failures a
//! privacy review asks about, and a support engineer who sees one wants to see all
//! of them together rather than filtered out of a hundred model codes.
//!
//! **`AURA-ML-50xx`** is everything about the models and the algorithms: a frame that
//! would not scan, a version drift, a clustering pass with nothing to cluster, an
//! ambiguous couple. `aura-index` set this precedent in phase 05 - the range follows
//! the concern rather than the crate - and these codes sit next to the embedding
//! ones because they mean the same kind of thing.
//!
//! **Nothing here stops a wedding.** A locked biometric store is a wedding culled
//! without a People panel. An unreadable envelope is one face the grouping ignores.
//! An ambiguous couple is a question in the panel. Invariant 9, ten times: a typed
//! error, a fallback path, and a telemetry event.
//!
//! The two that are *not* soft, and deliberately:
//!
//! * [`erasure_incomplete`] is run-blocking. A half-completed erasure is the one
//!   failure in this domain that must never be quiet: a photographer who pressed
//!   "erase biometric data" and was told it worked has made a promise to a client.
//! * [`cross_project_refused`] halts. It cannot happen through any supported path,
//!   so if it fires, something is wrong that continuing would make worse.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// The project's biometric key is not available.
pub const SEC_BIOMETRIC_LOCKED: ErrorCode = ErrorCode("AURA-SEC-9001");
/// A sealed record failed its authentication tag or carried an unknown version.
pub const SEC_ENVELOPE_UNUSABLE: ErrorCode = ErrorCode("AURA-SEC-9002");
/// Biometric erasure could not finish.
pub const SEC_ERASURE_INCOMPLETE: ErrorCode = ErrorCode("AURA-SEC-9003");
/// Something tried to read one project's people data from another project.
pub const SEC_CROSS_PROJECT: ErrorCode = ErrorCode("AURA-SEC-9004");
/// A merge, split, rename or role change was refused.
pub const SEC_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-SEC-9005");

/// One photograph could not be scanned for faces.
pub const ML_FACE_SCAN_FAILED: ErrorCode = ErrorCode("AURA-ML-5017");
/// Stored faces were produced by a different detector or recogniser version.
pub const ML_FACE_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5018");
/// There were not enough usable faces to form identities.
pub const ML_IDENTITY_UNDERDETERMINED: ErrorCode = ErrorCode("AURA-ML-5019");
/// The couple could not be identified with confidence.
pub const ML_COUPLE_AMBIGUOUS: ErrorCode = ErrorCode("AURA-ML-5020");
/// The project has more faces than the documented in-memory ceiling.
pub const ML_FACE_CEILING: ErrorCode = ErrorCode("AURA-ML-5021");

/// The operating system credential store has no key for this project, or refused.
///
/// Degraded and `AskUser` rather than fatal, because the fallback is complete: every
/// non-people feature works, the catalog opens, the wedding is culled and delivered.
/// What is lost is the People panel, and what the photographer is asked is whether to
/// unlock the store or to start a fresh one - which is a decision only they can make,
/// because starting fresh discards the identities they may have spent an hour naming.
#[must_use]
pub fn biometric_locked(project: &str, reason: &str) -> AuraError {
    AuraError::new(
        SEC_BIOMETRIC_LOCKED,
        Severity::Degraded,
        Recovery::AskUser,
        format!("biometric store for {project} is locked: {reason}"),
        "AURA cannot open this wedding's face data because the key is not available on this \
         computer. Everything else works; open Settings to unlock it or to start a new face \
         index for this project.",
    )
    .with_context("project", project)
    .with_context("reason", reason)
}

/// A sealed record did not authenticate.
///
/// Quarantine rather than retry: the bytes will not change, so retrying is a way of
/// doing nothing twice. One face is dropped from the grouping and the frame keeps its
/// detection - which is the right trade, because a face row that cannot be trusted
/// must never be compared against one that can.
#[must_use]
pub fn envelope_unusable(record: &str, reason: &str) -> AuraError {
    AuraError::new(
        SEC_ENVELOPE_UNUSABLE,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("sealed record {record} could not be opened: {reason}"),
        "AURA could not read one stored face and has set it aside. Grouping continues with the \
         rest; re-analysing this wedding will replace it.",
    )
    .with_context("record", record)
    .with_context("reason", reason)
}

/// Erasure did not complete.
///
/// Run-blocking and retryable. See this module's header: an erasure that half worked
/// and said nothing is a promise broken to somebody who is not in the room.
#[must_use]
pub fn erasure_incomplete(project: &str, remaining: usize, reason: &str) -> AuraError {
    AuraError::new(
        SEC_ERASURE_INCOMPLETE,
        Severity::RunBlocking,
        Recovery::Retry,
        format!("erasure for {project} left {remaining} records: {reason}"),
        "AURA could not finish deleting this wedding's face data, so some of it is still on this \
         computer. Nothing else was changed. Try again, and if it fails a second time the runbook \
         explains how to remove it by hand.",
    )
    .with_context("project", project)
    .with_context("remaining", remaining.to_string())
    .with_context("reason", reason)
}

/// A people query crossed a project boundary.
///
/// Halts. Section 2.2 forbids cross-wedding identity persistence outright and section
/// 6.5 requires it to be impossible rather than merely unlikely, so this code exists
/// to be *unreachable*: every query is scoped by `project_id` and a test asserts there
/// is no path around it. If it ever fires in the field, the safe action is to stop.
#[must_use]
pub fn cross_project_refused(expected: &str, found: &str) -> AuraError {
    AuraError::new(
        SEC_CROSS_PROJECT,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("people data for {found} was requested while {expected} was open"),
        "AURA stopped an operation that would have mixed two weddings' face data. Face data never \
         crosses weddings. Please report this, with the diagnostics bundle.",
    )
    .with_context("expected_project", expected)
    .with_context("found_project", found)
}

/// A merge, split, rename or role change was refused.
#[must_use]
pub fn edit_refused(action: &str, reason: &str) -> AuraError {
    AuraError::new(
        SEC_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("{action} refused: {reason}"),
        "AURA could not make that change to the People panel. Nothing was changed.",
    )
    .with_context("action", action)
    .with_context("reason", reason)
}

/// One frame could not be scanned.
///
/// The frame stays in the catalog, stays visible, and exports normally. It simply has
/// no people data, and `face_scan` has no row for it - so the next pass will try
/// again, which is the correct behaviour for a transient decode failure and harmless
/// for a permanent one.
#[must_use]
pub fn face_scan_failed(photo: &str, reason: &str) -> AuraError {
    AuraError::new(
        ML_FACE_SCAN_FAILED,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("face scan failed for {photo}: {reason}"),
        "AURA could not look for faces in one photograph. It is still in your catalog and still \
         exports normally.",
    )
    .with_context("photo", photo)
    .with_context("reason", reason)
}

/// Stored faces disagree with the model versions this build would use.
///
/// The direct analogue of `AURA-ML-5015` for templates, and it exists for the same
/// reason: comparing a template from one recogniser with a template from another
/// produces a number, and the number is meaningless. Rows are kept, the mismatch is
/// reported, and a background re-scan replaces them.
#[must_use]
pub fn face_version_mismatch(
    stored_detector: u16,
    stored_recogniser: u16,
    expected_detector: u16,
    expected_recogniser: u16,
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_FACE_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} faces were produced by detector {stored_detector} and recogniser \
             {stored_recogniser}; this build uses {expected_detector} and {expected_recogniser}"
        ),
        "AURA has improved how it recognises people, so it is re-reading this wedding's faces in \
         the background. The People panel stays available while it works.",
    )
    .with_context("stored_detector", stored_detector.to_string())
    .with_context("stored_recogniser", stored_recogniser.to_string())
    .with_context("expected_detector", expected_detector.to_string())
    .with_context("expected_recogniser", expected_recogniser.to_string())
    .with_context("rows", rows.to_string())
}

/// Not enough usable faces to form identities.
///
/// A wedding of landscapes, a card imported before the face pass ran, or a project
/// where the quality gate excluded almost everything. The fallback is an empty
/// hierarchy with honest coverage, and every later phase reads
/// `SubjectHierarchy::coverage` rather than assuming.
#[must_use]
pub fn identity_underdetermined(project: &str, voting: usize, needed: usize) -> AuraError {
    AuraError::new(
        ML_IDENTITY_UNDERDETERMINED,
        Severity::Degraded,
        Recovery::Fallback,
        format!("{voting} voting faces in {project}, below the {needed} needed to group anybody"),
        "AURA has not found enough clear faces in this wedding to group people yet. Analyse more \
         photographs, or lower the face quality filter in the People panel.",
    )
    .with_context("project", project)
    .with_context("voting_faces", voting.to_string())
    .with_context("needed", needed.to_string())
}

/// The couple could not be identified with confidence.
///
/// A warning that asks a human, which is exactly what section 12's first failure mode
/// prescribes: "confidence gate plus explicit user confirmation prompt on first
/// Autopilot run". The pipeline continues with the best guess and
/// `SubjectHierarchy::couple_unconfirmed` stays true, so nothing irreversible happens
/// on an unconfirmed couple.
#[must_use]
pub fn couple_ambiguous(project: &str, best: f32, runner_up: f32) -> AuraError {
    AuraError::new(
        ML_COUPLE_AMBIGUOUS,
        Severity::Warning,
        Recovery::AskUser,
        format!(
            "couple candidates in {project} scored {best:.3} and {runner_up:.3}, within the \
             ambiguity margin"
        ),
        "AURA is not sure which two people are the couple at this wedding. Open the People panel \
         and confirm - every later decision depends on getting this right.",
    )
    .with_context("project", project)
    .with_context("best", format!("{best:.3}"))
    .with_context("runner_up", format!("{runner_up:.3}"))
}

/// The project has more faces than the documented in-memory ceiling.
///
/// Section 11 budgets clustering for 25,000 faces. Past the ceiling the skeleton cap
/// in `aura_vision::face::cluster::MAX_SKELETON` still bounds the quadratic work, so
/// the answer is slower and no less correct - but the photographer is told, because a
/// three-day wedding taking a minute to group people needs an explanation rather than
/// a spinner.
#[must_use]
pub fn face_ceiling(faces: usize, ceiling: usize) -> AuraError {
    AuraError::new(
        ML_FACE_CEILING,
        Severity::Degraded,
        Recovery::Fallback,
        format!("{faces} faces exceeds the documented ceiling of {ceiling}"),
        "This wedding has more faces than AURA groups in one pass, so grouping people will take a \
         little longer. The result is the same.",
    )
    .with_context("faces", faces.to_string())
    .with_context("ceiling", ceiling.to_string())
}
