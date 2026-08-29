//! The small fixes a retoucher makes without being asked. PHASE-21.
//!
//! Phase 20 changed what somebody's **skin** looks like. This is the rest of the retoucher's
//! short list - a stray hair calmed, teeth and eyes evened, a lint taken off a lapel, a
//! reflection lifted off a pair of glasses - and section 1 says why it is a phase of its own:
//!
//! > They are also where automation most easily looks creepy - whitened teeth, glowing eyes,
//! > erased hair. Doing them conservatively and identity-aware is the differentiator, not doing
//! > them harder.
//!
//! ## The modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`matrix`] | which operations this studio permits, on this kind of photograph, and how far |
//! | [`glare`] | where a specular sheet has destroyed the record over somebody's eyes |
//! | [`borrow`] | whether a sibling frame may repair one, and by how well it aligned |
//! | [`hair`] | which thin structures beside the hair are strays over a quiet background |
//! | [`clothing`] | what is on the garment that was never part of it |
//! | [`teeth`] | how far these teeth sit outside the locus, and how much of that may go |
//! | [`eyes`] | how much redness is in the sclera and how much definition the iris has lost |
//! | [`guard`] | what the plan did to the catchlights, the hairline and the teeth, measured |
//! | [`ops`] | one decoded frame in, one plan out |
//! | [`store`] | the three tables migration 22 adds |
//! | [`api`] | the frozen `MicroService` and the resumable project walk |
//!
//! Plus [`fixtures`], the synthetic ground truth every section 10.1 gate is measured against.
//!
//! ## The three things to understand before reading the code
//!
//! **Everything this phase touches is permanent.** Phase 20 could separate a spot from a mole
//! and act only on the first. There is no such separation here: teeth, eyes, hair and clothing
//! are all features of a person rather than things that happened to them. What replaces the veto
//! is a ceiling enforced on the *pixel* - [`guard::enforce`] runs the plan through the real
//! renderer and measures three quantities on the result, and a family that misses its floor
//! after three re-solves is withdrawn rather than attenuated.
//!
//! **A borrow may only replace pixels that carry no information.** [`borrow`] is the first code
//! in this product that composites two photographs, and section 2.2 forbids the version everybody
//! asks for first. The rule that separates them is not about the mechanism: a specular sheet has
//! destroyed the record, and a closed eye *is* the record.
//! `aura_core::contract::micro::MIN_SPECULAR_FRACTION` is that rule as a number.
//!
//! **There is no absolute colour target anywhere.** Every measurement is against the frame's own
//! content: teeth against a locus centred on *this frame's* neutral, sclera against its own
//! redness, hair against the background immediately behind it, lint against the fabric around it.
//! A module with no constant to move a pixel toward cannot have a preferred appearance, which is
//! what lets `docs/skin-fairness.md` and `docs/retouch-ethics.md` say what they say.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **which small distractions may be reduced and by how much**, and expresses every
//! decision as a reversible operation in the edit recipe. It does not touch skin (phase 20 owns
//! it and `MicroRegion::Skin` is read-only here), it does not denoise or sharpen (phase 22), it
//! does not remove objects (phase 24), and it does not reshape anybody - which is not a scope
//! note but a permanent product decision, recorded in `docs/retouch-ethics.md` and enforced by
//! there being nowhere in `aura_core::contract::micro` or in migration 22 to put one.
//!
//! ## What this build does not have
//!
//! The three shipped heads are untrained placeholders and none is consulted, so what runs is the
//! measured detection described in ADR-0045 section 6. Phase 06's face detector is a placeholder
//! and phase 18's segmenter is one too, so on a real photograph there are no regions to work
//! through and no faces to work on. Every gate in section 10.1 is measured against synthetic
//! frames whose flyaways, glare sheets, lint and teeth were painted into the pixels and read back
//! through the real detectors, operators and renderer.
//! `docs/progress/PHASE-21-EXIT.md` carries all of it as conditions.

pub mod api;
pub mod borrow;
pub mod clothing;
pub mod eyes;
pub mod fixtures;
pub mod glare;
pub mod guard;
pub mod hair;
pub mod matrix;
pub mod ops;
pub mod store;
pub mod teeth;

pub use api::{Micro, MicroPass, MicroPassReport};
pub use matrix::{MicroTable, OpSwitches, SceneRow};
pub use ops::{Analyser, MicroFrame, MicroOutcome, Sibling, ANALYSIS_VER, MICRO_LEVEL, MODEL_VER};
pub use store::{MicroStore, BYTES_PER_IMAGE};
