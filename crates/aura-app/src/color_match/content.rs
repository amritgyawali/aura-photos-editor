//! Small image-structure evidence for conservative matching across different scenes.
use aura_raw::codec::Rgb8;

pub(super) const EDGE: u32 = 32;
pub(super) const BYTES: usize = EDGE as usize * EDGE as usize * 3;

pub(super) fn thumbnail(image: &Rgb8) -> Vec<u8> {
    let Some(rgb) = image::RgbImage::from_raw(image.width, image.height, image.data.clone()) else {
        return Vec::new();
    };
    image::imageops::resize(&rgb, EDGE, EDGE, image::imageops::FilterType::Triangle).into_raw()
}

pub(super) fn achromatic(image: &Rgb8) -> bool {
    !image.data.is_empty()
        && image.data.chunks_exact(3).all(|p| {
            p.iter()
                .max()
                .copied()
                .unwrap_or(255)
                .saturating_sub(p.iter().min().copied().unwrap_or(0))
                <= 1
        })
}

/// Signed luminance-gradient correlation tolerates exposure/grade changes but
/// does not treat two different scenes as paired samples merely for sharing colours.
pub(super) fn similarity(reference: &[u8], actual: &Rgb8) -> Option<f32> {
    if reference.len() != BYTES {
        return None;
    }
    let actual = thumbnail(actual);
    if actual.len() != BYTES {
        return None;
    }
    let luma = |bytes: &[u8], index: usize| {
        let channel = |offset: usize| {
            bytes
                .get(index * 3 + offset)
                .copied()
                .map_or(0.0, f32::from)
        };
        channel(0) * 0.2126 + channel(1) * 0.7152 + channel(2) * 0.0722
    };
    let (mut dot, mut norm_a, mut norm_b) = (0.0_f64, 0.0_f64, 0.0_f64);
    for row in 0..EDGE as usize - 1 {
        for column in 0..EDGE as usize - 1 {
            let i = row * EDGE as usize + column;
            for j in [i + 1, i + EDGE as usize] {
                let a = f64::from(luma(reference, j) - luma(reference, i));
                let b = f64::from(luma(&actual, j) - luma(&actual, i));
                dot += a * b;
                norm_a += a * a;
                norm_b += b * b;
            }
        }
    }
    (norm_a * norm_b > 1.0).then(|| (dot / (norm_a * norm_b).sqrt()).clamp(-1.0, 1.0) as f32)
}

pub(super) fn regularization(similarity: Option<f32>) -> f32 {
    if similarity.is_some_and(|v| v >= 0.95) {
        0.25
    } else {
        4.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structure_distinguishes_an_exposure_change_from_a_different_scene() {
        let image = |reverse: bool, offset: u8| Rgb8 {
            width: 32,
            height: 32,
            data: (0..1024)
                .flat_map(|i| {
                    let v = if reverse { (i / 32) * 3 } else { (i % 32) * 3 };
                    [v as u8 + offset; 3]
                })
                .collect(),
        };
        let original = image(false, 20);
        let signature = thumbnail(&original);
        assert!(similarity(&signature, &image(false, 40)).unwrap() > 0.99);
        assert!(similarity(&signature, &image(true, 20)).unwrap() < 0.1);
        assert!(regularization(Some(0.99)) < regularization(Some(0.0)));
        assert!(similarity(&[], &original).is_none());
    }

    #[test]
    fn selective_colour_is_not_mistaken_for_monochrome() {
        let mut image = Rgb8 {
            width: 32,
            height: 32,
            data: vec![120; BYTES],
        };
        assert!(achromatic(&image));
        image.data[0..3].copy_from_slice(&[200, 30, 20]);
        assert!(!achromatic(&image));
    }
}
