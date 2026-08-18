//! What colour the light was, and how bright the people in it should be.
//!
//! PHASE-15's single feature. Section 0 calls it "the most visible AI decision in the
//! product" and section 1 says why:
//!
//! > Correct exposure and believable skin colour across a whole wedding is what
//! > photographers judge an editor by in the first ten seconds.
//!
//! ## The twelve modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`targets`] | what a photograph of this kind should be exposed to |
//! | [`stats`] | what the pixels say, measured once |
//! | [`neutrals`] | what in this frame is supposed to be white |
//! | [`skin_locus`] | where this person's skin actually sits |
//! | [`illuminant`] | what lights are here, and which one is on the people |
//! | [`wb`] | what each candidate light does to that skin |
//! | [`solve`] | which light to believe, and how much of it to remove |
//! | [`exposure`] | how far to move the exposure, and what stops it |
//! | [`reference`] | which frames the rest of this chapter is matched to |
//! | [`analyse`] | one decoded frame in, one estimate out |
//! | [`store`] | the three tables migration 15 adds |
//! | [`api`] | the frozen `ToneService` and the resumable project walk |
//!
//! Plus [`fixtures`], the synthetic ground truth every section 10.1 gate is measured
//! against. Section 4 names six of these; `docs/adr/ADR-0031-exposure-white-balance-and-skin.md`
//! section 3 records the other six and why each of them is not a subdivision of one of the six.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **the exposure and the white balance of one photograph**. It does not grade
//! one: section 2.2 puts the tone curve, the contrast and the HSL in phase 16, the
//! photographer's style in 17, local masks and local white balance in 18, and gallery-wide
//! normalisation in 25. The boundary is kept structurally: there is no curve, contrast,
//! saturation or mask anywhere in `image_tone_estimate`, and this crate's only route to a
//! pixel is `aura_recipe::schema::merge` writing three fields.
//!
//! ## The two rules this phase adds, which every later phase inherits
//!
//! **`ToneService` is the only way to ask what colour the light was.** Eleventh service of
//! its kind. Phase 16 grades on top of these values, 17 shifts them, 18 corrects locally
//! against them, 25 normalises a gallery toward them and 26 matches two cameras with them.
//! Two answers to "what temperature was this room" is an album that does not match the
//! gallery.
//!
//! **A skin target is measured, never assumed.** There is no ideal-skin constant in this
//! module, in `aura_core::contract::tone`, in migration 15 or in
//! `config/exposure_targets.toml`. Section 6.3's fairness argument is that a fixed target is
//! how an editor lightens dark skin while believing it is correcting a cast, and the defence
//! is that nothing in this code path has a constant it could compare a person against. What
//! it has is [`skin_locus::LocusBuilder`], which learns that person's own appearance from
//! this wedding and then asks every white-balance hypothesis to be consistent with it.

pub mod analyse;
pub mod api;
pub mod exposure;
pub mod fixtures;
pub mod illuminant;
pub mod neutrals;
pub mod reference;
pub mod skin_locus;
pub mod solve;
pub mod stats;
pub mod store;
pub mod targets;
pub mod wb;

pub use analyse::{Analyser, FrameContext, FrameOutcome, ANALYSIS_VER, MODEL_VER, TONE_LEVEL};
pub use api::{PassReport, Tone, TonePass};
pub use illuminant::AsShot;
pub use skin_locus::LocusBuilder;
pub use store::{ToneStore, BYTES_PER_IMAGE};
pub use targets::{Mood, SceneTarget, TargetTable};
