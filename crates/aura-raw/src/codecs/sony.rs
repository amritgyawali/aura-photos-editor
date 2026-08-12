//! Sony's ARW2 mosaic: sixteen photosites in sixteen bytes.
//!
//! Sony codes each run of sixteen same-parity photosites as a block: the
//! brightest and darkest of the sixteen are stored exactly at eleven bits each
//! along with their positions, and the remaining fourteen are stored as
//! seven-bit offsets from the darkest, scaled by a shift chosen from the spread.
//! It is a fixed 8 bits per photosite - no entropy coding at all - which is why
//! an ARW is exactly `width * height` bytes and why the format is lossy.
//!
//! Two consequences worth naming, because both show up in real images:
//!
//! * Where a block spans a hard bright edge the shift grows and the fourteen
//!   ordinary samples quantise to steps of up to sixteen code values. This is
//!   the origin of ARW posterisation around specular highlights; it is in the
//!   file, not in this decoder.
//! * The eleven-bit values are *not* linear. Sony applies a five-segment
//!   linearisation curve whose knots live in the encrypted `SR2SubIFD`. When we
//!   can read the knots we apply the body's own curve; when we cannot we apply a
//!   plain linear expansion and say so, rather than inventing a curve.
//!
//! The format is the one documented by the open-source decoders; this is an
//! independent safe implementation of it.

/// Entries in the linearisation curve: one per twelve-bit index.
pub const CURVE_LEN: usize = 4096;

/// Photosites coded by one sixteen-byte block.
const BLOCK: usize = 16;

/// Build the linearisation curve from the four knots Sony stores in tag
/// `0x7010`, or the documented linear fallback when the tag was unreadable.
///
/// The curve is piecewise linear with slopes 1, 2, 4, 8 and 16 between the
/// knots, which is how every open decoder reconstructs it.
#[must_use]
pub fn build_curve(knots: Option<[u16; 4]>) -> Vec<u16> {
    let mut curve = vec![0u16; CURVE_LEN + 1];
    let Some(knots) = knots else {
        // No knots: expand eleven bits to fourteen and nothing else. The caller
        // records this as an approximate linearisation.
        for (index, slot) in curve.iter_mut().enumerate() {
            *slot = u16::try_from(index.saturating_mul(16)).unwrap_or(u16::MAX);
        }
        return curve;
    };

    let bounds = [
        0usize,
        usize::from(knots.first().copied().unwrap_or(0)),
        usize::from(knots.get(1).copied().unwrap_or(0)),
        usize::from(knots.get(2).copied().unwrap_or(0)),
        usize::from(knots.get(3).copied().unwrap_or(0)),
        CURVE_LEN - 1,
    ];
    for segment in 0..5usize {
        let from = bounds.get(segment).copied().unwrap_or(0);
        let to = bounds.get(segment + 1).copied().unwrap_or(0).min(CURVE_LEN);
        let slope = 1u16 << segment;
        for index in (from + 1)..=to {
            let previous = curve.get(index - 1).copied().unwrap_or(0);
            if let Some(slot) = curve.get_mut(index) {
                *slot = previous.saturating_add(slope);
            }
        }
    }
    // Hold the last value out to the end so an out-of-range index saturates
    // rather than reading zero, which would punch black holes into highlights.
    let last = curve.get(CURVE_LEN - 1).copied().unwrap_or(0);
    for index in CURVE_LEN..=CURVE_LEN {
        if let Some(slot) = curve.get_mut(index) {
            *slot = last;
        }
    }
    curve
}

