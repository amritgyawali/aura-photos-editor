//! The TIFF writer: baseline, uncompressed, eight or sixteen bit, with an ICC profile.
//!
//! Written by hand, which is the fourth format this repository writes by hand after phase 02's
//! three RAW codecs. The reason is the same shape: there is no pure-Rust baseline TIFF *writer* in
//! the licence allow list, the writer half of the format is a header and an IFD, and a delivery
//! needs the ICC tag - which most of the writers that do exist do not emit.
//!
//! ## Uncompressed, deliberately
//!
//! A TIFF in a delivery is going to a print lab or a retoucher. LZW would save perhaps a fifth on a
//! photograph, Deflate a third, and both cost the receiving software a decompression path that
//! several print labs' intake systems still get wrong on sixteen-bit data. A 45 MP sixteen-bit
//! frame is 270 MB uncompressed, which is large and is a number a photographer choosing TIFF has
//! already accepted.
//!
//! ## Strips, not one big block
//!
//! Rows are written in strips of about 8 MB. A single-strip TIFF is legal and is what the naive
//! writer produces; it is also what makes several readers allocate the whole frame before they can
//! show the first row, and one common intake tool refuses a strip above 2 GB outright. Strips cost
//! one extra IFD entry and two offset arrays.

use aura_core::contract::delivery::{DeliveryCode, DeliveryReason, MetadataPolicy};
use aura_core::AuraResult;

use crate::errors::job_refused;
use crate::icc;
use crate::metadata;
use crate::read::{Rendered, Samples};

/// Roughly how many bytes of image data go in one strip.
const STRIP_TARGET: usize = 8 * 1024 * 1024;

/// TIFF tag numbers, in the order the IFD must carry them.
mod tag {
    pub(super) const IMAGE_WIDTH: u16 = 0x0100;
    pub(super) const IMAGE_LENGTH: u16 = 0x0101;
    pub(super) const BITS_PER_SAMPLE: u16 = 0x0102;
    pub(super) const COMPRESSION: u16 = 0x0103;
    pub(super) const PHOTOMETRIC: u16 = 0x0106;
    pub(super) const STRIP_OFFSETS: u16 = 0x0111;
    pub(super) const SAMPLES_PER_PIXEL: u16 = 0x0115;
    pub(super) const ROWS_PER_STRIP: u16 = 0x0116;
    pub(super) const STRIP_BYTE_COUNTS: u16 = 0x0117;
    pub(super) const PLANAR_CONFIG: u16 = 0x011C;
    pub(super) const SOFTWARE: u16 = 0x0131;
    pub(super) const ARTIST: u16 = 0x013B;
    pub(super) const COPYRIGHT: u16 = 0x8298;
    pub(super) const XMP: u16 = 0x02BC;
    pub(super) const ICC_PROFILE: u16 = 0x8773;
}

const TYPE_BYTE: u16 = 1;
const TYPE_ASCII: u16 = 2;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;
const TYPE_UNDEFINED: u16 = 7;

/// One IFD entry plus the bytes it needs on the heap.
struct Field {
    tag: u16,
    kind: u16,
    count: u32,
    /// Inline value when the payload fits in four bytes; otherwise the heap bytes.
    payload: Payload,
}

enum Payload {
    Inline([u8; 4]),
    Heap(Vec<u8>),
}

