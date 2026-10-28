//! EXIF parsing on wedding cards is adversarial input: cameras write malformed
//! IFDs, third-party apps write circular pointers, and a corrupt card produces
//! arbitrary bytes.
//!
//! Rules:
//!   1. Never read more than 2 MiB looking for metadata.
//!   2. A parse failure is a warning, never an item failure. A photo with no
//!      EXIF is still a photo, and dropping it would be the worse bug.
//!   3. Cap every string at 256 chars before it reaches the catalog.
//!
//! Canon's CR3 is an ISO media file, which the EXIF reader does not open. Its metadata is in
//! two boxes, each a complete TIFF: `CMT1` is IFD0 (make, model, orientation) and `CMT2` is
//! the EXIF directory (capture time, ISO, shutter speed). Without them a CR3 imported with
//! no capture time, so no burst a Canon body shot could be found.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use aura_core::errors::io as ioerr;
use aura_core::AuraResult;
use exif::{Exif, In, Tag, Value};
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

/// How much of a file we are willing to read looking for metadata.
const MAX_METADATA_BYTES: u64 = 2 * 1024 * 1024;

/// What we managed to learn about one file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExifFacts {
    /// When the shutter fired, if the camera said so.
    pub capture_time: Option<OffsetDateTime>,
    /// Which tag the capture time came from.
    pub capture_time_source: &'static str,
    /// Camera manufacturer.
    pub camera_make: Option<String>,
    /// Camera model.
    pub camera_model: Option<String>,
    /// Body serial number, the clustering key for clock alignment.
    pub camera_serial: Option<String>,
    /// Lens model.
    pub lens_model: Option<String>,
    /// ISO speed.
    pub iso: Option<i64>,
    /// Aperture as an f-number.
    pub aperture: Option<f64>,
    /// Exposure time in whole microseconds.
    pub shutter_us: Option<i64>,
    /// Focal length in millimetres.
    pub focal_len_mm: Option<f64>,
    /// Whether the flash fired.
    pub flash_fired: Option<bool>,
    /// EXIF orientation, 1 to 8.
    pub orientation: u8,
    /// Pixel width.
    pub width_px: Option<i64>,
    /// Pixel height.
    pub height_px: Option<i64>,
    /// Sub-second field, used to break ties inside a burst.
    pub sub_sec: Option<i64>,
    /// Validated latitude and longitude.
    pub gps: Option<(f64, f64)>,
    /// Every tag, verbatim, for the exif table.
    pub raw_tags: Vec<(String, String)>,
}

/// Read what metadata the file is willing to give up.
///
/// # Errors
///
/// Returns `AURA-IO-1006` only when the file itself cannot be opened. A parse
/// failure returns default facts so the photograph still imports.
pub fn extract(path: &Path) -> AuraResult<ExifFacts> {
    let mut facts = ExifFacts {
        orientation: 1,
        capture_time_source: "unknown",
        ..Default::default()
    };

    let file = File::open(path).map_err(|e| ioerr::from_io(&e, path))?;

    // The parser needs Seek, and `Take` is not seekable, so the cap is applied by
    // reading a bounded prefix into memory and seeking inside that.
    let mut head = Vec::with_capacity(64 * 1024);
    BufReader::with_capacity(64 * 1024, file.take(MAX_METADATA_BYTES))
        .read_to_end(&mut head)
        .map_err(|e| ioerr::from_io(&e, path))?;
    let blocks = match exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&head)) {
        Ok(parsed) => vec![parsed],
        Err(e) => {
            let blocks = cr3_blocks(&head);
            if blocks.is_empty() {
                tracing::warn!(
                    target: "ingest",
                    ext = ?path.extension(),
                    error = %e,
                    "exif unreadable, continuing without metadata"
                );
                return Ok(facts);
            }
            blocks
        }
    };
    for parsed in &blocks {
        apply(parsed, &mut facts);
    }
    facts.raw_tags.sort(); // DETERMINISM: stable exif row order
    facts.raw_tags.dedup_by(|a, b| a.0 == b.0);
    Ok(facts)
}

