//! Shaping light inside one photograph, the way a retoucher does.
//!
//! PHASE-19's single feature. Section 0 calls the mission "the 'why does this look so much
//! better and I can't tell what changed' effect", and section 1 says why it is worth a phase:
//! global adjustments cannot fix a face in shadow under a mandap, a bright window behind the
//! couple or a hot spot on a forehead, and local light shaping is where perceived quality
//! jumps.
//!
//! ## The modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`policy`] | how much shaping a photograph of this kind gets |
//! | [`measure`] | what the pixels say, measured once |
//! | [`luminosity`] | how to lift a face without making it glow |
//! | [`face_light`] | how far each face moves, and how far everybody moves together |
//! | [`subject`] | how much more present the subject should be |
//! | [`background`] | what is behind them, and how softly to calm it |
//! | [`freqsep`] | which part of a face is form and which part is texture |
//! | [`dodgeburn`] | where a retoucher would deepen and lift |
//! | [`shine`] | where the sheen is, and how much of it to take off |
//! | [`governor`] | what to give up when the frame has changed enough |
//! | [`guard`] | which plans this phase refuses to store |
//! | [`plan`] | one decoded frame in, one plan out |
//! | [`store`] | the four tables migration 19 adds |
//! | [`api`] | the frozen `LocalService` and the resumable project walk |
//!
//! Plus [`fixtures`], the synthetic ground truth every section 10.1 gate is measured against.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **where the light inside one photograph should be moved**, and it expresses
//! every one of those decisions as an instruction for a mask in the edit recipe. It does not
//! retouch skin (phase 20), remove objects (phase 24) or normalise a gallery (phase 25), and
//! the boundaries are structural rather than remembered: there is no blur radius, no
//! smoothing strength and no texture parameter anywhere in
//! [`aura_core::contract::local`] or in migration 19, and nothing here reads a second
//! photograph.
//!
//! ## The rule this phase adds, which every later phase inherits
//!
//! **`LocalService` is the only way to ask how light was shaped inside a photograph.**
//! Thirteenth service of its kind. Phase 20 retouches skin this phase has already evened and
//! must not do it twice, phase 25 normalises a gallery whose frames were each shaped
//! differently, and phase 27 has to be able to answer "why does this one look edited". Two
//! answers to "what did we do to this face" is a portrait that gets lifted twice.
//!
//! ## The two things to understand before reading the code
//!
//! **This phase does not own a mask.** Phase 18 owns masks;
//! [`aura_core::contract::local::MaskField`] is the input port this phase reads them through,
//! and there is no mask generator, no segmentation model and no geometric fallback anywhere
//! in this module. When a field does not arrive, the operations that needed it are gated
//! rather than guessed, and the plan says which. A second answer to "where does the subject
//! end" is a background reduction that traces an outline nothing else in the product agrees
//! with, which is precisely the halo this phase exists to avoid.
//!
//! **Every number here is spent against one shared allowance.** Six operations that are each
//! individually defensible add up to a photograph that looks processed, so
//! [`governor::allocate`] is not advisory: the budget is stored, the schema bounds it, and
//! `tests/eval/local_eval.rs` fails on a frame that exceeds it.

pub mod api;
pub mod background;
pub mod dodgeburn;
pub mod face_light;
pub mod fixtures;
pub mod freqsep;
pub mod governor;
pub mod guard;
pub mod luminosity;
pub mod measure;
pub mod plan;
pub mod policy;
pub mod shine;
pub mod store;
pub mod subject;

pub use api::{Local, LocalPass, PassReport};
pub use dodgeburn::SHAPING_VER;
pub use plan::{Analyser, FrameContext, FrameOutcome, ANALYSIS_VER, LOCAL_LEVEL, MODEL_VER};
pub use policy::{PolicyTable, ScenePolicy};
pub use store::{LocalStore, BYTES_PER_IMAGE};
