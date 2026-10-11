//! Manual local proportional change. It is never invoked by automatic portrait cleanup.
// All coordinates are checked against the frame before indexing or interpolation.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::Edit;
use rayon::prelude::*;

pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    if edit.warmth == 0.0 && edit.tint == 0.0 {
        return;
    }
    let source = rgb.to_vec();
    let [cx, cy, rx, ry] = edit.region;
    // Inverse mapping reads only from the immutable input. One resampling, no ghost-image blend.
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        for x in coverage.bounds[0]..coverage.bounds[2] {
            let mask = coverage.at(x, y, w, h);
            if mask <= 0.0 {
                continue;
            }
            let dx = (x as f32 + 0.5) / w as f32 - cx;
            let dy = (y as f32 + 0.5) / h as f32 - cy;
            let distance2 = (dx / rx).powi(2) + (dy / ry).powi(2);
            if distance2 >= 1.0 {
                continue;
            }
            let falloff = (1.0 - distance2).powi(2) * mask * edit.amount;
            let sx = (cx + dx / (1.0 + edit.warmth * 0.25 * falloff)) * w as f32 - 0.5;
            let sy = (cy + dy / (1.0 + edit.tint * 0.25 * falloff)) * h as f32 - 0.5;
            if sx < 0.0 || sy < 0.0 || sx >= (w - 1) as f32 || sy >= (h - 1) as f32 {
                continue;
            }
            let x0 = sx.floor() as usize;
            let y0 = sy.floor() as usize;
            // A masked-off facial feature cannot be borrowed into adjacent skin.
            if [(x0, y0), (x0 + 1, y0), (x0, y0 + 1), (x0 + 1, y0 + 1)]
                .iter()
                .any(|(xx, yy)| coverage.at(*xx, *yy, w, h) <= 0.0)
            {
                continue;
            }
            let fx = sx - x0 as f32;
            let fy = sy - y0 as f32;
            for c in 0..3 {
                let top = source[(y0 * w + x0) * 3 + c] * (1.0 - fx)
                    + source[(y0 * w + x0 + 1) * 3 + c] * fx;
                let bottom = source[((y0 + 1) * w + x0) * 3 + c] * (1.0 - fx)
                    + source[((y0 + 1) * w + x0 + 1) * 3 + c] * fx;
                row[x * 3 + c] = top * (1.0 - fy) + bottom * fy;
            }
        }
    });
}
