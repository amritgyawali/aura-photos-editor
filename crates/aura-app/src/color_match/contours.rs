//! Bound newly amplified steps in originally smooth, dark rendered regions.
//!
//! These are authored RGB8 quality limits, not perceptual accuracy guarantees.
//! Fixed, stratified adjacent-pixel observations keep checking bounded and prevent
//! a histogram improvement from hiding conspicuous shadow contouring.
use aura_raw::codec::Rgb8;

const PIXELS: usize = 4096;
const MIN_PAIRS: usize = 128;
const DARK: u8 = 96;
const SOURCE_STEP: u8 = 2;
const OUTPUT_STEP: u8 = 12;
const MAX_FRACTION: f32 = 0.005;

#[derive(Debug, Default)]
pub(super) struct Contours {
    width: u32,
    height: u32,
    pairs: Vec<[usize; 2]>,
}

fn pixel(image: &Rgb8, index: usize) -> Option<&[u8]> {
    image
        .data
        .get(index.checked_mul(3)?..index.checked_mul(3)?.checked_add(3)?)
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
                }
            }
        }
        result
    }

    pub(super) fn accepts(&self, after: &Rgb8) -> bool {
        if after.width != self.width || after.height != self.height || !valid(after) {
            return false;
        }
        if self.pairs.len() < MIN_PAIRS {
            return true; // Insufficient flat-region evidence is not a quality proof.
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
}
