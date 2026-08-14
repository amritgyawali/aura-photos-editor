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
    clippy::cast_sign_loss
)]

//! The wedding brain: what each photograph is of, and how the day was structured.
//!
//! PHASE-07 ships one feature - the app reads the wedding as a story - and this crate
//! is all of it that is not a contract, a migration or a screen.
//!
//! ## What this crate is for
//!
//! Invariant 7 says "no threshold is global; every threshold is a function of the
//! detected scene and subject role". For six phases that has been a promise with
//! nothing behind it, because there was no scene. `SceneProfile` is the lookup that
//! makes it true, and everything else here exists to fill in its argument.
//!
//! ## The two halves
//!
//! [`scene`] answers *what is this photograph of*. One 22-way class, fourteen
//! attribute bits, and a named rite when the evidence supports one. It sits on the
//! phase 05 embedding, frozen, and opens no pixels - section 6.1's whole design, and
//! the reason section 11 can budget 35 seconds for four thousand images.
//!
//! [`story`] answers *where in the day is this*. The per-frame posteriors are smoothed
//! by a hidden Markov model over nine chapters, then a change-point detector finds the
//! boundaries, then a medoid picks each chapter's cover. The result is a small ordered
//! table that every phase from 09 to 29 reads.
//!
//! ## The rules this phase adds, which every later phase inherits
//!
//! * **`StoryService` is the only way to ask what a photograph is of.** No phase may
//!   keep its own scene classifier or its own idea of where the ceremony was. Two
//!   answers to "is this the ceremony" is two thresholds that disagree.
//! * **A profile is evidence about a scene; the deciding phase owns the action.**
//!   Nothing here culls, grades or crops. `SceneProfile::max_acceptable_noise` is a
//!   tolerance phase 09 measures against and phase 12 acts on, and this crate has no
//!   opinion about either.
//! * **Four version columns, because they invalidate four different things.**
//!   `model_ver` invalidates the posterior, `preprocess_ver` invalidates the context
//!   features, `taxonomy_ver` invalidates the rite's name, and `embed_ver` invalidates
//!   everything because the trunk is underneath all of it. `AURA-ML-5022` exists so a
//!   comparison across any of them never happens silently.
//! * **Report coverage when you report a result.** A story drawn over a
//!   40 %-classified wedding is a story about 40 % of a wedding, and
//!   `StoryOutline::coverage` is how a caller finds out.
//! * **A photographer's chapter is unbeatable.** `segments.user_locked` and
//!   `image_scenes.source = 'user'` are checked inside the statements that would
//!   overwrite them, exactly as `identities.user_locked` is in phase 06.
//!
//! ## What is deliberately not here
//!
//! Culling and coverage guarantees are phase 12. Editing parameters are phases 15 to
//! 17. Album sequencing is phase 29. Section 2.2 lists all three as out of scope, and
//! the boundary is kept structurally: this crate writes four tables and reads three,
//! and none of them is a decision about a photograph's fate.

pub mod errors;
pub mod fixtures;
pub mod scene;
pub mod story;

pub use scene::classifier::{SceneClassifier, SceneInput, MODEL_VER, PREPROCESS_VER};
pub use scene::pass::{run as classify_project, PassContext};
pub use scene::profile::SceneProfileRegistry;
pub use scene::ritual::{RitualClassifier, RitualVerdict, TraditionVote};
pub use scene::taxonomy::{Rite, Taxonomy};
pub use story::{ScenePassReport, ScenePassRow, Story, StoryReport, StoryStore};

/// The dense scene handle `aura_index::IndexFilter` filters on.
///
/// Phase 05 froze `aura_index::contract::index::SceneId(u32)` as "a dense handle for a
/// scene, assigned by phase 07" and left it unassigned. This is the assignment, and it
/// is the position in [`aura_core::SceneId::ALL`] - the same index the classifier's
/// output vector uses, so a filter, a posterior slot and a stored slug are three views
/// of one number.
///
/// A separate type rather than a re-export, because the index's handle is a *filter
/// key* and the core's `SceneId` is a *vocabulary*: the index must be able to filter on
/// a scene this build has never heard of, read from a catalog written by a newer one.
#[must_use]
pub fn index_handle(scene: aura_core::SceneId) -> aura_index::contract::index::SceneId {
    aura_index::contract::index::SceneId(u32::try_from(scene.as_index()).unwrap_or(u32::MAX))
}
