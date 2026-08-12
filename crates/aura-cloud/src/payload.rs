//! Everything that will be uploaded is built here, and nothing is built anywhere
//! else.
//!
//! One module owns this so that "no RAW or full-resolution pixels ever leave the
//! machine" - acceptance criterion 5 - is a property of a hundred lines rather
//! than of every future phase's good intentions. The refusals are the interesting
//! part of the module:
//!
//! * A [`PixelData::Tiled`] buffer is a tier 3 full-resolution decode. Refused,
//!   always, with no policy switch that permits it.
//! * A [`PixelData::Linear16`] buffer is refused too, and this one is worth
//!   explaining: it is a legitimate derivative, but converting scene-linear
//!   Rec.2020 to display sRGB is a colour decision, and Article VIII says colour
//!   decisions happen in one place. Callers hand over the sRGB half of the
//!   [`ProxyPair`](aura_raw::contract::pixels::ProxyPair) that phase 02 already
//!   produced for them.
//! * A payload over the byte ceiling is refused rather than truncated.
//!
//! ## The arithmetic behind the default tile size
//!
//! Section 7 of the phase document describes "a contact sheet of up to 12
//! thumbnails (768 px each)". Twelve 768 px tiles is a 3072 x 2304 sheet - seven
//! megapixels, about 10,000 image tokens, roughly USD 0.03 of input at
//! balanced-tier prices. The per-call ceiling is USD 0.02 and the whole-wedding
//! budget is USD 1.50 across 75 calls, so a sheet built that way is refused by
//! the cost governor before it is ever sent, and `SegmentNaming` would silently
//! run on local fallbacks for every segment.
//!
//! So 768 px is the **maximum** long edge this builder will emit for a single
//! crop, and the shipped contact-sheet policy uses 384 px tiles: a 4 x 3 grid is
//! 1536 x 1152, about 2,500 image tokens, near USD 0.008 of input. That leaves
//! room for the response inside the ceiling. Both numbers are in
//! [`PayloadPolicy`] rather than hard-coded, and the exit report records the
//! measurement.

use aura_core::errors::cloud::payload_refused;
use aura_core::AuraResult;
use aura_raw::codec::{encode_jpeg, Rgb8};
use aura_raw::contract::pixels::{PixelBuffer, PixelData};

use crate::contract::cloud::ImagePart;
use crate::redact::{blur_regions, Region};

/// The largest long edge a single crop may have. Section 2.1's 768 px.
pub const MAX_LONG_EDGE: u32 = 768;

/// The largest long edge a composed contact sheet may have.
///
/// A sheet is a grid of crops, so it is legitimately larger than one of them,
/// and the 768 px rule would make a twelve-tile sheet impossible. 1536 is four
/// 384 px tiles across - the shipped layout - and is also the ceiling the cost
/// arithmetic in the module note is based on. Anything larger is downscaled to
/// it rather than refused, because a sheet that is one row too tall is a layout
/// detail and not a privacy question.
pub const MAX_SHEET_LONG_EDGE: u32 = 1536;

/// The most tiles one contact sheet may hold.
pub const MAX_TILES: usize = 12;

/// The largest a whole encoded payload may be.
pub const MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

/// JPEG quality for uploads. Below about 78 the compression artefacts start to
/// look like sensor noise, which is a thing several later phases ask about.
pub const JPEG_QUALITY: u8 = 82;

/// How this payload should be built, and what it may not exceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadPolicy {
    /// Long edge of a single crop. Clamped to [`MAX_LONG_EDGE`].
    pub crop_long_edge: u32,
    /// Long edge of one tile inside a contact sheet.
    pub tile_long_edge: u32,
    /// Long edge of a whole sheet. Clamped to [`MAX_SHEET_LONG_EDGE`].
    pub sheet_long_edge: u32,
    /// Tiles per sheet. Clamped to [`MAX_TILES`].
    pub max_tiles: usize,
    /// Byte ceiling for one built payload.
    pub max_bytes: usize,
    /// JPEG quality.
    pub quality: u8,
    /// Blur supplied face regions before encoding.
    pub blur_faces: bool,
}

