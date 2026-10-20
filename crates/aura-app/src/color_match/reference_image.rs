//! Bounded reference decoding, EXIF orientation and ICC conversion for analysis only.
use std::io::{Cursor, Read};
use std::path::Path;

use aura_core::AuraResult;
use aura_raw::codec::Rgb8;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};

use super::invalid;

const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PIXELS: u64 = 64_000_000;

#[derive(Debug)]
pub(super) struct Decoded {
    pub image: Rgb8,
    pub hash: String,
    pub width: u32,
    pub height: u32,
    pub converted_icc: bool,
}

pub(super) fn decode(path: &Path) -> AuraResult<Decoded> {
    let file =
        std::fs::File::open(path).map_err(|_| invalid("Cannot open the reference image."))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("Cannot read the reference image."))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("Choose a reference smaller than 256 MB."));
    }
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| invalid("Cannot identify the reference format."))?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Tiff)
    ) {
        return Err(invalid("Choose a JPEG, PNG or TIFF reference image."));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut codec = reader
        .into_decoder()
        .map_err(|e| invalid(format!("Reference decode: {e}")))?;
    let (width, height) = codec.dimensions();
    if width < 8 || height < 8 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(invalid(
            "Reference dimensions must be at least 8 pixels and at most 64 megapixels.",
        ));
    }
    let icc = codec
        .icc_profile()
        .map_err(|_| invalid("The reference ICC profile could not be read."))?;
    if icc.as_ref().is_some_and(|p| p.len() > 4 * 1024 * 1024) {
        return Err(invalid(
            "The reference ICC profile exceeds the analysis limit.",
        ));
    }
    let orientation = codec
        .orientation()
        .map_err(|_| invalid("Cannot read reference orientation."))?;
    let mut decoded =
        DynamicImage::from_decoder(codec).map_err(|e| invalid(format!("Reference decode: {e}")))?;
    drop(bytes);
    decoded.apply_orientation(orientation);
    let (width, height) = (decoded.width(), decoded.height());
    let reduced = if decoded.width() > 512 || decoded.height() > 512 {
        let reduced = decoded.resize(512, 512, image::imageops::FilterType::Triangle);
        drop(decoded);
        reduced
    } else {
        decoded
    };
    if reduced.color().has_alpha()
        && reduced
            .to_rgba16()
            .pixels()
            .any(|p| p.0.last() != Some(&u16::MAX))
    {
        return Err(invalid(
            "Use an opaque edited photo as the colour reference.",
        ));
    }
    let converted_icc = icc.is_some();
    let rgb = to_srgb(&reduced, icc)?;
    Ok(Decoded {
        image: Rgb8 {
            width: reduced.width(),
            height: reduced.height(),
            data: rgb
                .into_iter()
                .map(|v| ((u32::from(v) + 128) / 257) as u8)
                .collect(),
        },
        hash,
        width,
        height,
        converted_icc,
    })
}

fn to_srgb(reduced: &DynamicImage, icc: Option<Vec<u8>>) -> AuraResult<Vec<u16>> {
    let mut rgb = reduced.to_rgb16().into_raw();
    if let Some(profile_bytes) = icc {
        let profile = ColorProfile::new_from_slice(&profile_bytes)
            .map_err(|e| invalid(format!("Unsupported reference ICC profile: {e}")))?;
        let (samples, layout) =
            match profile.color_space {
                DataColorSpace::Rgb => (rgb, Layout::Rgb),
                DataColorSpace::Gray => (reduced.to_luma16().into_raw(), Layout::Gray),
                _ => return Err(invalid(
                    "Use an RGB or grayscale reference; CMYK/Lab image decoding is not supported.",
                )),
            };
        let transform = profile
            .create_transform_16bit(
                layout,
                &ColorProfile::new_srgb(),
                Layout::Rgb,
                TransformOptions::default(),
            )
            .map_err(|e| invalid(format!("Reference colour conversion: {e}")))?;
        rgb = vec![0; reduced.width() as usize * reduced.height() as usize * 3];
        transform
            .transform(&samples, &mut rgb)
            .map_err(|e| invalid(format!("Reference colour conversion: {e}")))?;
    }
    Ok(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    #[test]
    fn sixteen_bit_tiff_and_png_decode_without_matching_target_dimensions() {
        let directory = tempfile::tempdir().unwrap();
        let pixels = image::ImageBuffer::from_fn(24, 16, |x, y| {
            image::Rgb([(x * 1500 + 1000) as u16, (y * 2000 + 2000) as u16, 24000u16])
        });
        for extension in ["tiff", "png"] {
            let path = directory.path().join(format!("reference.{extension}"));
            pixels.save(&path).unwrap();
            let decoded = decode(&path).unwrap();
            assert_eq!((decoded.width, decoded.height), (24, 16));
            assert_eq!(decoded.image.data.len(), 24 * 16 * 3);
            assert!(!decoded.converted_icc);
        }
    }

    #[test]
    fn embedded_srgb_profile_is_converted_and_keeps_colours_close() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profiled.png");
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(file);
        encoder
            .set_icc_profile(aura_export::icc::profile_for(
                aura_core::contract::delivery::DeliveryColour::Srgb,
            ))
            .unwrap();
        let pixels = [95, 140, 180].repeat(12 * 10);
        encoder
            .write_image(&pixels, 12, 10, image::ExtendedColorType::Rgb8)
            .unwrap();
        let decoded = decode(&path).unwrap();
        assert!(decoded.converted_icc);
        for (a, b) in pixels.iter().zip(&decoded.image.data) {
            assert!(a.abs_diff(*b) <= 2);
        }
    }

    #[test]
    fn transparency_and_unsupported_input_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cutout.png");
        image::RgbaImage::from_pixel(16, 16, image::Rgba([100, 90, 80, 0]))
            .save(&path)
            .unwrap();
        assert!(decode(&path).unwrap_err().user_message.contains("opaque"));
        std::fs::write(&path, b"not an image").unwrap();
        assert!(decode(&path).is_err());
    }
}
