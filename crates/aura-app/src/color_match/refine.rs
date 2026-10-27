//! Deterministic, bounded pattern search over rendered appearance.
//!
//! Multiplying a heuristic seed cannot discover a missing or oppositely signed
//! correction. Search additive steps in normalized parameter units instead. The
//! caller supplies real-render scoring, cancellation and native quality guards.
use aura_core::contract::colour::HslBand;
use aura_core::contract::style::{
    CurveShift, StyleDelta, MAX_CURVE_OFFSET, MAX_EXPOSURE_DELTA_EV, MAX_HSL_DELTA,
    MAX_PARAM_DELTA, MAX_TEMPERATURE_DELTA_K, MAX_TINT_DELTA, SKIN_BAND_FRACTION,
};
use aura_core::AuraResult;

pub(super) const MAX_EVALUATIONS: usize = 160;
const IMPROVEMENT: f32 = 0.0001;

#[derive(Clone, Copy, Debug)]
enum Axis {
    Scalar(usize),
    Curve(usize),
    Saturation(HslBand),
    Luminance(HslBand),
}

impl Axis {
    fn limit(self) -> f32 {
        match self {
            Self::Scalar(0) => MAX_EXPOSURE_DELTA_EV,
            Self::Scalar(1) => MAX_TEMPERATURE_DELTA_K,
            Self::Scalar(2) => MAX_TINT_DELTA,
            Self::Scalar(_) => MAX_PARAM_DELTA,
            Self::Curve(_) => MAX_CURVE_OFFSET,
            Self::Saturation(band) | Self::Luminance(band) => {
                MAX_HSL_DELTA
                    * if band.is_skin_band() {
                        SKIN_BAND_FRACTION
                    } else if band.is_skin_adjacent() {
                        (1.0 + SKIN_BAND_FRACTION) * 0.5
                    } else {
                        1.0
                    }
            }
        }
    }

    fn value(self, delta: &StyleDelta) -> f32 {
        match self {
            Self::Scalar(0) => delta.exposure,
            Self::Scalar(1) => delta.temperature_k,
            Self::Scalar(2) => delta.tint,
            Self::Scalar(3) => delta.contrast,
            Self::Scalar(4) => delta.highlights,
            Self::Scalar(5) => delta.shadows,
            Self::Scalar(6) => delta.whites,
            Self::Scalar(7) => delta.blacks,
            Self::Scalar(8) => delta.vibrance,
            Self::Scalar(_) => delta.saturation,
            Self::Curve(index) => delta
                .curve_shift
                .as_array()
                .get(index)
                .copied()
                .unwrap_or_default(),
            Self::Saturation(band) => delta.hsl.get(band).s,
            Self::Luminance(band) => delta.hsl.get(band).l,
        }
    }

    fn set(self, delta: &mut StyleDelta, value: f32) {
        let value = value.clamp(-self.limit(), self.limit());
        match self {
            Self::Scalar(0) => delta.exposure = value,
            Self::Scalar(1) => delta.temperature_k = value,
            Self::Scalar(2) => delta.tint = value,
            Self::Scalar(3) => delta.contrast = value,
            Self::Scalar(4) => delta.highlights = value,
            Self::Scalar(5) => delta.shadows = value,
            Self::Scalar(6) => delta.whites = value,
            Self::Scalar(7) => delta.blacks = value,
            Self::Scalar(8) => delta.vibrance = value,
            Self::Scalar(_) => delta.saturation = value,
            Self::Curve(index) => {
                let mut points = delta.curve_shift.as_array();
                if let Some(point) = points.get_mut(index) {
                    *point = if index == 4 { value.min(0.0) } else { value };
                }
                delta.curve_shift = CurveShift::from_array(points);
            }
            Self::Saturation(band) => {
                let mut shift = delta.hsl.get(band);
                shift.s = value;
                delta.hsl.set(band, shift);
            }
            Self::Luminance(band) => {
                let mut shift = delta.hsl.get(band);
                shift.l = value;
                delta.hsl.set(band, shift);
            }
        }
    }
}

pub(super) struct Outcome {
    pub delta: StyleDelta,
    pub distance: f32,
    pub evaluations: usize,
}

/// Edit effort in units of each parameter's limit, measured from the incumbent.
pub(super) fn effort(one: &StyleDelta, other: &StyleDelta) -> f32 {
    let mut axes: Vec<_> = (0..10)
        .map(Axis::Scalar)
        .chain((0..5).map(Axis::Curve))
        .collect();
    for band in HslBand::ALL {
        axes.extend([Axis::Saturation(band), Axis::Luminance(band)]);
    }
    axes.into_iter()
        .map(|axis| (axis.value(one) - axis.value(other)).abs() / axis.limit())
        .sum()
}

