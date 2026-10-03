//! Delivery-only sRGB graphics composited in linear output light, after output sharpening.
use crate::{
    read::{Rendered, Samples},
    resample::{from_linear, to_linear},
};
use aura_core::{contract::delivery::DeliveryColour, AuraResult};
use aura_render::OutputColour;

/// Portable raster graphic. Text is rasterized by the UI's installed fonts before submission.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Watermark {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub opacity: f32,
    pub width_fraction: f32,
    pub margin_fraction: f32,
    pub anchor: Anchor,
}

/// Placement inside the delivered image's edges.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl Watermark {
    /// Preserve the exact graphic/settings beside the delivery, without replacing any file.
    /// The manifest records the relative filename and its read-back hash for reproducibility.
    /// # Errors
    /// Invalid parameters, a write failure, or an existing asset with different contents.
    pub fn archive(&self, root: &std::path::Path) -> AuraResult<(String, String)> {
        use std::io::Write;
        self.validate()?;
        let bytes =
            serde_json::to_vec(self).map_err(|e| crate::errors::job_refused(e.to_string()))?;
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let name = format!(".aura-watermark-{hash}.json");
        let path = root.join(&name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .and_then(|()| file.flush())
                    .and_then(|()| file.sync_all())
                    .map_err(|e| {
                        crate::errors::destination_bad(format!("Cannot archive watermark: {e}"))
                    })?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(crate::errors::destination_bad(format!(
                    "Cannot archive watermark: {error}"
                )))
            }
        }
        if crate::verify::hash_file(&path)? != hash {
            return Err(crate::errors::job_refused("An existing watermark archive has different contents; choose a different destination"));
        }
        Ok((name, hash))
    }

    /// Reject malformed or unbounded payloads before opening an export job.
    /// # Errors
    /// An invalid size, alpha payload or placement parameter.
    pub fn validate(&self) -> AuraResult<()> {
        let size = u64::from(self.width) * u64::from(self.height);
        if self.width == 0
            || self.height == 0
            || self.width > 1024
            || self.height > 1024
            || size > 262_144
            || size * 4 != self.rgba.len() as u64
            || !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || !self.width_fraction.is_finite()
            || !(0.05..=0.8).contains(&self.width_fraction)
            || !self.margin_fraction.is_finite()
            || !(0.0..=0.1).contains(&self.margin_fraction)
        {
            return Err(crate::errors::job_refused(
                "Invalid watermark pixels or placement",
            ));
        }
        Ok(())
    }

    /// Composite without altering the input buffer. Preserve its output colour and bit depth.
    /// # Errors
    /// An invalid graphic or malformed rendered frame.
    pub fn apply(&self, source: &Rendered) -> AuraResult<Rendered> {
        self.validate()?;
        if !source.is_well_formed() {
            return Err(crate::errors::job_refused("Malformed watermark target"));
        }
        let mut output = source.clone();
        if self.opacity <= 0.0 || self.rgba.chunks_exact(4).all(|p| p.get(3) == Some(&0)) {
            return Ok(output);
        }
        let colour = source.colour;
        let space = match colour {
            DeliveryColour::Srgb => OutputColour::Srgb,
            DeliveryColour::AdobeRgb => OutputColour::AdobeRgb,
            DeliveryColour::DisplayP3 => OutputColour::DisplayP3,
        };
        let matrix = aura_render::output::srgb_to_output(space);
        let graphic: Vec<[f32; 4]> = self
            .rgba
            .chunks_exact(4)
            .map(|p| {
                let rgb = std::array::from_fn(|i| {
                    to_linear(
                        DeliveryColour::Srgb,
                        f32::from(p.get(i).copied().unwrap_or(0)) / 255.0,
                    )
                });
                let [r, g, b] = aura_render::colour::apply_f32(matrix, rgb);
                let a = f32::from(p.get(3).copied().unwrap_or(0)) / 255.0 * self.opacity;
                [r * a, g * a, b * a, a]
            })
            .collect();
        let margin = (source.width.min(source.height) as f32 * self.margin_fraction).round() as u32;
        let available_w = source.width.saturating_sub(2 * margin).max(1);
        let available_h = source.height.saturating_sub(2 * margin).max(1);
        let scale = (source.width as f32 * self.width_fraction / self.width as f32)
            .min(available_w as f32 / self.width as f32)
            .min(available_h as f32 / self.height as f32);
        let width = (self.width as f32 * scale).round().max(1.0) as u32;
        let height = (self.height as f32 * scale).round().max(1.0) as u32;
        let left = match self.anchor {
            Anchor::TopLeft | Anchor::BottomLeft => margin,
            Anchor::Center => (source.width - width) / 2,
            _ => source.width - width - margin,
        };
        let top = match self.anchor {
            Anchor::TopLeft | Anchor::TopRight => margin,
            Anchor::Center => (source.height - height) / 2,
            _ => source.height - height - margin,
        };
        for y in 0..height {
            for x in 0..width {
                let px = ((x as f32 + 0.5) * self.width as f32 / width as f32 - 0.5)
                    .clamp(0.0, (self.width - 1) as f32);
                let py = ((y as f32 + 0.5) * self.height as f32 / height as f32 - 0.5)
                    .clamp(0.0, (self.height - 1) as f32);
                let x0 = px.floor() as u32;
                let y0 = py.floor() as u32;
                let fx = px.fract();
                let fy = py.fract();
                let mut sample = [0.0; 4];
                for (sx, sy, weight) in [
                    (x0, y0, (1.0 - fx) * (1.0 - fy)),
                    ((x0 + 1).min(self.width - 1), y0, fx * (1.0 - fy)),
                    (x0, (y0 + 1).min(self.height - 1), (1.0 - fx) * fy),
                    (
                        (x0 + 1).min(self.width - 1),
                        (y0 + 1).min(self.height - 1),
                        fx * fy,
                    ),
                ] {
                    if let Some(pixel) = graphic.get((sy * self.width + sx) as usize) {
                        for (dst, value) in sample.iter_mut().zip(pixel) {
                            *dst += value * weight;
                        }
                    }
                }
                let alpha = sample.get(3).copied().unwrap_or(0.0);
                if alpha <= 0.0 {
                    continue;
                }
                let offset =
                    (((top + y) as usize * source.width as usize) + (left + x) as usize) * 3;
                for (channel, overlay) in sample.iter().take(3).enumerate() {
                    let blend = |value: f32| {
                        from_linear(colour, to_linear(colour, value) * (1.0 - alpha) + overlay)
                    };
                    match &mut output.data {
                        Samples::Eight(samples) => {
                            if let Some(value) = samples.get_mut(offset + channel) {
                                *value = (blend(f32::from(*value) / 255.0) * 255.0).round() as u8;
                            }
                        }
                        Samples::Sixteen(samples) => {
                            if let Some(value) = samples.get_mut(offset + channel) {
                                *value =
                                    (blend(f32::from(*value) / 65535.0) * 65535.0).round() as u16;
                            }
                        }
                    }
                }
            }
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"aura-watermark-v1\0");
        hash.update(source.render_hash.as_bytes());
        let encoded =
            serde_json::to_vec(self).map_err(|e| crate::errors::job_refused(e.to_string()))?;
        hash.update(&encoded);
        output.render_hash = hash.finalize().to_hex().to_string();
        Ok(output)
    }
}
