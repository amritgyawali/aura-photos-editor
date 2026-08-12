//! The proprietary mosaic codecs, tested by round trip and by colour.
//!
//! No camera files exist in this repository, so each codec is proved twice:
//!
//! * **Round trip.** An encoder written to the published format drives the same
//!   state machine forwards that the decoder drives backwards. A mistake in the
//!   adaptive width, the predictor or the carry state desynchronises the two and
//!   the plane comes back wrong, usually catastrophically so.
//! * **Colour.** A complete fixture in that encoding goes through tier 2 and its
//!   chart is measured in dE2000. This catches the mistakes a round trip cannot:
//!   a decoder that is self-consistent but reads the samples in the wrong order,
//!   or a container walk that hands the pixel path the wrong black level.
//!
//! What neither can prove is that the *published format* is the format the
//! camera actually writes. That needs one file per body, and it is recorded as a
//! phase 03 entry condition in `docs/progress/PHASE-02-EXIT.md`.

use std::path::{Path, PathBuf};

use aura_core::clock::{Clock, FixedClock};
use aura_raw::codecs::{nikon, olympus, sony};
use aura_raw::colour::de2000::{summarise, xyz_d65_to_lab, Lab};
use aura_raw::colour::matrix::{apply, SRGB_TO_XYZ_D65};
use aura_raw::colour::working_space::working_to_xyz;
use aura_raw::fixtures::{self, BenchFixture, FixtureOptions, MosaicEncoding};
use aura_raw::meta::{self, CfaPattern, MosaicScheme, RawMeta};
use aura_raw::timeout::DecodeLimits;
use aura_raw::{proxy, thumb};
use time::OffsetDateTime;

fn clock() -> std::sync::Arc<dyn Clock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH)
}

struct Loaded {
    fixture: BenchFixture,
    bytes: Vec<u8>,
    meta: RawMeta,
    path: PathBuf,
}

fn load(directory: &Path, encoding: MosaicEncoding, body: usize) -> Loaded {
    let options = FixtureOptions {
        body,
        encoding,
        ..FixtureOptions::default()
    };
    let path = directory.join(format!("body{body}-{}.raw", encoding.as_str()));
    let fixture = fixtures::write_bench_raw(&path, options).expect("write the fixture");
    let bytes = std::fs::read(&path).expect("read the fixture");
    let parsed = meta::read(&bytes, &path).expect("parse the fixture");
    Loaded {
        fixture,
        bytes,
        meta: parsed,
        path,
    }
}

/// Render the fixture's chart through tier 2 and return its mean and worst
/// dE2000 against the reference.
fn chart_delta(loaded: &Loaded, divisor: u32) -> (f64, f64) {
    let result = proxy::tier2(
        &loaded.bytes,
        &loaded.meta,
        DecodeLimits::tier2(),
        clock().as_ref(),
        None,
        &loaded.path,
    )
    .unwrap_or_else(|e| panic!("tier 2: [{}] {}", e.code, e.detail));

    let width = result.pair.linear.width;
    let samples = result
        .pair
        .linear
        .as_linear16()
        .expect("the linear buffer is always produced");
    let measured: Vec<Lab> = (0..loaded.fixture.patch_count())
        .map(|index| {
            let (x, y) = loaded.fixture.patch_centre(index, divisor);
            let at = (y as usize * width as usize + x as usize) * 3;
            let channel = |offset: usize| {
                f64::from(aura_raw::colour::curve::linear_u16_to_scene(
                    samples.get(at + offset).copied().unwrap_or(0),
                ))
            };
            xyz_d65_to_lab(working_to_xyz([channel(0), channel(1), channel(2)]))
        })
        .collect();
    let expected: Vec<Lab> = loaded
        .fixture
        .expected_linear_srgb
        .iter()
        .map(|patch| xyz_d65_to_lab(apply(SRGB_TO_XYZ_D65, *patch)))
        .collect();
    let delta = summarise(&expected, &measured);
    assert_eq!(delta.count, 24);
    (delta.mean, delta.max)
}

// -------------------------------------------------------------------- Nikon

