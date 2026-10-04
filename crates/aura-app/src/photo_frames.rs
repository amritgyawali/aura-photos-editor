//! The renderer's input is the imported photograph, never a synthetic stand-in.

use aura_core::AuraResult;
use aura_preview::contract::service::{PreviewService, Priority};
use aura_raw::colour::{curve, matrix, working_space};
use aura_raw::{PixelBuffer, PixelData, PixelLevel};
use aura_render::{Frame, FrameSource, RenderLevel};

pub(crate) struct CatalogFrames {
    state: crate::AppState,
}

impl std::fmt::Debug for CatalogFrames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogFrames")
    }
}

impl CatalogFrames {
    pub(crate) fn new(state: crate::AppState) -> Self {
        Self { state }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn working_samples(buffer: &PixelBuffer) -> AuraResult<Vec<f32>> {
    let count = (buffer.width as usize)
        .checked_mul(buffer.height as usize)
        .and_then(|n| n.checked_mul(3))
        .filter(|n| *n > 0)
        .ok_or_else(|| aura_core::errors::raw::corrupt("Invalid render dimensions"))?;
    let samples = match &buffer.data {
        PixelData::Srgb8(bytes) => {
            let transform =
                matrix::mul(working_space::xyz_d65_to_rec2020(), matrix::SRGB_TO_XYZ_D65);
            let mut output = Vec::with_capacity(count);
            for pixel in bytes.chunks_exact(3) {
                let linear = std::array::from_fn(|i| {
                    f64::from(curve::srgb_decode(
                        f32::from(pixel.get(i).copied().unwrap_or(0)) / 255.0,
                    ))
                });
                output.extend(matrix::apply(transform, linear).map(|v| v as f32));
            }
            output
        }
        PixelData::Linear16(codes) => codes
            .iter()
            .map(|v| curve::linear_u16_to_scene(*v))
            .collect(),
        PixelData::Tiled(tiles) => aura_raw::full::assemble(tiles, buffer.width, buffer.height)
            .iter()
            .map(|v| curve::linear_u16_to_scene(*v))
            .collect(),
    };
    if samples.len() != count {
        return Err(aura_core::errors::raw::corrupt(
            "Incomplete photograph pixels",
        ));
    }
    Ok(samples)
}

impl FrameSource for CatalogFrames {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn frame(&self, image: &aura_core::PhotoId, level: RenderLevel) -> AuraResult<Frame> {
        let key = image.to_db();
        let (project, camera): (String, Option<String>) =
            self.state.catalog().read(move |conn| {
                conn.query_row(
                    "SELECT project_id, camera_model FROM photo WHERE photo_id = ?1",
                    [key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| aura_core::errors::db::statement_failed("render photo", &e))
            })?;
        let rung = match level {
            RenderLevel::Full => PixelLevel::Full,
            RenderLevel::Screen(w, h) if w.max(h) <= 512 => PixelLevel::Thumb(w.max(h).max(1)),
            RenderLevel::Proxy2048 | RenderLevel::Screen(_, _) => PixelLevel::Proxy2048,
        };
        // Full exports must decode at full resolution or report failure, never silently
        // substitute a small preview. Missing/corrupt originals retain their typed errors.
        let buffer = self
            .state
            .previews(&project)?
            .get(*image, rung, Priority::Interactive)?;
        let pixels = aura_raw::demosaic::RgbF32 {
            width: buffer.width,
            height: buffer.height,
            data: working_samples(&buffer)?,
        };
        let edge = level.long_edge().unwrap_or(buffer.width.max(buffer.height));
        let scale = (f64::from(edge) / f64::from(buffer.width.max(buffer.height))).min(1.0);
        let width = (f64::from(buffer.width) * scale).round().max(1.0) as u32;
        let height = (f64::from(buffer.height) * scale).round().max(1.0) as u32;
        let pixels = if width != buffer.width || height != buffer.height {
            aura_raw::demosaic::resize(&pixels, width, height)
        } else {
            pixels
        };
        Ok(Frame::working(
            pixels.data,
            pixels.width,
            pixels.height,
            &camera.unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srgb_primaries_survive_the_working_space_roundtrip() {
        let buffer = PixelBuffer {
            width: 2,
            height: 1,
            data: PixelData::Srgb8(vec![230, 40, 20, 20, 60, 220]),
            colour_space: aura_raw::ColourSpace::Srgb,
            source: aura_raw::PixelSource::Embedded,
            decode_ms: 0,
        };
        let samples = working_samples(&buffer).unwrap();
        for (source, working) in [[230_u8, 40, 20], [20, 60, 220]]
            .iter()
            .zip(samples.chunks_exact(3))
        {
            let actual = working_space::working_to_linear_srgb([
                f64::from(working[0]),
                f64::from(working[1]),
                f64::from(working[2]),
            ]);
            for (channel, value) in source.iter().zip(actual) {
                assert!(
                    (value - f64::from(curve::srgb_decode(f32::from(*channel) / 255.0))).abs()
                        < 0.0001
                );
            }
        }
    }
    #[test]
    fn truncated_pixels_are_an_error() {
        let buffer = PixelBuffer {
            width: 2,
            height: 1,
            data: PixelData::Srgb8(vec![1, 2, 3]),
            colour_space: aura_raw::ColourSpace::Srgb,
            source: aura_raw::PixelSource::Embedded,
            decode_ms: 0,
        };
        assert!(working_samples(&buffer).is_err());
    }
}
