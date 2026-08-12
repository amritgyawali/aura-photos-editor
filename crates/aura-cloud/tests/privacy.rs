//! Acceptance criterion 5, and the key-safety test from section 10.1.
//!
//! "No RAW or full-resolution pixels ever leave the machine; verified by an
//! automated payload test." This is that test, and it is written the way a
//! security test should be: it inspects the bytes that would actually be
//! uploaded, rather than trusting that the builder was called correctly.

mod support;

use aura_cloud::contract::cloud::CloudTask;
use aura_cloud::payload::{self, PayloadPolicy, SourceImage};
use aura_cloud::redact::{self, ExifSummary, Region};
use aura_cloud::tasks::SegmentNaming;
use aura_cloud::{CallContext, CloudPolicy};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use support::Harness;

/// Container magic that must never appear in an uploaded payload.
///
/// The first four bytes of the RAW containers phase 02 decodes, plus the DNG and
/// CR3 brands. A payload containing any of them is a payload carrying an original.
const RAW_MAGIC: &[(&str, &[u8])] = &[
    ("TIFF little-endian (CR2, NEF, ARW, DNG, ORF)", b"II*\0"),
    ("TIFF big-endian", b"MM\0*"),
    ("Fujifilm RAF", b"FUJIFILMCCD-RAW"),
    ("Canon CR3 / ISO BMFF", b"ftypcrx "),
    ("Panasonic RW2", b"IIU\0"),
];

#[test]
fn a_built_payload_contains_no_raw_container_magic() {
    let frames: Vec<PixelBuffer> = (0..12).map(|i| support::frame(640, 480, i as u8)).collect();
    let sources: Vec<SourceImage<'_>> = frames.iter().map(SourceImage::new).collect();
    let sheet = payload::contact_sheet(&sources, PayloadPolicy::default()).expect("builds");

    assert_eq!(sheet.media_type, "image/jpeg");
    assert_eq!(
        sheet.bytes.get(..2),
        Some(&[0xFF, 0xD8][..]),
        "an uploaded payload is a JPEG and nothing else"
    );
    for (name, magic) in RAW_MAGIC {
        assert!(
            !contains(&sheet.bytes, magic),
            "the payload carries {name} magic"
        );
    }
}

#[test]
fn a_full_resolution_tiled_decode_is_refused() {
    let tiled = PixelBuffer {
        width: 8192,
        height: 5464,
        data: PixelData::Tiled(aura_raw::contract::pixels::TiledImage {
            tile_size: 512,
            halo: 32,
            tiles: Vec::new(),
        }),
        colour_space: ColourSpace::LinearRec2020,
        source: PixelSource::Demosaiced,
        decode_ms: 900,
    };
    let err = payload::crop(&SourceImage::new(&tiled), PayloadPolicy::default())
        .expect_err("tier 3 pixels must never be uploadable");
    assert_eq!(err.code.0, "AURA-CLOUD-6009");
    assert!(err.detail.contains("full-resolution"));
}

#[test]
fn scene_linear_pixels_are_refused_rather_than_converted_here() {
    let linear = PixelBuffer {
        width: 64,
        height: 64,
        data: PixelData::Linear16(vec![0u16; 64 * 64 * 3]),
        colour_space: ColourSpace::LinearRec2020,
        source: PixelSource::Demosaiced,
        decode_ms: 12,
    };
    let err = payload::crop(&SourceImage::new(&linear), PayloadPolicy::default())
        .expect_err("colour conversion does not happen in the payload builder");
    assert_eq!(err.code.0, "AURA-CLOUD-6009");
}

#[test]
fn nothing_larger_than_the_crop_limit_is_emitted() {
    let big = support::frame(6000, 4000, 3);
    let part = payload::crop(&SourceImage::new(&big), PayloadPolicy::default()).expect("builds");
    assert!(
        part.width.max(part.height) <= payload::MAX_LONG_EDGE,
        "{}x{} is over the 768 px limit",
        part.width,
        part.height
    );
}

