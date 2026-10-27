//! Independent deterministic inspection tools on the bounded working-space proxy.
//! These are descriptive statistics, never learned defect or identity decisions.
use super::{diagnostic, Diagnostic};
mod composition;
mod detail;
mod palette;

pub(super) fn measure(pixels: &[[f32; 3]], luma: &[f32], w: usize, h: usize) -> Vec<Diagnostic> {
    let mut tools = palette::measure(pixels, luma);
    tools.extend(detail::measure(luma, w, h));
    tools.extend(composition::measure(luma, w, h));
    tools
}

fn bars(
    id: &str,
    title: &str,
    description: &str,
    unit: &str,
    values: Vec<f32>,
    labels: &[&str],
    samples: usize,
) -> Diagnostic {
    diagnostic(
        (id, title, description),
        "bars",
        unit,
        (values.len(), 1),
        values,
        labels,
        samples,
    )
}

fn map(
    id: &str,
    title: &str,
    description: &str,
    unit: &str,
    values: Vec<f32>,
    samples: usize,
) -> Diagnostic {
    diagnostic(
        (id, title, description),
        "map",
        unit,
        (8, 8),
        values,
        &[],
        samples,
    )
}

fn avg(sum: f64, count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        (sum / count as f64) as f32
    }
}

fn bin(value: f32, bins: usize) -> usize {
    (value.clamp(0.0, 1.0) * (bins - 1) as f32).round() as usize
}

#[cfg(test)]
mod tests;