/// Decode an ARW2 mosaic. The buffer holds exactly one byte per photosite.
#[must_use]
pub fn decode(bytes: &[u8], width: usize, height: usize, curve: &[u16]) -> Vec<u16> {
    let mut plane = vec![0u16; width.saturating_mul(height)];
    if width < 32 {
        return plane;
    }

    for row in 0..height {
        let row_start = row * width;
        let mut col = 0usize;
        let mut block = 0usize;
        // The last thirty photosites of a row are padding Sony never codes.
        while col + 30 < width && row_start + block + BLOCK <= bytes.len() + BLOCK {
            let at = row_start + block;
            let byte =
                |offset: usize| -> u32 { u32::from(bytes.get(at + offset).copied().unwrap_or(0)) };
            let header = byte(0) | (byte(1) << 8) | (byte(2) << 16) | (byte(3) << 24);
            let high = (header & 0x7FF) as u16;
            let low = ((header >> 11) & 0x7FF) as u16;
            let index_high = ((header >> 22) & 0x0F) as usize;
            let index_low = ((header >> 26) & 0x0F) as usize;

            let spread = u32::from(high.saturating_sub(low));
            let mut shift = 0u32;
            while shift < 4 && (0x80u32 << shift) <= spread {
                shift += 1;
            }

            let mut bit = 30usize;
            for index in 0..BLOCK {
                let value = if index == index_high {
                    high
                } else if index == index_low {
                    low
                } else {
                    let pair = byte(bit >> 3) | (byte((bit >> 3) + 1) << 8);
                    let field = (pair >> (bit & 7)) & 0x7F;
                    bit += 7;
                    let widened = (field << shift) + u32::from(low);
                    u16::try_from(widened.min(0x7FF)).unwrap_or(0)
                };
                let linear = curve
                    .get(usize::from(value) << 1)
                    .copied()
                    .unwrap_or(value.saturating_mul(8))
                    >> 2;
                if let Some(slot) = plane.get_mut(row_start + col) {
                    *slot = linear;
                }
                col += 2;
            }
            // Sixteen photosites of one parity, then the sixteen between them:
            // the walk steps back thirty-one columns to start the odd pass and
            // one column to move on to the next pair of runs.
            col -= if col % 2 == 1 { 1 } else { 31 };
            block += BLOCK;
        }
    }
    plane
}

/// Encode a mosaic the way a Sony body would, for the fixture writer.
///
/// The encoding is exact when every block spans at most 127 code values after
/// the eleven-bit reduction, which is how the fixtures are laid out; wider
/// blocks quantise, exactly as they do in a real ARW.
#[must_use]
pub fn encode(plane: &[u16], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0u8; width.saturating_mul(height)];
    if width < 32 {
        return out;
    }

    for row in 0..height {
        let row_start = row * width;
        let mut col = 0usize;
        let mut block = 0usize;
        while col + 30 < width {
            let mut samples = [0u16; BLOCK];
            for (index, slot) in samples.iter_mut().enumerate() {
                let source = plane.get(row_start + col + index * 2).copied().unwrap_or(0);
                *slot = (source >> 3).min(0x7FF);
            }

            let mut index_high = 0usize;
            let mut index_low = 0usize;
            for (index, sample) in samples.iter().enumerate() {
                if *sample > samples.get(index_high).copied().unwrap_or(0) {
                    index_high = index;
                }
                if *sample < samples.get(index_low).copied().unwrap_or(0) {
                    index_low = index;
                }
            }
            // The two markers must name different slots even on a flat block.
            if index_high == index_low {
                index_low = usize::from(index_high == 0);
            }
            let high = samples.get(index_high).copied().unwrap_or(0);
            let low = samples.get(index_low).copied().unwrap_or(0);

            let spread = u32::from(high.saturating_sub(low));
            let mut shift = 0u32;
            while shift < 4 && (0x80u32 << shift) <= spread {
                shift += 1;
            }

            let at = row_start + block;
            let header = u32::from(high)
                | (u32::from(low) << 11)
                | ((index_high as u32) << 22)
                | ((index_low as u32) << 26);
            for offset in 0..4usize {
                if let Some(slot) = out.get_mut(at + offset) {
                    *slot = ((header >> (offset * 8)) & 0xFF) as u8;
                }
            }

            let mut bit = 30usize;
            for (index, sample) in samples.iter().enumerate() {
                if index == index_high || index == index_low {
                    continue;
                }
                let field = (u32::from(sample.saturating_sub(low)) >> shift).min(0x7F);
                for step in 0..7usize {
                    if (field >> step) & 1 == 1 {
                        let position = bit + step;
                        if let Some(slot) = out.get_mut(at + (position >> 3)) {
                            *slot |= 1u8 << (position & 7);
                        }
                    }
                }
                bit += 7;
            }

            col += BLOCK * 2;
            col -= if col % 2 == 1 { 1 } else { 31 };
            block += BLOCK;
        }
    }
    out
}
