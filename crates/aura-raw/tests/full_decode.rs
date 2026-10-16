//! The full-size decode of a developed file returns the same pixels the thumbnail path
//! returned at an unlimited edge - it only stops making a cache JPEG nobody uses.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

use aura_core::clock::SystemClock;
use aura_raw::codec::{encode_jpeg, Rgb8};
use aura_raw::{full, meta, thumb, DecodeLimits};

#[test]
fn full_decode_matches_the_unlimited_thumbnail_pixel_for_pixel() {
    let (w, h) = (97_u32, 61_u32);
    let data: Vec<u8> = (0..w * h)
        .flat_map(|i| {
            [
                (i % 251) as u8,
                ((i * 7) % 253) as u8,
                ((i * 13) % 255) as u8,
            ]
        })
        .collect();
    let bytes = encode_jpeg(
        &Rgb8 {
            width: w,
            height: h,
            data,
        },
        92,
    )
    .unwrap();
    let path = std::path::Path::new("photo.jpg");
    let parsed = meta::read(&bytes, path).unwrap();
    let clock = SystemClock::default();
    let old = thumb::tier1(
        &bytes,
        &parsed,
        u32::MAX,
        DecodeLimits::tier3(),
        &clock,
        path,
    )
    .unwrap();
    let new = full::tier3(&bytes, &parsed, DecodeLimits::tier3(), &clock).unwrap();
    assert_eq!(
        (new.width, new.height),
        (old.buffer.width, old.buffer.height)
    );
    assert_eq!(new.data, old.buffer.data);
    assert_eq!(new.source, old.buffer.source);
}
