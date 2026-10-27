//! Bilinear chromatic displacement: continuous across lattice cells, with no spatial resampling.
use aura_recipe::reference_grade::{
    palette::{EDGE, SPAN},
    PaletteMap,
};

pub(super) fn apply(map: &PaletteMap, lab: &mut [f32; 3]) {
    let [a, b] =
        [lab[1], lab[2]].map(|v| ((v + SPAN) / (2.0 * SPAN)).clamp(0.0, 1.0) * (EDGE - 1) as f32);
    let (x, y) = ((a as usize).min(EDGE - 2), (b as usize).min(EDGE - 2));
    let (tx, ty) = (a - x as f32, b - y as f32);
    for (index, weight) in [
        (y * EDGE + x, (1.0 - tx) * (1.0 - ty)),
        (y * EDGE + x + 1, tx * (1.0 - ty)),
        ((y + 1) * EDGE + x, (1.0 - tx) * ty),
        ((y + 1) * EDGE + x + 1, tx * ty),
    ] {
        if let Some(delta) = map.deltas.get(index) {
            lab[1] += weight * delta[0];
            lab[2] += weight * delta[1];
        }
    }
    lab[1] += map.offset[0];
    lab[2] += map.offset[1];
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chromatic_transport_is_continuous_and_keeps_lightness() {
        let map = PaletteMap {
            offset: [0.0, 0.06],
            deltas: (0..EDGE * EDGE)
                .map(|i| {
                    let [a, b] = PaletteMap::coordinate(i);
                    [-0.2 * a, -0.4 * b]
                })
                .collect(),
        };
        for i in 1..EDGE - 1 {
            let boundary = PaletteMap::coordinate(i)[0];
            let mut left = [0.5, boundary - 1e-6, -0.08];
            let mut right = [0.5, boundary + 1e-6, -0.08];
            apply(&map, &mut left);
            apply(&map, &mut right);
            assert_eq!(left[0].to_bits(), 0.5_f32.to_bits());
            assert!((left[1] - right[1]).abs() < 3e-6);
            assert!((left[2] - right[2]).abs() < 3e-6);
        }
    }
}
