//! Tier 2 - the AI proxy, where quality matters more than speed.
//!
//! Every model in phases 05 to 22 is trained on the pixels this module
//! produces, so its rendering is a contract, not a preference. The pipeline is
//! fixed and documented:
//!
//! ```text
//! raw code values
//!   -> subtract black, divide by (white - black)      [demosaic::Levels]
//!   -> multiply by as-shot neutral                    [white balance]
//!   -> half-size demosaic                             [demosaic::half_size]
//!   -> camera matrix, adapted to D65                  [colour::working_space]
//!   -> linear Rec.2020                                *** the 16-bit buffer is written here ***
//!   -> neutral filmic-lite curve                      [colour::curve]
//!   -> sRGB 8-bit                                     [the model and UI buffer]
//! ```
//!
//! Two things are deliberately absent: the camera's picture style, and any
//! automatic brightness. A Canon and a Sony pointed at the same altar must
//! produce the same numbers here, or phase 26 cannot match them and phase 17
//! cannot learn a photographer's style rather than their camera.

use aura_core::clock::Clock;
use aura_core::errors::raw::{mosaic_unsupported, profile_missing, unsupported};
use aura_core::{AuraError, AuraResult};

use crate::cfa;
use crate::codec::{self, Rgb8};
use crate::colour::curve::{scene_to_linear_u16, scene_to_srgb_byte};
use crate::colour::profile::{self, CameraProfile, ProfileSource};
use crate::colour::working_space;
use crate::contract::pixels::{
    ColourSpace, PixelBuffer, PixelData, PixelSource, ProxyPair,
};
use crate::contract::sidecar::ClippingStats;
use crate::demosaic::{self, Levels, RgbF32};
use crate::format::MosaicSupport;
use crate::meta::RawMeta;
use crate::orientation::{apply as apply_orientation, Orientation};
use crate::thumb::CACHE_QUALITY;
use crate::timeout::{timed, DecodeLimits};

/// The long edge every proxy is rendered to.
pub const PROXY_EDGE: u32 = 2048;

/// What tier 2 produced.
#[derive(Debug, Clone)]
pub struct TierTwo {
    /// The 8-bit sRGB and 16-bit linear buffers, always both.
    pub pair: ProxyPair,
    /// The 8-bit buffer encoded for the cache.
    pub jpeg: Vec<u8>,
    /// The profile that was actually used.
    pub profile: CameraProfile,
    /// How much of the frame hit the rails.
    pub clipping: ClippingStats,
    /// `demosaic_half` or `embedded_upscaled`.
    pub render_path: &'static str,
    /// Non-fatal problems: a missing profile, or an undecodable mosaic.
    pub warnings: Vec<AuraError>,
}

/// Render the 2048 px proxy.
///
/// `embedded` is the oriented full-size preview from tier 1, when the caller
/// already has it; passing it avoids reading and decoding the same JPEG twice
/// on the fallback path.
///
/// # Errors
///
/// `AURA-RAW-2001` when there is nothing to render from at all, plus the codes
/// the mosaic decoder and the codec raise.
pub fn tier2(
    bytes: &[u8],
    meta: &RawMeta,
    limits: DecodeLimits,
    clock: &dyn Clock,
    embedded: Option<&Rgb8>,
    path: &std::path::Path,
) -> AuraResult<TierTwo> {
    let mut warnings = Vec::new();
    let resolved = profile::resolve(&meta.make, &meta.model, meta.colour_matrix);
    if resolved.source == ProfileSource::Generic {
        warnings.push(profile_missing(&meta.make, &meta.model));
    }

    let (rendered, decode_ms) = timed(clock, || {
        render_working(bytes, meta, limits, embedded, &resolved, path)
    });
    let Rendered {
        working,
        source,
        render_path,
        warning,
        curved,
    } = rendered?;
    if let Some(problem) = warning {
        warnings.push(problem);
    }

    // Orientation is applied once, at the end, so the resize above worked on the
    // sensor's own axes and the two output buffers can never disagree.
    let oriented = orient_working(working, meta.orientation);
    let clipping = measure_clipping(&oriented);

    let srgb = to_srgb8(&oriented, curved);
    let linear = to_linear16(&oriented);
    let jpeg = codec::encode_jpeg(
        &Rgb8 {
            width: oriented.width,
            height: oriented.height,
            data: srgb.clone(),
        },
        CACHE_QUALITY,
    )?;

    Ok(TierTwo {
        pair: ProxyPair {
            srgb: PixelBuffer {
                width: oriented.width,
                height: oriented.height,
                data: PixelData::Srgb8(srgb),
                colour_space: ColourSpace::Srgb,
                source,
                decode_ms,
            },
            linear: PixelBuffer {
                width: oriented.width,
                height: oriented.height,
                data: PixelData::Linear16(linear),
                colour_space: ColourSpace::LinearRec2020,
                source,
                decode_ms,
            },
        },
        jpeg,
        profile: resolved,
        clipping,
        render_path,
        warnings,
    })
}

struct Rendered {
    working: RgbF32,
    source: PixelSource,
    render_path: &'static str,
    warning: Option<AuraError>,
    /// Whether the preview curve still has to be applied.
    ///
    /// A demosaiced render is scene-referred and needs it. An embedded camera
    /// JPEG is already display-referred: applying our curve on top of the
    /// camera's would double the contrast, so that path is only linearised,
    /// rotated and re-encoded. This is precisely why `PixelSource` travels with
    /// every buffer and every score.
    curved: bool,
}