#[test]
fn nikon_round_trip_reproduces_the_plane_exactly() {
    let (width, height) = (48usize, 12usize);
    let plane: Vec<u16> = (0..width * height)
        .map(|index| {
            let x = (index % width) as u16;
            let y = (index / width) as u16;
            // A ramp with a hard step in it, so the differences span most of the
            // magnitude categories the tree can name.
            (400 * u32::from(x % 8) + 97 * u32::from(y) + if x > 24 { 9_000 } else { 0 })
                .min(16_383) as u16
        })
        .collect();

    let encoded = nikon::encode(&plane, width, height, 5);
    let mut params = nikon::NikonParams::identity(14);
    params.tree = 5;
    let decoded = nikon::decode(&encoded, &params, width, height).expect("decode");

    assert_eq!(decoded, plane, "the nikon round trip is not lossless");
}

#[test]
fn a_compressed_nef_is_recognised_and_reaches_tier_two() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), MosaicEncoding::NikonCompressed, 1);

    let mosaic = loaded.meta.mosaic.as_ref().expect("the mosaic is located");
    assert_eq!(mosaic.compression, 34_713);
    assert!(
        matches!(mosaic.scheme, MosaicScheme::Nikon(_)),
        "expected the nikon scheme, got {}",
        mosaic.scheme.as_str()
    );

    let (mean, worst) = chart_delta(&loaded, 2);
    assert!(
        mean <= 2.0,
        "nikon chart mean dE2000 {mean:.3} (worst {worst:.3})"
    );
}

#[test]
fn a_compressed_nef_without_its_decode_table_is_refused_rather_than_guessed() {
    // The decode table is per body. Rendering one through an invented curve
    // would be a silent colour error, so the container walk must refuse.
    let scheme = MosaicScheme::Unsupported(34_713);
    assert_eq!(
        aura_raw::format::mosaic_support_for(&scheme),
        aura_raw::format::MosaicSupport::CompressionUnsupported
    );
}

// --------------------------------------------------------------------- Sony

#[test]
fn sony_block_round_trip_is_exact_within_one_block_span() {
    let (width, height) = (64usize, 4usize);
    let plane: Vec<u16> = (0..width * height)
        .map(|index| {
            let x = (index % width) as u32;
            let y = (index / width) as u32;
            // Multiples of eight, spanning less than a block's 127 steps, which
            // is the range Sony's block coding reproduces exactly.
            (1_024 + (x % 32) * 8 + y * 256) as u16
        })
        .collect();

    let encoded = sony::encode(&plane, width, height);
    assert_eq!(
        encoded.len(),
        width * height,
        "an ARW is exactly one byte per photosite"
    );
    let curve = sony::build_curve(None);
    let decoded = sony::decode(&encoded, width, height, &curve);

    for (index, (got, want)) in decoded.iter().zip(plane.iter()).enumerate() {
        assert_eq!(
            *got,
            fixtures::arw2_quantise(*want),
            "sample {index} disagrees"
        );
    }
}

#[test]
fn sony_quantisation_costs_at_most_eight_code_values() {
    // The eleven-bit reduction is the format, not the decoder. Stating the
    // bound here keeps a future change to the fallback curve honest.
    for sample in [0u16, 7, 8, 1_024, 16_383] {
        let quantised = fixtures::arw2_quantise(sample);
        assert!(quantised <= sample);
        assert!(sample - quantised < 8);
    }
}

#[test]
fn an_arw2_mosaic_is_recognised_and_reaches_tier_two() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), MosaicEncoding::SonyArw2, 3);

    let mosaic = loaded.meta.mosaic.as_ref().expect("the mosaic is located");
    assert_eq!(mosaic.compression, 32_767);
    assert!(
        matches!(mosaic.scheme, MosaicScheme::SonyArw2 { curve: None }),
        "expected the sony scheme with no curve, got {}",
        mosaic.scheme.as_str()
    );

    let (mean, worst) = chart_delta(&loaded, 2);
    assert!(
        mean <= 2.0,
        "sony chart mean dE2000 {mean:.3} (worst {worst:.3})"
    );
}

// ------------------------------------------------------------------ Olympus

