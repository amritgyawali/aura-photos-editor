//! EXIF parsing on wedding cards is adversarial input: cameras write malformed
//! IFDs, third-party apps write circular pointers, and a corrupt card produces
//! arbitrary bytes.
//!
//! Rules:
//!   1. Never read more than 2 MiB looking for metadata.
//!   2. A parse failure is a warning, never an item failure. A photo with no
//!      EXIF is still a photo, and dropping it would be the worse bug.
//!   3. Cap every string at 256 chars before it reaches the catalog.

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
    let mut reader = std::io::Cursor::new(head);

    let parsed = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                target: "ingest",
                ext = ?path.extension(),
                error = %e,
                "exif unreadable, continuing without metadata"
            );
            return Ok(facts);
        }
    };

    for field in parsed.fields() {
        let tag = field.tag.to_string();
        let mut value = field.display_value().to_string();
        value.truncate(256);
        facts.raw_tags.push((tag, value));
    }
    facts.raw_tags.sort(); // DETERMINISM: stable exif row order
    facts.raw_tags.dedup_by(|a, b| a.0 == b.0);

    facts.camera_make = text(&parsed, Tag::Make);
    facts.camera_model = text(&parsed, Tag::Model);
    facts.camera_serial = text(&parsed, Tag::BodySerialNumber);
    facts.lens_model = text(&parsed, Tag::LensModel);
    facts.iso = number(&parsed, Tag::PhotographicSensitivity);
    facts.width_px = number(&parsed, Tag::PixelXDimension);
    facts.height_px = number(&parsed, Tag::PixelYDimension);
    facts.sub_sec = text(&parsed, Tag::SubSecTimeOriginal).and_then(|s| s.trim().parse().ok());
    facts.flash_fired = number(&parsed, Tag::Flash).map(|v| v & 1 == 1);
    facts.orientation = number(&parsed, Tag::Orientation)
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);

    // Exposure time as an exact rational, converted to integer microseconds.
    if let Some((num, denom)) = rational(&parsed, Tag::ExposureTime) {
        if denom != 0 {
            #[allow(clippy::cast_possible_truncation)]
            let micros = ((f64::from(num) / f64::from(denom)) * 1_000_000.0).round() as i64;
            facts.shutter_us = Some(micros);
        }
    }
    facts.aperture = rational(&parsed, Tag::FNumber).and_then(ratio);
    facts.focal_len_mm = rational(&parsed, Tag::FocalLength).and_then(ratio);

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
        if let Some(raw) = text(&parsed, tag) {
            if let Some(t) = parse_exif_datetime(&raw, text(&parsed, offset_tag).as_deref()) {
                facts.capture_time = Some(t);
                facts.capture_time_source = source;
                break;
            }
        }
    }

    // GPS only if both coordinates are present and inside valid ranges. A corrupt
    // IFD frequently yields lat = 91.0, and a wrong venue on a map is worse than
    // no venue.
    if let (Some(lat), Some(lon)) = (gps_coord(&parsed, true), gps_coord(&parsed, false)) {
        if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
            facts.gps = Some((lat, lon));
        }
    }

    Ok(facts)
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
