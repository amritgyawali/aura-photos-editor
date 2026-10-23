//! Lossless JPEG (ITU T.81 annex H), the compression cameras use for sensor data.
//!
//! This is not the JPEG everyone knows. There is no DCT, no quantisation and no
//! colour transform: each sample is predicted from its already-decoded
//! neighbours and only the prediction error is Huffman coded. Canon, Adobe and
//! several others store Bayer mosaics this way, so a RAW decoder that cannot
//! read SOF3 cannot develop a CR2 or a compressed DNG at all.
//!
//! Layout note that trips everyone up once: a Bayer mosaic is normally stored as
//! **two or four components of reduced width**, interleaved one sample at a
//! time. Decoding therefore produces a plane `frame_width * components` wide,
//! and that plane - read as one long run of samples - is the mosaic in sensor
//! order. Canon additionally cuts the run into vertical slices; that is the
//! caller's business, see [`crate::codecs::canon`].
//!
//! Speed matters here because one 30-megapixel CR2 is one scan of thirty
//! million Huffman codes with no way to split it across threads. The stuffed
//! bytes are removed once up front, and codes are resolved through a 12-bit
//! lookup table with a canonical fallback for the rare longer code, instead of
//! one bit at a time.
//!
//! Every read is bounds-checked and the decoder is a pure function of its input,
//! so a hostile file can produce a wrong image or an error, never a panic and
//! never an out-of-bounds read.

use aura_core::errors::raw::{corrupt, too_large};
use aura_core::AuraResult;

/// Ceiling on samples in one lossless frame, checked before allocation.
const MAX_SAMPLES: usize = 512 * 1024 * 1024 / 2;

/// Bits resolved by one table lookup. Codes this long or shorter - almost all of
/// them in a camera file - cost one lookup.
const FAST_BITS: u32 = 12;

/// One decoded lossless frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LosslessImage {
    /// Width of the interleaved plane: frame width multiplied by components.
    pub width: u32,
    /// Height in rows.
    pub height: u32,
    /// How many components the frame declared.
    pub components: usize,
    /// Sample precision in bits, 8 to 16.
    pub precision: u8,
    /// `width * height` samples in row-major order.
    pub samples: Vec<u16>,
}

/// The frame header alone, read without decoding the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Samples per row per component.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Components interleaved in the scan.
    pub components: usize,
    /// Sample precision in bits.
    pub precision: u8,
    /// True when the first component is subsampled - a Canon sRAW or mRAW frame,
    /// which holds luma and chroma rather than a mosaic.
    pub subsampled: bool,
}

#[derive(Debug, Clone)]
struct HuffTable {
    /// `(code length, symbol)` for every `FAST_BITS`-bit prefix; length 0 means
    /// the code is longer than the table and the slow path resolves it.
    fast: Vec<(u8, u8)>,
    /// Largest code of each length, or -1 when the length is unused.
    max_code: [i32; 17],
    /// Code minus this gives the index into `values`, per length.
    offset: [i32; 17],
    /// The symbols, in canonical order.
    values: Vec<u8>,
    /// True once a DHT segment defined this table.
    defined: bool,
}

impl Default for HuffTable {
    fn default() -> Self {
        Self {
            fast: Vec::new(),
            max_code: [-1; 17],
            offset: [0; 17],
            values: Vec::new(),
            defined: false,
        }
    }
}

impl HuffTable {
    fn build(counts: &[u8; 16], values: Vec<u8>) -> Self {
        let mut table = Self {
            fast: vec![(0, 0); 1 << FAST_BITS],
            values,
            defined: true,
            ..Self::default()
        };
        let mut code: i32 = 0;
        let mut index: i32 = 0;
        for length in 1..=16usize {
            let count = i32::from(counts.get(length - 1).copied().unwrap_or(0));
            if let Some(slot) = table.offset.get_mut(length) {
                *slot = index - code;
            }
            if count > 0 {
                for _ in 0..count {
                    if length as u32 <= FAST_BITS {
                        let symbol = table.values.get(index as usize).copied().unwrap_or(0);
                        let shift = FAST_BITS - length as u32;
                        let start = (code as usize) << shift;
                        for prefix in start..start + (1usize << shift) {
                            if let Some(slot) = table.fast.get_mut(prefix) {
                                *slot = (length as u8, symbol);
                            }
                        }
                    }
                    code += 1;
                    index += 1;
                }
                if let Some(slot) = table.max_code.get_mut(length) {
                    *slot = code - 1;
                }
            }
            code <<= 1;
        }
        table
    }
}

