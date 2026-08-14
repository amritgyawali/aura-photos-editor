//! What one photograph is of: the scene, its attributes, and the rite in it.
//!
//! Five modules and one runner, and the split follows what changes independently:
//!
//! * [`taxonomy`] loads the ritual config, which a consultant edits;
//! * [`profile`] loads the threshold config, which a product manager signs off;
//! * [`attributes`] assembles the context features and decodes the attribute bits,
//!   which change when the catalog learns something new about a frame;
//! * [`classifier`] runs the scene head, which changes when a model is retrained;
//! * [`ritual`] runs the rite head, which changes when a tradition is added;
//! * [`pass`] walks a project, which changes when the storage rules change.
//!
//! Nothing here opens a pixel. The trunk is the phase 05 embedding, frozen, and the
//! whole of section 6.1 rests on that: scene inference costs almost nothing extra per
//! image because the expensive part already ran.

pub mod attributes;
pub mod classifier;
pub mod pass;
pub mod profile;
pub mod ritual;
pub mod taxonomy;
