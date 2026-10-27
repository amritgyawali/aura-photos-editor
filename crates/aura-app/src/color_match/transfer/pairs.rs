//! Refining a learned edit on corresponding pixels (ADR-0160).
//!
//! When the reference is structurally the same frame as a photograph, the two images line up,
//! and the edit can be measured where it acted: pixel by pixel. Distribution statistics cannot
//! tell which pixels became which - a desaturation and a lift measure alike in a histogram - so
//! the fit from statistics is only the starting point, and this walks every parameter of the
//! transform until the mean colour difference between the transformed photograph and the
//! reference at the same positions stops falling. Deterministic, bounded, and never worse than
//! where it started.
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_recipe::reference_grade::{
    ReferenceGrade, AXIS_NODES, CHROMA_NODES, CHROMA_SPAN, SECTORS, TONE_NODES, ZONES,
};
use aura_render::reference_grade::{oklab_from_linear_srgb, Evaluator};

/// At most this many corresponding pixels are compared.
const MAX_PAIRS: usize = 4096;
/// Step sizes for each kind of parameter, halved on each of [`ROUNDS`] passes.
const ROUNDS: usize = 4;

/// Corresponding pixels of a photograph and its edited reference, which must have the same size.
pub(in crate::color_match) struct Pairs {
    source: Vec<[f32; 3]>,
    target: Vec<[f32; 3]>,
}

impl Pairs {
    /// `None` unless the two images have the same dimensions and enough pixels.
    pub(in crate::color_match) fn new(photo: &Rgb8, reference: &Rgb8) -> Option<Self> {
        if photo.width != reference.width || photo.height != reference.height {
            return None;
        }
        let count = (photo.width * photo.height) as usize;
        if count < 256 || photo.data.len() < count * 3 || reference.data.len() < count * 3 {
            return None;
        }
        let step = count.div_ceil(MAX_PAIRS).max(1);
        let read = |image: &Rgb8, i: usize| {
            image
                .data
                .get(i * 3..i * 3 + 3)
                .and_then(|p| <[u8; 3]>::try_from(p).ok())
                .map(|p| p.map(srgb_byte_to_linear))
        };
        let (mut source, mut target) = (Vec::new(), Vec::new());
        for i in (0..count).step_by(step) {
            if let (Some(s), Some(t)) = (read(photo, i), read(reference, i)) {
                source.push(s);
                target.push(oklab_from_linear_srgb(t));
            }
        }
        Some(Self { source, target })
    }

    /// Mean `OKLab` distance, times a hundred, between `grade` applied to the photograph and the
    /// reference.
    pub(in crate::color_match) fn error(&self, grade: &ReferenceGrade) -> f32 {
        let evaluator = Evaluator::new(grade);
        let total: f32 = self
            .source
            .iter()
            .zip(&self.target)
            .map(|(s, t)| {
                let a = oklab_from_linear_srgb(evaluator.linear_srgb(*s));
                ((a[0] - t[0]).powi(2) + (a[1] - t[1]).powi(2) + (a[2] - t[2]).powi(2)).sqrt()
            })
            .sum();
        100.0 * total / self.source.len().max(1) as f32
    }
}

#[derive(Clone, Copy)]
enum Parameter {
    Tone(usize),
    CastOut(usize, usize),
    Chroma(usize),
    Gain(usize),
    Shift(usize),
    Light(usize),
    Zone(usize),
}

impl Parameter {
    fn all() -> Vec<Self> {
        let mut out = Vec::new();
        out.extend((1..TONE_NODES - 1).map(Self::Tone));
        out.extend((0..AXIS_NODES).flat_map(|k| [Self::CastOut(k, 0), Self::CastOut(k, 1)]));
        out.extend((1..CHROMA_NODES).map(Self::Chroma));
        out.extend((0..SECTORS).flat_map(|k| [Self::Gain(k), Self::Shift(k), Self::Light(k)]));
        out.extend((0..ZONES).map(Self::Zone));
        out
    }