/// An MSB-first reader over entropy data whose stuffing has already been removed.
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
    buffer: u64,
    held: u32,
}

impl<'a> Bits<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            buffer: 0,
            held: 0,
        }
    }

    /// Top up to at least 32 bits. Past the end of the data the reader supplies
    /// zero bits, which lets a truncated stream finish as a wrong image rather
    /// than as an error inside the innermost loop.
    #[inline]
    fn fill(&mut self) {
        while self.held <= 56 {
            let byte = self.bytes.get(self.at).copied().unwrap_or(0);
            self.at += 1;
            self.buffer |= u64::from(byte) << (56 - self.held);
            self.held += 8;
        }
    }

    #[inline]
    fn peek(&mut self, count: u32) -> u32 {
        if self.held < count {
            self.fill();
        }
        (self.buffer >> (64 - count)) as u32
    }

    #[inline]
    fn consume(&mut self, count: u32) {
        self.buffer <<= count;
        self.held -= count.min(self.held);
    }

    #[inline]
    fn take(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }
        let value = self.peek(count);
        self.consume(count);
        value
    }

    #[inline]
    fn symbol(&mut self, table: &HuffTable) -> AuraResult<u8> {
        let prefix = self.peek(16);
        if let Some((length, symbol)) = table.fast.get((prefix >> (16 - FAST_BITS)) as usize) {
            if *length > 0 {
                self.consume(u32::from(*length));
                return Ok(*symbol);
            }
        }
        for length in (FAST_BITS as usize + 1)..=16 {
            let code = (prefix >> (16 - length)) as i32;
            let max = table.max_code.get(length).copied().unwrap_or(-1);
            if max >= 0 && code <= max {
                self.consume(length as u32);
                let index = code + table.offset.get(length).copied().unwrap_or(0);
                return table
                    .values
                    .get(index.max(0) as usize)
                    .copied()
                    .ok_or_else(|| corrupt("lossless jpeg huffman symbol is out of range"));
            }
        }
        Err(corrupt("lossless jpeg huffman code matches no table entry"))
    }
}

/// Sign-extend a Huffman magnitude, T.81 figure F.12.
#[inline]
fn extend(value: u32, magnitude: u32) -> i32 {
    if magnitude == 0 {
        return 0;
    }
    let raw = value as i32;
    if raw < (1i32 << (magnitude - 1)) {
        raw - (1i32 << magnitude) + 1
    } else {
        raw
    }
}

/// The entropy-coded data of a scan with its byte stuffing removed, cut at each
/// restart marker. Scanning stops at the first marker that is not a restart.
fn unstuff(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut segments = vec![Vec::with_capacity(bytes.len())];
    let mut at = 0usize;
    while let Some(&byte) = bytes.get(at) {
        if byte != 0xFF {
            if let Some(current) = segments.last_mut() {
                current.push(byte);
            }
            at += 1;
            continue;
        }
        match bytes.get(at + 1).copied() {
            Some(0x00) => {
                if let Some(current) = segments.last_mut() {
                    current.push(0xFF);
                }
                at += 2;
            }
            // Fill bytes before a marker.
            Some(0xFF) => at += 1,
            Some(0xD0..=0xD7) => {
                segments.push(Vec::new());
                at += 2;
            }
            _ => break,
        }
    }
    segments
}

struct Parsed {
    tables: Vec<HuffTable>,
    header: FrameHeader,
    component_tables: Vec<usize>,
    predictor: u8,
    point_transform: u8,
    restart_interval: usize,
    scan_at: usize,
}

