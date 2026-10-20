//! Edge-aware base layers: local Shadows and Highlights, halo-free Clarity, and a physical
//! Dehaze. ADR-0126.
//!
//! # Why a base layer
//!
//! A per-pixel tone curve cannot lift a shadow without flattening it. Every pixel of a dark
//! wall moves by the amount its *own* brightness asks for, so the brighter grain of the wall
//! moves less than the darker grain and the texture is pressed together. What a photographer
//! means by "lift the shadows" is "lift the dark *regions*", and a region is a property of a
//! neighbourhood rather than of a pixel.
//!
//! So the region is measured. An edge-preserving smoothing of the frame's log luminance - a
//! **guided filter** (He, Sun and Tang, 2010/2013) - gives every pixel the brightness of the
//! region it belongs to, following the strong edges between regions (a window and the wall
//! around it) and averaging away the texture inside them (the grain of the wall). The tone
//! sliders then read that *base* and apply one gain to the region, and the detail riding on
//! it is carried along unchanged. That is the difference between a Lightroom-class Shadows
//! slider and a curve.
//!
//! The same base, at a smaller radius, is what Clarity needs: the detail that clarity boosts
//! is the frame minus an edge-aware base, so a strong edge is not in the detail and gets no
//! halo. And the same filter refines a dark-channel transmission map, which is what turns
//! Dehaze from a global black-point shift into a correction that removes more haze from the
//! distance than from the foreground.
//!
//! # Measured once over the whole frame, applied point-wise
//!
//! The coefficients of the filter are measured on a fixed reduction of the whole frame
//! ([`BASE_EDGE`] on its long edge) and stored as a low-resolution [`Grid`]. A pixel's base is
//! then `a * guide + b`, with `a` and `b` read bilinearly from the grid at the pixel's
//! position *in the whole frame* and `guide` the pixel's own full-resolution value - the
//! "fast guided filter" of He and Sun (2015). Three properties follow, and each is why it is
//! done this way:
//!
//! - **A proxy and an export look the same.** The radius is a fraction of the frame, measured
//!   on the same reduction at every render level, so a 2,048-pixel preview and a 6,000-pixel
//!   export get the same regions. A blur radius in render pixels does not have this property.
//! - **A tiled render is bit-identical to a whole one.** The grid is a frame-wide measurement,
//!   like [`crate::spatial::Stats`], and applying it reads only the pixel itself and its
//!   absolute position. The tiler measures once and hands every tile the same grid.
//! - **The edges are full resolution.** Only the smooth coefficients are low resolution; the
//!   guide is the pixel, so a hard edge in the export stays exactly where it is.

use rayon::prelude::*;

use crate::colour::{luma, set_luma};
use crate::spatial::Position;
use crate::tonemap::{curve_domain_encode, MID_GREY};

/// Version of the edge-aware stages. Folded into the engine string, because a change here
/// is a change to every photograph that uses Shadows, Highlights, Clarity or Dehaze.
pub const LOCAL_VER: u32 = 1;

/// Long edge of the reduction every grid is measured on.
pub const BASE_EDGE: usize = 512;

/// The tone base's radius, as a fraction of the reduction's long edge.
///
/// Large: a tone region is a wall, a sky, a face in shadow. Too small and Shadows becomes a
/// second Clarity; too large and the filter has to cross edges it should stop at.
pub const TONE_RADIUS: f32 = 0.035;

/// The tone base's edge threshold, in squared stops. An edge whose two sides differ by much
/// more than `sqrt(eps)` stops is kept in the base; texture well below it is averaged away.
pub const TONE_EPS: f64 = 0.10;

/// Clarity's radius, as a fraction of the reduction's long edge.
pub const CLARITY_RADIUS: f32 = 0.02;

/// Clarity's edge threshold, in squared stops. Larger than the tone base's, because clarity
/// is *for* mid-scale structure: an edge of half a stop is detail to clarity and a region
/// boundary to Shadows.
pub const CLARITY_EPS: f64 = 0.4;

/// Where clarity starts to step aside for a strong edge, as the filter's `a` coefficient.
pub const CLARITY_EDGE_FROM: f32 = 0.35;
/// Where it has stepped aside entirely.
pub const CLARITY_EDGE_FULL: f32 = 0.85;

/// How much of a region's detail Clarity +100 adds, before the soft limit.
pub const CLARITY_GAIN: f32 = 1.5;

/// The dark-channel patch, as a fraction of the reduction's long edge.
pub const HAZE_PATCH: f32 = 0.010;

/// The transmission refinement radius, as a fraction of the reduction's long edge.
pub const HAZE_RADIUS: f32 = 0.030;

/// The transmission refinement's edge threshold, on a guide in the curve domain.
pub const HAZE_EPS: f64 = 1.0e-3;

/// The share of the frame with the densest dark channel that the airlight is read from.
pub const AIRLIGHT_SHARE: f32 = 0.001;

/// The smallest transmission a dehaze divides by. He et al. use 0.1; the floor is what stops
/// a sky that is "all haze" from being amplified into noise.
pub const TRANSMISSION_FLOOR: f32 = 0.12;

/// A frame is *clipped* when at least this share of it sits at its own maximum. A JPEG with
/// a blown sky does; a raw frame whose highlights were reconstructed does not.
pub const CLIP_SHARE: f32 = 0.002;

/// Where a region starts to count as clipped: its brightest channel's display value as a
/// share of the ceiling's, through the same shoulder the output uses.
///
/// Measured in display terms rather than in stops because of what a display-referred file
/// is: the inverse of the shoulder stretches sRGB codes 250 to 255 across more than a stop
/// of working space, so a few codes of JPEG noise near white would read as a stop of
/// "detail" in linear terms - and pulling it down draws the JPEG's blocks. Through the
/// shoulder the same codes sit within a few per cent of the ceiling, which is what they are.
/// On a raw frame the same ratio is the top half-stop below the sensor's clip.
pub const CLIP_FROM: f32 = 0.86;
/// Where a region counts as fully clipped, on the same scale.
pub const CLIP_FULL: f32 = 0.985;

