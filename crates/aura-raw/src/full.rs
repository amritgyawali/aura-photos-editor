//! Tier 3 - full sensor resolution, decoded in tiles.
//!
//! A 60 MP frame demosaiced whole is 60 million pixels times three channels
//! times four bytes: 720 MB of intermediate, for one image, on a machine that
//! also has to hold a catalog, a UI and a model. Tiling is not an optimisation
//! here, it is how the memory ceiling in the phase budget is kept.
//!
//! The rule that makes tiling safe is the halo. Interpolation reads a pixel's
//! neighbours, so a tile decoded in isolation would get different answers along
//! its edges - visible as a 512 px grid over the whole frame. Each tile is
//! therefore decoded with 32 px of context on every side and only its centre is
//! committed. `tiled_matches_whole_image` in the test suite asserts the result
//! is identical to a whole-image decode, sample for sample.
//!
//! Full decodes are never speculative. Only the survivors that reach final
//! render get one.

use aura_core::clock::Clock;
use aura_core::errors::raw::{corrupt, mosaic_unsupported};
use aura_core::AuraResult;

use crate::cfa;
use crate::colour::{profile, working_space};
use crate::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource, Tile, TiledImage};
use crate::demosaic::{self, Levels};
use crate::format::MosaicSupport;
use crate::meta::RawMeta;
use crate::timeout::{check_dimensions, timed, DecodeLimits};

/// Edge length of the region a tile commits.
pub const TILE_SIZE: u32 = 512;

/// Context decoded around each tile and then discarded.
pub const HALO: u32 = 32;

/// Decode a file at full sensor resolution, tile by tile.
///
/// # Errors
///
/// `AURA-RAW-2007` when the mosaic cannot be decoded - tier 3 has no embedded
/// fallback, because a final render from a camera JPEG would be a lie - plus
/// the codes the mosaic decoder raises.
pub fn tier3(
    bytes: &[u8],
    meta: &RawMeta,
    limits: DecodeLimits,
    clock: &dyn Clock,
) -> AuraResult<PixelBuffer> {
    let (result, decode_ms) = timed(clock, || {
        let (mosaic, levels, matrix) = prepare(bytes, meta, limits)?;
        check_dimensions(mosaic.width, mosaic.height, 6, limits)?;

        let columns = mosaic.width.div_ceil(TILE_SIZE);
        let rows = mosaic.height.div_ceil(TILE_SIZE);
        let mut tiles = Vec::with_capacity((columns as usize) * (rows as usize));

        for row in 0..rows {
            for column in 0..columns {
                let x = column * TILE_SIZE;
                let y = row * TILE_SIZE;
                let width = TILE_SIZE.min(mosaic.width - x);
                let height = TILE_SIZE.min(mosaic.height - y);

                // Decode with context, then keep only the committed centre.
                let halo_x = x.saturating_sub(HALO);
                let halo_y = y.saturating_sub(HALO);
                let halo_width = (x + width + HALO).min(mosaic.width) - halo_x;
                let halo_height = (y + height + HALO).min(mosaic.height) - halo_y;

                let region = demosaic::full_bilinear_region(
                    &mosaic,
                    meta.cfa,
                    levels,
                    halo_x,
                    halo_y,
                    halo_width,
                    halo_height,
                );
                let working = demosaic::apply_matrix(&region, matrix);

                let mut committed = Vec::with_capacity((width as usize) * (height as usize) * 3);
                for local_y in 0..height {
                    for local_x in 0..width {
                        let pixel = working.pixel(x - halo_x + local_x, y - halo_y + local_y);
                        for channel in pixel {
                            committed.push(crate::colour::curve::scene_to_linear_u16(channel));
                        }
                    }
                }

                tiles.push(Tile {
                    x,
                    y,
                    width,
                    height,
                    data: committed,
                });
            }
        }

        Ok::<_, aura_core::AuraError>((mosaic.width, mosaic.height, tiles))
    });

    let (width, height, tiles) = result?;
    Ok(PixelBuffer {
        width,
        height,
        data: PixelData::Tiled(TiledImage {
            tile_size: TILE_SIZE,
            halo: HALO,
            tiles,
        }),
        colour_space: ColourSpace::LinearRec2020,
        source: PixelSource::Demosaiced,
        decode_ms,
    })
}

