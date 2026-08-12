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
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! The signed model registry.
//!
//! Model files are the only artefact in this product that arrives over a network
//! and is then executed. That single sentence decides everything in this crate:
//!
//! * **Verification order is fixed and offline** (Article IX rule S6): signature
//!   over the manifest, then sha256 per file, then the model card, then load. The
//!   first failure stops the chain, and no step needs the network.
//! * **The previous version keeps working.** A new version is `pending` until it
//!   has completed one real inference; only then does it become `active`. A
//!   version that fails its first use is rolled back automatically, and the
//!   photographer keeps the quality they had this morning.
//! * **Nothing is ever half-installed.** Transfers land in a `.part` file, are
//!   verified whole, and are renamed into place. A machine that loses power
//!   mid-update starts with the old file or the new one.
//!
//! The crate implements [`aura_infer::ModelSource`], which is the only thing the
//! runtime knows about it. `aura-infer` does not depend on this crate: the
//! dependency points this way so that the runtime cannot grow a notion of trust,
//! a network, or a file layout.

pub mod delta;
pub mod download;
pub mod registry;
pub mod rollback;
pub mod swap;
pub mod verify;

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod manifest;
}

pub use contract::manifest::{
    InputSpec, ModelEntry, ModelsLock, PrecisionPolicy, Variant, LOCK_SCHEMA,
};
pub use download::{fetch, FileTransport, TransferProgress, Transport};
pub use registry::ModelRegistry;
pub use rollback::{InstallState, VersionState};
pub use verify::{sha256_hex, verify_file, verify_manifest};