/// How much of Highlights a fully clipped region still receives. A clipped region has no
/// detail to recover, so pulling it down only turns white into flat grey.
pub const CLIPPED_RECOVERY: f32 = 0.05;

/// How far the brightest stop of shadow can be lifted at Shadows +100, in stops.
pub const SHADOW_LIFT: f32 = 1.5;
/// How far Shadows -100 can deepen them.
pub const SHADOW_DEEPEN: f32 = 1.1;
/// How far Highlights -100 can bring a bright region down, in stops.
pub const HIGHLIGHT_RECOVER: f32 = 1.1;
/// How far Highlights +100 can brighten a bright region.
pub const HIGHLIGHT_LIFT: f32 = 0.7;

/// Luminance floor for the log domain: about twenty stops below white.
const LOG_FLOOR: f32 = 1.0e-6;

/// The log2 luminance a base is measured in.
#[must_use]
pub fn log_luma(rgb: [f32; 3]) -> f32 {
    luma(rgb).max(LOG_FLOOR).log2()
}

/// The perceptual guide the haze filter follows: luminance in the curve domain.
#[must_use]
pub fn haze_guide(rgb: [f32; 3]) -> f32 {
    curve_domain_encode(luma(rgb).max(0.0)).min(2.0)
}

fn px(rgb: &[f32], index: usize) -> [f32; 3] {
    let base = index * 3;
    [
        rgb.get(base).copied().unwrap_or(0.0),
        rgb.get(base + 1).copied().unwrap_or(0.0),
        rgb.get(base + 2).copied().unwrap_or(0.0),
    ]
}

fn write(pixel: &mut [f32], value: [f32; 3]) {
    for (slot, out) in pixel.iter_mut().zip(value.iter()) {
        *slot = *out;
    }
}

/// A radius in reduction pixels, from a fraction of the reduction's long edge.
#[must_use]
pub fn radius_for(width: usize, height: usize, fraction: f32) -> usize {
    ((width.max(height) as f32 * fraction).round() as usize).max(1)
}

/// Reduce an interleaved RGB buffer so its long edge is at most `edge`, by area averaging.
///
/// Area averaging rather than the bilinear `spatial::resample`, because a base layer is a
/// statistic of each region and a bilinear reduction by a factor of six reads one pixel in
/// thirty-six - which on a noisy or finely patterned frame is a noisy base. Every output pixel
/// sums its own source rectangle in a fixed order, so the result does not depend on the
/// thread count.
#[must_use]
pub fn reduce(rgb: &[f32], width: usize, height: usize, edge: usize) -> (Vec<f32>, usize, usize) {
    let long = width.max(height);
    if width == 0 || height == 0 || long <= edge || edge == 0 {
        return (rgb.to_vec(), width, height);
    }
    let scale = edge as f64 / long as f64;
    let out_w = ((width as f64 * scale).round() as usize).max(1);
    let out_h = ((height as f64 * scale).round() as usize).max(1);
    let bounds = |i: usize, out: usize, full: usize| -> (usize, usize) {
        let start = i * full / out;
        let end = ((i + 1) * full / out).max(start + 1).min(full);
        (start, end)
    };
    let mut out = vec![0.0f32; out_w * out_h * 3];
    out.par_chunks_mut(out_w * 3)
        .enumerate()
        .for_each(|(oy, row)| {
            let (y0, y1) = bounds(oy, out_h, height);
            for (ox, pixel) in row.chunks_mut(3).enumerate() {
                let (x0, x1) = bounds(ox, out_w, width);
                let mut sum = [0.0f64; 3];
                for y in y0..y1 {
                    for x in x0..x1 {
                        let value = px(rgb, y * width + x);
                        for (s, v) in sum.iter_mut().zip(value.iter()) {
                            *s += f64::from(*v);
                        }
                    }
                }
                let count = ((y1 - y0) * (x1 - x0)).max(1) as f64;
                write(pixel, sum.map(|s| (s / count) as f32));
            }
        });
    (out, out_w, out_h)
}

/// The mean of every `(2r + 1)`-square window, truncated at the plane's edges and divided by
/// the samples actually inside it.
///
/// Constant time per pixel: horizontal window sums from one prefix sum per row (rows in
/// parallel), then a running prefix down the rows, added a whole row at a time. Every sum is
/// accumulated in the same order whatever the thread count, so the same plane always gives
/// the same means.
fn box_mean(plane: &[f64], width: usize, height: usize, r: usize) -> Vec<f64> {
    let n = width * height;
    if n == 0 {
        return Vec::new();
    }
    let window = |i: usize, len: usize| (i.saturating_sub(r), (i + r + 1).min(len));
    let mut rows = vec![0.0f64; n];
    rows.par_chunks_mut(width)
        .zip(plane.par_chunks(width))
        .for_each(|(out, src)| {
            let mut prefix = Vec::with_capacity(width + 1);
            let mut acc = 0.0f64;
            prefix.push(acc);
            for v in src {
                acc += v;
                prefix.push(acc);
            }
            for (x, slot) in out.iter_mut().enumerate() {
                let (x0, x1) = window(x, width);
                *slot =
                    prefix.get(x1).copied().unwrap_or(0.0) - prefix.get(x0).copied().unwrap_or(0.0);
            }
        });
    // Column prefix sums: row y + 1 of `prefix` is the sum of rows 0 ..= y.
    let mut prefix = vec![0.0f64; width * (height + 1)];
    for y in 0..height {
        let (done, rest) = prefix.split_at_mut((y + 1) * width);
        let previous = done.get(y * width..).unwrap_or(&[]);
        let current = rows.get(y * width..(y + 1) * width).unwrap_or(&[]);
        if let Some(next) = rest.get_mut(..width) {
            for ((slot, above), add) in next.iter_mut().zip(previous).zip(current) {
                *slot = above + add;
            }
        }
    }
    let mut out = vec![0.0f64; n];
    out.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        let (y0, y1) = window(y, height);
        for (x, slot) in row.iter_mut().enumerate() {
            let (x0, x1) = window(x, width);
            let total = prefix.get(y1 * width + x).copied().unwrap_or(0.0)
                - prefix.get(y0 * width + x).copied().unwrap_or(0.0);
            *slot = total / ((x1 - x0) * (y1 - y0)).max(1) as f64;
        }
    });
    out
}