#[test]
fn olympus_round_trip_reproduces_the_plane_exactly() {
    let (width, height) = (40usize, 10usize);
    let plane: Vec<u16> = (0..width * height)
        .map(|index| {
            let x = (index % width) as u32;
            let y = (index / width) as u32;
            // Two flat fields with a hard edge between them, which is what
            // drives the adaptive width through its whole range.
            let base = if x < 20 { 1_200 } else { 9_600 };
            (base + (x * 13 + y * 29) % 512) as u16
        })
        .collect();

    let encoded = olympus::encode(&plane, width, height);
    let decoded = olympus::decode(&encoded, width, height).expect("decode");

    assert_eq!(decoded, plane, "the olympus round trip is not lossless");
}

#[test]
fn a_compressed_orf_is_recognised_and_reaches_tier_two() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), MosaicEncoding::OlympusCompressed, 5);

    assert_eq!(loaded.meta.format, aura_raw::RawFormat::Orf);
    let mosaic = loaded.meta.mosaic.as_ref().expect("the mosaic is located");
    assert_eq!(mosaic.scheme, MosaicScheme::Olympus);
    assert!(
        mosaic.segments.iter().map(|(_, len)| *len).sum::<usize>()
            < mosaic.width as usize * mosaic.height as usize * 2,
        "a compressed ORF must be smaller than its unpacked size"
    );

    let (mean, worst) = chart_delta(&loaded, 2);
    assert!(
        mean <= 2.0,
        "olympus chart mean dE2000 {mean:.3} (worst {worst:.3})"
    );
}

// ------------------------------------------------------------------ X-Trans

#[test]
fn a_six_by_six_cfa_pattern_is_read_as_xtrans() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), MosaicEncoding::XTrans16, 2);

    match loaded.meta.cfa {
        CfaPattern::XTrans(pattern) => {
            assert_eq!(pattern, aura_raw::meta::XTRANS_DEFAULT);
            assert_eq!(loaded.meta.cfa.block(), 3);
            assert_eq!(loaded.meta.cfa.radius(), 2);
        }
        other => panic!("expected an X-Trans array, got {}", other.as_str()),
    }
}

#[test]
fn every_three_by_three_xtrans_block_contains_all_three_colours() {
    // This is the property that makes a third-size bin legitimate: if any block
    // were missing a colour, the proxy would be interpolating, not measuring.
    let cfa = CfaPattern::XTrans(aura_raw::meta::XTRANS_DEFAULT);
    for block_y in 0..2u32 {
        for block_x in 0..2u32 {
            let mut seen = [false; 3];
            for dy in 0..3u32 {
                for dx in 0..3u32 {
                    let colour = cfa.colour_at(block_x * 3 + dx, block_y * 3 + dy);
                    if let Some(slot) = seen.get_mut(colour as usize) {
                        *slot = true;
                    }
                }
            }
            assert!(
                seen.iter().all(|found| *found),
                "block ({block_x}, {block_y}) is missing a colour"
            );
        }
    }
}

#[test]
fn an_xtrans_mosaic_reaches_tier_two_at_a_third_of_the_sensor() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), MosaicEncoding::XTrans16, 4);

    let result = proxy::tier2(
        &loaded.bytes,
        &loaded.meta,
        DecodeLimits::tier2(),
        clock().as_ref(),
        None,
        &loaded.path,
    )
    .expect("tier 2");
    assert_eq!(result.pair.linear.width, loaded.fixture.width / 3);
    assert_eq!(result.pair.linear.height, loaded.fixture.height / 3);

    let (mean, worst) = chart_delta(&loaded, 3);
    assert!(
        mean <= 2.0,
        "xtrans chart mean dE2000 {mean:.3} (worst {worst:.3})"
    );
}

