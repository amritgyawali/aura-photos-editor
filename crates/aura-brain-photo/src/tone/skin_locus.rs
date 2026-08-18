//! Where one person's skin actually sits, learned from this wedding.
//!
//! PHASE-15 section 8's step 4: "implement the per-identity skin locus accumulation", and
//! section 6.3's whole argument:
//!
//! > Skin targets are measured per identity from the wedding's own best-lit frames, not from
//! > a fixed 'ideal' skin value - this is how the system avoids lightening or warming darker
//! > skin toward a Eurocentric target.
//!
//! **There is no ideal skin value in this module, in the contract, in the schema or in the
//! config.** That is not a courtesy. A fixed target is exactly how an image editor lightens
//! dark skin while believing it is correcting a colour cast, and the defence against it is
//! structural: nothing in this code path has a constant it could compare a person against.
//! What it has is [`LocusBuilder`], which accumulates that person's own appearance and then
//! asks every white-balance hypothesis to be consistent with it.
//!
//! ## Reflectance, not appearance
//!
//! A sample is not "what colour this face was in this photograph". It is **what colour this
//! face would have been under a neutral light**, obtained by dividing the observed linear
//! colour by the frame's own estimated illuminant (`aura_raw::colour::illuminant::adapt`).
//! Without that division a locus would be a record of the lighting rather than of the
//! person, and a wedding that moved from daylight to tungsten would produce a locus the size
//! of the chromaticity diagram.
//!
//! ## The chicken and the egg, and how it is resolved
//!
//! Section 6.2 scores an illuminant hypothesis by how plausible it makes known skin. Section
//! 6.3 measures skin by dividing out the illuminant. Each needs the other.
//!
//! The resolution is section 8's own ordering - step 4 before step 5 - made explicit as
//! **two passes over the wedding**:
//!
//! 1. **The locus pass** solves each frame's white balance *without* the skin term, using
//!    neutrals, grey-world, white-patch and the learned head, and contributes a skin sample
//!    only from frames whose answer was confident. Section 6.3's "best-lit frames" is that
//!    filter, and [`MIN_CONTRIBUTING_CONF`] is where it lives.
//! 2. **The solve pass** re-solves every frame with the skin term and the hard constraint.
//!
//! A wedding analysed once is therefore analysed correctly and a wedding analysed twice is
//! analysed better, which is a property worth knowing about rather than a defect: the pass
//! is resumable and idempotent, so the second run is what happens anyway the first time a
//! photographer imports a second card.
//!
//! ## Why a bad frame cannot mislead
//!
//! Section 6.2 asks for exactly this. Three things provide it, in order of how much work
//! they do: samples are **weighted** by the face's own quality and the frame's white-balance
//! confidence; the accumulation **trims** samples that sit more than [`TRIM_SIGMAS`] mean
//! deviations from the first-pass centre and recomputes; and the locus **does not exist at
//! all** below `MIN_LOCUS_SAMPLES` frames, because a locus fitted to two frames looks like
//! evidence and is not.

use std::collections::BTreeMap;

use aura_core::contract::tone::{SkinLocus, MIN_LOCUS_SAMPLES};
use aura_core::IdentityId;
use aura_raw::colour::illuminant::{adapt, linear_srgb_to_uv, uv_distance};

use crate::tone::stats::SkinPatch;

/// The white-balance confidence a frame needs before its skin samples count.
///
/// Section 6.3's "best-lit frames", as a number. Deliberately high: a locus is a *hard
/// constraint* on every later solve, so a locus built from mediocre evidence does not
/// produce mediocre answers - it produces confident wrong ones, and rejects the right answer
/// on the frames that needed help most.
pub const MIN_CONTRIBUTING_CONF: f32 = 0.70;

/// The face quality below which a patch is not sampled at all.
///
/// Phase 06's fused quality. A face at 0.35 is out of focus, at a steep angle or eleven
/// pixels across, and its mean chromaticity is a mean of the background as much as of a
/// cheek.
pub const MIN_FACE_QUALITY: f32 = 0.45;

/// The luminance band a contributing patch sits in.
///
/// A face at 2 % linear luminance carries about three bits of chroma and a face at 95 % has
/// begun to clip; neither says what colour anybody's skin is. This band is what "best-lit"
/// means in the photometric half of the phrase.
pub const CONTRIBUTING_LUMA: (f32, f32) = (0.02, 0.75);

