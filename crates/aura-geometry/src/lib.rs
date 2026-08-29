#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::disallowed_methods,
        clippy::uninlined_format_args
    )
)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::similar_names,
    clippy::doc_markdown
)]

//! Finishing the frame: the optics corrected, the world levelled, and a crop with judgement.
//!
//! PHASE-23's single feature. Section 1 says why each third is worth building and then says what
//! makes the third one different from the other two:
//!
//! > Smart crop is where automation is most dangerous, so a subject-aware, conservative,
//! > always-reversible crop is a trust feature as much as a quality feature.
//!
//! ## The modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`profiles`] | what this lens does, and how much room this kind of photograph has |
//! | [`lens`] | which of three sources knows about this lens, and what that becomes |
//! | [`straighten`] | whether this frame is off level by an amount worth a resample |
//! | [`keystone`] | whether the upright lines are architecture, and how far they may be pulled |
//! | [`crop`] | which rectangles are worth considering, and how good each one is |
//! | [`safety`] | which of them may not be delivered, and why |
//! | [`variants`] | the four aspects an album and a feed want |
//! | [`decide`] | one decoded frame in, one plan out |
//! | [`store`] | the two tables migration 23 adds |
//! | [`api`] | the frozen `GeometryService` and the resumable project walk |
//!
//! Plus [`fixtures`], the synthetic ground truth every section 10.1 gate is measured against, and
//! [`errors`], this crate's own half of the split every phase since 09 has kept.
//!
//! ## The four things to understand before reading the code
//!
//! **This phase mostly does nothing, and that is the specification rather than a shortfall.**
//! Section 10.1 requires that at least seventy per cent of frames keep the framing they were shot
//! at. [`aura_core::contract::geometry::MIN_IMPROVEMENT`] is the sentence as a number, and
//! twenty-three of the thirty reason codes in this phase describe something that was *not* done.
//! A wedding where this pass acted on most of the frames is a wedding where something is wrong
//! with the margin.
//!
//! **A safety rule is a veto, and it is evaluated before the score.** [`safety::check`] runs on
//! every candidate before [`crop::objective`] is compared to anything, so there is no arithmetic
//! path in this crate along which a very high-scoring rectangle can outweigh a face at its edge.
//! This is phase 20's protected-feature rule in a different domain: "cropped a little" through
//! somebody's hand is a cropped hand.
//!
//! **A rotation costs a crop, and the crop is computed before the rotation is agreed to.**
//! Section 6.2's last sentence - "if it cannot, the rotation is reduced or skipped" - is
//! [`straighten::solve`], which walks the angle down in steps and stops at the first one whose
//! induced crop keeps everything protected inside. There is no branch in which a frame is rotated
//! and *then* something is found to be missing, because by then the pixels are gone.
//!
//! **Nothing here fills a corner.** Rotating opens four triangles and section 2.2 puts
//! content-aware fill in phase 24. The triangles are removed by
//! [`aura_core::contract::geometry::rotation_crop`], and when removing them would breach a safety
//! rule the rotation is abandoned rather than the corners invented.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **which lens corrections to apply, how far to rotate, whether to correct converging
//! verticals, and which rectangle to deliver**, and expresses every one of them as reversible
//! parameters in the edit recipe. It does not fill (phase 24), it does not choose which crop an
//! album spread uses (phase 29), it does not build a panorama, and it cannot upscale - there is
//! no field anywhere in the contract or in this crate that could carry an output scale.
//!
//! ## What this build does not have
//!
//! **This phase ships no model** - the third since phase 08, after phase 17, and for the same
//! kind of reason as phase 17's: there is nothing to train. Section 9's MLL row asks for an
//! *objective spec* rather than a network, the four terms of that objective have closed forms,
//! and what is missing is not weights but **expert crop labels**. Section 9's DATA row asks for
//! 2,000 labelled frames and there are none in this repository, so section 10.1's straightening
//! gate is measured against synthetic frames whose tilt was painted in and section 10.1's crop
//! gates are measured against synthetic frames whose faces, hands and subject placement were
//! painted in. That proves the arithmetic, the safety filter, the improvement margin, the aspect
//! solver and the store; it is not evidence that a photographer would prefer AURA's crop.
//!
//! Beyond that, **phase 06's face detector is a placeholder**, so on a real photograph in this
//! build the safety filter has nothing to protect - `CropSafetyReport::considered` is zero and
//! the panel says so. Every crop gate in section 10.1 is therefore a statement about the filter
//! rather than about a wedding. `docs/progress/PHASE-23-EXIT.md` carries all of it as conditions.

pub mod api;
pub mod crop;
pub mod decide;
pub mod errors;
pub mod fixtures;
pub mod keystone;
pub mod lens;
pub mod profiles;
pub mod safety;
pub mod store;
pub mod straighten;
pub mod variants;

pub use api::{Geometry, GeometryPass, GeometryPassReport};
pub use decide::{Analyser, GeometryFrame, GeometryOutcome};
pub use store::GeometryStore;
