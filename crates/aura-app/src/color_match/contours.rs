//! Bound newly amplified luminance and colour steps in native smooth regions.
//!
//! These are authored RGB8 quality limits, not perceptual accuracy guarantees.
//! Fixed, stratified adjacent-pixel observations keep checking bounded and prevent
//! a histogram improvement from hiding conspicuous shadow contouring.
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_render::reference_grade::oklab_from_linear_srgb;

const PIXELS: usize = 4096;
const MIN_PAIRS: usize = 128;
const DARK: u8 = 96;
const SOURCE_STEP: u8 = 2;
const OUTPUT_STEP: u8 = 12;
const MAX_FRACTION: f32 = 0.005;
const TILES: usize = 4;
const MIN_TILE_PAIRS: usize = 32;
const MAX_PERCEPTUAL_STEP: f32 = 1.0;

#[derive(Debug)]
struct SmoothPair {
    indices: [usize; 2],
    before: f32,
    tile: usize,
}

#[derive(Debug, Default)]
pub(super) struct Contours {
    width: u32,
    height: u32,
    pairs: Vec<[usize; 2]>,
    smooth: Vec<SmoothPair>,
}

fn lab(pixel: [u8; 3]) -> [f32; 3] {
    oklab_from_linear_srgb(pixel.map(srgb_byte_to_linear))
}

fn distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (a, b) = (lab(a), lab(b));
    100.0 * ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn pixel(image: &Rgb8, index: usize) -> Option<&[u8; 3]> {
    image
        .data
        .get(index.checked_mul(3)?..index.checked_mul(3)?.checked_add(3)?)?
        .try_into()
        .ok()
}

