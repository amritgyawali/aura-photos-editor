//! From one colour per photosite to three colours per pixel.
//!
//! Two paths, chosen by what the caller is going to do with the result:
//!
//! * [`half_size`] bins each 2x2 Bayer quad into one RGB pixel. It is three to
//!   five times faster than a full interpolation, it introduces no interpolation
//!   artefacts at all (every channel is a real measurement), and half of a
//!   modern sensor is still far more than the 2048 px the proxy needs. This is
//!   the tier 2 path, and therefore the path every model is trained on.
//! * [`full_bilinear`] interpolates every missing sample from its neighbours at
//!   full sensor resolution. This is the tier 3 path, used only on the survivors
//!   that reach final render.
//!
//! Both paths take black level, white level and the as-shot white balance, so
//! their output is camera-native RGB in `0.0..1.0` with a neutral subject
//! landing on a neutral triple - the input the colour matrix expects.

use crate::meta::CfaPattern;

/// An interleaved RGB image in floating point, three samples per pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbF32 {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 3` samples in row-major order.
    pub data: Vec<f32>,
}

impl RgbF32 {
    /// An all-black image of the given size.
    #[must_use]
    pub fn black(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0.0; width as usize * height as usize * 3],
        }
    }

    /// The three samples at `(x, y)`, or black outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [f32; 3] {
        if x >= self.width || y >= self.height {
            return [0.0; 3];
        }
        let at = (y as usize * self.width as usize + x as usize) * 3;
        [
            self.data.get(at).copied().unwrap_or(0.0),
            self.data.get(at + 1).copied().unwrap_or(0.0),
            self.data.get(at + 2).copied().unwrap_or(0.0),
        ]
    }

    fn set(&mut self, x: u32, y: u32, value: [f32; 3]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let at = (y as usize * self.width as usize + x as usize) * 3;
        for (offset, sample) in value.iter().enumerate() {
            if let Some(slot) = self.data.get_mut(at + offset) {
                *slot = *sample;
            }
        }
    }
}

/// How raw code values are turned into scene-linear numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Levels {
    /// The sensor's black point in raw code values.
    pub black: u32,
    /// The sensor's saturation point in raw code values.
    pub white: u32,
    /// As-shot neutral multipliers for R, G and B.
    pub white_balance: [f32; 3],
}

impl Levels {
    /// Normalise one raw reading of a known colour into scene-linear light.
    ///
    /// Readings below black become zero rather than negative: a negative
    /// radiance is not a colour, and letting one through produces bright
    /// speckles once the colour matrix rotates it.
    #[must_use]
    pub fn normalise(&self, raw: u16, colour: u8) -> f32 {
        let span = self.white.saturating_sub(self.black).max(1) as f32;
        let value = (f32::from(raw) - self.black as f32) / span;
        let gain = self
            .white_balance
            .get(colour as usize)
            .copied()
            .unwrap_or(1.0);
        (value * gain).max(0.0)
    }
}

/// Bin each 2x2 Bayer quad into one RGB pixel.
///
/// The output is half the sensor's width and height, and every channel of every
/// output pixel is measured rather than guessed.
#[must_use]
pub fn half_size(mosaic: &crate::cfa::Mosaic, cfa: CfaPattern, levels: Levels) -> RgbF32 {
    let width = mosaic.width / 2;
    let height = mosaic.height / 2;
    let mut out = RgbF32::black(width, height);

    for y in 0..height {
        for x in 0..width {
            let mut sums = [0.0f32; 3];
            let mut counts = [0u32; 3];
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let sx = x * 2 + dx;
                    let sy = y * 2 + dy;
                    let colour = cfa.colour_at(sx, sy);
                    let value = levels.normalise(mosaic.at(sx, sy), colour);
                    if let (Some(sum), Some(count)) = (
                        sums.get_mut(colour as usize),
                        counts.get_mut(colour as usize),
                    ) {
                        *sum += value;
                        *count += 1;
                    }
                }
            }
            let mut pixel = [0.0f32; 3];
            for channel in 0..3usize {
                let count = counts.get(channel).copied().unwrap_or(0);
                if count > 0 {
                    if let (Some(slot), Some(sum)) = (pixel.get_mut(channel), sums.get(channel)) {
                        *slot = *sum / count as f32;
                    }
                }
            }
            out.set(x, y, pixel);
        }
    }
    out
}

