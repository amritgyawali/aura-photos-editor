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
#![warn(clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    // `Frame` carries one optional reading per upstream phase, and a selector handed a frame with
    // none of them must *skip* a term rather than score it at zero. Folding twenty options into a
    // bitmask would make "which input was absent" - which is the whole of ADR-0059 sections 8 and
    // 9 - an argument about how to decode an integer. Phase 27 took the same exemption.
    clippy::struct_excessive_bools,
    clippy::fn_params_excessive_bools,
    // Every weight in `curation.toml` is named for the term it weights and reads that way in the
    // file, in the contract and in the panel. Renaming them to satisfy a lint would put three
    // spellings of the same number in the product.
    clippy::struct_field_names,
    // The selectors in this crate are single passes with a lot of *explanation* in them: `hero::
    // select` is one greedy loop whose length is the three diversity constraints and the six reason
    // arms, and splitting it would put the constraint and the sentence that reports it in different
    // files. The store's readers are long for the same reason as every store in this repository -
    // a column list is a column list. The same exemption phases 14, 24, 25, 26 and 27 took.
    clippy::too_many_lines
)]
// The panic family and slice indexing are banned in library code and are how a test asserts. An
// inline `#[cfg(test)]` module is not compiled into the library at all, so nothing it does can
// reach a photographer; the lints stay denied everywhere else in the crate. The same exemption
// phases 14, 19, 23, 24, 25, 26 and 27 took, for the same reason.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::disallowed_methods,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )
)]

//! Curation. PHASE-29.
//!
//! Twenty-eight phases produced a gallery. This one produces the things a photographer sells out of
//! it: a portfolio, an album draft, three sets of posts and a teaser to send on the wedding night.
//!
//! ## Read this before reading the modules
//!
//! Curation is the first thing in this product that is a matter of **taste**. Everything before it
//! could be right or wrong against a measurement: a white balance against a measured illuminant, a
//! closed eye against an eyelid, a texture floor against a band-energy ratio. Whether the
//! second-best photograph of the first dance belongs on the left-hand page of spread eleven is a
//! judgement two competent photographers can disagree about all afternoon.
//!
//! A phase that cannot be right has exactly one way to be safe, and it is not "be more accurate".
//! It is to own no output. Every structural choice in this crate follows from that.
//!
//! **Nothing here is applied.** No recipe is written, no file is opened, no pixel moves, and no
//! frame leaves the gallery. [`bw`] solves a monochrome mix and stores it as a *proposal*; the `bw`
//! block phase 14 froze in the recipe is filled by the develop surface when a photographer accepts
//! one. [`tests/no_outputs.rs`](https://example.invalid) is the ninth grep-as-a-test in this
//! repository and fails the build if any of that stops being true.
//!
//! **The band a monochrome mix protects is measured per person.** [`bw::solve`] looks the band up
//! from phase 15's `ToneService::skin_loci` and **pins it at zero**, and when a frame has faces but
//! nobody in it has a usable locus, no mix is offered at all. There is no default skin band in this
//! crate, in `curation.toml`, in migration 29 or in the contract - a default skin band is exactly
//! the constant `docs/skin-fairness.md` says this product does not have, and a monochrome
//! conversion is the easiest place in the product to lighten somebody's face by accident.
//!
//! **Coverage is a filter, never a term.** [`album::allocate`] reserves a slot per satisfied
//! must-have and per under-covered close-family identity *before* any value ranking is consulted.
//! An album is 60 to 120 images out of a gallery of hundreds, so a coverage term would lose to two
//! beautiful portraits every time and the album would arrive without the ring exchange in it. Phase
//! 12's rule, fifth application.
//!
//! **Chapter order is inviolable.** [`album::optimise`] only proposes swaps inside one chapter's
//! span, [`api::Curate::set_order`] refuses an order that reorders chapters, and
//! [`sequence::apply`] validates the cloud's moves against the same rule. Three enforcers for one
//! rule, because a wedding album whose ceremony follows its reception is not an album with an
//! unusual sequence.
//!
//! **Unmeasurable is a third value.** [`ShotScale::Unknown`] frames are excluded from the rhythm
//! score's denominator rather than counted as misses, and [`SpreadPair::facing_known`] is the same
//! distinction one level down. On this build - where phase 06's detector finds no faces - both are
//! near zero, so a rhythm of 1.000 is a claim about eight per cent of an album. Phase 27's rule.
//!
//! **A caption may only contain words the wedding supplied.** [`caption::Vocabulary`] is the closed
//! set of content words built from this project's chapters, scenes, rituals and role words; a
//! caption is accepted when every content word in it is in that set. A blocklist of names cannot
//! enumerate names. The local template passes by construction because it is assembled *from* the
//! vocabulary, and a cloud draft that fails is replaced by it.
//!
//! **The cloud can only be agreed with.** [`sequence::apply`] applies a proposed move only when it
//! stays inside a chapter, breaks no hard constraint, and **improves the local objective**. So an
//! unreachable provider, a spent budget, a malformed answer and a model that proposes twenty bad
//! moves all produce the same album: the one the deterministic optimiser produced.
//!
//! ## What this crate does not contain
//!
//! No renderer, no recipe writer, no file handle, no pixel, no similarity index of its own and no
//! coverage engine of its own. Every reading arrives through the [`read::Field`] port that
//! `aura-app` implements out of the frozen services, which is what stops `aura-cull` - the crate
//! that decided what is in the gallery - from being visible to the crate that curates it.
//!
//! [`ShotScale::Unknown`]: aura_core::contract::curate::ShotScale::Unknown
//! [`SpreadPair::facing_known`]: aura_core::contract::curate::SpreadPair::facing_known

pub mod album;
pub mod api;
pub mod bw;
pub mod caption;
pub mod errors;
pub mod explain;
pub mod export;
pub mod fixtures;
pub mod hero;
pub mod policy;
pub mod read;
pub mod sequence;
pub mod social;
pub mod spread;
pub mod store;
pub mod teaser;

pub use api::{Curate, CuratePass};
pub use policy::Policy;
pub use read::{Facing, Field, Frame};

/// Which build's arithmetic produced a stored curation.
///
/// Bumped on any change to a score, a fusion, an allocation or an objective - not on a change to a
/// weight, which is `policy_ver`'s job, and not on a change to the embedding, which is phase 05's
/// `embed_ver`. Three columns because they invalidate three different things, and `AURA-ML-5142`
/// exists so a comparison across any of them never happens silently. Phase 08's rule.
pub const ANALYSIS_VER: u16 = 1;

/// Whether this build's hero ranker is a trained model.
///
/// False. Section 9's DATA row asks for 60 real album sequences, hero sets and B&W selections
/// collected with permission, and this repository has none - so what ships is the deterministic
/// blend of ADR-0059 section 6 and `ml/models/curate/train_hero.py` is there for the first studio
/// with a consented archive. A panel that did not show this would be presenting a solver's answer
/// as a learned one.
pub const HERO_HEAD_TRAINED: bool = false;

/// Whether this build's monochrome suitability model is a trained model.
///
/// False, and for the same reason. What ships is a *measurement* over phase 05's stored
/// descriptors, whose failure mode is offering fewer candidates rather than confidently wrong ones.
pub const BW_HEAD_TRAINED: bool = false;

/// True when either head is trained. Stored on the run, because a catalog outlives a build.
#[must_use]
pub const fn heads_trained() -> bool {
    HERO_HEAD_TRAINED || BW_HEAD_TRAINED
}
