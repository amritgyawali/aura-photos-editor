//! Continuous quantile transport, with coincident input knots collapsed once.
//!
//! Histogram atoms do not contain enough information to assign different outputs
//! to identical inputs. Their output centroid supplies one unambiguous value.
//! Bounded interval slopes avoid stretching a near-flat colour patch into bands.

#[derive(Debug)]
pub(super) struct QuantileMap {
    x: Vec<f32>,
    y: Vec<f32>,
    ceiling: f32,
    tails: [f32; 2],
}

impl QuantileMap {
    pub(super) fn new(from: &[f32], to: &[f32], ceiling: f32, slopes: (f32, f32)) -> Self {
        Self::build(from, to, ceiling, slopes, false)
    }

    pub(super) fn bounded(from: &[f32], to: &[f32], ceiling: f32, slopes: (f32, f32)) -> Self {
        Self::build(from, to, ceiling, slopes, true)
    }

    fn build(from: &[f32], to: &[f32], ceiling: f32, slopes: (f32, f32), bounded: bool) -> Self {
        let mut x = Vec::new();
        let mut y = Vec::new();
        let mut weights = Vec::new();
        for (&input, &output) in from.iter().zip(to) {
            if x.last()
                .is_some_and(|last: &f32| last.total_cmp(&input).is_eq())
            {
                if let (Some(sum), Some(count)) = (y.last_mut(), weights.last_mut()) {
                    *sum += output;
                    *count += 1.0;
                }
            } else {
                x.push(input);
                y.push(output);
                weights.push(1.0);
            }
        }
        for (sum, &count) in y.iter_mut().zip(&weights) {
            *sum /= count;
        }
        if x.len() < 2 {
            return Self {
                x,
                y,
                ceiling,
                tails: [1.0; 2],
            };
        }
        if !bounded {
            let gain = |inputs: &[f32], outputs: &[f32]| match (inputs, outputs) {
                ([low, high], [start, end]) => {
                    ((end - start) / (high - low)).clamp(slopes.0, slopes.1)
                }
                _ => 1.0,
            };
            let tails = [
                gain(
                    x.get(..2).unwrap_or_default(),
                    y.get(..2).unwrap_or_default(),
                ),
                gain(
                    x.get(x.len() - 2..).unwrap_or_default(),
                    y.get(y.len() - 2..).unwrap_or_default(),
                ),
            ];
            return Self {
                x,
                y,
                ceiling,
                tails,
            };
        }
        // Integrate bounded increments, then choose the weighted least-squares
        // intercept. This retains the mean displacement without an unstable gain
        // in intervals where the source has almost no measured variation.
        let mut integrated = vec![0.0];
        let mut value = 0.0;
        let mut gains = Vec::with_capacity(x.len() - 1);
        for (inputs, outputs) in x.windows(2).zip(y.windows(2)) {
            let ([low, high], [start, end]) = (inputs, outputs) else {
                continue;
            };
            let width = high - low;
            let gain = ((end - start) / width).clamp(slopes.0, slopes.1);
            value += gain * width;
            integrated.push(value);
            gains.push(gain);
        }
        let mass: f32 = weights.iter().sum();
        let intercept = y
            .iter()
            .zip(&integrated)
            .zip(&weights)
            .map(|((&wanted, &mapped), &weight)| weight * (wanted - mapped))
            .sum::<f32>()
            / mass;
        let tails = [
            gains.first().copied().unwrap_or(1.0),
            gains.last().copied().unwrap_or(1.0),
        ];
        Self {
            x,
            y: integrated.into_iter().map(|v| v + intercept).collect(),
            ceiling,
            tails,
        }
    }

    pub(super) fn apply(&self, value: f32) -> f32 {
        let Some((&first, &start)) = self.x.first().zip(self.y.first()) else {
            return value;
        };
        let Some((&last, &end)) = self.x.last().zip(self.y.last()) else {
            return value;
        };
        let mapped = if value <= first || self.x.len() == 1 {
            start + (value - first) * self.tails[0]
        } else if value >= last {
            end + (value - last) * self.tails[1]
        } else {
            let index = self.x.partition_point(|x| *x <= value) - 1;
            let (Some([low, high]), Some([start, end])) =
                (self.x.get(index..index + 2), self.y.get(index..index + 2))
            else {
                return value;
            };
            let fraction = (value - low) / (high - low);
            start + fraction * (end - start)
        };
        mapped.clamp(0.0, self.ceiling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_extreme_quantiles_have_no_colour_jump() {
        let map = QuantileMap::new(
            &[0.2, 0.2, 0.2, 0.5, 0.8, 0.8],
            &[0.1, 0.3, 0.5, 0.6, 0.7, 0.9],
            1.0,
            (0.25, 2.0),
        );
        for knot in [0.2, 0.5, 0.8] {
            assert!((map.apply(knot + 1e-6) - map.apply(knot - 1e-6)).abs() < 5e-6);
        }
        assert!((map.apply(0.2) - 0.3).abs() < 1e-6);
        assert!((map.apply(0.8) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn a_constant_source_translates_smoothly_instead_of_splitting_into_extremes() {
        let map = QuantileMap::new(
            &[0.5; 65],
            &(0..65).map(|i| 0.2 + i as f32 / 80.0).collect::<Vec<_>>(),
            1.2,
            (0.25, 2.0),
        );
        assert!((map.apply(0.5) - 0.6).abs() < 1e-6);
        assert!((map.apply(0.500_001) - map.apply(0.499_999)).abs() < 3e-6);
    }

    #[test]
    fn near_flat_colours_cannot_explode_between_measured_knots() {
        let map = QuantileMap::bounded(
            &[0.1, 0.4999, 0.5, 0.5001, 0.9],
            &[0.0, 0.1, 0.5, 0.9, 1.0],
            1.0,
            (0.25, 2.0),
        );
        let mut previous = map.apply(0.0);
        for i in 1..=10_000 {
            let next = map.apply(i as f32 / 10_000.0);
            assert!(next >= previous - 1e-6);
            assert!(next - previous <= 0.000_201);
            previous = next;
        }
    }

    #[test]
    fn ordinary_affine_grades_and_identity_remain_exact() {
        let source = [0.1, 0.3, 0.5, 0.7, 0.9];
        let target = source.map(|x| 0.8 * x + 0.1);
        let map = QuantileMap::new(&source, &target, 1.0, (0.25, 2.0));
        let identity = QuantileMap::new(&source, &source, 1.0, (0.25, 2.0));
        for i in 0..=1000 {
            let x = i as f32 / 1000.0;
            assert!((map.apply(x) - (0.8 * x + 0.1)).abs() < 1e-6);
            assert!((identity.apply(x) - x).abs() < 1e-6);
        }
        assert!((QuantileMap::new(&[], &[], 1.0, (0.25, 2.0)).apply(0.4) - 0.4).abs() < 1e-6);
    }
}
