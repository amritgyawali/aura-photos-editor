//! Container parsing and orientation: the parts that meet untrusted bytes.

use aura_raw::container::jpeg;
use aura_raw::container::tiff::{tag, TiffFile};
use aura_raw::fixtures::{self, FixtureOptions, MosaicEncoding};
use aura_raw::format::{sniff, MosaicSupport, RawFormat};
use aura_raw::meta::{self, CfaPattern};
use aura_raw::orientation::{apply, Orientation};

fn fixture_bytes(directory: &std::path::Path, options: FixtureOptions) -> (Vec<u8>, std::path::PathBuf) {
    let path = directory.join("fixture.dng");
    fixtures::write_bench_raw(&path, options).expect("write the fixture");
    let bytes = std::fs::read(&path).expect("read the fixture");
    (bytes, path)
}

// ------------------------------------------------------------------- sniff

#[test]
fn a_generated_fixture_is_recognised_as_a_dng() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (bytes, _) = fixture_bytes(directory.path(), FixtureOptions::default());
    assert_eq!(sniff(&bytes), RawFormat::Dng);
}

#[test]
fn the_extension_is_never_trusted() {
    // Bytes that are plainly a JPEG are a JPEG, whatever the file is called.
    let jpeg_bytes = aura_raw::codec::encode_jpeg(
        &aura_raw::codec::Rgb8 {
            width: 8,
            height: 8,
            data: vec![200; 8 * 8 * 3],
        },
        90,
    )
    .expect("encode");
    assert_eq!(sniff(&jpeg_bytes), RawFormat::Jpeg);

    // And an empty or short buffer is simply unknown, never a guess.
    assert_eq!(sniff(&[]), RawFormat::Unknown);
    assert_eq!(sniff(b"II"), RawFormat::Unknown);
    assert_eq!(sniff(b"\x89PNG\r\n\x1a\n"), RawFormat::Unknown);
}

#[test]
fn panasonic_and_olympus_magics_are_recognised_as_their_own_formats() {
    // Both keep the TIFF layout and change the version word.
    let mut panasonic = b"II\x55\x00\x08\x00\x00\x00".to_vec();
    panasonic.extend_from_slice(&[0u8; 8]);
    assert_eq!(sniff(&panasonic), RawFormat::Rw2);

    let mut olympus = b"IIRO\x08\x00\x00\x00".to_vec();
    olympus.extend_from_slice(&[0u8; 8]);
    assert_eq!(sniff(&olympus), RawFormat::Orf);
}

// -------------------------------------------------------------------- tiff

#[test]
fn the_tiff_reader_finds_the_tags_the_pipeline_depends_on() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (bytes, _) = fixture_bytes(directory.path(), FixtureOptions::default());
    let file = TiffFile::parse(&bytes).expect("parse");

    assert_eq!(file.text(tag::MAKE).as_deref(), Some("AURA"));
    assert_eq!(file.text(tag::MODEL).as_deref(), Some("Bench-01"));
    assert_eq!(file.scalar(tag::BLACK_LEVEL), Some(u64::from(fixtures::BLACK_LEVEL)));
    assert_eq!(file.scalar(tag::WHITE_LEVEL), Some(u64::from(fixtures::WHITE_LEVEL)));
    assert!(file.matrix3(tag::COLOR_MATRIX_2).is_some());
    assert!(file.find(tag::SUB_IFDS).is_some());
    assert!(file.ifds().len() >= 2, "IFD0 and the mosaic sub-directory");
}

#[test]
fn a_directory_offset_that_points_outside_the_file_is_refused() {
    // Valid header, first directory offset far past the end.
    let mut bytes = b"II\x2a\x00".to_vec();
    bytes.extend_from_slice(&9_000_000u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 16]);
    let error = TiffFile::parse(&bytes).expect_err("an impossible offset is refused");
    assert_eq!(error.code.0, "AURA-RAW-2002");
}

#[test]
fn a_directory_that_points_at_itself_terminates_instead_of_looping() {
    // IFD at offset 8 with zero entries whose "next" pointer is itself.
    let mut bytes = b"II\x2a\x00".to_vec();
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes());
    let file = TiffFile::parse(&bytes).expect("a self-referential chain still parses");
    assert_eq!(file.ifds().len(), 1);
}

// -------------------------------------------------------------------- jpeg

#[test]
fn probing_a_jpeg_reports_its_frame_and_its_true_extent() {
    let encoded = aura_raw::codec::encode_jpeg(
        &aura_raw::codec::Rgb8 {
            width: 37,
            height: 19,
            data: vec![64; 37 * 19 * 3],
        },
        85,
    )
    .expect("encode");
    let info = jpeg::probe(&encoded).expect("probe");
    assert_eq!((info.width, info.height), (37, 19));
    assert_eq!(info.components, 3);
    assert!(!info.is_lossless());
    assert_eq!(info.byte_len, Some(encoded.len()));
}

#[test]
fn a_stream_without_a_frame_header_is_refused() {
    let error = jpeg::probe(&[0xFF, 0xD8, 0xFF, 0xD9]).expect_err("no frame, no image");
    assert_eq!(error.code.0, "AURA-RAW-2002");
    assert!(jpeg::probe(b"not a jpeg at all").is_err());
}