impl Default for PayloadPolicy {
    fn default() -> Self {
        Self {
            crop_long_edge: MAX_LONG_EDGE,
            tile_long_edge: 384,
            sheet_long_edge: MAX_SHEET_LONG_EDGE,
            max_tiles: MAX_TILES,
            max_bytes: MAX_PAYLOAD_BYTES,
            quality: JPEG_QUALITY,
            blur_faces: false,
        }
    }
}

impl PayloadPolicy {
    /// The same policy with face blur switched on.
    #[must_use]
    pub const fn blurred(mut self) -> Self {
        self.blur_faces = true;
        self
    }

    /// The crop long edge, never above the hard maximum whatever was asked for.
    #[must_use]
    pub fn effective_crop_edge(&self) -> u32 {
        self.crop_long_edge.clamp(32, MAX_LONG_EDGE)
    }

    /// The tile long edge, never above the hard maximum.
    #[must_use]
    pub fn effective_tile_edge(&self) -> u32 {
        self.tile_long_edge.clamp(32, MAX_LONG_EDGE)
    }

    /// The sheet long edge, never above the hard maximum.
    #[must_use]
    pub fn effective_sheet_edge(&self) -> u32 {
        self.sheet_long_edge.clamp(64, MAX_SHEET_LONG_EDGE)
    }
}

/// One image on its way into a payload, with the faces that may need blurring.
#[derive(Debug, Clone, Copy)]
pub struct SourceImage<'a> {
    /// The decoded pixels. Must be [`PixelData::Srgb8`].
    pub buffer: &'a PixelBuffer,
    /// Face rectangles in the buffer's own coordinates.
    ///
    /// Empty is normal - phase 06 is what fills this in, and until then a
    /// project that requires blur has nothing to blur, which is a refusal rather
    /// than an upload.
    pub faces: &'a [Region],
}

impl<'a> SourceImage<'a> {
    /// An image with no known faces.
    #[must_use]
    pub const fn new(buffer: &'a PixelBuffer) -> Self {
        Self { buffer, faces: &[] }
    }

    /// An image with face rectangles.
    #[must_use]
    pub const fn with_faces(buffer: &'a PixelBuffer, faces: &'a [Region]) -> Self {
        Self { buffer, faces }
    }
}

/// Build one downscaled crop.
///
/// # Errors
///
/// `AURA-CLOUD-6009` for a tiled or linear buffer, a buffer whose length
/// disagrees with its dimensions, a blur that was required and had nothing to
/// blur, or an encoded result over the ceiling.
pub fn crop(image: &SourceImage<'_>, policy: PayloadPolicy) -> AuraResult<ImagePart> {
    let rgb = prepare(image, policy, policy.effective_crop_edge())?;
    encode(&rgb, policy)
}

