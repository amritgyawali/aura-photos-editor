//! Synthetic RAW files, so the decoder can be tested without a camera.
//!
//! Real wedding RAWs cannot be committed: they are other people's faces, and a
//! single CR2 is thirty megabytes. So the test corpus is generated - complete,
//! valid DNG-shaped files containing a synthetic colour chart, written by this
//! module and read back by the decoder.
//!
//! The generator is the other half of the golden suite's argument. Because the
//! same colour matrix is used to *write* the mosaic and to *read* it, a correct
//! pipeline reproduces the chart exactly, and any drift in the matrix chain, the
//! white balance convention, the demosaic or the curve shows up immediately as a
//! dE2000 above tolerance. A fixture that could not fail would prove nothing.
//!
//! Everything here is deterministic: same call, same bytes, byte for byte.

use std::path::{Path, PathBuf};

use crate::colour::curve::srgb_decode;
use crate::colour::matrix::{apply, bradford, Mat3, SRGB_TO_XYZ_D65, WHITE_D50, WHITE_D65};
use crate::colour::profile;
use crate::container::tiff::{compression, tag};

/// Patches across the synthetic chart.
pub const CHART_COLUMNS: u32 = 6;

/// Patch rows down the synthetic chart.
pub const CHART_ROWS: u32 = 4;

/// Sensor pixels along one edge of a patch. A multiple of four, so a patch stays
/// aligned after half-size demosaic and its interior is never mixed with a
/// neighbour.
pub const PATCH_EDGE: u32 = 64;

/// Black level written into every fixture.
pub const BLACK_LEVEL: u32 = 1_024;

/// White level written into every fixture: a 14-bit sensor.
pub const WHITE_LEVEL: u32 = 16_383;

/// As-shot neutral written into every fixture, as multipliers. Deliberately not
/// `[1, 1, 1]`, so a pipeline that ignores white balance fails the golden test.
pub const WHITE_BALANCE: [f64; 3] = [2.0, 1.0, 1.5];

/// How far each chart patch is pulled towards its own luminance.
///
/// A fully saturated ColorChecker blue sits outside some of the bench bodies'
/// gamuts, and a fixture that clips before it is even written cannot prove
/// anything about the pipeline. Desaturating by a documented, fixed amount keeps
/// every patch representable on every body while leaving the chart a genuine
/// colour test.
pub const CHART_DESATURATION: f64 = 0.8;

/// How the mosaic is stored in a generated fixture.
///
/// Each proprietary variant is written by the same encoder the corresponding
/// decoder is tested against, which is what makes the round trip a real test:
/// the encoder walks its format's state machine forwards, the decoder walks it
/// backwards, and the two share nothing but the published constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MosaicEncoding {
    /// Sixteen bits per sample, unpacked.
    Unpacked16,
    /// Fourteen bits per sample, packed MSB first.
    Packed14,
    /// Lossless JPEG, the way DNG and CR2 store a mosaic.
    LosslessJpeg,
    /// Sixteen bits per sample over a 6x6 X-Trans array, the way Adobe's
    /// converter writes a Fujifilm file.
    XTrans16,
    /// Nikon's Huffman coding, with a decode table in a synthetic MakerNote.
    NikonCompressed,
    /// Sony's ARW2 block coding. Lossy by construction: samples are reduced to
    /// eleven bits, so a fixture round trip is exact only to a multiple of
    /// eight code values.
    SonyArw2,
    /// Olympus's adaptive predictive coding, in an ORF-magic container.
    OlympusCompressed,
}

impl MosaicEncoding {
    const fn bits(self) -> u16 {
        match self {
            Self::Unpacked16 | Self::XTrans16 => 16,
            Self::Packed14
            | Self::LosslessJpeg
            | Self::NikonCompressed
            | Self::SonyArw2
            | Self::OlympusCompressed => 14,
        }
    }

    const fn tiff_compression(self) -> u16 {
        match self {
            Self::Unpacked16 | Self::Packed14 | Self::XTrans16 | Self::OlympusCompressed => {
                compression::NONE
            }
            Self::LosslessJpeg => compression::JPEG,
            Self::NikonCompressed => 34_713,
            Self::SonyArw2 => 32_767,
        }
    }

