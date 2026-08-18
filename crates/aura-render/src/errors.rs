//! The codes this crate raises, named once so a grep finds every site.
//!
//! Every constructor lives in `aura_core::errors::render`; this module is the crate's
//! shortlist and nothing more.

pub use aura_core::errors::render::{
    hash_mismatch, no_gpu_backend, parity_drift, profile_unknown, tiled_fallback,
    RENDER_HASH_MISMATCH, RENDER_NO_GPU_BACKEND, RENDER_PARITY_DRIFT, RENDER_PROFILE_UNKNOWN,
    RENDER_TILED_FALLBACK,
};