fn step(left: &[u8], right: &[u8]) -> u8 {
    left.iter()
        .zip(right)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0)
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
            pairs: Vec::new(),
            smooth: Vec::new(),
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
            for j in [
                (i % width + 1 < width).then_some(i + 1),
                (i + width < count).then_some(i + width),
            ]
            .into_iter()
            .flatten()
            {
                if let Some((a, b)) = pixel(before, i).zip(pixel(before, j)) {
                    if a.iter().chain(b).all(|v| *v <= DARK) && step(a, b) <= SOURCE_STEP {
                        result.pairs.push([i, j]);
                    }
                    // Require a smooth neighbourhood, not just an equal pair on opposite
                    // sides of texture. Include bright/coloured surfaces, not only shadows.
                    let neighbours = [
                        i.checked_sub(1).filter(|_| !i.is_multiple_of(width)),
                        i.checked_sub(width),
                        (i % width + 1 < width).then_some(i + 1),
                        (i + width < count).then_some(i + width),
                    ];
                    if step(a, b) <= SOURCE_STEP
                        && neighbours
                            .into_iter()
                            .flatten()
                            .all(|n| pixel(before, n).is_some_and(|p| step(a, p) <= 4))
                    {
                        let before = distance(*a, *b);
                        if before <= 1.0 {
                            result.smooth.push(SmoothPair {
                                indices: [i, j],
                                before,
                                tile: (i / width * TILES / result.height as usize) * TILES
                                    + i % width * TILES / width,
                            });
                        }
                    }
                }
            }
        }
        result
    }

    pub(super) fn accepts(&self, after: &Rgb8) -> bool {
        if after.width != self.width || after.height != self.height || !valid(after) {
            return false;
        }
        if !self.perceptually_smooth(after) {
            return false;
        }
        if self.pairs.len() < MIN_PAIRS {
            return true;
        }
        let bad = self
            .pairs
            .iter()
            .filter(|[i, j]| {
                pixel(after, *i)
                    .zip(pixel(after, *j))
                    .is_none_or(|(a, b)| step(a, b) >= OUTPUT_STEP)
            })
            .count();
        bad as f32 <= MAX_FRACTION * self.pairs.len() as f32
    }

    fn perceptually_smooth(&self, after: &Rgb8) -> bool {
        let mut tiles = [(0_usize, 0_usize); TILES * TILES];
        for pair in &self.smooth {
            let Some((support, bad)) = tiles.get_mut(pair.tile) else {
                return false;
            };
            *support += 1;
            let [i, j] = pair.indices;
            let Some((a, b)) = pixel(after, i).zip(pixel(after, j)) else {
                return false;
            };
            let step = distance(*a, *b);
            if step > MAX_PERCEPTUAL_STEP && step > 2.0 * pair.before + 0.25 {
                *bad += 1;
            }
        }
        // Local support prevents a large unchanged region from hiding one banded sky/floor.
        tiles.into_iter().all(|(support, bad)| {
            support < MIN_TILE_PAIRS || bad as f32 <= MAX_FRACTION * support as f32
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

    #[test]
    fn amplified_shadow_steps_are_rejected_but_a_smooth_grade_passes() {
        let original = ramp(|x| x);
        let guard = Contours::new(&original);
        assert!(guard.pairs.len() >= MIN_PAIRS && guard.pairs.len() <= 2 * PIXELS);
        assert!(guard.accepts(&original));
        assert!(guard.accepts(&ramp(|x| x * 2 + 8)));
        assert!(!guard.accepts(&ramp(|x| (x / 8) * 16)));
    }

    #[test]
    fn rich_texture_is_not_mistaken_for_a_smooth_dark_region() {
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

    #[test]
    fn smooth_bright_and_coloured_regions_cannot_hide_local_banding() {
        let before = ramp(|x| x + 125);
        let guard = Contours::new(&before);
        assert!(guard.accepts(&ramp(|x| x + 132)));
        let mut after = before.clone();
        // Damage a single bright quadrant, leaving most sampled pairs unchanged.
        for y in 0..64_usize {
            for x in 0..64_usize {
                let at = (y * 256 + x) * 3;
                let v = before.data[at];
                after.data[at + 1] = if v.is_multiple_of(2) { v } else { v + 20 };
            }
        }
        assert!(!guard.accepts(&after));
    }

    #[test]
    fn bright_fine_texture_and_a_real_edge_are_not_flat_gradients() {
        let mut before = ramp(|x| x + 120);
        for y in 0..128_usize {
            for x in 0..256_usize {
                let at = (y * 256 + x) * 3;
                before.data[at..at + 3].fill(if (x + y).is_multiple_of(2) { 130 } else { 240 });
            }
        }
        assert!(Contours::new(&before).accepts(&before));
        assert!(Contours::new(&before).smooth.is_empty());
        for (i, pixel) in before.data.chunks_exact_mut(3).enumerate() {
            pixel.fill(if i % 256 < 128 { 160 } else { 220 });
        }
        let mut after = before.clone();
        for p in after.data.chunks_exact_mut(3) {
            p.fill(if p[0] == 160 { 140 } else { 240 });
        }
        assert!(Contours::new(&before).accepts(&after));
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    #[ignore = "set AURA_CONTOUR_REVIEW to a completed twenty-photo assessment directory"]
    fn saved_native_photographs_expose_bright_gradient_regressions() {
        let directory = std::path::PathBuf::from(
            std::env::var("AURA_CONTOUR_REVIEW").expect("review directory"),
        );
        let decode = |stem: &str, suffix: &str| {
            let rgb = image::open(directory.join(format!("{stem}.{suffix}.png")))
                .unwrap()
                .into_rgb8();
            Rgb8 {
                width: rgb.width(),
                height: rgb.height(),
                data: rgb.into_raw(),
            }
        };
        for stem in [
            "09.collection.teal-orange",
            "20.collection.cool-cyan",
            "20.palette.cool-cyan",
        ] {
            let guard = Contours::new(&decode(stem, "before"));
            assert!(
                !guard.accepts(&decode(stem, "matched")),
                "Accepted archived artifact: {stem}"
            );
            assert!(
                guard.accepts(&decode(stem, "expected")),
                "Rejected authored smooth grade: {stem}"
            );
        }
        for stem in ["09.natural.teal-orange", "12.collection.monochrome"] {
            assert!(
                Contours::new(&decode(stem, "before")).accepts(&decode(stem, "matched")),
                "Rejected restrained/monochrome result: {stem}"
            );
        }
        if let Ok(directory) = std::env::var("AURA_CONTOUR_SOFT_REVIEW") {
            for stem in ["09.collection.teal-orange", "20.collection.cool-cyan"] {
                let read = |suffix| {
                    let rgb = image::open(
                        std::path::Path::new(&directory).join(format!("{stem}.{suffix}.png")),
                    )
                    .unwrap()
                    .into_rgb8();
                    Rgb8 {
                        width: rgb.width(),
                        height: rgb.height(),
                        data: rgb.into_raw(),
                    }
                };
                assert!(
                    !Contours::new(&read("before")).accepts(&read("matched")),
                    "An attenuated but still visibly banded endpoint must also be refused: {stem}"
                );
            }
        }
    }
}
