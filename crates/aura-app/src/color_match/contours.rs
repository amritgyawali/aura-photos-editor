//! Refuse new bands and blotches in regions that were smooth before a colour match.
//!
//! A colour transform with a steep spot turns a smooth gradient into steps (a banded sky) and a
//! near-uniform surface into patches (a blotchy pale floor). Both are invisible to a histogram
//! score, so every native review checks them on the rendered pixels. Pairs of pixels are sampled
//! across the whole frame at two distances - adjacent, for bands, and a few pixels apart, for
//! patches - and only pairs that were perceptually smooth in the original are kept, at any
//! lightness: a bright sky or backdrop is checked as well as a shadow. A pair fails when the
//! colour match turns a difference nobody could see into one anybody can, measured in `OKLab`
//! times a hundred (about one CIELAB unit). Real edges and texture were never smooth, so they are
//! never examined. These are authored quality limits, not a perceptual accuracy guarantee.
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_render::reference_grade::oklab_from_linear_srgb;

/// Sample starting pixels, stratified over the frame.
const PIXELS: usize = 8192;
/// Fewer smooth pairs than this is too little evidence to refuse anything.
const MIN_PAIRS: usize = 128;
/// The distance, in pixels, at which a patch is looked for.
const PATCH: usize = 6;

/// One spatial scale of the check.
#[derive(Debug, Clone, Copy)]
struct Scale {
    /// A pair counts as smooth in the original below this difference.
    smooth: f32,
    /// After the match it fails at or above this difference...
    visible: f32,
    /// ...and only when the difference grew at least this many times.
    growth: f32,
}

/// Adjacent pixels: a band is a new step where there was none.
const BAND: Scale = Scale {
    smooth: 1.0,
    visible: 3.0,
    growth: 3.0,
};
/// Pixels a few apart: a patch is a new colour difference across a surface that was even.
const BLOTCH: Scale = Scale {
    smooth: 1.5,
    visible: 6.0,
    growth: 3.0,
};
/// The share of smooth pairs that may fail, per scale. Noise and a legitimately strong grade
/// on a gradient cross the limits occasionally; a band or a patch crosses them in a cluster.
const MAX_FRACTION: f32 = 0.005;

#[derive(Debug, Default)]
pub(super) struct Contours {
    width: u32,
    height: u32,
    /// Smooth pairs per scale, with the original difference of each.
    pairs: [Vec<([usize; 2], f32)>; 2],
}

fn pixel(image: &Rgb8, index: usize) -> Option<[f32; 3]> {
    let bytes = image
        .data
        .get(index.checked_mul(3)?..index.checked_mul(3)?.checked_add(3)?)?;
    let linear: [f32; 3] =
        std::array::from_fn(|c| srgb_byte_to_linear(bytes.get(c).copied().unwrap_or(0)));
    Some(oklab_from_linear_srgb(linear).map(|v| v * 100.0))
}

fn difference(image: &Rgb8, [i, j]: [usize; 2]) -> Option<f32> {
    let (a, b) = (pixel(image, i)?, pixel(image, j)?);
    Some(
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt(),
    )
}

fn valid(image: &Rgb8) -> bool {
    (image.width as usize)
        .checked_mul(image.height as usize)
        .and_then(|n| n.checked_mul(3))
        == Some(image.data.len())
}

