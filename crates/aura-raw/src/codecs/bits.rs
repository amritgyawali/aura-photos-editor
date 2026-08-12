//! One MSB-first bit reader, shared by every proprietary mosaic codec.
//!
//! Nikon, Sony and Olympus all pack their entropy-coded data the same way at the
//! bit level - most significant bit first, no byte stuffing, no markers - and
//! they only differ in what the bits mean. Sharing one reader means the
//! bounds-checking is written once and audited once, which matters because this
//! is the code that meets a stranger's memory card.
//!
//! Reading past the end of the buffer yields zero bits rather than an error. A
//! truncated file then finishes as a visibly wrong image, which the caller can
//! detect and report, instead of failing inside the innermost loop of a decoder
//! that has already produced most of a usable frame.

/// A most-significant-bit-first reader over a borrowed buffer.
#[derive(Debug)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    at: usize,
    accumulator: u64,
    held: u32,
    overrun: bool,
}

impl<'a> BitReader<'a> {
    /// Start at the first byte of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            accumulator: 0,
            held: 0,
            overrun: false,
        }
    }

    /// True once the reader has been asked for bits that the buffer did not
    /// hold. The decoders check this at the end of a frame and report a
    /// truncated stream rather than pretending the zeros were real samples.
    #[must_use]
    pub const fn overrun(&self) -> bool {
        self.overrun
    }

    fn fill(&mut self) {
        while self.held <= 56 {
            let byte = if let Some(byte) = self.bytes.get(self.at) {
                self.at += 1;
                *byte
            } else {
                self.overrun = true;
                0
            };
            self.accumulator = (self.accumulator << 8) | u64::from(byte);
            self.held += 8;
        }
    }

    /// Look at the next `count` bits without consuming them. `count` is clamped
    /// to 32, which is more than any of these formats asks for.
    pub fn peek(&mut self, count: u32) -> u32 {
        let count = count.min(32);
        if count == 0 {
            return 0;
        }
        self.fill();
        let shift = self.held - count;
        let mask = (1u64 << count) - 1;
        ((self.accumulator >> shift) & mask) as u32
    }

    /// Drop `count` bits that a previous [`Self::peek`] looked at.
    pub fn consume(&mut self, count: u32) {
        let count = count.min(self.held);
        self.held -= count;
        self.accumulator &= (1u64 << self.held) - 1;
    }

    /// Read and consume `count` bits.
    pub fn bits(&mut self, count: u32) -> u32 {
        let value = self.peek(count);
        self.consume(count.min(32));
        value
    }

    /// Discard whatever is left of the current byte.
    pub fn align(&mut self) {
        let extra = self.held % 8;
        self.consume(extra);
    }
}

/// A flat Huffman lookup table indexed by a fixed number of peeked bits.
///
/// Every one of these codecs uses short codes - twelve bits at the most - so a
/// direct-indexed table is both the fastest and by far the simplest correct
/// decoder. The table is built once per image and read once per sample.
#[derive(Debug, Clone)]
pub struct FlatHuffman {
    /// How many bits index the table.
    index_bits: u32,
    /// `(code length, symbol)` for every possible index.
    entries: Vec<(u8, u16)>,
}

impl FlatHuffman {
    /// Build from JPEG-style canonical counts and values: `counts[n]` symbols of
    /// length `n + 1`, shortest code first, codes assigned in increasing order.
    ///
    /// Values beyond the end of `values` are read as zero, which is exactly what
    /// the reference decoders do with the zero-padded constant tables the camera
    /// makers publish.
    #[must_use]
    pub fn canonical(counts: &[u8; 16], values: &[u8]) -> Self {
        let mut longest = 16u32;
        while longest > 1 && counts.get(longest as usize - 1).copied().unwrap_or(0) == 0 {
            longest -= 1;
        }
        let mut entries = vec![(0u8, 0u16); 1usize << longest];
        let mut code: u32 = 0;
        let mut taken: usize = 0;
        for length in 1..=longest {
            let count = usize::from(counts.get(length as usize - 1).copied().unwrap_or(0));
            for _ in 0..count {
                let symbol = u16::from(values.get(taken).copied().unwrap_or(0));
                taken += 1;
                let span = 1usize << (longest - length);
                let start = (code as usize) << (longest - length);
                for offset in 0..span {
                    if let Some(slot) = entries.get_mut(start + offset) {
                        *slot = (length as u8, symbol);
                    }
                }
                code += 1;
            }
            code <<= 1;
        }
        Self {
            index_bits: longest,
            entries,
        }
    }

    /// Build from an explicit `(code length, symbol)` table already laid out by
    /// index, which is how the Olympus tree is defined.
    #[must_use]
    pub fn from_entries(index_bits: u32, entries: Vec<(u8, u16)>) -> Self {
        Self {
            index_bits,
            entries,
        }
    }

    /// Decode one symbol, or `None` when the peeked bits match no code.
    pub fn decode(&self, reader: &mut BitReader<'_>) -> Option<u16> {
        let index = reader.peek(self.index_bits) as usize;
        let (length, symbol) = self.entries.get(index).copied()?;
        if length == 0 {
            return None;
        }
        reader.consume(u32::from(length));
        Some(symbol)
    }
}
