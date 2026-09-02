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
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::struct_field_names,
    clippy::too_many_lines,
    clippy::similar_names,
    // Every byte count in this crate is a `u64` in the contract and an `INTEGER` in SQLite, which
    // is signed. The cast is at the boundary between the two and the value is a file size; a
    // wedding whose delivery exceeded 9.2 exabytes has other problems.
    clippy::cast_possible_wrap
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
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )
)]

//! Delivery. PHASE-30.
//!
//! `aura-export` answers "did the bytes survive the write". This crate answers "did the bytes
//! survive the *journey*", and the split is deliberate: those are two questions with two answers,
//! and a photographer whose backup drive is failing needs to be told something different from one
//! whose gallery service is down.
//!
//! ## Read this before reading the modules
//!
//! **The unit of an upload is a file with a digest.** [`resume`] stores per-file state, so a
//! network drop is a pause rather than a restart: a resumed job re-sends the tail of one file and
//! not the head of a wedding. A state machine whose unit was the job would have two states and no
//! way to be resumed at all - which is invariant 5 in the one place where the network makes it
//! unavoidable.
//!
//! **A provider is two things, and separating them is what makes a new one cheap.** A
//! [`providers::Provider`] knows what a service's collections are called, how a set maps onto one,
//! and how it reports what it received. A [`providers::Transport`] knows how to put bytes somewhere
//! and how to ask what arrived. Section 6.2's "adding a provider must not touch core code" is that
//! split: a new gallery service is a new `Provider`, and it reuses whichever transport it needs.
//!
//! **This build ships no network transport, and that is a lint rather than an omission.**
//! `scripts/check-banned.sh` refuses every outbound networking API outside `aura-cloud`, because
//! phase 04's rule is that one crate owns the socket. A gallery provider is not a model provider,
//! so the honest choice was to build everything except the socket: [`providers::FolderTransport`]
//! is real and is what a folder, a NAS and an external drive use, and
//! [`providers::ScriptedTransport`] is what the resume tests drop connections through. What is
//! missing is one implementation of a two-method trait. ADR-0061 decision 4, exit condition C3.
//!
//! **A backup that diverges halts, and a backup that is already there does not.**
//! [`DeliveryCode::BackupAlreadyPresent`] and [`DeliveryCode::BackupDiverged`] are separate codes
//! with separate runbooks, because a destination that already holds the identical file is a re-run
//! and a destination that holds a *different* file under the same name is a drive somebody should
//! stop trusting. Collapsing them would make the second look like the first on every re-run.
//!
//! **`corrupt` is not `failed`.** A file that did not arrive and a file that arrived wrong need
//! different responses, and only the second is worth re-sending immediately. Two states, two codes,
//! two rows.
//!
//! ## What this crate does not contain
//!
//! No renderer, no encoder, no naming, and no second implementation of the read-back. Files arrive
//! here already written and already verified by `aura-export`; this crate's job is to move them and
//! to check what arrived, and "did the bytes survive" has exactly one implementation in this
//! product for the same reason "what does this photograph look like" does.

pub mod api;
pub mod backup;
pub mod errors;
pub mod mapping;
pub mod providers;
pub mod resume;
pub mod store;

pub use aura_core::contract::delivery::DeliveryCode;

/// Which build wrote a row. Bumped when the resume protocol or the digest comparison changes.
pub const DELIVERY_VER: u16 = 1;

/// Whether this build can reach a gallery over a network.
///
/// **False**, and on the wire rather than in a comment. `DeliveryOutline` and the panel both read
/// it, so a photographer who configures a provider and sees nothing upload is told why rather than
/// left to conclude their credentials are wrong. Phase 24's rule: an absent capability is
/// ignorance, not permission, and the two must never render the same.
pub const NETWORK_TRANSPORT_AVAILABLE: bool = false;
