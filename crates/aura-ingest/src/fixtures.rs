//! Synthetic reference weddings.
//!
//! Real wedding imagery cannot be committed to a repository: it is other
//! people's faces. So the fixtures are generated - tiny JPEG files carrying real
//! EXIF written by this module, with the camera bodies, clock drift and file
//! pairing that the ingest tests need.

use std::path::{Path, PathBuf};

use time::{Duration, OffsetDateTime};

/// One synthetic camera body.
#[derive(Debug, Clone)]
pub struct FixtureBody {
    /// Manufacturer written into EXIF.
    pub make: &'static str,
    /// Model written into EXIF.
    pub model: &'static str,
    /// Serial written into EXIF; the clustering key for clock alignment.
    pub serial: &'static str,
    /// Milliseconds this body's clock is wrong by, applied to what it records.
    pub clock_skew_ms: i64,
    /// How many frames it shot.
    pub frames: usize,
    /// Whether it also writes a companion JPEG next to each RAW-named file.
    pub writes_companions: bool,
}

/// One synthetic wedding.
#[derive(Debug, Clone)]
pub struct FixtureWedding {
    /// Folder name under the output directory.
    pub slug: &'static str,
    /// Bodies that shot it.
    pub bodies: Vec<FixtureBody>,
    /// Seconds between consecutive frames on the reference body.
    pub cadence_s: i64,
}

/// The three reference weddings every phase tests against.
#[must_use]
pub fn reference_weddings() -> Vec<FixtureWedding> {
    vec![
        FixtureWedding {
            slug: "hindu_night",
            cadence_s: 4,
            bodies: vec![
                FixtureBody {
                    make: "Canon",
                    model: "EOS R5",
                    serial: "CANON-R5-0001",
                    clock_skew_ms: 0,
                    frames: 90,
                    writes_companions: true,
                },
                FixtureBody {
                    make: "Sony",
                    model: "ILCE-7M4",
                    serial: "SONY-A74-0002",
                    // The classic daylight-saving hour, plus a little drift.
                    clock_skew_ms: 3_600_000 + 12_000,
                    frames: 60,
                    writes_companions: false,
                },
            ],
        },
        FixtureWedding {
            slug: "daylight_church",
            cadence_s: 6,
            bodies: vec![
                FixtureBody {
                    make: "Nikon",
                    model: "Z 8",
                    serial: "NIKON-Z8-0003",
                    clock_skew_ms: 0,
                    frames: 100,
                    writes_companions: false,
                },
                FixtureBody {
                    make: "Fujifilm",
                    model: "X-T5",
                    serial: "FUJI-XT5-0004",
                    clock_skew_ms: -90_000,
                    frames: 50,
                    writes_companions: true,
                },
            ],
        },
        FixtureWedding {
            slug: "nepali_reception",
            cadence_s: 3,
            bodies: vec![
                FixtureBody {
                    make: "Canon",
                    model: "EOS R6",
                    serial: "CANON-R6-0005",
                    clock_skew_ms: 0,
                    frames: 110,
                    writes_companions: false,
                },
                FixtureBody {
                    make: "Sony",
                    model: "ILCE-7RM5",
                    serial: "SONY-A7R5-0006",
                    clock_skew_ms: 7 * 3_600_000,
                    frames: 40,
                    writes_companions: false,
                },
            ],
        },
    ]
}

/// What one generated wedding contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedWedding {
    /// Folder the files were written to.
    pub root: PathBuf,
    /// Total files written, including companions.
    pub files: usize,
    /// Photographs represented, counting a RAW plus its companion as one.
    pub photos: usize,
}

/// Write every reference wedding under `out`.
///
/// # Errors
///
/// Returns the underlying IO failure as a string; this is developer tooling.
pub fn generate_all(out: &Path) -> Result<Vec<GeneratedWedding>, String> {
    reference_weddings()
        .into_iter()
        .map(|wedding| generate(out, &wedding))
        .collect()
}

