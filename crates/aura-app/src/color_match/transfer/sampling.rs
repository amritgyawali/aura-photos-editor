//! Bounded spatial sampling without locking onto a regular image texture.
use super::{oklab_from_linear_srgb, srgb_byte_to_linear, Rgb8, Sample};

/// One deterministic sample per spatial stratum. Jitter breaks the fixed-grid aliasing that
/// can otherwise measure only one colour of striped fabric, screens or repeated patterns.
pub(in crate::color_match) fn samples(image: &Rgb8, limit: usize) -> Vec<Sample> {
    let (width, height) = (image.width as usize, image.height as usize);
    let Some(total) = width.checked_mul(height) else {
        return Vec::new();
    };
    if total == 0 || limit == 0 || total.checked_mul(3).is_none_or(|n| image.data.len() < n) {
        return Vec::new();
    }
    let count = total.min(limit);
    let columns = ((count as f64 * width as f64 / height as f64).sqrt().round() as usize)
        .clamp(1, width.min(count));
    let rows = (count / columns).clamp(1, height);
    let mut out = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        let (y0, y1) = (row * height / rows, (row + 1) * height / rows);
        for column in 0..columns {
            let (x0, x1) = (column * width / columns, (column + 1) * width / columns);
            let mut hash = (row as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ (column as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            hash ^= hash >> 31;
            let x = x0 + hash as usize % (x1 - x0);
            let y = y0 + (hash >> 32) as usize % (y1 - y0);
            let offset = (y * width + x) * 3;
            if let Some(&[r, g, b]) = image.data.get(offset..offset + 3) {
                let linear = [r, g, b].map(srgb_byte_to_linear);
                out.push(Sample {
                    linear,
                    lab: oklab_from_linear_srgb(linear),
                });
            }
        }
    }
    out
}
