//! The codes this crate raises, named once so a grep finds every site.
//!
//! Every constructor lives in `aura_core::errors::render`; this module is the crate's
//! shortlist and nothing more. Phase 09's convention, applied a sixth time: a crate that
//! spells its own error strings is a crate whose user-facing copy drifts from the registry.

pub use aura_core::errors::render::{
    recipe_from_future, recipe_invalid, sidecar_unreadable, user_field_protected, xmp_unreadable,
    RENDER_RECIPE_FROM_FUTURE, RENDER_RECIPE_INVALID, RENDER_SIDECAR_UNREADABLE,
    RENDER_USER_FIELD_PROTECTED, RENDER_XMP_UNREADABLE,
};