/// Write one wedding, deterministically: the same call always produces the same
/// bytes, which is what makes the catalog snapshot comparable.
///
/// # Errors
///
/// Returns the underlying IO failure as a string.
pub fn generate(out: &Path, wedding: &FixtureWedding) -> Result<GeneratedWedding, String> {
    let root = out.join(wedding.slug);
    std::fs::create_dir_all(&root).map_err(|e| format!("create {}: {e}", root.display()))?;

    // A fixed base time, so fixtures never depend on when they were generated.
    let base = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .map_err(|e| format!("base time: {e}"))?;

    let mut files = 0usize;
    let mut photos = 0usize;

    // One shared timeline of moments, shot by more than one body. Irregular gaps
    // matter: two bodies firing on a perfectly even cadence make every offset look
    // equally likely to a histogram, which is not what a wedding looks like.
    let event_count = wedding.bodies.iter().map(|b| b.frames).max().unwrap_or(0);
    let events = event_times(base, wedding.cadence_s, event_count);

    for (body_index, body) in wedding.bodies.iter().enumerate() {
        let dir = root.join(format!("body{}", body_index + 1));
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

        // The busiest body shoots every moment; the others shoot a subset of the
        // same moments, which is exactly the evidence the clock aligner needs.
        let stride = (event_count / body.frames.max(1)).max(1);

        for frame in 0..body.frames {
            let Some(true_time) = events.get(frame * stride).copied() else {
                break;
            };
            let recorded = true_time + Duration::milliseconds(body.clock_skew_ms);
            let stem = format!("IMG_{:04}", frame + 1);

            let primary = dir.join(format!("{stem}.JPG"));
            std::fs::write(&primary, jpeg_with_exif(body, recorded, frame))
                .map_err(|e| format!("write {}: {e}", primary.display()))?;
            files += 1;
            photos += 1;

            if body.writes_companions {
                // A sidecar shares the stem and must never become its own photo.
                let sidecar = dir.join(format!("{stem}.xmp"));
                std::fs::write(&sidecar, sidecar_xmp(&stem))
                    .map_err(|e| format!("write {}: {e}", sidecar.display()))?;
                files += 1;
            }
        }
    }

    Ok(GeneratedWedding {
        root,
        files,
        photos,
    })
}

/// Deterministic, irregular moments. A wedding is not a metronome, and a
/// metronome defeats the clock aligner's histogram.
fn event_times(base: OffsetDateTime, cadence_s: i64, count: usize) -> Vec<OffsetDateTime> {
    let mut times = Vec::with_capacity(count);
    let mut clock = base;
    // DETERMINISM: a fixed linear congruential sequence, never a random source.
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    for _ in 0..count {
        times.push(clock);
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let jitter = i64::try_from((seed >> 33) % 9).unwrap_or(0);
        clock += Duration::seconds(cadence_s + jitter);
    }
    times
}

fn sidecar_xmp(stem: &str) -> Vec<u8> {
    format!(
        "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n  <!-- fixture sidecar for {stem} -->\n\
         </x:xmpmeta>\n<?xpacket end=\"w\"?>\n"
    )
    .into_bytes()
}

/// A minimal but genuinely parseable JPEG carrying an EXIF APP1 segment.
fn jpeg_with_exif(body: &FixtureBody, recorded: OffsetDateTime, frame: usize) -> Vec<u8> {
    let exif = exif_payload(body, recorded, frame);

    let mut out = Vec::with_capacity(exif.len() + 256);
    out.extend_from_slice(&[0xFF, 0xD8]); // SOI

    // APP1 with the EXIF identifier.
    let app1_len = u16::try_from(exif.len() + 2).unwrap_or(u16::MAX);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&app1_len.to_be_bytes());
    out.extend_from_slice(&exif);

    // A tiny grey 8x8 baseline frame so the file is a real image, not a stub.
    out.extend_from_slice(MINIMAL_JPEG_BODY);
    out.extend_from_slice(&[0xFF, 0xD9]); // EOI
    out
}

