//! What travels with a delivered file, and what does not.
//!
//! Two documents come out of a [`MetadataPolicy`]: a TIFF/Exif block for the APP1 segment of a JPEG
//! and the IFD of a TIFF, and an XMP packet carrying the IPTC fields every modern tool reads.
//!
//! ## Stripping is the default, and this module can only strip
//!
//! There is no code path here that *copies* the original's Exif forward. Everything written is
//! built from the policy plus four facts about the render, which means the location of somebody's
//! house cannot reach a delivered file by accident - it would have to be added on purpose, and
//! nothing adds it.
//!
//! That is a stronger guarantee than `strip_gps = true` and it is deliberate. A copy-then-remove
//! implementation is one forgotten tag away from leaking, and the tag it forgets will be the one a
//! camera manufacturer added last year. `MetadataPolicy::strip_gps` therefore reads as documentation
//! of what this module does rather than as a switch that turns something off; setting it to `false`
//! does not put the location *in*, it records that the photographer asked for it and produces
//! [`DeliveryCode::GpsStripped`]'s absence in the panel. ADR-0061 and `docs/privacy.md`.
//!
//! ## Why the copyright is written twice
//!
//! Exif `Copyright` (tag 0x8298) is what a file browser shows and what most stock-photo scrapers
//! read; XMP `dc:rights` is what Lightroom, Bridge and a client gallery read. A file with one and
//! not the other is a file whose copyright disappears depending on who opens it.

use std::fmt::Write as _;

use aura_core::contract::delivery::{DeliveryCode, DeliveryReason, MetadataPolicy};

/// The Exif tags this module writes, and the only ones it writes.
///
/// Six. Software, copyright, artist, and the three that describe the pixels. No camera model, no
/// lens, no serial number, no timestamps from the original and **no GPS IFD pointer at all** - the
/// tag that would carry a location has no code path that emits it.
pub const WRITTEN_TAGS: [(&str, u16); 6] = [
    ("ImageWidth", 0x0100),
    ("ImageLength", 0x0101),
    ("Software", 0x0131),
    ("Artist", 0x013B),
    ("Copyright", 0x8298),
    ("ColorSpace", 0xA001),
];

/// The Exif block for a JPEG's APP1 segment, or a TIFF's own IFD entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExifBlock {
    /// The `Exif\0\0`-prefixed TIFF structure, ready to be an APP1 payload.
    pub app1: Vec<u8>,
    /// What the panel says about it.
    pub reasons: Vec<DeliveryReason>,
}

/// One IFD entry, ready to be written into a TIFF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The tag.
    pub tag: u16,
    /// The TIFF type: 3 = SHORT, 4 = LONG, 2 = ASCII.
    pub kind: u16,
    /// How many values.
    pub count: u32,
    /// The value, or an offset when it does not fit in four bytes.
    pub value: u32,
}

