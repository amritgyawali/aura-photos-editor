//! This phase's error registry. AURA-ML-5090 to 5095.
//!
//! Every code here is registered in `crates/aura-core/errors.toml` with a runbook. The split
//! is the one every phase since 09 has kept: `aura-core` owns the shapes and the predicates
//! that say what a sound value is, and the implementing crate owns the errors - so the
//! solver, the store, the IPC layer and the evaluation harness cannot disagree about what a
//! sound plan is.
//!
//! Five of the six shapes are the ones phases 09, 11, 15 and 19 have: version drift, a
//! refused edit, one item that could not be done, a refused config file, and a config file
//! missing a row. The sixth is this phase's own honesty, and it is a *warning* rather than a
//! failure: there was no profile for the lens, so the optics were left alone rather than
//! corrected by a guess.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored plans came from different lens profiles, arithmetic or crop rules.
const GEOMETRY_STALE: ErrorCode = ErrorCode("AURA-ML-5090");
/// A crop or straightening override was refused.
const GEOMETRY_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5091");
/// The crop rules table was refused.
const RULES_REFUSED: ErrorCode = ErrorCode("AURA-ML-5093");
/// A scene has no crop rule row.
const RULES_ROW_MISSING: ErrorCode = ErrorCode("AURA-ML-5094");
/// No lens profile matched, so the optics were left alone.
const LENS_PROFILE_MISSING: ErrorCode = ErrorCode("AURA-ML-5095");

/// `AURA-ML-5090`. A comparison would cross a version boundary.
///
/// Degraded rather than failing: stale plans are still plans, and re-planning a wedding in
/// the background is better than refusing to show a photographer the framing they already
/// have. `user_edited = 1` rows are skipped by the statement itself.
#[must_use]
pub fn versions_moved(stored: (u16, u16, u16), current: (u16, u16, u16)) -> AuraError {
    AuraError::new(
        GEOMETRY_STALE,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "geometry plans span versions: stored profile {} analysis {} rules {}, current \
             profile {} analysis {} rules {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it corrects lenses and chooses crops, so it is re-checking \
         this wedding in the background. Any framing you have set yourself is kept.",
    )
}

/// `AURA-ML-5091`. An override the contract's predicate rejected.
#[must_use]
pub fn framing_refused(problem: impl Into<String>) -> AuraError {
    AuraError::new(
        GEOMETRY_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        problem,
        "AURA could not record that framing. The photograph is unchanged.",
    )
}

/// `AURA-ML-5092`. A plan that broke one of this phase's own guarantees, or a frame whose
/// proxy would not decode.
///
/// Re-exported from `aura-core` rather than built here, because
/// `contract::geometry::Keystone::new` is the only constructor of a capped keystone and the
/// contract cannot depend on the crate that implements it.
#[must_use]
pub fn geometry_failed(detail: impl Into<String>) -> AuraError {
    aura_core::errors::ml::geometry_failed(detail)
}

/// `AURA-ML-5092`, naming the photograph.
#[must_use]
pub fn plan_failed(image: &str, problem: impl Into<String>) -> AuraError {
    geometry_failed(format!("{image}: {}", problem.into()))
}

/// `AURA-ML-5093`. The crop rules table did not load.
///
/// Run-blocking, and the distinction from [`rules_row_missing`] is the whole reason there are
/// two codes: a missing *row* falls back to leaving the frame as shot, and a missing *file*
/// means no row can be checked at all. Cropping every wedding to a default nobody approved is
/// worse than cropping nothing.
#[must_use]
pub fn rules_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RULES_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail,
        "AURA could not load the settings that decide what a crop is allowed to remove, so \
         it has not cropped anything. Restore the file or reinstall.",
    )
}

/// `AURA-ML-5094`. A scene with no row, which falls back to the frame as shot.
#[must_use]
pub fn rules_row_missing(scene: &str) -> AuraError {
    AuraError::new(
        RULES_ROW_MISSING,
        Severity::Warning,
        Recovery::Fallback,
        format!("no crop rule row for scene {scene}"),
        "AURA has no cropping guidance recorded for this kind of photograph yet, so it is \
         leaving those ones framed as they were shot. They are all still usable.",
    )
    .with_context("scene", scene.to_string())
}

/// `AURA-ML-5095`. No embedded data, no table entry and not enough straight edges.
#[must_use]
pub fn lens_profile_missing(lens: &str) -> AuraError {
    AuraError::new(
        LENS_PROFILE_MISSING,
        Severity::Warning,
        Recovery::Fallback,
        format!("no lens profile for {lens} and too few straight edges to estimate one"),
        "AURA has no correction profile for the lens some photographs were shot on, so it \
         has left their distortion and fringing alone. They are all still usable.",
    )
    .with_context("lens_id", lens.to_string())
}
