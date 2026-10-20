//! Conservative protection for strongly red/magenta forehead decorations.
//! Enabled by the wedding finish, separately from acne and permanent-mark controls.
use super::{Geometry, Pixels};
use std::collections::VecDeque;

pub(super) struct Region {
    bounds: [f32; 4],
    width: usize,
    height: usize,
    distance: Vec<f32>,
    feather: f32,
}

impl Region {
    pub(super) fn weight(&self, [x, y]: [f32; 2]) -> f32 {
        let [l, t, r, b] = self.bounds;
        if x < l || x >= r || y < t || y >= b {
            return 1.0;
        }
        let xx = (x - l - 0.5).clamp(0.0, self.width as f32 - 1.0);
        let yy = (y - t - 0.5).clamp(0.0, self.height as f32 - 1.0);
        let (x0, y0) = (xx.floor() as usize, yy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (xx - x0 as f32, yy - y0 as f32);
        let at = |a, b| self.distance[b * self.width + a];
        let a = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let b = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        // Cover the pigment pixel and half an analysis cell of resampling
        // uncertainty, then recover coverage on adjacent skin. Without this
        // margin full-resolution anti-aliased fringes can be trimmed again.
        // A component's bounding rectangle is not the shape of a tika.
        let v = ((a * (1.0 - fy) + b * fy - 1.3) / self.feather).clamp(0.0, 1.0);
        v * v * (3.0 - 2.0 * v)
    }
}

/// Landmarks locate the forehead, not a universal skin colour. Strong red pigment
/// or magenta decoration is protected there; ordinary skin and cheek acne are not
/// included. Connected components keep separate marks from excluding all skin
/// between a bindi and pigment at the hairline.
pub(super) fn regions(g: &Geometry, px: &Pixels<'_>) -> Vec<Region> {
    let [l, t, r, b] = g.bounds;
    let (x0, y0) = (l.floor().max(0.0) as usize, t.floor().max(0.0) as usize);
    let (x1, y1) = (
        (r.ceil().max(0.0) as usize).min(px.width),
        (b.ceil().max(0.0) as usize).min(px.height),
    );
    if x1 <= x0 || y1 <= y0 {
        return Vec::new();
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut candidates = vec![false; w * h];
    let mut fringe = vec![false; w * h];
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - g.mid[0];
            let dy = y as f32 + 0.5 - g.mid[1];
            let across = (dx * g.u[0] + dy * g.u[1]) / g.d;
            let down = (dx * g.v[0] + dy * g.v[1]) / g.d;
            if across.abs() > 0.65 || !(-1.8..=-0.18).contains(&down) {
                continue;
            }
            let [red, green, blue] = px.encoded(x, y);
            let vermilion = red > 1.8 * (green + blue);
            let magenta = red > 1.25 * blue && green < 0.55 * blue;
            if red > 0.08 && (vermilion || magenta) {
                candidates[(y - y0) * w + x - x0] = true;
            }
            // Anti-aliased pigment mixes with lit skin and is less saturated
            // than the seed. It can join an existing decoration, never create one.
            fringe[(y - y0) * w + x - x0] =
                red > 0.08 && red > 1.28 * green && red > 1.15 * blue && blue >= 0.80 * green;
        }
    }
    // Bounded hysteresis protects the soft edge without excluding an entire
    // bounding rectangle or bridging distant red marks through ordinary skin.
    let reach = (g.d * 0.02).ceil().clamp(1.0, 6.0) as usize;
    let mut queue: VecDeque<_> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, selected)| selected.then_some((i, 0_usize)))
        .collect();
    while let Some((i, depth)) = queue.pop_front() {
        if depth >= reach {
            continue;
        }
        let (x, y) = (i % w, i / w);
        for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
            for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                let j = yy * w + xx;
                if fringe[j] && !candidates[j] {
                    candidates[j] = true;
                    queue.push_back((j, depth + 1));
                }
            }
        }
    }
    super::deep_blemish::components(&candidates, w, h)
        .into_iter()
        .map(|component| {
            let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
            for &i in &component {
                left = left.min(i % w);
                top = top.min(i / w);
                right = right.max(i % w + 1);
                bottom = bottom.max(i / w + 1);
            }
            let feather = (g.d * 0.008).max(0.75);
            let pad = (feather * 2.0 + 3.0).ceil() as usize;
            let left = left.saturating_sub(pad);
            let top = top.saturating_sub(pad);
            let right = (right + pad).min(w);
            let bottom = (bottom + pad).min(h);
            let (rw, rh) = (right - left, bottom - top);
            let mut distance = vec![(rw + rh) as f32; rw * rh];
            for i in component {
                distance[(i / w - top) * rw + i % w - left] = 0.0;
            }
            // Distance to actual pigment, including concavities and holes.
            for y in 0..rh {
                for x in 0..rw {
                    let i = y * rw + x;
                    if x > 0 {
                        distance[i] = distance[i].min(distance[i - 1] + 1.0);
                    }
                    if y > 0 {
                        distance[i] = distance[i].min(distance[i - rw] + 1.0);
                    }
                    if x > 0 && y > 0 {
                        distance[i] =
                            distance[i].min(distance[i - rw - 1] + std::f32::consts::SQRT_2);
                    }
                    if x + 1 < rw && y > 0 {
                        distance[i] =
                            distance[i].min(distance[i - rw + 1] + std::f32::consts::SQRT_2);
                    }
                }
            }
            for y in (0..rh).rev() {
                for x in (0..rw).rev() {
                    let i = y * rw + x;
                    if x + 1 < rw {
                        distance[i] = distance[i].min(distance[i + 1] + 1.0);
                    }
                    if y + 1 < rh {
                        distance[i] = distance[i].min(distance[i + rw] + 1.0);
                    }
                    if x + 1 < rw && y + 1 < rh {
                        distance[i] =
                            distance[i].min(distance[i + rw + 1] + std::f32::consts::SQRT_2);
                    }
                    if x > 0 && y + 1 < rh {
                        distance[i] =
                            distance[i].min(distance[i + rw - 1] + std::f32::consts::SQRT_2);
                    }
                }
            }
            Region {
                bounds: [
                    (x0 + left) as f32,
                    (y0 + top) as f32,
                    (x0 + right) as f32,
                    (y0 + bottom) as f32,
                ],
                width: rw,
                height: rh,
                distance,
                feather,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connected_pigment_fringe_is_preserved_without_claiming_adjacent_skin() {
        let face = aura_vision::portrait::PortraitFace {
            bounds: [0.1, 0.05, 0.9, 0.95],
            confidence: 0.99,
            landmarks: [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.5, 0.58],
                [0.4, 0.72],
                [0.6, 0.72],
            ],
        };
        let mut rgb = [220_u8, 180, 150].repeat(256 * 256);
        for y in 63..=67 {
            for x in 126..=130 {
                let colour = if (127..=129).contains(&x) && (64..=66).contains(&y) {
                    [150, 25, 60]
                } else {
                    [210, 130, 135]
                };
                rgb[(y * 256 + x) * 3..(y * 256 + x) * 3 + 3].copy_from_slice(&colour);
            }
        }
        let px = Pixels::new(&rgb, 256, 256).unwrap();
        let g = Geometry::new(&face, &px).unwrap();
        let regions = regions(&g, &px);
        for point in [[126.5, 65.5], [130.5, 65.5], [128.5, 63.5], [128.5, 67.5]] {
            assert!(
                regions.iter().any(|r| r.weight(point) == 0.0),
                "pigment fringe lost at {point:?}"
            );
        }
        assert!(regions.iter().all(|r| r.weight([134.5, 65.5]) > 0.95));
    }
    #[test]
    fn irregular_tika_protects_pigment_without_a_rectangular_skin_hole() {
        let face = aura_vision::portrait::PortraitFace {
            bounds: [0.1, 0.05, 0.9, 0.95],
            landmarks: [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.5, 0.58],
                [0.4, 0.72],
                [0.6, 0.72],
            ],
            confidence: 0.99,
        };
        let mut rgb = [150_u8, 100, 75].repeat(256 * 256);
        for y in 40..=70 {
            for x in 112..=140 {
                if x <= 115 || y >= 67 {
                    rgb[(y * 256 + x) * 3..(y * 256 + x) * 3 + 3].copy_from_slice(&[190, 20, 15]);
                }
            }
        }
        let px = Pixels::new(&rgb, 256, 256).unwrap();
        let g = Geometry::new(&face, &px).unwrap();
        let regions = regions(&g, &px);
        assert!(regions.iter().any(|r| r.weight([113.5, 50.5]) == 0.0));
        assert!(regions.iter().any(|r| r.weight([130.5, 68.5]) == 0.0));
        assert!(
            regions.iter().all(|r| r.weight([130.5, 50.5]) == 1.0),
            "clear skin inside a pigment bounding box must remain available"
        );
        assert!(
            regions.iter().all(|r| r.weight([119.5, 50.5]) > 0.95),
            "skin beside the pigment must recover coverage promptly"
        );
    }
    #[test]
    fn bindi_and_hairline_pigment_are_kept_while_ordinary_skin_and_cheek_acne_remain_available() {
        let face = aura_vision::portrait::PortraitFace {
            bounds: [0.1, 0.05, 0.9, 0.95],
            landmarks: [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.5, 0.58],
                [0.4, 0.72],
                [0.6, 0.72],
            ],
            confidence: 0.99,
        };
        let mut rgb = [150_u8, 100, 75].repeat(256 * 256);
        for (cx, cy, colour) in [
            (128, 65, [125, 16, 60]),
            (128, 35, [190, 20, 15]),
            (90, 140, [190, 60, 45]),
        ] {
            for y in cy - 2..=cy + 2 {
                for x in cx - 2..=cx + 2 {
                    rgb[(y * 256 + x) * 3..(y * 256 + x) * 3 + 3].copy_from_slice(&colour);
                }
            }
        }
        let px = Pixels::new(&rgb, 256, 256).unwrap();
        let g = Geometry::new(&face, &px).unwrap();
        let regions = regions(&g, &px);
        assert_eq!(regions.len(), 2);
        for p in [[128., 65.], [128., 35.]] {
            assert!(regions.iter().any(|r| r.weight(p) == 0.0));
        }
        for p in [[90., 140.], [128., 90.], [80., 65.]] {
            assert!(regions.iter().all(|r| r.weight(p) == 1.0));
        }
    }
}
