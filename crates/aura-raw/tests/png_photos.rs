use aura_core::clock::SystemClock;
use aura_raw::{codec, full, meta, proxy, thumb, DecodeLimits, RawFormat};

fn png_bytes(color: png::ColorType, depth: png::BitDepth, width: u32, bytes: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, 1);
        encoder.set_color(color);
        encoder.set_depth(depth);
        encoder
            .write_header()
            .expect("valid photo fixture and successful operation")
            .write_image_data(bytes)
            .expect("valid photo fixture and successful operation");
    }
    encoded
}

#[test]
fn png_reaches_thumbnail_proxy_and_full_resolution() {
    let bytes = png_bytes(
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        2,
        &[210, 40, 20, 25, 90, 180],
    );
    let path = std::path::Path::new("photo.png");
    let metadata = meta::read(&bytes, path).expect("valid photo fixture and successful operation");
    assert_eq!(metadata.format, RawFormat::Png);
    let clock = SystemClock::default();
    let small = thumb::tier1(&bytes, &metadata, 512, DecodeLimits::tier1(), &clock, path)
        .expect("valid photo fixture and successful operation");
    let screen = proxy::tier2(&bytes, &metadata, DecodeLimits::tier2(), &clock, None, path)
        .expect("valid photo fixture and successful operation");
    let export = full::tier3(&bytes, &metadata, DecodeLimits::tier3(), &clock)
        .expect("valid photo fixture and successful operation");
    assert_eq!((export.width, export.height), (2, 1));
    assert_eq!(small.buffer.as_srgb8(), export.as_srgb8());
    assert!(screen.warnings.is_empty());
    for (actual, expected) in screen
        .pair
        .srgb
        .as_srgb8()
        .expect("valid photo fixture and successful operation")
        .iter()
        .zip([210_u8, 40, 20, 25, 90, 180])
    {
        assert!(
            actual.abs_diff(expected) <= 1,
            "PNG colors must survive proxy conversion"
        );
    }
}

#[test]
fn png_handles_alpha_grayscale_and_sixteen_bit() {
    let rgba = png_bytes(
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        2,
        &[0, 0, 0, 0, 210, 40, 20, 255],
    );
    assert_eq!(
        codec::decode_png(&rgba, DecodeLimits::tier1())
            .expect("valid photo fixture and successful operation")
            .data,
        [255, 255, 255, 210, 40, 20]
    );
    let gray = png_bytes(
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
        2,
        &[0, 0, 255, 255],
    );
    assert_eq!(
        codec::decode_png(&gray, DecodeLimits::tier1())
            .expect("valid photo fixture and successful operation")
            .data,
        [0, 0, 0, 255, 255, 255]
    );
}

#[test]
fn png_rejects_corrupt_and_oversized_data() {
    assert!(codec::decode_png(b"not a PNG", DecodeLimits::tier1()).is_err());
    let bytes = png_bytes(png::ColorType::Rgb, png::BitDepth::Eight, 2, &[0; 6]);
    let limits = DecodeLimits {
        max_pixels: 1,
        ..DecodeLimits::tier1()
    };
    assert!(codec::decode_png(&bytes, limits).is_err());
    assert!(codec::decode_png(&bytes[..bytes.len() / 2], DecodeLimits::tier1()).is_err());
}
