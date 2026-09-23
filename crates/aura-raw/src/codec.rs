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

/// Decode PNG to the editor's sRGB8 surface. Alpha is composited on white;
/// 16-bit samples are reduced to 8-bit. Original files remain untouched.
///
/// # Errors
/// Returns a typed decode error for corrupt data or excessive dimensions.
pub fn decode_png(bytes: &[u8], limits: DecodeLimits) -> AuraResult<Rgb8> {
    let mut decoder = png::Decoder::new_with_limits(
        std::io::Cursor::new(bytes),
        png::Limits {
            bytes: usize::try_from(limits.max_alloc_bytes).unwrap_or(usize::MAX),
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| corrupt(format!("PNG header: {e}")))?;
    let info = reader.info();
    check_dimensions(info.width, info.height, 8, limits)?;
    let mut decoded = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut decoded)
        .map_err(|e| corrupt(format!("PNG pixels: {e}")))?;
    let samples = frame.color_type.samples();
    let mut pixels = Vec::with_capacity(frame.width as usize * frame.height as usize * 3);
    for pixel in decoded
        .get(..frame.buffer_size())
        .unwrap_or_default()
        .chunks_exact(samples)
    {
        let (rgb, alpha) = match pixel {
            [gray] => ([*gray; 3], 255),
            [gray, alpha] => ([*gray; 3], *alpha),
            [r, g, b] => ([*r, *g, *b], 255),
            [r, g, b, alpha] => ([*r, *g, *b], *alpha),
            _ => return Err(corrupt("Unsupported PNG channel layout")),
        };
        for value in rgb {
            // Composite in linear light so translucent edges retain their color.
            let a = f32::from(alpha) / 255.0;
            let linear = crate::colour::curve::srgb_decode(f32::from(value) / 255.0);
            pixels.push(crate::colour::curve::quantise_u8(
                crate::colour::curve::srgb_encode(linear * a + 1.0 - a),
            ));
        }
    }
    Ok(Rgb8 {
        width: frame.width,
        height: frame.height,
        data: pixels,
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