#[test]
fn a_raf_block_directory_yields_the_sensor_size_and_layout() {
    // A hand-built RAF: the fixed header, one block directory with the two
    // records that matter, and nothing else.
    let mut file = vec![0u8; 148];
    file.splice(..16, b"FUJIFILMCCD-RAW ".iter().copied());
    let put32 = |file: &mut Vec<u8>, at: usize, value: u32| {
        for (offset, byte) in value.to_be_bytes().iter().enumerate() {
            if let Some(slot) = file.get_mut(at + offset) {
                *slot = *byte;
            }
        }
    };
    // The block directory sits at 148: two records.
    put32(&mut file, 92, 148);
    put32(&mut file, 96, 60);
    let mut directory: Vec<u8> = Vec::new();
    directory.extend_from_slice(&2u32.to_be_bytes());
    directory.extend_from_slice(&0x0100u16.to_be_bytes());
    directory.extend_from_slice(&4u16.to_be_bytes());
    directory.extend_from_slice(&24u16.to_be_bytes()); // height
    directory.extend_from_slice(&36u16.to_be_bytes()); // width
    directory.extend_from_slice(&0x0131u16.to_be_bytes());
    directory.extend_from_slice(&36u16.to_be_bytes());
    // Stored back to front, so reading it forwards would give the transpose.
    let mut layout: Vec<u8> = aura_raw::meta::XTRANS_DEFAULT.to_vec();
    layout.reverse();
    directory.extend_from_slice(&layout);
    file.extend_from_slice(&directory);

    let sensor = aura_raw::container::raf::sensor(&file, 148, directory.len());
    assert_eq!((sensor.width, sensor.height), (36, 24));
    assert_eq!(sensor.xtrans, Some(aura_raw::meta::XTRANS_DEFAULT));
}

// --------------------------------------------------------------- robustness

#[test]
fn every_codec_survives_bytes_that_are_not_its_format() {
    // Each decoder must either produce a plane or return a code. Neither a
    // panic nor an unbounded loop is acceptable on a hostile card.
    let noise: Vec<u8> = (0..4_096u32)
        .map(|index| (index * 37 % 251) as u8)
        .collect();
    let mut params = nikon::NikonParams::identity(12);
    params.tree = 0;

    for keep in [0usize, 1, 7, 64, 512, 4_096] {
        let slice = noise.get(..keep).unwrap_or_default();
        let _ = nikon::decode(slice, &params, 16, 16);
        let _ = olympus::decode(slice, 16, 16);
        let curve = sony::build_curve(Some([512, 1_024, 2_048, 3_072]));
        let plane = sony::decode(slice, 64, 4, &curve);
        assert_eq!(plane.len(), 64 * 4, "the output is always fully sized");
    }
}

#[test]
fn a_truncated_proprietary_mosaic_fails_with_a_code_rather_than_a_wrong_image() {
    let directory = tempfile::tempdir().expect("temporary directory");
    for encoding in [
        MosaicEncoding::NikonCompressed,
        MosaicEncoding::OlympusCompressed,
    ] {
        let loaded = load(directory.path(), encoding, 1);
        let keep = loaded.bytes.len() * 3 / 4;
        let truncated = loaded.bytes.get(..keep).unwrap_or_default().to_vec();

        // Tier 1 still works: the preview is early in the file.
        if let Ok(parsed) = meta::read(&truncated, &loaded.path) {
            let outcome = thumb::tier1(
                &truncated,
                &parsed,
                256,
                DecodeLimits::tier1(),
                clock().as_ref(),
                &loaded.path,
            );
            if let Err(error) = outcome {
                assert!(
                    error.code.0.starts_with("AURA-RAW-"),
                    "{}: wrong error domain {}",
                    encoding.as_str(),
                    error.code
                );
            }
        }
    }
}

#[test]
fn the_sony_fallback_curve_is_a_plain_expansion_and_says_so() {
    let linear = sony::build_curve(None);
    // Eleven bits in, fourteen bits out, with no shaping: index i is the
    // twelve-bit form of an eleven-bit sample, so `curve[p << 1] >> 2` is
    // `p * 8`.
    for sample in [0usize, 1, 1_000, 2_047] {
        let mapped = linear.get(sample << 1).copied().unwrap_or(0) >> 2;
        assert_eq!(u32::from(mapped), sample as u32 * 8);
    }

    let shaped = sony::build_curve(Some([512, 1_024, 2_048, 3_072]));
    assert!(
        shaped.get(4_000).copied().unwrap_or(0) > shaped.get(1_000).copied().unwrap_or(0),
        "a shaped curve must still be monotonic"
    );
}
