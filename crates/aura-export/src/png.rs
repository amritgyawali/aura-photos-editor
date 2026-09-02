//! The PNG writer: eight or sixteen bit, with the colour space described by chunks.
//!
//! ## PNG carries chromaticities rather than a profile, and this is stated rather than hidden
//!
//! An ICC profile lives in a PNG's `iCCP` chunk, and the encoder this workspace uses does not emit
//! one. What it does emit is `sRGB` for the sRGB case and `cHRM` plus `gAMA` for the other two,
//! which is an **exact** description of the space: the primaries, the white point and the transfer
//! exponent are what a matrix/TRC profile contains.
//!
//! It is not, however, the same thing, and two readers differ: an application that reads `iCCP` and
//! ignores `cHRM` will treat an Adobe RGB PNG as sRGB and show it desaturated. So a non-sRGB PNG
//! carries [`aura_core::DeliveryCode::IccUnavailable`], with the detail saying what it carries
//! instead.
//!
//! Phase 24's rule, in a small place: an absent input is ignorance, not permission. "The profile
//! was embedded" and "the space is described exactly but by a different mechanism" are different
//! facts, and a panel that rendered them the same would be telling a photographer their Adobe RGB
//! PNG is tagged when a print lab's software may disagree.
//!
//! ## Why PNG is here at all
//!
//! Section 2.1 lists JPEG, TIFF and PNG. In practice PNG is what a designer asks for when a couple's
//! photograph is going into an invitation or a website mock-up, and what a photographer reaches for
//! when they want lossless without a 270 MB file.

use aura_core::contract::delivery::{DeliveryCode, DeliveryColour, DeliveryReason, MetadataPolicy};
use aura_core::AuraResult;

use crate::errors::job_refused;
use crate::metadata;
use crate::read::{Rendered, Samples};

/// Encode one frame as a PNG.
///
/// # Errors
///
/// `AURA-RENDER-8021` when the buffer's length disagrees with its dimensions, or when the encoder
/// refuses the stream.
pub fn encode(
    image: &Rendered,
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

    let mut out = Vec::new();
    let mut reasons = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, image.width, image.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(if image.data.bit_depth() == 16 {
            png::BitDepth::Sixteen
        } else {
            png::BitDepth::Eight
        });
        // Deterministic output: a fixed compression level and a fixed filter, so two identical
        // exports produce identical bytes. Invariant 4 at the file level, and the reason a
        // delivery manifest's digest is worth comparing after a re-export.
        enc.set_compression(png::Compression::Default);
        enc.set_filter(png::FilterType::Sub);
        enc.set_adaptive_filter(png::AdaptiveFilterType::NonAdaptive);

        match image.colour {
            DeliveryColour::Srgb => {
                enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
                reasons.push(DeliveryReason::with(
                    DeliveryCode::IccEmbedded,
                    "srgb chunk".to_owned(),
                ));
            }
            other => {
                let c = chromaticities(other);
                enc.set_source_chromaticities(png::SourceChromaticities {
                    white: (
                        png::ScaledFloat::new(c.white.0),
                        png::ScaledFloat::new(c.white.1),
                    ),
                    red: (
                        png::ScaledFloat::new(c.red.0),
                        png::ScaledFloat::new(c.red.1),
                    ),
                    green: (
                        png::ScaledFloat::new(c.green.0),
                        png::ScaledFloat::new(c.green.1),
                    ),
                    blue: (
                        png::ScaledFloat::new(c.blue.0),
                        png::ScaledFloat::new(c.blue.1),
                    ),
                });
                enc.set_source_gamma(png::ScaledFloat::new(1.0 / c.gamma));
                reasons.push(DeliveryReason::with(
                    DeliveryCode::IccUnavailable,
                    format!("{} described by chromaticities", other.as_str()),
                ));
            }
        }

        // The XMP packet goes in an `iTXt` chunk under the keyword the specification reserves,
        // which is what Bridge, Lightroom and every gallery service look for.
        let xmp = metadata::xmp_packet(policy);
        enc.add_itxt_chunk("XML:com.adobe.xmp".to_owned(), xmp)
            .map_err(|e| job_refused(format!("png xmp chunk refused: {e}")))?;
        if let Some(c) = policy.copyright.as_ref().filter(|s| !s.trim().is_empty()) {
            let line = match policy.contact.as_ref().filter(|s| !s.trim().is_empty()) {
                Some(contact) => format!("{c} - {contact}"),
                None => c.clone(),
            };
            enc.add_text_chunk("Copyright".to_owned(), line)
                .map_err(|e| job_refused(format!("png text chunk refused: {e}")))?;
        }
        enc.add_text_chunk("Software".to_owned(), format!("AURA {}", crate::ENGINE))
            .map_err(|e| job_refused(format!("png text chunk refused: {e}")))?;

        let mut writer = enc
            .write_header()
            .map_err(|e| job_refused(format!("png header refused: {e}")))?;

        // PNG is big endian for sixteen-bit samples, which is the opposite of TIFF and is the
        // single most common way a sixteen-bit PNG writer produces a file that decodes to noise.
        let bytes: Vec<u8> = match &image.data {
            Samples::Eight(v) => v.clone(),
            Samples::Sixteen(v) => v.iter().flat_map(|s| s.to_be_bytes()).collect(),
        };
        writer
            .write_image_data(&bytes)
            .map_err(|e| job_refused(format!("png encode failed: {e}")))?;
        writer
            .finish()
            .map_err(|e| job_refused(format!("png finish failed: {e}")))?;
    }

    // The metadata policy's stripping notes apply to a PNG exactly as they do to a JPEG: there is
    // no code path here that writes a location, so both notes are true by construction.
    let exif = metadata::build(policy, image.width, image.height, true);
    reasons.extend(exif.reasons);

    Ok((out, reasons))
}

