//! The diffusion tier. Declared, refused, and reachable - not stubbed out. ADR-0049 section 5.
//!
//! Section 2.1 asks for local diffusion inpainting from an optional model pack, or the phase 04
//! cloud path with explicit consent. Neither exists in this build:
//!
//! * There is no diffusion model in `models.lock`, and there is no `AURADLT1` delta that would
//!   install one.
//! * Phase 03's interpreter implements a documented ONNX opset 13 subset with **no `Resize` and no
//!   `ConvTranspose`**, which is most of a U-Net's decoder. A pack could be downloaded and still
//!   not run.
//! * TLS is waived (ADR-0009), so the cloud path reaches `http://` OpenAI-compatible endpoints and
//!   no public image provider.
//!
//! So [`solve`] returns `Err(CleanupCode::InpaintUnavailable)` on every call, and
//! [`crate::INPAINT_PACK_INSTALLED`] is false.
//!
//! ## Why this is a refusal rather than a fallback
//!
//! Phase 20 shipped a measurement in place of an untrained blemish model, because a
//! difference-of-Gaussians is a real detector whose failure mode is finding fewer marks. Phase 22
//! shipped **nothing** in place of an untrained face-recovery model, because what would stand in
//! is unsharp masking, which is a different operation with a worse result and the same name.
//!
//! This is phase 22's shape. What would stand in for a diffusion inpaint is the classical fill -
//! which is *the tier below it*, and which the source selector has already tried and rejected by
//! the time it gets here. A fallback would therefore be the product doing the thing it had just
//! decided was insufficient, and then writing `method = inpaint` on the row.
//!
//! **`CleanupMethod::Inpaint` in a stored disclosure means a diffusion model ran.** There is no
//! build in which it means something else, and that is the entire value of the disclosure.
//!
//! ## What the module is for, given that it always refuses
//!
//! Three things, all of which would be lost by deleting it:
//!
//! 1. The call site exists, so [`crate::source::select`]'s ordering is the real ordering rather
//!    than a two-branch match that a later author would have to extend.
//! 2. [`Request`] is the shape the pack will be handed, so adding one is filling in a function
//!    rather than designing an interface under deadline.
//! 3. `AURA-ML-5122`'s sibling refusal is *reachable and testable*: a gate can assert that asking
//!    for an inpaint produces a refusal rather than a fill wearing an inpaint's name, which is a
//!    property nobody could test against an absent module.

use aura_core::contract::cleanup::{Box2, CleanupCode, CleanupMethod};

use crate::pixels::Image;

/// The model pack this tier would need, by the name it would carry in `models.lock`.
///
/// A constant rather than a string at the call site so that the day a pack ships, the disclosure,
/// the model card and the refusal message cannot disagree about what it is called.
pub const PACK_NAME: &str = "wedding_inpaint";

/// The largest region a diffusion tier would ever be offered, as a share of the frame.
///
/// The same [`AREA_CAP_DEFAULT`] every other method is bound by, restated here because a future
/// author wiring up a pack will read this file and not [`crate::safety`]. It is not a second
/// check - the safety engine has already run - it is a reminder that there is no larger cap
/// waiting for a better method.
///
/// [`AREA_CAP_DEFAULT`]: aura_core::contract::cleanup::AREA_CAP_DEFAULT
pub const AREA_CAP: f32 = aura_core::contract::cleanup::AREA_CAP_DEFAULT;

/// Everything a diffusion tier would be given.
///
/// **There is no prompt field, and there will not be one.** `docs/generative-policy.md` promises
/// that AURA never generates from a description, and the way that promise is kept is that no type
/// on this path can carry one. A model pack that required a text conditioning would be handed the
/// empty string by this shape, which is the correct outcome: the product removes an object, it
/// does not describe what should be there instead.
#[derive(Debug, Clone, PartialEq)]
pub struct Request<'a> {
    /// The photograph, linear.
    pub image: &'a Image,
    /// The region to replace, normalised.
    pub region: &'a Box2,
    /// Whether the studio has explicitly opted the diffusion tier in.
    ///
    /// Off at installation. Section 6.4: diffusion always requires review unless a studio opts in,
    /// and an opt-in that defaulted to true would be a default rather than a decision.
    pub studio_opted_in: bool,
}

/// The method a successful call would be disclosed as.
///
/// Constructed here rather than at the call site so the model name on a stored row is the name of
/// the pack that produced it, and cannot become a hard-coded string somewhere else.
#[must_use]
pub fn method() -> CleanupMethod {
    CleanupMethod::Inpaint {
        model: PACK_NAME.to_string(),
    }
}

/// Run a diffusion inpaint.
///
/// # Errors
///
/// [`CleanupCode::InpaintUnavailable`], always, in this build. See the module header for why there
/// is no fallback underneath it.
pub fn solve(request: &Request<'_>) -> Result<Image, CleanupCode> {
    let _ = request;
    Err(CleanupCode::InpaintUnavailable)
}

/// True when a diffusion inpaint could be attempted at all.
///
/// Two conditions, and a studio can only satisfy one of them. Written as a function rather than
/// checked inline at the call site because phase 28 will ask the same question when it decides
/// what may run unattended, and a second copy of the condition is a second answer.
#[must_use]
pub fn is_available(studio_opted_in: bool) -> bool {
    crate::INPAINT_PACK_INSTALLED && studio_opted_in
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Image {
        Image {
            w: 32,
            h: 32,
            rgb: vec![0.4; 32 * 32 * 3],
        }
    }

    fn region() -> Box2 {
        Box2 {
            x: 0.4,
            y: 0.4,
            w: 0.1,
            h: 0.1,
        }
    }

    #[test]
    fn every_call_refuses_and_names_the_reason() {
        let image = frame();
        let region = region();
        let outcome = solve(&Request {
            image: &image,
            region: &region,
            studio_opted_in: true,
        });
        assert_eq!(outcome.err(), Some(CleanupCode::InpaintUnavailable));
    }

    #[test]
    fn a_studio_opt_in_does_not_conjure_a_model_pack() {
        // The switch a studio can set is one of two conditions, and it is not the one that
        // matters here.
        assert!(!is_available(true));
        assert!(!is_available(false));
    }

    #[test]
    fn the_disclosed_method_names_the_pack_rather_than_a_literal() {
        match method() {
            CleanupMethod::Inpaint { model } => assert_eq!(model, PACK_NAME),
            other => panic!("the inpaint tier must disclose as an inpaint, got {other:?}"),
        }
    }

    #[test]
    fn the_inpaint_method_is_never_tier_one() {
        // Section 6.4. Asserted here as well as in the contract's own tests, because this is the
        // file somebody wiring up a pack will be editing.
        assert!(!method().tier_one());
        assert!(!method().is_real_pixels());
        assert_eq!(method().preference(), 2);
    }
}