/// How many mean deviations from the first-pass centre a sample may sit before it is
/// trimmed.
///
/// Two. Not three: this is a *mean absolute* deviation rather than a standard deviation, so
/// two of them is already a generous multiple, and the thing being defended against is one
/// frame where a green uplighter caught somebody's face.
pub const TRIM_SIGMAS: f32 = 2.0;

/// How many mean deviations wide the plausible region is.
///
/// Two and a half, so the radius admits samples slightly outside the trim that produced it.
/// A radius exactly equal to the trim would reject, at solve time, the very samples that
/// built the locus.
pub const RADIUS_SIGMAS: f32 = 2.5;

/// One contribution to one person's locus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// The reflectance chromaticity: observed colour divided by the frame's own illuminant.
    pub uv: [f32; 2],
    /// The reflectance luminance, `0..1`.
    pub luma: f32,
    /// How much this sample counts, `0..1`.
    pub weight: f32,
}

/// Turn one observed skin patch into a reflectance sample.
///
/// Returns `None` when the frame or the face was not good enough to contribute, which is the
/// common case rather than the exceptional one: a wedding contributes a few hundred samples
/// out of several thousand faces, and that is the filter working.
#[must_use]
pub fn sample_from(patch: &SkinPatch, illuminant_uv: [f32; 2], wb_conf: f32) -> Option<Sample> {
    if wb_conf < MIN_CONTRIBUTING_CONF || patch.quality < MIN_FACE_QUALITY {
        return None;
    }
    if patch.luma < CONTRIBUTING_LUMA.0 || patch.luma > CONTRIBUTING_LUMA.1 {
        return None;
    }
    let reflectance = adapt(patch.linear, illuminant_uv);
    let luma = 0.2126 * reflectance[0] + 0.7152 * reflectance[1] + 0.0722 * reflectance[2];
    if !luma.is_finite() || luma <= 0.0 {
        return None;
    }
    Some(Sample {
        uv: linear_srgb_to_uv(reflectance),
        luma: luma.clamp(0.0, 1.0),
        // The product of the two confidences rather than either alone, for the reason every
        // fusion in this product is a product: a perfect face in a frame nobody understood
        // is not evidence, and neither is a confident frame with a blurred face in it.
        weight: (patch.quality.clamp(0.0, 1.0) * wb_conf.clamp(0.0, 1.0)).clamp(0.0, 1.0),
    })
}

/// Accumulates one project's skin samples, by identity.
///
/// A `BTreeMap` rather than a hash map, for the reason every collection in this product is
/// ordered: the iteration order reaches a report, a log line and a determinism test, and a
/// hash map's order is a function of a seed nobody controls.
#[derive(Debug, Clone, Default)]
pub struct LocusBuilder {
    samples: BTreeMap<IdentityId, Vec<Sample>>,
}

impl LocusBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one sample for one person.
    pub fn add(&mut self, identity: IdentityId, sample: Sample) {
        self.samples.entry(identity).or_default().push(sample);
    }

    /// How many people have contributed anything at all.
    #[must_use]
    pub fn identities(&self) -> usize {
        self.samples.len()
    }

    /// How many samples one person has contributed.
    #[must_use]
    pub fn count(&self, identity: IdentityId) -> usize {
        self.samples.get(&identity).map_or(0, Vec::len)
    }

    /// Fit every locus that has enough evidence.
    ///
    /// People below [`MIN_LOCUS_SAMPLES`] are **absent from the result** rather than present
    /// with a low confidence. That is the module header's third defence and it is the one
    /// that matters: a caller holding a weak locus will use it, and a caller holding no
    /// locus emits `AURA-ML-5065` and says so in the panel.
    #[must_use]
    pub fn finish(&self, analysis_ver: u16) -> BTreeMap<IdentityId, SkinLocus> {
        let mut out = BTreeMap::new();
        for (identity, samples) in &self.samples {
            if let Some(locus) = fit(*identity, samples, analysis_ver) {
                out.insert(*identity, locus);
            }
        }
        out
    }
}

