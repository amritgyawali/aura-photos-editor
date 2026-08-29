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

//! Retouching skin the way a retoucher does: the mark goes, the person stays.
//!
//! PHASE-20 single feature. Section 1 says why it is worth a phase and why it is the most
//! scrutinised output in the product:
//!
//! > Clients want to look like themselves on a good day, not like a mannequin.
//!
//! ## The modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`presets`] | how much care a photograph of this kind gets |
//! | [`strength`] | how much care *this person* gets, everywhere in the gallery |
//! | [`blemish`] | which marks on this face are temporary |
//! | [`permanent`] | which marks are this person, and must never be touched |
//! | [`undereye`] | how much of the shadow under the eyes to lift, and no more |
//! | [`evening`] | which blotches to calm without smoothing anything |
//! | [`texture_guard`] | whether the skin kept its own texture, measured through the renderer |
//! | [`ops`] | one decoded frame in, one plan out |
//! | [`guard`] | which plans this phase refuses to store |
//! | [`store`] | the four tables migration 21 adds |
//! | [`api`] | the frozen `RetouchService` and the resumable project walk |
//!
//! Plus [`fixtures`], the synthetic ground truth every section 10.1 gate is measured against,
//! and [`errors`], this crate own half of the split every phase since 09 has kept.
//!
//! ## The three things to understand before reading the code
//!
//! **A permanent feature is a veto, not a discount.** [`permanent`] builds a protect set and
//! [`ops`] removes every candidate that touches one *before* strength, preset or texture guard
//! is consulted. There is no value of any parameter at which a mole is partially inpainted,
//! and [`aura_core::contract::retouch::ProtectedKind::is_absolute`] marks the one kind - a
//! tattoo - that a photographer cannot switch off either. Migration 21 carries a trigger that
//! aborts the delete, so the promise survives a caller nobody has written yet.
//!
//! **The texture guarantee is measured, not asserted.** [`texture_guard`] applies the plan
//! through `aura_render::retouch` - the same code the delivered JPEG goes through - and divides
//! the high-band skin energy after by the energy before. Below the preset floor it re-solves at
//! a lower strength, and if three re-solves do not reach it the retouch is **withdrawn** and the
//! frame ships unretouched. Phase 16 wrote this rule for skin colour; this is the same rule for
//! skin texture.
//!
//! **Strength belongs to a person, not to a photograph.** [`strength`] computes one number per
//! identity per project from four gallery statistics, and every frame in the wedding uses it.
//! The frame own face size and scene decide *which operations run*, never how strong they are.
//! `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 6 has the argument,
//! and section 10.1 cross-frame consistency gate is what forces it.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **what should be removed from somebody skin and what must stay on it**, and
//! expresses every one of those decisions as a reversible operation in the edit recipe. It does
//! not touch hair, teeth, eyes, clothing or glare (phase 21), it does not denoise or sharpen
//! (phase 22), and it does not reshape a body - which is not a scope note but a permanent
//! product decision, recorded in section 11 of `docs/plan/CLAUDE.md` and enforced by there
//! being nowhere in [`aura_core::contract::retouch`] or in migration 21 to put one.
//!
//! ## The rule this phase adds, which every later phase inherits
//!
//! **`RetouchService` is the only way to ask what was done to somebody skin.** Sixteenth
//! service of its kind. Phase 21 retouches what this phase left alone and must not re-smooth
//! what it did; phase 25 normalises a gallery of these decisions; phase 27 has to be able to
//! say why a face looks worked on. Two answers to "what did we do to her skin" is a delivery in
//! which the album and the gallery disagree about somebody face.
//!
//! ## What this build does not have
//!
//! The two shipped heads are untrained placeholders and neither is consulted, so what runs is
//! the measured detector described in ADR-0043 section 7. Phase 06 face detector is a
//! placeholder too, so on a real photograph there are no faces to retouch and no cross-frame
//! correspondence to protect anybody with. Every gate in section 10.1 is measured against
//! synthetic faces whose marks are painted into the pixels and read back through the real
//! pipeline. `docs/progress/PHASE-20-EXIT.md` carries all of it as conditions.

pub mod api;
pub mod blemish;
pub mod errors;
pub mod evening;
pub mod fixtures;
pub mod guard;
pub mod micro;
pub mod ops;
pub mod permanent;
pub mod presets;
pub mod store;
pub mod strength;
pub mod texture_guard;
pub mod undereye;

pub use api::{PassReport, Retouch, RetouchPass};
pub use micro::{Micro, MicroPass, MicroPassReport, MicroStore, MicroTable};
pub use ops::{Analyser, FrameContext, FrameOutcome, ANALYSIS_VER, MODEL_VER, RETOUCH_LEVEL};
pub use presets::{PresetRow, PresetTable, SceneRow};
pub use store::{RetouchStore, BYTES_PER_IMAGE};
