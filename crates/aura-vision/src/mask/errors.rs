//! The six codes phase 18 raises, `AURA-ML-5078` to `AURA-ML-5083`.
//!
//! The same five shapes every phase since 09 has - one item that could not be done, a refused
//! run, a warning about a case the product handled badly, a refused edit and version drift -
//! plus one that is new here.
//!
//! `AURA-ML-5081` is **the first code in the product that refuses to let a later phase act at
//! full strength**. Every warning before it was about a decision AURA had already made. This
//! one is about a decision phases 19 to 24 have not made yet: a mask whose boundary is not
//! well determined is a mask that must not carry skin smoothing, and the code is what carries
//! that constraint out of this phase and into theirs. Section 6.4, and the reason it exists is
//! property 3 of ADR-0037's context - a wrong mask is *silent*.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// One photograph could not be masked.
pub const ML_MASK_FAILED: ErrorCode = ErrorCode("AURA-ML-5078");
/// A masking run was refused before it started.
pub const ML_MASK_REFUSED: ErrorCode = ErrorCode("AURA-ML-5079");
/// A mask payload exceeded its storage budget and was stored at a coarser resolution.
pub const ML_MASK_PAYLOAD_COARSENED: ErrorCode = ErrorCode("AURA-ML-5080");
/// A mask's quality limits what may be done with it.
pub const ML_MASK_QUALITY_LIMITED: ErrorCode = ErrorCode("AURA-ML-5081");
/// A mask edit was refused.
pub const ML_MASK_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5082");
/// Stored masks came from a different model set or a different analysis.
pub const ML_MASK_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5083");

/// One photograph could not be masked. The pass continues with the next.
///
/// **No row is written.** Phase 17's `pair_failed` writes one and this does not, and the
/// difference is what the row would mean: a rejected style pair is evidence a photographer
/// needs, and a stored empty mask is a region that later phases would read as "there is no
/// skin in this photograph" rather than as "nobody looked".
#[must_use]
pub fn mask_failed(image: &str, detail: &str) -> AuraError {
    AuraError::new(
        ML_MASK_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{image}: {detail}"),
        "AURA could not work out the regions in one photograph, so it left that one \
         unmasked. Everything else in the selection is still being done.",
    )
    .with_context("image", image)
}

/// A masking run was refused before it started. Nothing is written.
///
/// The two reasons are an empty selection and a project phase 12 has not culled. Both are
/// refusals rather than silent no-ops because masking every frame of a wedding is section
/// 6.3's own counter-example: it is the work the lazy policy exists to avoid, and a caller
/// that asked for it by accident should find out.
#[must_use]
pub fn mask_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_MASK_REFUSED,
        Severity::RunBlocking,
        Recovery::AskUser,
        detail,
        "AURA has nothing selected to work on, so it has not made any masks. Cull the \
         wedding first, or choose the photographs you want.",
    )
}

/// A payload did not fit its budget and was stored at half resolution.
///
/// A warning rather than a failure, and the resolution loss is recorded on the row rather
/// than inferred: a mask that quietly halved its own resolution is a boundary that is
/// suddenly a pixel wide at 100 % zoom for no reason anybody can find later.
#[must_use]
pub fn payload_coarsened(kind: &str, was: usize, now: usize) -> AuraError {
    AuraError::new(
        ML_MASK_PAYLOAD_COARSENED,
        Severity::Warning,
        Recovery::Fallback,
        format!("{kind}: {was} bytes exceeded the budget, stored at {now}"),
        "One region in this photograph was unusually broken up, so AURA stored it a little \
         more coarsely to keep the catalog small.",
    )
    .with_context("kind", kind)
}

/// A mask's quality limits what later phases may do with it.
///
/// Carries the allowance so a caller does not have to recompute it, and names the kind so the
/// panel can say which region rather than "a mask".
#[must_use]
pub fn quality_limited(kind: &str, allowance: f32) -> AuraError {
    AuraError::new(
        ML_MASK_QUALITY_LIMITED,
        Severity::Warning,
        Recovery::Fallback,
        format!("{kind}: allowance {allowance:.2}"),
        "AURA is not confident about the edge of this region, so it will make smaller \
         changes there than usual and will not smooth skin inside it.",
    )
    .with_context("kind", kind)
}

/// A mask edit was refused. Nothing changed.
#[must_use]
pub fn edit_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_MASK_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not apply that change to the mask. Nothing was changed.",
    )
}

/// Stored masks were produced under a different model set or analysis.
///
/// Tenth version-drift code in the product. The two versions invalidate different things -
/// `model_ver` invalidates every class assignment, `analysis_ver` invalidates the boundaries
/// and the quality numbers - and comparing across either returns a plausible mask that means
/// nothing.
#[must_use]
pub fn version_mismatch(field: &str, stored: u16, current: u16) -> AuraError {
    AuraError::new(
        ML_MASK_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!("{field}: stored {stored}, current {current}"),
        "AURA has improved how it finds regions in a photograph, so it is redoing them in \
         the background. Any mask you have edited by hand is kept exactly as it is.",
    )
    .with_context("field", field)
}
