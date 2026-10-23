//! Inward-only correction envelopes. This constrains support, not skin classification.
// All indices are derived from a validated rectangular plane.
#![allow(clippy::indexing_slicing)]

pub(crate) const VERSION: &str = "inward-distance-v1";

/// Limit a correction by distance to zero coverage, including internal protected holes.
/// Two chamfer passes cost O(pixels), independent of feather width. A half-pixel offset
/// places the boundary between selected/unselected pixel centres. Narrow strips retain
/// reduced coverage; no pixel outside support is added. Frame edges are not exclusions.
pub(crate) fn inward(weights: &mut [f32], w: usize, h: usize, width: f32, exterior: [bool; 4]) {
    if w == 0 || h == 0 || w.checked_mul(h) != Some(weights.len()) || !width.is_finite() {
        weights.fill(0.0);
        return;
    }
    let width = width.clamp(1.0, 16.0);
    let mut distance = vec![width + 1.0; weights.len()];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !weights[i].is_finite() || weights[i] <= 0.0 {
                weights[i] = 0.0;
                distance[i] = 0.0;
            } else if (x == 0 && exterior[0])
                || (y == 0 && exterior[1])
                || (x + 1 == w && exterior[2])
                || (y + 1 == h && exterior[3])
            {
                distance[i] = 1.0;
            }
        }
    }
    let diagonal = std::f32::consts::SQRT_2;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x > 0 {
                distance[i] = distance[i].min(distance[i - 1] + 1.0);
            }
            if y > 0 {
                distance[i] = distance[i].min(distance[i - w] + 1.0);
                if x > 0 {
                    distance[i] = distance[i].min(distance[i - w - 1] + diagonal);
                }
                if x + 1 < w {
                    distance[i] = distance[i].min(distance[i - w + 1] + diagonal);
                }
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if x + 1 < w {
                distance[i] = distance[i].min(distance[i + 1] + 1.0);
            }
            if y + 1 < h {
                distance[i] = distance[i].min(distance[i + w] + 1.0);
                if x > 0 {
                    distance[i] = distance[i].min(distance[i + w - 1] + diagonal);
                }
                if x + 1 < w {
                    distance[i] = distance[i].min(distance[i + w + 1] + diagonal);
                }
            }
        }
    }
    for (weight, distance) in weights.iter_mut().zip(distance) {
        let t = ((distance - 0.5) / width).clamp(0.0, 1.0);
        // Do not multiply: an already gentler authored feather needs no extra erosion.
        *weight = weight.min(t * t * (3.0 - 2.0 * t));
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Exact exclusion and identity are part of the mask contract.
mod tests {
    use super::*;

    #[test]
    fn holes_and_exterior_never_gain_support_and_thin_skin_is_reduced_not_erased() {
        let mut mask = vec![1.0; 15 * 15];
        for y in 0..15 {
            mask[y * 15 + 7] = 0.0;
            mask[y * 15 + 9] = 0.0;
        }
        let before = mask.clone();
        inward(&mut mask, 15, 15, 3.0, [true; 4]);
        assert!(mask.iter().zip(before).all(|(a, b)| *a <= b));
        assert_eq!(mask[7 * 15 + 7], 0.0);
        assert!(mask[7 * 15 + 8] > 0.0 && mask[7 * 15 + 8] < 0.1);
        assert!(mask[7 * 15 + 5] > mask[7 * 15 + 6]);
        assert_eq!(mask[7 * 15 + 3], 1.0);
    }

    #[test]
    fn frame_edges_soft_authored_masks_and_rotation_are_consistent() {
        let mut full = vec![0.2; 9 * 9];
        inward(&mut full, 9, 9, 3.0, [false; 4]);
        assert!(full.iter().all(|v| *v == 0.2));
        let mut a = vec![1.0; 9 * 9];
        a[2 * 9 + 4] = 0.0;
        let mut b: Vec<_> = (0..81).map(|i| a[(i % 9) * 9 + i / 9]).collect();
        inward(&mut a, 9, 9, 3.0, [true; 4]);
        inward(&mut b, 9, 9, 3.0, [true; 4]);
        for i in 0..81 {
            assert!((a[i] - b[(i % 9) * 9 + i / 9]).abs() < 1e-6);
        }
    }

    #[test]
    fn invalid_support_cannot_become_an_edit() {
        let mut values = vec![1.0, f32::NAN, f32::INFINITY, -1.0];
        inward(&mut values, 2, 2, 1.0, [false; 4]);
        assert_eq!(&values[1..], &[0.0; 3]);
        inward(&mut values, 3, 2, 1.0, [false; 4]);
        assert_eq!(values, vec![0.0; 4]);
    }
}
