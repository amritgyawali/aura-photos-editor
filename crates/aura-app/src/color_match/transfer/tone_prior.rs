//! The tone of a look, estimated from one reference of another scene (ADR-0180).
//!
//! A reference's lightness distribution is its scene's distribution passed through the edit's
//! tone curve. One photograph cannot separate the two - a dark reference may be a night scene or
//! a moody edit - but ordinary photographs are not arbitrary: almost every one has something near
//! black and something near white, and their midtones fall in a known spread. So the question is
//! answered in proportion to the evidence, by Bayes' rule over tone curves:
//!
//! * the *scene prior* is a Gaussian over nine lightness landmarks (0.2 to 99.8 per cent) of
//!   ordinary finished photographs;
//! * the *edit prior* gives a fixed probability to "no tone edit" and spreads the rest as a
//!   Gaussian over a five-node monotone curve;
//! * each curve is weighted by how ordinary the photograph it implies would be - the reference's
//!   landmarks mapped back through it - and the posterior mean curve is the estimate.
//!
//! A matte reference, whose blacks sit far above ordinary photographs', yields a black lift; a
//! reference whose landmarks are ordinary yields no tone change; a dark reference yields part
//! scene, part edit, in the proportion the two priors give. The posterior mean has the least
//! expected squared error under those priors - a statement about the priors, not a recovery of
//! the unknown edit. The same estimate of the photograph being matched says how far *it* sits
//! from an ordinary finished photograph, and [`relative`] removes that before adding the look.
use aura_raw::colour::curve::{monotone_tangents, pchip};

/// Indices into the 65 lightness quantiles: 0.2, 5, 15, 30, 50, 70, 85, 95 and 99.8 per cent.
pub(super) const LANDMARKS: [usize; 9] = [0, 3, 9, 19, 32, 45, 55, 61, 64];
const Q: usize = LANDMARKS.len();
/// Curve nodes, evenly spaced over input lightness.
const NODES: usize = 5;
/// Candidate curves; a fixed low-discrepancy set, so an estimate repeats exactly.
const CANDIDATES: usize = 6144;
/// No curve segment may be flatter than this, so a candidate cannot posterise or divide by zero.
const MIN_SLOPE: f32 = 0.15;
/// One sampling coordinate per curve node, from the first five primes.
const PRIMES: [usize; NODES] = [2, 3, 5, 7, 11];

type Matrix = [[f64; Q]; Q];

/// What ordinary photographs' lightness landmarks look like.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::color_match) struct ScenePrior {
    mean: [f64; Q],
    precision: Matrix,
}

fn landmarks(lightness: &[f32]) -> [f64; Q] {
    LANDMARKS.map(|i| {
        f64::from(
            lightness
                .get(i)
                .or_else(|| lightness.last())
                .copied()
                .unwrap_or(0.5),
        )
    })
}

/// Invert a symmetric positive-definite matrix by Gauss-Jordan elimination with pivoting.
// Every index is a loop variable below `Q` into a `[[f64; Q]; Q]`, so none can be out of range.
#[allow(clippy::indexing_slicing)]
fn invert(mut a: Matrix) -> Option<Matrix> {
    let mut inverse = [[0.0_f64; Q]; Q];
    for (i, row) in inverse.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for col in 0..Q {
        let pivot = (col..Q).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(pivot, col);
        inverse.swap(pivot, col);
        let scale = a[col][col];
        for k in 0..Q {
            a[col][k] /= scale;
            inverse[col][k] /= scale;
        }
        for row in (0..Q).filter(|&r| r != col) {
            let factor = a[row][col];
            if factor.abs() > 0.0 {
                for k in 0..Q {
                    a[row][k] -= factor * a[col][k];
                    inverse[row][k] -= factor * inverse[col][k];
                }
            }
        }
    }
    Some(inverse)
}

