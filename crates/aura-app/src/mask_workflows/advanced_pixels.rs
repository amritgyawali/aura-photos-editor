//! Pixel-derived selections use read-only, unadjusted sRGB preview samples at the mask grid.
use super::{advanced::Options, check, invalid, plane::Plane, Action};
use crate::AppState;
use aura_core::{progress::CancelToken, AuraResult, PhotoId};
use aura_preview::contract::service::PreviewService;
use std::collections::VecDeque;

pub(super) struct Frame {
    pub(super) rgb: Vec<[f32; 3]>,
    pub(super) luminance: Vec<f32>,
    pub(super) edges: Vec<f32>,
}
impl Frame {
    pub(super) fn load(
        state: &AppState,
        project: &str,
        photo: PhotoId,
        width: usize,
        height: usize,
    ) -> AuraResult<Self> {
        let preview = state.previews(project)?.get(
            photo,
            aura_raw::PixelLevel::Proxy2048,
            aura_preview::contract::service::Priority::Interactive,
        )?;
        let pixels = preview
            .as_srgb8()
            .ok_or_else(|| invalid("An sRGB preview is required for sampled selections."))?;
        let image = image::RgbImage::from_raw(preview.width, preview.height, pixels.to_vec())
            .ok_or_else(|| invalid("Invalid source preview."))?;
        let width = u32::try_from(width).map_err(|_| invalid("Invalid mask width."))?;
        let height = u32::try_from(height).map_err(|_| invalid("Invalid mask height."))?;
        let resized =
            image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
        Self::from_rgb(resized.as_raw(), width as usize, height as usize)
    }
    #[allow(clippy::indexing_slicing)] // Validated packed RGB length and neighbors clamped to checked grid.
    pub(super) fn from_rgb(pixels: &[u8], width: usize, height: usize) -> AuraResult<Self> {
        if width == 0
            || height == 0
            || width > 512
            || height > 512
            || width
                .checked_mul(height)
                .and_then(|neighbor| neighbor.checked_mul(3))
                != Some(pixels.len())
        {
            return Err(invalid("Invalid sampled selection frame."));
        }
        let rgb: Vec<_> = pixels
            .chunks_exact(3)
            .map(|pixel| {
                [
                    f32::from(pixel[0]) / 255.0,
                    f32::from(pixel[1]) / 255.0,
                    f32::from(pixel[2]) / 255.0,
                ]
            })
            .collect();
        let luminance: Vec<_> = rgb
            .iter()
            .map(|pixel| pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722)
            .collect();
        let mut edges = vec![0.0; rgb.len()];
        for y in 0..height {
            for x in 0..width {
                let at = |x: usize, y: usize| luminance[y * width + x];
                let left = x.saturating_sub(1);
                let right = (x + 1).min(width - 1);
                let top = y.saturating_sub(1);
                let bottom = (y + 1).min(height - 1);
                let gx = at(right, top) + 2.0 * at(right, y) + at(right, bottom)
                    - at(left, top)
                    - 2.0 * at(left, y)
                    - at(left, bottom);
                let gy = at(left, bottom) + 2.0 * at(x, bottom) + at(right, bottom)
                    - at(left, top)
                    - 2.0 * at(x, top)
                    - at(right, top);
                edges[y * width + x] = (gx.hypot(gy) / 4.0).min(1.0);
            }
        }
        Ok(Self {
            rgb,
            luminance,
            edges,
        })
    }
}
fn difference(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() / 3.0_f32.sqrt()
}
fn chroma(pixel: [f32; 3]) -> [f32; 3] {
    let sum = (pixel[0] + pixel[1] + pixel[2]).max(0.001);
    [pixel[0] / sum, pixel[1] / sum, pixel[2] / sum]
}
fn band(value: f32, range: [f32; 2], softness: f32) -> f32 {
    let outside = (range[0] - value).max(value - range[1]).max(0.0);
    (1.0 - outside / softness).clamp(0.0, 1.0)
}
impl Plane {
    // Source indices share validated mask grid dimensions; values are clamped before alpha casts.
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub(super) fn pixel_selection(
        &mut self,
        action: Action,
        options: &Options,
        point: [f32; 2],
        frame: &Frame,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        if frame.rgb.len() != self.alpha.len() {
            return Err(invalid("Pixel and selection grids differ."));
        }
        let x = ((point[0] * self.width as f32) as usize).min(self.width - 1);
        let y = ((point[1] * self.height as f32) as usize).min(self.height - 1);
        let seed = y * self.width + x;
        let target = frame.rgb[seed];
        let mut patch = Vec::new();
        for sy in y.saturating_sub(options.radius as usize)
            ..(y + options.radius as usize + 1).min(self.height)
        {
            for sx in x.saturating_sub(options.radius as usize)
                ..(x + options.radius as usize + 1).min(self.width)
            {
                patch.push(frame.luminance[sy * self.width + sx]);
            }
        }
        let patch_mean = patch.iter().sum::<f32>() / patch.len() as f32;
        match action {
            Action::ColourWand => self.colour_wand(options, seed, frame, cancel)?,

            Action::EdgeGrow => self.edge_grow(options, frame, cancel)?,

            Action::GuidedSmooth => self.guided_smooth(options, frame, cancel)?,

            _ => {
                for y in 0..self.height {
                    check(cancel)?;
                    for x in 0..self.width {
                        let index = y * self.width + x;
                        let pixel = frame.rgb[index];
                        let coverage = match action {
                            Action::SampleColour => (1.0
                                - difference(pixel, target) / options.tolerance)
                                .clamp(0.0, 1.0),
                            Action::SampleLuminance => (1.0
                                - (frame.luminance[index] - patch_mean).abs() / options.tolerance)
                                .clamp(0.0, 1.0),
                            Action::SaturationRange => {
                                let max = pixel.iter().copied().fold(0.0_f32, f32::max);
                                let min = pixel.iter().copied().fold(1.0_f32, f32::min);
                                let saturation = if max > 0.0 { (max - min) / max } else { 0.0 };
                                band(saturation, options.range, options.tolerance)
                            }
                            Action::TextureRange => band(
                                self.local_deviation(frame, x, y, options.radius.min(8) as usize),
                                options.range,
                                options.tolerance,
                            ),
                            Action::ImageEdges => {
                                band(frame.edges[index], options.range, options.tolerance)
                            }
                            Action::ChromaKey => {
                                f32::from(self.alpha[index]) / 255.0
                                    * (difference(chroma(pixel), chroma(target))
                                        / options.tolerance)
                                        .clamp(0.0, 1.0)
                            }
                            _ => return Err(invalid("Unknown pixel-derived selection operation.")),
                        };
                        self.alpha[index] = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
        }
        // Excluding a key colour may intentionally remove the entire selection.
        if action != Action::ChromaKey && self.alpha.iter().all(|a| *a == 0) {
            return Err(invalid(
                "This operation found no selected pixels. The previous selection was kept.",
            ));
        }
        check(cancel)
    }

    // Coordinates and indices are confined to the validated <=512-cell grid.
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn colour_wand(
        &mut self,
        options: &Options,
        seed: usize,
        frame: &Frame,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        let mut visited = vec![false; self.alpha.len()];
        let mut queue = VecDeque::from([seed]);
        visited[seed] = true;
        let mut output = vec![0; self.alpha.len()];
        while let Some(index) = queue.pop_front() {
            if index % 1024 == 0 {
                check(cancel)?;
            }
            let delta = difference(frame.rgb[index], frame.rgb[seed]);
            if delta > options.tolerance {
                continue;
            }
            output[index] = 255;
            for neighbor in self.neighbors(index) {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        self.alpha = output;

        Ok(())
    }

    // Coordinates and indices are confined to the validated <=512-cell grid.
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn edge_grow(
        &mut self,
        options: &Options,
        frame: &Frame,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        let mut steps = vec![u32::MAX; self.alpha.len()];
        let mut queue = VecDeque::new();
        for (index, a) in self.alpha.iter().enumerate() {
            if *a >= 128 {
                steps[index] = 0;
                queue.push_back(index);
            }
        }
        if queue.is_empty() {
            return Err(invalid("Select an area before edge-constrained growth."));
        }
        while let Some(index) = queue.pop_front() {
            if index % 1024 == 0 {
                check(cancel)?;
            }
            if steps[index] >= options.radius {
                continue;
            }
            for neighbor in self.neighbors(index) {
                if steps[neighbor] > steps[index] + 1
                    && frame.edges[neighbor] <= options.tolerance
                    && difference(frame.rgb[index], frame.rgb[neighbor]) <= options.tolerance
                {
                    steps[neighbor] = steps[index] + 1;
                    queue.push_back(neighbor);
                }
            }
        }
        for (index, a) in self.alpha.iter_mut().enumerate() {
            if steps[index] != u32::MAX {
                *a = 255;
            }
        }

        Ok(())
    }

    // Coordinates and indices are confined to the validated <=512-cell grid.
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn guided_smooth(
        &mut self,
        options: &Options,
        frame: &Frame,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        if options.radius > 8 {
            return Err(invalid(
                "Image-guided smoothing radius is limited to eight cells.",
            ));
        }
        let mut output = vec![0; self.alpha.len()];
        let radius = options.radius as usize;
        for y in 0..self.height {
            check(cancel)?;
            for x in 0..self.width {
                let index = y * self.width + x;
                let (mut sum, mut weight) = (0.0, 0.0);
                for sy in y.saturating_sub(radius)..(y + radius + 1).min(self.height) {
                    for sx in x.saturating_sub(radius)..(x + radius + 1).min(self.width) {
                        let neighbor = sy * self.width + sx;
                        let delta = difference(frame.rgb[index], frame.rgb[neighbor]);
                        let spatial = ((sx as f32 - x as f32).powi(2)
                            + (sy as f32 - y as f32).powi(2))
                            / (options.radius as f32).powi(2);
                        let sample_weight =
                            (-0.5 * spatial - 0.5 * (delta / options.tolerance).powi(2)).exp();
                        sum += f32::from(self.alpha[neighbor]) * sample_weight;
                        weight += sample_weight;
                    }
                }
                output[index] = (sum / weight.max(0.0001)).round().clamp(0.0, 255.0) as u8;
            }
        }
        self.alpha = output;

        Ok(())
    }

    // Radius is at most eight and all samples are clipped to the validated source grid.
    #[allow(clippy::indexing_slicing)]
    fn local_deviation(&self, frame: &Frame, x: usize, y: usize, radius: usize) -> f32 {
        let (mut sum, mut square, mut count) = (0.0_f32, 0.0_f32, 0.0_f32);
        for sy in y.saturating_sub(radius)..(y + radius + 1).min(self.height) {
            for sx in x.saturating_sub(radius)..(x + radius + 1).min(self.width) {
                let value = frame.luminance[sy * self.width + sx];
                sum += value;
                square += value * value;
                count += 1.0;
            }
        }
        ((square / count - (sum / count).powi(2)).max(0.0)).sqrt() * 2.0
    }
}