/// A low-resolution field of guided-filter coefficients covering the whole frame.
///
/// The filtered value of a pixel is `a * guide + b`, with `a` and `b` read bilinearly at the
/// pixel's position in the whole frame. See the module documentation.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    width: usize,
    height: usize,
    a: Vec<f32>,
    b: Vec<f32>,
}

impl Grid {
    /// The grid's own size.
    #[must_use]
    pub const fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// A reader for a buffer at `position` inside the whole frame.
    #[must_use]
    pub fn sampler(&self, position: Position) -> Sampler<'_> {
        Sampler {
            grid: self,
            sx: self.width as f32 / position.full_width.max(1) as f32,
            sy: self.height as f32 / position.full_height.max(1) as f32,
            ox: position.x as usize,
            oy: position.y as usize,
        }
    }

    fn coefficients(&self, gx: f32, gy: f32) -> (f32, f32) {
        let (w, h) = (self.width.max(1), self.height.max(1));
        let clamp = |v: f32, n: usize| -> (usize, usize, f32) {
            let v = v.max(0.0);
            let i0 = (v.floor() as usize).min(n - 1);
            let i1 = (i0 + 1).min(n - 1);
            let f = (v - i0 as f32).clamp(0.0, 1.0);
            (i0, i1, f)
        };
        let (x0, x1, fx) = clamp(gx, w);
        let (y0, y1, fy) = clamp(gy, h);
        let lerp = |field: &[f32]| -> f32 {
            let at = |x: usize, y: usize| field.get(y * w + x).copied().unwrap_or(0.0);
            let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
            top * (1.0 - fy) + bottom * fy
        };
        (lerp(&self.a), lerp(&self.b))
    }
}

/// Reads a [`Grid`] for one buffer. Built once per buffer, so the per-pixel work is a
/// bilinear read and one multiply-add.
#[derive(Debug, Clone, Copy)]
pub struct Sampler<'a> {
    grid: &'a Grid,
    sx: f32,
    sy: f32,
    ox: usize,
    oy: usize,
}

impl Sampler<'_> {
    /// The filtered value at buffer pixel `(x, y)` whose own guide value is `guide`.
    ///
    /// The position is converted to whole-frame coordinates first, which is what makes a tile
    /// and a whole render read the same coefficients for the same pixel.
    #[must_use]
    pub fn at(&self, x: usize, y: usize, guide: f32) -> f32 {
        let (a, b) = self.coefficients(x, y);
        a * guide + b
    }

    /// The filter's two coefficients at buffer pixel `(x, y)`.
    ///
    /// `a` is also a measurement in its own right: it is near one where the windows around a
    /// pixel straddle an edge much stronger than the filter's threshold, and near zero in a
    /// flat or finely textured region.
    #[must_use]
    pub fn coefficients(&self, x: usize, y: usize) -> (f32, f32) {
        let gx = ((x + self.ox) as f32 + 0.5) * self.sx - 0.5;
        let gy = ((y + self.oy) as f32 + 0.5) * self.sy - 0.5;
        self.grid.coefficients(gx, gy)
    }
}

/// The guided filter: the coefficients that make `a * guide + b` the edge-preserving
/// smoothing of `src`, averaged over every window that contains each pixel. `None` filters
/// the guide against itself, which needs half the window sums.
fn guided(
    guide: &[f32],
    src: Option<&[f32]>,
    width: usize,
    height: usize,
    r: usize,
    eps: f64,
) -> Grid {
    let n = width * height;
    let g: Vec<f64> = guide.iter().take(n).map(|v| f64::from(*v)).collect();
    let gg: Vec<f64> = g.iter().map(|v| v * v).collect();
    let mean_g = box_mean(&g, width, height, r);
    let mean_gg = box_mean(&gg, width, height, r);
    let (mean_p, mean_gp) = if let Some(src) = src {
        let p: Vec<f64> = src.iter().take(n).map(|v| f64::from(*v)).collect();
        let gp: Vec<f64> = g.iter().zip(p.iter()).map(|(a, b)| a * b).collect();
        (
            Some(box_mean(&p, width, height, r)),
            Some(box_mean(&gp, width, height, r)),
        )
    } else {
        (None, None)
    };
    let mean_p = mean_p.as_ref().unwrap_or(&mean_g);
    let mean_gp = mean_gp.as_ref().unwrap_or(&mean_gg);

    let mut a = vec![0.0f64; n];
    let mut b = vec![0.0f64; n];
    a.par_iter_mut()
        .zip(b.par_iter_mut())
        .enumerate()
        .for_each(|(i, (sa, sb))| {
            let mg = mean_g.get(i).copied().unwrap_or(0.0);
            let mp = mean_p.get(i).copied().unwrap_or(0.0);
            let var = (mean_gg.get(i).copied().unwrap_or(0.0) - mg * mg).max(0.0);
            let cov = mean_gp.get(i).copied().unwrap_or(0.0) - mg * mp;
            *sa = cov / (var + eps);
            *sb = mp - *sa * mg;
        });
    let mean_a = box_mean(&a, width, height, r);
    let mean_b = box_mean(&b, width, height, r);
    Grid {
        width,
        height,
        a: mean_a.iter().map(|v| *v as f32).collect(),
        b: mean_b.iter().map(|v| *v as f32).collect(),
    }
}

/// The tone base of a frame, and where it is clipped.
#[derive(Debug, Clone, PartialEq)]
pub struct ToneBase {
    /// The edge-preserving smoothing of log luminance.
    pub grid: Grid,
    /// How clipped each region is, `0 ..= 1`, filtered against the same guide. `None` for a
    /// frame that is not clipped.
    pub clipped: Option<Grid>,
}

impl ToneBase {
    /// A reader for a buffer at `position` inside the whole frame.
    #[must_use]
    pub fn sampler(&self, position: Position) -> ToneSampler<'_> {
        ToneSampler {
            base: self.grid.sampler(position),
            clipped: self.clipped.as_ref().map(|grid| grid.sampler(position)),
        }
    }
}

