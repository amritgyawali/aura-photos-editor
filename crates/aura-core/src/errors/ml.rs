//! Model registry and inference constructors. Codes 5000-5999, registered in
//! `errors.toml`.
//!
//! Two rules shape this domain.
//!
//! The first is that **integrity failures are refusals, not fallbacks**. A model
//! whose signature or digest does not verify is never loaded "just this once":
//! model files are the one artefact in the product that arrives over a network
//! and is then executed, so Article IX rule S6 makes the verification order -
//! signature, digest, operator support, load - a security boundary rather than a
//! convenience check.
//!
//! The second is that **a missing capability is a degradation**. An operator we
//! cannot run, or a model that is not installed, leaves the caller with its
//! documented heuristic fallback and the wedding still finishes.

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// The requested model and version are not pinned in `models.lock`.
pub const ML_MODEL_UNKNOWN: ErrorCode = ErrorCode("AURA-ML-5001");
/// The detached manifest signature did not verify.
pub const ML_SIGNATURE_INVALID: ErrorCode = ErrorCode("AURA-ML-5002");
/// A model file's sha256 did not match the signed manifest.
pub const ML_DIGEST_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5003");
/// A transfer ended early; the partial file is resumable.
pub const ML_TRANSFER_INCOMPLETE: ErrorCode = ErrorCode("AURA-ML-5004");
/// The model card named by the manifest is missing.
pub const ML_CARD_MISSING: ErrorCode = ErrorCode("AURA-ML-5005");
/// The graph uses an operator or opset this build does not implement.
pub const ML_OP_UNSUPPORTED: ErrorCode = ErrorCode("AURA-ML-5006");
/// The caller's tensor disagrees with the manifest's declared input.
pub const ML_SHAPE_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5007");
/// The request did not finish inside its deadline.
pub const ML_DEADLINE_EXCEEDED: ErrorCode = ErrorCode("AURA-ML-5008");
/// A new version failed its first real use and the previous one is active again.
pub const ML_ROLLED_BACK: ErrorCode = ErrorCode("AURA-ML-5009");
/// The bytes verified but the model file could not be parsed.
pub const ML_PARSE_FAILED: ErrorCode = ErrorCode("AURA-ML-5010");
/// The photographer stopped the work.
pub const ML_CANCELLED: ErrorCode = ErrorCode("AURA-ML-5011");
/// A delta update could not be applied and the full file is needed.
pub const ML_DELTA_FAILED: ErrorCode = ErrorCode("AURA-ML-5012");
/// A tone curve was refused because its control points are not monotone. PHASE-16.
pub const ML_CURVE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5066");

/// PHASE-23. One photograph's geometry could not be planned.
pub const ML_GEOMETRY_FAILED: ErrorCode = ErrorCode("AURA-ML-5092");
/// A cleanup proposal broke one of phase 24's own guarantees, so it was refused. PHASE-24.
pub const ML_CLEANUP_PROPOSAL_REFUSED: ErrorCode = ErrorCode("AURA-ML-5116");
/// A photographer's cleanup decision could not be recorded. PHASE-24.
pub const ML_CLEANUP_OVERRIDE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5117");

/// Asked for a model that no signed entry pins.
#[must_use]
pub fn model_unknown(name: &str, version: &str) -> AuraError {
    AuraError::new(
        ML_MODEL_UNKNOWN,
        Severity::RunBlocking,
        Recovery::AskUser,
        format!("no registry entry for {name} {version}"),
        "This feature needs an AI model that is not installed. Install the model pack from \
         Settings, then try again.",
    )
    .with_context("model", name)
    .with_context("version", version)
}

/// The signature over `models.lock` did not verify. Treat as a security event.
#[must_use]
pub fn signature_invalid(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_SIGNATURE_INVALID,
        Severity::RunBlocking,
        Recovery::AskUser,
        detail,
        "The AI model files did not pass their safety check, so AURA refused to load them. \
         Re-download the model pack from Settings.",
    )
}

/// A file's digest is not the digest the signed manifest promised.
#[must_use]
pub fn digest_mismatch(file: &str, expected: &str, actual: &str) -> AuraError {
    AuraError::new(
        ML_DIGEST_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!("{file}: expected sha256 {expected}, found {actual}"),
        "One AI model file was damaged. AURA kept using the version that already worked and left \
         the damaged one unused.",
    )
    .with_context("file", file)
}