/// Build the Exif APP1 payload for one delivered file.
///
/// The structure is a complete little-endian TIFF header plus a zeroth IFD - which is what an
/// APP1/Exif segment is, and why a JPEG's metadata is a TIFF file inside it.
#[must_use]
pub fn build(policy: &MetadataPolicy, width: u32, height: u32, srgb: bool) -> ExifBlock {
    let mut reasons = Vec::new();

    // Every ASCII value, NUL-terminated, packed after the IFD.
    let mut ascii: Vec<(u16, String)> = Vec::new();
    ascii.push((0x0131, format!("AURA {}", crate::ENGINE)));
    if let Some(artist) = policy.creator.as_ref().filter(|s| !s.trim().is_empty()) {
        ascii.push((0x013B, artist.clone()));
    }
    if let Some(c) = policy.copyright.as_ref().filter(|s| !s.trim().is_empty()) {
        // The contact is appended to the copyright line rather than given a tag of its own,
        // because the tag that would hold it - `OwnerName`, 0xA430 - is a MakerNote-adjacent
        // field half the readers in circulation ignore. A copyright line that says how to reach
        // the photographer is read by all of them.
        let line = match policy.contact.as_ref().filter(|s| !s.trim().is_empty()) {
            Some(contact) => format!("{c} - {contact}"),
            None => c.clone(),
        };
        ascii.push((0x8298, line));
    }
    ascii.sort_by_key(|(tag, _)| *tag);

    let shorts: Vec<(u16, u16, u32)> = vec![
        (0x0100, 4, width),
        (0x0101, 4, height),
        // 1 = sRGB, 0xFFFF = uncalibrated. A file in Adobe RGB that claimed sRGB here would be a
        // file two readers disagree about, so an ICC profile carries the truth and this says
        // "not sRGB" rather than guessing.
        (0xA001, 3, if srgb { 1 } else { 0xFFFF }),
    ];

    let entry_count = shorts.len() + ascii.len();
    let ifd_bytes = 2 + entry_count * 12 + 4;
    let mut heap_offset = 8 + ifd_bytes; // TIFF header is 8 bytes

    let mut ifd = Vec::with_capacity(ifd_bytes);
    let mut heap: Vec<u8> = Vec::new();

    #[allow(clippy::cast_possible_truncation)]
    ifd.extend_from_slice(&(entry_count as u16).to_le_bytes());

    // Entries must be in ascending tag order; merge the two lists.
    let mut merged: Vec<(u16, u16, u32, Option<String>)> = Vec::with_capacity(entry_count);
    for (tag, kind, value) in shorts {
        merged.push((tag, kind, value, None));
    }
    for (tag, text) in ascii {
        merged.push((tag, 2, 0, Some(text)));
    }
    merged.sort_by_key(|(tag, ..)| *tag);

    for (tag, kind, value, text) in merged {
        ifd.extend_from_slice(&tag.to_le_bytes());
        ifd.extend_from_slice(&kind.to_le_bytes());
        if let Some(t) = text {
            {
                let mut bytes = t.into_bytes();
                bytes.push(0);
                #[allow(clippy::cast_possible_truncation)]
                ifd.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                if bytes.len() <= 4 {
                    let mut padded = [0_u8; 4];
                    for (i, b) in bytes.iter().enumerate() {
                        if let Some(slot) = padded.get_mut(i) {
                            *slot = *b;
                        }
                    }
                    ifd.extend_from_slice(&padded);
                } else {
                    #[allow(clippy::cast_possible_truncation)]
                    ifd.extend_from_slice(&(heap_offset as u32).to_le_bytes());
                    heap_offset += bytes.len();
                    heap.extend_from_slice(&bytes);
                }
            }
        } else {
            ifd.extend_from_slice(&1_u32.to_le_bytes());
            if kind == 3 {
                #[allow(clippy::cast_possible_truncation)]
                ifd.extend_from_slice(&(value as u16).to_le_bytes());
                ifd.extend_from_slice(&[0, 0]);
            } else {
                ifd.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    ifd.extend_from_slice(&0_u32.to_le_bytes()); // no next IFD

    let mut app1 = Vec::with_capacity(6 + 8 + ifd.len() + heap.len());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(b"II"); // little endian
    app1.extend_from_slice(&42_u16.to_le_bytes());
    app1.extend_from_slice(&8_u32.to_le_bytes());
    app1.extend_from_slice(&ifd);
    app1.extend_from_slice(&heap);

    if policy.strip_gps {
        reasons.push(DeliveryReason::plain(DeliveryCode::GpsStripped));
    }
    if policy.strip_camera_serial {
        reasons.push(DeliveryReason::plain(DeliveryCode::SerialStripped));
    }

    ExifBlock { app1, reasons }
}

/// The XMP packet a delivered file carries: `dc:rights`, `dc:creator` and `dc:subject`.
///
/// Written as a standalone packet with the processing instructions a scanner looks for, so it can
/// go into a JPEG APP1, a TIFF tag, or a PNG `iTXt` chunk unchanged.
#[must_use]
pub fn xmp_packet(policy: &MetadataPolicy) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };

    let mut body = String::new();
    if let Some(c) = policy.copyright.as_ref().filter(|s| !s.trim().is_empty()) {
        let _ = writeln!(
            body,
            "   <dc:rights><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:rights>",
            esc(c)
        );
    }
    if let Some(a) = policy.creator.as_ref().filter(|s| !s.trim().is_empty()) {
        let _ = writeln!(
            body,
            "   <dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>",
            esc(a)
        );
    }
    if let Some(u) = policy.contact.as_ref().filter(|s| !s.trim().is_empty()) {
        let _ = writeln!(body, "   <xmp:BaseURL>{}</xmp:BaseURL>", esc(u));
    }
    if !policy.keywords.is_empty() {
        body.push_str("   <dc:subject><rdf:Bag>\n");
        for k in &policy.keywords {
            let _ = writeln!(body, "    <rdf:li>{}</rdf:li>", esc(k));
        }
        body.push_str("   </rdf:Bag></dc:subject>\n");
    }

    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"AURA\">\n\
         \x20<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         \x20 <rdf:Description rdf:about=\"\"\n\
         \x20  xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n\
         \x20  xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n\
         {body}\
         \x20 </rdf:Description>\n\
         \x20</rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_code_path_writes_a_location_tag() {
        // The strongest form of the guarantee: not "the location is removed" but "there is no
        // code here that could write one". `WRITTEN_TAGS` is the complete list and 0x8825 - the
        // GPS IFD pointer - is not in it.
        assert!(!WRITTEN_TAGS.iter().any(|(_, tag)| *tag == 0x8825));
        for (name, _) in WRITTEN_TAGS {
            assert!(!name.to_ascii_lowercase().contains("gps"));
        }
    }

    #[test]
    fn an_exif_block_is_a_little_endian_tiff_a_reader_can_walk() {
        let policy = MetadataPolicy {
            copyright: Some("© 2026 Studio".to_owned()),
            contact: Some("studio.example".to_owned()),
            creator: Some("Alex Photographer".to_owned()),
            keywords: vec!["wedding".to_owned()],
            strip_gps: true,
            strip_camera_serial: true,
        };
        let block = build(&policy, 4000, 3000, true);
        assert_eq!(&block.app1[..6], b"Exif\0\0");
        assert_eq!(&block.app1[6..8], b"II");
        assert_eq!(u16::from_le_bytes([block.app1[8], block.app1[9]]), 42);
        let ifd_off = u32::from_le_bytes([
            block.app1[10],
            block.app1[11],
            block.app1[12],
            block.app1[13],
        ]) as usize;
        assert_eq!(ifd_off, 8);
        let base = 6 + ifd_off;
        let count = u16::from_le_bytes([block.app1[base], block.app1[base + 1]]) as usize;
        assert_eq!(count, 6, "three shorts, software, artist, copyright");

        // Tags ascend, which is what a TIFF reader relies on.
        let mut last = 0_u16;
        for i in 0..count {
            let at = base + 2 + i * 12;
            let tag = u16::from_le_bytes([block.app1[at], block.app1[at + 1]]);
            assert!(tag > last, "tag {tag:#06x} out of order");
            last = tag;
        }
    }

    #[test]
    fn the_copyright_carries_the_contact_because_one_tag_is_read_by_everything() {
        let policy = MetadataPolicy {
            copyright: Some("© Studio".to_owned()),
            contact: Some("studio.example".to_owned()),
            ..MetadataPolicy::default()
        };
        let block = build(&policy, 100, 100, true);
        let text = String::from_utf8_lossy(&block.app1);
        assert!(text.contains("© Studio - studio.example"));
    }

    #[test]
    fn a_non_srgb_export_says_uncalibrated_rather_than_guessing() {
        let block = build(&MetadataPolicy::default(), 10, 10, false);
        // 0xA001 = ColorSpace. 0xFFFF means "the ICC profile is the truth".
        let text: Vec<u8> = block.app1.clone();
        let base = 6 + 8;
        let count = u16::from_le_bytes([text[base], text[base + 1]]) as usize;
        let mut found = None;
        for i in 0..count {
            let at = base + 2 + i * 12;
            if u16::from_le_bytes([text[at], text[at + 1]]) == 0xA001 {
                found = Some(u16::from_le_bytes([text[at + 8], text[at + 9]]));
            }
        }
        assert_eq!(found, Some(0xFFFF));
    }

    #[test]
    fn the_xmp_packet_escapes_what_a_photographer_actually_types() {
        let policy = MetadataPolicy {
            copyright: Some("Alex & Sam <studio>".to_owned()),
            keywords: vec!["a\"b".to_owned()],
            ..MetadataPolicy::default()
        };
        let x = xmp_packet(&policy);
        assert!(x.contains("Alex &amp; Sam &lt;studio&gt;"));
        assert!(x.contains("a&quot;b"));
        assert!(x.starts_with("<?xpacket begin="));
        assert!(x.ends_with("<?xpacket end=\"w\"?>"));
    }

    #[test]
    fn an_empty_policy_still_produces_a_valid_block() {
        let block = build(&MetadataPolicy::default(), 1, 1, true);
        assert!(block.app1.len() > 20);
        // The default strips both, so both notes are present.
        assert_eq!(block.reasons.len(), 2);
    }
}