/// Reads a [`ToneBase`] for one buffer.
#[derive(Debug, Clone, Copy)]
pub struct ToneSampler<'a> {
    base: Sampler<'a>,
    clipped: Option<Sampler<'a>>,
}

impl ToneSampler<'_> {
    /// The base of the region the pixel at `(x, y)` belongs to, in log2 luminance.
    #[must_use]
    pub fn base_at(&self, x: usize, y: usize, rgb: [f32; 3]) -> f32 {
        self.base.at(x, y, log_luma(rgb))
    }

    /// How clipped the region the pixel at `(x, y)` belongs to is, `0 ..= 1`.
    #[must_use]
    pub fn clipped_at(&self, x: usize, y: usize, rgb: [f32; 3]) -> f32 {
        self.clipped
            .map_or(0.0, |s| s.at(x, y, log_luma(rgb)).clamp(0.0, 1.0))
    }

    /// Local Shadows and Highlights on the pixel at `(x, y)`.
    #[must_use]
    pub fn apply(
        &self,
        x: usize,
        y: usize,
        rgb: [f32; 3],
        shadows: f32,
        highlights: f32,
    ) -> [f32; 3] {
        let log = log_luma(rgb);
        let base = self.base.at(x, y, log);
        let clipped = self
            .clipped
            .map_or(0.0, |s| s.at(x, y, log).clamp(0.0, 1.0));
        let highlights = if highlights < 0.0 {
            highlights * (1.0 - (1.0 - CLIPPED_RECOVERY) * clipped)
        } else {
            highlights
        };
        tone_pixel(rgb, base, shadows, highlights)
    }
}

/// The tone base of a reduced frame: an edge-preserving smoothing of its log luminance, and
/// a map of the regions that are clipped.
///
/// `exposure` is added to every log value, because the main pass reads the base after the
/// exposure stage and the reduction is measured before it.
#[must_use]
pub fn tone_grid(rgb: &[f32], width: usize, height: usize, exposure: f32) -> ToneBase {
    let n = width * height;
    let guide: Vec<f32> = (0..n).map(|i| log_luma(px(rgb, i)) + exposure).collect();
    let r = radius_for(width, height, TONE_RADIUS);
    let grid = guided(&guide, None, width, height, r, TONE_EPS);

    // Clipping is a spike at the frame's own maximum: a share of pixels at the ceiling the
    // file could record. Between CLIP_FROM and CLIP_FULL of it a pixel counts as progressively
    // more clipped, and the measurement is smoothed at region scale so the protection has no
    // contour of its own.
    let peaks: Vec<f32> = (0..n)
        .map(|i| px(rgb, i).iter().copied().fold(0.0f32, f32::max))
        .collect();
    let ceiling = peaks.iter().copied().fold(0.0f32, f32::max);
    let at_ceiling = peaks.iter().filter(|v| **v >= ceiling * 0.9995).count();
    let clipped = (ceiling > 0.0 && at_ceiling as f32 >= n as f32 * CLIP_SHARE).then(|| {
        let top = aura_raw::colour::curve::filmic_lite(ceiling).max(LOG_FLOOR);
        let near: Vec<f32> = peaks
            .iter()
            .map(|v| {
                let share = aura_raw::colour::curve::filmic_lite(*v) / top;
                step(share, CLIP_FROM, CLIP_FULL)
            })
            .collect();
        guided(&guide, Some(&near), width, height, (r / 2).max(1), 0.05)
    });
    ToneBase { grid, clipped }
}

/// How a frame's tones are distributed over its *regions* rather than its pixels.
///
/// A histogram counts pixels, so a bright window and the grain of a dark wall look alike.
/// Read through the tone base, each pixel reports the brightness of the region it belongs
/// to, which is what Shadows and Highlights act on - so these are the numbers an automatic
/// edit should size them from. All brightnesses are stops from middle grey.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Regions {
    /// The 3rd percentile of region brightness.
    pub low: f32,
    /// The median region brightness.
    pub median: f32,
    /// The 97th percentile of region brightness.
    pub high: f32,
    /// Share of the frame in regions more than [`DEEP_SHADOW`] stops below middle grey.
    pub dark_share: f32,
    /// The median brightness of those regions.
    pub dark_depth: f32,
    /// Share of the frame in bright regions that still carry detail: above
    /// [`BRIGHT_REGION`] stops and not clipped.
    pub bright_share: f32,
    /// Share of the frame in regions that are clipped.
    pub clipped_share: f32,
}

impl Regions {
    /// Stops between the darkest and brightest regions.
    #[must_use]
    pub fn spread(&self) -> f32 {
        self.high - self.low
    }
}

/// Regions this far below middle grey are deep shadow: about sRGB 22% and darker.
pub const DEEP_SHADOW: f32 = -3.0;
/// Regions this far above middle grey are bright: about sRGB 82% and brighter.
pub const BRIGHT_REGION: f32 = 1.3;

/// Measure how a working-space frame's tones fall into regions, after `exposure`.
#[must_use]
pub fn regions(rgb: &[f32], width: usize, height: usize, exposure: f32) -> Regions {
    let (small, w, h) = reduce(rgb, width, height, BASE_EDGE);
    let n = w * h;
    if n == 0 {
        return Regions::default();
    }
    let base = tone_grid(&small, w, h, exposure);
    let sampler = base.sampler(Position::whole(w as u32, h as u32));
    let gain = exposure.exp2();
    let grey = MID_GREY.log2();
    let mut stops = Vec::with_capacity(n);
    let mut dark = Vec::new();
    let (mut bright, mut clipped) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let p = px(&small, y * w + x).map(|v| v * gain);
            let b = sampler.base_at(x, y, p) - grey;
            let c = sampler.clipped_at(x, y, p);
            if c >= 0.5 {
                clipped += 1;
            } else if b > BRIGHT_REGION {
                bright += 1;
            }
            if b < DEEP_SHADOW {
                dark.push(b);
            }
            stops.push(b);
        }
    }
    let at = |v: &mut Vec<f32>, q: f32| -> f32 {
        if v.is_empty() {
            return 0.0;
        }
        v.sort_by(f32::total_cmp);
        let i = ((v.len() - 1) as f32 * q).round() as usize;
        v.get(i).copied().unwrap_or(0.0)
    };
    let share = |k: usize| k as f32 / n as f32;
    let dark_share = share(dark.len());
    Regions {
        low: at(&mut stops, 0.03),
        median: at(&mut stops, 0.5),
        high: at(&mut stops, 0.97),
        dark_share,
        dark_depth: at(&mut dark, 0.5),
        bright_share: share(bright),
        clipped_share: share(clipped),
    }
}

