//! The protobuf wire format, read and written, in about three hundred lines.
//!
//! ONNX files are protobuf. Rather than depend on a code generator and a runtime
//! for a schema of which we use nine messages, this module implements the wire
//! format directly: varints, length-delimited fields, and the two fixed-width
//! forms. That is the whole format - protobuf's wire encoding has five wire types
//! and we use three.
//!
//! It ships with a **writer** as well as a reader, for the same reason phase 02's
//! camera codecs ship with encoders: with no third-party runtime in this build, a
//! round trip through an independent implementation of the same format is the
//! strongest proof available that the reader is right. `tests/onnx_roundtrip.rs`
//! is where that proof lives.
//!
//! Everything here is bounds-checked and allocation-bounded. A model file is an
//! artefact that arrives over a network, so a truncated length prefix must
//! produce `AURA-ML-5010`, never a panic and never a multi-gigabyte allocation.

use aura_core::AuraResult;

use crate::errors::malformed;

/// Largest length prefix we will honour, in bytes.
///
/// Two gigabytes is far above any model this product will ship - the largest in
/// the plan is under 400 MB - and far below the point where a corrupt varint can
/// exhaust a laptop's memory before the digest check would have caught it.
const MAX_FIELD_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// A protobuf wire type. Groups (3 and 4) are deprecated and refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireType {
    /// Varint-encoded integer.
    Varint,
    /// Fixed 64-bit little-endian.
    Fixed64,
    /// Length-prefixed bytes: strings, sub-messages, packed arrays.
    Bytes,
    /// Fixed 32-bit little-endian.
    Fixed32,
}

impl WireType {
    fn from_tag(tag: u64) -> Option<Self> {
        match tag & 0x7 {
            0 => Some(Self::Varint),
            1 => Some(Self::Fixed64),
            2 => Some(Self::Bytes),
            5 => Some(Self::Fixed32),
            _ => None,
        }
    }

    const fn code(self) -> u64 {
        match self {
            Self::Varint => 0,
            Self::Fixed64 => 1,
            Self::Bytes => 2,
            Self::Fixed32 => 5,
        }
    }
}

/// A cursor over one message's bytes.
#[derive(Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// Start reading a message.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    /// True when every byte has been consumed.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    /// How many bytes have been read so far, for error messages.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.at
    }

    /// Read the next field's number and wire type.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` on a truncated or deprecated tag.
    pub fn field(&mut self) -> AuraResult<(u32, WireType)> {
        let tag = self.varint()?;
        let wire = WireType::from_tag(tag)
            .ok_or_else(|| malformed(format!("unsupported wire type at offset {}", self.at)))?;
        let number = u32::try_from(tag >> 3)
            .map_err(|_| malformed(format!("absurd field number at offset {}", self.at)))?;
        Ok((number, wire))
    }

    /// Read one varint.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` when the varint runs off the end or exceeds ten bytes.
    pub fn varint(&mut self) -> AuraResult<u64> {
        let mut value = 0u64;
        for shift in 0..10u32 {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or_else(|| malformed(format!("varint truncated at offset {}", self.at)))?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << (shift * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(malformed(format!(
            "varint longer than ten bytes at offset {}",
            self.at
        )))
    }

    /// Read a length-delimited field's bytes.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` when the length is absurd or overruns the buffer.
    pub fn bytes(&mut self) -> AuraResult<&'a [u8]> {
        let len = self.varint()?;
        if len > MAX_FIELD_BYTES {
            return Err(malformed(format!("field claims {len} bytes")));
        }
        let len = usize::try_from(len)
            .map_err(|_| malformed("field length does not fit this machine".to_string()))?;
        let end = self
            .at
            .checked_add(len)
            .ok_or_else(|| malformed("field length overflows".to_string()))?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| malformed(format!("field of {len} bytes overruns the message")))?;
        self.at = end;
        Ok(slice)
    }

    /// Read a UTF-8 string field.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` on truncation or invalid UTF-8.
    pub fn string(&mut self) -> AuraResult<String> {
        let raw = self.bytes()?;
        std::str::from_utf8(raw)
            .map(ToOwned::to_owned)
            .map_err(|err| malformed(format!("string field is not UTF-8: {err}")))
    }

    /// Read a fixed 32-bit little-endian value.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` on truncation.
    pub fn fixed32(&mut self) -> AuraResult<u32> {
        let end = self.at + 4;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| malformed(format!("fixed32 truncated at offset {}", self.at)))?;
        self.at = end;
        let mut buf = [0u8; 4];
        buf.copy_from_slice(slice);
        Ok(u32::from_le_bytes(buf))
    }

    /// Read a fixed 64-bit little-endian value.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` on truncation.
    pub fn fixed64(&mut self) -> AuraResult<u64> {
        let end = self.at + 8;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| malformed(format!("fixed64 truncated at offset {}", self.at)))?;
        self.at = end;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(slice);
        Ok(u64::from_le_bytes(buf))
    }

    /// Skip a field whose number we do not implement.
    ///
    /// Unknown fields are skipped rather than refused: that is what makes a
    /// reader forward-compatible with a producer that writes `doc_string`s and
    /// metadata we do not care about.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` when the field cannot be measured.
    pub fn skip(&mut self, wire: WireType) -> AuraResult<()> {
        match wire {
            WireType::Varint => {
                self.varint()?;
            }
            WireType::Fixed64 => {
                self.fixed64()?;
            }
            WireType::Bytes => {
                self.bytes()?;
            }
            WireType::Fixed32 => {
                self.fixed32()?;
            }
        }
        Ok(())
    }
}

