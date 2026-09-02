//! The JPEG writer: eight-bit, baseline, with an ICC profile and an Exif block.
//!
//! ## Why the ICC profile goes in through the encoder rather than being spliced in afterwards
//!
//! An ICC profile larger than 65,533 bytes has to be split across several APP2 segments with a
//! two-byte chunk counter, and a splicer that got the counter wrong would produce a file that most
//! readers ignore the profile in and one reader rejects. `jpeg_encoder::Encoder::add_icc_profile`
//! does the chunking; the profiles this product writes are about 2 KB and would fit in one segment
//! anyway, which is exactly the situation in which somebody writes the naive version and it works
//! until the day a profile grows.
//!
//! ## Chroma subsampling is off
//!
//! The encoder's default is 4:2:0, which halves colour resolution in both directions and is
//! invisible on almost everything. What it is visible on is a red sari against a dark background
//! and a saturated bouquet - which is to say, on Indian and Nepali weddings more than on others,
//! and on exactly the frames a photographer would put in a portfolio. A delivery JPEG at quality 92
//! is not where the bytes need saving.

use aura_core::contract::delivery::{DeliveryCode, DeliveryReason, MetadataPolicy};
use aura_core::AuraResult;

use crate::errors::job_refused;
use crate::icc;
use crate::metadata;
use crate::read::{Rendered, Samples};

/// Encode one frame as a JPEG.
///
/// # Errors
///
/// `AURA-RENDER-8021` when the buffer's length disagrees with its dimensions, when the frame is
/// larger than JPEG's 65,535-pixel limit, or when the encoder refuses it.
pub fn encode(
    image: &Rendered,
    quality: u8,
    policy: &MetadataPolicy,
) -> AuraResult<(Vec<u8>, Vec<DeliveryReason>)> {
    if !image.is_well_formed() {
        return Err(job_refused(format!(
            "cannot encode {} samples as a {}x{} rgb image",
            image.data.len(),
            image.width,
            image.height
        )));
    }
    let width = u16::try_from(image.width)
        .map_err(|_| job_refused("jpeg cannot store an image wider than 65535 pixels"))?;
    let height = u16::try_from(image.height)
        .map_err(|_| job_refused("jpeg cannot store an image taller than 65535 pixels"))?;

    // JPEG is eight bit. A sixteen-bit render written as a JPEG is down-converted here rather than
    // refused, because a photographer who asked for a JPEG asked for eight bits and a job that
    // failed on the depth would be a job that failed on a setting nobody chose.
    let bytes: Vec<u8> = match &image.data {
        Samples::Eight(v) => v.clone(),
        Samples::Sixteen(v) => v.iter().map(|&s| (s >> 8) as u8).collect(),
    };

    let mut out = Vec::with_capacity(bytes.len() / 6);
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    // See the note above: full chroma resolution, deliberately.
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);

    let mut reasons = Vec::new();

    let exif = metadata::build(
        policy,
        image.width,
        image.height,
        image.colour == aura_core::contract::delivery::DeliveryColour::Srgb,
    );
    reasons.extend(exif.reasons);
    encoder
        .add_app_segment(1, &exif.app1)
        .map_err(|e| job_refused(format!("jpeg exif segment refused: {e}")))?;

    let profile = icc::profile_for(image.colour);
    encoder
        .add_icc_profile(&profile)
        .map_err(|e| job_refused(format!("jpeg icc segment refused: {e}")))?;
    reasons.push(DeliveryReason::with(
        DeliveryCode::IccEmbedded,
        image.colour.as_str().to_owned(),
    ));

    encoder
        .encode(&bytes, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| job_refused(format!("jpeg encode failed: {e}")))?;

    Ok((out, reasons))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::delivery::DeliveryColour;

    fn plate(w: u32, h: u32) -> Rendered {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[(x * 3) as u8, (y * 3) as u8, 128]);
            }
        }
        Rendered {
            width: w,
            height: h,
            data: Samples::Eight(data),
            colour: DeliveryColour::Srgb,
            render_hash: "0".repeat(64),
        }
    }

    #[test]
    fn a_written_jpeg_decodes_back_to_what_went_in() {
        // Section 10.1's first row. A test that checked the length would prove the encoder ran;
        // this proves the file is the photograph.
        let src = plate(64, 48);
        let (bytes, _) = encode(&src, 95, &MetadataPolicy::default()).unwrap();
        let mut d = zune_jpeg::JpegDecoder::new(&bytes[..]);
        let pixels = d.decode().expect("decode");
        let info = d.info().expect("info");
        assert_eq!(info.width, 64);
        assert_eq!(info.height, 48);

        let Samples::Eight(original) = &src.data else {
            panic!("eight bit")
        };
        assert_eq!(pixels.len(), original.len());
        let mut worst = 0_i32;
        for (a, b) in original.iter().zip(pixels.iter()) {
            worst = worst.max((i32::from(*a) - i32::from(*b)).abs());
        }
        // Quality 95 with no chroma subsampling. Six code values is the DCT's own rounding.
        assert!(worst <= 6, "worst channel error {worst}");
    }

    #[test]
    fn the_file_carries_an_exif_segment_and_an_icc_profile() {
        let (bytes, reasons) = encode(&plate(16, 16), 90, &MetadataPolicy::default()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Exif\0\0"), "no APP1/Exif");
        assert!(text.contains("ICC_PROFILE"), "no APP2/ICC");
        assert!(reasons.iter().any(|r| r.code == DeliveryCode::IccEmbedded));
        assert!(reasons.iter().any(|r| r.code == DeliveryCode::GpsStripped));
    }

    #[test]
    fn a_malformed_buffer_is_refused_rather_than_encoded() {
        let bad = Rendered {
            width: 10,
            height: 10,
            data: Samples::Eight(vec![0; 5]),
            colour: DeliveryColour::Srgb,
            render_hash: "0".repeat(64),
        };
        assert!(encode(&bad, 90, &MetadataPolicy::default()).is_err());
    }

    #[test]
    fn a_sixteen_bit_render_is_written_as_eight_rather_than_refused() {
        let src = Rendered {
            width: 8,
            height: 8,
            data: Samples::Sixteen(vec![40000_u16; 8 * 8 * 3]),
            colour: DeliveryColour::Srgb,
            render_hash: "0".repeat(64),
        };
        let (bytes, _) = encode(&src, 90, &MetadataPolicy::default()).unwrap();
        let mut d = zune_jpeg::JpegDecoder::new(&bytes[..]);
        let pixels = d.decode().expect("decode");
        // 40000 >> 8 is 156.
        assert!(
            pixels.iter().all(|&p| (150..=162).contains(&p)),
            "down-convert"
        );
    }

    #[test]
    fn two_encodes_of_one_frame_produce_identical_bytes() {
        // Invariant 4 at the file level. A JPEG whose bytes moved between runs would make the
        // delivery manifest's digest useless for comparing a re-export.
        let src = plate(32, 32);
        let a = encode(&src, 92, &MetadataPolicy::default()).unwrap().0;
        let b = encode(&src, 92, &MetadataPolicy::default()).unwrap().0;
        assert_eq!(a, b);
    }
}