/// Fit one locus from one person's samples, or refuse.
///
/// Two rounds: a weighted centre over everything, then a trim and a second centre. One trim
/// rather than an iterated robust fit, because the second round changes the answer by
/// something a photographer could see and the third does not - and every round costs a pass
/// over samples that a 4,000-image wedding has thousands of.
fn fit(identity: IdentityId, samples: &[Sample], analysis_ver: u16) -> Option<SkinLocus> {
    if samples.len() < MIN_LOCUS_SAMPLES as usize {
        return None;
    }
    let first = weighted_centre(samples)?;
    let deviation = mean_deviation(samples, first.0);
    let trim = (deviation * TRIM_SIGMAS).max(SkinLocus::MIN_RADIUS);

    let kept: Vec<Sample> = samples
        .iter()
        .copied()
        .filter(|sample| uv_distance(sample.uv, first.0) <= trim)
        .collect();
    // The trim can, on a pathological set, remove nearly everything. Falling back to the
    // untrimmed centre is right: a locus fitted to a scattered set is a wide locus, which
    // constrains little, and that is a better failure than no locus at all for somebody who
    // was photographed a hundred times.
    let effective: &[Sample] = if kept.len() >= MIN_LOCUS_SAMPLES as usize {
        &kept
    } else {
        samples
    };
    let (uv, luma) = weighted_centre(effective)?;
    let spread = mean_deviation(effective, uv);
    let radius = (spread * RADIUS_SIGMAS).clamp(SkinLocus::MIN_RADIUS, SkinLocus::MAX_RADIUS);

    Some(SkinLocus {
        identity,
        uv,
        radius,
        luma,
        samples: effective.len() as u32,
        // One at zero spread, falling to zero at the maximum radius. A number the panel
        // shows and the solve does not read: a wide locus already constrains less by being
        // wide, and multiplying that by a cohesion would charge for the same looseness twice.
        cohesion: (1.0 - spread / SkinLocus::MAX_RADIUS).clamp(0.0, 1.0),
        analysis_ver,
    })
}

/// The weight-weighted mean chromaticity and luminance.
fn weighted_centre(samples: &[Sample]) -> Option<([f32; 2], f32)> {
    let mut total = 0.0f64;
    let mut sums = [0.0f64; 3];
    for sample in samples {
        let weight = f64::from(sample.weight.max(1e-3));
        total += weight;
        sums[0] += f64::from(sample.uv[0]) * weight;
        sums[1] += f64::from(sample.uv[1]) * weight;
        sums[2] += f64::from(sample.luma) * weight;
    }
    if total <= 0.0 {
        return None;
    }
    Some((
        [(sums[0] / total) as f32, (sums[1] / total) as f32],
        (sums[2] / total) as f32,
    ))
}