/// Never replaces the incoming answer with a non-finite or worse score. Errors,
/// including cancellation, propagate instead of publishing a partially fitted session.
pub(super) fn search(
    initial: &StyleDelta,
    initial_distance: f32,
    supported_bands: [bool; 8],
    mut evaluate: impl FnMut(&StyleDelta) -> AuraResult<f32>,
) -> AuraResult<Outcome> {
    let mut axes: Vec<_> = (0..10)
        .map(Axis::Scalar)
        .chain((0..5).map(Axis::Curve))
        .collect();
    for (band, supported) in HslBand::ALL.into_iter().zip(supported_bands) {
        if supported {
            axes.extend([Axis::Saturation(band), Axis::Luminance(band)]);
        }
    }
    let mut best = initial.clamped();
    let mut distance = initial_distance;
    let mut evaluated = 0;
    // Canonical recipe de-duplication is also done by the caller: several
    // continuous parameters round to the same saved integer or curve point.
    let mut seen = vec![best.clone()];
    let mut score = |candidate: &StyleDelta| -> AuraResult<Option<f32>> {
        if evaluated >= MAX_EVALUATIONS || seen.iter().any(|old| old == candidate) {
            return Ok(None);
        }
        seen.push(candidate.clone());
        evaluated += 1;
        let result = evaluate(candidate)?;
        Ok(result.is_finite().then_some(result))
    };
    for step in [0.5, 0.2, 0.08] {
        let origin = best.clone();
        for &axis in &axes {
            let center = best.clone();
            let mut direction = 0.0;
            for sign in [-1.0, 1.0] {
                let mut candidate = center.clone();
                axis.set(
                    &mut candidate,
                    axis.value(&center) + sign * step * axis.limit(),
                );
                if let Some(value) = score(&candidate)? {
                    if value + IMPROVEMENT < distance {
                        best = candidate;
                        distance = value;
                        direction = sign;
                    }
                }
            }
            if direction != 0.0 {
                let mut candidate = best.clone();
                axis.set(
                    &mut candidate,
                    axis.value(&best) + direction * step * axis.limit(),
                );
                if let Some(value) = score(&candidate)? {
                    if value + IMPROVEMENT < distance {
                        best = candidate;
                        distance = value;
                    }
                }
            }
        }
        // A joint pattern move follows the accepted direction through coupled
        // exposure/curve/cast valleys instead of optimizing each axis only once.
        let mut candidate = best.clone();
        for &axis in &axes {
            axis.set(
                &mut candidate,
                2.0 * axis.value(&best) - axis.value(&origin),
            );
        }
        if let Some(value) = score(&candidate)? {
            if value + IMPROVEMENT < distance {
                best = candidate;
                distance = value;
            }
        }
    }
    Ok(Outcome {
        delta: best,
        distance,
        evaluations: evaluated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_seed_can_discover_tone_cast_and_desaturation() {
        let loss = |d: &StyleDelta| {
            ((d.exposure - 0.4) / 0.67).powi(2)
                + ((d.temperature_k - 300.0) / 800.0).powi(2)
                + ((d.saturation + 15.0) / 35.0).powi(2)
        };
        let zero = StyleDelta::neutral();
        let result = search(&zero, loss(&zero), [false; 8], |d| Ok(loss(d))).unwrap();
        assert!(result.distance < 0.02, "{}", result.distance);
        assert!(
            result.delta.exposure > 0.0
                && result.delta.temperature_k > 0.0
                && result.delta.saturation < 0.0
        );
    }

    #[test]
    fn an_incorrect_seed_can_cross_zero_and_reverse_direction() {
        let mut seed = StyleDelta::neutral();
        seed.temperature_k = -400.0;
        let loss = |d: &StyleDelta| ((d.temperature_k - 400.0) / 800.0).powi(2);
        let result = search(&seed, loss(&seed), [false; 8], |d| Ok(loss(d))).unwrap();
        assert!((result.delta.temperature_k - 400.0).abs() < 1.0);
    }

    #[test]
    fn curve_anchors_and_supported_bands_are_fitted_independently() {
        let loss = |d: &StyleDelta| {
            ((d.curve_shift.mid - 12.0) / 24.0).powi(2)
                + ((d.hsl.get(HslBand::Blue).s - 10.0) / 20.0).powi(2)
        };
        let seed = StyleDelta::neutral();
        let mut supported = [false; 8];
        supported[HslBand::Blue as usize] = true;
        let result = search(&seed, loss(&seed), supported, |d| Ok(loss(d))).unwrap();
        assert!(result.distance < 0.001);
        assert_eq!(result.delta.hsl.get(HslBand::Orange).s, 0.0);
        assert_eq!(result.delta.curve_shift.high, 0.0);
    }

    #[test]
    fn non_finite_unsafe_candidates_do_not_replace_the_initial_answer() {
        let seed = StyleDelta::neutral();
        let result = search(&seed, 1.0, [true; 8], |d| {
            assert_eq!(*d, d.clamped());
            Ok(if d.temperature_k > 0.0 {
                f32::NAN
            } else {
                f32::INFINITY
            })
        })
        .unwrap();
        assert_eq!(result.delta, seed);
        assert_eq!(result.distance, 1.0);
        assert!(result.evaluations <= MAX_EVALUATIONS);
    }

    #[test]
    fn error_or_cancellation_aborts_without_a_successful_fit() {
        let result = search(&StyleDelta::neutral(), 1.0, [false; 8], |_| {
            Err(super::super::invalid("cancelled probe"))
        });
        assert!(result.is_err());
    }

    #[test]
    fn search_is_deterministic_bounded_and_never_worsens_the_seed() {
        let run = || {
            search(&StyleDelta::neutral(), 50.0, [true; 8], |d| {
                Ok(50.0 - d.exposure - d.temperature_k / 1000.0 - d.saturation / 100.0)
            })
            .unwrap()
        };
        let a = run();
        let b = run();
        assert_eq!(a.delta, b.delta);
        assert_eq!(a.distance, b.distance);
        assert_eq!(a.evaluations, b.evaluations);
        assert!(a.distance < 50.0 && a.evaluations <= MAX_EVALUATIONS);
        assert_eq!(a.delta, a.delta.clamped());
    }
}
