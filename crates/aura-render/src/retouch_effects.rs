//! Selected effects in linear Rec.2020: liquify, optics, colour, light and portrait tools.
//! ADR-0131.
//!
//! Every tool here works inside the operation's selection (`Coverage`), reads the frame as it
//! was before the tool ran, and is deterministic: the same frame, recipe and size give the same
//! pixels on every machine, whatever the number of cores. The colour tools keep each pixel's
//! luminance unless their purpose is to change it.
// Frame dimensions and edit parameters are checked at the render/recipe boundaries.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
mod colour;
mod creative;
mod light;
mod optics;
mod portrait;
mod practical;
mod warp;

use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect};
use aura_recipe::retouch_tools::{Edit, Tool};
use rayon::prelude::*;

/// True for the tools this module renders.
pub(crate) fn supports(tool: Tool) -> bool {
    warp::supports(tool)
        || optics::supports(tool)
        || colour::supports(tool)
        || light::supports(tool)
        || portrait::supports(tool)
        || practical::supports(tool)
        || creative::supports(tool)
}

/// Render one effect operation into `rgb`.
pub(crate) fn apply(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    edit: &Edit,
    coverage: &Coverage,
) {
    if !edit.enabled
        || edit.amount <= 0.0
        || width == 0
        || height == 0
        || rgb.len() != width * height * 3
    {
        return;
    }
    let frame = Frame { width, height };
    if warp::supports(edit.tool) {
        warp::apply(rgb, frame, edit, coverage);
    } else if optics::supports(edit.tool) {
        optics::apply(rgb, frame, edit, coverage);
    } else if colour::supports(edit.tool) {
        colour::apply(rgb, frame, edit, coverage);
    } else if light::supports(edit.tool) {
        light::apply(rgb, frame, edit, coverage);
    } else if portrait::supports(edit.tool) {
        portrait::apply(rgb, frame, edit, coverage);
    } else if practical::supports(edit.tool) {
        practical::apply(rgb, frame, edit, coverage);
    } else if creative::supports(edit.tool) {
        creative::apply(rgb, frame, edit, coverage);
    }
}

#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub width: usize,
    pub height: usize,
}

impl Frame {
    pub(super) fn short(self) -> f32 {
        self.width.min(self.height) as f32
    }

    /// `radius` (a share of the short edge) in whole pixels, at least one.
    pub(super) fn radius_px(self, edit: &Edit) -> usize {
        (edit.radius * self.short()).round().max(1.0) as usize
    }

    /// The region centre in pixel coordinates (pixel centres at integer + 0.5).
    pub(super) fn centre(self, edit: &Edit) -> [f32; 2] {
        [
            edit.region[0] * self.width as f32,
            edit.region[1] * self.height as f32,
        ]
    }

    /// `source` if the operation has one, else the region centre, in pixel coordinates.
    pub(super) fn source_or_centre(self, edit: &Edit) -> [f32; 2] {
        edit.source.map_or_else(
            || self.centre(edit),
            |p| [p[0] * self.width as f32, p[1] * self.height as f32],
        )
    }

    /// The region's radii in pixels.
    pub(super) fn reach(self, edit: &Edit) -> [f32; 2] {
        [
            (edit.region[2] * self.width as f32).max(1.0),
            (edit.region[3] * self.height as f32).max(1.0),
        ]
    }
}

/// Part of the frame as it was before the operation, read while the frame is written.
pub(super) struct Snapshot {
    rect: Rect,
    pixels: Vec<f32>,
}

impl Snapshot {
    pub(super) fn take(rgb: &[f32], frame: Frame, rect: Rect) -> Self {
        let mut pixels = Vec::with_capacity(rect.len() * 3);
        for y in rect.y0..rect.y0 + rect.h {
            let start = (y * frame.width + rect.x0) * 3;
            pixels.extend_from_slice(&rgb[start..start + rect.w * 3]);
        }
        Self { rect, pixels }
    }

