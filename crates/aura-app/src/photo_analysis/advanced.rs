//! Descriptive texture, colour stability and delivery checks, never perfection scores.
use super::{diagnostic, Diagnostic};
mod colour;
mod integrity;
#[cfg(test)]
mod tests;
mod texture;

pub(super) fn measure(p: &[[f32; 3]], light: &[f32], w: usize, h: usize) -> Vec<Diagnostic> {
    let y: Vec<f64> = light.iter().map(|v| f64::from(*v).asinh()).collect();
    let mut output = texture::measure(&y, w, h);
    output.extend(colour::measure(p, &y, w, h));
    output.extend(integrity::measure(p, &y, w, h));
    output
}

fn bars(
    text: (&str, &str, &str),
    unit: &str,
    values: Vec<f64>,
    labels: &[&str],
    n: usize,
) -> Diagnostic {
    diagnostic(
        text,
        "bars",
        unit,
        (values.len(), 1),
        values.into_iter().map(|v| v as f32).collect(),
        labels,
        n,
    )
}
fn map(text: (&str, &str, &str), unit: &str, values: Vec<f64>, n: usize) -> Diagnostic {
    diagnostic(
        text,
        "map",
        unit,
        (8, 8),
        values.into_iter().map(|v| v as f32).collect(),
        &[],
        n,
    )
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}
fn quantile(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut sorted = v.to_vec();
    sorted.sort_by(f64::total_cmp);
    let pos = q.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}
fn entropy(hist: &[usize]) -> f64 {
    let n = hist.iter().sum::<usize>().max(1) as f64;
    hist.iter()
        .filter(|v| **v > 0)
        .map(|v| {
            let p = *v as f64 / n;
            -p * p.ln()
        })
        .sum()
}
fn bins(y: &[f64], levels: usize) -> Vec<usize> {
    let lo = y.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    y.iter()
        .map(|v| {
            (((v - lo) / (hi - lo).max(1e-12) * levels as f64).floor() as usize).min(levels - 1)
        })
        .collect()
}
fn cell(i: usize, w: usize, h: usize) -> usize {
    i / w * 8 / h * 8 + i % w * 8 / w
}
fn neighbours(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    [
        if i % w + 1 < w { Some(i + 1) } else { None },
        if i / w + 1 < h { Some(i + w) } else { None },
    ]
    .into_iter()
    .flatten()
}
