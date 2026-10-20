//! Preserve the chin/neck lighting boundary instead of healing it as a dark spot.
use super::{Geometry, Pixels};

const COLS: usize = 65;
const ROWS: usize = 65;
const SPAN: f32 = 0.85;
const TOP: f32 = 0.18;
const STEP: f32 = 0.82 / (ROWS - 1) as f32;

pub(super) struct Contour {
    origin: [f32; 2],
    u: [f32; 2],
    v: [f32; 2],
    scale: f32,
    down: Vec<f32>,
    pub(super) measured: bool,
}

impl Contour {
    pub(super) fn weight(&self, point: [f32; 2], margin: f32, healing: bool) -> f32 {
        if !self.measured && !healing {
            return 1.0;
        }
        let delta = [point[0] - self.origin[0], point[1] - self.origin[1]];
        let across = (delta[0] * self.u[0] + delta[1] * self.u[1]) / self.scale;
        if across.abs() >= SPAN {
            return 1.0;
        }
        let y = (delta[0] * self.v[0] + delta[1] * self.v[1]) / self.scale;
        let index =
            ((across + SPAN) / (2.0 * SPAN) * (COLS - 1) as f32).clamp(0.0, (COLS - 1) as f32);
        let lo = index.floor() as usize;
        let hi = (lo + 1).min(COLS - 1);
        let target = self.down[lo] * (1.0 - index.fract()) + self.down[hi] * index.fract();
        let core = if self.measured { 0.045 } else { 0.08 };
        let distance = (y - target).abs() - margin / self.scale;
        let clear = smooth((distance - core) / 0.045);
        let lateral = smooth((SPAN - across.abs()) / 0.1);
        1.0 - (1.0 - clear) * lateral
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Trace a coherent brightness drop below the mouth, in the rotated eye/mouth frame.
/// A single dark spot cannot establish a jaw: most columns must support the same
/// continuous boundary. Ratios make the evidence independent of exposure/complexion.
pub(super) fn measure(g: &Geometry, px: &Pixels<'_>) -> Option<Contour> {
    if g.d < 12.0 {
        return None;
    }
    let centre = if g.v[1] > 0.6 {
        ((g.bounds[3] - g.mouth_centre[1]) / g.v[1] / g.d).clamp(0.45, 0.95)
    } else {
        0.68
    };
    let expected: Vec<f32> = (0..COLS)
        .map(|col| {
            let x = -SPAN + 2.0 * SPAN * col as f32 / (COLS - 1) as f32;
            centre - 0.36 * (x / SPAN).powi(2)
        })
        .collect();
    let sample = |x: f32, y: f32| -> Option<f32> {
        let a = g.mouth_centre[0] + g.d * (g.u[0] * x + g.v[0] * y);
        let b = g.mouth_centre[1] + g.d * (g.u[1] * x + g.v[1] * y);
        if a < 0.0 || b < 0.0 || a >= px.width as f32 || b >= px.height as f32 {
            return None;
        }
        let p = px.linear(
            a.round().min((px.width - 1) as f32) as usize,
            b.round().min((px.height - 1) as f32) as usize,
        );
        Some(super::luma(p))
    };
    let mut evidence = vec![0.0_f32; COLS * ROWS];
    let mut score = vec![f32::NEG_INFINITY; COLS * ROWS];
    let mut previous = vec![0_usize; COLS * ROWS];
    for col in 0..COLS {
        let x = -SPAN + 2.0 * SPAN * col as f32 / (COLS - 1) as f32;
        for row in 0..ROWS {
            let y = TOP + row as f32 * STEP;
            if (y - expected[col]).abs() > 0.26 {
                continue;
            }
            let (mut above, mut below, mut count) = (0.0, 0.0, 0);
            for dx in [-0.018, 0.0, 0.018] {
                for dy in [0.018, 0.035, 0.055] {
                    if let (Some(a), Some(b)) = (sample(x + dx, y - dy), sample(x + dx, y + dy)) {
                        above += a;
                        below += b;
                        count += 1;
                    }
                }
            }
            if count < 9 || above < 0.0001 || below < 0.0001 {
                continue;
            }
            let contrast = (above / below).ln().clamp(0.0, 0.9);
            evidence[col * ROWS + row] = contrast;
            let local = contrast - 0.8 * (y - expected[col]).abs();
            if col == 0 {
                score[row] = local;
                continue;
            }
            let slope = expected[col] - expected[col - 1];
            for prev in row.saturating_sub(4)..=(row + 4).min(ROWS - 1) {
                let bend = ((row as f32 - prev as f32) * STEP - slope).abs();
                let value = score[(col - 1) * ROWS + prev] + local - bend * 1.5;
                if value > score[col * ROWS + row] {
                    score[col * ROWS + row] = value;
                    previous[col * ROWS + row] = prev;
                }
            }
        }
    }
    let mut row = (0..ROWS)
        .max_by(|a, b| score[(COLS - 1) * ROWS + *a].total_cmp(&score[(COLS - 1) * ROWS + *b]))?;
    let mut path = vec![0; COLS];
    for col in (0..COLS).rev() {
        path[col] = row;
        row = previous[col * ROWS + row];
    }
    let supported = path
        .iter()
        .enumerate()
        .filter(|(c, r)| evidence[*c * ROWS + **r] > 0.12)
        .count();
    let measured = supported * 5 >= COLS * 3;
    let down = if measured {
        (0..COLS)
            .map(|c| {
                let mut neighbours: Vec<_> = path[c.saturating_sub(2)..(c + 3).min(COLS)].to_vec();
                neighbours.sort_unstable();
                TOP + neighbours[neighbours.len() / 2] as f32 * STEP
            })
            .collect()
    } else {
        expected
    };
    Some(Contour {
        origin: g.mouth_centre,
        u: g.u,
        v: g.v,
        scale: g.d,
        down,
        measured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_vision::portrait::PortraitFace;
    fn face() -> PortraitFace {
        PortraitFace {
            bounds: [0.1, 0.08, 0.9, 0.85],
            landmarks: [
                [0.35, 0.3],
                [0.65, 0.3],
                [0.5, 0.45],
                [0.4, 0.6],
                [0.6, 0.6],
            ],
            confidence: 0.99,
        }
    }
    #[test]
    fn coherent_shadow_is_protected_without_excluding_chin_skin_above_it() {
        for (skin, neck) in [(210, 135), (100, 64), (45, 27)] {
            let mut rgb = vec![0; 256 * 256 * 3];
            for y in 0..256 {
                for x in 0..256 {
                    let edge = 210.0 - 0.003 * (x as f32 - 128.0).powi(2);
                    let value = if (y as f32) < edge { skin } else { neck };
                    rgb[(y * 256 + x) * 3..(y * 256 + x) * 3 + 3].copy_from_slice(&[value; 3]);
                }
            }
            let px = Pixels::new(&rgb, 256, 256).unwrap();
            let g = Geometry::new(&face(), &px).unwrap();
            let contour = measure(&g, &px).unwrap();
            assert!(contour.measured);
            for x in [90.0, 110.0, 128.0, 145.0, 165.0] {
                let edge = 210.0 - 0.003 * (x - 128.0_f32).powi(2);
                assert!(
                    contour.weight([x, edge], 1.0, true) < 0.05,
                    "jaw missed at {x}"
                );
                assert!(
                    contour.weight([x, edge - 18.0], 0.0, true) > 0.99,
                    "chin skin excluded"
                );
            }
        }
    }
    #[test]
    fn an_isolated_dark_spot_cannot_establish_an_anatomical_boundary() {
        let mut rgb = vec![150; 256 * 256 * 3];
        for y in 203..210 {
            for x in 125..132 {
                rgb[(y * 256 + x) * 3..(y * 256 + x) * 3 + 3].fill(40);
            }
        }
        let px = Pixels::new(&rgb, 256, 256).unwrap();
        let g = Geometry::new(&face(), &px).unwrap();
        let contour = measure(&g, &px).unwrap();
        assert!(!contour.measured);
        assert_eq!(contour.weight([128.0, 205.0], 0.0, false), 1.0);
    }

    #[test]
    fn a_quarter_turn_keeps_the_same_chin_boundary_protected() {
        let mut original = vec![0; 256 * 256 * 3];
        let mut rotated = original.clone();
        for y in 0..256 {
            for x in 0..256 {
                let edge = 210.0 - 0.003 * (x as f32 - 128.0).powi(2);
                let value = if (y as f32) < edge { 150 } else { 85 };
                original[(y * 256 + x) * 3..(y * 256 + x) * 3 + 3].fill(value);
                let at = (x * 256 + 255 - y) * 3;
                rotated[at..at + 3].fill(value);
            }
        }
        let mut turned = face();
        turned.landmarks = turned.landmarks.map(|[x, y]| [1.0 - y, x]);
        let [l, t, r, b] = turned.bounds;
        turned.bounds = [1.0 - b, l, 1.0 - t, r];
        let px = Pixels::new(&rotated, 256, 256).unwrap();
        let g = Geometry::new(&turned, &px).unwrap();
        let jaw = measure(&g, &px).unwrap();
        assert!(jaw.measured);
        for x in [105.0, 128.0, 150.0] {
            let y = 210.0 - 0.003 * (x - 128.0_f32).powi(2);
            assert!(jaw.weight([255.0 - y, x], 2.0, true) < 0.05);
            assert!(jaw.weight([255.0 - y + 20.0, x], 0.0, true) > 0.99);
        }
    }
}
