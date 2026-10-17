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

/// Display sRGB bytes to the renderer's scene-referred working space: the values the output
/// transform renders back to these same bytes.
pub(crate) fn srgb8_to_working(bytes: &[u8]) -> Vec<f32> {
    let mut samples = srgb8_to_linear(bytes);
    aura_render::output::unroll(&mut samples);
    samples
}

/// Display sRGB bytes to linear Rec.2020, still display-referred: what is averaged when a
/// frame is resized, so a white backdrop does not outweigh the hair in front of it.
#[allow(clippy::cast_possible_truncation)]
fn srgb8_to_linear(bytes: &[u8]) -> Vec<f32> {
    use rayon::prelude::*;
    let transform = matrix::mul(working_space::xyz_d65_to_rec2020(), matrix::SRGB_TO_XYZ_D65);
    let usable = bytes.len() - bytes.len() % 3;
    let mut output = vec![0.0_f32; usable];
    // Each pixel is independent, so the frame converts on every core; the table is the
    // sRGB formula evaluated once per code, so the samples are the same to the last bit.
    output
        .par_chunks_exact_mut(3)
        .zip(bytes.par_chunks_exact(3))
        .for_each(|(out, pixel)| {
            let linear = std::array::from_fn(|i| {
                f64::from(curve::srgb_byte_to_linear(
                    pixel.get(i).copied().unwrap_or(0),
                ))
            });
            for (slot, value) in out.iter_mut().zip(matrix::apply(transform, linear)) {
                *slot = value as f32;
            }
        });
    output
}

#[allow(clippy::cast_possible_truncation)]
/// The buffer's samples in Rec.2020 primaries, and whether a display rendering made them.
fn working_samples(buffer: &PixelBuffer) -> AuraResult<(Vec<f32>, bool)> {
    let count = (buffer.width as usize)
        .checked_mul(buffer.height as usize)
        .and_then(|n| n.checked_mul(3))
        .filter(|n| *n > 0)
        .ok_or_else(|| aura_core::errors::raw::corrupt("Invalid render dimensions"))?;
    let (samples, display): (Vec<f32>, bool) = match &buffer.data {
        PixelData::Srgb8(bytes) => (srgb8_to_linear(bytes), true),
        PixelData::Linear16(codes) => (
            codes
                .iter()
                .map(|v| curve::linear_u16_to_scene(*v))
                .collect(),
            false,
        ),
        PixelData::Tiled(tiles) => (
            aura_raw::full::assemble(tiles, buffer.width, buffer.height)
                .iter()
                .map(|v| curve::linear_u16_to_scene(*v))
                .collect(),
            false,
        ),
    };
    if samples.len() != count {
        return Err(aura_core::errors::raw::corrupt(
            "Incomplete photograph pixels",
        ));
    }
    Ok((samples, display))
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
        let (samples, display) = working_samples(&buffer)?;
        let pixels = aura_raw::demosaic::RgbF32 {
            width: buffer.width,
            height: buffer.height,
            data: samples,
        };
        let edge = level.long_edge().unwrap_or(buffer.width.max(buffer.height));
        let scale = (f64::from(edge) / f64::from(buffer.width.max(buffer.height))).min(1.0);
        let width = (f64::from(buffer.width) * scale).round().max(1.0) as u32;
        let height = (f64::from(buffer.height) * scale).round().max(1.0) as u32;
        let mut pixels = if width != buffer.width || height != buffer.height {
            aura_raw::demosaic::resize(&pixels, width, height)
        } else {
            pixels
        };
        if display {
            aura_render::output::unroll(&mut pixels.data);
        }
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
        let (samples, display) = working_samples(&buffer).unwrap();
        assert!(display, "sRGB bytes are a display rendering");
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
                assert!((value - f64::from(curve::srgb_byte_to_linear(*channel))).abs() < 0.0001);
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