/// Clarity's base of a reduced frame: the same filter at a smaller radius.
#[must_use]
pub fn clarity_grid(rgb: &[f32], width: usize, height: usize) -> Grid {
    let guide: Vec<f32> = (0..width * height).map(|i| log_luma(px(rgb, i))).collect();
    let r = radius_for(width, height, CLARITY_RADIUS);
    guided(&guide, None, width, height, r, CLARITY_EPS)
}

/// Histogram resolution for finding the densest share of the dark channel.
const BINS: usize = 4096;

/// The brightest display-linear value the haze model writes. Just short of the shoulder's
/// asymptote, so the way back into the working space stays finite.
const DISPLAY_WHITE: f32 = 0.9995;

/// Working space to display-linear, per channel, through the output's shoulder.
fn to_display(v: f32) -> f32 {
    aura_raw::colour::curve::filmic_lite(v)
}

/// Display-linear back to the working space.
fn from_display(v: f32) -> f32 {
    aura_raw::colour::curve::filmic_lite_inverse(v.clamp(0.0, DISPLAY_WHITE))
}

/// The haze in a frame: the colour of the airlight and a refined map of how much of it
/// each pixel carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Haze {
    /// The airlight, display-linear, as measured where the haze is densest.
    pub airlight: [f32; 3],
    /// Haze density: `0` is a clear pixel and `1` a pixel that is all airlight. Filtered
    /// against [`haze_guide`].
    density: Grid,
}

/// A separable minimum filter of radius `r`, clamped at the edges.
fn min_filter(plane: &[f32], width: usize, height: usize, r: usize) -> Vec<f32> {
    let mut rows = vec![0.0f32; width * height];
    rows.par_chunks_mut(width.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, slot) in row.iter_mut().enumerate() {
                let x0 = x.saturating_sub(r);
                let x1 = (x + r + 1).min(width);
                *slot = (x0..x1)
                    .map(|xx| plane.get(y * width + xx).copied().unwrap_or(f32::MAX))
                    .fold(f32::MAX, f32::min);
            }
        });
    let mut out = vec![0.0f32; width * height];
    out.par_chunks_mut(width.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            let y0 = y.saturating_sub(r);
            let y1 = (y + r + 1).min(height);
            for (x, slot) in row.iter_mut().enumerate() {
                *slot = (y0..y1)
                    .map(|yy| rows.get(yy * width + x).copied().unwrap_or(f32::MAX))
                    .fold(f32::MAX, f32::min);
            }
        });
    out
}

/// Measure the haze in a reduced frame, by the dark channel prior (He, Sun and Tang, 2009).
///
/// In a clear photograph almost every patch has *some* channel near black - a shadow, a
/// saturated colour, a dark object. Haze adds airlight to every channel, so the darkest
/// channel of a patch measures how much airlight it carries. The airlight's colour is read
/// where that measurement is highest, and the per-patch measurement is refined with the
/// guided filter so its edges follow the photograph's rather than the patch grid's.
///
/// `None` for a frame with no usable airlight: black, or with no pixel whose dark channel
/// rises above the noise.
///
/// The model runs in **display-linear** terms - each channel through the output's shoulder,
/// so white is 1 - rather than in the scene-linear working space. Haze is a veil over what a
/// viewer sees, and the working space has no white: a display-referred file's clipped sky
/// unrolls to four times diffuse white, and an airlight read there makes every real veil
/// look like a fortieth of one.
#[must_use]
pub fn haze(rgb: &[f32], width: usize, height: usize) -> Option<Haze> {
    let n = width * height;
    if n == 0 {
        return None;
    }
    let display: Vec<f32> = rgb.iter().map(|v| to_display(*v)).collect();
    let guide: Vec<f32> = (0..n).map(|i| haze_guide(px(rgb, i))).collect();
    let rgb = display.as_slice();
    let dark: Vec<f32> = (0..n)
        .map(|i| {
            let p = px(rgb, i);
            p.iter().copied().fold(f32::MAX, f32::min).max(0.0)
        })
        .collect();
    let patch = radius_for(width, height, HAZE_PATCH);
    let channel = min_filter(&dark, width, height, patch);

    // The airlight: the brightest pixels among the densest share of the dark channel. A
    // histogram rather than a sort, so ties cannot reorder anything.
    let peak = channel.iter().copied().fold(0.0f32, f32::max);
    if peak <= 1.0e-4 {
        return None;
    }
    let bin_of = |v: f32| {
        ((v / peak) * (BINS - 1) as f32)
            .round()
            .clamp(0.0, (BINS - 1) as f32) as usize
    };
    let mut histogram = vec![0usize; BINS];
    for v in &channel {
        if let Some(slot) = histogram.get_mut(bin_of(*v)) {
            *slot += 1;
        }
    }
    let wanted = ((n as f32 * AIRLIGHT_SHARE).ceil() as usize).max(1);
    let mut seen = 0usize;
    let mut threshold = BINS - 1;
    for (bin, count) in histogram.iter().enumerate().rev() {
        seen += count;
        threshold = bin;
        if seen >= wanted {
            break;
        }
    }
    let mut candidates: Vec<(f32, usize)> = channel
        .iter()
        .enumerate()
        .filter(|(_, v)| bin_of(**v) >= threshold)
        .map(|(i, _)| (luma(px(rgb, i)), i))
        .collect();
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let keep = candidates.len().div_ceil(2).max(1);
    let mut sum = [0.0f64; 3];
    for (_, i) in candidates.iter().take(keep) {
        for (s, v) in sum.iter_mut().zip(px(rgb, *i).iter()) {
            *s += f64::from(*v);
        }
    }
    let airlight = sum.map(|s| ((s / keep as f64) as f32).clamp(0.02, DISPLAY_WHITE));

    // Density: the dark channel of the frame normalised by the airlight.
    let normalised: Vec<f32> = (0..n)
        .map(|i| {
            let p = px(rgb, i);
            p.iter()
                .zip(airlight.iter())
                .map(|(v, a)| v / a)
                .fold(f32::MAX, f32::min)
                .clamp(0.0, 1.0)
        })
        .collect();
    let density = min_filter(&normalised, width, height, patch);
    let r = radius_for(width, height, HAZE_RADIUS);
    Some(Haze {
        airlight,
        density: guided(&guide, Some(&density), width, height, r, HAZE_EPS),
    })
}