    /// The container magic to write. Olympus needs its own, because "this file
    /// is uncompressed but too small to be" is only a safe inference for a
    /// container that identifies itself as an ORF.
    const fn container_magic(self) -> u16 {
        match self {
            Self::OlympusCompressed => 0x4F52,
            _ => 42,
        }
    }

    /// The colour filter array this encoding lays the chart out on.
    #[must_use]
    pub const fn cfa(self) -> crate::meta::CfaPattern {
        match self {
            Self::XTrans16 => crate::meta::CfaPattern::XTrans(crate::meta::XTRANS_DEFAULT),
            _ => crate::meta::CfaPattern::Rggb,
        }
    }

    /// The patch edge has to be a whole number of colour-array periods, or a
    /// patch starts on a different phase from its neighbour and the binned
    /// proxy mixes two patches in one pixel.
    const fn edge_multiple(self) -> u32 {
        match self {
            Self::XTrans16 => 6,
            _ => 2,
        }
    }

    /// Stable text for fixture file names.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unpacked16 => "unpacked16",
            Self::Packed14 => "packed14",
            Self::LosslessJpeg => "lossless",
            Self::XTrans16 => "xtrans16",
            Self::NikonCompressed => "nikon",
            Self::SonyArw2 => "arw2",
            Self::OlympusCompressed => "olympus",
        }
    }
}

/// What to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixtureOptions {
    /// Which bench body, 1 to 8.
    pub body: usize,
    /// How to store the mosaic.
    pub encoding: MosaicEncoding,
    /// Whether to embed a JPEG preview.
    pub with_preview: bool,
    /// EXIF orientation to declare, 1 to 8.
    pub orientation: u16,
    /// Sensor pixels along one edge of a patch.
    ///
    /// The default keeps fixtures small enough for CI. Raising it is how the
    /// performance lane measures a realistic frame: 24 patches at 1024 px each
    /// is a 6 x 4 chart of 6144 x 4096 photosites, which is a 25 MP sensor.
    pub patch_edge: u32,
    /// Whether to write the colour matrix into the file.
    ///
    /// Turning it off is how the bundled-table and generic-fallback paths get
    /// tested: without a matrix in the file, profile resolution has to find the
    /// body another way or admit that it cannot.
    pub with_colour_matrix: bool,
}

impl Default for FixtureOptions {
    fn default() -> Self {
        Self {
            body: 1,
            encoding: MosaicEncoding::Unpacked16,
            with_preview: true,
            orientation: 1,
            patch_edge: PATCH_EDGE,
            with_colour_matrix: true,
        }
    }
}

/// A generated fixture and everything a test needs to check it.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchFixture {
    /// Where the file was written.
    pub path: PathBuf,
    /// Mosaic width in photosites.
    pub width: u32,
    /// Mosaic height in photosites.
    pub height: u32,
    /// The body's model string, which is also its profile key.
    pub model: String,
    /// Sensor pixels along one edge of a patch in this fixture.
    pub patch_edge: u32,
    /// Linear sRGB of each patch, in reading order. The reference the golden
    /// suite compares a rendered proxy against.
    pub expected_linear_srgb: Vec<[f64; 3]>,
}

impl BenchFixture {
    /// Where the centre of patch `index` lands in an image scaled by `divisor`
    /// relative to the mosaic. Half-size demosaic uses a divisor of 2.
    #[must_use]
    pub fn patch_centre(&self, index: usize, divisor: u32) -> (u32, u32) {
        let column = index as u32 % CHART_COLUMNS;
        let row = index as u32 / CHART_COLUMNS;
        let edge = self.patch_edge / divisor.max(1);
        (column * edge + edge / 2, row * edge + edge / 2)
    }

    /// How many patches the chart has.
    #[must_use]
    pub fn patch_count(&self) -> usize {
        (CHART_COLUMNS * CHART_ROWS) as usize
    }
}

/// The model string of bench body `index`, 1 to 8.
#[must_use]
pub fn bench_model(index: usize) -> String {
    format!("Bench-{:02}", index.clamp(1, 8))
}

