//! FROZEN CONTRACT. The pixel vocabulary of the whole product.
//!
//! Phase 02 section 5 freezes these shapes before any decoder exists, because
//! every later phase asks for pixels through them and none of those phases may
//! learn where the pixels came from. Changing anything here requires an ADR.
//!
//! Two rules are encoded in the types rather than in prose:
//!
//! * A proxy is always available in **both** 8-bit sRGB and 16-bit linear. An
//!   8-bit JPEG cannot be re-linearised without losing the highlight detail that
//!   white-balance and exposure models need, so the linear buffer is not an
//!   optimisation, it is the contract.
//! * A full-resolution decode is **tiled**. A 60 MP frame decoded whole is
//!   360 MB of `u16`; the memory ceiling is a promise we keep by never having
//!   the whole frame resident.

use serde::{Deserialize, Serialize};

/// Which rung of the pyramid the caller needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelLevel {
    /// Tier 1. A thumbnail whose long edge is at most the given size, derived
    /// from the camera's embedded preview. Costs milliseconds.
    Thumb(u32),
    /// Tier 2. The 2048 px long-edge proxy every model is trained on.
    Proxy2048,
    /// Tier 3. Full sensor resolution, tiled, for final render only.
    Full,
}

impl PixelLevel {
    /// The tier number as stored in the `preview` table.
    #[must_use]
    pub const fn tier(self) -> i64 {
        match self {
            Self::Thumb(_) => 1,
            Self::Proxy2048 => 2,
            Self::Full => 3,
        }
    }

    /// Longest edge in pixels, or zero for [`PixelLevel::Full`] where the
    /// sensor decides.
    #[must_use]
    pub const fn long_edge(self) -> u32 {
        match self {
            Self::Thumb(size) => size,
            Self::Proxy2048 => 2048,
            Self::Full => 0,
        }
    }

    /// Stable text used in cache paths and telemetry. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thumb(_) => "thumb",
            Self::Proxy2048 => "proxy2048",
            Self::Full => "full",
        }
    }
}

/// Where the pixels came from. The distinction is load-bearing: models are
/// trained on demosaiced Tier 2 pixels, and a score computed from an embedded
/// camera JPEG carries the camera's baked-in look instead of ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelSource {
    /// Copied from the JPEG the camera stored inside the file.
    Embedded,
    /// Rendered by AURA from the sensor mosaic through the documented path.
    Demosaiced,
}

impl PixelSource {
    /// The text form stored in the `preview` table's `source` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Demosaiced => "decoded",
        }
    }
}

/// The colour space a buffer's numbers are in. There is no "unknown": a buffer
/// whose space we cannot name is a buffer we must not grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColourSpace {
    /// Display-referred sRGB with the sRGB transfer function. UI and models.
    Srgb,
    /// Scene-referred linear light with Rec.2020 primaries, D65 white. The
    /// working space for every colour decision in the product.
    LinearRec2020,
    /// Sensor native, white-balanced but not yet through a camera matrix.
    /// Produced only by the raw-tile diagnostic API, never by the three tiers.
    CameraNative,
}

impl ColourSpace {
    /// Stable text for the sidecar and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::LinearRec2020 => "linear_rec2020",
            Self::CameraNative => "camera_native",
        }
    }
}

/// One tile of a full-resolution decode, including its halo.
///
/// `x`/`y`/`width`/`height` describe the **committed** region - the pixels this
/// tile owns in the final image. The halo is decoded but discarded on assembly,
/// which is what makes tiled output identical to whole-image output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    /// Left edge of the committed region, in image coordinates.
    pub x: u32,
    /// Top edge of the committed region, in image coordinates.
    pub y: u32,
    /// Committed width in pixels.
    pub width: u32,
    /// Committed height in pixels.
    pub height: u32,
    /// Interleaved RGB, three `u16` per pixel, `width * height * 3` long.
    pub data: Vec<u16>,
}

/// A full-resolution image as a grid of tiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TiledImage {
    /// Edge length of a committed tile region, 512 px by default.
    pub tile_size: u32,
    /// Halo decoded around every tile so demosaic has neighbours, 32 px.
    pub halo: u32,
    /// Tiles in row-major order. Deterministic, so assembly is deterministic.
    pub tiles: Vec<Tile>,
}

/// The pixels themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelData {
    /// Interleaved RGB, three bytes per pixel, sRGB encoded.
    Srgb8(Vec<u8>),
    /// Interleaved RGB, three `u16` per pixel, linear light.
    Linear16(Vec<u16>),
    /// Full-resolution tiles, linear `u16`.
    Tiled(TiledImage),
}

impl PixelData {
    /// Stable discriminant text for the sidecar and for cache file naming.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Srgb8(_) => "srgb8",
            Self::Linear16(_) => "linear16",
            Self::Tiled(_) => "tiled",
        }
    }

    /// Bytes this buffer occupies, used by the cache budget.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Srgb8(bytes) => bytes.len(),
            Self::Linear16(words) => words.len() * 2,
            Self::Tiled(image) => image
                .tiles
                .iter()
                .map(|tile| tile.data.len() * 2)
                .sum::<usize>(),
        }
    }
}

/// One decoded image at one level, plus everything a caller needs to know about
/// how it was produced. `decode_ms` is not decoration: the scheduler uses the
/// measured cost of the last decode to size the next batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelBuffer {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The pixels.
    pub data: PixelData,
    /// The space `data` is expressed in.
    pub colour_space: ColourSpace,
    /// Embedded camera JPEG, or rendered by AURA.
    pub source: PixelSource,
    /// Wall-clock milliseconds this decode cost.
    pub decode_ms: u32,
}

impl PixelBuffer {
    /// Pixels in the image, ignoring channels.
    #[must_use]
    pub const fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Bytes the pixel payload occupies.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.data.byte_len()
    }

    /// Borrow the 8-bit sRGB payload, if that is what this buffer holds.
    #[must_use]
    pub fn as_srgb8(&self) -> Option<&[u8]> {
        match &self.data {
            PixelData::Srgb8(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Borrow the 16-bit linear payload, if that is what this buffer holds.
    #[must_use]
    pub fn as_linear16(&self) -> Option<&[u16]> {
        match &self.data {
            PixelData::Linear16(words) => Some(words),
            _ => None,
        }
    }

    /// Borrow the tile grid, if this is a full-resolution decode.
    #[must_use]
    pub fn as_tiled(&self) -> Option<&TiledImage> {
        match &self.data {
            PixelData::Tiled(image) => Some(image),
            _ => None,
        }
    }
}

/// Both halves of a tier 2 render. Produced together, cached together, and
/// invalidated together, because a proxy that exists in only one of the two
/// representations is a proxy some later phase cannot use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyPair {
    /// 8-bit sRGB, for the UI and for models.
    pub srgb: PixelBuffer,
    /// 16-bit linear Rec.2020, for colour maths.
    pub linear: PixelBuffer,
}
