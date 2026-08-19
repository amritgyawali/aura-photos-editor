//! What a photograph should look like, and the guarantee that making it look like that never
//! turns anybody's skin unnatural.
//!
//! PHASE-16's single feature. Section 1 says why it exists:
//!
//! > Exposure and WB make a frame correct; tone and colour make it *look like a photograph
//! > someone paid for*.
//!
//! and why the guarantee is the interesting part:
//!
//! > Every product that grades globally eventually produces orange or grey skin, and
//! > photographers notice immediately.
//!
//! ## The twelve modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`intent`] | what a photograph of this kind should look like |
//! | [`tone`] | how far the five parameters should move |
//! | [`curve`] | what curve gives the subject the separation the scene wants |
//! | [`content`] | what is actually in the frame |
//! | [`harmony`] | what should change about its colour, and why |
//! | [`hsl`] | what the recipe's eight bands should say |
//! | [`clip_guard`] | whether the grade clips, and whether it is too much grade |
//! | [`skin_guard`] | **what the grade did to the people in the frame** |
//! | [`analyse`] | one decoded frame in, one decision out |
//! | [`store`] | the one table migration 16 adds |
//! | [`api`] | the frozen `ColourService` and the resumable project walk |
//! | [`codec`] | the six documents the table stores |
//!
//! Plus [`fixtures`], the synthetic ground truth every section 10.1 gate is measured against.
//! Section 4 names six of these files;
//! `docs/adr/ADR-0033-tone-curves-hsl-and-skin-protection.md` records the other six and why
//! each of them is not a subdivision of one of the six.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **the tone and colour of one photograph**. It does not decide a photographer's
//! style (phase 17), it does not adjust anything locally (phases 18 and 19), it does not
//! normalise a gallery (phase 25) and it does not convert anything to black and white (phase
//! 29). The boundary is kept structurally: there is no mask, no profile, no per-photographer
//! anything in `image_colour_decision`, and this crate's only route to a pixel is
//! `aura_recipe::schema::merge` writing nine fields from `aura-app`.
//!
//! ## The two rules this phase adds, which every later phase inherits
//!
//! **`ColourService` is the only way to ask how a photograph should be graded.** Twelfth
//! service of its kind. Phase 17 shifts these values, 18 grades locally on top of them, 25
//! normalises a gallery of them and 27 checks them. Two answers to "how much contrast does
//! this frame want" is an album that does not match the gallery.
//!
//! **A guarantee is measured, not asserted.** Section 6.3 promises that skin never shifts
//! measurably; [`skin_guard`] grades this frame's own skin pixels through the real renderer,
//! measures the hue and chroma they actually moved, and re-solves - or withdraws the colour
//! entirely - until they are inside the ceilings. The measurement is stored on the row, so
//! "skin never shifts" is `SELECT MAX(skin_hue_shift)` rather than a sentence in a document.
//! A product that could only assert it would have no way to find out it had stopped being
//! true.

pub mod analyse;
pub mod api;
pub mod clip_guard;
pub mod codec;
pub mod content;
pub mod curve;
pub mod fixtures;
pub mod harmony;
pub mod hsl;
pub mod intent;
pub mod skin_guard;
pub mod store;
pub mod style;
pub mod tone;

pub use analyse::{Analyser, FrameContext, FrameOutcome, ANALYSIS_VER, COLOUR_LEVEL, MODEL_VER};
pub use api::{Colour, ColourPass, FrameExif, PassReport};
pub use intent::{IntentTable, SceneIntent};
pub use skin_guard::{Grade, SkinSamples};
pub use store::{ColourStore, BYTES_PER_IMAGE};
pub use tone::{ToneParams, TONE_HEAD_TRAINED};
