//! What a photograph is *worth*: expression, interaction, peak, reaction, score.
//!
//! PHASE-10 ships one feature - the app finds the moments that matter and ranks every
//! frame by emotional value - and this module is all of it that is not a contract, a
//! migration, a model or a screen.
//!
//! ## The one sentence to read before the rest
//!
//! **This module scores photographic expression. It makes no claim about anybody's inner
//! state.** Section 2.2 puts that permanently out of scope and section 12's second
//! failure mode is what happens when a product forgets. `FaceExpression::tears` is *how
//! much this face reads as crying in a photograph*; it is not a statement that somebody
//! was sad, and no reason code in `EmotionCode` can be made to say one.
//!
//! ## The eight files, and what each is responsible for
//!
//! | File | Answers |
//! |---|---|
//! | [`expression`] | eight continuous readings per face, from an aligned crop |
//! | [`gaze`] | where each face is looking, from geometry rather than a head |
//! | [`interaction`] | what the people are doing, from the frame plus a person prior |
//! | [`peak`] | which frame of a moment the action peaked in |
//! | [`reaction`] | which frames are reactions to which |
//! | [`score`] | nine features, a Bradley-Terry utility, a calibration and the reasons |
//! | [`weights`] | the scene, tradition, ranker and calibration table |
//! | [`store`] | the five tables, and the rules that live in the SQL |
//!
//! [`analyse`] fuses the first three into one decode per frame; [`api`] is the frozen
//! service and the project walk; [`fixtures`] builds frames whose expression is known by
//! construction, because the two shipped heads are placeholders and every gate in
//! section 10.1 is measured against an answer rather than against a model.
//!
//! ## What is deliberately not here
//!
//! **Choosing.** Section 2.2 puts final selection in phase 12, album sequencing and hero
//! picks in phase 29. Nothing in this module rejects, delivers, ranks into a gallery or
//! deletes a frame, and there is no column, field or command that would let it.
//! [`api::Emotion`] returns an *ordering* because "ranks every frame by emotional value"
//! is this phase's headline, and an ordering is not a selection.

pub mod analyse;
pub mod api;
pub mod expression;
pub mod fixtures;
pub mod gaze;
pub mod interaction;
pub mod peak;
pub mod reaction;
pub mod score;
pub mod store;
pub mod weights;

pub use analyse::{Analyser, FrameContext, ANALYSIS_VER, EMOTION_LEVEL};
pub use api::{Emotion, EmotionPass, EmotionReport};
pub use expression::MODEL_VER;
pub use store::{EmotionStore, BYTES_PER_IMAGE};
pub use weights::{Calibration, EmotionWeights, Ranker, SceneWeights, TraditionWeights};