/// Walk the markers up to the start of the scan.
#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
fn parse(bytes: &[u8], need_scan: bool) -> AuraResult<Parsed> {
    if !matches!(bytes.get(..2), Some([0xFF, 0xD8])) {
        return Err(corrupt("lossless jpeg does not start with FFD8"));
    }
    let mut tables: Vec<HuffTable> = vec![HuffTable::default(); 4];
    let mut header: Option<FrameHeader> = None;
    let mut component_tables: Vec<usize> = Vec::new();
    let mut predictor = 1u8;
    let mut point_transform = 0u8;
    let mut restart_interval = 0usize;
    let mut scan_at: Option<usize> = None;

    let mut at = 2usize;
    while at + 3 < bytes.len() {
        if bytes.get(at).copied() != Some(0xFF) {
            at += 1;
            continue;
        }
        let marker = bytes.get(at + 1).copied().unwrap_or(0);
        at += 2;
        if matches!(marker, 0xFF | 0x01 | 0xD0..=0xD7 | 0xD8) {
            continue;
        }
        if marker == 0xD9 {
            break;
        }
        let length = usize::from(u16::from_be_bytes([
            bytes.get(at).copied().unwrap_or(0),
            bytes.get(at + 1).copied().unwrap_or(0),
        ]));
        if length < 2 || at + length > bytes.len() {
            return Err(corrupt("lossless jpeg segment length is out of range"));
        }
        let payload = bytes.get(at + 2..at + length).unwrap_or(&[]);

        match marker {
            // DHT - one or more Huffman tables.
            0xC4 => {
                let mut cursor = 0usize;
                while cursor + 17 <= payload.len() {
                    let id = usize::from(payload.get(cursor).copied().unwrap_or(0) & 0x0F);
                    let mut counts = [0u8; 16];
                    let mut total = 0usize;
                    for (index, slot) in counts.iter_mut().enumerate() {
                        *slot = payload.get(cursor + 1 + index).copied().unwrap_or(0);
                        total += usize::from(*slot);
                    }
                    let values_start = cursor + 17;
                    let values = payload
                        .get(values_start..values_start + total)
                        .ok_or_else(|| corrupt("huffman table runs past its segment"))?
                        .to_vec();
                    if let Some(slot) = tables.get_mut(id.min(3)) {
                        *slot = HuffTable::build(&counts, values);
                    }
                    cursor = values_start + total;
                }
            }
            // SOF3 - the lossless frame header.
            0xC3 => {
                let read16 = |index: usize| {
                    u32::from(u16::from_be_bytes([
                        payload.get(index).copied().unwrap_or(0),
                        payload.get(index + 1).copied().unwrap_or(0),
                    ]))
                };
                let count = usize::from(payload.get(5).copied().unwrap_or(0));
                let sampling = payload.get(7).copied().unwrap_or(0x11);
                header = Some(FrameHeader {
                    precision: payload.first().copied().unwrap_or(0),
                    height: read16(1),
                    width: read16(3),
                    components: count,
                    subsampled: (sampling >> 4) * (sampling & 0x0F) > 1,
                });
                component_tables = vec![0; count];
            }
            // Any other start-of-frame is a profile we do not decode here.
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Err(corrupt(format!(
                    "expected a lossless frame, found start-of-frame {marker:#04X}"
                )));
            }
            // DRI - restart interval in MCUs.
            0xDD => {
                restart_interval = usize::from(u16::from_be_bytes([
                    payload.first().copied().unwrap_or(0),
                    payload.get(1).copied().unwrap_or(0),
                ]));
            }
            // SOS - the scan header; entropy data follows immediately.
            0xDA => {
                let count = usize::from(payload.first().copied().unwrap_or(0));
                for index in 0..count {
                    let table = usize::from(payload.get(2 + index * 2).copied().unwrap_or(0) >> 4);
                    if let Some(slot) = component_tables.get_mut(index) {
                        *slot = table.min(3);
                    }
                }
                predictor = payload.get(1 + count * 2).copied().unwrap_or(1);
                point_transform = payload.get(3 + count * 2).copied().unwrap_or(0) & 0x0F;
                scan_at = Some(at + length);
                break;
            }
            _ => {}
        }
        at += length;
        if !need_scan && header.is_some() {
            break;
        }
    }

    let header = header.ok_or_else(|| corrupt("lossless jpeg frame header is missing"))?;
    if header.components == 0 || header.width == 0 || header.height == 0 {
        return Err(corrupt("lossless jpeg frame header is incomplete"));
    }
    if !(2..=16).contains(&header.precision) {
        return Err(corrupt(format!(
            "lossless jpeg declares {}-bit samples",
            header.precision
        )));
    }
    let scan_at = if need_scan {
        scan_at.ok_or_else(|| corrupt("lossless jpeg has no scan"))?
    } else {
        0
    };
    Ok(Parsed {
        tables,
        header,
        component_tables,
        predictor,
        point_transform,
        restart_interval,
        scan_at,
    })
}

/// Read only the frame header, which is all a container walk needs to size the
/// mosaic and tell a Bayer frame from a Canon sRAW one.
///
/// # Errors
///
/// `AURA-RAW-2002` when the stream carries no lossless frame header.
pub fn frame_header(bytes: &[u8]) -> AuraResult<FrameHeader> {
    parse(bytes, false).map(|parsed| parsed.header)
}