/// Fill what one block of metadata knows and the blocks before it did not.
fn apply(parsed: &Exif, facts: &mut ExifFacts) {
    for field in parsed.fields() {
        let tag = field.tag.to_string();
        let mut value = field.display_value().to_string();
        value.truncate(256);
        facts.raw_tags.push((tag, value));
    }

    let first = |current: &mut Option<String>, tag: Tag| {
        if current.is_none() {
            *current = text(parsed, tag);
        }
    };
    first(&mut facts.camera_make, Tag::Make);
    first(&mut facts.camera_model, Tag::Model);
    first(&mut facts.camera_serial, Tag::BodySerialNumber);
    first(&mut facts.lens_model, Tag::LensModel);
    facts.iso = facts
        .iso
        .or_else(|| number(parsed, Tag::PhotographicSensitivity));
    facts.width_px = facts
        .width_px
        .or_else(|| number(parsed, Tag::PixelXDimension));
    facts.height_px = facts
        .height_px
        .or_else(|| number(parsed, Tag::PixelYDimension));
    facts.sub_sec = facts
        .sub_sec
        .or_else(|| text(parsed, Tag::SubSecTimeOriginal).and_then(|s| s.trim().parse().ok()));
    facts.flash_fired = facts
        .flash_fired
        .or_else(|| number(parsed, Tag::Flash).map(|v| v & 1 == 1));
    if let Some(orientation) = number(parsed, Tag::Orientation)
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
    {
        facts.orientation = orientation;
    }

    // Exposure time as an exact rational, converted to integer microseconds.
    if let Some((num, denom)) = rational(parsed, Tag::ExposureTime) {
        if denom != 0 && facts.shutter_us.is_none() {
            #[allow(clippy::cast_possible_truncation)]
            let micros = ((f64::from(num) / f64::from(denom)) * 1_000_000.0).round() as i64;
            facts.shutter_us = Some(micros);
        }
    }
    facts.aperture = facts
        .aperture
        .or_else(|| rational(parsed, Tag::FNumber).and_then(ratio));
    facts.focal_len_mm = facts
        .focal_len_mm
        .or_else(|| rational(parsed, Tag::FocalLength).and_then(ratio));
    if facts.capture_time.is_some() {
        return;
    }

    // Capture time, in order of trustworthiness. DateTimeOriginal is when the
    // shutter fired; DateTime is when the file was last written, which a camera
    // clock reset or an editing app will have changed.
    for (tag, offset_tag, source) in [
        (
            Tag::DateTimeOriginal,
            Tag::OffsetTimeOriginal,
            "exif_original",
        ),
        (
            Tag::DateTimeDigitized,
            Tag::OffsetTimeDigitized,
            "exif_digitized",
        ),
    ] {
        if let Some(raw) = text(parsed, tag) {
            if let Some(t) = parse_exif_datetime(&raw, text(parsed, offset_tag).as_deref()) {
                facts.capture_time = Some(t);
                facts.capture_time_source = source;
                break;
            }
        }
    }

    // GPS only if both coordinates are present and inside valid ranges. A corrupt
    // IFD frequently yields lat = 91.0, and a wrong venue on a map is worse than
    // no venue.
    if let (Some(lat), Some(lon)) = (gps_coord(parsed, true), gps_coord(parsed, false)) {
        if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
            facts.gps = Some((lat, lon));
        }
    }
}

/// A CR3's metadata blocks, IFD0 first; empty for anything that is not a CR3.
fn cr3_blocks(head: &[u8]) -> Vec<Exif> {
    if head.get(4..12) != Some(&b"ftypcrx "[..]) {
        return Vec::new();
    }
    let ifd0 = find_tiff_box(head, *b"CMT1").map(<[u8]>::to_vec);
    let exif = find_tiff_box(head, *b"CMT2").and_then(as_exif_directory);
    [ifd0, exif]
        .into_iter()
        .flatten()
        .filter_map(|tiff| exif::Reader::new().read_raw(tiff).ok())
        .collect()
}

/// The payload of the first box called `name` whose payload is a TIFF. Boxes are found by
/// scanning, as the decoder finds the CR3 previews, rather than by walking every container.
fn find_tiff_box(head: &[u8], name: [u8; 4]) -> Option<&[u8]> {
    let mut from = 0;
    while let Some(found) = head
        .get(from..)?
        .windows(4)
        .position(|w| w == name.as_slice())
    {
        let at = from + found;
        from = at + 4;
        let Some(size) = at
            .checked_sub(4)
            .and_then(|start| head.get(start..at))
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .and_then(|bytes| usize::try_from(u32::from_be_bytes(bytes)).ok())
        else {
            continue;
        };
        let Some(payload) = (at - 4)
            .checked_add(size)
            .and_then(|end| head.get(at + 4..end))
        else {
            continue;
        };
        if payload.starts_with(b"II*\0") || payload.starts_with(b"MM\0*") {
            return Some(payload);
        }
    }
    None
}

