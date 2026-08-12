//! The proprietary mosaic codecs, one module per manufacturer.
//!
//! Everything here decodes a compression scheme that a camera maker invented and
//! never published, but which the open-source photography community - dcraw,
//! LibRaw, RawTherapee, darktable - documented well enough that a correct
//! decoder can be written from the description. These modules are independent
//! safe-Rust implementations of those documented formats, written so that
//! `aura-raw` can keep `#![forbid(unsafe_code)]` and a permissive licence
//! (ADR-0004).
//!
//! Every module in here follows the same three rules:
//!
//! * **No panics on hostile input.** Reads go through [`bits::BitReader`], which
//!   returns zeros past the end of the buffer and records that it did, so a
//!   truncated file is reported as truncated rather than trapping in a loop.
//! * **An encoder beside every decoder.** The fixture writer builds a real file
//!   in each format, so the decoder is tested by round trip. Without camera
//!   files that is the only honest way to test a decoder, and it catches the
//!   state-machine mistakes that a single sample file would hide.
//! * **No invented constants.** Where a format needs a per-body table we cannot
//!   read, the decoder says so and the render is flagged, rather than guessing
//!   a table that would silently shift colour.

pub mod bits;
pub mod nikon;
pub mod olympus;
pub mod sony;
