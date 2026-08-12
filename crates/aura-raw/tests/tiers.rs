//! The three tiers, end to end, against generated bench fixtures.
//!
//! Every assertion here traces to a phase 02 acceptance criterion:
//!
//! * tier 1 produces real pixels from the embedded JPEG, oriented;
//! * tier 2 produces both an 8-bit and a 16-bit buffer, and reproduces a colour
//!   chart within dE2000 2.0 on all eight bodies;
//! * tier 3 tiled output is identical to whole-image output;
//! * a file that cannot be decoded fails loudly, with a code, and never panics.

use std::path::{Path, PathBuf};

use aura_core::clock::{Clock, FixedClock};
use aura_raw::colour::de2000::{summarise, xyz_d65_to_lab, Lab};
use aura_raw::colour::matrix::{apply, SRGB_TO_XYZ_D65};
use aura_raw::colour::working_space::working_to_xyz;
use aura_raw::contract::pixels::{PixelData, PixelSource};
use aura_raw::fixtures::{self, BenchFixture, FixtureOptions, MosaicEncoding};
use aura_raw::meta::{self, RawMeta};
use aura_raw::timeout::DecodeLimits;
use aura_raw::{full, proxy, thumb};
use time::OffsetDateTime;

struct Loaded {
    fixture: BenchFixture,
    bytes: Vec<u8>,
    meta: RawMeta,
    path: PathBuf,
}

fn clock() -> std::sync::Arc<dyn Clock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH)
}

fn load(directory: &Path, options: FixtureOptions) -> Loaded {
    let path = directory.join(format!(
        "body{}-{}.dng",
        options.body,
        options.encoding.as_str()
    ));
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

// ------------------------------------------------------------------- tier 1

#[test]
fn tier_one_copies_the_embedded_preview_rather_than_demosaicing() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), FixtureOptions::default());

    let result = thumb::tier1(
        &loaded.bytes,
        &loaded.meta,
        512,
        DecodeLimits::tier1(),
        clock().as_ref(),
        &loaded.path,
    )
    .expect("tier 1");

    assert_eq!(result.buffer.source, PixelSource::Embedded);
    assert_eq!(result.render_path, "embedded_jpeg");
    assert!(result.warning.is_none());
    assert!(result.buffer.width.max(result.buffer.height) <= 512);
    assert!(!result.jpeg.is_empty(), "a cacheable jpeg must be produced");
    match &result.buffer.data {
        PixelData::Srgb8(pixels) => assert_eq!(
            pixels.len(),
            result.buffer.width as usize * result.buffer.height as usize * 3
        ),
        other => panic!("tier 1 must be 8-bit sRGB, got {}", other.as_str()),
    }
}

#[test]
fn tier_one_falls_back_to_a_render_when_the_file_has_no_preview() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(
        directory.path(),
        FixtureOptions {
            with_preview: false,
            ..FixtureOptions::default()
        },
    );

    let result = thumb::tier1(
        &loaded.bytes,
        &loaded.meta,
        512,
        DecodeLimits::tier1(),
        clock().as_ref(),
        &loaded.path,
    )
    .expect("tier 1 falls back rather than failing");

    assert_eq!(result.buffer.source, PixelSource::Demosaiced);
    assert_eq!(result.render_path, "demosaic_quarter");
    let warning = result.warning.expect("the fallback is reported");
    assert_eq!(warning.code.0, "AURA-RAW-2003");
}

#[test]
fn tier_one_applies_the_orientation_tag() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let upright = load(directory.path(), FixtureOptions::default());
    let rotated = load(
        directory.path(),
        FixtureOptions {
            orientation: 6,
            ..FixtureOptions::default()
        },
    );

    let a = thumb::tier1(
        &upright.bytes,
        &upright.meta,
        512,
        DecodeLimits::tier1(),
        clock().as_ref(),
        &upright.path,
    )
    .expect("tier 1");
    let b = thumb::tier1(
        &rotated.bytes,
        &rotated.meta,
        512,
        DecodeLimits::tier1(),
        clock().as_ref(),
        &rotated.path,
    )
    .expect("tier 1");

    assert_eq!(rotated.meta.orientation, 6);
    assert_eq!(
        (a.buffer.width, a.buffer.height),
        (b.buffer.height, b.buffer.width),
        "a quarter turn must exchange the axes"
    );
}

// ------------------------------------------------------------------- tier 2

fn proxy_patches(fixture: &BenchFixture, samples: &[u16], width: u32) -> Vec<Lab> {
    (0..fixture.patch_count())
        .map(|index| {
            let (x, y) = fixture.patch_centre(index, 2);
            let at = (y as usize * width as usize + x as usize) * 3;
            let channel = |offset: usize| {
                f64::from(aura_raw::colour::curve::linear_u16_to_scene(
                    samples.get(at + offset).copied().unwrap_or(0),
                ))
            };
            xyz_d65_to_lab(working_to_xyz([channel(0), channel(1), channel(2)]))
        })
        .collect()
}

