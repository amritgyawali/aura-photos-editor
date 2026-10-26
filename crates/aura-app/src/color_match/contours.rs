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
//!
//! Near black an 8-bit file's levels are far apart in `OKLab` - one level is a unit or more - so
//! a dark gradient never looked smooth by that measure and was never examined; lifting it into the
//! midtones then showed its levels, and a JPEG's 8 x 8 blocks, as steps (ADR-0182). So adjacent
//! dark pairs within two levels on every channel are examined too, by their own rule: they fail
//! when the step grew by a visible amount and sits between flat colour on both sides, so detail a
//! lift legitimately reveals - bark in a shadow - is not mistaken for a band, and a difference a
//! match left as it was is not counted however large it measures near black.
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
/// Darker than this (largest channel, 8-bit), a pair within [`DARK_LEVELS`] counts as smooth.
const DARK: u8 = 72;
const DARK_LEVELS: u8 = 2;
/// A dark pair fails at or above this difference after the match...
const DARK_VISIBLE: f32 = 2.0;
/// ...when it grew by at least this much...
const DARK_RISE: f32 = 1.5;
/// ...and the pairs either side of it stay below this difference: a step between flat colour.
const FLAT: f32 = 1.5;
/// The share of smooth pairs that may fail, per scale. Noise and a legitimately strong grade
/// on a gradient cross the limits occasionally; a band or a patch crosses them in a cluster.
const MAX_FRACTION: f32 = 0.005;

#[derive(Debug, Default)]
pub(super) struct Contours {
    /// The full size of the photograph checked; pairs index its viewing-size copy.
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

fn bytes(image: &Rgb8, index: usize) -> Option<[u8; 3]> {
    match image
        .data
        .get(index.checked_mul(3)?..index.checked_mul(3)?.checked_add(3)?)?
    {
        [r, g, b] => Some([*r, *g, *b]),
        _ => None,
    }
}

/// Both pixels dark and within a couple of 8-bit levels on every channel.
fn dark_neighbours(image: &Rgb8, [i, j]: [usize; 2]) -> bool {
    match (bytes(image, i), bytes(image, j)) {
        (Some(a), Some(b)) => {
            a.iter().chain(&b).all(|&v| v <= DARK)
                && a.iter().zip(&b).all(|(x, y)| x.abs_diff(*y) <= DARK_LEVELS)
        }
        _ => false,
    }
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

/// The longest edge the check looks at. Bands and patches are large-scale defects, seen at the
/// size a photograph is viewed; at full resolution sensor and compression noise inside every flat
/// block would hide the step between two blocks from the flatness test.
const VIEWING_EDGE: u32 = 1024;

/// The image box-filtered to at most [`VIEWING_EDGE`] on its longer side.
fn viewed(image: &Rgb8) -> Rgb8 {
    let factor = image.width.max(image.height).div_ceil(VIEWING_EDGE).max(1) as usize;
    if factor == 1 || !valid(image) {
        return image.clone();
    }
    let (width, height) = (
        image.width as usize / factor,
        image.height as usize / factor,
    );
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0_u32; 3];
            for dy in 0..factor {
                for dx in 0..factor {
                    if let Some(p) = bytes(
                        image,
                        (y * factor + dy) * image.width as usize + x * factor + dx,
                    ) {
                        for (s, v) in sum.iter_mut().zip(p) {
                            *s += u32::from(v);
                        }
                    }
                }
            }
            let n = (factor * factor) as u32;
            pixels.extend(sum.map(|s| ((s + n / 2) / n) as u8));
        }
    }
    Rgb8 {
        width: width as u32,
        height: height as u32,
        data: pixels,
    }
}

fn valid(image: &Rgb8) -> bool {
    (image.width as usize)
        .checked_mul(image.height as usize)
        .and_then(|n| n.checked_mul(3))
        == Some(image.data.len())
}

impl Contours {
    pub(super) fn new(before: &Rgb8) -> Self {
        let full = (before.width, before.height);
        let before = &viewed(before);
        let mut result = Self {
            width: before.width,
            height: before.height,
            pairs: [Vec::new(), Vec::new()],
        };
        (result.width, result.height) = full;
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
                        if step < limits.smooth
                            || (distance == 1 && dark_neighbours(before, [i, j]))
                        {
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
        // The pairs were taken at viewing size; read the result at the same size.
        let after = &viewed(after);
        let width = after.width as usize;
        let count = after.data.len() / 3;
        // The pairs either side of an adjacent pair, along its own direction, when they exist.
        let flat_around = |[i, j]: [usize; 2]| {
            let step = j.saturating_sub(i);
            let horizontal = step == 1;
            let before = (i >= step && (!horizontal || i % width > 0)).then(|| [i - step, i]);
            let beyond = (j + step < count && (!horizontal || (j % width) + 1 < width))
                .then(|| [j, j + step]);
            [before, beyond].into_iter().all(|pair| {
                pair.and_then(|pair| difference(after, pair))
                    .is_some_and(|d| d <= FLAT)
            })
        };
        self.pairs
            .iter()
            .zip([BAND, BLOTCH])
            .enumerate()
            .all(|(scale, (pairs, limits))| {
                if pairs.len() < MIN_PAIRS {
                    return true;
                }
                let bad = pairs
                    .iter()
                    .filter(|(pair, before)| {
                        difference(after, *pair).is_none_or(|now| {
                            if scale > 0 || *before < limits.smooth {
                                now >= limits.visible && now >= limits.growth * before.max(0.5)
                            } else {
                                // A dark pair, smooth by its levels rather than its difference.
                                now >= DARK_VISIBLE
                                    && now >= *before + DARK_RISE
                                    && flat_around(*pair)
                            }
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

    /// A dark frame: 8 x 8 blocks one or two levels apart, as a JPEG stores a near-black
    /// backdrop, or per-pixel grain when `grain`.
    fn dark(grain: bool, lift: impl Fn(u8) -> u8) -> Rgb8 {
        let (width, height) = (256_u32, 128_u32);
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let level = if grain {
                    // Two levels from pixel to pixel: texture, not a gradient.
                    3 + 2 * ((x * 7 + y * 13) % 3) as u8
                } else {
                    3 + (((x / 8) + (y / 8)) % 3) as u8
                };
                data.extend([lift(level), lift(level), lift(level) / 2 + 1]);
            }
        }
        Rgb8 {
            width,
            height,
            data,
        }
    }

    #[test]
    fn a_near_black_backdrop_lifted_into_blocks_is_refused() {
        let guard = Contours::new(&dark(false, |v| v));
        assert!(guard.pairs[0].len() >= MIN_PAIRS, "dark pairs are examined");
        assert!(guard.accepts(&dark(false, |v| v)));
        // Lifted into the midtones, each level becomes a visible step between flat blocks.
        assert!(!guard.accepts(&dark(false, |v| 60 + v * 12)));
    }

    #[test]
    fn grain_a_lift_reveals_is_not_a_band() {
        let guard = Contours::new(&dark(true, |v| v));
        // The same lift on per-pixel grain is texture coming into view, with no flat side.
        assert!(guard.accepts(&dark(true, |v| 60 + v * 12)));
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
