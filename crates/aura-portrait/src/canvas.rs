//! The analysis canvas: one downsampled copy of the photograph in the three spaces the parse
//! reads.
//!
//! * **Encoded sRGB** for the chroma maps the face literature is written in (`YCbCr`, the
//!   mouth map), because those models were fitted on display-referred values and a ratio
//!   taken on linear light is a different number.
//! * **CIELAB** for every distance between two colours - is this pixel the same colour as her
//!   cheek, is this hair - because a distance in Lab is a distance a person would see.
//! * **Grey** bytes for the cascade, which was trained on 8-bit luma.
//!
//! The downsample happens in *linear* light and only then is anything encoded. Averaging
//! encoded values darkens every edge between a bright and a dark region, and an edge between
//! a white dress and a dark suit is exactly where a body matte has to be right.

use aura_raw::colour::{curve, matrix, working_space};
use rayon::prelude::*;

/// One photograph, downsampled and converted once.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Encoded sRGB, `0..=1`, one triple per pixel.
    pub srgb: Vec<[f32; 3]>,
    /// CIELAB under D65: `L` in `0..=100`, `a` and `b` roughly `-128..=127`.
    pub lab: Vec<[f32; 3]>,
    /// BT.601 luma of the encoded values, as the cascade was trained on.
    pub grey: Vec<u8>,
}

/// Which primaries a linear buffer is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primaries {
    /// Phase 14's working space: linear Rec.2020, D65.
    Rec2020,
    /// Linear sRGB / Rec.709, D65.
    Srgb,
}

impl Canvas {
    /// Build a canvas from interleaved linear RGB, downsampled so the long edge is at most
    /// `max_edge`.
    ///
    /// Returns `None` for an empty or truncated buffer.
    #[must_use]
    pub fn from_linear(
        rgb: &[f32],
        width: u32,
        height: u32,
        primaries: Primaries,
        max_edge: u32,
    ) -> Option<Self> {
        let pixels = (width as usize).checked_mul(height as usize)?;
        if pixels == 0 || rgb.len() < pixels * 3 {
            return None;
        }
        let (w, h) = fit(width, height, max_edge);
        let small = box_downsample(rgb, width, height, w, h);
        let to_srgb = match primaries {
            Primaries::Rec2020 => Some(working_space::rec2020_to_srgb()),
            Primaries::Srgb => None,
        };
        let linear: Vec<[f32; 3]> = small
            .par_chunks_exact(3)
            .map(|p| {
                let v = [
                    p.first().copied().unwrap_or(0.0),
                    p.get(1).copied().unwrap_or(0.0),
                    p.get(2).copied().unwrap_or(0.0),
                ];
                match to_srgb {
                    Some(m) => {
                        let out =
                            matrix::apply(m, [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]);
                        [out[0] as f32, out[1] as f32, out[2] as f32]
                    }
                    None => v,
                }
            })
            .collect();
        Some(Self::from_linear_srgb(&linear, w, h))
    }

    /// Build a canvas from interleaved 8-bit sRGB, downsampled so the long edge is at most
    /// `max_edge`. The bytes are decoded to linear before the downsample.
    #[must_use]
    pub fn from_srgb8(bytes: &[u8], width: u32, height: u32, max_edge: u32) -> Option<Self> {
        let pixels = (width as usize).checked_mul(height as usize)?;
        if pixels == 0 || bytes.len() < pixels * 3 {
            return None;
        }
        let mut lut = [0.0_f32; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            *slot = curve::srgb_decode(i as f32 / 255.0);
        }
        let linear: Vec<f32> = bytes
            .iter()
            .take(pixels * 3)
            .map(|b| lut.get(usize::from(*b)).copied().unwrap_or(0.0))
            .collect();
        Self::from_linear(&linear, width, height, Primaries::Srgb, max_edge)
    }