/// The synthetic chart, as sRGB display values.
///
/// The hues follow a ColorChecker's layout - skin tones, sky, foliage, primaries
/// and a neutral ramp - because those are the colours a wedding actually
/// contains and the ones a colour error is noticed in first. The values are
/// AURA's own, not a licensed chart's measurements.
const CHART_SRGB: [[f64; 3]; 24] = [
    [115.0, 82.0, 68.0],
    [194.0, 150.0, 130.0],
    [98.0, 122.0, 157.0],
    [87.0, 108.0, 67.0],
    [133.0, 128.0, 177.0],
    [103.0, 189.0, 170.0],
    [214.0, 126.0, 44.0],
    [80.0, 91.0, 166.0],
    [193.0, 90.0, 99.0],
    [94.0, 60.0, 108.0],
    [157.0, 188.0, 64.0],
    [224.0, 163.0, 46.0],
    [56.0, 61.0, 150.0],
    [70.0, 148.0, 73.0],
    [175.0, 54.0, 60.0],
    [231.0, 199.0, 31.0],
    [187.0, 86.0, 149.0],
    [8.0, 133.0, 161.0],
    [243.0, 243.0, 242.0],
    [200.0, 200.0, 200.0],
    [160.0, 160.0, 160.0],
    [122.0, 122.0, 122.0],
    [85.0, 85.0, 85.0],
    [52.0, 52.0, 52.0],
];

/// The chart in linear sRGB, desaturated by [`CHART_DESATURATION`].
#[must_use]
pub fn chart_linear_srgb() -> Vec<[f64; 3]> {
    CHART_SRGB
        .iter()
        .map(|patch| {
            let linear = [
                f64::from(srgb_decode(
                    (patch.first().copied().unwrap_or(0.0) / 255.0) as f32,
                )),
                f64::from(srgb_decode(
                    (patch.get(1).copied().unwrap_or(0.0) / 255.0) as f32,
                )),
                f64::from(srgb_decode(
                    (patch.get(2).copied().unwrap_or(0.0) / 255.0) as f32,
                )),
            ];
            // Rec.709 luminance, the axis to pull towards.
            let luma = 0.212_672_9 * linear[0] + 0.715_152_2 * linear[1] + 0.072_175_0 * linear[2];
            [
                luma + (linear[0] - luma) * CHART_DESATURATION,
                luma + (linear[1] - luma) * CHART_DESATURATION,
                luma + (linear[2] - luma) * CHART_DESATURATION,
            ]
        })
        .collect()
}

