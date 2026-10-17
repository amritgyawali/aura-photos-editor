//! Original-light support for a smooth quadratic surface, never a shadow step.
#![allow(clippy::indexing_slicing, clippy::too_many_arguments)]
#[derive(Clone)]
pub(super) struct Reference {
    pub light: f32,
    pub curvature: f32,
    pub samples: Vec<[f32; 2]>,
}

fn solve(mut a: [[f32; 7]; 6]) -> Option<[f32; 6]> {
    for col in 0..6 {
        let pivot = (col..6).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        a.swap(col, pivot);
        let divisor = a[col][col];
        if divisor.abs() < 1e-6 {
            return None;
        }
        for value in a[col].iter_mut().skip(col) {
            *value /= divisor;
        }
        let pivot_row = a[col];
        for (row, values) in a.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let factor = values[col];
            for (value, pivot_value) in values.iter_mut().zip(pivot_row).skip(col) {
                *value -= factor * pivot_value;
            }
        }
    }
    Some(std::array::from_fn(|i| a[i][6]))
}

pub(super) fn reference(
    luma: &[f32],
    redness: &[f32],
    score: &[f32],
    mask: &[bool],
    w: usize,
    h: usize,
    centre: [f32; 2],
    radii: [f32; 2],
    threshold: f32,
) -> Option<Reference> {
    let mut samples = Vec::new();
    let mut quadrants = [0_usize; 4];
    for radius in [1.05, 1.2, 1.35] {
        let mut support = 0;
        for n in 0..32 {
            let angle = std::f32::consts::TAU * n as f32 / 32.0;
            let uv = [angle.cos() * radius, angle.sin() * radius];
            let x = (centre[0] + uv[0] * radii[0]).round();
            let y = (centre[1] + uv[1] * radii[1]).round();
            if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                return None;
            }
            let at = y as usize * w + x as usize;
            if !mask[at] {
                return None;
            }
            if redness[at] > 0.012 && score[at] > threshold {
                continue;
            }
            let [x, y] = [(x - centre[0]) / radii[0], (y - centre[1]) / radii[1]];
            samples.push(([1.0, x, y, x * x, x * y, y * y], luma[at]));
            support += 1;
            quadrants[n / 8] += 1;
        }
        if support < 16 {
            return None;
        }
    }
    if samples.len() < 58 || quadrants.iter().any(|count| *count < 12) {
        return None;
    }
    let at = centre[1].round() as usize * w + centre[0].round() as usize;
    fit(&samples, luma[at], Vec::new())
}

/// Search beyond a contaminated rim without adding any pixels to the repair.
/// Persist the actual healthy, selected sample positions so export fits the
/// same support, rather than reading an unfiltered circular ring near a nostril.
pub(super) fn contextual_reference(
    luma: &[f32],
    redness: &[f32],
    score: &[f32],
    mask: &[bool],
    w: usize,
    h: usize,
    centre: [f32; 2],
    radii: [f32; 2],
    threshold: f32,
) -> Option<Reference> {
    let mut support = Vec::new();
    let mut positions = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut quadrants = [0_usize; 4];
    for radius in [1.05, 1.2, 1.35, 1.65, 2.0, 2.5, 3.0] {
        for n in 0..32 {
            let angle = std::f32::consts::TAU * n as f32 / 32.0;
            let x = (centre[0] + angle.cos() * radius * radii[0]).round();
            let y = (centre[1] + angle.sin() * radius * radii[1]).round();
            if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                continue;
            }
            let at = y as usize * w + x as usize;
            if !mask[at] || (redness[at] > 0.012 && score[at] > threshold) || !seen.insert(at) {
                continue;
            }
            let uv = [(x - centre[0]) / radii[0], (y - centre[1]) / radii[1]];
            if !(1.0..=3.1).contains(&uv[0].hypot(uv[1])) {
                continue;
            }
            support.push((
                [
                    1.0,
                    uv[0],
                    uv[1],
                    uv[0] * uv[0],
                    uv[0] * uv[1],
                    uv[1] * uv[1],
                ],
                luma[at],
            ));
            positions.push(uv);
            quadrants[n / 8] += 1;
        }
        if support.len() >= 58 && quadrants.iter().all(|count| *count >= 12) {
            let at = centre[1].round() as usize * w + centre[0].round() as usize;
            if let Some(reference) = fit(&support, luma[at], positions.clone()) {
                return Some(reference);
            }
        }
    }
    if support.len() < 58 || quadrants.iter().any(|count| *count < 12) {
        return None;
    }
    let at = centre[1].round() as usize * w + centre[0].round() as usize;
    #[cfg(test)]
    if std::env::var_os("AURA_SPOT_TRACE").is_some() {
        println!(
            "context-trace centre={centre:?} support={} quadrants={quadrants:?}",
            support.len()
        );
    }
    fit(&support, luma[at], positions)
}

