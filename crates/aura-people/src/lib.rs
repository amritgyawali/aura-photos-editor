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
    clippy::cast_sign_loss
)]

//! Who matters at this wedding, and the encrypted store that knows.
//!
//! Phase 06's single feature is a subject hierarchy: every later decision in this
//! product needs to know that sharpness on the bride's face outranks sharpness on a
//! stranger's elbow. This crate owns the durable half of that - the identities, the
//! co-occurrence graph, the roles, the timelines, and the sealed biometric store they
//! are derived from. The per-frame half, which is arithmetic over pixels, lives in
//! `aura_vision::face`.
//!
//! ## The split, and why it is structural
//!
//! `aura-vision` has no catalog dependency, so it **cannot** persist a template even by
//! mistake. `aura-people` has no socket dependency, so it cannot upload one. The two
//! most sensitive capabilities in the product - biometric storage and network access -
//! are in different dependency graphs, and `scripts/check-banned.sh` keeps them there.
//!
//! ## The five things this crate guarantees
//!
//! 1. **Templates are sealed.** [`vault`] is the only way in or out, the key lives in
//!    the operating system's credential store, and `faces.embed` never holds a vector.
//!    A catalog copied off the machine without the keychain entry has no biometric data
//!    in it.
//! 2. **Nothing crosses a wedding.** Every query is scoped by `project_id`, section
//!    2.2's prohibition on cross-wedding identity is enforced by the schema and by
//!    [`store::PeopleStore::assert_same_project`], and `AURA-SEC-9004` halts rather than
//!    degrades.
//! 3. **The photographer wins.** `identities.user_locked` is checked inside the
//!    statement that would overwrite it, and [`api::People::regroup`] replays the
//!    decision journal onto every fresh grouping before it draws a single conclusion
//!    from it.
//! 4. **Erasure is real and it is verified.** [`store::PeopleStore::erase`] deletes the
//!    key first - so a crash mid-erasure leaves unreadable data rather than readable
//!    data - then the crops, then the rows, then checks that nothing survived. Culling
//!    and edit decisions are untouched.
//! 5. **Coverage travels with every answer.** `SubjectHierarchy::coverage` is the phase
//!    05 rule inherited: a grouping conclusion drawn over a 40 %-scanned wedding is a
//!    conclusion about 40 % of a wedding, and a caller who does not know that will
//!    present it as fact.
//!
//! ## What is not real yet
//!
//! The three shipped models are placeholders with the right architecture and no
//! training, so the *templates* carry no identity information. Every mechanism around
//! them is real and measured against the synthetic ground truth in
//! `aura_vision::face::fixtures`. This is condition C1 in
//! `docs/progress/PHASE-06-EXIT.md`, it is a Sev 2 trigger, and **no later phase may
//! claim a quality result that depends on face recognition being accurate until it
//! closes.**

pub mod api;
pub mod errors;
pub mod graph;
pub mod importance;
pub mod scan;
pub mod store;
pub mod timeline;
pub mod vault;

pub use api::{GroupReport, People};
pub use graph::{CooccurrenceGraph, Edge};
pub use importance::{ImportanceInput, ImportanceProposal, IMPORTANCE_VER};
pub use scan::{FaceScanner, ScanReport};
pub use store::{EraseReport, FaceRow, IdentityLink, IdentityRow, PeopleStore};
pub use timeline::{Appearance, Gap, IdentityTimeline};
pub use vault::{BiometricKeyStore, MemoryKeyStore, RecordKind, Vault, ENVELOPE_VER};
