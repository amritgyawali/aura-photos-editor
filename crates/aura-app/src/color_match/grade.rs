//! Tone-dependent colour casts, fitted through the native three-way grade wheels.
use aura_core::AuraResult;
use aura_recipe::contract::recipe::{ColourGrade, GradeWheel};

pub(super) const MAX_CAST: f32 = 30.0;
pub(super) const MAX_EVALUATIONS: usize = 36;

/// Cartesian wheel offsets avoid the discontinuity at hue 0/359 and give a
/// neutral wheel a direction to explore. Existing luminance, global grade,
/// blending and balance remain with this photograph.
pub(super) fn apply(grade: &mut ColourGrade, offsets: &[f32; 6], strength: f32) {
    for (wheel, offset) in [
        &mut grade.shadows,
        &mut grade.midtones,
        &mut grade.highlights,
    ]
    .into_iter()
    .zip(offsets.chunks_exact(2))
    {
        let &[x, y] = offset else { continue };
        let x = x * strength;
        let y = y * strength;
        if x == 0.0 && y == 0.0 {
            continue;
        }
        let radius = x.hypot(y);
        let factor = (MAX_CAST / radius.max(MAX_CAST)).min(1.0);
        let angle = f32::from(wheel.hue).to_radians();
        let old_radius = f32::from(wheel.saturation);
        let x = angle.cos() * old_radius + x * factor;
        let y = angle.sin() * old_radius + y * factor;
        *wheel = GradeWheel {
            hue: (y.atan2(x).to_degrees().rem_euclid(360.0).round() as i16).rem_euclid(360),
            saturation: x.hypot(y).round().clamp(0.0, 100.0) as i16,
            luminance: wheel.luminance,
        };
    }
}

pub(super) fn search(
    initial_distance: f32,
    mut evaluate: impl FnMut(&[f32; 6]) -> AuraResult<f32>,
) -> AuraResult<([f32; 6], f32, usize)> {
    let mut best = [0.0_f32; 6];
    let mut distance = initial_distance;
    let mut evaluations = 0;
    for step in [12.0, 6.0, 3.0] {
        for axis in 0..6 {
            let center = best;
            for sign in [-1.0, 1.0] {
                let mut candidate = center;
                if let (Some(value), Some(&middle)) = (candidate.get_mut(axis), center.get(axis)) {
                    *value = (middle + sign * step).clamp(-MAX_CAST, MAX_CAST);
                }
                let value = evaluate(&candidate)?;
                evaluations += 1;
                if value.is_finite() && value + 0.0001 < distance {
                    best = candidate;
                    distance = value;
                }
            }
        }
    }
    debug_assert!(evaluations <= MAX_EVALUATIONS);
    Ok((best, distance, evaluations))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_strength_preserves_all_existing_wheels_and_luminance() {
        let grade = ColourGrade {
            shadows: GradeWheel {
                hue: 359,
                saturation: 17,
                luminance: -7,
            },
            balance: 23,
            blending: 65,
            ..ColourGrade::default()
        };
        let mut result = grade;
        apply(&mut result, &[12.0, -6.0, 0.0, 0.0, 9.0, 3.0], 0.0);
        assert_eq!(result, grade);
        apply(&mut result, &[12.0, -6.0, 0.0, 0.0, 9.0, 3.0], 1.0);
        assert_eq!(result.shadows.luminance, -7);
        assert_eq!(result.midtones, grade.midtones);
        assert_eq!(result.global, grade.global);
        assert_eq!((result.balance, result.blending), (23, 65));
    }

    #[test]
    fn neutral_wheels_can_acquire_a_cast_and_stronger_requests_remain_bounded() {
        let mut grade = ColourGrade::default();
        apply(&mut grade, &[-12.0, 0.0, 0.0, 0.0, 0.0, 12.0], 1.0);
        assert_eq!((grade.shadows.hue, grade.shadows.saturation), (180, 12));
        assert_eq!(
            (grade.highlights.hue, grade.highlights.saturation),
            (90, 12)
        );
        apply(&mut grade, &[30.0; 6], 16.0);
        assert!(grade.shadows.saturation <= 42 && grade.highlights.saturation <= 42);
    }

    #[test]
    fn shadow_and_highlight_casts_fit_independently_without_accepting_nan() {
        let loss =
            |v: &[f32; 6]| (v[0] + 12.0).powi(2) + (v[1] + 6.0).powi(2) + (v[4] - 9.0).powi(2);
        let (offsets, distance, evaluations) = search(loss(&[0.0; 6]), |v| {
            Ok(if v[2] == 0.0 { loss(v) } else { f32::NAN })
        })
        .unwrap();
        assert_eq!(offsets, [-12.0, -6.0, 0.0, 0.0, 9.0, 0.0]);
        assert_eq!(distance, 0.0);
        assert_eq!(evaluations, MAX_EVALUATIONS);
    }
}