impl ScenePrior {
    /// The prior from measured scenes, each given as its 65 lightness quantiles.
    #[cfg(test)]
    pub(in crate::color_match) fn from_scenes<'a>(
        scenes: impl IntoIterator<Item = &'a [f32]>,
    ) -> Option<Self> {
        Self::from_landmarks(scenes.into_iter().map(landmarks))
    }

    /// The prior from scenes reduced to their nine landmarks. The covariance is shrunk toward
    /// its diagonal and floored, because a few dozen photographs of nine strongly correlated
    /// numbers make a Gaussian more certain than photographs are. `None` below four scenes.
    fn from_landmarks(scenes: impl IntoIterator<Item = [f64; Q]>) -> Option<Self> {
        let points: Vec<[f64; Q]> = scenes.into_iter().collect();
        if points.len() < 4 {
            return None;
        }
        let n = points.len() as f64;
        let mut mean = [0.0_f64; Q];
        for point in &points {
            for (m, v) in mean.iter_mut().zip(point) {
                *m += v / n;
            }
        }
        let mut covariance = [[0.0_f64; Q]; Q];
        for point in &points {
            for ((row, vi), mi) in covariance.iter_mut().zip(point).zip(&mean) {
                for ((cell, vj), mj) in row.iter_mut().zip(point).zip(&mean) {
                    *cell += (vi - mi) * (vj - mj) / (n - 1.0);
                }
            }
        }
        for (i, row) in covariance.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                if i == j {
                    // No landmark is ever known better than a few hundredths of lightness.
                    *cell = cell.max(0.03_f64.powi(2));
                } else {
                    *cell *= 0.75;
                }
            }
        }
        Some(Self {
            mean,
            precision: invert(covariance)?,
        })
    }

    fn mahalanobis(&self, x: &[f64; Q]) -> f64 {
        let delta: [f64; Q] = std::array::from_fn(|i| {
            x.get(i).copied().unwrap_or(0.0) - self.mean.get(i).copied().unwrap_or(0.0)
        });
        self.precision
            .iter()
            .zip(&delta)
            .map(|(row, di)| di * row.iter().zip(&delta).map(|(p, dj)| p * dj).sum::<f64>())
            .sum()
    }
}

/// The radical inverse of `index` in `base`: one coordinate of a Halton point.
fn halton(mut index: usize, base: usize) -> f64 {
    let (mut fraction, mut result) = (1.0_f64, 0.0_f64);
    while index > 0 {
        fraction /= base as f64;
        result += fraction * (index % base) as f64;
        index /= base;
    }
    result
}

/// Evaluate a polynomial with the highest coefficient first.
fn horner(coefficients: &[f64], x: f64) -> f64 {
    coefficients.iter().fold(0.0, |acc, c| acc * x + c)
}

