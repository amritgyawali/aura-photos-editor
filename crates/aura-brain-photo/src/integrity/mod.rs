//! An honest technical verdict about one frame, where it matters.
//!
//! PHASE-09's single feature. Every earlier phase in the wedding brain answered a
//! question about *what a photograph is* - what it looks like, who is in it, what it is
//! of, what it was shot alongside. This one answers whether it worked.
//!
//! ## The nine modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`calibration`] | what does "sharp" mean on this body, and what does noise cost on it |
//! | [`focus`] | is the *right* subject sharp, and if not, where did the focus land |
//! | [`motion`] | was the blur a mistake or a decision |
//! | [`exposure`] | how much of the exposure can be brought back |
//! | [`noise`] | is it noisier than this scene at this ISO on this body tolerates |
//! | [`eyes`] | are the eyes that matter open, and does it matter that they are not |
//! | [`flags`] | which of the fourteen bits are true, and what sentence explains each |
//! | [`score`] | one number, and why it is a product rather than a sum |
//! | [`analyse`] | one decoded frame in, one verdict out |
//!
//! Two more that section 4 does not name and ADR-0019 section 7 records: [`store`] for
//! the two tables migration 9 adds, and [`api`] for the frozen `IntegrityService` and the
//! resumable project walk. Phases 06, 07 and 08 all ended up with the same pair, for the
//! same reason: an analyser that also owns its SQL is an analyser nobody can test without
//! a database.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **what is technically true of a photograph**. It does not decide whether
//! anybody sees it: section 2.2 puts that in phase 12, and the boundary is kept
//! structurally rather than by discipline. There is no `culled` column in migration 9,
//! no rank, and nothing in this crate can reject, delete or order a frame for delivery.
//!
//! That restraint costs more here than it did in phases 05, 07 and 08, because this is
//! the phase whose output looks most like a verdict. `technical_score = 0.31` reads as a
//! rejection, and section 12's first failure mode is that false rejections destroy trust
//! instantly. So the score is evidence, the flags are evidence, and the two bits that
//! say a photograph is *good* - `INTENTIONAL_MOTION` and `EYES_CLOSED_OK` - are as
//! carefully computed as any penalty.
//!
//! ## The rule this phase adds, which every later phase inherits
//!
//! **`IntegrityService` is the only way to ask whether a frame worked.** No phase may
//! keep its own sharpness measure, its own blink detector or its own idea of what
//! "recoverable" means. Two answers to "is this frame sharp" is two culling decisions
//! that disagree, and phase 12's coverage guarantee is written against this one.

pub mod analyse;
pub mod api;
pub mod calibration;
pub mod exposure;
pub mod eyes;
pub mod flags;
pub mod focus;
pub mod motion;
pub mod noise;
pub mod score;
pub mod store;

pub use analyse::{Analyser, FrameContext, FrameExif, ANALYSIS_VER, INTEGRITY_LEVEL};
pub use api::{Integrity, IntegrityPass, PassReport};
pub use calibration::{Calibration, CalibrationTable};
pub use focus::{FocusClass, FocusResult, MODEL_VER};
pub use store::{IntegrityStore, Provenance, BYTES_PER_IMAGE};