/// Everything the edge-aware stages need, measured once over the whole frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameBase {
    /// The tone base, when Shadows or Highlights is set.
    pub tone: Option<ToneBase>,
    /// Clarity's base, when Clarity is set.
    pub clarity: Option<Grid>,
    /// The haze, when Dehaze is set and the frame has an airlight.
    pub haze: Option<Haze>,
}

/// Smoothstep from `edge0` to `edge1`; `edge0 > edge1` gives a falling step.
fn step(x: f32, edge0: f32, edge1: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of the Shadows slider a region at `stops` from middle grey receives.
///
/// Full from about three stops below middle grey downward, falling to nothing just above
/// it, and easing to half in the deepest blacks - which are the Blacks slider's, and mostly
/// noise. The falling edge is three and a half stops wide, which is what keeps the mapping
/// from base to lifted base monotone at full travel: a region that was darker than another
/// is still darker after the lift, so no region can overtake its neighbour and draw a halo.
#[must_use]
pub fn shadow_weight(stops: f32) -> f32 {
    step(stops, 0.5, -3.0) * (1.0 - 0.5 * step(stops, -5.5, -7.5))
}

/// How much of the Highlights slider a region at `stops` from middle grey receives.
#[must_use]
pub fn highlight_weight(stops: f32) -> f32 {
    step(stops, -0.5, 2.0)
}

/// The change in stops that Shadows and Highlights make to a region whose base is `base`
/// (log2 luminance). `shadows` and `highlights` are `-1 ..= 1`.
#[must_use]
pub fn tone_gain(base: f32, shadows: f32, highlights: f32) -> f32 {
    let stops = base - MID_GREY.log2();
    let s = if shadows >= 0.0 {
        shadows * SHADOW_LIFT
    } else {
        shadows * SHADOW_DEEPEN
    };
    let h = if highlights >= 0.0 {
        highlights * HIGHLIGHT_LIFT
    } else {
        highlights * HIGHLIGHT_RECOVER
    };
    s * shadow_weight(stops) + h * highlight_weight(stops)
}

/// Local Shadows and Highlights on one pixel, given the base of its region.
///
/// The gain is decided by the region and applied to the pixel, so the pixel's departure from
/// its region - its texture - is kept exactly.
#[must_use]
pub fn tone_pixel(rgb: [f32; 3], base: f32, shadows: f32, highlights: f32) -> [f32; 3] {
    let l = luma(rgb);
    if l <= 0.0 {
        return rgb;
    }
    let gain = tone_gain(base, shadows, highlights);
    if gain == 0.0 {
        return rgb;
    }
    set_luma(rgb, l * gain.exp2())
}

/// Clarity's mid-tone weight: full through the body of the tonal range, fading toward
/// white and toward black so clarity neither clips a highlight nor crushes a shadow.
#[must_use]
pub fn midtone_weight(stops: f32) -> f32 {
    step(stops, 2.4, 1.0) * step(stops, -7.0, -4.0)
}

/// Halo-free clarity over a buffer, from a frame-wide clarity base.
///
/// `amount` is `-1 ..= 1`. Positive raises the contrast of each region's detail against its
/// edge-aware base, softly limited so a remaining edge cannot ring; negative lowers it,
/// which is a soft glow without the bloom a plain blur leaves around a hard edge.
pub fn clarity(rgb: &mut [f32], width: u32, position: Position, grid: &Grid, amount: f32) {
    if amount == 0.0 || width == 0 {
        return;
    }
    let sampler = grid.sampler(position);
    let width = width as usize;
    rgb.par_chunks_mut(width * 3)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.chunks_mut(3).enumerate() {
                let value = px(pixel, 0);
                let l = luma(value);
                if l <= 0.0 {
                    continue;
                }
                let log = l.max(LOG_FLOOR).log2();
                let (a, b) = sampler.coefficients(x, y);
                let base = a * log + b;
                let detail = log - base;
                // Beside an edge much stronger than texture the filter's base is a little
                // soft, and boosting the difference would draw a band of glow along it. That
                // is exactly where `a` approaches one, so clarity steps aside there.
                let weight = midtone_weight(base - MID_GREY.log2())
                    * (1.0 - step(a, CLARITY_EDGE_FROM, CLARITY_EDGE_FULL));
                let change = if amount > 0.0 {
                    // Up to about two and a half times the detail at +100, with each stop of
                    // detail beyond the first boosted progressively less, so what is left of
                    // a strong edge in the detail cannot ring.
                    detail * amount * CLARITY_GAIN / (1.0 + detail.abs() / 0.9)
                } else {
                    detail * amount * 0.85
                };
                let out = log + change * weight;
                write(pixel, set_luma(value, out.exp2()));
            }
        });
}

