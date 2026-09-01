//! Monochrome suitability, and the mix solved for one frame.
//!
//! Section 6.1: score on tonal separation, colour distraction, gesture strength, emotional intensity
//! and noise character, then generate "a per-frame channel mix that maximises subject separation
//! rather than applying one preset".
//!
//! # The instrument, and what it can and cannot see
//!
//! Everything here is measured from phase 05's stored 8x8x8 HSV histogram and six luminance
//! statistics. **Nothing in this module opens a photograph.** That is phase 05's rule - descriptors
//! are computed once - and it is what makes a whole gallery affordable inside section 11's 20 s
//! budget.
//!
//! It is also a coarse instrument and the phase says so. Five hundred and twelve bins over a whole
//! frame cannot tell a face from the hedge behind it; what it can tell is which *hue bands* the
//! frame's colour lives in, how much of each there is, how saturated each is, and how bright.
//! `docs/curation.md` says the same thing in the product's own words.
//!
//! # The eight bands, and why a histogram bin is split between them
//!
//! The histogram's hue axis has eight equal 45-degree bins; phase 16's eight HSL bands are **not**
//! equal - orange spans 30 degrees and green spans 60 - and their centres do not line up with the
//! histogram's. Assigning each bin to its nearest band leaves the *red* band with no bin at all,
//! which would mean no mix in this product could ever touch red: a lehenga, a bouquet of roses and a
//! chuppah drape would all be immovable while the code looked correct.
//!
//! So a bin's population is **split across the bands its 45 degrees overlap**, in proportion to the
//! overlap. Every band gets weight, the total is conserved, and the arithmetic is a fixed table
//! rather than a search.
//!
//! # The skin rule
//!
//! [`solve`] **pins every band a measured skin locus falls in at zero.** It does not move them a
//! little; it does not move them in a safe direction; it does not move them at all. Separation comes
//! from moving what skin is competing *with*, and every argument for moving the skin band itself is
//! an argument for changing how somebody looks in a photograph that has no colour left to explain
//! it. The contract permits up to `MAX_SKIN_BAND_SHIFT` so that a future solver is bounded rather
//! than free; this one uses none of it.
//!
//! And when a frame **has faces but nobody in it has a usable locus, no mix is offered at all.**
//! Phase 24's rule: an absent input is ignorance, not permission. A frame with no faces - a ring, a
//! flat-lay, an empty church - has no skin to protect and is solved normally.
//!
//! **On this build both halves of that are unreachable**, because phase 06's detector finds no faces
//! and phase 15 therefore has no loci: every frame is solved as a faceless frame. That is condition
//! C2 of the phase 29 exit report, and it is the reason the mechanism is tested against fixtures
//! that *do* carry faces and loci.

use std::collections::BTreeSet;

use aura_core::contract::curate::{
    BwMix, BwPick, BwTerms, CurateCode, CurateReason, ImageId, MAX_BAND_SHIFT, MAX_REASONS,
};

use crate::explain::rank_reasons;
use crate::policy::Policy;
use crate::read::{Descriptor, Frame};

/// Bins on one axis of phase 05's histogram.
const AXIS: usize = 8;

/// How the eight histogram hue bins divide among the eight HSL bands.
///
/// Row `i` is histogram bin `i`; each entry is `(band index, share)`. Derived from the overlap of
/// each bin's 45-degree span with the band spans implied by phase 16's band centres - red 0, orange
/// 30, yellow 60, green 120, aqua 180, blue 240, purple 280, magenta 320, with boundaries at their
/// midpoints. Written out rather than computed so that the arithmetic is inspectable and so that a
/// change to it is a diff somebody reviews.
///
/// The shares in each row sum to 1.0, and the whole table sums to 8.0, which
/// `the_band_table_conserves_population` asserts.
const BAND_SPLIT: [&[(usize, f32)]; AXIS] = [
    // bin 0: 0-45 deg. red [0,15), orange [15,45).
    &[(0, 15.0 / 45.0), (1, 30.0 / 45.0)],
    // bin 1: 45-90. yellow [45,90).
    &[(2, 1.0)],
    // bin 2: 90-135. green [90,135).
    &[(3, 1.0)],
    // bin 3: 135-180. green [135,150), aqua [150,180).
    &[(3, 15.0 / 45.0), (4, 30.0 / 45.0)],
    // bin 4: 180-225. aqua [180,210), blue [210,225).
    &[(4, 30.0 / 45.0), (5, 15.0 / 45.0)],
    // bin 5: 225-270. blue [225,260), purple [260,270).
    &[(5, 35.0 / 45.0), (6, 10.0 / 45.0)],
    // bin 6: 270-315. purple [270,300), magenta [300,315).
    &[(6, 30.0 / 45.0), (7, 15.0 / 45.0)],
    // bin 7: 315-360. magenta [315,340), red [340,360).
    &[(7, 25.0 / 45.0), (0, 20.0 / 45.0)],
];

