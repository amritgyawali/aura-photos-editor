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
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    // A `Frame` carries one optional fact per naming token, and a template asked for a token the
    // frame does not have must *drop* it rather than substitute a plausible default. Folding them
    // into a bitmask would make "which fact was missing" - which is the whole of
    // `DeliveryCode::NameTokenUnavailable` - an argument about how to decode an integer.
    clippy::struct_excessive_bools,
    clippy::fn_params_excessive_bools,
    clippy::struct_field_names,
    // The writers are single passes with a lot of format in them: a TIFF IFD is a list of tags and
    // splitting it would put the tag and the value it describes in different files. The store's
    // readers are long for the reason every store in this repository is - a column list is a
    // column list. The same exemption phases 14, 24 to 27 and 29 took.
    clippy::too_many_lines,
    clippy::similar_names,
    // `x`, `y`, `w`, `h`, `r`, `g`, `b`. Every one of them is the conventional name for what it
    // is, in a module about pixels or about colour primaries, and spelling them out would make
    // a matrix expression unreadable in exchange for nothing.
    clippy::many_single_char_names,
    clippy::match_same_arms,
    clippy::inconsistent_digit_grouping,
    clippy::unreadable_literal
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

//! Export. PHASE-30.
//!
//! Twenty-nine phases decided things about photographs. This one writes them to disk, and that
//! single difference is what every structural choice in this crate follows from.
//!
//! ## Read this before reading the modules
//!
//! A row that is wrong is a row somebody re-runs. **A file that is wrong is a gallery a couple
//! already has**, and there is no re-run for that. So this crate is built around one asymmetry: it
//! is always better to fail an export than to deliver a bad one.
//!
//! **Every file is read back.** [`verify::write_and_verify`] writes, flushes, syncs, re-opens, reads
//! and hashes - and the hash on [`aura_core::ExportedFile`] is the hash of what came *back*, never
//! of the buffer that went out. A short write, a full volume whose filesystem reported success, a
//! NAS that acknowledges and drops, and a failing SD card all produce a correct buffer and a wrong
//! file. Nothing but a read-back notices, and section 6.1's first sentence is that photographers
//! have lost galleries to exactly this.
//!
//! **A verification failure stops the job.** The only per-item failure in this product that halts a
//! run. A gallery missing one photograph is a phone call; a gallery containing one corrupt
//! photograph is a photograph nobody notices until the couple opens it. And a verification failure
//! is almost never about the file - it is about the volume, which means the next three hundred
//! frames are at the same risk. ADR-0061 decision 3.
//!
//! **A name is resolved, never overwritten.** [`naming::plan`] assigns every file in a job a name
//! before anything is written, resolves collisions by suffix, and returns the whole plan - which is
//! what `export_preview_names` shows a photographer before they commit a wedding to a template. Two
//! cameras produce `DSC_0431` twice on any real wedding; a writer that silently kept one of them
//! delivers 3,998 files out of 4,000 and reports success.
//!
//! **The resize happens in linear light and the sharpening does not.** [`resample::downscale`]
//! linearises the encoded samples, filters, and re-encodes, because averaging encoded values
//! darkens every edge - the classic gamma-incorrect downscale, and the reason a badly resized
//! wedding looks muddier than the original. [`resample::sharpen`] runs on the encoded samples,
//! because an unsharp mask's amount is defined against display response and one applied in linear
//! rings a highlight several times harder than a shadow. Two operations, two domains, and the
//! reason is different in each.
//!
//! **Nothing here re-derives a pixel.** The pixels come from phase 14's `RenderService` through the
//! [`read::Source`] port and from nowhere else. An exporter with its own output transform is a
//! delivered JPEG that does not match the proof the couple approved, and nothing would record which
//! of the two a gallery came from.
//!
//! **Stripping the location is the default and keeping it is the exception.** [`metadata::build`]
//! writes the copyright, the creator and the contact and writes no GPS unless a policy explicitly
//! keeps it. The getting-ready chapter of a wedding is shot at somebody's house.
//!
//! ## What this crate does not contain
//!
//! No renderer, no decoder, no cull engine, no opinion about what belongs in a gallery, and no
//! network. What to export arrives as an [`aura_core::ExportJob`] - a list somebody chose - and the
//! facts a naming template needs arrive through [`read::Field`]. There is no path from here to a
//! socket: getting a delivered file somewhere else is `aura-delivery`'s job, and the split is what
//! keeps "did the bytes survive the write" and "did the bytes survive the wire" two questions with
//! two answers.

pub mod api;
pub mod errors;
pub mod fixtures;
pub mod icc;
pub mod jpeg;
pub mod manifest;
pub mod metadata;
pub mod naming;
pub mod png;
pub mod read;
pub mod resample;
pub mod sets;
pub mod store;
pub mod tiff;
pub mod verify;

/// Which build wrote a row, and what invalidates every stored export record.
///
/// Bumped when the *writers* change in a way that would produce different bytes for the same
/// pixels: a new JPEG quantisation table, a different resampler, a changed ICC blob. Not bumped for
/// a change to the naming, the metadata policy or the store, because none of those changes a
/// photograph.
///
/// It is on `export_job.engine_versions` rather than in a column, because a delivered file's
/// provenance is four values and this is one of them. Phase 14's rule.
pub const EXPORT_VER: u16 = 1;

/// The name this crate reports as the engine that wrote a file.
pub const ENGINE: &str = "aura-export/1";