    fn step(self) -> f32 {
        match self {
            Self::Tone(_) => 0.02,
            Self::CastOut(..) => 0.01,
            Self::Chroma(_) => 0.25 * CHROMA_SPAN / (CHROMA_NODES - 1) as f32,
            Self::Gain(_) | Self::Zone(_) => 0.1,
            Self::Shift(_) => 0.06,
            Self::Light(_) => 0.015,
        }
    }

    fn slot(self, grade: &mut ReferenceGrade) -> Option<&mut f32> {
        match self {
            Self::Tone(i) => grade.tone.get_mut(i),
            Self::CastOut(k, c) => grade.cast_out.get_mut(k).and_then(|v| v.get_mut(c)),
            Self::Chroma(i) => grade.chroma.get_mut(i),
            Self::Gain(i) => grade.hue_gain.get_mut(i),
            Self::Shift(i) => grade.hue_shift.get_mut(i),
            Self::Light(i) => grade.hue_light.get_mut(i),
            Self::Zone(i) => grade.zone_gain.get_mut(i),
        }
    }
}

/// Walk every parameter of `grade` to reduce the pixel-by-pixel error against the reference.
///
/// A penalty on moving parameters the frame barely constrains was measured too: it helped
/// synthetic scenes that share no colours and cost the real shoot, so there is none.
pub(in crate::color_match) fn refine(
    grade: &ReferenceGrade,
    pairs: &Pairs,
    mut checkpoint: impl FnMut() -> aura_core::AuraResult<()>,
) -> aura_core::AuraResult<(ReferenceGrade, f32)> {
    let parameters = Parameter::all();
    let mut best = grade.clone();
    let mut error = pairs.error(&best);
    for round in 0..ROUNDS {
        checkpoint()?;
        let scale = 0.5_f32.powi(i32::try_from(round).unwrap_or(i32::MAX));
        for &parameter in &parameters {
            for sign in [1.0_f32, -1.0] {
                let mut candidate = best.clone();
                let Some(value) = parameter.slot(&mut candidate) else {
                    continue;
                };
                *value += sign * parameter.step() * scale;
                let candidate = candidate.bounded();
                let e = pairs.error(&candidate);
                if e.is_finite() && e + 1e-4 < error {
                    best = candidate;
                    error = e;
                    break;
                }
            }
        }
    }
    Ok((best, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Rgb8 {
        Rgb8 {
            width,
            height,
            data: (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .flat_map(|(x, y)| f(x, y))
                .collect(),
        }
    }

    #[test]
    fn refinement_never_ends_worse_and_reduces_a_known_error() {
        let photo = image(64, 48, |x, y| {
            [
                (x * 3 + 40) as u8,
                (y * 4 + 30) as u8,
                ((x + y) * 2 + 20) as u8,
            ]
        });
        let mut known = ReferenceGrade::identity();
        known.cast_out = vec![[0.02, 0.03]; AXIS_NODES];
        known.tone = known.tone.iter().map(|v| 0.05 + 0.9 * v).collect();
        let evaluator = Evaluator::new(&known);
        let reference = Rgb8 {
            width: photo.width,
            height: photo.height,
            data: photo
                .data
                .chunks_exact(3)
                .flat_map(|p| {
                    let lin = evaluator.linear_srgb([p[0], p[1], p[2]].map(srgb_byte_to_linear));
                    lin.map(|v| (aura_render::output::encode_srgb(v) * 255.0).round() as u8)
                })
                .collect(),
        };
        let pairs = Pairs::new(&photo, &reference).unwrap();
        let start = ReferenceGrade::identity();
        let before = pairs.error(&start);
        let (refined, after) = refine(&start, &pairs, || Ok(())).unwrap();
        refined.validate().unwrap();
        assert!(
            after <= before && after < 0.3 * before,
            "{before} -> {after}"
        );
        assert!(Pairs::new(&photo, &image(32, 48, |_, _| [0; 3])).is_none());
    }
}