/// Decode a complete lossless JPEG stream.
///
/// # Errors
///
/// Returns `AURA-RAW-2002` when the stream is not a lossless frame, when a
/// required header is missing, or when a Huffman code cannot be resolved, and
/// `AURA-RAW-2005` when the declared frame exceeds the sample ceiling.
#[allow(clippy::too_many_lines)] // Keep ordered format decoding and its validation together.
pub fn decode(bytes: &[u8]) -> AuraResult<LosslessImage> {
    let parsed = parse(bytes, true)?;
    let header = parsed.header;
    let components = header.components;
    let width = header
        .width
        .checked_mul(u32::try_from(components).unwrap_or(1))
        .ok_or_else(|| corrupt("lossless jpeg width overflows"))?;
    let stride = width as usize;
    let total = stride
        .checked_mul(header.height as usize)
        .ok_or_else(|| corrupt("lossless jpeg sample count overflows"))?;
    if total > MAX_SAMPLES {
        return Err(too_large(total as u64, MAX_SAMPLES as u64));
    }

    let mut component_tables: Vec<&HuffTable> = Vec::with_capacity(components);
    for index in &parsed.component_tables {
        let table = parsed
            .tables
            .get(*index)
            .filter(|table| table.defined)
            .ok_or_else(|| corrupt("scan names a huffman table that was never defined"))?;
        component_tables.push(table);
    }

    let segments = unstuff(bytes.get(parsed.scan_at..).unwrap_or(&[]));
    let mut segment = 0usize;
    let mut reader = Bits::new(segments.first().map_or(&[][..], Vec::as_slice));

    // The value the first pixel of the image is predicted from, T.81 H.2.1.
    let precision = header.precision;
    let seed = 1i32 << (precision - 1 - parsed.point_transform.min(precision - 1));
    let mut samples = vec![0u16; total];
    let frame_width = header.width as usize;
    let restart = parsed.restart_interval;
    let mut mcus_left = restart;

    for y in 0..header.height as usize {
        // A restart interval that ends on a row boundary starts the next row
        // afresh: new entropy segment, and prediction as if it were row zero.
        let mut fresh = y == 0;
        if restart > 0 && mcus_left == 0 {
            segment += 1;
            reader = Bits::new(segments.get(segment).map_or(&[][..], Vec::as_slice));
            mcus_left = restart;
            fresh = true;
        }
        let (done, rest) = samples.split_at_mut(y * stride);
        let above_row = if fresh {
            None
        } else {
            done.get(done.len().saturating_sub(stride)..)
        };
        let Some(row) = rest.get_mut(..stride) else {
            break;
        };
        for x in 0..frame_width {
            if restart > 0 {
                if mcus_left == 0 {
                    // Mid-row restart: realign on the next segment.
                    segment += 1;
                    reader = Bits::new(segments.get(segment).map_or(&[][..], Vec::as_slice));
                    mcus_left = restart;
                }
                mcus_left -= 1;
            }
            for (index, table) in component_tables.iter().enumerate() {
                let position = x * components + index;
                let magnitude = u32::from(reader.symbol(table)?);
                let difference = if magnitude >= 16 {
                    // T.81 H.1.2.2: SSSS = 16 codes a difference of 32768.
                    32_768i32
                } else {
                    extend(reader.take(magnitude), magnitude)
                };
                let left = if x > 0 {
                    i32::from(row.get(position - components).copied().unwrap_or(0))
                } else {
                    0
                };
                let prediction = match above_row {
                    None => {
                        if x == 0 {
                            seed
                        } else {
                            left
                        }
                    }
                    Some(above) => {
                        let rb = i32::from(above.get(position).copied().unwrap_or(0));
                        if x == 0 {
                            rb
                        } else {
                            let rc =
                                i32::from(above.get(position - components).copied().unwrap_or(0));
                            predict(parsed.predictor, left, rb, rc)
                        }
                    }
                };
                if let Some(slot) = row.get_mut(position) {
                    *slot = ((prediction + difference) & 0xFFFF) as u16;
                }
            }
        }
    }

    Ok(LosslessImage {
        width,
        height: header.height,
        components,
        precision,
        samples,
    })
}

/// The seven predictors of T.81 table H.1, for a sample that is neither in the
/// first row nor the first column.
#[inline]
fn predict(selector: u8, ra: i32, rb: i32, rc: i32) -> i32 {
    match selector {
        2 => rb,
        3 => rc,
        4 => ra + rb - rc,
        5 => ra + ((rb - rc) >> 1),
        6 => rb + ((ra - rc) >> 1),
        7 => (ra + rb) >> 1,
        // Selector 1, and 0 ("no prediction", hierarchical mode only), predict
        // from the left as the reference decoders do.
        _ => ra,
    }
}