#[test]
fn a_sheet_is_reduced_to_the_sheet_ceiling() {
    let frames: Vec<PixelBuffer> = (0..12).map(|i| support::frame(768, 768, i as u8)).collect();
    let sources: Vec<SourceImage<'_>> = frames.iter().map(SourceImage::new).collect();
    let sheet = payload::contact_sheet(&sources, PayloadPolicy::default()).expect("builds");
    assert!(sheet.width.max(sheet.height) <= payload::MAX_SHEET_LONG_EDGE);
}

#[test]
fn more_tiles_than_the_policy_allows_is_a_refusal() {
    let frames: Vec<PixelBuffer> = (0..13).map(|i| support::frame(320, 240, i as u8)).collect();
    let sources: Vec<SourceImage<'_>> = frames.iter().map(SourceImage::new).collect();
    let err = payload::contact_sheet(&sources, PayloadPolicy::default())
        .expect_err("thirteen tiles is over the limit");
    assert_eq!(err.code.0, "AURA-CLOUD-6009");
}

#[test]
fn blur_is_required_and_refuses_when_there_is_nothing_to_blur() {
    let frame = support::frame(512, 512, 4);
    let err = payload::crop(
        &SourceImage::new(&frame),
        PayloadPolicy::default().blurred(),
    )
    .expect_err("a project that requires blur and has no faces must not upload");
    assert_eq!(err.code.0, "AURA-CLOUD-6009");
    assert!(err.detail.contains("blurred"));
}

#[test]
fn blurring_actually_changes_the_pixels_it_covers() {
    // A frame with a hard edge inside the region: after blurring, the variance
    // across the region must collapse. A blur that did nothing would leave it.
    let mut pixels = vec![0u8; 128 * 128 * 3];
    for y in 0..128usize {
        for x in 0..128usize {
            let value = if x < 64 { 0u8 } else { 255u8 };
            for channel in 0..3 {
                if let Some(slot) = pixels.get_mut((y * 128 + x) * 3 + channel) {
                    *slot = value;
                }
            }
        }
    }
    // The metric is the largest step between two neighbouring pixels inside the
    // rectangle. A hard edge is 255; anything a face could be recognised from
    // still has sharp steps; a blurred patch is a gentle ramp.
    let before = max_step_in(&pixels, 128, 32, 64);
    assert_eq!(before, 255, "the fixture should start with a hard edge");

    let regions = [Region {
        x: 16,
        y: 16,
        width: 96,
        height: 96,
    }];
    let blurred = redact::blur_regions(&mut pixels, 128, 128, &regions);
    assert_eq!(blurred, 1);

    let after = max_step_in(&pixels, 128, 32, 64);
    assert!(
        after < 40,
        "the blur left a recoverable edge: step {before} -> {after}"
    );

    // And the pixels outside the padded rectangle are untouched, so the blur
    // cannot be undone by differencing the patch against its surroundings.
    let corner = pixels.first().copied().unwrap_or(1);
    assert_eq!(corner, 0, "the blur reached outside its region");
}

#[test]
fn the_exif_summary_is_an_allow_list_with_no_gps_and_no_filename() {
    let summary = ExifSummary {
        camera: Some("Nikon Z8".to_string()),
        lens: Some("Z 85mm f/1.2 S".to_string()),
        focal_mm: Some(85),
        aperture_x100: Some(120),
        shutter_denominator: Some(400),
        iso: Some(1600),
        flash: Some(true),
        offset_seconds: Some(90),
        orientation: Some(1),
        ..ExifSummary::default()
    };
    let rendered = summary.render();

    assert!(rendered.contains("Nikon Z8"));
    assert!(rendered.contains("ISO 1600"));
    assert!(rendered.contains("+90s"), "the offset is relative");

    // The type has no field for any of these, so the rendering cannot carry them.
    // A forward slash is not in the list because `f/1.2` and `1/400s` both need
    // one; what is checked is that nothing looks like a path or a date.
    for forbidden in [
        "GPS", "gps", "latitude", ".NEF", ".CR2", "C:\\", "Sarah", "2026-", "T00:", "Serial",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "the summary leaked {forbidden}"
        );
    }
}