/// Full-resolution interpolation.
///
/// Each pixel keeps its own measured channel and takes the mean of the nearest
/// same-colour neighbours for the other two. Averaging over the 3x3
/// neighbourhood handles the frame edges without a special case, which matters
/// because edge artefacts in a proxy become edge artefacts in a mask.
#[must_use]
pub fn full_bilinear(mosaic: &crate::cfa::Mosaic, cfa: CfaPattern, levels: Levels) -> RgbF32 {
    full_bilinear_region(mosaic, cfa, levels, 0, 0, mosaic.width, mosaic.height)
}

/// Interpolate one rectangular region, reading neighbours from outside it.
///
/// This is what makes tiled decoding exact: a tile decoded with a halo produces
/// the same numbers as the same region of a whole-image decode, because the
/// interpolation sees the same neighbours either way.
#[must_use]
pub fn full_bilinear_region(
    mosaic: &crate::cfa::Mosaic,
    cfa: CfaPattern,
    levels: Levels,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
) -> RgbF32 {
    let mut out = RgbF32::black(width, height);
    for y in 0..height {
        for x in 0..width {
            let sx = origin_x + x;
            let sy = origin_y + y;
            let mut sums = [0.0f32; 3];
            let mut counts = [0u32; 3];

            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let nx = sx as i64 + dx;
                    let ny = sy as i64 + dy;
                    if nx < 0 || ny < 0 {
                        continue;
                    }
                    let (nx, ny) = (nx as u32, ny as u32);
                    if nx >= mosaic.width || ny >= mosaic.height {
                        continue;
                    }
                    let colour = cfa.colour_at(nx, ny);
                    let value = levels.normalise(mosaic.at(nx, ny), colour);
                    if let (Some(sum), Some(count)) = (
                        sums.get_mut(colour as usize),
                        counts.get_mut(colour as usize),
                    ) {
                        *sum += value;
                        *count += 1;
                    }
                }
            }

            let own_colour = cfa.colour_at(sx, sy);
            let own_value = levels.normalise(mosaic.at(sx, sy), own_colour);
            let mut pixel = [0.0f32; 3];
            for channel in 0..3usize {
                let count = counts.get(channel).copied().unwrap_or(0);
                let value = if channel == own_colour as usize {
                    own_value
                } else if count > 0 {
                    sums.get(channel).copied().unwrap_or(0.0) / count as f32
                } else {
                    own_value
                };
                if let Some(slot) = pixel.get_mut(channel) {
                    *slot = value;
                }
            }
            out.set(x, y, pixel);
        }
    }
    out
}

/// Resample an RGB image to an exact size.
///
/// Downscaling uses an area average, which is the right filter when the ratio is
/// large - a 45 MP sensor to a 2048 px proxy throws away nine tenths of the
/// pixels, and point sampling that would alias every fabric texture into moire.
/// Upscaling uses bilinear interpolation.
#[must_use]
pub fn resize(source: &RgbF32, target_width: u32, target_height: u32) -> RgbF32 {
    if target_width == 0 || target_height == 0 || source.width == 0 || source.height == 0 {
        return RgbF32::black(target_width, target_height);
    }
    if target_width == source.width && target_height == source.height {
        return source.clone();
    }
    if target_width <= source.width && target_height <= source.height {
        area_average(source, target_width, target_height)
    } else {
        bilinear(source, target_width, target_height)
    }
}

fn area_average(source: &RgbF32, target_width: u32, target_height: u32) -> RgbF32 {
    let mut out = RgbF32::black(target_width, target_height);
    let scale_x = f64::from(source.width) / f64::from(target_width);
    let scale_y = f64::from(source.height) / f64::from(target_height);

    for y in 0..target_height {
        let from_y = (f64::from(y) * scale_y) as u32;
        let to_y = ((f64::from(y + 1) * scale_y).ceil() as u32).min(source.height);
        for x in 0..target_width {
            let from_x = (f64::from(x) * scale_x) as u32;
            let to_x = ((f64::from(x + 1) * scale_x).ceil() as u32).min(source.width);
            let mut sums = [0.0f64; 3];
            let mut count = 0u32;
            for sy in from_y..to_y.max(from_y + 1) {
                for sx in from_x..to_x.max(from_x + 1) {
                    let pixel = source.pixel(sx, sy);
                    for channel in 0..3usize {
                        if let (Some(sum), Some(value)) =
                            (sums.get_mut(channel), pixel.get(channel))
                        {
                            *sum += f64::from(*value);
                        }
                    }
                    count += 1;
                }
            }
            let divisor = f64::from(count.max(1));
            let mut pixel = [0.0f32; 3];
            for channel in 0..3usize {
                if let (Some(slot), Some(sum)) = (pixel.get_mut(channel), sums.get(channel)) {
                    *slot = (*sum / divisor) as f32;
                }
            }
            out.set(x, y, pixel);
        }
    }
    out
}

