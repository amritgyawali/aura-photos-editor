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
    clippy::cast_lossless,
    clippy::struct_field_names,
    clippy::too_many_lines,
    clippy::similar_names
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
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )
)]

//! The learning loop. PHASE-30.
//!
//! Every phase from 06 to 29 lets a photographer disagree with it. This one is what makes the
//! disagreement worth something.
//!
//! ## Read this before reading the modules
//!
//! **This is the one feature in the product that can make it worse over time with every test
//! green.** Section 12's second row is not hypothetical: a loop that fits on everything it sees
//! will learn a photographer's Tuesday-afternoon mood, a marquee's yellow canvas, and the forty
//! frames somebody fixed by hand because the model was wrong about one room - then apply all of it
//! to the next wedding, which arrives subtly wrong in a way nobody can point at.
//!
//! Every structural choice here follows from that.
//!
//! **Nothing is adopted without a person.** [`review::adopt`] is the only code path that sets
//! `adopted`, and `learn_update_no_self_adopt` in migration 30 refuses an INSERT that arrives
//! already adopted. Two locks, because a promise enforced in one layer lasts until somebody writes
//! a second caller - which phase 21 wrote down after finding it twice.
//!
//! **The held-out split is drawn from a hash of the correction's own decision id.** Not a shuffle.
//! A shuffle re-draws the split on every fit, so a fit whose measured improvement disappoints can
//! be re-run until the line falls somewhere flattering, and nothing about that would look wrong in
//! a review, a test or a panel. It is the single easiest way for this feature to become a number
//! generator. [`aggregate::hold_out`] is the whole of it and it is four lines.
//!
//! **A correction with no ledger decision behind it is refused.** [`capture::attribute`] looks the
//! decision up through phase 13's frozen service. A residual measured from no baseline is an
//! absolute edit wearing a residual's shape - phase 17's condition C4, in the phase that would
//! carry it into every future wedding.
//!
//! **The central tendency is a trimmed median, not a mean.** A mean over a bucket containing one
//! rescue of a single badly-lit room is a mean with that room in it. [`aggregate::fold`] drops
//! anything beyond [`aura_core::contract::learn::OUTLIER_MADS`] deviations and reports how many,
//! because a photographer should be able to see that the loop ignored their four extreme fixes.
//!
//! **An offset is bounded twice.** [`aura_core::Aggregate::proposed_offset`] takes half of the
//! measured shift and clamps to the value's own ceiling. The share bound alone would oscillate; the
//! ceiling alone would let one wedding move a profile a long way. Neither alone is enough, and
//! half of half of half still reaches a ceiling eventually if nothing catches it.
//!
//! **Rollback is a whole document, compared byte for byte.** [`rollback::restore`] returns the
//! stored snapshot and its digest. Section 10.1 asks that rollback "restores the previous profile
//! exactly", and exactly is a byte comparison rather than a re-derivation - phase 14's argument
//! about `edit_history.body`, in the phase where the alternative would be to re-fit.
//!
//! ## What this crate cannot do
//!
//! It cannot write a profile. It computes an offset and stores a snapshot; applying one is
//! `aura-style`'s, through the frozen service, with a person's click in between. It cannot move a
//! guarantee: [`aura_core::Learnable`] is closed at fifteen members and
//! `tests/no_guarantee_learning.rs` is the tenth grep-as-a-test in this repository.

pub mod aggregate;
pub mod api;
pub mod attribute;
pub mod capture;
pub mod errors;
pub mod fixtures;
pub mod review;
pub mod rollback;
pub mod store;
pub mod update;

/// Which build fitted a stored update. Bumped when the aggregation or the bounding changes.
///
/// On `learn_update` rather than in a column of its own, because what it invalidates is a
/// *comparison*: an improvement measured by one build against a candidate fitted by another is a
/// number about neither. Phases 05 to 27 all wrote a version column for the same reason.
pub const LEARN_VER: u16 = 1;

/// Whether this build has ever fitted a profile from real corrections.
///
/// **False**, and on the wire. There is no photographer's archive in this repository, so every
/// number in this phase's gates is measured against corrections the fixtures authored. A panel
/// that rendered a synthetic improvement as a measured one would be telling somebody their profile
/// got better. Exit condition C4.
pub const FITTED_ON_REAL_CORRECTIONS: bool = false;
