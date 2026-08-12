//! Tier 1 - the speed weapon.
//!
//! The camera already rendered a JPEG of every frame and stored it inside the
//! RAW. Copying it costs a few milliseconds; demosaicing the same frame costs
//! two orders of magnitude more. That difference is the economic core of the
//! product: 4,000 embedded previews in three minutes is what makes a wedding
//! feel browsable, and triage - scene, faces, expression, duplicates - is
//! perfectly well served by the camera's own rendering.
//!
//! What tier 1 is *not* for: anything a model is trained on, or any colour
//! decision. Those need the documented rendering of tier 2, and every score
//! records which tier it came from so the two are never mixed up.

use aura_core::clock::Clock;
use aura_core::errors::raw::{no_embedded_preview, unsupported};
use aura_core::{AuraError, AuraResult};

use crate::cfa;
use crate::codec::{self, Rgb8};
use crate::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use crate::demosaic::{self, Levels};
use crate::meta::RawMeta;
use crate::orientation::{apply as apply_orientation, Orientation};
use crate::timeout::{timed, DecodeLimits};

/// Default long edge of the thumbnail the grid renders.
pub const DEFAULT_THUMB_EDGE: u32 = 512;

/// JPEG quality used for cached previews.
pub const CACHE_QUALITY: u8 = 90;

/// What tier 1 produced.
#[derive(Debug, Clone)]
pub struct TierOne {
    /// The thumbnail, oriented and scaled, ready for the grid.
    pub buffer: PixelBuffer,
    /// The same thumbnail encoded for the cache.
    pub jpeg: Vec<u8>,
    /// The full-size embedded preview, oriented. Tier 2 reuses it as its
    /// fallback rather than reading the file a second time.
    pub full: Option<Rgb8>,
    /// Non-fatal problem worth showing the photographer, if any.
    pub warning: Option<AuraError>,
    /// How the pixels were produced, for the sidecar.
    pub render_path: &'static str,
}

/// Extract the embedded preview, orient it and derive a thumbnail.
///
/// # Errors
///
/// `AURA-RAW-2001` when the file has neither a preview nor a decodable mosaic,
/// `AURA-RAW-2002` when the preview will not decode, `AURA-RAW-2005` when it is
/// larger than the ceiling allows.
pub fn tier1(
    bytes: &[u8],
    meta: &RawMeta,
    long_edge: u32,
    limits: DecodeLimits,
    clock: &dyn Clock,
    path: &std::path::Path,
) -> AuraResult<TierOne> {
    let (outcome, decode_ms) = timed(clock, || extract(bytes, meta, limits, path));
    let Extracted {
        image,
        source,
        warning,
        render_path,
    } = outcome?;

    let oriented = orient(image, meta.orientation);
    let thumbnail = scale_to(&oriented, long_edge);
    let jpeg = codec::encode_jpeg(&thumbnail, CACHE_QUALITY)?;

    Ok(TierOne {
        buffer: PixelBuffer {
            width: thumbnail.width,
            height: thumbnail.height,
            data: PixelData::Srgb8(thumbnail.data),
            colour_space: ColourSpace::Srgb,
            source,
            decode_ms,
        },
        jpeg,
        full: Some(oriented),
        warning,
        render_path,
    })
}

struct Extracted {
    image: Rgb8,
    source: PixelSource,
    warning: Option<AuraError>,
    render_path: &'static str,
}

fn extract(
    bytes: &[u8],
    meta: &RawMeta,
    limits: DecodeLimits,
    path: &std::path::Path,
) -> AuraResult<Extracted> {
    if let Some(preview) = meta.best_preview() {
        let stream = bytes
            .get(preview.offset..preview.offset.saturating_add(preview.len))
            .ok_or_else(|| {
                aura_core::errors::raw::corrupt("preview range is outside the file")
            })?;
        let image = codec::decode_jpeg(stream, limits)?;
        return Ok(Extracted {
            image,
            source: PixelSource::Embedded,
            warning: None,
            render_path: "embedded_jpeg",
        });
    }

    // Rare, but real: some medium-format backs and some DNG converters store no
    // preview. A quarter-size demosaic is roughly twenty times slower and is
    // tagged so QA can tell the two paths apart.
    let Some(mosaic_ref) = &meta.mosaic else {
        return Err(unsupported(
            path,
            "file carries neither an embedded preview nor a mosaic",
        ));
    };
    let mosaic = cfa::decode(bytes, mosaic_ref, meta.little_endian, limits.max_pixels)?;
    let levels = Levels {
        black: meta.black_level,
        white: meta.white_level,
        white_balance: meta.as_shot_neutral,
    };
    let half = demosaic::half_size(&mosaic, meta.cfa, levels);
    let quarter = demosaic::resize(&half, (half.width / 2).max(1), (half.height / 2).max(1));

    Ok(Extracted {
        image: Rgb8 {
            width: quarter.width,
            height: quarter.height,
            data: demosaic::to_srgb8(&quarter),
        },
        source: PixelSource::Demosaiced,
        warning: Some(no_embedded_preview(format!(
            "{} has no embedded preview; rendered a quarter-size frame instead",
            meta.format.as_str()
        ))),
        render_path: "demosaic_quarter",
    })
}

fn orient(image: Rgb8, exif: u16) -> Rgb8 {
    let orientation = Orientation::from_exif(exif);
    if orientation.is_identity() {
        return image;
    }
    let (data, width, height) = apply_orientation(&image.data, image.width, image.height, 3, orientation);
    Rgb8 {
        width,
        height,
        data,
    }
}

/// Scale an 8-bit image so its long edge is at most `long_edge`, resampling in
/// linear light.
#[must_use]
pub fn scale_to(image: &Rgb8, long_edge: u32) -> Rgb8 {
    let (width, height) = demosaic::fit_long_edge(image.width, image.height, long_edge);
    if width == image.width && height == image.height {
        return image.clone();
    }
    let linear = demosaic::from_srgb8(&image.data, image.width, image.height);
    let resized = demosaic::resize(&linear, width, height);
    Rgb8 {
        width: resized.width,
        height: resized.height,
        data: demosaic::to_srgb8(&resized),
    }
}