#[test]
fn a_key_shaped_string_is_scrubbed_out_of_anything_loggable() {
    let cases = [
        "invalid x-api-key: sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "Authorization: Bearer sk-proj-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
        "key=AIzaSyD-CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
        "token gsk_DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD",
    ];
    for case in cases {
        let scrubbed = redact::scrub(case);
        assert!(
            scrubbed.contains(redact::REDACTED),
            "nothing was redacted in {case:?}"
        );
        assert!(redact::looks_like_a_key(case));
    }
}

#[test]
fn a_content_hash_is_not_mistaken_for_a_key() {
    // Every cache key, prompt hash and content hash in the product is 64 lower
    // case hexadecimal characters, and a support engineer needs to be able to
    // read them out of a log.
    let digest = blake3::hash(b"a photograph").to_hex().to_string();
    assert_eq!(digest.len(), 64);
    assert!(!redact::looks_like_a_key(&digest), "{digest} was redacted");
    assert_eq!(redact::scrub(&digest), digest);
}

#[test]
fn a_rejected_key_never_reaches_the_audit_trail() {
    // Cassette 070 answers 401 with a key-shaped string in the body, which is a
    // real provider behaviour. Nothing that string touches may keep it.
    let harness = Harness::new();
    let task = harness.task(6);
    let input = support::segment("seg-rejected", "portraits", 400, 6);
    drop(harness.gateway.run(
        &task,
        &input,
        &CallContext {
            project: &harness.project,
            decision_ref: None,
            cancel: &harness.cancel,
        },
    ));

    for row in harness.audit.rows() {
        let rendered = format!("{row:?}");
        assert!(
            !redact::looks_like_a_key(&rendered),
            "an audit row carries something key-shaped: {rendered}"
        );
    }
    if let Some(reason) = harness.gateway.client().breaker().reason() {
        assert!(
            !redact::looks_like_a_key(&reason),
            "the breaker reason carries something key-shaped: {reason}"
        );
    }
}

#[test]
fn the_stored_key_never_appears_in_a_debug_rendering() {
    let harness = Harness::new();
    let secret = "sk-ant-api03-TESTKEY000000000000000000000000000000000AA";

    // The gateway holds the store, the store holds the key, and `{:?}` on the
    // whole gateway is what a panic handler or a crash reporter would print.
    let rendered = format!("{:?}", harness.gateway);
    assert!(!rendered.contains(secret));
    assert!(!redact::looks_like_a_key(&rendered));
}

#[test]
fn a_prompt_carrying_something_key_shaped_is_refused() {
    // Nothing in this crate builds such a prompt, which is exactly why the guard
    // is asserted: a later phase that interpolated a config blob into a prompt
    // must be stopped at the gateway rather than at review.
    let harness = Harness::builder().policy(CloudPolicy::enabled()).build();
    let task = SegmentNaming::for_sheet(support::sheet(4));
    let mut input = support::segment("seg-mehndi", "candid", 410, 4);
    input.segment_id = "seg-sk-ant-api03-LEAKED000000000000000000000000000000000AA".to_string();

    let result = harness
        .gateway
        .run(
            &task,
            &input,
            &CallContext {
                project: &harness.project,
                decision_ref: None,
                cancel: &harness.cancel,
            },
        )
        .expect("refusing still completes");

    assert_eq!(result.source, aura_cloud::Source::LocalFallback);
    assert!(
        harness.transport.seen().is_empty(),
        "nothing was sent to the provider"
    );
    let rows = harness.audit.rows();
    assert_eq!(
        rows.first().and_then(|row| row.fallback_reason.as_deref()),
        Some("payload_refused")
    );
    assert_eq!(SegmentNaming::NAME, "segment_naming");
}

/// True when `haystack` contains `needle`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The largest step between horizontally neighbouring pixels in one square.
fn max_step_in(pixels: &[u8], stride_px: usize, origin: usize, size: usize) -> i32 {
    let mut worst = 0i32;
    for y in origin..origin + size {
        for x in origin..origin + size - 1 {
            for channel in 0..3 {
                let left = pixels.get((y * stride_px + x) * 3 + channel).copied();
                let right = pixels.get((y * stride_px + x + 1) * 3 + channel).copied();
                if let (Some(left), Some(right)) = (left, right) {
                    worst = worst.max((i32::from(right) - i32::from(left)).abs());
                }
            }
        }
    }
    worst
}