    fn from_linear_srgb(linear: &[[f32; 3]], width: u32, height: u32) -> Self {
        let converted: Vec<([f32; 3], [f32; 3], u8)> = linear
            .par_iter()
            .map(|p| {
                let clamped = [
                    p[0].clamp(0.0, 1.0),
                    p[1].clamp(0.0, 1.0),
                    p[2].clamp(0.0, 1.0),
                ];
                let encoded = [
                    curve::srgb_encode(clamped[0]),
                    curve::srgb_encode(clamped[1]),
                    curve::srgb_encode(clamped[2]),
                ];
                let lab = linear_srgb_to_lab(clamped);
                let grey = (255.0 * (0.299 * encoded[0] + 0.587 * encoded[1] + 0.114 * encoded[2]))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                (encoded, lab, grey)
            })
            .collect();
        let mut srgb = Vec::with_capacity(converted.len());
        let mut lab = Vec::with_capacity(converted.len());
        let mut grey = Vec::with_capacity(converted.len());
        for (s, l, g) in converted {
            srgb.push(s);
            lab.push(l);
            grey.push(g);
        }
        Self {
            width,
            height,
            srgb,
            lab,
            grey,
        }
    }

    /// Pixel index, clamped into the frame.
    #[must_use]
    pub fn index(&self, x: i64, y: i64) -> usize {
        let cx = x.clamp(0, i64::from(self.width.max(1)) - 1) as usize;
        let cy = y.clamp(0, i64::from(self.height.max(1)) - 1) as usize;
        cy * self.width as usize + cx
    }

    /// Lab at a pixel, clamped into the frame.
    #[must_use]
    pub fn lab_at(&self, x: i64, y: i64) -> [f32; 3] {
        self.lab.get(self.index(x, y)).copied().unwrap_or([0.0; 3])
    }

    /// Encoded sRGB at a pixel, clamped into the frame.
    #[must_use]
    pub fn srgb_at(&self, x: i64, y: i64) -> [f32; 3] {
        self.srgb.get(self.index(x, y)).copied().unwrap_or([0.0; 3])
    }

    /// `Y`, `Cb`, `Cr` in `0..=255` at a pixel index, BT.601 full range.
    #[must_use]
    pub fn ycbcr(&self, index: usize) -> [f32; 3] {
        let p = self.srgb.get(index).copied().unwrap_or([0.0; 3]);
        let r = p[0] * 255.0;
        let g = p[1] * 255.0;
        let b = p[2] * 255.0;
        [
            0.299 * r + 0.587 * g + 0.114 * b,
            128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b,
            128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b,
        ]
    }

    /// Number of pixels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lab.len()
    }

    /// True when the canvas has no pixels.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lab.is_empty()
    }

    /// True when the frame's colour can say anything about skin: at least a tenth of its
    /// pixels carry visible chroma. A black-and-white or heavily desaturated frame is judged on
    /// structure alone, and every colour test in the crate stands down rather than rejecting
    /// every face for being grey.
    #[must_use]
    pub fn is_colourful(&self) -> bool {
        let coloured = self
            .lab
            .iter()
            .filter(|lab| lab[1].hypot(lab[2]) > 6.0)
            .count();
        !self.lab.is_empty() && coloured * 10 >= self.lab.len()
    }

    /// The median chroma of the frame. Near zero for a black-and-white photograph, which
    /// switches off every colour test rather than letting it reject every face.
    #[must_use]
    pub fn median_chroma(&self) -> f32 {
        let mut histogram = [0_usize; 128];
        for lab in &self.lab {
            let c = lab[1].hypot(lab[2]).clamp(0.0, 127.0) as usize;
            if let Some(slot) = histogram.get_mut(c) {
                *slot += 1;
            }
        }
        let half = self.lab.len() / 2;
        let mut seen = 0;
        for (bin, count) in histogram.iter().enumerate() {
            seen += count;
            if seen > half {
                return bin as f32;
            }
        }
        0.0
    }
}