/// The four chromaticity pairs and the transfer exponent a `cHRM` plus `gAMA` pair carries.
///
/// Named rather than a tuple, because five anonymous pairs at a call site is exactly the shape in
/// which somebody swaps green and blue.
#[derive(Debug, Clone, Copy)]
struct Chromaticities {
    red: (f32, f32),
    green: (f32, f32),
    blue: (f32, f32),
    white: (f32, f32),
    gamma: f32,
}

/// The published chromaticities and transfer exponent for a space.
///
/// Display P3 uses the sRGB transfer, which is not a pure power law; 2.2 is the conventional
/// approximation a `gAMA` chunk carries, and the discrepancy is below one eight-bit code value
/// above about 4 % luminance. That is stated here rather than hidden, and it is the reason
/// `IccUnavailable` is the honest note on a P3 PNG rather than `IccEmbedded`.
fn chromaticities(space: DeliveryColour) -> Chromaticities {
    match space {
        DeliveryColour::AdobeRgb => Chromaticities {
            red: (0.6400, 0.3300),
            green: (0.2100, 0.7100),
            blue: (0.1500, 0.0600),
            white: (0.3127, 0.3290),
            gamma: 563.0 / 256.0,
        },
        DeliveryColour::DisplayP3 => Chromaticities {
            red: (0.6800, 0.3200),
            green: (0.2650, 0.6900),
            blue: (0.1500, 0.0600),
            white: (0.3127, 0.3290),
            gamma: 2.2,
        },
        DeliveryColour::Srgb => Chromaticities {
            red: (0.6400, 0.3300),
            green: (0.3000, 0.6000),
            blue: (0.1500, 0.0600),
            white: (0.3127, 0.3290),
            gamma: 2.2,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate(w: u32, h: u32, bits: u8, colour: DeliveryColour) -> Rendered {
        let n = (w * h * 3) as usize;
        let data = if bits == 16 {
            Samples::Sixteen((0..n).map(|i| (i as u16).wrapping_mul(1013)).collect())
        } else {
            Samples::Eight((0..n).map(|i| i as u8).collect())
        };
        Rendered {
            width: w,
            height: h,
            data,
            colour,
            render_hash: "0".repeat(64),
        }
    }

    fn chunks(bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = 8; // signature
        while at + 8 <= bytes.len() {
            let len = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
                as usize;
            let name = String::from_utf8_lossy(&bytes[at + 4..at + 8]).to_string();
            out.push(name.clone());
            at += 12 + len;
            if name == "IEND" {
                break;
            }
        }
        out
    }

    #[test]
    fn a_written_png_is_a_png_with_the_chunks_a_delivery_needs() {
        let (bytes, _) = encode(
            &plate(16, 16, 8, DeliveryColour::Srgb),
            &MetadataPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
        let names = chunks(&bytes);
        assert!(names.contains(&"IHDR".to_owned()));
        assert!(names.contains(&"sRGB".to_owned()));
        assert!(names.contains(&"iTXt".to_owned()), "no xmp: {names:?}");
        assert!(names.contains(&"IEND".to_owned()));
    }

    #[test]
    fn a_non_srgb_png_says_it_carries_chromaticities_rather_than_a_profile() {
        // Phase 24's rule in a small place: "the profile was embedded" and "the space is described
        // exactly by a different mechanism" are different facts.
        let (bytes, reasons) = encode(
            &plate(8, 8, 8, DeliveryColour::AdobeRgb),
            &MetadataPolicy::default(),
        )
        .unwrap();
        let names = chunks(&bytes);
        assert!(names.contains(&"cHRM".to_owned()), "{names:?}");
        assert!(names.contains(&"gAMA".to_owned()), "{names:?}");
        assert!(!names.contains(&"sRGB".to_owned()));
        assert!(reasons
            .iter()
            .any(|r| r.code == DeliveryCode::IccUnavailable));
        assert!(!reasons.iter().any(|r| r.code == DeliveryCode::IccEmbedded));
    }

    #[test]
    fn sixteen_bit_samples_are_big_endian_which_is_the_opposite_of_tiff() {
        // The single most common way a sixteen-bit PNG writer produces a file that decodes to
        // noise. Decoding it back is the only test that catches it.
        let src = plate(4, 4, 16, DeliveryColour::Srgb);
        let (bytes, _) = encode(&src, &MetadataPolicy::default()).unwrap();
        let decoder = png::Decoder::new(&bytes[..]);
        let mut reader = decoder.read_info().expect("read info");
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("frame");
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        let Samples::Sixteen(expected) = &src.data else {
            panic!()
        };
        let first = u16::from_be_bytes([buf[0], buf[1]]);
        assert_eq!(first, expected[0]);
    }

    #[test]
    fn two_encodes_of_one_frame_produce_identical_bytes() {
        let src = plate(20, 20, 8, DeliveryColour::Srgb);
        let a = encode(&src, &MetadataPolicy::default()).unwrap().0;
        let b = encode(&src, &MetadataPolicy::default()).unwrap().0;
        assert_eq!(a, b);
    }

    #[test]
    fn a_malformed_buffer_is_refused() {
        let bad = Rendered {
            width: 4,
            height: 4,
            data: Samples::Eight(vec![0; 3]),
            colour: DeliveryColour::Srgb,
            render_hash: "0".repeat(64),
        };
        assert!(encode(&bad, &MetadataPolicy::default()).is_err());
    }
}
