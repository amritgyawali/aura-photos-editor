//! Descriptive delivery checks. These measurements never assign a perfection score.
// Fixed-coordinate tensor and transform loops index several planes together.
#![allow(clippy::needless_range_loop)]
use super::{diagnostic, Diagnostic};
mod statistics;
mod structure;
#[cfg(test)]
mod tests;

pub(super) fn measure(p: &[[f32; 3]], y: &[f32], w: usize, h: usize) -> Vec<Diagnostic> {
    let mut output = statistics::measure(p, y, w, h);
    output.extend(structure::measure(p, y, w, h));
    output
}

fn bars(
    text: (&str, &str, &str),
    unit: &str,
    values: Vec<f32>,
    labels: &[&str],
    n: usize,
) -> Diagnostic {
    diagnostic(text, "bars", unit, (values.len(), 1), values, labels, n)
}
fn map(text: (&str, &str, &str), unit: &str, values: Vec<f32>, n: usize) -> Diagnostic {
    diagnostic(text, "map", unit, (8, 8), values, &[], n)
}
fn avg(sum: f64, n: usize) -> f32 {
    if n == 0 {
        0.0
    } else {
        (sum / n as f64) as f32
    }
}
fn quantile(sorted: &[f32], t: f32) -> f32 {
    let position = t * (sorted.len() - 1) as f32;
    let lo = position.floor() as usize;
    let hi = position.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (position - lo as f32)
}
fn entropy(hist: &[usize]) -> f32 {
    let total = hist.iter().sum::<usize>().max(1) as f64;
    hist.iter()
        .filter(|v| **v > 0)
        .map(|v| {
            let p = *v as f64 / total;
            -p * p.log2()
        })
        .sum::<f64>() as f32
}