/// `CMT2` is a TIFF whose first directory *is* the EXIF directory, so the reader would file
/// its tags under IFD0 and never find a capture time. An IFD0 holding one pointer to it is
/// appended - after the data, so no offset inside the block moves.
fn as_exif_directory(tiff: &[u8]) -> Option<Vec<u8>> {
    const EXIF_IFD: u16 = 0x8769;
    const LONG: u16 = 4;
    let little = tiff.starts_with(b"II");
    let u16_bytes = |v: u16| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let u32_bytes = |v: u32| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let directory = tiff.get(4..8)?.to_vec();
    let mut out = tiff.to_vec();
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let at = u32::try_from(out.len()).ok()?;
    out.extend_from_slice(&u16_bytes(1));
    out.extend_from_slice(&u16_bytes(EXIF_IFD));
    out.extend_from_slice(&u16_bytes(LONG));
    out.extend_from_slice(&u32_bytes(1));
    out.extend_from_slice(&directory);
    out.extend_from_slice(&u32_bytes(0));
    out.get_mut(4..8)?.copy_from_slice(&u32_bytes(at));
    Some(out)
}

/// Parse `YYYY:MM:DD HH:MM:SS` with an optional `+HH:MM` offset tag.
///
/// A camera that records no offset is treated as UTC: inventing a timezone would
/// be inventing data, and the clock aligner works on relative offsets anyway.
#[must_use]
pub fn parse_exif_datetime(raw: &str, offset: Option<&str>) -> Option<OffsetDateTime> {
    let trimmed = raw.trim();
    let (date_part, time_part) = trimmed.split_once(' ')?;
    let mut date_fields = date_part.split(&[':', '-'][..]);
    let year: i32 = date_fields.next()?.parse().ok()?;
    let month: u8 = date_fields.next()?.parse().ok()?;
    let day: u8 = date_fields.next()?.parse().ok()?;

    let mut time_fields = time_part.split(':');
    let hour: u8 = time_fields.next()?.parse().ok()?;
    let minute: u8 = time_fields.next()?.parse().ok()?;
    let second: u8 = time_fields
        .next()
        .unwrap_or("0")
        .split('.')
        .next()?
        .parse()
        .ok()?;

    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    let time = Time::from_hms(hour, minute, second).ok()?;
    let naive = PrimitiveDateTime::new(date, time);

    let utc_offset = offset.and_then(parse_utc_offset).unwrap_or(UtcOffset::UTC);
    Some(naive.assume_offset(utc_offset).to_offset(UtcOffset::UTC))
}

fn parse_utc_offset(raw: &str) -> Option<UtcOffset> {
    let trimmed = raw.trim();
    let sign = match trimmed.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let body = trimmed.get(1..)?;
    let (hours, minutes) = body.split_once(':')?;
    let hours: i8 = hours.parse().ok()?;
    let minutes: i8 = minutes.parse().ok()?;
    UtcOffset::from_hms(sign * hours, sign * minutes, 0).ok()
}

fn text(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let mut value = field.display_value().to_string();
    let cleaned = value.trim().trim_matches('"').to_string();
    value = cleaned;
    value.truncate(256);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn number(exif: &Exif, tag: Tag) -> Option<i64> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    field.value.get_uint(0).map(i64::from)
}

fn rational(exif: &Exif, tag: Tag) -> Option<(u32, u32)> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        Value::Rational(values) => values.first().map(|r| (r.num, r.denom)),
        _ => None,
    }
}

fn ratio((num, denom): (u32, u32)) -> Option<f64> {
    if denom == 0 {
        None
    } else {
        Some(f64::from(num) / f64::from(denom))
    }
}