    /// The pixel at absolute (x, y), clamped into the snapshot.
    pub(super) fn at(&self, x: isize, y: isize) -> [f32; 3] {
        let cx = (x - self.rect.x0 as isize).clamp(0, self.rect.w as isize - 1) as usize;
        let cy = (y - self.rect.y0 as isize).clamp(0, self.rect.h as isize - 1) as usize;
        let i = (cy * self.rect.w + cx) * 3;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2]]
    }

    /// Bilinear sample at absolute pixel coordinates, where pixel (x, y) covers x..x+1.
    pub(super) fn bilinear(&self, fx: f32, fy: f32) -> [f32; 3] {
        let x = fx - 0.5;
        let y = fy - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let tx = x - x0;
        let ty = y - y0;
        let (x0, y0) = (x0 as isize, y0 as isize);
        let a = self.at(x0, y0);
        let b = self.at(x0 + 1, y0);
        let c = self.at(x0, y0 + 1);
        let d = self.at(x0 + 1, y0 + 1);
        std::array::from_fn(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bottom = c[k] + (d[k] - c[k]) * tx;
            top + (bottom - top) * ty
        })
    }

    /// One channel as a plane the size of the snapshot.
    pub(super) fn plane(&self, channel: usize) -> Vec<f32> {
        self.pixels
            .iter()
            .skip(channel)
            .step_by(3)
            .copied()
            .collect()
    }

    /// Luminance as a plane the size of the snapshot.
    pub(super) fn luma_plane(&self) -> Vec<f32> {
        self.pixels
            .chunks_exact(3)
            .map(|p| luma([p[0], p[1], p[2]]))
            .collect()
    }

    pub(super) fn rect(&self) -> Rect {
        self.rect
    }

    /// Index of absolute (x, y) inside the snapshot's planes.
    pub(super) fn index(&self, x: usize, y: usize) -> usize {
        (y - self.rect.y0) * self.rect.w + (x - self.rect.x0)
    }
}

/// Rewrite every pixel of `rect` that `f` returns a value for, rows in parallel. `f` gets the
/// absolute position and the pixel as it is now.
pub(super) fn each(
    rgb: &mut [f32],
    frame: Frame,
    rect: Rect,
    f: impl Fn(usize, usize, [f32; 3]) -> Option<[f32; 3]> + Sync,
) {
    rgb.par_chunks_mut(frame.width * 3)
        .enumerate()
        .skip(rect.y0)
        .take(rect.h)
        .for_each(|(y, row)| {
            for x in rect.x0..rect.x0 + rect.w {
                let i = x * 3;
                let old = [row[i], row[i + 1], row[i + 2]];
                if let Some(new) = f(x, y, old) {
                    row[i] = new[0];
                    row[i + 1] = new[1];
                    row[i + 2] = new[2];
                }
            }
        });
}

/// The selection's strength at a pixel: coverage times the operation's amount.
pub(super) fn weight(edit: &Edit, coverage: &Coverage, frame: Frame, x: usize, y: usize) -> f32 {
    coverage.at(x, y, frame.width, frame.height) * edit.amount
}

pub(super) fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
}

/// Luminance and two colour differences, `cb = b - y` and `cr = r - y`. Converting back keeps
/// the luminance exactly, whatever is done to the two differences.
pub(super) fn opponent(p: [f32; 3]) -> [f32; 3] {
    let y = luma(p);
    [y, p[2] - y, p[0] - y]
}

pub(super) fn from_opponent(o: [f32; 3]) -> [f32; 3] {
    let [y, cb, cr] = o;
    let r = y + cr;
    let b = y + cb;
    // luma = 0.2627 r + 0.678 g + 0.0593 b, solved for g.
    let g = (y - 0.2627 * r - 0.0593 * b) / 0.678;
    [r.max(0.0), g.max(0.0), b.max(0.0)]
}

/// The mean of the 5 x 5 pixels around `point`.
pub(super) fn sample(rgb: &[f32], frame: Frame, point: [f32; 2]) -> [f32; 3] {
    let cx = point[0].clamp(0.0, frame.width as f32 - 1.0) as isize;
    let cy = point[1].clamp(0.0, frame.height as f32 - 1.0) as isize;
    let mut sum = [0.0; 3];
    let mut n = 0.0;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let x = (cx + dx).clamp(0, frame.width as isize - 1) as usize;
            let y = (cy + dy).clamp(0, frame.height as isize - 1) as usize;
            let i = (y * frame.width + x) * 3;
            for k in 0..3 {
                sum[k] += rgb[i + k];
            }
            n += 1.0;
        }
    }
    sum.map(|v| v / n)
}