/// Build a contact sheet of up to [`PayloadPolicy::max_tiles`] images.
///
/// Tiles are laid out left to right, top to bottom, in the order given, on a
/// near-square grid. Order is the caller's and is never sorted here: the prompt
/// refers to tiles by index, so a builder that reordered them would silently
/// change what every answer meant.
///
/// # Errors
///
/// `AURA-CLOUD-6009` when there are no images, when there are more than the
/// policy allows, or for any of the per-image refusals in [`crop`].
pub fn contact_sheet(images: &[SourceImage<'_>], policy: PayloadPolicy) -> AuraResult<ImagePart> {
    if images.is_empty() {
        return Err(payload_refused("a contact sheet needs at least one image"));
    }
    let allowed = policy.max_tiles.min(MAX_TILES);
    if images.len() > allowed {
        return Err(payload_refused(format!(
            "a contact sheet may hold {allowed} images, {} were offered",
            images.len()
        )));
    }

    let edge = policy.effective_tile_edge();
    let tiles: Vec<Rgb8> = images
        .iter()
        .map(|image| prepare(image, policy, edge))
        .collect::<AuraResult<Vec<_>>>()?;

    // A near-square grid: four across for twelve, three for nine, two for four.
    let columns = grid_columns(tiles.len());
    let rows = tiles.len().div_ceil(columns);
    // Cells are the largest tile's box, so a portrait frame beside a landscape
    // one is letterboxed rather than stretched. A stretched face is a face a
    // model reads differently.
    let cell_w = tiles.iter().map(|tile| tile.width).max().unwrap_or(edge);
    let cell_h = tiles.iter().map(|tile| tile.height).max().unwrap_or(edge);

    let sheet_w = cell_w as usize * columns;
    let sheet_h = cell_h as usize * rows;
    let mut sheet = vec![GUTTER; sheet_w * sheet_h * 3];

    for (index, tile) in tiles.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        // Centred in its cell, so the letterboxing is symmetrical.
        let offset_x = column * cell_w as usize + (cell_w as usize - tile.width as usize) / 2;
        let offset_y = row * cell_h as usize + (cell_h as usize - tile.height as usize) / 2;
        for y in 0..tile.height as usize {
            let source = y * tile.width as usize * 3;
            let target = ((offset_y + y) * sheet_w + offset_x) * 3;
            let span = tile.width as usize * 3;
            let Some(from) = tile.data.get(source..source + span) else {
                continue;
            };
            if let Some(into) = sheet.get_mut(target..target + span) {
                into.copy_from_slice(from);
            }
        }
    }

    let assembled = Rgb8 {
        width: u32::try_from(sheet_w).unwrap_or(u32::MAX),
        height: u32::try_from(sheet_h).unwrap_or(u32::MAX),
        data: sheet,
    };
    // A sheet whose grid came out larger than the ceiling is reduced rather than
    // refused: the tile count and the source aspect ratios are the caller's, and
    // an extra row is a layout detail, not a privacy question.
    encode(
        &downscale(&assembled, policy.effective_sheet_edge()),
        policy,
    )
}

/// Mid grey between the tiles: neutral, and unmistakably not part of a frame.
const GUTTER: u8 = 128;

/// Columns for a near-square grid of `count` tiles.
fn grid_columns(count: usize) -> usize {
    let mut columns = 1usize;
    while columns * columns < count {
        columns += 1;
    }
    columns.max(1)
}

/// Check the buffer, blur what needs blurring, and downscale.
fn prepare(image: &SourceImage<'_>, policy: PayloadPolicy, long_edge: u32) -> AuraResult<Rgb8> {
    let buffer = image.buffer;
    let pixels = match &buffer.data {
        PixelData::Srgb8(bytes) => bytes,
        PixelData::Tiled(_) => {
            return Err(payload_refused(
                "refusing to upload a full-resolution tiled decode; send a thumbnail or a proxy",
            ))
        }
        PixelData::Linear16(_) => return Err(payload_refused(
            "refusing to convert scene-linear pixels here; send the sRGB half of the proxy pair",
        )),
    };

    let expected = buffer.width as usize * buffer.height as usize * 3;
    if pixels.len() != expected {
        return Err(payload_refused(format!(
            "a {}x{} rgb buffer should be {expected} bytes, this one is {}",
            buffer.width,
            buffer.height,
            pixels.len()
        )));
    }
    if buffer.width == 0 || buffer.height == 0 {
        return Err(payload_refused("refusing to upload an empty image"));
    }

    let mut working = pixels.clone();
    if policy.blur_faces {
        if image.faces.is_empty() {
            return Err(payload_refused(
                "this project requires faces to be blurred before upload and none were supplied",
            ));
        }
        // Blurred **before** downscaling, so the blur is applied at the
        // resolution the detector worked at and the downscale cannot resurrect
        // detail from a partially blurred edge.
        let blurred = blur_regions(&mut working, buffer.width, buffer.height, image.faces);
        if blurred == 0 {
            return Err(payload_refused(
                "every supplied face region fell outside the image, so nothing was blurred",
            ));
        }
    }

    Ok(downscale(
        &Rgb8 {
            width: buffer.width,
            height: buffer.height,
            data: working,
        },
        long_edge,
    ))
}