/// The payload stopped short. The `.part` file stays and the transfer resumes.
#[must_use]
pub fn transfer_incomplete(file: &str, have: u64, want: u64) -> AuraError {
    AuraError::new(
        ML_TRANSFER_INCOMPLETE,
        Severity::Warning,
        Recovery::Retry,
        format!("{file}: {have} of {want} bytes"),
        "An AI model was only partly copied. AURA will carry on from where it stopped; nothing \
         already installed was touched.",
    )
    .with_context("file", file)
    .with_context("have_bytes", have.to_string())
    .with_context("want_bytes", want.to_string())
}

/// No model card. Article VI rule M1 makes this a refusal, not a warning.
#[must_use]
pub fn card_missing(name: &str, card_path: &str) -> AuraError {
    AuraError::new(
        ML_CARD_MISSING,
        Severity::RunBlocking,
        Recovery::AskUser,
        format!("{name}: model card {card_path} is missing or empty"),
        "An AI model arrived without its documentation, so AURA refused to use it. Re-install the \
         model pack from Settings.",
    )
    .with_context("model", name)
}

/// An operator outside the documented subset. Named, at load time, always.
#[must_use]
pub fn op_unsupported(op: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_OP_UNSUPPORTED,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "One AI model needs a capability this version of AURA does not have. That step was \
         skipped and the rest of the work continues.",
    )
    .with_context("op", op)
}

/// The tensor handed in is not the tensor the manifest declared.
#[must_use]
pub fn shape_mismatch(expected: &str, actual: &str) -> AuraError {
    AuraError::new(
        ML_SHAPE_MISMATCH,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("expected input {expected}, got {actual}"),
        "One photo could not be prepared for the AI step and was set aside. The rest of the batch \
         continues.",
    )
    .with_context("expected", expected)
    .with_context("actual", actual)
}

/// The deadline elapsed. Queue time and run time are logged separately.
#[must_use]
pub fn deadline_exceeded(model: &str, elapsed_ms: u64, deadline_ms: u64) -> AuraError {
    AuraError::new(
        ML_DEADLINE_EXCEEDED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{model}: {elapsed_ms} ms against a {deadline_ms} ms deadline"),
        "One AI step took longer than allowed and was stopped. AURA will try it again later in \
         the run.",
    )
    .with_context("model", model)
    .with_context("elapsed_ms", elapsed_ms.to_string())
}

/// A version failed its first real use and the previous one is active again.
#[must_use]
pub fn rolled_back(name: &str, from: &str, to: &str) -> AuraError {
    AuraError::new(
        ML_ROLLED_BACK,
        Severity::Degraded,
        Recovery::Fallback,
        format!("{name}: {from} failed first use, restored {to}"),
        "A newly installed AI model did not work, so AURA went back to the version that did. \
         Nothing in your catalog was affected.",
    )
    .with_context("model", name)
    .with_context("from_version", from)
    .with_context("to_version", to)
}

/// Verified bytes that the parser still cannot read: a bad artefact.
#[must_use]
pub fn parse_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_PARSE_FAILED,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "One AI model file could not be read. AURA kept the version that already worked and \
         reported the problem.",
    )
}

/// Cancellation is an outcome we record, never an outcome we infer.
#[must_use]
pub fn cancelled(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_CANCELLED,
        Severity::Warning,
        Recovery::Halt,
        detail,
        "The AI step was stopped. Everything finished so far has been saved.",
    )
}

/// The delta did not reconstruct the target. Fall back to the whole file.
#[must_use]
pub fn delta_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_DELTA_FAILED,
        Severity::Warning,
        Recovery::Retry,
        detail,
        "A small model update could not be applied, so AURA will fetch the whole model instead. \
         The version you have keeps working.",
    )
}

/// A tone curve's control points are not monotone, so the curve was refused.
///
/// PHASE-16 section 6.1: "monotonicity is enforced structurally, so no AI decision can
/// ever produce a posterised or inverted curve". This is the refusal that makes that
/// sentence true, and it lives in `aura-core` rather than in the phase because
/// `contract::colour::ToneCurve::new` is the only constructor and the contract cannot
/// depend on the crate that implements it.
///
/// It halts nothing. A refused curve leaves the frame with the identity curve and the
/// caller records the withdrawal, because a photograph rendered through a curve nobody
/// checked is worse than a photograph rendered through no curve at all.
#[must_use]
pub fn curve_refused(detail: impl Into<String>, points: usize) -> AuraError {
    AuraError::new(
        ML_CURVE_REFUSED,
        Severity::ItemFailed,
        Recovery::Fallback,
        detail,
        "AURA worked out a tone curve it could not use safely, so it left this photograph's \
         curve alone. Nothing else about the edit changed.",
    )
    .with_context("points", points.to_string())
}