fn reference_patches(fixture: &BenchFixture) -> Vec<Lab> {
    fixture
        .expected_linear_srgb
        .iter()
        .map(|patch| xyz_d65_to_lab(apply(SRGB_TO_XYZ_D65, *patch)))
        .collect()
}

/// Acceptance criterion: ColorChecker mean dE2000 <= 2.0 on all eight bodies.
#[test]
fn the_colour_chart_survives_the_proxy_pipeline_on_every_bench_body() {
    let directory = tempfile::tempdir().expect("temporary directory");
    for body in 1..=8usize {
        let loaded = load(
            directory.path(),
            FixtureOptions {
                body,
                ..FixtureOptions::default()
            },
        );
        let result = proxy::tier2(
            &loaded.bytes,
            &loaded.meta,
            DecodeLimits::tier2(),
            clock().as_ref(),
            None,
            &loaded.path,
        )
        .expect("tier 2");

        assert_eq!(result.render_path, "demosaic_half");
        assert_eq!(result.pair.srgb.source, PixelSource::Demosaiced);

        let samples = result
            .pair
            .linear
            .as_linear16()
            .expect("the linear buffer is always produced");
        let measured = proxy_patches(&loaded.fixture, samples, result.pair.linear.width);
        let expected = reference_patches(&loaded.fixture);
        let delta = summarise(&expected, &measured);

        assert_eq!(delta.count, 24);
        assert!(
            delta.mean <= 2.0,
            "{}: mean dE2000 {:.3} exceeds 2.0 (worst patch {:.3})",
            loaded.fixture.model,
            delta.mean,
            delta.max
        );
    }
}

#[test]
fn the_proxy_is_produced_in_both_representations_at_the_same_size() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), FixtureOptions::default());
    let result = proxy::tier2(
        &loaded.bytes,
        &loaded.meta,
        DecodeLimits::tier2(),
        clock().as_ref(),
        None,
        &loaded.path,
    )
    .expect("tier 2");

    let srgb = &result.pair.srgb;
    let linear = &result.pair.linear;
    assert_eq!((srgb.width, srgb.height), (linear.width, linear.height));
    assert_eq!(
        srgb.as_srgb8().map(<[u8]>::len),
        Some(srgb.width as usize * srgb.height as usize * 3)
    );
    assert_eq!(
        linear.as_linear16().map(<[u16]>::len),
        Some(linear.width as usize * linear.height as usize * 3)
    );
    assert_eq!(srgb.colour_space, aura_raw::ColourSpace::Srgb);
    assert_eq!(linear.colour_space, aura_raw::ColourSpace::LinearRec2020);
}

#[test]
fn every_mosaic_encoding_decodes_to_the_same_colours() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mut baseline: Option<Vec<Lab>> = None;

    for encoding in [
        MosaicEncoding::Unpacked16,
        MosaicEncoding::Packed14,
        MosaicEncoding::LosslessJpeg,
    ] {
        let loaded = load(
            directory.path(),
            FixtureOptions {
                encoding,
                ..FixtureOptions::default()
            },
        );
        let result = proxy::tier2(
            &loaded.bytes,
            &loaded.meta,
            DecodeLimits::tier2(),
            clock().as_ref(),
            None,
            &loaded.path,
        )
        .unwrap_or_else(|e| panic!("tier 2 for {}: [{}] {}", encoding.as_str(), e.code, e.detail));

        let samples = result.pair.linear.as_linear16().expect("linear buffer");
        let measured = proxy_patches(&loaded.fixture, samples, result.pair.linear.width);

        match &baseline {
            None => baseline = Some(measured),
            Some(reference) => {
                let delta = summarise(reference, &measured);
                assert!(
                    delta.max <= 1.0,
                    "{} drifted from the unpacked baseline by dE2000 {:.3}",
                    encoding.as_str(),
                    delta.max
                );
            }
        }
    }
}

#[test]
fn a_file_without_a_colour_matrix_uses_the_bundled_profile() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(
        directory.path(),
        FixtureOptions {
            with_colour_matrix: false,
            ..FixtureOptions::default()
        },
    );
    assert!(loaded.meta.colour_matrix.is_none());

    let result = proxy::tier2(
        &loaded.bytes,
        &loaded.meta,
        DecodeLimits::tier2(),
        clock().as_ref(),
        None,
        &loaded.path,
    )
    .expect("tier 2");

    assert_eq!(
        result.profile.source,
        aura_raw::colour::profile::ProfileSource::Bundled
    );
    assert!(
        result.warnings.is_empty(),
        "a signed bundled profile is not a degraded render"
    );

    let samples = result.pair.linear.as_linear16().expect("linear buffer");
    let measured = proxy_patches(&loaded.fixture, samples, result.pair.linear.width);
    let delta = summarise(&reference_patches(&loaded.fixture), &measured);
    assert!(delta.mean <= 2.0, "bundled profile drifted: {:.3}", delta.mean);
}

