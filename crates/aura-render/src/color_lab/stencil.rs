//! Fixed-capacity selected-only colour neighbourhoods: no per-pixel heap allocation.
use super::pixel;
#[derive(Clone, Copy, Default)]
pub(super) struct Sample {
    pub(super) rgb: [f32; 3],
    pub(super) weight: f32,
    pub(super) distance: f32,
}
pub(super) struct Samples {
    items: [Sample; 49],
    len: usize,
}
impl Samples {
    pub(super) fn iter(&self) -> std::slice::Iter<'_, Sample> {
        self.items[..self.len].iter()
    }
}
fn coordinates(at: usize, limit: usize, radius: usize) -> ([usize; 7], usize) {
    let mut values = [0; 7];
    let mut len = 0;
    for offset in -3_isize..=3 {
        let value =
            (at as isize + offset * radius as isize / 3).clamp(0, limit as isize - 1) as usize;
        if len == 0 || values[len - 1] != value {
            values[len] = value;
            len += 1;
        }
    }
    (values, len)
}
pub(super) fn selected(
    src: &[f32],
    weights: &[f32],
    w: usize,
    h: usize,
    radius: usize,
    i: usize,
) -> Samples {
    let mut samples = Samples {
        items: [Sample::default(); 49],
        len: 0,
    };
    let reach = radius.min(w.max(h)).max(1);
    let x = i % w;
    let y = i / w;
    let (xs, nx) = coordinates(x, w, reach);
    let (ys, ny) = coordinates(y, h, reach);
    for yy in &ys[..ny] {
        for xx in &xs[..nx] {
            let at = yy * w + xx;
            if weights[at] > 0.0 {
                samples.items[samples.len] = Sample {
                    rgb: pixel(src, at),
                    weight: weights[at],
                    distance: ((*xx as f32 - x as f32).powi(2) + (*yy as f32 - y as f32).powi(2))
                        / (reach * reach) as f32,
                };
                samples.len += 1;
            }
        }
    }
    samples
}
