//! The wedding as an ordered story: chapters, their boundaries, and their covers.
//!
//! Six modules, and the pipeline reads top to bottom:
//!
//! * [`hmm`] smooths per-frame posteriors into a chapter path;
//! * [`changepoint`] finds the boundaries in the smoothed timeline;
//! * [`keyframe`] chooses the frame that represents each chapter;
//! * [`segment`] is the storage, and the place the user-override rules live;
//! * [`naming`] is the cloud path for the chapters the local model is unsure about;
//! * [`api`] is `Story`, the one implementation of the frozen `StoryService`.
//!
//! The order of the first two is the design and is settled in ADR-0015 section 6:
//! **smooth first, segment second.** A wrong label that has already become a chapter
//! boundary is not a label any more.

pub mod api;
pub mod changepoint;
pub mod hmm;
pub mod keyframe;
pub mod naming;
pub mod segment;

pub use api::{ScenePassReport, Story, StoryReport};
pub use segment::{ScenePassRow, SceneRow, StoryStore};