/// Encode one frame as a baseline TIFF.
///
/// # Errors
///
/// `AURA-RENDER-8021` when the buffer's length disagrees with its dimensions.
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

    let bits = image.data.bit_depth();
    let bytes_per_sample = usize::from(bits / 8);
    let row_bytes = image.width as usize * 3 * bytes_per_sample;
    let rows_per_strip = (STRIP_TARGET / row_bytes.max(1))
        .max(1)
        .min(image.height as usize);
    let strips = (image.height as usize).div_ceil(rows_per_strip);

    // --- the pixel bytes, little endian, one strip at a time ---
    let mut pixels: Vec<u8> = Vec::with_capacity(row_bytes * image.height as usize);
    match &image.data {
        Samples::Eight(v) => pixels.extend_from_slice(v),
        Samples::Sixteen(v) => {
            for s in v {
                pixels.extend_from_slice(&s.to_le_bytes());
            }
        }
    }

    let mut strip_byte_counts: Vec<u32> = Vec::with_capacity(strips);
    for s in 0..strips {
        let first = s * rows_per_strip;
        let rows = rows_per_strip.min(image.height as usize - first);
        strip_byte_counts.push(u32::try_from(rows * row_bytes).unwrap_or(u32::MAX));
    }

    let mut reasons = Vec::new();
    let profile = icc::profile_for(image.colour);
    reasons.push(DeliveryReason::with(
        DeliveryCode::IccEmbedded,
        image.colour.as_str().to_owned(),
    ));
    let exif = metadata::build(policy, image.width, image.height, true);
    reasons.extend(exif.reasons);
    let xmp = metadata::xmp_packet(policy);

    // --- fields, in ascending tag order, which baseline TIFF requires ---
    let mut fields: Vec<Field> = Vec::new();
    fields.push(long_field(tag::IMAGE_WIDTH, image.width));
    fields.push(long_field(tag::IMAGE_LENGTH, image.height));
    fields.push(Field {
        tag: tag::BITS_PER_SAMPLE,
        kind: TYPE_SHORT,
        count: 3,
        // Three SHORTs is six bytes, so it goes on the heap. A writer that assumed it fit inline
        // is the most common TIFF bug there is.
        payload: Payload::Heap({
            let mut v = Vec::with_capacity(6);
            for _ in 0..3 {
                v.extend_from_slice(&u16::from(bits).to_le_bytes());
            }
            v
        }),
    });
    fields.push(short_field(tag::COMPRESSION, 1)); // none
    fields.push(short_field(tag::PHOTOMETRIC, 2)); // RGB
    fields.push(Field {
        tag: tag::STRIP_OFFSETS,
        kind: TYPE_LONG,
        count: u32::try_from(strips).unwrap_or(1),
        payload: Payload::Heap(vec![0; strips * 4]), // patched once the layout is known
    });
    fields.push(short_field(tag::SAMPLES_PER_PIXEL, 3));
    fields.push(long_field(
        tag::ROWS_PER_STRIP,
        u32::try_from(rows_per_strip).unwrap_or(1),
    ));
    fields.push(Field {
        tag: tag::STRIP_BYTE_COUNTS,
        kind: TYPE_LONG,
        count: u32::try_from(strips).unwrap_or(1),
        payload: Payload::Heap(
            strip_byte_counts
                .iter()
                .flat_map(|c| c.to_le_bytes())
                .collect(),
        ),
    });
    fields.push(short_field(tag::PLANAR_CONFIG, 1)); // chunky
    fields.push(ascii_field(
        tag::SOFTWARE,
        &format!("AURA {}", crate::ENGINE),
    ));
    if let Some(a) = policy.creator.as_ref().filter(|s| !s.trim().is_empty()) {
        fields.push(ascii_field(tag::ARTIST, a));
    }
    fields.push(Field {
        tag: tag::XMP,
        kind: TYPE_BYTE,
        count: u32::try_from(xmp.len()).unwrap_or(0),
        payload: Payload::Heap(xmp.into_bytes()),
    });
    if let Some(c) = policy.copyright.as_ref().filter(|s| !s.trim().is_empty()) {
        let line = match policy.contact.as_ref().filter(|s| !s.trim().is_empty()) {
            Some(contact) => format!("{c} - {contact}"),
            None => c.clone(),
        };
        fields.push(ascii_field(tag::COPYRIGHT, &line));
    }
    fields.push(Field {
        tag: tag::ICC_PROFILE,
        kind: TYPE_UNDEFINED,
        count: u32::try_from(profile.len()).unwrap_or(0),
        payload: Payload::Heap(profile),
    });
    fields.sort_by_key(|f| f.tag);

    // --- layout: header, IFD, heap, pixels ---
    let header = 8_usize;
    let ifd_bytes = 2 + fields.len() * 12 + 4;
    let mut heap_cursor = header + ifd_bytes;
    let mut heap_offsets: Vec<Option<usize>> = Vec::with_capacity(fields.len());
    for f in &fields {
        match &f.payload {
            Payload::Inline(_) => heap_offsets.push(None),
            Payload::Heap(bytes) => {
                heap_offsets.push(Some(heap_cursor));
                heap_cursor += bytes.len();
                // TIFF requires word alignment for heap values.
                if !heap_cursor.is_multiple_of(2) {
                    heap_cursor += 1;
                }
            }
        }
    }
    let pixels_at = heap_cursor;

    let mut strip_offsets: Vec<u32> = Vec::with_capacity(strips);
    let mut at = pixels_at;
    for count in &strip_byte_counts {
        strip_offsets.push(u32::try_from(at).unwrap_or(u32::MAX));
        at += *count as usize;
    }
    // Patch the placeholder now that the pixel block's position is known.
    for (i, f) in fields.iter_mut().enumerate() {
        if f.tag == tag::STRIP_OFFSETS {
            f.payload = Payload::Heap(strip_offsets.iter().flat_map(|o| o.to_le_bytes()).collect());
            let _ = i;
        }
    }

    let mut out = Vec::with_capacity(pixels_at + pixels.len());
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42_u16.to_le_bytes());
    out.extend_from_slice(&u32::try_from(header).unwrap_or(8).to_le_bytes());

    out.extend_from_slice(&u16::try_from(fields.len()).unwrap_or(0).to_le_bytes());
    for (i, f) in fields.iter().enumerate() {
        out.extend_from_slice(&f.tag.to_le_bytes());
        out.extend_from_slice(&f.kind.to_le_bytes());
        out.extend_from_slice(&f.count.to_le_bytes());
        match (&f.payload, heap_offsets.get(i).copied().flatten()) {
            (Payload::Inline(v), _) => out.extend_from_slice(v),
            (Payload::Heap(_), Some(off)) => {
                out.extend_from_slice(&u32::try_from(off).unwrap_or(0).to_le_bytes());
            }
            (Payload::Heap(_), None) => out.extend_from_slice(&[0; 4]),
        }
    }
    out.extend_from_slice(&0_u32.to_le_bytes()); // no next IFD

    for f in &fields {
        if let Payload::Heap(bytes) = &f.payload {
            out.extend_from_slice(bytes);
            if !out.len().is_multiple_of(2) {
                out.push(0);
            }
        }
    }
    debug_assert_eq!(out.len(), pixels_at);
    out.extend_from_slice(&pixels);

    Ok((out, reasons))
}

