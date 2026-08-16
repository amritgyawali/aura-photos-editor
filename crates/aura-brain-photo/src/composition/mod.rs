//! How a photograph is framed, and how much of that a picture editor would mind.
//!
//! PHASE-11's single feature. Phase 09 answered whether a frame *worked*; this answers
//! whether it was *made*. The mission sentence is the difference: "among six technically
//! equal frames the best-composed one wins - and Phase 23's smart crop has a target to
//! optimise toward."
//!
//! ## The eleven modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`rules`] | what good framing means in this kind of photograph |
//! | [`keypoints`] | where the bodies are, joint by joint |
//! | [`crop_audit`] | what the frame edge cut, and how much that matters here |
//! | [`placement`] | where the subject sits, and how much frame is left over |
//! | [`horizon`] | which way is level, and whether the tilt was a decision |
//! | [`background`] | what is behind them, and how much of it is fighting them |
//! | [`aesthetic`] | what taste says, over the measurements above |
//! | [`flags`] | which of the sixteen bits are true, and what sentence explains each |
//! | [`score`] | one number, and why it is a product rather than a sum |
//! | [`analyse`] | one decoded frame in, one judgement out |
//!
//! Three more that section 4 does not name and ADR-0023 section 7 records: [`store`] for
//! the table migration 11 adds, [`api`] for the frozen `CompositionService` and the
//! resumable project walk, and [`fixtures`] for the synthetic ground truth every section
//! 10.1 gate is measured against. Phases 06 to 10 all ended up with the same store/service
//! pair, for the same reason: an analyser that also owns its SQL is an analyser nobody can
//! test without a database.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **how a photograph is framed**. It does not re-frame one: section 2.2 puts
//! the cropping and the straightening in phase 23 and the distraction removal in phase 24,
//! and the boundary is kept structurally rather than by discipline. There is no `crop`
//! column in migration 11, no rotation applied anywhere, and `CropHint` is spelled *hint*
//! in the type, the field, the column and the wire.
//!
//! That restraint costs something different here than it did in phase 09. A technical
//! score *looks* like a verdict; a composition score *feels* like a verdict about taste,
//! which is worse, because a photographer can argue with a sharpness measurement and can
//! only be insulted by an aesthetic one. Section 6.3's cap - taste breaks ties, it does not
//! override substance - is applied in [`score::AESTHETIC_CAP`] rather than left to phase 12
//! to remember.
//!
//! ## The rule this phase adds, which every later phase inherits
//!
//! **`CompositionService` is the only way to ask how a photograph is framed.** No phase may
//! keep its own headroom band, its own idea of a crop violation or its own aesthetic score.
//! Phase 23's smart crop optimises toward this objective and phase 29 ranks albums with it;
//! two answers to "is this well composed" is a crop that fights the gallery it is in.

pub mod aesthetic;
pub mod analyse;
pub mod api;
pub mod background;
pub mod crop_audit;
pub mod fixtures;
pub mod flags;
pub mod horizon;
pub mod keypoints;
pub mod placement;
pub mod rules;
pub mod score;
pub mod store;

pub use analyse::{Analyser, FrameContext, ANALYSIS_VER, COMPOSITION_LEVEL};
pub use api::{Composition, CompositionPass, PassReport};
pub use keypoints::MODEL_VER;
pub use rules::{RuleTable, SceneRule};
pub use store::{CompositionStore, BYTES_PER_IMAGE};