/// Decode the whole frame in one piece.
///
/// This exists so the test suite has something to compare tiles against, and so
/// a small image can skip the tiling machinery. It is not the production path
/// for large sensors: nothing here bounds the intermediate allocation except
/// [`DecodeLimits::max_alloc_bytes`].
///
/// # Errors
///
/// The same codes as [`tier3`].
pub fn tier3_whole(
    bytes: &[u8],
    meta: &RawMeta,
    limits: DecodeLimits,
    clock: &dyn Clock,
) -> AuraResult<PixelBuffer> {
    let (result, decode_ms) = timed(clock, || {
        let (mosaic, levels, matrix) = prepare(bytes, meta, limits)?;
        check_dimensions(mosaic.width, mosaic.height, 12, limits)?;
        let rgb = demosaic::full_bilinear(&mosaic, meta.cfa, levels);
        let working = demosaic::apply_matrix(&rgb, matrix);
        let samples = working
            .data
            .iter()
            .map(|value| crate::colour::curve::scene_to_linear_u16(*value))
            .collect::<Vec<u16>>();
        Ok::<_, aura_core::AuraError>((working.width, working.height, samples))
    });

    let (width, height, samples) = result?;
    Ok(PixelBuffer {
        width,
        height,
        data: PixelData::Linear16(samples),
        colour_space: ColourSpace::LinearRec2020,
        source: PixelSource::Demosaiced,
        decode_ms,
    })
}

type Prepared = (cfa::Mosaic, Levels, crate::colour::matrix::Mat3);

fn prepare(bytes: &[u8], meta: &RawMeta, limits: DecodeLimits) -> AuraResult<Prepared> {
    if meta.mosaic_support() != MosaicSupport::Decodable {
        return Err(mosaic_unsupported(format!(
            "{} cannot be decoded at full resolution by this build",
            meta.format.as_str()
        )));
    }
    let mosaic_ref = meta
        .mosaic
        .as_ref()
        .ok_or_else(|| mosaic_unsupported("file carries no mosaic"))?;
    let mosaic = cfa::decode(bytes, mosaic_ref, meta.little_endian, limits.max_pixels)?;

    let levels = Levels {
        black: meta.black_level,
        white: meta.white_level,
        white_balance: meta.as_shot_neutral,
    };
    let resolved = profile::resolve(&meta.make, &meta.model, meta.colour_matrix);
    let matrix = working_space::camera_to_working(
        resolved.xyz_to_camera,
        profile::illuminant_white(&resolved),
    )
    .ok_or_else(|| corrupt("camera colour matrix is not invertible"))?;

    Ok((mosaic, levels, matrix))
}

/// Stitch a tiled image back into one buffer.
///
/// Used by the equivalence test and by any consumer that genuinely wants the
/// whole frame; production render code walks the tiles instead.
#[must_use]
pub fn assemble(image: &TiledImage, width: u32, height: u32) -> Vec<u16> {
    let mut out = vec![0u16; width as usize * height as usize * 3];
    for tile in &image.tiles {
        for row in 0..tile.height {
            let source = (row as usize) * (tile.width as usize) * 3;
            let target = ((tile.y + row) as usize * width as usize + tile.x as usize) * 3;
            let length = tile.width as usize * 3;
            let (Some(from), Some(to)) = (
                tile.data.get(source..source + length),
                out.get_mut(target..target + length),
            ) else {
                continue;
            };
            to.copy_from_slice(from);
        }
    }
    out
}