/// Write one synthetic RAW.
///
/// # Errors
///
/// Returns the IO failure as text; this is developer tooling, and the caller is
/// a test or the fixture command.
pub fn write_bench_raw(path: &Path, options: FixtureOptions) -> Result<BenchFixture, String> {
    let model = bench_model(options.body);
    let profile = profile::lookup(profile::BENCH_MAKE, &model)
        .ok_or_else(|| format!("no bundled profile for bench body {model}"))?;

    let chart = chart_linear_srgb();
    // A patch edge has to be a whole number of colour-array periods, or a
    // binned demosaic straddles the boundary between two patches and every
    // measurement is polluted.
    let multiple = options.encoding.edge_multiple();
    let edge = (options.patch_edge.max(multiple * 2) / multiple) * multiple;
    let width = CHART_COLUMNS * edge;
    let height = CHART_ROWS * edge;
    let mosaic = build_mosaic(
        &chart,
        profile.xyz_to_camera,
        width,
        height,
        edge,
        options.encoding.cfa(),
    );

    let preview = if options.with_preview {
        Some(build_preview(&chart, width, height, edge)?)
    } else {
        None
    };

    let bytes = write_tiff(
        &mosaic,
        width,
        height,
        &profile,
        preview.as_deref(),
        options,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;

    Ok(BenchFixture {
        path: path.to_path_buf(),
        width,
        height,
        model,
        patch_edge: edge,
        expected_linear_srgb: chart,
    })
}

/// Write all eight bench bodies into `dir`, one file each, unpacked mosaics.
///
/// # Errors
///
/// The first IO failure, as text.
pub fn write_bench_set(dir: &Path) -> Result<Vec<BenchFixture>, String> {
    let mut out = Vec::new();
    for body in 1..=8usize {
        let options = FixtureOptions {
            body,
            ..FixtureOptions::default()
        };
        let path = dir.join(format!("{}.dng", bench_model(body).to_lowercase()));
        out.push(write_bench_raw(&path, options)?);
    }
    Ok(out)
}

/// Raw code values of the chart, laid out on the given colour filter array.
fn build_mosaic(
    chart: &[[f64; 3]],
    xyz_to_camera: Mat3,
    width: u32,
    height: u32,
    edge: u32,
    cfa: crate::meta::CfaPattern,
) -> Vec<u16> {
    let adapt = bradford(WHITE_D65, WHITE_D50);
    let span = f64::from(WHITE_LEVEL - BLACK_LEVEL);
    let mut camera_patches = Vec::with_capacity(chart.len());

    for patch in chart {
        let xyz_d65 = apply(SRGB_TO_XYZ_D65, *patch);
        let xyz_d50 = apply(adapt, xyz_d65);
        let native = apply(xyz_to_camera, xyz_d50);
        // Undo the white balance the pipeline will apply, so the round trip is
        // exact and a pipeline that forgets the multipliers fails.
        let mut codes = [0u16; 3];
        for channel in 0..3usize {
            let balanced = native.get(channel).copied().unwrap_or(0.0)
                / WHITE_BALANCE.get(channel).copied().unwrap_or(1.0);
            let clamped = balanced.clamp(0.0, 1.0);
            if let Some(slot) = codes.get_mut(channel) {
                *slot = (f64::from(BLACK_LEVEL) + clamped * span).round() as u16;
            }
        }
        camera_patches.push(codes);
    }

    let mut mosaic = vec![0u16; width as usize * height as usize];
    for y in 0..height {
        for x in 0..width {
            let column = x / edge;
            let row = y / edge;
            let index = (row * CHART_COLUMNS + column) as usize;
            let codes = camera_patches.get(index).copied().unwrap_or([0; 3]);
            let colour = usize::from(cfa.colour_at(x, y));
            if let Some(slot) = mosaic.get_mut(y as usize * width as usize + x as usize) {
                *slot = codes.get(colour).copied().unwrap_or(0);
            }
        }
    }
    mosaic
}

/// A small JPEG preview of the same chart, rendered the way a camera would.
fn build_preview(
    chart: &[[f64; 3]],
    width: u32,
    height: u32,
    edge: u32,
) -> Result<Vec<u8>, String> {
    let preview_width = width / 2;
    let preview_height = height / 2;
    let mut pixels = Vec::with_capacity(preview_width as usize * preview_height as usize * 3);
    for y in 0..preview_height {
        for x in 0..preview_width {
            let column = x / (edge / 2);
            let row = y / (edge / 2);
            let index = (row * CHART_COLUMNS + column) as usize;
            let patch = chart.get(index).copied().unwrap_or([0.0; 3]);
            for channel in 0..3usize {
                let linear = patch.get(channel).copied().unwrap_or(0.0) as f32;
                pixels.push(crate::colour::curve::quantise_u8(
                    crate::colour::curve::srgb_encode(linear),
                ));
            }
        }
    }
    crate::codec::encode_jpeg(
        &crate::codec::Rgb8 {
            width: preview_width,
            height: preview_height,
            data: pixels,
        },
        92,
    )
    .map_err(|e| e.detail)
}

// ---------------------------------------------------------------------------
// TIFF writing
// ---------------------------------------------------------------------------

struct WriteEntry {
    tag: u16,
    kind: u16,
    count: u32,
    payload: Vec<u8>,
}

fn short(tag: u16, value: u16) -> WriteEntry {
    WriteEntry {
        tag,
        kind: 3,
        count: 1,
        payload: value.to_le_bytes().to_vec(),
    }
}

fn shorts(tag: u16, values: &[u16]) -> WriteEntry {
    let mut payload = Vec::with_capacity(values.len() * 2);
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    WriteEntry {
        tag,
        kind: 3,
        count: u32::try_from(values.len()).unwrap_or(0),
        payload,
    }
}

fn long(tag: u16, value: u32) -> WriteEntry {
    WriteEntry {
        tag,
        kind: 4,
        count: 1,
        payload: value.to_le_bytes().to_vec(),
    }
}

fn ascii(tag: u16, value: &str) -> WriteEntry {
    let mut payload = value.as_bytes().to_vec();
    payload.push(0);
    WriteEntry {
        tag,
        kind: 2,
        count: u32::try_from(payload.len()).unwrap_or(0),
        payload,
    }
}

fn bytes_entry(tag: u16, kind: u16, values: &[u8]) -> WriteEntry {
    WriteEntry {
        tag,
        kind,
        count: u32::try_from(values.len()).unwrap_or(0),
        payload: values.to_vec(),
    }
}

/// Nine signed rationals, DNG's encoding for a colour matrix.
fn srational_matrix(tag: u16, matrix: Mat3) -> WriteEntry {
    const DENOMINATOR: i32 = 1_000_000;
    let mut payload = Vec::with_capacity(9 * 8);
    for row in 0..3usize {
        for column in 0..3usize {
            let value = matrix
                .get(row)
                .and_then(|r| r.get(column))
                .copied()
                .unwrap_or(0.0);
            let numerator = (value * f64::from(DENOMINATOR)).round() as i32;
            payload.extend_from_slice(&numerator.to_le_bytes());
            payload.extend_from_slice(&DENOMINATOR.to_le_bytes());
        }
    }
    WriteEntry {
        tag,
        kind: 10,
        count: 9,
        payload,
    }
}

/// Three unsigned rationals: the camera-native reading of a neutral subject,
/// which is the reciprocal of the multipliers the pipeline applies.
fn as_shot_neutral(tag: u16) -> WriteEntry {
    const DENOMINATOR: u32 = 1_000_000;
    let mut payload = Vec::with_capacity(3 * 8);
    for multiplier in WHITE_BALANCE {
        let neutral = if multiplier.abs() > 1e-9 {
            1.0 / multiplier
        } else {
            1.0
        };
        let numerator = (neutral * f64::from(DENOMINATOR)).round() as u32;
        payload.extend_from_slice(&numerator.to_le_bytes());
        payload.extend_from_slice(&DENOMINATOR.to_le_bytes());
    }
    WriteEntry {
        tag,
        kind: 5,
        count: 3,
        payload,
    }
}

fn serialise_ifd(
    entries: &mut [WriteEntry],
    ifd_offset: u32,
    data_offset: u32,
) -> (Vec<u8>, Vec<u8>) {
    entries.sort_by_key(|entry| entry.tag);
    let _ = ifd_offset;
    let mut directory = Vec::with_capacity(2 + entries.len() * 12 + 4);
    let mut blob: Vec<u8> = Vec::new();
    directory.extend_from_slice(&u16::try_from(entries.len()).unwrap_or(0).to_le_bytes());

    for entry in entries.iter() {
        directory.extend_from_slice(&entry.tag.to_le_bytes());
        directory.extend_from_slice(&entry.kind.to_le_bytes());
        directory.extend_from_slice(&entry.count.to_le_bytes());
        if entry.payload.len() <= 4 {
            let mut inline = [0u8; 4];
            for (slot, byte) in inline.iter_mut().zip(entry.payload.iter()) {
                *slot = *byte;
            }
            directory.extend_from_slice(&inline);
        } else {
            let at = data_offset + u32::try_from(blob.len()).unwrap_or(0);
            directory.extend_from_slice(&at.to_le_bytes());
            blob.extend_from_slice(&entry.payload);
            if blob.len() % 2 == 1 {
                blob.push(0);
            }
        }
    }
    directory.extend_from_slice(&0u32.to_le_bytes());
    (directory, blob)
}

fn payload_size(entries: &[WriteEntry]) -> usize {
    entries
        .iter()
        .filter(|entry| entry.payload.len() > 4)
        .map(|entry| entry.payload.len() + entry.payload.len() % 2)
        .sum()
}

fn write_tiff(
    mosaic: &[u16],
    width: u32,
    height: u32,
    profile: &profile::CameraProfile,
    preview: Option<&[u8]>,
    options: FixtureOptions,
) -> Vec<u8> {
    let mosaic_bytes = encode_mosaic(mosaic, width, height, options.encoding);

    // Two passes: build the entries so the directory sizes are known, compute
    // where the big payloads land, then rewrite the entries that point at them.
    let build_ifd0 = |jpeg_at: u32, jpeg_len: u32, sub_ifd: u32| -> Vec<WriteEntry> {
        let mut entries = vec![
            ascii(tag::MAKE, profile::BENCH_MAKE),
            ascii(tag::MODEL, &profile.model),
            short(tag::ORIENTATION, options.orientation),
            long(tag::SUB_IFDS, sub_ifd),
            bytes_entry(tag::DNG_VERSION, 1, &[1, 4, 0, 0]),
            as_shot_neutral(tag::AS_SHOT_NEUTRAL),
        ];
        if options.with_colour_matrix {
            entries.push(srational_matrix(tag::COLOR_MATRIX_2, profile.xyz_to_camera));
        }
        if preview.is_some() {
            entries.push(long(tag::JPEG_INTERCHANGE_FORMAT, jpeg_at));
            entries.push(long(tag::JPEG_INTERCHANGE_LENGTH, jpeg_len));
        }
        if options.encoding == MosaicEncoding::NikonCompressed {
            entries.push(bytes_entry(tag::MAKER_NOTE, 7, &nikon_makernote()));
        }
        entries
    };
    let build_ifd1 = |strip_at: u32, strip_len: u32| -> Vec<WriteEntry> {
        let (repeat, pattern) = match options.encoding.cfa() {
            crate::meta::CfaPattern::XTrans(layout) => (6u16, layout.to_vec()),
            _ => (2u16, vec![0, 1, 1, 2]),
        };
        vec![
            long(tag::NEW_SUBFILE_TYPE, 0),
            long(tag::IMAGE_WIDTH, width),
            long(tag::IMAGE_LENGTH, height),
            short(tag::BITS_PER_SAMPLE, options.encoding.bits()),
            short(tag::COMPRESSION, options.encoding.tiff_compression()),
            short(tag::PHOTOMETRIC, 32_803),
            long(tag::STRIP_OFFSETS, strip_at),
            short(tag::SAMPLES_PER_PIXEL, 1),
            long(tag::ROWS_PER_STRIP, height),
            long(tag::STRIP_BYTE_COUNTS, strip_len),
            shorts(tag::CFA_REPEAT_DIM, &[repeat, repeat]),
            bytes_entry(tag::CFA_PATTERN, 1, &pattern),
            long(tag::BLACK_LEVEL, BLACK_LEVEL),
            long(tag::WHITE_LEVEL, WHITE_LEVEL),
        ]
    };

    let probe0 = build_ifd0(0, 0, 0);
    let probe1 = build_ifd1(0, 0);
    let ifd0_offset = 8u32;
    let ifd0_size = 2 + 12 * probe0.len() + 4;
    let ifd1_offset = ifd0_offset + u32::try_from(ifd0_size).unwrap_or(0);
    let ifd1_size = 2 + 12 * probe1.len() + 4;
    let data_offset = ifd1_offset + u32::try_from(ifd1_size).unwrap_or(0);
    let blob0_size = payload_size(&probe0);
    let blob1_size = payload_size(&probe1);
    let jpeg_at = data_offset + u32::try_from(blob0_size + blob1_size).unwrap_or(0);
    let jpeg_len = u32::try_from(preview.map_or(0, <[u8]>::len)).unwrap_or(0);
    let strip_at = jpeg_at + jpeg_len;
    let strip_len = u32::try_from(mosaic_bytes.len()).unwrap_or(0);

    let mut ifd0 = build_ifd0(jpeg_at, jpeg_len, ifd1_offset);
    let mut ifd1 = build_ifd1(strip_at, strip_len);
    let (directory0, blob0) = serialise_ifd(&mut ifd0, ifd0_offset, data_offset);
    let (directory1, blob1) = serialise_ifd(
        &mut ifd1,
        ifd1_offset,
        data_offset + u32::try_from(blob0.len()).unwrap_or(0),
    );

    let mut out = Vec::with_capacity(strip_at as usize + mosaic_bytes.len());
    out.extend_from_slice(b"II");
    out.extend_from_slice(&options.encoding.container_magic().to_le_bytes());
    out.extend_from_slice(&ifd0_offset.to_le_bytes());
    out.extend_from_slice(&directory0);
    out.extend_from_slice(&directory1);
    out.extend_from_slice(&blob0);
    out.extend_from_slice(&blob1);
    if let Some(jpeg) = preview {
        out.extend_from_slice(jpeg);
    }
    out.extend_from_slice(&mosaic_bytes);
    out
}

fn encode_mosaic(mosaic: &[u16], width: u32, height: u32, encoding: MosaicEncoding) -> Vec<u8> {
    match encoding {
        MosaicEncoding::Packed14 => {
            let row_bytes = (width as usize * 14).div_ceil(8);
            let mut out = vec![0u8; row_bytes * height as usize];
            for y in 0..height as usize {
                let mut accumulator: u32 = 0;
                let mut held: u32 = 0;
                let mut cursor = y * row_bytes;
                for x in 0..width as usize {
                    let value = u32::from(
                        mosaic
                            .get(y * width as usize + x)
                            .copied()
                            .unwrap_or(0)
                            .min(0x3FFF),
                    );
                    accumulator = (accumulator << 14) | value;
                    held += 14;
                    while held >= 8 {
                        let shift = held - 8;
                        if let Some(slot) = out.get_mut(cursor) {
                            *slot = u8::try_from((accumulator >> shift) & 0xFF).unwrap_or(0);
                        }
                        cursor += 1;
                        held -= 8;
                        accumulator &= (1u32 << held).saturating_sub(1);
                    }
                }
                if held > 0 {
                    if let Some(slot) = out.get_mut(cursor) {
                        *slot = u8::try_from((accumulator << (8 - held)) & 0xFF).unwrap_or(0);
                    }
                }
            }
            out
        }
        MosaicEncoding::Unpacked16 | MosaicEncoding::XTrans16 => {
            let mut out = Vec::with_capacity(mosaic.len() * 2);
            for sample in mosaic {
                out.extend_from_slice(&sample.to_le_bytes());
            }
            out
        }
        MosaicEncoding::LosslessJpeg => encode_lossless_jpeg(mosaic, width, height, 14),
        // Fourteen-bit lossless, which is what the version byte in the
        // synthetic MakerNote below declares.
        MosaicEncoding::NikonCompressed => {
            crate::codecs::nikon::encode(mosaic, width as usize, height as usize, 5)
        }
        MosaicEncoding::SonyArw2 => {
            crate::codecs::sony::encode(mosaic, width as usize, height as usize)
        }
        MosaicEncoding::OlympusCompressed => {
            crate::codecs::olympus::encode(mosaic, width as usize, height as usize)
        }
    }
}

/// What an ARW2 round trip returns for a given target sample.
///
/// Sony reduces every photosite to eleven bits, so a fixture cannot expect its
/// own values back. This is the exact value the decoder will produce, which is
/// what the round-trip test compares against.
#[must_use]
pub const fn arw2_quantise(sample: u16) -> u16 {
    (sample >> 3) << 3
}

/// A synthetic Nikon MakerNote carrying a decode table.
///
/// Version `0x46` means "lossless, no linearisation curve", which is the one
/// variant whose curve is the identity - so the fixture's code values survive
/// the round trip unchanged and the colour chart stays a colour test rather
/// than a test of a curve we made up.
fn nikon_makernote() -> Vec<u8> {
    let mut table = Vec::with_capacity(12);
    table.push(0x46);
    table.push(0x30);
    // Four seed predictors, all zero: the encoder starts from zero too.
    for _ in 0..4 {
        table.extend_from_slice(&0u16.to_le_bytes());
    }
    // Curve size zero: there is no curve.
    table.extend_from_slice(&0u16.to_le_bytes());

    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(b"Nikon\0");
    out.extend_from_slice(&[0x02, 0x10, 0x00, 0x00]);
    // From here the block is a complete little-endian TIFF of its own, and
    // every offset inside it is relative to this point.
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0x0096u16.to_le_bytes());
    out.extend_from_slice(&7u16.to_le_bytes());
    out.extend_from_slice(&u32::try_from(table.len()).unwrap_or(0).to_le_bytes());
    // Header (8) + entry count (2) + one entry (12) + next pointer (4) = 26.
    out.extend_from_slice(&26u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&table);
    out
}