fn fit(samples: &[([f32; 6], f32)], centre: f32, positions: Vec<[f32; 2]>) -> Option<Reference> {
    let mut weights = vec![1.0_f32; samples.len()];
    let mut model = [0.0; 6];
    for _ in 0..5 {
        let mut system = [[0.0; 7]; 6];
        for ((b, light), weight) in samples.iter().zip(&weights) {
            for row in 0..6 {
                for col in 0..6 {
                    system[row][col] += weight * b[row] * b[col];
                }
                system[row][6] += weight * b[row] * light;
            }
        }
        model = solve(system)?;
        let residuals: Vec<_> = samples
            .iter()
            .map(|(b, light)| (light - (0..6).map(|j| model[j] * b[j]).sum::<f32>()).abs())
            .collect();
        let mut sorted = residuals.clone();
        sorted.sort_by(f32::total_cmp);
        let cutoff = (sorted[sorted.len() / 2] * 2.5).max(0.001);
        for (weight, residual) in weights.iter_mut().zip(residuals) {
            *weight = (cutoff / residual.max(cutoff)).powi(2);
        }
    }
    let mut residuals: Vec<_> = samples
        .iter()
        .map(|(b, light)| (light - (0..6).map(|j| model[j] * b[j]).sum::<f32>()).abs())
        .collect();
    residuals.sort_by(f32::total_cmp);
    let curvature = model[3].abs() + model[4].abs() + model[5].abs();
    #[cfg(test)]
    if !positions.is_empty() && std::env::var_os("AURA_SPOT_TRACE").is_some() {
        println!(
            "context-fit model={model:?} residual90={} center={centre}",
            residuals[residuals.len() * 9 / 10]
        );
    }
    if model[0] < 0.02
        || model[0] < centre * 0.8
        || model[1].hypot(model[2]) + curvature > model[0]
        || curvature > model[0] * 0.35
        || residuals[residuals.len() * 9 / 10] > model[0] * 0.04
    {
        return None;
    }
    Some(Reference {
        light: model[0],
        curvature: curvature / model[0],
        samples: positions,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    #[test]
    fn wider_selected_support_recovers_a_contaminated_quadrant_but_rejects_shadow_steps() {
        let (w, h) = (160, 160);
        let mut light = vec![0.4; w * h];
        let mut redness = vec![0.0; w * h];
        let mut score = vec![0.0; w * h];
        for y in 64..=79 {
            for x in 64..=79 {
                let at = y * w + x;
                light[at] = 0.2;
                redness[at] = 0.08;
                score[at] = 0.1;
            }
        }
        let mask = vec![true; w * h];
        assert!(
            reference(&light, &redness, &score, &mask, w, h, [80.0; 2], [12.0; 2], 0.03).is_none()
        );
        let recovered = contextual_reference(
            &light, &redness, &score, &mask, w, h, [80.0; 2], [12.0; 2], 0.03,
        )
        .unwrap();
        assert!((recovered.light - 0.4).abs() < 0.001);
        assert!(recovered.samples.iter().any(|p| p[0].hypot(p[1]) > 1.5));
        for p in &recovered.samples {
            let at =
                (80.0 + p[1] * 12.0).round() as usize * w + (80.0 + p[0] * 12.0).round() as usize;
            assert!(mask[at] && redness[at] < 0.012);
        }
        let shadow: Vec<_> = (0..w * h)
            .map(|i| if i % w > 80 { 0.6 } else { 0.2 })
            .collect();
        assert!(contextual_reference(
            &shadow,
            &vec![0.0; w * h],
            &score,
            &mask,
            w,
            h,
            [80.0; 2],
            [12.0; 2],
            0.03
        )
        .is_none());
        assert!(contextual_reference(
            &light,
            &redness,
            &score,
            &vec![false; w * h],
            w,
            h,
            [80.0; 2],
            [12.0; 2],
            0.03
        )
        .is_none());
    }
    #[test]
    fn smooth_curved_skin_is_measured_but_shadow_steps_and_mask_holes_are_rejected() {
        let (w, h) = (160, 160);
        let mut light = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                light[y * w + x] = 0.4
                    + 0.06 * ((x as f32 - 80.0) / 12.0).powi(2)
                    + 0.015 * ((y as f32 - 80.0) / 12.0).powi(2);
            }
        }
        let reference = reference(
            &light,
            &vec![0.0; w * h],
            &vec![0.0; w * h],
            &vec![true; w * h],
            w,
            h,
            [80.0, 80.0],
            [12.0; 2],
            0.03,
        )
        .unwrap();
        assert!((reference.light - 0.4).abs() < 0.002);
        assert!(reference.curvature > 0.15);
        let step: Vec<_> = (0..w * h)
            .map(|i| if i % w > 80 { 0.6 } else { 0.2 })
            .collect();
        assert!(super::reference(
            &step,
            &vec![0.0; w * h],
            &vec![0.0; w * h],
            &vec![true; w * h],
            w,
            h,
            [80.0, 80.0],
            [12.0; 2],
            0.03
        )
        .is_none());
        assert!(super::reference(
            &light,
            &vec![0.0; w * h],
            &vec![0.0; w * h],
            &vec![false; w * h],
            w,
            h,
            [80.0, 80.0],
            [12.0; 2],
            0.03
        )
        .is_none());
    }
}