/// What one frame's colour looks like, band by band.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandReading {
    /// Share of the frame's coloured population in this band, `0..1`.
    pub share: [f32; 8],
    /// Mean value (brightness) of this band's population, `0..1`.
    pub luma: [f32; 8],
    /// Mean saturation of this band's population, `0..1`.
    pub saturation: [f32; 8],
    /// Share of the frame that is close to neutral and belongs to no band, `0..1`.
    pub neutral_share: f32,
}

impl BandReading {
    /// The band with the largest share, when any band has one.
    #[must_use]
    pub fn dominant(&self) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (ix, share) in self.share.iter().enumerate() {
            if best.is_none_or(|(_, b)| *share > b) {
                best = Some((ix, *share));
            }
        }
        best.filter(|(_, share)| *share > 0.0).map(|(ix, _)| ix)
    }
}

/// Read the eight bands out of phase 05's histogram.
///
/// A bin's population is weighted by its **saturation** before it is assigned to a band, because a
/// near-grey pixel has no hue worth acting on: a mix that moved the band a wall's faint warm cast
/// happens to land in would be moving the wall. Near-neutral population is counted separately, and
/// `neutral_share` is what tells the score that a frame is nearly monochrome already.
#[must_use]
pub fn bands(descriptor: &Descriptor) -> BandReading {
    let mut reading = BandReading::default();
    let mut weight = [0.0f32; 8];
    let mut luma_sum = [0.0f32; 8];
    let mut sat_sum = [0.0f32; 8];
    let mut neutral = 0.0f32;
    let mut total = 0.0f32;

    for (index, count) in descriptor.hsv_hist.iter().enumerate() {
        let population = f32::from(*count);
        if population <= 0.0 {
            continue;
        }
        let hue_bin = index / (AXIS * AXIS);
        let sat_bin = (index / AXIS) % AXIS;
        let val_bin = index % AXIS;
        let saturation = (sat_bin as f32 + 0.5) / AXIS as f32;
        let value = (val_bin as f32 + 0.5) / AXIS as f32;
        total += population;

        // A pixel with no meaningful hue contributes to the neutral share and to no band.
        let coloured = population * saturation;
        neutral += population * (1.0 - saturation);

        let Some(split) = BAND_SPLIT.get(hue_bin) else {
            continue;
        };
        for (band, portion) in *split {
            let Some(slot) = weight.get_mut(*band) else {
                continue;
            };
            let contribution = coloured * portion;
            *slot += contribution;
            if let Some(l) = luma_sum.get_mut(*band) {
                *l += contribution * value;
            }
            if let Some(s) = sat_sum.get_mut(*band) {
                *s += contribution * saturation;
            }
        }
    }

    let coloured_total: f32 = weight.iter().sum();
    for band in 0..8 {
        let Some(w) = weight.get(band) else { continue };
        if *w <= f32::EPSILON {
            continue;
        }
        if let Some(slot) = reading.share.get_mut(band) {
            *slot = if coloured_total > f32::EPSILON {
                w / coloured_total
            } else {
                0.0
            };
        }
        if let (Some(slot), Some(sum)) = (reading.luma.get_mut(band), luma_sum.get(band)) {
            *slot = sum / w;
        }
        if let (Some(slot), Some(sum)) = (reading.saturation.get_mut(band), sat_sum.get(band)) {
            *slot = sum / w;
        }
    }
    reading.neutral_share = if total > f32::EPSILON {
        neutral / total
    } else {
        0.0
    };
    reading
}

/// How far apart the frame's tones stay once the colour is gone, `0..1`.
///
/// The population-weighted spread of the value axis, measured as the distance between its 10th and
/// 90th percentiles and normalised against three quarters of the range - a frame that uses 75 % of
/// the tonal range from shadow to highlight is fully separated for this purpose.
///
/// Percentiles rather than a standard deviation because a bimodal frame - a white dress against a
/// dark church - is exactly the frame that converts best, and a standard deviation reads it the same
/// as a flat mid-grey with noise.
#[must_use]
pub fn tonal_separation(descriptor: &Descriptor) -> f32 {
    let mut value_hist = [0.0f32; AXIS];
    for (index, count) in descriptor.hsv_hist.iter().enumerate() {
        let val_bin = index % AXIS;
        if let Some(slot) = value_hist.get_mut(val_bin) {
            *slot += f32::from(*count);
        }
    }
    let total: f32 = value_hist.iter().sum();
    if total <= f32::EPSILON {
        return 0.0;
    }
    let percentile = |target: f32| -> f32 {
        let mut seen = 0.0f32;
        for (ix, count) in value_hist.iter().enumerate() {
            seen += count;
            if seen / total >= target {
                return (ix as f32 + 0.5) / AXIS as f32;
            }
        }
        1.0
    };
    let spread = (percentile(0.90) - percentile(0.10)).max(0.0);
    (spread / 0.75).clamp(0.0, 1.0)
}