fn short_field(tag: u16, value: u16) -> Field {
    let mut inline = [0_u8; 4];
    let bytes = value.to_le_bytes();
    inline[0] = bytes[0];
    inline[1] = bytes[1];
    Field {
        tag,
        kind: TYPE_SHORT,
        count: 1,
        payload: Payload::Inline(inline),
    }
}

fn long_field(tag: u16, value: u32) -> Field {
    Field {
        tag,
        kind: TYPE_LONG,
        count: 1,
        payload: Payload::Inline(value.to_le_bytes()),
    }
}

fn ascii_field(tag: u16, text: &str) -> Field {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    let count = u32::try_from(bytes.len()).unwrap_or(0);
    if bytes.len() <= 4 {
        let mut inline = [0_u8; 4];
        for (i, b) in bytes.iter().enumerate() {
            if let Some(slot) = inline.get_mut(i) {
                *slot = *b;
            }
        }
        Field {
            tag,
            kind: TYPE_ASCII,
            count,
            payload: Payload::Inline(inline),
        }
    } else {
        Field {
            tag,
            kind: TYPE_ASCII,
            count,
            payload: Payload::Heap(bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::delivery::DeliveryColour;

    fn plate(w: u32, h: u32, bits: u8) -> Rendered {
        let n = (w * h * 3) as usize;
        let data = if bits == 16 {
            Samples::Sixteen((0..n).map(|i| (i as u16).wrapping_mul(257)).collect())
        } else {
            Samples::Eight((0..n).map(|i| i as u8).collect())
        };
        Rendered {
            width: w,
            height: h,
            data,
            colour: DeliveryColour::AdobeRgb,
            render_hash: "0".repeat(64),
        }
    }

    /// Walk the IFD the way a reader does, and hand back every tag with its offset and count.
    fn ifd(bytes: &[u8]) -> Vec<(u16, u16, u32, u32)> {
        assert_eq!(&bytes[..2], b"II");
        assert_eq!(u16::from_le_bytes([bytes[2], bytes[3]]), 42);
        let at = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let count = u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let e = at + 2 + i * 12;
            out.push((
                u16::from_le_bytes([bytes[e], bytes[e + 1]]),
                u16::from_le_bytes([bytes[e + 2], bytes[e + 3]]),
                u32::from_le_bytes([bytes[e + 4], bytes[e + 5], bytes[e + 6], bytes[e + 7]]),
                u32::from_le_bytes([bytes[e + 8], bytes[e + 9], bytes[e + 10], bytes[e + 11]]),
            ));
        }
        out
    }

    #[test]
    fn an_eight_bit_tiff_is_walkable_and_its_pixels_are_where_the_ifd_says() {
        let src = plate(16, 12, 8);
        let (bytes, _) = encode(&src, &MetadataPolicy::default()).unwrap();
        let entries = ifd(&bytes);

        // Tags ascend. A reader is entitled to binary-search them.
        let mut last = 0;
        for (tag, ..) in &entries {
            assert!(*tag > last, "tag {tag:#06x} out of order");
            last = *tag;
        }

        let find = |t: u16| entries.iter().find(|(tag, ..)| *tag == t).copied();
        assert_eq!(find(tag::IMAGE_WIDTH).unwrap().3, 16);
        assert_eq!(find(tag::IMAGE_LENGTH).unwrap().3, 12);
        assert_eq!(find(tag::COMPRESSION).unwrap().3 & 0xFFFF, 1);
        assert_eq!(find(tag::PHOTOMETRIC).unwrap().3 & 0xFFFF, 2);
        assert_eq!(find(tag::SAMPLES_PER_PIXEL).unwrap().3 & 0xFFFF, 3);

        // Three SHORTs of bits-per-sample, on the heap because six bytes do not fit in four.
        let (_, kind, count, off) = find(tag::BITS_PER_SAMPLE).unwrap();
        assert_eq!((kind, count), (TYPE_SHORT, 3));
        let off = off as usize;
        for i in 0..3 {
            assert_eq!(
                u16::from_le_bytes([bytes[off + i * 2], bytes[off + i * 2 + 1]]),
                8
            );
        }

        // The strip actually contains the pixels.
        let (_, _, strips, so) = find(tag::STRIP_OFFSETS).unwrap();
        assert_eq!(strips, 1);
        let pixel_at = u32::from_le_bytes([
            bytes[so as usize],
            bytes[so as usize + 1],
            bytes[so as usize + 2],
            bytes[so as usize + 3],
        ]) as usize;
        let Samples::Eight(expected) = &src.data else {
            panic!()
        };
        assert_eq!(&bytes[pixel_at..pixel_at + expected.len()], &expected[..]);
    }

    #[test]
    fn a_sixteen_bit_tiff_stores_little_endian_samples() {
        let src = plate(4, 4, 16);
        let (bytes, _) = encode(&src, &MetadataPolicy::default()).unwrap();
        let entries = ifd(&bytes);
        let find = |t: u16| entries.iter().find(|(tag, ..)| *tag == t).copied();
        let off = find(tag::BITS_PER_SAMPLE).unwrap().3 as usize;
        assert_eq!(u16::from_le_bytes([bytes[off], bytes[off + 1]]), 16);

        let so = find(tag::STRIP_OFFSETS).unwrap().3 as usize;
        let pixel_at =
            u32::from_le_bytes([bytes[so], bytes[so + 1], bytes[so + 2], bytes[so + 3]]) as usize;
        let Samples::Sixteen(expected) = &src.data else {
            panic!()
        };
        assert_eq!(
            u16::from_le_bytes([bytes[pixel_at], bytes[pixel_at + 1]]),
            expected[0]
        );
    }

    #[test]
    fn a_tall_frame_is_written_in_several_strips() {
        // A single-strip TIFF is legal and is what the naive writer produces. Several readers
        // allocate the whole frame before showing a row, and one intake tool refuses a strip
        // above 2 GB outright.
        let src = plate(2048, 2600, 16);
        let (bytes, _) = encode(&src, &MetadataPolicy::default()).unwrap();
        let entries = ifd(&bytes);
        let strips = entries
            .iter()
            .find(|(t, ..)| *t == tag::STRIP_OFFSETS)
            .unwrap()
            .2;
        assert!(strips > 1, "expected several strips, got {strips}");

        // Every strip's byte count sums to the whole image.
        let (_, _, n, off) = entries
            .iter()
            .find(|(t, ..)| *t == tag::STRIP_BYTE_COUNTS)
            .copied()
            .unwrap();
        let mut total = 0_u64;
        for i in 0..n as usize {
            let at = off as usize + i * 4;
            total += u64::from(u32::from_le_bytes([
                bytes[at],
                bytes[at + 1],
                bytes[at + 2],
                bytes[at + 3],
            ]));
        }
        assert_eq!(total, 2048 * 2600 * 3 * 2);
    }

    #[test]
    fn the_icc_profile_and_the_xmp_packet_are_both_in_the_file() {
        let policy = MetadataPolicy {
            copyright: Some("© Studio".to_owned()),
            ..MetadataPolicy::default()
        };
        let (bytes, reasons) = encode(&plate(8, 8, 8), &policy).unwrap();
        let entries = ifd(&bytes);
        let icc = entries
            .iter()
            .find(|(t, ..)| *t == tag::ICC_PROFILE)
            .unwrap();
        assert!(icc.2 > 200, "icc profile too small");
        let at = icc.3 as usize;
        assert_eq!(&bytes[at + 36..at + 40], b"acsp");
        assert!(entries.iter().any(|(t, ..)| *t == tag::XMP));
        assert!(reasons.iter().any(|r| r.code == DeliveryCode::IccEmbedded));
    }

    #[test]
    fn two_encodes_of_one_frame_produce_identical_bytes() {
        let src = plate(24, 24, 8);
        let a = encode(&src, &MetadataPolicy::default()).unwrap().0;
        let b = encode(&src, &MetadataPolicy::default()).unwrap().0;
        assert_eq!(a, b);
    }

    #[test]
    fn a_malformed_buffer_is_refused() {
        let bad = Rendered {
            width: 10,
            height: 10,
            data: Samples::Eight(vec![0; 7]),
            colour: DeliveryColour::Srgb,
            render_hash: "0".repeat(64),
        };
        assert!(encode(&bad, &MetadataPolicy::default()).is_err());
    }
}