/// Decode a packed repeated field of varints.
///
/// # Errors
///
/// `AURA-ML-5010` when the packed block is malformed.
pub fn packed_varints(bytes: &[u8]) -> AuraResult<Vec<i64>> {
    let mut reader = Reader::new(bytes);
    let mut values = Vec::new();
    while !reader.is_done() {
        values.push(reader.varint()?.cast_signed());
    }
    Ok(values)
}

/// Decode a packed repeated field of 32-bit floats.
///
/// # Errors
///
/// `AURA-ML-5010` when the block is not a whole number of floats.
pub fn packed_floats(bytes: &[u8]) -> AuraResult<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return Err(malformed(format!(
            "packed float block of {} bytes is not a whole number of floats",
            bytes.len()
        )));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(chunk);
            f32::from_le_bytes(buf)
        })
        .collect())
}

/// A growing protobuf message.
#[derive(Debug, Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// An empty message.
    #[must_use]
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// The encoded bytes.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    /// Bytes written so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when nothing has been written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn tag(&mut self, field: u32, wire: WireType) {
        self.put_varint((u64::from(field) << 3) | wire.code());
    }

    fn put_varint(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                return;
            }
            self.bytes.push(byte | 0x80);
        }
    }

    /// Write a varint field.
    pub fn varint(&mut self, field: u32, value: i64) {
        self.tag(field, WireType::Varint);
        self.put_varint(value as u64);
    }

    /// Write a 32-bit float field.
    pub fn float(&mut self, field: u32, value: f32) {
        self.tag(field, WireType::Fixed32);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a length-delimited field.
    pub fn bytes(&mut self, field: u32, value: &[u8]) {
        self.tag(field, WireType::Bytes);
        self.put_varint(value.len() as u64);
        self.bytes.extend_from_slice(value);
    }

    /// Write a string field.
    pub fn string(&mut self, field: u32, value: &str) {
        self.bytes(field, value.as_bytes());
    }

    /// Write a nested message field.
    pub fn message(&mut self, field: u32, inner: Writer) {
        let encoded = inner.finish();
        self.bytes(field, &encoded);
    }

    /// Write a packed repeated varint field.
    pub fn packed_varints(&mut self, field: u32, values: &[i64]) {
        let mut inner = Writer::new();
        for value in values {
            inner.put_varint(*value as u64);
        }
        let encoded = inner.finish();
        self.bytes(field, &encoded);
    }

    /// Write a packed repeated float field.
    pub fn packed_floats(&mut self, field: u32, values: &[f32]) {
        let mut encoded = Vec::with_capacity(values.len() * 4);
        for value in values {
            encoded.extend_from_slice(&value.to_le_bytes());
        }
        self.bytes(field, &encoded);
    }
}