/// Decimal degrees for latitude (`is_lat`) or longitude, sign applied from the
/// reference tag.
fn gps_coord(exif: &Exif, is_lat: bool) -> Option<f64> {
    let (value_tag, ref_tag, negative) = if is_lat {
        (Tag::GPSLatitude, Tag::GPSLatitudeRef, "S")
    } else {
        (Tag::GPSLongitude, Tag::GPSLongitudeRef, "W")
    };

    let field = exif.get_field(value_tag, In::PRIMARY)?;
    let Value::Rational(parts) = &field.value else {
        return None;
    };
    let degrees = parts.first()?.to_f64();
    let minutes = parts.get(1).map_or(0.0, exif::Rational::to_f64);
    let seconds = parts.get(2).map_or(0.0, exif::Rational::to_f64);
    let decimal = degrees + minutes / 60.0 + seconds / 3600.0;

    let sign = exif
        .get_field(ref_tag, In::PRIMARY)
        .map(|f| f.display_value().to_string())
        .filter(|r| r.trim().eq_ignore_ascii_case(negative))
        .map_or(1.0, |_| -1.0);

    Some(decimal * sign)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    /// A little-endian TIFF whose first directory holds `entries` of ASCII or SHORT values.
    fn tiff(entries: &[(u16, &str)], shorts: &[(u16, u16)]) -> Vec<u8> {
        let count = entries.len() + shorts.len();
        let data_at = 8 + 2 + count * 12 + 4;
        let mut data = Vec::new();
        let mut out = b"II*\0".to_vec();
        out.extend_from_slice(&8_u32.to_le_bytes());
        out.extend_from_slice(&u16::try_from(count).unwrap().to_le_bytes());
        for (tag, text) in entries {
            let mut value = text.as_bytes().to_vec();
            value.push(0);
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&2_u16.to_le_bytes());
            out.extend_from_slice(&u32::try_from(value.len()).unwrap().to_le_bytes());
            // Four bytes or fewer live in the entry itself.
            if value.len() <= 4 {
                value.resize(4, 0);
                out.extend_from_slice(&value);
                continue;
            }
            out.extend_from_slice(&u32::try_from(data_at + data.len()).unwrap().to_le_bytes());
            data.extend_from_slice(&value);
            if data.len() % 2 == 1 {
                data.push(0);
            }
        }
        for (tag, value) in shorts {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&3_u16.to_le_bytes());
            out.extend_from_slice(&1_u32.to_le_bytes());
            out.extend_from_slice(&u32::from(*value).to_le_bytes());
        }
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    fn bmff_box(name: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(payload.len() + 8)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(&name);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn a_canon_cr3_gives_up_its_capture_time_and_settings() {
        let cmt1 = tiff(
            &[(0x010F, "Canon"), (0x0110, "Canon EOS R6")],
            &[(0x0112, 1)],
        );
        let cmt2 = tiff(
            &[(0x9003, "2026:02:14 15:04:05"), (0x9291, "42")],
            &[(0x8827, 1600), (0x9209, 16)],
        );
        let mut file = bmff_box(*b"ftyp", b"crx \0\0\0\x01crx isom");
        let mut canon =
            b"\x85\xc0\xb6\x87\x82\x0f\x11\xe0\x81\x11\xf4\xce\x46\x2b\x6a\x48".to_vec();
        canon.extend(bmff_box(*b"CMT1", &cmt1));
        canon.extend(bmff_box(*b"CMT2", &cmt2));
        file.extend(bmff_box(*b"moov", &bmff_box(*b"uuid", &canon)));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("IMG_0001.CR3");
        std::fs::write(&path, &file).unwrap();

        let facts = extract(&path).unwrap();
        assert_eq!(facts.capture_time_source, "exif_original");
        let time = facts.capture_time.unwrap();
        assert_eq!((time.hour(), time.minute(), time.second()), (15, 4, 5));
        assert_eq!(facts.sub_sec, Some(42));
        assert_eq!(facts.camera_make.as_deref(), Some("Canon"));
        assert_eq!(facts.camera_model.as_deref(), Some("Canon EOS R6"));
        assert_eq!(facts.iso, Some(1600));
        assert_eq!(facts.flash_fired, Some(false));
    }

    #[test]
    fn a_file_that_only_looks_like_a_cr3_imports_without_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.CR3");
        let mut file = bmff_box(*b"ftyp", b"crx \0\0\0\x01");
        // A CMT2 box whose size runs past the end of the file.
        file.extend_from_slice(&9999_u32.to_be_bytes());
        file.extend_from_slice(b"CMT2II*\0");
        std::fs::write(&path, &file).unwrap();
        let facts = extract(&path).unwrap();
        assert!(facts.capture_time.is_none());
        assert_eq!(facts.orientation, 1);
    }
}