/// Dehaze over a buffer, from the frame's measured haze.
///
/// Positive `amount` inverts the haze model `I = J t + A (1 - t)` with `t = 1 - w d`, where
/// `d` is the pixel's measured density and `w` grows with the slider to 0.95 at +100 - so the
/// distance, which carries the most haze, gets the most correction and a clear foreground
/// almost none. Negative `amount` runs the model forward and adds haze where there is already
/// some, which reads as atmosphere rather than as a grey wash.
pub fn dehaze(rgb: &mut [f32], width: u32, position: Position, haze: &Haze, amount: f32) {
    if amount == 0.0 || width == 0 {
        return;
    }
    let sampler = haze.density.sampler(position);
    let airlight = haze.airlight;
    let width = width as usize;
    rgb.par_chunks_mut(width * 3)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.chunks_mut(3).enumerate() {
                let value = px(pixel, 0);
                let density = sampler.at(x, y, haze_guide(value)).clamp(0.0, 1.0);
                let seen = value.map(to_display);
                let mut out = [0.0f32; 3];
                if amount > 0.0 {
                    let t = (1.0 - 0.95 * amount.min(1.0) * density).max(TRANSMISSION_FLOOR);
                    for ((o, v), a) in out.iter_mut().zip(seen.iter()).zip(airlight.iter()) {
                        *o = from_display((v - a) / t + a);
                    }
                } else {
                    let k = 0.6 * amount.abs().min(1.0) * (0.3 + 0.7 * density);
                    for ((o, v), a) in out.iter_mut().zip(seen.iter()).zip(airlight.iter()) {
                        *o = from_display(v + k * (a - v));
                    }
                }
                write(pixel, out);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(width: usize, height: usize, f: impl Fn(usize, usize) -> f32) -> Vec<f32> {
        let mut out = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let v = f(x, y);
                out.extend_from_slice(&[v, v, v]);
            }
        }
        out
    }

    #[test]
    fn reduce_keeps_the_mean_and_the_aspect() {
        let rgb = grey(300, 200, |x, y| ((x + y) % 7) as f32 / 7.0);
        let (small, w, h) = reduce(&rgb, 300, 200, 100);
        assert_eq!((w, h), (100, 67));
        let mean = |v: &[f32]| v.iter().map(|x| f64::from(*x)).sum::<f64>() / v.len() as f64;
        assert!((mean(&rgb) - mean(&small)).abs() < 0.01);
    }

    #[test]
    fn the_guided_filter_keeps_a_strong_edge_and_smooths_texture() {
        // A five-stop step with fine texture on both sides.
        let (w, h) = (128, 64);
        let rgb = grey(w, h, |x, y| {
            let base = if x < 64 { 0.01 } else { 0.32 };
            base * if (x + y) % 2 == 0 { 1.15 } else { 0.87 }
        });
        let grid = tone_grid(&rgb, w, h, 0.0).grid;
        let sampler = grid.sampler(Position::whole(w as u32, h as u32));
        let base_at = |x: usize| sampler.at(x, 32, log_luma(px(&rgb, 32 * w + x)));
        // Far from the edge the base is the region's level, without the texture.
        let dark = 0.01f32.log2();
        let bright = 0.32f32.log2();
        assert!((base_at(10) - dark).abs() < 0.12, "{}", base_at(10));
        assert!((base_at(11) - dark).abs() < 0.12);
        assert!((base_at(118) - bright).abs() < 0.12);
        // Next to the edge it still belongs to its own side.
        assert!(base_at(62) < dark + 1.0, "{}", base_at(62));
        assert!(base_at(66) > bright - 1.0, "{}", base_at(66));
    }

    #[test]
    fn local_shadows_lift_a_region_and_keep_its_texture() {
        let (w, h) = (128, 64);
        let rgb = grey(w, h, |x, y| {
            let base = if x < 64 { 0.01 } else { 0.32 };
            base * if (x + y) % 2 == 0 { 1.2 } else { 0.8 }
        });
        let base = tone_grid(&rgb, w, h, 0.0);
        let sampler = base.sampler(Position::whole(w as u32, h as u32));
        let out = |x: usize, y: usize| luma(sampler.apply(x, y, px(&rgb, y * w + x), 1.0, 0.0));
        // The dark region is lifted by more than a stop.
        let lift = (out(20, 20) / luma(px(&rgb, 20 * w + 20))).log2();
        assert!(lift > 1.2, "lift {lift}");
        // Its texture ratio is unchanged: neighbours move together.
        let before = luma(px(&rgb, 20 * w + 20)) / luma(px(&rgb, 20 * w + 21));
        let after = out(20, 20) / out(21, 20);
        assert!((before - after).abs() < 0.02, "{before} vs {after}");
        // The bright region barely moves.
        let bright = (out(100, 20) / luma(px(&rgb, 20 * w + 100))).log2();
        assert!(bright.abs() < 0.1, "bright moved {bright}");
    }

    #[test]
    fn highlights_recover_a_bright_region_and_leave_a_clipped_one_white() {
        // Left: a bright sky with detail, well short of the ceiling. Right: a blown sky,
        // flat at the ceiling the file could record. A dark band underneath both.
        let (w, h) = (160, 80);
        let rgb = grey(w, h, |x, y| {
            if y >= 50 {
                0.02
            } else if x < 80 {
                0.5 * if (x + y) % 2 == 0 { 1.1 } else { 0.9 }
            } else {
                4.0
            }
        });
        let base = tone_grid(&rgb, w, h, 0.0);
        assert!(base.clipped.is_some(), "a blown sky is a clipped frame");
        let sampler = base.sampler(Position::whole(w as u32, h as u32));
        let moved = |x: usize, y: usize| {
            let p = px(&rgb, y * w + x);
            (luma(sampler.apply(x, y, p, 0.0, -1.0)) / luma(p)).log2()
        };
        assert!(
            moved(20, 20) < -0.7,
            "detail region recovered by {}",
            moved(20, 20)
        );
        assert!(
            moved(140, 20) > -0.2,
            "clipped region pulled down by {}",
            moved(140, 20)
        );
    }

    #[test]
    fn a_smooth_frame_is_not_called_clipped() {
        let (w, h) = (120, 90);
        let rgb = grey(w, h, |x, y| 0.01 + (x * h + y) as f32 / (w * h) as f32);
        assert!(tone_grid(&rgb, w, h, 0.0).clipped.is_none());
    }

    #[test]
    fn the_tone_mapping_is_monotone_at_full_travel() {
        for (s, h) in [
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.0, -1.0),
            (0.0, 1.0),
            (1.0, -1.0),
        ] {
            let mut last = f32::MIN;
            for i in 0..=400 {
                let base = -14.0 + i as f32 * 0.04;
                let mapped = base + tone_gain(base, s, h);
                assert!(mapped > last, "not monotone at {base} for ({s}, {h})");
                last = mapped;
            }
        }
    }

    #[test]
    fn clarity_barely_rings_at_a_hard_edge_where_a_blur_rings_badly() {
        // A clean four-stop step. Any departure from the two levels is a halo.
        let (w, h) = (200, 40);
        let step = grey(w, h, |x, _| if x < 100 { 0.03 } else { 0.5 });
        // How far the worst pixel moved off its level, and how many pixels moved visibly.
        let halo = |rgb: &[f32]| -> (f32, usize) {
            let departures: Vec<f32> = (0..w)
                .map(|x| {
                    let l = luma(px(rgb, 20 * w + x));
                    let level = if x < 100 { 0.03 } else { 0.5 };
                    (l / level).log2().abs()
                })
                .collect();
            (
                departures.iter().copied().fold(0.0, f32::max),
                departures.iter().filter(|d| **d > 0.05).count(),
            )
        };
        let mut edge_aware = step.clone();
        let grid = clarity_grid(&edge_aware, w, h);
        clarity(
            &mut edge_aware,
            w as u32,
            Position::whole(w as u32, h as u32),
            &grid,
            1.0,
        );
        // The blur-based clarity this replaces, at the same reach on this frame.
        let mut blurred = step.clone();
        crate::spatial::local_contrast(
            &mut blurred,
            w,
            h,
            radius_for(w, h, CLARITY_RADIUS) * 3,
            1.0,
        );
        let ((depth, width), (blur_depth, blur_width)) = (halo(&edge_aware), halo(&blurred));
        // At full travel on a clean four-stop step, the blur draws a band of glow two stops
        // deep; the edge-aware clarity moves no pixel by more than a tenth of a stop.
        assert!(
            depth < 0.12,
            "edge-aware clarity moved an edge pixel by {depth} stops"
        );
        assert!(
            depth < blur_depth / 10.0,
            "depth: edge-aware {depth} vs blur {blur_depth}"
        );
        assert!(
            width < blur_width / 3,
            "width: edge-aware {width} vs blur {blur_width}"
        );
    }

    #[test]
    fn clarity_raises_texture_contrast() {
        let (w, h) = (128, 128);
        let mut rgb = grey(w, h, |x, y| {
            0.18 * if ((x / 6) + (y / 6)) % 2 == 0 {
                1.3
            } else {
                0.77
            }
        });
        let before = luma(px(&rgb, 64 * w + 64)) / luma(px(&rgb, 64 * w + 70));
        let grid = clarity_grid(&rgb, w, h);
        clarity(
            &mut rgb,
            w as u32,
            Position::whole(w as u32, h as u32),
            &grid,
            0.8,
        );
        let after = luma(px(&rgb, 64 * w + 64)) / luma(px(&rgb, 64 * w + 70));
        assert!(
            after.ln().abs() > before.ln().abs() * 1.2,
            "{before} -> {after}"
        );
    }

    #[test]
    fn dehaze_removes_more_haze_where_there_is_more() {
        // A landscape: a sky that is all airlight, a far band under thick haze and a near
        // band under thin haze, over the same textured scene.
        let (w, h) = (160, 120);
        let airlight = [0.78f32, 0.8, 0.84];
        let scene = |x: usize, y: usize| -> [f32; 3] {
            let r = 0.04 + 0.25 * ((x / 8 + y / 8) % 3) as f32 / 2.0;
            [r, r * 0.8, r * 0.6]
        };
        let t = |y: usize| {
            if y < 24 {
                0.0
            } else if y < 64 {
                0.45
            } else {
                0.85
            }
        };
        // The veil is laid on what a viewer sees, then brought into the working space.
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let j = scene(x, y);
                for (c, a) in j.iter().zip(airlight.iter()) {
                    rgb.push(from_display(c * t(y) + a * (1.0 - t(y))));
                }
            }
        }
        let original = rgb.clone();
        let measured = haze(&rgb, w, h).expect("hazy frame");
        for (got, want) in measured.airlight.iter().zip(airlight.iter()) {
            assert!(
                (got - want).abs() < 0.05,
                "airlight {:?}",
                measured.airlight
            );
        }
        dehaze(
            &mut rgb,
            w as u32,
            Position::whole(w as u32, h as u32),
            &measured,
            1.0,
        );
        let err = |buf: &[f32], y: usize| -> f32 {
            (0..w)
                .map(|x| {
                    let got = px(buf, y * w + x).map(to_display);
                    let want = scene(x, y);
                    got.iter()
                        .zip(want.iter())
                        .map(|(g, w)| (g - w).abs())
                        .sum::<f32>()
                })
                .sum::<f32>()
                / w as f32
        };
        let (far_after, far_before) = (err(&rgb, 44), err(&original, 44));
        let (near_after, near_before) = (err(&rgb, 100), err(&original, 100));
        // Closer to the clear scene in both bands, and much closer in the far one.
        assert!(
            far_after < far_before * 0.6,
            "far {far_after} vs {far_before}"
        );
        assert!(
            near_after < near_before,
            "near {near_after} vs {near_before}"
        );
        // The far band received the larger correction.
        assert!(far_before - far_after > near_before - near_after);
    }

    #[test]
    fn a_tile_reads_the_same_base_as_the_whole_frame() {
        let (w, h) = (90, 70);
        let rgb = grey(w, h, |x, y| 0.02 + ((x * 7 + y * 3) % 50) as f32 / 60.0);
        let grid = tone_grid(&rgb, w, h, 0.0).grid;
        let whole = grid.sampler(Position::whole(w as u32, h as u32));
        let tile = grid.sampler(Position {
            x: 30,
            y: 20,
            full_width: w as u32,
            full_height: h as u32,
        });
        for (x, y) in [(0usize, 0usize), (5, 7), (40, 30)] {
            let g = 0.3;
            assert_eq!(
                whole.at(x + 30, y + 20, g).to_bits(),
                tile.at(x, y, g).to_bits()
            );
        }
    }
}