/// How much saturated colour away from the subject is pulling the eye, `0..1`.
///
/// The share of the frame's coloured population that is both strongly saturated and **far in hue
/// from wherever the people are**. With no skin bands the hue distance term drops out and this is
/// the plain saturated share, which is honest: a frame with no measured people has no subject this
/// instrument can locate.
///
/// A global histogram cannot tell where in the frame anything is, so "away from the subject" is a
/// hue distance rather than a spatial one. That is the best proxy the stored descriptors support and
/// the module header says so.
#[must_use]
pub fn colour_distraction(reading: &BandReading, skin_bands: &BTreeSet<usize>) -> f32 {
    let mut distraction = 0.0f32;
    for band in 0..8 {
        let (Some(share), Some(saturation)) =
            (reading.share.get(band), reading.saturation.get(band))
        else {
            continue;
        };
        if *share <= 0.0 {
            continue;
        }
        // Circular distance between band centres, normalised to `0..1`.
        let hue_distance = if skin_bands.is_empty() {
            1.0
        } else {
            skin_bands
                .iter()
                .map(|skin| band_distance(band, *skin))
                .fold(1.0f32, f32::min)
        };
        // Saturation above a half is where a colour starts competing rather than describing.
        let competing = ((saturation - 0.5) / 0.5).clamp(0.0, 1.0);
        distraction += share * competing * hue_distance;
    }
    (distraction / 0.35).clamp(0.0, 1.0)
}

/// True when two substantial saturated bands would land on the same grey.
///
/// The measurement behind [`CurateCode::ColourIsTheSubject`]. Both bands need a real share of the
/// frame, both need enough saturation for the hue to be doing the work, and their greys need to be
/// close enough that a neutral desaturation merges them.
///
/// Skin bands are excluded from the pair: a face and a hand that land on the same grey is not a
/// frame whose subject is its colour, it is a person.
#[must_use]
pub fn hue_carried(reading: &BandReading, skin_bands: &BTreeSet<usize>) -> bool {
    let mut substantial: Vec<(f32, f32)> = Vec::new();
    for band in 0..8 {
        if skin_bands.contains(&band) {
            continue;
        }
        let (Some(share), Some(saturation), Some(luma)) = (
            reading.share.get(band),
            reading.saturation.get(band),
            reading.luma.get(band),
        ) else {
            continue;
        };
        if *share >= 0.15 && *saturation >= 0.55 {
            substantial.push((*luma, *share));
        }
    }
    for (ix, (luma, _)) in substantial.iter().enumerate() {
        for (other, _) in substantial.iter().skip(ix + 1) {
            if (luma - other).abs() < 0.06 {
                return true;
            }
        }
    }
    false
}

/// Circular distance between two band indices, `0..1`.
fn band_distance(a: usize, b: usize) -> f32 {
    let raw = (a as i32 - b as i32).abs();
    let circular = raw.min(8 - raw);
    circular as f32 / 4.0
}

/// How well this frame's noise would read as grain, `0..1`.
///
/// A peak rather than a slope. A clean frame has no grain to speak of and scores zero - which does
/// not mean it converts badly, only that grain is not a reason to convert it. A frame at three or
/// four tenths of a stop of relative noise is where film grain lives. Past that it is noise, and
/// monochrome does not make noise into grain.
#[must_use]
pub fn grain(noise_sigma_rel: f32) -> f32 {
    const PEAK: f32 = 0.35;
    if !noise_sigma_rel.is_finite() || noise_sigma_rel <= 0.0 {
        return 0.0;
    }
    (1.0 - (noise_sigma_rel - PEAK).abs() / PEAK).clamp(0.0, 1.0)
}