/// Weighted mean absolute distance from a centre, in `u'v'`.
fn mean_deviation(samples: &[Sample], centre: [f32; 2]) -> f32 {
    let mut total = 0.0f64;
    let mut weight_total = 0.0f64;
    for sample in samples {
        let weight = f64::from(sample.weight.max(1e-3));
        total += f64::from(uv_distance(sample.uv, centre)) * weight;
        weight_total += weight;
    }
    if weight_total <= 0.0 {
        return 0.0;
    }
    (total / weight_total) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_raw::colour::illuminant::{cct_to_uv, uv_to_linear_srgb, D65_UV};

    fn patch(linear: [f32; 3], quality: f32) -> SkinPatch {
        SkinPatch {
            linear,
            uv: linear_srgb_to_uv(linear),
            luma: 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2],
            area: aura_core::CropRect::FULL,
            samples: 500,
            quality,
        }
    }

    /// A surface of reflectance `refl` seen under an illuminant.
    fn under(refl: [f32; 3], illuminant: [f32; 2]) -> [f32; 3] {
        let light = uv_to_linear_srgb(illuminant);
        [refl[0] * light[0], refl[1] * light[1], refl[2] * light[2]]
    }

    /// Six lights spanning what a wedding contains, warm to cool.
    const LIGHTS: [f32; 6] = [5500.0, 2800.0, 4200.0, 6500.0, 3200.0, 5000.0];

    /// Fit one locus from one reflectance seen under `lights`.
    ///
    /// Returns `None` rather than unwrapping, because the panic family is denied in this
    /// crate - in tests as well as in library code - and every assertion below reads better
    /// as a statement about an `Option` anyway.
    fn locus_of(reflectance: [f32; 3], lights: &[f32], quality: f32) -> Option<SkinLocus> {
        let mut builder = LocusBuilder::new();
        let identity = IdentityId::new();
        for kelvin in lights {
            let light = cct_to_uv(*kelvin);
            if let Some(sample) =
                sample_from(&patch(under(reflectance, light), quality), light, 0.95)
            {
                builder.add(identity, sample);
            }
        }
        builder.finish(1).get(&identity).copied()
    }

    #[test]
    fn the_same_person_under_two_lights_gives_one_locus() {
        // The property the whole module exists for. A person photographed in daylight and
        // again under tungsten must produce ONE locus, because a sample is a reflectance and
        // not an appearance.
        let skin = [0.34f32, 0.22, 0.17];
        let Some(locus) = locus_of(skin, &LIGHTS, 0.9) else {
            unreachable!("six well-lit samples fit a locus");
        };
        assert!(locus.is_usable());
        // The centre has to be the skin's own reflectance chromaticity, not any one light's.
        let expected = linear_srgb_to_uv(skin);
        assert!(
            uv_distance(locus.uv, expected) < 0.01,
            "{:?} vs {expected:?}",
            locus.uv
        );
        assert!(
            locus.radius <= SkinLocus::MIN_RADIUS + 1e-4,
            "a consistent person gets the tightest radius: {}",
            locus.radius
        );
    }

    #[test]
    fn one_bad_frame_cannot_move_the_centre() {
        // Section 6.2's requirement, as a test. Eight honest frames and one where a green
        // uplighter caught the face; the centre must stay where the eight put it.
        let skin = [0.34f32, 0.22, 0.17];
        let daylight = cct_to_uv(5500.0);
        let mut builder = LocusBuilder::new();
        let identity = IdentityId::new();
        for _ in 0..8 {
            if let Some(sample) = sample_from(&patch(under(skin, daylight), 0.9), daylight, 0.95) {
                builder.add(identity, sample);
            }
        }
        let clean = builder.finish(1).get(&identity).copied();

        let poisoned = [0.10f32, 0.42, 0.12];
        if let Some(sample) = sample_from(&patch(under(poisoned, daylight), 0.9), daylight, 0.95) {
            builder.add(identity, sample);
        }
        let after = builder.finish(1).get(&identity).copied();

        let (Some(clean), Some(after)) = (clean, after) else {
            unreachable!("eight and nine samples both fit a locus");
        };
        assert!(
            uv_distance(clean.uv, after.uv) < 0.004,
            "the outlier moved the centre from {:?} to {:?}",
            clean.uv,
            after.uv
        );
    }

    #[test]
    fn four_samples_produce_no_locus_at_all() {
        let skin = [0.34f32, 0.22, 0.17];
        assert!(
            locus_of(skin, &[5500.0, 5500.0, 5500.0, 5500.0], 0.9).is_none(),
            "a weak locus is worse than none, because it looks like evidence"
        );
    }

    #[test]
    fn a_low_confidence_frame_contributes_nothing() {
        let skin = [0.34f32, 0.22, 0.17];
        assert!(sample_from(&patch(under(skin, D65_UV), 0.9), D65_UV, 0.40).is_none());
    }

    #[test]
    fn a_poor_face_contributes_nothing() {
        let skin = [0.34f32, 0.22, 0.17];
        assert!(sample_from(&patch(under(skin, D65_UV), 0.20), D65_UV, 0.95).is_none());
    }

    #[test]
    fn a_dark_and_a_light_person_get_loci_of_the_same_tightness() {
        // The fairness property this module is really about, at the level the code can
        // assert: the machinery must not be systematically less certain about darker skin.
        // Two reflectances a long way apart in lightness, the same lights, the same
        // consistency - the radii have to match.
        let light = locus_of([0.62, 0.46, 0.38], &LIGHTS, 0.9).map(|locus| locus.radius);
        let dark = locus_of([0.13, 0.085, 0.065], &LIGHTS, 0.9).map(|locus| locus.radius);
        let (Some(light), Some(dark)) = (light, dark) else {
            unreachable!("both fit a locus");
        };
        assert!(
            (light - dark).abs() < 1e-4,
            "the constraint must be equally tight: {light} vs {dark}"
        );
    }
}