/// Encode, then check the ceiling.
fn encode(image: &Rgb8, policy: PayloadPolicy) -> AuraResult<ImagePart> {
    let bytes = encode_jpeg(image, policy.quality)?;
    if bytes.len() > policy.max_bytes {
        return Err(payload_refused(format!(
            "the encoded payload is {} bytes, over the {} byte ceiling",
            bytes.len(),
            policy.max_bytes
        )));
    }
    let content_hash = blake3::hash(&bytes).to_hex().to_string();
    Ok(ImagePart {
        media_type: "image/jpeg".to_string(),
        bytes,
        content_hash,
        width: image.width,
        height: image.height,
    })
}

/// Area-average downscale to a long edge. Never upscales.
///
/// A box filter rather than something sharper, on purpose. The result is fed to
/// a vision model, not to a person, and a sharpening kernel manufactures edge
/// contrast that a model reading for "is this frame sharp" would then measure.
#[must_use]
pub fn downscale(image: &Rgb8, long_edge: u32) -> Rgb8 {
    let source_long = image.width.max(image.height);
    if source_long <= long_edge || source_long == 0 {
        return image.clone();
    }

    let scale = f64::from(long_edge) / f64::from(source_long);
    let width = ((f64::from(image.width) * scale).round() as u32).max(1);
    let height = ((f64::from(image.height) * scale).round() as u32).max(1);

    let mut out = vec![0u8; width as usize * height as usize * 3];
    for y in 0..height as usize {
        // Integer source spans, so the same input gives the same output on every
        // machine regardless of how the compiler orders the float maths.
        let y0 = y * image.height as usize / height as usize;
        let y1 = (((y + 1) * image.height as usize).div_ceil(height as usize)).max(y0 + 1);
        for x in 0..width as usize {
            let x0 = x * image.width as usize / width as usize;
            let x1 = (((x + 1) * image.width as usize).div_ceil(width as usize)).max(x0 + 1);

            let mut totals = [0u32; 3];
            let mut count = 0u32;
            for sy in y0..y1.min(image.height as usize) {
                for sx in x0..x1.min(image.width as usize) {
                    let base = (sy * image.width as usize + sx) * 3;
                    for (channel, total) in totals.iter_mut().enumerate() {
                        *total += u32::from(image.data.get(base + channel).copied().unwrap_or(0));
                    }
                    count += 1;
                }
            }
            let base = (y * width as usize + x) * 3;
            for (channel, total) in totals.iter().enumerate() {
                if let Some(slot) = out.get_mut(base + channel) {
                    *slot = u8::try_from(total / count.max(1)).unwrap_or(u8::MAX);
                }
            }
        }
    }

    Rgb8 {
        width,
        height,
        data: out,
    }
}

/// Standard base64 with padding.
///
/// Written here rather than taken as a dependency: it is twenty lines, it is on
/// the path every uploaded byte takes, and one fewer crate in the supply chain of
/// the network layer is worth twenty lines.
#[must_use]
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk.first().copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);

        let indices = [
            (triple >> 18) & 0x3F,
            (triple >> 12) & 0x3F,
            (triple >> 6) & 0x3F,
            triple & 0x3F,
        ];
        for (position, index) in indices.iter().enumerate() {
            if position > chunk.len() {
                out.push('=');
            } else if let Some(symbol) = ALPHABET.get(*index as usize) {
                out.push(char::from(*symbol));
            }
        }
    }
    out
}
