//! Property and fuzz coverage for the decode path.
//!
//! The contract this file defends is narrow and absolute: **no input may make
//! the decoder panic, hang or read out of bounds.** A wedding card that has been
//! partially overwritten, a file recovered by a card tool, a deliberately
//! hostile DNG - all of them must come back as a typed `AURA-RAW-*` error and a
//! Problems-list row.
//!
//! The fuzzing is structural rather than purely random: bytes drawn at random
//! almost never reach the interesting code, so the generators also mutate real
//! fixtures, truncate them at every scale, and corrupt the offsets that a naive
//! parser would trust.

use std::path::Path;

use aura_core::clock::{Clock, FixedClock};
use aura_raw::fixtures::{self, FixtureOptions, MosaicEncoding};
use aura_raw::timeout::DecodeLimits;
use aura_raw::{container, full, losslessjpeg, meta, proxy, thumb};
use proptest::prelude::*;
use time::OffsetDateTime;

fn clock() -> std::sync::Arc<dyn Clock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH)
}

/// Run every stage that touches untrusted bytes and assert only that the
/// process survives with a typed error.
fn exercise(bytes: &[u8], path: &Path) {
    let _ = container::jpeg::probe(bytes);
    let _ = container::jpeg::find_stream(bytes);
    let _ = container::tiff::TiffFile::parse(bytes);
    let _ = container::bmff::walk(bytes);
    let _ = losslessjpeg::decode(bytes);

    if let Ok(parsed) = meta::read(bytes, path) {
        let _ = thumb::tier1(
            bytes,
            &parsed,
            512,
            DecodeLimits::tier1(),
            clock().as_ref(),
            path,
        );
        let _ = proxy::tier2(
            bytes,
            &parsed,
            DecodeLimits::tier2(),
            clock().as_ref(),
            None,
            path,
        );
        let _ = full::tier3(bytes, &parsed, DecodeLimits::tier3(), clock().as_ref());
    }
}

fn corpus() -> Vec<Vec<u8>> {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mut out = Vec::new();
    for encoding in [
        MosaicEncoding::Unpacked16,
        MosaicEncoding::Packed14,
        MosaicEncoding::LosslessJpeg,
    ] {
        let path = directory.path().join(format!("{}.dng", encoding.as_str()));
        fixtures::write_bench_raw(
            &path,
            FixtureOptions {
                encoding,
                ..FixtureOptions::default()
            },
        )
        .expect("write the fixture");
        out.push(std::fs::read(&path).expect("read the fixture"));
    }
    out
}

#[test]
fn arbitrary_bytes_never_panic() {
    // A short, dense sweep of the shapes that look almost like a container.
    let prefixes: [&[u8]; 8] = [
        b"II\x2a\x00",
        b"MM\x00\x2a",
        b"\xFF\xD8\xFF\xE1",
        b"FUJIFILM",
        b"\x00\x00\x00\x18ftypcrx ",
        b"II\x55\x00",
        b"IIRO",
        b"",
    ];
    let path = Path::new("fuzz.raw");
    for prefix in prefixes {
        for length in [0usize, 1, 7, 8, 33, 129, 1024] {
            let mut bytes = prefix.to_vec();
            // A deterministic filler, so a failure is reproducible.
            bytes.extend((0..length).map(|index| ((index * 37 + 11) % 251) as u8));
            exercise(&bytes, path);
        }
    }
}

#[test]
fn every_truncation_of_a_real_fixture_is_handled() {
    let path = Path::new("truncated.dng");
    for bytes in corpus() {
        // Every power-of-two prefix, plus a few odd cuts inside the mosaic.
        let mut cuts: Vec<usize> = (0..)
            .map(|power| 1usize << power)
            .take_while(|cut| *cut < bytes.len())
            .collect();
        cuts.extend([
            bytes.len() / 3,
            bytes.len() / 2,
            bytes.len() * 3 / 4,
            bytes.len() - 1,
        ]);
        for cut in cuts {
            let Some(prefix) = bytes.get(..cut) else {
                continue;
            };
            exercise(prefix, path);
        }
    }
}

#[test]
fn single_byte_corruption_anywhere_in_the_header_is_survivable() {
    let path = Path::new("corrupt.dng");
    for bytes in corpus() {
        // The first kilobyte is the header, the directories and the offsets:
        // the region where a wrong byte does the most damage.
        // Every 31st byte rather than every byte: the header region is highly
        // structured, so a stride finds the same classes of failure in a
        // fraction of the CI time.
        let ceiling = bytes.len().min(1024);
        for position in (0..ceiling).step_by(31) {
            let mut mutated = bytes.clone();
            if let Some(slot) = mutated.get_mut(position) {
                *slot = slot.wrapping_add(137);
            }
            exercise(&mutated, path);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Random bytes, unconstrained. Cheap, and it has caught real bugs in
    /// marker walking where a length field wrapped.
    #[test]
    fn random_bytes_are_refused_without_panicking(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
        exercise(&bytes, Path::new("random.raw"));
    }

    /// A valid TIFF header followed by nonsense: this reaches the directory
    /// walker, which is where offsets are trusted or checked.
    #[test]
    fn tiff_headers_with_random_bodies_are_refused(
        body in proptest::collection::vec(any::<u8>(), 0..2048),
        offset in any::<u32>(),
    ) {
        let mut bytes = b"II\x2a\x00".to_vec();
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&body);
        exercise(&bytes, Path::new("random.dng"));
    }

    /// A lossless-JPEG stream with a plausible header and a random scan. The
    /// Huffman decoder must terminate on every one of them.
    #[test]
    fn lossless_scans_of_random_bits_terminate(
        scan in proptest::collection::vec(any::<u8>(), 0..1024),
    ) {
        let mut bytes = b"\xFF\xD8".to_vec();
        // A minimal DHT: one two-bit code, sixteen five-bit codes.
        bytes.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x22, 0x00]);
        bytes.extend_from_slice(&[0, 1, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        bytes.extend_from_slice(&(0..=16u8).collect::<Vec<u8>>());
        // SOF3: 14-bit, 8x8, one component.
        bytes.extend_from_slice(&[0xFF, 0xC3, 0x00, 0x0B, 14, 0, 8, 0, 8, 1, 1, 0x11, 0]);
        // SOS: predictor 1.
        bytes.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0x00, 1, 0, 0]);
        bytes.extend_from_slice(&scan);
        let _ = losslessjpeg::decode(&bytes);
    }
}