impl Contours {
    pub(super) fn new(before: &Rgb8) -> Self {
        let mut result = Self {
            width: before.width,
            height: before.height,
            pairs: [Vec::new(), Vec::new()],
        };
        if !valid(before) || before.width == 0 || before.height == 0 {
            return result;
        }
        let count = before.data.len() / 3;
        let budget = count.min(PIXELS);
        let width = before.width as usize;
        for k in 0..budget {
            let start = k * count / budget;
            let end = (k + 1) * count / budget;
            let hash = (k as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let i = start + ((hash ^ (hash >> 29)) as usize % (end - start).max(1));
            for (scale, (distance, limits)) in
                result.pairs.iter_mut().zip([(1, BAND), (PATCH, BLOTCH)])
            {
                for j in [
                    (i % width + distance < width).then_some(i + distance),
                    (i + distance * width < count).then_some(i + distance * width),
                ]
                .into_iter()
                .flatten()
                {
                    if let Some(step) = difference(before, [i, j]) {
                        if step < limits.smooth {
                            scale.push(([i, j], step));
                        }
                    }
                }
            }
        }
        result
    }

    /// Whether the rendered result keeps the original's smooth regions smooth. An image of
    /// another size is refused; too few smooth pairs is accepted, because a frame with no
    /// smooth region cannot band.
    pub(super) fn accepts(&self, after: &Rgb8) -> bool {
        if after.width != self.width || after.height != self.height || !valid(after) {
            return false;
        }
        self.pairs
            .iter()
            .zip([BAND, BLOTCH])
            .all(|(pairs, limits)| {
                if pairs.len() < MIN_PAIRS {
                    return true;
                }
                let bad = pairs
                    .iter()
                    .filter(|(pair, before)| {
                        difference(after, *pair).is_none_or(|now| {
                            now >= limits.visible && now >= limits.growth * before.max(0.5)
                        })
                    })
                    .count();
                bad as f32 <= MAX_FRACTION * pairs.len() as f32
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(transform: impl Fn(u8) -> u8) -> Rgb8 {
        let mut data = Vec::new();
        for _ in 0..128 {
            for x in 0..256 {
                data.extend([transform(16 + (x / 4) as u8); 3]);
            }
        }
        Rgb8 {
            width: 256,
            height: 128,
            data,
        }
    }

    /// A smooth sky-like gradient, bright and blue.
    fn sky(transform: impl Fn([f32; 3]) -> [f32; 3]) -> Rgb8 {
        let (width, height) = (320_u32, 160_u32);
        let mut data = Vec::new();
        for y in 0..height {
            for _ in 0..width {
                let t = y as f32 / height as f32;
                let rgb = transform([0.45 + 0.25 * t, 0.62 + 0.18 * t, 0.86 + 0.08 * t]);
                data.extend(rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
            }
        }
        Rgb8 {
            width,
            height,
            data,
        }
    }

    #[test]
    fn amplified_shadow_steps_are_rejected_but_a_smooth_grade_passes() {
        let original = ramp(|x| x);
        let guard = Contours::new(&original);
        assert!(guard.pairs[0].len() >= MIN_PAIRS);
        assert!(guard.accepts(&original));
        assert!(guard.accepts(&ramp(|x| x * 2 + 8)));
        assert!(!guard.accepts(&ramp(|x| (x / 8) * 16)));
    }

    #[test]
    fn bright_skies_are_checked_and_a_banded_one_is_refused() {
        let original = sky(|rgb| rgb);
        let guard = Contours::new(&original);
        assert!(
            guard.pairs[0].len() >= MIN_PAIRS,
            "bright smooth pairs are kept"
        );
        // A gentle warm grade keeps the sky smooth.
        assert!(guard.accepts(&sky(|[r, g, b]| [r * 1.05 + 0.02, g, b * 0.95])));
        // Posterising the sky into a few bands is refused, though it is bright.
        assert!(!guard.accepts(&sky(|rgb| rgb.map(|v| (v * 6.0).round() / 6.0))));
    }

    #[test]
    fn new_colour_patches_on_an_even_surface_are_refused() {
        let floor = |patches: bool| {
            let (width, height) = (300_u32, 200_u32);
            let mut data = Vec::new();
            for y in 0..height {
                for x in 0..width {
                    let tinted = patches && ((x / 25) + (y / 25)) % 2 == 0;
                    data.extend(if tinted {
                        [205, 232, 222]
                    } else {
                        [236, 233, 230]
                    });
                }
            }
            Rgb8 {
                width,
                height,
                data,
            }
        };
        let guard = Contours::new(&floor(false));
        assert!(guard.accepts(&floor(false)));
        assert!(!guard.accepts(&floor(true)));
    }

    #[test]
    fn rich_texture_is_not_mistaken_for_a_smooth_region() {
        let mut source = ramp(|x| x);
        for (i, p) in source.data.chunks_exact_mut(3).enumerate() {
            p.fill(if i % 2 == 0 { 8 } else { 64 });
        }
        let guard = Contours::new(&source);
        let mut after = source.clone();
        after
            .data
            .iter_mut()
            .for_each(|v| *v = if *v == 8 { 0 } else { 200 });
        assert!(guard.accepts(&after));
        after.width += 1;
        assert!(!guard.accepts(&after));
    }
}
