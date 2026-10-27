//! Sparse, regularised least squares for local RGB residuals. The global cubic remains the
//! prior in colours not observed in the reference; this lattice learns supported departures.
use aura_recipe::calibrated_look::{
    tetrahedral_stencil, ResidualLut, RESIDUAL_EDGE, RESIDUAL_NODES,
};

use super::{check_cancel, AuraResult, CalibratedLook, CancelToken, Pair, Partition};

type Rgb = [f64; 3];

fn channel<T: Copy + Default>(values: &[T; 3], index: usize) -> T {
    values.get(index).copied().unwrap_or_default()
}

fn quotient(numerator: Rgb, denominator: Rgb) -> Rgb {
    std::array::from_fn(|c| {
        let divisor = channel(&denominator, c);
        if divisor > 1e-24 {
            channel(&numerator, c) / divisor
        } else {
            0.0
        }
    })
}

struct Observation {
    stencil: [(usize, f64); 4],
    residual: Rgb,
    weight: f64,
}

fn at(values: &[Rgb], index: usize) -> Rgb {
    values.get(index).copied().unwrap_or([0.0; 3])
}

fn add(values: &mut [Rgb], index: usize, delta: Rgb, scale: f64) {
    if let Some(value) = values.get_mut(index) {
        for (out, d) in value.iter_mut().zip(delta) {
            *out += scale * d;
        }
    }
}

fn edges() -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    for i in 0..RESIDUAL_NODES {
        if i / (RESIDUAL_EDGE * RESIDUAL_EDGE) + 1 < RESIDUAL_EDGE {
            result.push((i, i + RESIDUAL_EDGE * RESIDUAL_EDGE));
        }
        if i / RESIDUAL_EDGE % RESIDUAL_EDGE + 1 < RESIDUAL_EDGE {
            result.push((i, i + RESIDUAL_EDGE));
        }
        if i % RESIDUAL_EDGE + 1 < RESIDUAL_EDGE {
            result.push((i, i + 1));
        }
    }
    result
}

struct System {
    observations: Vec<Observation>,
    edges: Vec<(usize, usize)>,
    ridge: f64,
    smooth: f64,
    diagonal: Vec<f64>,
    rhs: Vec<Rgb>,
}

impl System {
    fn new(observations: Vec<Observation>) -> Self {
        let edges = edges();
        let mass = observations.iter().map(|o| o.weight).sum::<f64>() / RESIDUAL_NODES as f64;
        // Authored smoothness/identity priors, selected without examining final validation.
        let (ridge, smooth) = (mass.max(1e-9) * 0.002, mass.max(1e-9) * 0.01);
        let mut diagonal = vec![ridge; RESIDUAL_NODES];
        let mut rhs = vec![[0.0; 3]; RESIDUAL_NODES];
        for &(a, b) in &edges {
            for i in [a, b] {
                if let Some(v) = diagonal.get_mut(i) {
                    *v += smooth;
                }
            }
        }
        for observation in &observations {
            for (i, weight) in observation.stencil {
                if let Some(v) = diagonal.get_mut(i) {
                    *v += observation.weight * weight * weight;
                }
                add(
                    &mut rhs,
                    i,
                    observation.residual,
                    observation.weight * weight,
                );
            }
        }
        Self {
            observations,
            edges,
            ridge,
            smooth,
            diagonal,
            rhs,
        }
    }

    fn product(&self, values: &[Rgb]) -> Vec<Rgb> {
        let mut result: Vec<_> = values.iter().map(|v| v.map(|c| c * self.ridge)).collect();
        for &(a, b) in &self.edges {
            let (va, vb) = (at(values, a), at(values, b));
            let delta = std::array::from_fn(|c| channel(&va, c) - channel(&vb, c));
            add(&mut result, a, delta, self.smooth);
            add(&mut result, b, delta, -self.smooth);
        }
        for observation in &self.observations {
            let mut predicted = [0.0; 3];
            for (i, weight) in observation.stencil {
                for (out, value) in predicted.iter_mut().zip(at(values, i)) {
                    *out += weight * value;
                }
            }
            for (i, weight) in observation.stencil {
                add(&mut result, i, predicted, observation.weight * weight);
            }
        }
        result
    }