#[test]
fn clipping_statistics_are_recorded_for_the_exposure_phases() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), FixtureOptions::default());
    let result = proxy::tier2(
        &loaded.bytes,
        &loaded.meta,
        DecodeLimits::tier2(),
        clock().as_ref(),
        None,
        &loaded.path,
    )
    .expect("tier 2");

    // The chart is deliberately inside the rails, so nothing should clip.
    assert!(
        result.clipping.highlight < 0.01,
        "unexpected highlight clipping: {}",
        result.clipping.highlight
    );
    assert!(result.clipping.shadow < 0.01);
}

// ------------------------------------------------------------------- tier 3

/// Acceptance criterion: tiled full decode matches whole-image decode exactly.
#[test]
fn tiled_full_decode_matches_a_whole_image_decode() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), FixtureOptions::default());

    let tiled = full::tier3(
        &loaded.bytes,
        &loaded.meta,
        DecodeLimits::tier3(),
        clock().as_ref(),
    )
    .expect("tiled decode");
    let whole = full::tier3_whole(
        &loaded.bytes,
        &loaded.meta,
        DecodeLimits::tier3(),
        clock().as_ref(),
    )
    .expect("whole decode");

    assert_eq!((tiled.width, tiled.height), (whole.width, whole.height));
    let image = tiled.as_tiled().expect("tier 3 is tiled");
    assert_eq!(image.tile_size, full::TILE_SIZE);
    assert_eq!(image.halo, full::HALO);

    let assembled = full::assemble(image, tiled.width, tiled.height);
    let reference = whole.as_linear16().expect("whole decode is linear");
    assert_eq!(assembled.len(), reference.len());
    assert!(
        assembled == reference,
        "tiled and whole-image decodes disagree; the halo is wrong"
    );
}

#[test]
fn tier_three_refuses_to_render_from_an_embedded_preview() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("preview-only.jpg");
    // A plain JPEG has no mosaic at all, which is the same situation as a RAW
    // whose compression we cannot decode.
    let jpeg = aura_raw::codec::encode_jpeg(
        &aura_raw::codec::Rgb8 {
            width: 16,
            height: 16,
            data: vec![128; 16 * 16 * 3],
        },
        90,
    )
    .expect("encode a jpeg");
    std::fs::write(&path, &jpeg).expect("write");

    let parsed = meta::read(&jpeg, &path).expect("a jpeg is readable");
    let error = full::tier3(&jpeg, &parsed, DecodeLimits::tier3(), clock().as_ref())
        .expect_err("tier 3 has no fallback");
    assert_eq!(error.code.0, "AURA-RAW-2007");
}

// ------------------------------------------------------------ failure paths

#[test]
fn a_truncated_file_fails_with_a_code_instead_of_panicking() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let loaded = load(directory.path(), FixtureOptions::default());

    for keep in [8usize, 64, 256, loaded.bytes.len() / 2] {
        let truncated = loaded.bytes.get(..keep).unwrap_or_default().to_vec();
        let outcome = meta::read(&truncated, &loaded.path).and_then(|parsed| {
            thumb::tier1(
                &truncated,
                &parsed,
                512,
                DecodeLimits::tier1(),
                clock().as_ref(),
                &loaded.path,
            )
            .map(|_| ())
        });
        if let Err(error) = outcome {
            assert!(
                error.code.0.starts_with("AURA-RAW-"),
                "truncation produced the wrong domain: {}",
                error.code
            );
        }
    }
}

#[test]
fn a_file_that_is_not_a_photograph_is_refused_by_code() {
    let path = Path::new("not-a-photo.cr2");
    let error = meta::read(b"this is a text file pretending to be a raw", path)
        .expect_err("a text file is not a raw");
    assert_eq!(error.code.0, "AURA-RAW-2001");
    assert_eq!(error.recovery, aura_core::Recovery::Quarantine);
    assert!(
        !error.user_message.contains("not-a-photo"),
        "a photographer-facing message must not leak a path"
    );
}

#[test]
fn a_decode_that_overruns_its_deadline_is_reported_as_a_timeout() {
    let limits = DecodeLimits {
        timeout_ms: 20,
        ..DecodeLimits::tier1()
    };
    let error = aura_raw::timeout::guarded("tier1", limits, || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        Ok(())
    })
    .expect_err("the watchdog fires");
    assert_eq!(error.code.0, "AURA-RAW-2004");
}

#[test]
fn an_absurd_declared_size_is_refused_before_anything_is_allocated() {
    let error = aura_raw::timeout::check_dimensions(60_000, 60_000, 12, DecodeLimits::tier2())
        .expect_err("the ceiling holds");
    assert_eq!(error.code.0, "AURA-RAW-2005");
}