fn bilinear(source: &RgbF32, target_width: u32, target_height: u32) -> RgbF32 {
    let mut out = RgbF32::black(target_width, target_height);
    let scale_x = f64::from(source.width.saturating_sub(1)) / f64::from(target_width.max(1));
    let scale_y = f64::from(source.height.saturating_sub(1)) / f64::from(target_height.max(1));

    for y in 0..target_height {
        let source_y = f64::from(y) * scale_y;
        let y0 = source_y as u32;
        let fy = (source_y - f64::from(y0)) as f32;
        for x in 0..target_width {
            let source_x = f64::from(x) * scale_x;
            let x0 = source_x as u32;
            let fx = (source_x - f64::from(x0)) as f32;

            let p00 = source.pixel(x0, y0);
            let p10 = source.pixel(x0 + 1, y0);
            let p01 = source.pixel(x0, y0 + 1);
            let p11 = source.pixel(x0 + 1, y0 + 1);

            let mut pixel = [0.0f32; 3];
            for channel in 0..3usize {
                let a = p00.get(channel).copied().unwrap_or(0.0);
                let b = p10.get(channel).copied().unwrap_or(0.0);
                let c = p01.get(channel).copied().unwrap_or(0.0);
                let d = p11.get(channel).copied().unwrap_or(0.0);
                let top = a + (b - a) * fx;
                let bottom = c + (d - c) * fx;
                if let Some(slot) = pixel.get_mut(channel) {
                    *slot = top + (bottom - top) * fy;
                }
            }
            out.set(x, y, pixel);
        }
    }
    out
}

/// Lift an 8-bit sRGB image into linear floating point.
///
/// Resampling has to happen in linear light. Averaging sRGB code values darkens
/// every edge, and on a wedding that shows up first as grey fringes around a
/// white dress against a dark background.
#[must_use]
pub fn from_srgb8(data: &[u8], width: u32, height: u32) -> RgbF32 {
    let mut out = RgbF32::black(width, height);
    for (index, sample) in data
        .iter()
        .take(width as usize * height as usize * 3)
        .enumerate()
    {
        if let Some(slot) = out.data.get_mut(index) {
            *slot = crate::colour::curve::srgb_decode(f32::from(*sample) / 255.0);
        }
    }
    out
}

/// Encode a linear image back to 8-bit sRGB, without the preview curve.
#[must_use]
pub fn to_srgb8(image: &RgbF32) -> Vec<u8> {
    image
        .data
        .iter()
        .map(|sample| crate::colour::curve::quantise_u8(crate::colour::curve::srgb_encode(*sample)))
        .collect()
}

/// Multiply every pixel by a 3x3 matrix, the one place a colour rotation
/// happens in the pixel path.
#[must_use]
pub fn apply_matrix(image: &RgbF32, matrix: crate::colour::matrix::Mat3) -> RgbF32 {
    let mut out = RgbF32::black(image.width, image.height);
    for (target, source) in out.data.chunks_exact_mut(3).zip(image.data.chunks_exact(3)) {
        let input = [
            f64::from(source.first().copied().unwrap_or(0.0)),
            f64::from(source.get(1).copied().unwrap_or(0.0)),
            f64::from(source.get(2).copied().unwrap_or(0.0)),
        ];
        let rotated = crate::colour::matrix::apply(matrix, input);
        for (slot, value) in target.iter_mut().zip(rotated.iter()) {
            *slot = *value as f32;
        }
    }
    out
}

/// The size that fits inside a square of `long_edge`, keeping the aspect ratio
/// and never returning a zero dimension.
#[must_use]
pub fn fit_long_edge(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    if width == 0 || height == 0 || long_edge == 0 {
        return (width.max(1), height.max(1));
    }
    let longest = width.max(height);
    if longest <= long_edge {
        return (width, height);
    }
    let scale = f64::from(long_edge) / f64::from(longest);
    let target_width = ((f64::from(width) * scale).round() as u32).max(1);
    let target_height = ((f64::from(height) * scale).round() as u32).max(1);
    (target_width, target_height)
}