/// The analysis size for a frame: the long edge at most `max_edge`, never upscaled.
#[must_use]
pub fn fit(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= max_edge || long == 0 {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(max_edge) / f64::from(long);
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}

/// Box-filtered downsample of interleaved RGB. Exact area weights, so a 2048 px proxy and a
/// 6000 px original produce the same canvas to within their own decode differences.
fn box_downsample(rgb: &[f32], width: u32, height: u32, w: u32, h: u32) -> Vec<f32> {
    if w == width && h == height {
        return rgb
            .iter()
            .take((w as usize) * (h as usize) * 3)
            .copied()
            .collect();
    }
    let sx = f64::from(width) / f64::from(w);
    let sy = f64::from(height) / f64::from(h);
    let src_w = width as usize;
    let mut out = vec![0.0_f32; (w as usize) * (h as usize) * 3];
    out.par_chunks_mut((w as usize) * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let fy0 = y as f64 * sy;
            let fy1 = (y as f64 + 1.0) * sy;
            for x in 0..w as usize {
                let fx0 = x as f64 * sx;
                let fx1 = (x as f64 + 1.0) * sx;
                let mut acc = [0.0_f64; 3];
                let mut total = 0.0_f64;
                let mut yy = fy0.floor() as usize;
                while (yy as f64) < fy1 && yy < height as usize {
                    let wy = (fy1.min(yy as f64 + 1.0) - fy0.max(yy as f64)).max(0.0);
                    let mut xx = fx0.floor() as usize;
                    while (xx as f64) < fx1 && xx < src_w {
                        let wx = (fx1.min(xx as f64 + 1.0) - fx0.max(xx as f64)).max(0.0);
                        let weight = wx * wy;
                        let base = (yy * src_w + xx) * 3;
                        for (c, slot) in acc.iter_mut().enumerate() {
                            *slot += weight * f64::from(rgb.get(base + c).copied().unwrap_or(0.0));
                        }
                        total += weight;
                        xx += 1;
                    }
                    yy += 1;
                }
                for (c, value) in acc.iter().enumerate() {
                    if let Some(slot) = row.get_mut(x * 3 + c) {
                        *slot = if total > 0.0 {
                            (value / total) as f32
                        } else {
                            0.0
                        };
                    }
                }
            }
        });
    out
}

/// Linear sRGB to CIELAB under D65.
#[must_use]
pub fn linear_srgb_to_lab(rgb: [f32; 3]) -> [f32; 3] {
    let xyz = matrix::apply(
        matrix::SRGB_TO_XYZ_D65,
        [f64::from(rgb[0]), f64::from(rgb[1]), f64::from(rgb[2])],
    );
    let f = |t: f64| -> f64 {
        const DELTA: f64 = 6.0 / 29.0;
        if t > DELTA * DELTA * DELTA {
            t.cbrt()
        } else {
            t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
        }
    };
    let fx = f(xyz[0] / 0.950_47);
    let fy = f(xyz[1]);
    let fz = f(xyz[2] / 1.088_83);
    [
        (116.0 * fy - 16.0) as f32,
        (500.0 * (fx - fy)) as f32,
        (200.0 * (fy - fz)) as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_is_l100_and_neutral() {
        let lab = linear_srgb_to_lab([1.0, 1.0, 1.0]);
        assert!((lab[0] - 100.0).abs() < 0.05);
        assert!(lab[1].abs() < 0.05 && lab[2].abs() < 0.05);
    }

    #[test]
    fn a_large_frame_is_brought_to_the_analysis_edge() {
        assert_eq!(fit(4000, 3000, 1024), (1024, 768));
        assert_eq!(fit(300, 200, 1024), (300, 200));
    }

    #[test]
    fn the_downsample_averages_in_linear_light() {
        // One black and one white column, averaged: linear 0.5, which encodes to ~0.735,
        // not the 0.5 that averaging encoded values would give.
        let rgb = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let canvas = Canvas::from_linear(&rgb, 2, 1, Primaries::Srgb, 1).unwrap();
        assert_eq!(canvas.width, 1);
        assert!((canvas.srgb[0][0] - 0.735).abs() < 0.01);
    }

    #[test]
    fn a_grey_frame_has_no_chroma() {
        let canvas = Canvas::from_srgb8(&[128; 30], 10, 1, 64).unwrap();
        assert!(canvas.median_chroma() < 1.0);
        assert_eq!(canvas.grey[0], 128);
    }
}
