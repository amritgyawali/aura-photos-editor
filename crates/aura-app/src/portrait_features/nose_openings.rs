//! Measure two closed, dark nasal cavities inside a bounded landmark search.
//! Ambiguous or unbounded shadows retain the conservative landmark fallback.
#![allow(clippy::indexing_slicing)]
use super::{deep_blemish::components, Geometry, Pixels};

#[derive(Clone, Copy, Debug)]
pub(super) struct Opening {
    pub across: f32,
    pub down: f32,
    pub rx: f32,
    pub ry: f32,
}

fn measure_one(g: &Geometry, px: &Pixels<'_>, side: f32) -> Option<Opening> {
    const W: usize = 81;
    const H: usize = 89;
    let coordinates = |i: usize| {
        [
            side * (0.03 + (i % W) as f32 / (W - 1) as f32 * 0.27),
            -0.015 + (i / W) as f32 / (H - 1) as f32 * 0.315,
        ]
    };
    let mut light = Vec::with_capacity(W * H);
    for i in 0..W * H {
        let [u, v] = coordinates(i);
        let x = g.nose[0] + g.d * (u * g.u[0] + v * g.v[0]);
        let y = g.nose[1] + g.d * (u * g.u[1] + v * g.v[1]);
        if x < 0.0 || y < 0.0 || x >= px.width as f32 || y >= px.height as f32 {
            return None;
        }
        light.push(super::luma(px.encoded(x as usize, y as usize)));
    }
    let mut sorted = light.clone();
    sorted.sort_by(f32::total_cmp);
    let reference = sorted[sorted.len() / 2];
    let cell_area = 0.27 / (W - 1) as f32 * 0.315 / (H - 1) as f32;
    let mut best = None;
    let mut best_area = 0.0;
    for threshold in [0.30, 0.38, 0.48] {
        let selected: Vec<_> = light.iter().map(|v| *v < reference * threshold).collect();
        for group in components(&selected, W, H) {
            let area = group.len() as f32 * cell_area;
            if !(0.0015..=0.018).contains(&area)
                || group
                    .iter()
                    .any(|i| i % W == 0 || i % W + 1 == W || i / W == 0 || i / W + 1 == H)
                || group.iter().map(|&i| light[i]).sum::<f32>() / group.len() as f32
                    > reference * 0.40
            {
                continue;
            }
            let count = group.len() as f32;
            let across = group.iter().map(|&i| coordinates(i)[0]).sum::<f32>() / count;
            let down = group.iter().map(|&i| coordinates(i)[1]).sum::<f32>() / count;
            let mut radii = [0.0_f32; 2];
            for &i in &group {
                let p = coordinates(i);
                radii[0] = radii[0].max((p[0] - across).abs());
                radii[1] = radii[1].max((p[1] - down).abs());
            }
            // Cover the measured opening and its immediate edge, including sampling.
            radii = [(radii[0] + 0.012).max(0.045), (radii[1] + 0.012).max(0.028)];
            let extent = group
                .iter()
                .map(|&i| {
                    let p = coordinates(i);
                    ((p[0] - across) / radii[0]).hypot((p[1] - down) / radii[1])
                })
                .fold(1.0_f32, f32::max);
            radii = radii.map(|r| r * extent);
            if radii[0] > 0.13 || radii[1] > 0.08 || !(0.02..=0.24).contains(&down) {
                continue;
            }
            if area > best_area {
                best_area = area;
                best = Some(Opening {
                    across,
                    down,
                    rx: radii[0],
                    ry: radii[1],
                });
            }
        }
    }
    best
}

pub(super) fn measure(g: &Geometry, px: &Pixels<'_>) -> Option<[Opening; 2]> {
    let left = measure_one(g, px, -1.0)?;
    let right = measure_one(g, px, 1.0)?;
    // Require the pair, with compatible vertical placement. One ambiguous side
    // cannot silently reduce the conservative protection on the other side.
    if (left.down - right.down).abs() > 0.10 {
        return None;
    }
    Some([left, right])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    #[test]
    fn uniform_skin_and_unbounded_shadows_keep_the_fallback() {
        let face = aura_vision::portrait::PortraitFace {
            bounds: [0.1, 0.1, 0.9, 0.9],
            confidence: 0.99,
            landmarks: [[0.35, 0.3], [0.65, 0.3], [0.5, 0.5], [0.4, 0.7], [0.6, 0.7]],
        };
        for shadow in [false, true] {
            let rgb: Vec<_> = (0..256 * 256)
                .flat_map(|i| {
                    if shadow && i / 256 > 130 {
                        [15_u8; 3]
                    } else {
                        [150_u8, 100, 75]
                    }
                })
                .collect();
            let px = Pixels::new(&rgb, 256, 256).unwrap();
            let g = Geometry::new(&face, &px).unwrap();
            assert!(
                g.nostrils.is_none(),
                "an unbounded shadow cannot reduce anatomy protection"
            );
        }
    }
}