#[test]
fn the_exif_block_is_found_inside_a_fixture_preview() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (bytes, path) = fixture_bytes(directory.path(), FixtureOptions::default());
    let parsed = meta::read(&bytes, &path).expect("parse");
    let preview = parsed.best_preview().copied().expect("a preview");
    let stream = bytes
        .get(preview.offset..preview.offset + preview.len)
        .expect("preview range");
    assert!(jpeg::looks_like_jpeg(stream));
    assert!(jpeg::probe(stream).is_ok());
}

// -------------------------------------------------------------------- meta

#[test]
fn metadata_carries_everything_the_colour_path_needs() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let (bytes, path) = fixture_bytes(directory.path(), FixtureOptions::default());
    let parsed = meta::read(&bytes, &path).expect("parse");

    assert_eq!(parsed.make, "AURA");
    assert_eq!(parsed.model, "Bench-01");
    assert_eq!(parsed.black_level, fixtures::BLACK_LEVEL);
    assert_eq!(parsed.white_level, fixtures::WHITE_LEVEL);
    assert_eq!(parsed.cfa, CfaPattern::Rggb);
    assert!(parsed.colour_matrix.is_some());
    assert!(parsed.mosaic.is_some());
    assert_eq!(parsed.mosaic_support(), MosaicSupport::Decodable);

    // As-shot neutral is stored as the camera's reading of a neutral subject,
    // so the multipliers we derive are its reciprocals.
    let expected_red = (1.0 / (1.0 / fixtures::WHITE_BALANCE[0])) as f32;
    assert!(
        (parsed.as_shot_neutral[0] - expected_red).abs() < 1e-3,
        "white balance multipliers were not inverted correctly: {:?}",
        parsed.as_shot_neutral
    );
}

#[test]
fn a_mosaic_we_cannot_decompress_is_reported_as_preview_only() {
    let directory = tempfile::tempdir().expect("temporary directory");
    // A JPEG has no mosaic at all, which is the "no mosaic" end of the matrix.
    let jpeg_bytes = aura_raw::codec::encode_jpeg(
        &aura_raw::codec::Rgb8 {
            width: 8,
            height: 8,
            data: vec![10; 8 * 8 * 3],
        },
        90,
    )
    .expect("encode");
    let path = directory.path().join("plain.jpg");
    let parsed = meta::read(&jpeg_bytes, &path).expect("a jpeg is readable");
    assert_eq!(parsed.mosaic_support(), MosaicSupport::NoMosaic);
    assert_eq!(parsed.previews.len(), 1);
}

#[test]
fn every_mosaic_encoding_produces_the_same_declared_geometry() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mut geometry = None;
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
        .expect("write");
        let bytes = std::fs::read(&path).expect("read");
        let parsed = meta::read(&bytes, &path).expect("parse");
        let mosaic = parsed.mosaic.expect("a mosaic");
        let shape = (mosaic.width, mosaic.height);
        match geometry {
            None => geometry = Some(shape),
            Some(expected) => assert_eq!(shape, expected, "{} disagrees", encoding.as_str()),
        }
    }
}

// ------------------------------------------------------------- orientation

#[test]
fn rotating_ninety_degrees_swaps_the_axes() {
    let data = vec![1u8, 2, 3, 4, 5, 6];
    let (out, width, height) = apply(&data, 2, 3, 1, Orientation::Rotate90);
    assert_eq!((width, height), (3, 2));
    assert_eq!(out, vec![5, 3, 1, 6, 4, 2]);
}

#[test]
fn every_orientation_is_a_permutation_of_the_same_pixels() {
    let data = vec![1u8, 2, 3, 4, 5, 6];
    for value in 1..=8u16 {
        let orientation = Orientation::from_exif(value);
        assert_eq!(orientation.to_exif(), value);
        let (out, width, height) = apply(&data, 2, 3, 1, orientation);
        assert_eq!(out.len(), data.len(), "orientation {value} changed the size");
        assert_eq!(width as usize * height as usize, data.len());
        let mut sorted = out.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![1, 2, 3, 4, 5, 6], "orientation {value} lost a pixel");
    }
}

#[test]
fn the_involutions_are_their_own_inverse() {
    let data = vec![1u8, 2, 3, 4, 5, 6];
    for orientation in [
        Orientation::FlipHorizontal,
        Orientation::FlipVertical,
        Orientation::Rotate180,
        Orientation::Transpose,
        Orientation::Transverse,
    ] {
        let (once, width, height) = apply(&data, 2, 3, 1, orientation);
        let (twice, back_width, back_height) = apply(&once, width, height, 1, orientation);
        assert_eq!((back_width, back_height), (2, 3));
        assert_eq!(twice, data, "{orientation:?} is not its own inverse");
    }
}

#[test]
fn an_unknown_orientation_value_is_treated_as_upright() {
    assert_eq!(Orientation::from_exif(0), Orientation::Normal);
    assert_eq!(Orientation::from_exif(99), Orientation::Normal);
}

#[test]
fn a_buffer_whose_length_disagrees_with_its_dimensions_is_returned_untouched() {
    let data = vec![1u8, 2, 3];
    let (out, width, height) = apply(&data, 4, 4, 1, Orientation::Rotate90);
    assert_eq!(out, data);
    assert_eq!((width, height), (4, 4));
}