/// The bands somebody's measured skin sits in, for the frame's own identities.
///
/// Empty when nobody in the frame has a usable locus. Distinct from "no people in the frame", which
/// is `frame.faces.is_empty()` - and the two lead to opposite decisions in [`suitability`].
#[must_use]
pub fn skin_bands_of(frame: &Frame, loci: &std::collections::BTreeMap<aura_core::contract::ids::IdentityId, u8>) -> BTreeSet<usize> {
    frame
        .identities
        .iter()
        .filter_map(|id| loci.get(id))
        .map(|band| usize::from(*band).min(7))
        .collect()
}

/// One frame's monochrome suitability, or `None` when it is not offered.
///
/// Three ways to get `None`, and they are not the same refusal:
///
/// * **No descriptors.** Nothing to measure. The frame is not offered and it is not a candidate that
///   scored badly.
/// * **Faces but no measured skin locus.** ADR-0059 section 5: the product does not propose a
///   monochrome conversion of a photograph of a person until it has measured that person's skin.
/// * **Below the flat floor, or below the candidate floor.** The conversion would flatten the frame,
///   or it is simply not worth offering.
#[must_use]
pub fn suitability(
    frame: &Frame,
    skin_bands: &BTreeSet<usize>,
    policy: &Policy,
) -> Option<BwPick> {
    let descriptor = frame.descriptor.as_ref()?;
    let has_faces = !frame.faces.is_empty();
    if has_faces && skin_bands.is_empty() {
        return None;
    }

    let reading = bands(descriptor);
    let terms = BwTerms {
        tonal_separation: tonal_separation(descriptor),
        colour_distraction: colour_distraction(&reading, skin_bands),
        gesture: frame.interaction.unwrap_or(0.0).clamp(0.0, 1.0),
        emotion: frame.emotion.unwrap_or(0.0).clamp(0.0, 1.0),
        grain: frame.noise_sigma_rel.map_or(0.0, grain),
    };

    if terms.tonal_separation < policy.bw_flat_floor {
        return None;
    }

    // The weighted blend, renormalised over the terms that were actually measured. A frame with no
    // emotion reading gets a narrower score and a lower confidence, never a confident zero.
    let available: [(f32, f32, bool); 5] = [
        (
            policy.bw_weights.tonal_separation,
            terms.tonal_separation,
            true,
        ),
        (
            policy.bw_weights.colour_distraction,
            terms.colour_distraction,
            true,
        ),
        (
            policy.bw_weights.gesture,
            terms.gesture,
            frame.interaction.is_some(),
        ),
        (
            policy.bw_weights.emotion,
            terms.emotion,
            frame.emotion.is_some(),
        ),
        (
            policy.bw_weights.grain,
            terms.grain,
            frame.noise_sigma_rel.is_some(),
        ),
    ];
    let mut weight_sum = 0.0f32;
    let mut score = 0.0f32;
    for (weight, value, measured) in available {
        if measured {
            weight_sum += weight;
            score += weight * value;
        }
    }
    if weight_sum <= f32::EPSILON {
        return None;
    }
    let mut score = score / weight_sum;

    // Colour is the subject when two substantial, saturated regions are told apart by their **hue
    // alone** - they land on the same grey, so desaturating merges them.
    //
    // Not "one saturated band dominates", which was the first formulation and is wrong: a forest
    // with light falling through it is one saturated band covering the whole frame and converts
    // beautifully, because its separation is in the luminance. `tonal_separation` already measures
    // that, and a rule that penalised a dominant hue was penalising the same frames twice.
    //
    // A judgement rather than a veto, and a mild one, because [`solve`] exists precisely to pull
    // two collapsing bands apart. What it cannot do is invent separation that was never there.
    let colour_is_subject = hue_carried(&reading, skin_bands);
    if colour_is_subject {
        score *= 0.80;
    }

    if score < policy.bw_candidate_floor {
        return None;
    }

    let mix = solve(&reading, skin_bands);

    // Confidence is the share of the weight that was actually measured, tempered by how far the
    // score sits above the floor. Invariant 2.
    let total_weight: f32 = policy
        .bw_weights
        .all()
        .iter()
        .map(|(_, w)| *w)
        .sum::<f32>()
        .max(f32::EPSILON);
    let measured_share = (weight_sum / total_weight).clamp(0.0, 1.0);
    let margin = ((score - policy.bw_candidate_floor) / (1.0 - policy.bw_candidate_floor).max(0.05))
        .clamp(0.0, 1.0);
    let confidence = (0.35 + 0.45 * measured_share + 0.20 * margin).clamp(0.0, 1.0);

    let mut reasons = Vec::new();
    if terms.tonal_separation >= 0.6 {
        reasons.push(CurateReason::detailed(
            CurateCode::StrongTonalSeparation,
            format!(
                "the tones stay {:.0}% of the way apart without the colour",
                terms.tonal_separation * 100.0
            ),
            terms.tonal_separation,
        ));
    }
    if terms.colour_distraction >= 0.4 {
        reasons.push(CurateReason::plain(
            CurateCode::ColourDistraction,
            terms.colour_distraction,
        ));
    }
    if terms.gesture >= 0.5 {
        reasons.push(CurateReason::plain(CurateCode::GestureLed, terms.gesture));
    }
    if terms.emotion >= 0.6 {
        reasons.push(CurateReason::plain(CurateCode::HighEmotion, terms.emotion));
    }
    if terms.grain >= 0.5 {
        reasons.push(CurateReason::plain(CurateCode::GrainTolerant, terms.grain));
    }
    if colour_is_subject {
        reasons.push(CurateReason::plain(CurateCode::ColourIsTheSubject, -0.35));
    }
    if skin_bands.is_empty() {
        reasons.push(CurateReason::plain(
            CurateCode::SkinLocusUnavailable,
            -0.05,
        ));
    } else {
        reasons.push(CurateReason::detailed(
            CurateCode::SkinSeparationHeld,
            "the mix leaves skin exactly where it was and moves what it competes with",
            0.15,
        ));
    }
    if mix.bands.iter().any(|v| v.abs() >= MAX_BAND_SHIFT) {
        reasons.push(CurateReason::plain(CurateCode::MixBounded, -0.10));
    }
    if reasons.is_empty() {
        reasons.push(CurateReason::plain(
            CurateCode::StrongTonalSeparation,
            terms.tonal_separation,
        ));
    }
    rank_reasons(&mut reasons, MAX_REASONS);

    Some(BwPick {
        image_id: frame.image_id,
        mix,
        score: score.clamp(0.0, 1.0),
        terms,
        skin_bands: skin_bands.iter().map(|b| *b as u8).collect(),
        reasons,
        confidence,
        accepted: None,
    })
}