    fn precondition(&self, residual: &[Rgb]) -> Vec<Rgb> {
        residual
            .iter()
            .zip(&self.diagonal)
            .map(|(r, &d)| r.map(|v| v / d))
            .collect()
    }
}

fn dot(a: &[Rgb], b: &[Rgb]) -> Rgb {
    a.iter().zip(b).fold([0.0; 3], |mut sum, (a, b)| {
        for (out, (x, y)) in sum.iter_mut().zip(a.iter().zip(b)) {
            *out += x * y;
        }
        sum
    })
}

/// Jacobi-preconditioned conjugate gradients, with three independent right-hand sides.
/// No dense matrix is built; sample stencils and neighbour differences define the operator.
fn solve(system: &System, cancel: &CancelToken) -> AuraResult<Vec<Rgb>> {
    let mut values = vec![[0.0; 3]; RESIDUAL_NODES];
    let mut residual = system.rhs.clone();
    let mut direction = system.precondition(&residual);
    let mut rho = dot(&residual, &direction);
    let initial = rho;
    for _ in 0..80 {
        check_cancel(cancel)?;
        let product = system.product(&direction);
        let denom = dot(&direction, &product);
        let step = quotient(rho, denom);
        for (((x, r), p), ap) in values
            .iter_mut()
            .zip(&mut residual)
            .zip(&direction)
            .zip(&product)
        {
            for (((x, r), (p, ap)), alpha) in x.iter_mut().zip(r).zip(p.iter().zip(ap)).zip(step) {
                *x += alpha * p;
                *r -= alpha * ap;
            }
        }
        let z = system.precondition(&residual);
        let next = dot(&residual, &z);
        if next
            .iter()
            .zip(initial)
            .all(|(&r, start)| r.abs() <= start.abs() * 1e-12 + 1e-20)
        {
            break;
        }
        let beta = quotient(next, rho);
        for (p, z) in direction.iter_mut().zip(z) {
            for ((p, z), beta) in p.iter_mut().zip(z).zip(beta) {
                *p = z + beta * *p;
            }
        }
        rho = next;
    }
    Ok(values)
}

pub(super) fn refine(
    rows: &[Pair],
    cells: &[usize; 64],
    base: &CalibratedLook,
    cancel: &CancelToken,
) -> AuraResult<Option<CalibratedLook>> {
    let mut model = base.clone();
    for _ in 0..2 {
        check_cancel(cancel)?;
        let mut observations = Vec::new();
        for (index, row) in rows
            .iter()
            .filter(|row| row.partition == Partition::Train)
            .enumerate()
        {
            if index.is_multiple_of(4096) {
                check_cancel(cancel)?;
            }
            let correction = base.correction(row.input);
            let predicted = model.encoded_srgb(row.input);
            let error = predicted
                .iter()
                .zip(row.output)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
                .sqrt();
            let robust = (0.035 / error.max(0.035)).max(0.02);
            observations.push(Observation {
                stencil: tetrahedral_stencil(row.input, RESIDUAL_EDGE)
                    .map(|(i, w)| (i, f64::from(w))),
                residual: std::array::from_fn(|c| {
                    f64::from(
                        channel(&row.output, c) - channel(&row.input, c) - channel(&correction, c),
                    )
                }),
                weight: f64::from(robust)
                    / (cells.get(row.cell).copied().unwrap_or(1).max(1) as f64).sqrt(),
            });
        }
        let values = solve(&System::new(observations), cancel)?;
        if values.iter().flatten().any(|v| !v.is_finite()) {
            return Ok(None);
        }
        model.version = 2;
        model.residual_lut = Some(ResidualLut {
            values: values
                .into_iter()
                .map(|v| v.map(|x| (x as f32).clamp(-0.35, 0.35)))
                .collect(),
        });
        if model.validate().is_err() {
            return Ok(None);
        }
    }
    Ok(Some(model))
}