// ---------------------------------------------------------------------------
// A minimal lossless JPEG encoder, so the SOF3 decoder has something to read
// ---------------------------------------------------------------------------

/// Code lengths for the seventeen magnitude categories.
///
/// One two-bit code for "no difference" and sixteen five-bit codes for the
/// rest. The Kraft sum is 0.75, so the table is a valid prefix code, and the
/// canonical construction matches the decoder's exactly.
const HUFFMAN_LENGTHS: [u8; 17] = [2, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5];

struct BitWriter {
    out: Vec<u8>,
    accumulator: u32,
    held: u32,
}

impl BitWriter {
    const fn new() -> Self {
        Self {
            out: Vec::new(),
            accumulator: 0,
            held: 0,
        }
    }

    fn write(&mut self, value: u32, bits: u32) {
        for index in (0..bits).rev() {
            let bit = (value >> index) & 1;
            self.accumulator = (self.accumulator << 1) | bit;
            self.held += 1;
            if self.held == 8 {
                let byte = u8::try_from(self.accumulator & 0xFF).unwrap_or(0);
                self.out.push(byte);
                // Byte stuffing: an FF in entropy data is followed by 00.
                if byte == 0xFF {
                    self.out.push(0x00);
                }
                self.accumulator = 0;
                self.held = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.held > 0 {
            let padding = 8 - self.held;
            self.write(u32::MAX >> (32 - padding), padding);
        }
        self.out
    }
}

/// Canonical Huffman codes from the fixed length table.
fn canonical_codes() -> Vec<(u32, u32)> {
    let mut codes = vec![(0u32, 0u32); HUFFMAN_LENGTHS.len()];
    let mut code = 0u32;
    for length in 1..=16u32 {
        for (symbol, symbol_length) in HUFFMAN_LENGTHS.iter().enumerate() {
            if u32::from(*symbol_length) == length {
                if let Some(slot) = codes.get_mut(symbol) {
                    *slot = (code, length);
                }
                code += 1;
            }
        }
        code <<= 1;
    }
    codes
}

/// The magnitude category of a difference, JPEG's SSSS.
fn category(difference: i32) -> u32 {
    let magnitude = difference.unsigned_abs();
    32 - magnitude.leading_zeros()
}

fn encode_lossless_jpeg(mosaic: &[u16], width: u32, height: u32, precision: u8) -> Vec<u8> {
    let codes = canonical_codes();
    let mut writer = BitWriter::new();
    let seed = 1i32 << (precision - 1);

    for y in 0..height as usize {
        for x in 0..width as usize {
            let position = y * width as usize + x;
            let value = i32::from(mosaic.get(position).copied().unwrap_or(0));
            let left = if x > 0 {
                i32::from(mosaic.get(position - 1).copied().unwrap_or(0))
            } else {
                0
            };
            let above = if y > 0 {
                i32::from(mosaic.get(position - width as usize).copied().unwrap_or(0))
            } else {
                0
            };
            // Predictor 1 with the same edge rules the decoder applies.
            let prediction = if x == 0 {
                if y == 0 {
                    seed
                } else {
                    above
                }
            } else {
                left
            };

            let difference = value - prediction;
            let magnitude = category(difference);
            let (code, length) = codes.get(magnitude as usize).copied().unwrap_or((0, 0));
            writer.write(code, length);
            if magnitude > 0 {
                let encoded = if difference < 0 {
                    (difference + (1i32 << magnitude) - 1) as u32
                } else {
                    difference as u32
                };
                writer.write(encoded, magnitude);
            }
        }
    }

    let scan = writer.finish();
    let mut out = Vec::with_capacity(scan.len() + 64);
    out.extend_from_slice(&[0xFF, 0xD8]);

    // DHT: class 0, table 0.
    let mut counts = [0u8; 16];
    for length in &HUFFMAN_LENGTHS {
        if let Some(slot) = counts.get_mut(usize::from(*length).saturating_sub(1)) {
            *slot += 1;
        }
    }
    let mut symbols: Vec<u8> = Vec::new();
    for length in 1..=16u8 {
        for (symbol, symbol_length) in HUFFMAN_LENGTHS.iter().enumerate() {
            if *symbol_length == length {
                symbols.push(u8::try_from(symbol).unwrap_or(0));
            }
        }
    }
    let dht_len = 2 + 1 + 16 + symbols.len();
    out.extend_from_slice(&[0xFF, 0xC4]);
    out.extend_from_slice(&u16::try_from(dht_len).unwrap_or(0).to_be_bytes());
    out.push(0x00);
    out.extend_from_slice(&counts);
    out.extend_from_slice(&symbols);

    // SOF3: one component, no subsampling.
    out.extend_from_slice(&[0xFF, 0xC3]);
    out.extend_from_slice(&11u16.to_be_bytes());
    out.push(precision);
    out.extend_from_slice(&u16::try_from(height).unwrap_or(0).to_be_bytes());
    out.extend_from_slice(&u16::try_from(width).unwrap_or(0).to_be_bytes());
    out.push(1);
    out.extend_from_slice(&[1, 0x11, 0]);

    // SOS: component 1, table 0, predictor 1, point transform 0.
    out.extend_from_slice(&[0xFF, 0xDA]);
    out.extend_from_slice(&8u16.to_be_bytes());
    out.push(1);
    out.extend_from_slice(&[1, 0x00]);
    out.extend_from_slice(&[1, 0, 0]);

    out.extend_from_slice(&scan);
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}
