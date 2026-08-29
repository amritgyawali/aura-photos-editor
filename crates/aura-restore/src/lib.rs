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

//! Rescuing the frames a wedding actually produces, without the smeared, over-sharpened output
//! that restoration tools are known for.
//!
//! PHASE-22's single feature. Section 1 says why it is worth a phase and then says why doing it
//! *harder* is the wrong instinct:
//!
//! > Restraint is the differentiator: applying denoise and sharpening everywhere is what makes
//! > AI-processed images look synthetic.
//!
//! ## The modules, in the order the pass uses them
//!
//! | Module | Question it answers |
//! |---|---|
//! | [`profiles`] | how far this kind of photograph may be repaired, and what this camera's sensor does |
//! | [`denoise`] | which of four tiers this frame's measured noise asks for, and what that becomes |
//! | [`kernel`] | how wide the blur on this frame actually is, measured from its own edges |
//! | [`sharpen`] | whether deconvolution would help here at all, and where it must not act |
//! | [`face_recovery`] | whether a face is inside the narrow band, and whether it is still the same person |
//! | [`selfcheck`] | what the plan did to the texture and the edges, measured through the renderer |
//! | [`schedule`] | where the heavy pixels are pushed, and when |
//! | [`decide`] | one decoded frame in, one plan out |
//! | [`store`] | the two tables migration 23 adds |
//! | [`api`] | the frozen `RestoreService` and the resumable project walk |
//!
//! Plus [`fixtures`], the synthetic ground truth every section 10.1 gate is measured against,
//! and [`errors`], this crate's own half of the split every phase since 09 has kept.
//!
//! ## The three things to understand before reading the code
//!
//! **This is the first phase that repairs rather than decides.** Every phase from 09 to 21 either
//! measured a photograph or chose how it should look, and a wrong answer from any of them is a
//! photograph graded differently from the one somebody wanted. A wrong answer here has *removed
//! information* - smeared lace, a ringed edge, a face that is slightly not the same face - and
//! none of the three can be edited back afterwards. Every operation is therefore bounded twice:
//! by a ceiling the code owns and a config file may only lower, and by a post-condition measured
//! on the rendered pixels.
//!
//! **The amount is conditioned on the sensor, not on a preference.** [`profiles::NoiseTable`]
//! carries a photon-transfer curve per camera body, and [`denoise`] chooses its tier from phase
//! 09's `noise_sigma_rel` - the measured sigma *relative to what this scene tolerates at this ISO
//! on this body*. A frame with no noise in it gets no denoising however strong the tier a studio
//! prefers, because there is no preference anywhere in this crate to express.
//!
//! **The identity constraint can only refuse.** [`face_recovery::enforce`] renders the plan,
//! embeds the face before and after through phase 06's frozen `PeopleService`, and compares. Over
//! the ceiling it reduces the strength and renders again; still over, and the face is **skipped**.
//! There is deliberately no outcome in which a face whose embedding has moved is delivered at a
//! lower strength, because a face that has drifted a little is a face that has drifted.
//!
//! ## What this phase decides, and what it refuses to
//!
//! It decides **how much noise to remove, whether an edge is worth recovering, and whether a
//! slightly soft face can be helped without changing who it is**, and expresses every decision as
//! reversible parameters in the edit recipe. It does not upscale (section 2.2, and there is
//! nowhere in the contract to put a scale factor), it does not reconstruct missing content (phase
//! 24), it does not try to undo motion blur (section 2.2 again - phase 12 rejects those frames
//! rather than rescuing them), and it does not touch skin texture or small features, which
//! phases 20 and 21 own.
//!
//! ## What this build does not have
//!
//! Two models are registered and **neither is consulted**, but for two different reasons and with
//! two different consequences, which ADR-0047 section 6 records.
//!
//! Denoising ships as a *measurement* - a noise-model-conditioned edge-preserving filter over
//! separated luminance and chroma planes - whose failure mode is leaving noise behind rather than
//! inventing texture. Section 10.1's PSNR/SSIM gate is measured against the bilinear baseline
//! exactly as written and the reference path clears it.
//!
//! Face recovery ships as a **refusal**: [`face_recovery::FACE_RECOVERY_HEAD_TRAINED`] is false
//! and [`face_recovery::solve`] returns `None` on every frame. There is deliberately no measured
//! fallback, because the measurement that would stand in for a face prior is unsharp masking on a
//! face, and that is not a weaker version of face recovery - it is a different operation with a
//! worse result and the same name.
//!
//! Beyond that, phase 06's face detector is a placeholder and phase 18's segmenter is one too, so
//! on a real photograph there are no regions to sharpen through and no faces to consider. Every
//! gate in section 10.1 is measured against synthetic frames whose noise, blur and structure were
//! painted into the pixels and read back through the real detectors, the real operators and the
//! real renderer. `docs/progress/PHASE-22-EXIT.md` carries all of it as conditions.

pub mod api;
pub mod decide;
pub mod denoise;
pub mod errors;
pub mod face_recovery;
pub mod fixtures;
pub mod kernel;
pub mod profiles;
pub mod schedule;
pub mod selfcheck;
pub mod sharpen;
pub mod store;

pub use api::{Restore, RestorePass, RestorePassReport};
pub use decide::{Analyser, RestoreFrame, RestoreOutcome, ANALYSIS_VER, MODEL_VER, RESTORE_LEVEL};
pub use profiles::{NoiseTable, RestoreProfiles, SceneRow};
pub use store::{RestoreStore, BYTES_PER_IMAGE};