/// The standard normal quantile (Acklam's rational approximation; ample for sampling).
fn probit(p: f64) -> f64 {
    const A: [f64; 6] = [
        -39.696_830_286_653_76,
        220.946_098_424_520_5,
        -275.928_510_446_968_7,
        138.357_751_867_269,
        -30.664_798_066_147_16,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -54.476_098_798_224_06,
        161.585_836_858_040_9,
        -155.698_979_859_886_6,
        66.801_311_887_719_72,
        -13.280_681_552_885_72,
    ];
    const C: [f64; 6] = [
        -0.007_784_894_002_430_293,
        -0.322_396_458_041_136_4,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        0.007_784_695_709_041_462,
        0.322_467_129_070_039_8,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let p = p.clamp(1e-6, 1.0 - 1e-6);
    let tail = |q: f64| horner(&C, q) / (horner(&D, q) * q + 1.0);
    if p < 0.024_25 {
        tail((-2.0 * p.ln()).sqrt())
    } else if p > 1.0 - 0.024_25 {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    } else {
        let q = p - 0.5;
        let r = q * q;
        horner(&A, r) * q / (horner(&B, r) * r + 1.0)
    }
}

struct Curve {
    xs: [f32; NODES],
    ys: [f32; NODES],
    tangents: Vec<f32>,
}

impl Curve {
    fn new(ys: [f32; NODES]) -> Self {
        let xs = std::array::from_fn(|i| i as f32 / (NODES - 1) as f32);
        let tangents = monotone_tangents(&xs, &ys);
        Self { xs, ys, tangents }
    }
    fn at(&self, x: f32) -> f32 {
        pchip(&self.xs, &self.ys, &self.tangents, x)
    }
    /// The input that maps to `y`.
    fn inverse(&self, y: f32) -> f32 {
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..22 {
            let mid = 0.5 * (lo + hi);
            if self.at(mid) < y {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }
}

/// How a tone estimate is made.
#[derive(Debug, Clone, Copy)]
pub(in crate::color_match) struct TonePrior {
    /// Standard deviation of each curve node's departure from no edit, when there is an edit.
    pub edit_sigma: f32,
    /// Prior probability that there is no tone edit at all.
    pub no_edit: f32,
    /// How much the scene evidence counts, `0..=1`; below one, the scene prior is widened.
    pub evidence: f32,
}

/// The look's tone: a strong expectation of no edit, a moderate spread when there is one, and the
/// scene evidence widened to half weight. Chosen on the looks harness (ADR-0180).
pub(in crate::color_match) const LOOK: TonePrior = TonePrior {
    edit_sigma: 0.1,
    no_edit: 0.8,
    evidence: 0.5,
};

/// The photograph's own departure is judged with the same caution as the look's. Believing it
/// more readily finishes flat renders better and makes softly toned finished photographs worse,
/// and the second cost was the larger one on the looks harness (ADR-0180).
pub(in crate::color_match) const OWN: TonePrior = LOOK;

/// The largest tone change, in `OKLab` lightness, that is treated as no change.
const DEAD_ZONE: f32 = 0.01;

/// The curve a set of node offsets describes, or `None` when a segment is too flat.
fn curve_of(offsets: &[f64; NODES]) -> Option<[f32; NODES]> {
    let ys: [f32; NODES] = std::array::from_fn(|k| {
        let x = k as f32 / (NODES - 1) as f32;
        (x + offsets.get(k).copied().unwrap_or(0.0) as f32).clamp(0.0, 1.0)
    });
    let spacing = 1.0 / (NODES - 1) as f32;
    let too_flat = ys
        .iter()
        .zip(ys.iter().skip(1))
        .any(|(a, b)| b - a < MIN_SLOPE * spacing);
    (!too_flat).then_some(ys)
}

/// The edit's posterior, given that there is an edit (the "slab").
struct Slab {
    /// Log of the mean likelihood over the edit prior: the slab's evidence, weighed against no
    /// edit.
    log_evidence: f64,
    /// The posterior mean curve within the slab.
    curve: [f64; NODES],
}

/// Weigh a fixed low-discrepancy set of curves drawn from the edit prior by their likelihood.
///
/// A two-round adaptive sampler, which estimated this posterior more precisely when the
/// evidence was strong, measured worse on the looks harness (3.81 against 3.68): the priors are
/// approximations, and the coarser estimate keeps closer to no change. ADR-0180.
fn slab(sigma: f64, log_likelihood: &impl Fn([f32; NODES]) -> f64) -> Option<Slab> {
    let drawn: Vec<(f64, [f32; NODES])> = (1..=CANDIDATES)
        .filter_map(|index| {
            let offsets = PRIMES.map(|base| sigma * probit(halton(index, base)));
            curve_of(&offsets).map(|ys| (log_likelihood(ys), ys))
        })
        .collect();
    let top = drawn.iter().map(|d| d.0).fold(f64::NEG_INFINITY, f64::max);
    if !top.is_finite() {
        return None;
    }
    let (mut total, mut curve) = (0.0_f64, [0.0_f64; NODES]);
    for (log_weight, ys) in &drawn {
        let weight = (log_weight - top).exp();
        total += weight;
        for (c, y) in curve.iter_mut().zip(ys) {
            *c += weight * f64::from(*y);
        }
    }
    (total > 0.0).then(|| Slab {
        log_evidence: top + (total / drawn.len().max(1) as f64).ln(),
        curve: curve.map(|c| c / total),
    })
}

/// The posterior-mean tone curve, as `nodes` evenly spaced values over input lightness: "no
/// edit" and "some edit" weighed by prior mass times evidence, and averaged.
pub(in crate::color_match) fn estimate(
    lightness: &[f32],
    scenes: &ScenePrior,
    prior: TonePrior,
    nodes: usize,
) -> Vec<f32> {
    let observed = landmarks(lightness);
    let log_likelihood = |ys: [f32; NODES]| -> f64 {
        let curve = Curve::new(ys);
        let implied = observed.map(|y| f64::from(curve.inverse(y as f32)));
        -0.5 * f64::from(prior.evidence) * scenes.mahalanobis(&implied)
    };
    let identity: [f32; NODES] = std::array::from_fn(|k| k as f32 / (NODES - 1) as f32);
    let no_edit = f64::from(prior.no_edit.clamp(0.0, 0.999));
    let ys = match slab(f64::from(prior.edit_sigma.max(1e-3)), &log_likelihood) {
        None => identity,
        Some(slab) => {
            let rest = (1.0 - no_edit).ln() + slab.log_evidence;
            let spike = if no_edit > 0.0 {
                no_edit.ln() + log_likelihood(identity)
            } else {
                f64::NEG_INFINITY
            };
            let top = spike.max(rest);
            let (keep, edit) = ((spike - top).exp(), (rest - top).exp());
            let mut ys = identity;
            for (y, s) in ys.iter_mut().zip(&slab.curve) {
                *y = ((keep * f64::from(*y) + edit * s) / (keep + edit)) as f32;
            }
            ys
        }
    };
    let curve = Curve::new(ys);
    (0..nodes)
        .map(|i| curve.at(i as f32 / (nodes - 1).max(1) as f32))
        .collect()
}

/// Lightness landmarks of ordinary finished photographs, measured by `print_scene_landmarks`
/// from 37 Pexels photographs of varied subjects and light (licence: free to use and modify).
/// Only these numbers are kept; no photograph is shipped.
#[rustfmt::skip]
const ORDINARY: [[f64; Q]; 37] = [
    [0.113, 0.136, 0.152, 0.190, 0.257, 0.353, 0.479, 0.658, 0.940], // p11008002
    [0.195, 0.215, 0.239, 0.291, 0.404, 0.581, 0.793, 0.873, 0.937], // p12779583
    [0.067, 0.279, 0.429, 0.590, 0.719, 0.899, 0.953, 0.979, 0.996], // p13822354
    [0.082, 0.127, 0.166, 0.207, 0.278, 0.312, 0.426, 0.654, 0.998], // p15069828
    [0.038, 0.197, 0.387, 0.614, 0.828, 0.901, 0.957, 0.981, 0.994], // p1652312
    [0.179, 0.300, 0.355, 0.449, 0.524, 0.582, 0.643, 0.708, 0.971], // p16646551
    [0.000, 0.067, 0.067, 0.067, 0.129, 0.364, 0.607, 0.776, 0.901], // p17000433
    [0.067, 0.142, 0.196, 0.262, 0.376, 0.492, 0.594, 0.697, 0.882], // p18688314
    [0.000, 0.140, 0.189, 0.256, 0.354, 0.462, 0.617, 0.808, 0.957], // p20699589
    [0.178, 0.187, 0.191, 0.201, 0.246, 0.421, 0.501, 0.602, 0.721], // p20756240
    [0.197, 0.223, 0.264, 0.298, 0.347, 0.464, 0.605, 0.757, 0.909], // p20788937
    [0.090, 0.183, 0.331, 0.609, 0.756, 0.852, 0.906, 0.938, 0.981], // p23891642
    [0.194, 0.233, 0.264, 0.485, 0.672, 0.736, 0.778, 0.801, 0.810], // p26651573
    [0.104, 0.276, 0.379, 0.454, 0.538, 0.731, 0.826, 0.884, 0.933], // p29209958
    [0.160, 0.227, 0.279, 0.364, 0.814, 0.854, 0.865, 0.875, 0.885], // p30154480
    [0.091, 0.145, 0.232, 0.772, 0.843, 0.956, 0.986, 0.997, 1.000], // p31714366
    [0.000, 0.000, 0.000, 0.090, 0.123, 0.151, 0.207, 0.753, 0.888], // p32972578
    [0.000, 0.105, 0.155, 0.526, 0.752, 0.775, 0.795, 0.873, 0.951], // p33202132
    [0.107, 0.159, 0.175, 0.190, 0.237, 0.618, 0.773, 0.847, 0.963], // p33380315
    [0.195, 0.206, 0.250, 0.326, 0.412, 0.500, 0.585, 0.668, 0.936], // p3455934
    [0.136, 0.237, 0.320, 0.396, 0.470, 0.554, 0.733, 0.997, 0.997], // p34704802
    [0.092, 0.170, 0.310, 0.463, 0.556, 0.639, 0.695, 0.750, 0.882], // p36701322
    [0.000, 0.108, 0.227, 0.283, 0.357, 0.530, 0.672, 0.805, 0.996], // p37164686
    [0.244, 0.397, 0.496, 0.582, 0.697, 0.764, 0.831, 0.910, 0.960], // p37461742
    [0.175, 0.373, 0.487, 0.566, 0.674, 0.795, 0.874, 0.935, 0.978], // p7322073
    [0.072, 0.154, 0.227, 0.376, 0.531, 0.631, 0.725, 0.783, 0.883], // p7808457
    [0.206, 0.228, 0.237, 0.251, 0.297, 0.430, 0.665, 0.846, 0.999], // u12655016
    [0.172, 0.282, 0.475, 0.652, 0.763, 0.827, 0.875, 0.912, 0.951], // u19403517
    [0.122, 0.179, 0.301, 0.545, 0.741, 0.822, 0.895, 0.901, 0.904], // u20479992
    [0.121, 0.180, 0.229, 0.277, 0.387, 0.552, 0.633, 0.705, 0.788], // u23531520
    [0.160, 0.187, 0.222, 0.284, 0.366, 0.482, 0.667, 0.820, 0.856], // u30267960
    [0.122, 0.202, 0.248, 0.380, 0.594, 0.725, 0.776, 0.873, 0.951], // u30482418
    [0.092, 0.113, 0.145, 0.185, 0.252, 0.351, 0.587, 0.928, 1.000], // u31705625
    [0.207, 0.495, 0.634, 0.701, 0.789, 0.955, 0.963, 0.963, 0.968], // u32747251
    [0.085, 0.167, 0.286, 0.396, 0.460, 0.565, 0.790, 0.902, 0.960], // u33271173
    [0.067, 0.115, 0.160, 0.204, 0.285, 0.776, 0.937, 0.973, 0.989], // u34923319
    [0.093, 0.264, 0.422, 0.560, 0.643, 0.700, 0.782, 0.827, 0.928], // u36646537
];

/// The built-in prior: what ordinary finished photographs' landmarks look like.
pub(in crate::color_match) fn ordinary() -> Option<&'static ScenePrior> {
    static PRIOR: std::sync::OnceLock<Option<ScenePrior>> = std::sync::OnceLock::new();
    PRIOR
        .get_or_init(|| ScenePrior::from_landmarks(ORDINARY.iter().copied()))
        .as_ref()
}

/// What the tone estimate concluded, for the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToneReading {
    /// Where the look puts black and white, as output lightness for input 0 and 1.
    pub look_black: f32,
    pub look_white: f32,
    /// Where this photograph's own black and white points were judged to sit relative to an
    /// ordinary finished photograph's; zero and one mean it was already ordinary.
    pub own_black: f32,
    pub own_white: f32,
}

/// The tone a photograph with lightness quantiles `target` takes from a reference with lightness
/// quantiles `reference`, over `nodes` evenly spaced inputs. `None` only without a prior.
pub(in crate::color_match) fn for_target(
    reference: &[f32],
    target: &[f32],
    nodes: usize,
) -> Option<(Vec<f32>, ToneReading)> {
    let prior = ordinary()?;
    let look = estimate(reference, prior, LOOK, nodes);
    let own = estimate(target, prior, OWN, nodes);
    let ends = |curve: &[f32]| {
        (
            curve.first().copied().unwrap_or(0.0),
            curve.last().copied().unwrap_or(1.0),
        )
    };
    let ((look_black, look_white), (own_black, own_white)) = (ends(&look), ends(&own));
    let identity: Vec<f32> = (0..nodes)
        .map(|i| i as f32 / (nodes - 1).max(1) as f32)
        .collect();
    let mut curve = relative(&look, &own, nodes);
    // A posterior mean is never exactly the identity. Below a hundredth of lightness anywhere,
    // the estimate is "no tone edit" and the photograph keeps its tone exactly - including any
    // tone an earlier histogram fit would have given it, which measured worse (ADR-0180).
    let moved = curve
        .iter()
        .zip(&identity)
        .map(|(v, x)| (v - x).abs())
        .fold(0.0_f32, f32::max);
    if moved < DEAD_ZONE {
        curve = identity;
    }
    Some((
        curve,
        ToneReading {
            look_black,
            look_white,
            own_black,
            own_white,
        },
    ))
}

/// Evaluate evenly spaced nodes at `x`, linearly.
fn at(nodes: &[f32], x: f32) -> f32 {
    let last = nodes.len().saturating_sub(1);
    if last == 0 {
        return x;
    }
    let position = x.clamp(0.0, 1.0) * last as f32;
    let index = (position as usize).min(last - 1);
    let left = nodes.get(index).copied().unwrap_or(0.0);
    let right = nodes.get(index + 1).copied().unwrap_or(left);
    left + (right - left) * (position - index as f32)
}

/// The tone change that takes a photograph whose own departure from an ordinary photograph is
/// `own` to one whose departure is `look`: `look(own⁻¹(x))`. An ordinary photograph keeps the
/// look as estimated; a flat one is first given an ordinary photograph's range.
pub(in crate::color_match) fn relative(look: &[f32], own: &[f32], nodes: usize) -> Vec<f32> {
    (0..nodes)
        .map(|i| {
            let y = i as f32 / (nodes - 1).max(1) as f32;
            let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if at(own, mid) < y {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            at(look, 0.5 * (lo + hi))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenes() -> Vec<Vec<f32>> {
        // Ordinary photographs: black near 0.05-0.2, white near 0.9-1, midtones spread.
        (0..24)
            .map(|k| {
                let key = 0.35 + 0.3 * (k as f32 / 23.0);
                let black = 0.03 + 0.12 * ((k * 7 % 24) as f32 / 23.0);
                (0..65)
                    .map(|i| {
                        let q = 0.002 + 0.996 * i as f32 / 64.0;
                        let shaped = q.powf((0.5_f32).ln() / key.ln());
                        black + (0.97 - black) * shaped
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn an_ordinary_reference_gets_almost_no_tone_change_and_a_matte_one_a_lift() {
        let scenes = scenes();
        let prior = ScenePrior::from_scenes(scenes.iter().map(Vec::as_slice)).unwrap();
        let ordinary = &scenes[11];
        let curve = estimate(ordinary, &prior, LOOK, 33);
        let worst = curve
            .iter()
            .enumerate()
            .map(|(i, v)| (v - i as f32 / 32.0).abs())
            .fold(0.0_f32, f32::max);
        assert!(worst < 0.03, "ordinary reference moved tone by {worst}");
        let matte: Vec<f32> = ordinary.iter().map(|v| 0.25 + 0.7 * v).collect();
        let curve = estimate(&matte, &prior, LOOK, 33);
        // One reference: the lift is believed in part (the truth here is 0.25), never invented.
        assert!(curve[0] > 0.04, "matte black lift not found: {}", curve[0]);
        assert!(curve.windows(2).all(|w| w[1] >= w[0] - 1e-6));
    }

    #[test]
    fn the_built_in_prior_leaves_an_ordinary_photograph_alone_and_deepens_a_flat_one() {
        let prior = ordinary().unwrap();
        // The median finished photograph of the prior's own table is ordinary.
        let lightness: Vec<f32> = (0..65)
            .map(|i| {
                let q = 0.002 + 0.996 * i as f32 / 64.0;
                0.1 + 0.85 * q.powf(0.85)
            })
            .collect();
        let (curve, _) = for_target(&lightness, &lightness, 33).unwrap();
        assert!(curve
            .iter()
            .enumerate()
            .all(|(i, v)| (v - i as f32 / 32.0).abs() < 1e-6));
        // A flat render of it, blacks lifted and whites lowered, has its shadows deepened -
        // partly, because some finished photographs are hazy too.
        let flat: Vec<f32> = lightness.iter().map(|v| 0.27 + 0.68 * v).collect();
        let (curve, _) = for_target(&lightness, &flat, 33).unwrap();
        assert!(
            curve[8] < 8.0 / 32.0 - 0.01,
            "shadows of a flat photograph deepen: {curve:?}"
        );
        assert!(prior.mahalanobis(&landmarks(&flat)) > prior.mahalanobis(&landmarks(&lightness)));
    }

    #[test]
    fn relative_tone_composes_the_look_with_the_inverse_of_the_photographs_own() {
        let identity: Vec<f32> = (0..33).map(|i| i as f32 / 32.0).collect();
        let lift: Vec<f32> = identity.iter().map(|x| 0.1 + 0.9 * x).collect();
        assert!(relative(&lift, &identity, 33)
            .iter()
            .zip(&lift)
            .all(|(a, b)| (a - b).abs() < 1e-4));
        let back = relative(&lift, &lift, 33);
        assert!(
            back.iter()
                .zip(&identity)
                .skip(4)
                .all(|(a, b)| (a - b).abs() < 2e-3),
            "{back:?}"
        );
    }
}