/// How close two bands' greys have to be before they are treated as collapsing onto each other.
///
/// A tenth of the tonal range. Two regions whose greys land inside that are the same tone in a
/// monochrome print, whatever their hues were.
const COLLAPSE: f32 = 0.10;

/// Solve the eight-band mix for one frame.
///
/// The objective is **separation**: after the conversion, no two things that were different colours
/// should be the same grey, and nothing should be the same grey as somebody's face.
///
/// # Why bands near the anchor are spread against each other rather than pushed away from it
///
/// The obvious solve - push every band away from the anchor tone - is wrong, and wrong in the case
/// the feature exists for. Two regions sitting *on* the anchor both have a gap of nearly zero, so
/// both get the same direction, both move the same way, and the two stay exactly as
/// indistinguishable as they were. The code that exists to separate a face from the hedge behind it
/// would separate them from the average and not from each other.
///
/// So the bands within [`COLLAPSE`] of the anchor are treated as one problem: they are ordered by
/// the little luminance difference they already have - which preserves their existing ordering, so
/// nothing inverts - and spread across the available range. Bands already well away from the anchor
/// keep the push-away rule, because they have nothing to be separated from.
///
/// # Why the shift is scaled by the square root of the band's share
///
/// A band covering forty per cent of the frame and a band covering two per cent both want
/// separating, and a shift proportional to share would leave the small one untouched while a shift
/// independent of share would drive a two-per-cent band to its ceiling - which is a hard-edged patch
/// of black or white where a small saturated object used to be. The square root is the compromise:
/// a band at 4 % of the frame gets a fifth of the movement of one at 100 %, rather than a
/// twenty-fifth or the same.
#[must_use]
pub fn solve(reading: &BandReading, skin_bands: &BTreeSet<usize>) -> BwMix {
    let mut mix = BwMix::neutral();
    let anchor = anchor_luma(reading, skin_bands);

    // Split the movable bands into the ones colliding with the anchor and the ones that are not.
    let mut collapsed: Vec<(usize, f32, f32)> = Vec::new();
    let mut separated: Vec<(usize, f32, f32)> = Vec::new();
    for band in 0..8 {
        // The skin band does not move. Not a little, not in a safe direction, not at all.
        if skin_bands.contains(&band) {
            continue;
        }
        let (Some(share), Some(luma)) = (reading.share.get(band), reading.luma.get(band)) else {
            continue;
        };
        if *share <= 0.005 {
            continue;
        }
        if (luma - anchor).abs() < COLLAPSE {
            collapsed.push((band, *luma, *share));
        } else {
            separated.push((band, *luma, *share));
        }
    }

    for (band, luma, share) in separated {
        let gap = luma - anchor;
        // A band already far from the anchor needs no help; `closeness` is 1 where the two greys
        // coincide and 0 once they are a third of the range apart, which is comfortably separated.
        let closeness = (1.0 - gap.abs() / 0.33).clamp(0.0, 1.0);
        let direction = if gap > 0.0 { 1.0 } else { -1.0 };
        set_band(
            &mut mix,
            band,
            direction * closeness * share.sqrt() * f32::from(MAX_BAND_SHIFT),
        );
    }

    // Order by the luminance the bands already have, so the spread preserves their ordering and
    // nothing inverts. Ties break on band index, which makes the result the same on every machine.
    collapsed.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let count = collapsed.len();
    for (rank, (band, _, share)) in collapsed.into_iter().enumerate() {
        // One band on the anchor has nothing to be spread against, so it is pushed **down**: a
        // background darker than a subject is the older and safer of the two portrait conventions,
        // and pushing it up competes with the highlight the subject sits in.
        let position = if count <= 1 {
            -1.0
        } else {
            -1.0 + 2.0 * (rank as f32) / ((count - 1) as f32)
        };
        set_band(
            &mut mix,
            band,
            position * share.sqrt() * f32::from(MAX_BAND_SHIFT),
        );
    }
    mix
}

