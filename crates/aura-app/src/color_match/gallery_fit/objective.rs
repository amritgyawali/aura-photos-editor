//! Prepared perceptual scoring, with conservative evidence for pixel correspondence.
use aura_core::contract::look::LookAggregate;
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_render::reference_grade::oklab_from_linear_srgb;

use super::super::content;

const EDGE: usize = content::EDGE as usize;
const PIXELS: usize = EDGE * EDGE;

#[derive(Debug)]
pub(super) struct Objective<'a> {
    target: &'a LookAggregate,
    reference: Option<Vec<Vec<[f32; 3]>>>,
}

fn luminance(bytes: &[u8]) -> Vec<f64> {
    bytes
        .chunks_exact(3)
        .filter_map(|p| match p {
            &[r, g, b] => {
                Some(0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b))
            }
            _ => None,
        })
        .collect()
}

/// Remove the best affine lightness plane. Two smooth gradients can have perfect signed
/// gradient correlation without providing any evidence that their subjects correspond.
fn residuals(values: &[f64]) -> Vec<f64> {
    let middle = (EDGE - 1) as f64 * 0.5;
    let mean = values.iter().sum::<f64>() / PIXELS as f64;
    let mut moment = [0.0; 2];
    let mut norm = 0.0;
    for (i, &value) in values.iter().enumerate() {
        let x = (i % EDGE) as f64 - middle;
        let y = (i / EDGE) as f64 - middle;
        moment[0] += x * value;
        moment[1] += y * value;
        norm += x * x;
    }
    values
        .iter()
        .enumerate()
        .map(|(i, &value)| {
            value
                - mean
                - (moment[0] * ((i % EDGE) as f64 - middle)
                    + moment[1] * ((i / EDGE) as f64 - middle))
                    / norm
        })
        .collect()
}