fn render_working(
    bytes: &[u8],
    meta: &RawMeta,
    limits: DecodeLimits,
    embedded: Option<&Rgb8>,
    resolved: &CameraProfile,
    path: &std::path::Path,
) -> AuraResult<Rendered> {
    if meta.mosaic_support() == MosaicSupport::Decodable {
        if let Some(mosaic_ref) = &meta.mosaic {
            let mosaic = cfa::decode(bytes, mosaic_ref, meta.little_endian, limits.max_pixels)?;
            let levels = Levels {
                black: meta.black_level,
                white: meta.white_level,
                white_balance: meta.as_shot_neutral,
            };
            let half = demosaic::half_size(&mosaic, meta.cfa, levels);
            let (width, height) = demosaic::fit_long_edge(half.width, half.height, PROXY_EDGE);
            let scaled = demosaic::resize(&half, width, height);

            let matrix = working_space::camera_to_working(
                resolved.xyz_to_camera,
                profile::illuminant_white(resolved),
            )
            .ok_or_else(|| {
                aura_core::errors::raw::corrupt("camera colour matrix is not invertible")
            })?;

            return Ok(Rendered {
                working: demosaic::apply_matrix(&scaled, matrix),
                source: PixelSource::Demosaiced,
                render_path: "demosaic_half",
                warning: None,
                curved: true,
            });
        }
    }

    // Fallback: the camera's own rendering. Linearised, rotated into the working
    // space and resized, so downstream code sees a buffer of the right shape in
    // the right space - but the pixels carry the camera's look, and the sidecar
    // says so.
    let preview = match embedded {
        Some(image) => image.clone(),
        None => {
            let reference = meta.best_preview().ok_or_else(|| {
                unsupported(path, "no mosaic and no embedded preview to render from")
            })?;
            let stream = bytes
                .get(reference.offset..reference.offset.saturating_add(reference.len))
                .ok_or_else(|| {
                    aura_core::errors::raw::corrupt("preview range is outside the file")
                })?;
            codec::decode_jpeg(stream, limits)?
        }
    };

    let linear_srgb = demosaic::from_srgb8(&preview.data, preview.width, preview.height);
    let (width, height) = demosaic::fit_long_edge(preview.width, preview.height, PROXY_EDGE);
    let scaled = demosaic::resize(&linear_srgb, width, height);
    let srgb_to_working = crate::colour::matrix::mul(
        working_space::xyz_d65_to_rec2020(),
        crate::colour::matrix::SRGB_TO_XYZ_D65,
    );

    let warning = match meta.mosaic_support() {
        MosaicSupport::CompressionUnsupported => Some(mosaic_unsupported(format!(
            "{} mosaic uses a compression this build cannot decode",
            meta.format.as_str()
        ))),
        _ => None,
    };

    Ok(Rendered {
        working: demosaic::apply_matrix(&scaled, srgb_to_working),
        source: PixelSource::Embedded,
        render_path: "embedded_upscaled",
        warning,
        curved: false,
    })
}

fn orient_working(image: RgbF32, exif: u16) -> RgbF32 {
    let orientation = Orientation::from_exif(exif);
    if orientation.is_identity() {
        return image;
    }
    let (data, width, height) = apply_orientation(&image.data, image.width, image.height, 3, orientation);
    RgbF32 {
        width,
        height,
        data,
    }
}

/// Fraction of pixels clipped at each end, measured on the working buffer
/// before the curve, which is the only place the answer is still true.
#[must_use]
pub fn measure_clipping(image: &RgbF32) -> ClippingStats {
    let mut highlight = 0u64;
    let mut shadow = 0u64;
    let mut total = 0u64;
    for pixel in image.data.chunks_exact(3) {
        let r = pixel.first().copied().unwrap_or(0.0);
        let g = pixel.get(1).copied().unwrap_or(0.0);
        let b = pixel.get(2).copied().unwrap_or(0.0);
        if r >= 1.0 || g >= 1.0 || b >= 1.0 {
            highlight += 1;
        }
        if r <= 0.0 && g <= 0.0 && b <= 0.0 {
            shadow += 1;
        }
        total += 1;
    }
    let divisor = total.max(1) as f32;
    ClippingStats {
        highlight: highlight as f32 / divisor,
        shadow: shadow as f32 / divisor,
    }
}

/// Working space to display bytes: rotate Rec.2020 into sRGB primaries first,
/// then encode. Skipping the rotation would leave every saturated colour
/// desaturated by exactly the gamut difference, which is the classic way a
/// wide-gamut pipeline quietly produces flat-looking previews.
fn to_srgb8(image: &RgbF32, curved: bool) -> Vec<u8> {
    let display = demosaic::apply_matrix(image, working_space::rec2020_to_srgb());
    display
        .data
        .iter()
        .map(|value| {
            if curved {
                scene_to_srgb_byte(*value)
            } else {
                crate::colour::curve::quantise_u8(crate::colour::curve::srgb_encode(*value))
            }
        })
        .collect()
}

fn to_linear16(image: &RgbF32) -> Vec<u16> {
    image
        .data
        .iter()
        .map(|value| scene_to_linear_u16(*value))
        .collect()
}
