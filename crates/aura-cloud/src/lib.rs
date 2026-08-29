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

//! The governed cloud AI gateway: one door, and the only crate in AURA allowed
//! to open a socket.
//!
//! A photographer pastes one API key and the product gains a reasoning layer.
//! Everything hard about that is in the word *governed*.
//!
//! Cloud reasoning is the only part of this product that costs money per use,
//! that sends a client's photographs to somebody else's computer, and that
//! answers differently on Tuesday than it did on Monday. Left to each phase to
//! arrange for itself, that becomes nine phases with nine privacy policies, nine
//! retry loops and no way to answer "what did this wedding cost". So there is
//! exactly one entry point, [`gateway::CloudAiGateway`], and every module here
//! exists to keep one of its promises:
//!
//! * [`keys`] holds the key in the operating system's own credential store, never
//!   in `argv`, never in the catalog, never in a log;
//! * [`payload`] builds the only things that may be uploaded - downscaled JPEG
//!   derivatives - and refuses everything else, including a RAW buffer, by type;
//! * [`redact`] strips the payload down to an allow-list and scrubs key-shaped
//!   strings out of anything that could reach a log;
//! * [`budget`] prices every call *before* it is made and refuses the ones the
//!   user cannot afford;
//! * [`schema`], [`validate`] and [`repair`] turn a language model's text into a
//!   typed value, or into exactly one repair round trip and then a local answer;
//! * [`cache`] makes the second run of a wedding nearly free;
//! * [`audit`] records every call, including the ones that never happened;
//! * [`fallback`] is where invariant 6 is actually kept - the product completes a
//!   full wedding with no network at all;
//! * [`agent`] is the bounded loop phases 27 and 29 will build on.
//!
//! ## Two ports, neither of them frozen
//!
//! [`provider::Provider`] turns the gateway's vocabulary into a vendor's;
//! [`provider::Transport`] carries the bytes. Four providers and three transports
//! ship. Neither trait is a frozen contract, deliberately: a new vendor or a TLS
//! stack must be addable without an ADR and without a caller noticing, in exactly
//! the way `Backend` works inside `aura-infer`.
//!
//! What *is* frozen is [`contract::cloud`], and phases 07, 10, 12, 13, 24, 27, 28,
//! 29 and 30 are written against it.
//!
//! ## What this build can and cannot reach
//!
//! [`http::HttpTransport`] is a complete HTTP/1.1 client, and it does not speak
//! TLS. It therefore reaches `http://` endpoints - a local or studio-network
//! OpenAI-compatible server, which is what [`compat`] exists for - and not the
//! public HTTPS endpoints of Anthropic, OpenAI or Google. Those providers'
//! request shaping, response parsing, error mapping and pricing are complete and
//! tested against cassettes; only the socket underneath is waived. The reasoning,
//! and the condition on which the waiver expires, is in
//! `docs/adr/ADR-0009-cloud-ai-policy.md`.
//!
//! CI never touches a network. [`cassette::CassetteTransport`] replays recorded
//! responses, and it is what every test and `aura-cli verify --phase 04` run
//! against.

pub mod agent;
pub mod anthropic;
pub mod audit;
pub mod budget;
pub mod cache;
pub mod cassette;
pub mod cleanup_judgement;
pub mod compat;
pub mod couple_hint;
pub mod explain_summary;
pub mod fallback;
pub mod gateway;
pub mod google;
pub mod http;
pub mod moment_significance;
pub mod openai;
pub mod payload;
pub mod provider;
pub mod redact;
pub mod repair;
pub mod schema;
pub mod tasks;
pub mod validate;

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod cloud;
}

pub mod keys;

pub use cassette::{Cassette, CassetteTransport, Matcher};
pub use cleanup_judgement::{
    CleanupJudgement, CleanupJudgementInput, CleanupJudgementOutput, CLEANUP_JUDGEMENT_SCHEMA,
    CLEANUP_JUDGEMENT_SYSTEM,
};
pub use contract::cloud::{
    CloudError, CloudResult, CloudTask, ImagePart, PromptSpec, Source, Tier, Validate,
};
pub use couple_hint::{CandidateStats, CoupleHint, CoupleHintInput, CoupleHintOutput, PairStats};
pub use explain_summary::{ExplainSummary, ExplainSummaryInput, ExplainSummaryOutput, ReasonFact};
pub use gateway::{CallContext, CloudAiGateway, CloudPolicy};
pub use keys::{KeyStore, MemoryKeyStore, OsKeyStore, SecretKey};
pub use moment_significance::{
    FrameStats, MomentSignificance, MomentSignificanceInput, MomentSignificanceOutput,
};
pub use provider::{
    OfflineTransport, Provider, ProviderClient, ProviderConfig, ProviderKind, Transport,
};
pub use tasks::{Scored, SegmentInput, SegmentNaming, SegmentNamingOutput};
