//! Turning three thousand files into a few hundred moments.
//!
//! PHASE-08's single feature. Photographers shoot in bursts - six frames of the kiss,
//! fourteen of the bouquet toss, forty of the dance floor - and every later decision in
//! this product is about *moments* rather than about files. Rejecting a burst is a
//! moment lost; rejecting individual frames is tidying, and phase 12's coverage
//! guarantees depend on the difference.
//!
//! ## The seven modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`cadence`] | how fast was the photographer shooting, here, on this body |
//! | [`graph`] | which pairs of frames are candidates, and does this pair clear the bar |
//! | [`burst`] | which of these frames came off one press of the shutter |
//! | [`duplicate`] | which of them are the same photograph, and which are alternatives |
//! | [`merge`] | did two photographers catch the same instant, and what did the person say |
//! | [`moment`] | assemble it, explain it, and store it |
//! | [`api`] | the frozen `MomentService`, and the pass that fills it |
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **which frames belong together**. It does not decide which of them a
//! client sees: section 2.2 puts that in phase 12, and the boundary is kept
//! structurally rather than by discipline. There is no `culled` column in migration 8,
//! no rank, and `DuplicateSet::keep_hint` is spelled *hint* in the contract, in the
//! schema and in the panel.
//!
//! This is phase 05's rule again - "a distance is evidence; the deciding phase owns the
//! threshold" - one level up: *a grouping is evidence; the deciding phase owns the
//! cull.*
//!
//! ## The rule this phase adds, which every later phase inherits
//!
//! **`MomentService` is the only way to ask what was shot once.** No phase may keep its
//! own grouping. Two answers to "are these the same shot" is two culling decisions that
//! disagree, and phase 12's coverage guarantee is written against this one.

pub mod api;
pub mod burst;
pub mod cadence;
pub mod duplicate;
pub mod graph;
pub mod merge;
pub mod moment;

pub use api::{GroupReport, Moments, PassContext};
pub use graph::{Frame, MomentProfile, MomentProfileRegistry, GROUP_VER};
pub use moment::{MomentRow, MomentStore, BYTES_PER_IMAGE};