fn correlation(a: &[f64], b: &[f64]) -> (f64, f64) {
    let (mut dot, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    let scale = (aa * bb).sqrt();
    (if scale > 1e-9 { dot / scale } else { 0.0 }, aa.min(bb))
}

fn corresponding(reference: &[u8], actual: &Rgb8) -> bool {
    if !content::similarity(reference, actual).is_some_and(|s| s >= 0.985) {
        return false;
    }
    let actual = content::thumbnail(actual);
    let reference = luminance(reference);
    let actual = luminance(&actual);
    if reference.len() != PIXELS || actual.len() != PIXELS {
        return false;
    }
    let (score, energy) = correlation(&residuals(&reference), &residuals(&actual));
    if score < 0.97 || energy < PIXELS as f64 * 4.0 {
        return false;
    }
    // Give supported tiles equal votes: one strong horizon must not hide unrelated objects
    // elsewhere. Thresholds are conservative authored heuristics, not identity probabilities.
    let (mut supported, mut matching) = (0, 0);
    for row in 0..4 {
        for column in 0..4 {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            for y in row * 8..row * 8 + 7 {
                for x in column * 8..column * 8 + 7 {
                    let i = y * EDGE + x;
                    for j in [i + 1, i + EDGE] {
                        let Some((&ra, &rb)) = reference.get(j).zip(reference.get(i)) else {
                            return false;
                        };
                        let Some((&aa, &ab)) = actual.get(j).zip(actual.get(i)) else {
                            return false;
                        };
                        a.push(ra - rb);
                        b.push(aa - ab);
                    }
                }
            }
            let (score, energy) = correlation(&a, &b);
            if energy >= 16.0 {
                supported += 1;
                matching += usize::from(score >= 0.9);
            }
        }
    }
    supported >= 6 && matching * 5 >= supported * 4
}

fn pyramid(bytes: &[u8]) -> Option<Vec<Vec<[f32; 3]>>> {
    if bytes.len() != content::BYTES {
        return None;
    }
    let mut linear: Vec<[f32; 3]> = bytes
        .chunks_exact(3)
        .filter_map(|p| match p {
            &[r, g, b] => Some([r, g, b].map(srgb_byte_to_linear)),
            _ => None,
        })
        .collect();
    let mut edge = EDGE;
    let mut levels = Vec::with_capacity(3);
    for _ in 0..3 {
        levels.push(linear.iter().copied().map(oklab_from_linear_srgb).collect());
        if edge == 8 {
            break;
        }
        let mut next = Vec::with_capacity(edge * edge / 4);
        for y in 0..edge / 2 {
            for x in 0..edge / 2 {
                let i = 2 * y * edge + 2 * x;
                let &[ar, ag, ab] = linear.get(i)?;
                let &[br, bg, bb] = linear.get(i + 1)?;
                let &[cr, cg, cb] = linear.get(i + edge)?;
                let &[dr, dg, db] = linear.get(i + edge + 1)?;
                next.push([
                    (ar + br + cr + dr) * 0.25,
                    (ag + bg + cg + dg) * 0.25,
                    (ab + bb + cb + db) * 0.25,
                ]);
            }
        }
        linear = next;
        edge /= 2;
    }
    Some(levels)
}

impl<'a> Objective<'a> {
    pub(super) fn new(target: &'a LookAggregate, reference: &[u8], baseline: &Rgb8) -> Self {
        Self {
            target,
            reference: corresponding(reference, baseline)
                .then(|| pyramid(reference))
                .flatten(),
        }
    }

    pub(super) fn corresponding(&self) -> bool {
        self.reference.is_some()
    }

    pub(super) fn distance(&self, image: &Rgb8) -> f32 {
        let Some(reference) = &self.reference else {
            let reading = aura_look::measure::read("reference-candidate", image);
            return aura_look::solve::distance(
                self.target,
                &aura_look::aggregate::fold(&[&reading]),
            );
        };
        let bytes = content::thumbnail(image);
        let Some(actual) = pyramid(&bytes) else {
            return f32::INFINITY;
        };
        // Fine colour details dominate; coarser scales also measure broad casts and tone.
        // Prepare the reference once per search, and use exactly this objective natively.
        reference
            .iter()
            .zip(actual)
            .zip([0.7, 0.2, 0.1])
            .map(|((a, b), weight)| {
                weight
                    * 100.0
                    * a.iter()
                        .zip(&b)
                        .map(|(a, b)| {
                            a.iter()
                                .zip(b)
                                .map(|(a, b)| (a - b).powi(2))
                                .sum::<f32>()
                                .sqrt()
                        })
                        .sum::<f32>()
                    / a.len() as f32
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(grade: bool) -> Rgb8 {
        let data = (0..PIXELS)
            .flat_map(|i| {
                let (x, y) = (i % EDGE, i / EDGE);
                let value = 35 + ((x * 13 + y * 7) % 140) as u8;
                if grade {
                    [value + 10, value + 4, value]
                } else {
                    [value; 3]
                }
            })
            .collect();
        Rgb8 {
            width: EDGE as u32,
            height: EDGE as u32,
            data,
        }
    }

    #[test]
    fn smooth_gradients_are_not_evidence_of_corresponding_objects() {
        let image = |offset: u8| Rgb8 {
            width: EDGE as u32,
            height: EDGE as u32,
            data: (0..PIXELS)
                .flat_map(|i| [20 + 4 * (i % EDGE) as u8 + offset; 3])
                .collect(),
        };
        let reference = content::thumbnail(&image(20));
        assert!(content::similarity(&reference, &image(0)).unwrap() > 0.99);
        assert!(!corresponding(&reference, &image(0)));
        assert!(!corresponding(&[], &image(0)));
    }

    #[test]
    fn supported_structure_uses_perceptual_scoring_at_multiple_scales() {
        let baseline = scene(false);
        let edited = scene(true);
        let target = aura_look::aggregate::fold(&[]);
        let reference = content::thumbnail(&edited);
        let objective = Objective::new(&target, &reference, &baseline);
        assert!(objective.corresponding());
        assert!(objective.distance(&baseline) > 0.5);
        assert!(objective.distance(&edited) < 1e-6);
        assert_eq!(
            pyramid(&reference)
                .unwrap()
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            [1024, 256, 64]
        );
        let unrelated = Rgb8 {
            data: edited.data.iter().rev().copied().collect(),
            ..edited
        };
        assert!(!corresponding(&reference, &unrelated));
    }

    #[test]
    fn a_shared_strong_horizon_cannot_hide_mismatched_local_details() {
        let image = |reversed: bool| Rgb8 {
            width: EDGE as u32,
            height: EDGE as u32,
            data: (0..PIXELS)
                .flat_map(|i| {
                    let base = if i / EDGE < EDGE / 2 { 20 } else { 220 };
                    let detail = 2 * u8::from((i % EDGE + i / EDGE).is_multiple_of(2) != reversed);
                    [base + detail; 3]
                })
                .collect(),
        };
        let reference = content::thumbnail(&image(false));
        let unrelated = image(true);
        assert!(content::similarity(&reference, &unrelated).unwrap() >= 0.985);
        assert!(!corresponding(&reference, &unrelated));
    }
}