/// A geometry plan broke one of phase 23's own guarantees, so it was refused.
///
/// PHASE-23 section 6.3's hard constraints, restated as a post-condition. It lives in
/// `aura-core` rather than in `aura-geometry` because
/// `contract::geometry::Keystone::new` is the only constructor of a capped keystone and the
/// contract cannot depend on the crate that implements it - the same reason
/// [`curve_refused`] is here.
///
/// **A refused plan is stored as no plan rather than as a weak one.** Four of the six
/// clauses it reports are the crop safety filter, and a plan that fails one of those is a
/// delivered photograph with somebody's hands cropped off it.
#[must_use]
pub fn geometry_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_GEOMETRY_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "AURA could not work out the lens corrections and framing for one photograph, so it \
         has left it exactly as it was shot. Everything else in this wedding is unaffected.",
    )
}

/// A cleanup proposal broke one of phase 24's own guarantees, so it was refused.
///
/// PHASE-24 section 5, restated as a post-condition on the only constructor. It lives in
/// `aura-core` rather than in `aura-generative` because `contract::cleanup::CleanupProposal::new`
/// is the only way to make one and the contract cannot depend on the crate that implements it -
/// the same reason [`curve_refused`] and [`geometry_failed`] are here.
///
/// **The clause that matters most is the first one.** A proposal whose safety verdict is absent,
/// malformed or not `allowed` cannot be constructed at all, which is what makes "the safety filter
/// runs before the score" a property of the type rather than an ordering in a function somebody
/// could reorder. ADR-0049 section 2.
#[must_use]
pub fn cleanup_proposal_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_CLEANUP_PROPOSAL_REFUSED,
        Severity::ItemFailed,
        Recovery::Fallback,
        detail,
        "AURA found something it might have tidied out of one photograph and could not show that          removing it was safe, so it left the photograph alone. Nothing has been changed.",
    )
}

/// A photographer's cleanup decision could not be recorded.
///
/// Accepting or rejecting a proposal is the only thing a person says on this surface - there is no
/// strength, no size and no description - so a refused override is always one of two things: it
/// named no proposal, or it asked for nothing.
#[must_use]
pub fn cleanup_override_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_CLEANUP_OVERRIDE_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not record that decision about tidying this photograph. The photograph is          unchanged.",
    )
}

/// A photographer's anchor decision could not be recorded.
pub const ML_GALLERY_ANCHOR_REFUSED: ErrorCode = ErrorCode("AURA-ML-5124");

/// A photographer's gallery override could not be recorded.
pub const ML_GALLERY_OVERRIDE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5125");

/// A photographer's anchor decision could not be recorded.
///
/// The frozen `GalleryService` documents this on `pin_anchor` and `reject_anchor`, so it lives here
/// rather than in `aura-brain-gallery`: a contract cannot depend on the crate that implements it.
/// The same split phases 16, 22, 23 and 24 made.
///
/// Three ways to reach it, and all three are the panel and the catalog disagreeing about the tree
/// rather than anything a photographer did wrong: the node is gone, the photograph is no longer in
/// it, or pinning would leave the node with more anchors than `MAX_ANCHORS`.
#[must_use]
pub fn gallery_anchor_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_GALLERY_ANCHOR_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not record that choice of reference photograph. Nothing about this part of \
         the wedding has changed; reopen the panel and try again.",
    )
}

/// A photographer's gallery override could not be recorded.
///
/// The values on this surface are five movements, every one of them bounded by the contract, so a
/// refused override is one of three things: the photograph has no delta, the override asked for
/// nothing, or a value was outside its bound. There is no strength field and no way to raise a
/// bound, which is phase 21's rule applied to a surface a photographer touches.
#[must_use]
pub fn gallery_override_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_GALLERY_OVERRIDE_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not record that adjustment. The photograph is unchanged.",
    )
}

/// A photographer's camera-matching decision could not be recorded.
pub const ML_CAMERA_DECISION_REFUSED: ErrorCode = ErrorCode("AURA-ML-5131");

/// A photographer's camera-matching decision could not be recorded.
///
/// The frozen `CameraMatchService` documents this on `set_reference`, `set_enabled` and
/// `set_override`, so it lives here rather than in `aura-brain-gallery`: a contract cannot depend on
/// the crate that implements it. The same split phases 16, 22, 23, 24 and 25 made.
///
/// Four ways to reach it, and none of them is anything a photographer did wrong: the body is not
/// in the project, the body shot no photographs and so cannot be a reference, the body has no
/// transform to override, or a value was outside its documented bound. There is no strength field
/// on the surface and no way to raise a bound - phase 21's rule, applied where a photographer
/// touches it.
#[must_use]
pub fn camera_decision_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_CAMERA_DECISION_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not record that camera choice. Nothing about the photographs from that camera \
         has changed; reopen the panel and try again.",
    )
}