/// The tone everything is being separated from: the skin's own grey where there is skin, and the
/// frame's share-weighted mean otherwise.
fn anchor_luma(reading: &BandReading, skin_bands: &BTreeSet<usize>) -> f32 {
    if skin_bands.is_empty() {
        return weighted_mean_luma(reading);
    }
    let mut sum = 0.0f32;
    let mut weight = 0.0f32;
    for band in skin_bands {
        let (Some(l), Some(s)) = (reading.luma.get(*band), reading.share.get(*band)) else {
            continue;
        };
        sum += l * s;
        weight += s;
    }
    if weight > f32::EPSILON {
        sum / weight
    } else {
        weighted_mean_luma(reading)
    }
}

/// Write one band, rounded and clamped to the contract's ceiling.
fn set_band(mix: &mut BwMix, band: usize, value: f32) {
    let shift = (value.round() as i16).clamp(-MAX_BAND_SHIFT, MAX_BAND_SHIFT);
    if let Some(slot) = mix.bands.get_mut(band) {
        *slot = shift;
    }
}

/// The share-weighted mean band luminance of a frame.
fn weighted_mean_luma(reading: &BandReading) -> f32 {
    let mut sum = 0.0f32;
    let mut weight = 0.0f32;
    for band in 0..8 {
        let (Some(l), Some(s)) = (reading.luma.get(band), reading.share.get(band)) else {
            continue;
        };
        sum += l * s;
        weight += s;
    }
    if weight > f32::EPSILON {
        sum / weight
    } else {
        0.5
    }
}

/// Every monochrome candidate in a gallery, best first.
///
/// Ties are broken by image id so the order is the same on every machine. Invariant 4.
#[must_use]
pub fn candidates(
    frames: &[Frame],
    loci: &std::collections::BTreeMap<aura_core::contract::ids::IdentityId, u8>,
    policy: &Policy,
) -> Vec<BwPick> {
    let mut picks: Vec<BwPick> = frames
        .iter()
        .filter_map(|frame| {
            let skin = skin_bands_of(frame, loci);
            suitability(frame, &skin, policy)
        })
        .collect();
    picks.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.image_id.to_db().cmp(&b.image_id.to_db()))
    });
    picks
}

