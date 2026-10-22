//! Selection-normalized correction transfer from the bounded grid.
pub(super) fn sample(
    correction: &[[f32; 3]],
    weights: &[f32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
) -> [f32; 3] {
    let (x, y) = (
        x.clamp(0.0, (width - 1) as f32),
        y.clamp(0.0, (height - 1) as f32),
    );
    let (left, top) = (x.floor() as usize, y.floor() as usize);
    let (fx, fy) = (x - left as f32, y - top as f32);
    let mut sum = [0.0; 3];
    let mut total = 0.0;
    for (dx, dy, interpolation) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let j = (top + dy).min(height - 1) * width + (left + dx).min(width - 1);
        let weight = weights[j] * interpolation;
        if weight <= 0.0 {
            continue;
        }
        for (c, value) in sum.iter_mut().enumerate() {
            *value += correction[j][c] * weight;
        }
        total += weight;
    }
    if total > 1e-8 {
        sum.map(|v| v / total)
    } else {
        [0.0; 3]
    }
}