/// A deterministic hash of two integers and a seed, in 0..1.
pub(super) fn hash(a: i64, b: i64, seed: u64) -> f32 {
    let mut h = seed ^ 0x9e37_79b9_7f4a_7c15;
    for v in [a as u64, b as u64] {
        h ^= v.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h = h.rotate_left(31).wrapping_mul(0x94d0_49bb_1331_11eb);
    }
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// A seed that follows the operation's placement, so a moved freckle field is a different one.
pub(super) fn seed_of(edit: &Edit) -> u64 {
    edit.region.iter().fold(0x51_7cc1_b727_220a_u64, |h, v| {
        (h ^ u64::from(v.to_bits())).wrapping_mul(0x0100_0000_01b3)
    })
}

/// 0 at `low` rising smoothly to 1 at `high`. `low` may be above `high`, which makes it fall
/// from 1 to 0 instead: the guard against a zero span keeps the span's sign.
pub(super) fn smoothstep(low: f32, high: f32, v: f32) -> f32 {
    let span = high - low;
    let span = if span.abs() < 1e-9 {
        1e-9_f32.copysign(span)
    } else {
        span
    };
    let t = ((v - low) / span).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The selection's rectangle grown by `margin` and clipped to the frame.
pub(super) fn area(coverage: &Coverage, frame: Frame, margin: usize) -> Option<Rect> {
    Rect::around(coverage.bounds, margin, frame.width, frame.height)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
pub(super) mod testing {
    use super::*;
    use aura_recipe::retouch_tools::Tool;

    pub(crate) fn edit(tool: Tool, region: [f32; 4]) -> Edit {
        Edit {
            id: "effect".into(),
            tool,
            enabled: true,
            repair_passes: None,
            region,
            source: None,
            source_scale: 1.0,
            amount: 1.0,
            feather: 0.0,
            radius: 0.03,
            texture: 1.0,
            preserve_microtexture: false,
            texture_heal: false,
            clean_ring_fit: false,
            curved_heal: false,
            heal_samples: Vec::new(),
            texture_sources: Vec::new(),
            sensitivity: None,
            keep_dark_marks: false,
            mole_edge: None,
            tone: 0.5,
            warmth: 0.0,
            tint: 0.0,
            target_color: None,
            mask: None,
            skin: None,
            selection: None,
            matte: None,
        }
    }

    /// Render `edit` over `rgb` the way the stack does.
    pub(crate) fn run(rgb: &mut [f32], w: usize, h: usize, edit: &Edit) {
        let coverage = Coverage::for_edit(edit, w, h, rgb);
        apply(rgb, w, h, edit, &coverage);
    }

    pub(crate) fn fill(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y))
            .collect()
    }

    pub(crate) fn px(rgb: &[f32], w: usize, x: usize, y: usize) -> [f32; 3] {
        let i = (y * w + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    /// Pixels further than `margin` outside the region's ellipse that changed.
    pub(crate) fn changed_outside(
        before: &[f32],
        after: &[f32],
        w: usize,
        h: usize,
        region: [f32; 4],
        margin: f32,
    ) -> usize {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let dx = ((x as f32 + 0.5) / w as f32 - region[0]) / region[2];
                let dy = ((y as f32 + 0.5) / h as f32 - region[1]) / region[3];
                dx.hypot(dy) > 1.0 + margin
            })
            .filter(|&(x, y)| px(before, w, x, y) != px(after, w, x, y))
            .count()
    }

    pub(crate) fn checker(w: usize, h: usize, cell: usize) -> Vec<f32> {
        fill(w, h, |x, y| {
            if (x / cell + y / cell).is_multiple_of(2) {
                [0.6, 0.5, 0.4]
            } else {
                [0.1, 0.12, 0.15]
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::testing::*;
    use super::*;

    const ALL: [Tool; 48] = [
        Tool::LiquifyPush,
        Tool::LiquifyBloat,
        Tool::LiquifyPucker,
        Tool::LiquifyTwirl,
        Tool::SpinBlur,
        Tool::Pixelate,
        Tool::PrivacyBlur,
        Tool::SoftFocus,
        Tool::OrtonGlow,
        Tool::LocalSharpen,
        Tool::LocalDenoise,
        Tool::HueShift,
        Tool::InvertColors,
        Tool::ChromaKey,
        Tool::VeinReduce,
        Tool::ColorUnify,
        Tool::LightLeak,
        Tool::Glitch,
        Tool::Haze,
        Tool::Starburst,
        Tool::LocalWhiteBalance,
        Tool::Catchlight,
        Tool::EyeGlowFix,
        Tool::LipGloss,
        Tool::Freckles,
        Tool::FloorReflection,
        Tool::DropShadow,
        Tool::Halation,
        Tool::DustScratches,
        Tool::JpegCleanup,
        Tool::Deband,
        Tool::AutoLevels,
        Tool::AutoNeutral,
        Tool::FlattenLight,
        Tool::HdrDetail,
        Tool::BokehHighlights,
        Tool::TiltShift,
        Tool::Duotone,
        Tool::Posterize,
        Tool::Halftone,
        Tool::PencilSketch,
        Tool::OilPaint,
        Tool::CrossProcess,
        Tool::Infrared,
        Tool::MirrorSymmetry,
        Tool::LensFlare,
        Tool::GodRays,
        Tool::Weather,
    ];

    /// A busy, colourful frame every tool has something to do on.
    fn busy(w: usize, h: usize) -> Vec<f32> {
        fill(w, h, |x, y| {
            // A smooth gradient on the right: deband acts only where an area is smooth.
            if x >= 44 {
                let t = (x - 44) as f32 / 20.0 + y as f32 / 200.0;
                return [0.2 + t * 0.1, 0.3 + t * 0.08, 0.45 + t * 0.05];
            }
            let fine = if (x + 2 * y) % 5 == 0 { 0.25 } else { 0.0 };
            let block = if (x / 9 + y / 7) % 2 == 0 { 0.5 } else { 0.08 };
            let bright = if (28..34).contains(&x) && (20..26).contains(&y) {
                1.4
            } else {
                0.0
            };
            [
                block + fine + bright,
                0.3 + fine * 0.5 + bright * 0.9,
                0.18 + (x as f32 / w as f32) * 0.5 + bright * 0.6,
            ]
        })
    }

    fn prepared(tool: Tool) -> Edit {
        let mut e = edit(tool, [0.5, 0.5, 0.3, 0.3]);
        e.feather = 0.3;
        e.warmth = 0.5;
        e.tint = 0.3;
        e.tone = 0.6;
        e.radius = 0.04;
        e.texture = 1.5;
        if matches!(tool, Tool::ChromaKey | Tool::LocalWhiteBalance) {
            e.source = Some([0.5, 0.5]);
        }
        if tool == Tool::DropShadow {
            e.region = [0.4, 0.4, 0.15, 0.15];
        }
        e
    }

    #[test]
    fn every_effect_is_supported_and_changes_the_busy_frame() {
        let (w, h) = (64, 48);
        for tool in ALL {
            assert!(supports(tool), "{tool:?}");
            let before = busy(w, h);
            let mut after = before.clone();
            run(&mut after, w, h, &prepared(tool));
            assert_ne!(before, after, "{tool:?} changed nothing");
            assert!(
                after.iter().all(|v| v.is_finite() && *v >= 0.0),
                "{tool:?} produced a bad value"
            );
        }
    }

    #[test]
    fn nothing_outside_the_selection_moves_except_a_cast_shadow() {
        let (w, h) = (64, 48);
        for tool in ALL.into_iter().filter(|t| *t != Tool::DropShadow) {
            let e = prepared(tool);
            let before = busy(w, h);
            let mut after = before.clone();
            run(&mut after, w, h, &e);
            assert_eq!(
                changed_outside(&before, &after, w, h, e.region, 0.08),
                0,
                "{tool:?}"
            );
        }
    }

    #[test]
    fn zero_strength_or_disabled_changes_nothing() {
        let (w, h) = (40, 30);
        for tool in ALL {
            let before = busy(w, h);
            let mut e = prepared(tool);
            e.amount = 0.0;
            let mut after = before.clone();
            run(&mut after, w, h, &e);
            assert_eq!(before, after, "{tool:?} at zero");
            let mut e = prepared(tool);
            e.enabled = false;
            run(&mut after, w, h, &e);
            assert_eq!(before, after, "{tool:?} disabled");
        }
    }

    #[test]
    fn every_effect_is_deterministic() {
        let (w, h) = (64, 48);
        for tool in ALL {
            let e = prepared(tool);
            let mut a = busy(w, h);
            let mut b = busy(w, h);
            run(&mut a, w, h, &e);
            run(&mut b, w, h, &e);
            assert_eq!(a, b, "{tool:?}");
        }
    }

    #[test]
    fn smoothstep_rises_and_falls_with_the_order_of_its_edges() {
        assert!(smoothstep(0.0, 1.0, 0.0).abs() < 1e-6);
        assert!((smoothstep(0.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!(
            (smoothstep(1.0, 0.0, 0.0) - 1.0).abs() < 1e-6,
            "reversed edges fall"
        );
        assert!(smoothstep(1.0, 0.0, 1.0).abs() < 1e-6);
        assert!((smoothstep(1.0, 0.0, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn opponent_round_trip_keeps_the_pixel() {
        for p in [
            [0.2, 0.5, 0.1],
            [1.3, 0.2, 0.7],
            [0.0, 0.0, 0.0],
            [0.4, 0.4, 0.4],
        ] {
            let back = from_opponent(opponent(p));
            for k in 0..3 {
                assert!((back[k] - p[k]).abs() < 1e-5, "{p:?} -> {back:?}");
            }
        }
    }
}