/// The image ids a caller accepted, for the store.
#[must_use]
pub fn accepted(picks: &[BwPick]) -> Vec<ImageId> {
    picks
        .iter()
        .filter(|p| p.accepted == Some(true))
        .map(|p| p.image_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use aura_core::contract::curate::MAX_SKIN_BAND_SHIFT;
    use aura_core::contract::ids::IdentityId;
    use aura_index::contract::index::LumaStats;

    use crate::read::FaceRead;

    /// A histogram with population placed at chosen (hue bin, saturation bin, value bin) triples.
    fn hist(entries: &[(usize, usize, usize, u8)]) -> Vec<u8> {
        let mut out = vec![0u8; AXIS * AXIS * AXIS];
        for (h, s, v, count) in entries {
            let ix = h * AXIS * AXIS + s * AXIS + v;
            if let Some(slot) = out.get_mut(ix) {
                *slot = *count;
            }
        }
        out
    }

    fn descriptor(entries: &[(usize, usize, usize, u8)]) -> Descriptor {
        Descriptor {
            hsv_hist: hist(entries),
            luma: LumaStats {
                mean: 0.5,
                p1: 0.05,
                p50: 0.5,
                p99: 0.95,
                clip_lo: 0.0,
                clip_hi: 0.0,
            },
            edge_energy: 0.2,
        }
    }

    fn frame_with(descriptor: Descriptor) -> Frame {
        let mut frame = Frame::bare(ImageId::new(), 0);
        frame.descriptor = Some(descriptor);
        frame.emotion = Some(0.7);
        frame.interaction = Some(0.6);
        frame.noise_sigma_rel = Some(0.35);
        frame
    }

    #[test]
    fn the_band_table_conserves_population_and_reaches_every_band() {
        let mut total = 0.0f32;
        let mut reached = [false; 8];
        for row in BAND_SPLIT {
            let mut row_sum = 0.0f32;
            for (band, share) in row {
                row_sum += share;
                reached[*band] = true;
            }
            assert!((row_sum - 1.0).abs() < 1e-5, "row sums to {row_sum}");
            total += row_sum;
        }
        assert!((total - 8.0).abs() < 1e-4);
        // The defect this table exists to avoid: a nearest-centre assignment leaves red unreachable,
        // and no mix in the product could ever move a lehenga, a bouquet or a chuppah drape.
        assert!(reached[0], "the red band must be reachable");
        assert!(reached.iter().all(|r| *r), "every band must be reachable");
    }

    #[test]
    fn a_saturated_green_frame_reads_as_green() {
        // Hue bin 2 is 90-135 degrees, which is entirely inside the green band.
        let reading = bands(&descriptor(&[(2, 7, 4, 255)]));
        assert_eq!(reading.dominant(), Some(3));
        assert!(reading.share[3] > 0.9);
    }

    #[test]
    fn a_near_neutral_frame_has_almost_no_band_population() {
        // Saturation bin 0 is nearly grey.
        let reading = bands(&descriptor(&[(2, 0, 4, 255)]));
        assert!(reading.neutral_share > 0.9, "{}", reading.neutral_share);
    }

    #[test]
    fn tonal_separation_reads_a_bimodal_frame_as_separated_and_a_flat_one_as_not() {
        let bimodal = descriptor(&[(2, 4, 0, 255), (2, 4, 7, 255)]);
        let flat = descriptor(&[(2, 4, 4, 255)]);
        assert!(tonal_separation(&bimodal) > 0.9);
        assert!(tonal_separation(&flat) < 0.2);
    }

    #[test]
    fn a_flat_frame_is_not_offered_at_all() {
        let policy = Policy::default();
        let frame = frame_with(descriptor(&[(2, 4, 4, 255)]));
        assert!(suitability(&frame, &BTreeSet::new(), &policy).is_none());
    }

    #[test]
    fn a_frame_with_faces_and_no_measured_locus_is_not_offered() {
        // ADR-0059 section 5. The product does not propose a monochrome conversion of a photograph
        // of a person until it has measured that person's skin.
        let policy = Policy::default();
        let mut frame = frame_with(descriptor(&[(2, 6, 1, 255), (2, 6, 7, 255)]));
        assert!(
            suitability(&frame, &BTreeSet::new(), &policy).is_some(),
            "a frame with no people has no skin to protect"
        );

        frame.faces = vec![FaceRead {
            identity: Some(IdentityId::new()),
            area_frac: 0.08,
            centre_x: 0.5,
            width: 0.2,
            eye_mid_x: Some(0.5),
        }];
        assert!(
            suitability(&frame, &BTreeSet::new(), &policy).is_none(),
            "faces but no locus is ignorance, not permission"
        );
        assert!(
            suitability(&frame, &BTreeSet::from([1]), &policy).is_some(),
            "a measured locus is what unlocks it"
        );
    }

    #[test]
    fn the_skin_band_does_not_move_at_all() {
        let reading = bands(&descriptor(&[(0, 6, 4, 255), (2, 6, 5, 200)]));
        let mix = solve(&reading, &BTreeSet::from([1]));
        assert_eq!(mix.bands[1], 0, "the skin band is pinned, not attenuated");
        assert!(mix.within_skin_bound(&[1]));
        assert!(mix.within_bounds());
        // And the point of pinning it: something else did move.
        assert!(
            mix.bands.iter().any(|v| *v != 0),
            "separation has to come from somewhere"
        );
    }

    #[test]
    fn the_contract_still_permits_a_small_skin_shift_that_this_solver_does_not_use() {
        // The bound exists so a future solver is bounded rather than free. This one uses none of it,
        // and that difference is worth an assertion so a change to either is visible.
        assert!(MAX_SKIN_BAND_SHIFT > 0);
        let reading = bands(&descriptor(&[(0, 6, 4, 255)]));
        let mix = solve(&reading, &BTreeSet::from([0]));
        assert_eq!(mix.bands[0], 0);
    }

    #[test]
    fn a_band_sitting_on_the_skins_tone_is_moved_further_than_one_already_far_from_it() {
        // Skin in the orange band at value bin 4; a green band at the same value and a blue band at
        // the extreme. The green is what needs separating.
        let reading = bands(&descriptor(&[
            (0, 6, 4, 255), // orange-ish, the skin
            (2, 6, 4, 255), // green, same tone as the skin
            (5, 6, 0, 255), // blue, already very dark
        ]));
        let mix = solve(&reading, &BTreeSet::from([1]));
        assert!(
            mix.bands[3].abs() > mix.bands[5].abs(),
            "green {} should move more than blue {}",
            mix.bands[3],
            mix.bands[5]
        );
    }

    #[test]
    fn a_tiny_band_is_not_driven_to_its_ceiling() {
        // A two-per-cent band driven to +/-70 is a hard-edged patch where a small object used to be.
        let reading = bands(&descriptor(&[(2, 6, 4, 255), (5, 6, 4, 5)]));
        let mix = solve(&reading, &BTreeSet::new());
        assert!(
            mix.bands[5].abs() < MAX_BAND_SHIFT / 2,
            "a small band moved {}",
            mix.bands[5]
        );
    }

    #[test]
    fn every_mix_this_solver_produces_is_inside_the_contracts_bounds() {
        let policy = Policy::default();
        for hue in 0..AXIS {
            for sat in 0..AXIS {
                let reading = bands(&descriptor(&[(hue, sat, 1, 255), (hue, sat, 6, 180)]));
                for skin in [BTreeSet::new(), BTreeSet::from([1]), BTreeSet::from([0, 3])] {
                    let mix = solve(&reading, &skin);
                    assert!(mix.within_bounds(), "hue {hue} sat {sat}: {mix:?}");
                    let bands: Vec<usize> = skin.iter().copied().collect();
                    assert!(mix.within_skin_bound(&bands), "hue {hue} sat {sat}");
                }
            }
        }
        drop(policy);
    }

    #[test]
    fn grain_peaks_and_a_clean_frame_scores_zero() {
        assert_eq!(grain(0.0), 0.0);
        assert!(grain(0.35) > 0.99);
        assert!(grain(0.70) < 0.01);
        assert!(grain(0.20) > 0.4 && grain(0.20) < 0.7);
    }

    #[test]
    fn colour_distraction_ignores_colour_close_to_the_skin_and_counts_colour_far_from_it() {
        // A saturated band at the skin's own hue is somebody's clothes reading as their skin, not a
        // distraction; a saturated band on the opposite side of the wheel is.
        let near = bands(&descriptor(&[(0, 7, 4, 255)]));
        let far = bands(&descriptor(&[(4, 7, 4, 255)]));
        let skin = BTreeSet::from([1]);
        assert!(colour_distraction(&far, &skin) > colour_distraction(&near, &skin));
    }

    #[test]
    fn every_offered_pick_is_well_formed_and_carries_a_reason() {
        let policy = Policy::default();
        let frame = frame_with(descriptor(&[(2, 6, 1, 255), (2, 6, 7, 255)]));
        let pick = suitability(&frame, &BTreeSet::new(), &policy).expect("offered");
        assert!(pick.is_well_formed(), "{pick:?}");
        assert!(!pick.reasons.is_empty());
        assert!(pick.reasons.len() <= MAX_REASONS);
        assert!(pick
            .reasons
            .iter()
            .any(|r| r.code == CurateCode::SkinLocusUnavailable));
    }

    #[test]
    fn candidates_are_ordered_best_first_and_deterministically() {
        let policy = Policy::default();
        let loci: BTreeMap<IdentityId, u8> = BTreeMap::new();
        let frames: Vec<Frame> = (0..6)
            .map(|i| {
                let mut f = frame_with(descriptor(&[(2, 6, 0, 255), (2, 6, 7, 200 + i * 5)]));
                f.order = u32::from(i);
                f.emotion = Some(0.5 + f32::from(i) * 0.05);
                f
            })
            .collect();
        let first = candidates(&frames, &loci, &policy);
        let second = candidates(&frames, &loci, &policy);
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.image_id, b.image_id);
        }
        for pair in first.windows(2) {
            assert!(pair[0].score >= pair[1].score);
        }
    }
}
