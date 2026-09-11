//! The only place that talks to a third-party image codec.
//!
//! Everything else in the crate works in AURA's own types. Keeping the codec
//! boundary in one small module means the error mapping is written once, the
//! decode-time ceilings are applied once, and swapping a codec later is a
//! change to this file rather than an archaeology project.

use aura_core::errors::raw::corrupt;
use aura_core::AuraResult;

use crate::timeout::{check_dimensions, DecodeLimits};

/// A decoded 8-bit image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb8 {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 3` interleaved samples.
    pub data: Vec<u8>,
}

/// Decode PNG, expanding palettes and greyscale and compositing alpha over white.
///
/// # Errors
/// Refuses corrupt images and dimensions beyond the decode allocation limits.
pub fn decode_png(bytes: &[u8], limits: DecodeLimits) -> AuraResult<Rgb8> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder.set_limits(png::Limits {
        bytes: usize::try_from(limits.max_alloc_bytes).unwrap_or(usize::MAX),
    });
    let mut reader = decoder
        .read_info()
        .map_err(|e| corrupt(format!("PNG header: {e}")))?;
    let info = reader.info();
    check_dimensions(info.width, info.height, 8, limits)?;
    let mut samples = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut samples)
        .map_err(|e| corrupt(format!("PNG pixels: {e}")))?;
    let channels = info.color_type.samples();
    let mut data = Vec::with_capacity(info.width as usize * info.height as usize * 3);
    for pixel in samples
        .get(..info.buffer_size())
        .ok_or_else(|| corrupt("truncated PNG output"))?
        .chunks_exact(channels)
    {
        let first = pixel.first().copied().unwrap_or(0);
        let (red, green, blue, alpha) = match info.color_type {
            png::ColorType::Rgb => (
                first,
                pixel.get(1).copied().unwrap_or(0),
                pixel.get(2).copied().unwrap_or(0),
                255,
            ),
            png::ColorType::Rgba => (
                first,
                pixel.get(1).copied().unwrap_or(0),
                pixel.get(2).copied().unwrap_or(0),
                pixel.get(3).copied().unwrap_or(255),
            ),
            png::ColorType::Grayscale => (first, first, first, 255),
            png::ColorType::GrayscaleAlpha => {
                (first, first, first, pixel.get(1).copied().unwrap_or(255))
            }
            png::ColorType::Indexed => return Err(corrupt("PNG palette was not expanded")),
        };
        for value in [red, green, blue] {
            data.push(
                ((u32::from(value) * u32::from(alpha) + 255 * (255 - u32::from(alpha)) + 127) / 255)
                    as u8,
            );
        }
    }
    Ok(Rgb8 {
        width: info.width,
        height: info.height,
        data,
    })
}

/// Decode a baseline or progressive JPEG into interleaved RGB.
///
/// # Errors
///
/// `AURA-RAW-2002` when the stream will not decode, `AURA-RAW-2005` when its
/// declared size is above the ceiling.
pub fn decode_jpeg(bytes: &[u8], limits: DecodeLimits) -> AuraResult<Rgb8> {
    let info = crate::container::jpeg::probe(bytes)?;
    check_dimensions(info.width, info.height, 3, limits)?;

    let mut decoder = zune_jpeg::JpegDecoder::new(bytes);
    let pixels = decoder
        .decode()
        .map_err(|e| corrupt(format!("jpeg decode failed: {e:?}")))?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| corrupt("jpeg decoder reported no dimensions"))?;
    let width = u32::try_from(width).unwrap_or(0);
    let height = u32::try_from(height).unwrap_or(0);
    if width == 0 || height == 0 {
        return Err(corrupt("jpeg decoded to a zero dimension"));
    }

    let expected = width as usize * height as usize * 3;
    let interleaved = match pixels.len() {
        // Already interleaved RGB.
        len if len == expected => pixels,
        // Greyscale: widen it so every caller sees three channels.
        len if len == width as usize * height as usize => {
            let mut widened = Vec::with_capacity(expected);
            for sample in &pixels {
                widened.extend_from_slice(&[*sample, *sample, *sample]);
            }
            widened
        }
        len => {
            return Err(corrupt(format!(
                "jpeg decoded to {len} bytes, expected {expected}"
            )))
        }
    };

    Ok(Rgb8 {
        width,
        height,
        data: interleaved,
    })
}

/// Encode interleaved RGB as a baseline JPEG.
///
/// Quality 90 is the cache format for tier 1 and tier 2: visually lossless at
/// preview sizes, and roughly a third of the bytes of quality 100.
///
/// # Errors
///
/// `AURA-RAW-2002` when the encoder rejects the buffer, which can only happen
/// if the dimensions and the data length disagree.
pub fn encode_jpeg(image: &Rgb8, quality: u8) -> AuraResult<Vec<u8>> {
    let expected = image.width as usize * image.height as usize * 3;
    if image.data.len() != expected {
        return Err(corrupt(format!(
            "cannot encode {} bytes as a {}x{} rgb image",
            image.data.len(),
            image.width,
            image.height
        )));
    }
    let width = u16::try_from(image.width)
        .map_err(|_| corrupt("jpeg cannot store an image wider than 65535 pixels"))?;
    let height = u16::try_from(image.height)
        .map_err(|_| corrupt("jpeg cannot store an image taller than 65535 pixels"))?;

    let mut out = Vec::with_capacity(expected / 8);
    let encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    encoder
        .encode(&image.data, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| corrupt(format!("jpeg encode failed: {e}")))?;
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod png_tests {
    use super::*;

    #[test]
    fn transparent_png_decodes_and_reaches_all_preview_tiers() {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG header");
            writer
                .write_image_data(&[255, 0, 0, 255, 0, 0, 255, 0])
                .expect("PNG pixels");
        }
        let image = decode_png(&encoded, DecodeLimits::tier1()).expect("decode PNG");
        assert_eq!(image.data, [255, 0, 0, 255, 255, 255]);
        let path = std::path::Path::new("photo.png");
        let meta = crate::read_meta(&encoded, path).expect("PNG metadata");
        assert_eq!(meta.format, crate::RawFormat::Png);
        let clock = aura_core::clock::SystemClock::default();
        let thumb = crate::thumb::tier1(&encoded, &meta, 512, DecodeLimits::tier1(), &clock, path)
            .expect("thumbnail");
        assert_eq!((thumb.buffer.width, thumb.buffer.height), (2, 1));
        let full = crate::full::tier3(&encoded, &meta, DecodeLimits::tier3(), &clock)
            .expect("full resolution PNG");
        assert_eq!((full.width, full.height), (2, 1));
        assert!(decode_png(
            &encoded,
            DecodeLimits {
                max_pixels: 1,
                ..DecodeLimits::tier1()
            }
        )
        .is_err());
    }
}