/// A photographer's quality-control verdict could not be recorded.
pub const ML_QC_DECISION_REFUSED: ErrorCode = ErrorCode("AURA-ML-5137");

/// A photographer's quality-control verdict could not be recorded.
///
/// The frozen `QcService` documents this on `decide`, so it lives here rather than in `aura-qc`:
/// a contract cannot depend on the crate that implements it. The same split phases 16, 22, 23, 24,
/// 25 and 26 made.
///
/// Three ways to reach it, and none of them is anything a photographer did wrong: the ticket is not
/// in the catalog, the override named a status automation owns rather than one a person owns, or
/// the note was longer than `QcOverride::MAX_NOTE`.
///
/// The second is the interesting one. `accepted` and `dismissed` are the two statuses a person
/// owns; `fixed`, `reverted`, `escalated` and `open` are written by the loop and are a record of
/// what happened rather than an opinion about it. A surface that let a person set `fixed` would let
/// somebody record a measurement they had not made.
#[must_use]
pub fn qc_decision_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_QC_DECISION_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not record what you decided about that finding. Nothing about the photograph          has changed; reopen the review queue and try again.",
    )
}

/// A curation choice could not be recorded.
pub const ML_CURATE_DECISION_REFUSED: ErrorCode = ErrorCode("AURA-ML-5143");

/// A curation choice could not be recorded.
///
/// The frozen `CurateService` documents this on `set_order` and `decide`, so it lives here rather
/// than in `aura-curate`: a contract cannot depend on the crate that implements it. The same split
/// phases 16, 22, 23, 24, 25, 26 and 27 made.
///
/// Three ways to reach it and none of them is anything a photographer did wrong: an album order
/// that puts one chapter before another, an order or a pick naming an image outside this project's
/// gallery, or a note above `MAX_NOTE`.
///
/// The first is the one worth understanding. Reordering *within* a chapter is the whole point of
/// the album builder and is always allowed; reordering the chapters themselves is refused by the
/// optimiser, by the drag handler and by the cloud validator alike, because a wedding album whose
/// ceremony follows its reception is not an album with an unusual sequence. ADR-0059 section 8.
///
/// `ExportFormat::parse` and `ExportSubject::parse` raise the same code, which in practice means a
/// shell and a core from different builds.
#[must_use]
pub fn curate_decision_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_CURATE_DECISION_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not record that choice. Nothing about your photographs has changed; reopen the \
         panel and try again.",
    )
}

/// One project's curation pass could not run.
pub const ML_CURATE_PASS_FAILED: ErrorCode = ErrorCode("AURA-ML-5144");

/// One project's curation pass could not run.
///
/// A whole-project failure rather than a per-image one, because curation's subject is a *set*: an
/// album, a portfolio and three social sets are decided over the whole gallery at once, and there is
/// no partial album that would be worth storing.
///
/// The pass writes its result in one transaction, so a failure leaves the previous curation exactly
/// as it was rather than half-replaced - and leaves every photograph untouched, because nothing in
/// phase 29 writes a recipe, opens a file or moves a pixel.
#[must_use]
pub fn curate_pass_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_CURATE_PASS_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "AURA could not work out an album, a portfolio or a set of posts for this wedding. Your \
         gallery and every edit in it are exactly as they were.",
    )
}

/// The curation policy table was refused.
pub const ML_CURATE_POLICY_REFUSED: ErrorCode = ErrorCode("AURA-ML-5145");

/// The curation policy table was refused.
///
/// `curation.toml` may tighten a bound the contract owns and may never widen one, which is phase
/// 21's rule in its sixth application. The pass halts rather than falling back on defaults, for the
/// reason phases 24 to 28 each halted: curation is a taste decision, this file is where a studio's
/// taste lives, and an album composed from numbers nobody chose is a proposal a photographer would
/// trust for the wrong reason.
///
/// The loader also refuses any key naming a skin target, as phases 15, 25 and 27 scan their own
/// schemas for one. The band a monochrome mix protects is measured per person from
/// `ToneService::skin_loci`; a configurable one would be the constant this product does not have.
#[must_use]
pub fn curate_policy_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_CURATE_POLICY_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail,
        "AURA could not load the settings that decide album sizes, rhythm and social formats, so it \
         has not curated anything. Restore the file or reinstall.",
    )
}