/// Build `Exif\0\0` plus a little-endian TIFF block with IFD0 and an EXIF IFD.
fn exif_payload(body: &FixtureBody, recorded: OffsetDateTime, frame: usize) -> Vec<u8> {
    const TAG_MAKE: u16 = 0x010F;
    const TAG_MODEL: u16 = 0x0110;
    const TAG_EXIF_IFD: u16 = 0x8769;
    const TAG_DATETIME_ORIGINAL: u16 = 0x9003;
    const TAG_SUBSEC_ORIGINAL: u16 = 0x9291;
    const TAG_BODY_SERIAL: u16 = 0xA431;
    const TYPE_ASCII: u16 = 2;
    const TYPE_LONG: u16 = 4;

    let make = nul(body.make);
    let model = nul(body.model);
    let serial = nul(body.serial);
    let datetime = nul(&format!(
        "{:04}:{:02}:{:02} {:02}:{:02}:{:02}",
        recorded.year(),
        u8::from(recorded.month()),
        recorded.day(),
        recorded.hour(),
        recorded.minute(),
        recorded.second()
    ));
    let subsec = nul(&format!("{:02}", frame % 100));

    // Layout: header(8) | IFD0 (3 entries) | EXIF IFD (3 entries) | data blob.
    let ifd0_size = 2 + 3 * 12 + 4;
    let exif_ifd_size = 2 + 3 * 12 + 4;
    let ifd0_offset: u32 = 8;
    let exif_ifd_offset: u32 = ifd0_offset + u32::try_from(ifd0_size).unwrap_or(0);
    let data_offset: u32 = exif_ifd_offset + u32::try_from(exif_ifd_size).unwrap_or(0);

    let mut blob = Vec::new();
    let push_value = |bytes: &[u8], blob: &mut Vec<u8>| -> u32 {
        let at = data_offset + u32::try_from(blob.len()).unwrap_or(0);
        blob.extend_from_slice(bytes);
        if blob.len() % 2 == 1 {
            blob.push(0);
        }
        at
    };

    let make_at = push_value(&make, &mut blob);
    let model_at = push_value(&model, &mut blob);
    let datetime_at = push_value(&datetime, &mut blob);
    let subsec_at = push_value(&subsec, &mut blob);
    let serial_at = push_value(&serial, &mut blob);

    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II"); // little endian
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&ifd0_offset.to_le_bytes());

    // IFD0
    tiff.extend_from_slice(&3u16.to_le_bytes());
    write_entry(&mut tiff, TAG_MAKE, TYPE_ASCII, make.len(), make_at);
    write_entry(&mut tiff, TAG_MODEL, TYPE_ASCII, model.len(), model_at);
    write_entry(&mut tiff, TAG_EXIF_IFD, TYPE_LONG, 1, exif_ifd_offset);
    tiff.extend_from_slice(&0u32.to_le_bytes()); // no IFD1

    // EXIF IFD
    tiff.extend_from_slice(&3u16.to_le_bytes());
    write_entry(
        &mut tiff,
        TAG_DATETIME_ORIGINAL,
        TYPE_ASCII,
        datetime.len(),
        datetime_at,
    );
    write_entry(
        &mut tiff,
        TAG_SUBSEC_ORIGINAL,
        TYPE_ASCII,
        subsec.len(),
        subsec_at,
    );
    write_entry(
        &mut tiff,
        TAG_BODY_SERIAL,
        TYPE_ASCII,
        serial.len(),
        serial_at,
    );
    tiff.extend_from_slice(&0u32.to_le_bytes());

    tiff.extend_from_slice(&blob);

    let mut payload = Vec::with_capacity(tiff.len() + 6);
    payload.extend_from_slice(b"Exif\0\0");
    payload.extend_from_slice(&tiff);
    payload
}

fn write_entry(out: &mut Vec<u8>, tag: u16, kind: u16, count: usize, value_or_offset: u32) {
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&u32::try_from(count).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&value_or_offset.to_le_bytes());
}

fn nul(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// An 8x8 grey baseline JPEG scan: quantisation table, frame, Huffman tables and
/// one MCU. Small enough to commit, real enough to open.
const MINIMAL_JPEG_BODY: &[u8] = &[
    0xFF, 0xDB, 0x00, 0x43, 0x00, 0x10, 0x0B, 0x0C, 0x0E, 0x0C, 0x0A, 0x10, 0x0E, 0x0D, 0x0E, 0x12,
    0x11, 0x10, 0x13, 0x18, 0x28, 0x1A, 0x18, 0x16, 0x16, 0x18, 0x31, 0x23, 0x25, 0x1D, 0x28, 0x3A,
    0x33, 0x3D, 0x3C, 0x39, 0x33, 0x38, 0x37, 0x40, 0x48, 0x5C, 0x4E, 0x40, 0x44, 0x57, 0x45, 0x37,
    0x38, 0x50, 0x6D, 0x51, 0x57, 0x5F, 0x62, 0x67, 0x68, 0x67, 0x3E, 0x4D, 0x71, 0x79, 0x70, 0x64,
    0x78, 0x5C, 0x65, 0x67, 0x63, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x08, 0x00, 0x08, 0x01, 0x01,
    0x11, 0x00, 0xFF, 0xC4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F,
    0x00,
];
